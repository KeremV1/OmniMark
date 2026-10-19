use std::path::PathBuf;

/// One guess about what a file is. Several guesses may exist when the extension and
/// the content disagree; converters are tried against each in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// Lower-case extension without the dot, e.g. `docx`.
    pub extension: Option<String>,
    pub mime: Option<String>,
}

impl FileInfo {
    pub fn has_extension(&self, candidates: &[&str]) -> bool {
        self.extension
            .as_deref()
            .is_some_and(|e| candidates.contains(&e))
    }

    pub fn mime_starts_with(&self, prefixes: &[&str]) -> bool {
        self.mime
            .as_deref()
            .is_some_and(|m| prefixes.iter().any(|p| m.starts_with(p)))
    }
}

/// A file loaded into memory for conversion.
#[derive(Debug, Clone)]
pub struct Input {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}
