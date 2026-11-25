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

/// A document with changes of each kind made while tracking: "one" added,
/// "two" and the break after it deleted, and "four" italic.
fn changed() -> (Editor, NodeId) {
    let (mut editor, p) = typed("two");
    editor
        .edit("Typing", |edit| {
            let next = edit.split_paragraph(Position::new(p, 3))?;
            edit.insert_text(next, "three four")
        })
        .unwrap();
    track(&mut editor, Tracking::On);
    let second = editor.document().body().get(1).unwrap().id();
    editor
        .edit("Typing", |edit| {
            edit.insert_text(Position::new(p, 0), "one ")
        })
        .unwrap();
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 4), Position::new(second, 0))
        })
        .unwrap();
    let italic = CharAttrs {
        italic: Some(true),
        ..CharAttrs::default()
    };
    editor
        .edit("Italic", |edit| {
            edit.set_character_attrs(Position::new(second, 6), Position::new(second, 10), &italic)
        })
        .unwrap();
    assert_eq!(editor.document().changes().len(), 3);
    (editor, p)
}

fn texts(editor: &Editor) -> Vec<String> {
    editor
        .document()
        .body()
        .iter()
        .map(|p| p.text().to_string())
        .collect()
}

#[test]
fn rev_06_accepting_all_keeps_what_was_added_and_drops_what_was_deleted() {
    let (mut editor, p) = changed();
    let count = editor
        .edit("Accept All Changes", |edit| {
            edit.resolve_changes(true, |_| true)
        })
        .unwrap();
    assert_eq!(count, 3);
    assert_eq!(texts(&editor), ["one three four"]);
    assert!(editor.document().changes().is_empty());
    let document = editor.document();
    let paragraph = document.paragraph(p).unwrap();
    assert!(
        paragraph
            .runs()
            .all(|r| r.attrs.change.is_none() && r.attrs.formatted.is_none())
    );
    assert_eq!(paragraph.runs().last().unwrap().attrs.italic, Some(true));
    assert_eq!(paragraph.attrs().break_change, None);
    // One step of undo puts them back.
    editor.undo().unwrap();
    assert_eq!(editor.document().changes().len(), 3);
    assert_eq!(texts(&editor), ["one two", "three four"]);
}

#[test]
fn rev_06_rejecting_all_puts_the_text_back_as_it_was() {
    let (mut editor, p) = changed();
    editor
        .edit("Reject All Changes", |edit| {
            edit.resolve_changes(false, |_| true)
        })
        .unwrap();
    assert_eq!(texts(&editor), ["two", "three four"]);
    assert!(editor.document().changes().is_empty());
    let document = editor.document();
    assert!(
        document
            .body()
            .iter()
            .all(|p| p.attrs().break_change.is_none()
                && p.runs().all(|r| r.attrs.change.is_none()
                    && r.attrs.formatted.is_none()
                    && r.attrs.italic.is_none()))
    );
    let _ = p;
}

#[test]
fn rev_06_a_paragraph_formatted_takes_back_what_it_had() {
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
    editor
        .edit("Reject All Changes", |edit| {
            edit.resolve_changes(false, |_| true)
        })
        .unwrap();
    let paragraph = editor.document().paragraph(p).unwrap();
    assert_eq!(paragraph.attrs().alignment, None);
    assert_eq!(paragraph.attrs().format_change, None);
    assert_eq!(paragraph.attrs().formatted, None);
}

