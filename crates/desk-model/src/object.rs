// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Objects on the pages: shapes, lines, text boxes, pictures and groups
//! (OBJ-01, OBJ-08, OBJ-13, OBJ-17).
//!
//! An object is a frame on a page with something drawn in it. A floating
//! object stays where it is put, on its page or beside the paragraph it
//! moves with, and the text of the body goes around it as its wrap says.
//! The objects of a document are kept from back to front, in the order
//! they are drawn.

use std::sync::Arc;

use crate::format::BorderStyle;
use crate::id::NodeId;
use crate::picture::Picture;
use crate::text::Paragraph;
use crate::units::{Length, Rgba};

/// The parts of a ten thousand that the points of a polygon are measured
/// in, across the width and down the height of its frame.
pub const POLYGON_SCALE: i32 = 10_000;

/// An object on the pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    /// The identifier.
    pub id: NodeId,
    /// What the object is.
    pub kind: ObjectKind,
    /// Where the object is, before it is rotated: by its top left corner,
    /// as its anchor says.
    pub frame: Frame,
    /// What the object moves with.
    pub anchor: Anchor,
    /// How far it is rotated around its center, counterclockwise, in
    /// hundredths of a degree, as the Metrics inspector shows it.
    pub rotation: i32,
    /// Whether it is flipped left to right.
    pub flip_horizontal: bool,
    /// Whether it is flipped upside down.
    pub flip_vertical: bool,
    /// How it is filled and outlined.
    pub style: GraphicStyle,
    /// How the text of the body goes around it (OBJ-17).
    pub wrap: Wrap,
    /// The room left between the object and the text around it.
    pub wrap_margin: Length,
    /// Whether it cannot be moved, resized or deleted until unlocked.
    pub locked: bool,
}

impl Object {
    /// An object with a frame on the first page, no fill and no line,
    /// that the text goes around on both sides.
    pub fn new(id: NodeId, kind: ObjectKind, frame: Frame) -> Self {
        Self {
            id,
            kind,
            frame,
            anchor: Anchor::Page(0),
            rotation: 0,
            flip_horizontal: false,
            flip_vertical: false,
            style: GraphicStyle::default(),
            wrap: Wrap::Both,
            wrap_margin: Length::from_points(12),
            locked: false,
        }
    }

    /// The paragraphs of its text, if it is a text box, and those of the
    /// objects of a group.
    pub fn paragraphs(&self) -> Box<dyn Iterator<Item = &Paragraph> + '_> {
        match &self.kind {
            ObjectKind::TextBox(text) => Box::new(text.paragraphs.iter()),
            ObjectKind::Group(objects) => Box::new(objects.iter().flat_map(Self::paragraphs)),
            _ => Box::new(std::iter::empty()),
        }
    }

    /// Every identifier inside the object: its own, those of its
    /// paragraphs, and those of the objects of a group.
    pub fn ids(&self) -> Vec<NodeId> {
        let mut ids = vec![self.id];
        if let ObjectKind::Group(objects) = &self.kind {
            ids.extend(objects.iter().flat_map(Self::ids));
        }
        ids.extend(self.paragraphs().map(Paragraph::id));
        ids
    }
}

/// What an object is.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ObjectKind {
    /// A shape or a line (OBJ-08).
    Shape(Shape),
    /// A text box (OBJ-13).
    TextBox(TextBox),
    /// A picture, scaled to its frame (OBJ-02).
    Image(Picture),
    /// Objects grouped into one, with frames on the same page as the
    /// group's, from back to front.
    Group(Arc<[Object]>),
}

impl From<Shape> for ObjectKind {
    fn from(shape: Shape) -> Self {
        Self::Shape(shape)
    }
}

/// The outline of a shape, scaled to its frame.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Shape {
    /// A rectangle.
    Rectangle,
    /// A rectangle with round corners.
    RoundedRectangle {
        /// The radius of the corners.
        radius: Length,
    },
    /// An ellipse, or a circle in a square frame.
    Ellipse,
    /// A closed polygon, by its points in parts of [`POLYGON_SCALE`] of
    /// the frame from its top left corner.
    Polygon(Arc<[(i32, i32)]>),
    /// A line between two corners of the frame: from the top left to the
    /// bottom right, or from the bottom left to the top right when it
    /// rises.
    Line {
        /// Whether it goes from the bottom left to the top right.
        rising: bool,
    },
}

impl Shape {
    /// A triangle with its point at the top middle.
    pub fn triangle() -> Self {
        Self::Polygon(Arc::from(
            [
                (POLYGON_SCALE / 2, 0),
                (POLYGON_SCALE, POLYGON_SCALE),
                (0, POLYGON_SCALE),
            ]
            .as_slice(),
        ))
    }
}

/// A text box: paragraphs laid out inside a frame (OBJ-13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBox {
    /// The text.
    pub paragraphs: Arc<[Paragraph]>,
    /// The room between the frame and the text, on every side.
    pub inset: Length,
}

/// A rectangle on the page, from its top left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Frame {
    /// The left edge.
    pub x: Length,
    /// The top edge.
    pub y: Length,
    /// The width.
    pub width: Length,
    /// The height.
    pub height: Length,
}

impl Frame {
    /// A frame from its position and size, in points.
    pub fn from_pt(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x: Length::from_pt(x),
            y: Length::from_pt(y),
            width: Length::from_pt(width),
            height: Length::from_pt(height),
        }
    }
}

