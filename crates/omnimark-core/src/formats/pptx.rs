//! PPTX → Markdown (behaviour reference: MarkItDown's `_pptx_converter.py`).

use quick_xml::events::Event;

use crate::archive::Archive;
use crate::error::{ConvertError, Result};
use crate::markdown;
use crate::ooxml::{self, Relationship};
use crate::registry::{ConvertContext, Converter};
use crate::xml;
use crate::{Document, FileInfo, Input};

/// Shapes whose `top` differs from the first shape of a row by at most this much share a row (0.5 cm).
const ROW_TOLERANCE: i128 = 180_000;
/// Guards the recursive tree walk against hostile, deeply nested XML.
const MAX_DEPTH: usize = 256;
/// Chart points with an index at or above this are ignored, so a hostile `c:pt@idx` cannot force a huge allocation.
const MAX_CHART_POINTS: usize = 10_000;
/// Series beyond this are ignored.
const MAX_CHART_SERIES: usize = 256;
/// Upper bound on `rows * series` cells; the row count shrinks to fit, so total allocation stays bounded.
const MAX_CHART_CELLS: usize = 100_000;

pub struct PptxConverter;

impl Converter for PptxConverter {
    fn name(&self) -> &'static str {
        "pptx"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["pptx"])
            || info
                .mime_starts_with(&["application/vnd.openxmlformats-officedocument.presentationml"])
    }

    fn convert(&self, input: &Input, ctx: &mut ConvertContext<'_>) -> Result<Document> {
        let mut archive = Archive::open(&input.bytes)?;
        let presentation = archive
            .read("ppt/presentation.xml")?
            .ok_or_else(|| ConvertError::corrupt("ppt/presentation.xml is missing"))?;
        let rels = read_rels(&mut archive, "ppt/presentation.xml")?;
        let slide_parts = slide_parts(&presentation, &rels)?;

        let mut doc = Document::new(String::new());
        let mut md = String::new();
        let total = slide_parts.len().max(1) as f32;
        for (index, part) in slide_parts.iter().enumerate() {
            ctx.check_cancel()?;
            md.push_str(&format!("\n\n<!-- Slide number: {} -->\n", index + 1));
            match archive.read(part)? {
                Some(bytes) => convert_slide(&mut archive, part, &bytes, ctx, &mut doc, &mut md)?,
                None => doc.warn(format!(
                    "Slide {} could not be found and was skipped.",
                    index + 1
                )),
            }
            ctx.report((index + 1) as f32 / total);
        }
        doc.markdown = md;
        Ok(doc)
    }
}

// ---------------------------------------------------------------------------------------------
// Minimal XML tree

#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    text: String,
}

impl Node {
    /// Attribute by local name (prefix ignored).
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| xml::local_name(k) == name)
            .map(|(_, v)| v.as_str())
    }

    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    fn path(&self, names: &[&str]) -> Option<&Node> {
        names.iter().try_fold(self, |node, name| node.child(name))
    }

    fn find(&self, name: &str) -> Option<&Node> {
        for child in &self.children {
            if child.name == name {
                return Some(child);
            }
            if let Some(found) = child.find(name) {
                return Some(found);
            }
        }
        None
    }

    fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a Node>) {
        for child in &self.children {
            if child.name == name {
                out.push(child);
            }
            child.find_all(name, out);
        }
    }
}

