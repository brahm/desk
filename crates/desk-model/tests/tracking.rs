// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! W148 and REV-03: while a document tracks its changes, what the user
//! types, deletes and formats is kept as changes, with the author and the
//! date, until they are accepted or rejected.

use desk_model::{
    ChangeKind, CharAttrs, Document, Editor, NodeId, Position, TextSettings, Tracking,
};

fn typed(text: &str) -> (Editor, NodeId) {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Typing", |edit| edit.insert_text(start, text))
        .unwrap();
    let paragraph = editor.document().body().get(0).unwrap().id();
    editor.set_author("Ana");
    (editor, paragraph)
}

fn track(editor: &mut Editor, tracking: Tracking) {
    let settings = TextSettings {
        tracking,
        ..editor.document().text_settings().clone()
    };
    editor
        .edit("Track Changes", |edit| edit.set_text_settings(settings))
        .unwrap();
}

/// The text of each run of a paragraph with the kind and author of its
/// change.
fn runs(editor: &Editor, paragraph: NodeId) -> Vec<(String, Option<(ChangeKind, String)>)> {
    let document = editor.document();
    document
        .paragraph(paragraph)
        .unwrap()
        .runs()
        .map(|run| {
            let change = run
                .attrs
                .change
                .flatten()
                .and_then(|id| document.changes().get(id))
                .map(|c| (c.kind, c.author.to_string()));
            (run.text().to_string(), change)
        })
        .collect()
}

fn ins(text: &str) -> (String, Option<(ChangeKind, String)>) {
    (text.into(), Some((ChangeKind::Insertion, "Ana".into())))
}

fn del(text: &str, author: &str) -> (String, Option<(ChangeKind, String)>) {
    (text.into(), Some((ChangeKind::Deletion, author.into())))
}

fn plain(text: &str) -> (String, Option<(ChangeKind, String)>) {
    (text.into(), None)
}

#[test]
fn w148_typed_text_is_one_insertion_by_the_author_with_a_date() {
    let (mut editor, p) = typed("one three");
    track(&mut editor, Tracking::On);
    editor
        .edit("Typing", |edit| edit.insert_text(Position::new(p, 4), "tw"))
        .unwrap();
    // Typed on, the text joins the insertion it follows.
    editor
        .edit("Typing", |edit| edit.insert_text(Position::new(p, 6), "o "))
        .unwrap();
    assert_eq!(
        runs(&editor, p),
        [plain("one "), ins("two "), plain("three")]
    );
    let change = editor.document().changes().iter().next().unwrap().clone();
    assert_eq!(editor.document().changes().len(), 1);
    let date = change.date.unwrap();
    assert_eq!(date.len(), "2026-11-25T09:30:00Z".len());
    assert_eq!(&date[10..11], "T");
    // Return makes the break an insertion too.
    let (next, _) = editor
        .edit("Typing", |edit| {
            let next = edit.split_paragraph(Position::new(p, 8))?;
            Ok((next, ()))
        })
        .unwrap();
    let first = editor.document().paragraph(p).unwrap();
    assert_eq!(
        first.attrs().break_change.flatten(),
        Some(change.id),
        "the new break is in the insertion"
    );
    assert_eq!(runs(&editor, next.paragraph), [plain("three")]);
}

#[test]
fn w148_deleting_marks_the_text_and_takes_the_authors_own_away() {
    let (mut editor, p) = typed("one two three");
    track(&mut editor, Tracking::On);
    editor
        .edit("Typing", |edit| {
            edit.insert_text(Position::new(p, 4), "new ")
        })
        .unwrap();
    // "one new two three": deleting "e new tw" deletes the author's own
    // text and marks the rest.
    let at = editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 2), Position::new(p, 10))
        })
        .unwrap();
    assert_eq!(at, Position::new(p, 2));
    assert_eq!(
        runs(&editor, p),
        [plain("on"), del("e tw", "Ana"), plain("o three")]
    );
    // Deleted again, the deleted text stays in its change, and the rest
    // is in a new one.
    let deletions = editor.document().changes().len();
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 0), Position::new(p, 7))
        })
        .unwrap();
    assert_eq!(
        runs(&editor, p),
        [
            del("on", "Ana"),
            del("e tw", "Ana"),
            del("o", "Ana"),
            plain(" three")
        ]
    );
    assert_eq!(editor.document().changes().len(), deletions + 1);
}

