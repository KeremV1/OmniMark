//! User settings, stored as a JSON file in the app config directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    Auto,
    Tr,
    En,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub output_dir: PathBuf,
    pub extract_images: bool,
    pub workers: usize,
    pub language: Language,
}

/// Cores minus one, between 1 and 4.
fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1))
        .unwrap_or(1)
        .clamp(1, 4)
}

impl Settings {
    pub fn defaults(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            extract_images: true,
            workers: default_workers(),
            language: Language::Auto,
        }
    }

    /// Reads the file; a missing or unreadable file gives the defaults.
    pub fn load(path: &Path, default_output_dir: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .map(|s| s.sanitized(default_output_dir))
            .unwrap_or_else(|| Self::defaults(default_output_dir))
    }

    /// Forces values into their valid ranges; an empty or relative output folder
    /// is replaced by `default_output_dir`.
    pub fn sanitized(mut self, default_output_dir: &Path) -> Self {
        self.workers = self.workers.clamp(1, 8);
        if !self.output_dir.is_absolute() {
            self.output_dir = default_output_dir.to_path_buf();
        }
        self
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        omnimark_core::output::write_atomic(path, &json)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_corrupt_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let default_out = dir.path().join("Documents/OmniMark");
        let s = Settings::load(&dir.path().join("settings.json"), &default_out);
        assert_eq!(s, Settings::defaults(&default_out));
        std::fs::write(dir.path().join("bad.json"), "{not json").unwrap();
        assert_eq!(
            Settings::load(&dir.path().join("bad.json"), &default_out),
            Settings::defaults(&default_out)
        );
    }

    #[test]
    fn save_then_load_round_trips_and_workers_are_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg/settings.json");
        let mut s = Settings::defaults(dir.path());
        s.workers = 99;
        s.language = Language::En;
        let saved = s.sanitized(dir.path());
        saved.save(&path).unwrap();
        assert_eq!(Settings::load(&path, dir.path()), saved);
        assert_eq!(saved.workers, 8);
    }

    #[test]
    fn empty_or_relative_output_dir_falls_back_to_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let default_out = dir.path().join("OmniMark");
        for bad in ["", "relative/out", "."] {
            let mut s = Settings::defaults(dir.path());
            s.output_dir = PathBuf::from(bad);
            assert_eq!(s.clone().sanitized(&default_out).output_dir, default_out);
            // Same rule when a stored file holds the bad value.
            let path = dir.path().join("settings.json");
            std::fs::write(&path, serde_json::to_string(&s).unwrap()).unwrap();
            assert_eq!(Settings::load(&path, &default_out).output_dir, default_out);
        }
        let ok = Settings::defaults(dir.path()).sanitized(&default_out);
        assert_eq!(ok.output_dir, dir.path());
    }
}
