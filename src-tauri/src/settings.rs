//! User settings, stored as a JSON file in the app config directory.

use std::path::{Path, PathBuf};

use omnimark_core::ocr::{EngineConfig, EngineKind, OcrChoice, OcrLanguage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    Auto,
    Tr,
    En,
}

/// OCR (text recognition) settings; off by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OcrSettings {
    pub enabled: bool,
    pub engine: OcrChoice,
    pub language: OcrLanguage,
    pub tesseract_path: Option<PathBuf>,
}

impl Default for OcrSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: OcrChoice::Auto,
            language: OcrLanguage::Auto,
            tesseract_path: None,
        }
    }
}

/// Missing fields take their defaults, so an older or partial file keeps the fields it has
/// (an empty output folder is replaced in `sanitized`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub output_dir: PathBuf,
    pub extract_images: bool,
    pub workers: usize,
    pub language: Language,
    pub ocr: OcrSettings,
}

/// Folder name created under the base folder for converted files.
const OUTPUT_FOLDER: &str = "OmniMark";

/// Default output folder: `<Documents>/OmniMark`; without a Documents folder (Linux without
/// XDG user dirs) `<home>/OmniMark`, then `<app data>/OmniMark`.
pub fn default_output_dir(
    documents: Option<PathBuf>,
    home: Option<PathBuf>,
    app_data: Option<PathBuf>,
) -> Option<PathBuf> {
    documents
        .or(home)
        .or(app_data)
        .map(|base| base.join(OUTPUT_FOLDER))
}

/// Cores minus one, between 1 and 4.
fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1))
        .unwrap_or(1)
        .clamp(1, 4)
}

impl Default for Settings {
    fn default() -> Self {
        Self::defaults(Path::new(""))
    }
}

impl Settings {
    pub fn defaults(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            extract_images: true,
            workers: default_workers(),
            language: Language::Auto,
            ocr: OcrSettings::default(),
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
        if self
            .ocr
            .tesseract_path
            .as_ref()
            .is_some_and(|p| !p.is_absolute())
        {
            self.ocr.tesseract_path = None;
        }
        self
    }

    pub fn engine_config(&self, models_root: &Path) -> EngineConfig {
        EngineConfig {
            models_root: Some(models_root.to_path_buf()),
            tesseract_path: self.ocr.tesseract_path.clone(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        omnimark_core::output::write_atomic(path, &json)
    }
}
/// The OCR language to use: `Auto` follows the interface language (itself `Auto` = system),
/// except for Tesseract, whose own `Auto` picks `tur+eng` from what is installed (an explicit
/// language is strict there and would fail when its data is missing).
pub fn effective_ocr_language(settings: &Settings, engine: Option<EngineKind>) -> OcrLanguage {
    match (settings.ocr.language, settings.language) {
        (OcrLanguage::Auto, _) if engine == Some(EngineKind::Tesseract) => OcrLanguage::Auto,
        (OcrLanguage::Auto, Language::Tr) => OcrLanguage::Tr,
        (OcrLanguage::Auto, Language::En) => OcrLanguage::En,
        (language, _) => language,
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

    #[test]
    fn partial_file_keeps_its_known_fields() {
        let dir = tempfile::tempdir().unwrap();
        let default_out = dir.path().join("OmniMark");
        let custom = dir.path().join("custom");
        let path = dir.path().join("settings.json");
        let json = serde_json::json!({
            "outputDir": custom,
            "extractImages": false,
            "workers": 3
        });
        std::fs::write(&path, json.to_string()).unwrap();
        let s = Settings::load(&path, &default_out);
        assert_eq!(s.output_dir, custom);
        assert!(!s.extract_images);
        assert_eq!(s.workers, 3);
        assert_eq!(s.language, Language::Auto);
    }

    #[test]
    fn default_output_dir_falls_back_from_documents_to_home_to_app_data() {
        let docs = Some(PathBuf::from("/docs"));
        let home = Some(PathBuf::from("/home/u"));
        let data = Some(PathBuf::from("/data"));
        assert_eq!(
            default_output_dir(docs, home.clone(), data.clone()),
            Some(PathBuf::from("/docs/OmniMark"))
        );
        assert_eq!(
            default_output_dir(None, home, data.clone()),
            Some(PathBuf::from("/home/u/OmniMark"))
        );
        assert_eq!(
            default_output_dir(None, None, data),
            Some(PathBuf::from("/data/OmniMark"))
        );
        assert_eq!(default_output_dir(None, None, None), None);
    }

    #[test]
    fn old_files_get_ocr_defaults_and_ocr_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"outputDir": "/x", "workers": 2}"#).unwrap();
        let s = Settings::load(&path, dir.path());
        assert_eq!(s.ocr, OcrSettings::default());
        assert!(!s.ocr.enabled);
        let mut on = Settings::defaults(dir.path());
        let tesseract = dir.path().join("tesseract.exe");
        on.ocr = OcrSettings {
            enabled: true,
            engine: OcrChoice::Tesseract,
            language: OcrLanguage::Tr,
            tesseract_path: Some(tesseract.clone()),
        };
        on.save(&path).unwrap();
        assert_eq!(Settings::load(&path, dir.path()).ocr, on.ocr);
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains(r#""engine": "tesseract""#));
        let config = on.engine_config(dir.path());
        assert_eq!(config.tesseract_path, Some(tesseract));
        assert_eq!(config.models_root.as_deref(), Some(dir.path()));
        std::fs::write(
            &path,
            r#"{"ocr": {"enabled": true, "tesseractPath": "relative/tesseract"}}"#,
        )
        .unwrap();
        let partial = Settings::load(&path, dir.path());
        assert!(partial.ocr.enabled);
        assert_eq!(partial.ocr.engine, OcrChoice::Auto);
        assert_eq!(partial.ocr.tesseract_path, None);
    }

    #[test]
    fn auto_ocr_language_follows_the_interface_language() {
        let mut s = Settings::defaults(Path::new("/x"));
        assert_eq!(effective_ocr_language(&s, None), OcrLanguage::Auto);
        s.language = Language::Tr;
        for kind in [EngineKind::Windows, EngineKind::Ocrs] {
            assert_eq!(effective_ocr_language(&s, Some(kind)), OcrLanguage::Tr);
        }
        s.language = Language::En;
        assert_eq!(
            effective_ocr_language(&s, Some(EngineKind::Ocrs)),
            OcrLanguage::En
        );
        s.ocr.language = OcrLanguage::En;
        assert_eq!(effective_ocr_language(&s, None), OcrLanguage::En);
    }

    #[test]
    fn auto_ocr_language_stays_auto_for_tesseract_but_explicit_stays_strict() {
        let mut s = Settings::defaults(Path::new("/x"));
        s.language = Language::Tr;
        let t = Some(EngineKind::Tesseract);
        assert_eq!(effective_ocr_language(&s, t), OcrLanguage::Auto);
        s.ocr.language = OcrLanguage::Tr;
        assert_eq!(effective_ocr_language(&s, t), OcrLanguage::Tr);
        s.ocr.language = OcrLanguage::En;
        assert_eq!(effective_ocr_language(&s, t), OcrLanguage::En);
    }
}
