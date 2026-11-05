// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Editing the points of a shape, as Make Editable does in Pages
//! (OBJ-10): its points show in red, and they and the handles of their
//! curves move by dragging. Shift selects several; Alt and a click on the
//! outline adds a point; a double click turns a corner into a curve and
//! back; Delete removes the selected points. Smooth Path and Sharpen Path
//! turn all the points of a shape into curves or into corners.

use desk_layout::PlacedObject;
use desk_model::{Frame, NodeId, Object, ObjectKind, POLYGON_SCALE, PathPoint, PathSegment, Shape};
use gtk::gdk;

use super::TextEditor;
use super::objects::find;
use crate::view::PathMark;

/// How near a point a click takes it, in pixels.
const REACH: f64 = 5.0;

/// A point of a shape in points from the top left corner of its frame,
/// unturned, with the handles of its curves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Point {
    at: (f64, f64),
    before: Option<(f64, f64)>,
    after: Option<(f64, f64)>,
}

/// What a drag of the points moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// The selected points, with their handles.
    Points,
    /// The handle into a point.
    Before(usize),
    /// The handle out of a point.
    After(usize),
}

/// A drag of the points of a shape.
#[derive(Clone, Debug)]
struct PointDrag {
    part: Part,
    /// Where it started, in the frame.
    from: (f64, f64),
    /// The points as they were.
    points: Vec<Point>,
    /// The shape as it was placed, whose frame the drag is measured in.
    placed: PlacedObject,
    /// The shape as the document had it.
    start: Object,
    /// Whether only one handle of a curve moves (Ctrl), not both.
    one_handle: bool,
    /// Whether it has changed the shape yet, as one step of the history.
    moved: bool,
}

/// The shape whose points are being edited.
#[derive(Clone, Debug)]
pub(super) struct PointsEdit {
    id: NodeId,
    /// The points selected, by index.
    selected: Vec<usize>,
    drag: Option<PointDrag>,
}

/// The points of a shape drawn through points, in its frame of `w` by
/// `h` points, and whether it is closed.
fn points_of(shape: &Shape, w: f64, h: f64) -> Option<(Vec<Point>, bool)> {
    let Shape::Path { points, closed } = shape else {
        return None;
    };
    let scale = f64::from(POLYGON_SCALE);
    let at = |(x, y): (i32, i32)| (f64::from(x) / scale * w, f64::from(y) / scale * h);
    Some((
        points
            .iter()
            .map(|p| Point {
                at: at(p.at),
                before: p.before.map(at),
                after: p.after.map(at),
            })
            .collect(),
        *closed,
    ))
}

/// The points of an outline, as Make Editable turns a shape into them.
pub(super) fn points_of_outline(outline: &[PathSegment]) -> (Vec<Point>, bool) {
    let mut points: Vec<Point> = Vec::new();
    let mut closed = false;
    let point = |at: (f64, f64)| Point {
        at,
        before: None,
        after: None,
    };
    for segment in outline {
        match *segment {
            PathSegment::Move(x, y) if points.is_empty() => points.push(point((x, y))),
            PathSegment::Line(x, y) => points.push(point((x, y))),
            PathSegment::Curve(ax, ay, bx, by, x, y) => {
                if let Some(last) = points.last_mut() {
                    last.after = Some((ax, ay));
                }
                points.push(Point {
                    at: (x, y),
                    before: Some((bx, by)),
                    after: None,
                });
            }
            PathSegment::Close => {
                closed = true;
                break;
            }
            // Only the first part of an outline of several.
            PathSegment::Move(..) => break,
        }
    }
    // An outline that comes back to its start ends on its first point.
    let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) < 1e-6;
    if closed && points.len() > 1 && near(points[0].at, points[points.len() - 1].at) {
        let last = points.pop().unwrap_or_else(|| point((0.0, 0.0)));
        points[0].before = last.before;
    }
    (points, closed)
}

