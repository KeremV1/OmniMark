//! Optional text recognition (OCR): the engine interface, image preparation, the per-document
//! cache and the Markdown blocks. Real engines live behind the `ocr` (ocrs) and `ocr-windows`
//! (Windows.Media.Ocr) Cargo features; the Tesseract engine runs an installed program and needs
//! no feature. With `ConvertOptions::ocr == None` nothing here runs.

mod block;
mod cache;
pub mod choice;
mod decode;
mod doc;
mod mock;
#[cfg(feature = "ocr")]
pub mod models;
#[cfg(feature = "ocr")]
pub mod ocrs_engine;
#[cfg(test)]
mod quality;
pub mod tesseract_engine;
#[cfg(test)]
pub(crate) mod test_render;
#[cfg(all(windows, feature = "ocr-windows"))]
pub mod windows_engine;

pub use block::{
    block_inline, block_markdown, block_plain, escape_ocr_text, NO_PAGE_TEXT, OCR_CLOSE, OCR_OPEN,
};
pub use cache::OcrCache;
pub use choice::{
    availability, build_engine, select, unavailable_message, Availability, EngineConfig,
    EngineKind, OcrChoice,
};
pub use decode::{
    decode, from_dynamic, MAX_DECODE_ALLOC, MAX_INPUT_DIMENSION, MAX_OCR_SIDE, MIN_OCR_SIDE,
};
pub use doc::{clean_text, DocumentOcr, DECODE_WARNING};
pub use mock::{MockEngine, MOCK_TEXT};

use serde::{Deserialize, Serialize};

/// A text recognizer. Created once per batch and shared by the worker threads.
pub trait OcrEngine: Send + Sync {
    /// Short identifier: `"windows"`, `"ocrs"`, `"mock"`.
    fn name(&self) -> &'static str;

    /// Text of one prepared image, lines separated by `\n`.
    fn recognize(&self, image: &OcrImage, language: &OcrLanguage) -> Result<String, OcrError>;
}

/// Recognition language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OcrLanguage {
    /// The system's languages (Windows: the user profile languages).
    #[default]
    Auto,
    Tr,
    En,
}

impl OcrLanguage {
    /// BCP-47 primary subtag; `None` for `Auto`.
    pub fn primary_tag(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Tr => Some("tr"),
            Self::En => Some("en"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OcrError {
    #[error("OCR engine is not available: {0}")]
    Unavailable(String),
    #[error("image could not be read: {0}")]
    Image(String),
    #[error("OCR failed: {0}")]
    Engine(String),
}

/// An RGBA image prepared for OCR: decoded within the limits and scaled (see [`decode`]).
#[derive(Clone, PartialEq, Eq)]
pub struct OcrImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl OcrImage {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, OcrError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4));
        if width == 0 || height == 0 || expected != Some(rgba.len()) {
            return Err(OcrError::Image(format!(
                "{width}x{height} image with {} bytes of RGBA data",
                rgba.len()
            )));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

impl std::fmt::Debug for OcrImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OcrImage({}x{})", self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_image_checks_its_buffer_length() {
        assert!(OcrImage::new(2, 2, vec![0; 16]).is_ok());
        assert!(OcrImage::new(2, 2, vec![0; 15]).is_err());
        assert!(OcrImage::new(0, 2, Vec::new()).is_err());
        assert_eq!(
            format!("{:?}", OcrImage::new(2, 1, vec![0; 8]).unwrap()),
            "OcrImage(2x1)"
        );
    }

    #[test]
    fn languages_serialize_in_camel_case_and_expose_primary_tags() {
        assert_eq!(serde_json::to_string(&OcrLanguage::Tr).unwrap(), "\"tr\"");
        assert_eq!(
            serde_json::from_str::<OcrLanguage>("\"auto\"").unwrap(),
            OcrLanguage::Auto
        );
        assert_eq!(OcrLanguage::En.primary_tag(), Some("en"));
        assert_eq!(OcrLanguage::Auto.primary_tag(), None);
    }

    #[test]
    fn convert_options_default_to_no_ocr_and_debug_names_the_engine() {
        let options = crate::ConvertOptions::default();
        assert!(options.ocr.is_none());
        assert_eq!(options.ocr_language, OcrLanguage::Auto);
        let with = crate::ConvertOptions {
            ocr: Some(std::sync::Arc::new(MockEngine::default())),
            ..crate::ConvertOptions::default()
        };
        assert!(format!("{with:?}").contains("Some(\"mock\")"));
    }
}
