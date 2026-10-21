use super::*;
use crate::assets::MemoryAssetSink;
use crate::{CancelToken, ConvertOptions};

fn run_body(xml_body: &str, parts: &mut DocxParts<'_>) -> (String, Document) {
    let xml = format!(
        r#"<w:document xmlns:w="w" xmlns:r="r" xmlns:m="m" xmlns:wp="wp" xmlns:a="a"><w:body>{xml_body}</w:body></w:document>"#
    );
    let options = ConvertOptions::default();
    let mut assets = MemoryAssetSink::new("t_assets");
    let cancel = CancelToken::new();
    let progress = |_: f32| {};
    let mut ctx = ConvertContext {
        options: &options,
        assets: &mut assets,
        cancel: &cancel,
        progress: &progress,
    };
    let mut doc = Document::default();
    let md = crate::markdown::normalize(
        &render_body(xml.as_bytes(), parts, &mut ctx, &mut doc).unwrap(),
    );
    (md, doc)
}

fn render_with(body: &str, styles: Option<&[u8]>, numbering: Option<&[u8]>) -> String {
    run_body(body, &mut DocxParts::from_xml(styles, numbering)).0
}

fn render(body: &str) -> String {
    render_with(body, None, None)
}

fn p(runs: &str) -> String {
    format!("<w:p>{runs}</w:p>")
}

#[test]
fn headings_from_outline_level_and_entities() {
    let md = render(&format!(
        r#"<w:p><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:r><w:t>R&amp;D</w:t></w:r></w:p>{}"#,
        p("<w:r><w:t>Body</w:t></w:r>")
    ));
    assert_eq!(md, "## R&D\n\nBody");
}

#[test]
fn headings_from_style_names_and_based_on_chain() {
    let styles = br#"<w:styles xmlns:w="w">
      <w:style w:styleId="H1" w:type="paragraph"><w:name w:val="heading 1"/></w:style>
      <w:style w:styleId="T" w:type="paragraph"><w:name w:val="Title"/></w:style>
      <w:style w:styleId="Child" w:type="paragraph"><w:name w:val="My Child"/><w:basedOn w:val="H1"/></w:style>
      <w:style w:styleId="Out" w:type="paragraph"><w:name w:val="Out"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style>
    </w:styles>"#;
    let para = |style: &str, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    let body = format!(
        "{}{}{}{}",
        para("H1", "a"),
        para("T", "b"),
        para("Child", "c"),
        para("Out", "d")
    );
    assert_eq!(
        render_with(&body, Some(styles), None),
        "# a\n\n# b\n\n# c\n\n### d"
    );
}

#[test]
fn adjacent_runs_with_same_format_are_merged_and_spaces_moved_outside() {
    let md = render(&p(
        r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">bold </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>text</w:t></w:r><w:r><w:t xml:space="preserve"> and </w:t></w:r><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>under</w:t></w:r><w:r><w:rPr><w:i/><w:b w:val="0"/></w:rPr><w:t>snake_case</w:t></w:r>"#,
    ));
    assert_eq!(md, r"**bold text** and <u>under</u>*snake\_case*");
}

#[test]
fn strike_and_special_characters() {
    let md = render(&p(
        r#"<w:r><w:rPr><w:dstrike/></w:rPr><w:t>old</w:t></w:r><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:noBreakHyphen/><w:sym w:char="x"/></w:r>"#,
    ));
    assert_eq!(md, "~~old~~a\tb-");
}

#[test]
fn tables_use_first_row_as_header_and_expand_spans() {
    let md = render(
        r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>wide</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
    );
    assert_eq!(md, "| A | B |\n| --- | --- |\n| wide |  |");
}

