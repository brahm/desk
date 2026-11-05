// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The outlines of shapes, in points from the top left corner of their
//! frame, which the renderer draws and the ODF filter writes (OBJ-08,
//! OBJ-11).

use std::f64::consts::PI;

use crate::{POLYGON_SCALE, Shape};

/// A step of an outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    /// Starts a part at a point.
    Move(f64, f64),
    /// A straight line to a point.
    Line(f64, f64),
    /// A cubic Bézier curve, by its two control points and its end.
    Curve(f64, f64, f64, f64, f64, f64),
    /// Closes the part, back to where it started.
    Close,
}

/// How far the control points of a Bézier curve that draws a quarter of a
/// circle are from its ends, for a radius of one.
const KAPPA: f64 = 0.552_284_749_8;

fn part(value: impl Into<f64>) -> f64 {
    value.into() / f64::from(POLYGON_SCALE)
}

/// A closed outline through points.
fn closed(points: impl IntoIterator<Item = (f64, f64)>) -> Vec<PathSegment> {
    let mut path: Vec<PathSegment> = points
        .into_iter()
        .enumerate()
        .map(|(i, (x, y))| {
            if i == 0 {
                PathSegment::Move(x, y)
            } else {
                PathSegment::Line(x, y)
            }
        })
        .collect();
    if !path.is_empty() {
        path.push(PathSegment::Close);
    }
    path
}

/// The points of a shape with `count` corners around the middle of the
/// frame, the first at the top, each `radius(i)` of the way out.
fn around(count: usize, w: f64, h: f64, radius: impl Fn(usize) -> f64) -> Vec<PathSegment> {
    closed((0..count).map(|i| {
        let a = 2.0 * PI * i as f64 / count as f64 - PI / 2.0;
        let r = radius(i);
        (w / 2.0 * (1.0 + r * a.cos()), h / 2.0 * (1.0 + r * a.sin()))
    }))
}

/// A rectangle with round corners of radius `r`, from `(x, y)`, without
/// closing it, so that a tail can join its bottom edge at `tail`.
fn rounded(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    r: f64,
    tail: Option<[(f64, f64); 3]>,
) -> Vec<PathSegment> {
    use PathSegment::{Close, Curve, Line, Move};
    let r = r.clamp(0.0, w.min(h) / 2.0);
    let k = r * (1.0 - KAPPA);
    let (x1, y1) = (x + w, y + h);
    let mut path = vec![
        Move(x + r, y),
        Line(x1 - r, y),
        Curve(x1 - k, y, x1, y + k, x1, y + r),
        Line(x1, y1 - r),
        Curve(x1, y1 - k, x1 - k, y1, x1 - r, y1),
    ];
    if let Some(points) = tail {
        path.extend(points.map(|(px, py)| Line(px, py)));
    }
    path.extend([
        Line(x + r, y1),
        Curve(x + k, y1, x, y1 - k, x, y1 - r),
        Line(x, y + r),
        Curve(x, y + k, x + k, y, x + r, y),
        Close,
    ]);
    path
}

