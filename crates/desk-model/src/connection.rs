// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Connection lines, which join two objects and follow them when they
//! move, as those of Pages do (OBJ-31).
//!
//! A connection line starts and ends on the outline of the objects it
//! joins, on the way from the middle of each to the middle of the line.
//! It is straight, or bent by pulling its middle aside. Each edit moves
//! the lines to the objects they join; one whose object is gone, grouped
//! or put elsewhere lets go of it and stays where it was.

use crate::error::ModelError;
use crate::id::NodeId;
use crate::object::{Anchor, Frame, Object, ObjectKind, POLYGON_SCALE, Shape};
use crate::shape_path::PathSegment;
use crate::transaction::Edit;
use crate::units::Length;

/// A line that joins two objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Connection {
    /// The object it starts at, if it is joined to one.
    pub start: Option<NodeId>,
    /// The object it ends at.
    pub end: Option<NodeId>,
    /// Where it starts, in parts of [`POLYGON_SCALE`] of its frame from
    /// the top left corner.
    pub from: (i32, i32),
    /// Where it ends.
    pub to: (i32, i32),
    /// How far its middle is pulled aside, to the left of the way from
    /// its start to its end, or to the right when below zero.
    pub bend: Length,
}

/// A point, in points.
type Point = (f64, f64);

/// How far apart two placements of an end can be and still be taken as
/// the same, in points, so that an edit leaves a line that has not moved
/// as it is.
const SAME: f64 = 0.01;

impl Connection {
    /// Where it starts and ends, in points from the top left corner of a
    /// frame of `w` by `h` points.
    pub fn ends_in(&self, w: f64, h: f64) -> (Point, Point) {
        let scale = f64::from(POLYGON_SCALE);
        let at = |(x, y): (i32, i32)| (f64::from(x) / scale * w, f64::from(y) / scale * h);
        (at(self.from), at(self.to))
    }

    /// Where it starts and ends on the page of its frame.
    pub fn ends_on(&self, frame: &Frame) -> (Point, Point) {
        let (a, b) = self.ends_in(frame.width.to_pt(), frame.height.to_pt());
        let (x, y) = (frame.x.to_pt(), frame.y.to_pt());
        ((x + a.0, y + a.1), (x + b.0, y + b.1))
    }

    /// The line between two points, bent by `bend`, joined to `start` and
    /// `end`, with the frame that holds it.
    pub fn between(
        from: Point,
        to: Point,
        bend: Length,
        start: Option<NodeId>,
        end: Option<NodeId>,
    ) -> (Frame, Self) {
        let (x0, y0, x1, y1) = bounds(from, to, bend.to_pt());
        // A straight line along an edge still has a frame.
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let scale = f64::from(POLYGON_SCALE);
        let part = |(x, y): Point| {
            (
                ((x - x0) / w * scale).round() as i32,
                ((y - y0) / h * scale).round() as i32,
            )
        };
        let frame = Frame::from_pt(x0, y0, w, h);
        (
            frame,
            Self {
                start,
                end,
                from: part(from),
                to: part(to),
                bend,
            },
        )
    }
}

/// The control point of the curve of a quadratic Bézier curve from
/// `from` to `to` whose middle is pulled `bend` points to the left.
fn control(from: Point, to: Point, bend: f64) -> Point {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    let (mx, my) = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    if length == 0.0 {
        return (mx, my);
    }
    // To the left of the way, on a page whose heights go down; twice as
    // far as the middle of the curve.
    let (nx, ny) = (dy / length, -dx / length);
    (mx + 2.0 * bend * nx, my + 2.0 * bend * ny)
}

/// The middle of a line from `from` to `to` bent by `bend` points.
pub fn bent_middle(from: Point, to: Point, bend: f64) -> Point {
    let q = control(from, to, bend);
    (
        (from.0 + 2.0 * q.0 + to.0) / 4.0,
        (from.1 + 2.0 * q.1 + to.1) / 4.0,
    )
}

/// How far the middle of a line from `from` to `to` is from `point`
/// aside, to the left, as the bend that puts it there.
pub fn bend_through(from: Point, to: Point, point: Point) -> f64 {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    if length == 0.0 {
        return 0.0;
    }
    let (nx, ny) = (dy / length, -dx / length);
    (point.0 - (from.0 + to.0) / 2.0) * nx + (point.1 - (from.1 + to.1) / 2.0) * ny
}

