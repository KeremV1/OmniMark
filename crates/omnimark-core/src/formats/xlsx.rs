use crate::error::{ConvertError, Result};
use crate::registry::{ConvertContext, Converter};
use crate::{Document, FileInfo, Input};

pub struct XlsxConverter;

impl Converter for XlsxConverter {
    fn name(&self) -> &'static str {
        "xlsx"
    }

    fn accepts(&self, info: &FileInfo) -> bool {
        info.has_extension(&["xlsx", "xlsm", "xls", "ods"])
            || info.mime_starts_with(&[
                "application/vnd.openxmlformats-officedocument.spreadsheetml",
                "application/vnd.ms-excel",
                "application/vnd.oasis.opendocument.spreadsheet",
            ])
    }

    fn convert(&self, _input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        Err(ConvertError::Unsupported)
    }
}
