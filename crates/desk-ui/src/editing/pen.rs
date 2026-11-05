// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pen of Pages, which draws a shape point by point (OBJ-09): each
//! click adds a corner, and a drag pulls the point into a curve. A click
//! on the first point closes the shape; a double click, Return or Esc
//! leave it open. Delete takes back the last point.

use std::sync::Arc;

use desk_model::{
    Anchor, Fill, Frame, Length, NodeId, Object, POLYGON_SCALE, PathPoint, PathSegment, Rgba,
    Shape, Stroke,
};
use gtk::gdk;

use super::TextEditor;
use crate::view::PathMark;

/// A point of a shape being drawn, in points on its page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PenPoint {
    at: (f64, f64),
    /// The handle of the curve that leaves the point; the one that comes
    /// in is its mirror.
    handle: Option<(f64, f64)>,
}

/// A shape being drawn with the pen.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Pen {
    /// The page it is drawn on, from its first point.
    page: Option<usize>,
    points: Vec<PenPoint>,
    /// Where the press that added the last point was, in the view, while
    /// it is dragged into a curve.
    pressed: Option<(f64, f64)>,
}

/// How near the first point a click closes the shape, in pixels.
const CLOSE_DISTANCE: f64 = 6.0;

/// The handle coming into a point, the mirror of the one leaving it.
fn mirror(point: &PenPoint) -> Option<(f64, f64)> {
    point
        .handle
        .map(|(hx, hy)| (2.0 * point.at.0 - hx, 2.0 * point.at.1 - hy))
}

/// The outline through the points, in points on the page.
fn outline(points: &[PenPoint], closed: bool) -> Vec<PathSegment> {
    let mut path = Vec::new();
    let Some(first) = points.first() else {
        return path;
    };
    path.push(PathSegment::Move(first.at.0, first.at.1));
    let back = (closed && points.len() > 2).then(|| (points[points.len() - 1], *first));
    for (from, to) in points.windows(2).map(|p| (p[0], p[1])).chain(back) {
        let (x, y) = to.at;
        match (from.handle, mirror(&to)) {
            (None, None) => path.push(PathSegment::Line(x, y)),
            (a, b) => {
                let (ax, ay) = a.unwrap_or(from.at);
                let (bx, by) = b.unwrap_or(to.at);
                path.push(PathSegment::Curve(ax, ay, bx, by, x, y));
            }
        }
    }
    if closed {
        path.push(PathSegment::Close);
    }
    path
}

/// The frame of a shape drawn through points, which holds its points and
/// handles, and its points in parts of the frame; none for fewer than two
/// points.
pub(super) fn drawn_shape(points: &[PenPoint], closed: bool) -> Option<(Frame, Shape)> {
    if points.len() < 2 {
        return None;
    }
    let all = points
        .iter()
        .flat_map(|p| [Some(p.at), p.handle, mirror(p)])
        .flatten();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in all {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    // A straight line along an edge still has a frame to scale in.
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
                before: mirror(p).map(part),
                after: p.handle.map(part),
            })
            .collect::<Arc<[PathPoint]>>(),
        closed,
    };
    Some((Frame::from_pt(x0, y0, w, h), shape))
}

impl TextEditor {
    /// Turns the pen on, so that clicks draw a shape (OBJ-09).
    pub fn start_pen(&self) {
        self.stop_instant_alpha();
        self.leave_story(None);
        self.state.borrow_mut().objects.clear();
        self.state.borrow_mut().pen = Some(Pen::default());
        gtk::prelude::WidgetExt::set_cursor_from_name(self.view.area(), Some("crosshair"));
        self.changed();
    }

    /// Whether the pen is on.
    pub fn pen_on(&self) -> bool {
        self.state.borrow().pen.is_some()
    }

    /// Shows the shape being drawn.
    fn show_pen(&self) {
        let mark = self.state.borrow().pen.as_ref().and_then(|pen| {
            Some(PathMark {
                page: pen.page?,
                outline: outline(&pen.points, false),
                points: pen.points.iter().map(|p| (p.at, false)).collect(),
                handles: pen
                    .points
                    .iter()
                    .flat_map(|p| [p.handle, mirror(p)].map(|h| h.map(|h| (p.at, h))))
                    .flatten()
                    .collect(),
            })
        });
        self.view.set_path_mark(mark);
    }

    /// Ends the pen: the shape drawn, if it has two points, goes on the
    /// page, closed or open, and is selected.
    fn finish_pen(&self, closed: bool) {
        let Some(pen) = self.state.borrow_mut().pen.take() else {
            return;
        };
        gtk::prelude::WidgetExt::set_cursor_from_name(self.view.area(), None);
        self.view.set_path_mark(None);
        let (Some(page), Some((frame, shape))) = (
            pen.page,
            drawn_shape(&pen.points, closed && pen.points.len() > 2),
        ) else {
            self.changed();
            return;
        };
        let selection = self.selection();
        let result: Result<NodeId, desk_model::ModelError> =
            self.state.borrow_mut().editor.edit("Draw Shape", |edit| {
                let id = edit.new_id();
                let mut object = Object::new(id, shape.into(), frame);
                object.anchor = Anchor::Page(page as u32);
                // Filled with the blue of the shapes of Pages when
                // closed; an open one is only its line.
                object.style.fill = closed.then(|| Fill::Color(Rgba::rgb(0x6c, 0x9f, 0xd8)));
                object.style.stroke = Some(Stroke::new(Rgba::BLACK, Length::from_points(1)));
                edit.add_object(object)?;
                Ok(id)
            });
        match result {
            Ok(id) => self.select_new_object(selection, id),
            Err(error) => gtk::glib::g_warning!("desk", "cannot draw: {error}"),
        }
    }