fn parse_tree(bytes: &[u8]) -> Result<Node> {
    let mut reader = xml::reader(bytes);
    let mut stack: Vec<Node> = vec![Node::default()];
    loop {
        let event = reader.read_event().map_err(ConvertError::corrupt)?;
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let mut node = Node {
                    name: xml::local_name(e.name().as_ref()).to_string(),
                    ..Node::default()
                };
                for attribute in e.attributes().flatten() {
                    let key = attribute.key.as_ref().to_string();
                    if let Ok(value) =
                        attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    {
                        node.attrs.push((key, value.into_owned()));
                    }
                }
                if matches!(event, Event::Start(_)) {
                    if stack.len() > MAX_DEPTH {
                        return Err(ConvertError::corrupt("XML is nested too deeply"));
                    }
                    stack.push(node);
                } else if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                }
            }
            Event::End(_) => {
                if stack.len() > 1 {
                    let node = stack.pop().expect("non-empty stack");
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    }
                }
            }
            Event::Eof => break,
            other => {
                if let Some(top) = stack.last_mut() {
                    xml::append_text(&mut top.text, other);
                }
            }
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().expect("non-empty stack");
        if let Some(parent) = stack.last_mut() {
            parent.children.push(node);
        }
    }
    let root = stack.pop().unwrap_or_default();
    root.children
        .into_iter()
        .next()
        .ok_or_else(|| ConvertError::corrupt("empty XML part"))
}

// ---------------------------------------------------------------------------------------------
// Package structure

fn read_rels(archive: &mut Archive<'_>, part: &str) -> Result<Vec<Relationship>> {
    match archive.read(&ooxml::rels_path_for(part))? {
        Some(bytes) => ooxml::parse_relationships(&bytes),
        None => Ok(Vec::new()),
    }
}

fn slide_parts(presentation: &[u8], rels: &[Relationship]) -> Result<Vec<String>> {
    let tree = parse_tree(presentation)?;
    let mut parts = Vec::new();
    if let Some(list) = tree.child("sldIdLst") {
        for id in list.children.iter().filter(|c| c.name == "sldId") {
            // The relationship id is the namespaced `r:id`; the plain `id` is the numeric slide id.
            let rid = id
                .attrs
                .iter()
                .find(|(k, _)| k.contains(':') && xml::local_name(k) == "id")
                .map(|(_, v)| v.as_str());
            if let Some(rel) = rid.and_then(|r| rels.iter().find(|rel| rel.id == r)) {
                if !rel.external {
                    parts.push(ooxml::resolve_part("ppt/presentation.xml", &rel.target));
                }
            }
        }
    }
    Ok(parts)
}

// ---------------------------------------------------------------------------------------------
// Shapes

#[derive(Debug, Default, Clone)]
struct Para {
    text: String,
    /// `Some(level)` when the paragraph is a list item.
    bullet: Option<usize>,
}

#[derive(Debug, Default)]
enum Kind {
    #[default]
    None,
    Text(Vec<Para>),
    Table(Vec<Vec<String>>),
    Chart(String),
    Picture {
        alt: String,
        embed: Option<String>,
    },
    Group(Vec<Shape>),
}

#[derive(Debug, Default)]
pub(crate) struct Shape {
    #[cfg_attr(not(test), allow(dead_code))] // read by the ordering tests only
    pub name: String,
    pub top: Option<i64>,
    pub left: Option<i64>,
    pub is_title: bool,
    kind: Kind,
}

/// Title first; the rest in rows (top to bottom), each row left to right.
pub(crate) fn order_shapes(shapes: &mut Vec<Shape>) {
    let (titles, mut rest): (Vec<Shape>, Vec<Shape>) =
        std::mem::take(shapes).into_iter().partition(|s| s.is_title);
    let key = |v: Option<i64>| v.map_or(i128::MIN / 2, i128::from);
    rest.sort_by_key(|s| key(s.top));

    let mut ordered = titles;
    let mut rows: Vec<Vec<Shape>> = Vec::new();
    let mut row_top = 0i128;
    for shape in rest {
        let top = key(shape.top);
        match rows.last_mut() {
            Some(row) if top - row_top <= ROW_TOLERANCE => row.push(shape),
            _ => {
                row_top = top;
                rows.push(vec![shape]);
            }
        }
    }
    for mut row in rows {
        row.sort_by_key(|s| key(s.left));
        ordered.extend(row);
    }
    *shapes = ordered;
}

fn offset(xfrm: Option<&Node>) -> (Option<i64>, Option<i64>) {
    let Some(off) = xfrm.and_then(|x| x.child("off")) else {
        return (None, None);
    };
    let num = |name| off.attr(name).and_then(|v| v.trim().parse::<i64>().ok());
    (num("y"), num("x"))
}

