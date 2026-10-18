use crate::detect::decode_text;
use crate::error::Result;
use crate::markdown::table;
use crate::registry::{ConvertContext, Converter};
use crate::{Document, FileInfo, Input};

pub struct CsvConverter;

impl Converter for CsvConverter {
    fn name(&self) -> &'static str {
        "csv"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["csv"]) || info.mime_starts_with(&["text/csv", "application/csv"])
    }

    fn convert(&self, input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        let decoded = decode_text(&input.bytes);
        let text = decoded.trim_start_matches('\u{feff}');
        let mut rows = parse_csv(text, sniff_delimiter(text));
        trim_outer_blank_rows(&mut rows);
        Ok(Document::new(table(&rows)))
    }
}

/// Python `csv.reader` semantics: quotes with `""` escapes, CR / LF / CRLF record ends,
/// a completely empty line is an empty row.
pub(crate) fn parse_csv(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut row_started = false;
    let mut chars = text.chars().peekable();

    let end_record = |row: &mut Vec<String>,
                      field: &mut String,
                      row_started: &mut bool,
                      rows: &mut Vec<Vec<String>>| {
        if *row_started {
            row.push(std::mem::take(field));
            rows.push(std::mem::take(row));
        } else {
            rows.push(Vec::new());
        }
        *row_started = false;
    };

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' if field.is_empty() => {
                in_quotes = true;
                row_started = true;
            }
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                end_record(&mut row, &mut field, &mut row_started, &mut rows);
            }
            c if c == delimiter => {
                row.push(std::mem::take(&mut field));
                row_started = true;
            }
            c => {
                field.push(c);
                row_started = true;
            }
        }
    }
    if row_started {
        end_record(&mut row, &mut field, &mut row_started, &mut rows);
    }
    rows
}

/// Picks `,`, `;` or tab: the delimiter whose most common field count (>1) covers the most of the
/// first 50 non-empty rows. On a tie between `,` and `;`, `;` wins when any `;`-separated field is a
/// decimal-comma number (`1,5`) — Turkish/European Excel exports. Other ties keep the earlier candidate.
pub(crate) fn sniff_delimiter(text: &str) -> char {
    let sample: String = text.lines().take(50).collect::<Vec<_>>().join("\n");
    let score_of = |candidate: char| -> usize {
        let mut counts = std::collections::HashMap::new();
        for row in parse_csv(&sample, candidate)
            .iter()
            .filter(|r| !r.is_empty())
        {
            *counts.entry(row.len()).or_insert(0usize) += 1;
        }
        counts
            .iter()
            .filter(|(len, _)| **len > 1)
            .map(|(_, n)| *n)
            .max()
            .unwrap_or(0)
    };
    let (comma, semicolon, tab) = (score_of(','), score_of(';'), score_of('\t'));
    let decimal_comma = parse_csv(&sample, ';')
        .iter()
        .flatten()
        .any(|field| is_decimal_comma(field));
    if semicolon > comma || (semicolon == comma && semicolon > 0 && decimal_comma) {
        if tab > semicolon {
            '\t'
        } else {
            ';'
        }
    } else if tab > comma {
        '\t'
    } else {
        ','
    }
}

/// `-12,5` style numbers.
fn is_decimal_comma(field: &str) -> bool {
    let digits = field.trim().trim_start_matches('-');
    match digits.split_once(',') {
        Some((int, frac)) => {
            !int.is_empty()
                && !frac.is_empty()
                && int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// Removes empty rows before the header, right after it, and at the end (MarkItDown `_trim_outer_blank_rows`).
pub(crate) fn trim_outer_blank_rows(rows: &mut Vec<Vec<String>>) {
    let Some(header) = rows.iter().position(|r| !r.is_empty()) else {
        rows.clear();
        return;
    };
    rows.drain(..header);
    let after_header = rows[1..].iter().take_while(|r| r.is_empty()).count();
    rows.drain(1..1 + after_header);
    while rows.last().is_some_and(Vec::is_empty) {
        rows.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::NullAssetSink;
    use crate::{CancelToken, ConvertOptions};
    use std::path::PathBuf;

    fn md(bytes: &[u8]) -> String {
        let options = ConvertOptions::default();
        let mut assets = NullAssetSink;
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        CsvConverter
            .convert(
                &Input {
                    path: PathBuf::from("t.csv"),
                    bytes: bytes.to_vec(),
                },
                &mut ctx,
            )
            .unwrap()
            .markdown
    }

    #[test]
    fn basic_table_for_every_line_ending() {
        for input in [
            "name,age\nAlice,30",
            "name,age\r\nAlice,30\r\n",
            "name,age\rAlice,30\r",
        ] {
            assert_eq!(
                md(input.as_bytes()),
                "| name | age |\n| --- | --- |\n| Alice | 30 |",
                "{input:?}"
            );
        }
    }

    #[test]
    fn quoted_newlines_become_spaces() {
        assert_eq!(
            md(b"name,notes\nAlice,\"first\nsecond\""),
            "| name | notes |\n| --- | --- |\n| Alice | first second |"
        );
    }

    #[test]
    fn blank_rows_are_trimmed_at_the_edges_but_kept_in_the_middle() {
        let input = format!(
            "{}name,value\n{}Alice,1\n\nBob,2,extra\n{}",
            "\n".repeat(1000),
            "\n".repeat(1000),
            "\n".repeat(1000)
        );
        assert_eq!(
            md(input.as_bytes()),
            "| name | value |  |\n| --- | --- | --- |\n| Alice | 1 |  |\n|  |  |  |\n| Bob | 2 | extra |"
        );
    }

    #[test]
    fn banner_row_is_the_header_and_rows_are_padded() {
        assert_eq!(
            md(b"banner\nname,age,city\nAlice,30,Seattle"),
            "| banner |  |  |\n| --- | --- | --- |\n| name | age | city |\n| Alice | 30 | Seattle |"
        );
    }

    #[test]
    fn empty_and_bom_inputs() {
        assert_eq!(md(b"\n\n\n"), "");
        assert!(md(b"\xef\xbb\xbfname,age\nAlice,30\n").starts_with("| name | age |"));
    }

    #[test]
    fn semicolon_and_tab_delimiters_are_detected() {
        assert_eq!(
            md("Ad;Soyad;Şehir\nAyşe;Yılmaz;İzmir\nAli;\"Kaya;Demir\";Ankara".as_bytes()),
            "| Ad | Soyad | Şehir |\n| --- | --- | --- |\n| Ayşe | Yılmaz | İzmir |\n| Ali | Kaya;Demir | Ankara |"
        );
        assert_eq!(md(b"a\tb\n1\t2"), "| a | b |\n| --- | --- |\n| 1 | 2 |");
        assert_eq!(
            md(b"1,5;2\n3,5;4"),
            "| 1,5 | 2 |\n| --- | --- |\n| 3,5 | 4 |"
        );
    }

    #[test]
    fn quoted_empty_field_is_not_a_blank_row() {
        assert_eq!(
            parse_csv("a\n\"\"\n\nb", ','),
            vec![
                vec!["a".to_string()],
                vec![String::new()],
                vec![],
                vec!["b".to_string()]
            ]
        );
    }
}