pub(crate) fn outline(shape: &Shape, w: f64, h: f64) -> Vec<PathSegment> {
    use PathSegment::{Close, Curve, Line, Move};
    match shape {
        Shape::Rectangle => closed([(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]),
        Shape::RoundedRectangle { radius } => rounded(0.0, 0.0, w, h, radius.to_pt(), None),
        Shape::Ellipse => {
            let (rx, ry) = (w / 2.0, h / 2.0);
            let (kx, ky) = (rx * KAPPA, ry * KAPPA);
            vec![
                Move(w, ry),
                Curve(w, ry + ky, rx + kx, h, rx, h),
                Curve(rx - kx, h, 0.0, ry + ky, 0.0, ry),
                Curve(0.0, ry - ky, rx - kx, 0.0, rx, 0.0),
                Curve(rx + kx, 0.0, w, ry - ky, w, ry),
                Close,
            ]
        }
        Shape::Polygon(points) => closed(points.iter().map(|&(x, y)| (w * part(x), h * part(y)))),
        Shape::Line { rising } => {
            if *rising {
                vec![Move(0.0, h), Line(w, 0.0)]
            } else {
                vec![Move(0.0, 0.0), Line(w, h)]
            }
        }
        Shape::Star { points, inner } => {
            let points = usize::from((*points).clamp(3, crate::MAX_SHAPE_SIDES));
            let inner = part(*inner).clamp(0.0, 1.0);
            around(points * 2, w, h, |i| if i % 2 == 0 { 1.0 } else { inner })
        }
        Shape::RegularPolygon { sides } => around(
            usize::from((*sides).clamp(3, crate::MAX_SHAPE_SIDES)),
            w,
            h,
            |_| 1.0,
        ),
        Shape::Arrow { double, head, tail } => {
            let most = if *double { 0.5 } else { 1.0 };
            let head = w * part(*head).clamp(0.0, most);
            let (top, bottom) = {
                let t = h * part(*tail).clamp(0.0, 1.0);
                ((h - t) / 2.0, (h + t) / 2.0)
            };
            let back = w - head;
            if *double {
                closed([
                    (0.0, h / 2.0),
                    (head, 0.0),
                    (head, top),
                    (back, top),
                    (back, 0.0),
                    (w, h / 2.0),
                    (back, h),
                    (back, bottom),
                    (head, bottom),
                    (head, h),
                ])
            } else {
                closed([
                    (0.0, top),
                    (back, top),
                    (back, 0.0),
                    (w, h / 2.0),
                    (back, h),
                    (back, bottom),
                    (0.0, bottom),
                ])
            }
        }
        Shape::Path { points, closed } => {
            let at = |(x, y): (i32, i32)| (w * part(x), h * part(y));
            let mut path = Vec::new();
            let Some(first) = points.first() else {
                return path;
            };
            let (x, y) = at(first.at);
            path.push(Move(x, y));
            let pairs = points.windows(2).map(|p| (p[0], p[1]));
            let back = (*closed && points.len() > 2).then(|| (points[points.len() - 1], *first));
            for (from, to) in pairs.chain(back) {
                let (x, y) = at(to.at);
                if from.after.is_some() || to.before.is_some() {
                    let (ax, ay) = at(from.after.unwrap_or(from.at));
                    let (bx, by) = at(to.before.unwrap_or(to.at));
                    path.push(Curve(ax, ay, bx, by, x, y));
                } else {
                    path.push(Line(x, y));
                }
            }
            if *closed {
                path.push(Close);
            }
            path
        }
        Shape::Connection(connection) => crate::connection::outline(connection, w, h),
        Shape::Balloon { radius, body, tip } => {
            let body_h = h * part(*body).clamp(0.1, 1.0);
            let r = radius.to_pt().clamp(0.0, w.min(body_h) / 2.0);
            // The tail starts on the bottom edge, between the corners, a
            // sixth of the width wide, toward its point.
            let tip_x = w * part(*tip).clamp(0.0, 1.0);
            let base = (w / 6.0).min(w - 2.0 * r);
            let left = (tip_x - base / 2.0).clamp(r, (w - r - base).max(r));
            let tail = (body_h < h && base > 0.0).then_some([
                (left + base, body_h),
                (tip_x, h),
                (left, body_h),
            ]);
            rounded(0.0, 0.0, w, body_h, r, tail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corners(path: &[PathSegment]) -> Vec<(i64, i64)> {
        path.iter()
            .filter_map(|s| match *s {
                PathSegment::Move(x, y) | PathSegment::Line(x, y) => {
                    Some((x.round() as i64, y.round() as i64))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn obj_11_stars_and_polygons_have_their_points_and_sides() {
        let star = Shape::Star {
            points: 4,
            inner: 5_000,
        }
        .outline(100.0, 100.0);
        assert_eq!(
            corners(&star),
            [
                (50, 0),
                (68, 32),
                (100, 50),
                (68, 68),
                (50, 100),
                (32, 68),
                (0, 50),
                (32, 32)
            ]
        );
        let square = Shape::RegularPolygon { sides: 4 }.outline(100.0, 100.0);
        assert_eq!(corners(&square), [(50, 0), (100, 50), (50, 100), (0, 50)]);
        // Too few sides are taken as three.
        let triangle = Shape::RegularPolygon { sides: 1 }.outline(100.0, 100.0);
        assert_eq!(corners(&triangle).len(), 3);
    }

    #[test]
    fn obj_11_arrows_have_their_head_and_tail() {
        let arrow = Shape::Arrow {
            double: false,
            head: 2_500,
            tail: 4_000,
        }
        .outline(200.0, 100.0);
        assert_eq!(
            corners(&arrow),
            [
                (0, 30),
                (150, 30),
                (150, 0),
                (200, 50),
                (150, 100),
                (150, 70),
                (0, 70)
            ]
        );
        // A double arrow's heads take at most half of it each.
        let double = Shape::Arrow {
            double: true,
            head: 9_000,
            tail: 4_000,
        }
        .outline(200.0, 100.0);
        assert!(corners(&double).contains(&(100, 0)));
    }

    #[test]
    fn obj_09_obj_10_paths_join_their_points_with_lines_and_curves() {
        use crate::PathPoint;
        let curve = PathPoint {
            at: (10_000, 0),
            before: Some((5_000, 0)),
            after: None,
        };
        let points = [
            PathPoint::corner(0, 0),
            curve,
            PathPoint::corner(10_000, 10_000),
        ];
        let open = Shape::Path {
            points: points.as_slice().into(),
            closed: false,
        }
        .outline(100.0, 50.0);
        assert_eq!(
            open,
            [
                PathSegment::Move(0.0, 0.0),
                PathSegment::Curve(0.0, 0.0, 50.0, 0.0, 100.0, 0.0),
                PathSegment::Line(100.0, 50.0),
            ]
        );
        let closed = Shape::Path {
            points: points.as_slice().into(),
            closed: true,
        }
        .outline(100.0, 50.0);
        assert_eq!(
            &closed[3..],
            [PathSegment::Line(0.0, 0.0), PathSegment::Close]
        );
    }

    #[test]
    fn obj_11_the_tail_of_a_balloon_points_where_it_is_set() {
        let callout = Shape::Balloon {
            radius: crate::Length::ZERO,
            body: 8_000,
            tip: 2_500,
        }
        .outline(120.0, 100.0);
        let points = corners(&callout);
        assert!(points.contains(&(30, 100)), "{points:?}");
        assert!(points.iter().all(|&(_, y)| y <= 80 || y == 100));
        // A body as tall as the frame has no tail.
        let bubble = Shape::Balloon {
            radius: crate::Length::from_points(10),
            body: 10_000,
            tip: 2_500,
        }
        .outline(120.0, 100.0);
        assert!(corners(&bubble).iter().all(|&(_, y)| y <= 100));
        assert!(bubble.iter().any(|s| matches!(s, PathSegment::Curve(..))));
    }
}
