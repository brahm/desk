// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! EST-27: fields belong to the text. Typed text does not take them,
//! styles never hold them, and reverting or pasting a style keeps them.

use desk_model::{CharAttrs, Document, Editor, FIELD_CHARACTER, Field, FontWeight, Style};

fn field(document: &Document, offset: usize) -> Option<Option<Field>> {
    let paragraph = document.body().get(0).unwrap();
    let mut start = 0;
    for run in paragraph.runs() {
        if offset < start + run.len() {
            return run.attrs.field.clone();
        }
        start += run.len();
    }
    None
}

#[test]
fn est_27_fields_stay_with_their_character() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let end = editor
        .edit("Field", |edit| {
            let end = edit.insert_text(start, &FIELD_CHARACTER.to_string())?;
            edit.set_character_attrs(
                start,
                end,
                &CharAttrs {
                    field: Some(Some(Field::PAGE_NUMBER)),
                    weight: Some(FontWeight::BOLD),
                    ..CharAttrs::default()
                },
            )?;
            Ok(end)
        })
        .unwrap();
    // Text typed after the field is bold, but not a field.
    let after = editor
        .edit("Typing", |edit| edit.insert_text(end, "x"))
        .unwrap();
    assert_eq!(field(editor.document(), 0), Some(Some(Field::PAGE_NUMBER)));
    assert_eq!(field(editor.document(), end.offset), None);
    let paragraph = editor.document().body().get(0).unwrap();
    assert_eq!(
        paragraph.format_at(after.offset).1.weight,
        Some(FontWeight::BOLD)
    );

    // Reverting to the style drops the bold and keeps the field.
    editor
        .edit("Revert", |edit| edit.revert_to_style(start, after))
        .unwrap();
    let paragraph = editor.document().body().get(0).unwrap();
    let first = paragraph.runs().next().unwrap();
    assert_eq!(first.attrs.weight, None);
    assert_eq!(first.attrs.field, Some(Some(Field::PAGE_NUMBER)));

    // A character style cannot hold a field.
    let id = editor
        .edit("Style", |edit| {
            edit.new_character_style(
                "Numbered",
                CharAttrs {
                    field: Some(Some(Field::PAGE_COUNT)),
                    ..CharAttrs::default()
                },
                start,
                start,
                false,
            )
        })
        .unwrap();
    let Some(Style::Character(style)) = editor.document().styles().get(id) else {
        panic!("the style is there");
    };
    assert_eq!(style.character.field, None);
}

#[test]
fn est_27_auto_page_numbers_fill_the_footers_of_every_section() {
    use desk_model::{
        Alignment, AutoPageNumbers, Layout, MarginKind, MarginPages, MarginPlace, NumberAlignment,
        ParaAttrs, Paragraph, Run, Section, Start,
    };
    use std::sync::Arc;
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let header = MarginPlace::new(MarginKind::Header, MarginPages::Default);
    let footer = |pages| MarginPlace::new(MarginKind::Footer, pages);
    let second = editor
        .edit("Sections", |edit| {
            let at = edit.insert_text(start, "one")?;
            let at = edit.split_paragraph(at)?;
            let own = Section {
                own_margins: true,
                ..Section::default()
            };
            edit.set_paragraph_attrs(
                at,
                at,
                &ParaAttrs {
                    start: Some(Some(Arc::new(Start::Section(own, Layout::default())))),
                    ..ParaAttrs::default()
                },
            )?;
            let style = edit.document().styles().default_paragraph();
            let title = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs::default(),
                [Run::new("Title")],
            )?;
            edit.set_header_footer(header, vec![title])?;
            Ok(at.paragraph)
        })
        .unwrap();
    let numbers = AutoPageNumbers {
        kind: MarginKind::Footer,
        alignment: NumberAlignment::Outside,
        first_page: false,
        whole_document: true,
    };
    editor
        .edit("Auto Page Numbers", |edit| {
            let style = edit.document().styles().default_paragraph();
            let line = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs::default(),
                [Run::with_attrs(
                    &FIELD_CHARACTER.to_string(),
                    Default::default(),
                    CharAttrs {
                        field: Some(Some(Field::PAGE_NUMBER)),
                        ..CharAttrs::default()
                    },
                )],
            )?;
            edit.set_auto_page_numbers(0, numbers, &line)
        })
        .unwrap();
    let document = editor.document();
    assert!(document.page_setup().footers);
    for key in [None, Some(second)] {
        let margins = document.section_headers_footers(key);
        let alignment = |pages| {
            let paragraphs = margins.get(footer(pages));
            assert_eq!(paragraphs.len(), 1, "{key:?} {pages:?}");
            paragraphs[0].attrs().alignment
        };
        assert_eq!(alignment(MarginPages::Default), Some(Alignment::Right));
        assert_eq!(alignment(MarginPages::Left), Some(Alignment::Left));
        assert!(margins.get(footer(MarginPages::First)).is_empty());
    }
    // The first page keeps its header, which now has a copy of its own.
    let first = document.section_headers_footers(None);
    let text = first.get(MarginPlace::new(MarginKind::Header, MarginPages::First));
    assert_eq!(text.len(), 1);
    assert_eq!(text[0].text(), "Title");
    for index in [0, 1] {
        let section = document.section_at(index).1;
        assert!(section.first_page_different && section.left_right_different);
    }
}
