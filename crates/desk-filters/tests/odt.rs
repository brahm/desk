// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ODT to and from the model, named by the IDs of the parity matrix. A
//! document written and read again must describe the same text with the
//! same styles and formats.

use desk_filters::odt::{export, import};
use desk_filters::{read_odt, write_odt};
use desk_model::FontWeight;
use desk_model::{
    Alignment, Baseline, BorderSides, BorderStyle, Capitalization, CharAttrs, CharacterStyle,
    Document, FontFeature, FontFeatures, LINE_BREAK, Language, Length, Ligatures, LineColor,
    LineSpacing, LineStyle, Metadata, NO_BREAK_SPACE, Op, PageSetup, ParaAttrs, ParaBorder,
    Paragraph, ParagraphSpacing, ParagraphStyle, Rgba, Run, Shadow, Style, TextSettings,
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
        "{:?}\n{:?}\n{:?}\n",
        document.page_setup(),
        document.metadata(),
        document.text_settings()
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
                weight: Some(FontWeight::BOLD),
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
                weight: Some(FontWeight::BOLD),
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
                        underline_color: Some(LineColor::Custom(Rgba::rgb(0, 0, 255))),
                        ..CharAttrs::default()
                    },
                ),
                Run::new(" "),
                Run::with_attrs(
                    "ultralight",
                    None,
                    CharAttrs {
                        weight: Some(FontWeight::THIN),
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
                            // Kept in desk:shadow-opacity (FMT-03).
                            opacity: 45,
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
        .apply(Op::SetTextSettings {
            settings: TextSettings {
                hyphenation: false,
                ligatures: false,
                never_hyphenate: ["desk".to_owned(), "odt".to_owned()].into(),
                spacing: ParagraphSpacing::Larger,
                notes: Default::default(),
                toc: Default::default(),
                links_inactive: true,
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
                    ..Default::default()
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
                section: None,
                marks: Vec::new(),
                toc: false,
                spans: vec![
                    odf::Span {
                        style: None,
                        text: "bold, right".into(),
                        ..odf::Span::default()
                    },
                    odf::Span {
                        style: Some("T1".into()),
                        text: "raised".into(),
                        ..odf::Span::default()
                    },
                    odf::Span {
                        style: Some("missing".into()),
                        text: " unknown style".into(),
                        ..odf::Span::default()
                    },
                ],
                ..Default::default()
            },
            odf::Paragraph {
                style: Some("A".into()),
                outline_level: Some(1),
                list: None,
                section: None,
                marks: Vec::new(),
                toc: false,
                spans: vec![],
                ..Default::default()
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
    assert!(runs.iter().all(|r| resolve(r).weight.is_bold()));
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
    // A file without a header or a footer has none to show.
    assert_eq!(
        document.page_setup(),
        &PageSetup {
            headers: false,
            footers: false,
            ..PageSetup::a4()
        }
    );
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
            master_page: None,
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
            master_page: None,
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
            edit.set_list_level(legal, 3, level)?;
            // A label of 18 points whatever the size of the text (LST-03).
            let mut level = edit.document().styles().list(bullet).unwrap().levels[1].clone();
            level.size = 150;
            level.scale_with_text = false;
            edit.set_list_level(bullet, 1, level)?;
            // An item with a bullet of its own over its list style.
            let f = at(edit, 6);
            edit.change_list_items(f, f, |level| level.label = ListLabel::bullet("*"))
        })
        .unwrap();
    let document = editor.document().clone();
    assert_eq!(
        labels(&document),
        ["1.", "1.1.", "1.1.1.", "2.", "", "->", "*", "3.", "5."]
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

#[test]
fn txt_09_hyphenation_of_styles_and_paragraphs_is_kept() {
    let mut document = Document::new_word_processing();
    let body = document.styles().default_paragraph();
    let hyphenated = document.new_id();
    let mut style = ParagraphStyle::new(hyphenated, "Hyphenated");
    style.parent = Some(body);
    style.paragraph.hyphenate = Some(true);
    document.put_style(style).unwrap();
    let first = document.body().get(0).unwrap().id();
    for (index, (style, attrs)) in [
        (hyphenated, ParaAttrs::default()),
        (
            hyphenated,
            ParaAttrs {
                hyphenate: Some(false),
                ..ParaAttrs::default()
            },
        ),
        (
            body,
            ParaAttrs {
                hyphenate: Some(true),
                ..ParaAttrs::default()
            },
        ),
        (body, ParaAttrs::default()),
    ]
    .into_iter()
    .enumerate()
    {
        let paragraph =
            Paragraph::from_runs(document.new_id(), style, attrs, vec![Run::new("Text")]).unwrap();
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

    let bytes = write_odt(&document).unwrap();
    keep("filters-hyphenation.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    let hyphenate: Vec<bool> = read
        .body()
        .iter()
        .map(|p| {
            read.styles()
                .resolve_paragraph(p.style(), p.attrs())
                .hyphenate
        })
        .collect();
    assert_eq!(hyphenate, [true, false, true, false]);

    // In a document that is not hyphenated, other programs read that no
    // paragraph is, and Desk what each allows.
    let mut settings = document.text_settings().clone();
    settings.hyphenation = false;
    document.apply(Op::SetTextSettings { settings }).unwrap();
    let bytes = write_odt(&document).unwrap();
    keep("filters-hyphenation-off.odt", &bytes);
    let odf = odf::read_odt(&bytes).unwrap();
    assert!(
        odf.styles
            .iter()
            .chain(&odf.automatic_styles)
            .all(|s| s.properties.text.hyphenate != Some(true))
    );
    let read = read_odt(&bytes).unwrap();
    assert!(!read.text_settings().hyphenation);
    let again: Vec<bool> = read
        .body()
        .iter()
        .map(|p| {
            read.styles()
                .resolve_paragraph(p.style(), p.attrs())
                .hyphenate
        })
        .collect();
    assert_eq!(again, hyphenate);
}

#[test]
fn fmt_10_tracking_keeps_its_part_of_the_size_under_another_size() {
    // A style tracks its text by 10%, and a run of it twice the size: the
    // tracking stays 10% of each size, though ODF writes the spacing of
    // the style in points.
    let mut document = Document::new_word_processing();
    let body = document.styles().default_paragraph();
    let tracked = document.new_id();
    let mut style = ParagraphStyle::new(tracked, "Tracked");
    style.parent = Some(body);
    style.character.tracking = Some(100);
    document.put_style(style).unwrap();
    let first = document.body().get(0).unwrap().id();
    let paragraph = Paragraph::from_runs(
        document.new_id(),
        tracked,
        ParaAttrs::default(),
        vec![
            Run::new("Small "),
            Run::with_attrs(
                "large",
                None,
                CharAttrs {
                    font_size: Some(Length::from_points(24)),
                    ..CharAttrs::default()
                },
            ),
            Run::with_attrs(
                " odd",
                None,
                CharAttrs {
                    font_size: Some(Length::from_pt(9.7)),
                    tracking: Some(33),
                    ..CharAttrs::default()
                },
            ),
        ],
    )
    .unwrap();
    document
        .apply(Op::InsertParagraph {
            index: 1,
            paragraph,
        })
        .unwrap();
    document
        .apply(Op::RemoveParagraph { paragraph: first })
        .unwrap();
    let read = read_odt(&write_odt(&document).unwrap()).unwrap();
    let paragraph = read.body().get(0).unwrap();
    for run in paragraph.runs() {
        let format = read
            .styles()
            .resolve_character(paragraph.style(), run.style, &run.attrs);
        let expected = if run.text() == " odd" { 33 } else { 100 };
        assert_eq!(format.tracking, expected, "{}", run.text());
    }
}

#[test]
fn fmt_15_documents_of_other_programs_add_the_spaces_between_paragraphs() {
    let foreign = odf::TextDocument::default();
    let document = import(&foreign).unwrap();
    assert_eq!(document.text_settings().spacing, ParagraphSpacing::Sum);
    // Desk says it takes the larger, so its documents keep that.
    let mine = Document::new_word_processing();
    let read = read_odt(&write_odt(&mine).unwrap()).unwrap();
    assert_eq!(read.text_settings().spacing, ParagraphSpacing::Larger);
}

/// The page setup and, for each paragraph, its column break and the
/// section or layout it starts.
fn structure(document: &Document) -> (PageSetup, Vec<(bool, Option<desk_model::Start>, bool)>) {
    let styles = document.styles();
    let paragraphs = document
        .body()
        .iter()
        .map(|p| {
            let format = styles.resolve_paragraph(p.style(), p.attrs());
            (
                format.column_break_before,
                format.start.as_deref().copied(),
                format.page_break_before,
            )
        })
        .collect();
    (*document.page_setup(), paragraphs)
}

/// Whether columns are the same, within a ten-thousandth of their width.
fn same_columns(a: &desk_model::Columns, b: &desk_model::Columns, name: &str) {
    assert_eq!((a.count, a.equal), (b.count, b.equal), "{name}");
    for (s, t) in a.spans(1000.0).iter().zip(b.spans(1000.0)) {
        assert!(
            (s.0 - t.0).abs() < 0.1 && (s.1 - t.1).abs() < 0.1,
            "{name}: {a:?} {b:?}"
        );
    }
}

#[test]
fn est_07_to_est_18_layouts_sections_and_facing_pages_are_kept() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages");
    for name in [
        "est-07-08-columns.pages",
        "est-09-10-layouts.pages",
        "est-11-facing-pages.pages",
        "est-16-18-sections.pages",
    ] {
        let document = desk_filters::read_pages_at(&fixtures.join(name)).unwrap();
        let bytes = write_odt(&document).unwrap();
        keep(&name.replace(".pages", ".odt"), &bytes);
        let read = read_odt(&bytes).unwrap();
        let (setup, paragraphs) = structure(&document);
        let (read_setup, read_paragraphs) = structure(&read);
        assert_eq!(
            PageSetup {
                layout: setup.layout,
                ..read_setup
            },
            setup,
            "{name}"
        );
        same_columns(&read_setup.layout.columns, &setup.layout.columns, name);
        // Columns come back within a millionth of the width.
        for (a, b) in read_paragraphs.iter().zip(&paragraphs) {
            assert_eq!((a.0, a.2), (b.0, b.2), "{name}");
            match (a.1, b.1) {
                (Some(x), Some(y)) => {
                    let (lx, ly) = (x.layout(), y.layout());
                    same_columns(&lx.columns, &ly.columns, name);
                    assert_eq!(
                        (lx.before, lx.after, lx.left, lx.right, lx.new_page),
                        (ly.before, ly.after, ly.left, ly.right, ly.new_page),
                        "{name}"
                    );
                    assert_eq!(
                        matches!(x, desk_model::Start::Section(..)),
                        matches!(y, desk_model::Start::Section(..)),
                        "{name}"
                    );
                    if let (desk_model::Start::Section(s, _), desk_model::Start::Section(t, _)) =
                        (x, y)
                    {
                        assert_eq!(s, t, "{name}");
                    }
                }
                (x, y) => assert_eq!(x, y, "{name}"),
            }
        }
        assert_eq!(read_paragraphs.len(), paragraphs.len(), "{name}");
    }
}

#[test]
fn est_07_est_18_columns_and_page_numbers_of_other_programs_are_read() {
    let content = r#"<office:document-content
    xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
    xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
    xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
    xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
<office:automatic-styles>
<style:style style:name="P1" style:family="paragraph"><style:paragraph-properties fo:break-before="page" style:page-number="7"/></style:style>
<style:style style:name="S1" style:family="section"><style:section-properties><style:columns fo:column-count="2" fo:column-gap="0.5in"/></style:section-properties></style:style>
</office:automatic-styles>
<office:body><office:text>
<text:p>One</text:p>
<text:section text:name="A" text:style-name="S1"><text:p>Two</text:p></text:section>
<text:p>Three</text:p>
<text:p text:style-name="P1">Four</text:p>
</office:text></office:body></office:document-content>"#;
    let document = import(&odf::read_streams(content, None, None).unwrap()).unwrap();
    let (_, paragraphs) = structure(&document);
    let starts: Vec<Option<desk_model::Start>> = paragraphs.iter().map(|p| p.1).collect();
    assert_eq!(starts[0], None);
    let Some(desk_model::Start::Layout(two)) = starts[1] else {
        panic!("{starts:?}");
    };
    assert_eq!(two.columns.count, 2);
    assert_eq!(
        starts[2],
        Some(desk_model::Start::Layout(desk_model::Layout::default()))
    );
    let Some(desk_model::Start::Section(section, _)) = starts[3] else {
        panic!("{starts:?}");
    };
    assert_eq!(section.first_number, Some(7));
}

#[test]
fn est_13_est_14_notes_are_kept() {
    use desk_model::{FIELD_CHARACTER, Field, Note, NoteFormat, NoteKind, NoteNumbering};
    let mut document = Document::new_word_processing();
    let body = document.styles().default_paragraph();
    let first = document.body().get(0).unwrap().id();
    let marked = |note| CharAttrs {
        field: Some(Some(Field::NoteMark { note })),
        baseline: Some(Baseline::Superscript),
        ..CharAttrs::default()
    };
    let mark = FIELD_CHARACTER.to_string();
    let numbered = document.new_id();
    let own = document.new_id();
    for (note, own_mark, text) in [
        (numbered, None, "A numbered note."),
        (own, Some("**"), "A note with a mark of its own."),
    ] {
        let id = document.new_id();
        let paragraph = Paragraph::from_runs(
            id,
            body,
            ParaAttrs::default(),
            vec![Run::with_attrs(&mark, None, marked(note)), Run::new(text)],
        )
        .unwrap();
        document
            .apply(Op::SetNote {
                id: note,
                note: Some(Note {
                    id: note,
                    mark: own_mark.map(Into::into),
                    paragraphs: vec![paragraph].into(),
                }),
            })
            .unwrap();
    }
    let id = document.new_id();
    let paragraph = Paragraph::from_runs(
        id,
        body,
        ParaAttrs::default(),
        vec![
            Run::new("Two notes"),
            Run::with_attrs(&mark, None, marked(numbered)),
            Run::new(" here"),
            Run::with_attrs(&mark, None, marked(own)),
        ],
    )
    .unwrap();
    document
        .apply(Op::InsertParagraph {
            index: 1,
            paragraph,
        })
        .unwrap();
    document
        .apply(Op::RemoveParagraph { paragraph: first })
        .unwrap();
    let mut settings = document.text_settings().clone();
    settings.notes.kind = NoteKind::SectionEndnotes;
    settings.notes.format = NoteFormat::Symbols;
    settings.notes.numbering = NoteNumbering::Page;
    settings.notes.gap = Length::from_points(20);
    document.apply(Op::SetTextSettings { settings }).unwrap();

    let bytes = write_odt(&document).unwrap();
    keep("filters-notes.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    assert_eq!(read.text_settings().notes, document.text_settings().notes);
    // The body refers to each note where it did, and each note keeps its
    // text, after its mark, and its mark of its own.
    let refs: Vec<_> = read
        .body()
        .get(0)
        .unwrap()
        .runs()
        .filter_map(|r| match &r.attrs.field {
            Some(Some(Field::NoteMark { note })) => Some(*note),
            _ => None,
        })
        .collect();
    assert_eq!(refs.len(), 2);
    assert_eq!(
        read.body()
            .get(0)
            .unwrap()
            .text()
            .replace(FIELD_CHARACTER, ""),
        "Two notes here"
    );
    let notes: Vec<_> = refs
        .iter()
        .map(|id| read.notes().get(*id).unwrap())
        .collect();
    assert_eq!(notes[0].mark, None);
    assert_eq!(notes[1].mark.as_deref(), Some("**"));
    for (note, text) in notes
        .iter()
        .zip(["A numbered note.", "A note with a mark of its own."])
    {
        let paragraph = &note.paragraphs[0];
        let runs: Vec<_> = paragraph.runs().collect();
        assert_eq!(
            runs[0].attrs.field,
            Some(Some(Field::NoteMark { note: note.id }))
        );
        assert_eq!(paragraph.text().replace(FIELD_CHARACTER, ""), text);
    }
}

#[test]
fn est_22_to_est_26_tables_of_contents_links_and_bookmarks_are_kept() {
    use desk_model::{Editor, FIELD_CHARACTER, Field, Position, TocLevel, TocSettings};
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Setup", |edit| {
            let mut ids = vec![start.paragraph];
            let mut at = edit.insert_text(start, "Chapter one")?;
            for text in ["See the site or the chapter", "Chapter two"] {
                at = edit.split_paragraph(at)?;
                ids.push(at.paragraph);
                at = edit.insert_text(at, text)?;
            }
            let heading = edit.new_paragraph_style("Heading", start, start, false)?;
            let entry = edit.new_paragraph_style("TOC Heading", start, start, false)?;
            for &id in [ids[0], ids[2]].iter() {
                edit.set_paragraph_style(Position::new(id, 0), Position::new(id, 0), heading)?;
            }
            let link = |url: &str| CharAttrs {
                link: Some(Some(url.into())),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(
                Position::new(ids[1], 8),
                Position::new(ids[1], 12),
                &link("https://example.com/"),
            )?;
            edit.set_character_attrs(
                Position::new(ids[1], 20),
                Position::new(ids[1], 27),
                &link("#Two"),
            )?;
            edit.set_character_attrs(
                Position::new(ids[2], 0),
                Position::new(ids[2], 11),
                &CharAttrs {
                    bookmark: Some(Some("Two".into())),
                    ..CharAttrs::default()
                },
            )?;
            let mut settings = edit.document().text_settings().clone();
            settings.toc = TocSettings {
                levels: vec![TocLevel {
                    style: heading,
                    entry,
                    page_number: true,
                }],
                page_links: true,
            };
            settings.links_inactive = true;
            edit.set_text_settings(settings)?;
            edit.insert_table_of_contents(0)
        })
        .unwrap();
    let document = editor.document();

    let bytes = write_odt(document).unwrap();
    keep("filters-toc.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    assert!(read.text_settings().links_inactive);
    let name = |d: &Document, id| d.styles().get(id).unwrap().name().to_owned();
    let levels: Vec<_> = read
        .text_settings()
        .toc
        .levels
        .iter()
        .map(|l| (name(&read, l.style), name(&read, l.entry), l.page_number))
        .collect();
    assert_eq!(
        levels,
        [("Heading".to_owned(), "TOC Heading".to_owned(), true)]
    );
    let body: Vec<_> = read.body().iter().collect();
    assert_eq!(body.len(), 5);
    // The entries keep their text, and their page fields the headings
    // they list.
    for (entry, (target, text)) in body[..2]
        .iter()
        .zip([(2, "Chapter one"), (4, "Chapter two")])
    {
        assert_eq!(
            entry.attrs().toc_entry.flatten().unwrap().target,
            Some(body[target].id())
        );
        assert_eq!(entry.text(), format!("{text}\t{FIELD_CHARACTER}"));
        let field = entry.runs().find_map(|r| r.attrs.field.clone().flatten());
        assert_eq!(
            field,
            Some(Field::PageOf {
                paragraph: body[target].id()
            })
        );
    }
    let attrs = |p: &Paragraph, text: &str| -> CharAttrs {
        let at = p.text().find(text).unwrap();
        p.format_at(at + 1).1
    };
    assert_eq!(
        attrs(body[3], "site").link.flatten().as_deref(),
        Some("https://example.com/")
    );
    assert_eq!(
        attrs(body[3], "chapter").link.flatten().as_deref(),
        Some("#Two")
    );
    assert_eq!(attrs(body[3], "See").link, None);
    assert_eq!(
        attrs(body[4], "Chapter").bookmark.flatten().as_deref(),
        Some("Two")
    );
}

#[test]
fn est_12_est_16_est_27_sections_keep_their_headers_and_fields_their_formats() {
    use desk_model::{
        Editor, FIELD_CHARACTER, Field, MarginKind, MarginPages, MarginPlace, NumberFormat,
        Position, Section, Start,
    };
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let header = MarginPlace::new(MarginKind::Header, MarginPages::Default);
    let second = editor
        .edit("Setup", |edit| {
            let at = edit.insert_text(start, "One")?;
            let at = edit.split_paragraph(at)?;
            edit.insert_text(at, "Two")?;
            let mut setup = *edit.document().page_setup();
            setup.headers = true;
            edit.set_page_setup(setup)?;
            let body = edit.document().styles().default_paragraph();
            let field = |field| {
                Run::with_attrs(
                    &FIELD_CHARACTER.to_string(),
                    None,
                    CharAttrs {
                        field: Some(Some(field)),
                        ..CharAttrs::default()
                    },
                )
            };
            let first = Paragraph::from_runs(
                edit.new_id(),
                body,
                ParaAttrs::default(),
                [
                    Run::new("Page "),
                    field(Field::PageNumber {
                        format: NumberFormat::LowerRoman,
                    }),
                    Run::new(" "),
                    field(Field::DateTime {
                        pattern: "y".into(),
                        shown: "2026".into(),
                        update: true,
                    }),
                    Run::new(" "),
                    field(Field::FileName {
                        shown: "Notes".into(),
                        path: false,
                        extension: false,
                    }),
                ],
            )?;
            edit.set_header_footer(header, vec![first])?;
            edit.set_paragraph_attrs(
                Position::new(at.paragraph, 0),
                Position::new(at.paragraph, 0),
                &ParaAttrs {
                    start: Some(Some(std::sync::Arc::new(Start::Section(
                        Section {
                            own_margins: true,
                            ..Section::default()
                        },
                        desk_model::Layout::default(),
                    )))),
                    ..ParaAttrs::default()
                },
            )?;
            let own = Paragraph::from_runs(
                edit.new_id(),
                body,
                ParaAttrs::default(),
                [Run::new("Chapter two")],
            )?;
            edit.set_section_header_footer(Some(at.paragraph), header, vec![own])?;
            Ok(at.paragraph)
        })
        .unwrap();
    let document = editor.document();
    assert_eq!(document.headers_section(1), Some(second));

    let bytes = write_odt(document).unwrap();
    keep("filters-section-headers.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    let second = read.body().get(1).unwrap().id();
    assert_eq!(read.headers_section(1), Some(second));
    assert_eq!(read.headers_section(0), None);
    assert_eq!(
        read.section_headers_footers(Some(second)).get(header)[0].text(),
        "Chapter two"
    );
    let fields: Vec<Field> = read.headers_footers().get(header)[0]
        .runs()
        .filter_map(|r| r.attrs.field.clone().flatten())
        .collect();
    assert_eq!(
        fields,
        [
            Field::PageNumber {
                format: NumberFormat::LowerRoman
            },
            Field::DateTime {
                pattern: "y".into(),
                shown: "2026".into(),
                update: true,
            },
            Field::FileName {
                shown: "Notes".into(),
                path: false,
                extension: false,
            },
        ]
    );
}

#[test]
fn est_13_notes_follow_the_configuration_of_other_apps() {
    use desk_model::{NoteFormat, NoteNumbering};
    let source = odf::TextDocument {
        notes_configuration: vec![odf::NotesConfiguration {
            class: "footnote".into(),
            num_format: Some("i".into()),
            start_numbering_at: Some("page".into()),
            footnotes_position: Some("page".into()),
        }],
        ..odf::TextDocument::default()
    };
    let document = import(&source).unwrap();
    let notes = document.text_settings().notes;
    assert_eq!(notes.format, NoteFormat::Roman);
    assert_eq!(notes.numbering, NoteNumbering::Page);
    // And Desk writes it for them.
    let out = export(&document);
    let footnotes = out
        .notes_configuration
        .iter()
        .find(|c| c.class == "footnote")
        .unwrap();
    assert_eq!(footnotes.num_format.as_deref(), Some("i"));
    assert_eq!(footnotes.start_numbering_at.as_deref(), Some("page"));
}

#[test]
fn est_12_the_header_margin_is_kept_while_headers_are_off() {
    use desk_model::{Editor, Length};
    let mut editor = Editor::new(desk_model::Document::new_word_processing());
    editor
        .edit("Margins", |edit| {
            let mut setup = *edit.document().page_setup();
            setup.headers = false;
            setup.footers = false;
            setup.header_margin = Length::from_points(20);
            setup.footer_margin = Length::from_points(50);
            edit.set_page_setup(setup)
        })
        .unwrap();
    let read = read_odt(&write_odt(editor.document()).unwrap()).unwrap();
    let setup = read.page_setup();
    assert!(!setup.headers && !setup.footers);
    assert_eq!(setup.header_margin, Length::from_points(20));
    assert_eq!(setup.footer_margin, Length::from_points(50));
    assert_eq!(setup.margin_top, editor.document().page_setup().margin_top);
}

#[test]
fn int_08_doc_16_the_view_is_kept_in_the_settings() {
    let mut document = Document::new_word_processing();
    assert_eq!(
        read_odt(&write_odt(&document).unwrap()).unwrap().view(),
        None
    );
    for view in [
        desk_model::ViewSettings {
            zoom: desk_model::ViewZoom::Percent(150),
            two_pages: true,
            outline: false,
        },
        desk_model::ViewSettings {
            zoom: desk_model::ViewZoom::FitWidth,
            two_pages: false,
            outline: true,
        },
        desk_model::ViewSettings {
            zoom: desk_model::ViewZoom::FitPage,
            two_pages: false,
            outline: false,
        },
    ] {
        document.set_view(Some(view));
        let bytes = write_odt(&document).unwrap();
        // With the items of LibreOffice: 3 is the width of the page.
        let odf = export(&document).view;
        if view.zoom == desk_model::ViewZoom::FitWidth {
            assert_eq!(odf.zoom_type, Some(3));
        }
        assert_eq!(odf.columns, Some(if view.two_pages { 2 } else { 1 }));
        assert_eq!(read_odt(&bytes).unwrap().view(), Some(view));
    }
}

/// A PNG of one pixel.
fn pixel_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[10, 20, 30, 255]).unwrap();
    }
    bytes
}

/// A document with objects of every kind, fixed on pages and moving with
/// paragraphs.
fn with_objects() -> Document {
    use desk_model::{
        Anchor, Fill, Frame, Object, ObjectKind, Picture, Shape, Stroke, TextBox, Wrap,
    };
    use std::sync::Arc;
    let mut editor = desk_model::Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Objects", |edit| {
            let at = edit.insert_text(start, "First")?;
            let at = edit.split_paragraph(at)?;
            let second = at.paragraph;
            edit.insert_text(at, "Second")?;
            let style = edit.document().styles().default_paragraph();
            let mut square = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::RoundedRectangle {
                    radius: Length::from_points(5),
                }),
                Frame::from_pt(100.0, 200.0, 50.0, 30.0),
            );
            square.anchor = Anchor::Page(1);
            square.rotation = 3000;
            square.flip_horizontal = true;
            square.locked = true;
            square.wrap = Wrap::Left;
            square.wrap_margin = Length::from_points(6);
            square.style.fill = Some(Fill::Color(Rgba::rgb(200, 10, 20)));
            square.style.stroke = Some(Stroke {
                style: BorderStyle::Dashed,
                ..Stroke::new(Rgba::rgb(0, 0, 255), Length::from_points(2))
            });
            square.style.shadow = Some(desk_model::ObjectShadow {
                color: Rgba::rgb(0, 80, 0),
                opacity: 50,
                ..desk_model::ObjectShadow::default()
            });
            square.style.opacity = 80;
            edit.add_object(square)?;
            let mut ellipse = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Ellipse),
                Frame::from_pt(90.0, 12.0, 40.0, 40.0),
            );
            ellipse.anchor = Anchor::Paragraph(second);
            ellipse.wrap = Wrap::Behind;
            ellipse.style.fill = Some(Fill::Gradient {
                from: Rgba::rgb(255, 200, 0),
                to: Rgba::rgb(0, 0, 120),
                angle: 4500,
            });
            let typed = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs::default(),
                [Run::new("In an oval")],
            )?;
            let mut inside = TextBox::in_shape(Arc::from([typed]));
            inside.vertical = desk_model::VerticalAlignment::Bottom;
            inside.inset = Length::from_points(2);
            ellipse.text = Some(inside);
            edit.add_object(ellipse)?;
            let mut line = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Line { rising: true }),
                Frame::from_pt(10.0, 20.0, 100.0, 50.0),
            );
            line.style.stroke = Some(Stroke {
                start: desk_model::LineEnd::Circle,
                end: desk_model::LineEnd::Arrow,
                ..Stroke::new(Rgba::BLACK, Length::from_points(1))
            });
            line.wrap = Wrap::Through;
            edit.add_object(line)?;
            let mut tiled = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Ellipse),
                Frame::from_pt(200.0, 20.0, 60.0, 40.0),
            );
            tiled.style.fill = Some(Fill::Image {
                picture: Picture::from_png(pixel_png()).unwrap(),
                mode: desk_model::ImageFill::ScaleToFit,
            });
            edit.add_object(tiled)?;
            let bold = CharAttrs {
                weight: Some(FontWeight::BOLD),
                ..CharAttrs::default()
            };
            let paragraph = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs {
                    alignment: Some(Alignment::Center),
                    ..ParaAttrs::default()
                },
                [Run::new("In a "), Run::with_attrs("box", None, bold)],
            )?;
            let mut text_box = TextBox::new(Arc::from([paragraph]), Length::from_points(4));
            text_box.vertical = desk_model::VerticalAlignment::Middle;
            text_box.columns = 2;
            text_box.column_gap = Length::from_points(9);
            let mut text = Object::new(
                edit.new_id(),
                ObjectKind::TextBox(text_box),
                Frame::from_pt(300.0, 400.0, 120.0, 60.0),
            );
            text.wrap = Wrap::AboveAndBelow;
            let first_box = text.id;
            edit.add_object(text)?;
            let style = edit.document().styles().default_paragraph();
            let empty = Paragraph::new(edit.new_id(), style);
            let linked = Object::new(
                edit.new_id(),
                ObjectKind::TextBox(TextBox::new(Arc::from([empty]), Length::from_points(4))),
                Frame::from_pt(300.0, 500.0, 120.0, 60.0),
            );
            let linked_id = linked.id;
            edit.add_object(linked)?;
            edit.link_text_box(first_box, linked_id)?;
            let picture = Picture::from_png(pixel_png()).unwrap();
            let mut image = Object::new(
                edit.new_id(),
                ObjectKind::Image(picture),
                Frame::from_pt(50.0, 600.0, 80.0, 80.0),
            );
            image.wrap = Wrap::Largest;
            image.picture = desk_model::PictureSettings {
                mask: Some(Frame::from_pt(-10.0, -20.0, 120.0, 100.0)),
                alpha: Some(desk_model::InstantAlpha {
                    color: Rgba::rgb(255, 255, 255),
                    tolerance: 25,
                }),
                adjust: desk_model::Adjustments {
                    brightness: 10,
                    contrast: -20,
                    saturation: 30,
                    exposure: -5,
                    temperature: 40,
                    tint: -50,
                },
            };
            edit.add_object(image)?;
            let mut triangle = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::triangle()),
                Frame::from_pt(100.0, 5.0, 20.0, 20.0),
            );
            let style = edit.document().styles().default_paragraph();
            let typed =
                Paragraph::from_runs(edit.new_id(), style, ParaAttrs::default(), [Run::new("Up")])?;
            triangle.text = Some(TextBox::in_shape(Arc::from([typed])));
            let circle = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Ellipse),
                Frame::from_pt(130.0, 5.0, 20.0, 20.0),
            );
            let mut group = Object::new(
                edit.new_id(),
                ObjectKind::Group(Arc::from([triangle, circle])),
                Frame::from_pt(100.0, 5.0, 50.0, 20.0),
            );
            group.anchor = Anchor::Paragraph(second);
            edit.add_object(group)
        })
        .unwrap();
    editor.document().clone()
}

