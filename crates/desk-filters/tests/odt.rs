// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ODT to and from the model, named by the IDs of the parity matrix. A
//! document written and read again must describe the same text with the
//! same styles and formats.

use desk_filters::odt::{export, import};
use desk_filters::{read_odt, write_odt};
use desk_model::{
    Alignment, Baseline, BorderSides, BorderStyle, Capitalization, CharAttrs, CharacterStyle,
    Document, FontFeature, FontFeatures, LINE_BREAK, Language, Length, Ligatures, LineColor,
    LineSpacing, LineStyle, Metadata, NO_BREAK_SPACE, Op, PageSetup, ParaAttrs, ParaBorder,
    Paragraph, ParagraphStyle, Rgba, Run, Shadow, Style,
};
use desk_odf as odf;

/// Keeps a written file for ci/validate-odf.sh, which validates it against
/// the ODF schemas, when `DESK_ODF_OUT` names a directory.
fn keep(name: &str, bytes: &[u8]) {
    if let Some(dir) = std::env::var_os("DESK_ODF_OUT") {
        std::fs::write(std::path::Path::new(&dir).join(name), bytes).unwrap();
    }
}

/// Everything a reader of the document sees: the styles by name, and each
/// paragraph's style name, resolved format and runs.
fn describe(document: &Document) -> String {
    let styles = document.styles();
    let name = |id| styles.get(id).map(|s: &Style| s.name().to_owned());
    let mut out = String::new();
    // List styles by name, since their identifiers change.
    let attrs = |attrs: &desk_model::ParaAttrs| {
        let list = attrs.list_style.map(|l| l.and_then(name));
        let attrs = desk_model::ParaAttrs {
            list_style: None,
            ..attrs.clone()
        };
        format!("{attrs:?} list {list:?}")
    };
    let resolved = |format: desk_model::ParaFormat| {
        let list = format.list_style.and_then(name);
        let format = desk_model::ParaFormat {
            list_style: None,
            ..format
        };
        format!("{format:?} list {list:?}")
    };
    let mut all: Vec<String> = styles
        .iter()
        .map(|style| match style {
            Style::Paragraph(p) => format!(
                "paragraph style {:?} parent {:?} next {:?} key {:?} {} {:?}",
                p.name,
                p.parent.and_then(name),
                p.next.and_then(name),
                p.shortcut,
                attrs(&p.paragraph),
                p.character
            ),
            Style::List(l) => format!(
                "list style {:?} key {:?} {:?}",
                l.name, l.shortcut, l.levels
            ),
            Style::Character(c) => format!(
                "character style {:?} parent {:?} key {:?} {:?}",
                c.name,
                c.parent.and_then(name),
                c.shortcut,
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
            "paragraph {:?} {}\n",
            name(paragraph.style()),
            resolved(styles.resolve_paragraph(paragraph.style(), paragraph.attrs()))
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
            shortcut: Some(1),
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
                ligatures: Some(Ligatures::None),
                font_features: Some(FontFeatures::new(FontFeature::new("onum", 1))),
                tracking: Some(50),
                ..CharAttrs::default()
            },
        })
        .unwrap();
    document
        .put_style(ParagraphStyle {
            id: quote,
            name: "Quote".into(),
            shortcut: None,
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
            shortcut: Some(8),
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
            shortcut: None,
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
                background: Some(Some(Rgba::rgb(255, 255, 117))),
                border: Some(ParaBorder {
                    sides: BorderSides::Box,
                    width: Length::from_points(2),
                    color: Rgba::rgb(0, 0, 128),
                    style: BorderStyle::Dashed,
                    offset: Length::from_points(6),
                }),
                ..ParaAttrs::default()
            },
            vec![
                Run::new("A quote, "),
                Run::with_attrs(
                    "shadowed and outlined",
                    None,
                    CharAttrs {
                        outline: Some(true),
                        shadow: Some(Some(Shadow {
                            angle: 300,
                            offset: Length::from_points(4),
                            blur: Length::from_points(3),
                            // ODF has no opacity.
                            opacity: 100,
                            color: Rgba::rgb(0x40, 0x40, 0x40),
                        })),
                        tracking: Some(-50),
                        background: Some(Some(Rgba::rgb(0xc0, 0xff, 0xc0))),
                        ..CharAttrs::default()
                    },
                ),
                Run::with_attrs(
                    " with rules",
                    None,
                    CharAttrs {
                        ligatures: Some(Ligatures::All),
                        font_family: Some("Liberation Serif".into()),
                        background: Some(None),
                        shadow: Some(None),
                        ..CharAttrs::default()
                    },
                ),
            ],
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
    keep("filters-sample.odt", &bytes);
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
                list: None,
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
                list: None,
                spans: vec![],
            },
        ],
        ..odf::TextDocument::default()
    }
}

