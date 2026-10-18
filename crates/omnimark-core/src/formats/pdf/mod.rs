//! PDF to Markdown via pdfium: glyph boxes -> words -> form tables or headed text.

pub(crate) mod form;
pub(crate) mod layout;

use std::path::PathBuf;
use std::sync::Mutex;

use pdfium_render::prelude::*;

use crate::error::{ConvertError, Result};
use crate::registry::{ConvertContext, Converter};
use crate::{Document, FileInfo, Input};

use layout::{Glyph, Word};

/// File name of the pdfium shared library on this platform.
pub fn pdfium_library_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}

/// Weight at or above which a glyph counts as bold.
const BOLD_WEIGHT: u32 = 600;
const BOLD_FONT_NAMES: [&str; 5] = ["Bold", "Black", "Heavy", "-Medi", "Semibold"];

/// The bound pdfium, shared by all conversions (one lock serialises PDF work). Only a
/// successful bind is cached, so a missing or half-downloaded library can be retried later.
/// Once bound, the first library path stays in use; a later call with another path reuses it.
static PDFIUM: Mutex<Option<Pdfium>> = Mutex::new(None);

pub struct PdfConverter {
    library: Option<PathBuf>,
}

impl PdfConverter {
    pub fn new(library: Option<PathBuf>) -> Self {
        Self { library }
    }
}

struct PageData {
    words: Vec<Word>,
    /// Blank-preserving words for form detection.
    phrases: Vec<Word>,
    width: f32,
}

impl Converter for PdfConverter {
    fn name(&self) -> &'static str {
        "pdf"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["pdf"])
            || info.mime_starts_with(&["application/pdf", "application/x-pdf"])
    }

    fn convert(&self, input: &Input, ctx: &mut ConvertContext<'_>) -> Result<Document> {
        let library = match &self.library {
            Some(path) if path.is_file() => path,
            Some(path) => return Err(missing_library(&path.display().to_string())),
            None => return Err(missing_library("(no path configured)")),
        };
        let mut guard = PDFIUM.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            let bindings = Pdfium::bind_to_library(library).map_err(|e| {
                ConvertError::MissingComponent(format!("{}: {e}", library.display()))
            })?;
            *guard = Some(Pdfium::new(bindings));
        }
        let pdfium = guard.as_ref().expect("pdfium bound above");

        let document = match pdfium.load_pdf_from_byte_slice(&input.bytes, None) {
            Ok(document) => document,
            Err(PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError)) => {
                return Err(ConvertError::Encrypted)
            }
            Err(e) => return Err(ConvertError::corrupt(format!("{e:?}"))),
        };

        let mut result = Document::default();
        let pages = document.pages();
        let page_count = pages.len() as usize;
        let mut page_data = Vec::with_capacity(page_count);
        for (i, page) in pages.iter().enumerate() {
            ctx.check_cancel()?;
            let glyphs = page_glyphs(&page)?;
            let words = layout::group_words(&glyphs);
            if words.is_empty() {
                result.warn(format!(
                    "Page {} has no text layer (scanned?); OCR is planned for a later version.",
                    i + 1
                ));
            }
            page_data.push(PageData {
                phrases: layout::group_phrases(&glyphs),
                words,
                width: page.width().value,
            });
            ctx.report((i + 1) as f32 / page_count as f32);
        }

        result.markdown = render_pages(&page_data);
        Ok(result)
    }
}

fn missing_library(path: &str) -> ConvertError {
    ConvertError::MissingComponent(format!("pdfium library not found at {path}"))
}

/// Characters of a page with top-left-origin boxes. Characters without bounds are skipped.
fn page_glyphs(page: &PdfPage<'_>) -> Result<Vec<Glyph>> {
    let page_height = page.height().value;
    let text = page.text().map_err(ConvertError::corrupt)?;
    let mut glyphs = Vec::new();
    for ch in text.chars().iter() {
        let Some(unicode) = ch.unicode_char() else {
            continue;
        };
        let Ok(rect) = ch.loose_bounds() else {
            continue;
        };
        let weight = ch.font_weight().map(weight_value).unwrap_or(0);
        let font_name = ch.font_name();
        glyphs.push(Glyph {
            ch: unicode,
            left: rect.left().value,
            right: rect.right().value,
            top: page_height - rect.top().value,
            bottom: page_height - rect.bottom().value,
            size: ch.scaled_font_size().value,
            bold: weight >= BOLD_WEIGHT
                || BOLD_FONT_NAMES.iter().any(|n| font_name.contains(n))
                || ch.font_is_bold_reenforced(),
            generated: ch.is_generated().unwrap_or(false),
        });
    }
    Ok(glyphs)
}

fn weight_value(weight: PdfFontWeight) -> u32 {
    match weight {
        PdfFontWeight::Weight100 => 100,
        PdfFontWeight::Weight200 => 200,
        PdfFontWeight::Weight300 => 300,
        PdfFontWeight::Weight400Normal => 400,
        PdfFontWeight::Weight500 => 500,
        PdfFontWeight::Weight600 => 600,
        PdfFontWeight::Weight700Bold => 700,
        PdfFontWeight::Weight800 => 800,
        PdfFontWeight::Weight900 => 900,
        PdfFontWeight::Custom(n) => n,
    }
}

