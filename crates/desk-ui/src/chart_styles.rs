// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chart styles in view, as Pages 15 shows them (W080): small column
//! charts drawn in the fills of each style, in the popover of the Chart
//! button, where a click inserts a chart of that style, and in the Chart
//! tab of the Format pane, where a click gives the style to the selected
//! chart.

use desk_model::ChartStyle;
use desk_model::chart::{SeriesFill, lighter};
use desk_model::table::Color;
use gtk::prelude::*;

use crate::i18n::tr;

/// The size of a picture of a style, in pixels.
const WIDTH: i32 = 64;
const HEIGHT: i32 = 46;
/// The heights of its columns, in parts of the picture: one of each of
/// the first four series.
const COLUMNS: [f64; 4] = [0.55, 0.8, 0.4, 0.65];

fn rgb(c: Color) -> (f64, f64, f64) {
    let part = |v: u8| f64::from(v) / 255.0;
    (part(c.r), part(c.g), part(c.b))
}

/// Sets the paint of a column of a fill: its color, its gradient from the
/// top, or the main color of its texture.
fn paint(cr: &gtk::cairo::Context, fill: &SeriesFill, top: f64, bottom: f64) {
    match fill {
        SeriesFill::Color(c) => {
            let (r, g, b) = rgb(*c);
            cr.set_source_rgb(r, g, b);
        }
        SeriesFill::Gradient(from, to) => {
            let gradient = gtk::cairo::LinearGradient::new(0.0, top, 0.0, bottom);
            let (r, g, b) = rgb(*from);
            gradient.add_color_stop_rgb(0.0, r, g, b);
            let (r, g, b) = rgb(*to);
            gradient.add_color_stop_rgb(1.0, r, g, b);
            cr.set_source(&gradient).ok();
        }
        SeriesFill::Texture(t) => {
            let (r, g, b) = rgb(lighter(t.base()));
            cr.set_source_rgb(r, g, b);
        }
    }
}

/// A small column chart in the look of a style.
pub fn picture(style: &ChartStyle) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(WIDTH)
        .content_height(HEIGHT)
        .build();
    let style = style.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (f64::from(w), f64::from(h));
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.rectangle(0.0, 0.0, w, h);
        cr.fill().ok();
        let base = h - 6.0;
        let slot = (w - 8.0) / COLUMNS.len() as f64;
        for (n, part) in COLUMNS.iter().enumerate() {
            let top = base - part * (base - 4.0);
            let x = 4.0 + n as f64 * slot + slot * 0.15;
            paint(cr, &style.fill(n), top, base);
            cr.rectangle(x, top, slot * 0.7, base - top);
            cr.fill().ok();
        }
        // The axis, in the color of the text.
        let (r, g, b) = rgb(style.text.color);
        cr.set_source_rgb(r, g, b);
        cr.set_line_width(1.0);
        cr.move_to(3.0, base + 0.5);
        cr.line_to(w - 3.0, base + 0.5);
        cr.stroke().ok();
    });
    area
}

/// The chart styles of a document as a grid of pictures, the current one
/// shown as chosen; `pick` runs with the place of the style that a click
/// chooses.
pub fn grid(
    styles: &[ChartStyle],
    current: Option<usize>,
    pick: impl Fn(usize) + 'static,
) -> gtk::FlowBox {
    let items = styles
        .iter()
        .enumerate()
        .map(|(n, s)| {
            let name = tr("Chart Style {number}").replace("{number}", &(n + 1).to_string());
            (picture(s).upcast(), name)
        })
        .collect();
    crate::table_styles::pictures(&tr("Chart Styles"), items, current, pick)
}
