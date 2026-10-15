//! OmniMark conversion core: turns documents into Markdown.

pub mod cancel;
pub mod document;
pub mod error;
pub mod markdown;

pub use cancel::CancelToken;
pub use document::{ConvertOptions, Document, DEFAULT_MAX_FILE_SIZE};
pub use error::{ConvertError, ErrorKind, Result};
