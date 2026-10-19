//! Per-document OCR cache keyed by the SHA-256 of the image (markitdown-ocr does the same).

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use super::OcrImage;

/// Recognized text per image within one document; failures are stored as `None`.
#[derive(Debug, Default)]
pub struct OcrCache {
    entries: HashMap<[u8; 32], Option<String>>,
}

impl OcrCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Key for encoded image bytes.
    pub fn key(bytes: &[u8]) -> [u8; 32] {
        Sha256::digest(bytes).into()
    }

    /// Key for decoded pixels (PDF image objects, rendered pages).
    pub fn pixel_key(image: &OcrImage) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(image.width().to_le_bytes());
        hasher.update(image.height().to_le_bytes());
        hasher.update(image.rgba());
        hasher.finalize().into()
    }

    pub fn get(&self, key: &[u8; 32]) -> Option<Option<String>> {
        self.entries.get(key).cloned()
    }

    pub fn insert(&mut self, key: [u8; 32], text: Option<String>) {
        self.entries.insert(key, text);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_sha256_and_failures_are_cached_too() {
        let mut cache = OcrCache::new();
        let key = OcrCache::key(b"abc");
        assert_eq!(key[..4], [0xba, 0x78, 0x16, 0xbf]);
        assert_eq!(cache.get(&key), None);
        cache.insert(key, None);
        assert_eq!(cache.get(&key), Some(None));
        cache.insert(key, Some("t".into()));
        assert_eq!(cache.get(&key), Some(Some("t".into())));
        assert_eq!(cache.len(), 1);
        let a = OcrImage::new(1, 2, vec![0; 8]).unwrap();
        let b = OcrImage::new(2, 1, vec![0; 8]).unwrap();
        assert_ne!(OcrCache::pixel_key(&a), OcrCache::pixel_key(&b));
    }
}