fn paragraphs(body: &Node) -> Vec<Para> {
    body.children
        .iter()
        .filter(|p| p.name == "p")
        .map(|p| {
            let mut text = String::new();
            for run in &p.children {
                match run.name.as_str() {
                    "r" | "fld" => {
                        if let Some(t) = run.child("t") {
                            text.push_str(&t.text);
                        }
                    }
                    "br" => text.push('\n'),
                    _ => {}
                }
            }
            let mut bullet = None;
            if let Some(props) = p.child("pPr") {
                let level = props
                    .attr("lvl")
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if level > 0
                    || props.child("buChar").is_some()
                    || props.child("buAutoNum").is_some()
                {
                    bullet = Some(level);
                }
            }
            Para { text, bullet }
        })
        .collect()
}

fn parse_shapes(tree: &Node, depth: usize) -> Vec<Shape> {
    let mut shapes = Vec::new();
    if depth > MAX_DEPTH {
        return shapes;
    }
    for node in &tree.children {
        match node.name.as_str() {
            "sp" => {
                let name = node
                    .path(&["nvSpPr", "cNvPr"])
                    .and_then(|c| c.attr("name"))
                    .unwrap_or("")
                    .to_string();
                let is_title = node
                    .path(&["nvSpPr", "nvPr", "ph"])
                    .and_then(|ph| ph.attr("type"))
                    .is_some_and(|t| t == "title" || t == "ctrTitle");
                let (top, left) = offset(node.path(&["spPr", "xfrm"]));
                let kind = match node.child("txBody") {
                    Some(body) => Kind::Text(paragraphs(body)),
                    None => Kind::None,
                };
                shapes.push(Shape {
                    name,
                    top,
                    left,
                    is_title,
                    kind,
                });
            }
            "pic" => {
                let c_nv = node.path(&["nvPicPr", "cNvPr"]);
                let name = c_nv.and_then(|c| c.attr("name")).unwrap_or("").to_string();
                let descr = c_nv.and_then(|c| c.attr("descr")).unwrap_or("");
                let alt = if descr.trim().is_empty() {
                    name.clone()
                } else {
                    descr.to_string()
                };
                let blip = node.path(&["blipFill", "blip"]);
                let embed = blip
                    .and_then(|b| b.attr("embed"))
                    .map(str::to_string)
                    .or_else(|| {
                        blip.and_then(|b| b.find("svgBlip"))
                            .and_then(|s| s.attr("embed"))
                            .map(str::to_string)
                    });
                let (top, left) = offset(node.path(&["spPr", "xfrm"]));
                shapes.push(Shape {
                    name,
                    top,
                    left,
                    is_title: false,
                    kind: Kind::Picture { alt, embed },
                });
            }
            "graphicFrame" => {
                let name = node
                    .path(&["nvGraphicFramePr", "cNvPr"])
                    .and_then(|c| c.attr("name"))
                    .unwrap_or("")
                    .to_string();
                let (top, left) = offset(node.child("xfrm"));
                let kind = if let Some(table) = node.find("tbl") {
                    Kind::Table(table_rows(table))
                } else if let Some(rid) = node.find("chart").and_then(|c| c.attr("id")) {
                    Kind::Chart(rid.to_string())
                } else {
                    Kind::None
                };
                shapes.push(Shape {
                    name,
                    top,
                    left,
                    is_title: false,
                    kind,
                });
            }
            "grpSp" => {
                let name = node
                    .path(&["nvGrpSpPr", "cNvPr"])
                    .and_then(|c| c.attr("name"))
                    .unwrap_or("")
                    .to_string();
                let (top, left) = offset(node.path(&["grpSpPr", "xfrm"]));
                shapes.push(Shape {
                    name,
                    top,
                    left,
                    is_title: false,
                    kind: Kind::Group(parse_shapes(node, depth + 1)),
                });
            }
            _ => {}
        }
    }
    shapes
}

