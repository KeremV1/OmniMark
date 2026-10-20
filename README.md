<p align="center">
  <img src="docs/media/hero-dark.png" alt="OmniMark: file queue with an open Markdown preview (dark theme)" width="100%">
</p>

<p align="center">
  <b>Turn any file into Markdown.</b><br>
  PDF, Word, Excel, PowerPoint, HTML, images and more — a light, fast, cross-platform desktop app.
</p>

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.90%2B-B7410E?style=flat-square">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square">
  <img alt="Tests" src="https://img.shields.io/badge/tests-277%20passing-15803D?style=flat-square">
  <img alt="Platforms" src="https://img.shields.io/badge/Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-5A5F6E?style=flat-square">
  <img alt="Languages" src="https://img.shields.io/badge/UI-7%20languages-0E7490?style=flat-square">
  <a href="LICENSE"><img alt="MIT" src="https://img.shields.io/badge/license-MIT-4F46E5?style=flat-square"></a>
</p>

---

# OmniMark

**OmniMark** converts documents, spreadsheets, slides, web pages and images into clean Markdown (`.md`) — locally, with no cloud service. It is built in Rust with a Tauri 2 shell, so it starts fast, stays small (≈7 MB installer) and idles at about 71 MB of RAM.

> **Status:** ✅ M1 (core converters + desktop app) and ✅ M2 (image metadata + OCR) are complete. M2 closed with a simplification round: a single automatic OCR engine per platform and a multilingual interface. Next: **M3 — audio transcription**.

## Highlights

