use std::path::PathBuf;

use crate::error::{ConvertError, Result};
use crate::registry::{ConvertContext, Converter};
use crate::{Document, FileInfo, Input};

/// File name of the pdfium shared library on this platform.
pub fn pdfium_library_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}

pub struct PdfConverter {
    library: Option<PathBuf>,
}

impl PdfConverter {
    pub fn new(library: Option<PathBuf>) -> Self {
        Self { library }
    }
}

impl Converter for PdfConverter {
    fn name(&self) -> &'static str {
        "pdf"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["pdf"])
            || info.mime_starts_with(&["application/pdf", "application/x-pdf"])
    }

    fn convert(&self, _input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        let _ = &self.library;
        Err(ConvertError::Unsupported)
    }
}
