use crate::detect::decode_text;
use crate::error::Result;
use crate::markdown::{table, MAX_TABLE_CELLS, MAX_TABLE_COLUMNS, TABLE_WARNING};
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
        let (mut rows, parse_truncated) = parse_csv(text, sniff_delimiter(text));
        trim_outer_blank_rows(&mut rows);
        let (markdown, table_truncated) = table(&rows);
        let mut doc = Document::new(markdown);
        if parse_truncated || table_truncated {
            doc.warn_once(TABLE_WARNING);
        }
        Ok(doc)
    }
}

/// Python `csv.reader` semantics: quotes with `""` escapes, CR / LF / CRLF record ends,
/// a completely empty line is an empty row.
///
/// The grid is bounded while parsing: fields past [`MAX_TABLE_COLUMNS`] in a row are dropped,
/// and parsing stops once [`MAX_TABLE_CELLS`] cells (an empty row counts as one) are stored.
/// Leading empty rows are not stored (the table trims them anyway). The flag is `true` when
/// anything was dropped.
pub(crate) fn parse_csv(text: &str, delimiter: char) -> (Vec<Vec<String>>, bool) {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut row_started = false;
    let mut cells = 0usize;
    let mut truncated = false;
    let mut chars = text.chars().peekable();

    let push_field = |row: &mut Vec<String>, field: &mut String, truncated: &mut bool| {
        if row.len() < MAX_TABLE_COLUMNS {
            row.push(std::mem::take(field));
        } else {
            field.clear();
            *truncated = true;
        }
    };
    let end_record = |row: &mut Vec<String>,
                      field: &mut String,
                      row_started: &mut bool,
                      rows: &mut Vec<Vec<String>>,
                      cells: &mut usize,
                      truncated: &mut bool| {
        if *row_started {
            push_field(row, field, truncated);
            *cells += row.len();
            rows.push(std::mem::take(row));
        } else if !rows.is_empty() {
            *cells += 1;
            rows.push(Vec::new());
        }
        *row_started = false;
    };

    while let Some(ch) = chars.next() {
        if cells >= MAX_TABLE_CELLS {
            truncated = true;
            row_started = false;
            break;
        }
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
                end_record(
                    &mut row,
                    &mut field,
                    &mut row_started,
                    &mut rows,
                    &mut cells,
                    &mut truncated,
                );
            }
            c if c == delimiter => {
                push_field(&mut row, &mut field, &mut truncated);
                row_started = true;
            }
            c => {
                field.push(c);
                row_started = true;
            }
        }
    }
    if row_started {
        end_record(
            &mut row,
            &mut field,
            &mut row_started,
            &mut rows,
            &mut cells,
            &mut truncated,
        );
    }
    (rows, truncated)
}

/// Picks `,`, `;` or tab: the delimiter whose most common field count (>1) covers the most of the
/// first 50 non-empty rows. On a tie between `,` and `;`, `;` wins when any `;`-separated field is a
/// decimal-comma number (`1,5`) — Turkish/European Excel exports. Other ties keep the earlier candidate.
pub(crate) fn sniff_delimiter(text: &str) -> char {
    let sample: String = text.lines().take(50).collect::<Vec<_>>().join("\n");
    let score_of = |candidate: char| -> usize {
        let mut counts = std::collections::HashMap::new();
        for row in parse_csv(&sample, candidate)
            .0
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
        .0
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
        convert(bytes).markdown
    }

    fn convert(bytes: &[u8]) -> Document {
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
    }

    #[test]
    fn wide_header_row_is_capped_with_one_warning() {
        let input = format!("{}\n{}", ",".repeat(5_000), "x\n".repeat(5_000));
        let doc = convert(input.as_bytes());
        let header = doc.markdown.lines().next().unwrap();
        assert_eq!(header.matches('|').count(), MAX_TABLE_COLUMNS + 1);
        assert!(doc.markdown.len() < 5_000_000, "{}", doc.markdown.len());
        assert_eq!(doc.warnings, vec![TABLE_WARNING.to_string()]);
    }

    #[test]
    fn parser_caps_fields_per_row_and_total_cells() {
        let (rows, truncated) = parse_csv(&",".repeat(50_000), ',');
        assert!(truncated);
        assert_eq!(rows[0].len(), MAX_TABLE_COLUMNS);
        let (rows, truncated) = parse_csv(&"a\n".repeat(MAX_TABLE_CELLS + 10), ',');
        assert!(truncated);
        assert_eq!(rows.len(), MAX_TABLE_CELLS);
        let (rows, truncated) = parse_csv("a,b\n1,2", ',');
        assert!(!truncated);
        assert_eq!(rows.len(), 2);
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
            parse_csv("a\n\"\"\n\nb", ',').0,
            vec![
                vec!["a".to_string()],
                vec![String::new()],
                vec![],
                vec!["b".to_string()]
            ]
        );
    }
}
