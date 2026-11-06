// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Graphic inspector of Pages, for the selected objects (OBJ-09,
//! OBJ-10, OBJ-11, OBJ-12): what fills them, the line around them and at
//! the ends of a line, their shadow and reflection and how opaque they
//! are.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{
    BorderStyle, Fill, Gradient, GradientStop, GraphicStyle, ImageFill, Length, LineEnd,
    ObjectKind, ObjectShadow, Picture, Rgba, Stroke,
};

use crate::editing::ObjectInfo;
use crate::formatting::{EditorOf, from_gdk, labelled, to_gdk};
use crate::gradient_bar::GradientBar;
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
    /// The rows of the advanced gradient (OBJ-41).
    gradient_kind: adw::ComboRow,
    gradient_bar: Rc<GradientBar>,
    gradient_colors_row: adw::ActionRow,
    gradient_remove: gtk::Button,
    stop_position: adw::SpinRow,
    stop_blend: adw::SpinRow,
    center_x: adw::SpinRow,
    center_y: adw::SpinRow,
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
    reflection: adw::SwitchRow,
    reflection_opacity: adw::SpinRow,
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
                // A picture with a color over it (OBJ-42).
                tr("Tinted Image Fill"),
                // Several colors, along a line or out from a point
                // (OBJ-41).
                tr("Advanced Gradient Fill"),
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
        let gradient_kind = combo(&tr("Gradient Type"), &[tr("Linear"), tr("Radial")]);
        let gradient_bar = GradientBar::new();
        let gradient_colors_row = adw::ActionRow::builder()
            .title(tr("Gradient Colors"))
            .subtitle(tr("Click to add a color, drag to move it"))
            .build();
        let icon_button = |icon: &str, label: &str| {
            let button = gtk::Button::builder()
                .icon_name(icon)
                .valign(gtk::Align::Center)
                .tooltip_text(label)
                .css_classes(["flat"])
                .build();
            labelled(&button, label);
            button
        };
        let gradient_add = icon_button("list-add-symbolic", &tr("Add Color"));
        let gradient_remove = icon_button("list-remove-symbolic", &tr("Remove Color"));
        gradient_colors_row.add_suffix(gradient_bar.widget());
        gradient_colors_row.add_suffix(&gradient_add);
        gradient_colors_row.add_suffix(&gradient_remove);
        let stop_position = spin(&tr("Color Position"), 0.0, 100.0, 1.0);
        stop_position.set_subtitle(&tr("Percent"));
        let stop_blend = spin(&tr("Blend Point"), 1.0, 99.0, 1.0);
        stop_blend.set_subtitle(&tr("Percent of the way to the next color"));
        let center_x = spin(&tr("Horizontal Center"), 0.0, 100.0, 1.0);
        center_x.set_subtitle(&tr("Percent"));
        let center_y = spin(&tr("Vertical Center"), 0.0, 100.0, 1.0);
        center_y.set_subtitle(&tr("Percent"));
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
        // A reflection below the object, as in Pages (OBJ-38).
        let reflection = adw::SwitchRow::builder().title(tr("Reflection")).build();
        let reflection_opacity = spin(&tr("Reflection Opacity"), 0.0, 100.0, 1.0);
        reflection_opacity.set_subtitle(&tr("Percent"));
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
            gradient_kind.upcast_ref(),
            gradient_angle.upcast_ref(),
            center_x.upcast_ref(),
            center_y.upcast_ref(),
            gradient_colors_row.upcast_ref(),
            stop_position.upcast_ref(),
            stop_blend.upcast_ref(),
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
            reflection.upcast_ref(),
            reflection_opacity.upcast_ref(),
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
            gradient_kind,
            gradient_bar,
            gradient_colors_row,
            gradient_remove,
            stop_position,
            stop_blend,
            center_x,
            center_y,
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
                if matches!(part, Part::Fill)
                    && matches!(this.fill.selected(), 3 | 4)
                    && this.picture.borrow().is_none()
                {
                    // An image fill begins with an image.
                    this.choose_picture();
                    return;
                }
                let fill = this.fill_value();
                let stroke = this.stroke_value();
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
        let on = changed.clone();
        let weak = Rc::downgrade(&this);
        this.fill.connect_selected_notify(move |row| {
            // An advanced gradient starts from the colors there are.
            if let Some(this) = weak.upgrade()
                && row.selected() == 5
                && !this.updating.get()
            {
                if this.gradient_bar.stops().len() < 2 {
                    let from = from_gdk(&this.fill_color.rgba());
                    this.gradient_bar.set_stops(vec![
                        GradientStop::new(0, from),
                        GradientStop::new(1000, Rgba::WHITE),
                    ]);
                }
                this.show_stop();
            }
            on("Fill", Part::Fill)
        });
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
        let on = changed.clone();
        let weak = Rc::downgrade(&this);
        let stop_changed = move || {
            // The rows of the chosen color set it in the bar first.
            if let Some(this) = weak.upgrade()
                && this.fill.selected() == 5
                && !this.updating.get()
            {
                this.take_stop();
            }
            on("Fill", Part::Fill)
        };
        let on = stop_changed.clone();
        this.fill_color.connect_rgba_notify(move |_| on());
        for row in [&this.stop_position, &this.stop_blend] {
            let on = stop_changed.clone();
            row.connect_value_notify(move |_| on());
        }
        let on = changed.clone();
        this.gradient_end
            .connect_rgba_notify(move |_| on("Fill", Part::Fill));
        for row in [&this.gradient_angle, &this.center_x, &this.center_y] {
            let on = changed.clone();
            row.connect_value_notify(move |_| on("Fill", Part::Fill));
        }
        let on = changed.clone();
        this.gradient_kind
            .connect_selected_notify(move |_| on("Fill", Part::Fill));
        // The bar changes its colors itself: the fill is set from them, and
        // the rows show the color chosen in it.
        let on = changed.clone();
        let weak = Rc::downgrade(&this);
        this.gradient_bar.connect_change(move || {
            if let Some(this) = weak.upgrade() {
                this.show_stop();
            }
            on("Fill", Part::Fill);
        });
        let weak = Rc::downgrade(&this);
        this.gradient_bar.connect_select(move || {
            if let Some(this) = weak.upgrade() {
                this.show_stop();
            }
        });
        let weak = Rc::downgrade(&this);
        gradient_add.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.gradient_bar.add();
            }
        });
        let weak = Rc::downgrade(&this);
        this.gradient_remove.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.gradient_bar.remove();
            }
        });
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
    pub fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    /// Asks for an image file, and fills with its picture.
    fn choose_picture(self: &Rc<Self>) {
        let filters = crate::bullet_pictures::document_picture_filters(false);
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

    /// Sets the color chosen in the bar of the advanced gradient from the
    /// rows that show it.
    fn take_stop(&self) {
        let percent = |row: &adw::SpinRow| (row.value() * 10.0).round().clamp(0.0, 1000.0) as u16;
        self.gradient_bar.set_current(GradientStop {
            at: percent(&self.stop_position),
            color: from_gdk(&self.fill_color.rgba()),
            blend: percent(&self.stop_blend),
        });
    }

    /// Shows the color chosen in the bar of the advanced gradient in the
    /// rows that set it.
    fn show_stop(&self) {
        let stops = self.gradient_bar.stops();
        let current = self.gradient_bar.current();
        let Some(stop) = stops.get(current) else {
            return;
        };
        let updating = self.updating.replace(true);
        let color = to_gdk(stop.color);
        if self.fill_color.rgba() != color {
            self.fill_color.set_rgba(&color);
        }
        self.stop_position.set_value(f64::from(stop.at) / 10.0);
        self.stop_blend.set_value(f64::from(stop.blend) / 10.0);
        self.updating.set(updating);
        self.show_rows();
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
            fill @ (3 | 4) => self.picture.borrow().clone().map(|picture| Fill::Image {
                picture,
                mode: IMAGE_FILLS
                    .get(self.image_mode.selected() as usize)
                    .copied()
                    .unwrap_or_default(),
                // An opaque tint would hide the picture: it starts at half.
                tint: (fill == 4).then(|| {
                    let mut tint = from;
                    if tint.a == 255 {
                        tint.a = 128;
                    }
                    tint
                }),
            }),
            5 => Some(Fill::Advanced(Gradient {
                stops: self.gradient_bar.stops(),
                angle: (self.gradient_angle.value() * 100.0).round() as i32,
                center: (self.gradient_kind.selected() == 1).then(|| {
                    let part =
                        |row: &adw::SpinRow| (row.value() * 10.0).round().clamp(0.0, 1000.0) as u16;
                    (part(&self.center_x), part(&self.center_y))
                }),
            })),
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
            .set_visible(shown && matches!(fill, 1 | 2 | 4 | 5));
        self.fill_color_row.set_title(&if fill == 5 {
            tr("Color")
        } else {
            tr("Fill Color")
        });
        let advanced = shown && fill == 5;
        let radial = advanced && self.gradient_kind.selected() == 1;
        self.gradient_kind.set_visible(advanced);
        self.gradient_colors_row.set_visible(advanced);
        self.stop_position.set_visible(advanced);
        let stops = self.gradient_bar.stops().len();
        // The last color blends into none.
        self.stop_blend
            .set_visible(advanced && self.gradient_bar.current() + 1 < stops);
        self.gradient_remove.set_sensitive(stops > 2);
        self.center_x.set_visible(radial);
        self.center_y.set_visible(radial);
        self.image_row.set_visible(shown && matches!(fill, 3 | 4));
        self.image_mode.set_visible(shown && matches!(fill, 3 | 4));
        self.gradient_end_row
            .set_visible(self.fill.is_visible() && fill == 2);
        self.gradient_angle
            .set_visible(shown && (fill == 2 || (fill == 5 && !radial)));
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
                // An advanced gradient chosen next starts from it.
                self.gradient_bar
                    .set_stops(Gradient::linear(from, to, angle).stops);
            }
            Some(Fill::Advanced(gradient)) => {
                select(&self.fill, 5);
                set(&self.gradient_angle, f64::from(gradient.angle) / 100.0);
                select(&self.gradient_kind, u32::from(gradient.center.is_some()));
                let (cx, cy) = gradient.center.unwrap_or((500, 500));
                set(&self.center_x, f64::from(cx) / 10.0);
                set(&self.center_y, f64::from(cy) / 10.0);
                self.gradient_bar.set_stops(gradient.stops);
                if let Some(stop) = self.gradient_bar.stops().get(self.gradient_bar.current()) {
                    set_color(&self.fill_color, stop.color);
                    set(&self.stop_position, f64::from(stop.at) / 10.0);
                    set(&self.stop_blend, f64::from(stop.blend) / 10.0);
                }
            }
            Some(Fill::Image {
                picture,
                mode,
                tint,
            }) => {
                select(&self.fill, if tint.is_some() { 4 } else { 3 });
                if let Some(tint) = tint {
                    set_color(&self.fill_color, tint);
                }
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
        if !matches!(style.fill, Some(Fill::Gradient { .. } | Fill::Advanced(_))) {
            self.gradient_bar.set_stops(Vec::new());
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
