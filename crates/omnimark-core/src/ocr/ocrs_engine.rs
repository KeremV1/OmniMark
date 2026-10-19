//! ocrs engine (pure Rust, Latin script only). Skeleton from Task M2-0; Task M2-2 implements it.

use std::path::Path;

use super::{OcrEngine, OcrError, OcrImage, OcrLanguage};

pub struct OcrsEngine;

impl OcrsEngine {
    /// Loads the models from `<models_root>/ocrs/`.
    pub fn load(_models_root: &Path) -> Result<Self, OcrError> {
        Err(OcrError::Unavailable(
            "ocrs engine is not implemented yet".into(),
        ))
    }
}

impl OcrEngine for OcrsEngine {
    fn name(&self) -> &'static str {
        "ocrs"
    }

    fn recognize(&self, _image: &OcrImage, _language: &OcrLanguage) -> Result<String, OcrError> {
        Err(OcrError::Unavailable(
            "ocrs engine is not implemented yet".into(),
        ))
    }
}
