// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The floating objects of a Pages document as objects of the model:
//! shapes, text boxes and groups, on pages and on the masters of the
//! sections (OBJ-08, OBJ-13, OBJ-30, INT-05).

use std::collections::HashMap;
use std::sync::Arc;

use desk_iwork::pages::{self as iwork, Drawable, DrawableKind, Place, ShapePath};
use desk_iwork::sf::{StrokePattern, Value};
use desk_model::{
    Anchor, BorderStyle, Fill, Frame, GraphicStyle, Length, LineEnd, MAX_SHAPE_SIDES, NodeId,
    Object, ObjectKind, Op, POLYGON_SCALE, PathPoint, Shape, Stroke, TextBox, Wrap,
};

use super::{Importer, rgba};
use crate::FilterError;

/// A part of [`POLYGON_SCALE`], from a part of 1.
fn part(v: f64) -> u16 {
    (v * f64::from(POLYGON_SCALE))
        .round()
        .clamp(0.0, f64::from(POLYGON_SCALE)) as u16
}

/// The shape of the model for an outline of Pages, in an object of
/// natural size `natural`.
/// A line that Pages draws from right to left is drawn from left to right
/// by the model: the second is whether the ends of its line swap.
fn shape(path: &ShapePath, natural: (f64, f64)) -> (Shape, bool) {
    let (w, h) = natural;
    let shape = match path {
        ShapePath::Bezier(path) => {
            let Some(Shape::Path { points, closed }) =
                crate::odt::objects::path_shape(path, Some((0.0, 0.0, w, h)))
            else {
                return (Shape::Rectangle, false);
            };
            let mut points = points.to_vec();
            let mut closed = closed;
            // Pages ends a closed curve where it starts, without closing it.
            if !closed && points.len() > 2 && points[0].at == points[points.len() - 1].at {
                let last = points.pop().expect("more than two points");
                points[0].before = last.before;
                closed = true;
            }
            if let [a, b] = points.as_slice()
                && !closed
                && !a.is_curve()
                && !b.is_curve()
            {
                let rising = (b.at.0 - a.at.0) * (b.at.1 - a.at.1) < 0;
                return (Shape::Line { rising }, a.at.0 > b.at.0);
            }
            plain_shape(&points, closed).unwrap_or(Shape::Path {
                points: points.into(),
                closed,
            })
        }
        ShapePath::Scalar { kind: 0, scalar } => Shape::RoundedRectangle {
            radius: Length::from_pt(scalar.clamp(0.0, w.min(h) / 2.0)),
        },
        ShapePath::Scalar { scalar, .. } => Shape::RegularPolygon {
            sides: scalar.clamp(3.0, f64::from(MAX_SHAPE_SIDES)) as u8,
        },
        ShapePath::Point { kind, point } if kind == "star" => Shape::Star {
            points: point.0.clamp(3.0, f64::from(MAX_SHAPE_SIDES)) as u8,
            inner: part(point.1),
        },
        ShapePath::Point { kind, point } => {
            let double = kind == "double";
            // Each head of a double arrow is as long as the one of an
            // arrow; the model measures it against the whole width.
            Shape::Arrow {
                double,
                head: part(if w > 0.0 { point.0 / w } else { 0.5 }),
                tail: part(point.1),
            }
        }
        ShapePath::Callout { radius } => match Shape::balloon(true) {
            Shape::Balloon { body, tip, .. } => Shape::Balloon {
                radius: Length::from_pt(radius.clamp(0.0, w.min(h) / 2.0)),
                body,
                tip,
            },
            other => other,
        },
        ShapePath::Other(_) => Shape::Rectangle,
    };
    (shape, false)
}

/// The plain shape that a closed path is: a rectangle, a polygon or an
/// ellipse.
fn plain_shape(points: &[PathPoint], closed: bool) -> Option<Shape> {
    let s = POLYGON_SCALE;
    if !closed {
        return None;
    }
    if points.iter().all(PathPoint::is_curve) {
        // Pages draws an ellipse with four curves.
        return (points.len() == 4).then_some(Shape::Ellipse);
    }
    if points.iter().any(PathPoint::is_curve) {
        return None;
    }
    let corners: Vec<(i32, i32)> = points.iter().map(|p| p.at).collect();
    let mut rectangle = [(0, 0), (s, 0), (s, s), (0, s)];
    for _ in 0..4 {
        if corners == rectangle {
            return Some(Shape::Rectangle);
        }
        rectangle.rotate_left(1);
    }
    Some(Shape::Polygon(corners.into()))
}