fn table_rows(table: &Node) -> Vec<Vec<String>> {
    table
        .children
        .iter()
        .filter(|r| r.name == "tr")
        .map(|row| {
            row.children
                .iter()
                .filter(|c| c.name == "tc")
                .map(|cell| {
                    cell.child("txBody")
                        .map(|body| {
                            paragraphs(body)
                                .into_iter()
                                .map(|p| p.text)
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Rendering

struct Slide<'r, 'a, 'c> {
    archive: &'r mut Archive<'a>,
    ctx: &'r mut ConvertContext<'c>,
    doc: &'r mut Document,
    part: &'r str,
    rels: Vec<Relationship>,
}

fn convert_slide(
    archive: &mut Archive<'_>,
    part: &str,
    bytes: &[u8],
    ctx: &mut ConvertContext<'_>,
    doc: &mut Document,
    md: &mut String,
) -> Result<()> {
    let tree = parse_tree(bytes)?;
    let rels = read_rels(archive, part)?;
    let notes_target = rels
        .iter()
        .find(|r| !r.external && r.rel_type.ends_with("/notesSlide"))
        .map(|r| ooxml::resolve_part(part, &r.target));
    let shapes = tree
        .path(&["cSld", "spTree"])
        .map(|t| parse_shapes(t, 0))
        .unwrap_or_default();
    let mut slide = Slide {
        archive,
        ctx,
        doc,
        part,
        rels,
    };
    slide.render(shapes, md)?;

    if let Some(notes_part) = notes_target {
        if let Some(notes_bytes) = slide.archive.read(&notes_part)? {
            if let Some(text) = notes_text(&notes_bytes)? {
                md.push_str("\n\n### Notes:\n");
                md.push_str(&text);
            }
        }
    }
    Ok(())
}

fn notes_text(bytes: &[u8]) -> Result<Option<String>> {
    let tree = parse_tree(bytes)?;
    let mut sps = Vec::new();
    tree.find_all("sp", &mut sps);
    for sp in sps {
        let is_body = sp
            .path(&["nvSpPr", "nvPr", "ph"])
            .and_then(|ph| ph.attr("type"))
            .is_some_and(|t| t == "body");
        if !is_body {
            continue;
        }
        let Some(body) = sp.child("txBody") else {
            continue;
        };
        let text = paragraphs(body)
            .into_iter()
            .map(|p| p.text)
            .collect::<Vec<_>>()
            .join("\n");
        let text = text.trim();
        return Ok((!text.is_empty()).then(|| text.to_string()));
    }
    Ok(None)
}

impl Slide<'_, '_, '_> {
    fn render(&mut self, mut shapes: Vec<Shape>, out: &mut String) -> Result<()> {
        order_shapes(&mut shapes);
        for shape in shapes {
            self.ctx.check_cancel()?;
            match shape.kind {
                Kind::None => {}
                Kind::Text(paras) => {
                    if shape.is_title {
                        let text = paras
                            .iter()
                            .map(|p| p.text.trim())
                            .filter(|t| !t.is_empty())
                            .collect::<Vec<_>>()
                            .join(" ");
                        if !text.is_empty() {
                            out.push_str(&format!("# {text}\n"));
                        }
                    } else {
                        for para in &paras {
                            if let Some(level) = para.bullet {
                                out.push_str(&"  ".repeat(level));
                                out.push_str("- ");
                            }
                            out.push_str(&para.text);
                            out.push('\n');
                        }
                    }
                }
                Kind::Table(rows) => {
                    if !rows.is_empty() {
                        let (md, truncated) = markdown::table(&rows);
                        if truncated {
                            self.doc.warn_once(markdown::TABLE_WARNING);
                        }
                        out.push('\n');
                        out.push_str(&md);
                        out.push_str("\n\n");
                    }
                }
                Kind::Chart(rid) => {
                    let md = self.chart(&rid)?;
                    out.push_str("\n\n");
                    out.push_str(&md);
                    out.push('\n');
                }
                Kind::Picture { alt, embed } => {
                    let md = self.picture(&alt, embed.as_deref())?;
                    out.push_str(&format!("\n{md}\n"));
                }
                Kind::Group(children) => self.render(children, out)?,
            }
        }
        Ok(())
    }

    fn rel(&self, id: &str) -> Option<&Relationship> {
        self.rels.iter().find(|r| r.id == id)
    }

    fn chart(&mut self, rid: &str) -> Result<String> {
        let target = match self.rel(rid) {
            Some(rel) if !rel.external => ooxml::resolve_part(self.part, &rel.target),
            _ => {
                self.doc.warn("A chart could not be read.");
                return Ok("[unsupported chart]".to_string());
            }
        };
        let md = self
            .archive
            .read(&target)?
            .and_then(|bytes| chart_markdown(&bytes));
        Ok(md.unwrap_or_else(|| {
            self.doc.warn("A chart could not be read.");
            "[unsupported chart]".to_string()
        }))
    }

    fn picture(&mut self, alt: &str, embed: Option<&str>) -> Result<String> {
        let Some(rid) = embed else {
            return Ok(markdown::image(alt, None));
        };
        let (target, external) = match self.rel(rid) {
            Some(rel) => (ooxml::resolve_part(self.part, &rel.target), rel.external),
            None => {
                self.doc.warn("An image reference could not be resolved.");
                return Ok(markdown::image(alt, None));
            }
        };
        if external {
            self.doc.warn("An externally linked image was skipped.");
            return Ok(markdown::image(alt, None));
        }
        match self.archive.read(&target)? {
            Some(bytes) => {
                let name = target.rsplit('/').next().unwrap_or(&target);
                let link = self.ctx.assets.store(name, &bytes)?;
                Ok(markdown::image(alt, link.as_deref()))
            }
            None => {
                self.doc.warn("An embedded image could not be found.");
                Ok(markdown::image(alt, None))
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Charts

fn cache_points(node: &Node) -> Vec<(usize, String)> {
    let mut points = Vec::new();
    let mut pts = Vec::new();
    node.find_all("pt", &mut pts);
    for pt in pts {
        if let Some(idx) = pt
            .attr("idx")
            .and_then(|i| i.trim().parse::<usize>().ok())
            .filter(|idx| *idx < MAX_CHART_POINTS)
        {
            let value = pt.child("v").map(|v| v.text.clone()).unwrap_or_default();
            points.push((idx, value));
        }
    }
    points
}

fn dense(points: &[(usize, String)], len: usize) -> Vec<String> {
    let mut out = vec![String::new(); len];
    for (idx, value) in points {
        if *idx < len {
            out[*idx] = value.clone();
        }
    }
    out
}

/// Cached chart data as a table; `None` when the part has no series.
pub(crate) fn chart_markdown(chart_xml: &[u8]) -> Option<String> {
    let tree = parse_tree(chart_xml).ok()?;
    let chart = if tree.name == "chart" {
        &tree
    } else {
        tree.child("chart")?
    };

    let title = chart.child("title").map(|t| {
        let mut texts = Vec::new();
        t.find_all("t", &mut texts);
        texts.iter().map(|n| n.text.as_str()).collect::<String>()
    });

    let mut series = Vec::new();
    chart.child("plotArea")?.find_all("ser", &mut series);
    if series.is_empty() {
        return None;
    }
    series.truncate(MAX_CHART_SERIES);

    let categories = series[0].child("cat").map(cache_points).unwrap_or_default();
    let names: Vec<String> = series
        .iter()
        .enumerate()
        .map(|(i, s)| {
            s.child("tx")
                .and_then(|tx| tx.find("v"))
                .map(|v| v.text.clone())
                .unwrap_or_else(|| format!("Series {}", i + 1))
        })
        .collect();
    let values: Vec<Vec<(usize, String)>> = series
        .iter()
        .map(|s| s.child("val").map(cache_points).unwrap_or_default())
        .collect();

    let len = categories
        .iter()
        .chain(values.iter().flatten())
        .map(|(idx, _)| idx + 1)
        .max()
        .unwrap_or(0)
        .min(MAX_CHART_CELLS / series.len());
    let cats = dense(&categories, len);
    let columns: Vec<Vec<String>> = values.iter().map(|v| dense(v, len)).collect();

    let mut rows = vec![std::iter::once("Category".to_string())
        .chain(names)
        .collect::<Vec<_>>()];
    for (i, cat) in cats.into_iter().enumerate() {
        let mut row = vec![cat];
        row.extend(columns.iter().map(|c| c[i].clone()));
        rows.push(row);
    }

    let heading = match title.as_deref().map(str::trim) {
        Some(t) if !t.is_empty() => format!("### Chart: {t}"),
        _ => "### Chart".to_string(),
    };
    // MAX_CHART_SERIES and MAX_CHART_CELLS keep chart tables inside the shared table caps.
    let (table, _) = markdown::table(&rows);
    Some(format!("{heading}\n\n{table}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(name: &str, top: Option<i64>, left: Option<i64>, title: bool) -> Shape {
        Shape {
            name: name.into(),
            top,
            left,
            is_title: title,
            ..Shape::default()
        }
    }

    #[test]
    fn title_first_then_rows_then_left_to_right() {
        let mut shapes = vec![
            shape("right-low", Some(1_000_000), Some(5_000_000), false),
            shape("left-high", Some(1_100_000), Some(100_000), false),
            shape("footer", Some(6_000_000), Some(0), false),
            shape("title", Some(9_000_000), Some(0), true),
            shape("floating", None, None, false),
        ];
        order_shapes(&mut shapes);
        let names: Vec<_> = shapes.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["title", "floating", "left-high", "right-low", "footer"]
        );
    }

    #[test]
    fn chart_table_from_cached_values() {
        let xml = br#"<c:chartSpace xmlns:c="c" xmlns:a="a"><c:chart><c:title><c:tx><c:rich><a:p><a:r><a:t>Revenue</a:t></a:r></a:p></c:rich></c:tx></c:title><c:plotArea><c:barChart>
          <c:ser><c:tx><c:strRef><c:strCache><c:pt idx="0"><c:v>Series A</c:v></c:pt></c:strCache></c:strRef></c:tx>
            <c:cat><c:strRef><c:strCache><c:pt idx="0"><c:v>C0</c:v></c:pt><c:pt idx="1"><c:v>C1</c:v></c:pt></c:strCache></c:strRef></c:cat>
            <c:val><c:numRef><c:numCache><c:pt idx="0"><c:v>10</c:v></c:pt><c:pt idx="1"><c:v>2003</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser>
        </c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
        assert_eq!(
            chart_markdown(xml).unwrap(),
            "### Chart: Revenue\n\n| Category | Series A |\n| --- | --- |\n| C0 | 10 |\n| C1 | 2003 |\n"
        );
    }

    #[test]
    fn chart_ignores_points_beyond_the_cap() {
        let xml = br#"<c:chartSpace xmlns:c="c"><c:chart><c:plotArea><c:barChart>
          <c:ser><c:cat><c:strRef><c:strCache><c:pt idx="0"><c:v>A</c:v></c:pt><c:pt idx="4000000000"><c:v>Z</c:v></c:pt></c:strCache></c:strRef></c:cat>
            <c:val><c:numRef><c:numCache><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="4000000000"><c:v>2</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser>
        </c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
        assert_eq!(
            chart_markdown(xml).unwrap(),
            "### Chart

| Category | Series 1 |
| --- | --- |
| A | 1 |
"
        );
    }

    #[test]
    fn chart_caps_series_count_and_total_cells() {
        let one = r#"<c:ser><c:val><c:numRef><c:numCache><c:pt idx="9999"><c:v>1</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser>"#;
        let xml = format!(
            r#"<c:chartSpace xmlns:c="c"><c:chart><c:plotArea><c:barChart>{}</c:barChart></c:plotArea></c:chart></c:chartSpace>"#,
            one.repeat(5000)
        );
        let md = chart_markdown(xml.as_bytes()).unwrap();
        let lines: Vec<&str> = md.lines().filter(|l| l.starts_with('|')).collect();
        let columns = |l: &str| l.matches(" | ").count() + 1;
        assert_eq!(columns(lines[0]), 1 + MAX_CHART_SERIES);
        assert!((lines.len() - 2) * MAX_CHART_SERIES <= MAX_CHART_CELLS);
    }

    #[test]
    fn chart_without_series_is_unsupported() {
        let xml = br#"<c:chartSpace xmlns:c="c"><c:chart><c:plotArea/></c:chart></c:chartSpace>"#;
        assert!(chart_markdown(xml).is_none());
    }

    #[test]
    fn paragraphs_handle_bullets_breaks_and_fields() {
        let xml = br#"<p:txBody xmlns:p="p" xmlns:a="a">
          <a:p><a:r><a:t>Plain</a:t></a:r><a:br/><a:fld><a:t>7</a:t></a:fld></a:p>
          <a:p><a:pPr lvl="1"/><a:r><a:t>Nested</a:t></a:r></a:p>
          <a:p><a:pPr><a:buChar char="x"/></a:pPr><a:r><a:t>Bullet &amp; more</a:t></a:r></a:p></p:txBody>"#;
        let tree = parse_tree(xml).unwrap();
        let paras = paragraphs(&tree);
        assert_eq!(paras[0].text, "Plain\n7");
        assert_eq!(paras[0].bullet, None);
        assert_eq!(paras[1].bullet, Some(1));
        assert_eq!(paras[2].text, "Bullet & more");
        assert_eq!(paras[2].bullet, Some(0));
    }

    /// A minimal in-memory deck: `presentation.xml` body plus slide XML parts.
    fn deck(presentation_body: &str, slides: &[&str]) -> Vec<u8> {
        use std::io::Write;
        let mut ids = String::new();
        let mut rels = String::new();
        for n in 1..=slides.len() {
            ids.push_str(&format!(r#"<p:sldId id="{}" r:id="rId{n}"/>"#, 255 + n));
            rels.push_str(&format!(
                r#"<Relationship Id="rId{n}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{n}.xml"/>"#
            ));
        }
        let mut entries = vec![
            (
                "ppt/presentation.xml".to_string(),
                format!(
                    r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst>{ids}</p:sldIdLst>{presentation_body}</p:presentation>"#
                ),
            ),
            (
                "ppt/_rels/presentation.xml.rels".to_string(),
                format!(r#"<Relationships xmlns="rel">{rels}</Relationships>"#),
            ),
        ];
        for (i, slide) in slides.iter().enumerate() {
            entries.push((format!("ppt/slides/slide{}.xml", i + 1), slide.to_string()));
        }
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

    fn convert_deck(bytes: Vec<u8>) -> Result<Document> {
        let input = Input {
            path: "deck.pptx".into(),
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
        PptxConverter.convert(&input, &mut ctx)
    }

    #[test]
    fn wide_slide_table_is_capped_with_one_warning() {
        let cells =
            "<a:tc><a:txBody><a:p><a:r><a:t>c</a:t></a:r></a:p></a:txBody></a:tc>".repeat(1_500);
        let slide = format!(
            r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:graphicFrame><a:graphic><a:graphicData><a:tbl><a:tr>{cells}</a:tr><a:tr>{cells}</a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame></p:spTree></p:cSld></p:sld>"#
        );
        let doc = convert_deck(deck("", &[&slide, &slide])).unwrap();
        assert_eq!(doc.warnings, vec![markdown::TABLE_WARNING.to_string()]);
        let header = doc.markdown.lines().find(|l| l.starts_with('|')).unwrap();
        assert_eq!(header.matches('|').count(), markdown::MAX_TABLE_COLUMNS + 1);
    }
}
