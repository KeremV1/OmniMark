//! Office Math (OMML) → LaTeX. Implemented in Task 11.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OmmlError {
    #[error("invalid OMML: {0}")]
    Xml(String),
    #[error("unsupported OMML construct: {0}")]
    Unsupported(String),
}

/// Converts one `<m:oMath>` element (its full XML text, namespace prefixes as in the document)
/// to LaTeX without surrounding `$` delimiters.
pub fn omml_to_latex(_omath_xml: &str) -> Result<String, OmmlError> {
    Err(OmmlError::Unsupported(
        "OMML conversion is not available yet".into(),
    ))
}
