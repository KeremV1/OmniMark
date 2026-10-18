//! `omnimark convert` — converts files to Markdown with the same core as the desktop app.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use omnimark_core::assets::{AssetSink, DirAssetSink, NullAssetSink};
use omnimark_core::formats::{default_registry, RegistryConfig};
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
    /// Path to the pdfium library file.
    #[arg(long, env = "OMNIMARK_PDFIUM_PATH")]
    pdfium: Option<PathBuf>,
}

fn main() -> ExitCode {
    let Command::Convert(args) = Cli::parse().command;
    let registry = default_registry(&RegistryConfig {
        pdfium_library: args.pdfium.clone(),
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
