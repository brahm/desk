// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The full Adjust Image of Pages '09 (OBJ-07, W110), behind the lean
//! adjustments of the Image tab of Pages 15.4: the levels over the
//! histogram of the picture, with Auto Levels, and each adjustment with a
//! slider.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Adjustments, Histogram, Picture};

use crate::compact;
use crate::i18n::tr;

/// A row of a number from `min` to `max` with its slider below, both on
/// the same adjustment.
pub(crate) fn slider_row(
    group: &compact::Group,
    title: &str,
    min: f64,
    max: f64,
) -> compact::SpinRow {
    let row = compact::SpinRow::with_range(min, max, 1.0);
    row.set_title(title);
    let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&row.spin().adjustment()));
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.update_property(&[gtk::accessible::Property::Label(title)]);
    group.add(&row);
    group.add_widget(&scale);
    row
}

/// The rows of the window, and what they show.
struct Rows {
    brightness: compact::SpinRow,
    contrast: compact::SpinRow,
    saturation: compact::SpinRow,
    exposure: compact::SpinRow,
    temperature: compact::SpinRow,
    tint: compact::SpinRow,
    sharpness: compact::SpinRow,
    black: compact::SpinRow,
    white: compact::SpinRow,
    /// Whether the rows are being set from the adjustments, so that they
    /// do not apply them again.
    updating: Cell<bool>,
}

impl Rows {
    fn adjustments(&self) -> Adjustments {
        let unit = |row: &compact::SpinRow| row.value().round().clamp(-100.0, 100.0) as i8;
        let level = |row: &compact::SpinRow| row.value().round().clamp(0.0, 255.0) as u8;
        Adjustments {
            brightness: unit(&self.brightness),
            contrast: unit(&self.contrast),
            saturation: unit(&self.saturation),
            exposure: unit(&self.exposure),
            temperature: unit(&self.temperature),
            tint: unit(&self.tint),
            sharpness: unit(&self.sharpness),
            black: level(&self.black),
            white: level(&self.white),
        }
    }

    fn show(&self, a: Adjustments) {
        self.updating.set(true);
        for (row, value) in [
            (&self.brightness, f64::from(a.brightness)),
            (&self.contrast, f64::from(a.contrast)),
            (&self.saturation, f64::from(a.saturation)),
            (&self.exposure, f64::from(a.exposure)),
            (&self.temperature, f64::from(a.temperature)),
            (&self.tint, f64::from(a.tint)),
            (&self.sharpness, f64::from(a.sharpness)),
            (&self.black, f64::from(a.black)),
            (&self.white, f64::from(a.white)),
        ] {
            row.set_value(value);
        }
        self.updating.set(false);
    }
}

/// Draws the histogram of the lightness of a picture, with the levels as
/// two lines and the values cut off them shaded.
fn draw_histogram(
    cr: &gtk::cairo::Context,
    width: f64,
    height: f64,
    histogram: Option<&Histogram>,
    (black, white): (u8, u8),
    color: gtk::gdk::RGBA,
) {
    let tall = histogram
        .and_then(|h| h.luma.iter().copied().max())
        .unwrap_or(0)
        .max(1);
    let (r, g, b) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );
    if let Some(histogram) = histogram {
        cr.set_source_rgba(r, g, b, 0.7);
        let bar = width / 256.0;
        for (v, &n) in histogram.luma.iter().enumerate() {
            // The square root, so that a peak does not flatten the rest.
            let share = (f64::from(n) / f64::from(tall)).sqrt();
            cr.rectangle(
                v as f64 * bar,
                height * (1.0 - share),
                bar.max(1.0),
                height * share,
            );
        }
        let _ = cr.fill();
    }
    let x = |v: u8| f64::from(v) / 255.0 * width;
    cr.set_source_rgba(r, g, b, 0.15);
    cr.rectangle(0.0, 0.0, x(black), height);
    cr.rectangle(x(white), 0.0, width - x(white), height);
    let _ = cr.fill();
    cr.set_source_rgba(r, g, b, 1.0);
    cr.set_line_width(2.0);
    for v in [black, white] {
        cr.move_to(x(v), 0.0);
        cr.line_to(x(v), height);
    }
    let _ = cr.stroke();
}

