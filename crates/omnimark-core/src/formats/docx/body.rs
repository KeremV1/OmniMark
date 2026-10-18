//! Single-pass walk over `word/document.xml` (and the notes parts) that builds Markdown blocks.

use quick_xml::events::{BytesStart, Event};

use super::inline::{render_segs, safe_link_target, toggle_on, Fmt, Seg};
use super::model::{join_blocks, Block, Cell, Drawing, Math, Note, Para, Table};
use super::styles::outline_to_heading;
use super::{DocxParts, DOCUMENT_PART};
use crate::error::{ConvertError, Result};
use crate::formats::omml::omml_to_latex;
use crate::markdown::{image, table};
use crate::ooxml::resolve_part;
use crate::registry::ConvertContext;
use crate::xml::{append_text, attr, local_name, reader};
use crate::Document;

/// Subtrees that never contribute content: deletions, the `mc:Fallback` copy of a drawing,
/// and revision records of formatting changes (their `rPr` would be misread as current).
const SKIPPED: &[&str] = &[
    "del",
    "moveFrom",
    "Fallback",
    "rPrChange",
    "pPrChange",
    "tblPrChange",
    "tcPrChange",
    "sectPrChange",
];
const CANCEL_EVERY: usize = 200;

pub(crate) fn render_body(
    document_xml: &[u8],
    parts: &mut DocxParts<'_>,
    ctx: &mut ConvertContext<'_>,
    doc: &mut Document,
) -> Result<String> {
    let mut engine = Engine::new(document_xml, parts, ctx, doc, None);
    engine.run()?;
    let mut out = join_blocks(&engine.blocks);
    let mut definitions = Vec::new();
    for (xml, prefix) in [
        (parts.footnotes.take(), "fn"),
        (parts.endnotes.take(), "en"),
    ] {
        let Some(xml) = xml else { continue };
        let mut engine = Engine::new(&xml, parts, ctx, doc, Some(prefix));
        engine.run()?;
        definitions.append(&mut engine.notes);
    }
    for definition in definitions {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&definition);
    }
    Ok(out)
}

struct Engine<'a, 'b, 'c> {
    src: &'a [u8],
    parts: &'a mut DocxParts<'b>,
    ctx: &'a mut ConvertContext<'c>,
    doc: &'a mut Document,
    /// `Some("fn")`/`Some("en")` when reading a notes part.
    note_prefix: Option<&'static str>,
    paras: Vec<Para>,
    tables: Vec<Table>,
    blocks: Vec<Block>,
    math: Option<Math>,
    display: Option<Vec<String>>,
    skip: usize,
    paragraphs_seen: usize,
    note: Option<Note>,
    notes: Vec<String>,
}

impl<'a, 'b, 'c> Engine<'a, 'b, 'c> {
    fn new(
        src: &'a [u8],
        parts: &'a mut DocxParts<'b>,
        ctx: &'a mut ConvertContext<'c>,
        doc: &'a mut Document,
        note_prefix: Option<&'static str>,
    ) -> Self {
        Self {
            src,
            parts,
            ctx,
            doc,
            note_prefix,
            paras: Vec::new(),
            tables: Vec::new(),
            blocks: Vec::new(),
            math: None,
            display: None,
            skip: 0,
            paragraphs_seen: 0,
            note: None,
            notes: Vec::new(),
        }
    }

    fn run(&mut self) -> Result<()> {
        let src = self.src;
        let mut reader = reader(src);
        loop {
            let before = reader.buffer_position() as usize;
            let event = reader.read_event().map_err(ConvertError::corrupt)?;
            let after = reader.buffer_position() as usize;
            match &event {
                Event::Eof => break,
                Event::Start(e) => {
                    let name = qname(e);
                    if self.skip > 0 {
                        self.skip += 1;
                    } else if SKIPPED.contains(&local_name(&name)) {
                        self.skip = 1;
                    } else {
                        self.open(e, &name, before)?;
                    }
                }
                Event::Empty(e) => {
                    let name = qname(e);
                    if self.skip == 0 && !SKIPPED.contains(&local_name(&name)) {
                        self.open(e, &name, before)?;
                        self.close(&name, after)?;
                    }
                }
                Event::End(e) => {
                    if self.skip > 0 {
                        self.skip -= 1;
                    } else {
                        let name = qname_of(e.name());
                        self.close(&name, after)?;
                    }
                }
                other => {
                    if self.skip == 0 {
                        self.text(other);
                    }
                }
            }
        }
        Ok(())
    }