/// What a floating object moves with (OBJ-01).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Anchor {
    /// Fixed on a page, by its index from 0: the frame is measured from
    /// the top left corner of the paper, as Pages keeps floating objects.
    Page(u32),
    /// Moving with a paragraph of the body, as other programs anchor
    /// objects: the frame is measured from the left of the paper and the
    /// top of the first line of the paragraph, on the page it starts on.
    Paragraph(NodeId),
}

/// How the text of the body goes around an object (OBJ-17).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Wrap {
    /// On both sides.
    #[default]
    Both,
    /// Only on the side with more room.
    Largest,
    /// Only on its left.
    Left,
    /// Only on its right.
    Right,
    /// Above and below it, and not beside it.
    AboveAndBelow,
    /// Through it, as if it were not there, with the object in front of
    /// the text.
    Through,
    /// Through it, with the object behind the text.
    Behind,
}

/// How an object is filled and outlined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphicStyle {
    /// What fills its inside; nothing when none.
    pub fill: Option<Fill>,
    /// The line around it, or the line itself; none when it has none.
    pub stroke: Option<Stroke>,
    /// How opaque the whole object is, in percent.
    pub opacity: u8,
}

impl Default for GraphicStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            opacity: 100,
        }
    }
}

/// What fills an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Fill {
    /// One color.
    Color(Rgba),
}

/// The line around an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stroke {
    /// The color.
    pub color: Rgba,
    /// The width.
    pub width: Length,
    /// How it is drawn.
    pub style: BorderStyle,
}

/// The objects of a document, from back to front.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Objects(imbl::Vector<Object>);

impl Objects {
    /// How many objects there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is no object.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The objects, from back to front.
    pub fn iter(&self) -> impl Iterator<Item = &Object> {
        self.0.iter()
    }

    /// An object by identifier.
    pub fn get(&self, id: NodeId) -> Option<&Object> {
        self.0.iter().find(|o| o.id == id)
    }

    /// The place of an object from the back, by identifier.
    pub fn index_of(&self, id: NodeId) -> Option<usize> {
        self.0.iter().position(|o| o.id == id)
    }

    /// Every paragraph of every text box.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.0.iter().flat_map(Object::paragraphs)
    }

    pub(crate) fn insert(&mut self, index: usize, object: Object) {
        self.0.insert(index, object);
    }

    pub(crate) fn remove(&mut self, index: usize) -> Object {
        self.0.remove(index)
    }

    pub(crate) fn set(&mut self, index: usize, object: Object) -> Object {
        self.0.set(index, object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Editor, ModelError, ParaAttrs, Run};

    fn shape(id: NodeId) -> Object {
        Object::new(
            id,
            ObjectKind::Shape(Shape::Rectangle),
            Frame::from_pt(10.0, 20.0, 30.0, 40.0),
        )
    }

    #[test]
    fn obj_01_objects_are_added_moved_and_removed_with_undo() {
        let mut editor = Editor::new(Document::new_word_processing());
        let empty = editor.document().clone();
        let (a, b) = editor
            .edit("Shapes", |edit| {
                let (a, b) = (edit.new_id(), edit.new_id());
                edit.add_object(shape(a))?;
                edit.insert_object(0, shape(b))?;
                Ok((a, b))
            })
            .unwrap();
        let order: Vec<NodeId> = editor.document().objects().iter().map(|o| o.id).collect();
        assert_eq!(order, [b, a]);
        editor
            .edit("Move", |edit| {
                let mut moved = shape(a);
                moved.frame.x = Length::from_points(100);
                moved.rotation = 4500;
                edit.replace_object(moved)
            })
            .unwrap();
        let moved = editor.document().objects().get(a).unwrap();
        assert_eq!(
            (moved.frame.x, moved.rotation),
            (Length::from_points(100), 4500)
        );
        editor.edit("Delete", |edit| edit.remove_object(b)).unwrap();
        assert_eq!(editor.document().objects().len(), 1);
        editor.undo().unwrap();
        editor.undo().unwrap();
        assert_eq!(editor.document().objects().get(a), Some(&shape(a)));
        editor.undo().unwrap();
        assert_eq!(editor.document(), &empty);
        assert_eq!(
            editor.edit("Unknown", |edit| edit.remove_object(a)),
            Err(ModelError::UnknownObject(a))
        );
    }

    #[test]
    fn obj_13_text_boxes_keep_their_ids_and_styles_in_use() {
        let mut editor = Editor::new(Document::new_word_processing());
        let body = editor.document().styles().default_paragraph();
        let first = editor.document().start().paragraph;
        let result = editor.edit("Box", |edit| {
            let id = edit.new_id();
            let paragraph =
                Paragraph::from_runs(first, body, ParaAttrs::default(), [Run::new("x")])?;
            let text = TextBox {
                paragraphs: Arc::from([paragraph]),
                inset: Length::from_points(4),
            };
            edit.add_object(Object::new(id, ObjectKind::TextBox(text), Frame::default()))
        });
        // The paragraph of the body already has that identifier.
        assert_eq!(result, Err(ModelError::DuplicateId(first)));
        let object = editor
            .edit("Box", |edit| {
                let (id, p) = (edit.new_id(), edit.new_id());
                let paragraph =
                    Paragraph::from_runs(p, body, ParaAttrs::default(), [Run::new("x")])?;
                let text = TextBox {
                    paragraphs: Arc::from([paragraph]),
                    inset: Length::from_points(4),
                };
                let object = Object::new(id, ObjectKind::TextBox(text), Frame::default());
                edit.add_object(object.clone())?;
                Ok(object)
            })
            .unwrap();
        assert_eq!(object.ids().len(), 2);
        assert_eq!(editor.document().objects().paragraphs().count(), 1);
    }
}
