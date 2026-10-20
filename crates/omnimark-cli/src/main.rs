//! `omnimark convert` — converts files to Markdown with the same core as the desktop app.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use omnimark_core::assets::{AssetSink, DirAssetSink, NullAssetSink};
use omnimark_core::formats::{default_registry, pdfium_library_file_name, RegistryConfig};
use omnimark_core::ocr::models::{
    default_models_root, fetch_ocrs, ocrs_dir, ocrs_status, resolve_models_root, DownloadProgress,
    MODELS_ENV,
};
use omnimark_core::ocr::{build_engine, tesseract_engine, EngineConfig, OcrChoice, OcrLanguage};
use omnimark_core::output::{write_atomic, OutputAllocator};
use omnimark_core::{convert_isolated, CancelToken, ConvertContext, ConvertOptions};

#[derive(Parser)]
#[command(name = "omnimark", version, about = "Convert documents to Markdown")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert one or more files.
    Convert(ConvertArgs),
    /// List or download the optional OCR models.
    #[command(subcommand)]
    Models(ModelsCommand),
}

#[derive(Args)]
struct ModelsDir {
    /// Models folder [default: OMNIMARK_MODELS_DIR, then the local data folder].
    #[arg(long)]
    models_dir: Option<PathBuf>,
    /// The tesseract program [default: OMNIMARK_TESSERACT, PATH, then the usual install folders].
    #[arg(long)]
    tesseract_path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum ModelsCommand {
    /// Show which model files are present.
    List(ModelsDir),
    /// Download a model (checked with SHA-256).
    Fetch {
        #[arg(value_enum)]
        model: ModelName,
        #[command(flatten)]
        dir: ModelsDir,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ModelName {
    Ocrs,
}

#[derive(Clone, Copy, ValueEnum)]
enum OcrArg {
    Auto,
    Windows,
    Tesseract,
    Ocrs,
}

#[derive(Clone, Copy, ValueEnum)]
enum LangArg {
    Auto,
    Tr,
    En,
}

impl From<OcrArg> for OcrChoice {
    fn from(arg: OcrArg) -> Self {
        match arg {
            OcrArg::Auto => Self::Auto,
            OcrArg::Windows => Self::Windows,
            OcrArg::Tesseract => Self::Tesseract,
            OcrArg::Ocrs => Self::Ocrs,
        }
    }
}

impl From<LangArg> for OcrLanguage {
    fn from(arg: LangArg) -> Self {
        match arg {
            LangArg::Auto => Self::Auto,
            LangArg::Tr => Self::Tr,
            LangArg::En => Self::En,
        }
    }
}

#[derive(Args)]
struct ConvertArgs {
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
    /// Output folder.
    #[arg(short, long, default_value = ".")]
    out: PathBuf,
    /// Do not write embedded images.
    #[arg(long)]
    no_images: bool,
    /// Print Markdown to stdout instead of writing files (images are not written).
    #[arg(long)]
    stdout: bool,
    /// Path to the pdfium library file [default: vendor/pdfium/ under the current folder, then
    /// next to the executable].
    #[arg(long, env = "OMNIMARK_PDFIUM_PATH")]
    pdfium: Option<PathBuf>,
    /// Read text in images: `--ocr` (auto), `--ocr=windows`, `--ocr=tesseract` or `--ocr=ocrs`. Off by default.
    #[arg(
        long,
        value_enum,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "auto"
    )]
    ocr: Option<OcrArg>,
    /// OCR language.
    #[arg(long, value_enum, default_value = "auto")]
    ocr_lang: LangArg,
    #[command(flatten)]
    models: ModelsDir,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Convert(args) => convert(args),
        Command::Models(command) => models(command),
    }
}

fn models_root(dir: &ModelsDir) -> Option<PathBuf> {
    resolve_models_root(
        dir.models_dir.clone(),
        std::env::var_os(MODELS_ENV),
        default_models_root(),
    )
}

fn engine_config(dir: &ModelsDir) -> EngineConfig {
    EngineConfig {
        models_root: models_root(dir),
        tesseract_path: dir.tesseract_path.clone(),
    }
}

