// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Chart Data Editor of iWork (GRF-02): the data of a chart as a
//! grid, with the names of the rows down the left and those of the
//! columns across the top, rows and columns to add and take out, and
//! whether each row or each column is a series.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::chart::{Chart, ChartData};
use desk_model::table::{Locale, Num, parse_number};

use crate::i18n::tr;

/// The grid of the editor, as its fields hold it.
struct Grid {
    grid: gtk::Grid,
    /// The names of the rows.
    rows: Vec<gtk::Entry>,
    /// The names of the columns.
    cols: Vec<gtk::Entry>,
    /// The values, by rows.
    values: Vec<Vec<gtk::Entry>>,
}

fn entry(text: &str, header: bool) -> gtk::Entry {
    let entry = gtk::Entry::builder()
        .text(text)
        .width_chars(8)
        .max_width_chars(12)
        .build();
    if header {
        entry.add_css_class("heading");
    } else {
        EditableExt::set_alignment(&entry, 1.0);
    }
    entry
}

fn number_text(value: Option<f64>, locale: &Locale) -> String {
    match value {
        Some(v) => {
            let text = format!("{v}");
            if locale.decimal == ',' {
                text.replace('.', ",")
            } else {
                text
            }
        }
        None => String::new(),
    }
}

impl Grid {
    fn new(data: &ChartData, locale: &Locale) -> Self {
        let grid = gtk::Grid::builder()
            .row_spacing(4)
            .column_spacing(4)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        let mut this = Self {
            grid,
            rows: Vec::new(),
            cols: Vec::new(),
            values: Vec::new(),
        };
        for name in &data.cols {
            this.push_col(name);
        }
        for (r, name) in data.rows.iter().enumerate() {
            this.push_row(name);
            for (c, entry) in this.values[r].iter().enumerate() {
                entry.set_text(&number_text(data.get(r, c), locale));
            }
        }
        this
    }

    fn push_col(&mut self, name: &str) {
        let c = self.cols.len() as i32;
        let e = entry(name, true);
        self.grid.attach(&e, c + 1, 0, 1, 1);
        self.cols.push(e);
        for (r, row) in self.values.iter_mut().enumerate() {
            let v = entry("", false);
            self.grid.attach(&v, c + 1, r as i32 + 1, 1, 1);
            row.push(v);
        }
    }

    fn push_row(&mut self, name: &str) {
        let r = self.rows.len() as i32;
        let e = entry(name, true);
        self.grid.attach(&e, 0, r + 1, 1, 1);
        self.rows.push(e);
        let row: Vec<gtk::Entry> = (0..self.cols.len())
            .map(|c| {
                let v = entry("", false);
                self.grid.attach(&v, c as i32 + 1, r + 1, 1, 1);
                v
            })
            .collect();
        self.values.push(row);
    }

    fn pop_row(&mut self) {
        if self.rows.len() <= 1 {
            return;
        }
        if let Some(e) = self.rows.pop() {
            self.grid.remove(&e);
        }
        for v in self.values.pop().unwrap_or_default() {
            self.grid.remove(&v);
        }
    }

    fn pop_col(&mut self) {
        if self.cols.len() <= 1 {
            return;
        }
        if let Some(e) = self.cols.pop() {
            self.grid.remove(&e);
        }
        for row in &mut self.values {
            if let Some(v) = row.pop() {
                self.grid.remove(&v);
            }
        }
    }

    fn data(&self, locale: &Locale) -> ChartData {
        ChartData {
            rows: self.rows.iter().map(|e| e.text().to_string()).collect(),
            cols: self.cols.iter().map(|e| e.text().to_string()).collect(),
            values: self
                .values
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|e| parse_number(&e.text(), locale).map(|(v, _)| Num(v)))
                        .collect()
                })
                .collect(),
        }
    }
}