#[test]
fn rev_06_changes_of_notes_are_resolved_too() {
    let (mut editor, p) = typed("a");
    let note = editor
        .edit("Note", |edit| {
            let note = edit.new_id();
            let text = edit.new_id();
            let paragraph = desk_model::Paragraph::from_runs(
                text,
                edit.document().styles().default_paragraph(),
                desk_model::ParaAttrs::default(),
                [desk_model::Run::new("kept added")],
            )?;
            let added = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
            edit.set_note(
                note,
                Some(desk_model::Note {
                    id: note,
                    mark: None,
                    paragraphs: vec![paragraph].into(),
                }),
            )?;
            let mark = CharAttrs {
                field: Some(Some(desk_model::Field::NoteMark { note })),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(Position::new(p, 0), Position::new(p, 1), &mark)?;
            edit.edit_story(desk_model::Story::Note(note), |edit| {
                let first = edit.document().body().get(0).unwrap().id();
                edit.mark_change(Position::new(first, 4), Position::new(first, 10), added)
            })?;
            Ok(note)
        })
        .unwrap();
    assert_eq!(editor.document().change_places().len(), 1);
    editor
        .edit("Reject All Changes", |edit| {
            edit.resolve_changes(false, |_| true)
        })
        .unwrap();
    let text = editor.document().notes().get(note).unwrap().paragraphs[0]
        .text()
        .to_string();
    assert_eq!(text, "kept");
    assert!(editor.document().changes().is_empty());
}

#[test]
fn rev_04_text_typed_by_a_deletion_is_not_deleted_and_copies_leave_hidden_text_out() {
    let (mut editor, p) = typed("one two three");
    track(&mut editor, Tracking::On);
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 4), Position::new(p, 8))
                .map(|_| ())
        })
        .unwrap();
    assert_eq!(
        runs(&editor, p),
        [plain("one "), del("two ", "Ana"), plain("three")]
    );
    // Off, text typed after the deletion is not in it.
    track(&mut editor, Tracking::Off);
    editor
        .edit("Typing", |edit| {
            edit.insert_text(Position::new(p, 8), "and ")
        })
        .unwrap();
    assert_eq!(
        runs(&editor, p),
        [plain("one "), del("two ", "Ana"), plain("and three")]
    );
    // A copy has the deleted text only while the view shows it.
    let copied = |editor: &Editor| -> String {
        let end = Position::new(p, 17);
        let fragment = editor
            .document()
            .copy_fragment(Position::new(p, 0), end)
            .unwrap();
        fragment.paragraphs[0]
            .runs
            .iter()
            .map(|(text, _, _)| text.as_str())
            .collect()
    };
    assert_eq!(copied(&editor), "one two and three");
    editor.set_markup(desk_model::Markup::WithoutDeletions);
    assert_eq!(copied(&editor), "one and three");
    assert_eq!(
        editor.document().markup(),
        desk_model::Markup::WithoutDeletions
    );
}

#[test]
fn b_070_deleted_text_keeps_the_change_it_was_in() {
    let (mut editor, p) = typed("one two three");
    // Bo added "two ".
    let ((bo, _), _) = editor
        .edit("Changes", |edit| {
            let bo = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
            edit.mark_change(Position::new(p, 4), Position::new(p, 8), bo)?;
            Ok(((bo, ()), ()))
        })
        .unwrap();
    track(&mut editor, Tracking::On);
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 0), Position::new(p, 13))
                .map(|_| ())
        })
        .unwrap();
    let deletion = editor
        .document()
        .changes()
        .iter()
        .find(|c| c.kind == ChangeKind::Deletion)
        .unwrap()
        .id;
    let resolve = |editor: &mut Editor, accept: bool, id: NodeId| {
        editor
            .edit("Resolve", |edit| {
                edit.resolve_changes(accept, |c| c == id).map(|_| ())
            })
            .unwrap();
    };
    // Rejecting the deletion gives "two " back to Bo's insertion.
    resolve(&mut editor, false, deletion);
    assert_eq!(
        runs(&editor, p),
        [
            plain("one "),
            ("two ".into(), Some((ChangeKind::Insertion, "Bo".into()))),
            plain("three")
        ]
    );
    editor.undo().unwrap();
    // Bo's insertion accepted while the text is deleted: rejecting the
    // deletion then leaves plain text.
    resolve(&mut editor, true, bo);
    resolve(&mut editor, false, deletion);
    assert_eq!(runs(&editor, p), [plain("one two three")]);
    editor.undo().unwrap();
    editor.undo().unwrap();
    // Bo's insertion rejected while the text is deleted: the text goes.
    resolve(&mut editor, false, bo);
    assert_eq!(editor.document().paragraph(p).unwrap().text(), "one three");
    editor.undo().unwrap();
    // Everything rejected: the text that Bo added goes, the rest stays.
    editor
        .edit("Reject All", |edit| {
            edit.resolve_changes(false, |_| true).map(|_| ())
        })
        .unwrap();
    assert_eq!(runs(&editor, p), [plain("one three")]);
}

