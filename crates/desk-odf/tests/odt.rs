// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Reading and writing text documents.

use desk_odf::{
    Background, Border, BorderLine, Break, Color, Family, LINE_BREAK, LeaderStyle, Length, Line,
    LineColor, LineHeight, Meta, OdfError, PageLayout, Paragraph, ParagraphProperties, Properties,
    Span, Style, TabStop, TabType, TextAlign, TextDocument, TextPosition, TextProperties,
    TextShadow, TextTransform, read_odt, read_streams, write_odt, write_odt_with_thumbnail,
};

const CONTENT_START: &str = r#"<office:document-content
    xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
    xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
    xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
    xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
    xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
    xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0">"#;

fn content(body: &str) -> String {
    format!(
        "{CONTENT_START}<office:body><office:text>{body}</office:text></office:body></office:document-content>"
    )
}

/// Keeps a written file for ci/validate-odf.sh, which validates it against
/// the ODF schemas, when `DESK_ODF_OUT` names a directory.
fn keep(name: &str, bytes: &[u8]) {
    if let Some(dir) = std::env::var_os("DESK_ODF_OUT") {
        std::fs::write(std::path::Path::new(&dir).join(name), bytes).unwrap();
    }
}

fn texts(document: &TextDocument) -> Vec<String> {
    document.body.iter().map(Paragraph::text).collect()
}

fn span(style: Option<&str>, text: &str) -> Span {
    Span {
        style: style.map(str::to_owned),
        text: text.to_owned(),
    }
}

fn full_document() -> TextDocument {
    TextDocument {
        meta: Meta {
            title: Some("Relatório & <notas>".into()),
            subject: Some("Assunto".into()),
            description: Some("Comentários".into()),
            keywords: vec!["um".into(), "dois".into()],
            initial_creator: Some("Brahm".into()),
            creator: Some("Brahm".into()),
            generator: Some("Desk".into()),
        },
        default_style: Properties {
            paragraph: ParagraphProperties {
                widows: Some(2),
                orphans: Some(2),
                ..ParagraphProperties::default()
            },
            text: TextProperties {
                font_family: Some("Liberation Serif".into()),
                font_size: Some(Length(12.0)),
                language: Some("pt".into()),
                country: Some("BR".into()),
                ..TextProperties::default()
            },
        },
        styles: vec![
            Style {
                name: "Body".into(),
                family: Family::Paragraph,
                next: Some("Body".into()),
                properties: Properties {
                    paragraph: ParagraphProperties {
                        align: Some(TextAlign::Justify),
                        line_height: Some(LineHeight::Percent(150.0)),
                        margin_top: Some(Length(6.0)),
                        margin_bottom: Some(Length(12.0)),
                        margin_left: Some(Length(36.0)),
                        margin_right: Some(Length(18.0)),
                        text_indent: Some(Length(-18.0)),
                        tab_stops: Some(vec![
                            TabStop {
                                position: Length(72.0),
                                ..TabStop::default()
                            },
                            TabStop {
                                position: Length(144.0),
                                kind: TabType::Char,
                                char: Some(','),
                                leader_style: LeaderStyle::Dotted,
                                leader_text: Some('.'),
                            },
                            TabStop {
                                position: Length(200.0),
                                kind: TabType::Right,
                                ..TabStop::default()
                            },
                        ]),
                        tab_stop_distance: Some(Length(35.4)),
                        ..ParagraphProperties::default()
                    },
                    text: TextProperties::default(),
                },
                ..Style::default()
            },
            Style {
                name: "Heading_20_1".into(),
                display_name: Some("Heading 1".into()),
                family: Family::Paragraph,
                parent: Some("Body".into()),
                next: Some("Body".into()),
                properties: Properties {
                    paragraph: ParagraphProperties {
                        break_before: Some(Break::Page),
                        keep_with_next: Some(true),
                        keep_together: Some(true),
                        line_height_at_least: Some(Length(20.0)),
                        // No stops, over those of Body.
                        tab_stops: Some(Vec::new()),
                        ..ParagraphProperties::default()
                    },
                    text: TextProperties {
                        font_family: Some("Liberation Sans".into()),
                        font_size: Some(Length(18.0)),
                        font_weight: Some(700),
                        ..TextProperties::default()
                    },
                },
            },
            Style {
                name: "Emphasis".into(),
                family: Family::Text,
                properties: Properties {
                    text: TextProperties {
                        italic: Some(true),
                        ..TextProperties::default()
                    },
                    ..Properties::default()
                },
                ..Style::default()
            },
        ],
        automatic_styles: vec![
            Style {
                name: "P1".into(),
                family: Family::Paragraph,
                parent: Some("Body".into()),
                properties: Properties {
                    paragraph: ParagraphProperties {
                        line_height: Some(LineHeight::Exactly(Length(14.5))),
                        line_spacing: Some(Length(3.0)),
                        background: Some(Background::Color(Color {
                            r: 255,
                            g: 255,
                            b: 117,
                        })),
                        border_top: Some(Border::Line {
                            width: Length(1.0),
                            style: BorderLine::Dashed,
                            color: Color { r: 0, g: 0, b: 0 },
                        }),
                        border_bottom: Some(Border::None),
                        padding_top: Some(Length(6.0)),
                        ..ParagraphProperties::default()
                    },
                    ..Properties::default()
                },
                ..Style::default()
            },
            Style {
                name: "T1".into(),
                family: Family::Text,
                properties: Properties {
                    text: TextProperties {
                        underline: Some(Line::Double),
                        line_through: Some(Line::Single),
                        line_through_color: Some(LineColor::Color(Color { r: 255, g: 0, b: 0 })),
                        color: Some(Color { r: 0, g: 0, b: 255 }),
                        position: Some(TextPosition::Super),
                        position_scale: Some(66.0),
                        transform: Some(TextTransform::Uppercase),
                        small_caps: Some(true),
                        letter_spacing: Some(Length(2.4)),
                        outline: Some(true),
                        shadow: Some(TextShadow::Shadow {
                            dx: Length(2.0),
                            dy: Length(3.464),
                            blur: Some(Length(3.0)),
                            color: Some(Color {
                                r: 64,
                                g: 64,
                                b: 64,
                            }),
                        }),
                        background: Some(Background::Transparent),
                        ..TextProperties::default()
                    },
                    ..Properties::default()
                },
                ..Style::default()
            },
            Style {
                name: "T2".into(),
                family: Family::Text,
                properties: Properties {
                    text: TextProperties {
                        position: Some(TextPosition::Percent(-15.0)),
                        line_through_color: Some(LineColor::FontColor),
                        font_size_percent: Some(80.0),
                        letter_spacing: Some(Length(0.0)),
                        shadow: Some(TextShadow::None),
                        ..TextProperties::default()
                    },
                    ..Properties::default()
                },
                ..Style::default()
            },
        ],
        page_layout: PageLayout {
            width: Some(Length(612.0)),
            height: Some(Length(792.0)),
            margin_top: Some(Length(72.0)),
            margin_bottom: Some(Length(54.0)),
            margin_left: Some(Length(90.0)),
            margin_right: Some(Length(36.0)),
        },
        body: vec![
            Paragraph {
                style: Some("Heading_20_1".into()),
                outline_level: Some(1),
                spans: vec![span(None, "Título")],
            },
            Paragraph {
                style: Some("P1".into()),
                outline_level: None,
                spans: vec![
                    span(None, "  two leading spaces, a\ttab, "),
                    span(Some("T1"), "styled   three"),
                    span(None, " after a space "),
                    span(Some("Emphasis"), " and"),
                    span(Some("T2"), &format!("a{LINE_BREAK}line break")),
                ],
            },
            Paragraph::default(),
        ],
        tabs_relative_to_indent: None,
    }
}

