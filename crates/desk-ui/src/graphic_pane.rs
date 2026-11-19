// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Graphic inspector of Pages, for the selected objects (OBJ-09,
//! OBJ-10, OBJ-11, OBJ-12): what fills them, the line around them and at
//! the ends of a line, their shadow and reflection and how opaque they
//! are.

use crate::compact;
use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{
    BorderStyle, FrameStyle, GraphicStyle, Length, LineEnd, ObjectKind, ObjectShadow, PictureFrame,
    Rgba, Stroke,
};

use crate::editing::ObjectInfo;
use crate::fill_rows::{FillRows, Kind, color_row, combo, spin};
use crate::formatting::{EditorOf, from_gdk, to_gdk};
use crate::i18n::tr;

/// The ends of a line, in the order of their lists.
const LINE_ENDS: [LineEnd; 6] = [
    LineEnd::None,
    LineEnd::Arrow,
    LineEnd::OpenArrow,
    LineEnd::Circle,
    LineEnd::Square,
    LineEnd::Bar,
];

/// The styles of a line, after "None" in its list.
const STROKES: [BorderStyle; 3] = [BorderStyle::Solid, BorderStyle::Dashed, BorderStyle::Dotted];

/// The name of a picture frame of the library (OBJ-36).
fn frame_style_name(style: FrameStyle) -> String {
    match style {
        FrameStyle::Photo => tr("Photo"),
        FrameStyle::Gallery => tr("Gallery"),
        FrameStyle::Classic => tr("Classic"),
        FrameStyle::Metal => tr("Metal"),
        FrameStyle::Tape => tr("Tape"),
    }
}

/// The color a fill or a line starts with when it is turned on.
const BLUE: Rgba = Rgba::rgb(0x46, 0x82, 0xb4);

/// The Graphic group of the Inspector.
pub struct GraphicGroup {
    group: compact::Group,
    fill: Rc<FillRows>,
    stroke: compact::ComboRow,
    stroke_color: gtk::ColorDialogButton,
    stroke_color_row: compact::ActionRow,
    stroke_width: compact::SpinRow,
    /// The picture frame, which the Stroke list offers after the lines
    /// (OBJ-36).
    frame_style: compact::ComboRow,
    frame_scale: compact::SpinRow,
    start: compact::ComboRow,
    end: compact::ComboRow,
    shadow: compact::SwitchRow,
    shadow_color: gtk::ColorDialogButton,
    shadow_color_row: compact::ActionRow,
    shadow_angle: compact::SpinRow,
    shadow_offset: compact::SpinRow,
    shadow_blur: compact::SpinRow,
    shadow_opacity: compact::SpinRow,
    reflection: compact::SwitchRow,
    reflection_opacity: compact::SpinRow,
    opacity: compact::SpinRow,
    updating: Cell<bool>,
}

/// The name of an end of a line, as its list shows it.
fn line_end_name(end: LineEnd) -> String {
    match end {
        LineEnd::None => tr("None"),
        LineEnd::Arrow => tr("Arrow"),
        LineEnd::OpenArrow => tr("Open Arrow"),
        LineEnd::Circle => tr("Circle"),
        LineEnd::Square => tr("Square"),
        LineEnd::Bar => tr("Bar"),
    }
}

/// Whether an object is a line, which has ends and no fill.
fn is_line(kind: &ObjectKind) -> bool {
    matches!(kind, ObjectKind::Shape(shape) if shape.is_line())
}

