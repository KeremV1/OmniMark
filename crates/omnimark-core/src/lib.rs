//! OmniMark conversion core: turns documents into Markdown.

pub mod cancel;
pub mod detect;
pub mod document;
pub mod error;
pub mod info;
pub mod markdown;

pub use info::{FileInfo, Input};

pub use cancel::CancelToken;
pub use document::{ConvertOptions, Document, DEFAULT_MAX_FILE_SIZE};
pub use error::{ConvertError, ErrorKind, Result};
pub mod assets;
pub mod output;
