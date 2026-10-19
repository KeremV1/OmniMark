//! Windows.Media.Ocr engine. Skeleton from Task M2-0; Task M2-3 implements it.

use super::{OcrEngine, OcrError, OcrImage, OcrLanguage};

/// Installed recognizer languages as BCP-47 tags; empty when Windows OCR is unusable.
pub fn available_languages() -> Vec<String> {
    Vec::new()
}

pub struct WindowsEngine;

impl WindowsEngine {
    pub fn new() -> Result<Self, OcrError> {
        Err(OcrError::Unavailable(
            "Windows OCR is not implemented yet".into(),
        ))
    }
}

impl OcrEngine for WindowsEngine {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn recognize(&self, _image: &OcrImage, _language: &OcrLanguage) -> Result<String, OcrError> {
        Err(OcrError::Unavailable(
            "Windows OCR is not implemented yet".into(),
        ))
    }
}
