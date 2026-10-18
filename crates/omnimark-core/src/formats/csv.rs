use crate::error::{ConvertError, Result};
use crate::registry::{ConvertContext, Converter};
use crate::{Document, FileInfo, Input};

pub struct CsvConverter;

impl Converter for CsvConverter {
    fn name(&self) -> &'static str {
        "csv"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["csv"]) || info.mime_starts_with(&["text/csv", "application/csv"])
    }

    fn convert(&self, _input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        Err(ConvertError::Unsupported)
    }
}