- **Many formats, one drop zone:** drag files or folders in, convert them in parallel, preview the result.
- **Faithful output:** headings, lists, tables, links, formulas (LaTeX), slide notes, chart data, sheet-by-sheet tables. Behaviour follows Microsoft [MarkItDown](https://github.com/microsoft/markitdown) and is checked with golden tests.
- **Optional OCR:** reads text in images, scanned PDF pages and pictures inside Office files. Off by default, one switch to turn it on.
- **Private by design:** everything runs on your machine. Nothing is uploaded.
- **Light:** ≈7 MB installer, ≈71 MB idle memory (target < 100 MB), no bundled browser.
- **7 interface languages:** English, Türkçe, Русский, Deutsch, Español, Français, Português (Brasil). Follows the system language, falls back to English, can be changed in Settings.

## Supported formats

| Format | Extensions | Output |
| --- | --- | --- |
| PDF | `.pdf` | Text, tables where possible; optional OCR for embedded images and scanned pages |
| Word | `.docx` | Headings, lists, tables, links, equations (LaTeX); optional OCR for images |
| Excel | `.xlsx`, `.xlsm`, `.xls`, `.ods` | One heading and Markdown table per sheet; optional OCR for `.xlsx` images |
| PowerPoint | `.pptx` | Per-slide titles, text, tables, chart data, speaker notes; optional OCR for images |
| HTML | `.html`, `.htm` | Body content; scripts and styles removed |
| Images | `.png`, `.jpg`, `.jpeg`, `.webp`, `.gif`, `.bmp`, `.tif`, `.tiff` | EXIF/XMP metadata, optional OCR |
| Text | `.txt`, `.md`, `.markdown`, `.json`, `.jsonl`, `.csv` | Plain text; JSON as a code block; CSV as a Markdown table |
| Audio (M3) | `.wav`, `.mp3`, `.m4a` … | Coming in M3: metadata and optional speech-to-text |

## OCR

OCR is an **optional module** and is off by default. When you turn it on (**Settings → OCR**), OmniMark picks the engine for you:

| Platform | Engine | Download |
| --- | --- | --- |
| Windows 10/11 | Built-in Windows OCR — fast (≈0.7 s for a 3-page scan), uses the OCR languages installed in Windows | None |
| macOS, Linux (and Windows without OCR languages) | [ocrs](https://github.com/robertknight/ocrs), pure Rust — Latin-script text | ≈12 MB model, downloaded on first use |

Recognized text appears in the output between `*[Image OCR]` … `[End OCR]*` markers. The command-line tool can force an engine with `--ocr=windows` or `--ocr=ocrs`.

## How to use

1. Open OmniMark.
2. Optionally turn on **Settings → OCR**.
3. Drop files or folders onto the window, or use **Add files** / **Add folder**.
4. Press **Create Markdown**. Progress and errors are shown per file; ✕ on a row or **Cancel** stops the run.
5. Markdown files are written to the output folder you chose. Open the folder or preview the result in the app.

## Architecture

```
OmniMark
├── Rust core            file detection, converters, Markdown helpers
│   ├── one converter per format (pdf, docx, xlsx, pptx, html, image …)
│   └── OCR layer        engine interface: Windows OCR · ocrs; image limits, cache
├── CLI                  omnimark convert / models fetch
├── Tauri 2 shell        window, drag & drop, dialogs, OS integration
└── UI                   HTML/CSS/TypeScript; queue, progress, preview, 7 locales (lazy-loaded)
```

Each converter first says whether it accepts a file (extension, MIME type, content signature), then turns it into Markdown. A final pass trims trailing spaces and collapses repeated blank lines.

## Roadmap

| Milestone | Scope | Status |
| --- | --- | --- |
| **M1** | App shell, drag & drop queue, conversion flow, preview; PDF, Word, Excel, PowerPoint, HTML, CSV, TXT/JSON | ✅ Done (19/19 tasks) |
| **M2** | Image metadata (EXIF/XMP) and OCR; closing round: simplified OCR + multilingual UI | ✅ Done (9/9 tasks + closing round) |
| **M3** | Audio transcription (local Whisper, optional module) | ⏭ Next |
| **M4** | Hardening: error handling, more tests, performance | ⏳ Planned |
| **M5** | Signed installers for Windows, macOS and Linux | ⏳ Planned |

## Platform support

| Platform | Build | CI | Installer | Status |
| --- | --- | --- | --- | --- |
| **Windows** 10/11 (x64) | ✅ | ✅ GitHub Actions | 🔄 NSIS `.exe` (≈7 MB) built locally; signed release in M5 | ✅ Tested by hand |
| **macOS** (Apple Silicon, Intel) | ✅ in CI | ✅ GitHub Actions | ⏳ M5 · `.dmg` | 🔄 CI only |
| **Linux** (x64, Ubuntu 22.04+) | ✅ in CI | ✅ GitHub Actions | ⏳ M5 · AppImage, `.deb` | 🔄 CI only |

## Development

Requirements:

- Rust 1.90+ (MSVC toolchain on Windows) and, on Windows, Visual Studio 2022 Build Tools (C++) and WebView2
- Node.js 20+
- pdfium, fetched **before building**: `scripts/fetch-pdfium.ps1` (Windows) or `scripts/fetch-pdfium.sh` (macOS/Linux). Pinned to `chromium/8066`, SHA-256 verified ([bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)).

```powershell
scripts/fetch-pdfium.ps1                                   # pdfium (macOS/Linux: bash scripts/fetch-pdfium.sh)
npm ci                                                     # UI dependencies
npm run tauri dev                                          # desktop app, dev mode
npm run tauri build -- --bundles nsis                      # Windows installer
cargo test --workspace                                     # Rust tests (incl. golden tests)
npm test                                                   # UI tests
cargo run -p omnimark-cli -- convert file.pdf -o out       # command line
cargo run -p omnimark-cli -- convert photo.jpg --stdout --ocr
cargo run -p omnimark-cli -- models fetch ocrs             # ocrs model (≈12 MB)
```

If the project path contains `&`, npm/Tauri break: map the parent folder with `subst O: "<parent>"`, run from `O:\OmniMark`, then `subst O: /D`.

Adding a UI language: copy `src/locales/en.ts` to `src/locales/<code>.ts`, translate the values (TypeScript reports missing keys), and register it in `src/i18n.ts` and the `Language` setting in `src-tauri/src/settings.rs`.

Research notes (Turkish): [Rust libraries](docs/research/rust-kutuphaneleri.md) · [Tauri 2 guide](docs/research/tauri2-rehberi.md) · [MarkItDown behaviour spec](docs/research/markitdown-davranis-spec.md). Project notes and measurements in [docs/](docs/) and [HANDOFF.md](HANDOFF.md) are kept in Turkish.

A Turkish-first snapshot of the app at the end of M2 is tagged [`v0.1.0-tr`](https://github.com/KeremV1/OmniMark/releases/tag/v0.1.0-tr).

## Acknowledgements

Conversion behaviour is designed by studying Microsoft's [MarkItDown](https://github.com/microsoft/markitdown) (MIT). OmniMark contains no MarkItDown code; it is written from scratch in Rust. Test files and expected outputs live in `tests/fixtures/markitdown/` with their own MIT notice (Microsoft Corporation).

## License

[MIT](LICENSE) © 2026 MoNoR (KeremV1).