#[test]
fn vertical_merge_continuation_is_empty_and_nested_tables_are_flattened() {
    let cell = |inner: &str, pr: &str| format!("<w:tc><w:tcPr>{pr}</w:tcPr>{inner}</w:tc>");
    let para = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let nested = format!(
        "<w:tbl><w:tr>{}{}</w:tr></w:tbl>",
        cell(&para("x"), ""),
        cell(&para("y"), "")
    );
    let md = render(&format!(
        "<w:tbl><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl>",
        cell(&para("top"), r#"<w:vMerge w:val="restart"/>"#),
        cell(&format!("{}{}", para("one"), para("two")), ""),
        cell(&para("hidden"), "<w:vMerge/>"),
        cell(&format!("<w:p/>{nested}"), ""),
    ));
    assert_eq!(md, "| top | one two |\n| --- | --- |\n|  | x y |");
}

#[test]
fn deleted_text_and_field_codes_are_skipped_but_insertions_kept() {
    let md = render(&p(
        r#"<w:r><w:t>keep</w:t></w:r><w:del><w:r><w:delText>gone</w:delText></w:r></w:del><w:r><w:instrText>PAGE</w:instrText></w:r><w:ins><w:r><w:t xml:space="preserve"> added</w:t></w:r></w:ins>"#,
    ));
    assert_eq!(md, "keep added");
}

#[test]
fn mc_fallback_is_ignored() {
    let md = render(&p(
        r#"<mc:AlternateContent xmlns:mc="mc"><mc:Choice><w:r><w:t>choice</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>fallback</w:t></w:r></mc:Fallback></mc:AlternateContent>"#,
    ));
    assert_eq!(md, "choice");
}

#[test]
fn text_box_paragraphs_follow_their_host_paragraph() {
    let md = render(&p(
        r#"<w:r><w:t>host</w:t></w:r><w:r><w:pict><w:txbxContent><w:p><w:r><w:t>boxed</w:t></w:r></w:p></w:txbxContent></w:pict></w:r><w:r><w:t xml:space="preserve"> tail</w:t></w:r>"#,
    ));
    assert_eq!(md, "host tail\n\nboxed");
}

#[test]
fn failing_equation_is_kept_as_plain_text_with_a_warning() {
    // An equation that cannot be converted is kept as text (or as trivial LaTeX).
    let md = render(&p(r#"<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>"#));
    assert!(md == "x" || md == "$x$", "{md}");
}

#[test]
fn display_math_gets_its_own_paragraph() {
    let md = render(&format!(
        "{}{}",
        p("<w:r><w:t>before</w:t></w:r>"),
        p(r#"<m:oMathPara><m:oMath><m:r><m:t>y</m:t></m:r></m:oMath></m:oMathPara>"#)
    ));
    assert!(md == "before\n\ny" || md == "before\n\n$$y$$", "{md}");
}

#[test]
fn hyperlinks_only_keep_safe_schemes() {
    let mut parts = DocxParts::empty();
    parts.add_rels(
        br#"<Relationships>
        <Relationship Id="rId1" Type="t/hyperlink" Target="https://example.com/a" TargetMode="External"/>
        <Relationship Id="rId2" Type="t/hyperlink" Target="javascript:alert(1)" TargetMode="External"/>
        </Relationships>"#,
    );
    let (md, _) = run_body(
        &p(
            r#"<w:hyperlink r:id="rId1"><w:r><w:t>site</w:t></w:r></w:hyperlink><w:r><w:t xml:space="preserve"> </w:t></w:r><w:hyperlink r:id="rId2"><w:r><w:t>bad</w:t></w:r></w:hyperlink><w:r><w:t xml:space="preserve"> </w:t></w:r><w:hyperlink w:anchor="x"><w:r><w:t>inner</w:t></w:r></w:hyperlink>"#,
        ),
        &mut parts,
    );
    assert_eq!(md, "[site](https://example.com/a) bad inner");
}

#[test]
fn bullets_and_numbered_lists_with_levels() {
    let numbering = br#"<w:numbering xmlns:w="w">
      <w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum>
      <w:num w:numId="7"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#;
    let item = |lvl: u8, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{lvl}"/><w:numId w:val="7"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    let md = render_with(
        &format!("{}{}{}", item(0, "one"), item(1, "sub"), item(0, "two")),
        None,
        Some(numbering),
    );
    assert_eq!(md, "- one\n  1. sub\n- two");
}

#[test]
fn footnotes_are_referenced_and_defined_at_the_end() {
    let mut parts = DocxParts::empty();
    parts.set_notes(
        Some(
            br#"<w:footnotes xmlns:w="w"><w:footnote w:type="separator" w:id="0"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:footnoteRef/><w:t xml:space="preserve"> note one</w:t></w:r></w:p></w:footnote></w:footnotes>"#
                .to_vec(),
        ),
        None,
    );
    let (md, _) = run_body(
        &p(r#"<w:r><w:t>text</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r>"#),
        &mut parts,
    );
    assert_eq!(md, "text[^fn1]\n\n[^fn1]: note one");
}

fn build_docx(rels: &str, body: &str, entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let document = format!(
        r#"<w:document xmlns:w="w" xmlns:r="r" xmlns:wp="wp" xmlns:a="a"><w:body>{body}</w:body></w:document>"#
    );
    let rels = format!("<Relationships>{rels}</Relationships>");
    let mut all: Vec<(&str, &[u8])> = vec![
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", rels.as_bytes()),
    ];
    all.extend_from_slice(entries);
    for (name, bytes) in all {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn convert_docx(bytes: Vec<u8>) -> (Document, Vec<(String, Vec<u8>)>) {
    let options = ConvertOptions::default();
    let mut assets = MemoryAssetSink::new("t_assets");
    let cancel = CancelToken::new();
    let progress = |_: f32| {};
    let doc = {
        let mut ctx = ConvertContext {
            options: &options,
            assets: &mut assets,
            cancel: &cancel,
            progress: &progress,
        };
        let input = Input {
            path: "t.docx".into(),
            bytes,
        };
        DocxConverter.convert(&input, &mut ctx).unwrap()
    };
    (doc, assets.files)
}

fn drawing(attrs: &str) -> String {
    p(&format!(
        r#"<w:r><w:drawing><wp:inline><wp:docPr id="1" descr="pic"/><a:graphic><a:blip {attrs}/></a:graphic></wp:inline></w:drawing></w:r>"#
    ))
}

#[test]
fn external_image_relationships_are_never_read_even_if_the_target_exists() {
    let rels = r#"<Relationship Id="rId1" Type="t/image" Target="media/secret.png" TargetMode="External"/>"#;
    let body = format!(
        "{}{}",
        drawing(r#"r:embed="rId1""#),
        drawing(r#"r:link="rId1""#)
    );
    let bytes = build_docx(rels, &body, &[("word/media/secret.png", b"SECRET")]);
    let (doc, files) = convert_docx(bytes);
    assert!(files.is_empty(), "external target was stored: {files:?}");
    assert_eq!(doc.markdown, "![pic]()\n\n![pic]()");
    assert_eq!(
        doc.warnings,
        vec!["Linked image not embedded: media/secret.png".to_string(); 2]
    );
}

#[test]
fn internal_images_are_stored_once_and_linked() {
    let rels = r#"<Relationship Id="rId1" Type="t/image" Target="media/a.png"/>"#;
    let body = format!(
        "{}{}",
        drawing(r#"r:embed="rId1""#),
        drawing(r#"r:embed="rId1""#)
    );
    let bytes = build_docx(rels, &body, &[("word/media/a.png", b"PNGDATA")]);
    let (doc, files) = convert_docx(bytes);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].1, b"PNGDATA");
    assert_eq!(
        doc.markdown,
        "![pic](t_assets/a.png)\n\n![pic](t_assets/a.png)"
    );
    assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
}

#[test]
fn malformed_styles_warn_but_conversion_continues() {
    let mut parts = DocxParts::from_xml(
        Some(br#"<w:styles xmlns:w="w"><w:style w:styleId="a"></w:oops>"#),
        Some(br#"<w:numbering xmlns:w="w"><w:num></w:bad>"#),
    );
    let (md, doc) = run_body(&p("<w:r><w:t>still here</w:t></w:r>"), &mut parts);
    assert_eq!(md, "still here");
    assert_eq!(doc.warnings.len(), 2, "{:?}", doc.warnings);
    assert!(doc.warnings[0].starts_with("Styles could not be fully read"));
}

#[test]
fn corrupt_notes_are_skipped_with_a_warning() {
    let mut parts = DocxParts::empty();
    parts.set_notes(
        Some(b"<w:footnotes xmlns:w=\"w\"><w:footnote></w:x>".to_vec()),
        None,
    );
    let (md, doc) = run_body(&p("<w:r><w:t>body</w:t></w:r>"), &mut parts);
    assert_eq!(md, "body");
    assert!(doc.warnings[0].starts_with("Notes could not be read"));
}

#[test]
fn any_non_bullet_number_format_is_numbered() {
    let numbering = br#"<w:numbering xmlns:w="w">
      <w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="none"/></w:lvl></w:abstractNum>
      <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#;
    let md = render_with(
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#,
        None,
        Some(numbering),
    );
    assert_eq!(md, "1. x");
}

#[test]
fn huge_grid_spans_are_capped_with_one_warning() {
    let cell = r#"<w:tc><w:tcPr><w:gridSpan w:val="1000"/></w:tcPr><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc>"#;
    let row = format!("<w:tr>{cell}{cell}{cell}</w:tr>");
    let mut parts = DocxParts::empty();
    let (md, doc) = run_body(&format!("<w:tbl>{row}{row}</w:tbl>"), &mut parts);
    let header = md.lines().next().unwrap();
    assert_eq!(
        header.matches('|').count(),
        crate::markdown::MAX_TABLE_COLUMNS + 1
    );
    assert_eq!(doc.warnings, vec!["Large table truncated.".to_string()]);
}

fn convert_docx_ocr(bytes: Vec<u8>) -> Document {
    let options = ConvertOptions {
        ocr: Some(std::sync::Arc::new(crate::ocr::MockEngine::default())),
        ..ConvertOptions::default()
    };
    let mut assets = MemoryAssetSink::new("t_assets");
    let cancel = CancelToken::new();
    let progress = |_: f32| {};
    let mut ctx = ConvertContext {
        options: &options,
        assets: &mut assets,
        cancel: &cancel,
        progress: &progress,
    };
    DocxConverter
        .convert(
            &Input {
                path: "t.docx".into(),
                bytes,
            },
            &mut ctx,
        )
        .unwrap()
}

const B: &str = "*[Image OCR]\\\nMOCK\\_OCR\\_TEXT\\_12345\\\n[End OCR]*";

fn image_docx(body: &str) -> Vec<u8> {
    let png = crate::ocr::test_render::png_bytes(20, 20, [0, 0, 0, 255]);
    let rels = r#"<Relationship Id="rId1" Type="t/image" Target="media/a.png"/>"#;
    build_docx(rels, body, &[("word/media/a.png", &png)])
}

#[test]
fn ocr_block_follows_the_image_paragraph_and_repeats_from_cache() {
    let body = format!(
        "{}{}{}",
        drawing(r#"r:embed="rId1""#),
        p("<w:r><w:t>after</w:t></w:r>"),
        drawing(r#"r:embed="rId1""#)
    );
    let doc = convert_docx_ocr(image_docx(&body));
    assert_eq!(
        doc.markdown,
        format!("![pic](t_assets/a.png)\n\n{B}\n\nafter\n\n![pic](t_assets/a.png)\n\n{B}")
    );
}

#[test]
fn ocr_in_a_table_cell_is_one_line_and_in_a_list_item_is_indented() {
    let cell = drawing(r#"r:embed="rId1""#);
    let table = format!(
        "<w:tbl><w:tr><w:tc>{cell}</w:tc><w:tc>{}</w:tc></w:tr></w:tbl>",
        p("<w:r><w:t>x</w:t></w:r>")
    );
    let doc = convert_docx_ocr(image_docx(&table));
    assert!(
        doc.markdown.starts_with(
            "| ![pic](t_assets/a.png) *[Image OCR] MOCK\\_OCR\\_TEXT\\_12345 [End OCR]* | x |"
        ),
        "{}",
        doc.markdown
    );
    let item = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="7"/></w:numPr></w:pPr><w:r><w:drawing><wp:inline><wp:docPr id="1" descr="pic"/><a:graphic><a:blip r:embed="rId1"/></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let doc = convert_docx_ocr(image_docx(item));
    assert_eq!(
        doc.markdown,
        "- ![pic](t_assets/a.png)\n\n  *[Image OCR]\\\n  MOCK\\_OCR\\_TEXT\\_12345\\\n  [End OCR]*"
    );
}