fn models(command: ModelsCommand) -> ExitCode {
    let (dir, fetch) = match &command {
        ModelsCommand::List(dir) => (dir, false),
        ModelsCommand::Fetch {
            model: ModelName::Ocrs,
            dir,
        } => (dir, true),
    };
    let Some(root) = models_root(dir) else {
        eprintln!("error: no models folder; pass --models-dir");
        return ExitCode::FAILURE;
    };
    if !fetch {
        println!("models folder: {}", root.display());
        for status in ocrs_status(&root) {
            let state = if status.present { "present" } else { "missing" };
            println!(
                "ocrs  {:<24} {:>6.1} MB  {state}",
                status.name,
                status.size as f64 / 1e6
            );
        }
        match tesseract_engine::resolve_path(dir.tesseract_path.as_deref()) {
            Some(path) => println!(
                "tesseract  {}  languages: {}",
                path.display(),
                tesseract_engine::languages(&path).join(", ")
            ),
            None => println!("tesseract  not found (https://github.com/tesseract-ocr/tesseract)"),
        }
        return ExitCode::SUCCESS;
    }
    let last = std::cell::Cell::new(u64::MAX);
    let progress = |p: DownloadProgress| {
        let percent = p.downloaded * 100 / p.total.max(1);
        if percent != last.replace(percent) {
            eprint!(
                "\rDownloading ocrs models: {percent:>3}% ({:.1}/{:.1} MB)",
                p.downloaded as f64 / 1e6,
                p.total as f64 / 1e6
            );
        }
    };
    let result = fetch_ocrs(&root, &CancelToken::new(), &progress);
    eprintln!();
    match result {
        Ok(()) => {
            println!("ok ocrs -> {}", ocrs_dir(&root).display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn convert(args: ConvertArgs) -> ExitCode {
    let bases: Vec<PathBuf> = [
        std::env::current_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf)),
    ]
    .into_iter()
    .flatten()
    .collect();
    let registry = default_registry(&RegistryConfig {
        pdfium_library: resolve_pdfium(args.pdfium.clone(), &bases, Path::is_file),
    });
    let mut options = ConvertOptions {
        extract_images: !args.no_images && !args.stdout,
        ocr_language: args.ocr_lang.into(),
        ..ConvertOptions::default()
    };
    if let Some(choice) = args.ocr {
        match build_engine(choice.into(), &engine_config(&args.models)) {
            Ok(engine) => options.ocr = Some(engine),
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    let allocator = OutputAllocator::new(&args.out);
    let cancel = CancelToken::new();
    let progress = |_: f32| {};
    let mut failed = false;

    for input in &args.inputs {
        if args.stdout {
            let mut sink = NullAssetSink;
            let mut ctx = ConvertContext {
                options: &options,
                assets: &mut sink,
                cancel: &cancel,
                progress: &progress,
            };
            match convert_isolated(&registry, input, &mut ctx) {
                Ok(document) => {
                    println!("{}", document.markdown);
                    for warning in &document.warnings {
                        eprintln!("warning {}: {warning}", input.display());
                    }
                }
                Err(error) => {
                    eprintln!("error {}: {error}", input.display());
                    failed = true;
                }
            }
            continue;
        }

        let target = allocator.allocate(input);
        let mut dir_sink = DirAssetSink::new(target.assets_dir.clone());
        let mut null_sink = NullAssetSink;
        let sink: &mut dyn AssetSink = if options.extract_images {
            &mut dir_sink
        } else {
            &mut null_sink
        };
        let mut ctx = ConvertContext {
            options: &options,
            assets: sink,
            cancel: &cancel,
            progress: &progress,
        };
        let result = convert_isolated(&registry, input, &mut ctx).and_then(|document| {
            write_atomic(&target.markdown_path, &document.markdown)
                .map(|_| document)
                .map_err(Into::into)
        });
        match result {
            Ok(document) => {
                println!(
                    "ok {} -> {}",
                    input.display(),
                    target.markdown_path.display()
                );
                for warning in &document.warnings {
                    eprintln!("warning {}: {warning}", input.display());
                }
            }
            Err(error) => {
                let _ = dir_sink.remove();
                eprintln!("error {}: {error}", input.display());
                failed = true;
            }
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Where a release or development build finds pdfium without `--pdfium`: the fetch scripts copy
/// the library to `vendor/pdfium/<name>` on every OS; the archive's own `bin/` (Windows) and
/// `lib/` (macOS, Linux) folders are fallbacks.
fn pdfium_candidates(base: &Path) -> Vec<PathBuf> {
    let name = pdfium_library_file_name();
    let vendor = base.join("vendor").join("pdfium");
    vec![
        vendor.join(name),
        vendor.join("bin").join(name),
        vendor.join("lib").join(name),
    ]
}

/// `--pdfium` (or `OMNIMARK_PDFIUM_PATH`, which clap reads into the same option) wins; otherwise
/// the first existing vendored library under each base folder, in order.
fn resolve_pdfium(
    explicit: Option<PathBuf>,
    bases: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    explicit.or_else(|| {
        bases
            .iter()
            .flat_map(|base| pdfium_candidates(base))
            .find(|candidate| exists(candidate))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdfium_resolution_order() {
        let name = pdfium_library_file_name();
        let cwd = PathBuf::from("cwd");
        let exe = PathBuf::from("exe");
        let bases = [cwd.clone(), exe.clone()];
        let at = |p: PathBuf| move |c: &Path| c == p;

        // An explicit path (flag or env var) always wins.
        let explicit = Some(PathBuf::from("custom/pdfium"));
        assert_eq!(resolve_pdfium(explicit.clone(), &bases, |_| true), explicit);
        // vendor/pdfium/<name> under the current dir comes first ...
        let cwd_lib = cwd.join("vendor/pdfium").join(name);
        assert_eq!(
            resolve_pdfium(None, &bases, |c| c == cwd_lib || c.starts_with(&exe)),
            Some(cwd_lib.clone())
        );
        // ... then the archive's bin/ folder, then the executable's dir.
        let cwd_bin = cwd.join("vendor/pdfium/bin").join(name);
        assert_eq!(
            resolve_pdfium(None, &bases, at(cwd_bin.clone())),
            Some(cwd_bin)
        );
        let exe_lib = exe.join("vendor/pdfium").join(name);
        assert_eq!(
            resolve_pdfium(None, &bases, at(exe_lib.clone())),
            Some(exe_lib)
        );
        assert_eq!(resolve_pdfium(None, &bases, |_| false), None);
    }
}
