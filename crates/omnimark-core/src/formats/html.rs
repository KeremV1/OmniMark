use htmd::element_handler::{HandlerResult, Handlers};
use htmd::options::{BulletListMarker, Options};
use htmd::{Element, HtmlToMarkdown};

use crate::detect::decode_text;
use crate::error::{ConvertError, Result};
use crate::registry::{ConvertContext, Converter, Priority};
use crate::{Document, FileInfo, Input};

const MAX_DEPTH: usize = 1500;
/// Upper bound on the number of start tags; larger documents fall back to plain text so the
/// DOM built by html5ever/htmd stays bounded.
const MAX_ELEMENTS: usize = 200_000;
const STACK_SIZE: usize = 64 * 1024 * 1024;
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];
const BLOCK_ELEMENTS: &[&str] = &[
    "p", "div", "br", "li", "h1", "h2", "h3", "h4", "h5", "h6", "tr",
];

pub struct HtmlConverter;

impl Converter for HtmlConverter {
    fn name(&self) -> &'static str {
        "html"
    }

    fn priority(&self) -> Priority {
        Priority::Generic
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["html", "htm"])
            || info.mime_starts_with(&["text/html", "application/xhtml"])
    }

    fn convert(&self, input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        let html = decode_html(&input.bytes);
        let title = extract_title(&html);
        let (depth, elements) = html_stats(&html);
        let mut document = if depth > MAX_DEPTH || elements > MAX_ELEMENTS {
            let mut d = Document::new(strip_tags(&html));
            d.warn("HTML is too deeply nested; converted as plain text.");
            d
        } else {
            let owned = html.clone();
            let markdown = std::thread::Builder::new()
                .stack_size(STACK_SIZE)
                .spawn(move || html_to_markdown_checked(&owned))
                .map_err(ConvertError::Io)?
                .join()
                .map_err(|_| ConvertError::Corrupt("HTML conversion failed".into()))?;
            let (markdown, converted) = markdown;
            let mut d = Document::new(markdown);
            if !converted {
                d.warn("HTML could not be fully converted; plain text was used.");
            }
            d
        };
        document.title = title;
        Ok(document)
    }
}

fn attr<'a>(el: &'a Element, name: &str) -> Option<&'a str> {
    el.attrs
        .iter()
        .find(|a| &*a.name.local == name)
        .map(|a| &*a.value)
}

fn title_suffix(el: &Element) -> String {
    attr(el, "title")
        .map(|t| format!(" \"{}\"", t.replace('"', "\\\"")))
        .unwrap_or_default()
}

fn is_data_uri(s: &str) -> bool {
    s.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("data:"))
}

#[cfg(test)]
pub(crate) fn html_to_markdown(html: &str) -> String {
    html_to_markdown_checked(html).0
}

/// Returns the Markdown and whether htmd succeeded (`false` means the plain-text fallback ran).
fn html_to_markdown_checked(html: &str) -> (String, bool) {
    let converter = HtmlToMarkdown::builder()
        .options(Options {
            bullet_list_marker: BulletListMarker::Dash,
            ul_bullet_spacing: 1,
            ..Options::default()
        })
        .skip_tags(vec!["head", "script", "style", "noscript", "template"])
        .add_handler(
            vec!["a"],
            |h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
                let text = h.walk_children(el.node).content;
                let text = text.trim();
                if text.is_empty() {
                    return Some("".into());
                }
                let Some(href) = attr(&el, "href").map(str::trim).filter(|h| !h.is_empty()) else {
                    return Some(text.to_string().into());
                };
                let scheme = href
                    .split_once(':')
                    .map(|(s, _)| s.to_ascii_lowercase())
                    .filter(|s| !s.contains(['/', '?', '#']));
                let target = match scheme.as_deref() {
                    None | Some("http" | "https" | "file") => quote_url(href),
                    Some("mailto" | "tel") => href.to_string(),
                    Some(_) => return Some(text.to_string().into()),
                };
                let title = title_suffix(&el);
                if title.is_empty() && text.replace("\\_", "_") == href {
                    return Some(format!("<{href}>").into());
                }
                Some(format!("[{text}]({target}{title})").into())
            },
        )
        .add_handler(
            vec!["img"],
            |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
                let alt = attr(&el, "alt").unwrap_or("").replace(['\r', '\n'], " ");
                let mut src = attr(&el, "src").unwrap_or("").to_string();
                let data_src = attr(&el, "data-src").unwrap_or("");
                if !data_src.is_empty() && (src.is_empty() || is_data_uri(&src)) {
                    src = data_src.to_string();
                }
                if is_data_uri(&src) {
                    src = format!("{}...", src.split(',').next().unwrap_or(""));
                }
                let title = title_suffix(&el);
                Some(format!("![{alt}]({src}{title})").into())
            },
        )
        .add_handler(
            vec!["input"],
            |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
                if attr(&el, "type").is_some_and(|t| t.eq_ignore_ascii_case("checkbox")) {
                    let checked = el.attrs.iter().any(|a| &*a.name.local == "checked");
                    return Some(if checked { "[x] " } else { "[ ] " }.into());
                }
                Some("".into())
            },
        )
        .add_handler(
            vec!["u"],
            |h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
                let inner = h.walk_children(el.node).content;
                if inner.trim().is_empty() {
                    return Some(inner.into());
                }
                Some(format!("<u>{inner}</u>").into())
            },
        )
        .add_handler(
            vec!["s", "strike", "del"],
            |h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
                let inner = h.walk_children(el.node).content;
                Some(format!("~~{}~~", inner.trim()).into())
            },
        )
        .build();
    match converter.convert(html) {
        Ok(markdown) => (markdown, true),
        Err(_) => (strip_tags(html), false),
    }
}

