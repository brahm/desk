// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The objects on the pages and the text around them (OBJ-01, OBJ-13,
//! OBJ-17).
//!
//! The rules, as Pages '09 places floating objects:
//!
//! - An object fixed on a page is where its frame says, from the top left
//!   corner of the paper. One on a page past the last is drawn on the
//!   last page.
//! - An object that moves with a paragraph goes on the page where the
//!   paragraph starts, its top measured from the top of the paragraph's
//!   first line, and moves with it to the next page or column.
//! - The text goes around the box that holds the object, rotated, with
//!   the room of its wrap margin on every side: on both sides, only on
//!   the side with more room, only on its left or its right, or above and
//!   below it. A piece of a line narrower than
//!   [`desk_text::MIN_SEGMENT_WIDTH`] is left empty. An object that the
//!   text goes through takes no room.

use std::collections::HashMap;
use std::sync::Arc;

use desk_model::{Anchor, Document, Object, ObjectKind, Wrap};
use desk_text::{Column, LineSpace, ParagraphText, TextSystem};

use crate::pages::{DocumentLayout, PlacedLine};

/// An object placed on a page.
#[derive(Clone, Debug)]
pub struct PlacedObject {
    /// The object.
    pub object: Object,
    /// The paragraph of the body it moves with, by index, if it does.
    pub paragraph: Option<usize>,
    /// The left edge of its frame before it is rotated, in points from
    /// the left of the paper.
    pub x: f32,
    /// The top edge of its frame before it is rotated, in points from the
    /// top of the paper.
    pub y: f32,
    /// The width of its frame.
    pub width: f32,
    /// The height of its frame.
    pub height: f32,
    /// The lines of a text box, placed on the page as if it were not
    /// rotated. Their paragraphs are indexes in the text box.
    pub lines: Vec<PlacedLine>,
    /// The objects of a group, placed.
    pub children: Vec<PlacedObject>,
}

impl PlacedObject {
    /// An object placed with its frame moved down by `dy`, as its anchor
    /// says, with the objects of a group moved with it.
    pub(crate) fn new(object: &Object, paragraph: Option<usize>, dy: f32) -> Self {
        let frame = object.frame;
        let children = match &object.kind {
            ObjectKind::Group(objects) => objects
                .iter()
                .map(|o| Self::new(o, paragraph, dy))
                .collect(),
            _ => Vec::new(),
        };
        Self {
            object: object.clone(),
            paragraph,
            x: frame.x.to_pt() as f32,
            y: frame.y.to_pt() as f32 + dy,
            width: frame.width.to_pt() as f32,
            height: frame.height.to_pt() as f32,
            lines: Vec::new(),
            children,
        }
    }

    /// The center of the frame, which it rotates around.
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// The angle it is rotated by, in radians, counterclockwise.
    pub fn angle(&self) -> f32 {
        (self.object.rotation as f32 / 100.0).to_radians()
    }

