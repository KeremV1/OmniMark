//! Port of MarkItDown's `_extract_form_content_from_words`: detects form-like pages
//! (aligned columns) and renders them as Markdown tables.

use std::collections::BTreeSet;

use super::layout::{is_partial_numbering, y_key, Word};

/// Tunable constants of the form heuristic (names follow the MarkItDown source).
struct FormHeuristics {
    /// A new x group starts when `x0` is further than this from the group's start.
    group_gap: f32,
    /// Minimum x groups for a line to propose columns.
    min_line_columns: usize,
    /// Line is a paragraph when wider than this fraction of the page ...
    paragraph_width_ratio: f32,
    /// ... and longer than this many characters.
    paragraph_min_chars: usize,
    /// Column-gap statistics ignore gaps up to this size.
    min_column_gap: f32,
    /// Percentile (of the sorted gaps) used as the adaptive tolerance.
    gap_percentile: f32,
    /// Fewer gaps than this fall back to `default_tolerance`.
    min_gaps_for_adaptive: usize,
    default_tolerance: f32,
    tolerance_range: (f32, f32),
    /// Reject the page when the average column width is below this (points).
    min_avg_column_width: f32,
    /// Reject the page when columns per inch exceed this.
    max_columns_per_inch: f32,
    /// Maximum number of columns at 612 pt page width (scaled with the width).
    max_columns_base: usize,
    /// A word aligns to a column when within this distance.
    align_tolerance: f32,
    /// A word belongs to the next column once it passes `column - this`.
    cell_margin: f32,
    /// Minimum share of table rows for the page to count as a form.
    min_table_ratio: f32,
    min_cell_width: usize,
}

const HEURISTICS: FormHeuristics = FormHeuristics {
    group_gap: 50.0,
    min_line_columns: 3,
    paragraph_width_ratio: 0.55,
    paragraph_min_chars: 60,
    min_column_gap: 5.0,
    gap_percentile: 0.70,
    min_gaps_for_adaptive: 3,
    default_tolerance: 35.0,
    tolerance_range: (25.0, 50.0),
    min_avg_column_width: 30.0,
    max_columns_per_inch: 10.0,
    max_columns_base: 15,
    align_tolerance: 40.0,
    cell_margin: 20.0,
    min_table_ratio: 0.2,
    min_cell_width: 3,
};

struct LineInfo<'a> {
    words: Vec<&'a Word>,
    text: String,
    x_groups: Vec<f32>,
    is_paragraph: bool,
    has_partial_numbering: bool,
}

/// Returns the page as table/plain Markdown, or `None` when it is not form-like.
pub(crate) fn extract_form_content(words: &[Word], page_width: f32) -> Option<String> {
    let h = &HEURISTICS;
    if words.is_empty() {
        return None;
    }
    let page_width = if page_width > 0.0 { page_width } else { 612.0 };
    let lines = build_lines(words, page_width);

    // Global column candidates from multi-column, non-paragraph lines.
    let mut positions: Vec<f32> = lines
        .iter()
        .filter(|l| l.x_groups.len() >= h.min_line_columns && !l.is_paragraph)
        .flat_map(|l| l.x_groups.iter().copied())
        .collect();
    if positions.is_empty() {
        return None;
    }
    positions.sort_by(f32::total_cmp);
    positions.dedup();

    let tolerance = adaptive_tolerance(&positions);
    let mut columns: Vec<f32> = vec![positions[0]];
    for &x in &positions[1..] {
        if x - columns[columns.len() - 1] > tolerance {
            columns.push(x);
        }
    }
    if columns.len() < 2 {
        return None;
    }
    let span = columns[columns.len() - 1] - columns[0];
    let count = columns.len() as f32;
    if span / count < h.min_avg_column_width {
        return None;
    }
    if count / (span / 72.0) > h.max_columns_per_inch {
        return None;
    }
    let max_columns = h.max_columns_base.max((20.0 * page_width / 612.0) as usize);
    if columns.len() > max_columns {
        return None;
    }

    // Classify rows.
    let is_table: Vec<bool> = lines
        .iter()
        .map(|line| {
            if line.is_paragraph || line.has_partial_numbering {
                return false;
            }
            let aligned: BTreeSet<usize> = line
                .words
                .iter()
                .filter_map(|w| {
                    columns
                        .iter()
                        .position(|c| (w.x0 - c).abs() < h.align_tolerance)
                })
                .collect();
            aligned.len() >= 2
        })
        .collect();
    let table_rows = is_table.iter().filter(|t| **t).count();
    if (table_rows as f32) / (lines.len() as f32) < h.min_table_ratio {
        return None;
    }

    // Format: runs of table rows become tables, the rest stays plain lines.
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !is_table[i] {
            out.push(lines[i].text.clone());
            i += 1;
            continue;
        }
        let mut j = i;
        while j < lines.len() && is_table[j] {
            j += 1;
        }
        if out.last().is_some_and(|l| !l.is_empty()) {
            out.push(String::new());
        }
        out.extend(format_table(&lines[i..j], &columns));
        if j < lines.len() {
            out.push(String::new());
        }
        i = j;
    }
    Some(out.join("\n"))
}

