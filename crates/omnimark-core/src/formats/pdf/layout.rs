//! Pdfium-independent text layout: glyphs to words, words to lines, headings and lists.

/// One character with its box in top-left-origin page coordinates (points).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Glyph {
    pub ch: char,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub size: f32,
    pub bold: bool,
    /// Synthetic character inserted by the text engine (space, newline).
    pub generated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Word {
    pub text: String,
    pub x0: f32,
    pub x1: f32,
    pub top: f32,
    pub bottom: f32,
    /// Largest glyph size in the word.
    pub size: f32,
    /// True when most glyphs are bold (kept for later heading heuristics).
    #[allow(dead_code)]
    pub bold: bool,
}

/// Maximum vertical distance between glyph centres (or word tops) on one line.
const LINE_TOLERANCE: f32 = 3.0;
/// Maximum horizontal gap between glyphs of the same word.
const WORD_GAP: f32 = 3.0;
/// A line is a heading when its largest size is at least this multiple of the body size.
const HEADING_RATIO: f32 = 1.25;
/// Headings are shorter than this many characters.
const HEADING_MAX_CHARS: usize = 120;
/// Heading levels (`#`..`###`).
const MAX_HEADING_LEVELS: usize = 3;
/// A gap larger than this multiple of the median line pitch ends a paragraph.
const PARAGRAPH_GAP_RATIO: f32 = 1.5;
/// Sizes closer than this are the same size.
const SIZE_EPSILON: f32 = 0.5;
const BULLETS: [char; 5] = ['•', '◦', '▪', '–', '-'];

/// Groups glyphs (in reading order) into words.
pub(crate) fn group_words(glyphs: &[Glyph]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut current: Vec<&Glyph> = Vec::new();
    for glyph in glyphs {
        if glyph.generated || glyph.ch.is_whitespace() || glyph.ch.is_control() {
            flush_word(&mut current, &mut words);
            continue;
        }
        if let Some(last) = current.last() {
            let same_line = (center_y(glyph) - center_y(last)).abs() <= LINE_TOLERANCE;
            let close = glyph.left - last.right <= WORD_GAP;
            if !(same_line && close) {
                flush_word(&mut current, &mut words);
            }
        }
        current.push(glyph);
    }
    flush_word(&mut current, &mut words);
    words
}

fn center_y(g: &Glyph) -> f32 {
    (g.top + g.bottom) / 2.0
}

fn flush_word(current: &mut Vec<&Glyph>, words: &mut Vec<Word>) {
    if !current.is_empty() {
        words.push(build_word(current));
        current.clear();
    }
}

fn build_word(glyphs: &[&Glyph]) -> Word {
    let bold = glyphs.iter().filter(|g| g.bold).count() * 2 > glyphs.len();
    Word {
        text: glyphs.iter().map(|g| g.ch).collect(),
        x0: glyphs.iter().map(|g| g.left).fold(f32::MAX, f32::min),
        x1: glyphs.iter().map(|g| g.right).fold(f32::MIN, f32::max),
        top: glyphs.iter().map(|g| g.top).fold(f32::MAX, f32::min),
        bottom: glyphs.iter().map(|g| g.bottom).fold(f32::MIN, f32::max),
        size: glyphs.iter().map(|g| g.size).fold(0.0, f32::max),
        bold,
    }
}

/// Groups glyphs into phrases like pdfplumber's `extract_words(keep_blank_chars=True)`:
/// real blanks stay inside the phrase, only a horizontal gap above 3 pt (or a new line)
/// starts a new one. Synthetic blanks are ignored, so the gap rule decides. Used for forms,
/// where a cell such as "Customer Information" must stay in one piece.
pub(crate) fn group_phrases(glyphs: &[Glyph]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut current: Vec<&Glyph> = Vec::new();
    for glyph in glyphs {
        if glyph.generated || glyph.ch.is_control() {
            continue;
        }
        if let Some(last) = current.last() {
            let same_line = (center_y(glyph) - center_y(last)).abs() <= LINE_TOLERANCE;
            let close = glyph.left - last.right <= WORD_GAP;
            if !(same_line && close) {
                flush_phrase(&mut current, &mut words);
            }
        }
        if glyph.ch.is_whitespace() && current.is_empty() {
            continue;
        }
        current.push(glyph);
    }
    flush_phrase(&mut current, &mut words);
    words
}

fn flush_phrase(current: &mut Vec<&Glyph>, words: &mut Vec<Word>) {
    while current.last().is_some_and(|g| g.ch.is_whitespace()) {
        current.pop();
    }
    // Runs of blanks collapse to one space.
    let mut kept: Vec<&Glyph> = Vec::new();
    for g in current.iter() {
        if g.ch.is_whitespace() && kept.last().is_some_and(|k| k.ch.is_whitespace()) {
            continue;
        }
        kept.push(g);
    }
    if !kept.is_empty() {
        words.push(build_word(&kept));
    }
    current.clear();
}

