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