    fn text(&mut self, event: &Event<'_>) {
        if let Some(math) = self.math.as_mut() {
            if math.in_t {
                append_text(&mut math.plain, event);
            }
            return;
        }
        if let Some(para) = self.paras.last_mut() {
            if para.in_t {
                let mut text = String::new();
                append_text(&mut text, event);
                para.push_text(&text);
            }
        }
    }

    fn open(&mut self, e: &BytesStart<'_>, name: &str, before: usize) -> Result<()> {
        let local = local_name(name);
        if let Some(math) = self.math.as_mut() {
            match local {
                "oMath" => math.depth += 1,
                "t" => math.in_t = true,
                _ => {}
            }
            return Ok(());
        }
        match local {
            "footnote" | "endnote" if self.note_prefix.is_some() => {
                let id = attr(e, "w:id").unwrap_or_default();
                let kind = attr(e, "w:type").unwrap_or_default();
                self.note = Some(Note {
                    label: format!("[^{}{id}]", self.note_prefix.unwrap_or("fn")),
                    skip: matches!(
                        kind.as_str(),
                        "separator" | "continuationSeparator" | "continuationNotice"
                    ),
                });
                self.blocks.clear();
            }
            "p" => {
                self.paragraphs_seen += 1;
                if self.paragraphs_seen.is_multiple_of(CANCEL_EVERY) {
                    self.ctx.check_cancel()?;
                    self.ctx
                        .report(before as f32 / self.src.len().max(1) as f32);
                }
                self.paras.push(Para::new());
            }
            "tbl" => self.tables.push(Table::default()),
            "tr" => {
                if let Some(table) = self.tables.last_mut() {
                    table.row.clear();
                }
            }
            "tc" => {
                if let Some(table) = self.tables.last_mut() {
                    table.cell = Some(Cell {
                        span: 1,
                        ..Cell::default()
                    });
                }
            }
            "gridSpan" if self.paras.is_empty() => {
                if let Some(cell) = self.tables.last_mut().and_then(|t| t.cell.as_mut()) {
                    cell.span = attr(e, "w:val")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, 1000);
                }
            }
            "vMerge" if self.paras.is_empty() => {
                if let Some(cell) = self.tables.last_mut().and_then(|t| t.cell.as_mut()) {
                    cell.continued = attr(e, "w:val").is_none_or(|v| v == "continue");
                }
            }
            "oMathPara" if !self.paras.is_empty() => self.display = Some(Vec::new()),
            "oMath" if !self.paras.is_empty() => {
                self.math = Some(Math {
                    start: before,
                    depth: 1,
                    ..Math::default()
                });
            }
            _ => self.open_paragraph_part(e, name, local),
        }
        Ok(())
    }

    /// Elements that only matter inside the paragraph currently being read.
    fn open_paragraph_part(&mut self, e: &BytesStart<'_>, name: &str, local: &str) {
        let Some(para) = self.paras.last_mut() else {
            return;
        };
        match local {
            "pPr" => para.in_ppr = true,
            "pStyle" if para.in_ppr => para.style = attr(e, "w:val"),
            "outlineLvl" if para.in_ppr => {
                para.outline = attr(e, "w:val").and_then(|v| v.parse().ok());
            }
            "ilvl" if para.in_ppr => {
                para.level = attr(e, "w:val").and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            "numId" if para.in_ppr => para.num_id = attr(e, "w:val"),
            "r" => {
                para.in_run = true;
                para.fmt = Fmt::default();
            }
            "rPr" if para.in_run => para.in_rpr = true,
            "b" if para.in_rpr => para.fmt.bold = toggle_on(attr(e, "w:val").as_deref()),
            "i" if para.in_rpr => para.fmt.italic = toggle_on(attr(e, "w:val").as_deref()),
            "strike" | "dstrike" if para.in_rpr => {
                para.fmt.strike = toggle_on(attr(e, "w:val").as_deref());
            }
            "u" if para.in_rpr => para.fmt.underline = toggle_on(attr(e, "w:val").as_deref()),
            "t" if para.in_run && !name.starts_with("a:") => para.in_t = true,
            "tab" if para.in_run => para.push_text("\t"),
            "br" | "cr" if para.in_run => para.push_text("\n"),
            "noBreakHyphen" if para.in_run => para.push_text("-"),
            "footnoteReference" | "endnoteReference" => {
                let prefix = if local == "footnoteReference" {
                    "fn"
                } else {
                    "en"
                };
                if let Some(id) = attr(e, "w:id") {
                    para.push(Seg::Raw(format!("[^{prefix}{id}]")));
                }
            }
            "hyperlink" => {
                let target = attr(e, "r:id")
                    .and_then(|id| self.parts.rels.get(&id))
                    .filter(|rel| rel.external)
                    .and_then(|rel| safe_link_target(&rel.target));
                para.levels.push(Vec::new());
                para.links.push(target);
            }
            "drawing" => para.drawing = Some(Drawing::default()),
            "docPr" => {
                if let Some(drawing) = para.drawing.as_mut() {
                    drawing.alt = attr(e, "descr").unwrap_or_default();
                }
            }
            "blip" => {
                if let Some(drawing) = para.drawing.as_mut() {
                    drawing.embed = attr(e, "r:embed");
                    drawing.link = attr(e, "r:link");
                }
            }
            "imagedata" if para.drawing.is_none() => {
                let drawing = Drawing {
                    alt: attr(e, "o:title").unwrap_or_default(),
                    embed: attr(e, "r:id"),
                    link: None,
                };
                let markdown = self.image_markdown(&drawing);
                if let Some(para) = self.paras.last_mut() {
                    para.push(Seg::Raw(markdown));
                }
            }
            _ => {}
        }
    }

    fn close(&mut self, name: &str, after: usize) -> Result<()> {
        let local = local_name(name);
        if let Some(math) = self.math.as_mut() {
            match local {
                "t" => math.in_t = false,
                "oMath" => {
                    math.depth -= 1;
                    if math.depth == 0 {
                        self.finish_math(after);
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        match local {
            "footnote" | "endnote" if self.note_prefix.is_some() => {
                if let Some(note) = self.note.take() {
                    let text: Vec<&str> = self.blocks.iter().map(|b| b.plain.as_str()).collect();
                    let text = text.join(" ");
                    if !note.skip && !text.is_empty() {
                        self.notes.push(format!("{}: {text}", note.label));
                    }
                    self.blocks.clear();
                }
            }
            "p" => self.finish_paragraph(),
            "oMathPara" => {
                if let (Some(display), Some(para)) = (self.display.take(), self.paras.last_mut()) {
                    if !display.is_empty() {
                        let text = display.join("\n\n");
                        para.extras.push(Block {
                            md: text.clone(),
                            plain: text,
                            list: false,
                        });
                    }
                }
            }
            "tc" => self.finish_cell(),
            "tr" => {
                if let Some(table) = self.tables.last_mut() {
                    let row = std::mem::take(&mut table.row);
                    table.rows.push(row);
                }
            }
            "tbl" => self.finish_table(),
            _ => self.close_paragraph_part(local),
        }
        Ok(())
    }

    fn close_paragraph_part(&mut self, local: &str) {
        let Some(para) = self.paras.last_mut() else {
            return;
        };
        match local {
            "pPr" => para.in_ppr = false,
            "r" => para.in_run = false,
            "rPr" => para.in_rpr = false,
            "t" => para.in_t = false,
            "hyperlink" => para.close_link(),
            "drawing" => {
                if let Some(drawing) = para.drawing.take() {
                    let markdown = self.image_markdown(&drawing);
                    if let Some(para) = self.paras.last_mut() {
                        para.push(Seg::Raw(markdown));
                    }
                }
            }
            _ => {}
        }
    }

    fn finish_math(&mut self, end: usize) {
        let Some(math) = self.math.take() else { return };
        let end = end.min(self.src.len());
        let omml = String::from_utf8_lossy(&self.src[math.start.min(end)..end]).into_owned();
        let (latex, plain) = match omml_to_latex(&omml) {
            Ok(latex) => (latex.trim().to_string(), None),
            Err(_) => {
                self.doc
                    .warn("Equation could not be converted; kept as plain text.");
                (String::new(), Some(math.plain))
            }
        };
        let display = self.display.is_some();
        let piece = match (plain, latex.is_empty()) {
            (Some(plain), _) if !plain.trim().is_empty() => Piece::Plain(plain),
            (None, false) => Piece::Latex(latex),
            _ => return,
        };
        match (display, piece) {
            (true, Piece::Latex(latex)) => {
                self.display
                    .get_or_insert_with(Vec::new)
                    .push(format!("$${latex}$$"));
            }
            (true, Piece::Plain(plain)) => {
                self.display.get_or_insert_with(Vec::new).push(plain);
            }
            (false, piece) => {
                if let Some(para) = self.paras.last_mut() {
                    para.push(match piece {
                        Piece::Latex(latex) => Seg::Raw(format!("${latex}$")),
                        Piece::Plain(plain) => Seg::Text(Fmt::default(), plain),
                    });
                }
            }
        }
    }

    fn finish_paragraph(&mut self) {
        let Some(mut para) = self.paras.pop() else {
            return;
        };
        while para.levels.len() > 1 {
            para.close_link();
        }
        let text = render_segs(&para.levels[0]).trim().to_string();
        if !text.is_empty() {
            let heading = self.heading_level(&para);
            let block = if let Some(level) = heading {
                let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
                Block {
                    md: format!("{} {line}", "#".repeat(usize::from(level))),
                    plain: text,
                    list: false,
                }
            } else if let Some(num_id) = para.num_id.as_deref().filter(|id| *id != "0") {
                let marker = if self.parts.numbering.is_numbered(num_id, para.level) {
                    "1. "
                } else {
                    "- "
                };
                Block {
                    md: format!("{}{marker}{text}", "  ".repeat(para.level.min(8))),
                    plain: text,
                    list: true,
                }
            } else {
                Block {
                    md: text.clone(),
                    plain: text,
                    list: false,
                }
            };
            self.emit(block);
        }
        for extra in para.extras {
            self.emit(extra);
        }
    }

    fn heading_level(&self, para: &Para) -> Option<u8> {
        para.outline.and_then(outline_to_heading).or_else(|| {
            para.style
                .as_deref()
                .and_then(|id| self.parts.styles.heading_level(id))
        })
    }

    /// Routes a finished block to its container: an enclosing paragraph (text box), a table
    /// cell, or the top level.
    fn emit(&mut self, block: Block) {
        if let Some(parent) = self.paras.last_mut() {
            parent.extras.push(block);
        } else if let Some(cell) = self.tables.last_mut().and_then(|t| t.cell.as_mut()) {
            cell.paragraphs.push(block.plain);
        } else {
            self.blocks.push(block);
        }
    }

    fn finish_cell(&mut self) {
        let Some(table) = self.tables.last_mut() else {
            return;
        };
        let Some(cell) = table.cell.take() else {
            return;
        };
        let text = if cell.continued {
            String::new()
        } else {
            cell.paragraphs.join(" ")
        };
        table.row.push(text);
        for _ in 1..cell.span {
            table.row.push(String::new());
        }
    }

    fn finish_table(&mut self) {
        let Some(finished) = self.tables.pop() else {
            return;
        };
        let nested = !self.tables.is_empty() || !self.paras.is_empty();
        let rows: Vec<Vec<String>> = finished
            .rows
            .into_iter()
            .filter(|row| !row.is_empty())
            .collect();
        if rows.is_empty() {
            return;
        }
        if nested {
            let flat: Vec<&str> = rows
                .iter()
                .flatten()
                .map(String::as_str)
                .filter(|c| !c.is_empty())
                .collect();
            let text = flat.join(" ");
            if !text.is_empty() {
                self.emit(Block {
                    md: text.clone(),
                    plain: text,
                    list: false,
                });
            }
        } else {
            let md = table(&rows);
            self.emit(Block {
                md,
                plain: String::new(),
                list: false,
            });
        }
    }

    /// Markdown for a picture. Linked (external) pictures are never opened.
    fn image_markdown(&mut self, drawing: &Drawing) -> String {
        let alt = drawing.alt.as_str();
        let rel = |id: &Option<String>| id.as_ref().and_then(|id| self.parts.rels.get(id)).cloned();
        let embedded = rel(&drawing.embed).filter(|r| !r.external);
        let Some(embedded) = embedded else {
            let linked = rel(&drawing.link).or_else(|| rel(&drawing.embed));
            if let Some(linked) = linked {
                self.doc
                    .warn(format!("Linked image not embedded: {}", linked.target));
            }
            return image(alt, None);
        };
        if !self.ctx.options.extract_images {
            return image(alt, None);
        }
        let part = resolve_part(DOCUMENT_PART, &embedded.target);
        if let Some(cached) = self.parts.image_cache.get(&part) {
            return image(alt, cached.as_deref());
        }
        let bytes = match self.parts.archive.as_mut().map(|a| a.read(&part)) {
            Some(Ok(Some(bytes))) => bytes,
            Some(Err(_)) | Some(Ok(None)) => {
                self.doc.warn(format!("Image could not be read: {part}"));
                return image(alt, None);
            }
            None => return image(alt, None),
        };
        let file_name = part.rsplit('/').next().unwrap_or("image");
        let link = match self.ctx.assets.store(file_name, &bytes) {
            Ok(link) => link,
            Err(_) => {
                self.doc.warn(format!("Image could not be saved: {part}"));
                None
            }
        };
        self.parts.image_cache.insert(part, link.clone());
        image(alt, link.as_deref())
    }
}

enum Piece {
    Latex(String),
    Plain(String),
}

pub(super) fn qname_of(name: quick_xml::name::QName<'_>) -> String {
    let text: &str = name.as_ref();
    text.to_string()
}

pub(super) fn qname(e: &BytesStart<'_>) -> String {
    qname_of(e.name())
}