fn build_lines(words: &[Word], page_width: f32) -> Vec<LineInfo<'_>> {
    let h = &HEURISTICS;
    let mut sorted: Vec<&Word> = words.iter().collect();
    sorted.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let mut rows: Vec<(f32, Vec<&Word>)> = Vec::new();
    for word in sorted {
        let key = y_key(word.top);
        match rows.iter_mut().find(|(k, _)| *k == key) {
            Some((_, row)) => row.push(word),
            None => rows.push((key, vec![word])),
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));

    rows.into_iter()
        .map(|(_, row)| {
            let first_x0 = row[0].x0;
            let last_x1 = row.iter().map(|w| w.x1).fold(f32::MIN, f32::max);
            let text = row
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let mut x_groups: Vec<f32> = Vec::new();
            for w in &row {
                match x_groups.last() {
                    Some(start) if w.x0 - start <= h.group_gap => {}
                    _ => x_groups.push(w.x0),
                }
            }
            let is_paragraph = last_x1 - first_x0 > page_width * h.paragraph_width_ratio
                && text.chars().count() > h.paragraph_min_chars;
            let has_partial_numbering = is_partial_numbering(&row[0].text);
            LineInfo {
                words: row,
                text,
                x_groups,
                is_paragraph,
                has_partial_numbering,
            }
        })
        .collect()
}

fn adaptive_tolerance(positions: &[f32]) -> f32 {
    let h = &HEURISTICS;
    let mut gaps: Vec<f32> = positions
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|g| *g > h.min_column_gap)
        .collect();
    if gaps.len() < h.min_gaps_for_adaptive {
        return h.default_tolerance;
    }
    gaps.sort_by(f32::total_cmp);
    let index = ((gaps.len() as f32 * h.gap_percentile) as usize).min(gaps.len() - 1);
    gaps[index].clamp(h.tolerance_range.0, h.tolerance_range.1)
}

fn assign_cells(line: &LineInfo<'_>, columns: &[f32]) -> Vec<String> {
    let h = &HEURISTICS;
    let mut cells = vec![String::new(); columns.len()];
    for word in &line.words {
        let mut assigned = columns.len() - 1;
        for col in 0..columns.len() - 1 {
            if word.x0 < columns[col + 1] - h.cell_margin {
                assigned = col;
                break;
            }
        }
        let cell = &mut cells[assigned];
        if !cell.is_empty() {
            cell.push(' ');
        }
        cell.push_str(&word.text);
    }
    cells
}

fn format_table(rows: &[LineInfo<'_>], columns: &[f32]) -> Vec<String> {
    let cells: Vec<Vec<String>> = rows.iter().map(|r| assign_cells(r, columns)).collect();
    let widths: Vec<usize> = (0..columns.len())
        .map(|c| {
            cells
                .iter()
                .map(|row| row[c].chars().count())
                .max()
                .unwrap_or(0)
                .max(HEURISTICS.min_cell_width)
        })
        .collect();
    let render = |row: &Vec<String>| {
        let padded: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(cell, w)| format!("{cell:<w$}"))
            .collect();
        format!("| {} |", padded.join(" | "))
    };
    let separator: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    let mut out = vec![render(&cells[0]), format!("| {} |", separator.join(" | "))];
    out.extend(cells[1..].iter().map(render));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::pdf::layout::Word;

    fn w(text: &str, x0: f32, top: f32) -> Word {
        Word {
            text: text.into(),
            x0,
            x1: x0 + 30.0,
            top,
            bottom: top + 10.0,
            size: 10.0,
            bold: false,
        }
    }

    #[test]
    fn aligned_columns_become_a_table() {
        let mut words = Vec::new();
        for (row, top) in [100.0, 115.0, 130.0, 145.0].iter().enumerate() {
            for (col, x) in [72.0, 200.0, 330.0].iter().enumerate() {
                words.push(w(&format!("r{row}c{col}"), *x, *top));
            }
        }
        let md = extract_form_content(&words, 612.0).expect("form page");
        assert!(md.starts_with("| r0c0 | r0c1 | r0c2 |"), "{md}");
        assert!(md.contains("| ---- | ---- | ---- |"), "{md}");
    }

    #[test]
    fn prose_is_not_a_form() {
        let words: Vec<Word> = (0..10)
            .map(|i| Word {
                text: "a long line of ordinary prose text that spans the page width easily".into(),
                x0: 72.0,
                x1: 540.0,
                top: 100.0 + i as f32 * 14.0,
                bottom: 110.0 + i as f32 * 14.0,
                size: 10.0,
                bold: false,
            })
            .collect();
        assert_eq!(extract_form_content(&words, 612.0), None);
    }

    #[test]
    fn plain_lines_around_a_table_are_kept_and_separated() {
        let mut words = vec![w("Heading", 72.0, 60.0)];
        for top in [100.0, 115.0, 130.0] {
            for (col, x) in [72.0, 200.0, 330.0].iter().enumerate() {
                words.push(w(&format!("c{col}"), *x, top));
            }
        }
        words.push(w("Footer", 72.0, 180.0));
        let md = extract_form_content(&words, 612.0).expect("form page");
        let lines: Vec<&str> = md.lines().collect();
        assert_eq!(lines[0], "Heading");
        assert_eq!(lines[1], "");
        assert_eq!(lines.last(), Some(&"Footer"));
        assert_eq!(lines[lines.len() - 2], "");
    }

    #[test]
    fn numbered_rows_never_become_table_rows() {
        let mut words = Vec::new();
        for top in [100.0, 115.0, 130.0, 145.0] {
            for x in [72.0, 200.0, 330.0] {
                words.push(w(".1", x, top));
            }
        }
        // First word of every row is ".1": rows are plain text, so the table ratio is 0.
        assert_eq!(extract_form_content(&words, 612.0), None);
    }
}
