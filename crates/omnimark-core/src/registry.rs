//! Converter registry: tries every file-type guess against every converter in priority order.

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use crate::assets::AssetSink;
use crate::cancel::CancelToken;
use crate::detect;
use crate::document::{ConvertOptions, Document};
use crate::error::{ConvertError, Result};
use crate::info::{FileInfo, Input};
use crate::markdown::normalize;

#[cfg(test)]
use crate::error::ErrorKind;

/// Lower values are tried first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// A specific format such as PDF or DOCX.
    Specific = 0,
    /// Near catch-all converters (plain text, HTML).
    Generic = 10,
}

pub trait Converter: Send + Sync {
    fn name(&self) -> &'static str;

    fn priority(&self) -> Priority {
        Priority::Specific
    }

    /// Quick decision from the guess alone; must not read the file.
    fn accepts(&self, info: &FileInfo) -> bool;

    fn convert(&self, input: &Input, ctx: &mut ConvertContext<'_>) -> Result<Document>;
}

/// Everything a converter may use besides the input bytes.
pub struct ConvertContext<'a> {
    pub options: &'a ConvertOptions,
    pub assets: &'a mut dyn AssetSink,
    pub cancel: &'a CancelToken,
    /// Receives progress in `0.0..=1.0`.
    pub progress: &'a (dyn Fn(f32) + Sync),
}

impl ConvertContext<'_> {
    pub fn report(&self, fraction: f32) {
        (self.progress)(fraction.clamp(0.0, 1.0));
    }

    pub fn check_cancel(&self) -> Result<()> {
        self.cancel.check()
    }
}

#[derive(Default)]
pub struct Registry {
    converters: Vec<Box<dyn Converter>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Converters with equal priority are tried in registration order.
    pub fn register(&mut self, converter: impl Converter + 'static) {
        self.converters.push(Box::new(converter));
        self.converters.sort_by_key(|c| c.priority());
    }

    pub fn accepts_any(&self, path: &Path, head: &[u8]) -> bool {
        detect::guesses(path, head)
            .iter()
            .any(|info| self.converters.iter().any(|c| c.accepts(info)))
    }

    pub fn convert(&self, input: &Input, ctx: &mut ConvertContext<'_>) -> Result<Document> {
        ctx.check_cancel()?;
        let mut tried = HashSet::new();
        let mut first_error = None;
        for info in detect::guesses(&input.path, &input.bytes) {
            for converter in &self.converters {
                if tried.contains(converter.name()) || !converter.accepts(&info) {
                    continue;
                }
                tried.insert(converter.name());
                match converter.convert(input, ctx) {
                    Ok(mut document) => {
                        document.markdown = normalize(&document.markdown);
                        return Ok(document);
                    }
                    Err(ConvertError::Cancelled) => return Err(ConvertError::Cancelled),
                    Err(error) => {
                        first_error.get_or_insert(error);
                    }
                }
            }
        }
        Err(first_error.unwrap_or(ConvertError::Unsupported))
    }

    pub fn convert_file(&self, path: &Path, ctx: &mut ConvertContext<'_>) -> Result<Document> {
        let size = std::fs::metadata(path)?.len();
        let limit = ctx.options.max_file_size;
        if size > limit {
            return Err(ConvertError::TooLarge { size, limit });
        }
        let bytes = std::fs::read(path)?;
        self.convert(
            &Input {
                path: path.to_path_buf(),
                bytes,
            },
            ctx,
        )
    }
}

