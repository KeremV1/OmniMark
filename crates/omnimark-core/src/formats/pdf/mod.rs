//! PDF to Markdown via pdfium: glyph boxes -> words -> form tables or headed text.

pub(crate) mod form;
pub(crate) mod layout;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pdfium_render::prelude::*;

use crate::error::{ConvertError, Result};
use crate::ocr::{
    block_plain, from_dynamic, DocumentOcr, OcrImage, DECODE_WARNING, MAX_DECODE_ALLOC,
    MAX_INPUT_DIMENSION, MAX_OCR_SIDE, NO_PAGE_TEXT,
};
use crate::registry::{ConvertContext, Converter};
use crate::{Document, FileInfo, Input};

use layout::{Glyph, PlacedBlock, Word};

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
/// Resolution a page without a text layer is rendered at for OCR.
const SCAN_DPI: f32 = 300.0;
/// A round ends once this many bytes of pixels wait for OCR.
const OCR_ROUND_BYTES: usize = 128 * 1024 * 1024;

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

#[cfg(test)]
thread_local! {
    /// True while this thread holds `PDFIUM` (tests check OCR never runs under it).
    static HOLDS_PDFIUM: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
struct HeldFlag;
#[cfg(test)]
impl HeldFlag {
    fn set() -> Self {
        HOLDS_PDFIUM.with(|h| h.set(true));
        Self
    }
}
#[cfg(test)]
impl Drop for HeldFlag {
    fn drop(&mut self) {
        HOLDS_PDFIUM.with(|h| h.set(false));
    }
}

/// One page as read under the lock; images are freed once OCR'd.
struct RawPage {
    words: Vec<Word>,
    /// Blank-preserving words for form detection.
    phrases: Vec<Word>,
    width: f32,
    images: Vec<(f32, OcrImage)>,
    scan: Option<OcrImage>,
}

struct PageData {
    words: Vec<Word>,
    phrases: Vec<Word>,
    width: f32,
    blocks: Vec<PlacedBlock>,
    /// `Some` for a page without a text layer when OCR is on: its recognized text, if any.
    scanned: Option<Option<String>>,
}

struct Round {
    pages: Vec<RawPage>,
    page_count: usize,
}

pub(crate) fn no_text_layer_warning(page: usize) -> String {
    format!("Page {page} has no text layer (scanned?); enable OCR to read it.")
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
        self.convert_with(input, ctx, OCR_ROUND_BYTES)
    }
}

impl PdfConverter {
    /// `convert` with an injectable round size (tests use tiny ones).
    fn convert_with(
        &self,
        input: &Input,
        ctx: &mut ConvertContext<'_>,
        round_bytes: usize,
    ) -> Result<Document> {
        let library = match &self.library {
            Some(path) if path.is_file() => path,
            Some(path) => return Err(missing_library(&path.display().to_string())),
            None => return Err(missing_library("(no path configured)")),
        };
        let mut ocr = DocumentOcr::new(ctx.options);
        let mut result = Document::default();
        let mut pages: Vec<PageData> = Vec::new();
        loop {
            let round = extract_round(
                library,
                &input.bytes,
                pages.len(),
                ocr.is_some(),
                round_bytes,
                ctx,
                &mut result,
            )?;
            let read = round.pages.len();
            for raw in round.pages {
                pages.push(finish_page(raw, ocr.as_mut(), ctx, &mut result)?);
                if ocr.is_some() {
                    ctx.report(pages.len() as f32 / round.page_count.max(1) as f32);
                }
            }
            if read == 0 || pages.len() >= round.page_count {
                break;
            }
        }
        result.markdown = render_pages(&pages, ocr.is_some());
        Ok(result)
    }
}

fn missing_library(path: &str) -> ConvertError {
    ConvertError::MissingComponent(format!("pdfium library not found at {path}"))
}