/// Opens Adjust Image for a picture with its adjustments, calling `apply`
/// with them as they change.
pub fn open(
    parent: &impl IsA<gtk::Widget>,
    picture: &Picture,
    adjust: Adjustments,
    apply: impl Fn(Adjustments) + 'static,
) {
    let histogram = Rc::new(desk_render::histogram(picture, Adjustments::default()));

    let levels = compact::Group::builder().title(tr("Levels")).build();
    let area = gtk::DrawingArea::builder()
        .content_height(96)
        .hexpand(true)
        .accessible_role(gtk::AccessibleRole::Img)
        .build();
    area.update_property(&[gtk::accessible::Property::Label(&tr(
        "Histogram of the lightness of the picture",
    ))]);
    levels.add_widget(&area);
    let black = slider_row(&levels, &tr("Black Point"), 0.0, 254.0);
    let white = slider_row(&levels, &tr("White Point"), 1.0, 255.0);
    let auto_levels = gtk::Button::builder()
        .label(tr("Auto Levels"))
        .valign(gtk::Align::Center)
        .build();
    let auto_row = compact::ActionRow::new();
    auto_row.add_suffix(&auto_levels);
    levels.add(&auto_row);

    let colors = compact::Group::builder().title(tr("Adjustments")).build();
    let row = |title: String| slider_row(&colors, &title, -100.0, 100.0);
    let rows = Rc::new(Rows {
        brightness: row(tr("Brightness")),
        contrast: row(tr("Contrast")),
        saturation: row(tr("Saturation")),
        exposure: row(tr("Exposure")),
        temperature: row(tr("Temperature")),
        tint: row(tr("Tint")),
        sharpness: row(tr("Sharpness")),
        black,
        white,
        updating: Cell::new(false),
    });
    rows.show(adjust);
    let reset = gtk::Button::builder()
        .label(tr("Reset Image"))
        .valign(gtk::Align::Center)
        .build();
    let reset_row = compact::ActionRow::new();
    reset_row.add_suffix(&reset);
    colors.add(&reset_row);

    {
        let rows = Rc::downgrade(&rows);
        let histogram = histogram.clone();
        area.set_draw_func(move |area, cr, width, height| {
            let Some(rows) = rows.upgrade() else { return };
            let a = rows.adjustments();
            draw_histogram(
                cr,
                f64::from(width),
                f64::from(height),
                histogram.as_ref().as_ref(),
                (a.black, a.white),
                area.color(),
            );
        });
    }

    let apply = Rc::new(apply);
    let current = Rc::new(RefCell::new(adjust));
    let changed = {
        let rows = Rc::downgrade(&rows);
        let area = area.downgrade();
        let apply = apply.clone();
        let current = current.clone();
        move || {
            let Some(rows) = rows.upgrade() else { return };
            if rows.updating.get() {
                return;
            }
            let mut a = rows.adjustments();
            // The black stays below the white.
            if a.white <= a.black {
                a.white = a.black.saturating_add(1);
                rows.show(a);
            }
            if let Some(area) = area.upgrade() {
                area.queue_draw();
            }
            if *current.borrow() != a {
                current.replace(a);
                apply(a);
            }
        }
    };
    let changed = Rc::new(changed);
    for row in [
        &rows.brightness,
        &rows.contrast,
        &rows.saturation,
        &rows.exposure,
        &rows.temperature,
        &rows.tint,
        &rows.sharpness,
        &rows.black,
        &rows.white,
    ] {
        let changed = changed.clone();
        row.connect_value_notify(move |_| changed());
    }
    {
        let rows = Rc::downgrade(&rows);
        let changed = changed.clone();
        let histogram = histogram.clone();
        auto_levels.set_sensitive(histogram.is_some());
        auto_levels.connect_clicked(move |_| {
            let (Some(rows), Some(histogram)) = (rows.upgrade(), histogram.as_ref()) else {
                return;
            };
            rows.show(rows.adjustments().auto_levels(histogram));
            changed();
        });
    }
    {
        let rows = Rc::downgrade(&rows);
        reset.connect_clicked(move |_| {
            if let Some(rows) = rows.upgrade() {
                rows.show(Adjustments::default());
                changed();
            }
        });
    }

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(levels.widget());
    content.append(colors.widget());
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .build();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&scrolled));
    let dialog = adw::Dialog::builder()
        .title(tr("Adjust Image"))
        .content_width(360)
        .child(&view)
        .build();
    // The rows live as long as the window.
    dialog.connect_closed(move |_| {
        let _ = &rows;
    });
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w110_the_levels_are_drawn_over_the_histogram() {
        let mut histogram = Histogram::default();
        for v in [20u8, 128, 128, 230] {
            histogram.add([v, v, v]);
        }
        let surface =
            gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, 256, 50).unwrap();
        {
            let cr = gtk::cairo::Context::new(&surface).unwrap();
            draw_histogram(
                &cr,
                256.0,
                50.0,
                Some(&histogram),
                (10, 240),
                gtk::gdk::RGBA::BLACK,
            );
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let mut surface = surface;
        let data = surface.data().unwrap();
        let alpha = |x: usize, y: usize| data[y * stride + x * 4 + 3];
        // The tallest bar reaches the top; a level is a line from top to
        // bottom; a value with no pixels is bare.
        assert!(alpha(128, 1) > 100);
        assert!(alpha(10, 25) > 200);
        assert_eq!(alpha(70, 25), 0);
    }
}