/// Rounds `top / 5` half-to-even (Python `round`), as MarkItDown does for form rows.
pub(crate) fn y_key(top: f32) -> f32 {
    (f64::from(top) / 5.0).round_ties_even() as f32 * 5.0
}

#[derive(Debug, Clone)]
pub(crate) struct Line {
    pub text: String,
    pub top: f32,
    pub size: f32,
}

/// Groups words into lines (top within 3 pt of the line's first word), words ordered by `x0`.
pub(crate) fn lines_from_words(words: &[Word]) -> Vec<Line> {
    let mut sorted: Vec<&Word> = words.iter().collect();
    sorted.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.x0.total_cmp(&b.x0)));
    let mut groups: Vec<Vec<&Word>> = Vec::new();
    let mut anchor = f32::NAN;
    for word in sorted {
        match groups.last_mut() {
            Some(group) if (word.top - anchor).abs() <= LINE_TOLERANCE => group.push(word),
            _ => {
                anchor = word.top;
                groups.push(vec![word]);
            }
        }
    }
    groups
        .into_iter()
        .map(|mut group| {
            group.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            Line {
                text: group
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                top: group[0].top,
                size: group.iter().map(|w| w.size).fold(0.0, f32::max),
            }
        })
        .collect()
}

/// Median glyph size weighted by character count; `10.0` for an empty document.
pub(crate) fn body_size(words: &[Word]) -> f32 {
    let mut by_size: Vec<(f32, usize)> = words
        .iter()
        .map(|w| (w.size, w.text.chars().count()))
        .collect();
    by_size.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: usize = by_size.iter().map(|(_, n)| n).sum();
    let mut seen = 0;
    for (size, n) in by_size {
        seen += n;
        if seen * 2 >= total {
            return size;
        }
    }
    10.0
}

fn is_heading_line(line: &Line, body: f32) -> bool {
    line.size >= HEADING_RATIO * body && line.text.chars().count() < HEADING_MAX_CHARS
}

/// Distinct heading sizes (largest first, at most three) among the lines of `words`.
pub(crate) fn heading_sizes(words: &[Word], body: f32) -> Vec<f32> {
    let sizes = lines_from_words(words)
        .iter()
        .filter(|l| is_heading_line(l, body))
        .map(|l| l.size)
        .collect();
    finalize_heading_sizes(sizes)
}

/// Sorts descending, merges near-equal sizes and keeps the three largest.
pub(crate) fn finalize_heading_sizes(mut sizes: Vec<f32>) -> Vec<f32> {
    sizes.sort_by(|a, b| b.total_cmp(a));
    let mut out: Vec<f32> = Vec::new();
    for size in sizes {
        if out
            .last()
            .is_none_or(|last| (last - size).abs() > SIZE_EPSILON)
        {
            out.push(size);
        }
    }
    out.truncate(MAX_HEADING_LEVELS);
    out
}

fn list_item(text: &str) -> Option<&str> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if !BULLETS.contains(&first) {
        return None;
    }
    let rest = chars.as_str().strip_prefix(' ')?.trim_start();
    (!rest.is_empty()).then_some(rest)
}

fn median(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    Some(values[values.len() / 2])
}

/// Renders the words of one page as Markdown text (headings, lists, paragraphs).
pub(crate) fn render_text_page(words: &[Word], body: f32, heading_sizes: &[f32]) -> String {
    let lines = lines_from_words(words);
    let mut pitches: Vec<f32> = lines.windows(2).map(|w| w[1].top - w[0].top).collect();
    let paragraph_gap = median(&mut pitches).map(|m| m * PARAGRAPH_GAP_RATIO);

    let mut out: Vec<String> = Vec::new();
    let blank = |out: &mut Vec<String>| {
        if out.last().is_some_and(|l| !l.is_empty()) {
            out.push(String::new());
        }
    };
    let mut prev: Option<&Line> = None;
    let mut prev_heading_level: Option<usize> = None;
    let mut prev_was_list = false;

    for line in &lines {
        let level = if is_heading_line(line, body) {
            heading_sizes
                .iter()
                .position(|s| (s - line.size).abs() <= SIZE_EPSILON)
        } else {
            None
        };
        if let Some(level) = level {
            if prev_heading_level == Some(level) {
                // Consecutive lines of the same heading size form one heading.
                let at = out.len().saturating_sub(2);
                if let Some(heading) = out.get_mut(at) {
                    heading.push(' ');
                    heading.push_str(&line.text);
                }
            } else {
                blank(&mut out);
                out.push(format!("{} {}", "#".repeat(level + 1), line.text));
                out.push(String::new());
            }
            prev_heading_level = Some(level);
            prev_was_list = false;
            prev = Some(line);
            continue;
        }
        if let (Some(p), Some(gap)) = (prev, paragraph_gap) {
            if prev_heading_level.is_none() && line.top - p.top > gap {
                blank(&mut out);
            }
        }
        match list_item(&line.text) {
            Some(rest) => {
                if !prev_was_list {
                    blank(&mut out);
                }
                out.push(format!("- {rest}"));
                prev_was_list = true;
            }
            None => {
                out.push(line.text.clone());
                prev_was_list = false;
            }
        }
        prev_heading_level = None;
        prev = Some(line);
    }
    out.join("\n").trim().to_string()
}

