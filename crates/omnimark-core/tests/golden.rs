//! Golden tests: converts fixtures from `tests/fixtures/markitdown/files` and checks the
//! expectations in `tests/golden/vectors/*.json`. Deliberate differences from MarkItDown are
//! documented in `tests/golden/overrides.md`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use omnimark_core::assets::{AssetSink, MemoryAssetSink, NullAssetSink};
use omnimark_core::formats::{default_registry, pdfium_library_file_name, RegistryConfig};
use omnimark_core::ocr::{MockEngine, OcrEngine};
use omnimark_core::{convert_isolated, CancelToken, ConvertContext, ConvertOptions, ErrorKind};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    file: String,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    regex: Vec<String>,
    #[serde(default)]
    counts: Vec<CountCheck>,
    #[serde(default)]
    expect_error: Option<ErrorKind>,
    #[serde(default = "default_true")]
    extract_images: bool,
    /// Fixture folder under `tests/fixtures/markitdown/` (default `files`).
    #[serde(default)]
    dir: Option<String>,
    /// Convert with `MockEngine::default()` as the OCR engine.
    #[serde(default)]
    ocr: bool,
    /// The whole normalized output.
    #[serde(default)]
    exact: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CountCheck {
    text: String,
    count: usize,
}

fn default_true() -> bool {
    true
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pdfium_path(root: &Path) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OMNIMARK_PDFIUM_PATH") {
        return Some(PathBuf::from(path));
    }
    let vendored = root.join("vendor/pdfium").join(pdfium_library_file_name());
    vendored.exists().then_some(vendored)
}

fn load_vectors(root: &Path) -> Vec<Vector> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("tests/golden/vectors"))
        .expect("tests/golden/vectors")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
        .iter()
        .flat_map(|f| {
            let text = std::fs::read_to_string(f).unwrap();
            serde_json::from_str::<Vec<Vector>>(&text)
                .unwrap_or_else(|e| panic!("{}: {e}", f.display()))
        })
        .collect()
}

#[test]
fn golden_vectors() {
    let root = repo_root();
    let vectors = load_vectors(&root);
    let pdfium = pdfium_path(&root);
    let skip_pdf = std::env::var_os("OMNIMARK_SKIP_PDFIUM").is_some();
    let registry = default_registry(&RegistryConfig {
        pdfium_library: pdfium.clone(),
    });
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden-out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut failures = Vec::new();

    for vector in &vectors {
        let label = if vector.ocr {
            format!("{} [ocr]", vector.file)
        } else {
            vector.file.clone()
        };
        let path = root
            .join("tests/fixtures/markitdown")
            .join(vector.dir.as_deref().unwrap_or("files"))
            .join(&vector.file);
        if vector.file.to_ascii_lowercase().ends_with(".pdf") {
            if skip_pdf {
                continue;
            }
            if pdfium.is_none() {
                failures.push(format!(
                    "{label}: pdfium library not found - run scripts/fetch-pdfium or set OMNIMARK_SKIP_PDFIUM=1"
                ));
                continue;
            }
        }
        let stem = Path::new(&vector.file)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let options = ConvertOptions {
            extract_images: vector.extract_images,
            ocr: vector
                .ocr
                .then(|| Arc::new(MockEngine::default()) as Arc<dyn OcrEngine>),
            ..ConvertOptions::default()
        };
        let mut memory = MemoryAssetSink::new(format!("{stem}_assets"));
        let mut null = NullAssetSink;
        let sink: &mut dyn AssetSink = if vector.extract_images {
            &mut memory
        } else {
            &mut null
        };
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: sink,
            cancel: &cancel,
            progress: &progress,
        };

        match (
            convert_isolated(&registry, &path, &mut ctx),
            vector.expect_error,
        ) {
            (Err(error), Some(kind)) if error.kind() == kind => {}
            (Err(error), _) => failures.push(format!("{label}: conversion failed: {error}")),
            (Ok(_), Some(kind)) => {
                failures.push(format!("{label}: expected error {kind:?}, got success"))
            }
            (Ok(document), None) => {
                let md = &document.markdown;
                let prefix = if vector.ocr { "ocr-" } else { "" };
                std::fs::write(out_dir.join(format!("{prefix}{}.md", vector.file)), md).unwrap();
                if let Some(exact) = &vector.exact {
                    if md != exact {
                        failures.push(format!(
                            "{label}: output differs from `exact`:
--- expected
{exact}
--- actual
{md}"
                        ));
                    }
                }
                for needle in &vector.include {
                    if !md.contains(needle.as_str()) {
                        failures.push(format!("{label}: missing {needle:?}"));
                    }
                }
                for needle in &vector.exclude {
                    if md.contains(needle.as_str()) {
                        failures.push(format!("{label}: must not contain {needle:?}"));
                    }
                }
                for pattern in &vector.regex {
                    let re = regex::Regex::new(pattern).unwrap();
                    if !re.is_match(md) {
                        failures.push(format!("{label}: no match for /{pattern}/"));
                    }
                }
                for check in &vector.counts {
                    let found = md.matches(check.text.as_str()).count();
                    if found != check.count {
                        failures.push(format!(
                            "{label}: {:?} found {found}x, expected {}x",
                            check.text, check.count
                        ));
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} golden failure(s) (outputs in {}):\n{}",
        failures.len(),
        out_dir.display(),
        failures.join("\n")
    );
}
