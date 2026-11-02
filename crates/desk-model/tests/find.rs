// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Model tests for finding and replacing (TXT-12, TXT-13). Each test name
//! starts with the row ID.

use desk_model::FontWeight;
use desk_model::{CharAttrs, Document, Editor, Position, Query};

fn editor(text: &str) -> Editor {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Setup", |edit| edit.insert_text(start, text))
        .unwrap();
    editor
}

fn texts(editor: &Editor) -> Vec<String> {
    editor.document().body().iter().map(|p| p.text()).collect()
}

#[test]
fn txt_13_find_lists_every_match_in_order() {
    let editor = editor("Gato e gato\nsem\nGATOS");
    let found = editor.document().find(&Query::new("gato"));
    let places: Vec<_> = found.iter().map(|f| (f.index, f.range.clone())).collect();
    assert_eq!(places, [(0, 0..4), (0, 7..11), (2, 0..4)]);
    let whole = Query {
        whole_words: true,
        match_case: true,
        ..Query::new("gato")
    };
    let found = editor.document().find(&whole);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].range, 7..11);
}

#[test]
fn txt_12_replace_all_keeps_the_formatting_and_undoes() {
    let mut editor = editor("one cat, two cats\ncat");
    let first = editor.document().body().get(0).unwrap().id();
    // "cat" in "cats" is bold.
    editor
        .edit("Bold", |edit| {
            edit.set_character_attrs(
                Position::new(first, 13),
                Position::new(first, 16),
                &CharAttrs {
                    weight: Some(FontWeight::BOLD),
                    ..CharAttrs::default()
                },
            )
        })
        .unwrap();
    let count = editor
        .edit("Replace All", |edit| {
            edit.replace_all(&Query::new("cat"), "dog")
        })
        .unwrap();
    assert_eq!(count, 3);
    assert_eq!(texts(&editor), ["one dog, two dogs", "dog"]);
    let paragraph = editor.document().body().get(0).unwrap();
    let (_, attrs) = paragraph.format_at(14);
    assert_eq!(
        attrs.weight,
        Some(FontWeight::BOLD),
        "the replacement keeps the bold"
    );
    editor.undo().unwrap();
    assert_eq!(texts(&editor), ["one cat, two cats", "cat"]);
}

#[test]
fn txt_12_replace_one_and_with_nothing() {
    let mut editor = editor("a-b-c");
    let found = editor.document().find(&Query::new("-"));
    let after = editor
        .edit("Replace", |edit| edit.replace_found(&found[1], ""))
        .unwrap();
    assert_eq!(texts(&editor), ["a-bc"]);
    assert_eq!(after.offset, 3);
}
