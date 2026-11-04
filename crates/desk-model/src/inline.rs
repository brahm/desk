// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Objects in the line of the text, as Pages puts inline objects
//! (OBJ-01).
//!
//! An inline object is kept with the other objects of the document,
//! anchored in the text ([`Anchor::InText`]), and its place is a mark in
//! the body: a field character ([`Field::ObjectMark`]). The text lays the
//! mark out as a character as wide and as tall as the frame of the
//! object, standing on the baseline. Deleting the mark deletes the
//! object, and deleting the object deletes its mark.

use std::collections::HashMap;

use crate::document::Document;
use crate::error::ModelError;
use crate::header::{FIELD_CHARACTER, Field};
use crate::id::NodeId;
use crate::object::{Anchor, Frame, Object};
use crate::text::Position;
use crate::transaction::Edit;

impl Document {
    /// Where the mark of each object in the text is: by object, the
    /// position of its mark in the body.
    pub fn object_marks(&self) -> HashMap<NodeId, Position> {
        let mut marks = HashMap::new();
        for paragraph in self.body().iter() {
            let mut offset = 0;
            for run in paragraph.runs() {
                if let Some(Some(Field::ObjectMark { object })) = &run.attrs.field {
                    marks
                        .entry(*object)
                        .or_insert(Position::new(paragraph.id(), offset));
                }
                offset += run.len();
            }
        }
        marks
    }

    /// The position of the mark of an object in the text, if it has one.
    pub fn object_mark(&self, id: NodeId) -> Option<Position> {
        self.object_marks().get(&id).copied()
    }
}

impl Edit {
    /// Puts an object in the line of the text, at a position of the body,
    /// and returns the position after its mark (OBJ-01).
    pub fn insert_object_in_text(
        &mut self,
        at: Position,
        mut object: Object,
    ) -> Result<Position, ModelError> {
        self.document().paragraph(at.paragraph)?;
        object.anchor = Anchor::InText;
        let id = object.id;
        self.add_object(object)?;
        self.insert_mark(at, id)
    }

    fn insert_mark(&mut self, at: Position, id: NodeId) -> Result<Position, ModelError> {
        let after = self.insert_text(at, &FIELD_CHARACTER.to_string())?;
        // The mark keeps the formatting of the text around it, for the
        // height of its line, and is nothing but the place of the object.
        let attrs = crate::CharAttrs {
            field: Some(Some(Field::ObjectMark { object: id })),
            ..crate::CharAttrs::default()
        };
        self.set_character_attrs(at, after, &attrs)?;
        Ok(after)
    }