    /// A press while the pen is on: a point, or on the first point the
    /// end of a closed shape; a double click ends an open one. Returns
    /// whether the press is used.
    pub(super) fn press_pen(&self, presses: i32, x: f64, y: f64) -> bool {
        if !self.pen_on() {
            return false;
        }
        if presses >= 2 {
            // The first click of the two added the last point.
            self.finish_pen(false);
            return true;
        }
        let Some((page, px, py)) = self.view.page_at(x, y) else {
            return true;
        };
        let scale = self.view.zoom() * crate::geometry::PIXELS_PER_POINT;
        let closes = {
            let state = self.state.borrow();
            let pen = state.pen.as_ref();
            pen.and_then(|p| Some((p.page?, p.points.first()?.at, p.points.len())))
                .is_some_and(|(on, (fx, fy), count)| {
                    on == page && count > 2 && (fx - px).hypot(fy - py) * scale <= CLOSE_DISTANCE
                })
        };
        if closes {
            self.finish_pen(true);
            return true;
        }
        if let Some(pen) = self.state.borrow_mut().pen.as_mut() {
            // The shape stays on the page of its first point.
            if pen.page.is_some_and(|p| p != page) {
                return true;
            }
            pen.page = Some(page);
            pen.points.push(PenPoint {
                at: (px, py),
                handle: None,
            });
            pen.pressed = Some((x, y));
        }
        self.show_pen();
        true
    }

    /// A drag after a press of the pen pulls the point into a curve, its
    /// handle following the pointer. Returns whether the drag is used.
    pub(super) fn drag_pen(&self, dx: f64, dy: f64) -> bool {
        let pressed = match self.state.borrow().pen.as_ref() {
            Some(pen) => pen.pressed,
            None => return false,
        };
        let Some((x, y)) = pressed else { return true };
        let Some((_, px, py)) = self.view.page_at(x + dx, y + dy) else {
            return true;
        };
        if let Some(last) = self
            .state
            .borrow_mut()
            .pen
            .as_mut()
            .and_then(|p| p.points.last_mut())
        {
            // A small drag, of a few pixels, is still a click.
            last.handle = (dx.hypot(dy) > 3.0).then_some((px, py));
        }
        self.show_pen();
        true
    }

    /// The end of a drag of the pen.
    pub(super) fn end_pen_drag(&self) {
        if let Some(pen) = self.state.borrow_mut().pen.as_mut() {
            pen.pressed = None;
        }
    }

    /// The keys of the pen: Delete takes back the last point, and Return
    /// and Esc end the shape open. Returns whether the key is used.
    pub(super) fn pen_key(&self, key: gdk::Key) -> bool {
        if !self.pen_on() {
            return false;
        }
        match key {
            gdk::Key::BackSpace | gdk::Key::Delete | gdk::Key::KP_Delete => {
                if let Some(pen) = self.state.borrow_mut().pen.as_mut() {
                    pen.points.pop();
                    if pen.points.is_empty() {
                        pen.page = None;
                    }
                }
                self.show_pen();
                true
            }
            gdk::Key::Escape | gdk::Key::Return | gdk::Key::KP_Enter => {
                self.finish_pen(false);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_09_a_drawn_shape_takes_the_frame_of_its_points() {
        let points = [
            PenPoint {
                at: (100.0, 200.0),
                handle: None,
            },
            PenPoint {
                at: (200.0, 100.0),
                handle: Some((250.0, 100.0)),
            },
            PenPoint {
                at: (300.0, 200.0),
                handle: None,
            },
        ];
        let (frame, shape) = drawn_shape(&points, false).unwrap();
        assert_eq!(frame, Frame::from_pt(100.0, 100.0, 200.0, 100.0));
        let Shape::Path { points, closed } = shape else {
            panic!("{shape:?}");
        };
        assert!(!closed);
        assert_eq!(points[0], PathPoint::corner(0, 10_000));
        assert_eq!(
            points[1],
            PathPoint {
                at: (5_000, 0),
                before: Some((2_500, 0)),
                after: Some((7_500, 0)),
            }
        );
        // One point is no shape.
        assert!(drawn_shape(&points_of(&[(0.0, 0.0)]), false).is_none());
    }

    fn points_of(at: &[(f64, f64)]) -> Vec<PenPoint> {
        at.iter().map(|&at| PenPoint { at, handle: None }).collect()
    }

    #[test]
    fn obj_09_the_outline_of_the_pen_joins_its_points() {
        let points = points_of(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
        assert_eq!(outline(&points, false).len(), 3);
        assert_eq!(outline(&points, true).last(), Some(&PathSegment::Close));
    }
}
