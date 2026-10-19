use std::sync::Arc;

use crate::ocr::{OcrEngine, OcrLanguage};

/// Files larger than this are rejected with `ConvertError::TooLarge`.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 500 * 1024 * 1024;

/// The result of converting one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub markdown: String,
    pub title: Option<String>,
    /// Content that was skipped or degraded; shown to the user.
    pub warnings: Vec<String>,
}

impl Document {
    pub fn new(markdown: impl Into<String>) -> Self {
        Self {
            markdown: markdown.into(),
            ..Self::default()
        }
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    /// Adds `message` unless the document already carries it.
    pub fn warn_once(&mut self, message: &str) {
        if !self.warnings.iter().any(|w| w == message) {
            self.warnings.push(message.to_string());
        }
    }
}

#[derive(Clone)]
pub struct ConvertOptions {
    /// Write embedded images next to the Markdown file.
    pub extract_images: bool,
    pub max_file_size: u64,
    /// OCR engine for this batch; `None` = OCR off (output identical to M1).
    pub ocr: Option<Arc<dyn OcrEngine>>,
    pub ocr_language: OcrLanguage,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            extract_images: true,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            ocr: None,
            ocr_language: OcrLanguage::Auto,
        }
    }
}

impl std::fmt::Debug for ConvertOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvertOptions")
            .field("extract_images", &self.extract_images)
            .field("max_file_size", &self.max_file_size)
            .field("ocr", &self.ocr.as_ref().map(|engine| engine.name()))
            .field("ocr_language", &self.ocr_language)
            .finish()
    }
}
