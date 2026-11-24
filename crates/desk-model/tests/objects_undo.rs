// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests: any sequence of edits of objects and of the cells of
//! tables can be undone back to the start and redone to the end, and
//! each one's inverse restores the document (review RV, batch 3).

use std::sync::Arc;

use desk_model::table::{Area, Locale};
use desk_model::{
    Document, Edit, Editor, Frame, Layer, Length, ModelError, NodeId, Object, ObjectKind,
    ParaAttrs, Paragraph, Run, Table, TextBox,
};
use proptest::prelude::*;

/// An edit described by numbers, which pick objects as the document has
/// them, from the back, when the edit runs.
#[derive(Clone, Debug)]
enum Action {
    AddTextBox(u8),
    AddTable(u8, u8),
    Delete(usize),
    Move(usize, i8),
    Rotate(usize, i16),
    Flip(usize, bool),
    Lock(usize, bool),
    Link(usize, usize),
    Layer(usize, u8),
    Duplicate(usize),
    Group(usize, usize),
    Ungroup(usize),
    Cells(usize, CellEdit),
}

/// A change of the cells of a table.
#[derive(Clone, Debug)]
enum CellEdit {
    InsertRows(u8, u8),
    InsertCols(u8, u8),
    DeleteRows(u8),
    DeleteCols(u8),
    Type(u8, u8, String),
    Footer(u8),
    Merge(u8, u8),
    Sort(u8, bool),
}

fn cell_edit() -> impl Strategy<Value = CellEdit> {
    prop_oneof![
        (0..6u8, 1..3u8).prop_map(|(a, n)| CellEdit::InsertRows(a, n)),
        (0..6u8, 1..3u8).prop_map(|(a, n)| CellEdit::InsertCols(a, n)),
        (0..6u8).prop_map(CellEdit::DeleteRows),
        (0..6u8).prop_map(CellEdit::DeleteCols),
        (
            0..6u8,
            0..6u8,
            prop_oneof![
                Just("1".to_owned()),
                Just("x".to_owned()),
                Just("=A1+1".to_owned()),
                Just("=SUM(A1:B3)".to_owned()),
            ]
        )
            .prop_map(|(r, c, t)| CellEdit::Type(r, c, t)),
        (0..3u8).prop_map(CellEdit::Footer),
        (0..6u8, 0..6u8).prop_map(|(r, c)| CellEdit::Merge(r, c)),
        (0..6u8, any::<bool>()).prop_map(|(c, up)| CellEdit::Sort(c, up)),
    ]
}

fn action() -> impl Strategy<Value = Action> {
    let pick = 0..8usize;
    prop_oneof![
        2 => (0..3u8).prop_map(Action::AddTextBox),
        2 => (1..5u8, 1..5u8).prop_map(|(r, c)| Action::AddTable(r, c)),
        1 => pick.clone().prop_map(Action::Delete),
        1 => (pick.clone(), any::<i8>()).prop_map(|(i, d)| Action::Move(i, d)),
        1 => (pick.clone(), any::<i16>()).prop_map(|(i, r)| Action::Rotate(i, r)),
        1 => (pick.clone(), any::<bool>()).prop_map(|(i, h)| Action::Flip(i, h)),
        1 => (pick.clone(), any::<bool>()).prop_map(|(i, l)| Action::Lock(i, l)),
        2 => (pick.clone(), pick.clone()).prop_map(|(a, b)| Action::Link(a, b)),
        1 => (pick.clone(), 0..4u8).prop_map(|(i, l)| Action::Layer(i, l)),
        1 => pick.clone().prop_map(Action::Duplicate),
        1 => (pick.clone(), pick.clone()).prop_map(|(a, b)| Action::Group(a, b)),
        1 => pick.clone().prop_map(Action::Ungroup),
        4 => (pick, cell_edit()).prop_map(|(i, e)| Action::Cells(i, e)),
    ]
}

/// The object at a place from the back, wrapping round.
fn object(edit: &Edit, i: usize) -> Option<NodeId> {
    let objects = edit.document().objects();
    let n = objects.len();
    (n > 0)
        .then(|| objects.iter().nth(i % n).map(|o| o.id))
        .flatten()
}

fn frame() -> Frame {
    Frame::from_pt(10.0, 10.0, 80.0, 40.0)
}

fn change_cells(table: &mut Table, edit: &CellEdit) {
    let en = Locale::en_us();
    let rows = u32::from;
    // Errors of out-of-range edits leave the table as it was.
    match edit {
        CellEdit::InsertRows(at, n) => {
            let _ = table.insert_rows(rows(*at).min(table.rows()), rows(*n));
        }
        CellEdit::InsertCols(at, n) => {
            let _ = table.insert_cols(rows(*at).min(table.cols()), rows(*n));
        }
        CellEdit::DeleteRows(at) => {
            let _ = table.delete_rows(rows(*at) % table.rows(), 1);
        }
        CellEdit::DeleteCols(at) => {
            let _ = table.delete_cols(rows(*at) % table.cols(), 1);
        }
        CellEdit::Type(r, c, text) => {
            let (r, c) = (rows(*r) % table.rows(), rows(*c) % table.cols());
            table.type_text(r, c, text, &en);
        }
        CellEdit::Footer(n) => {
            let _ = table.set_footer_rows(rows(*n));
        }
        CellEdit::Merge(r, c) => {
            let (r, c) = (rows(*r) % table.rows(), rows(*c) % table.cols());
            let to = ((r + 1).min(table.rows() - 1), (c + 1).min(table.cols() - 1));
            let _ = table.merge(Area::between((r, c), to), &en);
        }
        CellEdit::Sort(c, up) => {
            let c = rows(*c) % table.cols();
            table.sort(c, *up, None, 0.0);
        }
    }
}

