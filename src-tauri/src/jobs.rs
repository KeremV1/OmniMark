//! Batch conversion: a small worker pool that reports every step as a `ConvertEvent`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use omnimark_core::assets::{AssetSink, DirAssetSink, NullAssetSink};
use omnimark_core::output::{write_atomic, OutputAllocator};
use omnimark_core::zip_output::{write_zip_file, ZipOutput, ZipWritten};
use omnimark_core::{
    convert_isolated, CancelToken, ConvertContext, ConvertError, ConvertOptions, ErrorKind,
    Registry,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum ConvertEvent {
    Started {
        total: usize,
    },
    FileStarted {
        path: String,
    },
    Progress {
        path: String,
        fraction: f32,
    },
    FileFinished {
        path: String,
        output: String,
        warnings: Vec<String>,
        /// Set for a `.zip`: how it was written and how many files were converted or skipped.
        zip: Option<ZipInfo>,
    },
    FileFailed {
        path: String,
        kind: ErrorKind,
        message: String,
    },
    FileCancelled {
        path: String,
    },
    Finished {
        ok: usize,
        failed: usize,
        cancelled: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipInfo {
    pub mode: ZipOutput,
    pub converted: usize,
    pub skipped: usize,
}

pub struct BatchRequest {
    pub files: Vec<PathBuf>,
    pub output_dir: PathBuf,
    pub options: ConvertOptions,
    pub workers: usize,
    /// Added to the warnings of every finished file (for example "OCR was skipped").
    pub ocr_warning: Option<String>,
    pub zip_output: ZipOutput,
}

enum Outcome {
    Ok,
    Failed,
    Cancelled,
}

/// Smallest progress change worth an event.
const PROGRESS_STEP: f32 = 0.01;

/// Converts `request.files` with up to `request.workers` threads and reports every step through `emit`.
pub fn run_batch(
    registry: &Registry,
    request: &BatchRequest,
    cancel: &CancelToken,
    emit: &(dyn Fn(ConvertEvent) + Sync),
) {
    emit(ConvertEvent::Started {
        total: request.files.len(),
    });
    let allocator = OutputAllocator::new(&request.output_dir);
    let next = AtomicUsize::new(0);
    let (ok, failed, cancelled) = (
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
    let workers = request.workers.clamp(1, 8).min(request.files.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = request.files.get(index) else {
                    break;
                };
                let counter = match convert_one(registry, &allocator, path, request, cancel, emit) {
                    Outcome::Ok => &ok,
                    Outcome::Failed => &failed,
                    Outcome::Cancelled => &cancelled,
                };
                counter.fetch_add(1, Ordering::Relaxed);
            });
        }
    });
    emit(ConvertEvent::Finished {
        ok: ok.into_inner(),
        failed: failed.into_inner(),
        cancelled: cancelled.into_inner(),
    });
}

fn convert_one(
    registry: &Registry,
    allocator: &OutputAllocator,
    path: &Path,
    request: &BatchRequest,
    cancel: &CancelToken,
    emit: &(dyn Fn(ConvertEvent) + Sync),
) -> Outcome {
    let options = &request.options;
    let shown = path.display().to_string();
    if cancel.is_cancelled() {
        emit(ConvertEvent::FileCancelled { path: shown });
        return Outcome::Cancelled;
    }
    emit(ConvertEvent::FileStarted {
        path: shown.clone(),
    });

    let last_reported = Mutex::new(0.0f32);
    let progress = |fraction: f32| {
        let mut last = last_reported
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if fraction - *last >= PROGRESS_STEP {
            *last = fraction;
            emit(ConvertEvent::Progress {
                path: shown.clone(),
                fraction,
            });
        }
    };

    if is_zip_name(path) {
        match write_zip_file(
            request.zip_output,
            registry,
            path,
            allocator,
            options,
            cancel,
            &progress,
        ) {
            Ok(Some(written)) => return finish_zip(shown, written, request, emit),
            Ok(None) => {} // an Office file with a .zip name: the normal converters take it
            Err(error) => return report_error(shown, error, emit),
        }
    }

    let target = allocator.allocate(path);

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
        progress: &progress,
    };
    let result = convert_isolated(registry, path, &mut ctx).and_then(|document| {
        write_atomic(&target.markdown_path, &document.markdown)?;
        Ok(document)
    });

    match result {
        Ok(document) => {
            emit(ConvertEvent::FileFinished {
                path: shown,
                output: target.markdown_path.display().to_string(),
                warnings: document
                    .warnings
                    .into_iter()
                    .chain(request.ocr_warning.clone())
                    .collect(),
                zip: None,
            });
            Outcome::Ok
        }
        Err(error) => {
            let _ = dir_sink.remove();
            report_error(shown, error, emit)
        }
    }
}