    /// The smallest upright box that holds the object as it is rotated:
    /// left, top, right and bottom.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let (cx, cy) = self.center();
        let (sin, cos) = self.angle().sin_cos();
        let half_w = (self.width * cos).abs() / 2.0 + (self.height * sin).abs() / 2.0;
        let half_h = (self.width * sin).abs() / 2.0 + (self.height * cos).abs() / 2.0;
        (cx - half_w, cy - half_h, cx + half_w, cy + half_h)
    }

    /// A point of the page in the frame's own coordinates: from its top
    /// left corner, as if it were not rotated.
    pub fn to_frame(&self, x: f32, y: f32) -> (f32, f32) {
        let (cx, cy) = self.center();
        let (sin, cos) = self.angle().sin_cos();
        let (dx, dy) = (x - cx, y - cy);
        // The page has y going down, so a counterclockwise rotation by a
        // is undone by turning the point clockwise on the screen.
        let fx = dx * cos - dy * sin;
        let fy = dx * sin + dy * cos;
        (fx + self.width / 2.0, fy + self.height / 2.0)
    }

    /// A point of the frame, from its top left corner as if it were not
    /// rotated, on the page.
    pub fn to_page(&self, fx: f32, fy: f32) -> (f32, f32) {
        let (cx, cy) = self.center();
        let (sin, cos) = self.angle().sin_cos();
        let (dx, dy) = (fx - self.width / 2.0, fy - self.height / 2.0);
        (cx + dx * cos + dy * sin, cy - dx * sin + dy * cos)
    }

    /// Whether a point of the page is on the object, or within
    /// `tolerance` of a line (OBJ-03). A shape is hit anywhere in its
    /// frame, as Pages selects it.
    pub fn contains(&self, x: f32, y: f32, tolerance: f32) -> bool {
        let (fx, fy) = self.to_frame(x, y);
        if let ObjectKind::Shape(desk_model::Shape::Line { rising }) = &self.object.kind {
            let (y0, y1) = if *rising {
                (self.height, 0.0)
            } else {
                (0.0, self.height)
            };
            return distance_to_segment((fx, fy), (0.0, y0), (self.width, y1)) <= tolerance;
        }
        fx >= -tolerance
            && fy >= -tolerance
            && fx <= self.width + tolerance
            && fy <= self.height + tolerance
    }

    /// The room the object takes from the text, if it takes any.
    pub(crate) fn exclusion(&self) -> Option<Exclusion> {
        if matches!(self.object.wrap, Wrap::Through | Wrap::Behind) {
            return None;
        }
        let margin = self.object.wrap_margin.to_pt() as f32;
        let (x0, y0, x1, y1) = self.bounds();
        Some(Exclusion {
            x0: x0 - margin,
            y0: y0 - margin,
            x1: x1 + margin,
            y1: y1 + margin,
            wrap: self.object.wrap,
        })
    }
}

/// A floating object of a document, with what it moves with.
#[derive(Clone, Debug)]
pub(crate) struct Floating {
    pub object: Object,
    pub anchor: Place,
}

/// Where a floating object goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// On a page, by index.
    Page(usize),
    /// With a paragraph of the text laid out, by index.
    Paragraph(usize),
}

/// The floating objects of a document. An object whose paragraph is gone
/// stays on the first page.
pub(crate) fn floating(document: &Document) -> Arc<Vec<Floating>> {
    let objects = document.objects();
    if objects.is_empty() {
        return Arc::default();
    }
    let indexes: HashMap<desk_model::NodeId, usize> = document
        .body()
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id(), i))
        .collect();
    Arc::new(
        objects
            .iter()
            .map(|object| Floating {
                object: object.clone(),
                anchor: match object.anchor {
                    Anchor::Page(page) => Place::Page(page as usize),
                    Anchor::Paragraph(id) => indexes
                        .get(&id)
                        .map_or(Place::Page(0), |&i| Place::Paragraph(i)),
                },
            })
            .collect(),
    )
}

/// The room that an object takes from the text, in points on the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Exclusion {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub wrap: Wrap,
}

/// The part of a paragraph on one page or column: from a height of the
/// paragraph down, where the paragraph is on the page and what takes room
/// there.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Piece {
    /// The height in the paragraph where the part starts.
    pub from: f32,
    /// The left of the column on the page.
    pub left: f32,
    /// Where the top of the paragraph would be on the page.
    pub origin: f32,
    /// The objects in the way.
    pub exclusions: Vec<Exclusion>,
}

/// A column with objects in it, part by part of a paragraph.
pub(crate) struct WrapSpace<'a> {
    pub width: f32,
    pub pieces: &'a [Piece],
}

impl LineSpace for WrapSpace<'_> {
    fn width(&self) -> f32 {
        self.width
    }

    fn segments(&self, top: f32, height: f32) -> Vec<(f32, f32)> {
        let Some(piece) = self
            .pieces
            .iter()
            .rev()
            .find(|p| p.from <= top + 0.001)
            .or(self.pieces.first())
        else {
            return vec![(0.0, self.width)];
        };
        free_segments(piece, self.width, top, height)
    }
}

