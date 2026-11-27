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
//! EST-13): their marks are in the paragraphs of those stories. So do the
//! texts of text boxes and shapes (W106).

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
    /// footers, the notes and the texts of text boxes and shapes are: by
    /// object, the story and the position in its paragraphs (EST-12,
    /// EST-13, W106).
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
        // A text shows in the first of the boxes linked with it.
        let heads = self.text_box_heads();
        for object in self.objects().iter() {
            if let Some(text) = object.text_box()
                && heads.get(&object.id).is_none_or(|head| *head == object.id)
            {
                scan(Story::TextBox(object.id), &text.paragraphs);
            }
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

/// The objects whose marks are in the texts of objects, those of the
/// objects of groups too, in order (W106).
pub fn objects_in_texts<'a>(objects: impl IntoIterator<Item = &'a Object>) -> Vec<NodeId> {
    let mut out = Vec::new();
    for object in objects {
        if let Some(text) = object.text_box() {
            out.extend(marks_in(&text.paragraphs).into_iter().map(|(id, _)| id));
        }
        if let crate::ObjectKind::Group(members) = &object.kind {
            out.extend(objects_in_texts(members.iter()));
        }
    }
    out
}

impl Document {
    /// The objects of the document in the texts of objects, and in theirs
    /// in turn, each once, to copy them with those objects (W106).
    pub fn objects_in_texts_of(&self, objects: &[Object]) -> Vec<Object> {
        let mut out: Vec<Object> = Vec::new();
        let mut seen: std::collections::HashSet<NodeId> = objects.iter().map(|o| o.id).collect();
        let mut next = objects_in_texts(objects.iter());
        while !next.is_empty() {
            let mut found = Vec::new();
            for id in next {
                if seen.insert(id)
                    && let Some(object) = self.objects().get(id)
                {
                    found.push(object.clone());
                }
            }
            next = objects_in_texts(found.iter());
            out.extend(found);
        }
        out
    }
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
        let marked: Vec<Object> = now
            .keys()
            .filter_map(|id| from.objects().get(*id))
            .filter(|o| o.anchor == Anchor::InText)
            .cloned()
            .collect();
        for object in &marked {
            match self.document().objects().get(object.id) {
                Some(same) if same == object => {}
                Some(_) => self.replace_object(object.clone())?,
                None => self.add_object(object.clone())?,
            }
        }
        // With the objects in their texts pasted with them, which the
        // document does not have yet (W106).
        for object in from.objects_in_texts_of(&marked) {
            if object.anchor == Anchor::InText && self.document().objects().get(object.id).is_none()
            {
                self.add_object(object)?;
            }
        }
        let before: Vec<NodeId> = marks_in(before).into_iter().map(|(id, _)| id).collect();
        self.remove_unmarked_objects(&before)
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
        // The tracked changes its mark is in, which the mark keeps where
        // it goes (W148).
        let mut changes = (None, None);
        if let Some(mark) = self.document().object_mark(id) {
            let end = mark.offset + FIELD_CHARACTER.len_utf8();
            if let Some(run) = self
                .document()
                .paragraph(mark.paragraph)?
                .slice(mark.offset..end)?
                .first()
            {
                changes = (run.attrs.change, run.attrs.former);
            }
            self.remove_mark(mark)?;
            if mark.paragraph == at.paragraph && mark.offset < at.offset {
                at.offset -= FIELD_CHARACTER.len_utf8();
            }
        }
        if object.anchor != Anchor::InText {
            // Out of the change that moved it, which goes when no object
            // is left in it.
            let mut placed = object;
            placed.anchor = Anchor::InText;
            placed.rotation = 0;
            placed.moved = None;
            self.replace_object(placed)?;
        }
        // Moving an object that is there already, its mark is no text
        // added, which rejecting would take away with the object: it
        // keeps the changes it was in (W148).
        let after = self.untracked(|edit| edit.insert_mark(at, id))?;
        if changes != (None, None) {
            self.map_runs(at, after, |run| {
                run.attrs.change = changes.0;
                run.attrs.former = changes.1;
            })?;
        }
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
        // A copy is a new object, without the comments of the first, on
        // it or on its cells.
        copy.comments = crate::CommentIds::none();
        // Nor its tracked changes of place or of cells (W148, REV-05).
        copy.untrack();
        if let crate::ObjectKind::Table(table) = &mut copy.kind
            && !table.comments().is_empty()
        {
            let table = std::sync::Arc::make_mut(table);
            for ((r, c), _) in table.comments() {
                table.cell_mut(r, c).comment = None;
            }
        }
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

