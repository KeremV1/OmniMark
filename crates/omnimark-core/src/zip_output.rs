//! Writing the result of a ZIP archive: one combined file, a folder tree or a zip of that tree.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::assets::{AssetSink, DirAssetSink, NullAssetSink};
use crate::cancel::CancelToken;
use crate::document::{ConvertOptions, Document};
use crate::error::{ConvertError, Result};
use crate::formats::zip::{
    convert_combined, for_each_entry, is_ooxml_container, EntryAssets, EntryHandler, ZipSummary,
};
use crate::info::Input;
use crate::markdown::normalize;
use crate::output::{sanitize_stem, write_atomic, OutputAllocator};
use crate::registry::{ConvertContext, Registry};

/// What a converted `.zip` becomes. Unknown values read as `Folder`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZipOutput {
    /// `<out>/<zip stem>.md`: MarkItDown's combined text.
    Single,
    /// `<out>/<zip stem>-md.zip`: the folder tree, packed.
    Zip,
    /// `<out>/<zip stem>/<inner path>.md`, keeping the archive's folders.
    #[default]
    #[serde(other)]
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipWritten {
    /// The `.md` file (`Single`), the folder (`Folder`) or the packed zip (`Zip`).
    pub output: PathBuf,
    pub converted: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

/// Reads `path` and writes its result, or `Ok(None)` when it is not a plain archive (an Office
/// file named `.zip`, say) and should go through the normal converters instead.
pub fn write_zip_file(
    mode: ZipOutput,
    registry: &Registry,
    path: &Path,
    allocator: &OutputAllocator,
    options: &ConvertOptions,
    cancel: &CancelToken,
    progress: &(dyn Fn(f32) + Sync),
) -> Result<Option<ZipWritten>> {
    cancel.check()?;
    let size = std::fs::metadata(path)?.len();
    if size > options.max_file_size {
        return Err(ConvertError::TooLarge {
            size,
            limit: options.max_file_size,
        });
    }
    let bytes = std::fs::read(path)?;
    if is_ooxml_container(&bytes) {
        return Ok(None);
    }
    let input = Input {
        path: path.to_path_buf(),
        bytes,
    };
    match catch_unwind(AssertUnwindSafe(|| {
        write_zip(mode, registry, &input, allocator, options, cancel, progress)
    })) {
        Ok(result) => result.map(Some),
        Err(_) => Err(ConvertError::Corrupt("internal error".into())),
    }
}

pub fn write_zip(
    mode: ZipOutput,
    registry: &Registry,
    input: &Input,
    allocator: &OutputAllocator,
    options: &ConvertOptions,
    cancel: &CancelToken,
    progress: &(dyn Fn(f32) + Sync),
) -> Result<ZipWritten> {
    match mode {
        ZipOutput::Single => write_single(registry, input, allocator, options, cancel, progress),
        ZipOutput::Folder | ZipOutput::Zip => {
            write_tree(mode, registry, input, allocator, options, cancel, progress)
        }
    }
}

fn nothing_to_convert() -> ConvertError {
    ConvertError::Unsupported
}

fn write_single(
    registry: &Registry,
    input: &Input,
    allocator: &OutputAllocator,
    options: &ConvertOptions,
    cancel: &CancelToken,
    progress: &(dyn Fn(f32) + Sync),
) -> Result<ZipWritten> {
    let target = allocator.allocate(&input.path);
    let mut dir_sink = DirAssetSink::new(target.assets_dir.clone());
    let mut null_sink = NullAssetSink;
    let sink: &mut dyn AssetSink = if options.extract_images {
        &mut dir_sink
    } else {
        &mut null_sink
    };
    let mut ctx = ConvertContext {
        options,
        assets: sink,
        cancel,
        progress,
    };
    let name = input
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "(unknown)".to_string());
    let result = convert_combined(&input.bytes, &name, registry, &mut ctx).and_then(
        |(document, summary)| {
            if summary.converted == 0 {
                return Err(nothing_to_convert());
            }
            write_atomic(&target.markdown_path, &normalize(&document.markdown))?;
            Ok(ZipWritten {
                output: target.markdown_path.clone(),
                converted: summary.converted,
                skipped: summary.skipped,
                warnings: document.warnings,
            })
        },
    );
    if result.is_err() {
        let _ = dir_sink.remove();
    }
    result
}

/// Where files of the tree go: a folder on disk or a zip being written.
enum Destination {
    Folder(PathBuf),
    Zip {
        writer: Box<ZipWriter<BufWriter<File>>>,
        temp: PathBuf,
        target: PathBuf,
    },
}

