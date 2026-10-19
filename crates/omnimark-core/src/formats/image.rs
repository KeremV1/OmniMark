//! Image files: MarkItDown's exiftool metadata lines (EXIF via nom-exif, XMP read here) and,
//! with OCR on, the recognized text.

use nom_exif::{EntryValue, Exif, ExifTag, IfdIndex, MediaParser, MediaSource};
use quick_xml::events::{BytesStart, Event};

use crate::error::Result;
use crate::ocr::{block_plain, DocumentOcr};
use crate::registry::{ConvertContext, Converter};
use crate::xml;
use crate::{Document, FileInfo, Input};

pub struct ImageConverter;

const EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];
const MIME_TYPES: [&str; 6] = [
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/gif",
    "image/bmp",
    "image/tiff",
];
/// Metadata is only looked for in the first bytes of a file.
pub(crate) const METADATA_SCAN_LIMIT: usize = 16 * 1024 * 1024;
pub(crate) const MAX_XMP_PACKET: usize = 4 * 1024 * 1024;
const XMP_JPEG_SIGNATURE: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const XMP_PNG_KEYWORD: &[u8] = b"XML:com.adobe.xmp";
const ARTIST_TAG: u16 = 0x013b;
const EXIF_DATE: &str = "%Y:%m:%d %H:%M:%S";
const XMP_PROPERTIES: [&str; 8] = [
    "dc:title",
    "dc:description",
    "dc:creator",
    "dc:subject",
    "pdf:Author",
    "pdf:Keywords",
    "photoshop:Caption",
    "exif:UserComment",
];

