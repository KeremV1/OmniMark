# Contributing to OmniMark

Thanks for your interest! Bug reports and focused pull requests are welcome.

## Building

Prerequisites and build commands are in the [README](README.md). In short:
Rust (stable, MSVC toolchain on Windows), Node.js, then `npm ci` and `npm run tauri dev`.
pdfium is fetched with `scripts/fetch-pdfium.ps1` (Windows) or `scripts/fetch-pdfium.sh`.

## Before opening a pull request

Run all of these and make sure they pass:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test
npm run typecheck
```

## Pull request expectations

- One focused change per PR, with a clear description of the problem it solves.
- Add or update tests. Conversion behaviour is covered by golden tests in `tests/golden/`.
- Keep the app light: no new heavy dependencies without discussion (low RAM and small installer are goals).
- UI strings go through `src/locales/*`; add keys to every locale (English is the source).
- Code, comments and commit messages are in English.

## MarkItDown: no code copying

OmniMark's behaviour follows Microsoft's [MarkItDown](https://github.com/microsoft/markitdown)
(MIT), but **no MarkItDown code may be copied or translated line by line**. Behaviour is
re-implemented in Rust from observed input/output. Test files under `tests/fixtures/markitdown/`
are the only copied material and keep their original license notice.

## License

By contributing you agree that your contribution is licensed under the [MIT License](LICENSE).
