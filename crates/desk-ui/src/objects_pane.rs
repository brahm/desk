// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Metrics and Wrap inspectors of Pages, for the selected objects
//! (OBJ-23, OBJ-29, OBJ-33, OBJ-34, OBJ-17): where the object is and how big, how far it
//! is rotated and whether it is flipped; how the text goes around it, and
//! whether it is locked.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Frame, Length, VerticalAlignment, Wrap};

use crate::editing::{Format, ObjectInfo};
use crate::formatting::EditorOf;
use crate::i18n::tr;
use crate::preferences::Units;
use crate::tab_stops;

/// The wraps, in the order of the list.
const WRAPS: [Wrap; 7] = [
    Wrap::Both,
    Wrap::Largest,
    Wrap::Left,
    Wrap::Right,
    Wrap::AboveAndBelow,
    Wrap::Through,
    Wrap::Behind,
];

/// The largest position or size of an object, in points.
const LARGEST: f64 = 10_000.0;

/// The Metrics and Wrap groups of the Inspector.
pub struct ObjectGroups {
    metrics: adw::PreferencesGroup,
    wrap_group: adw::PreferencesGroup,
    /// The top left corner of the box that holds the object as it is
    /// turned (OBJ-29).
    x: adw::SpinRow,
    y: adw::SpinRow,
    /// Where a line starts and ends on the page, in place of its box.
    ends: [adw::SpinRow; 4],
    width: adw::SpinRow,
    height: adw::SpinRow,
    keep_proportions: adw::SwitchRow,
    rotation: adw::SpinRow,
    wrap: adw::ComboRow,
    /// Whether the text goes around the outline of the object rather
    /// than its box (OBJ-17).
    tight: adw::SwitchRow,
    margin: adw::SpinRow,
    locked: adw::SwitchRow,
    /// Whether the object is in the line of the text (OBJ-01).
    inline: adw::SwitchRow,
    /// The room between the frame of a text box and its text (OBJ-13).
    inset: adw::SpinRow,
    /// Where the text of a text box sits, and its columns.
    text_group: adw::PreferencesGroup,
    vertical: adw::ComboRow,
    columns: adw::SpinRow,
    /// What fills the objects, their lines and their shadows.
    graphic: Rc<crate::graphic_pane::GraphicGroup>,
    /// The mask, Instant Alpha and adjustments of a picture.
    picture: Rc<crate::picture_pane::PictureGroup>,
    /// The corners, points, sides, heads and tails of a shape.
    shape: Rc<crate::shape_pane::ShapeGroup>,
    /// How a movie or a sound plays (OBJ-19).
    media: Rc<crate::media_pane::MediaGroup>,
    units: Cell<Units>,
    updating: Cell<bool>,
}

/// What a row of the Metrics inspector sets.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Metric {
    X,
    Y,
    Width,
    Height,
}

/// The smallest upright box that holds a frame turned by `rotation`
/// hundredths of a degree about its center, as Pages measures where an
/// object is (OBJ-29).
pub(crate) fn bounding_box(frame: (f64, f64, f64, f64), rotation: i32) -> (f64, f64, f64, f64) {
    let (x, y, w, h) = frame;
    let (sin, cos) = (f64::from(rotation) / 100.0).to_radians().sin_cos();
    let bw = (w * cos).abs() + (h * sin).abs();
    let bh = (w * sin).abs() + (h * cos).abs();
    (x + (w - bw) / 2.0, y + (h - bh) / 2.0, bw, bh)
}

/// A point of a frame, from its top left corner as if it were not turned,
/// on the page, as `desk_layout::PlacedObject::to_page` puts it.
fn frame_to_page(frame: (f64, f64, f64, f64), rotation: i32, p: (f64, f64)) -> (f64, f64) {
    let (x, y, w, h) = frame;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let (sin, cos) = (f64::from(rotation) / 100.0).to_radians().sin_cos();
    let (dx, dy) = (p.0 - w / 2.0, p.1 - h / 2.0);
    (cx + dx * cos + dy * sin, cy - dx * sin + dy * cos)
}