/// Reads pages from `start` under the pdfium lock until the end or until the pixels waiting
/// for OCR reach `round_bytes` ([`OCR_ROUND_BYTES`] in production); the lock and the document are released on return.
fn extract_round(
    library: &Path,
    bytes: &[u8],
    start: usize,
    ocr_on: bool,
    round_bytes: usize,
    ctx: &ConvertContext<'_>,
    result: &mut Document,
) -> Result<Round> {
    let mut guard = PDFIUM.lock().unwrap_or_else(|e| e.into_inner());
    #[cfg(test)]
    let _held = HeldFlag::set();
    if guard.is_none() {
        let bindings = Pdfium::bind_to_library(library)
            .map_err(|e| ConvertError::MissingComponent(format!("{}: {e}", library.display())))?;
        *guard = Some(Pdfium::new(bindings));
    }
    let pdfium = guard.as_ref().expect("pdfium bound above");

    let document = match pdfium.load_pdf_from_byte_slice(bytes, None) {
        Ok(document) => document,
        Err(PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError)) => {
            return Err(ConvertError::Encrypted)
        }
        Err(e) => return Err(ConvertError::corrupt(format!("{e:?}"))),
    };

    let pages = document.pages();
    let page_count = pages.len() as usize;
    let mut taken = Vec::new();
    let mut pending = 0usize;
    for (i, page) in pages.iter().enumerate().skip(start) {
        ctx.check_cancel()?;
        let glyphs = page_glyphs(&page)?;
        let words = layout::group_words(&glyphs);
        let mut images = Vec::new();
        let mut scan = None;
        if words.is_empty() {
            if ocr_on {
                scan = render_scan(&page, result);
            } else {
                result.warn(no_text_layer_warning(i + 1));
            }
        } else if ocr_on {
            images = page_images(&page, round_bytes.saturating_sub(pending), result);
        }
        pending += images.iter().map(|(_, im)| im.rgba().len()).sum::<usize>()
            + scan.as_ref().map_or(0, |im| im.rgba().len());
        taken.push(RawPage {
            phrases: layout::group_phrases(&glyphs),
            words,
            width: page.width().value,
            images,
            scan,
        });
        if !ocr_on {
            ctx.report((i + 1) as f32 / page_count as f32);
        } else if pending >= round_bytes {
            break;
        }
    }
    Ok(Round {
        pages: taken,
        page_count,
    })
}

/// Top-level image objects of a page as OCR input with their top edge (top-left origin).
/// Images past `budget` bytes of pixel estimate (`w*h*4`, from metadata) are skipped with a
/// warning; the first image is always read.
fn page_images(page: &PdfPage<'_>, budget: usize, result: &mut Document) -> Vec<(f32, OcrImage)> {
    let page_height = page.height().value;
    let mut images = Vec::new();
    let mut used = 0usize;
    for object in page.objects().iter() {
        let Some(image) = object.as_image_object() else {
            continue;
        };
        let (Ok(w), Ok(h)) = (image.width(), image.height()) else {
            result.warn_once(DECODE_WARNING);
            continue;
        };
        let too_big = w <= 0
            || h <= 0
            || w as u32 > MAX_INPUT_DIMENSION
            || h as u32 > MAX_INPUT_DIMENSION
            || (w as u64) * (h as u64) * 4 > MAX_DECODE_ALLOC;
        if too_big {
            result.warn_once(DECODE_WARNING);
            continue;
        }
        let estimate = (w as usize) * (h as usize) * 4;
        if used > 0 && used.saturating_add(estimate) > budget {
            result.warn_once(DECODE_WARNING);
            break;
        }
        used = used.saturating_add(estimate);
        let Ok(bounds) = object.bounds() else {
            result.warn_once(DECODE_WARNING);
            continue;
        };
        match image.get_raw_image() {
            Ok(decoded) => {
                if let Some(prepared) = from_dynamic(decoded) {
                    images.push((page_height - bounds.top().value, prepared));
                }
            }
            Err(_) => result.warn_once(DECODE_WARNING),
        }
    }
    images.sort_by(|a, b| a.0.total_cmp(&b.0));
    images
}

/// Draws a page without a text layer for OCR (300 DPI, long side at most 4096 px).
fn render_scan(page: &PdfPage<'_>, result: &mut Document) -> Option<OcrImage> {
    let (width, height) = (page.width().value, page.height().value);
    let scale = (SCAN_DPI / 72.0).min(MAX_OCR_SIDE as f32 / width.max(height).max(1.0));
    let config =
        PdfRenderConfig::new().set_target_width((width * scale).round().max(1.0) as Pixels);
    match page
        .render_with_config(&config)
        .and_then(|bitmap| bitmap.as_image())
    {
        Ok(image) => from_dynamic(image),
        Err(_) => {
            result.warn_once("A scanned page could not be rendered for OCR.");
            None
        }
    }
}