/// The frame that holds the points, as left, top, width and height in
/// the old frame, and the shape with the points in parts of it.
fn framed(points: &[Point], closed: bool) -> ((f64, f64, f64, f64), Shape) {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in points
        .iter()
        .flat_map(|p| [Some(p.at), p.before, p.after])
        .flatten()
    {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    let scale = f64::from(POLYGON_SCALE);
    let part = |(x, y): (f64, f64)| {
        (
            ((x - x0) / w * scale).round() as i32,
            ((y - y0) / h * scale).round() as i32,
        )
    };
    let shape = Shape::Path {
        points: points
            .iter()
            .map(|p| PathPoint {
                at: part(p.at),
                before: p.before.map(part),
                after: p.after.map(part),
            })
            .collect(),
        closed,
    };
    ((x0, y0, w, h), shape)
}

/// The object with new points: its frame grows or shrinks to hold them,
/// staying where it was on the page, turned as it was.
fn with_points(object: &Object, points: &[Point], closed: bool) -> Object {
    let mut changed = object.clone();
    let f = object.frame;
    let (w, h) = (f.width.to_pt(), f.height.to_pt());
    let ((bx, by, bw, bh), shape) = framed(points, closed);
    // The middle moves by the middle of the points, turned and flipped
    // as the object is.
    let (mut dx, mut dy) = (bx + bw / 2.0 - w / 2.0, by + bh / 2.0 - h / 2.0);
    if object.flip_horizontal {
        dx = -dx;
    }
    if object.flip_vertical {
        dy = -dy;
    }
    let a = (f64::from(object.rotation) / 100.0).to_radians();
    let (dx, dy) = (dx * a.cos() + dy * a.sin(), -dx * a.sin() + dy * a.cos());
    let (cx, cy) = (f.x.to_pt() + w / 2.0 + dx, f.y.to_pt() + h / 2.0 + dy);
    changed.frame = Frame::from_pt(cx - bw / 2.0, cy - bh / 2.0, bw, bh);
    changed.kind = ObjectKind::Shape(shape);
    changed
}

/// A point of the frame on the page of a placed object.
fn to_page(placed: &PlacedObject, (lx, ly): (f64, f64)) -> (f64, f64) {
    let object = &placed.object;
    let (w, h) = (f64::from(placed.width), f64::from(placed.height));
    let (mut dx, mut dy) = (lx - w / 2.0, ly - h / 2.0);
    if object.flip_horizontal {
        dx = -dx;
    }
    if object.flip_vertical {
        dy = -dy;
    }
    let a = (f64::from(object.rotation) / 100.0).to_radians();
    let (dx, dy) = (dx * a.cos() + dy * a.sin(), -dx * a.sin() + dy * a.cos());
    (
        f64::from(placed.x) + w / 2.0 + dx,
        f64::from(placed.y) + h / 2.0 + dy,
    )
}

/// A point of a page in the frame of a placed object.
fn to_frame(placed: &PlacedObject, (px, py): (f64, f64)) -> (f64, f64) {
    let object = &placed.object;
    let (w, h) = (f64::from(placed.width), f64::from(placed.height));
    let (cx, cy) = (f64::from(placed.x) + w / 2.0, f64::from(placed.y) + h / 2.0);
    let (dx, dy) = (px - cx, py - cy);
    let a = (f64::from(object.rotation) / 100.0).to_radians();
    let (mut lx, mut ly) = (dx * a.cos() - dy * a.sin(), dx * a.sin() + dy * a.cos());
    if object.flip_horizontal {
        lx = -lx;
    }
    if object.flip_vertical {
        ly = -ly;
    }
    (lx + w / 2.0, ly + h / 2.0)
}

/// The distance from a point to a segment.
fn distance_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let length = vx * vx + vy * vy;
    let t = if length > 0.0 {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.0 - a.0 - t * vx).hypot(p.1 - a.1 - t * vy)
}

/// The points with every one a curve, its handles along the line between
/// its neighbors, as Smooth Path makes them.
pub(super) fn smoothed(points: &[Point], closed: bool) -> Vec<Point> {
    let n = points.len();
    (0..n)
        .map(|i| {
            let p = points[i];
            let prev = if i > 0 {
                Some(points[i - 1].at)
            } else {
                closed.then(|| points[n - 1].at)
            };
            let next = if i + 1 < n {
                Some(points[i + 1].at)
            } else {
                closed.then(|| points[0].at)
            };
            let (from, to) = (prev.unwrap_or(p.at), next.unwrap_or(p.at));
            let (tx, ty) = ((to.0 - from.0) / 6.0, (to.1 - from.1) / 6.0);
            Point {
                at: p.at,
                before: prev.map(|_| (p.at.0 - tx, p.at.1 - ty)),
                after: next.map(|_| (p.at.0 + tx, p.at.1 + ty)),
            }
        })
        .collect()
}