/// The start and the end of a line on the page, with its flips and its
/// rotation, or `None` when the object is no line.
pub(crate) fn line_ends(
    object: &desk_model::Object,
    frame: (f64, f64, f64, f64),
) -> Option<((f64, f64), (f64, f64))> {
    let (_, _, w, h) = frame;
    let (start, end) = object.line_ends_in(w, h)?;
    Some((
        frame_to_page(frame, object.rotation, start),
        frame_to_page(frame, object.rotation, end),
    ))
}

/// Makes a line go from `start` to `end` on the page, keeping how far it
/// is turned and which end has which arrow: the start stays the start,
/// as in Pages, even when it lies right of the end (OBJ-29).
pub(crate) fn set_line_ends(object: &mut desk_model::Object, start: (f64, f64), end: (f64, f64)) {
    let (sin, cos) = (f64::from(object.rotation) / 100.0).to_radians().sin_cos();
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    // In the axes of the frame, which turn with it.
    let (fx, fy) = (dx * cos - dy * sin, dx * sin + dy * cos);
    let (w, h) = (fx.abs(), fy.abs());
    let (cx, cy) = ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0);
    object.set_line_direction(fx, fy);
    object.frame = Frame::from_pt(cx - w / 2.0, cy - h / 2.0, w, h);
}