    /// Moves a floating object into the line of the text, at a position of
    /// the body. An object already in the text moves there. A locked
    /// object stays where it is.
    pub fn put_object_in_text(&mut self, id: NodeId, at: Position) -> Result<(), ModelError> {
        let object = self
            .document()
            .objects()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownObject(id))?;
        if object.locked {
            return Ok(());
        }
        self.document().paragraph(at.paragraph)?;
        let mut at = at;
        if let Some(mark) = self.document().object_mark(id) {
            self.remove_mark(mark)?;
            if mark.paragraph == at.paragraph && mark.offset < at.offset {
                at.offset -= FIELD_CHARACTER.len_utf8();
            }
        }
        if object.anchor != Anchor::InText {
            let mut placed = object;
            placed.anchor = Anchor::InText;
            placed.rotation = 0;
            self.replace_object(placed)?;
        }
        self.insert_mark(at, id)?;
        Ok(())
    }

    /// Takes an object out of the text to float at a frame and an anchor,
    /// as where it was drawn. A locked object stays where it is.
    pub fn float_object(
        &mut self,
        id: NodeId,
        anchor: Anchor,
        frame: Frame,
    ) -> Result<(), ModelError> {
        let object = self
            .document()
            .objects()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownObject(id))?;
        if object.locked || anchor == Anchor::InText {
            return Ok(());
        }
        if let Some(mark) = self.document().object_mark(id) {
            self.remove_mark(mark)?;
        }
        let mut placed = crate::reframed(&object, frame);
        placed.anchor = anchor;
        self.replace_object(placed)
    }

    /// A copy of an object with identifiers of its own, for it and its
    /// paragraphs, and those of the objects of a group.
    pub(crate) fn renumbered_object(&mut self, object: &Object) -> Result<Object, ModelError> {
        let mut copy = object.clone();
        copy.id = self.new_id();
        let renumber = |edit: &mut Self, text: &mut crate::TextBox| -> Result<(), ModelError> {
            let paragraphs = text
                .paragraphs
                .iter()
                .map(|p| {
                    crate::Paragraph::from_runs(
                        edit.new_id(),
                        p.style(),
                        p.attrs().clone(),
                        p.runs().cloned(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            text.paragraphs = paragraphs.into();
            text.next = None;
            Ok(())
        };
        if let Some(text) = copy.text_box_mut() {
            let mut text = text.clone();
            renumber(self, &mut text)?;
            *copy.text_box_mut().expect("it has text") = text;
        }
        if let crate::ObjectKind::Group(members) = &copy.kind {
            let members = members
                .iter()
                .map(|m| self.renumbered_object(m))
                .collect::<Result<Vec<_>, _>>()?;
            copy.kind = crate::ObjectKind::Group(members.into());
        }
        Ok(copy)
    }

    /// Deletes the mark at a position, and only it.
    pub(crate) fn remove_mark(&mut self, at: Position) -> Result<(), ModelError> {
        let end = Position::new(at.paragraph, at.offset + FIELD_CHARACTER.len_utf8());
        self.apply(crate::op::Op::ReplaceText {
            paragraph: at.paragraph,
            range: at.offset..end.offset,
            runs: Vec::new(),
        })
    }

    /// Removes the objects in the text whose marks are gone.
    pub(crate) fn remove_unmarked_objects(&mut self, ids: &[NodeId]) -> Result<(), ModelError> {
        if ids.is_empty() {
            return Ok(());
        }
        let marks = self.document().object_marks();
        for &id in ids {
            let in_text = self
                .document()
                .objects()
                .get(id)
                .is_some_and(|o| o.anchor == Anchor::InText);
            if in_text && !marks.contains_key(&id) {
                self.remove_object(id)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectKind, Shape};
    use crate::{Editor, Length};

    fn shape(id: NodeId) -> Object {
        Object::new(
            id,
            ObjectKind::Shape(Shape::Rectangle),
            Frame::from_pt(0.0, 0.0, 30.0, 20.0),
        )
    }

    #[test]
    fn obj_01_objects_go_in_the_text_and_out_of_it() {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        let empty = editor.document().clone();
        let id = editor
            .edit("Inline", |edit| {
                let at = edit.insert_text(start, "ab")?;
                let id = edit.new_id();
                let at = Position::new(at.paragraph, 1);
                let after = edit.insert_object_in_text(at, shape(id))?;
                assert_eq!(after.offset, 1 + FIELD_CHARACTER.len_utf8());
                Ok(id)
            })
            .unwrap();
        let document = editor.document();
        assert_eq!(document.objects().get(id).unwrap().anchor, Anchor::InText);
        assert_eq!(
            document.object_mark(id),
            Some(Position::new(start.paragraph, 1))
        );
        // Out of the text, and back at the end.
        editor
            .edit("Float", |edit| {
                edit.float_object(id, Anchor::Page(0), Frame::from_pt(5.0, 6.0, 30.0, 20.0))
            })
            .unwrap();
        let document = editor.document();
        assert_eq!(document.object_mark(id), None);
        assert_eq!(document.body().get(0).unwrap().text(), "ab");
        assert_eq!(
            document.objects().get(id).unwrap().frame.x,
            Length::from_points(5)
        );
        editor
            .edit("Inline", |edit| {
                edit.put_object_in_text(id, Position::new(start.paragraph, 2))
            })
            .unwrap();
        assert_eq!(
            editor.document().object_mark(id),
            Some(Position::new(start.paragraph, 2))
        );
        // Deleting the text deletes the object, and undo brings both back.
        editor
            .edit("Delete", |edit| {
                let end = Position::new(start.paragraph, 2 + FIELD_CHARACTER.len_utf8());
                edit.delete(Position::new(start.paragraph, 1), end)
                    .map(|_| ())
            })
            .unwrap();
        assert!(editor.document().objects().is_empty());
        assert_eq!(editor.document().body().get(0).unwrap().text(), "a");
        editor.undo().unwrap();
        assert!(editor.document().objects().get(id).is_some());
        // Deleting the object deletes its mark.
        editor
            .edit("Delete Object", |edit| edit.delete_objects(&[id]))
            .unwrap();
        assert_eq!(editor.document().body().get(0).unwrap().text(), "ab");
        for _ in 0..5 {
            editor.undo().unwrap();
        }
        assert_eq!(editor.document(), &empty);
    }

    #[test]
    fn obj_01_a_copied_object_in_the_text_is_pasted_as_another() {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        let id = editor
            .edit("Inline", |edit| {
                let at = edit.insert_text(start, "ab")?;
                let id = edit.new_id();
                edit.insert_object_in_text(Position::new(at.paragraph, 1), shape(id))?;
                Ok(id)
            })
            .unwrap();
        let mark = Position::new(start.paragraph, 1);
        let after = Position::new(start.paragraph, 1 + FIELD_CHARACTER.len_utf8());
        let fragment = editor.document().copy_fragment(mark, after).unwrap();
        assert_eq!(fragment.objects.len(), 1);
        let end = editor.document().end();
        editor
            .edit("Paste", |edit| {
                edit.paste_fragment(end, &fragment).map(|_| ())
            })
            .unwrap();
        let document = editor.document();
        let marks = document.object_marks();
        assert_eq!(document.objects().len(), 2);
        assert_eq!(marks.len(), 2);
        let other = marks.keys().find(|k| **k != id).unwrap();
        assert_eq!(
            document.objects().get(*other).unwrap().frame,
            document.objects().get(id).unwrap().frame
        );
    }
}
