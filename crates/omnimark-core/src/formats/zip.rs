//! ZIP archives: every supported entry is converted with the normal converters.
//!
//! MarkItDown's `ZipConverter` joins the entries into one Markdown text:
//! `Content from the zip file `<name>`:`, then `## File: <path>` and the entry's Markdown for each
//! one, in archive order (duplicate names included). [`ZipConverter`] reproduces that;
//! [`for_each_entry`] is the same walk for callers (the app's folder and zip outputs) that want
//! the entries one by one.
//!
//! Rules, beyond what MarkItDown does:
//! - entries the converters do not support are skipped with one summary warning (MarkItDown skips
//!   them silently);
//! - a nested `.zip` is not opened (depth 1) and is reported;
//! - names with `..`, an absolute path or a drive letter are rejected and reported;
//! - folder entries are ignored, encrypted entries are skipped and reported;
//! - limits: at most [`crate::archive::MAX_ENTRIES`] entries, [`MAX_ENTRY_SIZE`] per entry and
//!   [`crate::archive::MAX_TOTAL_UNCOMPRESSED`] in total; going over the total stops the walk.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;

use crate::archive::Archive;
use crate::assets::{AssetSink, MemoryAssetSink, NullAssetSink};
use crate::detect;
use crate::document::Document;
use crate::error::{ConvertError, Result};
use crate::info::{FileInfo, Input};
use crate::registry::{ConvertContext, Converter, Priority, Registry};

/// Largest single entry that is decompressed and converted (64 MiB). Smaller than the generic
/// archive limit because a whole entry is held in memory and then parsed again by its converter;
/// bigger documents are skipped with a warning, not failed.
pub const MAX_ENTRY_SIZE: u64 = 64 * 1024 * 1024;

/// How many names a skip warning lists before "and N more".
const WARNING_NAMES: usize = 3;

/// Where the images of one entry go.
pub enum EntryAssets {
    /// The context's own sink (single combined output).
    Shared,
    /// Kept in memory and handed to [`EntryHandler::handle`]; links in the Markdown start with
    /// this folder name (`<name>_assets`).
    Folder(String),
}

/// Receives the entries of an archive, in order.
pub trait EntryHandler {
    /// Called before an entry is converted.
    fn assets(&mut self, inner_path: &str) -> EntryAssets;

