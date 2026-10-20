use std::io::Cursor;

use calamine::{
    open_workbook_auto_from_rs, Data, DataRef, Reader, SheetVisible, Sheets, Xlsx, XlsxError,
};
use quick_xml::events::Event;

use crate::archive::Archive;
use crate::error::{ConvertError, Result};
use crate::markdown::{self, MAX_TABLE_CELLS, MAX_TABLE_COLUMNS};
use crate::ocr::{block_markdown, DocumentOcr};
use crate::ooxml::{parse_relationships, rels_path_for, resolve_part};
use crate::registry::{ConvertContext, Converter};
use crate::xml;
use crate::{Document, FileInfo, Input};

const REL_DRAWING: &str = "/drawing";
const REL_IMAGE: &str = "/image";
/// Sheets whose used bounding box (rows × columns) is larger than this are skipped with a
/// warning: a 2 KB file can name cells A1 and XFD1048576, a ~17-billion-cell box.
const MAX_SHEET_CELLS: u64 = 10_000_000;
/// Cells streamed between cancellation checks.
const CANCEL_EVERY: usize = 65_536;

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
        let mut workbook = open_workbook_auto_from_rs(Cursor::new(input.bytes.as_slice()))
            .map_err(ConvertError::corrupt)?;
        let extension = input
            .path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let mut ocr = DocumentOcr::new(ctx.options);
        let mut notes = Document::default();
        let want_images = (ctx.options.extract_images || ocr.is_some())
            && matches!(extension.as_str(), "xlsx" | "xlsm");

        let sheets: Vec<(String, bool)> = workbook
            .sheets_metadata()
            .iter()
            .map(|s| (s.name.clone(), s.visible == SheetVisible::Visible))
            .collect();
        let mut warnings = Vec::new();
        let mut truncated = false;
        let mut parts = Vec::new();
        let total = sheets.len().max(1) as f32;
        for (index, (name, visible)) in sheets.iter().enumerate() {
            ctx.check_cancel()?;
            if !visible {
                warnings.push(format!("Hidden sheet skipped: {name}"));
                continue;
            }
            let Some((grid, cut)) = sheet_grid(&mut workbook, name, ctx)? else {
                warnings.push(format!("Sheet {name} is too large and was skipped."));
                continue;
            };
            let grid = trim_grid(grid);
            let mut section = format!("## {name}");
            if !grid.is_empty() {
                let (table, table_cut) = markdown::table(&grid);
                truncated |= cut || table_cut;
                section.push_str("\n\n");
                section.push_str(&table);
            }
            if want_images {
                match store_sheet_images(&input.bytes, name, ctx, ocr.as_mut(), &mut notes) {
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
        if truncated {
            document.warn_once(markdown::TABLE_WARNING);
        }
        for warning in notes.warnings {
            document.warn_once(&warning);
        }
        Ok(document)
    }
}

/// The used cells of one sheet, already cut to the shared table caps, plus whether anything
/// was cut. `None` when the sheet's bounding box exceeds [`MAX_SHEET_CELLS`] (skipped, with a warning).
fn sheet_grid(
    workbook: &mut Sheets<Cursor<&[u8]>>,
    name: &str,
    ctx: &ConvertContext<'_>,
) -> Result<Option<(Vec<Vec<String>>, bool)>> {
    match workbook {
        Sheets::Xlsx(xlsx) => xlsx_grid(xlsx, name, ctx),
        // Residual risk: calamine parses XLS and ODS sheets into dense ranges while the workbook
        // opens (`Range::from_sparse`), before any check here can run; ODS caps that at 100M
        // cells, XLS has no cap. Only the copy into strings below is bounded.
        other => {
            let range = other.worksheet_range(name).map_err(ConvertError::corrupt)?;
            let (height, width) = range.get_size();
            if height as u64 * width as u64 > MAX_SHEET_CELLS {
                return Ok(None);
            }
            let (mut grid, cut) = empty_window(height, width);
            for (row, cells) in grid.iter_mut().zip(range.rows()) {
                for (slot, cell) in row.iter_mut().zip(cells) {
                    *slot = cell_text(cell);
                }
            }
            Ok(Some((grid, cut)))
        }
    }
}

/// Streams the sheet XML twice so calamine never builds the dense bounding-box grid
/// (`worksheet_range` would): first for the bounds, then to fill the capped window.
fn xlsx_grid(
    xlsx: &mut Xlsx<Cursor<&[u8]>>,
    name: &str,
    ctx: &ConvertContext<'_>,
) -> Result<Option<(Vec<Vec<String>>, bool)>> {
    let mut bounds: Option<((u32, u32), (u32, u32))> = None;
    let mut seen = 0usize;
    {
        let Some(mut reader) = cells_reader(xlsx, name)? else {
            return Ok(Some((Vec::new(), false)));
        };
        while let Some(cell) = reader.next_cell().map_err(ConvertError::corrupt)? {
            seen += 1;
            if seen.is_multiple_of(CANCEL_EVERY) {
                ctx.check_cancel()?;
            }
            if matches!(cell.get_value(), DataRef::Empty) {
                continue;
            }
            let (r, c) = cell.get_position();
            bounds = Some(match bounds {
                None => ((r, c), (r, c)),
                Some((start, end)) => (
                    (start.0.min(r), start.1.min(c)),
                    (end.0.max(r), end.1.max(c)),
                ),
            });
        }
    }
    let Some((start, end)) = bounds else {
        return Ok(Some((Vec::new(), false)));
    };
    let height = u64::from(end.0 - start.0) + 1;
    let width = u64::from(end.1 - start.1) + 1;
    if height * width > MAX_SHEET_CELLS {
        return Ok(None);
    }
    let (mut grid, cut) = empty_window(height as usize, width as usize);
    let Some(mut reader) = cells_reader(xlsx, name)? else {
        return Ok(Some((Vec::new(), false)));
    };
    while let Some(cell) = reader.next_cell().map_err(ConvertError::corrupt)? {
        seen += 1;
        if seen.is_multiple_of(CANCEL_EVERY) {
            ctx.check_cancel()?;
        }
        if matches!(cell.get_value(), DataRef::Empty) {
            continue;
        }
        let (r, c) = cell.get_position();
        let slot = grid
            .get_mut((r - start.0) as usize)
            .and_then(|row| row.get_mut((c - start.1) as usize));
        if let Some(slot) = slot {
            *slot = cell_text(&Data::from(cell.get_value().clone()));
        }
    }
    Ok(Some((grid, cut)))
}

/// `None` for a sheet part that is not a worksheet (a chartsheet), like `worksheet_range`.
fn cells_reader<'a, 'b>(
    xlsx: &'a mut Xlsx<Cursor<&'b [u8]>>,
    name: &str,
) -> Result<Option<calamine::XlsxCellReader<'a, Cursor<&'b [u8]>>>> {
    match xlsx.worksheet_cells_reader(name) {
        Ok(reader) => Ok(Some(reader)),
        Err(XlsxError::NotAWorksheet(_)) => Ok(None),
        Err(error) => Err(ConvertError::corrupt(error)),
    }
}

/// An empty `height × width` grid cut to the shared table caps, and whether it was cut.
fn empty_window(height: usize, width: usize) -> (Vec<Vec<String>>, bool) {
    let kept_width = width.min(MAX_TABLE_COLUMNS);
    let kept_height = match kept_width {
        0 => 0,
        w => height.min(MAX_TABLE_CELLS / w),
    };
    let cut = kept_width < width || kept_height < height;
    (vec![vec![String::new(); kept_width]; kept_height], cut)
}

/// Stores (and, with OCR on, reads) every image of a sheet and returns the Markdown lines.
fn store_sheet_images(
    bytes: &[u8],
    sheet_name: &str,
    ctx: &mut ConvertContext<'_>,
    mut ocr: Option<&mut DocumentOcr>,
    notes: &mut Document,
) -> Result<Vec<String>> {
    let mut archive = Archive::open(bytes)?;
    let images = sheet_images(&mut archive, sheet_name)?;
    let mut lines = Vec::new();
    for (part, alt) in images {
        ctx.check_cancel()?;
        let data = read_part(&mut archive, &part)?;
        let file_name = part.rsplit('/').next().unwrap_or("image");
        let link = if ctx.options.extract_images {
            ctx.assets.store(file_name, &data)?
        } else {
            None
        };
        let md = markdown::image(alt.as_deref().unwrap_or(""), link.as_deref());
        let text = match ocr.as_deref_mut() {
            Some(ocr) => ocr.image_bytes(&data, ctx.cancel, notes)?,
            None => None,
        };
        lines.push(match text {
            Some(text) => format!("{md}\n\n{}\n", block_markdown(&text)),
            None => md,
        });
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
fn sheet_images(
    archive: &mut Archive<'_>,
    sheet_name: &str,
) -> Result<Vec<(String, Option<String>)>> {
    let workbook_part = "xl/workbook.xml";
    let workbook = read_part(archive, workbook_part)?;
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
    let workbook_rels = parse_relationships(&read_part(archive, &rels_path_for(workbook_part))?)?;
    let sheet_target = workbook_rels
        .iter()
        .find(|r| r.id == sheet_rid && !r.external)
        .ok_or_else(|| ConvertError::Corrupt("sheet relationship not found".into()))?;
    let sheet_part = resolve_part(workbook_part, &sheet_target.target);

    let sheet_rels_path = rels_path_for(&sheet_part);
    if !archive.contains(&sheet_rels_path) {
        return Ok(Vec::new());
    }
    let sheet_rels = parse_relationships(&read_part(archive, &sheet_rels_path)?)?;
    let mut images = Vec::new();
    for drawing in sheet_rels
        .iter()
        .filter(|r| !r.external && r.rel_type.ends_with(REL_DRAWING))
    {
        let drawing_part = resolve_part(&sheet_part, &drawing.target);
        let drawing_xml = read_part(archive, &drawing_part)?;
        let drawing_rels_path = rels_path_for(&drawing_part);
        let drawing_rels = if archive.contains(&drawing_rels_path) {
            parse_relationships(&read_part(archive, &drawing_rels_path)?)?
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

    /// A minimal in-memory workbook; each sheet is `(name, sheetData inner XML)`.
    fn workbook(sheets: &[(&str, &str)]) -> Vec<u8> {
        use std::io::Write;
        let mut entries = vec![
            (
                "[Content_Types].xml".to_string(),
                r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#.to_string(),
            ),
            (
                "_rels/.rels".to_string(),
                r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_string(),
            ),
        ];
        let mut sheet_list = String::new();
        let mut rels = String::new();
        for (i, (name, data)) in sheets.iter().enumerate() {
            let n = i + 1;
            sheet_list.push_str(&format!(
                r#"<sheet name="{name}" sheetId="{n}" r:id="rId{n}"/>"#
            ));
            rels.push_str(&format!(
                r#"<Relationship Id="rId{n}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{n}.xml"/>"#
            ));
            entries.push((
                format!("xl/worksheets/sheet{n}.xml"),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{data}</sheetData></worksheet>"#
                ),
            ));
        }
        entries.push((
            "xl/workbook.xml".to_string(),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>{sheet_list}</sheets></workbook>"#
            ),
        ));
        entries.push((
            "xl/_rels/workbook.xml.rels".to_string(),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#
            ),
        ));
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut buf);
        for (name, data) in entries {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        buf.into_inner()
    }

    fn convert_bytes(bytes: Vec<u8>) -> Document {
        let input = Input {
            path: "book.xlsx".into(),
            bytes,
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
        XlsxConverter.convert(&input, &mut ctx).unwrap()
    }

    #[test]
    fn sparse_sheet_with_a_huge_bounding_box_is_skipped_with_a_warning() {
        let sparse = r#"<row r="1"><c r="A1"><v>1</v></c></row><row r="1048576"><c r="XFD1048576"><v>2</v></c></row>"#;
        let normal = r#"<row r="1"><c r="A1"><v>7</v></c><c r="B1"><v>8</v></c></row>"#;
        let doc = convert_bytes(workbook(&[("Big", sparse), ("Small", normal)]));
        assert_eq!(
            doc.warnings,
            vec!["Sheet Big is too large and was skipped.".to_string()]
        );
        assert_eq!(doc.markdown, "## Small\n\n| 7 | 8 |\n| --- | --- |");
    }

    #[test]
    fn wide_sheet_is_cut_to_the_table_caps_with_one_warning() {
        // ALL is column 1000 and BXX column 2000: the bounding box is small but too wide.
        let wide = r#"<row r="1"><c r="A1"><v>1</v></c><c r="ALL1"><v>9</v></c><c r="BXX1"><v>2</v></c></row><row r="2"><c r="A2"><v>3</v></c></row>"#;
        let doc = convert_bytes(workbook(&[("One", wide), ("Two", wide)]));
        assert_eq!(doc.warnings, vec![markdown::TABLE_WARNING.to_string()]);
        let header = doc.markdown.lines().nth(2).unwrap();
        assert_eq!(header.matches('|').count(), markdown::MAX_TABLE_COLUMNS + 1);
        assert!(header.starts_with("| 1 |"));
    }

    #[test]
    fn ocr_reads_sheet_images_even_when_extraction_is_off() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/markitdown/ocr/xlsx_image_middle.xlsx");
        let options = crate::ConvertOptions {
            extract_images: false,
            ocr: Some(std::sync::Arc::new(crate::ocr::MockEngine::default())),
            ..crate::ConvertOptions::default()
        };
        let mut sink = crate::assets::NullAssetSink;
        let cancel = crate::CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut sink,
            cancel: &cancel,
            progress: &progress,
        };
        let doc = XlsxConverter
            .convert(
                &Input {
                    path: path.clone(),
                    bytes: std::fs::read(&path).unwrap(),
                },
                &mut ctx,
            )
            .unwrap();
        assert_eq!(doc.markdown.matches("MOCK\\_OCR\\_TEXT\\_12345").count(), 2);
        assert!(doc.markdown.contains("]()\n\n*[Image OCR]\\\n"));
    }
}
