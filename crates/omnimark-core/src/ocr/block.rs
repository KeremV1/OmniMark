//! The Markdown around recognized text (markitdown-ocr markers).

pub const OCR_OPEN: &str = "*[Image OCR]";
pub const OCR_CLOSE: &str = "[End OCR]*";
/// Written for a scanned PDF page whose OCR found no text.
pub const NO_PAGE_TEXT: &str = "*[No text could be extracted from this page]*";

/// PDF pages and single image files: the text as recognized, unescaped (markitdown-ocr PDF).
pub fn block_plain(text: &str) -> String {
    format!("{OCR_OPEN}\n{text}\n{OCR_CLOSE}")
}

/// Office documents: escaped lines joined by Markdown hard breaks (`\` at the line end). The
/// two-space breaks markitdown-ocr writes would be removed by the final normalization.
pub fn block_markdown(text: &str) -> String {
    let mut out = String::from(OCR_OPEN);
    for line in lines(text) {
        out.push_str("\\\n");
        out.push_str(&escape_ocr_text(line));
    }
    out.push_str("\\\n");
    out.push_str(OCR_CLOSE);
    out
}

/// Table cells: a single line, recognized lines joined by spaces.
pub fn block_inline(text: &str) -> String {
    let joined = lines(text)
        .map(escape_ocr_text)
        .collect::<Vec<_>>()
        .join(" ");
    format!("{OCR_OPEN} {joined} {OCR_CLOSE}")
}

/// Escapes `\`, `*`, `_`, `[` and `]` so recognized text cannot start emphasis or links.
pub fn escape_ocr_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        if matches!(ch, '\\' | '*' | '_' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::normalize;

    #[test]
    fn plain_block_matches_markitdown_ocr_pdf() {
        assert_eq!(
            block_plain("MOCK_OCR_TEXT_12345"),
            "*[Image OCR]\nMOCK_OCR_TEXT_12345\n[End OCR]*"
        );
    }

    #[test]
    fn markdown_block_escapes_and_uses_hard_breaks_that_survive_normalization() {
        assert_eq!(
            block_markdown("MOCK_OCR_TEXT_12345"),
            "*[Image OCR]\\\nMOCK\\_OCR\\_TEXT\\_12345\\\n[End OCR]*"
        );
        assert_eq!(
            block_markdown("a *b*\n\n  [c] \\d  \n"),
            "*[Image OCR]\\\na \\*b\\*\\\n\\[c\\] \\\\d\\\n[End OCR]*"
        );
        let block = block_markdown("x\ny");
        assert_eq!(normalize(&block), block);
    }

    #[test]
    fn inline_block_is_one_line() {
        assert_eq!(
            block_inline("one\ntwo_2\n"),
            "*[Image OCR] one two\\_2 [End OCR]*"
        );
    }
}