impl GraphicGroup {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let fill = FillRows::new(
            &[
                Kind::Plain,
                Kind::Color,
                Kind::Gradient,
                Kind::Image,
                // A picture with a color over it (OBJ-42).
                Kind::TintedImage,
                // Several colors, along a line or out from a point
                // (OBJ-41).
                Kind::Advanced,
            ],
            &tr("None"),
        );
        let stroke = combo(
            &tr("Stroke"),
            &[
                tr("None"),
                tr("Solid"),
                tr("Dashed"),
                tr("Dotted"),
                tr("Picture Frame"),
            ],
        );
        let frames: Vec<String> = FrameStyle::ALL
            .iter()
            .map(|f| frame_style_name(*f))
            .collect();
        let frame_style = combo(&tr("Frame"), &frames);
        let frame_scale = spin(&tr("Frame Thickness"), 10.0, 400.0, 5.0);
        frame_scale.set_subtitle(&tr("Percent"));
        let (stroke_color_row, stroke_color, stroke_colors) = color_row(&tr("Stroke Color"));
        let stroke_width = spin(&tr("Stroke Width"), 0.5, 100.0, 0.5);
        stroke_width.set_digits(1);
        stroke_width.set_subtitle(&tr("Points"));
        let ends: Vec<String> = LINE_ENDS.iter().map(|e| line_end_name(*e)).collect();
        let start = combo(&tr("Start of the Line"), &ends);
        let end = combo(&tr("End of the Line"), &ends);
        let shadow = compact::SwitchRow::builder().title(tr("Shadow")).build();
        let (shadow_color_row, shadow_color, shadow_colors) = color_row(&tr("Shadow Color"));
        let shadow_angle = spin(&tr("Shadow Angle"), 0.0, 359.0, 1.0);
        shadow_angle.set_subtitle(&tr("Degrees, counterclockwise"));
        let shadow_offset = spin(&tr("Shadow Offset"), 0.0, 100.0, 1.0);
        shadow_offset.set_subtitle(&tr("Points"));
        let shadow_blur = spin(&tr("Shadow Blur"), 0.0, 100.0, 1.0);
        shadow_blur.set_subtitle(&tr("Points"));
        let shadow_opacity = spin(&tr("Shadow Opacity"), 0.0, 100.0, 1.0);
        shadow_opacity.set_subtitle(&tr("Percent"));
        // A reflection below the object, as in Pages (OBJ-38).
        let reflection = compact::SwitchRow::builder()
            .title(tr("Reflection"))
            .build();
        let reflection_opacity = spin(&tr("Reflection Opacity"), 0.0, 100.0, 1.0);
        reflection_opacity.set_subtitle(&tr("Percent"));
        let opacity = spin(&tr("Opacity"), 0.0, 100.0, 1.0);
        opacity.set_subtitle(&tr("Percent"));
        let group = compact::Group::builder().title(tr("Graphic")).build();
        for row in fill.rows() {
            group.add(row);
        }
        for row in [
            stroke.upcast_ref(),
            stroke_color_row.upcast_ref(),
            stroke_width.upcast_ref(),
            frame_style.upcast_ref(),
            frame_scale.upcast_ref(),
            start.upcast_ref(),
            end.upcast_ref(),
            shadow.upcast_ref(),
            shadow_color_row.upcast_ref(),
            shadow_angle.upcast_ref(),
            shadow_offset.upcast_ref(),
            shadow_blur.upcast_ref(),
            shadow_opacity.upcast_ref(),
            reflection.upcast_ref(),
            reflection_opacity.upcast_ref(),
            opacity.upcast_ref(),
        ] {
            group.add(row);
        }
        let this = Rc::new(Self {
            group,
            fill,
            stroke,
            stroke_color,
            stroke_color_row,
            stroke_width,
            frame_style,
            frame_scale,
            start,
            end,
            shadow,
            shadow_color,
            shadow_color_row,
            shadow_angle,
            shadow_offset,
            shadow_blur,
            shadow_opacity,
            reflection,
            reflection_opacity,
            opacity,
            updating: Cell::new(false),
        });
        this.show(None);