impl ObjectGroups {
    pub fn new(editor: EditorOf, units: Units) -> Rc<Self> {
        let spin = |title: &str, min: f64, max: f64| {
            let row = adw::SpinRow::with_range(min, max, 1.0);
            row.set_title(title);
            row
        };
        let x = spin(&tr("X"), -LARGEST, LARGEST);
        let y = spin(&tr("Y"), -LARGEST, LARGEST);
        let width = spin(&tr("Width"), 0.0, LARGEST);
        let height = spin(&tr("Height"), 0.0, LARGEST);
        let ends = [
            spin(&tr("Start X"), -LARGEST, LARGEST),
            spin(&tr("Start Y"), -LARGEST, LARGEST),
            spin(&tr("End X"), -LARGEST, LARGEST),
            spin(&tr("End Y"), -LARGEST, LARGEST),
        ];
        let keep_proportions = adw::SwitchRow::builder()
            .title(tr("Constrain Proportions"))
            .build();
        let rotation = spin(&tr("Rotate"), 0.0, 359.0);
        rotation.set_subtitle(&tr("Degrees, counterclockwise"));
        rotation.set_wrap(true);
        let flips = gtk::Box::builder()
            .spacing(6)
            .valign(gtk::Align::Center)
            .build();
        for (icon, label, action) in [
            (
                "object-flip-horizontal-symbolic",
                tr("Flip Horizontally"),
                "win.flip-horizontal",
            ),
            (
                "object-flip-vertical-symbolic",
                tr("Flip Vertically"),
                "win.flip-vertical",
            ),
        ] {
            let button = gtk::Button::builder()
                .icon_name(icon)
                .tooltip_text(&label)
                .action_name(action)
                .css_classes(["flat"])
                .build();
            crate::formatting::labelled(&button, &label);
            flips.append(&button);
        }
        let flip = adw::ActionRow::builder().title(tr("Flip")).build();
        flip.add_suffix(&flips);
        let metrics = adw::PreferencesGroup::builder()
            .title(tr("Metrics"))
            .build();
        for row in [&x, &y].into_iter().chain(&ends).chain([&width, &height]) {
            metrics.add(row);
        }
        metrics.add(&keep_proportions);
        metrics.add(&rotation);
        metrics.add(&flip);

        let names: Vec<String> = [
            tr("Both Sides"),
            tr("Largest Side"),
            tr("Left Side"),
            tr("Right Side"),
            tr("Above and Below"),
            tr("In Front of the Text"),
            tr("Behind the Text"),
        ]
        .into();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let wrap = adw::ComboRow::builder()
            .title(tr("Text Wrap"))
            .model(&gtk::StringList::new(&names))
            .build();
        let tight = adw::SwitchRow::builder()
            .title(tr("Tight"))
            .subtitle(tr(
                "Around the outline of a shape or the opaque part of a picture",
            ))
            .build();
        let margin = spin(&tr("Extra Space"), 0.0, 1584.0);
        let locked = adw::SwitchRow::builder()
            .title(tr("Locked"))
            .subtitle(tr("Cannot be moved, resized or deleted"))
            .build();
        let inset = spin(&tr("Inset"), 0.0, 1584.0);
        let inline = adw::SwitchRow::builder()
            .title(tr("Inline with Text"))
            .subtitle(tr("Moves with the text, as a character"))
            .build();
        let wrap_group = adw::PreferencesGroup::builder().title(tr("Wrap")).build();
        wrap_group.add(&inline);
        wrap_group.add(&wrap);
        wrap_group.add(&tight);
        wrap_group.add(&margin);
        wrap_group.add(&locked);
        let vertical_names = [tr("Top"), tr("Middle"), tr("Bottom")];
        let vertical_names: Vec<&str> = vertical_names.iter().map(String::as_str).collect();
        let vertical = adw::ComboRow::builder()
            .title(tr("Vertical Alignment"))
            .model(&gtk::StringList::new(&vertical_names))
            .build();
        let columns =
            adw::SpinRow::with_range(1.0, f64::from(desk_model::MAX_TEXT_BOX_COLUMNS), 1.0);
        columns.set_title(&tr("Columns"));
        let text_group = adw::PreferencesGroup::builder()
            .title(tr("Text Box"))
            .build();
        text_group.add(&inset);
        text_group.add(&vertical);
        text_group.add(&columns);

        let this = Rc::new(Self {
            metrics,
            wrap_group,
            x,
            y,
            ends,
            width,
            height,
            keep_proportions,
            rotation,
            wrap,
            tight,
            margin,
            locked,
            inline,
            inset,
            text_group,
            vertical,
            columns,
            graphic: crate::graphic_pane::GraphicGroup::new(editor.clone()),
            picture: crate::picture_pane::PictureGroup::new(editor.clone()),
            shape: crate::shape_pane::ShapeGroup::new(editor.clone()),
            media: crate::media_pane::MediaGroup::new(editor.clone()),
            units: Cell::new(units),
            updating: Cell::new(false),
        });
        this.set_units(units);
        this.show(None);
        for (row, metric) in [
            (&this.x, Metric::X),
            (&this.y, Metric::Y),
            (&this.width, Metric::Width),
            (&this.height, Metric::Height),
        ] {
            let weak = Rc::downgrade(&this);
            let editor = editor.clone();
            row.connect_value_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let Some(editor) = editor() else { return };
                let Some(info) = editor.object_info() else {
                    return;
                };
                let frame = this.frame(&info, metric, row.value());
                let name = match metric {
                    Metric::X | Metric::Y => "Move",
                    Metric::Width | Metric::Height => "Resize",
                };
                editor.set_object(name, |object| object.frame = frame);
            });
        }
        for i in 0..4 {
            let weak = Rc::downgrade(&this);
            let editor = editor.clone();
            this.ends[i].connect_value_notify(move |_| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let Some(editor) = editor() else { return };
                let units = this.units.get();
                let points = |row: &adw::SpinRow| tab_stops::from_units(row.value(), units).to_pt();
                let [sx, sy, ex, ey] = this.ends.each_ref().map(points);
                editor.set_object("Move", |object| {
                    set_line_ends(object, (sx, sy), (ex, ey));
                });
            });
        }
        let weak = Rc::downgrade(&this);
        let rotate_editor = editor.clone();
        this.rotation.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = rotate_editor() {
                let rotation = (row.value() * 100.0).round() as i32;
                editor.set_object("Rotate", |object| object.rotation = rotation);
            }
        });
        let weak = Rc::downgrade(&this);
        let wrap_editor = editor.clone();
        this.wrap.connect_selected_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let Some(&wrap) = WRAPS.get(row.selected() as usize) else {
                return;
            };
            if let Some(editor) = wrap_editor() {
                editor.set_object("Text Wrap", |object| object.wrap = wrap);
            }
        });
        let weak = Rc::downgrade(&this);
        let tight_editor = editor.clone();
        this.tight.connect_active_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let tight = row.is_active();
            if let Some(editor) = tight_editor() {
                editor.set_object("Text Wrap", |object| {
                    object.contour = if tight {
                        desk_render::object_contour(object)
                    } else {
                        None
                    };
                });
            }
        });
        let weak = Rc::downgrade(&this);
        let margin_editor = editor.clone();
        this.margin.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let margin = tab_stops::from_units(row.value(), this.units.get());
            if let Some(editor) = margin_editor() {
                editor.set_object("Text Wrap", |object| object.wrap_margin = margin);
            }
        });
        let weak = Rc::downgrade(&this);
        let inset_editor = editor.clone();
        this.inset.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let inset = tab_stops::from_units(row.value(), this.units.get());
            if let Some(editor) = inset_editor() {
                editor.set_object("Inset", |object| {
                    if let Some(text) = text_of(object) {
                        text.inset = inset;
                    }
                });
            }
        });
        let weak = Rc::downgrade(&this);
        let vertical_editor = editor.clone();
        this.vertical.connect_selected_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let vertical = match row.selected() {
                1 => VerticalAlignment::Middle,
                2 => VerticalAlignment::Bottom,
                _ => VerticalAlignment::Top,
            };
            if let Some(editor) = vertical_editor() {
                editor.set_object("Vertical Alignment", |object| {
                    if let Some(text) = text_of(object) {
                        text.vertical = vertical;
                    }
                });
            }
        });
        let weak = Rc::downgrade(&this);
        let columns_editor = editor.clone();
        this.columns.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let columns = row.value().round() as u8;
            if let Some(editor) = columns_editor() {
                editor.set_object("Columns", |object| {
                    if let Some(text) = text_of(object) {
                        text.columns = columns;
                    }
                });
            }
        });
        let weak = Rc::downgrade(&this);
        let inline_editor = editor.clone();
        this.inline.connect_active_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = inline_editor() {
                editor.set_objects_inline(row.is_active());
            }
        });
        let weak = Rc::downgrade(&this);
        this.locked.connect_active_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = editor() {
                editor.lock_objects(row.is_active());
            }
        });
        this
    }

    /// The groups, to put at the top of the Inspector.
    pub fn groups(&self) -> [&adw::PreferencesGroup; 7] {
        [
            &self.metrics,
            self.shape.group(),
            self.picture.group(),
            self.media.group(),
            self.graphic.group(),
            &self.wrap_group,
            &self.text_group,
        ]
    }

    /// The frame on the page that a row asks for, with the other side
    /// keeping the proportions when they are constrained.
    fn frame(&self, info: &ObjectInfo, metric: Metric, value: f64) -> Frame {
        let points = tab_stops::from_units(value, self.units.get()).to_pt();
        let (mut x, mut y, mut w, mut h) = info.frame;
        let keep = self.keep_proportions.is_active();
        // X and Y place the box that holds the turned object.
        let (bx, by, _, _) = bounding_box(info.frame, info.object.rotation);
        match metric {
            Metric::X => x += points - bx,
            Metric::Y => y += points - by,
            Metric::Width => {
                if keep && w > 0.0 {
                    h *= points / w;
                }
                w = points;
            }
            Metric::Height => {
                if keep && h > 0.0 {
                    w *= points / h;
                }
                h = points;
            }
        }
        Frame::from_pt(x, y, w, h)
    }

    /// Measures the rows in other units.
    pub fn set_units(&self, units: Units) {
        self.updating.set(true);
        self.units.set(units);
        for row in [&self.x, &self.y].into_iter().chain(&self.ends) {
            tab_stops::set_units(row, units, -LARGEST, LARGEST);
        }
        for row in [&self.width, &self.height] {
            tab_stops::set_units(row, units, 0.0, LARGEST);
        }
        tab_stops::set_units(&self.margin, units, 0.0, 1584.0);
        tab_stops::set_units(&self.inset, units, 0.0, 1584.0);
        self.updating.set(false);
    }

    /// Shows the selected objects, or hides the groups when none is.
    pub fn show(&self, info: Option<&ObjectInfo>) {
        self.metrics.set_visible(info.is_some());
        self.wrap_group.set_visible(info.is_some());
        self.text_group.set_visible(false);
        self.graphic.show(info);
        self.picture.show(info);
        self.shape.show(info);
        self.media.show(info);
        let Some(info) = info else {
            return;
        };
        self.updating.set(true);
        let units = self.units.get();
        let set = |row: &adw::SpinRow, value: f64| {
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        };
        let (_, _, w, h) = info.frame;
        let (x, y, _, _) = bounding_box(info.frame, info.object.rotation);
        let in_units = |points: f64| tab_stops::in_units(Length::from_pt(points), units);
        set(&self.x, in_units(x));
        set(&self.y, in_units(y));
        let ends = line_ends(&info.object, info.frame);
        for row in &self.ends {
            row.set_visible(ends.is_some());
        }
        if let Some(((sx, sy), (ex, ey))) = ends {
            for (row, value) in self.ends.iter().zip([sx, sy, ex, ey]) {
                set(row, in_units(value));
            }
        }
        set(&self.width, in_units(w));
        set(&self.height, in_units(h));
        set(&self.rotation, f64::from(info.object.rotation) / 100.0);
        let one = info.count == 1;
        let editable = one && !info.object.locked;
        for row in [&self.x, &self.y, &self.width, &self.height, &self.rotation]
            .into_iter()
            .chain(&self.ends)
        {
            row.set_sensitive(editable);
        }
        self.keep_proportions.set_sensitive(editable);
        let index = WRAPS
            .iter()
            .position(|w| *w == info.object.wrap)
            .unwrap_or(0) as u32;
        if self.wrap.selected() != index {
            self.wrap.set_selected(index);
        }
        set(
            &self.margin,
            tab_stops::in_units(info.object.wrap_margin, units),
        );
        // An object in the text stands where the text puts it, and the
        // text does not go around it.
        let inline = info.object.anchor == desk_model::Anchor::InText;
        if self.inline.is_active() != inline {
            self.inline.set_active(inline);
        }
        self.inline.set_sensitive(editable);
        self.x.set_sensitive(editable && !inline);
        self.y.set_sensitive(editable && !inline);
        self.wrap.set_sensitive(editable && !inline);
        // Only a shape with an inside or a picture has an outline, and only
        // text that goes beside it follows it.
        let outlined = match &info.object.kind {
            desk_model::ObjectKind::Shape(shape) => !shape.is_line(),
            desk_model::ObjectKind::Image(_) => true,
            _ => false,
        };
        let beside = matches!(
            info.object.wrap,
            Wrap::Both | Wrap::Largest | Wrap::Left | Wrap::Right
        );
        let tight = info.object.contour.is_some();
        if self.tight.is_active() != tight {
            self.tight.set_active(tight);
        }
        self.tight.set_visible(outlined);
        self.tight.set_sensitive(editable && !inline && beside);
        self.margin.set_sensitive(editable && !inline);
        let shape_text = desk_model::TextBox::in_shape(std::sync::Arc::from([]));
        let text = info
            .object
            .text_box()
            .or_else(|| info.object.holds_text().then_some(&shape_text));
        match text {
            Some(text) => {
                self.text_group.set_visible(true);
                self.text_group.set_sensitive(editable);
                set(&self.inset, tab_stops::in_units(text.inset, units));
                let index = match text.vertical {
                    VerticalAlignment::Top => 0,
                    VerticalAlignment::Middle => 1,
                    VerticalAlignment::Bottom => 2,
                };
                if self.vertical.selected() != index {
                    self.vertical.set_selected(index);
                }
                set(&self.columns, f64::from(text.columns));
            }
            None => self.text_group.set_visible(false),
        }
        if self.locked.is_active() != info.object.locked {
            self.locked.set_active(info.object.locked);
        }
        self.updating.set(false);
    }

    /// Shows what the selection has.
    pub fn show_format(&self, format: &Format) {
        self.show(format.object.as_ref());
    }
}

