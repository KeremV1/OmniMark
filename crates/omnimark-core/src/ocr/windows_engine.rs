//! Windows.Media.Ocr engine: no download; uses the OCR languages installed in Windows.

use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine as WinOcr;
use windows::Storage::Streams::DataWriter;

use super::{OcrEngine, OcrError, OcrImage, OcrLanguage};

/// Installed recognizer languages as BCP-47 tags; empty when Windows OCR is unusable.
pub fn available_languages() -> Vec<String> {
    let Ok(languages) = WinOcr::AvailableRecognizerLanguages() else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    for language in &languages {
        if let Ok(tag) = language.LanguageTag() {
            tags.push(tag.to_string_lossy());
        }
    }
    tags
}

/// The installed tag whose primary subtag matches `wanted` (`en` matches `en-US`).
pub fn pick_language(available: &[String], wanted: OcrLanguage) -> Option<String> {
    let primary = wanted.primary_tag()?;
    available
        .iter()
        .find(|tag| {
            tag.split('-')
                .next()
                .is_some_and(|p| p.eq_ignore_ascii_case(primary))
        })
        .cloned()
}

/// Holds only the tag list: the WinRT engine is created per call, so the type is `Send + Sync`
/// without relying on the thread affinity of WinRT objects.
pub struct WindowsEngine {
    languages: Vec<String>,
}

impl WindowsEngine {
    pub fn new() -> Result<Self, OcrError> {
        let languages = available_languages();
        if languages.is_empty() {
            return Err(OcrError::Unavailable(
                "no Windows OCR language is installed".into(),
            ));
        }
        Ok(Self { languages })
    }

    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    fn engine_for(&self, language: OcrLanguage) -> Result<WinOcr, OcrError> {
        let unavailable = |e: windows::core::Error| OcrError::Unavailable(e.to_string());
        if let Some(tag) = pick_language(&self.languages, language) {
            let language =
                Language::CreateLanguage(&HSTRING::from(tag.as_str())).map_err(unavailable)?;
            if let Ok(engine) = WinOcr::TryCreateFromLanguage(&language) {
                return Ok(engine);
            }
        }
        WinOcr::TryCreateFromUserProfileLanguages().map_err(unavailable)
    }
}

impl OcrEngine for WindowsEngine {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn recognize(&self, image: &OcrImage, language: &OcrLanguage) -> Result<String, OcrError> {
        let failed = |e: windows::core::Error| OcrError::Engine(e.to_string());
        let max = WinOcr::MaxImageDimension().map_err(failed)?;
        if image.width() > max || image.height() > max {
            return Err(OcrError::Image(format!(
                "{}x{} exceeds the Windows OCR limit of {max} px",
                image.width(),
                image.height()
            )));
        }
        let engine = self.engine_for(*language)?;
        let writer = DataWriter::new().map_err(failed)?;
        writer.WriteBytes(image.rgba()).map_err(failed)?;
        let buffer = writer.DetachBuffer().map_err(failed)?;
        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Rgba8,
            image.width() as i32,
            image.height() as i32,
            BitmapAlphaMode::Ignore,
        )
        .map_err(failed)?;
        let bitmap = SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8).map_err(failed)?;
        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(failed)?
            .join()
            .map_err(failed)?;
        let mut lines = Vec::new();
        for line in result.Lines().map_err(failed)? {
            lines.push(line.Text().map_err(failed)?.to_string_lossy());
        }
        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::test_render::{render_text, system_font};
    use crate::ocr::{build_engine, EngineConfig, OcrChoice};

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn languages_match_on_the_primary_subtag() {
        assert_eq!(
            pick_language(&tags(&["tr", "en-US"]), OcrLanguage::En).as_deref(),
            Some("en-US")
        );
        assert_eq!(
            pick_language(&tags(&["TR"]), OcrLanguage::Tr).as_deref(),
            Some("TR")
        );
        assert_eq!(pick_language(&tags(&["tr"]), OcrLanguage::En), None);
        assert_eq!(pick_language(&tags(&["tr"]), OcrLanguage::Auto), None);
    }

    /// Skipped when Windows has no OCR language installed.
    #[test]
    fn reads_rendered_turkish_and_english_text() {
        if available_languages().is_empty() {
            eprintln!("no Windows OCR language installed; skipping");
            return;
        }
        let Some(font) = system_font() else {
            eprintln!("no system font; skipping");
            return;
        };
        let engine = WindowsEngine::new().unwrap();
        for (text, language, expected) in [
            (
                "Merhaba dünya, bugün hava güzel.",
                OcrLanguage::Tr,
                "Merhaba",
            ),
            (
                "The quick brown fox jumps over 12 lazy dogs.",
                OcrLanguage::En,
                "quick brown",
            ),
        ] {
            let image = render_text(text, &font, 32.0);
            let result = engine.recognize(&image, &language).unwrap();
            println!("{language:?}: {result:?}");
            assert!(result.contains(expected), "{language:?}: {result:?}");
        }
    }

    #[test]
    fn auto_selects_windows_when_a_language_is_installed() {
        if available_languages().is_empty() {
            return;
        }
        assert_eq!(
            build_engine(OcrChoice::Auto, &EngineConfig::default())
                .unwrap()
                .name(),
            "windows"
        );
    }
}