fn run(edit: &mut Edit, action: &Action) -> Result<(), ModelError> {
    match action {
        Action::AddTextBox(n) => {
            let style = edit.document().styles().default_paragraph();
            let paragraphs: Vec<Paragraph> = (0..=*n)
                .map(|k| {
                    Paragraph::from_runs(
                        edit.new_id(),
                        style,
                        ParaAttrs::default(),
                        [Run::new(&format!("text {k}"))],
                    )
                })
                .collect::<Result<_, _>>()?;
            let id = edit.new_id();
            edit.add_object(Object::new(
                id,
                ObjectKind::TextBox(TextBox::new(Arc::from(paragraphs), Length::from_points(4))),
                frame(),
            ))
        }
        Action::AddTable(r, c) => {
            let id = edit.new_id();
            let table = Table::new(u32::from(*r).max(2), u32::from(*c).max(2));
            edit.add_object(Object::new(id, ObjectKind::Table(Arc::new(table)), frame()))
        }
        Action::Delete(i) => match object(edit, *i) {
            Some(id) => edit.delete_objects(&[id]),
            None => Ok(()),
        },
        Action::Move(i, d) => match object(edit, *i) {
            Some(id) => {
                let d = Length::from_points(i32::from(*d));
                edit.move_objects(&[id], d, d).map(drop)
            }
            None => Ok(()),
        },
        Action::Rotate(i, r) => match object(edit, *i) {
            Some(id) => edit.rotate_objects(&[id], i32::from(*r) * 10),
            None => Ok(()),
        },
        Action::Flip(i, h) => match object(edit, *i) {
            Some(id) => edit.flip_objects(&[id], *h),
            None => Ok(()),
        },
        Action::Lock(i, l) => match object(edit, *i) {
            Some(id) => edit.lock_objects(&[id], *l),
            None => Ok(()),
        },
        Action::Link(a, b) => match (object(edit, *a), object(edit, *b)) {
            (Some(a), Some(b)) if a != b => {
                let boxes = |id| {
                    matches!(
                        edit.document().objects().get(id).map(|o| &o.kind),
                        Some(ObjectKind::TextBox(_))
                    )
                };
                if boxes(a) && boxes(b) {
                    edit.link_text_box(a, b)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        },
        Action::Layer(i, l) => match object(edit, *i) {
            Some(id) => {
                let layer = [Layer::Front, Layer::Forward, Layer::Backward, Layer::Back]
                    [usize::from(*l) % 4];
                edit.layer_objects(&[id], layer)
            }
            None => Ok(()),
        },
        Action::Duplicate(i) => match object(edit, *i) {
            Some(id) => edit
                .duplicate_objects(&[id], Length::from_points(5), Length::from_points(5))
                .map(drop),
            None => Ok(()),
        },
        Action::Group(a, b) => match (object(edit, *a), object(edit, *b)) {
            (Some(a), Some(b)) if a != b => edit.group_objects(&[a, b]).map(drop),
            _ => Ok(()),
        },
        Action::Ungroup(i) => match object(edit, *i) {
            Some(id) => edit.ungroup_objects(&[id]).map(drop),
            None => Ok(()),
        },
        Action::Cells(i, cells) => {
            let Some(id) = object(edit, *i) else {
                return Ok(());
            };
            let Some(mut object) = edit.document().objects().get(id).cloned() else {
                return Ok(());
            };
            let ObjectKind::Table(table) = &mut object.kind else {
                return Ok(());
            };
            if object.locked {
                return Ok(());
            }
            change_cells(Arc::make_mut(table), cells);
            edit.replace_object(object)
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn edits_of_objects_and_cells_undo_and_redo_through_the_same_documents(
        actions in prop::collection::vec(action(), 1..25)
    ) {
        let mut editor = Editor::new(Document::new_word_processing());
        let mut states = vec![editor.snapshot()];
        for action in &actions {
            editor.edit("Action", |edit| run(edit, action)).unwrap();
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
    fn every_inverse_of_an_edit_of_objects_restores_the_document(
        actions in prop::collection::vec(action(), 1..15)
    ) {
        let mut doc = Document::new_word_processing();
        for action in &actions {
            let before = doc.clone();
            let (_, transaction) = doc.transact("Action", |edit| run(edit, action)).unwrap();
            let after = doc.clone();
            doc.revert(&transaction).unwrap();
            prop_assert_eq!(&doc, &before);
            doc.replay(&transaction).unwrap();
            prop_assert_eq!(&doc, &after);
        }
    }
}