#[test]
fn w148_deleting_across_paragraphs_marks_the_break() {
    let (mut editor, p) = typed("one");
    let second = editor
        .edit("Typing", |edit| {
            let next = edit.split_paragraph(Position::new(p, 3))?;
            edit.insert_text(next, "two")?;
            Ok(next.paragraph)
        })
        .unwrap();
    track(&mut editor, Tracking::On);
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 2), Position::new(second, 1))
        })
        .unwrap();
    // Both paragraphs stay, and the break between them is deleted.
    assert_eq!(editor.document().body().len(), 2);
    let deleted = editor
        .document()
        .paragraph(p)
        .unwrap()
        .attrs()
        .break_change
        .flatten()
        .and_then(|id| editor.document().changes().get(id))
        .map(|c| c.kind);
    assert_eq!(deleted, Some(ChangeKind::Deletion));
    assert_eq!(runs(&editor, p), [plain("on"), del("e", "Ana")]);
    assert_eq!(runs(&editor, second), [del("t", "Ana"), plain("wo")]);
    // A break the author added goes when it is deleted.
    let third = editor
        .edit("Typing", |edit| {
            Ok(edit.split_paragraph(Position::new(second, 3))?.paragraph)
        })
        .unwrap();
    assert_eq!(editor.document().body().len(), 3);
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(second, 3), Position::new(third, 0))
        })
        .unwrap();
    assert_eq!(editor.document().body().len(), 2);
}

#[test]
fn w148_formatting_is_a_change_that_keeps_the_formatting_before() {
    let (mut editor, p) = typed("one two");
    track(&mut editor, Tracking::On);
    let italic = CharAttrs {
        italic: Some(true),
        ..CharAttrs::default()
    };
    editor
        .edit("Italic", |edit| {
            edit.set_character_attrs(Position::new(p, 0), Position::new(p, 3), &italic)
        })
        .unwrap();
    let document = editor.document();
    let run = document
        .paragraph(p)
        .unwrap()
        .runs()
        .next()
        .unwrap()
        .clone();
    assert_eq!(run.text(), "one");
    assert_eq!(run.attrs.italic, Some(true));
    let change = document.changes().get(run.attrs.change.flatten().unwrap());
    assert_eq!(change.map(|c| c.kind), Some(ChangeKind::Format));
    let before = run.attrs.formatted.clone().flatten().unwrap();
    assert_eq!(before.attrs, CharAttrs::default());
    // Formatting text the way it is changes nothing.
    let count = document.changes().len();
    editor
        .edit("Italic", |edit| {
            edit.set_character_attrs(Position::new(p, 0), Position::new(p, 3), &italic)
        })
        .unwrap();
    assert_eq!(editor.document().changes().len(), count);
    // Text the author added is formatted in its insertion.
    editor
        .edit("Typing", |edit| {
            edit.insert_text(Position::new(p, 7), " new")
        })
        .unwrap();
    editor
        .edit("Italic", |edit| {
            edit.set_character_attrs(Position::new(p, 7), Position::new(p, 11), &italic)
        })
        .unwrap();
    assert_eq!(runs(&editor, p).last().unwrap(), &ins(" new"));
}

#[test]
fn w148_a_paragraph_formatted_is_a_change() {
    let (mut editor, p) = typed("one");
    track(&mut editor, Tracking::On);
    let attrs = desk_model::ParaAttrs {
        alignment: Some(desk_model::Alignment::Center),
        ..desk_model::ParaAttrs::default()
    };
    editor
        .edit("Center", |edit| {
            edit.set_paragraph_attrs(Position::new(p, 0), Position::new(p, 0), &attrs)
        })
        .unwrap();
    let document = editor.document();
    let paragraph = document.paragraph(p).unwrap();
    assert_eq!(
        paragraph.attrs().alignment,
        Some(desk_model::Alignment::Center)
    );
    let change = paragraph
        .attrs()
        .format_change
        .flatten()
        .and_then(|id| document.changes().get(id));
    assert_eq!(change.map(|c| c.kind), Some(ChangeKind::Format));
    let before = paragraph.attrs().formatted.clone().flatten().unwrap();
    assert_eq!(before.attrs.alignment, None);
}

#[test]
fn w148_paused_or_off_changes_are_made_as_they_are_and_undo_takes_tracked_ones_away() {
    let (mut editor, p) = typed("one");
    track(&mut editor, Tracking::Paused);
    editor
        .edit("Typing", |edit| {
            edit.insert_text(Position::new(p, 3), " two")
        })
        .unwrap();
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 0), Position::new(p, 1))
        })
        .unwrap();
    assert_eq!(runs(&editor, p), [plain("ne two")]);
    assert!(editor.document().changes().is_empty());
    track(&mut editor, Tracking::On);
    editor
        .edit("Typing", |edit| edit.insert_text(Position::new(p, 6), "!"))
        .unwrap();
    assert_eq!(runs(&editor, p), [plain("ne two"), ins("!")]);
    editor.undo().unwrap();
    assert_eq!(runs(&editor, p), [plain("ne two")]);
    assert!(editor.document().changes().is_empty());
}

#[test]
fn w148_typing_on_after_any_character_joins_the_insertion() {
    let (mut editor, p) = typed("");
    track(&mut editor, Tracking::On);
    let mut at = Position::new(p, 0);
    for c in ["c", "a", "f", "é", "s", "日", "本"] {
        at = editor
            .edit("Typing", |edit| edit.insert_text(at, c))
            .unwrap();
    }
    assert_eq!(runs(&editor, p), [ins("cafés日本")]);
    assert_eq!(editor.document().changes().len(), 1);
}