/// B-070 review: text typed next to deleted text that was in another
/// author's change is in neither, so rejecting that change keeps it.
#[test]
fn b_070_text_typed_next_to_deleted_text_is_in_no_change_it_was_in() {
    let (mut editor, p) = typed("one two three");
    let ((bo, _), _) = editor
        .edit("Changes", |edit| {
            let bo = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
            edit.mark_change(Position::new(p, 4), Position::new(p, 8), bo)?;
            Ok(((bo, ()), ()))
        })
        .unwrap();
    track(&mut editor, Tracking::On);
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 4), Position::new(p, 8))
                .map(|_| ())
        })
        .unwrap();
    track(&mut editor, Tracking::Off);
    editor
        .edit("Typing", |edit| {
            edit.insert_text(Position::new(p, 8), "X").map(|_| ())
        })
        .unwrap();
    let paragraph = editor.document().paragraph(p).unwrap();
    assert!(
        paragraph
            .runs()
            .filter(|r| r.text().contains('X'))
            .all(|r| r.attrs.former.is_none() && r.attrs.change.is_none()),
        "{:?}",
        paragraph.runs().collect::<Vec<_>>()
    );
    editor
        .edit("Resolve", |edit| {
            edit.resolve_changes(false, |c| c == bo).map(|_| ())
        })
        .unwrap();
    assert_eq!(editor.document().paragraph(p).unwrap().text(), "one Xthree");
}

/// B-072: a paragraph break that another author added keeps that
/// insertion when a tracked deletion takes it, as text does (B-070).
#[test]
fn b_072_a_deleted_break_keeps_the_insertion_it_was_in() {
    let (mut editor, p) = typed("one two");
    // Bo added the break after "one".
    let ((bo, q), _) = editor
        .edit("Changes", |edit| {
            let q = edit.split_paragraph(Position::new(p, 3))?.paragraph;
            let bo = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
            edit.mark_break_change(p, Some(bo))?;
            Ok(((bo, q), ()))
        })
        .unwrap();
    track(&mut editor, Tracking::On);
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 3), Position::new(q, 0))
                .map(|_| ())
        })
        .unwrap();
    let document = editor.document();
    let deletion = document
        .changes()
        .iter()
        .find(|c| c.kind == ChangeKind::Deletion)
        .unwrap()
        .id;
    let attrs = document.paragraph(p).unwrap().attrs();
    assert_eq!(attrs.break_change.flatten(), Some(deletion));
    assert_eq!(attrs.break_former.flatten(), Some(bo));
    let resolve = |editor: &mut Editor, accept: bool, id: NodeId| {
        editor
            .edit("Resolve", |edit| {
                edit.resolve_changes(accept, |c| c == id).map(|_| ())
            })
            .unwrap();
    };
    let texts = |editor: &Editor| -> Vec<String> {
        editor
            .document()
            .body()
            .iter()
            .map(|p| p.text().to_owned())
            .collect()
    };
    let ends = |editor: &Editor| {
        let attrs = editor.document().paragraph(p).unwrap().attrs().clone();
        (attrs.break_change.flatten(), attrs.break_former.flatten())
    };
    // Rejecting the deletion gives the break back to Bo's insertion.
    resolve(&mut editor, false, deletion);
    assert_eq!(texts(&editor), ["one", " two"]);
    assert_eq!(ends(&editor), (Some(bo), None));
    editor.undo().unwrap();
    // Bo's insertion accepted while the break is deleted: rejecting the
    // deletion then leaves a plain break.
    resolve(&mut editor, true, bo);
    assert_eq!(ends(&editor), (Some(deletion), None));
    resolve(&mut editor, false, deletion);
    assert_eq!(texts(&editor), ["one", " two"]);
    assert_eq!(ends(&editor), (None, None));
    editor.undo().unwrap();
    editor.undo().unwrap();
    // Bo's insertion rejected while the break is deleted: the break goes.
    resolve(&mut editor, false, bo);
    assert_eq!(texts(&editor), ["one two"]);
    editor.undo().unwrap();
    // The deletion accepted: the break goes too.
    resolve(&mut editor, true, deletion);
    assert_eq!(texts(&editor), ["one two"]);
    editor.undo().unwrap();
    // Everything rejected: the break that Bo added goes.
    editor
        .edit("Reject All", |edit| {
            edit.resolve_changes(false, |_| true).map(|_| ())
        })
        .unwrap();
    assert_eq!(texts(&editor), ["one two"]);
}

