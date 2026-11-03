// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pages documents to the model, from small indexes written by hand in the
//! form that Pages 4.3 saves (DOC-22). The corpus checks whole documents
//! saved by Pages itself, in desk-testkit.

use desk_filters::log::{Content, Formatting, Warning};
use desk_filters::{read_pages, read_pages_logged};
use desk_model::{
    Alignment, Baseline, BorderSides, BorderStyle, Capitalization, CharFormat, Document,
    LINE_BREAK, Length, Ligatures, LineSpacing, LineStyle, ParaFormat, Rgba, Section, SectionStart,
    Start, StyleFamily,
};

/// An index with a stylesheet and a body, in the namespaces of Pages.
fn index(styles: &str, anon: &str, body: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<sl:document xmlns:sf="http://developer.apple.com/namespaces/sf" xmlns:sfa="http://developer.apple.com/namespaces/sfa" xmlns:sl="http://developer.apple.com/namespaces/sl" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<sf:metadata><sf:title><sf:string sfa:string="Relatório"/></sf:title><sf:authors><sf:array><sf:string sfa:string="Brahm"/></sf:array></sf:authors><sf:keywords><sf:array><sf:string sfa:string="desk"/><sf:string sfa:string="pages"/></sf:array></sf:keywords><sf:comment><sf:string sfa:string="Notas"/></sf:comment></sf:metadata>
<sl:slprint-info sl:page-width="595" sl:page-height="842"><sf:page-margins sf:top="36" sf:left="54" sf:bottom="36" sf:right="54"/></sl:slprint-info>
<sl:stylesheet sfa:ID="SFSStylesheet-2"><sf:styles>{styles}</sf:styles><sf:anon-styles>{anon}</sf:anon-styles></sl:stylesheet>
<sf:text-storage sf:kind="body" sfa:ID="SFWPStorage-8"><sf:stylesheet-ref sfa:IDREF="SFSStylesheet-2"/><sf:text-body><sf:page-start sf:page-index="0"/><sf:section sf:name="Text Page"><sf:layout>{body}</sf:layout></sf:section><sf:insertion-point/></sf:text-body></sf:text-storage>
</sl:document>"#
    )
}

