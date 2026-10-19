//! `word/styles.xml` (heading detection) and `word/numbering.xml` (bullet vs. numbered).

use std::collections::HashMap;

use quick_xml::events::Event;

use super::body::{qname, qname_of};
use crate::xml::{attr, local_name, reader};

const MAX_BASED_ON_STEPS: usize = 10;

#[derive(Debug, Default)]
struct StyleDef {
    name: Option<String>,
    based_on: Option<String>,
    outline: Option<u8>,
}

#[derive(Debug, Default)]
pub(crate) struct Styles {
    defs: HashMap<String, StyleDef>,
    pub(crate) incomplete: bool,
}

/// `heading 3` / `Heading3` → 3 (clamped to 6); `Title` → 1.
fn heading_from_name(name: &str) -> Option<u8> {
    let lower = name.trim().to_ascii_lowercase();
    if lower == "title" {
        return Some(1);
    }
    let digits = lower.strip_prefix("heading")?.trim();
    let level: u8 = digits.parse().ok()?;
    (level >= 1).then(|| level.min(6))
}

/// Zero-based `w:outlineLvl` → heading level 1..=6 (7-9 mean body text).
pub(crate) fn outline_to_heading(zero_based: u8) -> Option<u8> {
    (zero_based < 6).then_some(zero_based + 1)
}

impl Styles {
    /// Lenient: a malformed part yields the styles read so far.
    pub(crate) fn parse(xml: &[u8]) -> Self {
        let mut reader = reader(xml);
        let mut styles = HashMap::new();
        let mut current: Option<(String, StyleDef)> = None;
        let mut incomplete = false;
        loop {
            let Ok(event) = reader.read_event() else {
                incomplete = true;
                break;
            };
            match &event {
                Event::Eof => break,
                Event::Start(e) | Event::Empty(e) => {
                    let qname = qname(e);
                    match (local_name(&qname), current.as_mut()) {
                        ("style", _) => {
                            // Styles without an id cannot be referenced; skip them.
                            current = attr(e, "w:styleId").map(|id| (id, StyleDef::default()));
                            if matches!(event, Event::Empty(_)) {
                                current = None;
                            }
                        }
                        ("name", Some((_, def))) => def.name = attr(e, "w:val"),
                        ("basedOn", Some((_, def))) => def.based_on = attr(e, "w:val"),
                        ("outlineLvl", Some((_, def))) => {
                            def.outline = attr(e, "w:val").and_then(|v| v.parse().ok());
                        }
                        _ => {}
                    }
                }
                Event::End(e) if local_name(&qname_of(e.name())) == "style" => {
                    if let Some((id, def)) = current.take() {
                        styles.insert(id, def);
                    }
                }
                _ => {}
            }
        }
        Self {
            defs: styles,
            incomplete,
        }
    }

    /// Heading level for a paragraph style id, following `basedOn` (at most 10 steps).
    pub(crate) fn heading_level(&self, style_id: &str) -> Option<u8> {
        let mut id = style_id;
        for _ in 0..MAX_BASED_ON_STEPS {
            let Some(def) = self.defs.get(id) else {
                // Unknown style: Word's built-in ids are `Heading1`, `Title`, ...
                return heading_from_name(id);
            };
            if let Some(level) = def.name.as_deref().and_then(heading_from_name) {
                return Some(level);
            }
            if let Some(level) = def.outline.and_then(outline_to_heading) {
                return Some(level);
            }
            id = def.based_on.as_deref()?;
        }
        None
    }
}

#[derive(Debug, Default)]
pub(crate) struct Numbering {
    /// `w:numId` → `w:abstractNumId`.
    nums: HashMap<String, String>,
    /// (abstract id, level) → `w:numFmt`.
    formats: HashMap<(String, usize), String>,
    pub(crate) incomplete: bool,
}

impl Numbering {
    pub(crate) fn parse(xml: &[u8]) -> Self {
        let mut reader = reader(xml);
        let mut out = Self::default();
        let mut abstract_id: Option<String> = None;
        let mut level: Option<usize> = None;
        let mut num_id: Option<String> = None;
        loop {
            let Ok(event) = reader.read_event() else {
                out.incomplete = true;
                break;
            };
            match &event {
                Event::Eof => break,
                Event::Start(e) | Event::Empty(e) => {
                    let qname = qname(e);
                    match local_name(&qname) {
                        "abstractNum" => abstract_id = attr(e, "w:abstractNumId"),
                        "lvl" if abstract_id.is_some() => {
                            level = attr(e, "w:ilvl").and_then(|v| v.parse().ok());
                        }
                        "numFmt" => {
                            if let (Some(id), Some(lvl), Some(fmt)) =
                                (&abstract_id, level, attr(e, "w:val"))
                            {
                                out.formats.insert((id.clone(), lvl), fmt);
                            }
                        }
                        "num" => num_id = attr(e, "w:numId"),
                        "abstractNumId" => {
                            if let (Some(num), Some(abs)) = (&num_id, attr(e, "w:val")) {
                                out.nums.insert(num.clone(), abs);
                            }
                        }
                        _ => {}
                    }
                }
                Event::End(e) => {
                    let qname = qname_of(e.name());
                    match local_name(&qname) {
                        "abstractNum" => {
                            abstract_id = None;
                            level = None;
                        }
                        "lvl" => level = None,
                        "num" => num_id = None,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Any explicit `numFmt` other than `bullet` is numbered; a missing definition falls back to a bullet.
    pub(crate) fn is_numbered(&self, num_id: &str, level: usize) -> bool {
        self.nums
            .get(num_id)
            .and_then(|abs| self.formats.get(&(abs.clone(), level)))
            .is_some_and(|fmt| fmt != "bullet")
    }
}