fn is_zip_name(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

fn finish_zip(
    shown: String,
    written: ZipWritten,
    request: &BatchRequest,
    emit: &(dyn Fn(ConvertEvent) + Sync),
) -> Outcome {
    emit(ConvertEvent::FileFinished {
        path: shown,
        output: written.output.display().to_string(),
        warnings: written
            .warnings
            .into_iter()
            .chain(request.ocr_warning.clone())
            .collect(),
        zip: Some(ZipInfo {
            mode: request.zip_output,
            converted: written.converted,
            skipped: written.skipped,
        }),
    });
    Outcome::Ok
}

fn report_error(
    shown: String,
    error: ConvertError,
    emit: &(dyn Fn(ConvertEvent) + Sync),
) -> Outcome {
    if matches!(error, ConvertError::Cancelled) {
        emit(ConvertEvent::FileCancelled { path: shown });
        Outcome::Cancelled
    } else {
        emit(ConvertEvent::FileFailed {
            path: shown,
            kind: error.kind(),
            message: error.to_string(),
        });
        Outcome::Failed
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use omnimark_core::formats::{default_registry, RegistryConfig};
    use std::sync::Mutex;

    fn collect(
        registry: &Registry,
        request: &BatchRequest,
        cancel: &CancelToken,
    ) -> Vec<ConvertEvent> {
        let events = Mutex::new(Vec::new());
        run_batch(registry, request, cancel, &|e| {
            events.lock().unwrap().push(e)
        });
        events.into_inner().unwrap()
    }

    #[test]
    fn converts_files_and_reports_results() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        std::fs::create_dir(&input).unwrap();
        std::fs::write(input.join("a.txt"), "hello").unwrap();
        std::fs::write(input.join("b.bin"), [0u8, 1, 2, 3]).unwrap();
        let out = dir.path().join("out");
        let request = BatchRequest {
            files: vec![input.join("a.txt"), input.join("b.bin")],
            output_dir: out.clone(),
            options: ConvertOptions::default(),
            workers: 2,
            ocr_warning: None,
            zip_output: ZipOutput::default(),
        };
        let registry = default_registry(&RegistryConfig::default());
        let events = collect(&registry, &request, &CancelToken::new());
        assert_eq!(events.first(), Some(&ConvertEvent::Started { total: 2 }));
        assert_eq!(
            events.last(),
            Some(&ConvertEvent::Finished {
                ok: 1,
                failed: 1,
                cancelled: 0
            })
        );
        assert!(events.iter().any(|e| matches!(
            e,
            ConvertEvent::FileFailed {
                kind: ErrorKind::Unsupported,
                ..
            }
        )));
        assert_eq!(std::fs::read_to_string(out.join("a.md")).unwrap(), "hello");
        assert!(!out.join("b_assets").exists());
    }

    #[test]
    fn cancelled_before_start_cancels_every_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        let cancel = CancelToken::new();
        cancel.cancel();
        let request = BatchRequest {
            files: vec![dir.path().join("a.txt")],
            output_dir: dir.path().join("out"),
            options: ConvertOptions::default(),
            workers: 1,
            ocr_warning: None,
            zip_output: ZipOutput::default(),
        };
        let events = collect(
            &default_registry(&RegistryConfig::default()),
            &request,
            &cancel,
        );
        assert!(events.contains(&ConvertEvent::FileCancelled {
            path: dir.path().join("a.txt").display().to_string()
        }));
        assert_eq!(
            events.last(),
            Some(&ConvertEvent::Finished {
                ok: 0,
                failed: 0,
                cancelled: 1
            })
        );
    }

    #[test]
    fn ocr_warning_is_added_to_every_finished_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        let request = BatchRequest {
            files: vec![dir.path().join("a.txt")],
            output_dir: dir.path().join("out"),
            options: ConvertOptions::default(),
            workers: 1,
            ocr_warning: Some("OCR was skipped: none.".into()),
            zip_output: ZipOutput::default(),
        };
        let events = collect(
            &default_registry(&RegistryConfig::default()),
            &request,
            &CancelToken::new(),
        );
        assert!(events.iter().any(|e| matches!(e, ConvertEvent::FileFinished { warnings, .. } if warnings == &vec!["OCR was skipped: none.".to_string()])));
    }

    fn write_zip(path: &std::path::Path) {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in [
            ("a.txt", &b"alpha"[..]),
            ("dir/b.txt", b"beta"),
            ("c.bin", &[0u8, 1]),
        ] {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn a_zip_is_one_row_with_counts_in_every_mode() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("pack.zip");
        write_zip(&archive);
        let registry = default_registry(&RegistryConfig::default());
        for (mode, expected) in [
            (ZipOutput::Folder, "out/pack"),
            (ZipOutput::Single, "out/pack.md"),
            (ZipOutput::Zip, "out/pack-md.zip"),
        ] {
            let out = dir.path().join(format!("{mode:?}")).join("out");
            let request = BatchRequest {
                files: vec![archive.clone()],
                output_dir: out.clone(),
                options: ConvertOptions::default(),
                workers: 1,
                ocr_warning: None,
                zip_output: mode,
            };
            let events = collect(&registry, &request, &CancelToken::new());
            let finished: Vec<_> = events
                .iter()
                .filter_map(|e| match e {
                    ConvertEvent::FileFinished { output, zip, .. } => Some((output, zip)),
                    _ => None,
                })
                .collect();
            assert_eq!(finished.len(), 1, "{mode:?}");
            let (output, zip) = finished[0];
            assert!(
                std::path::Path::new(output).ends_with(expected.trim_start_matches("out/")),
                "{mode:?}: {output}"
            );
            assert!(std::path::Path::new(output).exists());
            assert_eq!(
                zip,
                &Some(ZipInfo {
                    mode,
                    converted: 2,
                    skipped: 1
                })
            );
            assert_eq!(
                events.last(),
                Some(&ConvertEvent::Finished {
                    ok: 1,
                    failed: 0,
                    cancelled: 0
                })
            );
        }
    }

    #[test]
    fn a_zip_without_supported_files_fails_as_one_row() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("junk.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        writer
            .start_file("c.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut writer, &[0u8, 1]).unwrap();
        writer.finish().unwrap();
        let request = BatchRequest {
            files: vec![archive],
            output_dir: dir.path().join("out"),
            options: ConvertOptions::default(),
            workers: 1,
            ocr_warning: None,
            zip_output: ZipOutput::Folder,
        };
        let events = collect(
            &default_registry(&RegistryConfig::default()),
            &request,
            &CancelToken::new(),
        );
        assert!(events.iter().any(|e| matches!(
            e,
            ConvertEvent::FileFailed {
                kind: ErrorKind::Unsupported,
                ..
            }
        )));
    }
}