        // Every row sets the part of the style it shows, from all the rows
        // of that part.
        let changed = {
            let weak = Rc::downgrade(&this);
            move |name: &'static str, part: Part| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let Some(editor) = editor() else { return };
                let fill = this.fill.value();
                let stroke = this.stroke_value();
                let frame = this.frame_value();
                let shadow = this.shadow_value();
                let reflection = this
                    .reflection
                    .is_active()
                    .then(|| this.reflection_opacity.value().round().clamp(0.0, 100.0) as u8);
                let opacity = this.opacity.value().round().clamp(0.0, 100.0) as u8;
                editor.style_objects(name, move |style, line| match part {
                    Part::Fill => {
                        if !line {
                            style.fill.clone_from(&fill);
                        }
                    }
                    Part::Stroke => {
                        // A line has no frame; the line of another
                        // object stays, for when its frame goes.
                        style.frame = frame.filter(|_| !line);
                        if frame.is_some() {
                            return;
                        }
                        style.stroke = stroke.map(|mut s| {
                            if !line {
                                s.start = LineEnd::None;
                                s.end = LineEnd::None;
                            }
                            s
                        });
                    }
                    Part::Shadow => style.shadow = shadow,
                    Part::Reflection => style.reflection = reflection,
                    Part::Opacity => style.opacity = opacity,
                });
                this.show_rows();
            }
        };
        // None in the Colors popovers takes the fill, the line or the
        // shadow away, as None in their lists and the Shadow switch do; on
        // a tinted picture it takes the tint away and keeps the picture
        // (B-026).
        let none = |f: fn(&Self)| {
            let weak = Rc::downgrade(&this);
            move || {
                if let Some(this) = weak.upgrade() {
                    f(&this);
                }
            }
        };
        crate::color_popover::add_none(&stroke_colors, none(|this| this.stroke.set_selected(0)));
        crate::color_popover::add_none(&shadow_colors, none(|this| this.shadow.set_active(false)));
        let on = changed.clone();
        this.fill.connect_changed(move || on("Fill", Part::Fill));
        let on = changed.clone();
        this.frame_scale
            .connect_value_notify(move |_| on("Stroke", Part::Stroke));
        for row in [&this.stroke, &this.start, &this.end, &this.frame_style] {
            let on = changed.clone();
            row.connect_selected_notify(move |_| on("Stroke", Part::Stroke));
        }
        let on = changed.clone();
        this.stroke_color
            .connect_rgba_notify(move |_| on("Stroke", Part::Stroke));
        let on = changed.clone();
        this.stroke_width
            .connect_value_notify(move |_| on("Stroke", Part::Stroke));
        let on = changed.clone();
        this.shadow
            .connect_active_notify(move |_| on("Shadow", Part::Shadow));
        let on = changed.clone();
        this.shadow_color
            .connect_rgba_notify(move |_| on("Shadow", Part::Shadow));
        for row in [
            &this.shadow_angle,
            &this.shadow_offset,
            &this.shadow_blur,
            &this.shadow_opacity,
        ] {
            let on = changed.clone();
            row.connect_value_notify(move |_| on("Shadow", Part::Shadow));
        }
        let on = changed.clone();
        this.reflection
            .connect_active_notify(move |_| on("Reflection", Part::Reflection));
        let on = changed.clone();
        this.reflection_opacity
            .connect_value_notify(move |_| on("Reflection", Part::Reflection));
        let on = changed;
        this.opacity
            .connect_value_notify(move |_| on("Opacity", Part::Opacity));
        this
    }

    /// The group, to put in the Inspector.
    pub fn group(&self) -> &compact::Group {
        &self.group
    }

    /// The line that the rows ask for.
    fn stroke_value(&self) -> Option<Stroke> {
        let selected = self.stroke.selected() as usize;
        let style = *STROKES.get(selected.checked_sub(1)?)?;
        let end = |row: &compact::ComboRow| {
            LINE_ENDS
                .get(row.selected() as usize)
                .copied()
                .unwrap_or_default()
        };
        Some(Stroke {
            style,
            start: end(&self.start),
            end: end(&self.end),
            ..Stroke::new(
                from_gdk(&self.stroke_color.rgba()),
                Length::from_pt(self.stroke_width.value()),
            )
        })
    }

    /// The picture frame that the rows ask for.
    fn frame_value(&self) -> Option<PictureFrame> {
        (self.stroke.selected() == 4).then(|| PictureFrame {
            style: FrameStyle::ALL
                .get(self.frame_style.selected() as usize)
                .copied()
                .unwrap_or_default(),
            scale: self.frame_scale.value().round().clamp(10.0, 400.0) as u16,
        })
    }

    /// The shadow that the rows ask for.
    fn shadow_value(&self) -> Option<ObjectShadow> {
        self.shadow.is_active().then(|| ObjectShadow {
            color: from_gdk(&self.shadow_color.rgba()),
            angle: (self.shadow_angle.value() * 100.0).round() as i32,
            offset: Length::from_pt(self.shadow_offset.value()),
            blur: Length::from_pt(self.shadow_blur.value()),
            opacity: self.shadow_opacity.value().round().clamp(0.0, 100.0) as u8,
        })
    }

    /// Shows the rows that the choices of the others call for.
    fn show_rows(&self) {
        let framed = self.stroke.selected() == 4;
        let stroked = self.stroke.selected() > 0 && !framed;
        self.stroke_color_row.set_visible(stroked);
        self.stroke_width.set_visible(stroked);
        self.frame_style.set_visible(framed);
        self.frame_scale.set_visible(framed);
        let line = !self.fill.is_shown();
        self.start.set_visible(stroked && line);
        self.end.set_visible(stroked && line);
        let shadow = self.shadow.is_active();
        for row in [
            self.shadow_angle.upcast_ref(),
            self.shadow_offset.upcast_ref(),
            self.shadow_blur.upcast_ref(),
            self.shadow_opacity.upcast_ref(),
            self.shadow_color_row.upcast_ref(),
        ] {
            row.set_visible(shadow);
        }
        self.reflection_opacity
            .set_visible(self.reflection.is_active());
    }

    /// Shows the style of the selected objects, or hides the group when
    /// none is.
    pub fn show(&self, info: Option<&ObjectInfo>) {
        let Some(info) = info else {
            self.group.set_visible(false);
            return;
        };
        self.group.set_visible(true);
        self.group.set_sensitive(!info.object.locked);
        self.updating.set(true);
        let style: &GraphicStyle = &info.object.style;
        let line = is_line(&info.object.kind);
        self.fill.set_shown(!line);
        let set_color = |button: &gtk::ColorDialogButton, color: Rgba| {
            let color = to_gdk(color);
            if button.rgba() != color {
                button.set_rgba(&color);
            }
        };
        let set = |row: &compact::SpinRow, value: f64| {
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        };
        let select = |row: &compact::ComboRow, index: u32| {
            if row.selected() != index {
                row.set_selected(index);
            }
        };
        self.fill.show(style.fill.as_ref(), BLUE);
        let frame = style.frame.unwrap_or(PictureFrame::new(FrameStyle::Photo));
        let index = FrameStyle::ALL
            .iter()
            .position(|f| *f == frame.style)
            .unwrap_or(0);
        select(&self.frame_style, index as u32);
        set(&self.frame_scale, f64::from(frame.scale));
        match style.stroke {
            _ if style.frame.is_some() && !line => select(&self.stroke, 4),
            Some(stroke) => {
                let index = STROKES.iter().position(|s| *s == stroke.style).unwrap_or(0);
                select(&self.stroke, index as u32 + 1);
                set_color(&self.stroke_color, stroke.color);
                set(&self.stroke_width, stroke.width.to_pt());
                let end = |e: LineEnd| LINE_ENDS.iter().position(|x| *x == e).unwrap_or(0) as u32;
                select(&self.start, end(stroke.start));
                select(&self.end, end(stroke.end));
            }
            None => {
                select(&self.stroke, 0);
                set_color(&self.stroke_color, Rgba::BLACK);
                set(&self.stroke_width, 1.0);
                select(&self.start, 0);
                select(&self.end, 0);
            }
        }
        let shadow = style.shadow.unwrap_or_default();
        if self.shadow.is_active() != style.shadow.is_some() {
            self.shadow.set_active(style.shadow.is_some());
        }
        set_color(&self.shadow_color, shadow.color);
        set(&self.shadow_angle, f64::from(shadow.angle) / 100.0);
        set(&self.shadow_offset, shadow.offset.to_pt());
        set(&self.shadow_blur, shadow.blur.to_pt());
        set(&self.shadow_opacity, f64::from(shadow.opacity));
        if self.reflection.is_active() != style.reflection.is_some() {
            self.reflection.set_active(style.reflection.is_some());
        }
        set(
            &self.reflection_opacity,
            f64::from(style.reflection.unwrap_or(GraphicStyle::REFLECTION)),
        );
        set(&self.opacity, f64::from(style.opacity));
        self.show_rows();
        self.updating.set(false);
    }
}

/// The part of the style that a row sets.
#[derive(Clone, Copy)]
enum Part {
    Fill,
    Stroke,
    Shadow,
    Reflection,
    Opacity,
}