/// The text of a text box or a shape, set up for a shape that has none
/// yet, to change how it is laid out.
fn text_of(object: &mut desk_model::Object) -> Option<&mut desk_model::TextBox> {
    if object.text_box().is_none() && object.holds_text() {
        object.text = Some(desk_model::TextBox::in_shape(std::sync::Arc::from([])));
    }
    object.text_box_mut()
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{LineEnd, Object, ObjectKind, Shape, Stroke};

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
    }

    #[test]
    fn obj_29_x_and_y_place_the_box_of_a_turned_object() {
        // Upright, the box is the frame.
        let frame = (100.0, 50.0, 80.0, 40.0);
        assert_eq!(bounding_box(frame, 0), frame);
        // A quarter turn about the center: 40 wide and 80 tall.
        let (x, y, w, h) = bounding_box(frame, 9000);
        assert!(close((x, y), (120.0, 30.0)) && close((w, h), (40.0, 80.0)));
        // An eighth of a turn: a square of the two sides over the root of 2.
        let (x, _, w, h) = bounding_box(frame, 4500);
        let side = 120.0 / 2f64.sqrt();
        assert!(close((w, h), (side, side)));
        assert!((x - (140.0 - side / 2.0)).abs() < 1e-6);
    }

    #[test]
    fn obj_29_lines_have_a_start_and_an_end() {
        let mut line = Object::new(
            desk_model::NodeId::new(1).unwrap(),
            ObjectKind::Shape(Shape::Line { rising: true }),
            Frame::from_pt(10.0, 20.0, 100.0, 50.0),
        );
        line.style.stroke = Some(Stroke {
            end: LineEnd::Arrow,
            ..Stroke::new(desk_model::Rgba::BLACK, Length::from_points(1))
        });
        let frame = (10.0, 20.0, 100.0, 50.0);
        let (start, end) = line_ends(&line, frame).unwrap();
        assert!(close(start, (10.0, 70.0)) && close(end, (110.0, 20.0)));
        // Flipped left to right, it starts at the top right.
        line.flip_horizontal = true;
        let (start, end) = line_ends(&line, frame).unwrap();
        assert!(close(start, (110.0, 70.0)) && close(end, (10.0, 20.0)));
        line.flip_horizontal = false;

        // Ends typed in move the line. Typed right to left, the start
        // stays the start, with its arrow, and the line flips across.
        set_line_ends(&mut line, (200.0, 100.0), (50.0, 40.0));
        let frame = (
            line.frame.x.to_pt(),
            line.frame.y.to_pt(),
            line.frame.width.to_pt(),
            line.frame.height.to_pt(),
        );
        let (start, end) = line_ends(&line, frame).unwrap();
        assert!(close(start, (200.0, 100.0)), "{start:?}");
        assert!(close(end, (50.0, 40.0)), "{end:?}");
        assert!(line.flip_horizontal);
        let stroke = line.style.stroke.as_ref().unwrap();
        assert_eq!((stroke.start, stroke.end), (LineEnd::None, LineEnd::Arrow));
        // So does a turned line, whose frame turns with it.
        line.rotation = 3000;
        set_line_ends(&mut line, (0.0, 0.0), (30.0, 80.0));
        let frame = (
            line.frame.x.to_pt(),
            line.frame.y.to_pt(),
            line.frame.width.to_pt(),
            line.frame.height.to_pt(),
        );
        let (start, end) = line_ends(&line, frame).unwrap();
        // The model keeps lengths to a thousandth of a point or so.
        let near =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01;
        assert!(
            near(start, (0.0, 0.0)) && near(end, (30.0, 80.0)),
            "{start:?} {end:?}"
        );
    }
}