/// The points with every one a corner, as Sharpen Path makes them.
fn sharpened(points: &[Point]) -> Vec<Point> {
    points
        .iter()
        .map(|p| Point {
            at: p.at,
            before: None,
            after: None,
        })
        .collect()
}

impl TextEditor {
    /// The shape whose points are edited, as placed, with its points.
    fn edited_points(&self) -> Option<(usize, PlacedObject, Vec<Point>, bool)> {
        let state = self.state.borrow();
        let id = state.points_edit.as_ref()?.id;
        let (page, placed) = find(&state.layout, id)?;
        let ObjectKind::Shape(shape) = &placed.object.kind else {
            return None;
        };
        let (points, closed) = points_of(shape, f64::from(placed.width), f64::from(placed.height))?;
        Some((page, placed.clone(), points, closed))
    }

    /// Shows the points of the shape being edited.
    pub(super) fn show_points(&self) {
        let Some((page, placed, points, closed)) = self.edited_points() else {
            self.view.set_path_mark(None);
            return;
        };
        let selected = self
            .state
            .borrow()
            .points_edit
            .as_ref()
            .map(|e| e.selected.clone())
            .unwrap_or_default();
        let page_of = |p| to_page(&placed, p);
        let outline = {
            let ObjectKind::Shape(shape) = &placed.object.kind else {
                return;
            };
            shape
                .outline(f64::from(placed.width), f64::from(placed.height))
                .into_iter()
                .map(|s| match s {
                    PathSegment::Move(x, y) => {
                        let (x, y) = page_of((x, y));
                        PathSegment::Move(x, y)
                    }
                    PathSegment::Line(x, y) => {
                        let (x, y) = page_of((x, y));
                        PathSegment::Line(x, y)
                    }
                    PathSegment::Curve(ax, ay, bx, by, x, y) => {
                        let (ax, ay) = page_of((ax, ay));
                        let (bx, by) = page_of((bx, by));
                        let (x, y) = page_of((x, y));
                        PathSegment::Curve(ax, ay, bx, by, x, y)
                    }
                    PathSegment::Close => PathSegment::Close,
                })
                .collect()
        };
        let _ = closed;
        self.view.set_path_mark(Some(PathMark {
            page,
            outline,
            points: points
                .iter()
                .enumerate()
                .map(|(i, p)| (page_of(p.at), selected.contains(&i)))
                .collect(),
            // The handles of the selected points, as in Pages.
            handles: selected
                .iter()
                .filter_map(|&i| points.get(i))
                .flat_map(|p| [p.before, p.after].map(|h| h.map(|h| (page_of(p.at), page_of(h)))))
                .flatten()
                .collect(),
        }));
    }

    /// Shows the points of the selected shape to edit them, turning a
    /// shape of another kind into one drawn through points (OBJ-10).
    /// Returns whether its points show.
    pub fn make_editable(&self) -> bool {
        let Some(info) = self.object_info() else {
            return false;
        };
        let object = &info.object;
        let ObjectKind::Shape(shape) = &object.kind else {
            return false;
        };
        if info.count != 1 || object.locked || shape.is_line() {
            return false;
        }
        if !matches!(shape, Shape::Path { .. }) {
            let (w, h) = (object.frame.width.to_pt(), object.frame.height.to_pt());
            let (points, closed) = points_of_outline(&shape.outline(w, h));
            let mut changed = object.clone();
            changed.kind = ObjectKind::Shape(framed(&points, closed).1);
            self.set_object("Make Editable", |o| *o = changed);
        }
        self.stop_instant_alpha();
        self.state.borrow_mut().points_edit = Some(PointsEdit {
            id: object.id,
            selected: Vec::new(),
            drag: None,
        });
        self.show_points();
        true
    }

    /// Whether the points of a shape show.
    pub fn editing_points(&self) -> bool {
        self.state.borrow().points_edit.is_some()
    }

