//! Destinations for images embedded in converted documents.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::error::Result;
use crate::markdown::link_target;

/// Receives embedded images during conversion.
pub trait AssetSink {
    /// Stores `bytes` under a name derived from `suggested_name` and returns the Markdown link
    /// target, or `None` when images are not being extracted.
    fn store(&mut self, suggested_name: &str, bytes: &[u8]) -> Result<Option<String>>;
}

/// Discards images (extraction disabled, or `--stdout`).
pub struct NullAssetSink;

impl AssetSink for NullAssetSink {
    fn store(&mut self, _suggested_name: &str, _bytes: &[u8]) -> Result<Option<String>> {
        Ok(None)
    }
}

/// Keeps a file name to `[A-Za-z0-9._-]`, drops any directory part.
pub fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('.');
    if trimmed.is_empty() {
        "image".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Default)]
struct UniqueNames(HashSet<String>);

impl UniqueNames {
    fn claim(&mut self, suggested: &str) -> String {
        let clean = sanitize_file_name(suggested);
        let (stem, ext) = match clean.rfind('.') {
            Some(i) if i > 0 => (clean[..i].to_string(), clean[i..].to_string()),
            _ => (clean.clone(), String::new()),
        };
        let mut candidate = clean;
        let mut n = 2;
        while !self.0.insert(candidate.to_ascii_lowercase()) {
            candidate = format!("{stem}-{n}{ext}");
            n += 1;
        }
        candidate
    }
}

/// Collects images in memory; used by tests and the golden runner.
pub struct MemoryAssetSink {
    link_prefix: String,
    names: UniqueNames,
    pub files: Vec<(String, Vec<u8>)>,
}

impl MemoryAssetSink {
    pub fn new(link_prefix: impl Into<String>) -> Self {
        Self {
            link_prefix: link_prefix.into(),
            names: UniqueNames::default(),
            files: Vec::new(),
        }
    }
}

impl AssetSink for MemoryAssetSink {
    fn store(&mut self, suggested_name: &str, bytes: &[u8]) -> Result<Option<String>> {
        let name = self.names.claim(suggested_name);
        let link = format!("{}/{}", link_target(&self.link_prefix), link_target(&name));
        self.files.push((name, bytes.to_vec()));
        Ok(Some(link))
    }
}

/// Writes images into `<stem>_assets/`, creating the folder on first use.
pub struct DirAssetSink {
    dir: PathBuf,
    link_prefix: String,
    names: UniqueNames,
    created: bool,
}

impl DirAssetSink {
    pub fn new(dir: PathBuf) -> Self {
        let link_prefix = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            dir,
            link_prefix,
            names: UniqueNames::default(),
            created: false,
        }
    }

    pub fn created(&self) -> bool {
        self.created
    }

    /// Deletes the folder if this sink created it (used when the conversion fails).
    pub fn remove(&self) -> std::io::Result<()> {
        if self.created {
            std::fs::remove_dir_all(&self.dir)?;
        }
        Ok(())
    }
}

impl AssetSink for DirAssetSink {
    fn store(&mut self, suggested_name: &str, bytes: &[u8]) -> Result<Option<String>> {
        if !self.created {
            std::fs::create_dir_all(&self.dir)?;
            self.created = true;
        }
        let name = self.names.claim(suggested_name);
        std::fs::write(self.dir.join(&name), bytes)?;
        Ok(Some(format!(
            "{}/{}",
            link_target(&self.link_prefix),
            link_target(&name)
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitized_and_unique() {
        let mut sink = MemoryAssetSink::new("rapor (2)_assets");
        let a = sink.store("media/image 1.png", b"a").unwrap();
        let b = sink.store("IMAGE_1.PNG", b"b").unwrap();
        let c = sink.store("../../evil.exe", b"c").unwrap();
        assert_eq!(a.as_deref(), Some("rapor%20%282%29_assets/image_1.png"));
        assert_eq!(b.as_deref(), Some("rapor%20%282%29_assets/IMAGE_1-2.PNG"));
        assert_eq!(c.as_deref(), Some("rapor%20%282%29_assets/evil.exe"));
        assert_eq!(sink.files.len(), 3);
    }

    #[test]
    fn empty_names_fall_back_to_image() {
        assert_eq!(sanitize_file_name(""), "image");
        assert_eq!(sanitize_file_name("..."), "image");
        assert_eq!(sanitize_file_name("çizim.png"), "_izim.png");
    }

    #[test]
    fn null_sink_returns_none() {
        assert_eq!(NullAssetSink.store("a.png", b"x").unwrap(), None);
    }

    #[test]
    fn dir_sink_creates_directory_lazily_and_can_remove_it() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("doc_assets");
        let mut sink = DirAssetSink::new(dir.clone());
        assert!(!dir.exists());
        let link = sink.store("a.png", b"png").unwrap();
        assert_eq!(link.as_deref(), Some("doc_assets/a.png"));
        assert_eq!(std::fs::read(dir.join("a.png")).unwrap(), b"png");
        assert!(sink.created());
        sink.remove().unwrap();
        assert!(!dir.exists());
    }
}
