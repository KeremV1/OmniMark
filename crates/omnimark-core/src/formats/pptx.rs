use crate::error::{ConvertError, Result};
use crate::registry::{ConvertContext, Converter};
use crate::{Document, FileInfo, Input};

pub struct PptxConverter;

impl Converter for PptxConverter {
    fn name(&self) -> &'static str {
        "pptx"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["pptx"])
            || info
                .mime_starts_with(&["application/vnd.openxmlformats-officedocument.presentationml"])
    }

    fn convert(&self, _input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        Err(ConvertError::Unsupported)
    }
}
