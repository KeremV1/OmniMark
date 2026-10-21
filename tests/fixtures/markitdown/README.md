# MarkItDown test files

The files in this folder were copied from Microsoft [MarkItDown](https://github.com/microsoft/markitdown)
(version **0.1.8**) and are used by OmniMark's golden tests. MarkItDown is MIT licensed; the license text
is in `LICENSE-MARKITDOWN`.

| Folder | Contents |
| --- | --- |
| `files/` | All sample files from `packages/markitdown/tests/test_files/` (except the generator `.py` scripts) |
| `files/expected_outputs/` | Expected Markdown that MarkItDown 0.1.8 produces for its PDF fixtures (`packages/markitdown/tests/test_files/expected_outputs/`), used for golden content checks |
| `ocr/` | Files under 100 KB from `packages/markitdown-ocr/tests/ocr_test_data/`, plus `pdf_scanned_minimal.pdf` |
| `reference/tests/` | MarkItDown's tests (including `_test_vectors.py`): the expected `must_include` / `must_not_include` strings |
| `reference/ocr-tests/` | Tests of the OCR plugin |

The Python files under `reference/` are **not run**; they are only the source of the expected outputs.
Where OmniMark deliberately differs from MarkItDown is listed in [`tests/golden/overrides.md`](../../golden/overrides.md).
