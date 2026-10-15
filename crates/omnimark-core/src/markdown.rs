//! Shared Markdown building blocks used by every converter.

/// Final normalization applied to every conversion result: trailing whitespace is removed
/// from each line, runs of blank lines collapse to one, and leading/trailing newlines are trimmed.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0usize;
    for line in text.split('\n') {
        let line = line.trim_end();
        if line.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_matches('\n').to_string()
}

/// Escapes a GFM table cell: the run of backslashes before each `|` is doubled and the pipe
/// escaped (MarkItDown `_escape_table_cell`); line breaks become spaces. Runs in linear time.
pub fn escape_table_cell(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len() + 8);
    let mut backslashes = 0usize;
    let mut chars = cell.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            backslashes += 1;
            continue;
        }
        let pending = if ch == '|' {
            backslashes * 2
        } else {
            backslashes
        };
        out.extend(std::iter::repeat_n('\\', pending));
        backslashes = 0;
        match ch {
            '|' => out.push_str("\\|"),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(' ');
            }
            '\n' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes));
    out
}

/// Renders rows as a GFM table. The first row is the header; rows are padded to the widest row.
pub fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return String::new();
    }
    let mut lines = Vec::with_capacity(rows.len() + 1);
    for (index, row) in rows.iter().enumerate() {
        let cells: Vec<String> = (0..columns)
            .map(|c| row.get(c).map(|s| escape_table_cell(s)).unwrap_or_default())
            .collect();
        lines.push(format!("| {} |", cells.join(" | ")));
        if index == 0 {
            lines.push(format!("| {} |", vec!["---"; columns].join(" | ")));
        }
    }
    lines.join("\n")
}

/// Escapes emphasis markers in plain document text (same set markdownify escapes by default).
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '*' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Builds `![alt](target)`; brackets and line breaks are removed from `alt`.
/// `None` (image extraction disabled) produces an empty destination.
pub fn image(alt: &str, target: Option<&str>) -> String {
    let cleaned: String = alt
        .chars()
        .map(|c| {
            if matches!(c, '[' | ']' | '\r' | '\n') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let alt = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("![{alt}]({})", target.unwrap_or(""))
}

/// Percent-encodes characters that would break a Markdown link destination.
pub fn link_target(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for ch in path.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_lines_and_collapses_blank_runs() {
        assert_eq!(normalize("a  \r\nb\t\n\n\n\nc\n\n"), "a\nb\n\nc");
        assert_eq!(normalize("\n\n  indented\n"), "  indented");
        assert_eq!(normalize("x\u{c}\ny"), "x\ny");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn escape_table_cell_matches_markitdown() {
        assert_eq!(escape_table_cell("cheap | fast"), r"cheap \| fast");
        assert_eq!(escape_table_cell(r"left\|right"), r"left\\\|right");
        assert_eq!(escape_table_cell(r"left\\|right"), r"left\\\\\|right");
        assert_eq!(escape_table_cell(r"C:\temp\file.txt"), r"C:\temp\file.txt");
        assert_eq!(
            escape_table_cell("first\nsecond\r\nthird\rfourth"),
            "first second third fourth"
        );
    }

    #[test]
    fn escape_table_cell_is_linear_on_long_backslash_runs() {
        let cell = format!("{}|", "\\".repeat(65_536));
        let out = escape_table_cell(&cell);
        assert_eq!(out.len(), 65_536 * 2 + 2);
    }

    #[test]
    fn table_uses_first_row_as_header_and_pads() {
        let rows = vec![
            vec!["name".to_string(), "age".to_string()],
            vec!["Alice".to_string(), "30".to_string()],
        ];
        assert_eq!(
            table(&rows),
            "| name | age |\n| --- | --- |\n| Alice | 30 |"
        );
        let ragged = vec![
            vec!["a".to_string()],
            vec!["b".to_string(), "c".to_string()],
        ];
        assert_eq!(table(&ragged), "| a |  |\n| --- | --- |\n| b | c |");
        assert_eq!(table(&[]), "");
    }

    #[test]
    fn escape_text_escapes_emphasis_markers_only() {
        assert_eq!(escape_text("snake_case *x* [y]"), r"snake\_case \*x\* [y]");
    }

    #[test]
    fn image_sanitizes_alt_and_handles_missing_target() {
        assert_eq!(
            image("A [big]\npicture", Some("doc_assets/image1.png")),
            "![A big picture](doc_assets/image1.png)"
        );
        assert_eq!(image("", None), "![]()");
    }

    #[test]
    fn link_target_encodes_unsafe_path_characters() {
        assert_eq!(
            link_target("rapor (2)_assets/image 1.png"),
            "rapor%20%282%29_assets/image%201.png"
        );
        assert_eq!(link_target("çizim<1>.png"), "çizim%3C1%3E.png");
    }
}
