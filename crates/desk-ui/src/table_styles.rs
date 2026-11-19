// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The table styles in view, as Pages 15 shows them (PXV-14): small
//! tables drawn in the look of each style, in the popover of the Table
//! button, where a click inserts a table of that style, and in the Table
//! tab of the Format pane, where a click gives the style to the selected
//! table.

use desk_model::table::Color;
use desk_model::{StyleId, TableLook, TableStyleEntry};
use gtk::prelude::*;

use crate::i18n::{n_, tr};

/// The names of the table styles of a new document, marked for
/// translation; desk_model::table_style::library has them in English.
#[allow(dead_code, reason = "only read by xgettext")]
const NAMES: [&str; 6] = [
    n_("Basic"),
    n_("Plain"),
    n_("Blue"),
    n_("Green"),
    n_("Orange"),
    n_("Striped"),
];

/// The size of a picture of a style, in pixels.
const WIDTH: i32 = 64;
const HEIGHT: i32 = 46;
/// The rows and columns of a picture: one of headers, three of body.
const ROWS: usize = 4;
const COLUMNS: usize = 4;

fn rgb(c: Color) -> (f64, f64, f64) {
    let part = |v: u8| f64::from(v) / 255.0;
    (part(c.r), part(c.g), part(c.b))
}

/// The fill of a cell of the picture: the header row, the header column,
/// every other body row, or the body.
fn cell_fill(look: &TableLook, row: usize, col: usize) -> Option<Color> {
    let s = &look.style;
    if row == 0 {
        s.header_row_fill
    } else if col == 0 && s.header_column_fill.is_some() {
        s.header_column_fill
    } else if row.is_multiple_of(2) && s.alternating.is_some() {
        s.alternating
    } else {
        s.body_fill
    }
}

/// A small table in the look of a style.
pub fn picture(look: &TableLook) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(WIDTH)
        .content_height(HEIGHT)
        .build();
    let look = look.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (f64::from(w), f64::from(h));
        let (cw, ch) = (w / COLUMNS as f64, h / ROWS as f64);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.rectangle(0.0, 0.0, w, h);
        cr.fill().ok();
        for row in 0..ROWS {
            for col in 0..COLUMNS {
                if let Some(fill) = cell_fill(&look, row, col) {
                    let (r, g, b) = rgb(fill);
                    cr.set_source_rgb(r, g, b);
                    cr.rectangle(col as f64 * cw, row as f64 * ch, cw, ch);
                    cr.fill().ok();
                }
            }
        }
        if let Some(border) = look.border {
            let (r, g, b) = rgb(border.color);
            cr.set_source_rgb(r, g, b);
            cr.set_line_width(1.0);
            for i in 0..=ROWS {
                let y = (i as f64 * ch).round().clamp(0.5, h - 0.5);
                cr.move_to(0.0, y);
                cr.line_to(w, y);
            }
            for i in 0..=COLUMNS {
                let x = (i as f64 * cw).round().clamp(0.5, w - 0.5);
                cr.move_to(x, 0.0);
                cr.line_to(x, h);
            }
            cr.stroke().ok();
        }
    });
    area
}

/// Pictures of styles in a grid of three a line, each with its name for
/// the tooltip and the screen reader, the current one shown as chosen;
/// `pick` runs with the place of the one a click chooses. The table
/// styles and the chart styles show this way (PXV-14, W080).
pub fn pictures(
    label: &str,
    items: Vec<(gtk::Widget, String)>,
    current: Option<usize>,
    pick: impl Fn(usize) + 'static,
) -> gtk::FlowBox {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .min_children_per_line(3)
        .max_children_per_line(3)
        .homogeneous(true)
        .column_spacing(6)
        .row_spacing(6)
        .activate_on_single_click(true)
        .build();
    flow.update_property(&[gtk::accessible::Property::Label(label)]);
    for (index, (picture, name)) in items.into_iter().enumerate() {
        let child = gtk::FlowBoxChild::new();
        child.set_child(Some(&picture));
        child.set_tooltip_text(Some(&name));
        child.update_property(&[gtk::accessible::Property::Label(&name)]);
        flow.append(&child);
        if current == Some(index) {
            flow.select_child(&child);
        }
    }
    flow.connect_child_activated(move |_, child| {
        if let Ok(index) = usize::try_from(child.index()) {
            pick(index);
        }
    });
    flow
}

/// The table styles of a document as a grid of pictures, the current one
/// shown as chosen; `pick` runs with the style that a click chooses.
pub fn grid(
    styles: &[TableStyleEntry],
    current: Option<StyleId>,
    pick: impl Fn(StyleId) + 'static,
) -> gtk::FlowBox {
    let items = styles
        .iter()
        .map(|s| (picture(&s.look).upcast(), tr(&s.name)))
        .collect();
    let chosen = styles.iter().position(|s| Some(s.id) == current);
    let ids: Vec<StyleId> = styles.iter().map(|s| s.id).collect();
    pictures(&tr("Table Styles"), items, chosen, move |i| {
        if let Some(&id) = ids.get(i) {
            pick(id);
        }
    })
}
