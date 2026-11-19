// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rows that choose a fill, as the Graphic inspector of Pages offers
//! them for an object (OBJ-09, OBJ-41, OBJ-42) and the text color of the
//! Text inspector offers them for letters (PXV-04): one color, a gradient
//! of two, a picture with or without a tint, or an advanced gradient.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Fill, Gradient, GradientStop, ImageFill, Picture, Rgba};

use crate::compact;
use crate::formatting::{from_gdk, labelled, to_gdk};
use crate::gradient_bar::GradientBar;
use crate::i18n::tr;

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

/// A kind of fill, as the Fill list offers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// No fill: none for an object, the color of the text for letters.
    Plain,
    /// One color.
    Color,
    /// A gradient of two colors.
    Gradient,
    /// A picture.
    Image,
    /// A picture with a color over it (OBJ-42).
    TintedImage,
    /// Several colors, along a line or out from a point (OBJ-41).
    Advanced,
}

impl Kind {
    /// The kind of a fill; plain for none.
    fn of(fill: Option<&Fill>) -> Self {
        match fill {
            Some(Fill::Color(_)) => Self::Color,
            Some(Fill::Gradient { .. }) => Self::Gradient,
            Some(Fill::Advanced(_)) => Self::Advanced,
            Some(Fill::Image { tint: Some(_), .. }) => Self::TintedImage,
            Some(Fill::Image { .. }) => Self::Image,
            _ => Self::Plain,
        }
    }

    fn is_image(self) -> bool {
        matches!(self, Self::Image | Self::TintedImage)
    }
}

/// A row with a color button at its end: the row, the holder of the color
/// and the button of the Colors popover.
pub fn color_row(title: &str) -> (compact::ActionRow, gtk::ColorDialogButton, gtk::MenuButton) {
    let button = gtk::ColorDialogButton::builder()
        .dialog(&gtk::ColorDialog::builder().title(title).build())
        .valign(gtk::Align::Center)
        .build();
    labelled(&button, title);
    let row = compact::ActionRow::builder().title(title).build();
    let colors = crate::color_popover::button(&button, title);
    row.add_suffix(&colors);
    (row, button, colors)
}

pub fn spin(title: &str, min: f64, max: f64, step: f64) -> compact::SpinRow {
    let row = compact::SpinRow::with_range(min, max, step);
    row.set_title(title);
    row
}

pub fn combo(title: &str, names: &[String]) -> compact::ComboRow {
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    compact::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(&names))
        .build()
}

/// The rows of a fill.
pub struct FillRows {
    /// The kinds of the Fill list, in its order.
    kinds: Vec<Kind>,
    fill: compact::ComboRow,
    fill_color: gtk::ColorDialogButton,
    fill_color_row: compact::ActionRow,
    image_row: compact::ActionRow,
    image_button: gtk::Button,
    image_mode: compact::ComboRow,
    /// The picture that fills, or that an image fill would fill with.
    picture: RefCell<Option<Picture>>,
    gradient_end: gtk::ColorDialogButton,
    gradient_end_row: compact::ActionRow,
    gradient_angle: compact::SpinRow,
    gradient_kind: compact::ComboRow,
    gradient_bar: Rc<GradientBar>,
    gradient_colors_row: compact::ActionRow,
    gradient_remove: gtk::Button,
    stop_position: compact::SpinRow,
    stop_blend: compact::SpinRow,
    center_x: compact::SpinRow,
    center_y: compact::SpinRow,
    /// Whether the Fill row is offered at all: a line has no fill.
    shown: Cell<bool>,
    updating: Cell<bool>,
    /// Called when the rows ask for another fill.
    on_change: RefCell<Option<Rc<dyn Fn()>>>,
}