/// B-072 review: accepting the deletion of the break of the last
/// paragraph, which cannot go, leaves it in no change.
#[test]
fn b_072_the_last_break_accepted_is_in_no_change() {
    let (mut editor, p) = typed("one");
    let ((bo, ana), _) = editor
        .edit("Changes", |edit| {
            let bo = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
            let ana = edit.add_change(ChangeKind::Deletion, "Ana", None)?;
            edit.mark_break_change(p, Some(ana))?;
            edit.mark_break_former(p, bo)?;
            Ok(((bo, ana), ()))
        })
        .unwrap();
    editor
        .edit("Resolve", |edit| {
            edit.resolve_changes(true, |c| c == ana).map(|_| ())
        })
        .unwrap();
    let attrs = editor.document().paragraph(p).unwrap().attrs().clone();
    assert_eq!(attrs.break_change.flatten(), None);
    assert_eq!(attrs.break_former.flatten(), None);
    let _ = bo;
}

/// A shape in the line of the text of a paragraph, inserted at `offset`.
fn inline_shape(editor: &mut Editor, p: NodeId, offset: usize) -> NodeId {
    use desk_model::{Frame, Object, Shape};
    editor
        .edit("Insert Shape", |edit| {
            let shape = Object::new(
                edit.new_id(),
                Shape::Ellipse.into(),
                Frame::from_pt(0.0, 0.0, 20.0, 10.0),
            );
            let id = shape.id;
            edit.insert_object_in_text(Position::new(p, offset), shape)?;
            Ok(id)
        })
        .unwrap()
}

#[test]
fn rev_01_an_object_put_in_the_text_is_an_insertion() {
    let (mut editor, p) = typed("ab");
    track(&mut editor, Tracking::On);
    let shape = inline_shape(&mut editor, p, 1);
    // The mark of the object is in one insertion, with no change of its
    // formatting.
    assert_eq!(editor.document().changes().len(), 1);
    let mark = desk_model::FIELD_CHARACTER.to_string();
    assert_eq!(runs(&editor, p), [plain("a"), ins(&mark), plain("b")]);
    // Rejected, the object goes with its mark; accepted, it stays.
    editor
        .edit("Reject", |edit| edit.resolve_changes(false, |_| true))
        .unwrap();
    assert_eq!(texts(&editor), ["ab"]);
    assert!(editor.document().objects().get(shape).is_none());
    editor.undo().unwrap();
    editor
        .edit("Accept", |edit| edit.resolve_changes(true, |_| true))
        .unwrap();
    assert_eq!(runs(&editor, p), [plain("a"), plain(&mark), plain("b")]);
    assert!(editor.document().objects().get(shape).is_some());
}

#[test]
fn rev_01_an_object_deleted_from_the_text_stays_until_the_deletion_is_accepted() {
    let (mut editor, p) = typed("ab");
    let shape = inline_shape(&mut editor, p, 1);
    track(&mut editor, Tracking::On);
    editor
        .edit("Delete", |edit| {
            edit.delete(
                Position::new(p, 1),
                Position::new(p, 1 + desk_model::FIELD_CHARACTER.len_utf8()),
            )
        })
        .unwrap();
    assert!(editor.document().objects().get(shape).is_some());
    editor
        .edit("Reject", |edit| edit.resolve_changes(false, |_| true))
        .unwrap();
    assert!(editor.document().objects().get(shape).is_some());
    assert!(editor.document().changes().is_empty());
    editor.undo().unwrap();
    editor
        .edit("Accept", |edit| edit.resolve_changes(true, |_| true))
        .unwrap();
    assert_eq!(texts(&editor), ["ab"]);
    assert!(editor.document().objects().get(shape).is_none());
}

/// A shape fixed on the first page.
fn floating_shape(editor: &mut Editor) -> NodeId {
    use desk_model::{Anchor, Frame, Object, Shape};
    editor
        .edit("Insert Shape", |edit| {
            let mut shape = Object::new(
                edit.new_id(),
                Shape::Ellipse.into(),
                Frame::from_pt(100.0, 100.0, 50.0, 40.0),
            );
            shape.anchor = Anchor::Page(0);
            let id = shape.id;
            edit.add_object(shape)?;
            Ok(id)
        })
        .unwrap()
}