    /// Called with the conversion result of every entry that was attempted and is not simply
    /// unsupported. `images` is empty unless [`EntryHandler::assets`] chose
    /// [`EntryAssets::Folder`].
    fn handle(
        &mut self,
        inner_path: &str,
        result: Result<Document>,
        images: Vec<(String, Vec<u8>)>,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZipSummary {
    /// Entries that were converted.
    pub converted: usize,
    /// Entries that were not: unsupported, unsafe, nested, encrypted, too big or failed.
    pub skipped: usize,
    /// One message per kind of problem.
    pub warnings: Vec<String>,
}

#[derive(Default)]
struct Skips {
    unsupported: Vec<String>,
    nested: Vec<String>,
    unsafe_names: Vec<String>,
    encrypted: Vec<String>,
    too_big: Vec<String>,
    failed: Vec<String>,
}

fn list(names: &[String]) -> String {
    let shown = names
        .iter()
        .take(WARNING_NAMES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    match names.len().saturating_sub(WARNING_NAMES) {
        0 => shown,
        more => format!("{shown} and {more} more"),
    }
}

impl Skips {
    fn count(&self) -> usize {
        self.unsupported.len()
            + self.nested.len()
            + self.unsafe_names.len()
            + self.encrypted.len()
            + self.too_big.len()
            + self.failed.len()
    }

    fn warnings(&self) -> Vec<String> {
        let kinds: [(&Vec<String>, &str); 6] = [
            (
                &self.unsupported,
                "{n} files in the archive are of an unsupported format and were skipped",
            ),
            (
                &self.nested,
                "{n} nested archives were not opened and were skipped",
            ),
            (
                &self.unsafe_names,
                "{n} files with an unsafe path were skipped",
            ),
            (&self.encrypted, "{n} password-protected files were skipped"),
            (
                &self.too_big,
                "{n} files are too large to convert and were skipped",
            ),
            (
                &self.failed,
                "{n} files could not be converted and were skipped",
            ),
        ];
        kinds
            .iter()
            .filter(|(names, _)| !names.is_empty())
            .map(|(names, template)| {
                let message = template.replace("{n}", &names.len().to_string());
                format!("{message}: {}.", list(names))
            })
            .collect()
    }
}

/// `Ok(clean path with `/` separators)` or the reason the name is rejected.
fn safe_entry_path(name: &str) -> std::result::Result<String, ()> {
    let name = name.replace('\\', "/");
    let mut chars = name.chars();
    let drive_letter = matches!(
        (chars.next(), chars.next()),
        (Some(c), Some(':')) if c.is_ascii_alphabetic()
    );
    if name.starts_with('/') || drive_letter || name.contains('\0') {
        return Err(());
    }
    let mut parts = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(()),
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(());
    }
    Ok(parts.join("/"))
}

fn convert_entry(
    registry: &Registry,
    inner_path: &str,
    bytes: Vec<u8>,
    ctx: &mut ConvertContext<'_>,
) -> Result<Document> {
    let input = Input {
        path: PathBuf::from(inner_path),
        bytes,
    };
    match catch_unwind(AssertUnwindSafe(|| registry.convert(&input, ctx))) {
        Ok(result) => result,
        Err(_) => Err(ConvertError::Corrupt("internal error".into())),
    }
}

/// Converts every supported entry of `bytes` with `registry` and passes it to `handler`.
/// Cancellation (checked before each entry) and handler errors end the walk with an error.
pub fn for_each_entry(
    bytes: &[u8],
    registry: &Registry,
    ctx: &mut ConvertContext<'_>,
    handler: &mut dyn EntryHandler,
) -> Result<ZipSummary> {
    walk(
        Archive::open(bytes)?.with_entry_limit(MAX_ENTRY_SIZE),
        registry,
        ctx,
        handler,
    )
}

fn walk(
    mut archive: Archive<'_>,
    registry: &Registry,
    ctx: &mut ConvertContext<'_>,
    handler: &mut dyn EntryHandler,
) -> Result<ZipSummary> {
    let mut skips = Skips::default();
    let mut converted = 0;
    let mut stopped = false;
    let total = archive.len().max(1);
    for index in 0..archive.len() {
        ctx.check_cancel()?;
        ctx.report(index as f32 / total as f32);
        let info = archive.entry_info(index)?;
        if info.is_dir {
            continue;
        }
        let Ok(path) = safe_entry_path(&info.name) else {
            skips.unsafe_names.push(info.name);
            continue;
        };
        if info.encrypted {
            skips.encrypted.push(path);
            continue;
        }
        if path.to_ascii_lowercase().ends_with(".zip") {
            skips.nested.push(path);
            continue;
        }
        if info.size > archive.budget_left() && info.size <= archive.entry_limit() {
            stopped = true;
            break;
        }
        let data = match archive.read_index(index) {
            Ok(data) => data,
            Err(ConvertError::Corrupt(message)) if message.contains("limit") => {
                skips.too_big.push(path);
                continue;
            }
            Err(error) => {
                handler.handle(&path, Err(error), Vec::new())?;
                skips.failed.push(path);
                continue;
            }
        };
        if detect::sniff(&data).is_some_and(|i| i.mime.as_deref() == Some("application/zip")) {
            skips.nested.push(path);
            continue;
        }
        let mut memory = None;
        let mut null = NullAssetSink;
        let assets = handler.assets(&path);
        let result = {
            let sink: &mut dyn AssetSink = match (&assets, ctx.options.extract_images) {
                (EntryAssets::Shared, _) => &mut *ctx.assets,
                (EntryAssets::Folder(_), false) => &mut null,
                (EntryAssets::Folder(folder), true) => memory.insert(MemoryAssetSink::new(folder)),
            };
            let quiet = |_: f32| {};
            let mut inner = ConvertContext {
                options: ctx.options,
                assets: sink,
                cancel: ctx.cancel,
                progress: &quiet,
            };
            convert_entry(registry, &path, data, &mut inner)
        };
        match result {
            Err(ConvertError::Cancelled) => return Err(ConvertError::Cancelled),
            Err(ConvertError::Unsupported) => skips.unsupported.push(path),
            result => {
                let images = memory.map(|m| m.files).unwrap_or_default();
                let ok = result.is_ok();
                handler.handle(&path, result, images)?;
                if ok {
                    converted += 1;
                } else {
                    skips.failed.push(path);
                }
            }
        }
    }
    ctx.report(1.0);
    let mut warnings = skips.warnings();
    if stopped {
        warnings.push(
            "The archive is larger than the 1 GB limit; the remaining files were skipped.".into(),
        );
    }
    Ok(ZipSummary {
        converted,
        skipped: skips.count(),
        warnings,
    })
}

/// Collects the single combined Markdown.
struct Combined {
    markdown: String,
    warnings: Vec<String>,
}

impl EntryHandler for Combined {
    fn assets(&mut self, _inner_path: &str) -> EntryAssets {
        EntryAssets::Shared
    }

    fn handle(
        &mut self,
        inner_path: &str,
        result: Result<Document>,
        _images: Vec<(String, Vec<u8>)>,
    ) -> Result<()> {
        // A failed entry is skipped (MarkItDown does the same); the walk counts and reports it.
        if let Ok(document) = result {
            self.markdown
                .push_str(&format!("\n## File: {inner_path}\n\n"));
            self.markdown.push_str(&document.markdown);
            self.markdown.push_str("\n\n");
            self.warnings.extend(
                document
                    .warnings
                    .into_iter()
                    .map(|w| format!("{inner_path}: {w}")),
            );
        }
        Ok(())
    }
}

/// `.zip` files as one combined Markdown document.
pub struct ZipConverter {
    entries: Arc<Registry>,
}

impl ZipConverter {
    /// `entries` converts the files inside the archive; it should not contain a `ZipConverter`.
    pub fn new(entries: Arc<Registry>) -> Self {
        Self { entries }
    }
}

impl Converter for ZipConverter {
    fn name(&self) -> &'static str {
        "zip"
    }

    /// After every specific format: a `.docx` is a zip too, but never gets here first.
    fn priority(&self) -> Priority {
        Priority::Generic
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["zip"]) || info.mime.as_deref() == Some("application/zip")
    }