impl Destination {
    fn write(&mut self, relative: &str, bytes: &[u8]) -> Result<()> {
        match self {
            Self::Folder(root) => {
                let path = relative
                    .split('/')
                    .fold(root.clone(), |path, part| path.join(part));
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, bytes)?;
            }
            Self::Zip { writer, .. } => {
                writer
                    .start_file(relative, SimpleFileOptions::default())
                    .map_err(ConvertError::corrupt)?;
                writer.write_all(bytes)?;
            }
        }
        Ok(())
    }

    /// Completes the output and returns its final path.
    fn finish(self) -> Result<PathBuf> {
        match self {
            Self::Folder(root) => Ok(root),
            Self::Zip {
                writer,
                temp,
                target,
            } => {
                let mut inner = writer.finish().map_err(ConvertError::corrupt)?;
                inner.flush()?;
                drop(inner);
                std::fs::rename(&temp, &target)?;
                Ok(target)
            }
        }
    }

    /// Deletes a half-written result.
    fn discard(self) {
        match self {
            Self::Folder(root) => {
                let _ = std::fs::remove_dir_all(root);
            }
            Self::Zip { writer, temp, .. } => {
                drop(writer);
                let _ = std::fs::remove_file(temp);
            }
        }
    }
}

/// The `.md` name an entry gets inside the tree; every path part is made safe for every OS.
fn markdown_stem(file_part: &str) -> String {
    let stem = match file_part.rfind('.') {
        Some(i) if i > 0 => &file_part[..i],
        _ => file_part,
    };
    sanitize_stem(stem)
}

struct Tree {
    destination: Destination,
    used: HashSet<String>,
    /// Folder and name chosen by `assets` for the entry being converted.
    pending: Option<(String, String)>,
    warnings: Vec<String>,
}

impl Tree {
    /// A free `<dir>/<name>` for the entry; `name` gets ` (2)`, ... on case-insensitive clashes
    /// with another entry's `.md` or `_assets` folder.
    fn claim(&mut self, inner_path: &str) -> (String, String) {
        let mut parts: Vec<&str> = inner_path.split('/').collect();
        let file_part = parts.pop().unwrap_or_default();
        let dir: Vec<String> = parts.into_iter().map(sanitize_stem).collect();
        let dir = dir.join("/");
        let stem = markdown_stem(file_part);
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut n = 1u32;
        loop {
            let name = if n == 1 {
                stem.clone()
            } else {
                format!("{stem} ({n})")
            };
            let md_key = format!("{prefix}{name}.md").to_lowercase();
            let assets_key = format!("{prefix}{name}_assets").to_lowercase();
            if !self.used.contains(&md_key) && !self.used.contains(&assets_key) {
                self.used.insert(md_key);
                self.used.insert(assets_key);
                return (dir, name);
            }
            n += 1;
        }
    }
}

impl EntryHandler for Tree {
    fn assets(&mut self, inner_path: &str) -> EntryAssets {
        let (dir, name) = self.claim(inner_path);
        let folder = format!("{name}_assets");
        self.pending = Some((dir, name));
        EntryAssets::Folder(folder)
    }

    fn handle(
        &mut self,
        inner_path: &str,
        result: Result<Document>,
        images: Vec<(String, Vec<u8>)>,
    ) -> Result<()> {
        let Some((dir, name)) = self.pending.take() else {
            return Ok(());
        };
        let Ok(document) = result else {
            return Ok(());
        };
        let join = |file: String| {
            if dir.is_empty() {
                file
            } else {
                format!("{dir}/{file}")
            }
        };
        self.destination
            .write(&join(format!("{name}.md")), document.markdown.as_bytes())?;
        for (image, bytes) in images {
            self.destination
                .write(&join(format!("{name}_assets/{image}")), &bytes)?;
        }
        self.warnings.extend(
            document
                .warnings
                .into_iter()
                .map(|w| format!("{inner_path}: {w}")),
        );
        Ok(())
    }
}

