use crate::error::{ConvertError, Result};
use crate::registry::{ConvertContext, Converter};
use crate::{Document, FileInfo, Input};

pub struct DocxConverter;

impl Converter for DocxConverter {
    fn name(&self) -> &'static str {
        "docx"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["docx"])
            || info.mime_starts_with(&[
                "application/vnd.openxmlformats-officedocument.wordprocessingml",
            ])
    }

    fn convert(&self, _input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        Err(ConvertError::Unsupported)
    }
}