/// Turns the extracted words of all pages into the document Markdown.
fn render_pages(pages: &[PageData]) -> String {
    let all_words: Vec<Word> = pages.iter().flat_map(|p| p.words.iter().cloned()).collect();
    let body = layout::body_size(&all_words);

    let forms: Vec<Option<String>> = pages
        .iter()
        .map(|p| form::extract_form_content(&p.phrases, p.width))
        .collect();
    let heading_candidates = pages
        .iter()
        .zip(&forms)
        .filter(|(_, form)| form.is_none())
        .flat_map(|(p, _)| layout::heading_sizes(&p.words, body))
        .collect();
    let heading_sizes = layout::finalize_heading_sizes(heading_candidates);

    let chunks: Vec<String> = pages
        .iter()
        .zip(forms)
        .map(|(page, form)| match form {
            Some(content) => content,
            None => layout::render_text_page(&page.words, body, &heading_sizes),
        })
        .filter(|chunk| !chunk.trim().is_empty())
        .collect();
    layout::merge_partial_numbering(&chunks.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::NullAssetSink;
    use crate::error::ErrorKind;
    use crate::{CancelToken, ConvertOptions};
    use std::path::Path;

    #[test]
    fn missing_library_is_a_missing_component_error() {
        let converter = PdfConverter::new(Some(PathBuf::from("Z:/nope/pdfium.dll")));
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/markitdown/files/test.pdf");
        let input = Input {
            path: "test.pdf".into(),
            bytes: std::fs::read(fixture).unwrap(),
        };
        let options = ConvertOptions::default();
        let mut sink = NullAssetSink;
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut sink,
            cancel: &cancel,
            progress: &progress,
        };
        let err = converter.convert(&input, &mut ctx).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::MissingComponent);
        assert!(err.to_string().contains("pdfium library not found at"));
    }

    #[test]
    fn no_configured_library_is_a_missing_component_error() {
        let converter = PdfConverter::new(None);
        let input = Input {
            path: "x.pdf".into(),
            bytes: b"%PDF-1.4".to_vec(),
        };
        let options = ConvertOptions::default();
        let mut sink = NullAssetSink;
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut sink,
            cancel: &cancel,
            progress: &progress,
        };
        let err = converter.convert(&input, &mut ctx).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::MissingComponent);
    }

    fn run(converter: &PdfConverter, name: &str, bytes: Vec<u8>) -> Result<Document> {
        let input = Input {
            path: name.into(),
            bytes,
        };
        let options = ConvertOptions::default();
        let mut sink = NullAssetSink;
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut sink,
            cancel: &cancel,
            progress: &progress,
        };
        converter.convert(&input, &mut ctx)
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/markitdown/files")
                .join(name),
        )
        .unwrap()
    }

    fn real_library() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("OMNIMARK_PDFIUM_PATH") {
            return Some(PathBuf::from(path));
        }
        let vendored = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/pdfium")
            .join(pdfium_library_file_name());
        vendored.is_file().then_some(vendored)
    }

    #[test]
    fn failed_bind_is_retried_with_the_real_library() {
        let Some(library) = real_library() else {
            eprintln!("pdfium not available; skipping");
            return;
        };
        let bad = PdfConverter::new(Some(PathBuf::from("Z:/nope/pdfium.dll")));
        let err = run(&bad, "test.pdf", fixture("test.pdf")).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::MissingComponent);
        let good = PdfConverter::new(Some(library));
        let doc = run(&good, "test.pdf", fixture("test.pdf")).unwrap();
        assert!(doc.markdown.contains("contemporaneous exploration"));
        // Once bound, a nonexistent path still fails fast without losing the binding.
        assert!(run(&bad, "test.pdf", fixture("test.pdf")).is_err());
        assert!(run(&good, "test.pdf", fixture("test.pdf")).is_ok());
    }

    #[test]
    fn garbage_bytes_are_corrupt() {
        let Some(library) = real_library() else {
            eprintln!("pdfium not available; skipping");
            return;
        };
        let converter = PdfConverter::new(Some(library));
        let err = run(&converter, "x.pdf", b"this is not a pdf".to_vec()).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Corrupt);
    }

    #[test]
    fn scanned_page_gives_empty_output_and_exact_warning() {
        let Some(library) = real_library() else {
            eprintln!("pdfium not available; skipping");
            return;
        };
        let converter = PdfConverter::new(Some(library));
        let doc = run(
            &converter,
            "scan.pdf",
            fixture("MEDRPT-2024-PAT-3847_medical_report_scan.pdf"),
        )
        .unwrap();
        assert_eq!(doc.markdown, "");
        assert!(!doc.warnings.is_empty());
        assert_eq!(
            doc.warnings[0],
            "Page 1 has no text layer (scanned?); OCR is planned for a later version."
        );
    }

    #[test]
    fn real_pdfs_produce_headings_or_bullets() {
        let Some(library) = real_library() else {
            eprintln!("pdfium not available; skipping");
            return;
        };
        let converter = PdfConverter::new(Some(library));
        let mut found = Vec::new();
        for name in [
            "RECEIPT-2024-TXN-98765_retail_purchase.pdf",
            "test.pdf",
            "masterformat_partial_numbering.pdf",
            "pdf_cleanup_plain.pdf",
        ] {
            let doc = run(&converter, name, fixture(name)).unwrap();
            if doc
                .markdown
                .lines()
                .any(|l| l.starts_with("# ") || l.starts_with("- "))
            {
                found.push(name);
            }
        }
        assert!(
            found.contains(&"RECEIPT-2024-TXN-98765_retail_purchase.pdf"),
            "{found:?}"
        );
    }
}
