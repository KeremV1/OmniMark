use crate::error::{ConvertError, Result};
use crate::registry::{ConvertContext, Converter, Priority};
use crate::{Document, FileInfo, Input};

pub struct TextConverter;

impl Converter for TextConverter {
    fn name(&self) -> &'static str {
        "text"
    }

    fn priority(&self) -> Priority {
        Priority::Generic
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["txt", "text", "md", "markdown", "json", "jsonl"])
            || info.mime_starts_with(&["text/", "application/json", "application/jsonl"])
    }

    fn convert(&self, _input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        Err(ConvertError::Unsupported)
    }
}
