// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Combining shapes, as Format > Shapes and Lines of Pages 15.4 does
//! (W113, PXV-27): Unite, Intersect, Subtract and Exclude make one shape
//! of the selected ones, and Break Apart makes a shape of several parts
//! into one shape for each.

use std::sync::Arc;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

use crate::{
    Anchor, Edit, Frame, Length, ModelError, NodeId, Object, ObjectKind, POLYGON_SCALE, PathPoint,
    PathSegment, Shape,
};

/// How shapes are combined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Combine {
    /// All that any of them covers.
    Unite,
    /// What all of them cover.
    Intersect,
    /// The shape at the back, without what those in front of it cover.
    Subtract,
    /// What an odd number of them cover: where two overlap is a hole.
    Exclude,
}

/// The most parts of a shape: those of a combination, and those read
/// from a file.
pub const MAX_SHAPE_PARTS: usize = 4096;

/// How many straight lines stand for a curve of an outline.
const CURVE_STEPS: usize = 16;

/// Closed outlines, each a list of points.
type Contours = Vec<Vec<[f64; 2]>>;

/// The outline of a shape as closed lists of points on the page, as it is
/// placed, flipped and rotated.
fn page_contours(object: &Object, shape: &Shape) -> Contours {
    let frame = object.frame;
    let (x, y) = (frame.x.to_pt(), frame.y.to_pt());
    let (w, h) = (frame.width.to_pt(), frame.height.to_pt());
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let angle = -f64::from(object.rotation) / 100.0 * std::f64::consts::PI / 180.0;
    let (sin, cos) = angle.sin_cos();
    let sx = if object.flip_horizontal { -1.0 } else { 1.0 };
    let sy = if object.flip_vertical { -1.0 } else { 1.0 };
    let place = |(px, py): (f64, f64)| {
        let (dx, dy) = ((x + px - cx) * sx, (y + py - cy) * sy);
        [cx + cos * dx - sin * dy, cy + sin * dx + cos * dy]
    };
    let mut contours: Contours = Vec::new();
    let mut current: Vec<[f64; 2]> = Vec::new();
    let mut at = (0.0, 0.0);
    for segment in shape.outline(w, h) {
        match segment {
            PathSegment::Move(px, py) => {
                if current.len() > 2 {
                    contours.push(std::mem::take(&mut current));
                }
                current.clear();
                at = (px, py);
                current.push(place(at));
            }
            PathSegment::Line(px, py) => {
                at = (px, py);
                current.push(place(at));
            }
            PathSegment::Curve(ax, ay, bx, by, px, py) => {
                for step in 1..=CURVE_STEPS {
                    let t = step as f64 / CURVE_STEPS as f64;
                    let u = 1.0 - t;
                    let b = |p0: f64, p1: f64, p2: f64, p3: f64| {
                        u * u * u * p0
                            + 3.0 * u * u * t * p1
                            + 3.0 * u * t * t * p2
                            + t * t * t * p3
                    };
                    current.push(place((b(at.0, ax, bx, px), b(at.1, ay, by, py))));
                }
                at = (px, py);
            }
            PathSegment::Close => {
                if current.len() > 2 {
                    contours.push(std::mem::take(&mut current));
                }
                current.clear();
            }
        }
    }
    if current.len() > 2 {
        contours.push(current);
    }
    contours
}

