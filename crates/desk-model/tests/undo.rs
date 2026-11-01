// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests: any sequence of edits can be undone back to the start
//! and redone to the end, passing through the same documents.

use desk_model::{
    Alignment, CharAttrs, CharacterStyle, Document, Edit, Editor, LINE_BREAK, Length, ModelError,
    NO_BREAK_SPACE, ParaAttrs, ParagraphStyle, Position,
};
use proptest::prelude::*;

/// An edit described by numbers, which pick paragraphs and offsets in the
/// document as it is when the edit runs.
#[derive(Clone, Debug)]
enum Action {
    Insert {
        at: (usize, usize),
        text: String,
    },
    Delete {
        from: (usize, usize),
        to: (usize, usize),
    },
    Split {
        at: (usize, usize),
    },
    Join {
        paragraph: usize,
    },
    Bold {
        from: (usize, usize),
        to: (usize, usize),
        on: bool,
    },
    Size {
        from: (usize, usize),
        to: (usize, usize),
        points: i32,
    },
    CharacterStyle {
        from: (usize, usize),
        to: (usize, usize),
        set: bool,
    },
    Clear {
        from: (usize, usize),
        to: (usize, usize),
    },
    Align {
        paragraph: usize,
        center: bool,
    },
    ParagraphStyle {
        paragraph: usize,
        heading: bool,
    },
}

fn text() -> impl Strategy<Value = String> {
    let pieces = prop_oneof![
        Just("a".to_string()),
        Just("word ".to_string()),
        Just("é".to_string()),
        Just("中文".to_string()),
        Just(NO_BREAK_SPACE.to_string()),
        Just(LINE_BREAK.to_string()),
        Just("\n".to_string()),
        Just("🙂".to_string()),
    ];
    prop::collection::vec(pieces, 1..4).prop_map(|parts| parts.concat())
}

fn point() -> impl Strategy<Value = (usize, usize)> {
    (0..8usize, 0..40usize)
}

fn action() -> impl Strategy<Value = Action> {
    prop_oneof![
        3 => (point(), text()).prop_map(|(at, text)| Action::Insert { at, text }),
        2 => (point(), point()).prop_map(|(from, to)| Action::Delete { from, to }),
        1 => point().prop_map(|at| Action::Split { at }),
        1 => (0..8usize).prop_map(|paragraph| Action::Join { paragraph }),
        1 => (point(), point(), any::<bool>())
            .prop_map(|(from, to, on)| Action::Bold { from, to, on }),
        1 => (point(), point(), 8..30i32)
            .prop_map(|(from, to, points)| Action::Size { from, to, points }),
        1 => (point(), point(), any::<bool>())
            .prop_map(|(from, to, set)| Action::CharacterStyle { from, to, set }),
        1 => (point(), point()).prop_map(|(from, to)| Action::Clear { from, to }),
        1 => (0..8usize, any::<bool>())
            .prop_map(|(paragraph, center)| Action::Align { paragraph, center }),
        1 => (0..8usize, any::<bool>())
            .prop_map(|(paragraph, heading)| Action::ParagraphStyle { paragraph, heading }),
    ]
}

/// Turns numbers into a valid position: a paragraph by index and an
/// offset that falls between characters.
fn position(doc: &Document, (paragraph, offset): (usize, usize)) -> Position {
    let body = doc.body();
    let paragraph = body.get(paragraph % body.len()).unwrap();
    let text = paragraph.text();
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    Position::new(paragraph.id(), boundaries[offset % boundaries.len()])
}

fn ordered(doc: &Document, a: (usize, usize), b: (usize, usize)) -> (Position, Position) {
    let (a, b) = (position(doc, a), position(doc, b));
    let index = |p: &Position| (doc.body().index_of(p.paragraph).unwrap(), p.offset);
    if index(&a) <= index(&b) {
        (a, b)
    } else {
        (b, a)
    }
}

struct Fixture {
    emphasis: desk_model::StyleId,
    heading: desk_model::StyleId,
}

fn setup() -> (Document, Fixture) {
    let mut doc = Document::new_word_processing();
    let emphasis = doc.new_id();
    let mut style = CharacterStyle::new(emphasis, "Emphasis");
    style.character.italic = Some(true);
    doc.put_style(style).unwrap();
    let heading = doc.new_id();
    let mut style = ParagraphStyle::new(heading, "Heading");
    style.parent = Some(doc.styles().default_paragraph());
    style.next = Some(doc.styles().default_paragraph());
    doc.put_style(style).unwrap();
    (doc, Fixture { emphasis, heading })
}