    fn convert(&self, input: &Input, ctx: &mut ConvertContext<'_>) -> Result<Document> {
        // An OOXML container (a Word file renamed to .zip, say) is for the other converters.
        if is_ooxml_container(&input.bytes) {
            return Err(ConvertError::Unsupported);
        }
        let name = input
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "(unknown)".to_string());
        convert_combined(&input.bytes, &name, &self.entries, ctx).map(|(document, _)| document)
    }
}

/// True for docx/xlsx/pptx-style containers (they hold `[Content_Types].xml`).
pub fn is_ooxml_container(bytes: &[u8]) -> bool {
    Archive::open(bytes).is_ok_and(|archive| archive.contains("[Content_Types].xml"))
}

/// MarkItDown's single combined Markdown for the archive `bytes`, named `name` in its header.
pub fn convert_combined(
    bytes: &[u8],
    name: &str,
    registry: &Registry,
    ctx: &mut ConvertContext<'_>,
) -> Result<(Document, ZipSummary)> {
    let archive = Archive::open(bytes)?.with_entry_limit(MAX_ENTRY_SIZE);
    let mut combined = Combined {
        markdown: format!(
            "Content from the zip file `{name}`:

"
        ),
        warnings: Vec::new(),
    };
    let mut summary = walk(archive, registry, ctx, &mut combined)?;
    summary.warnings.extend(combined.warnings);
    let mut document = Document::new(combined.markdown.trim().to_string());
    document.warnings = summary.warnings.clone();
    Ok((document, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::MemoryAssetSink;
    use crate::cancel::CancelToken;
    use crate::formats::{default_registry, RegistryConfig};
    use crate::ocr::{MockEngine, OcrEngine, MOCK_TEXT};
    use crate::ConvertOptions;
    use std::io::Write;
    use std::path::Path;
    use std::sync::Mutex;

    fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut buf);
        for (name, data) in entries {
            if name.ends_with('/') {
                writer
                    .add_directory(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
            } else {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(data).unwrap();
            }
        }
        writer.finish().unwrap();
        buf.into_inner()
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/markitdown/files")
                .join(name),
        )
        .unwrap()
    }

    fn registry() -> Registry {
        default_registry(&RegistryConfig::default())
    }

    fn convert_with(
        zip_bytes: &[u8],
        file_name: &str,
        options: &ConvertOptions,
        cancel: &CancelToken,
    ) -> Result<Document> {
        let mut assets = MemoryAssetSink::new("x_assets");
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options,
            assets: &mut assets,
            cancel,
            progress: &progress,
        };
        registry().convert(
            &Input {
                path: PathBuf::from(file_name),
                bytes: zip_bytes.to_vec(),
            },
            &mut ctx,
        )
    }

    fn convert_zip(entries: &[(&str, &[u8])]) -> Document {
        convert_with(
            &build(entries),
            "pack.zip",
            &ConvertOptions::default(),
            &CancelToken::new(),
        )
        .unwrap()
    }

    #[test]
    fn combined_format_matches_markitdown() {
        // MarkItDown's own test also has two entries with one name; the zip crate cannot read
        // such archives at all (it reports a corrupt file), so that case is not covered here.
        let doc = convert_zip(&[
            ("notes.txt", b"First archived entry."),
            ("more/notes.txt", b"Second archived entry."),
        ]);
        assert_eq!(
            doc.markdown,
            "Content from the zip file `pack.zip`:\n\n\
             ## File: notes.txt\n\nFirst archived entry.\n\n\
             ## File: more/notes.txt\n\nSecond archived entry."
        );
        assert!(doc.warnings.is_empty());
    }

    #[test]
    fn missing_file_name_is_unknown() {
        let bytes = build(&[("a.txt", b"hello")]);
        let doc = convert_with(
            &bytes,
            "noname.zip",
            &ConvertOptions::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert!(doc
            .markdown
            .starts_with("Content from the zip file `noname.zip`:"));
        // No usable file name at all.
        let mut assets = MemoryAssetSink::new("x");
        let cancel = CancelToken::new();
        let options = ConvertOptions::default();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        let doc = ZipConverter::new(Arc::new(registry()))
            .convert(
                &Input {
                    path: PathBuf::new(),
                    bytes,
                },
                &mut ctx,
            )
            .unwrap();
        assert!(doc
            .markdown
            .starts_with("Content from the zip file `(unknown)`:\n\n"));
    }

    #[test]
    fn markitdown_test_archive_has_every_office_member() {
        let bytes = fixture("test_files.zip");
        let doc = convert_with(
            &bytes,
            "test_files.zip",
            &ConvertOptions::default(),
            &CancelToken::new(),
        )
        .unwrap();
        let text = doc.markdown;
        assert!(text.starts_with("Content from the zip file `test_files.zip`:\n\n"));
        for heading in ["test.docx", "test.xlsx", "test.pptx", "test_blog.html"] {
            assert!(text.contains(&format!("## File: {heading}\n")), "{heading}");
        }
        assert!(!text.contains("[Content_Types].xml"));
        assert!(!text.contains("word/document.xml"));
        // The Word member is converted, not unpacked.
        assert!(text.contains(
            "## File: test.jpg

ImageSize: 1615x1967"
        ));
    }

    #[test]
    fn office_files_win_over_the_zip_converter() {
        let docx = fixture("test.docx");
        let own = convert_with(
            &docx,
            "test.docx",
            &ConvertOptions::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert!(!own.markdown.starts_with("Content from the zip file"));
        // A Word file with a .zip name is still read as Word.
        let renamed = convert_with(
            &docx,
            "test.zip",
            &ConvertOptions::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(renamed.markdown, own.markdown);
    }

    #[test]
    fn unsupported_entries_are_skipped_with_one_warning() {
        let doc = convert_zip(&[
            ("ok.txt", b"fine"),
            ("a.bin", &[0, 1, 2]),
            ("b.bin", &[0, 1, 2]),
            ("c.bin", &[0, 1, 2]),
            ("d.bin", &[0, 1, 2]),
            ("e.bin", &[0, 1, 2]),
        ]);
        assert!(doc.markdown.contains("## File: ok.txt"));
        assert!(!doc.markdown.contains("a.bin"));
        assert_eq!(doc.warnings.len(), 1);
        assert_eq!(
            doc.warnings[0],
            "5 files in the archive are of an unsupported format and were skipped: \
             a.bin, b.bin, c.bin and 2 more."
        );
    }

    #[test]
    fn nested_archives_are_not_opened() {
        let inner = build(&[("deep.txt", b"deep secret")]);
        let doc = convert_zip(&[
            ("inner.zip", &inner),
            ("hidden.dat", &inner),
            ("top.txt", b"top"),
        ]);
        assert!(doc.markdown.contains("top"));
        assert!(!doc.markdown.contains("deep"));
        assert!(!doc.markdown.contains("## File: inner.zip"));
        assert_eq!(doc.warnings.len(), 1);
        assert!(doc.warnings[0].starts_with("2 nested archives were not opened"));
        assert!(doc.warnings[0].contains("inner.zip"));
    }

    #[test]
    fn unsafe_paths_are_rejected() {
        for bad in [
            "../evil.txt",
            "a/../../evil.txt",
            "/abs.txt",
            "\\abs.txt",
            "C:\\x.txt",
            "d:/x.txt",
            "a\\..\\..\\x.txt",
        ] {
            let doc = convert_zip(&[(bad, b"bad"), ("good.txt", b"good")]);
            assert!(!doc.markdown.contains("bad"), "{bad}");
            assert!(doc.markdown.contains("good"));
            assert_eq!(doc.warnings.len(), 1, "{bad}");
            assert!(
                doc.warnings[0].starts_with("1 files with an unsafe path were skipped"),
                "{bad}"
            );
        }
        assert_eq!(safe_entry_path("a//b/./c.txt").unwrap(), "a/b/c.txt");
        assert_eq!(safe_entry_path("a\\b.txt").unwrap(), "a/b.txt");
        assert!(safe_entry_path("").is_err());
        assert!(safe_entry_path("./").is_err());
    }

    #[test]
    fn directory_entries_are_ignored_silently() {
        let doc = convert_zip(&[("docs/", b""), ("docs/a.txt", b"inside")]);
        assert!(doc.warnings.is_empty());
        assert!(doc.markdown.contains("## File: docs/a.txt"));
        assert!(!doc.markdown.contains("## File: docs/\n"));
    }

    #[test]
    fn encrypted_entries_are_skipped_with_a_warning() {
        use zip::unstable::write::FileOptionsExt;
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut buf);
        writer
            .start_file(
                "secret.txt",
                zip::write::SimpleFileOptions::default()
                    .with_deprecated_encryption(b"pw")
                    .unwrap(),
            )
            .unwrap();
        writer.write_all(b"classified").unwrap();
        writer
            .start_file("open.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"public").unwrap();
        writer.finish().unwrap();
        let doc = convert_with(
            &buf.into_inner(),
            "p.zip",
            &ConvertOptions::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert!(doc.markdown.contains("public"));
        assert!(!doc.markdown.contains("classified"));
        assert_eq!(doc.warnings.len(), 1);
        assert!(doc.warnings[0].starts_with("1 password-protected files were skipped"));
    }

    #[test]
    fn entry_size_limit_skips_the_entry() {
        let bytes = build(&[("big.txt", &[b'a'; 64]), ("small.txt", b"tiny")]);
        let archive = Archive::open(&bytes).unwrap().with_entry_limit(32);
        let mut handler = Combined {
            markdown: String::new(),
            warnings: Vec::new(),
        };
        let (cancel, options) = (CancelToken::new(), ConvertOptions::default());
        let mut assets = MemoryAssetSink::new("x");
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        let summary = walk(archive, &registry(), &mut ctx, &mut handler).unwrap();
        assert_eq!((summary.converted, summary.skipped), (1, 1));
        assert!(summary.warnings[0].starts_with("1 files are too large to convert"));
        assert!(handler.markdown.contains("tiny"));
    }

    #[test]
    fn total_budget_stops_the_walk() {
        let bytes = build(&[
            ("a.txt", &[b'a'; 64]),
            ("b.txt", &[b'b'; 64]),
            ("c.txt", &[b'c'; 64]),
        ]);
        let archive = Archive::open(&bytes).unwrap().with_budget(100);
        let mut handler = Combined {
            markdown: String::new(),
            warnings: Vec::new(),
        };
        let (cancel, options) = (CancelToken::new(), ConvertOptions::default());
        let mut assets = MemoryAssetSink::new("x");
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        let summary = walk(archive, &registry(), &mut ctx, &mut handler).unwrap();
        assert_eq!(summary.converted, 1);
        assert!(summary
            .warnings
            .iter()
            .any(|w| w.contains("larger than the 1 GB limit")));
    }

    #[test]
    fn too_many_entries_is_corrupt() {
        let names: Vec<String> = (0..crate::archive::MAX_ENTRIES + 1)
            .map(|i| format!("{i}.txt"))
            .collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        let err = convert_with(
            &build(&entries),
            "many.zip",
            &ConvertOptions::default(),
            &CancelToken::new(),
        )
        .unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Corrupt);
    }

    #[test]
    fn broken_archives_are_corrupt() {
        let err = convert_with(
            b"PK\x03\x04 not really",
            "bad.zip",
            &ConvertOptions::default(),
            &CancelToken::new(),
        )
        .unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Corrupt);
    }

    struct Recorder<'a> {
        paths: Vec<String>,
        cancel_after_first: Option<&'a CancelToken>,
    }

    impl EntryHandler for Recorder<'_> {
        fn assets(&mut self, _: &str) -> EntryAssets {
            EntryAssets::Folder("x_assets".into())
        }
        fn handle(
            &mut self,
            path: &str,
            result: Result<Document>,
            _: Vec<(String, Vec<u8>)>,
        ) -> Result<()> {
            assert!(result.is_ok());
            self.paths.push(path.to_string());
            if let Some(token) = self.cancel_after_first {
                token.cancel();
            }
            Ok(())
        }
    }

    fn run_walk(
        bytes: &[u8],
        options: &ConvertOptions,
        cancel: &CancelToken,
        progress: &(dyn Fn(f32) + Sync),
        handler: &mut dyn EntryHandler,
    ) -> Result<ZipSummary> {
        let mut assets = MemoryAssetSink::new("unused");
        let mut ctx = ConvertContext {
            options,
            assets: &mut assets,
            cancel,
            progress,
        };
        for_each_entry(bytes, &registry(), &mut ctx, handler)
    }

    #[test]
    fn cancellation_is_checked_between_entries() {
        let bytes = build(&[("a.txt", b"1"), ("b.txt", b"2"), ("c.txt", b"3")]);
        let cancel = CancelToken::new();
        let mut handler = Recorder {
            paths: Vec::new(),
            cancel_after_first: Some(&cancel),
        };
        let err = run_walk(
            &bytes,
            &ConvertOptions::default(),
            &cancel,
            &|_| {},
            &mut handler,
        )
        .unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Cancelled);
        assert_eq!(handler.paths, ["a.txt"]);
    }

    #[test]
    fn progress_is_reported_per_entry() {
        let bytes = build(&[
            ("a.txt", b"1"),
            ("b.txt", b"2"),
            ("c.txt", b"3"),
            ("d.txt", b"4"),
        ]);
        let seen = Mutex::new(Vec::new());
        let progress = |f: f32| seen.lock().unwrap().push(f);
        let mut handler = Recorder {
            paths: Vec::new(),
            cancel_after_first: None,
        };
        let summary = run_walk(
            &bytes,
            &ConvertOptions::default(),
            &CancelToken::new(),
            &progress,
            &mut handler,
        )
        .unwrap();
        assert_eq!(summary.converted, 4);
        let seen = seen.into_inner().unwrap();
        assert_eq!(seen, [0.0, 0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn images_inside_entries_are_read_with_ocr() {
        let png = {
            let mut out = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(64, 64)
                .write_to(&mut out, image::ImageFormat::Png)
                .unwrap();
            out.into_inner()
        };
        let engine = Arc::new(MockEngine::default());
        let options = ConvertOptions {
            ocr: Some(engine.clone() as Arc<dyn OcrEngine>),
            ..ConvertOptions::default()
        };
        let doc = convert_with(
            &build(&[("scans/page.png", &png), ("a.txt", b"plain")]),
            "ocr.zip",
            &options,
            &CancelToken::new(),
        )
        .unwrap();
        assert!(doc.markdown.contains("## File: scans/page.png"));
        assert!(doc.markdown.contains(MOCK_TEXT));
        assert_eq!(engine.calls(), 1);
        // Without OCR the same image yields no text.
        let plain = convert_with(
            &build(&[("scans/page.png", &png)]),
            "ocr.zip",
            &ConvertOptions::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert!(!plain.markdown.contains(MOCK_TEXT));
    }
}