    /// Removes the objects in the text whose marks are gone, and those in
    /// their texts.
    pub(crate) fn remove_unmarked_objects(&mut self, ids: &[NodeId]) -> Result<(), ModelError> {
        // Those in the texts of the objects removed go after them, as
        // their marks go with those texts (W106).
        let mut next = ids.to_vec();
        let mut seen = std::collections::HashSet::new();
        while !next.is_empty() {
            let marks = self.document().object_marks();
            let stories = self.document().story_object_marks();
            let mut inner = Vec::new();
            for id in next {
                if !seen.insert(id) || marks.contains_key(&id) || stories.contains_key(&id) {
                    continue;
                }
                let Some(object) = self.document().objects().get(id) else {
                    continue;
                };
                if object.anchor != Anchor::InText {
                    continue;
                }
                inner.extend(objects_in_texts([object]));
                self.remove_object(id)?;
            }
            next = inner;
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

    /// A document with a text box on its first page, and the box.
    fn with_text_box() -> (Editor, NodeId) {
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Box", |edit| {
                let style = edit.document().styles().default_paragraph();
                let paragraph = crate::Paragraph::new(edit.new_id(), style);
                let mut text_box = Object::new(
                    edit.new_id(),
                    ObjectKind::TextBox(crate::TextBox::new(
                        std::sync::Arc::from([paragraph]),
                        Length::from_points(4),
                    )),
                    Frame::from_pt(10.0, 10.0, 200.0, 100.0),
                );
                text_box.anchor = Anchor::Page(0);
                let id = text_box.id;
                edit.add_object(text_box)?;
                Ok(id)
            })
            .unwrap();
        (editor, id)
    }

    #[test]
    fn w106_objects_go_in_the_text_of_a_text_box_and_out_with_it() {
        let (mut editor, boxed) = with_text_box();
        let before = editor.document().clone();
        let story = Story::TextBox(boxed);
        let inner = editor
            .edit("Inline", |edit| {
                edit.edit_story(story, |edit| {
                    let start = edit.document().start();
                    let at = edit.insert_text(start, "ab")?;
                    let id = edit.new_id();
                    edit.insert_object_in_text(Position::new(at.paragraph, 1), shape(id))?;
                    Ok(id)
                })
            })
            .unwrap();
        let document = editor.document();
        assert_eq!(
            document.objects().get(inner).unwrap().anchor,
            Anchor::InText
        );
        let marks = document.story_object_marks();
        assert_eq!(marks.get(&inner).map(|m| m.0), Some(story));
        assert_eq!(objects_in_texts(document.objects().iter()), [inner]);
        // A copy of the box has a copy of the object in its text.
        let copies = editor
            .edit("Duplicate", |edit| {
                edit.duplicate_objects(&[boxed], Length::ZERO, Length::ZERO)
            })
            .unwrap();
        let copy = editor.document().objects().get(copies[0]).unwrap().clone();
        let copied = objects_in_texts([&copy]);
        assert_eq!(copied.len(), 1);
        assert_ne!(copied[0], inner);
        assert_eq!(
            editor.document().objects().get(copied[0]).unwrap().anchor,
            Anchor::InText
        );
        // Deleting the copy deletes the object in its text, and only it.
        editor
            .edit("Delete", |edit| edit.delete_objects(&copies))
            .unwrap();
        assert!(editor.document().objects().get(copied[0]).is_none());
        assert!(editor.document().objects().get(inner).is_some());
        // Deleting its mark in the text deletes it.
        editor
            .edit("Delete", |edit| {
                edit.edit_story(story, |edit| {
                    let first = edit.document().start().paragraph;
                    let end = Position::new(first, 1 + FIELD_CHARACTER.len_utf8());
                    edit.delete(Position::new(first, 1), end).map(|_| ())
                })
            })
            .unwrap();
        assert!(editor.document().objects().get(inner).is_none());
        assert_eq!(editor.document().story_paragraphs(story)[0].text(), "ab");
        for _ in 0..4 {
            editor.undo().unwrap();
        }
        assert_eq!(editor.document(), &before);
    }

    /// A shape with a mark of an object as its text.
    fn holding(id: NodeId, inner: NodeId, edit: &mut Edit) -> Result<Object, ModelError> {
        let mut holder = shape(id);
        let style = edit.document().styles().default_paragraph();
        let runs = [crate::Run::with_attrs(
            &FIELD_CHARACTER.to_string(),
            None,
            crate::CharAttrs {
                field: Some(Some(Field::ObjectMark { object: inner })),
                ..crate::CharAttrs::default()
            },
        )];
        let paragraph =
            Paragraph::from_runs(edit.new_id(), style, crate::ParaAttrs::default(), runs)?;
        holder.text = Some(crate::TextBox::in_shape(std::sync::Arc::from([paragraph])));
        holder.anchor = Anchor::InText;
        Ok(holder)
    }