fn run(edit: &mut Edit, fixture: &Fixture, action: &Action) -> Result<(), ModelError> {
    let doc = edit.document().clone();
    let paragraph_id = |index: usize| doc.body().get(index % doc.body().len()).unwrap().id();
    match action {
        Action::Insert { at, text } => edit.insert_text(position(&doc, *at), text).map(drop),
        Action::Delete { from, to } => {
            let (a, b) = ordered(&doc, *from, *to);
            edit.delete(a, b).map(drop)
        }
        Action::Split { at } => edit.split_paragraph(position(&doc, *at)).map(drop),
        Action::Join { paragraph } => edit.join_with_next(paragraph_id(*paragraph)),
        Action::Bold { from, to, on } => {
            let (a, b) = ordered(&doc, *from, *to);
            let attrs = CharAttrs {
                bold: Some(*on),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(a, b, &attrs)
        }
        Action::Size { from, to, points } => {
            let (a, b) = ordered(&doc, *from, *to);
            let attrs = CharAttrs {
                font_size: Some(Length::from_points(*points)),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(a, b, &attrs)
        }
        Action::CharacterStyle { from, to, set } => {
            let (a, b) = ordered(&doc, *from, *to);
            edit.set_character_style(a, b, set.then_some(fixture.emphasis))
        }
        Action::Clear { from, to } => {
            let (a, b) = ordered(&doc, *from, *to);
            edit.clear_character_attrs(a, b)
        }
        Action::Align { paragraph, center } => {
            let at = Position::new(paragraph_id(*paragraph), 0);
            let attrs = ParaAttrs {
                alignment: Some(if *center {
                    Alignment::Center
                } else {
                    Alignment::Left
                }),
                ..ParaAttrs::default()
            };
            edit.set_paragraph_attrs(at, at, &attrs)
        }
        Action::ParagraphStyle { paragraph, heading } => {
            let at = Position::new(paragraph_id(*paragraph), 0);
            let style = if *heading {
                fixture.heading
            } else {
                doc.styles().default_paragraph()
            };
            edit.set_paragraph_style(at, at, style)
        }
    }
}

fn check_invariants(doc: &Document) {
    assert!(!doc.body().is_empty());
    for paragraph in doc.body().iter() {
        assert!(
            paragraph.is_normalized(),
            "runs not normalized: {paragraph:?}"
        );
        assert!(!paragraph.text().contains('\n'));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn undo_and_redo_walk_through_the_same_documents(
        actions in prop::collection::vec(action(), 1..25)
    ) {
        let (doc, fixture) = setup();
        let mut editor = Editor::new(doc);
        let mut states = vec![editor.snapshot()];
        for action in &actions {
            editor.edit("Action", |edit| run(edit, &fixture, action)).unwrap();
            check_invariants(editor.document());
            if editor.document() != states.last().unwrap() {
                states.push(editor.snapshot());
            }
        }

        for state in states.iter().rev().skip(1) {
            prop_assert!(editor.undo().unwrap());
            prop_assert_eq!(editor.document(), state);
        }
        prop_assert!(!editor.undo().unwrap());

        for state in states.iter().skip(1) {
            prop_assert!(editor.redo().unwrap());
            prop_assert_eq!(editor.document(), state);
        }
        prop_assert!(!editor.redo().unwrap());
    }

    #[test]
    fn every_inverse_restores_the_document(
        actions in prop::collection::vec(action(), 1..15)
    ) {
        let (mut doc, fixture) = setup();
        for action in &actions {
            let before = doc.clone();
            let (_, transaction) = doc.transact("Action", |edit| run(edit, &fixture, action)).unwrap();
            let after = doc.clone();
            doc.revert(&transaction).unwrap();
            prop_assert_eq!(&doc, &before);
            doc.replay(&transaction).unwrap();
            prop_assert_eq!(&doc, &after);
        }
    }
}

#[test]
fn doc_12_letters_typed_in_a_row_undo_together() {
    let mut editor = Editor::new(Document::new_word_processing());
    let first = editor.document().body().get(0).unwrap().id();
    let start = editor.snapshot();
    let mut at = Position::new(first, 0);
    for letter in ["a", "b", "c"] {
        let (next, _) = editor
            .edit_continuing("Typing", |edit| edit.insert_text(at, letter))
            .unwrap();
        at = next;
    }
    let typed = editor.snapshot();
    assert_eq!(typed.body().get(0).unwrap().text(), "abc");
    // Another action starts a new step.
    let (_, joined) = editor
        .edit_continuing("Delete", |edit| {
            edit.delete(Position::new(first, 2), Position::new(first, 3))
        })
        .unwrap();
    assert!(!joined);
    assert!(editor.undo().unwrap());
    assert_eq!(editor.snapshot(), typed);
    assert!(editor.undo().unwrap());
    assert_eq!(editor.snapshot(), start);
    assert!(!editor.undo().unwrap());
    assert!(editor.redo().unwrap());
    assert_eq!(editor.snapshot(), typed);
}