/// Recognizes the images of one page outside the pdfium lock and frees them.
fn finish_page(
    raw: RawPage,
    ocr: Option<&mut DocumentOcr>,
    ctx: &ConvertContext<'_>,
    result: &mut Document,
) -> Result<PageData> {
    let mut blocks = Vec::new();
    let mut scanned = None;
    if let Some(ocr) = ocr {
        for (top, image) in &raw.images {
            if let Some(text) = ocr.image(image, ctx.cancel, result)? {
                blocks.push(PlacedBlock {
                    top: *top,
                    markdown: block_plain(&text),
                });
            }
        }
        if raw.words.is_empty() {
            scanned = Some(match &raw.scan {
                Some(image) => ocr.image(image, ctx.cancel, result)?,
                None => None,
            });
        }
    }
    Ok(PageData {
        words: raw.words,
        phrases: raw.phrases,
        width: raw.width,
        blocks,
        scanned,
    })
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
fn render_pages(pages: &[PageData], ocr_on: bool) -> String {
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

    if !ocr_on {
        let chunks: Vec<String> = pages
            .iter()
            .zip(forms)
            .map(|(page, form)| match form {
                Some(content) => content,
                None => layout::render_text_page(&page.words, body, &heading_sizes, &[]),
            })
            .filter(|chunk| !chunk.trim().is_empty())
            .collect();
        return layout::merge_partial_numbering(&chunks.join("\n\n"));
    }
    let chunks: Vec<String> = pages
        .iter()
        .zip(forms)
        .enumerate()
        .map(|(i, (page, form))| {
            let content = match (&page.scanned, form) {
                (Some(text), _) => text
                    .as_deref()
                    .map(block_plain)
                    .unwrap_or_else(|| NO_PAGE_TEXT.to_string()),
                (None, Some(mut form)) => {
                    for block in &page.blocks {
                        form.push_str("\n\n");
                        form.push_str(&block.markdown);
                    }
                    form
                }
                (None, None) => {
                    layout::render_text_page(&page.words, body, &heading_sizes, &page.blocks)
                }
            };
            if content.is_empty() {
                format!("## Page {}", i + 1)
            } else {
                format!("## Page {}\n\n{content}", i + 1)
            }
        })
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
            "Page 1 has no text layer (scanned?); enable OCR to read it."
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

    use crate::ocr::{MockEngine, OcrEngine, OcrError, OcrLanguage};
    use std::sync::Arc;

    fn run_with(converter: &PdfConverter, path: &Path, engine: Arc<dyn OcrEngine>) -> Document {
        let input = Input {
            path: path.to_path_buf(),
            bytes: std::fs::read(path).unwrap(),
        };
        let options = ConvertOptions {
            ocr: Some(engine),
            ..ConvertOptions::default()
        };
        let mut sink = NullAssetSink;
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut sink,
            cancel: &cancel,
            progress: &progress,
        };
        converter.convert(&input, &mut ctx).unwrap()
    }

    fn ocr_fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/markitdown/ocr")
            .join(name)
    }

    /// Fails the test if recognition runs while this thread holds the pdfium lock.
    struct LockProbe;
    impl OcrEngine for LockProbe {
        fn name(&self) -> &'static str {
            "probe"
        }
        fn recognize(
            &self,
            _: &OcrImage,
            _: &OcrLanguage,
        ) -> std::result::Result<String, OcrError> {
            assert!(
                !HOLDS_PDFIUM.with(|held| held.get()),
                "OCR ran under the pdfium lock"
            );
            Ok("probe".into())
        }
    }

    #[test]
    fn scanned_page_is_rendered_and_recognized_outside_the_lock() {
        let Some(library) = real_library() else {
            return;
        };
        let converter = PdfConverter::new(Some(library));
        let engine = Arc::new(MockEngine::default());
        let doc = run_with(
            &converter,
            &ocr_fixture("pdf_scanned_minimal.pdf"),
            engine.clone(),
        );
        assert_eq!(
            doc.markdown,
            "## Page 1\n\n*[Image OCR]\nMOCK_OCR_TEXT_12345\n[End OCR]*"
        );
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
        assert_eq!(engine.calls(), 1);
        let probed = run_with(
            &converter,
            &ocr_fixture("pdf_image_middle.pdf"),
            Arc::new(LockProbe),
        );
        assert!(probed.markdown.contains("*[Image OCR]\nprobe\n[End OCR]*"));
    }

    #[test]
    fn empty_ocr_on_a_scanned_page_says_so() {
        let Some(library) = real_library() else {
            return;
        };
        let converter = PdfConverter::new(Some(library));
        let doc = run_with(
            &converter,
            &ocr_fixture("pdf_scanned_minimal.pdf"),
            Arc::new(MockEngine::new("  ")),
        );
        assert_eq!(
            doc.markdown,
            "## Page 1\n\n*[No text could be extracted from this page]*"
        );
    }

    fn convert_capped(
        converter: &PdfConverter,
        path: &Path,
        engine: Arc<dyn OcrEngine>,
        round_bytes: usize,
        cancel: &CancelToken,
    ) -> Result<Document> {
        let input = Input {
            path: path.to_path_buf(),
            bytes: std::fs::read(path).unwrap(),
        };
        let options = ConvertOptions {
            ocr: Some(engine),
            ..ConvertOptions::default()
        };
        let mut sink = NullAssetSink;
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut sink,
            cancel,
            progress: &progress,
        };
        converter.convert_with(&input, &mut ctx, round_bytes)
    }

    #[test]
    fn one_round_per_page_matches_a_single_round() {
        let Some(library) = real_library() else {
            return;
        };
        let converter = PdfConverter::new(Some(library));
        let path = ocr_fixture("pdf_multipage.pdf");
        let whole_engine = Arc::new(MockEngine::default());
        let whole = convert_capped(
            &converter,
            &path,
            whole_engine.clone(),
            OCR_ROUND_BYTES,
            &CancelToken::new(),
        )
        .unwrap();
        let split_engine = Arc::new(MockEngine::default());
        let split = convert_capped(
            &converter,
            &path,
            split_engine.clone(),
            1,
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(split.markdown, whole.markdown);
        assert_eq!(split.warnings, whole.warnings);
        assert_eq!(split_engine.calls(), whole_engine.calls());
        assert!(whole_engine.calls() >= 3);
    }

    #[test]
    fn cancelling_during_the_first_round_stops_the_conversion() {
        let Some(library) = real_library() else {
            return;
        };
        struct CancelOnUse(CancelToken);
        impl OcrEngine for CancelOnUse {
            fn name(&self) -> &'static str {
                "cancel"
            }
            fn recognize(
                &self,
                _: &OcrImage,
                _: &OcrLanguage,
            ) -> std::result::Result<String, OcrError> {
                self.0.cancel();
                Ok("x".into())
            }
        }
        let converter = PdfConverter::new(Some(library));
        let cancel = CancelToken::new();
        let err = convert_capped(
            &converter,
            &ocr_fixture("pdf_multipage.pdf"),
            Arc::new(CancelOnUse(cancel.clone())),
            1,
            &cancel,
        )
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Cancelled);
    }

    #[test]
    fn images_beyond_the_page_budget_are_skipped_with_a_warning() {
        let Some(library) = real_library() else {
            return;
        };
        let converter = PdfConverter::new(Some(library));
        let path = ocr_fixture("pdf_multiple_images.pdf");
        let engine = Arc::new(MockEngine::default());
        let doc =
            convert_capped(&converter, &path, engine.clone(), 1, &CancelToken::new()).unwrap();
        assert_eq!(engine.calls(), 1, "only the first image fits");
        assert!(doc.warnings.contains(&DECODE_WARNING.to_string()));
        let full = Arc::new(MockEngine::default());
        let all = convert_capped(
            &converter,
            &path,
            full,
            OCR_ROUND_BYTES,
            &CancelToken::new(),
        )
        .unwrap();
        assert!(!all.warnings.contains(&DECODE_WARNING.to_string()));
    }
}