/// Opens the editor of the data of a chart; `done` gets the data,
/// whether the rows are the series and whether the first series holds the
/// x values of the others (W142) when it closes with Done.
pub fn open(
    parent: Option<&gtk::Window>,
    chart: &Chart,
    locale: Locale,
    done: impl Fn(ChartData, bool, bool) + 'static,
) {
    let grid = Rc::new(RefCell::new(Grid::new(&chart.data, &locale)));
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&grid.borrow().grid)
        .vexpand(true)
        .hexpand(true)
        .build();
    let button = |icon: &str, tip: String| {
        let b = gtk::Button::builder()
            .icon_name(icon)
            .tooltip_text(tip.as_str())
            .build();
        b.update_property(&[gtk::accessible::Property::Label(&tip)]);
        b
    };
    let add_row = button("desk-add-row-symbolic", tr("Add Row"));
    let remove_row = button("desk-remove-row-symbolic", tr("Delete Row"));
    let add_col = button("desk-add-column-symbolic", tr("Add Column"));
    let remove_col = button("desk-remove-column-symbolic", tr("Delete Column"));
    for (b, fallback) in [
        (&add_row, "list-add-symbolic"),
        (&remove_row, "list-remove-symbolic"),
        (&add_col, "list-add-symbolic"),
        (&remove_col, "list-remove-symbolic"),
    ] {
        let theme = gtk::IconTheme::for_display(&b.display());
        if !theme.has_icon(&b.icon_name().unwrap_or_default()) {
            b.set_icon_name(fallback);
        }
    }
    let in_rows = adw::ToggleGroup::builder().css_classes(["round"]).build();
    for (name, label) in [
        ("rows", tr("Series in Rows")),
        ("cols", tr("Series in Columns")),
    ] {
        in_rows.add(adw::Toggle::builder().name(name).label(label).build());
    }
    in_rows.set_active_name(Some(if chart.series_in_rows { "rows" } else { "cols" }));
    let bar = gtk::Box::builder()
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .build();
    for b in [&add_row, &remove_row, &add_col, &remove_col] {
        b.add_css_class("flat");
        bar.append(b);
    }
    let spacer = gtk::Box::builder().hexpand(true).build();
    bar.append(&spacer);
    // Share X Values, as the Data Editor of Pages 15.4 has it for scatter
    // and bubble charts: the first series is the x of all the others.
    let share_x = gtk::CheckButton::builder()
        .label(tr("Share X Values"))
        .active(chart.share_x)
        .visible(chart.kind.has_x_values())
        .build();
    bar.append(&share_x);
    bar.append(&in_rows);
    {
        let grid = grid.clone();
        add_row.connect_clicked(move |_| {
            let n = grid.borrow().rows.len() + 1;
            grid.borrow_mut().push_row(&format!("{} {n}", tr("Row")));
        });
    }
    {
        let grid = grid.clone();
        remove_row.connect_clicked(move |_| grid.borrow_mut().pop_row());
    }
    {
        let grid = grid.clone();
        add_col.connect_clicked(move |_| {
            let n = grid.borrow().cols.len() + 1;
            grid.borrow_mut().push_col(&format!("{} {n}", tr("Column")));
        });
    }
    {
        let grid = grid.clone();
        remove_col.connect_clicked(move |_| grid.borrow_mut().pop_col());
    }
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    content.append(&bar);
    content.append(&scrolled);
    let header = adw::HeaderBar::new();
    let apply = gtk::Button::builder()
        .label(tr("Done"))
        .css_classes(["suggested-action"])
        .build();
    header.pack_end(&apply);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .title(tr("Chart Data"))
        .content_width(560)
        .content_height(360)
        .child(&view)
        .build();
    let closing = dialog.downgrade();
    apply.connect_clicked(move |_| {
        let data = grid.borrow().data(&locale);
        done(
            data,
            in_rows.active_name().as_deref() == Some("rows"),
            share_x.is_active(),
        );
        if let Some(dialog) = closing.upgrade() {
            dialog.close();
        }
    });
    dialog.present(parent);
}
