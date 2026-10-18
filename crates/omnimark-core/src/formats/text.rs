use crate::detect::{decode_text, extension_of};
use crate::error::Result;
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

    fn convert(&self, input: &Input, _ctx: &mut ConvertContext<'_>) -> Result<Document> {
        let text = decode_text(&input.bytes);
        let is_json = matches!(extension_of(&input.path).as_deref(), Some("json" | "jsonl"));
        if !is_json {
            return Ok(Document::new(text));
        }
        let fence = "`".repeat(longest_backtick_run(&text).max(2) + 1);
        Ok(Document::new(format!(
            "{fence}json
{}
{fence}",
            text.trim_end()
        )))
    }
}

fn longest_backtick_run(text: &str) -> usize {
    let (mut longest, mut current) = (0, 0);
    for ch in text.chars() {
        current = if ch == '`' { current + 1 } else { 0 };
        longest = longest.max(current);
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::NullAssetSink;
    use crate::{CancelToken, ConvertOptions};
    use std::path::PathBuf;

    fn convert(name: &str, bytes: &[u8]) -> Document {
        let options = ConvertOptions::default();
        let mut assets = NullAssetSink;
        let cancel = CancelToken::new();
        let progress = |_: f32| {};
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        TextConverter
            .convert(
                &Input {
                    path: PathBuf::from(name),
                    bytes: bytes.to_vec(),
                },
                &mut ctx,
            )
            .unwrap()
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(
            convert("a.txt", b"hello, world\n").markdown,
            "hello, world\n"
        );
        assert_eq!(convert("a.md", "# Başlık".as_bytes()).markdown, "# Başlık");
    }

    #[test]
    fn json_is_fenced_with_a_long_enough_fence() {
        assert_eq!(
            convert("a.json", br#"{"a": 1}"#).markdown,
            "```json\n{\"a\": 1}\n```"
        );
        let tricky = convert("a.jsonl", b"{\"s\": \"```\"}\n");
        assert!(tricky.markdown.starts_with("````json\n"));
        assert!(tricky.markdown.ends_with("\n````"));
    }

    #[test]
    fn binary_garbage_never_becomes_the_word_none() {
        let doc = convert("a.txt", &[0xC3, 0x28, 0xA0, 0xA1]);
        assert!(!doc.markdown.contains("None"));
    }
}
