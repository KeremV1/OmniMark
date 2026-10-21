# Deliberate differences from MarkItDown

The golden vectors come from the MarkItDown 0.1.8 tests (`tests/fixtures/markitdown/reference/`). This file lists
the places where OmniMark intentionally behaves differently, with the reason.

## Converters

| Fixture | MarkItDown expects | OmniMark expects | Reason |
| --- | --- | --- | --- |
| test.json, json_late_non_ascii.json | JSON text as is | Inside a ```` ```json ```` code block | Readability in Markdown viewers |
| (inline tests) | `;`-separated CSV is one column | `,` `;` and tab detected automatically | Excel in many locales saves CSV with `;` |
| test.xlsx, test.xls | Empty cell `NaN`, `Unnamed: N`, `100.0` | Empty cell empty, integers without `.0` | pandas artifacts |
| test_wikipedia.html | Only `#mw-content-text` (URL-dependent WikipediaConverter) | Whole body; sidebar strings are not checked | No site-specific converters |
| (inline tests) | `mailto:`/`tel:` links are dropped | Kept | Meaningful links for the reader |
| test.pptx | `![…](Picture4.jpg)` (a file that does not exist, always .jpg) | A real `test_assets/<name>` file | Images stay reachable |
| (inline tests) | No line break after a chart, `10.0` | Line break, cached value (`10`) | Text glued to the chart; pandas/Python float format |
| PDF fixtures | Exact equality with pdfminer/pdfplumber output | Content and structure checks; headings with `#`, bullets with `- ` | pdfium is a different text engine; MarkItDown produces no headings |
| PDF form pages | Table row directly next to a plain text line | Blank line before and after a table | An adjacent plain line must not become a table row |
| test.docx | Image truncated as `data:image/png;base64...` | Extracted to `test_assets/imageN.png` | Images are not lost |
| test_with_comment.docx | Comment texts (with a style_map) | No comments | Not supported yet |
| (inline tests) | First table row is an empty header | First row is the header | Readable GFM table |
| equations.docx | `$m=1$`, `$$…$$` | Same; `m:sty=p` wraps only runs with ASCII letters in `\mathrm{}`, invisible operators (U+2061) are dropped | Digit/operator runs are upright anyway; `$m=1$` must not break |

## Images

| Fixture | MarkItDown expects | OmniMark expects | Reason |
| --- | --- | --- | --- |
| (inline tests) | `Keywords: ['a', 'b']` (Python list format) | `Keywords: a, b` | Readability |
| test.jpg and all images | No OCR (OCR only inside Office/PDF) | With OCR on: metadata + blank line + `*[Image OCR]` block | Image files are a main OCR use case |
| (inline tests) | Needs a separate exiftool install, otherwise empty output | Built-in EXIF/XMP reader; HEIC, IPTC and XMP inside TIFF are not read | No external tool dependency |

## PDF OCR

| Fixture | markitdown-ocr expects | OmniMark expects | Reason |
| --- | --- | --- | --- |
| pdf_image_*, pdf_multipage | 3-4 blank lines around a block (raw output) | One blank line | Final normalization |
| pdf_complex_layout | `ItemQuantity` (words glued) | `Item Quantity` | pdfium keeps word spaces |
| (OCR on, all PDFs) | Built-in table/form and heading logic is lost | Kept; blocks are placed between lines | OCR must not degrade text pages |
| pdf_scanned_*, MEDRPT… | Embedded raster images are OCRed; a "scan" without images is not read | A page without a text layer is rendered at 300 DPI (long side ≤ 4096) and OCRed | Every scanned page is read |
| (OCR off) | — | Warning: `Page N has no text layer (scanned?); enable OCR to read it.` | Tell the user OCR exists |

## Office OCR

| Fixture | markitdown-ocr expects | OmniMark expects | Reason |
| --- | --- | --- | --- |
| docx_*, pptx_*, xlsx_* | `*[Image OCR]  \n…  \n[End OCR]*` (two-space line breaks) | `*[Image OCR]\` + line break (backslash hard break); `\ * _ [ ]` escaped | Final normalization strips trailing spaces |
| pptx_* | Block **replaces** the image link | `![alt](path)` kept, block below it | The image stays reachable |
| xlsx_* | `### Images in this sheet:` heading + blocks, `NaN` cells | Image line with the block below; no heading, empty cells empty | No pandas artifacts |
| docx (table cell) | HTML block inside the cell | One line `*[Image OCR] … [End OCR]*` | A GFM cell cannot contain a line break |

## PDF tables

| Fixture | MarkItDown expects | OmniMark expects | Reason |
| --- | --- | --- | --- |
| (real PDFs, e.g. lecture notes) | Any run of aligned lines becomes a table, whatever its column count; justified paragraphs and scattered formula letters often become tables with empty cells | A run of lines becomes a table only if it passes four rules, otherwise it stays text: `min_run_fill` 0.5 (filled cells / (rows × used columns)), `min_run_rows` 2, `max_prose_gap_ratio` 3.0 (a row whose widest word gap is ≤ 3 × font size is "prose-like"), `min_prose_row_share` 0.5 (rejected when ≥ half of the rows are prose-like) | Removes false-table noise |
| (real PDFs) | The page is written as a "form" with plain lines even when no table remains | If no run is accepted the page is not a form; the normal layout path (headings, paragraphs, OCR block placement) is used | Keeps heading/paragraph detection and OCR placement |
| (real PDFs) | — | No separate rule for long sparse runs: the fill threshold applies to runs of every length | Tried: dropping only runs of ≤ 3 rows produced 1790 table lines instead of 363 on lecture notes |