#[test]
fn rev_01_a_floating_object_moved_or_resized_keeps_where_it_was() {
    use desk_model::{Anchor, Frame, Length};
    let (mut editor, p) = typed("ab");
    let shape = floating_shape(&mut editor);
    let before = Frame::from_pt(100.0, 100.0, 50.0, 40.0);
    track(&mut editor, Tracking::On);
    let moved = |editor: &Editor| editor.document().objects().get(shape).unwrap().moved;
    editor
        .edit("Move", |edit| {
            edit.move_objects(&[shape], Length::from_pt(10.0), Length::from_pt(5.0))
        })
        .unwrap();
    // One change, by the author, which keeps where it was.
    let change = editor.document().changes().iter().next().unwrap().clone();
    assert_eq!(change.kind, ChangeKind::Placement);
    assert_eq!(change.author.as_ref(), "Ana");
    assert_eq!(
        moved(&editor),
        Some(desk_model::Moved {
            change: change.id,
            anchor: Anchor::Page(0),
            frame: before,
        })
    );
    // Resized and moved to the paragraph after, it stays in that change,
    // with where it was first.
    editor
        .edit("Resize", |edit| {
            edit.place_object(
                shape,
                Anchor::Paragraph(p),
                Frame::from_pt(20.0, 0.0, 80.0, 60.0),
            )
        })
        .unwrap();
    assert_eq!(editor.document().changes().len(), 1);
    assert_eq!(moved(&editor).unwrap().frame, before);
    // Rejected, it is back where it was, in no change.
    editor
        .edit("Reject", |edit| edit.resolve_changes(false, |_| true))
        .unwrap();
    let object = editor.document().objects().get(shape).unwrap();
    assert_eq!(
        (object.anchor, object.frame, object.moved),
        (Anchor::Page(0), before, None)
    );
    assert!(editor.document().changes().is_empty());
    // Accepted, it stays where it went.
    editor.undo().unwrap();
    editor
        .edit("Accept", |edit| edit.resolve_changes(true, |_| true))
        .unwrap();
    let object = editor.document().objects().get(shape).unwrap();
    assert_eq!(
        (object.anchor, object.frame, object.moved),
        (
            Anchor::Paragraph(p),
            Frame::from_pt(20.0, 0.0, 80.0, 60.0),
            None
        )
    );
}

#[test]
fn rev_01_an_object_put_back_where_it_was_or_deleted_is_in_no_change() {
    use desk_model::Length;
    let (mut editor, _) = typed("ab");
    let shape = floating_shape(&mut editor);
    let copy = floating_shape(&mut editor);
    track(&mut editor, Tracking::On);
    editor
        .edit("Move", |edit| {
            edit.move_objects(&[shape, copy], Length::from_pt(10.0), Length::ZERO)
        })
        .unwrap();
    // Each object moved is a change of its own.
    assert_eq!(editor.document().changes().len(), 2);
    editor
        .edit("Move", |edit| {
            edit.move_objects(&[shape], Length::from_pt(-10.0), Length::ZERO)
        })
        .unwrap();
    assert_eq!(editor.document().objects().get(shape).unwrap().moved, None);
    assert_eq!(editor.document().changes().len(), 1);
    // The object of a change deleted, the change goes too.
    editor
        .edit("Delete", |edit| edit.delete_objects(&[copy]))
        .unwrap();
    assert!(editor.document().changes().is_empty());
    // Paused, moving is no change.
    track(&mut editor, Tracking::Paused);
    editor
        .edit("Move", |edit| {
            edit.move_objects(&[shape], Length::from_pt(10.0), Length::ZERO)
        })
        .unwrap();
    assert!(editor.document().changes().is_empty());
}

#[test]
fn rev_01_an_object_moved_into_the_text_is_no_insertion() {
    let (mut editor, p) = typed("ab");
    let shape = floating_shape(&mut editor);
    track(&mut editor, Tracking::On);
    editor
        .edit("Inline", |edit| {
            edit.put_object_in_text(shape, Position::new(p, 1))
        })
        .unwrap();
    assert!(editor.document().changes().is_empty());
    // Rejecting all leaves it in the text.
    editor
        .edit("Reject", |edit| edit.resolve_changes(false, |_| true))
        .unwrap();
    assert!(editor.document().objects().get(shape).is_some());
}

#[test]
fn rev_01_an_object_moved_in_the_text_keeps_the_change_of_its_mark() {
    let (mut editor, p) = typed("abc");
    track(&mut editor, Tracking::On);
    let shape = inline_shape(&mut editor, p, 1);
    editor
        .edit("Move", |edit| {
            edit.put_object_in_text(shape, Position::new(p, 5))
        })
        .unwrap();
    let mark = desk_model::FIELD_CHARACTER.to_string();
    assert_eq!(runs(&editor, p), [plain("ab"), ins(&mark), plain("c")]);
    // Rejected, the object goes, as it was added.
    editor
        .edit("Reject", |edit| edit.resolve_changes(false, |_| true))
        .unwrap();
    assert!(editor.document().objects().get(shape).is_none());
}

