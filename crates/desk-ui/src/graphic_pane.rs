// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Graphic inspector of Pages, for the selected objects (OBJ-09,
//! OBJ-10, OBJ-11, OBJ-12): what fills them, the line around them and at
//! the ends of a line, their shadow and how opaque they are.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{
    BorderStyle, Fill, GraphicStyle, ImageFill, Length, LineEnd, ObjectKind, ObjectShadow, Picture,
    Rgba, Stroke,
};

use crate::editing::ObjectInfo;
use crate::formatting::{EditorOf, from_gdk, labelled, to_gdk};
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

/// How a picture fills, in the order of its list.
const IMAGE_FILLS: [ImageFill; 5] = [
    ImageFill::ScaleToFill,
    ImageFill::ScaleToFit,
    ImageFill::Stretch,
    ImageFill::OriginalSize,
    ImageFill::Tile,
];

/// The name of a way to fill with a picture, as its list shows it.
fn image_fill_name(mode: ImageFill) -> String {
    match mode {
        ImageFill::ScaleToFill => tr("Scale to Fill"),
        ImageFill::ScaleToFit => tr("Scale to Fit"),
        ImageFill::Stretch => tr("Stretch"),
        ImageFill::OriginalSize => tr("Original Size"),
        ImageFill::Tile => tr("Tile"),
    }
}

/// The styles of a line, after "None" in its list.
const STROKES: [BorderStyle; 3] = [BorderStyle::Solid, BorderStyle::Dashed, BorderStyle::Dotted];

/// The color a fill or a line starts with when it is turned on.
const BLUE: Rgba = Rgba::rgb(0x46, 0x82, 0xb4);

/// The Graphic group of the Inspector.
pub struct GraphicGroup {
    group: adw::PreferencesGroup,
    fill: adw::ComboRow,
    fill_color: gtk::ColorDialogButton,
    fill_color_row: adw::ActionRow,
    image_row: adw::ActionRow,
    image_button: gtk::Button,
    image_mode: adw::ComboRow,
    /// The picture that fills, or that an image fill would fill with.
    picture: RefCell<Option<Picture>>,
    /// Sets the fill after a picture is chosen.
    on_picture: RefCell<Option<Rc<dyn Fn()>>>,
    gradient_end: gtk::ColorDialogButton,
    gradient_end_row: adw::ActionRow,
    gradient_angle: adw::SpinRow,
    stroke: adw::ComboRow,
    stroke_color: gtk::ColorDialogButton,
    stroke_color_row: adw::ActionRow,
    stroke_width: adw::SpinRow,
    start: adw::ComboRow,
    end: adw::ComboRow,
    shadow: adw::SwitchRow,
    shadow_color: gtk::ColorDialogButton,
    shadow_color_row: adw::ActionRow,
    shadow_angle: adw::SpinRow,
    shadow_offset: adw::SpinRow,
    shadow_blur: adw::SpinRow,
    shadow_opacity: adw::SpinRow,
    opacity: adw::SpinRow,
    updating: Cell<bool>,
}

/// A row with a color button at its end.
fn color_row(title: &str) -> (adw::ActionRow, gtk::ColorDialogButton) {
    let button = gtk::ColorDialogButton::builder()
        .dialog(&gtk::ColorDialog::builder().title(title).build())
        .valign(gtk::Align::Center)
        .build();
    labelled(&button, title);
    let row = adw::ActionRow::builder().title(title).build();
    row.add_suffix(&crate::eyedropper::button(&button));
    row.add_suffix(&button);
    (row, button)
}

fn spin(title: &str, min: f64, max: f64, step: f64) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(min, max, step);
    row.set_title(title);
    row
}

