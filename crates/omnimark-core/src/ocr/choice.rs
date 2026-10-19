//! Which engine runs: `auto` = Windows OCR when a language is installed, else an installed
//! Tesseract, else downloaded ocrs. An explicit choice never falls back.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{tesseract_engine, OcrEngine, OcrError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OcrChoice {
    #[default]
    Auto,
    Windows,
    Tesseract,
    Ocrs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineKind {
    Windows,
    Tesseract,
    Ocrs,
}

/// Where the engines' files are; built by the CLI and the app for each batch / status call.
#[derive(Debug, Clone, Default)]
pub struct EngineConfig {
    /// Models root (contains `ocrs/`).
    pub models_root: Option<PathBuf>,
    /// User-configured `tesseract` program; `None` = search (see `tesseract_engine::resolve_path`).
    pub tesseract_path: Option<PathBuf>,
}

/// What this build and this machine offer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    /// Compiled with `ocr-windows` on Windows.
    pub windows_built: bool,
    /// Installed Windows OCR languages (BCP-47 tags such as `tr`, `en-US`).
    pub windows_languages: Vec<String>,
    /// The `tesseract` program found (serialized as a string or null).
    pub tesseract_path: Option<PathBuf>,
    /// Tesseract language codes (`tur`, `eng`, …) without `osd`.
    pub tesseract_languages: Vec<String>,
    /// Compiled with `ocr`.
    pub ocrs_built: bool,
    /// Both ocrs model files are in the models folder.
    pub ocrs_models: bool,
}

impl Availability {
    pub fn windows(&self) -> bool {
        self.windows_built && !self.windows_languages.is_empty()
    }

    pub fn tesseract(&self) -> bool {
        self.tesseract_path.is_some() && !self.tesseract_languages.is_empty()
    }

    pub fn ocrs(&self) -> bool {
        self.ocrs_built && self.ocrs_models
    }
}

/// `Auto`: Windows → Tesseract → ocrs; an explicit choice never falls back.
pub fn select(choice: OcrChoice, available: &Availability) -> Option<EngineKind> {
    match choice {
        OcrChoice::Auto if available.windows() => Some(EngineKind::Windows),
        OcrChoice::Auto if available.tesseract() => Some(EngineKind::Tesseract),
        OcrChoice::Auto if available.ocrs() => Some(EngineKind::Ocrs),
        OcrChoice::Auto => None,
        OcrChoice::Windows => available.windows().then_some(EngineKind::Windows),
        OcrChoice::Tesseract => available.tesseract().then_some(EngineKind::Tesseract),
        OcrChoice::Ocrs => available.ocrs().then_some(EngineKind::Ocrs),
    }
}

/// Never touches the network; asks Windows for its OCR languages and runs `tesseract
/// --version` / `--list-langs` (a few ms). Nothing is cached: it is called rarely.
pub fn availability(config: &EngineConfig) -> Availability {
    let tesseract_path = tesseract_engine::resolve_path(config.tesseract_path.as_deref());
    let tesseract_languages = tesseract_path
        .as_deref()
        .map(tesseract_engine::languages)
        .unwrap_or_default();
    Availability {
        windows_built: cfg!(all(windows, feature = "ocr-windows")),
        windows_languages: windows_languages(),
        tesseract_path,
        tesseract_languages,
        ocrs_built: cfg!(feature = "ocr"),
        ocrs_models: config.models_root.as_deref().is_some_and(ocrs_present),
    }
}

/// Builds the engine for one batch (drop it when the batch ends).
pub fn build_engine(
    choice: OcrChoice,
    config: &EngineConfig,
) -> Result<Arc<dyn OcrEngine>, OcrError> {
    let available = availability(config);
    match select(choice, &available) {
        Some(EngineKind::Windows) => windows_engine(),
        Some(EngineKind::Tesseract) => {
            let path = available.tesseract_path.ok_or_else(|| {
                OcrError::Unavailable(unavailable_message(OcrChoice::Tesseract).into())
            })?;
            Ok(Arc::new(tesseract_engine::TesseractEngine::new(path)?))
        }
        Some(EngineKind::Ocrs) => ocrs_engine(config.models_root.as_deref()),
        None => Err(OcrError::Unavailable(
            unavailable_message(choice).to_string(),
        )),
    }
}

pub fn unavailable_message(choice: OcrChoice) -> &'static str {
    match choice {
        OcrChoice::Auto => "no OCR engine is available (install a Windows OCR language or Tesseract, or download the ocrs models)",
        OcrChoice::Windows => "Windows OCR is not available on this system",
        OcrChoice::Tesseract => "Tesseract is not installed or has no language data (install it from https://github.com/tesseract-ocr/tesseract or set its path)",
        OcrChoice::Ocrs => "the ocrs models are not downloaded (run `omnimark models fetch ocrs` or use Settings)",
    }
}

#[cfg(all(windows, feature = "ocr-windows"))]
fn windows_languages() -> Vec<String> {
    super::windows_engine::available_languages()
}

#[cfg(not(all(windows, feature = "ocr-windows")))]
fn windows_languages() -> Vec<String> {
    Vec::new()
}

#[cfg(all(windows, feature = "ocr-windows"))]
fn windows_engine() -> Result<Arc<dyn OcrEngine>, OcrError> {
    Ok(Arc::new(super::windows_engine::WindowsEngine::new()?))
}