    #[test]
    fn w106_objects_in_texts_in_texts_are_copied_and_deleted_with_them() {
        let (mut editor, boxed) = with_text_box();
        let story = Story::TextBox(boxed);
        // A shape in the box, and a square in the text of the shape.
        let (outer, inner) = editor
            .edit("Nest", |edit| {
                let inner = edit.new_id();
                let mut square = shape(inner);
                square.anchor = Anchor::InText;
                edit.add_object(square)?;
                let outer = edit.new_id();
                let holder = holding(outer, inner, edit)?;
                edit.add_object(holder)?;
                edit.edit_story(story, |edit| {
                    let start = edit.document().start();
                    edit.put_object_in_text(outer, start)
                })?;
                Ok((outer, inner))
            })
            .unwrap();
        let all = editor.document().objects().len();
        // Copied into another document, from the objects alone.
        let objects: Vec<Object> = [boxed, outer, inner]
            .iter()
            .map(|id| editor.document().objects().get(*id).unwrap().clone())
            .collect();
        let mut other = Editor::new(Document::new_word_processing());
        let ids = other
            .edit("Paste", |edit| edit.add_copies(&objects))
            .unwrap();
        assert_eq!(ids.len(), 1);
        let pasted = other.document();
        assert_eq!(pasted.objects().len(), 3);
        let copy = pasted.objects().get(ids[0]).unwrap();
        let [shape_copy] = objects_in_texts([copy])[..] else {
            panic!("one object in the box");
        };
        let shape_copy = pasted.objects().get(shape_copy).unwrap();
        let [square_copy] = objects_in_texts([shape_copy])[..] else {
            panic!("one object in the shape");
        };
        assert!(pasted.objects().get(square_copy).is_some());
        // Duplicated, with both.
        editor
            .edit("Duplicate", |edit| {
                edit.duplicate_objects(&[boxed], Length::ZERO, Length::ZERO)
            })
            .unwrap();
        assert_eq!(editor.document().objects().len(), all * 2);
        // Deleting the box deletes the shape and the square in it.
        editor
            .edit("Delete", |edit| edit.delete_objects(&[boxed]))
            .unwrap();
        let document = editor.document();
        assert_eq!(document.objects().len(), all);
        assert!(document.objects().get(outer).is_none());
        assert!(document.objects().get(inner).is_none());
    }

    #[test]
    fn w106_objects_pasted_in_a_story_come_with_those_in_their_texts() {
        let (mut editor, boxed) = with_text_box();
        let pasted = editor
            .edit("Paste", |edit| {
                let inner = edit.new_id();
                let square = shape(inner);
                let outer = edit.new_id();
                let mut holder = holding(outer, inner, edit)?;
                holder.anchor = Anchor::Page(0);
                edit.edit_story(Story::TextBox(boxed), |edit| {
                    let start = edit.document().start();
                    let ids = edit.add_copies(&[holder, square])?;
                    edit.put_object_in_text(ids[0], start)?;
                    Ok(ids[0])
                })
            })
            .unwrap();
        let document = editor.document();
        let holder = document.objects().get(pasted).unwrap();
        let [inner] = objects_in_texts([holder])[..] else {
            panic!("one object in the shape");
        };
        assert_eq!(
            document.objects().get(inner).unwrap().anchor,
            Anchor::InText
        );
    }

    #[test]
    fn w106_a_mark_in_the_text_of_its_own_object_is_copied_once() {
        let (mut editor, boxed) = with_text_box();
        editor
            .edit("Nest", |edit| {
                // A shape in the box, whose text holds the box: a loop.
                let id = edit.new_id();
                let mut shape_with_text = shape(id);
                let style = edit.document().styles().default_paragraph();
                let runs = [crate::Run::with_attrs(
                    &FIELD_CHARACTER.to_string(),
                    None,
                    crate::CharAttrs {
                        field: Some(Some(Field::ObjectMark { object: boxed })),
                        ..crate::CharAttrs::default()
                    },
                )];
                let paragraph =
                    Paragraph::from_runs(edit.new_id(), style, crate::ParaAttrs::default(), runs)?;
                shape_with_text.text =
                    Some(crate::TextBox::in_shape(std::sync::Arc::from([paragraph])));
                shape_with_text.anchor = Anchor::InText;
                edit.add_object(shape_with_text)?;
                edit.edit_story(Story::TextBox(boxed), |edit| {
                    let start = edit.document().start();
                    edit.put_object_in_text(id, start)
                })
            })
            .unwrap();
        let count = editor.document().objects().len();
        editor
            .edit("Duplicate", |edit| {
                edit.duplicate_objects(&[boxed], Length::ZERO, Length::ZERO)
            })
            .unwrap();
        // The box and the shape in it, whose text leaves the box out.
        assert_eq!(editor.document().objects().len(), count + 2);
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
