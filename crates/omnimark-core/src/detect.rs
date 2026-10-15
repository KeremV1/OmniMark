//! File type detection (extension + magic bytes) and text decoding.

use std::path::Path;

use serde::Serialize;

use crate::info::FileInfo;

/// Bytes inspected by the "is this text?" heuristic.
const TEXT_SNIFF_LEN: usize = 8 * 1024;

pub fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty())
}

pub fn mime_for_extension(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xlsm" => "application/vnd.ms-excel.sheet.macroEnabled.12",
        "xls" => "application/vnd.ms-excel",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "html" | "htm" => "text/html",
        "csv" => "text/csv",
        "txt" | "text" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "json" => "application/json",
        "jsonl" => "application/jsonl",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "heic" => "image/heic",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        _ => return None,
    })
}

/// Guess from content only: magic bytes (incl. OOXML containers via `infer`), then a text heuristic.
pub fn sniff(bytes: &[u8]) -> Option<FileInfo> {
    if let Some(kind) = infer::get(bytes) {
        return Some(FileInfo {
            extension: Some(kind.extension().to_string()),
            mime: Some(kind.mime_type().to_string()),
        });
    }
    looks_like_text(bytes).then(|| FileInfo {
        extension: Some("txt".into()),
        mime: Some("text/plain".into()),
    })
}

fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    let head = &bytes[..bytes.len().min(TEXT_SNIFF_LEN)];
    !head.is_empty() && !head.contains(&0)
}

fn is_textual_mime(mime: &str) -> bool {
    mime.starts_with("text/") || mime == "application/json" || mime == "application/jsonl"
}

/// Ordered guesses for a file: a merged guess when extension and content agree,
/// otherwise the extension-based guess followed by the content-based one.
pub fn guesses(path: &Path, bytes: &[u8]) -> Vec<FileInfo> {
    let extension = extension_of(path);
    let base = FileInfo {
        mime: extension
            .as_deref()
            .and_then(mime_for_extension)
            .map(str::to_string),
        extension,
    };
    let Some(content) = sniff(bytes) else {
        return vec![base];
    };
    if base.extension.is_none() {
        return vec![content];
    }
    let content_is_text = content.mime.as_deref() == Some("text/plain");
    let compatible = base.extension == content.extension
        || (content_is_text && base.mime.as_deref().is_some_and(is_textual_mime));
    if compatible {
        vec![FileInfo {
            extension: base.extension,
            mime: base.mime.or(content.mime),
        }]
    } else {
        vec![base, content]
    }
}

/// Decodes text: BOM first, then strict UTF-8 over the whole input, then a chardetng guess.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some((encoding, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        return encoding
            .decode_without_bom_handling(&bytes[bom_len..])
            .0
            .into_owned();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, chardetng::Utf8Detection::Allow);
    encoding.decode_without_bom_handling(bytes).0.into_owned()
}

/// Coarse kind used by the UI to label queue entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Pdf,
    Word,
    Excel,
    PowerPoint,
    Html,
    Csv,
    Text,
    Json,
    Image,
    Audio,
    Unknown,
}

pub fn classify(path: &Path, head: &[u8]) -> FileKind {
    guesses(path, head)
        .iter()
        .map(kind_of)
        .find(|k| *k != FileKind::Unknown)
        .unwrap_or(FileKind::Unknown)
}

fn kind_of(info: &FileInfo) -> FileKind {
    let by_extension = match info.extension.as_deref() {
        Some("pdf") => FileKind::Pdf,
        Some("docx") => FileKind::Word,
        Some("xlsx" | "xlsm" | "xls" | "ods") => FileKind::Excel,
        Some("pptx") => FileKind::PowerPoint,
        Some("html" | "htm") => FileKind::Html,
        Some("csv") => FileKind::Csv,
        Some("json" | "jsonl") => FileKind::Json,
        Some("txt" | "text" | "md" | "markdown") => FileKind::Text,
        _ => FileKind::Unknown,
    };
    if by_extension != FileKind::Unknown {
        return by_extension;
    }
    match info.mime.as_deref() {
        Some(m) if m.starts_with("image/") => FileKind::Image,
        Some(m) if m.starts_with("audio/") || m == "video/mp4" => FileKind::Audio,
        Some(m) if m.starts_with("text/") => FileKind::Text,
        _ => FileKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> (PathBuf, Vec<u8>) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/markitdown/files")
            .join(name);
        let bytes = std::fs::read(&path).expect("fixture");
        (path, bytes)
    }

    #[test]
    fn office_files_are_detected_by_extension_and_content() {
        let (path, bytes) = fixture("test.docx");
        let g = guesses(&path, &bytes);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].extension.as_deref(), Some("docx"));
        assert_eq!(
            g[0].mime.as_deref(),
            Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        );
        let (path, bytes) = fixture("test.xls");
        assert_eq!(guesses(&path, &bytes)[0].extension.as_deref(), Some("xls"));
    }

    #[test]
    fn content_wins_when_there_is_no_extension() {
        let (_, bytes) = fixture("test.pdf");
        let g = guesses(Path::new("download"), &bytes);
        assert_eq!(
            g,
            vec![FileInfo {
                extension: Some("pdf".into()),
                mime: Some("application/pdf".into())
            }]
        );
    }

    #[test]
    fn mismatched_extension_yields_both_guesses() {
        let (_, bytes) = fixture("test.pdf");
        let g = guesses(Path::new("report.docx"), &bytes);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].extension.as_deref(), Some("docx"));
        assert_eq!(g[1].extension.as_deref(), Some("pdf"));
    }

    #[test]
    fn text_without_extension_is_plain_text() {
        let g = guesses(Path::new("notes"), b"hello world\n");
        assert_eq!(g[0].mime.as_deref(), Some("text/plain"));
    }

    #[test]
    fn binary_noise_has_no_mime() {
        let (path, bytes) = fixture("random.bin");
        let g = guesses(&path, &bytes);
        assert_eq!(
            g,
            vec![FileInfo {
                extension: Some("bin".into()),
                mime: None
            }]
        );
    }

    #[test]
    fn decode_text_handles_utf8_bom_utf16_and_shift_jis() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFname"), "name");
        assert_eq!(decode_text(b"\xFF\xFEh\x00i\x00"), "hi");
        let (_, sjis) = fixture("test_mskanji.csv");
        assert!(decode_text(&sjis).contains("髙橋淳"));
        let (_, late) = fixture("json_late_non_ascii.json");
        let text = decode_text(&late);
        assert!(text.contains("café, 日本語, 😀"));
        assert!(!text.contains('\u{fffd}'));
    }

    #[test]
    fn classify_maps_to_display_kinds() {
        let (path, bytes) = fixture("test.pptx");
        assert_eq!(classify(&path, &bytes), FileKind::PowerPoint);
        assert_eq!(classify(Path::new("a.JPG"), b""), FileKind::Image);
        assert_eq!(classify(Path::new("a.m4a"), b""), FileKind::Audio);
        assert_eq!(classify(Path::new("a.xyz"), &[0u8; 16]), FileKind::Unknown);
    }
}