/// Converts one file; a panic inside a converter fails only this file.
pub fn convert_isolated(
    registry: &Registry,
    path: &Path,
    ctx: &mut ConvertContext<'_>,
) -> Result<Document> {
    match catch_unwind(AssertUnwindSafe(|| registry.convert_file(path, ctx))) {
        Ok(result) => result,
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            Err(ConvertError::Corrupt(format!("internal error: {message}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::MemoryAssetSink;
    use std::path::PathBuf;

    struct Fake {
        name: &'static str,
        ext: &'static str,
        priority: Priority,
        outcome: fn() -> Result<Document>,
    }

    impl Converter for Fake {
        fn name(&self) -> &'static str {
            self.name
        }
        fn priority(&self) -> Priority {
            self.priority
        }
        fn accepts(&self, info: &FileInfo) -> bool {
            info.has_extension(&[self.ext])
        }
        fn convert(&self, _: &Input, _: &mut ConvertContext<'_>) -> Result<Document> {
            (self.outcome)()
        }
    }

    fn run(registry: &Registry, file: &str) -> Result<Document> {
        let options = ConvertOptions::default();
        let mut assets = MemoryAssetSink::new("x_assets");
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        registry.convert(
            &Input {
                path: PathBuf::from(file),
                bytes: b"irrelevant".to_vec(),
            },
            &mut ctx,
        )
    }

    #[test]
    fn specific_converters_run_before_generic_ones_and_output_is_normalized() {
        let mut r = Registry::new();
        r.register(Fake {
            name: "generic",
            ext: "x",
            priority: Priority::Generic,
            outcome: || Ok(Document::new("generic")),
        });
        r.register(Fake {
            name: "specific",
            ext: "x",
            priority: Priority::Specific,
            outcome: || Ok(Document::new("spec  \n\n\n\nific\n")),
        });
        assert_eq!(run(&r, "a.x").unwrap().markdown, "spec\n\nific");
    }

    #[test]
    fn falls_through_failures_and_reports_the_first_error() {
        let mut r = Registry::new();
        r.register(Fake {
            name: "broken",
            ext: "x",
            priority: Priority::Specific,
            outcome: || Err(ConvertError::Corrupt("bad".into())),
        });
        r.register(Fake {
            name: "fallback",
            ext: "x",
            priority: Priority::Generic,
            outcome: || Ok(Document::new("ok")),
        });
        assert_eq!(run(&r, "a.x").unwrap().markdown, "ok");

        let mut only_broken = Registry::new();
        only_broken.register(Fake {
            name: "broken",
            ext: "x",
            priority: Priority::Specific,
            outcome: || Err(ConvertError::Encrypted),
        });
        assert_eq!(
            run(&only_broken, "a.x").unwrap_err().kind(),
            ErrorKind::Encrypted
        );
    }

    #[test]
    fn cancellation_stops_immediately() {
        let mut r = Registry::new();
        r.register(Fake {
            name: "cancel",
            ext: "x",
            priority: Priority::Specific,
            outcome: || Err(ConvertError::Cancelled),
        });
        r.register(Fake {
            name: "never",
            ext: "x",
            priority: Priority::Generic,
            outcome: || Ok(Document::new("no")),
        });
        assert_eq!(run(&r, "a.x").unwrap_err().kind(), ErrorKind::Cancelled);
    }

    #[test]
    fn nothing_accepting_is_unsupported() {
        assert_eq!(
            run(&Registry::new(), "a.x").unwrap_err().kind(),
            ErrorKind::Unsupported
        );
    }

    #[test]
    fn panics_are_isolated() {
        let mut r = Registry::new();
        r.register(Fake {
            name: "boom",
            ext: "txt",
            priority: Priority::Specific,
            outcome: || panic!("parser exploded"),
        });
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.txt");
        std::fs::write(&path, "x").unwrap();
        let options = ConvertOptions::default();
        let mut assets = MemoryAssetSink::new("a_assets");
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        let err = convert_isolated(&r, &path, &mut ctx).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Corrupt);
        assert!(err.to_string().contains("parser exploded"));
    }

    #[test]
    fn too_large_files_are_rejected_before_reading() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("big.x");
        std::fs::write(&path, vec![b'a'; 10]).unwrap();
        let options = ConvertOptions {
            max_file_size: 5,
            ..ConvertOptions::default()
        };
        let mut assets = MemoryAssetSink::new("big_assets");
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        let err = Registry::new().convert_file(&path, &mut ctx).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::TooLarge);
    }
}