/// The pieces of a column of a width that the objects of a part of a
/// paragraph leave free, for a line from `top` that is `height` tall.
fn free_segments(piece: &Piece, width: f32, top: f32, height: f32) -> Vec<(f32, f32)> {
    let (y0, y1) = (piece.origin + top, piece.origin + top + height);
    let mut free = vec![(0.0f32, width)];
    for e in &piece.exclusions {
        if e.y1 <= y0 || e.y0 >= y1 {
            continue;
        }
        let (a, b) = (e.x0 - piece.left, e.x1 - piece.left);
        if b <= 0.0 || a >= width {
            continue;
        }
        let (cut0, cut1) = match e.wrap {
            Wrap::Both => (a, b),
            // The text only on the left of the object.
            Wrap::Left => (a, f32::INFINITY),
            Wrap::Right => (f32::NEG_INFINITY, b),
            Wrap::Largest => {
                if a >= width - b {
                    (a, f32::INFINITY)
                } else {
                    (f32::NEG_INFINITY, b)
                }
            }
            Wrap::AboveAndBelow => (f32::NEG_INFINITY, f32::INFINITY),
            Wrap::Through | Wrap::Behind => continue,
        };
        free = free
            .into_iter()
            .flat_map(|(s, t)| {
                let mut out = Vec::with_capacity(2);
                if cut0 > s {
                    out.push((s, cut0.min(t)));
                }
                if cut1 < t {
                    out.push((cut1.max(s), t));
                }
                out
            })
            .filter(|(s, t)| t > s)
            .collect();
    }
    free.into_iter().map(|(s, t)| (s, t - s)).collect()
}

/// Whether an exclusion is in the way of a column from `left` to `right`
/// below `top` on its page.
pub(crate) fn in_the_way(e: &Exclusion, left: f32, right: f32, top: f32) -> bool {
    e.y1 > top && e.x1 > left && e.x0 < right
}

/// Lays out the text of the text boxes on the pages of a layout (OBJ-13):
/// inside the frame less its inset, from the top, as a column without a
/// bottom.
pub fn place_object_text(
    document: &Document,
    layout: &mut DocumentLayout,
    system: &mut TextSystem,
) {
    for page in &mut layout.pages {
        for placed in &mut page.objects {
            lay_out_text(document, placed, system);
        }
    }
}

fn lay_out_text(document: &Document, placed: &mut PlacedObject, system: &mut TextSystem) {
    for child in &mut placed.children {
        lay_out_text(document, child, system);
    }
    let ObjectKind::TextBox(text) = &placed.object.kind else {
        return;
    };
    let inset = text.inset.to_pt() as f32;
    let width = (placed.width - 2.0 * inset).max(1.0);
    let styles = document.styles();
    let settings = document.text_settings();
    let items = styles.list_items(text.paragraphs.iter());
    let mut lines = Vec::new();
    let mut y = placed.y + inset;
    let mut after = 0.0f32;
    for (i, paragraph) in text.paragraphs.iter().enumerate() {
        let text = ParagraphText::new_in_document_with(
            styles,
            settings,
            paragraph,
            items[i].as_ref(),
            Default::default(),
        );
        let format = text.paragraph();
        if i > 0 {
            y += after.max(format.space_before.to_pt() as f32);
        }
        after = format.space_after.to_pt() as f32;
        let laid = system.layout(&text, &Column::new(width), 0.0).lines;
        let mut bottom = y;
        for (n, line) in laid.into_iter().enumerate() {
            bottom = y + line.top + line.height;
            lines.push(PlacedLine {
                paragraph: i,
                index: n,
                line,
                origin: (placed.x + inset, y),
            });
        }
        y = bottom;
    }
    placed.lines = lines;
}

/// How far a point is from a segment.
fn distance_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length).clamp(0.0, 1.0)
    };
    let (x, y) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - x).powi(2) + (p.1 - y).powi(2)).sqrt()
}