/// The shape that outlines on the page make, in a frame around them, or
/// none when they cover nothing.
fn shape_of(contours: &[Vec<[f64; 2]>]) -> Option<(Frame, Shape)> {
    let points = contours.iter().flatten();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for [x, y] in points.copied() {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    let (w, h) = (x1 - x0, y1 - y0);
    if !(w > 0.01 && h > 0.01) {
        return None;
    }
    let scale = f64::from(POLYGON_SCALE);
    let parts: Vec<Arc<[PathPoint]>> = contours
        .iter()
        .filter(|c| c.len() > 2)
        .map(|contour| {
            contour
                .iter()
                .map(|[x, y]| {
                    PathPoint::corner(
                        ((x - x0) / w * scale).round() as i32,
                        ((y - y0) / h * scale).round() as i32,
                    )
                })
                .collect()
        })
        .collect();
    let frame = Frame {
        x: Length::from_pt(x0),
        y: Length::from_pt(y0),
        width: Length::from_pt(w),
        height: Length::from_pt(h),
    };
    let shape = match &parts[..] {
        [] => return None,
        // As many as ODF keeps.
        _ if parts.len() > MAX_SHAPE_PARTS => return None,
        // One part is a shape drawn with the pen, whose points edit.
        [one] => Shape::Path {
            points: one.clone(),
            closed: true,
        },
        _ => Shape::Compound(parts.into()),
    };
    Some((frame, shape))
}

/// Whether an object is a shape that combines: one with an inside.
fn combinable(object: &Object) -> Option<&Shape> {
    match &object.kind {
        // An open path has no inside either.
        ObjectKind::Shape(Shape::Path { closed: false, .. }) => None,
        ObjectKind::Shape(shape) if !shape.is_line() => Some(shape),
        _ => None,
    }
}

impl Edit {
    /// Combines the shapes `ids` into one, which takes the place and the
    /// look of the one at the back, and returns it; none when fewer than
    /// two of them are unlocked shapes on the same anchor, or they make
    /// nothing, as shapes that do not meet intersected (W113).
    pub fn combine_shapes(
        &mut self,
        ids: &[NodeId],
        how: Combine,
    ) -> Result<Option<NodeId>, ModelError> {
        let objects = self.document().objects();
        // From back to front.
        let mut members: Vec<(usize, Object)> = ids
            .iter()
            .filter_map(|id| Some((objects.index_of(*id)?, objects.get(*id)?.clone())))
            .filter(|(_, o)| !o.locked && combinable(o).is_some())
            .collect();
        members.sort_by_key(|(at, _)| *at);
        members.dedup_by_key(|(at, _)| *at);
        let Some((back_at, back)) = members.first().cloned() else {
            return Ok(None);
        };
        if members.len() < 2
            || back.anchor == Anchor::InText
            || members.iter().any(|(_, o)| o.anchor != back.anchor)
        {
            return Ok(None);
        }
        let contours: Vec<Contours> = members
            .iter()
            .filter_map(|(_, o)| Some(page_contours(o, combinable(o)?)))
            .collect();
        let Some((first, rest)) = contours.split_first() else {
            return Ok(None);
        };
        let mut result: Contours = first.clone();
        match how {
            Combine::Intersect => {
                for other in rest {
                    result = flat(result.overlay(other, OverlayRule::Intersect, FillRule::NonZero));
                }
            }
            Combine::Exclude => {
                // Where an even number of them overlap is a hole.
                for other in rest {
                    result = flat(result.overlay(other, OverlayRule::Xor, FillRule::NonZero));
                }
            }
            Combine::Unite => {
                for other in rest {
                    result = flat(result.overlay(other, OverlayRule::Union, FillRule::NonZero));
                }
            }
            Combine::Subtract => {
                // Those in front first make one, each on its own rule, so
                // that one flipped, which goes the other way around, still
                // covers what it covers; then it is taken out.
                let mut front: Contours = Contours::new();
                for other in rest {
                    front = flat(front.overlay(other, OverlayRule::Union, FillRule::NonZero));
                }
                result = flat(result.overlay(&front, OverlayRule::Difference, FillRule::NonZero));
            }
        }
        let Some((frame, shape)) = shape_of(&result) else {
            return Ok(None);
        };
        for (_, object) in &members {
            self.remove_object(object.id)?;
        }
        let id = self.new_id();
        let mut combined = Object::new(id, ObjectKind::Shape(shape), frame);
        combined.anchor = back.anchor;
        combined.style = back.style.clone();
        combined.object_style = back.object_style;
        combined.wrap = back.wrap;
        combined.wrap_margin = back.wrap_margin;
        combined.wrap_threshold = back.wrap_threshold;
        // The text, link and captions of the one at the back stay; those of
        // the others go with them, as in Pages.
        combined.text = back.text.clone();
        combined.link = back.link.clone();
        combined.captions = back.captions.clone();
        let index = back_at.min(self.document().objects().len());
        self.insert_object(index, combined)?;
        Ok(Some(id))
    }

    /// Makes each shape of several parts among `ids` into one shape for
    /// each part, in its place, and returns them (W113).
    pub fn break_apart_shapes(&mut self, ids: &[NodeId]) -> Result<Vec<NodeId>, ModelError> {
        let mut made = Vec::new();
        for &id in ids {
            let objects = self.document().objects();
            let (Some(index), Some(object)) = (objects.index_of(id), objects.get(id).cloned())
            else {
                continue;
            };
            let ObjectKind::Shape(Shape::Compound(parts)) = &object.kind else {
                continue;
            };
            if object.locked || object.anchor == Anchor::InText || parts.len() < 2 {
                continue;
            }
            let mut pieces = Vec::new();
            for part in parts.iter() {
                let single = Shape::Path {
                    points: part.clone(),
                    closed: true,
                };
                let contours = page_contours(&object, &single);
                if let Some((frame, piece)) = shape_of(&contours) {
                    pieces.push((frame, piece));
                }
            }
            // A shape none of whose parts makes one stays as it is.
            if pieces.is_empty() {
                continue;
            }
            self.remove_object(id)?;
            for (i, (frame, piece)) in pieces.into_iter().enumerate() {
                let new = self.new_id();
                let mut part = Object::new(new, ObjectKind::Shape(piece), frame);
                part.anchor = object.anchor;
                part.style = object.style.clone();
                part.object_style = object.object_style;
                part.wrap = object.wrap;
                part.wrap_margin = object.wrap_margin;
                part.wrap_threshold = object.wrap_threshold;
                // The first part keeps the text, link and captions.
                if i == 0 {
                    part.text = object.text.clone();
                    part.link = object.link.clone();
                    part.captions = object.captions.clone();
                }
                self.insert_object(index + i, part)?;
                made.push(new);
            }
        }
        Ok(made)
    }
}

/// The contours of shapes, all in one list.
fn flat(shapes: Vec<Vec<Vec<[f64; 2]>>>) -> Contours {
    shapes.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Editor};

    fn square(edit: &mut Edit, x: f64, y: f64, side: f64) -> Result<NodeId, ModelError> {
        let id = edit.new_id();
        let mut object = Object::new(
            id,
            ObjectKind::Shape(Shape::Rectangle),
            Frame::from_pt(x, y, side, side),
        );
        object.anchor = Anchor::Page(0);
        edit.add_object(object)?;
        Ok(id)
    }

    /// The area a shape covers, by the nonzero rule, on a grid.
    fn area(object: &Object) -> f64 {
        let ObjectKind::Shape(shape) = &object.kind else {
            return 0.0;
        };
        let contours = page_contours(object, shape);
        let f = object.frame;
        let mut covered = 0;
        let (x0, y0) = (f.x.to_pt(), f.y.to_pt());
        let (w, h) = (f.width.to_pt(), f.height.to_pt());
        for i in 0..100 {
            for j in 0..100 {
                let (px, py) = (
                    x0 + (i as f64 + 0.5) * w / 100.0,
                    y0 + (j as f64 + 0.5) * h / 100.0,
                );
                let mut winding = 0;
                for c in &contours {
                    for k in 0..c.len() {
                        let [ax, ay] = c[k];
                        let [bx, by] = c[(k + 1) % c.len()];
                        if (ay <= py) != (by <= py) {
                            let cross = ax + (py - ay) / (by - ay) * (bx - ax);
                            if cross > px {
                                winding += if by > ay { 1 } else { -1 };
                            }
                        }
                    }
                }
                if winding != 0 {
                    covered += 1;
                }
            }
        }
        f64::from(covered) / 10_000.0 * w * h
    }

    fn combined(how: Combine) -> (Editor, Object) {
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Combine", |edit| {
                let a = square(edit, 0.0, 0.0, 100.0)?;
                let b = square(edit, 50.0, 50.0, 100.0)?;
                edit.combine_shapes(&[b, a], how)
            })
            .unwrap()
            .unwrap();
        let object = editor.document().objects().get(id).cloned().unwrap();
        (editor, object)
    }

    #[test]
    fn w113_shapes_unite_intersect_subtract_and_exclude() {
        let close = |a: f64, b: f64| (a - b).abs() < 150.0;
        let (editor, united) = combined(Combine::Unite);
        assert_eq!(editor.document().objects().len(), 1);
        assert_eq!(united.frame, Frame::from_pt(0.0, 0.0, 150.0, 150.0));
        assert!(close(area(&united), 17_500.0), "{}", area(&united));
        let (_, both) = combined(Combine::Intersect);
        assert_eq!(both.frame, Frame::from_pt(50.0, 50.0, 50.0, 50.0));
        assert!(close(area(&both), 2_500.0));
        let (_, rest) = combined(Combine::Subtract);
        // The back square without the front one.
        assert_eq!(rest.frame, Frame::from_pt(0.0, 0.0, 100.0, 100.0));
        assert!(close(area(&rest), 7_500.0), "{}", area(&rest));
        let (_, either) = combined(Combine::Exclude);
        assert!(matches!(&either.kind, ObjectKind::Shape(Shape::Compound(p)) if p.len() == 2));
        assert!(close(area(&either), 15_000.0), "{}", area(&either));
    }

    #[test]
    fn w113_a_hole_is_kept_and_parts_break_apart() {
        let mut editor = Editor::new(Document::new_word_processing());
        let squares = editor
            .edit("Squares", |edit| {
                Ok([
                    square(edit, 0.0, 0.0, 100.0)?,
                    square(edit, 25.0, 25.0, 50.0)?,
                ])
            })
            .unwrap();
        let id = editor
            .edit("Subtract", |edit| {
                edit.combine_shapes(&squares, Combine::Subtract)
            })
            .unwrap()
            .unwrap();
        let ring = editor.document().objects().get(id).cloned().unwrap();
        assert!(matches!(&ring.kind, ObjectKind::Shape(Shape::Compound(p)) if p.len() == 2));
        assert!((area(&ring) - 7_500.0).abs() < 150.0, "{}", area(&ring));
        let parts = editor
            .edit("Break Apart", |edit| edit.break_apart_shapes(&[id]))
            .unwrap();
        assert_eq!(parts.len(), 2);
        let frames: Vec<Frame> = parts
            .iter()
            .map(|id| editor.document().objects().get(*id).unwrap().frame)
            .collect();
        assert!(frames.contains(&Frame::from_pt(25.0, 25.0, 50.0, 50.0)));
        assert!(frames.contains(&Frame::from_pt(0.0, 0.0, 100.0, 100.0)));
        // Undone, the ring is back, and undone again, the two squares.
        editor.undo().unwrap();
        assert!(editor.document().objects().get(id).is_some());
        editor.undo().unwrap();
        assert_eq!(editor.document().objects().len(), 2);
    }

    #[test]
    fn w113_a_flipped_shape_in_front_still_covers_what_it_covers() {
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Unite", |edit| {
                let back = square(edit, 0.0, 0.0, 100.0)?;
                let a = square(edit, 150.0, 0.0, 100.0)?;
                let b = square(edit, 200.0, 0.0, 100.0)?;
                // A pen shape that goes the other way around.
                let mut flipped = edit.document().objects().get(b).cloned().unwrap();
                flipped.kind = ObjectKind::Shape(Shape::Path {
                    points: [(0, 0), (0, 10_000), (10_000, 10_000), (10_000, 0)]
                        .map(|(x, y)| PathPoint::corner(x, y))
                        .as_slice()
                        .into(),
                    closed: true,
                });
                flipped.flip_horizontal = true;
                edit.replace_object(flipped)?;
                edit.combine_shapes(&[back, a, b], Combine::Unite)
            })
            .unwrap()
            .unwrap();
        let united = editor.document().objects().get(id).unwrap();
        // No hole where the two in front overlap.
        assert!((area(united) - 25_000.0).abs() < 400.0, "{}", area(united));
    }

    #[test]
    fn w113_open_paths_and_text_are_kept_apart() {
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Unite", |edit| {
                let back = square(edit, 0.0, 0.0, 100.0)?;
                let front = square(edit, 50.0, 0.0, 100.0)?;
                let mut labeled = edit.document().objects().get(back).cloned().unwrap();
                labeled.link = Some(Arc::from("https://example.com"));
                edit.replace_object(labeled)?;
                let open = edit.new_id();
                let mut stroke = Object::new(
                    open,
                    ObjectKind::Shape(Shape::Path {
                        points: [PathPoint::corner(0, 0), PathPoint::corner(10_000, 10_000)]
                            .as_slice()
                            .into(),
                        closed: false,
                    }),
                    Frame::from_pt(0.0, 200.0, 50.0, 50.0),
                );
                stroke.anchor = Anchor::Page(0);
                edit.add_object(stroke)?;
                edit.combine_shapes(&[back, front, open], Combine::Unite)
            })
            .unwrap()
            .unwrap();
        let document = editor.document();
        // The open stroke stays; the link of the one at the back too.
        assert_eq!(document.objects().len(), 2);
        let united = document.objects().get(id).unwrap();
        assert_eq!(united.link.as_deref(), Some("https://example.com"));
        assert_eq!(united.frame, Frame::from_pt(0.0, 0.0, 150.0, 100.0));
    }

    #[test]
    fn w113_a_rotated_shape_combines_as_it_is_placed() {
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Unite", |edit| {
                let a = square(edit, 0.0, 0.0, 100.0)?;
                let b = square(edit, 0.0, 0.0, 100.0)?;
                edit.rotate_objects(&[b], 4_500)?;
                edit.combine_shapes(&[a, b], Combine::Unite)
            })
            .unwrap()
            .unwrap();
        let star = editor.document().objects().get(id).unwrap();
        // The corners of the turned square reach out by half the diagonal.
        let reach = 50.0 * std::f64::consts::SQRT_2;
        assert!((star.frame.width.to_pt() - 2.0 * reach).abs() < 0.5);
        assert!((star.frame.x.to_pt() - (50.0 - reach)).abs() < 0.5);
    }
}
