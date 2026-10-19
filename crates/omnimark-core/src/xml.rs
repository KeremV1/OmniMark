//! quick-xml 0.42 helpers. Text arrives split into `Text` and `GeneralRef` events;
//! `append_text` must be used for every character-data event or `&`/`<` are lost.
//! DTDs are never processed: unknown entity references are dropped.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

pub fn reader(xml: &[u8]) -> Reader<&[u8]> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    reader
}

pub fn append_text(out: &mut String, event: &Event<'_>) {
    match event {
        Event::Text(text) => out.push_str(&text.xml10_content()),
        Event::CData(cdata) => out.push_str(&cdata.xml10_content()),
        Event::GeneralRef(reference) => {
            if let Ok(Some(ch)) = reference.resolve_char_ref() {
                out.push(ch);
                return;
            }
            match &*reference.xml10_content() {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                _ => {}
            }
        }
        _ => {}
    }
}

/// Attribute value by qualified name (e.g. `w:val`), with predefined entities resolved.
pub fn attr(element: &BytesStart<'_>, name: &str) -> Option<String> {
    let attribute = element.try_get_attribute(name).ok().flatten()?;
    attribute
        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
        .ok()
        .map(|v| v.into_owned())
}

/// `w:p` → `p`.
pub fn local_name(qualified: &str) -> &str {
    qualified.rsplit(':').next().unwrap_or(qualified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quick_xml::events::Event;

    #[test]
    fn append_text_keeps_entities_and_char_refs() {
        let mut r =
            reader(br#"<w:t xml:space="preserve">A &amp; B &lt;&#65;&#x42;&gt; &unknown;</w:t>"#);
        let mut out = String::new();
        loop {
            let event = r.read_event().unwrap();
            if matches!(event, Event::Eof) {
                break;
            }
            append_text(&mut out, &event);
        }
        assert_eq!(out, "A & B <AB> ");
    }

    #[test]
    fn attr_reads_qualified_attributes() {
        let mut r = reader(br#"<w:pStyle w:val="Heading1" r:id="rId5"/>"#);
        match r.read_event().unwrap() {
            Event::Empty(e) => {
                assert_eq!(attr(&e, "w:val").as_deref(), Some("Heading1"));
                assert_eq!(attr(&e, "r:id").as_deref(), Some("rId5"));
                assert_eq!(attr(&e, "w:missing"), None);
                assert_eq!(local_name(e.name().as_ref()), "pStyle");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
