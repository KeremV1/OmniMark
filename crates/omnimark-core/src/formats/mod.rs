//! Built-in converters.

pub mod csv;
pub mod docx;
pub mod html;
pub mod omml;
pub mod pdf;
pub mod pptx;
pub mod text;
pub mod xlsx;

use std::path::PathBuf;

use crate::registry::Registry;

pub use pdf::pdfium_library_file_name;

#[derive(Debug, Clone, Default)]
pub struct RegistryConfig {
    /// Full path to the pdfium library file; PDF conversion fails with `MissingComponent` without it.
    pub pdfium_library: Option<PathBuf>,
}

pub fn default_registry(config: &RegistryConfig) -> Registry {
    let mut registry = Registry::new();
    registry.register(pdf::PdfConverter::new(config.pdfium_library.clone()));
    registry.register(docx::DocxConverter);
    registry.register(xlsx::XlsxConverter);
    registry.register(pptx::PptxConverter);
    registry.register(csv::CsvConverter);
    registry.register(html::HtmlConverter);
    registry.register(text::TextConverter);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn default_registry_accepts_every_m1_extension_and_rejects_media() {
        let registry = default_registry(&RegistryConfig::default());
        for ext in [
            "pdf", "docx", "xlsx", "xlsm", "xls", "ods", "pptx", "html", "htm", "csv", "txt", "md",
            "markdown", "json", "jsonl",
        ] {
            assert!(
                registry.accepts_any(Path::new(&format!("f.{ext}")), b""),
                "{ext}"
            );
        }
        assert!(!registry.accepts_any(Path::new("photo.jpg"), &[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!registry.accepts_any(
            Path::new("song.mp3"),
            b"ID3\x04\x00\x00\x00\x00\x00\x00\x00"
        ));
    }
}
