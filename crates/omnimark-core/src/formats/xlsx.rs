use std::io::Cursor;

use calamine::{open_workbook_auto_from_rs, Data, Reader, SheetVisible};
use quick_xml::events::Event;

use crate::archive::Archive;
use crate::error::{ConvertError, Result};
use crate::markdown;
use crate::ooxml::{parse_relationships, rels_path_for, resolve_part};
use crate::registry::{ConvertContext, Converter};
use crate::xml;
use crate::{Document, FileInfo, Input};

const REL_DRAWING: &str = "/drawing";
const REL_IMAGE: &str = "/image";

pub struct XlsxConverter;

impl Converter for XlsxConverter {
    fn name(&self) -> &'static str {
        "xlsx"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["xlsx", "xlsm", "xls", "ods"])
            || info.mime_starts_with(&[
                "application/vnd.openxmlformats-officedocument.spreadsheetml",
                "application/vnd.ms-excel",
                "application/vnd.oasis.opendocument.spreadsheet",
            ])
    }

    fn convert(&self, input: &Input, ctx: &mut ConvertContext<'_>) -> Result<Document> {
        let mut workbook = open_workbook_auto_from_rs(Cursor::new(input.bytes.clone()))
            .map_err(ConvertError::corrupt)?;
        let extension = input
            .path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let want_images =
            ctx.options.extract_images && matches!(extension.as_str(), "xlsx" | "xlsm");

        let sheets: Vec<(String, bool)> = workbook
            .sheets_metadata()
            .iter()
            .map(|s| (s.name.clone(), s.visible == SheetVisible::Visible))
            .collect();
        let mut warnings = Vec::new();
        let mut parts = Vec::new();
        let total = sheets.len().max(1) as f32;
        for (index, (name, visible)) in sheets.iter().enumerate() {
            ctx.check_cancel()?;
            if !visible {
                warnings.push(format!("Hidden sheet skipped: {name}"));
                continue;
            }
            let range = workbook
                .worksheet_range(name)
                .map_err(ConvertError::corrupt)?;
            let grid: Vec<Vec<String>> = range
                .rows()
                .map(|row| row.iter().map(cell_text).collect())
                .collect();
            let grid = trim_grid(grid);
            let mut section = format!("## {name}");
            if !grid.is_empty() {
                section.push_str("\n\n");
                section.push_str(&markdown::table(&grid));
            }
            if want_images {
                match store_sheet_images(&input.bytes, name, ctx) {
                    Ok(lines) if !lines.is_empty() => {
                        section.push_str("\n\n");
                        section.push_str(&lines.join("\n"));
                    }
                    Ok(_) => {}
                    Err(ConvertError::Cancelled) => return Err(ConvertError::Cancelled),
                    Err(_) => warnings.push(format!("Images on sheet {name} could not be read.")),
                }
            }
            parts.push(section);
            ctx.report((index + 1) as f32 / total);
        }
        let mut document = Document::new(parts.join("\n\n"));
        document.warnings = warnings;
        Ok(document)
    }
}

/// Stores every image of a sheet and returns the Markdown image lines.
fn store_sheet_images(
    bytes: &[u8],
    sheet_name: &str,
    ctx: &mut ConvertContext<'_>,
) -> Result<Vec<String>> {
    let images = sheet_images(bytes, sheet_name)?;
    let mut archive = Archive::open(bytes)?;
    let mut lines = Vec::new();
    for (part, alt) in images {
        ctx.check_cancel()?;
        let data = read_part(&mut archive, &part)?;
        let file_name = part.rsplit('/').next().unwrap_or("image");
        let link = ctx.assets.store(file_name, &data)?;
        lines.push(markdown::image(
            alt.as_deref().unwrap_or(""),
            link.as_deref(),
        ));
    }
    Ok(lines)
}

/// Text of one cell without pandas artifacts (`NaN`, `100.0`).
fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Data::Bool(true) => "TRUE".to_string(),
        Data::Bool(false) => "FALSE".to_string(),
        Data::DateTime(dt) => {
            if dt.is_duration() {
                return dt.as_f64().to_string();
            }
            let (y, mo, d, h, mi, sec, _) = dt.to_ymd_hms_milli();
            let date = format!("{y:04}-{mo:02}-{d:02}");
            if (h, mi, sec) == (0, 0, 0) {
                date
            } else if sec == 0 {
                format!("{date} {h:02}:{mi:02}")
            } else {
                format!("{date} {h:02}:{mi:02}:{sec:02}")
            }
        }
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(e) => e.to_string(),
    }
}