#[test]
fn a_document_comes_back_as_it_was_written() {
    let document = full_document();
    let bytes = write_odt(&document).unwrap();
    keep("odf-full.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    assert_eq!(read, document);
}

#[test]
fn fmt_16_tab_stops_are_measured_as_the_settings_say() {
    let mut bytes = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("content.xml", options).unwrap();
        std::io::Write::write_all(&mut zip, content("<text:p>a</text:p>").as_bytes()).unwrap();
        zip.start_file("settings.xml", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            br#"<office:document-settings
                xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
                xmlns:config="urn:oasis:names:tc:opendocument:xmlns:config:1.0">
              <office:settings><config:config-item-set config:name="ooo:configuration-settings">
                <config:config-item config:name="TabsRelativeToIndent" config:type="boolean">false</config:config-item>
              </config:config-item-set></office:settings></office:document-settings>"#,
        )
        .unwrap();
        zip.finish().unwrap();
    }
    assert_eq!(
        read_odt(&bytes).unwrap().tabs_relative_to_indent,
        Some(false)
    );
    // Without settings, LibreOffice measures them from the indent.
    assert_eq!(
        read_odt(&write_odt(&full_document()).unwrap())
            .unwrap()
            .tabs_relative_to_indent,
        None
    );
}

#[test]
fn doc_09_a_preview_goes_in_the_package_and_its_manifest() {
    let png = b"\x89PNG\r\n\x1a\n not really";
    let bytes = write_odt_with_thumbnail(&full_document(), Some(png)).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut stored = Vec::new();
    std::io::Read::read_to_end(
        &mut archive.by_name(desk_odf::THUMBNAIL).unwrap(),
        &mut stored,
    )
    .unwrap();
    assert_eq!(stored, png);
    let mut manifest = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("META-INF/manifest.xml").unwrap(),
        &mut manifest,
    )
    .unwrap();
    assert!(
        manifest.contains(
            r#"manifest:full-path="Thumbnails/thumbnail.png" manifest:media-type="image/png""#
        ),
        "{manifest}"
    );
    assert_eq!(read_odt(&bytes).unwrap(), full_document());
}

