// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings of a selected shape that Pages changes with its controls:
//! the corners of a rounded rectangle, the points of a star, the sides of
//! a polygon, the head and tail of an arrow and the tail of a balloon
//! (OBJ-11).

use crate::compact;
use std::cell::Cell;
use std::rc::Rc;

use desk_model::{Length, MAX_SHAPE_SIDES, ObjectKind, POLYGON_SCALE, Shape};

use crate::editing::ObjectInfo;
use crate::formatting::EditorOf;
use crate::i18n::tr;

/// What a row sets.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Setting {
    Radius,
    Points,
    Inner,
    Sides,
    Head,
    Tail,
    TailPosition,
    TailLength,
}

/// The Shape group of the Inspector.
pub struct ShapeGroup {
    group: compact::Group,
    rows: Vec<(Setting, compact::SpinRow)>,
    updating: Cell<bool>,
}

/// A percent of [`POLYGON_SCALE`].
fn percent(part: u16) -> f64 {
    f64::from(part) * 100.0 / f64::from(POLYGON_SCALE)
}

/// The part of [`POLYGON_SCALE`] of a percent.
fn part(percent: f64) -> u16 {
    (percent.clamp(0.0, 100.0) * f64::from(POLYGON_SCALE) / 100.0).round() as u16
}

/// The settings that a shape has, with their values in the rows' units.
fn settings(shape: &Shape) -> Vec<(Setting, f64)> {
    match *shape {
        Shape::RoundedRectangle { radius } => vec![(Setting::Radius, radius.to_pt())],
        Shape::Star { points, inner } => {
            vec![
                (Setting::Points, f64::from(points)),
                (Setting::Inner, percent(inner)),
            ]
        }
        Shape::RegularPolygon { sides } => vec![(Setting::Sides, f64::from(sides))],
        Shape::Arrow { head, tail, .. } => {
            vec![
                (Setting::Head, percent(head)),
                (Setting::Tail, percent(tail)),
            ]
        }
        Shape::Balloon { radius, body, tip } => vec![
            (Setting::Radius, radius.to_pt()),
            (Setting::TailPosition, percent(tip)),
            (Setting::TailLength, 100.0 - percent(body)),
        ],
        _ => Vec::new(),
    }
}

/// The shape with a setting changed.
fn changed(shape: &Shape, setting: Setting, value: f64) -> Shape {
    let mut shape = shape.clone();
    let count = |v: f64| v.round().clamp(3.0, f64::from(MAX_SHAPE_SIDES)) as u8;
    match (&mut shape, setting) {
        (Shape::RoundedRectangle { radius } | Shape::Balloon { radius, .. }, Setting::Radius) => {
            *radius = Length::from_pt(value.max(0.0))
        }
        (Shape::Star { points, .. }, Setting::Points) => *points = count(value),
        (Shape::Star { inner, .. }, Setting::Inner) => *inner = part(value),
        (Shape::RegularPolygon { sides }, Setting::Sides) => *sides = count(value),
        (Shape::Arrow { head, .. }, Setting::Head) => *head = part(value),
        (Shape::Arrow { tail, .. }, Setting::Tail) => *tail = part(value),
        (Shape::Balloon { tip, .. }, Setting::TailPosition) => *tip = part(value),
        (Shape::Balloon { body, .. }, Setting::TailLength) => {
            *body = part(100.0 - value.clamp(0.0, 90.0));
        }
        _ => {}
    }
    shape
}

impl ShapeGroup {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let row = |title: String, subtitle: String, min: f64, max: f64, digits: u32| {
            let row = compact::SpinRow::with_range(min, max, 1.0);
            row.set_title(&title);
            row.set_subtitle(&subtitle);
            row.set_digits(digits);
            row
        };
        let sides = f64::from(MAX_SHAPE_SIDES);
        let rows = vec![
            (
                Setting::Radius,
                row(tr("Corner Radius"), tr("Points"), 0.0, 1000.0, 1),
            ),
            (
                Setting::Points,
                row(tr("Star Points"), String::new(), 3.0, sides, 0),
            ),
            (
                Setting::Inner,
                row(
                    tr("Inner Radius"),
                    tr("Percent of the outer radius"),
                    0.0,
                    100.0,
                    0,
                ),
            ),
            (
                Setting::Sides,
                row(tr("Sides"), String::new(), 3.0, sides, 0),
            ),
            (
                Setting::Head,
                row(tr("Arrow Head"), tr("Percent of the width"), 0.0, 100.0, 0),
            ),
            (
                Setting::Tail,
                row(tr("Arrow Tail"), tr("Percent of the height"), 0.0, 100.0, 0),
            ),
            (
                Setting::TailPosition,
                row(
                    tr("Tail Position"),
                    tr("Percent of the width"),
                    0.0,
                    100.0,
                    0,
                ),
            ),
            (
                Setting::TailLength,
                row(tr("Tail Length"), tr("Percent of the height"), 0.0, 90.0, 0),
            ),
        ];
        let group = compact::Group::builder().title(tr("Shape")).build();
        for (_, row) in &rows {
            group.add(row);
        }
        let this = Rc::new(Self {
            group,
            rows,
            updating: Cell::new(false),
        });
        this.show(None);
        for (setting, row) in &this.rows {
            let weak = Rc::downgrade(&this);
            let editor = editor.clone();
            let setting = *setting;
            row.connect_value_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let value = row.value();
                if let Some(editor) = editor() {
                    editor.set_object("Shape", |object| {
                        if let ObjectKind::Shape(shape) = &object.kind {
                            object.kind = ObjectKind::Shape(changed(shape, setting, value));
                        }
                    });
                }
            });
        }
        this
    }

    /// The group, to put in the Inspector.
    pub fn group(&self) -> &compact::Group {
        &self.group
    }

    /// Shows the settings of the selected shape, or hides the group when
    /// it has none.
    pub fn show(&self, info: Option<&ObjectInfo>) {
        let shown = match info.map(|i| &i.object.kind) {
            Some(ObjectKind::Shape(shape)) => settings(shape),
            _ => Vec::new(),
        };
        self.group.set_visible(!shown.is_empty());
        let Some(info) = info else { return };
        self.group
            .set_sensitive(info.count == 1 && !info.object.locked);
        self.updating.set(true);
        for (setting, row) in &self.rows {
            let value = shown.iter().find(|(s, _)| s == setting).map(|(_, v)| *v);
            row.set_visible(value.is_some());
            if let Some(value) = value
                && (row.value() - value).abs() > 0.001
            {
                row.set_value(value);
            }
        }
        self.updating.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_11_the_rows_change_the_settings_of_the_shape() {
        let star = Shape::star();
        assert_eq!(
            changed(&star, Setting::Points, 8.4),
            Shape::Star {
                points: 8,
                inner: 3_820
            }
        );
        assert_eq!(
            changed(&star, Setting::Inner, 50.0),
            Shape::Star {
                points: 5,
                inner: 5_000
            }
        );
        // Settings of other shapes leave it alone.
        assert_eq!(changed(&star, Setting::Sides, 9.0), star);
        let balloon = Shape::balloon(true);
        let longer = changed(&balloon, Setting::TailLength, 40.0);
        assert!(matches!(longer, Shape::Balloon { body: 6_000, .. }));
        assert_eq!(
            settings(&longer)
                .iter()
                .find(|(s, _)| *s == Setting::TailLength)
                .map(|(_, v)| v.round()),
            Some(40.0)
        );
        assert!(settings(&Shape::Ellipse).is_empty());
    }
}