/// Drops fully empty trailing rows and fully empty leading/trailing columns.
fn trim_grid(mut grid: Vec<Vec<String>>) -> Vec<Vec<String>> {
    while grid.last().is_some_and(|r| r.iter().all(String::is_empty)) {
        grid.pop();
    }
    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    let used = |c: usize| grid.iter().any(|r| r.get(c).is_some_and(|v| !v.is_empty()));
    let Some(first) = (0..width).find(|&c| used(c)) else {
        return Vec::new();
    };
    let last = (0..width).rev().find(|&c| used(c)).unwrap_or(first);
    grid.iter()
        .map(|r| {
            (first..=last)
                .map(|c| r.get(c).cloned().unwrap_or_default())
                .collect()
        })
        .collect()
}

fn read_part(archive: &mut Archive<'_>, part: &str) -> Result<Vec<u8>> {
    archive
        .read(part)?
        .ok_or_else(|| ConvertError::Corrupt(format!("missing part {part}")))
}

/// Images of one sheet in drawing order: `(media part path, alt text)`.
fn sheet_images(bytes: &[u8], sheet_name: &str) -> Result<Vec<(String, Option<String>)>> {
    let mut archive = Archive::open(bytes)?;
    let workbook_part = "xl/workbook.xml";
    let workbook = read_part(&mut archive, workbook_part)?;
    let mut sheet_rid = None;
    let mut reader = xml::reader(&workbook);
    loop {
        match reader.read_event().map_err(ConvertError::corrupt)? {
            Event::Start(e) | Event::Empty(e)
                if xml::local_name(e.name().as_ref()) == "sheet"
                    && xml::attr(&e, "name").as_deref() == Some(sheet_name) =>
            {
                sheet_rid = xml::attr(&e, "r:id");
                break;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let sheet_rid = sheet_rid.ok_or_else(|| ConvertError::Corrupt("sheet not found".into()))?;
    let workbook_rels =
        parse_relationships(&read_part(&mut archive, &rels_path_for(workbook_part))?)?;
    let sheet_target = workbook_rels
        .iter()
        .find(|r| r.id == sheet_rid && !r.external)
        .ok_or_else(|| ConvertError::Corrupt("sheet relationship not found".into()))?;
    let sheet_part = resolve_part(workbook_part, &sheet_target.target);

    let sheet_rels_path = rels_path_for(&sheet_part);
    if !archive.contains(&sheet_rels_path) {
        return Ok(Vec::new());
    }
    let sheet_rels = parse_relationships(&read_part(&mut archive, &sheet_rels_path)?)?;
    let mut images = Vec::new();
    for drawing in sheet_rels
        .iter()
        .filter(|r| !r.external && r.rel_type.ends_with(REL_DRAWING))
    {
        let drawing_part = resolve_part(&sheet_part, &drawing.target);
        let drawing_xml = read_part(&mut archive, &drawing_part)?;
        let drawing_rels_path = rels_path_for(&drawing_part);
        let drawing_rels = if archive.contains(&drawing_rels_path) {
            parse_relationships(&read_part(&mut archive, &drawing_rels_path)?)?
        } else {
            Vec::new()
        };
        for (alt, embed) in drawing_pictures(&drawing_xml)? {
            let rel = drawing_rels
                .iter()
                .find(|r| r.id == embed && !r.external && r.rel_type.ends_with(REL_IMAGE))
                .ok_or_else(|| ConvertError::Corrupt("image relationship not found".into()))?;
            images.push((resolve_part(&drawing_part, &rel.target), alt));
        }
    }
    Ok(images)
}

/// `(alt, r:embed)` for every `xdr:pic` that embeds an image, in document order.
fn drawing_pictures(xml_bytes: &[u8]) -> Result<Vec<(Option<String>, String)>> {
    let mut reader = xml::reader(xml_bytes);
    let mut out = Vec::new();
    let mut in_pic = false;
    let mut alt: Option<String> = None;
    let mut seen_props = false;
    let mut embed: Option<String> = None;
    loop {
        let event = reader.read_event().map_err(ConvertError::corrupt)?;
        match &event {
            Event::Start(e) | Event::Empty(e) => match xml::local_name(e.name().as_ref()) {
                "pic" => {
                    in_pic = true;
                    seen_props = false;
                    alt = None;
                    embed = None;
                }
                "cNvPr" if in_pic && !seen_props => {
                    seen_props = true;
                    alt = xml::attr(e, "descr").filter(|d| !d.is_empty());
                }
                "blip" if in_pic && embed.is_none() => embed = xml::attr(e, "r:embed"),
                _ => {}
            },
            Event::End(e) => {
                if xml::local_name(e.name().as_ref()) == "pic" && in_pic {
                    in_pic = false;
                    if let Some(embed) = embed.take() {
                        out.push((alt.take(), embed));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{Data, ExcelDateTime, ExcelDateTimeType};

    #[test]
    fn cell_text_formats_values_without_pandas_artifacts() {
        assert_eq!(cell_text(&Data::Empty), "");
        assert_eq!(cell_text(&Data::Float(100.0)), "100");
        assert_eq!(cell_text(&Data::Float(0.25)), "0.25");
        assert_eq!(cell_text(&Data::Int(-3)), "-3");
        assert_eq!(cell_text(&Data::Bool(true)), "TRUE");
        assert_eq!(
            cell_text(&Data::DateTime(ExcelDateTime::new(
                45000.0,
                ExcelDateTimeType::DateTime,
                false
            ))),
            "2023-03-15"
        );
        assert_eq!(
            cell_text(&Data::DateTime(ExcelDateTime::new(
                45000.5,
                ExcelDateTimeType::DateTime,
                false
            ))),
            "2023-03-15 12:00"
        );
    }

    #[test]
    fn trim_grid_removes_empty_edges_only() {
        let s = |v: &str| v.to_string();
        let grid = vec![
            vec![s(""), s("a"), s(""), s("")],
            vec![s(""), s(""), s(""), s("")],
            vec![s(""), s("b"), s("c"), s("")],
            vec![s(""), s(""), s(""), s("")],
        ];
        assert_eq!(
            trim_grid(grid),
            vec![
                vec![s("a"), s("")],
                vec![s(""), s("")],
                vec![s("b"), s("c")]
            ]
        );
    }

    #[test]
    fn drawing_pictures_returns_alt_and_embed_in_order() {
        let xml = br#"<xdr:wsDr xmlns:xdr="x" xmlns:a="a" xmlns:r="r">
<xdr:twoCellAnchor><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="P1" descr="first &amp; best"/></xdr:nvPicPr>
<xdr:blipFill><a:blip r:embed="rId1"/></xdr:blipFill></xdr:pic></xdr:twoCellAnchor>
<xdr:twoCellAnchor><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="3" name="P2"/></xdr:nvPicPr>
<xdr:blipFill><a:blip r:embed="rId2"/></xdr:blipFill></xdr:pic></xdr:twoCellAnchor></xdr:wsDr>"#;
        assert_eq!(
            drawing_pictures(xml).unwrap(),
            vec![
                (Some("first & best".to_string()), "rId1".to_string()),
                (None, "rId2".to_string())
            ]
        );
    }

    fn convert_fixture(
        path: &std::path::Path,
        extract: bool,
    ) -> (Document, crate::assets::MemoryAssetSink) {
        let input = Input {
            path: path.to_path_buf(),
            bytes: std::fs::read(path).unwrap(),
        };
        let options = crate::ConvertOptions {
            extract_images: extract,
            ..crate::ConvertOptions::default()
        };
        let mut sink = crate::assets::MemoryAssetSink::new("x_assets");
        let cancel = crate::CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut sink,
            cancel: &cancel,
            progress: &progress,
        };
        let doc = XlsxConverter.convert(&input, &mut ctx).unwrap();
        (doc, sink)
    }

    #[test]
    fn extracts_sheet_images_into_the_asset_sink() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/markitdown/ocr/xlsx_image_middle.xlsx");
        let (doc, sink) = convert_fixture(&path, true);
        assert!(!sink.files.is_empty());
        assert!(doc.markdown.contains("!["), "{}", doc.markdown);
        assert!(doc.markdown.contains("](x_assets/"), "{}", doc.markdown);
        let (doc, sink) = convert_fixture(&path, false);
        assert!(sink.files.is_empty());
        assert!(!doc.markdown.contains("!["));
    }

    #[test]
    fn corrupt_input_is_reported_as_corrupt() {
        let input = Input {
            path: "a.xlsx".into(),
            bytes: b"not a workbook".to_vec(),
        };
        let options = crate::ConvertOptions::default();
        let mut sink = crate::assets::NullAssetSink;
        let cancel = crate::CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut sink,
            cancel: &cancel,
            progress: &progress,
        };
        let err = XlsxConverter.convert(&input, &mut ctx).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Corrupt);
    }
}
