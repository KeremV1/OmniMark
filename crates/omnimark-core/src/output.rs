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
        // Compare case-insensitively so names stay unique on every filesystem
        // (Windows and macOS are case-insensitive, Linux is not).
        let on_disk: HashSet<String> = std::fs::read_dir(&self.dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
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
            let taken = on_disk.contains(&format!("{key}.md"))
                || on_disk.contains(&format!("{key}_assets"))
                || markdown_path.exists()
                || assets_dir.exists();
            if !reserved.contains(&key) && !taken {
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

impl OutputAllocator {
    /// A unique folder name `<stem>` (then `<stem> (2)`, ...) for the contents of an archive.
    pub fn allocate_dir(&self, source: &Path) -> PathBuf {
        self.claim(&Self::stem_of(source), "")
    }

    /// A unique `<stem>-md.zip` file name for an archive's converted contents.
    pub fn allocate_zip(&self, source: &Path) -> PathBuf {
        self.claim(&format!("{}-md", Self::stem_of(source)), ".zip")
    }

    fn stem_of(source: &Path) -> String {
        sanitize_stem(
            &source
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    }

    /// Reserves `<name><suffix>` (`name` = `base`, `base (2)`, ...) in the output folder.
    fn claim(&self, base: &str, suffix: &str) -> PathBuf {
        let mut reserved = self
            .reserved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let on_disk: HashSet<String> = std::fs::read_dir(&self.dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        let mut n = 1u32;
        loop {
            let name = if n == 1 {
                base.to_string()
            } else {
                format!("{base} ({n})")
            };
            let file_name = format!("{name}{suffix}");
            let key = file_name.to_lowercase();
            if !reserved.contains(&key) && !on_disk.contains(&key) {
                reserved.insert(key);
                return self.dir.join(file_name);
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
    } else if is_reserved_on_windows(trimmed) {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// `CON`, `NUL`, `COM1`… (also with an extension, any case) name devices on Windows.
fn is_reserved_on_windows(name: &str) -> bool {
    let base = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit()
            && base.as_bytes()[3] != b'0')
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
    fn archive_folders_and_zips_get_unique_names_too() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("pack")).unwrap();
        std::fs::write(tmp.path().join("pack-md.zip"), "old").unwrap();
        let alloc = OutputAllocator::new(tmp.path());
        assert_eq!(
            alloc.allocate_dir(Path::new("in/pack.zip")),
            tmp.path().join("pack (2)")
        );
        assert_eq!(
            alloc.allocate_dir(Path::new("other/PACK.zip")),
            tmp.path().join("PACK (3)")
        );
        assert_eq!(
            alloc.allocate_zip(Path::new("in/pack.zip")),
            tmp.path().join("pack-md (2).zip")
        );
        assert_eq!(
            alloc.allocate_zip(Path::new("in/a:b.zip")),
            tmp.path().join("a_b-md.zip")
        );
    }

    #[test]
    fn stems_are_made_safe_for_every_os() {
        assert_eq!(sanitize_stem("a:b*c?"), "a_b_c_");
        assert_eq!(sanitize_stem("trailing. "), "trailing");
        assert_eq!(sanitize_stem(""), "document");
        assert_eq!(sanitize_stem("CON"), "_CON");
        assert_eq!(sanitize_stem("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_stem("com1"), "_com1");
        assert_eq!(sanitize_stem("COM0"), "COM0");
        assert_eq!(sanitize_stem("console"), "console");
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