fn write_tree(
    mode: ZipOutput,
    registry: &Registry,
    input: &Input,
    allocator: &OutputAllocator,
    options: &ConvertOptions,
    cancel: &CancelToken,
    progress: &(dyn Fn(f32) + Sync),
) -> Result<ZipWritten> {
    let destination = if mode == ZipOutput::Zip {
        let target = allocator.allocate_zip(&input.path);
        let temp = target.with_file_name(format!(
            ".{}.tmp",
            target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        if let Some(parent) = temp.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Destination::Zip {
            writer: Box::new(ZipWriter::new(BufWriter::new(File::create(&temp)?))),
            temp,
            target,
        }
    } else {
        let root = allocator.allocate_dir(&input.path);
        std::fs::create_dir_all(&root)?;
        Destination::Folder(root)
    };
    let mut tree = Tree {
        destination,
        used: HashSet::new(),
        pending: None,
        warnings: Vec::new(),
    };
    let mut assets = NullAssetSink;
    let mut ctx = ConvertContext {
        options,
        assets: &mut assets,
        cancel,
        progress,
    };
    let walked = for_each_entry(&input.bytes, registry, &mut ctx, &mut tree);
    let Tree {
        destination,
        warnings,
        ..
    } = tree;
    let summary: ZipSummary = match walked {
        Ok(summary) if summary.converted > 0 => summary,
        Ok(_) => {
            destination.discard();
            return Err(nothing_to_convert());
        }
        Err(error) => {
            destination.discard();
            return Err(error);
        }
    };
    let output = destination.finish()?;
    Ok(ZipWritten {
        output,
        converted: summary.converted,
        skipped: summary.skipped,
        warnings: summary.warnings.into_iter().chain(warnings).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{default_registry, RegistryConfig};
    use std::io::Read;

    fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(&mut buf);
        for (name, data) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        buf.into_inner()
    }

    fn run(
        mode: ZipOutput,
        out: &Path,
        zip_path: &Path,
        options: &ConvertOptions,
    ) -> Result<Option<ZipWritten>> {
        write_zip_file(
            mode,
            &default_registry(&RegistryConfig::default()),
            zip_path,
            &OutputAllocator::new(out),
            options,
            &CancelToken::new(),
            &|_| {},
        )
    }

    fn sample(dir: &Path) -> PathBuf {
        let path = dir.join("Pack.zip");
        std::fs::write(
            &path,
            build(&[
                ("top.txt", b"top level"),
                ("docs/a.csv", b"x,y\n1,2"),
                ("docs/sub/b:c.txt", b"colon"),
                ("docs/a.txt", b"same stem as a.csv"),
                ("skip.bin", &[0, 1, 2]),
            ]),
        )
        .unwrap();
        path
    }

    #[test]
    fn modes_parse_and_unknown_values_become_folder() {
        for (text, mode) in [
            ("folder", ZipOutput::Folder),
            ("single", ZipOutput::Single),
            ("zip", ZipOutput::Zip),
            ("other", ZipOutput::Folder),
        ] {
            let parsed: ZipOutput = serde_json::from_str(&format!("\"{text}\"")).unwrap();
            assert_eq!(parsed, mode);
        }
        assert_eq!(ZipOutput::default(), ZipOutput::Folder);
        assert_eq!(serde_json::to_string(&ZipOutput::Zip).unwrap(), "\"zip\"");
    }

    #[test]
    fn folder_mode_keeps_the_structure_with_safe_names_and_resolves_clashes() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = sample(tmp.path());
        let out = tmp.path().join("out");
        let written = run(
            ZipOutput::Folder,
            &out,
            &zip_path,
            &ConvertOptions::default(),
        )
        .unwrap()
        .unwrap();
        let root = out.join("Pack");
        assert_eq!(written.output, root);
        assert_eq!((written.converted, written.skipped), (4, 1));
        assert_eq!(
            std::fs::read_to_string(root.join("top.md")).unwrap(),
            "top level"
        );
        assert!(root.join("docs/a.md").is_file());
        assert!(root.join("docs/a (2).md").is_file());
        assert_eq!(
            std::fs::read_to_string(root.join("docs/sub/b_c.md")).unwrap(),
            "colon"
        );
        assert!(!root.join("skip.md").exists());
        // The archive itself is untouched and a second run never overwrites.
        let again = run(
            ZipOutput::Folder,
            &out,
            &zip_path,
            &ConvertOptions::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(again.output, out.join("Pack (2)"));
    }

    #[test]
    fn folder_mode_writes_images_next_to_their_markdown() {
        let tmp = tempfile::tempdir().unwrap();
        // A real Office file with pictures.
        let docx = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/markitdown/files/test.docx"),
        )
        .unwrap();
        let zip_path = tmp.path().join("office.zip");
        std::fs::write(&zip_path, build(&[("reports/Q1 report.docx", &docx)])).unwrap();
        let out = tmp.path().join("out");
        let written = run(
            ZipOutput::Folder,
            &out,
            &zip_path,
            &ConvertOptions::default(),
        )
        .unwrap()
        .unwrap();
        let md = written.output.join("reports/Q1 report.md");
        let text = std::fs::read_to_string(&md).unwrap();
        let assets = written.output.join("reports/Q1 report_assets");
        let link = text
            .split("](Q1%20report_assets/")
            .nth(1)
            .expect("the document links an image");
        let file = link.split(')').next().unwrap();
        assert!(assets.join(file).is_file(), "{file}");
        // Without image extraction no assets folder is written.
        let no_images = ConvertOptions {
            extract_images: false,
            ..ConvertOptions::default()
        };
        let out2 = tmp.path().join("out2");
        let written = run(ZipOutput::Folder, &out2, &zip_path, &no_images)
            .unwrap()
            .unwrap();
        assert!(!written.output.join("reports/Q1 report_assets").exists());
    }

    #[test]
    fn zip_mode_packs_the_folder_content() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = sample(tmp.path());
        let out = tmp.path().join("out");
        let written = run(ZipOutput::Zip, &out, &zip_path, &ConvertOptions::default())
            .unwrap()
            .unwrap();
        assert_eq!(written.output, out.join("Pack-md.zip"));
        assert_eq!((written.converted, written.skipped), (4, 1));
        let mut archive = zip::ZipArchive::new(File::open(&written.output).unwrap()).unwrap();
        let mut names: Vec<String> = archive
            .file_names()
            .filter_map(|n| n.ok().map(|n| n.into_owned()))
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["docs/a (2).md", "docs/a.md", "docs/sub/b_c.md", "top.md"]
        );
        let mut text = String::new();
        archive
            .by_name("top.md")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "top level");
        // No temp file is left, and a second run picks a new name.
        let leftovers: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, ["Pack-md.zip"]);
        let again = run(ZipOutput::Zip, &out, &zip_path, &ConvertOptions::default())
            .unwrap()
            .unwrap();
        assert_eq!(again.output, out.join("Pack-md (2).zip"));
    }

    #[test]
    fn single_mode_writes_the_combined_markdown() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = sample(tmp.path());
        let out = tmp.path().join("out");
        let written = run(
            ZipOutput::Single,
            &out,
            &zip_path,
            &ConvertOptions::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(written.output, out.join("Pack.md"));
        assert_eq!((written.converted, written.skipped), (4, 1));
        let text = std::fs::read_to_string(&written.output).unwrap();
        assert!(text.starts_with("Content from the zip file `Pack.zip`:\n\n## File: top.txt"));
        assert!(text.contains("## File: docs/sub/b:c.txt"));
        assert!(!written.warnings.is_empty());
        let again = run(
            ZipOutput::Single,
            &out,
            &zip_path,
            &ConvertOptions::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(again.output, out.join("Pack (2).md"));
    }

    #[test]
    fn an_archive_without_convertible_files_fails_and_leaves_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("junk.zip");
        std::fs::write(&zip_path, build(&[("a.bin", &[0, 1]), ("b.bin", &[0, 3])])).unwrap();
        for mode in [ZipOutput::Folder, ZipOutput::Single, ZipOutput::Zip] {
            let out = tmp.path().join(format!("out-{mode:?}"));
            let err = run(mode, &out, &zip_path, &ConvertOptions::default()).unwrap_err();
            assert_eq!(err.kind(), crate::ErrorKind::Unsupported, "{mode:?}");
            let left = std::fs::read_dir(&out).map(|d| d.count()).unwrap_or(0);
            assert_eq!(left, 0, "{mode:?}");
        }
    }

    #[test]
    fn cancelling_removes_the_partial_result() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = sample(tmp.path());
        for mode in [ZipOutput::Folder, ZipOutput::Zip] {
            let cancel = CancelToken::new();
            let token = cancel.clone();
            let first = std::sync::atomic::AtomicBool::new(true);
            // Cancel once the first entry has been reported.
            let progress = move |f: f32| {
                if f > 0.0 && first.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    token.cancel();
                }
            };
            let out = tmp.path().join(format!("o-{mode:?}"));
            let err = write_zip_file(
                mode,
                &default_registry(&RegistryConfig::default()),
                &zip_path,
                &OutputAllocator::new(&out),
                &ConvertOptions::default(),
                &cancel,
                &progress,
            )
            .unwrap_err();
            assert_eq!(err.kind(), crate::ErrorKind::Cancelled);
            assert_eq!(std::fs::read_dir(&out).map(|d| d.count()).unwrap_or(0), 0);
        }
    }

    #[test]
    fn office_files_named_zip_are_left_to_the_normal_converters() {
        let tmp = tempfile::tempdir().unwrap();
        let docx = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/markitdown/files/test.docx"),
        )
        .unwrap();
        let path = tmp.path().join("really-word.zip");
        std::fs::write(&path, docx).unwrap();
        let result = run(
            ZipOutput::Folder,
            &tmp.path().join("out"),
            &path,
            &ConvertOptions::default(),
        )
        .unwrap();
        assert!(result.is_none());
    }
}
