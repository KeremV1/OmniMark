//! ocrs engine (pure Rust on rten). Latin script only: its alphabet has no Turkish letters
//! (ğ ş ı İ …), so the language hint is ignored.

use std::path::Path;

use ocrs::{ImageSource, OcrEngineParams};
use rten::Model;

use super::models::{ocrs_dir, verify, DETECTION_MODEL, OCRS_MODELS, RECOGNITION_MODEL};
use super::{OcrEngine, OcrError, OcrImage, OcrLanguage};

pub struct OcrsEngine {
    engine: ocrs::OcrEngine,
}

impl OcrsEngine {
    /// Verifies (size + SHA-256) and loads both models from `<models_root>/ocrs/`.
    pub fn load(models_root: &Path) -> Result<Self, OcrError> {
        let dir = ocrs_dir(models_root);
        for file in &OCRS_MODELS {
            if !verify(&dir.join(file.name), file).unwrap_or(false) {
                return Err(OcrError::Unavailable(format!(
                    "ocrs model {} is missing or damaged; download it again",
                    file.name
                )));
            }
        }
        let load = |name: &str| {
            Model::load_file(dir.join(name))
                .map_err(|e| OcrError::Unavailable(format!("{name}: {e}")))
        };
        let engine = ocrs::OcrEngine::new(OcrEngineParams {
            detection_model: Some(load(DETECTION_MODEL)?),
            recognition_model: Some(load(RECOGNITION_MODEL)?),
            ..OcrEngineParams::default()
        })
        .map_err(|e| OcrError::Unavailable(e.to_string()))?;
        Ok(Self { engine })
    }
}

impl OcrEngine for OcrsEngine {
    fn name(&self) -> &'static str {
        "ocrs"
    }

    fn recognize(&self, image: &OcrImage, _language: &OcrLanguage) -> Result<String, OcrError> {
        let source = ImageSource::from_bytes(image.rgba(), (image.width(), image.height()))
            .map_err(|e| OcrError::Engine(e.to_string()))?;
        let input = self
            .engine
            .prepare_input(source)
            .map_err(|e| OcrError::Engine(e.to_string()))?;
        let text = self
            .engine
            .get_text(&input)
            .map_err(|e| OcrError::Engine(e.to_string()))?;
        // Single characters are mostly spurious detections (ocrs example does the same).
        Ok(text
            .lines()
            .filter(|line| line.trim().chars().count() > 1)
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::test_render::{render_text, system_font};

    #[test]
    fn missing_models_are_unavailable() {
        let root = tempfile::tempdir().unwrap();
        match OcrsEngine::load(root.path()) {
            Err(OcrError::Unavailable(message)) => {
                assert!(message.contains("text-detection.onnx"), "{message}")
            }
            Err(other) => panic!("unexpected error {other}"),
            Ok(_) => panic!("loaded without models"),
        }
    }

    /// Runs only with `OMNIMARK_OCR_MODELS=<models root>` (CI downloads and caches the models).
    #[test]
    fn recognizes_rendered_english_text() {
        let Some(root) = std::env::var_os("OMNIMARK_OCR_MODELS") else {
            eprintln!("OMNIMARK_OCR_MODELS not set; skipping");
            return;
        };
        let Some(font) = system_font() else {
            eprintln!("no system font; skipping");
            return;
        };
        let engine = OcrsEngine::load(Path::new(&root)).unwrap();
        let image = render_text("The quick brown fox jumps over 12 lazy dogs.", &font, 32.0);
        let text = engine.recognize(&image, &OcrLanguage::En).unwrap();
        assert!(text.contains("quick brown fox"), "{text:?}");
    }
}