/// What an end of a line ends in, by the name Pages gives it.
fn line_end(value: Option<&Value>) -> LineEnd {
    let Some(Value::LineEnd(name)) = value else {
        return LineEnd::None;
    };
    let name = name.to_lowercase();
    if name.contains("circle") {
        LineEnd::Circle
    } else if name.contains("square") || name.contains("diamond") {
        LineEnd::Square
    } else if name.contains("arrow") && !name.contains("filled") {
        LineEnd::OpenArrow
    } else if name.contains("arrow") {
        LineEnd::Arrow
    } else if name.contains("bar") || name.contains("line") {
        LineEnd::Bar
    } else {
        LineEnd::None
    }
}

/// The style of the model for the properties of a graphic style.
fn graphic_style(drawable: &Drawable) -> GraphicStyle {
    let properties = &drawable.style;
    let fill = match properties.get("fill") {
        Some(Value::Color(color)) => Some(Fill::Color(rgba(*color))),
        Some(Value::Gradient { angle, stops }) => match (stops.first(), stops.last()) {
            (Some(from), Some(to)) => Some(Fill::Gradient {
                from: rgba(from.1),
                to: rgba(to.1),
                angle: (angle.to_degrees() * 100.0).round().rem_euclid(36_000.0) as i32,
            }),
            _ => None,
        },
        _ => None,
    };
    let stroke = match properties.get("stroke") {
        Some(Value::Stroke(stroke))
            if stroke.width > 0.0 && stroke.pattern != StrokePattern::Empty =>
        {
            Some(Stroke {
                style: match stroke.pattern {
                    StrokePattern::Dashes(_) => BorderStyle::Dashed,
                    _ => BorderStyle::Solid,
                },
                start: line_end(properties.get("tailLineEnd")),
                end: line_end(properties.get("headLineEnd")),
                ..Stroke::new(rgba(stroke.color), Length::from_pt(stroke.width))
            })
        }
        _ => None,
    };
    GraphicStyle {
        fill,
        stroke,
        opacity: properties
            .number("opacity")
            .map_or(100, |o| (o * 100.0).round().clamp(0.0, 100.0) as u8),
        shadow: None,
    }
}

/// How text goes around an object.
fn wrap(drawable: &Drawable) -> (Wrap, Length) {
    let margin = drawable.wrap.as_ref().map_or(Length::from_points(12), |w| {
        Length::from_pt(w.margin.max(0.0))
    });
    // An object sent to the background is behind the text (OBJ-25).
    if drawable.order < 0 {
        return (Wrap::Behind, margin);
    }
    let Some(wrap) = &drawable.wrap else {
        return (Wrap::Both, margin);
    };
    let kind = if !wrap.enabled {
        Wrap::Through
    } else if wrap.kind == "largest" {
        Wrap::Largest
    } else {
        match wrap.direction.as_str() {
            "left" => Wrap::Left,
            "right" => Wrap::Right,
            "neither" => Wrap::AboveAndBelow,
            _ => Wrap::Both,
        }
    };
    (kind, margin)
}

/// Builds the objects of the model, with the ids that the text boxes
/// link to each other by.
struct Builder<'a, 'b> {
    importer: &'b mut Importer<'a>,
    /// The ids of the objects, by the object IDs of Pages.
    ids: HashMap<String, NodeId>,
}

