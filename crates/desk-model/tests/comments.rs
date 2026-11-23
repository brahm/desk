// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! REV-08 and PXV-19: comments are kept with the document and anchored on
//! the text they are on, which carries them as it is edited.

use std::sync::Arc;

use desk_model::{Comment, Document, Editor, Position, Reply};

fn comment(text: &str) -> Comment {
    Comment {
        id: desk_model::NodeId::new(1).unwrap(),
        author: "Ana".into(),
        date: Some("2026-11-22T20:33:34".into()),
        text: text.into(),
        replies: Arc::new([]),
    }
}

fn typed(text: &str) -> (Editor, desk_model::NodeId) {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Typing", |edit| edit.insert_text(start, text))
        .unwrap();
    let paragraph = editor.document().body().get(0).unwrap().id();
    (editor, paragraph)
}

#[test]
fn rev_08_a_comment_is_on_its_range_and_undo_takes_it_away() {
    let (mut editor, p) = typed("Hello commented world");
    let id = editor
        .edit("Comment", |edit| {
            edit.add_comment(Position::new(p, 6), Position::new(p, 15), comment("Why?"))
        })
        .unwrap();
    let document = editor.document();
    assert_eq!(document.comments().get(id).unwrap().text.as_ref(), "Why?");
    assert_eq!(
        document.comment_ranges(),
        vec![(id, Position::new(p, 6), Position::new(p, 15))]
    );
    editor.undo().unwrap();
    assert!(editor.document().comments().is_empty());
    assert!(editor.document().comment_ranges().is_empty());
}

#[test]
fn rev_08_comments_overlap_and_text_typed_inside_joins_them() {
    let (mut editor, p) = typed("one two three");
    let (a, b) = editor
        .edit("Comment", |edit| {
            let a = edit.add_comment(Position::new(p, 0), Position::new(p, 7), comment("a"))?;
            let b = edit.add_comment(Position::new(p, 4), Position::new(p, 13), comment("b"))?;
            Ok((a, b))
        })
        .unwrap();
    // Typed inside "two", under both; typed at the end of "three", under
    // neither.
    editor
        .edit("Typing", |edit| edit.insert_text(Position::new(p, 5), "w"))
        .unwrap();
    editor
        .edit("Typing", |edit| edit.insert_text(Position::new(p, 14), "!"))
        .unwrap();
    let ranges = editor.document().comment_ranges();
    assert_eq!(ranges[0], (a, Position::new(p, 0), Position::new(p, 8)));
    assert_eq!(ranges[1], (b, Position::new(p, 4), Position::new(p, 14)));
}

#[test]
fn rev_08_deleting_all_its_text_deletes_the_comment() {
    let (mut editor, p) = typed("keep gone keep");
    let id = editor
        .edit("Comment", |edit| {
            edit.add_comment(Position::new(p, 5), Position::new(p, 9), comment("x"))
        })
        .unwrap();
    // Part of it: it stays.
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 5), Position::new(p, 6))
        })
        .unwrap();
    assert!(editor.document().comments().get(id).is_some());
    // All of it: it goes, and undo brings it back.
    editor
        .edit("Delete", |edit| {
            edit.delete(Position::new(p, 4), Position::new(p, 9))
        })
        .unwrap();
    assert!(editor.document().comments().get(id).is_none());
    editor.undo().unwrap();
    assert!(editor.document().comments().get(id).is_some());
    assert!(editor.document().has_comment_anchor(id));
}

#[test]
fn rev_08_removing_a_comment_takes_it_off_its_text() {
    let (mut editor, p) = typed("one two");
    let id = editor
        .edit("Comment", |edit| {
            let mut first = comment("a");
            first.replies = Arc::new([Reply {
                author: "Bo".into(),
                date: None,
                text: "b".into(),
            }]);
            edit.add_comment(Position::new(p, 0), Position::new(p, 7), first)
        })
        .unwrap();
    editor
        .edit("Delete Comment", |edit| edit.remove_comment(id))
        .unwrap();
    let document = editor.document();
    assert!(document.comments().is_empty());
    let paragraph = document.body().get(0).unwrap();
    assert!(paragraph.runs().all(|r| r.attrs.comments.is_none()));
    assert_eq!(paragraph.runs().count(), 1);
}

#[test]
fn pxv_19_a_comment_needs_text_to_be_on() {
    let (mut editor, p) = typed("one");
    let at = Position::new(p, 1);
    assert!(
        editor
            .edit("Highlight", |edit| edit.add_comment(at, at, comment("")))
            .is_err()
    );
    assert!(editor.document().comments().is_empty());
}

#[test]
fn rev_08_a_comment_at_the_cursor_is_on_the_word_there() {
    let (editor, p) = typed("One, two");
    let document = editor.document();
    let word = |offset| {
        document
            .word_at(Position::new(p, offset))
            .map(|(s, e)| (s.offset, e.offset))
    };
    // Inside a word, at its start and at its end.
    assert_eq!(word(1), Some((0, 3)));
    assert_eq!(word(0), Some((0, 3)));
    assert_eq!(word(8), Some((5, 8)));
    // Between words, the character after the cursor.
    assert_eq!(word(4), Some((4, 5)));
    let (empty, q) = typed("");
    assert_eq!(empty.document().word_at(Position::new(q, 0)), None);
}

#[test]
fn rev_08_a_comment_in_a_header_is_kept_with_its_text() {
    use desk_model::{MarginKind, MarginPages, MarginPlace, Paragraph, Run, Story};
    let (mut editor, _) = typed("Body");
    let place = MarginPlace::new(MarginKind::Header, MarginPages::Default);
    let story = Story::Margin {
        section: None,
        place,
    };
    let header = editor
        .edit("Header", |edit| {
            let id = edit.new_id();
            let style = edit.document().styles().default_paragraph();
            let paragraph = Paragraph::from_runs(
                id,
                style,
                desk_model::ParaAttrs::default(),
                [Run::new("Header text")],
            )?;
            edit.set_header_footer(place, vec![paragraph])?;
            Ok(id)
        })
        .unwrap();
    // A comment written in the copy whose body is the header.
    let id = editor
        .edit("Comment", |edit| {
            edit.edit_story(story, |edit| {
                edit.add_comment(
                    Position::new(header, 0),
                    Position::new(header, 6),
                    comment("In the header"),
                )
            })
        })
        .unwrap();
    let document = editor.document();
    assert_eq!(
        document.comments().get(id).unwrap().text.as_ref(),
        "In the header"
    );
    assert!(document.comment_ranges().is_empty());
    let places = document.comment_places();
    assert_eq!(places.len(), 1);
    assert_eq!(places[0].story, Some(story));
    assert_eq!(
        (places[0].start, places[0].end),
        (Position::new(header, 0), Position::new(header, 6))
    );
    // Deleted from outside the header, it goes with its marks.
    editor
        .edit("Delete Comment", |edit| edit.remove_comment(id))
        .unwrap();
    let document = editor.document();
    assert!(document.comments().is_empty());
    assert!(document.comment_places().is_empty());
    let runs = &document.headers_footers().get(place)[0];
    assert!(runs.runs().all(|r| r.attrs.comments.is_none()));
    // Its text deleted in the header, it goes too.
    editor.undo().unwrap();
    editor
        .edit("Delete", |edit| {
            edit.edit_story(story, |edit| {
                edit.delete(Position::new(header, 0), Position::new(header, 7))
                    .map(|_| ())
            })
        })
        .unwrap();
    assert!(editor.document().comments().is_empty());
}
