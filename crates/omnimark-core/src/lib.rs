//! OmniMark conversion core: turns documents into Markdown.

pub mod archive;
pub mod assets;
pub mod cancel;
pub mod detect;
pub mod document;
pub mod error;
pub mod formats;
pub mod info;
pub mod markdown;
pub mod ocr;
pub mod ooxml;
pub mod output;
pub mod registry;
pub mod xml;
pub mod zip_output;

pub use cancel::CancelToken;
pub use document::{ConvertOptions, Document, DEFAULT_MAX_FILE_SIZE};
pub use error::{ConvertError, ErrorKind, Result};
pub use info::{FileInfo, Input};
pub use registry::{convert_isolated, ConvertContext, Converter, Priority, Registry};