#[test]
fn fmt_files_from_other_programs_map_to_styles_and_direct_formatting() {
    let document = import(&foreign()).unwrap();
    keep("filters-foreign.odt", &write_odt(&document).unwrap());
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

/// Found by the fuzzer: without a `Standard` style, a style named like the
/// default took its name, and the next round trip renamed it.
#[test]
fn a_style_named_like_the_default_keeps_one_name() {
    let source = odf::TextDocument {
        styles: vec![odf::Style {
            name: "Body".into(),
            display_name: None,
            family: odf::Family::Paragraph,
            parent: None,
            next: None,
            list_style: None,
            properties: odf::Properties::default(),
        }],
        ..odf::TextDocument::default()
    };
    let document = import(&source).unwrap();
    let mut names: Vec<_> = document
        .styles()
        .iter()
        .map(|s| s.name().to_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["Body", "Body 2"]);
    let again = import(&export(&document)).unwrap();
    assert_eq!(describe(&again), describe(&document));
}

/// Found by the fuzzer: a carriage return in a style name came back as a
/// space.
#[test]
fn control_characters_in_style_names_are_kept() {
    let source = odf::TextDocument {
        styles: vec![odf::Style {
            name: "a".into(),
            display_name: Some("a\rb\tc\nd".into()),
            family: odf::Family::Paragraph,
            parent: None,
            next: None,
            list_style: None,
            properties: odf::Properties::default(),
        }],
        ..odf::TextDocument::default()
    };
    let document = import(&source).unwrap();
    let again = read_odt(&write_odt(&document).unwrap()).unwrap();
    assert_eq!(describe(&again), describe(&document));
    assert!(describe(&again).contains(r#""a\rb\tc\nd""#));
}

#[test]
fn fmt_22_languages_of_styles_and_runs_are_kept() {
    let mut document = Document::new_word_processing();
    let body = document.styles().default_paragraph();
    let runs = ["pt-BR", "en-US", "sr-Latn-RS", "de", "und"].map(|tag| {
        Run::with_attrs(
            tag,
            None,
            CharAttrs {
                language: Some(Language::new(tag)),
                ..CharAttrs::default()
            },
        )
    });
    let paragraph =
        Paragraph::from_runs(document.new_id(), body, ParaAttrs::default(), runs.to_vec()).unwrap();
    let first = document.body().get(0).unwrap().id();
    document
        .apply(Op::InsertParagraph {
            index: 1,
            paragraph,
        })
        .unwrap();
    document
        .apply(Op::RemoveParagraph { paragraph: first })
        .unwrap();

    let bytes = write_odt(&document).unwrap();
    keep("filters-languages.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    let paragraph = read.body().get(0).unwrap();
    let tags: Vec<String> = paragraph
        .runs()
        .map(|run| {
            read.styles()
                .resolve_character(paragraph.style(), run.style, &run.attrs)
                .language
                .tag()
                .to_owned()
        })
        .collect();
    assert_eq!(tags, ["pt-BR", "en-US", "sr-Latn-RS", "de", "und"]);

    // ODF splits a tag into a language, a script and a country.
    let odt = odf::read_odt(&bytes).unwrap();
    let serbian = odt
        .automatic_styles
        .iter()
        .map(|style| &style.properties.text)
        .find(|text| text.language.as_deref() == Some("sr"))
        .unwrap();
    assert_eq!(serbian.script.as_deref(), Some("Latn"));
    assert_eq!(serbian.country.as_deref(), Some("RS"));
}

#[test]
fn fmt_16_tab_stops_move_between_the_indent_and_the_edge_of_the_text() {
    let stop = |position| odf::TabStop {
        position: odf::Length(position),
        ..odf::TabStop::default()
    };
    let body = odf::Style {
        name: "Standard".into(),
        properties: odf::Properties {
            paragraph: odf::ParagraphProperties {
                margin_left: Some(odf::Length(36.0)),
                tab_stops: Some(vec![stop(72.0)]),
                ..odf::ParagraphProperties::default()
            },
            ..odf::Properties::default()
        },
        ..odf::Style::default()
    };
    let indented = odf::Style {
        name: "P1".into(),
        parent: Some("Standard".into()),
        properties: odf::Properties {
            paragraph: odf::ParagraphProperties {
                margin_left: Some(odf::Length(72.0)),
                ..odf::ParagraphProperties::default()
            },
            ..odf::Properties::default()
        },
        ..odf::Style::default()
    };
    let paragraph = |style: &str| odf::Paragraph {
        style: Some(style.into()),
        ..odf::Paragraph::default()
    };
    let source = odf::TextDocument {
        styles: vec![body],
        automatic_styles: vec![indented],
        body: vec![paragraph("Standard"), paragraph("P1")],
        ..odf::TextDocument::default()
    };
    let positions = |document: &Document| -> Vec<Vec<f64>> {
        let styles = document.styles();
        document
            .body()
            .iter()
            .map(|p| {
                let format = styles.resolve_paragraph(p.style(), p.attrs());
                format
                    .tab_stops
                    .stops()
                    .iter()
                    .map(|s| s.position.to_pt())
                    .collect()
            })
            .collect()
    };
    // LibreOffice measures from the indent, the model from the edge.
    let document = import(&source).unwrap();
    assert_eq!(positions(&document), [vec![108.0], vec![144.0]]);
    // Unless the settings say otherwise.
    let absolute = import(&odf::TextDocument {
        tabs_relative_to_indent: Some(false),
        ..source.clone()
    })
    .unwrap();
    assert_eq!(positions(&absolute), [vec![72.0], vec![72.0]]);

    // Written from the indent again, with the stops of the indented
    // paragraph said where its indent changes.
    let written = export(&document);
    let stops = |style: &odf::Style| {
        style
            .properties
            .paragraph
            .tab_stops
            .as_ref()
            .map(|s| s.iter().map(|s| s.position.0).collect::<Vec<_>>())
    };
    assert_eq!(stops(&written.styles[0]), Some(vec![72.0]));
    assert_eq!(stops(&written.automatic_styles[0]), Some(vec![72.0]));
    let again = import(&written).unwrap();
    assert_eq!(describe(&again), describe(&document));
}

#[test]
fn fmt_16_tab_stops_keep_their_alignment_leader_and_separator() {
    use desk_model::{TabAlignment, TabLeader, TabStop, TabStops};
    let mut document = Document::new_word_processing();
    let default = document.styles().default_paragraph();
    let mut body = ParagraphStyle::new(default, "Body");
    body.paragraph.tab_stops = Some(TabStops::new([
        TabStop::left(Length::from_points(36)),
        TabStop {
            alignment: TabAlignment::Center,
            leader: TabLeader::Dots,
            ..TabStop::left(Length::from_points(100))
        },
        TabStop {
            alignment: TabAlignment::Right,
            leader: TabLeader::Line,
            ..TabStop::left(Length::from_points(200))
        },
        TabStop {
            alignment: TabAlignment::Decimal,
            leader: TabLeader::Dashes,
            decimal: ',',
            ..TabStop::left(Length::from_points(300))
        },
        TabStop {
            leader: TabLeader::Arrows,
            ..TabStop::left(Length::from_points(400))
        },
    ]));
    body.paragraph.default_tab_interval = Some(Length::from_pt(35.43307));
    document.put_style(body).unwrap();
    let bytes = write_odt(&document).unwrap();
    keep("tabs.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    assert_eq!(describe(&read), describe(&document));
}

#[test]
fn fmt_08_fmt_09_features_of_a_font_name_become_ligatures_and_features() {
    let mut source = odf::TextDocument::default();
    source.styles.push(odf::Style {
        name: "Standard".into(),
        family: odf::Family::Paragraph,
        properties: odf::Properties {
            text: odf::TextProperties {
                font_family: Some("Linux Libertine G:liga=0&smcp&ss02=3".into()),
                font_size: Some(odf::Length(10.0)),
                letter_spacing: Some(odf::Length(1.0)),
                ..odf::TextProperties::default()
            },
            ..odf::Properties::default()
        },
        ..odf::Style::default()
    });
    let document = import(&source).unwrap();
    let styles = document.styles();
    let format = styles.resolve_character(styles.default_paragraph(), None, &CharAttrs::default());
    assert_eq!(&*format.font_family, "Linux Libertine G");
    assert_eq!(format.ligatures, Ligatures::None);
    assert_eq!(format.font_features.get("smcp"), Some(1));
    assert_eq!(format.font_features.get("ss02"), Some(3));
    // A point of spacing at 10 points is a tracking of 10%.
    assert_eq!(format.tracking, 100);
    // And back, as LibreOffice reads it.
    let written = export(&document);
    let standard = written
        .styles
        .iter()
        .find(|s| s.name == "Standard")
        .unwrap();
    assert_eq!(
        standard.properties.text.font_family.as_deref(),
        Some("Linux Libertine G:liga=0&clig=0&smcp&ss02=3")
    );
}

/// The labels of the paragraphs, empty for those outside a list.
fn labels(document: &Document) -> Vec<String> {
    document
        .styles()
        .list_items(document.body().iter())
        .into_iter()
        .map(|item| item.map(|i| i.label).unwrap_or_default())
        .collect()
}

#[test]
fn lst_02_05_lists_are_kept() {
    use desk_model::{Editor, ListLabel, ListStyle, NumberFormat, Position};
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Lists", |edit| {
            edit.insert_text(start, "a\nb\nc\nd\ntext\ne\nf\ng\nh")?;
            let at = |edit: &desk_model::Edit, i: usize| {
                Position::new(edit.document().body().get(i).unwrap().id(), 0)
            };
            let legal = edit.list_style_or_add("Legal", ListStyle::legal)?;
            let bullet = edit.list_style_or_add("Bullet", ListStyle::bullet)?;
            let (a, d) = (at(edit, 0), at(edit, 3));
            edit.set_list_style(a, d, Some(legal))?;
            let (b, c) = (at(edit, 1), at(edit, 2));
            edit.change_list_level(b, c, 1, false)?;
            edit.change_list_level(c, c, 1, false)?;
            // Bullets of more than one character, in color, large and
            // raised, after a list that goes on.
            let mut level = edit.document().styles().list(bullet).unwrap().levels[0].clone();
            level.label = ListLabel::bullet("->");
            level.size = 150;
            level.color = Some(Rgba::rgb(200, 0, 0));
            level.raise = Length::from_points(2);
            edit.set_list_level(bullet, 0, level)?;
            let (e, f) = (at(edit, 5), at(edit, 6));
            edit.set_list_style(e, f, Some(bullet))?;
            // The legal list goes on after the bullets, then starts again.
            let (g, h) = (at(edit, 7), at(edit, 8));
            edit.set_list_style(g, h, Some(legal))?;
            edit.set_list_start(h.paragraph, Some(5))?;
            // A paragraph style with a list, and a paragraph with it out
            // of the list.
            let id = edit.new_id();
            let mut style = ParagraphStyle::new(id, "Numbered");
            style.parent = Some(edit.document().styles().default_paragraph());
            style.paragraph.list_style = Some(Some(legal));
            edit.apply(Op::PutStyle {
                style: style.into(),
            })?;
            let text = at(edit, 4);
            edit.set_paragraph_style(text, text, id)?;
            edit.set_list_style(text, text, None)?;
            // Roman numbers from 3.
            let mut level = edit.document().styles().list(legal).unwrap().levels[3].clone();
            level.label = ListLabel::Number {
                format: NumberFormat::LowerRoman,
                prefix: "(".into(),
                suffix: ")".into(),
                tiered: false,
            };
            level.start = 3;
            edit.set_list_level(legal, 3, level)
        })
        .unwrap();
    let document = editor.document().clone();
    assert_eq!(
        labels(&document),
        ["1.", "1.1.", "1.1.1.", "2.", "", "->", "->", "3.", "5."]
    );
    let bytes = write_odt(&document).unwrap();
    keep("filters-lists.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    assert_eq!(labels(&read), labels(&document));
    assert_eq!(describe(&read), describe(&document));
    let again = read_odt(&write_odt(&read).unwrap()).unwrap();
    assert_eq!(describe(&again), describe(&document));
}

#[test]
fn lst_lists_of_pages_are_kept_in_odt() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages");
    for name in [
        "lst-02-levels",
        "lst-03-bullets",
        "lst-04-numbered",
        "lst-05-tiered",
    ] {
        let document =
            desk_filters::read_pages_at(&fixtures.join(format!("{name}.pages"))).unwrap();
        let bytes = write_odt(&document).unwrap();
        keep(&format!("filters-{name}.odt"), &bytes);
        let read = read_odt(&bytes).unwrap();
        assert_eq!(labels(&read), labels(&document), "{name}");
        let items = |d: &Document| -> Vec<_> {
            d.styles()
                .list_items(d.body().iter())
                .into_iter()
                .map(|i| i.map(|i| (i.level, i.definition)))
                .collect()
        };
        assert_eq!(items(&read), items(&document), "{name}");
    }
}

#[test]
fn lst_lists_from_other_programs_are_read() {
    // As LibreOffice writes a list formatted directly: an automatic list
    // style with the label alignment mode, nested lists and a header.
    let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.3">
<office:automatic-styles>
<text:list-style style:name="L1">
<text:list-level-style-number text:level="1" style:num-suffix="." style:num-format="1">
<style:list-level-properties text:list-level-position-and-space-mode="label-alignment">
<style:list-level-label-alignment text:label-followed-by="listtab" text:list-tab-stop-position="0.5in" fo:text-indent="-0.25in" fo:margin-left="0.5in"/>
</style:list-level-properties>
</text:list-level-style-number>
<text:list-level-style-bullet text:level="2" text:bullet-char="◦">
<style:list-level-properties text:space-before="0.5in" text:min-label-width="0.25in"/>
</text:list-level-style-bullet>
</text:list-style>
</office:automatic-styles>
<office:body><office:text>
<text:list text:style-name="L1">
<text:list-item><text:p>one</text:p>
<text:list><text:list-item><text:p>sub</text:p></text:list-item></text:list>
</text:list-item>
<text:list-header><text:p>header</text:p></text:list-header>
<text:list-item><text:p>two</text:p></text:list-item>
</text:list>
<text:p>after</text:p>
</office:text></office:body>
</office:document-content>"#;
    let source = odf::read_streams(content, None, None).unwrap();
    let document = import(&source).unwrap();
    assert_eq!(labels(&document), ["1.", "\u{25e6}", "", "2.", ""]);
    let items = document.styles().list_items(document.body().iter());
    let first = items[0].as_ref().unwrap();
    assert_eq!(first.definition.label_indent, Length::from_points(18));
    assert_eq!(first.definition.text_indent, Length::from_points(18));
    assert_eq!(items[1].as_ref().unwrap().level, 1);
    let _ = export(&document);
}
