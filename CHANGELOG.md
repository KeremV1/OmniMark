# Changelog

All notable changes to OmniMark are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2026-11-01

### Fixed

- macOS: PDF conversion failed on every file with "A required component is missing". macOS refused
  to load the bundled PDF engine (pdfium) because the library was unsigned. The library is now
  signed, so PDFs convert on both Intel and Apple silicon Macs. ([#1])

### Changed

- When a required component exists but cannot be loaded, the error now says so instead of
  reporting it as missing.
- The release build checks the macOS code signatures before the packages are published.

## [0.2.0] - 2026-10-21

First public beta.

### Added

- Converters for PDF, Word (`.docx`), Excel (`.xlsx`, `.xls`, `.ods`), PowerPoint (`.pptx`),
  HTML, CSV, text and JSON.
- ZIP archives: every supported file inside, as a folder, one combined `.md` or a `.zip`.
- Images: EXIF/XMP metadata and optional OCR (built-in Windows OCR, or ocrs on macOS and Linux).
- OCR for scanned PDF pages and for pictures inside Office files.
- Drag and drop queue, parallel conversion, in-app preview, light and dark themes.
- Interface in English, Türkçe, Русский, Deutsch, Español, Français and Português (Brasil).
- Automatic update check, which can be turned off in Settings.

[Unreleased]: https://github.com/KeremV1/OmniMark/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/KeremV1/OmniMark/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/KeremV1/OmniMark/releases/tag/v0.2.0
[#1]: https://github.com/KeremV1/OmniMark/issues/1