    /// Hides the points.
    pub(super) fn stop_editing_points(&self) {
        if self.state.borrow_mut().points_edit.take().is_some() {
            self.view.set_path_mark(None);
        }
    }

    /// Gives the shape being edited new points, as a step of the history,
    /// or as part of the last when `continuing`.
    fn set_points(&self, points: &[Point], closed: bool, continuing: bool) {
        let Some(id) = self.state.borrow().points_edit.as_ref().map(|e| e.id) else {
            return;
        };
        let Some(object) = self
            .state
            .borrow()
            .editor
            .document()
            .objects()
            .get(id)
            .cloned()
        else {
            return;
        };
        self.set_points_of("Edit Points", &object, points, closed, continuing);
    }

    /// Gives a shape new points, measured in its frame as `object` has it.
    fn set_points_of(
        &self,
        name: &str,
        object: &Object,
        points: &[Point],
        closed: bool,
        continuing: bool,
    ) {
        let selection = self.selection();
        let depth = self.state.borrow().editor.undo_depth();
        let changed = with_points(object, points, closed);
        let edits = |edit: &mut desk_model::Edit| edit.replace_object(changed);
        let result = {
            let mut state = self.state.borrow_mut();
            if continuing {
                state.editor.edit_continuing(name, edits).map(|_| ())
            } else {
                state.editor.edit(name, edits)
            }
        };
        if let Err(error) = result {
            gtk::glib::g_warning!("desk", "cannot edit the points: {error}");
            return;
        }
        if self.state.borrow().editor.undo_depth() > depth {
            let mut state = self.state.borrow_mut();
            let action = state.next_action;
            state.next_action += 1;
            state.undo.push((selection, selection, action));
            state.redo.clear();
        }
        self.after_change(selection);
        self.show_points();
    }

