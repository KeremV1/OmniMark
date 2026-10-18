//! DOCX → Markdown. Reads `word/document.xml` directly (no intermediate HTML).

mod body;
mod inline;
mod model;
mod styles;

use std::collections::HashMap;

use crate::archive::Archive;
use crate::error::{ConvertError, Result};
use crate::ooxml::{parse_relationships, Relationship};
use crate::registry::{ConvertContext, Converter};
use crate::{Document, FileInfo, Input};

pub(crate) use body::render_body;
use styles::{Numbering, Styles};

pub(crate) const DOCUMENT_PART: &str = "word/document.xml";

pub struct DocxConverter;

impl Converter for DocxConverter {
    fn name(&self) -> &'static str {
        "docx"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["docx"])
            || info.mime_starts_with(&[
                "application/vnd.openxmlformats-officedocument.wordprocessingml",
            ])
    }

    fn convert(&self, input: &Input, ctx: &mut ConvertContext<'_>) -> Result<Document> {
        let mut archive = Archive::open(&input.bytes)?;
        let document_xml = archive
            .read(DOCUMENT_PART)?
            .ok_or_else(|| ConvertError::Corrupt("missing word/document.xml".into()))?;
        let mut parts = DocxParts::open(archive)?;
        let mut doc = Document::default();
        doc.markdown = render_body(&document_xml, &mut parts, ctx, &mut doc)?;
        Ok(doc)
    }
}

/// Everything besides the main body that rendering needs: styles, numbering,
/// relationships, notes and (when reading a real file) the archive for images.
pub(crate) struct DocxParts<'a> {
    archive: Option<Archive<'a>>,
    styles: Styles,
    numbering: Numbering,
    rels: HashMap<String, Relationship>,
    footnotes: Option<Vec<u8>>,
    endnotes: Option<Vec<u8>>,
    /// Part path → stored link target, so a repeated picture is written once.
    image_cache: HashMap<String, Option<String>>,
}

impl<'a> DocxParts<'a> {
    fn new() -> Self {
        Self {
            archive: None,
            styles: Styles::default(),
            numbering: Numbering::default(),
            rels: HashMap::new(),
            footnotes: None,
            endnotes: None,
            image_cache: HashMap::new(),
        }
    }

    fn open(mut archive: Archive<'a>) -> Result<Self> {
        let mut parts = Self::new();
        if let Some(rels) = archive.read("word/_rels/document.xml.rels")? {
            parts.add_rels(&rels);
        }
        if let Some(xml) = archive.read("word/styles.xml")? {
            parts.styles = Styles::parse(&xml);
        }
        if let Some(xml) = archive.read("word/numbering.xml")? {
            parts.numbering = Numbering::parse(&xml);
        }
        parts.footnotes = archive.read("word/footnotes.xml")?;
        parts.endnotes = archive.read("word/endnotes.xml")?;
        parts.archive = Some(archive);
        Ok(parts)
    }

    /// Parts without a zip: used by unit tests that render XML fragments.
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self::new()
    }

    #[cfg(test)]
    pub(crate) fn from_xml(styles: Option<&[u8]>, numbering: Option<&[u8]>) -> Self {
        let mut parts = Self::new();
        if let Some(xml) = styles {
            parts.styles = Styles::parse(xml);
        }
        if let Some(xml) = numbering {
            parts.numbering = Numbering::parse(xml);
        }
        parts
    }

    /// Lenient: a malformed relationships part only loses links and images.
    pub(crate) fn add_rels(&mut self, xml: &[u8]) {
        if let Ok(rels) = parse_relationships(xml) {
            for rel in rels {
                self.rels.insert(rel.id.clone(), rel);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn set_notes(&mut self, footnotes: Option<Vec<u8>>, endnotes: Option<Vec<u8>>) {
        self.footnotes = footnotes;
        self.endnotes = endnotes;
    }
}

#[cfg(test)]
mod tests;