/// The two control points of the cubic Bézier curve of a bent line, or
/// none for a straight one.
pub fn bent_controls(from: Point, to: Point, bend: f64) -> Option<(Point, Point)> {
    if bend == 0.0 {
        return None;
    }
    let q = control(from, to, bend);
    let third = |a: Point| (a.0 + 2.0 / 3.0 * (q.0 - a.0), a.1 + 2.0 / 3.0 * (q.1 - a.1));
    Some((third(from), third(to)))
}

/// The box of a line from `from` to `to` bent by `bend` points.
fn bounds(from: Point, to: Point, bend: f64) -> (f64, f64, f64, f64) {
    let q = control(from, to, bend);
    let mut points = vec![from, to];
    // Where the curve turns back, across and down.
    let turn = |a: f64, q: f64, b: f64| {
        let d = a - 2.0 * q + b;
        (d != 0.0)
            .then(|| (a - q) / d)
            .filter(|t| *t > 0.0 && *t < 1.0)
    };
    let at = |t: f64| {
        let u = 1.0 - t;
        (
            u * u * from.0 + 2.0 * u * t * q.0 + t * t * to.0,
            u * u * from.1 + 2.0 * u * t * q.1 + t * t * to.1,
        )
    };
    points.extend(turn(from.0, q.0, to.0).map(at));
    points.extend(turn(from.1, q.1, to.1).map(at));
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in points {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    (x0, y0, x1, y1)
}

/// The outline of a connection line in a frame of `w` by `h` points.
pub(crate) fn outline(connection: &Connection, w: f64, h: f64) -> Vec<PathSegment> {
    let (a, b) = connection.ends_in(w, h);
    let mut path = vec![PathSegment::Move(a.0, a.1)];
    match bent_controls(a, b, connection.bend.to_pt()) {
        Some((c1, c2)) => path.push(PathSegment::Curve(c1.0, c1.1, c2.0, c2.1, b.0, b.1)),
        None => path.push(PathSegment::Line(b.0, b.1)),
    }
    path
}

/// The middle of an object's frame, on its page.
fn center(object: &Object) -> Point {
    let f = &object.frame;
    (
        f.x.to_pt() + f.width.to_pt() / 2.0,
        f.y.to_pt() + f.height.to_pt() / 2.0,
    )
}

/// The segments of an outline, with its curves as short lines.
pub fn flattened(path: &[PathSegment]) -> Vec<(Point, Point)> {
    const STEPS: usize = 12;
    let mut out = Vec::new();
    let (mut at, mut first) = ((0.0, 0.0), (0.0, 0.0));
    for segment in path {
        match *segment {
            PathSegment::Move(x, y) => {
                at = (x, y);
                first = at;
            }
            PathSegment::Line(x, y) => {
                out.push((at, (x, y)));
                at = (x, y);
            }
            PathSegment::Curve(ax, ay, bx, by, x, y) => {
                let from = at;
                for i in 1..=STEPS {
                    let t = i as f64 / STEPS as f64;
                    let u = 1.0 - t;
                    let p = (
                        u * u * u * from.0
                            + 3.0 * u * u * t * ax
                            + 3.0 * u * t * t * bx
                            + t * t * t * x,
                        u * u * u * from.1
                            + 3.0 * u * u * t * ay
                            + 3.0 * u * t * t * by
                            + t * t * t * y,
                    );
                    out.push((at, p));
                    at = p;
                }
            }
            PathSegment::Close => {
                out.push((at, first));
                at = first;
            }
        }
    }
    out
}

/// How far along a ray from `origin` the way `dir`, of length one, meets
/// a segment, if it does.
fn meets(origin: Point, dir: Point, (p, q): (Point, Point)) -> Option<f64> {
    let (ex, ey) = (q.0 - p.0, q.1 - p.1);
    let cross = dir.0 * ey - dir.1 * ex;
    if cross.abs() < 1e-12 {
        return None;
    }
    let (wx, wy) = (p.0 - origin.0, p.1 - origin.1);
    let t = (wx * ey - wy * ex) / cross;
    let s = (wx * dir.1 - wy * dir.0) / cross;
    (t >= 0.0 && (-1e-9..=1.0 + 1e-9).contains(&s)).then_some(t)
}

/// Where the way from the middle of an object toward a point leaves its
/// outline, as it is rotated and flipped: the farthest it meets, so that
/// the line does not cross it.
fn edge_toward(object: &Object, toward: Point) -> Point {
    let (cx, cy) = center(object);
    let (px, py) = (toward.0 - cx, toward.1 - cy);
    let length = px.hypot(py);
    if length < 1e-9 {
        return (cx, cy);
    }
    let (w, h) = (object.frame.width.to_pt(), object.frame.height.to_pt());
    let (sin, cos) = (f64::from(object.rotation) / 100.0).to_radians().sin_cos();
    // The way in the terms of the frame, as if it were not rotated or
    // flipped.
    let mut dir = (
        (cos * px - sin * py) / length,
        (sin * px + cos * py) / length,
    );
    if object.flip_horizontal {
        dir.0 = -dir.0;
    }
    if object.flip_vertical {
        dir.1 = -dir.1;
    }
    let origin = (w / 2.0, h / 2.0);
    let rectangle = || flattened(&Shape::Rectangle.outline(w, h));
    let outline = match &object.kind {
        ObjectKind::Shape(shape) => flattened(&shape.outline(w, h)),
        _ => rectangle(),
    };
    let farthest = |segments: Vec<(Point, Point)>| {
        segments
            .into_iter()
            .filter_map(|s| meets(origin, dir, s))
            .fold(None, |best: Option<f64>, t| {
                Some(best.map_or(t, |b| b.max(t)))
            })
    };
    let t = farthest(outline)
        .or_else(|| farthest(rectangle()))
        .unwrap_or(0.0);
    let (mut fx, mut fy) = (dir.0 * t, dir.1 * t);
    if object.flip_horizontal {
        fx = -fx;
    }
    if object.flip_vertical {
        fy = -fy;
    }
    // Back to the page, turned as the object is.
    (cx + fx * cos + fy * sin, cy - fx * sin + fy * cos)
}

/// Whether an object can be at an end of a connection line: one with the
/// same anchor, not in the text, and not a connection line itself.
fn joinable(line: &Object, object: &Object) -> bool {
    object.id != line.id
        && object.anchor == line.anchor
        && object.anchor != Anchor::InText
        && !matches!(object.kind, ObjectKind::Shape(Shape::Connection(_)))
}

/// A connection line moved to the objects it joins, from `objects`, or
/// none when it stays as it is.
pub(crate) fn followed(
    line: &Object,
    objects: impl Fn(NodeId) -> Option<Object>,
) -> Option<Object> {
    let ObjectKind::Shape(Shape::Connection(connection)) = &line.kind else {
        return None;
    };
    if connection.start.is_none() && connection.end.is_none() {
        return None;
    }
    let find = |id: Option<NodeId>| {
        id.and_then(&objects)
            .filter(|object| joinable(line, object))
    };
    let (start, end) = (find(connection.start), find(connection.end));
    let (from, to) = connection.ends_on(&line.frame);
    let bend = connection.bend.to_pt();
    // The middles of the objects, and the middle of the line between
    // them, which the ends are on the way to.
    let a = start.as_ref().map_or(from, center);
    let b = end.as_ref().map_or(to, center);
    let middle = bent_middle(a, b, bend);
    let from = start.as_ref().map_or(from, |o| edge_toward(o, middle));
    let to = end.as_ref().map_or(to, |o| edge_toward(o, middle));
    let (old_from, old_to) = connection.ends_on(&line.frame);
    let near = |p: Point, q: Point| (p.0 - q.0).abs() < SAME && (p.1 - q.1).abs() < SAME;
    let (start_id, end_id) = (start.map(|o| o.id), end.map(|o| o.id));
    let joined = (start_id, end_id) == (connection.start, connection.end);
    let upright = line.rotation == 0 && !line.flip_horizontal && !line.flip_vertical;
    if joined && upright && near(from, old_from) && near(to, old_to) {
        return None;
    }
    let (frame, moved) = Connection::between(from, to, connection.bend, start_id, end_id);
    let mut followed = line.clone();
    followed.frame = frame;
    followed.rotation = 0;
    followed.flip_horizontal = false;
    followed.flip_vertical = false;
    followed.kind = ObjectKind::Shape(Shape::Connection(moved));
    Some(followed)
}

/// A connection line from one object to another, with the frame that
/// holds it, anchored as they are, or none when they cannot be joined:
/// when they are the same, anchored apart or in the text, or connection
/// lines.
pub fn connect(from: &Object, to: &Object) -> Option<(Frame, Connection)> {
    let line = |o: &Object| matches!(o.kind, ObjectKind::Shape(Shape::Connection(_)));
    if from.id == to.id
        || from.anchor != to.anchor
        || from.anchor == Anchor::InText
        || line(from)
        || line(to)
    {
        return None;
    }
    let (a, b) = (center(from), center(to));
    let (start, end) = (edge_toward(from, b), edge_toward(to, a));
    Some(Connection::between(
        start,
        end,
        Length::ZERO,
        Some(from.id),
        Some(to.id),
    ))
}

impl Edit {
    /// Moves the connection lines to the objects they join, after the
    /// edits of a transaction.
    pub(crate) fn follow_connections(&mut self) -> Result<(), ModelError> {
        let lines: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Shape(Shape::Connection(_))))
            .cloned()
            .collect();
        for line in lines {
            let objects = self.document().objects();
            if let Some(moved) = followed(&line, |id| objects.get(id).cloned()) {
                self.replace_object(moved)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Editor};

    fn object(id: NodeId, shape: Shape, x: f64, y: f64) -> Object {
        let mut o = Object::new(id, shape.into(), Frame::from_pt(x, y, 100.0, 100.0));
        o.anchor = Anchor::Page(0);
        o
    }

    fn ends(editor: &Editor, id: NodeId) -> (Point, Point) {
        let line = editor.document().objects().get(id).unwrap();
        let ObjectKind::Shape(Shape::Connection(c)) = &line.kind else {
            panic!("{line:?}");
        };
        let ((ax, ay), (bx, by)) = c.ends_on(&line.frame);
        ((ax.round(), ay.round()), (bx.round(), by.round()))
    }

    #[test]
    fn obj_31_connection_lines_follow_the_objects_they_join() {
        let mut editor = Editor::new(Document::new_word_processing());
        let (a, b, line) = editor
            .edit("Shapes", |edit| {
                let (a, b, line) = (edit.new_id(), edit.new_id(), edit.new_id());
                let left = object(a, Shape::Rectangle, 0.0, 0.0);
                let right = object(b, Shape::Ellipse, 300.0, 0.0);
                let mut joining = object(line, Shape::Rectangle, 0.0, 0.0);
                let (frame, connection) = connect(&left, &right).unwrap();
                joining.frame = frame;
                joining.kind = ObjectKind::Shape(Shape::Connection(connection));
                edit.add_object(left)?;
                edit.add_object(right)?;
                edit.add_object(joining)?;
                Ok((a, b, line))
            })
            .unwrap();
        // From the right edge of the square to the left of the circle.
        assert_eq!(ends(&editor, line), ((100.0, 50.0), (300.0, 50.0)));
        editor
            .edit("Move", |edit| {
                edit.move_objects(&[b], Length::ZERO, Length::from_points(200))
            })
            .unwrap();
        // Down to the circle, from the right edge of the square, on the
        // way from its middle to that of the circle.
        let ((ax, ay), (bx, by)) = ends(&editor, line);
        assert_eq!((ax, ay), (100.0, 83.0));
        let (cx, cy) = (350.0, 250.0);
        assert!(((bx - cx).hypot(by - cy) - 50.0).abs() < 1.5, "{bx} {by}");
        // An edit that leaves the objects where they are leaves the line.
        let still = editor.document().objects().get(line).cloned();
        editor
            .edit("Lock", |edit| edit.lock_objects(&[a], true))
            .unwrap();
        assert_eq!(editor.document().objects().get(line).cloned(), still);
        // A line whose object is gone stays where it was and lets go.
        let before = ends(&editor, line);
        editor
            .edit("Delete", |edit| edit.delete_objects(&[b]))
            .unwrap();
        assert_eq!(ends(&editor, line), before);
        let ObjectKind::Shape(Shape::Connection(c)) =
            &editor.document().objects().get(line).unwrap().kind
        else {
            panic!();
        };
        assert_eq!((c.start, c.end), (Some(a), None));
        editor.undo().unwrap();
        editor.undo().unwrap();
        editor.undo().unwrap();
        assert_eq!(ends(&editor, line), ((100.0, 50.0), (300.0, 50.0)));
    }

    #[test]
    fn obj_31_a_bent_line_passes_through_its_pulled_middle() {
        let (frame, line) = Connection::between(
            (0.0, 0.0),
            (100.0, 0.0),
            Length::from_points(20),
            None,
            None,
        );
        // Pulled up, to the left of the way right.
        assert_eq!(frame, Frame::from_pt(0.0, -20.0, 100.0, 20.0));
        let (a, b) = line.ends_on(&frame);
        assert_eq!((a, b), ((0.0, 0.0), (100.0, 0.0)));
        let middle = bent_middle(a, b, 20.0);
        assert_eq!(middle, (50.0, -20.0));
        assert!((bend_through(a, b, middle) - 20.0).abs() < 1e-9);
        let path = outline(&line, 100.0, 20.0);
        assert!(matches!(path[1], PathSegment::Curve(..)), "{path:?}");
    }
}
