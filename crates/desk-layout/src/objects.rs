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
}