#[test]
fn w148_formatting_text_already_in_a_change_makes_no_new_one() {
    let (mut editor, p) = typed("one");
    track(&mut editor, Tracking::On);
    editor
        .edit("Typing", |edit| {
            edit.insert_text(Position::new(p, 3), " two")
        })
        .unwrap();
    let italic = CharAttrs {
        italic: Some(true),
        ..CharAttrs::default()
    };
    editor
        .edit("Italic", |edit| {
            edit.set_character_attrs(Position::new(p, 3), Position::new(p, 7), &italic)
        })
        .unwrap();
    assert_eq!(editor.document().changes().len(), 1);
    // A paragraph formatted twice is in its first change.
    for alignment in [desk_model::Alignment::Center, desk_model::Alignment::Right] {
        let attrs = desk_model::ParaAttrs {
            alignment: Some(alignment),
            ..desk_model::ParaAttrs::default()
        };
        editor
            .edit("Align", |edit| {
                edit.set_paragraph_attrs(Position::new(p, 0), Position::new(p, 0), &attrs)
            })
            .unwrap();
    }
    assert_eq!(editor.document().changes().len(), 2);
}

#[test]
fn w148_the_delete_key_goes_past_the_text_it_deletes() {
    let (mut editor, p) = typed("abc");
    let second = editor
        .edit("Typing", |edit| {
            let next = edit.split_paragraph(Position::new(p, 3))?;
            edit.insert_text(next, "de")?;
            Ok(next.paragraph)
        })
        .unwrap();
    track(&mut editor, Tracking::On);
    let mut at = Position::new(p, 0);
    for _ in 0..3 {
        let next = Position::new(p, at.offset + 1);
        at = editor
            .edit("Delete", |edit| edit.delete_forward(at, next))
            .unwrap();
    }
    assert_eq!(at, Position::new(p, 3));
    // Over the break.
    at = editor
        .edit("Delete", |edit| {
            edit.delete_forward(at, Position::new(second, 0))
        })
        .unwrap();
    assert_eq!(at, Position::new(second, 0));
    // Text the author added goes, and the cursor stays after what stays.
    editor
        .edit("Typing", |edit| {
            edit.insert_text(Position::new(second, 1), "x")
        })
        .unwrap();
    at = editor
        .edit("Delete", |edit| {
            edit.delete_forward(Position::new(second, 0), Position::new(second, 2))
        })
        .unwrap();
    assert_eq!(at, Position::new(second, 1));
    assert_eq!(runs(&editor, second), [del("d", "Ana"), plain("e")]);
}

#[test]
fn w148_a_change_of_no_known_author_is_not_the_users_own() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let p = start.paragraph;
    editor
        .edit("Typing", |edit| {
            edit.insert_text(start, "abc")?;
            let added = edit.add_change(ChangeKind::Insertion, "", None)?;
            edit.mark_change(Position::new(p, 0), Position::new(p, 3), added)
        })
        .unwrap();
    track(&mut editor, Tracking::On);
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 0), Position::new(p, 1))
        })
        .unwrap();
    assert_eq!(editor.document().paragraph(p).unwrap().text(), "abc");
}

#[test]
fn w148_pasted_and_replaced_text_is_tracked() {
    let (mut editor, p) = typed("one two");
    let fragment = editor
        .document()
        .copy_fragment(Position::new(p, 0), Position::new(p, 3))
        .unwrap();
    track(&mut editor, Tracking::On);
    editor
        .edit("Paste", |edit| {
            edit.paste_fragment(Position::new(p, 7), &fragment)
        })
        .unwrap();
    assert_eq!(runs(&editor, p), [plain("one two"), ins("one")]);
    let query = desk_model::Query {
        text: "two".into(),
        match_case: true,
        whole_words: false,
    };
    let count = editor
        .edit("Replace All", |edit| edit.replace_all(&query, "2"))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        runs(&editor, p),
        [plain("one "), del("two", "Ana"), ins("2"), ins("one")]
    );
}

#[test]
fn w148_changes_made_in_a_story_come_back_with_it() {
    let (mut main, p) = typed("one");
    track(&mut main, Tracking::On);
    let mut story = Editor::new(main.document().clone());
    story.set_signer(main.signer().clone());
    story
        .edit("Typing", |edit| edit.insert_text(Position::new(p, 3), "!"))
        .unwrap();
    let copy = story.document().clone();
    let change = copy.changes().iter().next().unwrap().clone();
    assert_eq!(change.author.as_ref(), "Ana");
    main.edit("Story", |edit| edit.adopt_document_settings(&copy))
        .unwrap();
    assert_eq!(main.document().changes().get(change.id), Some(&change));
}