impl FillRows {
    /// The rows, with the kinds of fill that the Fill list offers, in its
    /// order, and the name of the plain kind.
    pub fn new(kinds: &[Kind], plain: &str) -> Rc<Self> {
        let names: Vec<String> = kinds
            .iter()
            .map(|kind| match kind {
                Kind::Plain => plain.to_owned(),
                Kind::Color => tr("Color Fill"),
                Kind::Gradient => tr("Gradient Fill"),
                Kind::Image => tr("Image Fill"),
                Kind::TintedImage => tr("Tinted Image Fill"),
                Kind::Advanced => tr("Advanced Gradient Fill"),
            })
            .collect();
        let fill = combo(&tr("Fill"), &names);
        let image_button = gtk::Button::builder()
            .label(tr("Choose…"))
            .valign(gtk::Align::Center)
            .build();
        let image_row = compact::ActionRow::builder().title(tr("Image")).build();
        image_row.add_suffix(&image_button);
        let modes: Vec<String> = IMAGE_FILLS.iter().map(|m| image_fill_name(*m)).collect();
        let image_mode = combo(&tr("Image Size"), &modes);
        let (fill_color_row, fill_color, fill_colors) = color_row(&tr("Fill Color"));
        let (gradient_end_row, gradient_end, _) = color_row(&tr("Gradient End Color"));
        let gradient_angle = spin(&tr("Gradient Angle"), 0.0, 359.0, 1.0);
        gradient_angle.set_subtitle(&tr("Degrees, counterclockwise"));
        let gradient_kind = combo(&tr("Gradient Type"), &[tr("Linear"), tr("Radial")]);
        let gradient_bar = GradientBar::new();
        let gradient_colors_row = compact::ActionRow::builder()
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
        let this = Rc::new(Self {
            kinds: kinds.to_vec(),
            fill,
            fill_color,
            fill_color_row,
            image_row,
            image_button,
            image_mode,
            picture: RefCell::new(None),
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
            shown: Cell::new(true),
            updating: Cell::new(false),
            on_change: RefCell::new(None),
        });

        let changed = {
            let weak = Rc::downgrade(&this);
            move || {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if this.kind().is_image() && this.picture.borrow().is_none() {
                    // An image fill begins with an image.
                    this.choose_picture();
                    return;
                }
                this.changed();
            }
        };
        // None in the Colors popover takes the fill away; on a tinted
        // picture it takes the tint away and keeps the picture (B-026).
        let weak = Rc::downgrade(&this);
        crate::color_popover::add_none(&fill_colors, move || {
            if let Some(this) = weak.upgrade() {
                let kind = if this.kind() == Kind::TintedImage {
                    Kind::Image
                } else {
                    Kind::Plain
                };
                this.select(kind);
            }
        });
        let on = changed.clone();
        let weak = Rc::downgrade(&this);
        this.fill.connect_selected_notify(move |_| {
            // An advanced gradient starts from the colors there are.
            if let Some(this) = weak.upgrade()
                && this.kind() == Kind::Advanced
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
            on();
        });
        let on = changed.clone();
        this.image_mode.connect_selected_notify(move |_| on());
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
                && this.kind() == Kind::Advanced
                && !this.updating.get()
            {
                this.take_stop();
            }
            on();
        };
        let on = stop_changed.clone();
        this.fill_color.connect_rgba_notify(move |_| on());
        for row in [&this.stop_position, &this.stop_blend] {
            let on = stop_changed.clone();
            row.connect_value_notify(move |_| on());
        }
        let on = changed.clone();
        this.gradient_end.connect_rgba_notify(move |_| on());
        for row in [&this.gradient_angle, &this.center_x, &this.center_y] {
            let on = changed.clone();
            row.connect_value_notify(move |_| on());
        }
        let on = changed.clone();
        this.gradient_kind.connect_selected_notify(move |_| on());
        // The bar changes its colors itself: the fill is set from them, and
        // the rows show the color chosen in it.
        let on = changed;
        let weak = Rc::downgrade(&this);
        this.gradient_bar.connect_change(move || {
            if let Some(this) = weak.upgrade() {
                this.show_stop();
            }
            on();
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
        this.show_rows();
        this
    }

    /// The rows, in order, to put in a group.
    pub fn rows(&self) -> [&dyn compact::Row; 12] {
        [
            &self.fill,
            &self.fill_color_row,
            &self.image_row,
            &self.image_mode,
            &self.gradient_end_row,
            &self.gradient_kind,
            &self.gradient_angle,
            &self.center_x,
            &self.center_y,
            &self.gradient_colors_row,
            &self.stop_position,
            &self.stop_blend,
        ]
    }

    /// Calls `f` when the rows ask for another fill.
    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        *self.on_change.borrow_mut() = Some(Rc::new(f));
    }

