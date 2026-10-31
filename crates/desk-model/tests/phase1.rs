// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Model tests for the rows of the parity matrix in phase 1. Each test
//! name starts with the row ID. These cover only the model layer: a row is
//! done when ODF, layout and drawing pass too.

use desk_model::{
    Alignment, Baseline, Capitalization, CharAttrs, CharacterStyle, Document, Editor, LINE_BREAK,
    Language, Length, LineColor, LineSpacing, LineStyle, Metadata, ModelError, NO_BREAK_SPACE, Op,
    PageSetup, ParaAttrs, ParagraphStyle, Position, Rgba, Style,
};

fn editor_with(text: &str) -> Editor {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Typing", |edit| edit.insert_text(start, text))
        .unwrap();
    editor
}

fn first_paragraph(editor: &Editor) -> desk_model::Paragraph {
    editor.document().body().get(0).unwrap().clone()
}

#[test]
fn int_02_text_flows_in_one_body() {
    let editor = editor_with("one\ntwo\nthree");
    let body = editor.document().body();
    assert_eq!(body.len(), 3);
    let texts: Vec<_> = body.iter().map(|p| p.text()).collect();
    assert_eq!(texts, ["one", "two", "three"]);
}

#[test]
fn fmt_01_bold_italic_and_underline_resolve() {
    let mut editor = editor_with("plain bold");
    let id = first_paragraph(&editor).id();
    editor
        .edit("Format", |edit| {
            edit.set_character_attrs(
                Position::new(id, 6),
                Position::new(id, 10),
                &CharAttrs {
                    bold: Some(true),
                    italic: Some(true),
                    underline: Some(LineStyle::Double),
                    ..CharAttrs::default()
                },
            )
        })
        .unwrap();
    let doc = editor.document();
    let paragraph = first_paragraph(&editor);
    let runs: Vec<_> = paragraph.runs().collect();
    assert_eq!(runs.len(), 2);
    let format = doc
        .styles()
        .resolve_character(paragraph.style(), runs[1].style, &runs[1].attrs);
    assert!(format.bold && format.italic);
    assert_eq!(format.underline, LineStyle::Double);
    let plain = doc
        .styles()
        .resolve_character(paragraph.style(), runs[0].style, &runs[0].attrs);
    assert!(!plain.bold);
}

#[test]
fn fmt_02_strikethrough_has_its_own_color() {
    let mut editor = editor_with("struck");
    let (start, end) = (editor.document().start(), editor.document().end());
    let red = Rgba::rgb(255, 0, 0);
    editor
        .edit("Strikethrough", |edit| {
            edit.set_character_attrs(
                start,
                end,
                &CharAttrs {
                    strikethrough: Some(LineStyle::Single),
                    strikethrough_color: Some(LineColor::Custom(red)),
                    ..CharAttrs::default()
                },
            )
        })
        .unwrap();
    let paragraph = first_paragraph(&editor);
    let run = paragraph.runs().next().unwrap();
    let format =
        editor
            .document()
            .styles()
            .resolve_character(paragraph.style(), run.style, &run.attrs);
    assert_eq!(format.strikethrough, LineStyle::Single);
    assert_eq!(format.strikethrough_color, LineColor::Custom(red));
    assert_eq!(format.color, Rgba::BLACK);
}

#[test]
fn fmt_05_06_07_11_22_character_attributes_inherit_from_styles() {
    let mut doc = Document::new_word_processing();
    let body = doc.styles().default_paragraph();
    let mut body_style = doc.styles().paragraph(body).unwrap().clone();
    body_style.character = CharAttrs {
        font_size: Some(Length::from_points(11)),
        language: Some(Language::new("pt-BR")),
        ..CharAttrs::default()
    };
    doc.put_style(body_style).unwrap();
    let emphasis = doc.new_id();
    let mut style = CharacterStyle::new(emphasis, "Emphasis");
    style.character = CharAttrs {
        color: Some(Rgba::rgb(0, 0, 128)),
        capitalization: Some(Capitalization::SmallCaps),
        ..CharAttrs::default()
    };
    doc.put_style(style).unwrap();

    let format = doc.styles().resolve_character(
        body,
        Some(emphasis),
        &CharAttrs {
            baseline: Some(Baseline::Superscript),
            baseline_shift: Some(Length::from_pt(1.5)),
            ..CharAttrs::default()
        },
    );
    assert_eq!(format.font_size, Length::from_points(11));
    assert_eq!(format.language.tag(), "pt-BR");
    assert_eq!(format.color, Rgba::rgb(0, 0, 128));
    assert_eq!(format.capitalization, Capitalization::SmallCaps);
    assert_eq!(format.baseline, Baseline::Superscript);
    assert_eq!(format.baseline_shift, Length::from_millipoints(1500));
}