/// Joins MasterFormat numbering fragments (`.1`) with the next non-empty line.
pub(crate) fn merge_partial_numbering(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut result: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let stripped = lines[i].trim();
        if is_partial_numbering(stripped) {
            if let Some(j) = (i + 1..lines.len()).find(|&j| !lines[j].trim().is_empty()) {
                result.push(format!("{stripped} {}", lines[j].trim()));
                i = j + 1;
                continue;
            }
        }
        result.push(lines[i].to_string());
        i += 1;
    }
    result.join("\n")
}

/// `^\.\d+$`: a lone MasterFormat number such as `.1` or `.10`.
pub(crate) fn is_partial_numbering(s: &str) -> bool {
    s.strip_prefix('.')
        .is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, x0: f32, top: f32, size: f32) -> Word {
        Word {
            text: text.into(),
            x0,
            x1: x0 + text.len() as f32 * size * 0.5,
            top,
            bottom: top + size,
            size,
            bold: false,
        }
    }

    fn glyph(ch: char, left: f32, top: f32) -> Glyph {
        Glyph {
            ch,
            left,
            right: left + 5.0,
            top,
            bottom: top + 10.0,
            size: 10.0,
            bold: false,
            generated: false,
        }
    }

    #[test]
    fn y_key_uses_bankers_rounding() {
        assert_eq!(y_key(12.5), 10.0); // 2.5 -> 2 (ties to even) -> 10
        assert_eq!(y_key(17.5), 20.0); // 3.5 -> 4 -> 20
    }

    #[test]
    fn headings_lists_and_paragraphs() {
        let words = vec![
            word("Report", 72.0, 72.0, 20.0),
            word("Intro", 72.0, 110.0, 14.0),
            word("Body", 72.0, 140.0, 10.0),
            word("text", 100.0, 140.0, 10.0),
            word("more", 72.0, 152.0, 10.0),
            word("•", 72.0, 190.0, 10.0),
            word("item", 82.0, 190.0, 10.0),
        ];
        let body = body_size(&words);
        let headings = heading_sizes(&words, body);
        let md = render_text_page(&words, body, &headings);
        assert_eq!(
            md.trim(),
            "# Report\n\n## Intro\n\nBody text\nmore\n\n- item"
        );
    }

    #[test]
    fn consecutive_heading_lines_of_one_size_merge() {
        let words = vec![
            word("Long", 72.0, 72.0, 20.0),
            word("title", 72.0, 94.0, 20.0),
            word("Body", 72.0, 130.0, 10.0),
        ];
        let body = 10.0;
        let md = render_text_page(&words, body, &heading_sizes(&words, body));
        assert_eq!(md, "# Long title\n\nBody");
    }

    #[test]
    fn partial_numbering_is_merged_with_the_next_line() {
        assert_eq!(
            merge_partial_numbering(".1\n\nThe intent\n.2 Already\n.3"),
            ".1 The intent\n.2 Already\n.3"
        );
    }

    #[test]
    fn partial_numbering_keeps_trailing_fragment_untouched() {
        assert_eq!(merge_partial_numbering("x\n.5\n\n"), "x\n.5\n\n");
    }

    #[test]
    fn glyphs_split_into_words_on_spaces_gaps_and_line_changes() {
        let mut space = glyph(' ', 15.0, 100.0);
        space.generated = true;
        let glyphs = vec![
            glyph('a', 0.0, 100.0),
            glyph('b', 5.0, 100.0),
            space,
            glyph('c', 20.0, 100.0),
            glyph('d', 40.0, 100.0), // 15 pt gap
            glyph('e', 45.0, 130.0), // next line
        ];
        let words = group_words(&glyphs);
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["ab", "c", "d", "e"]);
        assert_eq!((words[0].x0, words[0].x1), (0.0, 10.0));
    }

    #[test]
    fn phrases_keep_real_blanks_but_split_on_wide_gaps() {
        let mut generated = glyph(' ', 10.0, 100.0);
        generated.generated = true;
        let glyphs = vec![
            glyph('a', 0.0, 100.0),
            glyph(' ', 5.0, 100.0), // real blank
            glyph('b', 10.0, 100.0),
            generated,
            glyph('c', 19.0, 100.0), // 4 pt after b: wider than 3 pt, new phrase
            glyph('d', 40.0, 100.0),
        ];
        let texts: Vec<String> = group_phrases(&glyphs).into_iter().map(|w| w.text).collect();
        assert_eq!(texts, ["a b", "c", "d"]);
    }

    #[test]
    fn body_size_is_weighted_by_characters() {
        let words = vec![
            word("aaaaaaaaaa", 0.0, 0.0, 10.0),
            word("BIG", 0.0, 20.0, 30.0),
        ];
        assert_eq!(body_size(&words), 10.0);
    }
}