    fn changed(&self) {
        let on = self.on_change.borrow().clone();
        if let Some(on) = on {
            on();
        }
        self.show_rows();
    }

    /// The kind of fill chosen in the Fill list.
    pub fn kind(&self) -> Kind {
        self.kinds
            .get(self.fill.selected() as usize)
            .copied()
            .unwrap_or(Kind::Plain)
    }

    /// Chooses a kind in the Fill list, when it offers it.
    fn select(&self, kind: Kind) {
        if let Some(index) = self.kinds.iter().position(|k| *k == kind)
            && self.fill.selected() != index as u32
        {
            self.fill.set_selected(index as u32);
        }
    }

    /// Offers the rows, or hides them all, as for a line.
    pub fn set_shown(&self, shown: bool) {
        self.shown.set(shown);
        self.show_rows();
    }

    /// Whether the rows are offered.
    pub fn is_shown(&self) -> bool {
        self.shown.get()
    }

    /// The window the rows are in, for the file dialog.
    fn window(&self) -> Option<gtk::Window> {
        self.fill.root().and_downcast::<gtk::Window>()
    }

    /// Asks for an image file, and fills with its picture.
    fn choose_picture(self: &Rc<Self>) {
        let filters = crate::bullet_pictures::document_picture_filters(false);
        let dialog = gtk::FileDialog::builder()
            .title(tr("Choose an Image to Fill With"))
            .modal(true)
            .filters(&filters)
            .build();
        let weak = Rc::downgrade(self);
        dialog.open(
            self.window().as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| {
                let Some(this) = weak.upgrade() else { return };
                let picture = result
                    .ok()
                    .and_then(|file| crate::bullet_pictures::document_picture_from_file(&file));
                match picture {
                    Some(picture) => {
                        *this.picture.borrow_mut() = Some(picture);
                        this.changed();
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
        let percent =
            |row: &compact::SpinRow| (row.value() * 10.0).round().clamp(0.0, 1000.0) as u16;
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

    /// The fill that the rows ask for; none for the plain kind.
    pub fn value(&self) -> Option<Fill> {
        let from = from_gdk(&self.fill_color.rgba());
        let angle = (self.gradient_angle.value() * 100.0).round() as i32;
        match self.kind() {
            Kind::Plain => None,
            Kind::Color => Some(Fill::Color(from)),
            Kind::Gradient => Some(Fill::Gradient {
                from,
                to: from_gdk(&self.gradient_end.rgba()),
                angle,
            }),
            kind @ (Kind::Image | Kind::TintedImage) => {
                self.picture.borrow().clone().map(|picture| Fill::Image {
                    picture,
                    mode: IMAGE_FILLS
                        .get(self.image_mode.selected() as usize)
                        .copied()
                        .unwrap_or_default(),
                    // An opaque tint would hide the picture: it starts at
                    // half.
                    tint: (kind == Kind::TintedImage).then(|| {
                        let mut tint = from;
                        if tint.a == 255 {
                            tint.a = 128;
                        }
                        tint
                    }),
                })
            }
            Kind::Advanced => Some(Fill::Advanced(Gradient {
                stops: self.gradient_bar.stops(),
                angle,
                center: (self.gradient_kind.selected() == 1).then(|| {
                    let part = |row: &compact::SpinRow| {
                        (row.value() * 10.0).round().clamp(0.0, 1000.0) as u16
                    };
                    (part(&self.center_x), part(&self.center_y))
                }),
            })),
        }
    }

    /// Shows the rows that the kind of fill calls for.
    fn show_rows(&self) {
        let kind = self.kind();
        let shown = self.shown.get();
        self.fill.set_visible(shown);
        self.fill_color_row.set_visible(
            shown
                && matches!(
                    kind,
                    Kind::Color | Kind::Gradient | Kind::TintedImage | Kind::Advanced
                ),
        );
        self.fill_color_row.set_title(&if kind == Kind::Advanced {
            tr("Color")
        } else {
            tr("Fill Color")
        });
        let advanced = shown && kind == Kind::Advanced;
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
        self.image_row.set_visible(shown && kind.is_image());
        self.image_mode.set_visible(shown && kind.is_image());
        self.gradient_end_row
            .set_visible(shown && kind == Kind::Gradient);
        self.gradient_angle
            .set_visible(shown && (kind == Kind::Gradient || (kind == Kind::Advanced && !radial)));
    }

    /// Shows a fill; `color` is where the colors start when there is none.
    pub fn show(&self, fill: Option<&Fill>, color: Rgba) {
        let updating = self.updating.replace(true);
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
        if !matches!(fill, Some(Fill::Image { .. })) {
            *self.picture.borrow_mut() = None;
        }
        // A fill that the list does not offer shows as the plain kind.
        let kind = Kind::of(fill);
        self.select(if self.kinds.contains(&kind) {
            kind
        } else {
            Kind::Plain
        });
        match fill.cloned() {
            Some(Fill::Color(color)) => set_color(&self.fill_color, color),
            Some(Fill::Gradient { from, to, angle }) => {
                set_color(&self.fill_color, from);
                set_color(&self.gradient_end, to);
                set(&self.gradient_angle, f64::from(angle) / 100.0);
                // An advanced gradient chosen next starts from it.
                self.gradient_bar
                    .set_stops(Gradient::linear(from, to, angle).stops);
            }
            Some(Fill::Advanced(gradient)) => {
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
                if let Some(tint) = tint {
                    set_color(&self.fill_color, tint);
                }
                *self.picture.borrow_mut() = Some(picture);
                let index = IMAGE_FILLS.iter().position(|m| *m == mode).unwrap_or(0);
                select(&self.image_mode, index as u32);
            }
            _ => {
                set_color(&self.fill_color, color);
                set_color(&self.gradient_end, Rgba::WHITE);
            }
        }
        if !matches!(fill, Some(Fill::Gradient { .. } | Fill::Advanced(_))) {
            self.gradient_bar.set_stops(Vec::new());
        }
        self.show_rows();
        self.updating.set(updating);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pxv_04_the_rows_show_a_fill_and_give_it_back() {
        crate::with_gtk(rows_show_a_fill);
    }

    fn rows_show_a_fill() {
        let rows = FillRows::new(
            &[Kind::Plain, Kind::Gradient, Kind::Advanced, Kind::Image],
            "Color Fill",
        );
        let gradient = Fill::Gradient {
            from: Rgba::rgb(200, 0, 0),
            to: Rgba::rgb(0, 0, 200),
            angle: 4500,
        };
        rows.show(Some(&gradient), Rgba::BLACK);
        assert_eq!(rows.kind(), Kind::Gradient);
        assert_eq!(rows.fill.selected(), 1);
        assert_eq!(rows.value(), Some(gradient));
        assert!(rows.gradient_end_row.is_visible() && !rows.image_row.is_visible());
        let advanced = Fill::Advanced(Gradient {
            stops: vec![
                GradientStop::new(0, Rgba::rgb(10, 20, 30)),
                GradientStop::new(600, Rgba::rgb(40, 50, 60)),
                GradientStop::new(1000, Rgba::WHITE),
            ],
            angle: 0,
            center: Some((300, 700)),
        });
        rows.show(Some(&advanced), Rgba::BLACK);
        assert_eq!(rows.value(), Some(advanced));
        assert!(rows.center_x.is_visible() && !rows.gradient_angle.is_visible());
        // A fill that the list does not offer shows as plain.
        rows.show(Some(&Fill::Color(Rgba::WHITE)), Rgba::BLACK);
        assert_eq!(rows.kind(), Kind::Plain);
        rows.show(None, Rgba::rgb(1, 2, 3));
        assert_eq!(rows.value(), None);
        assert!(!rows.fill_color_row.is_visible());
        // The gradient chosen next starts from the color of the text.
        rows.fill.set_selected(1);
        assert_eq!(
            rows.value(),
            Some(Fill::Gradient {
                from: Rgba::rgb(1, 2, 3),
                to: Rgba::WHITE,
                // The angle of the last gradient shown.
                angle: 0,
            })
        );
    }
}
