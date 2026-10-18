//! Finds the pdfium shared library shipped with (or next to) the app.

use std::path::{Path, PathBuf};

use omnimark_core::formats::pdfium_library_file_name;

/// Bundled resource first; in debug builds fall back to the repository's vendor/pdfium.
pub fn locate(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let name = pdfium_library_file_name();
    let bundled = resource_dir.map(|d| d.join("pdfium").join(name));
    if let Some(path) = bundled.filter(|p| p.exists()) {
        return Some(path);
    }
    if cfg!(debug_assertions) {
        let vendored = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/pdfium")
            .join(name);
        if vendored.exists() {
            return Some(vendored);
        }
    }
    None
}
