//! Office Math (OMML) → LaTeX. Behavior follows MarkItDown 0.1.8 (dwml port) with the
//! improvements listed in `docs/research/markitdown-davranis-spec.md` §3.4.

mod latex_table;

use crate::xml::{append_text, attr, local_name, reader};
use quick_xml::events::{BytesStart, Event};
use std::collections::HashMap;

/// Maximum element nesting accepted; deeper input is rejected instead of recursing further.
const MAX_NESTING_DEPTH: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OmmlError {
    #[error("invalid OMML: {0}")]
    Xml(String),
    #[error("unsupported OMML construct: {0}")]
    Unsupported(String),
}

/// Converts one `<m:oMath>` element (its full XML text, namespace prefixes as in the document)
/// to LaTeX without surrounding `$` delimiters.
pub fn omml_to_latex(omath_xml: &str) -> Result<String, OmmlError> {
    let root = parse(omath_xml)?;
    latex(&root, 0)
}

/// Element of the math tree. `name` is the local name; `attrs` keys are local names too.
#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: HashMap<String, String>,
    children: Vec<Node>,
    text: String,
}

fn node_from(start: &BytesStart<'_>) -> Node {
    let qualified = start.name().as_ref().to_string();
    let mut attrs = HashMap::new();
    for a in start.attributes().flatten() {
        let key = a.key.as_ref().to_string();
        if let Some(value) = attr(start, &key) {
            attrs.insert(local_name(&key).to_string(), value);
        }
    }
    Node {
        name: local_name(&qualified).to_string(),
        attrs,
        ..Node::default()
    }
}

fn prefix_of(start: &BytesStart<'_>) -> String {
    let qualified = start.name().as_ref().to_string();
    match qualified.rsplit_once(':') {
        Some((prefix, _)) => prefix.to_string(),
        None => String::new(),
    }
}

