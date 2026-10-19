//! OCR state of one document conversion.

use std::sync::Arc;

use super::{decode, OcrCache, OcrEngine, OcrImage, OcrLanguage};
use crate::cancel::CancelToken;
use crate::document::{ConvertOptions, Document};
use crate::error::Result;

/// Added once per document when an image cannot be decoded for OCR.
pub const DECODE_WARNING: &str = "Some images could not be read for OCR and were skipped.";

/// The batch's engine, the language and a cache that lives for one document.
pub struct DocumentOcr {
    engine: Arc<dyn OcrEngine>,
    language: OcrLanguage,
    cache: OcrCache,
}

impl DocumentOcr {
    /// `None` when OCR is off.
    pub fn new(options: &ConvertOptions) -> Option<Self> {
        options.ocr.clone().map(|engine| Self {
            engine,
            language: options.ocr_language,
            cache: OcrCache::new(),
        })
    }

    pub fn engine_name(&self) -> &'static str {
        self.engine.name()
    }

    /// Text of an encoded image. `Ok(None)` when it holds no text or cannot be read (with a
    /// warning); only cancellation is an error.
    pub fn image_bytes(
        &mut self,
        bytes: &[u8],
        cancel: &CancelToken,
        doc: &mut Document,
    ) -> Result<Option<String>> {
        cancel.check()?;
        let key = OcrCache::key(bytes);
        if let Some(hit) = self.cache.get(&key) {
            return Ok(hit);
        }
        let text = match decode(bytes) {
            Ok(Some(image)) => self.run(&image, doc),
            Ok(None) => None,
            Err(_) => {
                doc.warn_once(DECODE_WARNING);
                None
            }
        };
        self.cache.insert(key, text.clone());
        Ok(text)
    }

    /// Text of an already prepared image (PDF image objects and rendered pages).
    pub fn image(
        &mut self,
        image: &OcrImage,
        cancel: &CancelToken,
        doc: &mut Document,
    ) -> Result<Option<String>> {
        cancel.check()?;
        let key = OcrCache::pixel_key(image);
        if let Some(hit) = self.cache.get(&key) {
            return Ok(hit);
        }
        let text = self.run(image, doc);
        self.cache.insert(key, text.clone());
        Ok(text)
    }

    fn run(&self, image: &OcrImage, doc: &mut Document) -> Option<String> {
        match self.engine.recognize(image, &self.language) {
            Ok(text) => clean_text(&text),
            Err(error) => {
                doc.warn_once(&format!("Image OCR failed: {error}"));
                None
            }
        }
    }
}

/// Unifies line endings, trims the end of every line and the whole text; `None` if empty.
pub fn clean_text(text: &str) -> Option<String> {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    let joined = unified
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    let trimmed = joined.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::ocr::test_render::png_bytes;
    use crate::ocr::{MockEngine, OcrError, MOCK_TEXT};

    fn ocr_with(engine: &Arc<MockEngine>) -> DocumentOcr {
        let options = ConvertOptions {
            ocr: Some(engine.clone() as Arc<dyn OcrEngine>),
            ..ConvertOptions::default()
        };
        DocumentOcr::new(&options).unwrap()
    }

    #[test]
    fn off_without_an_engine() {
        assert!(DocumentOcr::new(&ConvertOptions::default()).is_none());
    }

    #[test]
    fn identical_images_are_recognized_once() {
        let engine = Arc::new(MockEngine::default());
        let mut ocr = ocr_with(&engine);
        let mut doc = Document::default();
        let cancel = CancelToken::new();
        let png = png_bytes(20, 20, [0, 0, 0, 255]);
        for _ in 0..2 {
            let text = ocr.image_bytes(&png, &cancel, &mut doc).unwrap();
            assert_eq!(text.as_deref(), Some(MOCK_TEXT));
        }
        let pixels = crate::ocr::decode(&png).unwrap().unwrap();
        ocr.image(&pixels, &cancel, &mut doc).unwrap();
        ocr.image(&pixels, &cancel, &mut doc).unwrap();
        assert_eq!(engine.calls(), 2);
        assert!(doc.warnings.is_empty());
        assert_eq!(ocr.engine_name(), "mock");
    }

    #[test]
    fn engine_failures_warn_once_and_keep_going() {
        let engine = Arc::new(MockEngine::failing(OcrError::Engine("boom".into())));
        let mut ocr = ocr_with(&engine);
        let mut doc = Document::default();
        let cancel = CancelToken::new();
        assert_eq!(
            ocr.image_bytes(&png_bytes(20, 20, [0; 4]), &cancel, &mut doc)
                .unwrap(),
            None
        );
        assert_eq!(
            ocr.image_bytes(&png_bytes(21, 20, [0; 4]), &cancel, &mut doc)
                .unwrap(),
            None
        );
        assert_eq!(
            doc.warnings,
            vec!["Image OCR failed: OCR failed: boom".to_string()]
        );
    }

    #[test]
    fn unreadable_images_warn_once_without_calling_the_engine() {
        let engine = Arc::new(MockEngine::default());
        let mut ocr = ocr_with(&engine);
        let mut doc = Document::default();
        let cancel = CancelToken::new();
        assert_eq!(ocr.image_bytes(b"junk", &cancel, &mut doc).unwrap(), None);
        assert_eq!(ocr.image_bytes(b"junk2", &cancel, &mut doc).unwrap(), None);
        assert_eq!(doc.warnings, vec![DECODE_WARNING.to_string()]);
        assert_eq!(engine.calls(), 0);
    }

    #[test]
    fn cancellation_is_checked_before_each_image() {
        let engine = Arc::new(MockEngine::default());
        let mut ocr = ocr_with(&engine);
        let cancel = CancelToken::new();
        cancel.cancel();
        let err = ocr
            .image_bytes(
                &png_bytes(20, 20, [0; 4]),
                &cancel,
                &mut Document::default(),
            )
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Cancelled);
        assert_eq!(engine.calls(), 0);
    }

    #[test]
    fn clean_text_unifies_line_endings_and_trims() {
        assert_eq!(clean_text("  a \r\nb\r\r").as_deref(), Some("a\nb"));
        assert_eq!(clean_text(" \n \t"), None);
    }
}