/// Decodes HTML bytes: BOM, then `<meta charset>` within the first 4 KB, then generic detection.
pub(crate) fn decode_html(bytes: &[u8]) -> String {
    if encoding_rs::Encoding::for_bom(bytes).is_some() {
        return decode_text(bytes);
    }
    let head: Vec<u8> = bytes
        .iter()
        .take(4096)
        .map(|b| b.to_ascii_lowercase())
        .collect();
    if let Some(pos) = head.windows(8).position(|w| w == b"charset=") {
        let label: Vec<u8> = head[pos + 8..]
            .iter()
            .skip_while(|b| matches!(**b, b'"' | b'\'' | b' '))
            .copied()
            .take_while(|b| {
                !matches!(
                    *b,
                    b'"' | b'\'' | b';' | b'>' | b' ' | b'\t' | b'\r' | b'\n' | b'/'
                )
            })
            .collect();
        if let Some(encoding) = encoding_rs::Encoding::for_label(&label) {
            return encoding.decode_without_bom_handling(bytes).0.into_owned();
        }
    }
    decode_text(bytes)
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = start + lower[start..].find('>')? + 1;
    let close = open_end + lower[open_end..].find("</title")?;
    let title = decode_entities(html[open_end..close].trim());
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
}

fn decode_entities(s: &str) -> String {
    const ENTITIES: [(&str, &str); 5] = [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
    ];
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match ENTITIES.iter().find(|(ent, _)| rest.starts_with(ent)) {
            Some((ent, rep)) => {
                out.push_str(rep);
                rest = &rest[ent.len()..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_hex_pair(b: &[u8]) -> bool {
    b.len() >= 2 && b[0].is_ascii_hexdigit() && b[1].is_ascii_hexdigit()
}

/// Percent-encodes only the path, like Python's `urllib.parse.quote(path, safe="/")`,
/// keeping existing valid `%HH` escapes. Scheme, host, query and fragment are untouched.
pub(crate) fn quote_url(href: &str) -> String {
    let (prefix, rest) = match href.find("://") {
        Some(i) => {
            let after = i + 3;
            let end = href[after..]
                .find(['/', '?', '#'])
                .map_or(href.len(), |j| after + j);
            href.split_at(end)
        }
        None => ("", href),
    };
    let split = rest.find(['?', '#']).unwrap_or(rest.len());
    let (path, tail) = rest.split_at(split);
    let bytes = path.as_bytes();
    let mut out = String::with_capacity(href.len());
    out.push_str(prefix);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && is_hex_pair(&bytes[i + 1..]) {
            out.push_str(&path[i..i + 3]);
            i += 3;
            continue;
        }
        if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
        i += 1;
    }
    out.push_str(tail);
    out
}

/// Parses the tag starting at `html[i] == '<'`. Returns (lowercased name, is_closing,
/// self_closing, index just past the tag).
fn scan_tag(html: &str, i: usize) -> (String, bool, bool, usize) {
    let end = html[i..].find('>').map_or(html.len(), |j| i + j + 1);
    let inner_end = if html[..end].ends_with('>') {
        end - 1
    } else {
        end
    };
    let inner = &html[i + 1..inner_end.max(i + 1)];
    let closing = inner.starts_with('/');
    let name: String = inner
        .trim_start_matches('/')
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '/')
        .collect::<String>()
        .to_ascii_lowercase();
    (name, closing, inner.ends_with('/'), end)
}

/// Index just past the closing tag of a raw-text element, or the end of input.
fn skip_raw_text(html: &str, from: usize, name: &str) -> usize {
    let needle = format!("</{name}");
    let n = needle.len();
    let bytes = html.as_bytes();
    let found = (from..bytes.len().saturating_sub(n - 1))
        .find(|&j| bytes[j..j + n].eq_ignore_ascii_case(needle.as_bytes()));
    match found {
        Some(j) => html[j..].find('>').map_or(html.len(), |k| j + k + 1),
        None => html.len(),
    }
}

fn skip_markup_decl(html: &str, i: usize) -> usize {
    if html[i..].starts_with("<!--") {
        html[i + 4..]
            .find("-->")
            .map_or(html.len(), |j| i + 4 + j + 3)
    } else {
        html[i..].find('>').map_or(html.len(), |j| i + j + 1)
    }
}

fn starts_tag(html: &str, i: usize) -> bool {
    matches!(html.as_bytes().get(i + 1), Some(c) if *c == b'/' || c.is_ascii_alphabetic())
}

#[cfg(test)]
pub(crate) fn max_tag_depth(html: &str) -> usize {
    html_stats(html).0
}

/// Returns (maximum nesting depth, number of start tags). HTML parsers ignore `/>` on
/// non-void elements, so `<div/>` counts as an open element (the safe side, also in svg/math).
fn html_stats(html: &str) -> (usize, usize) {
    let (mut depth, mut max, mut elements, mut i) = (0usize, 0usize, 0usize, 0usize);
    while let Some(off) = html[i..].find('<') {
        i += off;
        if matches!(html.as_bytes().get(i + 1), Some(b'!' | b'?')) {
            i = skip_markup_decl(html, i);
            continue;
        }
        if !starts_tag(html, i) {
            i += 1;
            continue;
        }
        let (name, closing, _, end) = scan_tag(html, i);
        i = end;
        if closing {
            depth = depth.saturating_sub(1);
        } else {
            elements += 1;
        }
        if !closing && !VOID_ELEMENTS.contains(&name.as_str()) {
            if name == "script" || name == "style" {
                i = skip_raw_text(html, i, &name);
            } else {
                depth += 1;
                max = max.max(depth);
            }
        }
    }
    (max, elements)
}

pub(crate) fn strip_tags(html: &str) -> String {
    fn newline(out: &mut String) {
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
    }
    let mut out = String::new();
    let mut i = 0;
    while i < html.len() {
        let Some(off) = html[i..].find('<') else {
            out.push_str(&html[i..]);
            break;
        };
        out.push_str(&html[i..i + off]);
        i += off;
        if matches!(html.as_bytes().get(i + 1), Some(b'!' | b'?')) {
            i = skip_markup_decl(html, i);
        } else if starts_tag(html, i) {
            let (name, closing, _, end) = scan_tag(html, i);
            i = end;
            if !closing && matches!(name.as_str(), "script" | "style" | "title") {
                i = skip_raw_text(html, i, &name);
            }
            if BLOCK_ELEMENTS.contains(&name.as_str()) && (closing || name == "br") {
                newline(&mut out);
            }
        } else {
            out.push('<');
            i += 1;
        }
    }
    decode_entities(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_only_with_atx_headings_and_no_scripts() {
        let md = html_to_markdown("<html><head><title>T</title><style>p{}</style></head><body><h2>Başlık</h2><script>x()</script><p>Metin</p></body></html>");
        assert_eq!(md.trim(), "## Başlık\n\nMetin");
    }

    #[test]
    fn links_are_filtered_and_quoted() {
        assert_eq!(
            html_to_markdown(r#"<a href="javascript:alert(1)">bad</a>"#).trim(),
            "bad"
        );
        assert_eq!(
            html_to_markdown(r#"<a href="mailto:a@b.c">mail</a>"#).trim(),
            "[mail](mailto:a@b.c)"
        );
        assert_eq!(
            html_to_markdown(r#"<a href="https://example.com/a path/日本語">example</a>"#).trim(),
            "[example](https://example.com/a%20path/%E6%97%A5%E6%9C%AC%E8%AA%9E)"
        );
        assert_eq!(
            html_to_markdown(r#"<a href="/wiki/Bill_Gates" title="Bill Gates">Bill Gates</a>"#)
                .trim(),
            r#"[Bill Gates](/wiki/Bill_Gates "Bill Gates")"#
        );
    }

    #[test]
    fn quote_url_matches_python_quote() {
        assert_eq!(
            quote_url("https://example.com/100% complete"),
            "https://example.com/100%25%20complete"
        );
        assert_eq!(
            quote_url("https://example.com/items/%ZZ/%2F"),
            "https://example.com/items/%25ZZ/%2F"
        );
        assert_eq!(
            quote_url("https://example.com/a path?query=a b%20c#fragment with spaces"),
            "https://example.com/a%20path?query=a b%20c#fragment with spaces"
        );
        assert_eq!(
            quote_url("https://example.com/%a5%c8"),
            "https://example.com/%a5%c8"
        );
    }

    #[test]
    fn images_checkboxes_and_strikes() {
        assert_eq!(
            html_to_markdown(r#"<img alt="dot" src="DATA:image/png;base64,AAAA">"#).trim(),
            "![dot](DATA:image/png;base64...)"
        );
        assert_eq!(
            html_to_markdown(
                r#"<img alt="lazy" src="data:image/gif;base64,R0" data-src="/real.png">"#
            )
            .trim(),
            "![lazy](/real.png)"
        );
        assert_eq!(
            html_to_markdown("<p>Plain <s>s element</s> after.</p>").trim(),
            "Plain ~~s element~~ after."
        );
        assert_eq!(
            html_to_markdown("<p>plain <u>underlined</u></p>").trim(),
            "plain <u>underlined</u>"
        );
        let list = html_to_markdown(
            r#"<ul><li><input type="checkbox" checked>done</li><li><input type="checkbox">todo</li></ul>"#,
        );
        assert!(
            list.contains("- [x] done") && list.contains("- [ ] todo"),
            "{list}"
        );
    }

    #[test]
    fn deep_nesting_falls_back_to_plain_text() {
        let html = format!("{}deep{}", "<div>".repeat(5000), "</div>".repeat(5000));
        assert_eq!(max_tag_depth(&html), 5000);
        assert_eq!(strip_tags(&html).trim(), "deep");
    }

    fn run_convert(html: &str) -> Document {
        use crate::assets::MemoryAssetSink;
        let options = crate::ConvertOptions::default();
        let mut assets = MemoryAssetSink::new("x_assets");
        let cancel = crate::CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        let input = Input {
            path: std::path::PathBuf::from("a.html"),
            bytes: html.as_bytes().to_vec(),
        };
        HtmlConverter.convert(&input, &mut ctx).unwrap()
    }

    const NESTED_WARNING: &str = "HTML is too deeply nested; converted as plain text.";

    #[test]
    fn self_closing_non_void_tags_count_as_open() {
        let html = "<div/>".repeat(5000);
        assert_eq!(max_tag_depth(&html), 5000);
        let doc = run_convert(&format!("<title>T</title>{html}deep"));
        assert!(
            doc.warnings.iter().any(|w| w == NESTED_WARNING),
            "{:?}",
            doc.warnings
        );
        assert_eq!(doc.markdown.trim(), "deep");
        assert_eq!(doc.title.as_deref(), Some("T"));
    }

    #[test]
    fn void_self_closing_tags_do_not_add_depth() {
        let html = "<br/>".repeat(5000);
        assert_eq!(max_tag_depth(&html), 0);
        assert!(run_convert(&html).warnings.is_empty());
    }

    #[test]
    fn too_many_elements_fall_back_to_plain_text() {
        let html = format!("<p>{}</p>", "<i></i>".repeat(MAX_ELEMENTS));
        let doc = run_convert(&html);
        assert!(
            doc.warnings.iter().any(|w| w == NESTED_WARNING),
            "{:?}",
            doc.warnings
        );
    }

    #[test]
    fn normal_html_converts_without_warnings() {
        let doc = run_convert("<h1>Hi</h1><p>there</p>");
        assert!(doc.warnings.is_empty());
        assert_eq!(
            doc.markdown.trim(),
            "# Hi

there"
        );
    }

    #[test]
    fn meta_charset_is_honoured() {
        let mut bytes = b"<html><head><meta charset=\"windows-1254\"></head><body><p>".to_vec();
        bytes.extend_from_slice(&[0xDE, 0xFD]); // "Şı" in windows-1254
        bytes.extend_from_slice(b"</p></body></html>");
        assert_eq!(
            decode_html(&bytes),
            "<html><head><meta charset=\"windows-1254\"></head><body><p>Şı</p></body></html>"
        );
    }
}