#[test]
fn the_package_starts_with_the_media_type_stored() {
    let bytes = write_odt(&TextDocument::default()).unwrap();
    // The local header of the first entry, then its name and its content.
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    assert_eq!(&bytes[30..38], b"mimetype");
    assert_eq!(
        &bytes[38..38 + 39],
        b"application/vnd.oasis.opendocument.text"
    );
}

#[test]
fn white_space_collapses_as_odf_says() {
    let document = read_streams(
        &content(
            "<text:p>  one \n  two<text:span> three </text:span> four<text:s text:c=\"3\"/>five</text:p>\
             <text:p><text:s/>six<text:tab/>seven<text:line-break/>eight</text:p>",
        ),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        texts(&document),
        [
            "one two three four   five".to_owned(),
            format!(" six\tseven{LINE_BREAK}eight")
        ]
    );
}

#[test]
fn lists_sections_and_tables_give_their_paragraphs() {
    let document = read_streams(
        &content(
            "<text:h text:outline-level=\"2\">Heading</text:h>\
             <text:list><text:list-item><text:p>item</text:p></text:list-item></text:list>\
             <text:section><text:p>in a section</text:p></text:section>\
             <text:p>with <text:a>a link</text:a><text:note><text:note-body><text:p>note</text:p></text:note-body></text:note>\
             <draw:frame><draw:text-box><text:p>frame</text:p></draw:text-box></draw:frame>.</text:p>",
        ),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        texts(&document),
        ["Heading", "item", "in a section", "with a link."]
    );
    assert_eq!(document.body[0].outline_level, Some(2));
}

#[test]
fn font_names_resolve_through_the_declarations() {
    let document = read_streams(
        &format!(
            "{CONTENT_START}<office:font-face-decls><style:font-face style:name=\"F1\" svg:font-family=\"&apos;Times New Roman&apos;\"/></office:font-face-decls>\
             <office:automatic-styles>\
             <style:style style:name=\"T1\" style:family=\"text\"><style:text-properties style:font-name=\"F1\"/></style:style>\
             <style:style style:name=\"T2\" style:family=\"text\"><style:text-properties fo:font-family=\"Courier\" fo:font-weight=\"600\" fo:font-style=\"oblique\"/></style:style>\
             </office:automatic-styles><office:body><office:text/></office:body></office:document-content>"
        ),
        None,
        None,
    )
    .unwrap();
    let t1 = &document.style(Family::Text, "T1").unwrap().properties.text;
    assert_eq!(t1.font_family.as_deref(), Some("Times New Roman"));
    let t2 = &document.style(Family::Text, "T2").unwrap().properties.text;
    assert_eq!(t2.font_family.as_deref(), Some("Courier"));
    assert_eq!(t2.font_weight, Some(600));
    assert_eq!(t2.italic, Some(true));
}

#[test]
fn font_families_are_css_lists() {
    let document = read_streams(
        &format!(
            "{CONTENT_START}<office:automatic-styles>\
             <style:style style:name=\"T1\" style:family=\"text\"><style:text-properties fo:font-family=\"&quot;Gill Sans&quot;, serif\"/></style:style>\
             <style:style style:name=\"T2\" style:family=\"text\"><style:text-properties fo:font-family=\" Gill   Sans , serif\"/></style:style>\
             <style:style style:name=\"T3\" style:family=\"text\"><style:text-properties fo:font-family=\"'O\\'Neil, Bold'\"/></style:style>\
             </office:automatic-styles><office:body><office:text/></office:body></office:document-content>"
        ),
        None,
        None,
    )
    .unwrap();
    let family = |name| {
        document
            .style(Family::Text, name)
            .unwrap()
            .properties
            .text
            .font_family
            .clone()
    };
    assert_eq!(family("T1").as_deref(), Some("Gill Sans"));
    assert_eq!(family("T2").as_deref(), Some("Gill Sans"));
    assert_eq!(family("T3").as_deref(), Some("O'Neil, Bold"));

    // Found by the fuzzer: a quote in a family name was written unescaped.
    let mut written = TextDocument::default();
    written.automatic_styles.push(Style {
        name: "T1".into(),
        display_name: None,
        family: Family::Text,
        parent: None,
        next: None,
        properties: Properties {
            text: TextProperties {
                font_family: Some("O'Neil \\ Sans".into()),
                ..TextProperties::default()
            },
            ..Properties::default()
        },
    });
    let read = read_odt(&write_odt(&written).unwrap()).unwrap();
    assert_eq!(read.automatic_styles, written.automatic_styles);
}