impl Converter for ImageConverter {
    fn name(&self) -> &'static str {
        "image"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&EXTENSIONS)
            || info
                .mime
                .as_deref()
                .is_some_and(|m| MIME_TYPES.contains(&m))
    }

    fn convert(&self, input: &Input, ctx: &mut ConvertContext<'_>) -> Result<Document> {
        ctx.check_cancel()?;
        let mut doc = Document::new(read_metadata(&input.bytes).lines().join("\n"));
        if let Some(mut ocr) = DocumentOcr::new(ctx.options) {
            if let Some(text) = ocr.image_bytes(&input.bytes, ctx.cancel, &mut doc)? {
                if !doc.markdown.is_empty() {
                    doc.markdown.push_str("\n\n");
                }
                doc.markdown.push_str(&block_plain(&text));
            }
        }
        ctx.report(1.0);
        Ok(doc)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Metadata {
    pub image_size: Option<String>,
    pub title: Option<String>,
    pub caption: Option<String>,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub artist: Option<String>,
    pub author: Option<String>,
    pub date_time_original: Option<String>,
    pub create_date: Option<String>,
    pub gps_position: Option<String>,
}

impl Metadata {
    /// `{Field}: {value}` in MarkItDown's order, present fields only.
    pub(crate) fn lines(&self) -> Vec<String> {
        let keywords = (!self.keywords.is_empty()).then(|| self.keywords.join(", "));
        [
            ("ImageSize", &self.image_size),
            ("Title", &self.title),
            ("Caption", &self.caption),
            ("Description", &self.description),
            ("Keywords", &keywords),
            ("Artist", &self.artist),
            ("Author", &self.author),
            ("DateTimeOriginal", &self.date_time_original),
            ("CreateDate", &self.create_date),
            ("GPSPosition", &self.gps_position),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.as_deref().map(|v| format!("{name}: {v}")))
        .collect()
    }
}

pub(crate) fn read_metadata(bytes: &[u8]) -> Metadata {
    let head = &bytes[..bytes.len().min(METADATA_SCAN_LIMIT)];
    let mut meta = Metadata {
        image_size: imagesize::blob_size(head)
            .ok()
            .map(|size| format!("{}x{}", size.width, size.height)),
        ..Metadata::default()
    };
    read_exif(head, &mut meta);
    if let Some(packet) = find_xmp(head) {
        apply_xmp(parse_xmp(packet), &mut meta);
    }
    meta
}

/// Any parse failure means "no EXIF" (GIF/BMP have none; broken files are not an error).
fn read_exif(head: &[u8], meta: &mut Metadata) {
    let Ok(source) = MediaSource::from_memory(head.to_vec()) else {
        return;
    };
    let Ok(iter) = MediaParser::new().parse_exif(source) else {
        return;
    };
    let exif: Exif = iter.into();
    meta.description = exif.get(ExifTag::ImageDescription).and_then(entry_text);
    meta.artist = exif
        .get_by_code(IfdIndex::MAIN, ARTIST_TAG)
        .and_then(entry_text);
    meta.date_time_original = exif.get(ExifTag::DateTimeOriginal).and_then(entry_text);
    meta.create_date = exif.get(ExifTag::CreateDate).and_then(entry_text);
    meta.gps_position = exif.gps_info().and_then(|gps| {
        Some(format_gps(
            gps.latitude.to_decimal_degrees()?,
            gps.latitude_ref.as_char(),
            gps.longitude.to_decimal_degrees()?,
            gps.longitude_ref.as_char(),
        ))
    });
}

fn entry_text(value: &EntryValue) -> Option<String> {
    let text = match value {
        EntryValue::Text(text) => text.clone(),
        EntryValue::NaiveDateTime(time) => time.format(EXIF_DATE).to_string(),
        EntryValue::DateTime(time) => time.naive_local().format(EXIF_DATE).to_string(),
        _ => return None,
    };
    one_line(&text)
}

fn one_line(text: &str) -> Option<String> {
    let joined = text
        .replace('\0', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!joined.is_empty()).then_some(joined)
}

/// exiftool's `GPSPosition`: `47 deg 36' 22.80" N, 122 deg 19' 59.88" W`.
pub(crate) fn format_gps(
    latitude: f64,
    latitude_ref: char,
    longitude: f64,
    longitude_ref: char,
) -> String {
    format!(
        "{} {latitude_ref}, {} {longitude_ref}",
        dms(latitude),
        dms(longitude)
    )
}

fn dms(value: f64) -> String {
    let value = value.abs();
    let mut degrees = value.trunc() as u64;
    let minutes_total = (value - degrees as f64) * 60.0;
    let mut minutes = minutes_total.trunc() as u64;
    let mut hundredths = ((minutes_total - minutes as f64) * 6000.0).round() as u64;
    if hundredths >= 6000 {
        hundredths -= 6000;
        minutes += 1;
    }
    if minutes >= 60 {
        minutes -= 60;
        degrees += 1;
    }
    format!(
        "{degrees} deg {minutes}' {}.{:02}\"",
        hundredths / 100,
        hundredths % 100
    )
}

/// The XMP packet of a JPEG (APP1), PNG (`iTXt`) or WebP (`XMP ` chunk); `None` above 4 MB.
pub(crate) fn find_xmp(bytes: &[u8]) -> Option<&[u8]> {
    let packet = if bytes.starts_with(&[0xFF, 0xD8]) {
        jpeg_xmp(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png_xmp(bytes)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        webp_xmp(bytes)
    } else {
        None
    }?;
    (packet.len() <= MAX_XMP_PACKET).then_some(packet)
}

fn jpeg_xmp(b: &[u8]) -> Option<&[u8]> {
    let mut pos = 2;
    while pos + 4 <= b.len() {
        if b[pos] != 0xFF {
            return None;
        }
        let marker = b[pos + 1];
        match marker {
            0xFF => pos += 1,
            0xD9 | 0xDA => return None,
            0x01 | 0xD0..=0xD7 => pos += 2,
            _ => {
                let len = usize::from(u16::from_be_bytes([b[pos + 2], b[pos + 3]]));
                if len < 2 {
                    return None;
                }
                let payload = b.get(pos + 4..pos + 2 + len)?;
                if marker == 0xE1 {
                    if let Some(packet) = payload.strip_prefix(XMP_JPEG_SIGNATURE) {
                        return Some(packet);
                    }
                }
                pos += 2 + len;
            }
        }
    }
    None
}

fn png_xmp(b: &[u8]) -> Option<&[u8]> {
    let mut pos = 8usize;
    while pos.checked_add(8)? <= b.len() {
        let len = u32::from_be_bytes(b[pos..pos + 4].try_into().ok()?) as usize;
        let kind = &b[pos + 4..pos + 8];
        let data = b.get(pos + 8..pos.checked_add(8)?.checked_add(len)?)?;
        if kind == b"iTXt" {
            if let Some(packet) = itxt_xmp(data) {
                return Some(packet);
            }
        }
        if kind == b"IEND" {
            return None;
        }
        pos = pos.checked_add(12)?.checked_add(len)?;
    }
    None
}

fn itxt_xmp(data: &[u8]) -> Option<&[u8]> {
    let rest = data.strip_prefix(XMP_PNG_KEYWORD)?.strip_prefix(&[0u8])?;
    let (&compressed, rest) = rest.split_first()?;
    let (_method, rest) = rest.split_first()?;
    if compressed != 0 {
        return None;
    }
    let rest = &rest[rest.iter().position(|&c| c == 0)? + 1..];
    Some(&rest[rest.iter().position(|&c| c == 0)? + 1..])
}

fn webp_xmp(b: &[u8]) -> Option<&[u8]> {
    let mut pos = 12usize;
    while pos.checked_add(8)? <= b.len() {
        let kind = &b[pos..pos + 4];
        let len = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().ok()?) as usize;
        let data = b.get(pos + 8..pos.checked_add(8)?.checked_add(len)?)?;
        if kind == b"XMP " {
            return Some(data);
        }
        pos = pos.checked_add(8)?.checked_add(len)?.checked_add(len & 1)?;
    }
    None
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Xmp {
    pub title: Option<String>,
    pub description: Option<String>,
    pub creators: Vec<String>,
    pub subjects: Vec<String>,
    pub author: Option<String>,
    pub keywords: Option<String>,
    pub caption: Option<String>,
}

fn known_property(name: &str) -> Option<&'static str> {
    XMP_PROPERTIES.iter().copied().find(|p| *p == name)
}

/// Innermost open property element.
fn open_property(stack: &[String]) -> Option<&'static str> {
    stack.iter().rev().find_map(|name| known_property(name))
}

