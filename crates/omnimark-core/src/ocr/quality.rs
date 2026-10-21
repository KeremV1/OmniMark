//! OCR quality measurement: character error rate (CER) and time of each available engine
//! (Windows, ocrs) on
//! generated Turkish/English text and on the page image of `pdf_scanned_minimal.pdf`. Manual:
//! `cargo test -p omnimark-core --release --features ocr,ocr-windows ocr::quality -- --ignored --nocapture`
//! with `OMNIMARK_OCR_MODELS` set for ocrs; Windows OCR runs when a language is installed.

use std::path::Path;
use std::time::Instant;

use super::test_render::{render_text, system_font};
use super::{from_dynamic, OcrEngine, OcrImage, OcrLanguage};

const SAMPLES: [(&str, OcrLanguage, &str); 6] = [
    (
        "tr-1",
        OcrLanguage::Tr,
        "Görüşme tarihi: 14 Şubat 2026, saat 09:30",
    ),
    (
        "tr-2",
        OcrLanguage::Tr,
        "Işık, ılık süt ve çiğ börek İstanbul'da üç öğünde yenir.",
    ),
    (
        "tr-3",
        OcrLanguage::Tr,
        "Ödeme tutarı 1.250,75 TL; fatura no: ÇĞÜ-2026/0042",
    ),
    (
        "en-1",
        OcrLanguage::En,
        "The quick brown fox jumps over 12 lazy dogs.",
    ),
    (
        "en-2",
        OcrLanguage::En,
        "Invoice #4521 issued on March 3, 2026 for $1,299.00",
    ),
    (
        "en-3",
        OcrLanguage::En,
        "Please return the signed contract by Friday.",
    ),
];

/// Text of the page image in `pdf_scanned_minimal.pdf` (read by eye, 2026-10-19).
const SCANNED_REFERENCE: &str = "# NOTICE\nThis is a minimal test document\nwith just a few lines of text.\nIt should still be processed correctly.";

fn normalize(text: &str) -> Vec<char> {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect()
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[b.len()]
}

/// Edits needed per reference character, whitespace collapsed.
fn cer(reference: &str, hypothesis: &str) -> f64 {
    let (r, h) = (normalize(reference), normalize(hypothesis));
    levenshtein(&r, &h) as f64 / r.len().max(1) as f64
}

/// The fixture holds one uncompressed 800x1100 RGB image (`/Length 2640000`); no pdfium needed.
fn scanned_page() -> OcrImage {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/markitdown/ocr/pdf_scanned_minimal.pdf");
    let pdf = std::fs::read(path).unwrap();
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle);
    let dict = find(&pdf, b"/Subtype/Image").unwrap();
    let start = dict + find(&pdf[dict..], b"stream\n").unwrap() + b"stream\n".len();
    let rgb = pdf[start..start + 800 * 1100 * 3].to_vec();
    let image = image::RgbImage::from_raw(800, 1100, rgb).unwrap();
    from_dynamic(image::DynamicImage::ImageRgb8(image)).unwrap()
}

fn engines() -> Vec<Box<dyn OcrEngine>> {
    let mut engines: Vec<Box<dyn OcrEngine>> = Vec::new();
    #[cfg(all(windows, feature = "ocr-windows"))]
    match super::windows_engine::WindowsEngine::new() {
        Ok(engine) => {
            println!("windows languages: {:?}", engine.languages());
            engines.push(Box::new(engine));
        }
        Err(error) => println!("windows: {error}"),
    }
    #[cfg(feature = "ocr")]
    if let Some(root) = std::env::var_os("OMNIMARK_OCR_MODELS") {
        let start = Instant::now();
        match super::ocrs_engine::OcrsEngine::load(Path::new(&root)) {
            Ok(engine) => {
                println!(
                    "ocrs load: {:.0} ms",
                    start.elapsed().as_secs_f64() * 1000.0
                );
                engines.push(Box::new(engine));
            }
            Err(error) => println!("ocrs: {error}"),
        }
    }
    engines
}

#[test]
fn cer_counts_edits_per_reference_character() {
    assert_eq!(cer("abc", "abc"), 0.0);
    assert!((cer("şu an", "su an") - 0.2).abs() < 1e-9);
    assert_eq!(cer("ab", ""), 1.0);
}

#[test]
#[ignore = "manual quality measurement; prints a table"]
fn measure_ocr_quality() {
    let font = system_font().expect("a system font (or OMNIMARK_TEST_FONT)");
    let mut cases: Vec<(String, &str, OcrLanguage, String, OcrImage)> = Vec::new();
    for (name, language, text) in SAMPLES {
        for px in [32.0, 20.0] {
            let group = if language == OcrLanguage::Tr {
                "tr"
            } else {
                "en"
            };
            cases.push((
                format!("{name}@{px}px"),
                group,
                language,
                text.to_string(),
                render_text(text, &font, px),
            ));
        }
    }
    cases.push((
        "pdf_scanned_minimal".into(),
        "scan",
        OcrLanguage::En,
        SCANNED_REFERENCE.into(),
        scanned_page(),
    ));
    let engines = engines();
    assert!(!engines.is_empty(), "no engine available");
    println!("| Örnek | Motor | CER | Süre (ms) | Çıktı |\n| --- | --- | --- | --- | --- |");
    let mut summary = Vec::new();
    for engine in &engines {
        for group in ["tr", "en", "scan"] {
            let (mut total_cer, mut total_ms, mut n) = (0.0, 0.0, 0);
            for (name, case_group, language, reference, image) in &cases {
                if *case_group != group {
                    continue;
                }
                let start = Instant::now();
                let text = engine
                    .recognize(image, language)
                    .unwrap_or_else(|e| format!("<error: {e}>"));
                let ms = start.elapsed().as_secs_f64() * 1000.0;
                let error_rate = cer(reference, &text);
                println!(
                    "| {name} | {} | {error_rate:.3} | {ms:.0} | {} |",
                    engine.name(),
                    text.replace('\n', " / ").replace('|', "\\|")
                );
                total_cer += error_rate;
                total_ms += ms;
                n += 1;
            }
            summary.push(format!(
                "| {} | {group} | {:.3} | {:.0} |",
                engine.name(),
                total_cer / n as f64,
                total_ms / n as f64
            ));
        }
    }
    println!(
        "\n| Motor | Grup | Ortalama CER | Ortalama süre (ms) |\n| --- | --- | --- | --- |\n{}",
        summary.join("\n")
    );
}