#[test]
fn rev_01_an_object_moved_then_put_in_the_text_or_grouped_leaves_its_change() {
    use desk_model::Length;
    let (mut editor, p) = typed("ab");
    let shape = floating_shape(&mut editor);
    let other = floating_shape(&mut editor);
    track(&mut editor, Tracking::On);
    let step = |editor: &mut Editor, ids: &[NodeId]| {
        editor
            .edit("Move", |edit| {
                edit.move_objects(ids, Length::from_pt(10.0), Length::ZERO)
            })
            .unwrap();
        assert_eq!(editor.document().changes().len(), ids.len());
    };
    step(&mut editor, &[shape]);
    editor
        .edit("Inline", |edit| {
            edit.put_object_in_text(shape, Position::new(p, 1))
        })
        .unwrap();
    assert!(editor.document().changes().is_empty());
    let third = floating_shape(&mut editor);
    step(&mut editor, &[other, third]);
    let group = editor
        .edit("Group", |edit| edit.group_objects(&[other, third]))
        .unwrap()
        .unwrap();
    assert!(editor.document().changes().is_empty());
    editor
        .edit("Ungroup", |edit| edit.ungroup_objects(&[group]))
        .unwrap();
    assert!(
        editor
            .document()
            .objects()
            .iter()
            .all(|o| o.moved.is_none())
    );
    // Moved again, they are in a change again.
    step(&mut editor, &[other]);
}

/// A table of two by two, fixed on the first page, "a" in its first cell.
fn table_object(editor: &mut Editor) -> NodeId {
    use desk_model::table::Content;
    use desk_model::{Anchor, Frame, Object, ObjectKind, Table};
    editor
        .edit("Insert Table", |edit| {
            let mut table = Table::new(2, 2);
            table.cell_mut(0, 0).content = Content::Text("a".into());
            let mut object = Object::new(
                edit.new_id(),
                ObjectKind::Table(std::sync::Arc::new(table)),
                Frame::from_pt(100.0, 100.0, 144.0, 40.0),
            );
            object.anchor = Anchor::Page(0);
            let id = object.id;
            edit.add_object(object)?;
            Ok(id)
        })
        .unwrap()
}

/// Changes the cells of a table, as the window does.
fn change_table(editor: &mut Editor, id: NodeId, change: impl FnOnce(&mut desk_model::Table)) {
    editor
        .edit("Cells", |edit| {
            let mut object = edit.document().objects().get(id).unwrap().clone();
            let desk_model::ObjectKind::Table(table) = &mut object.kind else {
                unreachable!()
            };
            change(std::sync::Arc::make_mut(table));
            edit.replace_object(object)
        })
        .unwrap();
}

fn cell(editor: &Editor, id: NodeId, r: u32, c: u32) -> desk_model::table::Cell<String> {
    let desk_model::ObjectKind::Table(table) = &editor.document().objects().get(id).unwrap().kind
    else {
        unreachable!()
    };
    table.cell(r, c).clone()
}

#[test]
fn rev_05_a_cell_changed_keeps_what_it_was() {
    use desk_model::table::{CellFill, Color, Content};
    let (mut editor, _) = typed("ab");
    let table = table_object(&mut editor);
    track(&mut editor, Tracking::On);
    change_table(&mut editor, table, |t| {
        t.cell_mut(0, 0).content = Content::Text("b".into());
        t.cell_mut(1, 1).style.fill = Some(CellFill::Color(Color::rgb(255, 0, 0)));
    });
    // Each cell changed is a change of its own, by the author.
    assert_eq!(editor.document().changes().len(), 2);
    assert!(
        editor
            .document()
            .changes()
            .iter()
            .all(|c| c.kind == ChangeKind::Cell && c.author.as_ref() == "Ana")
    );
    let first = cell(&editor, table, 0, 0).tracked.unwrap();
    assert_eq!(first.content, Content::Text("a".into()));
    assert!(cell(&editor, table, 0, 1).tracked.is_none());
    // Changed again, it stays in its change, with what it was first.
    change_table(&mut editor, table, |t| {
        t.cell_mut(0, 0).content = Content::Text("c".into());
    });
    assert_eq!(
        cell(&editor, table, 0, 0).tracked.unwrap().change,
        first.change
    );
    // Rejected, the cells are as they were.
    editor
        .edit("Reject", |edit| edit.resolve_changes(false, |_| true))
        .unwrap();
    assert_eq!(
        cell(&editor, table, 0, 0).content,
        Content::Text("a".into())
    );
    assert_eq!(cell(&editor, table, 1, 1).style.fill, None);
    assert!(cell(&editor, table, 0, 0).tracked.is_none());
    assert!(editor.document().changes().is_empty());
    // Accepted, they keep what they are.
    editor.undo().unwrap();
    editor
        .edit("Accept", |edit| edit.resolve_changes(true, |_| true))
        .unwrap();
    assert_eq!(
        cell(&editor, table, 0, 0).content,
        Content::Text("c".into())
    );
    assert!(cell(&editor, table, 1, 1).tracked.is_none());
}