#[test]
fn fmt_12_14_15_18_paragraph_attributes_inherit_from_styles() {
    let mut doc = Document::new_word_processing();
    let body = doc.styles().default_paragraph();
    let quote = doc.new_id();
    let mut style = ParagraphStyle::new(quote, "Quote");
    style.parent = Some(body);
    style.paragraph = ParaAttrs {
        alignment: Some(Alignment::Justify),
        left_indent: Some(Length::from_points(36)),
        first_line_indent: Some(-Length::from_points(18)),
        space_after: Some(Length::from_points(6)),
        ..ParaAttrs::default()
    };
    doc.put_style(style).unwrap();

    let format = doc.styles().resolve_paragraph(
        quote,
        &ParaAttrs {
            line_spacing: Some(LineSpacing::Between(Length::from_points(4))),
            right_indent: Some(Length::from_points(36)),
            ..ParaAttrs::default()
        },
    );
    assert_eq!(format.alignment, Alignment::Justify);
    assert_eq!(format.left_indent, Length::from_points(36));
    assert_eq!(format.first_line_indent, Length::from_points(-18));
    assert_eq!(format.right_indent, Length::from_points(36));
    assert_eq!(format.space_after, Length::from_points(6));
    assert_eq!(
        format.line_spacing,
        LineSpacing::Between(Length::from_points(4))
    );
}

#[test]
fn styles_refuse_cycles_and_wrong_families() {
    let mut doc = Document::new_word_processing();
    let body = doc.styles().default_paragraph();
    let a = doc.new_id();
    let mut style_a = ParagraphStyle::new(a, "A");
    style_a.parent = Some(body);
    doc.put_style(style_a).unwrap();

    let mut body_style = doc.styles().paragraph(body).unwrap().clone();
    body_style.parent = Some(a);
    assert_eq!(doc.put_style(body_style), Err(ModelError::StyleCycle(body)));

    let c = doc.new_id();
    let mut character = CharacterStyle::new(c, "C");
    character.parent = Some(a);
    assert!(matches!(
        doc.put_style(character),
        Err(ModelError::WrongStyleFamily { .. })
    ));
    assert_eq!(
        doc.apply(Op::RemoveStyle { style: body }),
        Err(ModelError::DefaultStyle)
    );
}

#[test]
fn styles_in_use_cannot_be_removed() {
    let mut editor = editor_with("text");
    let style = ParagraphStyle::new(editor.snapshot().new_id(), "Heading");
    let id = style.id;
    let (start, end) = (editor.document().start(), editor.document().end());
    editor
        .edit("Style", |edit| {
            edit.apply(Op::PutStyle {
                style: Style::Paragraph(style),
            })?;
            edit.set_paragraph_style(start, end, id)
        })
        .unwrap();
    let result = editor.edit("Remove", |edit| edit.apply(Op::RemoveStyle { style: id }));
    assert_eq!(result, Err(ModelError::StyleInUse(id)));
}

#[test]
fn txt_08_no_break_space_is_text() {
    let text = format!("10{NO_BREAK_SPACE}km");
    let editor = editor_with(&text);
    assert_eq!(editor.document().body().len(), 1);
    assert_eq!(first_paragraph(&editor).text(), text);
    assert_eq!(editor.document().statistics().words, 2);
}

#[test]
fn est_02_page_size_and_orientation() {
    let page = PageSetup::a4();
    assert!(!page.is_landscape());
    let landscape = page.rotated();
    assert!(landscape.is_landscape());
    assert_eq!(landscape.width, page.height);
    assert_eq!(landscape.rotated(), page);

    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Page Setup", |edit| {
            edit.set_page_setup(PageSetup::letter())
        })
        .unwrap();
    assert_eq!(editor.document().page_setup(), &PageSetup::letter());
    editor.undo().unwrap();
    assert_eq!(editor.document().page_setup(), &PageSetup::a4());
}

#[test]
fn est_03_default_margins_are_one_inch() {
    let page = PageSetup::letter();
    for margin in [
        page.margin_top,
        page.margin_bottom,
        page.margin_left,
        page.margin_right,
    ] {
        assert_eq!(margin, Length::from_in(1.0));
    }
    assert_eq!(page.body_width(), Length::from_pt(6.5 * 72.0));
}

#[test]
fn est_04_05_page_break_belongs_to_the_first_half_only() {
    let mut editor = editor_with("first second");
    let id = first_paragraph(&editor).id();
    editor
        .edit("Pagination", |edit| {
            edit.set_paragraph_attrs(
                Position::new(id, 0),
                Position::new(id, 0),
                &ParaAttrs {
                    page_break_before: Some(true),
                    keep_with_next: Some(true),
                    keep_lines_together: Some(true),
                    widow_control: Some(true),
                    ..ParaAttrs::default()
                },
            )
        })
        .unwrap();
    editor
        .edit("Typing", |edit| edit.split_paragraph(Position::new(id, 6)))
        .unwrap();
    let body = editor.document().body();
    let first = body.get(0).unwrap().attrs();
    let second = body.get(1).unwrap().attrs();
    assert_eq!(first.page_break_before, Some(true));
    assert_eq!(second.page_break_before, None);
    assert_eq!(second.keep_with_next, Some(true));
    assert_eq!(second.widow_control, Some(true));
}

