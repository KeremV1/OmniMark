//! `omnimark convert` — converts files to Markdown with the same core as the desktop app.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use omnimark_core::assets::{AssetSink, DirAssetSink, NullAssetSink};
use omnimark_core::formats::{default_registry, pdfium_library_file_name, RegistryConfig};
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
}

fn main() -> ExitCode {
    let Command::Convert(args) = Cli::parse().command;
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
    let options = ConvertOptions {
        extract_images: !args.no_images && !args.stdout,
        ..ConvertOptions::default()
    };
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
                Ok(document) => println!("{}", document.markdown),
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