#[test]
fn rev_05_a_cell_put_back_or_deleted_is_in_no_change() {
    use desk_model::table::Content;
    let (mut editor, _) = typed("ab");
    let table = table_object(&mut editor);
    track(&mut editor, Tracking::On);
    change_table(&mut editor, table, |t| {
        t.cell_mut(0, 0).content = Content::Text("b".into());
        t.cell_mut(1, 0).content = Content::Text("x".into());
    });
    change_table(&mut editor, table, |t| {
        t.cell_mut(0, 0).content = Content::Text("a".into());
    });
    assert!(cell(&editor, table, 0, 0).tracked.is_none());
    assert_eq!(editor.document().changes().len(), 1);
    // A row added is no change; the row of a changed cell deleted takes
    // its change away.
    change_table(&mut editor, table, |t| t.insert_rows(2, 1).unwrap());
    assert_eq!(editor.document().changes().len(), 1);
    change_table(&mut editor, table, |t| t.delete_rows(1, 1).unwrap());
    assert!(editor.document().changes().is_empty());
}

#[test]
fn rev_05_a_cell_changed_by_several_authors_says_so_and_a_table_grown_is_not_moved() {
    use desk_model::table::Content;
    let (mut editor, _) = typed("ab");
    let table = table_object(&mut editor);
    track(&mut editor, Tracking::On);
    change_table(&mut editor, table, |t| {
        t.cell_mut(0, 0).content = Content::Text("b".into());
    });
    assert!(!cell(&editor, table, 0, 0).tracked.unwrap().others);
    // The same author again: still theirs alone.
    change_table(&mut editor, table, |t| {
        t.cell_mut(0, 0).content = Content::Text("c".into());
    });
    assert!(!cell(&editor, table, 0, 0).tracked.unwrap().others);
    // Another author: the same change, of several authors.
    editor.set_author("Bo");
    change_table(&mut editor, table, |t| {
        t.cell_mut(0, 0).content = Content::Text("d".into());
    });
    let tracked = cell(&editor, table, 0, 0).tracked.unwrap();
    assert!(tracked.others);
    assert_eq!(tracked.content, Content::Text("a".into()));
    assert_eq!(editor.document().changes().len(), 1);
    // A table that grows with its cells is not moved; moved, it is.
    editor
        .edit("Cells", |edit| {
            let mut object = edit.document().objects().get(table).unwrap().clone();
            object.frame.height = desk_model::Length::from_pt(80.0);
            edit.replace_object(object)
        })
        .unwrap();
    assert_eq!(editor.document().changes().len(), 1);
    assert!(
        editor
            .document()
            .objects()
            .get(table)
            .unwrap()
            .moved
            .is_none()
    );
    editor
        .edit("Move", |edit| {
            let mut object = edit.document().objects().get(table).unwrap().clone();
            object.frame.x = desk_model::Length::from_pt(150.0);
            object.frame.height = desk_model::Length::from_pt(60.0);
            edit.replace_object(object)
        })
        .unwrap();
    assert_eq!(editor.document().changes().len(), 2);
    // Rejected, it goes back where it was with the size of its cells.
    editor
        .edit("Reject", |edit| {
            edit.resolve_changes(false, |_| true)?;
            Ok(())
        })
        .unwrap();
    let object = editor.document().objects().get(table).unwrap().clone();
    assert_eq!(
        (object.frame.x, object.frame.height),
        (
            desk_model::Length::from_pt(100.0),
            desk_model::Length::from_pt(60.0)
        )
    );
    assert!(editor.document().changes().is_empty());
}