impl Builder<'_, '_> {
    /// The object for a drawable, placed from `origin`, or none for one
    /// that the import leaves out.
    fn object(
        &mut self,
        drawable: &Drawable,
        anchor: Anchor,
        origin: (f64, f64),
    ) -> Result<Option<Object>, FilterError> {
        let geometry = &drawable.geometry;
        let (w, h) = geometry.size;
        let (cx, cy) = geometry.center();
        let frame = Frame::from_pt(
            origin.0 + cx - w / 2.0,
            origin.1 + cy - h / 2.0,
            w.max(0.0),
            h.max(0.0),
        );
        let id = match drawable.id.as_ref().and_then(|i| self.ids.get(i)) {
            Some(&id) => id,
            None => self.importer.document.new_id(),
        };
        let mut text = Vec::new();
        let mut swap_ends = false;
        let kind = match &drawable.kind {
            DrawableKind::Shape {
                path,
                text_box,
                text: paragraphs,
                next,
            } => {
                for paragraph in paragraphs {
                    text.extend(self.importer.paragraph(paragraph, None)?);
                }
                if *text_box {
                    if text.is_empty() {
                        text.push(desk_model::Paragraph::new(
                            self.importer.document.new_id(),
                            self.importer.body,
                        ));
                    }
                    let mut text_box =
                        TextBox::new(std::mem::take(&mut text).into(), Length::from_points(4));
                    text_box.next = next.as_ref().and_then(|n| self.ids.get(n)).copied();
                    ObjectKind::TextBox(text_box)
                } else {
                    let (shape, reversed) = shape(path, geometry.natural);
                    swap_ends = reversed;
                    ObjectKind::Shape(shape)
                }
            }
            DrawableKind::Group(children) => {
                // The objects of a group are placed from its top left
                // corner, and kept on the page.
                let origin = (
                    origin.0 + geometry.position.0,
                    origin.1 + geometry.position.1,
                );
                let mut objects = Vec::new();
                for child in children {
                    if let Some(object) = self.object(child, anchor, origin)? {
                        objects.push(object);
                    }
                }
                if objects.is_empty() {
                    return Ok(None);
                }
                ObjectKind::Group(objects.into())
            }
            DrawableKind::Other(_) => return Ok(None),
        };
        let (wrap, wrap_margin) = wrap(drawable);
        let mut object = Object::new(id, kind, frame);
        object.anchor = anchor;
        object.rotation = (geometry.angle * 100.0).round().rem_euclid(36_000.0) as i32;
        object.flip_horizontal = geometry.flip_horizontal;
        object.flip_vertical = geometry.flip_vertical;
        object.style = graphic_style(drawable);
        if swap_ends && let Some(stroke) = &mut object.style.stroke {
            std::mem::swap(&mut stroke.start, &mut stroke.end);
        }
        object.wrap = wrap;
        object.wrap_margin = wrap_margin;
        object.locked = drawable.locked;
        if object.holds_text() && !text.iter().all(desk_model::Paragraph::is_empty) {
            // Pages centers the text of a shape that does not say how it
            // is aligned.
            let centered = text
                .into_iter()
                .map(|p| {
                    if p.attrs().alignment.is_some() {
                        return Ok(p);
                    }
                    let attrs = desk_model::ParaAttrs {
                        alignment: Some(desk_model::Alignment::Center),
                        ..p.attrs().clone()
                    };
                    desk_model::Paragraph::from_runs(p.id(), p.style(), attrs, p.runs().cloned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            object.text = Some(TextBox::in_shape(Arc::from(centered)));
        }
        Ok(Some(object))
    }
}

/// Puts the floating objects of a document in it: those of the pages on
/// their pages, and those of the masters on the masters of their
/// sections, by the paragraphs that start the sections with headers of
/// their own. The first section is that of `first`. Returns how many it
/// left out.
pub(super) fn import_objects(
    importer: &mut Importer<'_>,
    source: &iwork::PagesDocument,
    first: Option<&str>,
    sections: &HashMap<&str, NodeId>,
) -> Result<usize, FilterError> {
    let mut drawables: Vec<&Drawable> = source.drawables.iter().collect();
    // From back to front: the masters behind the pages, the objects behind
    // the text first, then by their order.
    drawables.sort_by_key(|d| match d.place {
        Place::Master { background, .. } => (0, i32::from(!background), d.order),
        Place::Page(page) => (1, i32::try_from(page).unwrap_or(i32::MAX), d.order),
    });
    let mut builder = Builder {
        importer,
        ids: HashMap::new(),
    };
    for drawable in &drawables {
        if let Some(id) = &drawable.id {
            let node = builder.importer.document.new_id();
            builder.ids.insert(id.clone(), node);
        }
    }
    let mut left_out = 0;
    for drawable in drawables {
        let anchor = match &drawable.place {
            Place::Page(page) => Anchor::Page(*page),
            Place::Master { section, .. } if Some(section.as_str()) == first => {
                Anchor::Master(None)
            }
            Place::Master { section, .. } => match sections.get(section.as_str()) {
                Some(&id) => Anchor::Master(Some(id)),
                None => {
                    left_out += 1;
                    continue;
                }
            },
        };
        let Some(mut object) = builder.object(drawable, anchor, (0.0, 0.0))? else {
            continue;
        };
        if matches!(
            drawable.place,
            Place::Master {
                background: true,
                ..
            }
        ) {
            object.wrap = Wrap::Behind;
        }
        let index = builder.importer.document.objects().len();
        builder
            .importer
            .document
            .apply(Op::InsertObject { index, object })?;
    }
    Ok(left_out)
}