/// The objects of a document without their identifiers, with the
/// paragraphs they move with by index and the text of text boxes.
fn objects(document: &Document) -> Vec<String> {
    let index = |id| {
        document
            .body()
            .iter()
            .position(|p| p.id() == id)
            .or_else(|| document.objects().index_of(id).map(|i| 1000 + i))
    };
    fn strip(
        o: &desk_model::Object,
        index: &dyn Fn(desk_model::NodeId) -> Option<usize>,
    ) -> String {
        let mut o = o.clone();
        let anchor = match o.anchor {
            desk_model::Anchor::Paragraph(id) => format!("paragraph {:?}", index(id)),
            desk_model::Anchor::Page(page) => format!("page {page}"),
            desk_model::Anchor::InText => "in the text".to_owned(),
        };
        let kind = match &o.kind {
            desk_model::ObjectKind::TextBox(t) => format!(
                "text {:?} inset {:?} {:?} columns {} gap {:?} next {:?}",
                t.paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>(),
                t.inset,
                t.vertical,
                t.columns,
                t.column_gap,
                t.next.map(index),
            ),
            desk_model::ObjectKind::Group(g) => {
                format!(
                    "group {:?}",
                    g.iter().map(|o| strip(o, index)).collect::<Vec<_>>()
                )
            }
            other => format!("{other:?}"),
        };
        let text = o.text.take().map(|t| {
            format!(
                "with text {:?} inset {:?} {:?} columns {}",
                t.paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>(),
                t.inset,
                t.vertical,
                t.columns,
            )
        });
        let kind = format!("{kind} {text:?}");
        o.id = desk_model::NodeId::new(1).unwrap();
        o.kind = desk_model::ObjectKind::Shape(desk_model::Shape::Rectangle);
        o.anchor = desk_model::Anchor::Page(0);
        // Positions to a hundredth of a point, as ODF writes thousandths.
        let round = |l: Length| Length::from_pt((l.to_pt() * 100.0).round() / 100.0);
        o.frame = desk_model::Frame {
            x: round(o.frame.x),
            y: round(o.frame.y),
            width: round(o.frame.width),
            height: round(o.frame.height),
        };
        format!("{kind} {anchor} {o:?}")
    }
    document
        .objects()
        .iter()
        .map(|o| strip(o, &index))
        .collect()
}

