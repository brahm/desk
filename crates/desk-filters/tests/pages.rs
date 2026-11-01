// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pages documents to the model, from small indexes written by hand in the
//! form that Pages 4.3 saves (DOC-22). The corpus checks whole documents
//! saved by Pages itself, in desk-testkit.

use desk_filters::log::{Content, Formatting, Setting, Warning};
use desk_filters::{read_pages, read_pages_logged};
use desk_model::{
    Alignment, Baseline, Capitalization, Document, LINE_BREAK, Length, LineSpacing, LineStyle,
    Rgba, StyleFamily,
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
    assert!(character(3).bold);
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
    assert!(riscado.bold);
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
        number("tracking", "0.1", "f"),
        number("tracking", "0", "f"),
        number("ligatures", "1", "i"),
        number("tracking", "0.2", "f"),
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
            (Warning::Content(Content::Headers), 1),
            (Warning::Content(Content::FloatingObjects), 2),
            (Warning::Content(Content::InlineObjects), 2),
            (Warning::Formatting(Formatting::Lists), 1),
            (Warning::Formatting(Formatting::CharacterSpacing), 2),
        ]
    );
    assert_eq!(
        log.entries()[4].to_string(),
        "Left out character spacing, in 2 paragraphs"
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
            (Warning::Content(Content::Headers), 1),
            (Warning::Content(Content::Footers), 1),
            (Warning::Content(Content::FloatingObjects), 1),
            (Warning::Content(Content::InlineObjects), 1),
            (Warning::Content(Content::Footnotes), 1),
            (Warning::Content(Content::Comments), 1),
            (Warning::Content(Content::Fields), 2),
            (Warning::Formatting(Formatting::Lists), 2),
            (Warning::Formatting(Formatting::TabStops), 1),
            (Warning::Formatting(Formatting::CharacterSpacing), 1),
            (Warning::Formatting(Formatting::TextShadows), 1),
            (Warning::Formatting(Formatting::ParagraphBackgrounds), 1),
            (Warning::Formatting(Formatting::ParagraphBorders), 1),
        ]
    );
    // The text of the body is kept, without that of the footnote and the
    // comment.
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
}

#[test]
fn est_11_txt_09_facing_pages_and_automatic_hyphenation_are_logged() {
    let anon = format!(
        r#"<sf:paragraphstyle sf:parent-ident="paragraph-style-32" sfa:ID="A-1"><sf:property-map>{}</sf:property-map></sf:paragraphstyle>"#,
        number("hyphenate", "0", "c"),
    );
    let body = r#"<sf:p sf:style="paragraph-style-32">Hifenizado<sf:crbr/></sf:p>
<sf:p sf:style="A-1">Sem hifenização<sf:crbr/></sf:p>
<sf:p sf:style="paragraph-style-90">Hifenizado também</sf:p>"#;
    let plain = index(STYLES, &anon, body);
    let (_, log) = read_pages_logged(plain.as_bytes()).unwrap();
    // Without automatic hyphenation, Pages breaks words only at soft
    // hyphens, as Desk does.
    assert!(log.is_empty(), "{:?}", log.entries());
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
    let (_, log) = read_pages_logged(hyphenated.as_bytes()).unwrap();
    let entries: Vec<(Warning, usize)> = log
        .entries()
        .iter()
        .map(|e| (e.warning.clone(), e.count))
        .collect();
    assert_eq!(
        entries,
        [
            (Warning::Setting(Setting::FacingPages), 1),
            (Warning::Formatting(Formatting::Hyphenation), 2),
        ]
    );
    let off = plain.replace("<sl:slprint-info", &setting("0"));
    assert!(read_pages_logged(off.as_bytes()).unwrap().1.is_empty());
}

#[test]
fn est_11_facing_pages_saved_by_pages_are_logged() {
    let (_, log) =
        desk_filters::read_pages_at_logged(&fixture("est-11-facing-pages.pages")).unwrap();
    assert!(
        log.entries()
            .iter()
            .any(|e| e.warning == Warning::Setting(Setting::FacingPages))
    );
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
    // A section starts on a new page.
    let (document, _) =
        desk_filters::read_pages_at_logged(&fixture("est-16-18-sections.pages")).unwrap();
    assert!(
        document
            .body()
            .iter()
            .skip(1)
            .any(|p| p.attrs().page_break_before == Some(true))
    );
}