#[test]
fn est_06_manual_line_break_stays_in_the_paragraph() {
    let text = format!("line one{LINE_BREAK}line two");
    let editor = editor_with(&text);
    assert_eq!(editor.document().body().len(), 1);
    assert_eq!(first_paragraph(&editor).text(), text);
}

#[test]
fn doc_19_metadata_is_undoable() {
    let mut editor = Editor::new(Document::new_word_processing());
    let metadata = Metadata {
        title: "Relatório".into(),
        author: "Brahm".into(),
        comments: "Rascunho".into(),
        keywords: vec!["desk".into(), "teste".into()],
    };
    editor
        .edit("Document Info", |edit| edit.set_metadata(metadata.clone()))
        .unwrap();
    assert_eq!(editor.document().metadata(), &metadata);
    editor.undo().unwrap();
    assert_eq!(editor.document().metadata(), &Metadata::default());
    editor.redo().unwrap();
    assert_eq!(editor.document().metadata(), &metadata);
}

#[test]
fn doc_20_statistics_of_document_and_selection() {
    let editor = editor_with("Olá mundo.\n\nDuas palavras");
    let doc = editor.document();
    let statistics = doc.statistics();
    assert_eq!(statistics.paragraphs, 3);
    assert_eq!(statistics.words, 4);
    assert_eq!(statistics.characters, 23);
    assert_eq!(statistics.characters_without_spaces, 21);

    let first = doc.body().get(0).unwrap().id();
    let selection = doc
        .statistics_between(Position::new(first, 0), Position::new(first, 4))
        .unwrap();
    assert_eq!(selection.words, 1);
    assert_eq!(selection.characters, 3);
}

#[test]
fn return_at_the_end_uses_the_next_style() {
    let mut doc = Document::new_word_processing();
    let body = doc.styles().default_paragraph();
    let heading = doc.new_id();
    let mut style = ParagraphStyle::new(heading, "Heading 1");
    style.next = Some(body);
    doc.put_style(style).unwrap();
    let mut editor = Editor::new(doc);
    let (start, end) = (editor.document().start(), editor.document().end());
    editor
        .edit("Style", |edit| {
            edit.set_paragraph_style(start, end, heading)
        })
        .unwrap();
    let end = editor
        .edit("Typing", |edit| edit.insert_text(start, "Title\nBody text"))
        .unwrap();
    let body_flow = editor.document().body();
    assert_eq!(body_flow.get(0).unwrap().style(), heading);
    assert_eq!(body_flow.get(1).unwrap().style(), body);
    assert_eq!(end.offset, "Body text".len());
}

#[test]
fn delete_across_paragraphs_joins_them() {
    let mut editor = editor_with("abc\ndef\nghi");
    let body = editor.document().body();
    let (a, c) = (body.get(0).unwrap().id(), body.get(2).unwrap().id());
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(a, 1), Position::new(c, 2))
        })
        .unwrap();
    let body = editor.document().body();
    assert_eq!(body.len(), 1);
    assert_eq!(body.get(0).unwrap().text(), "ai");
    assert_eq!(body.get(0).unwrap().id(), a);
    editor.undo().unwrap();
    let texts: Vec<_> = editor.document().body().iter().map(|p| p.text()).collect();
    assert_eq!(texts, ["abc", "def", "ghi"]);
}

#[test]
fn a_failed_transaction_changes_nothing() {
    let mut editor = editor_with("abc");
    let before = editor.snapshot();
    let id = first_paragraph(&editor).id();
    let result = editor.edit("Broken", |edit| {
        edit.insert_text(Position::new(id, 3), "def")?;
        edit.insert_text(Position::new(id, 99), "x")
    });
    assert!(result.is_err());
    assert_eq!(editor.document(), &before);
    assert_eq!(editor.undo_name(), Some("Typing"));
}

#[test]
fn snapshots_do_not_see_later_edits() {
    let mut editor = editor_with("before");
    let snapshot = editor.snapshot();
    let end = editor.document().end();
    editor
        .edit("Typing", |edit| edit.insert_text(end, " after"))
        .unwrap();
    assert_eq!(snapshot.body().get(0).unwrap().text(), "before");
    assert_eq!(first_paragraph(&editor).text(), "before after");
    let worker = std::thread::spawn(move || snapshot.statistics().words);
    assert_eq!(worker.join().unwrap(), 1);
}