#[test]
fn obj_01_obj_08_obj_13_objects_are_kept_in_odt() {
    let document = with_objects();
    let bytes = write_odt(&document).unwrap();
    keep("obj-objects.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    assert_eq!(objects(&read), objects(&document));
    // The formatting of the text of a text box is kept.
    let desk_model::ObjectKind::TextBox(text) = &read.objects().iter().nth(4).unwrap().kind else {
        panic!("a text box");
    };
    let paragraph = &text.paragraphs[0];
    assert_eq!(paragraph.attrs().alignment, Some(Alignment::Center));
    assert_eq!(
        paragraph.runs().last().unwrap().attrs.weight,
        Some(FontWeight::BOLD)
    );
}

#[test]
fn obj_08_shapes_of_other_programs_are_read() {
    let content = r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
  xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
  xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" office:version="1.3">
 <office:automatic-styles>
  <style:style style:name="gr1" style:family="graphic">
   <style:graphic-properties draw:fill="solid" draw:fill-color="#729fcf" svg:stroke-color="#3465a4"
     style:wrap="dynamic" style:horizontal-rel="paragraph"/>
  </style:style>
 </office:automatic-styles>
 <office:body><office:text>
  <text:p>Text<draw:custom-shape text:anchor-type="paragraph" draw:z-index="0" draw:style-name="gr1"
     svg:width="2cm" svg:height="1cm" svg:x="1cm" svg:y="0.5cm"
     draw:transform="rotate (1.5707963267949) translate (1cm 3cm)">
    <text:p>Peak</text:p>
    <draw:enhanced-geometry draw:type="isosceles-triangle"/>
   </draw:custom-shape></text:p>
 </office:text></office:body>
</office:document-content>"##;
    let source = odf::read_streams(content, None, None).unwrap();
    let document = import(&source).unwrap();
    let object = document.objects().iter().next().unwrap();
    assert_eq!(
        object.kind,
        desk_model::ObjectKind::Shape(desk_model::Shape::triangle())
    );
    assert_eq!(
        object.anchor,
        desk_model::Anchor::Paragraph(document.body().get(0).unwrap().id())
    );
    assert_eq!(object.rotation, 9000);
    // Its text is in its middle.
    let text = object.text_box().unwrap();
    assert_eq!(text.paragraphs[0].text(), "Peak");
    assert_eq!(text.vertical, desk_model::VerticalAlignment::Middle);
    assert_eq!(document.body().get(0).unwrap().text(), "Text");
    assert_eq!(
        object.style.fill,
        Some(desk_model::Fill::Color(Rgba::rgb(0x72, 0x9f, 0xcf)))
    );
    assert_eq!(
        object.style.stroke.unwrap().color,
        Rgba::rgb(0x34, 0x65, 0xa4)
    );
    // Turned a quarter about its corner at (1cm, 3cm) from the margin,
    // its center is a centimeter right and below.
    let cm = 72.0 / 2.54;
    let margin = document.page_setup().margin_left.to_pt();
    let center_x = object.frame.x.to_pt() + 1.0 * cm;
    let center_y = object.frame.y.to_pt() + 0.5 * cm;
    assert!((center_x - (margin + 1.5 * cm)).abs() < 0.01, "{center_x}");
    assert!((center_y - 2.0 * cm).abs() < 0.01, "{center_y}");
    let _ = export(&document);
}

#[test]
fn obj_09_obj_10_obj_11_effects_of_other_programs_are_read() {
    let ns = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
  xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"
  xmlns:loext="urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0"
  office:version="1.3""#;
    let styles = format!(
        r##"<office:document-styles {ns}><office:styles>
  <draw:gradient draw:name="Sunrise" draw:style="linear" draw:start-color="#ff0000"
    draw:end-color="#0000ff" draw:angle="90deg" draw:border="0%"/>
  <draw:marker draw:name="Line_20_Arrow" svg:viewBox="0 0 10 10" svg:d="M0 0h10v10z"/>
  <draw:marker draw:name="Circle" svg:viewBox="0 0 10 10" svg:d="M0 0h10v10z"/>
 </office:styles></office:document-styles>"##
    );
    let content = format!(
        r##"<office:document-content {ns}>
 <office:automatic-styles>
  <style:style style:name="gr1" style:family="graphic">
   <style:graphic-properties draw:fill="gradient" draw:fill-gradient-name="Sunrise"
     draw:shadow="visible" draw:shadow-offset-x="3pt" draw:shadow-offset-y="-3pt"
     draw:shadow-color="#808080" draw:shadow-opacity="40%" loext:shadow-blur="2pt"/>
  </style:style>
  <style:style style:name="gr2" style:family="graphic">
   <style:graphic-properties draw:stroke="solid" draw:marker-start="Circle"
     draw:marker-end="Line_20_Arrow"/>
  </style:style>
 </office:automatic-styles>
 <office:body><office:text>
  <text:p>Text<draw:rect text:anchor-type="paragraph" draw:style-name="gr1"
     svg:width="2cm" svg:height="1cm" svg:x="1cm" svg:y="0.5cm"/><draw:line
     text:anchor-type="paragraph" draw:style-name="gr2"
     svg:x1="0cm" svg:y1="0cm" svg:x2="2cm" svg:y2="1cm"/></text:p>
 </office:text></office:body>
</office:document-content>"##
    );
    let source = odf::read_streams(&content, Some(&styles), None).unwrap();
    let document = import(&source).unwrap();
    let mut objects = document.objects().iter();
    let rect = objects.next().unwrap();
    // ODF turns a gradient from going down; at a quarter, it goes right.
    assert_eq!(
        rect.style.fill,
        Some(desk_model::Fill::Gradient {
            from: Rgba::rgb(255, 0, 0),
            to: Rgba::rgb(0, 0, 255),
            angle: 0,
        })
    );
    let shadow = rect.style.shadow.unwrap();
    assert_eq!(shadow.color, Rgba::rgb(0x80, 0x80, 0x80));
    assert_eq!(shadow.opacity, 40);
    assert_eq!(shadow.angle, 4500);
    assert_eq!(shadow.offset, Length::from_pt(4.2));
    assert_eq!(shadow.blur, Length::from_points(2));
    let line = objects.next().unwrap().style.stroke.unwrap();
    assert_eq!(line.start, desk_model::LineEnd::Circle);
    assert_eq!(line.end, desk_model::LineEnd::OpenArrow);
}

#[test]
fn obj_01_objects_in_the_text_are_kept_in_odt() {
    use desk_model::{FIELD_CHARACTER, Frame, Object, Picture, Position, Shape};
    let mut editor = desk_model::Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let (image, oval) = editor
        .edit("Inline", |edit| {
            let at = edit.insert_text(start, "ab cd")?;
            let image = Object::new(
                edit.new_id(),
                desk_model::ObjectKind::Image(Picture::from_png(pixel_png()).unwrap()),
                Frame::from_pt(0.0, 0.0, 40.0, 30.0),
            );
            let oval = Object::new(
                edit.new_id(),
                Shape::Ellipse.into(),
                Frame::from_pt(0.0, 0.0, 20.0, 10.0),
            );
            let (i, o) = (image.id, oval.id);
            edit.insert_object_in_text(at, oval)?;
            edit.insert_object_in_text(Position::new(at.paragraph, 2), image)?;
            Ok((i, o))
        })
        .unwrap();
    let document = editor.document().clone();
    let bytes = write_odt(&document).unwrap();
    keep("obj-inline.odt", &bytes);
    let exported = export(&document);
    let inline: Vec<&odf::DrawObject> = exported.body[0]
        .spans
        .iter()
        .filter_map(|s| match &s.field {
            Some(odf::Field::Object(o)) => Some(o.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(inline.len(), 2);
    assert!(
        inline
            .iter()
            .all(|o| o.anchor.as_deref() == Some("as-char") && o.name.is_none())
    );
    assert!(exported.body[0].frames.is_empty());
    let read = read_odt(&bytes).unwrap();
    let paragraph = read.body().get(0).unwrap();
    let mark = FIELD_CHARACTER;
    assert_eq!(paragraph.text(), format!("ab{mark} cd{mark}"));
    let marks = read.object_marks();
    assert_eq!(marks.len(), 2);
    let mut by_offset: Vec<(usize, &desk_model::Object)> = marks
        .iter()
        .map(|(id, at)| (at.offset, read.objects().get(*id).unwrap()))
        .collect();
    by_offset.sort_by_key(|(offset, _)| *offset);
    let original = |id| document.objects().get(id).unwrap();
    for ((_, read), id) in by_offset.iter().zip([image, oval]) {
        assert_eq!(read.anchor, desk_model::Anchor::InText);
        assert_eq!(read.kind, original(id).kind);
        assert_eq!(read.frame, original(id).frame);
    }
}

#[test]
fn obj_01_objects_in_the_text_of_other_programs_are_read() {
    let content = r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
  xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
  xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" office:version="1.3">
 <office:body><office:text>
  <text:p>One <text:span><draw:rect text:anchor-type="as-char" svg:width="1in" svg:height="0.5in"/></text:span> two<draw:frame text:anchor-type="as-char" svg:width="1in" svg:height="1in"><draw:image xlink:href="Pictures/missing.png" xmlns:xlink="http://www.w3.org/1999/xlink"/></draw:frame></text:p>
 </office:text></office:body>
</office:document-content>"##;
    let source = odf::read_streams(content, None, None).unwrap();
    let document = import(&source).unwrap();
    // The picture that is not in the package is left out, mark and all.
    assert_eq!(document.objects().len(), 1);
    let object = document.objects().iter().next().unwrap();
    assert_eq!(object.anchor, desk_model::Anchor::InText);
    assert_eq!(object.frame.width, Length::from_points(72));
    let paragraph = document.body().get(0).unwrap();
    assert_eq!(
        paragraph.text(),
        format!("One {} two", desk_model::FIELD_CHARACTER)
    );
    assert_eq!(document.object_mark(object.id).map(|p| p.offset), Some(4));
}