fn combo(title: &str, names: &[String]) -> adw::ComboRow {
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(&names))
        .build()
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
        let fill = combo(
            &tr("Fill"),
            &[
                tr("None"),
                tr("Color Fill"),
                tr("Gradient Fill"),
                tr("Image Fill"),
            ],
        );
        let image_button = gtk::Button::builder()
            .label(tr("Choose…"))
            .valign(gtk::Align::Center)
            .build();
        let image_row = adw::ActionRow::builder().title(tr("Image")).build();
        image_row.add_suffix(&image_button);
        let modes: Vec<String> = IMAGE_FILLS.iter().map(|m| image_fill_name(*m)).collect();
        let image_mode = combo(&tr("Image Size"), &modes);
        let (fill_color_row, fill_color) = color_row(&tr("Fill Color"));
        let (gradient_end_row, gradient_end) = color_row(&tr("Gradient End Color"));
        let gradient_angle = spin(&tr("Gradient Angle"), 0.0, 359.0, 1.0);
        gradient_angle.set_subtitle(&tr("Degrees, counterclockwise"));
        let stroke = combo(
            &tr("Stroke"),
            &[tr("None"), tr("Solid"), tr("Dashed"), tr("Dotted")],
        );
        let (stroke_color_row, stroke_color) = color_row(&tr("Stroke Color"));
        let stroke_width = spin(&tr("Stroke Width"), 0.5, 100.0, 0.5);
        stroke_width.set_digits(1);
        stroke_width.set_subtitle(&tr("Points"));
        let ends: Vec<String> = LINE_ENDS.iter().map(|e| line_end_name(*e)).collect();
        let start = combo(&tr("Start of the Line"), &ends);
        let end = combo(&tr("End of the Line"), &ends);
        let shadow = adw::SwitchRow::builder().title(tr("Shadow")).build();
        let (shadow_color_row, shadow_color) = color_row(&tr("Shadow Color"));
        let shadow_angle = spin(&tr("Shadow Angle"), 0.0, 359.0, 1.0);
        shadow_angle.set_subtitle(&tr("Degrees, counterclockwise"));
        let shadow_offset = spin(&tr("Shadow Offset"), 0.0, 100.0, 1.0);
        shadow_offset.set_subtitle(&tr("Points"));
        let shadow_blur = spin(&tr("Shadow Blur"), 0.0, 100.0, 1.0);
        shadow_blur.set_subtitle(&tr("Points"));
        let shadow_opacity = spin(&tr("Shadow Opacity"), 0.0, 100.0, 1.0);
        shadow_opacity.set_subtitle(&tr("Percent"));
        let opacity = spin(&tr("Opacity"), 0.0, 100.0, 1.0);
        opacity.set_subtitle(&tr("Percent"));
        let group = adw::PreferencesGroup::builder()
            .title(tr("Graphic"))
            .build();
        for row in [
            fill.upcast_ref::<gtk::Widget>(),
            fill_color_row.upcast_ref(),
            image_row.upcast_ref(),
            image_mode.upcast_ref(),
            gradient_end_row.upcast_ref(),
            gradient_angle.upcast_ref(),
            stroke.upcast_ref(),
            stroke_color_row.upcast_ref(),
            stroke_width.upcast_ref(),
            start.upcast_ref(),
            end.upcast_ref(),
            shadow.upcast_ref(),
            shadow_color_row.upcast_ref(),
            shadow_angle.upcast_ref(),
            shadow_offset.upcast_ref(),
            shadow_blur.upcast_ref(),
            shadow_opacity.upcast_ref(),
            opacity.upcast_ref(),
        ] {
            group.add(row);
        }
        let this = Rc::new(Self {
            group,
            fill,
            fill_color,
            fill_color_row,
            image_row,
            image_button,
            image_mode,
            picture: RefCell::new(None),
            on_picture: RefCell::new(None),
            gradient_end,
            gradient_end_row,
            gradient_angle,
            stroke,
            stroke_color,
            stroke_color_row,
            stroke_width,
            start,
            end,
            shadow,
            shadow_color,
            shadow_color_row,
            shadow_angle,
            shadow_offset,
            shadow_blur,
            shadow_opacity,
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
                if matches!(part, Part::Fill)
                    && this.fill.selected() == 3
                    && this.picture.borrow().is_none()
                {
                    // An image fill begins with an image.
                    this.choose_picture();
                    return;
                }
                let fill = this.fill_value();
                let stroke = this.stroke_value();
                let shadow = this.shadow_value();
                let opacity = this.opacity.value().round().clamp(0.0, 100.0) as u8;
                editor.style_objects(name, move |style, line| match part {
                    Part::Fill => {
                        if !line {
                            style.fill.clone_from(&fill);
                        }
                    }
                    Part::Stroke => {
                        style.stroke = stroke.map(|mut s| {
                            if !line {
                                s.start = LineEnd::None;
                                s.end = LineEnd::None;
                            }
                            s
                        });
                    }
                    Part::Shadow => style.shadow = shadow,
                    Part::Opacity => style.opacity = opacity,
                });
                this.show_rows();
            }
        };
        let on = changed.clone();
        this.fill
            .connect_selected_notify(move |_| on("Fill", Part::Fill));
        let on = changed.clone();
        this.image_mode
            .connect_selected_notify(move |_| on("Fill", Part::Fill));
        let on = changed.clone();
        *this.on_picture.borrow_mut() = Some(Rc::new(move || on("Fill", Part::Fill)));
        let weak = Rc::downgrade(&this);
        this.image_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.choose_picture();
            }
        });
        for button in [&this.fill_color, &this.gradient_end] {
            let on = changed.clone();
            button.connect_rgba_notify(move |_| on("Fill", Part::Fill));
        }
        let on = changed.clone();
        this.gradient_angle
            .connect_value_notify(move |_| on("Fill", Part::Fill));
        for row in [&this.stroke, &this.start, &this.end] {
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
        let on = changed;
        this.opacity
            .connect_value_notify(move |_| on("Opacity", Part::Opacity));
        this
    }

    /// The group, to put in the Inspector.
    pub fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    /// Asks for an image file, and fills with its picture.
    fn choose_picture(self: &Rc<Self>) {
        let filters = crate::bullet_pictures::document_picture_filters();
        let dialog = gtk::FileDialog::builder()
            .title(tr("Choose an Image to Fill With"))
            .modal(true)
            .filters(&filters)
            .build();
        let parent = self.group.root().and_downcast::<gtk::Window>();
        let weak = Rc::downgrade(self);
        dialog.open(
            parent.as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| {
                let Some(this) = weak.upgrade() else { return };
                let picture = result
                    .ok()
                    .and_then(|file| crate::bullet_pictures::document_picture_from_file(&file));
                match picture {
                    Some(picture) => {
                        *this.picture.borrow_mut() = Some(picture);
                        let on = this.on_picture.borrow().clone();
                        if let Some(on) = on {
                            on();
                        }
                    }
                    // Nothing chosen: the rows show the fill there is.
                    None => this.show_rows(),
                }
            },
        );
    }

    /// The fill that the rows ask for.
    fn fill_value(&self) -> Option<Fill> {
        let from = from_gdk(&self.fill_color.rgba());
        match self.fill.selected() {
            1 => Some(Fill::Color(from)),
            2 => Some(Fill::Gradient {
                from,
                to: from_gdk(&self.gradient_end.rgba()),
                angle: (self.gradient_angle.value() * 100.0).round() as i32,
            }),
            3 => self.picture.borrow().clone().map(|picture| Fill::Image {
                picture,
                mode: IMAGE_FILLS
                    .get(self.image_mode.selected() as usize)
                    .copied()
                    .unwrap_or_default(),
            }),
            _ => None,
        }
    }

    /// The line that the rows ask for.
    fn stroke_value(&self) -> Option<Stroke> {
        let selected = self.stroke.selected() as usize;
        let style = *STROKES.get(selected.checked_sub(1)?)?;
        let end = |row: &adw::ComboRow| {
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
        let fill = self.fill.selected();
        let shown = self.fill.is_visible();
        self.fill_color_row
            .set_visible(shown && (fill == 1 || fill == 2));
        self.image_row.set_visible(shown && fill == 3);
        self.image_mode.set_visible(shown && fill == 3);
        self.gradient_end_row
            .set_visible(self.fill.is_visible() && fill == 2);
        self.gradient_angle
            .set_visible(self.fill.is_visible() && fill == 2);
        let stroked = self.stroke.selected() > 0;
        self.stroke_color_row.set_visible(stroked);
        self.stroke_width.set_visible(stroked);
        let line = !self.fill.is_visible();
        self.start.set_visible(stroked && line);
        self.end.set_visible(stroked && line);
        let shadow = self.shadow.is_active();
        for row in [
            self.shadow_angle.upcast_ref::<gtk::Widget>(),
            self.shadow_offset.upcast_ref(),
            self.shadow_blur.upcast_ref(),
            self.shadow_opacity.upcast_ref(),
            self.shadow_color_row.upcast_ref(),
        ] {
            row.set_visible(shadow);
        }
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
        self.fill.set_visible(!line);
        let set_color = |button: &gtk::ColorDialogButton, color: Rgba| {
            let color = to_gdk(color);
            if button.rgba() != color {
                button.set_rgba(&color);
            }
        };
        let set = |row: &adw::SpinRow, value: f64| {
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        };
        let select = |row: &adw::ComboRow, index: u32| {
            if row.selected() != index {
                row.set_selected(index);
            }
        };
        if !matches!(style.fill, Some(Fill::Image { .. })) {
            *self.picture.borrow_mut() = None;
        }
        match style.fill.clone() {
            Some(Fill::Color(color)) => {
                select(&self.fill, 1);
                set_color(&self.fill_color, color);
            }
            Some(Fill::Gradient { from, to, angle }) => {
                select(&self.fill, 2);
                set_color(&self.fill_color, from);
                set_color(&self.gradient_end, to);
                set(&self.gradient_angle, f64::from(angle) / 100.0);
            }
            Some(Fill::Image { picture, mode }) => {
                select(&self.fill, 3);
                *self.picture.borrow_mut() = Some(picture);
                let index = IMAGE_FILLS.iter().position(|m| *m == mode).unwrap_or(0);
                select(&self.image_mode, index as u32);
            }
            _ => {
                select(&self.fill, 0);
                set_color(&self.fill_color, BLUE);
                set_color(&self.gradient_end, Rgba::WHITE);
            }
        }
        match style.stroke {
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
    Opacity,
}
