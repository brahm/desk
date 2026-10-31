// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ODT to and from the model, named by the IDs of the parity matrix. A
//! document written and read again must describe the same text with the
//! same styles and formats.

use desk_filters::odt::{export, import};
use desk_filters::{read_odt, write_odt};
use desk_model::{
    Alignment, Baseline, Capitalization, CharAttrs, CharacterStyle, Document, LINE_BREAK, Language,
    Length, LineColor, LineSpacing, LineStyle, Metadata, NO_BREAK_SPACE, Op, PageSetup, ParaAttrs,
    Paragraph, ParagraphStyle, Rgba, Run, Style,
};
use desk_odf as odf;

/// Everything a reader of the document sees: the styles by name, and each
/// paragraph's style name, resolved format and runs.
fn describe(document: &Document) -> String {
    let styles = document.styles();
    let name = |id| styles.get(id).map(|s: &Style| s.name().to_owned());
    let mut out = String::new();
    let mut all: Vec<String> = styles
        .iter()
        .map(|style| match style {
            Style::Paragraph(p) => format!(
                "paragraph style {:?} parent {:?} next {:?} {:?} {:?}",
                p.name,
                p.parent.and_then(name),
                p.next.and_then(name),
                p.paragraph,
                p.character
            ),
            Style::Character(c) => format!(
                "character style {:?} parent {:?} {:?}",
                c.name,
                c.parent.and_then(name),
                c.character
            ),
        })
        .collect();
    all.sort();
    for line in all {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str(&format!("default {:?}\n", name(styles.default_paragraph())));
    for paragraph in document.body().iter() {
        out.push_str(&format!(
            "paragraph {:?} {:?}\n",
            name(paragraph.style()),
            styles.resolve_paragraph(paragraph.style(), paragraph.attrs())
        ));
        for run in paragraph.runs() {
            out.push_str(&format!(
                "  run {:?} style {:?} {:?}\n",
                run.text(),
                run.style.and_then(name),
                styles.resolve_character(paragraph.style(), run.style, &run.attrs)
            ));
        }
    }
    out.push_str(&format!(
        "{:?}\n{:?}\n",
        document.page_setup(),
        document.metadata()
    ));
    out
}

fn sample() -> Document {
    let mut document = Document::new_word_processing();
    let body = document.styles().default_paragraph();
    let heading = document.new_id();
    let quote = document.new_id();
    let emphasis = document.new_id();
    let strong = document.new_id();

    let mut base = ParagraphStyle::new(body, "Corpo do texto");
    base.paragraph = ParaAttrs {
        space_after: Some(Length::from_points(6)),
        widow_control: Some(true),
        ..ParaAttrs::default()
    };
    base.character = CharAttrs {
        font_family: Some("Liberation Serif".into()),
        font_size: Some(Length::from_points(11)),
        language: Some(Language::new("pt-BR")),
        ..CharAttrs::default()
    };
    document.put_style(base).unwrap();
    document
        .put_style(ParagraphStyle {
            id: heading,
            name: "Título 1".into(),
            parent: Some(body),
            next: Some(body),
            paragraph: ParaAttrs {
                page_break_before: Some(true),
                keep_with_next: Some(true),
                keep_lines_together: Some(true),
                alignment: Some(Alignment::Center),
                line_spacing: Some(LineSpacing::AtLeast(Length::from_points(20))),
                space_before: Some(Length::from_pt(14.5)),
                ..ParaAttrs::default()
            },
            character: CharAttrs {
                font_family: Some("Liberation Sans".into()),
                font_size: Some(Length::from_points(18)),
                bold: Some(true),
                capitalization: Some(Capitalization::SmallCaps),
                ..CharAttrs::default()
            },
        })
        .unwrap();
    document
        .put_style(ParagraphStyle {
            id: quote,
            name: "Quote".into(),
            parent: Some(body),
            next: None,
            paragraph: ParaAttrs {
                left_indent: Some(Length::from_points(36)),
                right_indent: Some(Length::from_points(36)),
                first_line_indent: Some(Length::from_points(-12)),
                line_spacing: Some(LineSpacing::Multiple { percent: 150 }),
                alignment: Some(Alignment::Justify),
                ..ParaAttrs::default()
            },
            character: CharAttrs {
                italic: Some(true),
                ..CharAttrs::default()
            },
        })
        .unwrap();
    document
        .put_style(CharacterStyle {
            id: emphasis,
            name: "Ênfase".into(),
            parent: None,
            character: CharAttrs {
                italic: Some(true),
                color: Some(Rgba::rgb(0x80, 0, 0x20)),
                ..CharAttrs::default()
            },
        })
        .unwrap();
    document
        .put_style(CharacterStyle {
            id: strong,
            name: "Strong Emphasis".into(),
            parent: Some(emphasis),
            character: CharAttrs {
                bold: Some(true),
                ..CharAttrs::default()
            },
        })
        .unwrap();

    let paragraphs = [
        (heading, ParaAttrs::default(), vec![Run::new("Capítulo um")]),
        (
            body,
            ParaAttrs {
                line_spacing: Some(LineSpacing::Exactly(Length::from_points(15))),
                ..ParaAttrs::default()
            },
            vec![
                Run::new("  Two leading spaces,\ta tab, "),
                Run::with_attrs(
                    "struck  red",
                    None,
                    CharAttrs {
                        strikethrough: Some(LineStyle::Double),
                        strikethrough_color: Some(LineColor::Custom(Rgba::rgb(255, 0, 0))),
                        underline: Some(LineStyle::Single),
                        ..CharAttrs::default()
                    },
                ),
                Run::new(" x"),
                Run::with_attrs(
                    "2",
                    None,
                    CharAttrs {
                        baseline: Some(Baseline::Superscript),
                        ..CharAttrs::default()
                    },
                ),
                Run::new(" and H"),
                Run::with_attrs(
                    "2",
                    None,
                    CharAttrs {
                        baseline: Some(Baseline::Subscript),
                        ..CharAttrs::default()
                    },
                ),
                Run::new(&format!("O{LINE_BREAK}new line, 10{NO_BREAK_SPACE}km, ")),
                Run::with_attrs("emphasis", Some(emphasis), CharAttrs::default()),
                Run::new(" and "),
                Run::with_attrs(
                    "strong caps",
                    Some(strong),
                    CharAttrs {
                        capitalization: Some(Capitalization::AllCaps),
                        font_size: Some(Length::from_pt(13.5)),
                        ..CharAttrs::default()
                    },
                ),
                Run::with_attrs(
                    " title case in English",
                    None,
                    CharAttrs {
                        capitalization: Some(Capitalization::Title),
                        language: Some(Language::new("en-US")),
                        ..CharAttrs::default()
                    },
                ),
            ],
        ),
        (
            quote,
            ParaAttrs {
                line_spacing: Some(LineSpacing::Between(Length::from_points(4))),
                widow_control: Some(false),
                ..ParaAttrs::default()
            },
            vec![Run::new("A quote.")],
        ),
        (body, ParaAttrs::default(), vec![]),
    ];
    let first = document.body().get(0).unwrap().id();
    for (index, (style, attrs, runs)) in paragraphs.into_iter().enumerate() {
        let id = document.new_id();
        let paragraph = Paragraph::from_runs(id, style, attrs, runs).unwrap();
        document
            .apply(Op::InsertParagraph {
                index: index + 1,
                paragraph,
            })
            .unwrap();
    }
    document
        .apply(Op::RemoveParagraph { paragraph: first })
        .unwrap();
    document
        .apply(Op::SetPageSetup {
            page: PageSetup::letter().rotated(),
        })
        .unwrap();
    document
        .apply(Op::SetMetadata {
            metadata: Metadata {
                title: "Relatório anual".into(),
                author: "Brahm".into(),
                comments: "Notas & observações".into(),
                keywords: vec!["desk".into(), "odt".into()],
            },
        })
        .unwrap();
    document
}

#[test]
fn a_document_written_and_read_again_is_the_same() {
    let document = sample();
    let bytes = write_odt(&document).unwrap();
    let read = read_odt(&bytes).unwrap();
    assert_eq!(describe(&read), describe(&document));
    // And once more, from what was read.
    let again = read_odt(&write_odt(&read).unwrap()).unwrap();
    assert_eq!(describe(&again), describe(&document));
}

#[test]
fn doc_19_est_02_est_03_information_and_pages_are_kept() {
    let read = read_odt(&write_odt(&sample()).unwrap()).unwrap();
    assert_eq!(read.metadata().title, "Relatório anual");
    assert_eq!(read.metadata().keywords, ["desk", "odt"]);
    assert!(read.page_setup().is_landscape());
    assert_eq!(read.page_setup().width, Length::from_points(792));
}

#[test]
fn the_default_style_is_written_as_standard() {
    let exported = export(&sample());
    let standard = exported
        .styles
        .iter()
        .find(|s| s.name == "Standard")
        .unwrap();
    assert_eq!(standard.display_name.as_deref(), Some("Corpo do texto"));
    assert!(standard.parent.is_none());
    let heading = exported
        .styles
        .iter()
        .find(|s| s.display_name.as_deref() == Some("Título 1"))
        .unwrap();
    assert_eq!(heading.name, "T_ed_tulo_20_1");
    assert_eq!(heading.parent.as_deref(), Some("Standard"));
    // One automatic style per distinct direct format.
    let paragraphs = exported
        .automatic_styles
        .iter()
        .filter(|s| s.family == odf::Family::Paragraph)
        .count();
    assert_eq!(paragraphs, 2);
}

/// A file as another program writes it: a default style, `Standard`, a
/// common style, and automatic styles over them.
fn foreign() -> odf::TextDocument {
    odf::TextDocument {
        default_style: odf::Properties {
            text: odf::TextProperties {
                font_family: Some("Liberation Serif".into()),
                font_size: Some(odf::Length(12.0)),
                ..odf::TextProperties::default()
            },
            ..odf::Properties::default()
        },
        styles: vec![
            odf::Style {
                name: "Standard".into(),
                family: odf::Family::Paragraph,
                ..odf::Style::default()
            },
            odf::Style {
                name: "Text_20_body".into(),
                display_name: Some("Text body".into()),
                family: odf::Family::Paragraph,
                parent: Some("Standard".into()),
                properties: odf::Properties {
                    paragraph: odf::ParagraphProperties {
                        margin_bottom: Some(odf::Length(7.0)),
                        line_height: Some(odf::LineHeight::Percent(115.0)),
                        ..odf::ParagraphProperties::default()
                    },
                    ..odf::Properties::default()
                },
                ..odf::Style::default()
            },
            // A cycle, which is broken when reading.
            odf::Style {
                name: "A".into(),
                family: odf::Family::Text,
                parent: Some("B".into()),
                ..odf::Style::default()
            },
            odf::Style {
                name: "B".into(),
                family: odf::Family::Text,
                parent: Some("A".into()),
                ..odf::Style::default()
            },
        ],
        automatic_styles: vec![
            odf::Style {
                name: "P1".into(),
                family: odf::Family::Paragraph,
                parent: Some("Text_20_body".into()),
                properties: odf::Properties {
                    paragraph: odf::ParagraphProperties {
                        align: Some(odf::TextAlign::End),
                        ..odf::ParagraphProperties::default()
                    },
                    text: odf::TextProperties {
                        font_weight: Some(700),
                        ..odf::TextProperties::default()
                    },
                },
                ..odf::Style::default()
            },
            odf::Style {
                name: "T1".into(),
                family: odf::Family::Text,
                properties: odf::Properties {
                    text: odf::TextProperties {
                        position: Some(odf::TextPosition::Percent(33.0)),
                        position_scale: Some(58.0),
                        color: Some(odf::Color { r: 0, g: 128, b: 0 }),
                        ..odf::TextProperties::default()
                    },
                    ..odf::Properties::default()
                },
                ..odf::Style::default()
            },
        ],
        body: vec![
            odf::Paragraph {
                style: Some("P1".into()),
                outline_level: None,
                spans: vec![
                    odf::Span {
                        style: None,
                        text: "bold, right".into(),
                    },
                    odf::Span {
                        style: Some("T1".into()),
                        text: "raised".into(),
                    },
                    odf::Span {
                        style: Some("missing".into()),
                        text: " unknown style".into(),
                    },
                ],
            },
            odf::Paragraph {
                style: Some("A".into()),
                outline_level: Some(1),
                spans: vec![],
            },
        ],
        ..odf::TextDocument::default()
    }
}

#[test]
fn fmt_files_from_other_programs_map_to_styles_and_direct_formatting() {
    let document = import(&foreign()).unwrap();
    let styles = document.styles();
    let default = styles.default_paragraph();
    assert_eq!(styles.get(default).unwrap().name(), "Standard");
    let paragraph = document.body().get(0).unwrap();
    assert_eq!(styles.get(paragraph.style()).unwrap().name(), "Text body");
    // FMT-12 from the automatic style, FMT-14 and FMT-15 from the common one.
    let format = styles.resolve_paragraph(paragraph.style(), paragraph.attrs());
    assert_eq!(format.alignment, Alignment::Right);
    assert_eq!(format.line_spacing, LineSpacing::Multiple { percent: 115 });
    assert_eq!(format.space_after, Length::from_points(7));
    let runs: Vec<_> = paragraph.runs().collect();
    assert_eq!(runs.len(), 3);
    let resolve = |run: &Run| styles.resolve_character(paragraph.style(), run.style, &run.attrs);
    // FMT-01 from the paragraph's automatic style, on every run.
    assert!(runs.iter().all(|r| resolve(r).bold));
    assert_eq!(&*resolve(runs[0]).font_family, "Liberation Serif");
    // FMT-06 and FMT-11.
    assert_eq!(resolve(runs[1]).baseline, Baseline::Superscript);
    assert_eq!(resolve(runs[1]).color, Rgba::rgb(0, 128, 0));
    assert_eq!(resolve(runs[2]).baseline, Baseline::Normal);
    // A paragraph naming a text style gets the default paragraph style.
    let second = document.body().get(1).unwrap();
    assert_eq!(second.style(), default);
    // Both styles of the cycle exist, one of them without its parent.
    let a = styles
        .by_name(desk_model::StyleFamily::Character, "A")
        .unwrap();
    let b = styles
        .by_name(desk_model::StyleFamily::Character, "B")
        .unwrap();
    assert!(a.parent().is_none() || b.parent().is_none());
}

#[test]
fn an_empty_file_gives_one_empty_paragraph() {
    let document = import(&odf::TextDocument::default()).unwrap();
    assert_eq!(document.body().len(), 1);
    assert_eq!(document.page_setup(), &PageSetup::a4());
}