#[test]
fn fmt_03_fmt_20_fmt_21_effects_read_as_other_programs_write_them() {
    let document = read_streams(
        &format!(
            "{CONTENT_START}<office:automatic-styles>\
             <style:style style:name=\"P1\" style:family=\"paragraph\">\
             <style:paragraph-properties fo:border=\"0.06pt solid #000080\" fo:padding=\"0.2cm\" fo:border-left=\"none\" fo:background-color=\"transparent\"/>\
             <style:text-properties fo:text-shadow=\"#808080 1pt 1pt\" fo:letter-spacing=\"-0.5pt\" style:text-outline=\"false\" fo:background-color=\"#ffff00\"/>\
             </style:style>\
             <style:style style:name=\"P2\" style:family=\"paragraph\">\
             <style:paragraph-properties fo:border-bottom=\"thin dotted\"/>\
             <style:text-properties fo:text-shadow=\"1pt\"/>\
             </style:style>\
             </office:automatic-styles><office:body><office:text/></office:body></office:document-content>"
        ),
        None,
        None,
    )
    .unwrap();
    let p1 = &document.style(Family::Paragraph, "P1").unwrap().properties;
    let line = Border::Line {
        width: Length(0.06),
        style: BorderLine::Solid,
        color: Color { r: 0, g: 0, b: 128 },
    };
    assert_eq!(p1.paragraph.border_top, Some(line));
    assert_eq!(p1.paragraph.border_right, Some(line));
    assert_eq!(p1.paragraph.border_left, Some(Border::None));
    assert!((p1.paragraph.padding_bottom.unwrap().0 - 5.669).abs() < 0.01);
    assert_eq!(p1.paragraph.background, Some(Background::Transparent));
    assert_eq!(
        p1.text.shadow,
        Some(TextShadow::Shadow {
            dx: Length(1.0),
            dy: Length(1.0),
            blur: None,
            color: Some(Color {
                r: 128,
                g: 128,
                b: 128
            }),
        })
    );
    assert_eq!(p1.text.letter_spacing, Some(Length(-0.5)));
    assert_eq!(p1.text.outline, Some(false));
    let p2 = &document.style(Family::Paragraph, "P2").unwrap().properties;
    assert_eq!(
        p2.paragraph.border_bottom,
        Some(Border::Line {
            width: Length(0.75),
            style: BorderLine::Dotted,
            color: Color { r: 0, g: 0, b: 0 },
        })
    );
    // A shadow with one length does not parse.
    assert_eq!(p2.text.shadow, None);
}

#[test]
fn hostile_input_is_refused_cleanly() {
    assert!(matches!(read_odt(b"not a zip"), Err(OdfError::Zip(_))));

    let deep = format!(
        "{CONTENT_START}<office:body><office:text>{}{}</office:text></office:body></office:document-content>",
        "<text:span>".repeat(1000),
        "</text:span>".repeat(1000)
    );
    assert!(matches!(
        read_streams(&deep, None, None),
        Err(OdfError::Malformed(_))
    ));
    assert!(read_streams("<office:document-content", None, None).is_err());
    assert!(read_streams(&content("<text:p>&unknown;</text:p>"), None, None).is_err());
    assert!(matches!(
        read_streams("<x/>", None, None),
        Err(OdfError::Malformed(_))
    ));

    // A huge space count is capped.
    let document = read_streams(
        &content("<text:p><text:s text:c=\"999999999\"/></text:p>"),
        None,
        None,
    )
    .unwrap();
    assert_eq!(document.body[0].text().len(), 1024);
}

#[test]
fn a_spreadsheet_is_not_a_text_document() {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("mimetype", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"application/vnd.oasis.opendocument.spreadsheet")
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert!(matches!(read_odt(&bytes), Err(OdfError::WrongType(_))));

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("mimetype", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(desk_odf::MEDIA_TYPE.as_bytes()).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert!(matches!(
        read_odt(&bytes),
        Err(OdfError::Missing("content.xml"))
    ));
}

/// Found by the fuzzer: a scale with a position that does not parse was
/// kept, and then lost on writing.
#[test]
fn a_text_position_that_does_not_parse_is_dropped_whole() {
    let document = read_streams(
        &format!(
            "{CONTENT_START}<office:automatic-styles>\
             <style:style style:name=\"T1\" style:family=\"text\"><style:text-properties style:text-position=\"high 58%\"/></style:style>\
             </office:automatic-styles><office:body><office:text/></office:body></office:document-content>"
        ),
        None,
        None,
    )
    .unwrap();
    let text = &document.style(Family::Text, "T1").unwrap().properties.text;
    assert_eq!((text.position, text.position_scale), (None, None));
}
