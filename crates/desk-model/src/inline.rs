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
//!
//! Headers, footers and notes hold objects in their lines too (EST-12,
//! EST-13): their marks are in the paragraphs of those stories.

use std::collections::HashMap;

use crate::document::Document;
use crate::error::ModelError;
use crate::header::{FIELD_CHARACTER, Field};
use crate::id::NodeId;
use crate::object::{Anchor, Frame, Object};
use crate::story::Story;
use crate::text::{Paragraph, Position};
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

    /// Where the marks of the objects in the lines of the headers, the
    /// footers and the notes are: by object, the story and the position
    /// in its paragraphs (EST-12, EST-13).
    pub fn story_object_marks(&self) -> HashMap<NodeId, (Story, Position)> {
        let mut marks = HashMap::new();
        let mut scan = |story: Story, paragraphs: &[Paragraph]| {
            for (object, at) in marks_in(paragraphs) {
                marks.entry(object).or_insert((story, at));
            }
        };
        for (section, margins) in self.all_headers_footers() {
            for place in crate::MarginPlace::ALL {
                scan(Story::Margin { section, place }, margins.get(place));
            }
        }
        for note in self.notes().iter() {
            scan(Story::Note(note.id), &note.paragraphs);
        }
        marks
    }

    /// The position of the mark of an object in the text, if it has one.
    pub fn object_mark(&self, id: NodeId) -> Option<Position> {
        self.object_marks().get(&id).copied()
    }
}

/// The marks of objects in paragraphs, in order: the object and the
/// position of its mark.
pub fn marks_in(paragraphs: &[Paragraph]) -> Vec<(NodeId, Position)> {
    let mut marks = Vec::new();
    for paragraph in paragraphs {
        let mut offset = 0;
        for run in paragraph.runs() {
            if let Some(Some(Field::ObjectMark { object })) = &run.attrs.field {
                marks.push((*object, Position::new(paragraph.id(), offset)));
            }
            offset += run.len();
        }
    }
    marks
}

impl Edit {
    /// Takes the objects in the lines of a story as a copy of the document
    /// has them, after the text of the story was edited in its body with
    /// the tools of the body (see [`Document::with_body`]): those added or
    /// changed there, and not those whose marks are gone from `before`,
    /// the paragraphs the story had (EST-12, EST-13).
    pub fn adopt_story_objects(
        &mut self,
        from: &Document,
        before: &[Paragraph],
    ) -> Result<(), ModelError> {
        let now = from.object_marks();
        for id in now.keys() {
            let Some(object) = from.objects().get(*id) else {
                continue;
            };
            if object.anchor != Anchor::InText {
                continue;
            }
            match self.document().objects().get(*id) {
                Some(same) if same == object => {}
                Some(_) => self.replace_object(object.clone())?,
                None => self.add_object(object.clone())?,
            }
        }
        let body = self.document().object_marks();
        let stories = self.document().story_object_marks();
        for (id, _) in marks_in(before) {
            let gone =
                !now.contains_key(&id) && !body.contains_key(&id) && !stories.contains_key(&id);
            let in_text = self
                .document()
                .objects()
                .get(id)
                .is_some_and(|o| o.anchor == Anchor::InText);
            if gone && in_text {
                self.remove_object(id)?;
            }
        }
        Ok(())
    }

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
        // A copy of a connection line joins nothing until it is joined.
        if let crate::ObjectKind::Shape(crate::Shape::Connection(line)) = &mut copy.kind {
            line.start = None;
            line.end = None;
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

    /// Deletes the mark of an object in a header, a footer or a note.
    pub(crate) fn remove_story_mark(
        &mut self,
        story: Story,
        at: Position,
    ) -> Result<(), ModelError> {
        let mut paragraphs = self.document().story_paragraphs(story);
        let Some(paragraph) = paragraphs.iter_mut().find(|p| p.id() == at.paragraph) else {
            return Ok(());
        };
        let end = at.offset + FIELD_CHARACTER.len_utf8();
        let mut runs = paragraph.slice(0..at.offset)?;
        runs.extend(paragraph.slice(end..paragraph.len())?);
        *paragraph = Paragraph::from_runs(
            paragraph.id(),
            paragraph.style(),
            paragraph.attrs().clone(),
            runs,
        )?;
        self.set_story(story, paragraphs)
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
        let stories = self.document().story_object_marks();
        for &id in ids {
            let in_text = self
                .document()
                .objects()
                .get(id)
                .is_some_and(|o| o.anchor == Anchor::InText);
            if in_text && !marks.contains_key(&id) && !stories.contains_key(&id) {
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

    #[test]
    fn est_12_objects_in_a_header_are_kept_and_go_with_their_marks() {
        use crate::{MarginKind, MarginPages, MarginPlace, Run, Story};
        let place = MarginPlace::new(MarginKind::Header, MarginPages::Default);
        let story = Story::Margin {
            section: None,
            place,
        };
        let mut editor = Editor::new(Document::new_word_processing());
        editor
            .edit("Header", |edit| {
                let style = edit.document().styles().default_paragraph();
                let header = Paragraph::from_runs(
                    edit.new_id(),
                    style,
                    crate::ParaAttrs::default(),
                    vec![Run::new("Hi")],
                )?;
                edit.set_header_footer(place, vec![header])
            })
            .unwrap();
        // Edits the header as its editor does: in the body of a copy.
        let edit_story =
            |editor: &mut Editor, change: &dyn Fn(&mut Edit) -> Result<(), ModelError>| {
                let before = editor.document().story_paragraphs(story);
                let mut copy = Editor::new(editor.document().with_body(before.clone()));
                copy.edit("Story", |edit| change(edit)).unwrap();
                let copy = copy.document().clone();
                editor
                    .edit("Story", |edit| {
                        edit.set_story(story, copy.body().iter().cloned().collect())?;
                        edit.adopt_story_objects(&copy, &before)
                    })
                    .unwrap();
            };
        let id = editor.edit("Id", |edit| Ok(edit.new_id())).unwrap();
        edit_story(&mut editor, &|edit| {
            let line = edit.document().body().get(0).unwrap().id();
            edit.insert_object_in_text(Position::new(line, 2), shape(id))?;
            Ok(())
        });
        let marks = editor.document().story_object_marks();
        assert_eq!(marks.len(), 1, "{marks:?}");
        assert_eq!(marks[&id].0, story);
        assert_eq!(
            editor.document().objects().get(id).unwrap().anchor,
            Anchor::InText
        );
        assert!(editor.document().object_marks().is_empty());
        // Its mark deleted with the text, the object goes too.
        edit_story(&mut editor, &|edit| {
            let line = edit.document().body().get(0).unwrap().id();
            edit.delete(
                Position::new(line, 1),
                Position::new(line, 2 + FIELD_CHARACTER.len_utf8()),
            )?;
            Ok(())
        });
        assert!(editor.document().objects().get(id).is_none());
        assert_eq!(editor.document().story_paragraphs(story)[0].text(), "H");
        // Deleted as an object, its mark goes from the header.
        edit_story(&mut editor, &|edit| {
            let line = edit.document().body().get(0).unwrap().id();
            edit.insert_object_in_text(Position::new(line, 1), shape(id))?;
            Ok(())
        });
        assert!(editor.document().objects().get(id).is_some());
        editor
            .edit("Delete", |edit| edit.delete_objects(&[id]))
            .unwrap();
        assert!(editor.document().story_object_marks().is_empty());
        assert_eq!(editor.document().story_paragraphs(story)[0].text(), "H");
    }
}