#[cfg(not(all(windows, feature = "ocr-windows")))]
fn windows_engine() -> Result<Arc<dyn OcrEngine>, OcrError> {
    Err(OcrError::Unavailable(
        "this build has no Windows OCR support".into(),
    ))
}

#[cfg(feature = "ocr")]
fn ocrs_present(models_root: &Path) -> bool {
    super::models::ocrs_models_present(models_root)
}

#[cfg(not(feature = "ocr"))]
fn ocrs_present(_models_root: &Path) -> bool {
    false
}

#[cfg(feature = "ocr")]
fn ocrs_engine(models_root: Option<&Path>) -> Result<Arc<dyn OcrEngine>, OcrError> {
    let root = models_root.ok_or_else(|| OcrError::Unavailable("no models folder".into()))?;
    Ok(Arc::new(super::ocrs_engine::OcrsEngine::load(root)?))
}

#[cfg(not(feature = "ocr"))]
fn ocrs_engine(_models_root: Option<&Path>) -> Result<Arc<dyn OcrEngine>, OcrError> {
    Err(OcrError::Unavailable(
        "this build has no ocrs support".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn avail(windows: &[&str], tesseract: &[&str], ocrs: bool) -> Availability {
        Availability {
            windows_built: true,
            windows_languages: windows.iter().map(|s| s.to_string()).collect(),
            tesseract_path: (!tesseract.is_empty()).then(|| PathBuf::from("/usr/bin/tesseract")),
            tesseract_languages: tesseract.iter().map(|s| s.to_string()).collect(),
            ocrs_built: true,
            ocrs_models: ocrs,
        }
    }

    #[test]
    fn auto_prefers_windows_then_tesseract_then_ocrs() {
        assert_eq!(
            select(OcrChoice::Auto, &avail(&["tr"], &["tur"], true)),
            Some(EngineKind::Windows)
        );
        assert_eq!(
            select(OcrChoice::Auto, &avail(&[], &["tur"], true)),
            Some(EngineKind::Tesseract)
        );
        assert_eq!(
            select(OcrChoice::Auto, &avail(&[], &[], true)),
            Some(EngineKind::Ocrs)
        );
        assert_eq!(select(OcrChoice::Auto, &avail(&[], &[], false)), None);
    }

    #[test]
    fn explicit_choices_never_fall_back() {
        assert_eq!(
            select(OcrChoice::Windows, &avail(&[], &["eng"], true)),
            None
        );
        assert_eq!(
            select(OcrChoice::Tesseract, &avail(&["tr"], &[], true)),
            None
        );
        assert_eq!(
            select(OcrChoice::Tesseract, &avail(&[], &["eng"], false)),
            Some(EngineKind::Tesseract)
        );
        assert_eq!(
            select(OcrChoice::Ocrs, &avail(&["en-US"], &["eng"], false)),
            None
        );
        assert_eq!(
            select(OcrChoice::Ocrs, &avail(&["en-US"], &[], true)),
            Some(EngineKind::Ocrs)
        );
    }

    #[test]
    fn tesseract_needs_a_path_and_languages() {
        let mut a = avail(&[], &[], false);
        a.tesseract_path = Some(PathBuf::from("/usr/bin/tesseract"));
        assert!(!a.tesseract());
        a.tesseract_languages = vec!["eng".into()];
        assert!(a.tesseract());
        a.tesseract_path = None;
        assert!(!a.tesseract());
    }

    #[test]
    fn engines_missing_from_the_build_are_unavailable() {
        let mut a = avail(&["en-US"], &[], true);
        a.windows_built = false;
        a.ocrs_built = false;
        assert_eq!(select(OcrChoice::Auto, &a), None);
    }

    #[test]
    fn build_engine_explains_why_nothing_is_available() {
        let dir = tempfile::tempdir().unwrap();
        let config = EngineConfig {
            models_root: Some(dir.path().to_path_buf()),
            tesseract_path: None,
        };
        match build_engine(OcrChoice::Ocrs, &config) {
            Err(error) => assert_eq!(
                error,
                OcrError::Unavailable(unavailable_message(OcrChoice::Ocrs).to_string())
            ),
            Ok(engine) => panic!("unexpected engine {}", engine.name()),
        }
        assert!(unavailable_message(OcrChoice::Tesseract)
            .contains("https://github.com/tesseract-ocr/tesseract"));
    }

    #[test]
    fn choices_serialize_in_camel_case() {
        assert_eq!(serde_json::to_string(&OcrChoice::Ocrs).unwrap(), "\"ocrs\"");
        assert_eq!(
            serde_json::to_string(&OcrChoice::Tesseract).unwrap(),
            "\"tesseract\""
        );
        assert_eq!(
            serde_json::to_string(&EngineKind::Tesseract).unwrap(),
            "\"tesseract\""
        );
        assert_eq!(
            serde_json::to_string(&EngineKind::Windows).unwrap(),
            "\"windows\""
        );
        assert_eq!(
            serde_json::to_value(avail(&["tr"], &[], false)).unwrap(),
            serde_json::json!({"windowsBuilt": true, "windowsLanguages": ["tr"], "tesseractPath": null, "tesseractLanguages": [], "ocrsBuilt": true, "ocrsModels": false})
        );
        assert_eq!(
            serde_json::to_value(avail(&[], &["tur"], false)).unwrap()["tesseractPath"],
            "/usr/bin/tesseract"
        );
    }
}