fn push_value(property: &'static str, text: &mut String, values: &mut Vec<(&'static str, String)>) {
    if let Some(value) = one_line(text) {
        values.push((property, value));
    }
    text.clear();
}

/// Reads the properties OmniMark prints. The text of a property, or of each of its `rdf:li`,
/// is one value; `rdf:Description` attributes are the shorthand form. DTDs are never expanded.
pub(crate) fn parse_xmp(packet: &[u8]) -> Xmp {
    let mut values: Vec<(&'static str, String)> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut reader = xml::reader(packet);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_string();
                if xml::local_name(&name) == "Description" {
                    shorthand(&e, &mut values);
                }
                stack.push(name);
            }
            Ok(Event::Empty(e)) => {
                if xml::local_name(e.name().as_ref()) == "Description" {
                    shorthand(&e, &mut values);
                }
            }
            Ok(event @ (Event::Text(_) | Event::CData(_) | Event::GeneralRef(_))) => {
                if open_property(&stack).is_some() {
                    xml::append_text(&mut text, &event);
                }
            }
            Ok(Event::End(_)) => {
                let closed = stack.pop().unwrap_or_default();
                if let Some(property) = known_property(&closed) {
                    push_value(property, &mut text, &mut values);
                } else if closed == "rdf:li" {
                    if let Some(property) = open_property(&stack) {
                        push_value(property, &mut text, &mut values);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
    let first = |property: &str| {
        values
            .iter()
            .find(|(p, _)| *p == property)
            .map(|(_, v)| v.clone())
    };
    let all = |property: &str| {
        values
            .iter()
            .filter(|(p, _)| *p == property)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
    };
    Xmp {
        title: first("dc:title"),
        description: first("dc:description"),
        creators: all("dc:creator"),
        subjects: all("dc:subject"),
        author: first("pdf:Author"),
        keywords: first("pdf:Keywords"),
        caption: first("photoshop:Caption").or_else(|| first("exif:UserComment")),
    }
}

fn shorthand(element: &BytesStart<'_>, values: &mut Vec<(&'static str, String)>) {
    for property in XMP_PROPERTIES {
        if let Some(value) = xml::attr(element, property).as_deref().and_then(one_line) {
            values.push((property, value));
        }
    }
}

fn apply_xmp(xmp: Xmp, meta: &mut Metadata) {
    meta.title = xmp.title;
    meta.caption = xmp.caption;
    if meta.description.is_none() {
        meta.description = xmp.description;
    }
    meta.keywords = if xmp.subjects.is_empty() {
        xmp.keywords.into_iter().collect()
    } else {
        xmp.subjects
    };
    meta.author = xmp
        .author
        .or_else(|| (!xmp.creators.is_empty()).then(|| xmp.creators.join(", ")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::NullAssetSink;
    use crate::ocr::test_render::png_bytes;
    use crate::ocr::{MockEngine, OcrEngine};
    use crate::{CancelToken, ConvertOptions};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn convert(name: &str, bytes: Vec<u8>, ocr: bool) -> Document {
        let options = ConvertOptions {
            ocr: ocr.then(|| Arc::new(MockEngine::default()) as Arc<dyn OcrEngine>),
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
        ImageConverter
            .convert(
                &Input {
                    path: PathBuf::from(name),
                    bytes,
                },
                &mut ctx,
            )
            .unwrap()
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/markitdown/files")
                .join(name),
        )
        .unwrap()
    }

    /// Inserts a chunk right after IHDR (CRC left zero; nothing here checks it).
    fn png_with_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut png = png_bytes(20, 20, [255, 255, 255, 255]);
        let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        chunk.extend_from_slice(&[0, 0, 0, 0]);
        png.splice(33..33, chunk);
        png
    }

    fn webp_with_xmp(xmp: &[u8]) -> Vec<u8> {
        let raw = vec![255u8; 20 * 20 * 4];
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
            .encode(&raw, 20, 20, image::ExtendedColorType::Rgba8)
            .unwrap();
        webp.extend_from_slice(b"XMP ");
        webp.extend_from_slice(&(xmp.len() as u32).to_le_bytes());
        webp.extend_from_slice(xmp);
        if xmp.len() % 2 == 1 {
            webp.push(0);
        }
        let riff_size = (webp.len() - 8) as u32;
        webp[4..8].copy_from_slice(&riff_size.to_le_bytes());
        webp
    }

    const XMP: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" photoshop:Caption="Kıyı &amp; deniz">
        <dc:title><rdf:Alt><rdf:li xml:lang="x-default">Ege
          kıyısı</rdf:li></rdf:Alt></dc:title>
        <dc:creator><rdf:Seq><rdf:li>Ayşe</rdf:li><rdf:li>Ali</rdf:li></rdf:Seq></dc:creator>
        <dc:subject><rdf:Bag><rdf:li>deniz</rdf:li><rdf:li>yaz</rdf:li></rdf:Bag></dc:subject>
      </rdf:Description></rdf:RDF></x:xmpmeta>"#;

    #[test]
    fn test_jpg_matches_markitdown_fields_in_order() {
        let doc = convert("test.jpg", fixture("test.jpg"), false);
        let lines: Vec<&str> = doc.markdown.lines().collect();
        assert_eq!(lines[0], "ImageSize: 1615x1967");
        assert_eq!(
            lines[1],
            "Title: AutoGen: Enabling Next-Gen LLM Applications via Multi-Agent Conversation"
        );
        assert!(lines[2].starts_with("Description: AutoGen enables diverse LLM-based applications"));
        assert_eq!(lines[3], "Author: AutoGen Authors");
        assert_eq!(lines[4], "DateTimeOriginal: 2024:03:14 22:10:00");
        assert_eq!(lines.len(), 5);
        assert!(doc.warnings.is_empty());
    }

    #[test]
    fn xmp_reads_alt_seq_bag_and_attribute_shorthand() {
        let xmp = parse_xmp(XMP.as_bytes());
        assert_eq!(xmp.title.as_deref(), Some("Ege kıyısı"));
        assert_eq!(xmp.creators, ["Ayşe", "Ali"]);
        assert_eq!(xmp.subjects, ["deniz", "yaz"]);
        assert_eq!(xmp.caption.as_deref(), Some("Kıyı & deniz"));
        assert_eq!(xmp.author, None);
    }

    #[test]
    fn png_itxt_and_webp_xmp_are_found_and_mapped() {
        let mut itxt = b"XML:com.adobe.xmp\0\0\0\0\0".to_vec();
        itxt.extend_from_slice(XMP.as_bytes());
        for (name, bytes) in [
            ("a.png", png_with_chunk(b"iTXt", &itxt)),
            ("a.webp", webp_with_xmp(XMP.as_bytes())),
        ] {
            let doc = convert(name, bytes, false);
            assert_eq!(
                doc.markdown,
                "ImageSize: 20x20\nTitle: Ege kıyısı\nCaption: Kıyı & deniz\nKeywords: deniz, yaz\nAuthor: Ayşe, Ali",
                "{name}"
            );
        }
    }

    #[test]
    fn oversized_xmp_packets_are_ignored() {
        let mut itxt = b"XML:com.adobe.xmp\0\0\0\0\0".to_vec();
        itxt.extend_from_slice(XMP.as_bytes());
        itxt.resize(itxt.len() + MAX_XMP_PACKET, b' ');
        assert_eq!(find_xmp(&png_with_chunk(b"iTXt", &itxt)), None);
    }

    #[test]
    fn gps_uses_exiftool_format() {
        let lat = 47.0 + 36.0 / 60.0 + 22.8 / 3600.0;
        let lon = 122.0 + 19.0 / 60.0 + 59.88 / 3600.0;
        assert_eq!(
            format_gps(lat, 'N', lon, 'W'),
            "47 deg 36' 22.80\" N, 122 deg 19' 59.88\" W"
        );
        assert_eq!(
            format_gps(10.0 + 59.0 / 60.0 + 59.999 / 3600.0, 'S', 0.0, 'E'),
            "11 deg 0' 0.00\" S, 0 deg 0' 0.00\" E"
        );
    }

    #[test]
    fn plain_and_broken_images_never_fail() {
        let plain = convert("p.png", png_bytes(20, 20, [0, 0, 0, 255]), false);
        assert_eq!(plain.markdown, "ImageSize: 20x20");
        let broken = convert("x.png", b"definitely not a png".to_vec(), false);
        assert_eq!(broken.markdown, "");
        assert!(broken.warnings.is_empty());
    }

    #[test]
    fn ocr_text_follows_the_metadata() {
        let doc = convert("p.png", png_bytes(20, 20, [0, 0, 0, 255]), true);
        assert_eq!(
            doc.markdown,
            "ImageSize: 20x20\n\n*[Image OCR]\nMOCK_OCR_TEXT_12345\n[End OCR]*"
        );
    }

    #[test]
    fn accepts_listed_extensions_and_mime_types_only() {
        let info = |ext: &str, mime: Option<&str>| FileInfo {
            extension: Some(ext.into()),
            mime: mime.map(String::from),
        };
        for ext in ["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"] {
            assert!(ImageConverter.accepts(&info(ext, None)), "{ext}");
        }
        assert!(ImageConverter.accepts(&FileInfo {
            extension: None,
            mime: Some("image/webp".into())
        }));
        assert!(!ImageConverter.accepts(&info("heic", Some("image/heif"))));
        assert!(!ImageConverter.accepts(&info("svg", Some("image/svg+xml"))));
    }
}