fn number(name: &str, value: &str, kind: &str) -> String {
    format!(r#"<sf:{name}><sf:number sfa:number="{value}" sfa:type="{kind}"/></sf:{name}>"#)
}

const STYLES: &str = r#"
<sf:paragraphstyle sf:name="Body" sf:ident="paragraph-style-32" sfa:ID="P-1"><sf:property-map><sf:fontName><sf:string sfa:string="LiberationSerif"/></sf:fontName><sf:spaceAfter><sf:number sfa:number="12" sfa:type="f"/></sf:spaceAfter><sf:leftIndent/><sf:language><sf:string sfa:string="pt_BR"/></sf:language></sf:property-map></sf:paragraphstyle>
<sf:paragraphstyle sf:name="Title" sf:ident="paragraph-style-38" sfa:ID="P-2"><sf:property-map><sf:fontSize><sf:number sfa:number="24" sfa:type="f"/></sf:fontSize><sf:alignment><sf:number sfa:number="2" sfa:type="i"/></sf:alignment></sf:property-map></sf:paragraphstyle>
<sf:paragraphstyle sf:name="Quote" sf:ident="paragraph-style-90" sf:parent-ident="paragraph-style-32" sfa:ID="P-3"><sf:property-map><sf:leftIndent><sf:number sfa:number="36" sfa:type="f"/></sf:leftIndent><sf:italic><sf:number sfa:number="1" sfa:type="c"/></sf:italic></sf:property-map></sf:paragraphstyle>
<sf:characterstyle sf:name="None" sf:ident="character-style-null" sfa:ID="C-0"><sf:property-map/></sf:characterstyle>
<sf:characterstyle sf:name="Emphasis" sf:ident="character-style-1" sf:parent-ident="character-style-null" sfa:ID="C-1"><sf:property-map><sf:bold><sf:number sfa:number="1" sfa:type="c"/></sf:bold></sf:property-map></sf:characterstyle>
<sf:paragraphstyle-ref sfa:IDREF="P-2"/>
"#;

fn anon() -> String {
    [
        format!(
            r#"<sf:paragraphstyle sf:parent-ident="paragraph-style-32" sfa:ID="A-1"><sf:property-map>{}{}<sf:lineSpacing><sf:linespacing sf:amt="1.5" sf:mode="relative"/></sf:lineSpacing>{}</sf:property-map></sf:paragraphstyle>"#,
            number("firstLineIndent", "72", "f"),
            number("leftIndent", "36", "f"),
            number("capitalization", "2", "i"),
        ),
        format!(
            r#"<sf:paragraphstyle sf:parent-ident="paragraph-style-90" sfa:ID="A-2"><sf:property-map><sf:lineSpacing><sf:linespacing sf:amt="18" sf:mode="absolute"/></sf:lineSpacing>{}</sf:property-map></sf:paragraphstyle>"#,
            number("widowControl", "0", "c"),
        ),
        format!(
            r#"<sf:characterstyle sf:parent-ident="character-style-null" sfa:ID="A-3"><sf:property-map>{}{}<sf:fontColor><sf:color xsi:type="sfa:calibrated-rgb-color-type" sfa:r="1" sfa:g="0" sfa:b="0" sfa:a="1"/></sf:fontColor><sf:fontName><sf:string sfa:string="LiberationSans-Italic"/></sf:fontName></sf:property-map></sf:characterstyle>"#,
            number("superscript", "1", "i"),
            number("underline", "2", "i"),
        ),
        format!(
            r#"<sf:characterstyle sf:parent-ident="character-style-1" sfa:ID="A-4"><sf:property-map>{}</sf:property-map></sf:characterstyle>"#,
            number("strikethru", "1", "i"),
        ),
    ]
    .concat()
}

const BODY: &str = r#"
<sf:p sf:style="paragraph-style-38">Um título<sf:crbr/></sf:p>
<sf:p sf:style="A-1">Texto <sf:span sf:style="A-3">sobrescrito</sf:span> e <sf:span sf:style="character-style-1">ênfase</sf:span><sf:lnbr/>depois<sf:tab/>fim<sf:crbr/></sf:p>
<sf:container-hint sf:page-index="0"/>
<sf:p sf:style="A-2">Antes<sf:pgbr/>depois da quebra <sf:span sf:style="A-4">riscado</sf:span></sf:p>
"#;

fn document() -> Document {
    read_pages(index(STYLES, &anon(), BODY).as_bytes()).unwrap()
}

#[test]
fn doc_22_named_styles_become_styles_and_body_the_default() {
    let document = document();
    let styles = document.styles();
    let body = styles.paragraph(styles.default_paragraph()).unwrap();
    assert_eq!(&*body.name, "Body");
    assert_eq!(
        body.character.font_family.as_deref(),
        Some("Liberation Serif")
    );
    // Pages has widow control on when nothing sets it.
    assert_eq!(body.paragraph.widow_control, Some(true));
    let title = styles.by_name(StyleFamily::Paragraph, "Title").unwrap();
    let quote = styles.by_name(StyleFamily::Paragraph, "Quote").unwrap();
    assert_eq!(quote.parent(), Some(styles.default_paragraph()));
    assert!(styles.by_name(StyleFamily::Character, "Emphasis").is_some());
    // The style that Pages lists twice is imported once.
    assert_eq!(styles.iter().filter(|s| s.name() == "Title").count(), 1);
    // A style without a parent does not take the font of Body.
    let title = styles.resolve_paragraph(title.id(), &Default::default());
    assert_eq!(title.alignment, Alignment::Center);
    let first = document.body().get(0).unwrap();
    let run = first.runs().next().unwrap();
    let format = styles.resolve_character(first.style(), run.style, &run.attrs);
    assert_eq!(&*format.font_family, "Helvetica");
    assert_eq!(format.font_size, Length::from_points(24));
}

#[test]
fn doc_22_fmt_anonymous_styles_become_direct_formatting() {
    let document = document();
    let styles = document.styles();
    let paragraph = document.body().get(1).unwrap();
    assert_eq!(paragraph.style(), styles.default_paragraph());
    let format = styles.resolve_paragraph(paragraph.style(), paragraph.attrs());
    assert_eq!(format.line_spacing, LineSpacing::Multiple { percent: 150 });
    assert_eq!(format.space_after, Length::from_points(12));
    // Pages measures the first line from the margin, the model from the
    // left indent.
    assert_eq!(format.left_indent, Length::from_points(36));
    assert_eq!(format.first_line_indent, Length::from_points(36));

    let runs: Vec<_> = paragraph.runs().collect();
    let texts: Vec<&str> = runs.iter().map(|r| r.text()).collect();
    assert_eq!(
        texts,
        [
            "Texto ",
            "sobrescrito",
            " e ",
            "ênfase",
            &format!("{LINE_BREAK}depois\tfim")
        ]
    );
    let character =
        |i: usize| styles.resolve_character(paragraph.style(), runs[i].style, &runs[i].attrs);
    // Character properties of the paragraph's anonymous style reach every
    // run.
    assert_eq!(character(0).capitalization, Capitalization::SmallCaps);
    assert_eq!(character(0).language.tag(), "pt-BR");
    let sup = character(1);
    assert_eq!(sup.baseline, Baseline::Superscript);
    assert_eq!(sup.underline, LineStyle::Double);
    assert_eq!(sup.color, Rgba::rgb(255, 0, 0));
    assert_eq!(&*sup.font_family, "Liberation Sans");
    assert!(sup.italic);
    assert!(character(3).weight.is_bold());
    assert!(runs[3].style.is_some());
}

#[test]
fn doc_22_est_04_page_breaks_split_paragraphs() {
    let document = document();
    let styles = document.styles();
    assert_eq!(document.body().len(), 4);
    let before = document.body().get(2).unwrap();
    let after = document.body().get(3).unwrap();
    assert_eq!(before.runs().next().unwrap().text(), "Antes");
    let format = styles.resolve_paragraph(after.style(), after.attrs());
    assert!(format.page_break_before);
    assert_eq!(
        format.line_spacing,
        LineSpacing::Exactly(Length::from_points(18))
    );
    assert!(!format.widow_control);
    // Quote, a named style over Body.
    assert_eq!(format.left_indent, Length::from_points(36));
    let runs: Vec<_> = after.runs().collect();
    let riscado = styles.resolve_character(after.style(), runs[1].style, &runs[1].attrs);
    assert_eq!(riscado.strikethrough, LineStyle::Single);
    // The named style under the anonymous one.
    assert!(riscado.weight.is_bold());
    assert!(riscado.italic);
}

#[test]
fn doc_19_est_02_est_03_information_and_page_are_read() {
    let document = document();
    let meta = document.metadata();
    assert_eq!(meta.title, "Relatório");
    assert_eq!(meta.author, "Brahm");
    assert_eq!(meta.keywords, ["desk", "pages"]);
    assert_eq!(meta.comments, "Notas");
    let page = document.page_setup();
    assert_eq!(page.width, Length::from_points(595));
    assert_eq!(page.margin_left, Length::from_points(54));
    assert_eq!(page.margin_top, Length::from_points(36));
}

#[test]
fn doc_22_broken_references_and_empty_documents_still_open() {
    let body = r#"<sf:p sf:style="missing">Sem estilo <sf:span sf:style="missing-too">nem aqui</sf:span></sf:p>"#;
    let document = read_pages(index("", "", body).as_bytes()).unwrap();
    assert_eq!(document.body().len(), 1);
    let styles = document.styles();
    assert_eq!(
        &*styles.paragraph(styles.default_paragraph()).unwrap().name,
        "Body"
    );
    let empty = read_pages(index("", "", "").as_bytes()).unwrap();
    assert_eq!(empty.body().len(), 1);
}

#[test]
fn doc_22_a_cycle_of_parents_ends() {
    let styles = r#"
<sf:paragraphstyle sf:name="A" sf:ident="a" sf:parent-ident="b"><sf:property-map/></sf:paragraphstyle>
<sf:paragraphstyle sf:name="B" sf:ident="b" sf:parent-ident="a"><sf:property-map/></sf:paragraphstyle>"#;
    let document =
        read_pages(index(styles, "", r#"<sf:p sf:style="a">x</sf:p>"#).as_bytes()).unwrap();
    assert!(
        document
            .styles()
            .by_name(StyleFamily::Paragraph, "A")
            .is_some()
    );
}

#[test]
fn doc_22_what_the_model_cannot_hold_is_logged() {
    let lists = r#"<sf:liststyle sf:name="Bullet" sf:ident="list-style-1" sfa:ID="L-1"><sf:property-map><sf:listLabelTypes><sf:array><sf:list-label-typeinfo sf:type="text"/></sf:array></sf:listLabelTypes></sf:property-map></sf:liststyle>
<sf:liststyle sf:name="None" sf:ident="list-style-default" sfa:ID="L-0"><sf:property-map><sf:listLabelTypes><sf:array><sf:list-label-typeinfo sf:type="none"/></sf:array></sf:listLabelTypes></sf:property-map></sf:liststyle>"#;
    let anon = format!(
        r#"<sf:paragraphstyle sf:parent-ident="paragraph-style-32" sfa:ID="A-1"><sf:property-map><sf:listStyle><sf:liststyle-ref sfa:IDREF="L-1"/></sf:listStyle>{}</sf:property-map></sf:paragraphstyle>
<sf:paragraphstyle sf:parent-ident="paragraph-style-32" sfa:ID="A-2"><sf:property-map><sf:listStyle><sf:liststyle-ref sfa:IDREF="L-0"/></sf:listStyle>{}{}<sf:textShadow><sf:null/></sf:textShadow></sf:property-map></sf:paragraphstyle>
<sf:characterstyle sf:parent-ident="character-style-null" sfa:ID="A-3"><sf:property-map>{}</sf:property-map></sf:characterstyle>"#,
        number("dropCap", "1", "i"),
        number("dropCap", "0", "i"),
        number("ligatures", "1", "i"),
        number("dropCap", "1", "i"),
    );
    let body = r#"<sf:p sf:style="A-1">Um item<sf:crbr/></sf:p>
<sf:p sf:style="A-2">Nada perdido aqui<sf:crbr/></sf:p>
<sf:p sf:style="A-2">Mas <sf:span sf:style="A-3">aqui</sf:span>, e uma imagem <sf:attachment-ref sfa:IDREF="X-1"/><sf:attachment-ref sfa:IDREF="X-2"/></sf:p>"#;
    let outside = r#"<sf:headers><sf:header sf:name="Odd"><sf:text-storage sf:kind="header"><sf:text-body><sf:p>Cabeçalho</sf:p></sf:text-body></sf:text-storage></sf:header><sf:header sf:name="Even"><sf:text-storage sf:kind="header"><sf:text-body><sf:p sf:style="x"> </sf:p></sf:text-body></sf:text-storage></sf:header></sf:headers>
<sl:drawables><sl:masters-group><sl:section-drawables><sf:style/><sl:foregroundmaster-group/><sl:backgroundmaster-group/></sl:section-drawables></sl:masters-group><sl:page-group><sf:drawable-shape/><sf:image/></sl:page-group></sl:drawables>
"#;
    let source = index(&format!("{STYLES}{lists}"), &anon, body).replace(
        "<sf:text-storage sf:kind=\"body\"",
        &format!("{outside}<sf:text-storage sf:kind=\"body\""),
    );
    let (document, log) = read_pages_logged(source.as_bytes()).unwrap();
    assert_eq!(document.body().len(), 3);
    let entries: Vec<(Warning, usize)> = log
        .entries()
        .iter()
        .map(|e| (e.warning.clone(), e.count))
        .collect();
    assert_eq!(
        entries,
        [
            (Warning::Content(Content::FloatingObjects), 2),
            (Warning::Content(Content::InlineObjects), 2),
            (Warning::Formatting(Formatting::DropCaps), 2),
        ]
    );
    assert_eq!(
        log.entries()[2].to_string(),
        "Left out drop caps, in 2 paragraphs"
    );
    // The documents of the other tests lose nothing.
    let (_, log) = read_pages_logged(index(STYLES, "", BODY).as_bytes()).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
}

#[test]
fn doc_22_a_document_saved_by_pages_logs_what_it_loses() {
    // Saved by Pages '09 on the reference machine, with one of each
    // thing that the model cannot hold yet.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/pages/lossy-import.pages");
    let (document, log) = desk_filters::read_pages_at_logged(&path).unwrap();
    let entries: Vec<(Warning, usize)> = log
        .entries()
        .iter()
        .map(|e| (e.warning.clone(), e.count))
        .collect();
    assert_eq!(
        entries,
        [
            (Warning::Content(Content::FloatingObjects), 1),
            (Warning::Content(Content::InlineObjects), 1),
            (Warning::Content(Content::Comments), 1),
        ]
    );
    // The text of the body is kept, without that of the footnote, which
    // is a note of its own (EST-13), and the comment.
    let text: Vec<String> = document.body().iter().map(|p| p.text()).collect();
    assert!(
        text.iter()
            .any(|t| t.starts_with("Footnote on this paragraph"))
    );
    assert!(text.iter().any(|t| t == "Bulleted item one"));
    assert!(
        !text
            .iter()
            .any(|t| t.contains("Footnote text") || t.contains("Comment text"))
    );
    let notes: Vec<String> = document.notes().paragraphs().map(|p| p.text()).collect();
    assert!(
        notes.iter().any(|t| t.contains("Footnote text")),
        "{notes:?}"
    );
}

#[test]
fn est_11_txt_09_facing_pages_and_hyphenation_are_kept() {
    let anon = format!(
        r#"<sf:paragraphstyle sf:parent-ident="paragraph-style-32" sfa:ID="A-1"><sf:property-map>{}</sf:property-map></sf:paragraphstyle>"#,
        number("hyphenate", "0", "c"),
    );
    let body = r#"<sf:p sf:style="paragraph-style-32">Hifenizado<sf:crbr/></sf:p>
<sf:p sf:style="A-1">Sem hifenização<sf:crbr/></sf:p>
<sf:p sf:style="paragraph-style-90">Hifenizado também</sf:p>"#;
    let plain = index(STYLES, &anon, body);
    let hyphenated_paragraphs = |document: &desk_model::Document| -> Vec<bool> {
        document
            .body()
            .iter()
            .map(|p| {
                document
                    .styles()
                    .resolve_paragraph(p.style(), p.attrs())
                    .hyphenate
            })
            .collect()
    };
    let (document, log) = read_pages_logged(plain.as_bytes()).unwrap();
    // Without automatic hyphenation, Pages breaks words only at soft
    // hyphens: the document is not hyphenated, and its paragraphs keep
    // what they allow.
    assert!(log.is_empty(), "{:?}", log.entries());
    assert!(!document.text_settings().hyphenation);
    assert!(document.text_settings().ligatures);
    assert_eq!(hyphenated_paragraphs(&document), [true, false, true]);
    let setting = |on: &str| {
        format!(
            r#"<sl:publication-info>{}</sl:publication-info><sl:slprint-info"#,
            number("kSFWPAutoHyphenationProperty", on, "c").replace("sf:", "sl:")
        )
    };
    let hyphenated = plain.replace("<sl:slprint-info", &setting("1")).replace(
        r#"sf:right="54"/>"#,
        r#"sf:right="54" sf:facing-pages="true"/>"#,
    );
    let (document, log) = read_pages_logged(hyphenated.as_bytes()).unwrap();
    // A style that turns it off keeps its paragraphs whole.
    assert!(document.text_settings().hyphenation);
    assert_eq!(hyphenated_paragraphs(&document), [true, false, true]);
    assert!(document.page_setup().facing_pages);
    assert!(log.is_empty(), "{:?}", log.entries());
    let off = plain.replace("<sl:slprint-info", &setting("0"));
    assert!(read_pages_logged(off.as_bytes()).unwrap().1.is_empty());
}

#[test]
fn est_11_facing_pages_saved_by_pages_are_kept() {
    let document = desk_filters::read_pages_at(&fixture("est-11-facing-pages.pages")).unwrap();
    let setup = document.page_setup();
    assert!(setup.facing_pages);
    assert_eq!(setup.margin_left, Length::from_points(100));
    assert_eq!(setup.margin_right, Length::from_points(40));
}

#[test]
fn est_07_est_08_columns_and_column_breaks_saved_by_pages_are_read() {
    let document = desk_filters::read_pages_at(&fixture("est-07-08-columns.pages")).unwrap();
    let columns = document.page_setup().layout.columns;
    assert!(columns.count > 1, "{columns:?}");
    let formats = fixture_formats("est-07-08-columns.pages");
    // The paragraph after the one that ends with the column break starts
    // the next column.
    let broken: Vec<bool> = formats.iter().map(|f| f.1.column_break_before).collect();
    assert_eq!(broken, [false, false, true, false]);
    assert!(formats.iter().all(|f| f.1.start.is_none()));
}

#[test]
fn est_09_est_10_layouts_saved_by_pages_are_read() {
    let formats = fixture_formats("est-09-10-layouts.pages");
    let starts: Vec<Option<Start>> = formats
        .iter()
        .map(|f| f.1.start.as_deref().copied())
        .collect();
    assert_eq!(starts[0], None);
    let Some(Start::Layout(layout)) = starts[1] else {
        panic!("{starts:?}");
    };
    assert_eq!(layout.columns.count, 3);
    assert!(layout.columns.equal);
    let spans = layout.columns.spans(100.0);
    assert!((spans[1].0 - 35.0).abs() < 0.01, "{spans:?}");
    assert_eq!(layout.before, Length::from_points(12));
    assert_eq!(layout.after, Length::from_points(24));
    assert_eq!(layout.left, Length::from_points(20));
    assert_eq!(layout.right, Length::from_points(30));
    assert!(!layout.new_page);
    assert_eq!(starts[2], None);
}

#[test]
fn est_16_est_18_sections_saved_by_pages_are_read() {
    let document = desk_filters::read_pages_at(&fixture("est-16-18-sections.pages")).unwrap();
    assert_eq!(document.page_setup().section, Section::default());
    let formats = fixture_formats("est-16-18-sections.pages");
    assert_eq!(formats[0].0, "First section.");
    let Some(Start::Section(section, _)) = formats[1].1.start.as_deref().copied() else {
        panic!("{:?}", formats[1].1.start);
    };
    assert_eq!(section.first_number, Some(5));
    assert_eq!(section.start, SectionStart::AnyPage);
    // The break of the section is not a paragraph of its own.
    assert!(formats.iter().all(|f| !f.1.page_break_before));
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/pages")
        .join(name)
}

#[test]
fn doc_22_every_document_saved_by_pages_opens_and_names_what_it_loses() {
    let dir = fixture("");
    let mut opened = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let extension = path.extension().and_then(|e| e.to_str());
        if !matches!(extension, Some("pages" | "template")) {
            continue;
        }
        let (_, log) = desk_filters::read_pages_at_logged(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        // Every kind of content that Pages saves has a name in the log.
        for entry in log.entries() {
            assert!(
                !matches!(entry.warning, Warning::Content(Content::Other(_))),
                "{}: {entry}",
                path.display()
            );
        }
        opened += 1;
    }
    assert!(opened > 100, "{opened}");
}

#[test]
fn doc_22_tracked_changes_and_bookmarks_keep_their_text() {
    let text = |name: &str| -> (Vec<String>, desk_filters::ImportLog) {
        let (document, log) = desk_filters::read_pages_at_logged(&fixture(name)).unwrap();
        (document.body().iter().map(|p| p.text()).collect(), log)
    };
    // Pages shows the inserted and the deleted text, with its markup.
    let (body, log) = text("rev-01-04-tracked-changes.pages");
    assert!(
        body.contains(&"Original text  trackedwith an insertion here".to_owned()),
        "{body:?}"
    );
    assert!(
        body.contains(&"Text with a deletion here".to_owned()),
        "{body:?}"
    );
    assert!(
        log.entries()
            .iter()
            .any(|e| e.warning == Warning::Content(Content::TrackedChanges))
    );
    let (body, _) = text("est-25-26-links-bookmarks.pages");
    assert!(
        body.contains(&"Bookmarked text here".to_owned()),
        "{body:?}"
    );
}

/// The formats of the paragraphs of a fixture saved by Pages, with those
/// of their first runs.
fn fixture_formats(name: &str) -> Vec<(String, ParaFormat, CharFormat)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/pages")
        .join(name);
    let document = desk_filters::read_pages_at(&path).unwrap();
    let styles = document.styles();
    document
        .body()
        .iter()
        .map(|p| {
            let (style, attrs) = p.format_at(0);
            (
                p.text(),
                styles.resolve_paragraph(p.style(), p.attrs()),
                styles.resolve_character(p.style(), style, &attrs),
            )
        })
        .collect()
}

#[test]
fn fmt_03_fmt_04_fmt_09_fmt_10_effects_of_characters_are_read() {
    let formats = fixture_formats("fmt-10-tracking.pages");
    assert_eq!(formats[0].2.tracking, 200);
    assert_eq!(formats[1].2.tracking, -50);
    let formats = fixture_formats("fmt-09-ligatures.pages");
    assert_eq!(formats[0].2.ligatures, Ligatures::All);
    assert_eq!(formats[1].2.ligatures, Ligatures::None);
    let formats = fixture_formats("fmt-03-04-shadow-outline.pages");
    let shadow = formats[0].2.shadow.unwrap();
    assert_eq!((shadow.angle, shadow.opacity), (300, 60));
    assert_eq!(shadow.offset, Length::from_points(4));
    assert_eq!(shadow.blur, Length::from_points(3));
    assert!(!formats[0].2.outline);
    assert!(formats[1].2.outline && formats[1].2.shadow.is_none());
}

#[test]
fn fmt_20_fmt_21_backgrounds_and_borders_are_read() {
    let formats = fixture_formats("fmt-20-backgrounds.pages");
    let yellow = Rgba::rgb(255, 255, 117);
    assert_eq!(formats[0].2.background, Some(yellow));
    assert_eq!(formats[0].1.background, None);
    assert_eq!(formats[1].1.background, Some(yellow));
    let formats = fixture_formats("fmt-21-borders-rules.pages");
    let sides: Vec<BorderSides> = formats.iter().map(|f| f.1.border.sides).collect();
    assert_eq!(
        sides,
        [
            BorderSides::Above,
            BorderSides::Below,
            BorderSides::AboveAndBelow,
            BorderSides::Box,
            BorderSides::Below,
        ]
    );
    let dashed = formats[4].1.border;
    assert_eq!(dashed.style, BorderStyle::Dashed);
    assert_eq!(dashed.offset, Length::from_points(6));
    assert_eq!(dashed.width, Length::from_points(1));
}

#[test]
fn int_15_faces_keep_their_weight() {
    let face = |id: &str, name: &str, bold: Option<&str>| {
        format!(
            r#"<sf:characterstyle sf:parent-ident="character-style-null" sfa:ID="{id}"><sf:property-map><sf:fontName><sf:string sfa:string="{name}"/></sf:fontName>{}</sf:property-map></sf:characterstyle>"#,
            bold.map(|b| number("bold", b, "c")).unwrap_or_default()
        )
    };
    let anon = [
        face("W-1", "HelveticaNeue-UltraLight", None),
        face("W-2", "HelveticaNeue-Light", Some("1")),
        face("W-3", "HelveticaNeue-Bold", Some("0")),
        face("W-4", "HelveticaNeue-Medium", Some("0")),
    ]
    .concat();
    let body = r#"<sf:p sf:style="paragraph-style-32"><sf:span sf:style="W-1">a</sf:span><sf:span sf:style="W-2">b</sf:span><sf:span sf:style="W-3">c</sf:span><sf:span sf:style="W-4">d</sf:span></sf:p>"#;
    let document = read_pages(index(STYLES, &anon, body).as_bytes()).unwrap();
    let styles = document.styles();
    let paragraph = document.body().get(0).unwrap();
    let weights: Vec<u16> = paragraph
        .runs()
        .map(|r| {
            styles
                .resolve_character(paragraph.style(), r.style, &r.attrs)
                .weight
                .value()
        })
        .collect();
    // Bold over a light face is bold, and a bold face without bold is
    // regular.
    assert_eq!(weights, [100, 700, 400, 500]);
}

/// The fields and links of every run of some paragraphs, in order.
fn fields_and_links(
    paragraphs: impl IntoIterator<Item = desk_model::Paragraph>,
) -> (Vec<desk_model::Field>, Vec<String>) {
    let mut fields = Vec::new();
    let mut links = Vec::new();
    for paragraph in paragraphs {
        for run in paragraph.runs() {
            if let Some(Some(field)) = &run.attrs.field {
                fields.push(field.clone());
            }
            if let Some(Some(link)) = &run.attrs.link
                && links.last() != Some(&link.to_string())
            {
                links.push(link.to_string());
            }
        }
    }
    (fields, links)
}

#[test]
fn est_27_formats_of_fields_saved_by_pages_are_read() {
    use desk_model::{Field, MarginKind, MarginPages, MarginPlace, NumberFormat};
    let document = desk_filters::read_pages_at(&fixture("est-27-field-formats.pages")).unwrap();
    let footer = document
        .section_headers_footers(None)
        .get(MarginPlace::new(MarginKind::Footer, MarginPages::Default))
        .to_vec();
    let (fields, _) = fields_and_links(footer);
    assert_eq!(
        fields,
        [
            Field::PageNumber {
                format: NumberFormat::LowerRoman
            },
            Field::PageCount {
                format: NumberFormat::UpperRoman
            },
        ]
    );
    let document = desk_filters::read_pages_at(&fixture("est-27-date-filename.pages")).unwrap();
    let (fields, _) = fields_and_links(document.body().iter().cloned());
    let shape: Vec<(bool, bool, bool)> = fields
        .iter()
        .map(|f| match f {
            Field::DateTime { update, .. } => (true, *update, false),
            Field::FileName {
                path, extension, ..
            } => (false, *path, *extension),
            other => panic!("{other:?}"),
        })
        .collect();
    // A date that updates and one that does not; a file name with its
    // folder and extension, and one with neither.
    assert_eq!(
        shape,
        [
            (true, true, false),
            (true, false, false),
            (false, true, true),
            (false, false, false),
        ]
    );
}

#[test]
fn est_12_est_16_headers_of_sections_saved_by_pages_are_read() {
    use desk_model::{MarginKind, MarginPages, MarginPlace};
    let document =
        desk_filters::read_pages_at(&fixture("est-12-16-section-headers.pages")).unwrap();
    assert!(document.page_setup().facing_pages);
    let sections: Vec<(desk_model::NodeId, Section)> = document
        .body()
        .iter()
        .filter_map(|p| match p.attrs().start.clone().flatten().as_deref() {
            Some(Start::Section(section, _)) => Some((p.id(), section.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(sections.len(), 2);
    assert!(sections[0].1.own_margins);
    assert_eq!(sections[0].1.start, SectionStart::AnyPage);
    assert!(!sections[1].1.own_margins);
    assert_eq!(sections[1].1.start, SectionStart::RightPage);
    let header = |section| {
        document
            .section_headers_footers(section)
            .get(MarginPlace::new(MarginKind::Header, MarginPages::Default))
            .iter()
            .map(|p| p.text())
            .collect::<Vec<_>>()
    };
    assert_eq!(header(None), ["Header one"]);
    assert_eq!(header(Some(sections[0].0)), ["Header two"]);
}

#[test]
fn est_22_est_25_links_saved_by_pages_are_read() {
    let document = desk_filters::read_pages_at(&fixture("est-22-toc-page-links.pages")).unwrap();
    let toc = &document.text_settings().toc;
    assert!(toc.page_links);
    // Heading 1 shows its page number, Heading 2 does not.
    let shown = |name: &str| {
        let style = document
            .styles()
            .by_name(StyleFamily::Paragraph, name)
            .unwrap()
            .id();
        toc.levels
            .iter()
            .find(|l| l.style == style)
            .unwrap()
            .page_number
    };
    assert!(shown("Heading 1"));
    assert!(!shown("Heading 2"));
    let document = desk_filters::read_pages_at(&fixture("est-25-26-links.pages")).unwrap();
    let (_, links) = fields_and_links(document.body().iter().cloned());
    assert_eq!(
        links,
        [
            "https://www.example.com/desk/page.html",
            "mailto:desk@example.com?subject=Desk%20test%20subject",
            "#this%20bookmarked%20text",
            "file:///Users/desk/export/fx/link-target.pages#target",
        ]
    );
}
