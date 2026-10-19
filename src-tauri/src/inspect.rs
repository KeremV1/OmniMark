//! Expands dropped paths into a flat, de-duplicated list of files for the queue.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use omnimark_core::detect::{classify, FileKind};
use omnimark_core::Registry;
use serde::Serialize;
use walkdir::WalkDir;

const HEAD_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub kind: FileKind,
    pub size: u64,
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectResult {
    pub entries: Vec<FileEntry>,
    pub truncated: bool,
}

fn read_head(path: &Path) -> Vec<u8> {
    let mut head = Vec::new();
    if let Ok(file) = std::fs::File::open(path) {
        let _ = file.take(HEAD_BYTES).read_to_end(&mut head);
    }
    head
}

fn entry_for(registry: &Registry, path: &Path) -> Option<FileEntry> {
    let size = std::fs::metadata(path).ok()?.len();
    let head = read_head(path);
    Some(FileEntry {
        path: path.display().to_string(),
        name: path.file_name()?.to_string_lossy().into_owned(),
        kind: classify(path, &head),
        size,
        supported: registry.accepts_any(path, &head),
    })
}

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

/// Explicit files are always listed (flagged when unsupported); folders contribute only
/// supported, non-hidden files, in name order. Stops after `limit` entries.
pub fn inspect(registry: &Registry, paths: &[PathBuf], limit: usize) -> InspectResult {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut truncated = false;

    'outer: for root in paths {
        let candidates: Vec<(PathBuf, bool)> = if root.is_dir() {
            WalkDir::new(root)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|e| e.depth() == 0 || !is_hidden(e.file_name()))
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .map(|e| (e.into_path(), true))
                .collect()
        } else {
            vec![(root.clone(), false)]
        };
        for (path, from_folder) in candidates {
            if seen.contains(&path) {
                continue;
            }
            let Some(entry) = entry_for(registry, &path) else {
                continue;
            };
            if from_folder && !entry.supported {
                continue;
            }
            if entries.len() >= limit {
                truncated = true;
                break 'outer;
            }
            seen.insert(path);
            entries.push(entry);
        }
    }
    InspectResult { entries, truncated }
}
#[cfg(test)]
mod tests {
    use super::*;
    use omnimark_core::formats::{default_registry, RegistryConfig};

    #[test]
    fn expands_folders_dedupes_and_flags_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        std::fs::write(dir.path().join("sub/b.csv"), "a,b").unwrap();
        std::fs::write(dir.path().join("sub/c.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        let registry = default_registry(&RegistryConfig::default());
        let a = dir.path().join("a.txt");
        let result = inspect(&registry, &[dir.path().to_path_buf(), a.clone()], 1000);
        assert!(!result.truncated);
        let names: Vec<_> = result
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.supported))
            .collect();
        assert_eq!(names, [("a.txt", true), ("b.csv", true)]);
        let single = inspect(&registry, &[dir.path().join("sub/c.jpg")], 1000);
        assert_eq!(
            single.entries[0].kind,
            omnimark_core::detect::FileKind::Image
        );
        assert!(!single.entries[0].supported);
    }

    #[test]
    fn folder_expansion_stops_at_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            std::fs::write(dir.path().join(format!("{i}.txt")), "x").unwrap();
        }
        let registry = default_registry(&RegistryConfig::default());
        let result = inspect(&registry, &[dir.path().to_path_buf()], 3);
        assert_eq!(result.entries.len(), 3);
        assert!(result.truncated);
    }
}
