//! Fake engine for tests and the golden runner (markitdown-ocr's tests use the same text).

use std::sync::atomic::{AtomicUsize, Ordering};

use super::{OcrEngine, OcrError, OcrImage, OcrLanguage};

pub const MOCK_TEXT: &str = "MOCK_OCR_TEXT_12345";

/// Returns a fixed reply for every image and counts the calls.
pub struct MockEngine {
    reply: Result<String, OcrError>,
    calls: AtomicUsize,
}

impl MockEngine {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            reply: Ok(text.into()),
            calls: AtomicUsize::new(0),
        }
    }

    pub fn failing(error: OcrError) -> Self {
        Self {
            reply: Err(error),
            calls: AtomicUsize::new(0),
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new(MOCK_TEXT)
    }
}

impl OcrEngine for MockEngine {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn recognize(&self, _image: &OcrImage, _language: &OcrLanguage) -> Result<String, OcrError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.reply.clone()
    }
}
