//! Package relationships (`_rels/*.rels`) shared by the DOCX, PPTX and XLSX converters.

use quick_xml::events::Event;

use crate::error::{ConvertError, Result};
use crate::xml;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    pub rel_type: String,
    pub target: String,
    /// `TargetMode="External"` — never opened (CVE-2025-11849 class of bugs).
    pub external: bool,
}

pub fn parse_relationships(xml_bytes: &[u8]) -> Result<Vec<Relationship>> {
    let mut reader = xml::reader(xml_bytes);
    let mut out = Vec::new();
    loop {
        match reader.read_event().map_err(ConvertError::corrupt)? {
            Event::Start(e) | Event::Empty(e)
                if xml::local_name(e.name().as_ref()) == "Relationship" =>
            {
                out.push(Relationship {
                    id: xml::attr(&e, "Id").unwrap_or_default(),
                    rel_type: xml::attr(&e, "Type").unwrap_or_default(),
                    target: xml::attr(&e, "Target").unwrap_or_default(),
                    external: xml::attr(&e, "TargetMode")
                        .is_some_and(|m| m.eq_ignore_ascii_case("External")),
                });
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// `word/document.xml` → `word/_rels/document.xml.rels`.
pub fn rels_path_for(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Resolves a relationship target relative to the part that owns the relationship.
pub fn resolve_part(base_part: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_string();
    }
    let mut segments: Vec<&str> = base_part
        .rsplit_once('/')
        .map(|(dir, _)| dir.split('/').collect())
        .unwrap_or_default();
    for segment in target.split('/') {
        match segment {
            ".." => {
                segments.pop();
            }
            "." | "" => {}
            other => segments.push(other),
        }
    }
    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relationships_including_external_ones() {
        let xml = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/?a=1&amp;b=2" TargetMode="External"/>
</Relationships>"#;
        let rels = parse_relationships(xml).unwrap();
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].id, "rId1");
        assert!(rels[0].rel_type.ends_with("/image"));
        assert!(!rels[0].external);
        assert_eq!(rels[1].target, "https://example.com/?a=1&b=2");
        assert!(rels[1].external);
    }

    #[test]
    fn resolves_part_paths() {
        assert_eq!(
            rels_path_for("word/document.xml"),
            "word/_rels/document.xml.rels"
        );
        assert_eq!(
            rels_path_for("ppt/slides/slide1.xml"),
            "ppt/slides/_rels/slide1.xml.rels"
        );
        assert_eq!(
            resolve_part("word/document.xml", "media/image1.png"),
            "word/media/image1.png"
        );
        assert_eq!(
            resolve_part("ppt/slides/slide1.xml", "../media/image2.jpg"),
            "ppt/media/image2.jpg"
        );
        assert_eq!(
            resolve_part("xl/drawings/drawing1.xml", "/xl/media/a.png"),
            "xl/media/a.png"
        );
    }
}