    /// A press while the points of a shape show: on a point or a handle,
    /// it takes it to drag; with Alt on the outline, it adds a point; a
    /// double click on a point turns it into a curve or a corner. A press
    /// anywhere else hides the points. Returns whether the press is used.
    pub(super) fn press_points(
        &self,
        presses: i32,
        x: f64,
        y: f64,
        modifiers: gdk::ModifierType,
    ) -> bool {
        let Some((page, placed, mut points, closed)) = self.edited_points() else {
            self.stop_editing_points();
            return false;
        };
        let Some((on, px, py)) = self.view.page_at(x, y) else {
            return false;
        };
        let reach = REACH / self.view.scale();
        let local = to_frame(&placed, (px, py));
        let near = |p: (f64, f64)| {
            let (qx, qy) = to_page(&placed, p);
            on == page && (qx - px).hypot(qy - py) <= reach
        };
        let selected = self
            .state
            .borrow()
            .points_edit
            .as_ref()
            .map(|e| e.selected.clone())
            .unwrap_or_default();
        let handle = selected.iter().find_map(|&i| {
            let p = points.get(i)?;
            if p.before.is_some_and(near) {
                Some(Part::Before(i))
            } else if p.after.is_some_and(near) {
                Some(Part::After(i))
            } else {
                None
            }
        });
        let point = points.iter().position(|p| near(p.at));
        let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
        let one_handle = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        if let Some(part) = handle {
            self.start_point_drag(part, local, &placed, &points, one_handle, selected);
            return true;
        }
        if let Some(i) = point {
            if presses >= 2 {
                // A corner becomes a curve along its neighbors, and a
                // curve a corner.
                let p = points[i];
                points[i] = if p.before.is_some() || p.after.is_some() {
                    sharpened(&[p])[0]
                } else {
                    smoothed(&points, closed)[i]
                };
                self.set_points(&points, closed, false);
                return true;
            }
            let mut selected = selected;
            if shift {
                if let Some(at) = selected.iter().position(|&s| s == i) {
                    selected.remove(at);
                } else {
                    selected.push(i);
                }
            } else if !selected.contains(&i) {
                selected = vec![i];
            }
            self.start_point_drag(Part::Points, local, &placed, &points, false, selected);
            self.show_points();
            return true;
        }
        // Alt and a click near the outline adds a corner there.
        let segments = points.len() - usize::from(!closed);
        let nearest = (0..segments)
            .map(|i| {
                let (a, b) = (points[i].at, points[(i + 1) % points.len()].at);
                let (a, b) = (to_page(&placed, a), to_page(&placed, b));
                (i, distance_to_segment((px, py), a, b))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if modifiers.contains(gdk::ModifierType::ALT_MASK)
            && let Some((i, distance)) = nearest
            && on == page
            && distance <= reach * 2.0
        {
            points.insert(
                i + 1,
                Point {
                    at: local,
                    before: None,
                    after: None,
                },
            );
            if let Some(edit) = self.state.borrow_mut().points_edit.as_mut() {
                edit.selected = vec![i + 1];
            }
            self.set_points(&points, closed, false);
            return true;
        }
        self.stop_editing_points();
        false
    }

    fn start_point_drag(
        &self,
        part: Part,
        from: (f64, f64),
        placed: &PlacedObject,
        points: &[Point],
        one_handle: bool,
        selected: Vec<usize>,
    ) {
        let start = self
            .state
            .borrow()
            .editor
            .document()
            .objects()
            .get(placed.object.id)
            .cloned()
            .unwrap_or_else(|| placed.object.clone());
        if let Some(edit) = self.state.borrow_mut().points_edit.as_mut() {
            edit.selected = selected;
            edit.drag = Some(PointDrag {
                part,
                from,
                points: points.to_vec(),
                placed: placed.clone(),
                start,
                one_handle,
                moved: false,
            });
        }
    }

    /// A drag of points or of a handle. Returns whether the drag is used.
    pub(super) fn drag_points(&self, x: f64, y: f64) -> bool {
        let (drag, selected, closed) = {
            let state = self.state.borrow();
            let Some(edit) = state.points_edit.as_ref() else {
                return false;
            };
            let Some(drag) = edit.drag.clone() else {
                return false;
            };
            let closed = matches!(
                drag.placed.object.kind,
                ObjectKind::Shape(Shape::Path { closed: true, .. })
            );
            (drag, edit.selected.clone(), closed)
        };
        let Some((_, px, py)) = self.view.page_at(x, y) else {
            return true;
        };
        // Measured in the frame as it was at the press, since the frame
        // grows and moves with the points.
        let at = to_frame(&drag.placed, (px, py));
        let mut points = drag.points.clone();
        let (dx, dy) = (at.0 - drag.from.0, at.1 - drag.from.1);
        let shift = |p: (f64, f64)| (p.0 + dx, p.1 + dy);
        match drag.part {
            Part::Points => {
                for &i in &selected {
                    if let Some(p) = points.get_mut(i) {
                        p.at = shift(p.at);
                        p.before = p.before.map(shift);
                        p.after = p.after.map(shift);
                    }
                }
            }
            Part::Before(i) | Part::After(i) => {
                let Some(p) = points.get_mut(i) else {
                    return true;
                };
                let before = matches!(drag.part, Part::Before(_));
                let moved = shift(if before { p.before } else { p.after }.unwrap_or(p.at));
                let mirror = (2.0 * p.at.0 - moved.0, 2.0 * p.at.1 - moved.1);
                let (this, other) = if before {
                    (&mut p.before, &mut p.after)
                } else {
                    (&mut p.after, &mut p.before)
                };
                *this = Some(moved);
                if !drag.one_handle && other.is_some() {
                    *other = Some(mirror);
                }
            }
        }
        if points == drag.points {
            return true;
        }
        self.set_points_of("Edit Points", &drag.start, &points, closed, drag.moved);
        if let Some(d) = self
            .state
            .borrow_mut()
            .points_edit
            .as_mut()
            .and_then(|e| e.drag.as_mut())
        {
            d.moved = true;
        }
        true
    }

    /// The end of a drag of points.
    pub(super) fn end_points_drag(&self) {
        if let Some(edit) = self.state.borrow_mut().points_edit.as_mut() {
            edit.drag = None;
        }
    }

    /// The keys while the points of a shape show: Delete removes the
    /// selected points, and Esc and Return hide them. Returns whether the
    /// key is used.
    pub(super) fn points_key(&self, key: gdk::Key) -> bool {
        if !self.editing_points() {
            return false;
        }
        match key {
            gdk::Key::BackSpace | gdk::Key::Delete | gdk::Key::KP_Delete => {
                let Some((_, _, points, closed)) = self.edited_points() else {
                    return false;
                };
                let selected = self
                    .state
                    .borrow()
                    .points_edit
                    .as_ref()
                    .map(|e| e.selected.clone())
                    .unwrap_or_default();
                let left: Vec<Point> = points
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !selected.contains(i))
                    .map(|(_, p)| *p)
                    .collect();
                // A shape keeps two points at least.
                if !selected.is_empty() && left.len() >= 2 {
                    if let Some(edit) = self.state.borrow_mut().points_edit.as_mut() {
                        edit.selected.clear();
                    }
                    self.set_points(&left, closed && left.len() > 2, false);
                }
                true
            }
            gdk::Key::Escape | gdk::Key::Return | gdk::Key::KP_Enter => {
                self.stop_editing_points();
                true
            }
            _ => false,
        }
    }

    /// Turns every point of the selected shape into a curve, or with
    /// `sharp` into a corner, as Smooth Path and Sharpen Path do.
    pub fn reshape_path(&self, sharp: bool) {
        let Some(info) = self.object_info() else {
            return;
        };
        let object = info.object.clone();
        let ObjectKind::Shape(shape) = &object.kind else {
            return;
        };
        if object.locked || shape.is_line() {
            return;
        }
        let (w, h) = (object.frame.width.to_pt(), object.frame.height.to_pt());
        let (points, closed) =
            points_of(shape, w, h).unwrap_or_else(|| points_of_outline(&shape.outline(w, h)));
        let points = if sharp {
            sharpened(&points)
        } else {
            smoothed(&points, closed)
        };
        let name = if sharp { "Sharpen Path" } else { "Smooth Path" };
        self.set_points_of(name, &object, &points, closed, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corner(x: f64, y: f64) -> Point {
        Point {
            at: (x, y),
            before: None,
            after: None,
        }
    }

    #[test]
    fn obj_10_make_editable_takes_the_points_of_the_outline() {
        let (points, closed) = points_of_outline(&Shape::Rectangle.outline(100.0, 50.0));
        assert!(closed);
        assert_eq!(
            points,
            [
                corner(0.0, 0.0),
                corner(100.0, 0.0),
                corner(100.0, 50.0),
                corner(0.0, 50.0)
            ]
        );
        // An oval's points are curves, and the last one is its first.
        let (points, closed) = points_of_outline(&Shape::Ellipse.outline(100.0, 50.0));
        assert!(closed);
        assert_eq!(points.len(), 4);
        assert!(
            points
                .iter()
                .all(|p| p.before.is_some() && p.after.is_some())
        );
    }

    #[test]
    fn obj_10_points_moved_out_of_the_frame_grow_it() {
        let object = Object::new(
            NodeId::new(1).unwrap(),
            Shape::Rectangle.into(),
            Frame::from_pt(100.0, 100.0, 100.0, 50.0),
        );
        let points = [corner(0.0, 0.0), corner(150.0, 0.0), corner(0.0, 50.0)];
        let moved = with_points(&object, &points, true);
        assert_eq!(moved.frame, Frame::from_pt(100.0, 100.0, 150.0, 50.0));
        // Turned upside down, it grows the other way on the page.
        let mut turned = object.clone();
        turned.rotation = 18_000;
        let moved = with_points(&turned, &points, true);
        let x = moved.frame.x.to_pt();
        assert!((x - 50.0).abs() < 0.01, "{x}");
        assert_eq!(moved.frame.width, desk_model::Length::from_pt(150.0));
    }

    #[test]
    fn obj_10_smooth_and_sharpen_change_every_point() {
        let points = [corner(0.0, 0.0), corner(60.0, 0.0), corner(60.0, 60.0)];
        let smooth = smoothed(&points, false);
        // The ends of an open shape have one handle each.
        assert_eq!(smooth[0].before, None);
        assert_eq!(smooth[0].after, Some((10.0, 0.0)));
        assert_eq!(smooth[1].before, Some((50.0, -10.0)));
        assert_eq!(smooth[1].after, Some((70.0, 10.0)));
        assert_eq!(sharpened(&smooth), points);
        assert!(distance_to_segment((5.0, 5.0), (0.0, 0.0), (10.0, 0.0)) == 5.0);
    }
}
