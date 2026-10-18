//! Inline content of a paragraph: formatted text runs and pre-rendered pieces.

use crate::markdown::escape_text;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Fmt {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Seg {
    /// Plain text with run formatting; escaped when rendered.
    Text(Fmt, String),
    /// Already Markdown (images, links, math, note references); never escaped.
    Raw(String),
}

/// `w:b`-style toggle: absent value or anything but an explicit "off" means on.
pub(crate) fn toggle_on(value: Option<&str>) -> bool {
    !matches!(value, Some("0" | "false" | "off" | "none"))
}

/// Merges adjacent runs with the same format, escapes, and wraps in Markdown markers.
pub(crate) fn render_segs(segs: &[Seg]) -> String {
    let mut out = String::new();
    let mut pending: Option<(Fmt, String)> = None;
    for seg in segs {
        match seg {
            Seg::Text(fmt, text) => match pending.as_mut() {
                Some((current, buffer)) if current == fmt => buffer.push_str(text),
                _ => {
                    flush(&mut out, pending.take());
                    pending = Some((*fmt, text.clone()));
                }
            },
            Seg::Raw(raw) => {
                flush(&mut out, pending.take());
                out.push_str(raw);
            }
        }
    }
    flush(&mut out, pending.take());
    out
}

fn flush(out: &mut String, pending: Option<(Fmt, String)>) {
    let Some((fmt, text)) = pending else { return };
    // Markers must hug the text: whitespace goes outside, and each line is wrapped on its own.
    let mut lines = text.split('\n').peekable();
    while let Some(line) = lines.next() {
        let core = line.trim();
        if core.is_empty() {
            out.push_str(line);
        } else {
            let start = line.len() - line.trim_start().len();
            out.push_str(&line[..start]);
            out.push_str(&wrap(&escape_text(core), fmt));
            out.push_str(&line[start + core.len()..]);
        }
        if lines.peek().is_some() {
            out.push('\n');
        }
    }
}

fn wrap(text: &str, fmt: Fmt) -> String {
    let mut s = text.to_string();
    if fmt.underline {
        s = format!("<u>{s}</u>");
    }
    if fmt.strike {
        s = format!("~~{s}~~");
    }
    if fmt.italic {
        s = format!("*{s}*");
    }
    if fmt.bold {
        s = format!("**{s}**");
    }
    s
}

/// Only links that are safe to open from a Markdown viewer keep their destination.
pub(crate) fn safe_link_target(target: &str) -> Option<String> {
    let lower = target.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:", "tel:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
        .then(|| encode_destination(target.trim()))
}

fn encode_destination(target: &str) -> String {
    let mut out = String::with_capacity(target.len());
    for ch in target.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}
