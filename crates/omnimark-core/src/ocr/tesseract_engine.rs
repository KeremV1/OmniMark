//! Tesseract engine: runs the `tesseract` program installed on the system (never downloaded or
//! bundled). Skeleton from Task M2-0; Task M2-8 implements it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{OcrEngine, OcrError, OcrImage, OcrLanguage};

const NOT_YET: &str = "Tesseract engine is not implemented yet";

/// Search order: configured → `OMNIMARK_TESSERACT` → `PATH` → well-known install locations.
pub fn candidate_paths(
    _configured: Option<&Path>,
    _env: Option<OsString>,
    _path_var: Option<OsString>,
) -> Vec<PathBuf> {
    Vec::new()
}

/// The first acceptable candidate that answers `--version`.
pub fn resolve_path(_configured: Option<&Path>) -> Option<PathBuf> {
    None
}

/// Installed language codes from `--list-langs`, without `osd`.
pub fn languages(_path: &Path) -> Vec<String> {
    Vec::new()
}

pub fn parse_list_langs(_output: &str) -> Vec<String> {
    Vec::new()
}

/// The `-l` argument for `requested` (`tur`, `eng`, or `tur+eng` for `Auto`).
pub fn lang_arg(_requested: OcrLanguage, _installed: &[String]) -> Result<String, OcrError> {
    Err(OcrError::Unavailable(NOT_YET.into()))
}

pub struct TesseractEngine;

impl TesseractEngine {
    pub fn new(_path: PathBuf) -> Result<Self, OcrError> {
        Err(OcrError::Unavailable(NOT_YET.into()))
    }
}

impl OcrEngine for TesseractEngine {
    fn name(&self) -> &'static str {
        "tesseract"
    }

    fn recognize(&self, _image: &OcrImage, _language: &OcrLanguage) -> Result<String, OcrError> {
        Err(OcrError::Unavailable(NOT_YET.into()))
    }
}
