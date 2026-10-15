//! Output file naming (never overwrite) and atomic writes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputTarget {
    pub markdown_path: PathBuf,
    pub assets_dir: PathBuf,
}

/// Hands out unique `<stem>.md` / `<stem>_assets` pairs inside one output folder.
/// Safe to share between worker threads of one batch.
pub struct OutputAllocator {
    dir: PathBuf,
    reserved: Mutex<HashSet<String>>,
}

impl OutputAllocator {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            reserved: Mutex::new(HashSet::new()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn allocate(&self, source: &Path) -> OutputTarget {
        let stem = sanitize_stem(
            &source
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        let mut reserved = self
            .reserved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut n = 1u32;
        loop {
            let name = if n == 1 {
                stem.clone()
            } else {
                format!("{stem} ({n})")
            };
            let markdown_path = self.dir.join(format!("{name}.md"));
            let assets_dir = self.dir.join(format!("{name}_assets"));
            let key = name.to_lowercase();
            if !reserved.contains(&key) && !markdown_path.exists() && !assets_dir.exists() {
                reserved.insert(key);
                return OutputTarget {
                    markdown_path,
                    assets_dir,
                };
            }
            n += 1;
        }
    }
}

/// Replaces characters that are invalid in file names on Windows, macOS or Linux.
pub fn sanitize_stem(stem: &str) -> String {
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_end_matches(['.', ' ']).trim();
    if trimmed.is_empty() {
        "document".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Writes through a hidden temporary file and renames it, so a crash never leaves a half-written `.md`.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{file_name}.tmp"));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn allocates_unique_names_against_disk_and_reservations() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("rapor.md"), "old").unwrap();
        let alloc = OutputAllocator::new(tmp.path());
        let first = alloc.allocate(Path::new("C:/in/rapor.pdf"));
        assert_eq!(first.markdown_path, tmp.path().join("rapor (2).md"));
        assert_eq!(first.assets_dir, tmp.path().join("rapor (2)_assets"));
        let second = alloc.allocate(Path::new("D:/other/RAPOR.docx"));
        assert_eq!(second.markdown_path, tmp.path().join("RAPOR (3).md"));
    }

    #[test]
    fn existing_assets_dir_also_blocks_a_name() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("x_assets")).unwrap();
        let target = OutputAllocator::new(tmp.path()).allocate(Path::new("x.pptx"));
        assert_eq!(target.markdown_path, tmp.path().join("x (2).md"));
    }

    #[test]
    fn stems_are_made_safe_for_every_os() {
        assert_eq!(sanitize_stem("a:b*c?"), "a_b_c_");
        assert_eq!(sanitize_stem("trailing. "), "trailing");
        assert_eq!(sanitize_stem(""), "document");
    }

    #[test]
    fn write_atomic_creates_parents_and_leaves_no_temp_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested/out.md");
        write_atomic(&path, "# hi").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# hi");
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
    }
}
