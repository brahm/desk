// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Colors popover of window M+ (INT-16, step 2): the color rows of the
//! Format pane open it in place of the Colors window of iWork. It has the
//! palette of the GNOME HIG, five shades of nine hues, the eyedropper
//! that picks a color from the screen, and Custom…, which opens the color
//! chooser of GTK for any other color.
//!
//! Libadwaita has no color popover, so this one is built from GTK widgets
//! and recorded in the component table of
//! `docs/interface/proposals/README.md`, to be reviewed at each new
//! Libadwaita (AGENTS.md, item 4).
//!
//! A row keeps its `gtk::ColorDialogButton` as the holder of the color:
//! the panes read it, set it and follow its changes as before, and the
//! popover button takes its place in the row.

use adw::prelude::*;
use gtk::{gdk, gio};

use crate::formatting::labelled;
use crate::i18n::{n_, tr};

/// The palette of the GNOME HIG: for each hue its name and five shades,
/// lightest first.
const PALETTE: [(&str, [&str; 5]); 9] = [
    (
        n_("Blue"),
        ["#99c1f1", "#62a0ea", "#3584e4", "#1c71d8", "#1a5fb4"],
    ),
    (
        n_("Green"),
        ["#8ff0a4", "#57e389", "#33d17a", "#2ec27e", "#26a269"],
    ),
    (
        n_("Yellow"),
        ["#f9f06b", "#f8e45c", "#f6d32d", "#f5c211", "#e5a50a"],
    ),
    (
        n_("Orange"),
        ["#ffbe6f", "#ffa348", "#ff7800", "#e66100", "#c64600"],
    ),
    (
        n_("Red"),
        ["#f66151", "#ed333b", "#e01b24", "#c01c28", "#a51d2d"],
    ),
    (
        n_("Purple"),
        ["#dc8add", "#c061cb", "#9141ac", "#813d9c", "#613583"],
    ),
    (
        n_("Brown"),
        ["#cdab8f", "#b5835a", "#986a44", "#865e3c", "#63452c"],
    ),
    (
        n_("Light Gray"),
        ["#ffffff", "#f6f5f4", "#deddda", "#c0bfbc", "#9a9996"],
    ),
    (
        n_("Dark Gray"),
        ["#77767b", "#5e5c64", "#3d3846", "#241f31", "#000000"],
    ),
];

/// A square that shows a color.
fn swatch(color: gdk::RGBA, size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .build();
    set_swatch(&area, color);
    area
}

/// Draws `color` in a swatch, with a thin outline so that white shows.
fn set_swatch(area: &gtk::DrawingArea, color: gdk::RGBA) {
    area.set_draw_func(move |_, cr, width, height| {
        let (w, h) = (f64::from(width), f64::from(height));
        cr.rectangle(0.5, 0.5, w - 1.0, h - 1.0);
        cr.set_source_rgba(
            f64::from(color.red()),
            f64::from(color.green()),
            f64::from(color.blue()),
            f64::from(color.alpha()),
        );
        let _ = cr.fill_preserve();
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.25);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
    });
    area.queue_draw();
}

/// The button that takes the place of `holder` in a row: it shows the
/// color and opens the Colors popover, whose choice goes into `holder`.
/// It is named `name` for the accessibility tree.
pub fn button(holder: &gtk::ColorDialogButton, name: &str) -> gtk::MenuButton {
    let shown = swatch(holder.rgba(), 20);
    let button = gtk::MenuButton::builder()
        .child(&shown)
        .valign(gtk::Align::Center)
        .tooltip_text(name)
        .build();
    labelled(&button, name);
    {
        let shown = shown.downgrade();
        holder.connect_rgba_notify(move |holder| {
            if let Some(shown) = shown.upgrade() {
                set_swatch(&shown, holder.rgba());
            }
        });
    }

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(10)
        .margin_end(10)
        .build();
    content.append(
        &gtk::Label::builder()
            .label(name)
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    let popover = gtk::Popover::builder()
        .child(&content)
        .position(gtk::PositionType::Left)
        .build();
    let grid = gtk::Grid::builder()
        .row_spacing(4)
        .column_spacing(4)
        .build();
    for (column, (hue, shades)) in PALETTE.iter().enumerate() {
        for (row, hex) in shades.iter().enumerate() {
            let Ok(color) = gdk::RGBA::parse(*hex) else {
                continue;
            };
            let choice = gtk::Button::builder()
                .child(&swatch(color, 22))
                .css_classes(["flat"])
                .tooltip_text(format!("{} {}", tr(hue), row + 1))
                .build();
            labelled(&choice, &format!("{} {}", tr(hue), row + 1));
            let holder = holder.downgrade();
            let popover = popover.downgrade();
            choice.connect_clicked(move |_| {
                if let Some(holder) = holder.upgrade() {
                    holder.set_rgba(&color);
                }
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            });
            grid.attach(&choice, column as i32, row as i32, 1, 1);
        }
    }
    content.append(&grid);

    let actions = gtk::Box::builder().spacing(6).build();
    let picker = crate::eyedropper::button(holder);
    picker.remove_css_class("flat");
    {
        // The popover closes while the color is picked from the screen,
        // as it does on a color of the palette.
        let popover = popover.downgrade();
        picker.connect_clicked(move |_| {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
        });
    }
    actions.append(&picker);
    let custom = gtk::Button::builder()
        .label(tr("Custom…"))
        .hexpand(true)
        .build();
    {
        let holder = holder.downgrade();
        let popover = popover.downgrade();
        custom.connect_clicked(move |custom| {
            let Some(holder) = holder.upgrade() else {
                return;
            };
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
            let dialog = holder.dialog().unwrap_or_default();
            let root = custom.root().and_downcast::<gtk::Window>();
            let current = holder.rgba();
            let holder = holder.downgrade();
            dialog.choose_rgba(
                root.as_ref(),
                Some(&current),
                gio::Cancellable::NONE,
                move |chosen| {
                    if let (Ok(color), Some(holder)) = (chosen, holder.upgrade()) {
                        holder.set_rgba(&color);
                    }
                },
            );
        });
    }
    actions.append(&custom);
    content.append(&actions);
    button.set_popover(Some(&popover));
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_16_every_color_of_the_palette_parses() {
        for (_, shades) in PALETTE {
            for hex in shades {
                assert!(gdk::RGBA::parse(hex).is_ok(), "{hex}");
            }
        }
    }
}
