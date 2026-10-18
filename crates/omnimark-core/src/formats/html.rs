use crate::error::{ConvertError, Result};
use crate::registry::{ConvertContext, Converter, Priority};
use crate::{Document, FileInfo, Input};

pub struct HtmlConverter;

impl Converter for HtmlConverter {
    fn name(&self) -> &'static str {
        "html"
    }

    fn priority(&self) -> Priority {
        Priority::Generic
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["html", "htm"])
            || info.mime_starts_with(&["text/html", "application/xhtml"])
    }

    fn convert(&self, _input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        Err(ConvertError::Unsupported)
    }
}