/// The frontmost object on a page under a point, by its index among the
/// objects of the page, if any is (OBJ-03). Objects behind the text are
/// found only when `behind` is set, as Pages selects them only when
/// nothing else is under the pointer.
pub fn object_at(
    objects: &[PlacedObject],
    x: f32,
    y: f32,
    tolerance: f32,
    behind: bool,
) -> Option<usize> {
    objects
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, o)| behind || o.object.wrap != Wrap::Behind)
        .find(|(_, o)| o.contains(x, y, tolerance))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(wrap: Wrap) -> Piece {
        Piece {
            from: f32::NEG_INFINITY,
            left: 100.0,
            origin: 50.0,
            exclusions: vec![Exclusion {
                x0: 200.0,
                y0: 100.0,
                x1: 300.0,
                y1: 200.0,
                wrap,
            }],
        }
    }

    #[test]
    fn obj_17_wraps_leave_the_sides_they_say() {
        let width = 400.0;
        // Above the object, the whole column.
        assert_eq!(
            free_segments(&piece(Wrap::Both), width, 0.0, 20.0),
            vec![(0.0, 400.0)]
        );
        // Beside it, both sides, one side or none.
        assert_eq!(
            free_segments(&piece(Wrap::Both), width, 60.0, 20.0),
            vec![(0.0, 100.0), (200.0, 200.0)]
        );
        assert_eq!(
            free_segments(&piece(Wrap::Left), width, 60.0, 20.0),
            vec![(0.0, 100.0)]
        );
        assert_eq!(
            free_segments(&piece(Wrap::Right), width, 60.0, 20.0),
            vec![(200.0, 200.0)]
        );
        assert_eq!(
            free_segments(&piece(Wrap::Largest), width, 60.0, 20.0),
            vec![(200.0, 200.0)]
        );
        assert!(free_segments(&piece(Wrap::AboveAndBelow), width, 60.0, 20.0).is_empty());
        assert_eq!(
            free_segments(&piece(Wrap::Through), width, 60.0, 20.0),
            vec![(0.0, 400.0)]
        );
    }

    #[test]
    fn obj_17_rotated_objects_take_their_bounding_box() {
        let mut object = Object::new(
            desk_model::NodeId::new(1).unwrap(),
            ObjectKind::Shape(desk_model::Shape::Rectangle),
            desk_model::Frame::from_pt(100.0, 100.0, 100.0, 50.0),
        );
        object.rotation = 9000;
        let placed = PlacedObject::new(&object, None, 0.0);
        let (x0, y0, x1, y1) = placed.bounds();
        assert!((x0 - 125.0).abs() < 0.01 && (x1 - 175.0).abs() < 0.01);
        assert!((y0 - 75.0).abs() < 0.01 && (y1 - 175.0).abs() < 0.01);
    }

    #[test]
    fn obj_03_objects_are_hit_where_they_are_drawn() {
        use desk_model::{Frame, NodeId, Shape};
        let shape = |id: u64, shape: Shape, rotation: i32| {
            let mut object = Object::new(
                NodeId::new(id).unwrap(),
                ObjectKind::Shape(shape),
                Frame::from_pt(100.0, 100.0, 100.0, 20.0),
            );
            object.rotation = rotation;
            PlacedObject::new(&object, None, 0.0)
        };
        let flat = shape(1, Shape::Rectangle, 0);
        assert!(flat.contains(190.0, 115.0, 0.0));
        assert!(!flat.contains(150.0, 150.0, 0.0));
        // Turned a quarter, it stands from 60 to 160 down the page.
        let standing = shape(2, Shape::Rectangle, 9000);
        assert!(standing.contains(150.0, 150.0, 0.0));
        assert!(!standing.contains(190.0, 115.0, 0.0));
        let (fx, fy) = standing.to_frame(150.0, 65.0);
        let (px, py) = standing.to_page(fx, fy);
        assert!((px - 150.0).abs() < 1e-3 && (py - 65.0).abs() < 1e-3);
        // Counterclockwise: the right end of the frame goes to the top.
        assert!(fx > 90.0 && (fy - 10.0).abs() < 1e-3);
        // A line is hit near it, not anywhere in its frame.
        let line = shape(3, Shape::Line { rising: false }, 0);
        assert!(line.contains(150.0, 111.0, 2.0));
        assert!(!line.contains(110.0, 118.0, 2.0));
        let objects = [flat, line];
        assert_eq!(object_at(&objects, 150.0, 110.0, 2.0, false), Some(1));
        assert_eq!(object_at(&objects, 110.0, 118.0, 2.0, false), Some(0));
        assert_eq!(object_at(&objects, 10.0, 10.0, 2.0, false), None);
    }
}
