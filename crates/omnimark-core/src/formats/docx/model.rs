//! Paragraph, table and equation state kept while walking the XML.

use super::inline::{render_segs, Fmt, Seg};

pub(super) fn join_blocks(blocks: &[Block]) -> String {
    let mut out = String::new();
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            out.push_str(if block.list && blocks[index - 1].list {
                "\n"
            } else {
                "\n\n"
            });
        }
        out.push_str(&block.md);
    }
    out
}

/// A finished paragraph: with its Markdown prefix, and without it (for table cells).
#[derive(Debug, Clone)]
pub(super) struct Block {
    pub(super) md: String,
    pub(super) plain: String,
    pub(super) list: bool,
}

#[derive(Default)]
pub(super) struct Drawing {
    pub(super) alt: String,
    pub(super) embed: Option<String>,
    pub(super) link: Option<String>,
}

#[derive(Default)]
pub(super) struct Para {
    pub(super) style: Option<String>,
    pub(super) outline: Option<u8>,
    pub(super) num_id: Option<String>,
    pub(super) level: usize,
    pub(super) in_ppr: bool,
    pub(super) in_run: bool,
    pub(super) in_rpr: bool,
    pub(super) in_t: bool,
    pub(super) fmt: Fmt,
    /// Segment levels: index 0 is the paragraph, every open hyperlink adds one.
    pub(super) levels: Vec<Vec<Seg>>,
    /// Link destination for each open hyperlink level (`None` = keep text only).
    pub(super) links: Vec<Option<String>>,
    pub(super) drawing: Option<Drawing>,
    /// Blocks that belong after this paragraph (text boxes, display equations).
    pub(super) extras: Vec<Block>,
}

impl Para {
    pub(super) fn new() -> Self {
        Self {
            levels: vec![Vec::new()],
            ..Self::default()
        }
    }

    pub(super) fn push(&mut self, seg: Seg) {
        if let Some(level) = self.levels.last_mut() {
            level.push(seg);
        }
    }

    pub(super) fn push_text(&mut self, text: &str) {
        let fmt = self.fmt;
        self.push(Seg::Text(fmt, text.to_string()));
    }

    pub(super) fn close_link(&mut self) {
        if self.levels.len() < 2 {
            return;
        }
        let inner = self.levels.pop().unwrap_or_default();
        let target = self.links.pop().flatten();
        let text = render_segs(&inner);
        let rendered = match target {
            Some(target) if !text.trim().is_empty() => format!("[{text}]({target})"),
            _ => text,
        };
        self.push(Seg::Raw(rendered));
    }
}

#[derive(Default)]
pub(super) struct Cell {
    pub(super) paragraphs: Vec<String>,
    pub(super) span: usize,
    pub(super) continued: bool,
}

#[derive(Default)]
pub(super) struct Table {
    pub(super) rows: Vec<Vec<String>>,
    pub(super) row: Vec<String>,
    pub(super) cell: Option<Cell>,
}

#[derive(Default)]
pub(super) struct Math {
    pub(super) start: usize,
    pub(super) depth: usize,
    pub(super) in_t: bool,
    pub(super) plain: String,
}

pub(super) struct Note {
    pub(super) label: String,
    pub(super) skip: bool,
}