/// Builds the tree iteratively. Only elements sharing the root's prefix (the math namespace)
/// are kept; foreign elements such as `w:rPr` are skipped with their subtree.
fn parse(xml: &str) -> Result<Node, OmmlError> {
    let mut rd = reader(xml.as_bytes());
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let mut root_prefix: Option<String> = None;
    let mut skipping = 0usize;
    loop {
        let event = rd.read_event().map_err(|e| OmmlError::Xml(e.to_string()))?;
        match &event {
            Event::Start(start) | Event::Empty(start) => {
                let is_empty = matches!(event, Event::Empty(_));
                if skipping > 0 {
                    if !is_empty {
                        skipping += 1;
                    }
                    continue;
                }
                let prefix = prefix_of(start);
                let expected = root_prefix.get_or_insert_with(|| prefix.clone());
                if *expected != prefix {
                    if !is_empty {
                        skipping = 1;
                    }
                    continue;
                }
                if stack.len() >= MAX_NESTING_DEPTH {
                    return Err(OmmlError::Unsupported(format!(
                        "nesting deeper than {MAX_NESTING_DEPTH} levels"
                    )));
                }
                let node = node_from(start);
                if is_empty {
                    attach(&mut stack, &mut root, node)?;
                } else {
                    stack.push(node);
                }
            }
            Event::End(_) => {
                if skipping > 0 {
                    skipping -= 1;
                    continue;
                }
                let node = stack
                    .pop()
                    .ok_or_else(|| OmmlError::Xml("unbalanced end tag".into()))?;
                attach(&mut stack, &mut root, node)?;
            }
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                if skipping == 0 {
                    if let Some(top) = stack.last_mut() {
                        append_text(&mut top.text, &event);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(OmmlError::Xml("unexpected end of input".into()));
    }
    root.ok_or_else(|| OmmlError::Xml("no math element found".into()))
}

fn attach(stack: &mut [Node], root: &mut Option<Node>, node: Node) -> Result<(), OmmlError> {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None if root.is_none() => *root = Some(node),
        None => return Err(OmmlError::Xml("multiple root elements".into())),
    }
    Ok(())
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// `m:val` of the named property child of `props` (e.g. `chr` in `naryPr`).
    fn prop(&self, props: &str, name: &str) -> Option<&str> {
        self.child(props)?
            .child(name)?
            .attrs
            .get("val")
            .map(String::as_str)
    }

    /// All text of `m:t` descendants, concatenated in document order.
    fn plain_text(&self) -> String {
        let mut out = String::new();
        if self.name == "t" {
            out.push_str(&self.text);
        }
        for child in &self.children {
            out.push_str(&child.plain_text());
        }
        out
    }
}

/// LaTeX of the named child, or empty when it is absent.
fn part(node: &Node, name: &str, depth: usize) -> Result<String, OmmlError> {
    match node.child(name) {
        Some(child) => latex(child, depth + 1),
        None => Ok(String::new()),
    }
}

/// Concatenated LaTeX of all children; property elements (`*Pr`) carry no content.
fn children(node: &Node, depth: usize) -> Result<String, OmmlError> {
    let mut out = String::new();
    for child in node.children.iter().filter(|c| !c.name.ends_with("Pr")) {
        out.push_str(&latex(child, depth + 1)?);
    }
    Ok(out)
}

fn latex(node: &Node, depth: usize) -> Result<String, OmmlError> {
    if depth > MAX_NESTING_DEPTH {
        return Err(OmmlError::Unsupported("nesting too deep".into()));
    }
    let d = depth;
    Ok(match node.name.as_str() {
        "r" => run(node),
        "f" => {
            let (num, den) = (part(node, "num", d)?, part(node, "den", d)?);
            match node.prop("fPr", "type") {
                Some("skw") => format!("^{{{num}}}/_{{{den}}}"),
                Some("noBar") => format!("\\genfrac{{}}{{}}{{0pt}}{{}}{{{num}}}{{{den}}}"),
                Some("lin") => format!("{{{num}}}/{{{den}}}"),
                _ => format!("\\frac{{{num}}}{{{den}}}"),
            }
        }
        "sub" => format!("_{{{}}}", children(node, d)?),
        "sup" => format!("^{{{}}}", children(node, d)?),
        "sPre" => format!(
            "{{}}{}{}{}",
            part(node, "sub", d)?,
            part(node, "sup", d)?,
            part(node, "e", d)?
        ),
        "rad" => {
            let (deg, e) = (part(node, "deg", d)?, part(node, "e", d)?);
            if deg.is_empty() {
                format!("\\sqrt{{{e}}}")
            } else {
                format!("\\sqrt[{deg}]{{{e}}}")
            }
        }
        "d" => delimiter(node, d)?,
        "nary" => {
            let op = match node.prop("naryPr", "chr") {
                None => "\\int".to_string(),
                Some(chr) => match chr.chars().next() {
                    Some(c) => latex_table::big_operator(c)
                        .map(str::to_string)
                        .unwrap_or_else(|| escape_latex(&map_symbols(chr))),
                    None => String::new(),
                },
            };
            format!(
                "{op}{}{}{}",
                part(node, "sub", d)?,
                part(node, "sup", d)?,
                part(node, "e", d)?
            )
        }
        "func" => {
            let arg = part(node, "e", d)?;
            let name_node = node.child("fName");
            let plain_runs = name_node.is_some_and(|n| {
                n.children
                    .iter()
                    .filter(|c| !c.name.ends_with("Pr"))
                    .all(|c| c.name == "r")
            });
            match name_node {
                Some(n) if plain_runs => {
                    let name = clean_name(&n.plain_text());
                    match latex_table::function(&name) {
                        Some(cmd) => format!("{cmd}({arg})"),
                        None => format!("\\operatorname{{{}}}({arg})", escape_latex(&name)),
                    }
                }
                // Structured names such as limLow("lim", n→∞) render themselves.
                Some(n) => format!("{}{arg}", children(n, d)?),
                None => arg,
            }
        }
        "acc" => {
            let cmd = node
                .prop("accPr", "chr")
                .and_then(|c| c.chars().next())
                .and_then(latex_table::accent)
                .unwrap_or("\\hat");
            format!("{cmd}{{{}}}", part(node, "e", d)?)
        }
        "bar" => {
            let cmd = match node.prop("barPr", "pos") {
                Some("bot") => "\\underline",
                _ => "\\overline",
            };
            format!("{cmd}{{{}}}", part(node, "e", d)?)
        }
        "groupChr" => {
            let cmd = node
                .prop("groupChrPr", "chr")
                .and_then(|c| c.chars().next())
                .and_then(latex_table::accent)
                .unwrap_or("\\underbrace");
            format!("{cmd}{{{}}}", part(node, "e", d)?)
        }
        "eqArr" => {
            let rows = join_all(node, "e", "\\\\", d)?;
            format!("\\begin{{array}}{{c}}{rows}\\end{{array}}")
        }
        "m" => {
            let mut rows = Vec::new();
            for row in node.children.iter().filter(|c| c.name == "mr") {
                rows.push(join_all(row, "e", "&", d)?);
            }
            format!("\\begin{{matrix}}{}\\end{{matrix}}", rows.join("\\\\"))
        }
        "limLow" => {
            let (e, lim) = (part(node, "e", d)?, part(node, "lim", d)?);
            let base = node.child("e").map(|n| clean_name(&n.plain_text()));
            match base.as_deref() {
                Some(b @ ("lim" | "max" | "min")) => format!("\\{b}_{{{lim}}}"),
                _ => format!("\\underset{{{lim}}}{{{e}}}"),
            }
        }
        "limUpp" => format!(
            "\\overset{{{}}}{{{}}}",
            part(node, "lim", d)?,
            part(node, "e", d)?
        ),
        "lim" => children(node, d)?.replace("\\rightarrow", "\\to"),
        // Unknown and container elements (e, num, den, deg, box, phant, sSub, ...): keep content.
        _ => children(node, d)?,
    })
}

/// Text with invisible operators (U+2061..U+2064) and whitespace removed.
fn clean_name(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !('\u{2061}'..='\u{2064}').contains(c))
        .collect()
}

fn join_all(node: &Node, name: &str, sep: &str, depth: usize) -> Result<String, OmmlError> {
    let mut items = Vec::new();
    for child in node.children.iter().filter(|c| c.name == name) {
        items.push(latex(child, depth + 1)?);
    }
    Ok(items.join(sep))
}

fn delimiter(node: &Node, depth: usize) -> Result<String, OmmlError> {
    let bracket = |name: &str, default: &str| match node.prop("dPr", name) {
        None => default.to_string(),
        Some("") => ".".to_string(),
        Some(value) => escape_latex(&map_symbols(value)),
    };
    let (left, right) = (bracket("begChr", "("), bracket("endChr", ")"));
    let separator = match node.prop("dPr", "sepChr") {
        None => "|".to_string(),
        Some(value) => escape_latex(&map_symbols(value)),
    };
    let body = join_all(node, "e", &separator, depth)?;
    Ok(format!("\\left{left}{body}\\right{right}"))
}

/// Text of a math run: every `m:t`, symbol-mapped and escaped. `m:sty` = `p` makes letters upright
/// (`\mathrm`); runs without ASCII letters (digits, operators) are upright in LaTeX already.
/// Invisible operators (U+2061..U+2064, e.g. "function application") are dropped.
fn run(node: &Node) -> String {
    let raw: String = node
        .plain_text()
        .chars()
        .filter(|c| !('\u{2061}'..='\u{2064}').contains(c))
        .collect();
    let text = escape_latex(&map_symbols(&raw));
    match node.prop("rPr", "sty") {
        Some("p") if raw.chars().any(|c| c.is_ascii_alphabetic()) => {
            format!("\\mathrm{{{text}}}")
        }
        _ => text,
    }
}

fn map_symbols(text: &str) -> String {
    text.chars().fold(String::new(), |mut out, c| {
        match latex_table::symbol(c) {
            Some(mapped) => out.push_str(mapped),
            None => out.push(c),
        }
        out
    })
}

/// `\\` → `\`, then LaTeX-special characters get a backslash unless already escaped.
fn escape_latex(text: &str) -> String {
    const CHARS: [char; 9] = ['{', '}', '_', '^', '#', '&', '$', '%', '~'];
    let text = text.replace("\\\\", "\\");
    let mut out = String::with_capacity(text.len());
    let mut previous = None;
    for c in text.chars() {
        if CHARS.contains(&c) && previous != Some('\\') {
            out.push('\\');
        }
        out.push(c);
        previous = Some(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tex(inner: &str) -> String {
        omml_to_latex(&format!("<m:oMath>{inner}</m:oMath>")).unwrap()
    }
    fn r(t: &str) -> String {
        format!("<m:r><m:t>{t}</m:t></m:r>")
    }

    #[test]
    fn runs_fractions_scripts_and_radicals() {
        assert_eq!(tex(&format!("{}{}", r("m"), r("=1"))), "m=1");
        assert_eq!(
            tex(&format!(
                "<m:f><m:num>{}</m:num><m:den>{}</m:den></m:f>",
                r("a"),
                r("b")
            )),
            r"\frac{a}{b}"
        );
        assert_eq!(
            tex(&format!(
                "<m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub>",
                r("x"),
                r("1")
            )),
            "x_{1}"
        );
        assert_eq!(
            tex(&format!(
                "<m:rad><m:radPr><m:degHide m:val=\"1\"/></m:radPr><m:deg/><m:e>{}</m:e></m:rad>",
                r("2")
            )),
            r"\sqrt{2}"
        );
    }

    #[test]
    fn nary_without_chr_defaults_to_integral() {
        assert_eq!(
            tex(&format!(
                "<m:nary><m:naryPr/><m:sub>{}</m:sub><m:sup>{}</m:sup><m:e>{}</m:e></m:nary>",
                r("0"),
                r("1"),
                r("x")
            )),
            r"\int_{0}^{1}x"
        );
    }

    #[test]
    fn delimiters_functions_and_italic_letters() {
        assert_eq!(
            tex(&format!(
                "<m:d><m:e>{}</m:e><m:e>{}</m:e></m:d>",
                r("a"),
                r("b")
            )),
            r"\left(a|b\right)"
        );
        assert_eq!(
            tex(&format!(
                "<m:func><m:fName>{}</m:fName><m:e>{}</m:e></m:func>",
                r("sin"),
                r("x")
            )),
            r"\sin(x)"
        );
        assert_eq!(tex(&r("ℎ(𝑥)=𝑔(𝑥)")), "h(x)=g(x)");
    }

    #[test]
    fn invalid_xml_is_an_error() {
        assert!(omml_to_latex("<m:oMath><m:r>").is_err());
    }

    #[test]
    fn improvements_over_markitdown() {
        let e = |t: &str| format!("<m:e>{}</m:e>", r(t));
        // acc without chr defaults to \hat; with U+20D7 becomes \vec
        assert_eq!(tex(&format!("<m:acc>{}</m:acc>", e("x"))), r"\hat{x}");
        assert_eq!(
            tex(&format!(
                "<m:acc><m:accPr><m:chr m:val=\"\u{20d7}\"/></m:accPr>{}</m:acc>",
                e("v")
            )),
            r"\vec{v}"
        );
        // unknown limLow base
        assert_eq!(
            tex(&format!(
                "<m:limLow>{}<m:lim>{}</m:lim></m:limLow>",
                e("x"),
                r("n")
            )),
            r"\underset{n}{x}"
        );
        assert_eq!(
            tex(&format!(
                "<m:limLow>{}<m:lim>{}</m:lim></m:limLow>",
                e("lim"),
                r("n→∞")
            )),
            r"\lim_{n\to \infty }"
        );
        // sPre keeps its content
        assert_eq!(
            tex(&format!(
                "<m:sPre><m:sub>{}</m:sub><m:sup>{}</m:sup>{}</m:sPre>",
                r("a"),
                r("b"),
                e("X")
            )),
            "{}_{a}^{b}X"
        );
        // upright style, multiple m:t, escaping, entities
        assert_eq!(
            tex("<m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr><m:t>ab</m:t><m:t>c</m:t></m:r>"),
            r"\mathrm{abc}"
        );
        assert_eq!(tex(&r("a_b &amp; 50%")), r"a\_b \& 50\%");
        // unknown element keeps content, w: properties are ignored
        assert_eq!(
            tex(&format!(
                "<m:phant>{}</m:phant><m:r><w:rPr><w:b/></w:rPr><m:t>z</m:t></m:r>",
                e("q")
            )),
            "qz"
        );
        // named functions and matrices
        assert_eq!(
            tex(&format!(
                "<m:func><m:fName>{}{}</m:fName>{}</m:func>",
                r("my"),
                r("fn"),
                e("x")
            )),
            r"\operatorname{myfn}(x)"
        );
        assert_eq!(
            tex(&format!(
                "<m:m><m:mr>{0}{1}</m:mr><m:mr>{1}{0}</m:mr></m:m>",
                e("1"),
                e("0")
            )),
            r"\begin{matrix}1&0\\0&1\end{matrix}"
        );
        // brace delimiters are escaped; empty chr becomes a dot
        assert_eq!(
            tex(&format!(
                "<m:d><m:dPr><m:begChr m:val=\"{{\"/><m:endChr m:val=\"\"/></m:dPr>{}</m:d>",
                e("x")
            )),
            r"\left\{x\right."
        );
    }

    #[test]
    fn other_prefixes_and_math_para_are_accepted() {
        assert_eq!(
            omml_to_latex("<mm:oMath><mm:r><mm:t>k</mm:t></mm:r></mm:oMath>").unwrap(),
            "k"
        );
        assert_eq!(
            omml_to_latex("<m:oMathPara><m:oMath><m:r><m:t>a</m:t></m:r></m:oMath><m:oMath><m:r><m:t>b</m:t></m:r></m:oMath></m:oMathPara>").unwrap(),
            "ab"
        );
    }

    #[test]
    fn very_deep_nesting_is_rejected_not_overflowed() {
        let depth = 5000;
        let xml = format!(
            "<m:oMath>{}{}{}</m:oMath>",
            "<m:e>".repeat(depth),
            r("x"),
            "</m:e>".repeat(depth)
        );
        assert!(matches!(
            omml_to_latex(&xml),
            Err(OmmlError::Unsupported(_))
        ));
    }

    #[test]
    fn lim_constructs_from_word() {
        let sty = "<m:rPr><m:sty m:val=\"p\"/></m:rPr>";
        let lim_low = format!(
            "<m:limLow><m:e><m:r>{sty}<m:t>lim</m:t></m:r></m:e><m:lim>{}</m:lim></m:limLow>",
            r("n→∞")
        );
        assert_eq!(tex(&lim_low), r"\lim_{n\to \infty }");
        assert_eq!(
            tex(&format!(
                "<m:func><m:fName>{lim_low}</m:fName><m:e>{}</m:e></m:func>",
                r("x")
            )),
            r"\lim_{n\to \infty }x"
        );
    }

    #[test]
    fn upright_style_only_wraps_letters() {
        let up = |t: &str| format!("<m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr><m:t>{t}</m:t></m:r>");
        assert_eq!(tex(&up("=1")), "=1");
        assert_eq!(tex(&up("sin\u{2061}")), r"\mathrm{sin}");
    }
}
