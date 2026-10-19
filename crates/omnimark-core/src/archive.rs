//! Bounded ZIP reading for OOXML containers (zip-bomb protection).

use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::error::{ConvertError, Result};

pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_TOTAL_UNCOMPRESSED: u64 = 1024 * 1024 * 1024;
/// Largest single entry that is decompressed; the declared size is never trusted beyond it.
pub const MAX_ENTRY_UNCOMPRESSED: u64 = 256 * 1024 * 1024;

pub struct Archive<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    /// Bytes that may still be decompressed from this archive.
    budget: u64,
    /// Bytes one entry may decompress to.
    entry_limit: u64,
}

impl<'a> Archive<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self> {
        let zip = ZipArchive::new(Cursor::new(bytes)).map_err(ConvertError::corrupt)?;
        if zip.len() > MAX_ENTRIES {
            return Err(ConvertError::Corrupt(format!(
                "archive has too many entries ({})",
                zip.len()
            )));
        }
        Ok(Self {
            zip,
            budget: MAX_TOTAL_UNCOMPRESSED,
            entry_limit: MAX_ENTRY_UNCOMPRESSED,
        })
    }

    /// Overrides the decompression budget (tests).
    pub fn with_budget(mut self, budget: u64) -> Self {
        self.budget = budget;
        self
    }

    /// Overrides the per-entry limit (tests).
    pub fn with_entry_limit(mut self, limit: u64) -> Self {
        self.entry_limit = limit;
        self
    }

    pub fn names(&self) -> Vec<String> {
        self.zip
            .file_names()
            .filter_map(|n| n.ok())
            .map(|n| n.into_owned())
            .collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.zip.index_for_name(name).is_some()
    }

    /// Reads an entry by its forward-slash path; `Ok(None)` when it does not exist.
    pub fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>> {
        let Some(index) = self.zip.index_for_name(name) else {
            return Ok(None);
        };
        let mut file = self.zip.by_index(index).map_err(ConvertError::corrupt)?;
        let too_big = |limit: u64, budget: u64| {
            ConvertError::Corrupt(if limit < budget {
                format!("archive entry {name} expands beyond the per-entry limit")
            } else {
                "archive expands beyond the 1 GB limit".into()
            })
        };
        let limit = self.budget.min(self.entry_limit);
        if file.size() > limit {
            return Err(too_big(limit, self.budget));
        }
        // The declared size is only a hint: it may lie, so it never sizes the buffer beyond the limit.
        let mut buf = Vec::with_capacity(file.size().min(limit) as usize);
        (&mut file).take(limit + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > limit {
            return Err(too_big(limit, self.budget));
        }
        self.budget -= buf.len() as u64;
        Ok(Some(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut buf);
        for (name, data) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        buf.into_inner()
    }

    #[test]
    fn reads_entries_and_reports_missing_ones() {
        let bytes = zip_with(&[("word/document.xml", b"<w:document/>")]);
        let mut archive = Archive::open(&bytes).unwrap();
        assert!(archive.contains("word/document.xml"));
        assert_eq!(
            archive.read("word/document.xml").unwrap().unwrap(),
            b"<w:document/>"
        );
        assert_eq!(archive.read("missing.xml").unwrap(), None);
    }

    #[test]
    fn rejects_non_zip_input() {
        assert_eq!(
            Archive::open(b"not a zip").err().unwrap().kind(),
            crate::ErrorKind::Corrupt
        );
    }

    #[test]
    fn enforces_the_uncompressed_budget() {
        let bytes = zip_with(&[("a", &[b'x'; 64]), ("b", &[b'y'; 64])]);
        let mut archive = Archive::open(&bytes).unwrap().with_budget(100);
        assert!(archive.read("a").is_ok());
        assert_eq!(
            archive.read("b").unwrap_err().kind(),
            crate::ErrorKind::Corrupt
        );
    }

    #[test]
    fn enforces_the_per_entry_limit() {
        let bytes = zip_with(&[("small", &[b'x'; 16]), ("big", &[b'y'; 64])]);
        let mut archive = Archive::open(&bytes).unwrap().with_entry_limit(32);
        assert_eq!(archive.read("small").unwrap().unwrap().len(), 16);
        assert_eq!(
            archive.read("big").unwrap_err().kind(),
            crate::ErrorKind::Corrupt
        );
    }
}
