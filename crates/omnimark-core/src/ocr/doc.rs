//! OCR state of one document conversion.

use std::sync::Arc;

use super::OcrError;
use super::{decode, OcrCache, OcrEngine, OcrImage, OcrLanguage};
use crate::cancel::CancelToken;
use crate::document::{ConvertOptions, Document};
use crate::error::{ConvertError, Result};

/// Added once per document when an image cannot be decoded for OCR.
pub const DECODE_WARNING: &str = "Some images could not be read for OCR and were skipped.";

/// Most engine calls (cache hits are free) made for one document.
pub const MAX_OCR_IMAGES: usize = 1000;
/// Added once per document when [`MAX_OCR_IMAGES`] is reached.
pub const OCR_LIMIT_WARNING: &str = "OCR stopped after 1000 images in this document.";

/// The batch's engine, the language and a cache that lives for one document.
pub struct DocumentOcr {
    engine: Arc<dyn OcrEngine>,
    language: OcrLanguage,
    cache: OcrCache,
    calls: usize,
}

impl DocumentOcr {
    /// `None` when OCR is off.
    pub fn new(options: &ConvertOptions) -> Option<Self> {
        options.ocr.clone().map(|engine| Self {
            engine,
            language: options.ocr_language,
            cache: OcrCache::new(),
            calls: 0,
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
        if self.limit_reached(doc) {
            return Ok(None);
        }
        let text = match decode(bytes) {
            Ok(Some(image)) => self.run(&image, doc)?,
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
        let text = self.run(image, doc)?;
        self.cache.insert(key, text.clone());
        Ok(text)
    }

    /// True (with a one-time warning) once the per-document engine-call limit is used up.
    fn limit_reached(&self, doc: &mut Document) -> bool {
        let reached = self.calls >= MAX_OCR_IMAGES;
        if reached {
            doc.warn_once(OCR_LIMIT_WARNING);
        }
        reached
    }

    /// Only cancellation is an error; an engine failure becomes a warning.
    fn run(&mut self, image: &OcrImage, doc: &mut Document) -> Result<Option<String>> {
        if self.limit_reached(doc) {
            return Ok(None);
        }
        self.calls += 1;
        match self.engine.recognize(image, &self.language) {
            Ok(text) => Ok(clean_text(&text)),
            Err(OcrError::Cancelled) => Err(ConvertError::Cancelled),
            Err(error) => {
                doc.warn_once(&format!("Image OCR failed: {error}"));
                Ok(None)
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
    fn an_engine_reporting_cancellation_cancels_the_conversion() {
        let engine = Arc::new(MockEngine::failing(OcrError::Cancelled));
        let mut ocr = ocr_with(&engine);
        let mut doc = Document::default();
        let err = ocr
            .image_bytes(&png_bytes(20, 20, [0; 4]), &CancelToken::new(), &mut doc)
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Cancelled);
        assert!(doc.warnings.is_empty());
    }

    #[test]
    fn ocr_stops_after_the_per_document_image_limit() {
        let engine = Arc::new(MockEngine::default());
        let mut ocr = ocr_with(&engine);
        let mut doc = Document::default();
        let cancel = CancelToken::new();
        let png = |n: u32| png_bytes(20 + n % 500, 20 + n / 500, [0, 0, 0, 255]);
        for n in 0..MAX_OCR_IMAGES as u32 {
            ocr.image_bytes(&png(n), &cancel, &mut doc).unwrap();
        }
        assert_eq!(engine.calls(), MAX_OCR_IMAGES);
        assert!(doc.warnings.is_empty());
        // A cache hit still answers.
        assert!(ocr
            .image_bytes(&png(0), &cancel, &mut doc)
            .unwrap()
            .is_some());
        let over = ocr.image_bytes(&png(5000), &cancel, &mut doc).unwrap();
        assert_eq!(over, None);
        // Over the limit nothing is decoded: junk bytes give the limit warning, not a decode one.
        assert_eq!(ocr.image_bytes(b"junk", &cancel, &mut doc).unwrap(), None);
        ocr.image_bytes(&png(5001), &cancel, &mut doc).unwrap();
        assert_eq!(engine.calls(), MAX_OCR_IMAGES);
        assert_eq!(doc.warnings, vec![OCR_LIMIT_WARNING.to_string()]);
    }

    #[test]
    fn clean_text_unifies_line_endings_and_trims() {
        assert_eq!(clean_text("  a \r\nb\r\r").as_deref(), Some("a\nb"));
        assert_eq!(clean_text(" \n \t"), None);
    }
}
