// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The variants of the format bar for selected cells and for a selected
//! chart (INT-12), as the plan of interface B draws them: for cells, the
//! data format, the text, its alignment, the fill, merging and the
//! Function Browser; for a chart, its kind, its data, the title, the
//! legend and 3D.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::chart::ChartKind;
use desk_model::table::{CellFill, Color, HAlign};

use crate::editing::TextEditor;
use crate::formatting::{EditorOf, labelled};
use crate::i18n::{n_, tr};
use crate::table_pane::{FORMATS, format_at, format_index, from_gdk, to_gdk};

/// What a toggle does to the text of the cells.
type TextChange = fn(&mut desk_model::table::TextStyle, bool);

/// The name of the variant of cells in the format bar.
pub const CELLS: &str = "cells";
/// The name of the variant of a chart in the format bar.
pub const CHART: &str = "chart";

/// The alignments of the text of cells, as in the menu of the Inspector.
const ALIGNMENTS: [(&str, HAlign, &str); 3] = [
    (
        "format-justify-left-symbolic",
        HAlign::Left,
        n_("Align Left"),
    ),
    (
        "format-justify-center-symbolic",
        HAlign::Center,
        n_("Center"),
    ),
    (
        "format-justify-right-symbolic",
        HAlign::Right,
        n_("Align Right"),
    ),
];

fn dropdown(items: &[&str], tip: &str) -> gtk::DropDown {
    let names: Vec<String> = items.iter().map(|s| tr(s)).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let d = gtk::DropDown::from_strings(&names);
    d.set_tooltip_text(Some(&tr(tip)));
    labelled(&d, &tr(tip));
    d
}

fn toggle(icon: &str, tip: &str) -> gtk::ToggleButton {
    let b = gtk::ToggleButton::builder()
        .icon_name(icon)
        .tooltip_text(tr(tip))
        .build();
    labelled(&b, &tr(tip));
    b
}

fn action_button(icon: &str, action: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .icon_name(icon)
        .action_name(action)
        .tooltip_text(tr(tip))
        .build();
    labelled(&b, &tr(tip));
    b
}

fn color_button(title: &str) -> gtk::ColorDialogButton {
    let b = gtk::ColorDialogButton::new(Some(
        gtk::ColorDialog::builder()
            .title(tr(title))
            .modal(true)
            .with_alpha(false)
            .build(),
    ));
    b.set_tooltip_text(Some(&tr(title)));
    labelled(&b, &tr(title));
    b
}

/// What a control of a bar does to the cells or the chart.
type Run = Rc<dyn Fn(&TableBars, &TextEditor)>;

/// The variants of cells and of a chart, and what they show.
pub struct TableBars {
    cells: gtk::Box,
    chart: gtk::Box,
    format: gtk::DropDown,
    bold: gtk::ToggleButton,
    italic: gtk::ToggleButton,
    underline: gtk::ToggleButton,
    text_color: gtk::ColorDialogButton,
    align: [gtk::ToggleButton; 3],
    fill: gtk::ColorDialogButton,
    kind: gtk::DropDown,
    title: gtk::ToggleButton,
    legend: gtk::ToggleButton,
    three_d: gtk::ToggleButton,
    updating: Cell<bool>,
}

impl TableBars {
    /// The variants, which change the cells or the chart of `editor`.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let cells = gtk::Box::builder().spacing(6).build();
        let format = dropdown(&FORMATS, n_("Data Format"));
        cells.append(&format);
        let text = gtk::Box::builder().css_classes(["linked"]).build();
        let bold = toggle("format-text-bold-symbolic", n_("Bold"));
        let italic = toggle("format-text-italic-symbolic", n_("Italic"));
        let underline = toggle("format-text-underline-symbolic", n_("Underline"));
        for b in [&bold, &italic, &underline] {
            text.append(b);
        }
        cells.append(&text);
        let text_color = color_button(n_("Text Color"));
        cells.append(&text_color);
        let aligns = gtk::Box::builder().css_classes(["linked"]).build();
        let align = ALIGNMENTS.map(|(icon, _, tip)| toggle(icon, tip));
        for b in &align {
            aligns.append(b);
        }
        cells.append(&aligns);
        let fill = color_button(n_("Cell Fill"));
        cells.append(&fill);
        let rows = gtk::Box::builder().css_classes(["linked"]).build();
        rows.append(&action_button(
            "desk-add-row-symbolic",
            "win.table-add-row-below",
            n_("Add Row Below"),
        ));
        rows.append(&action_button(
            "desk-add-column-symbolic",
            "win.table-add-column-after",
            n_("Add Column After"),
        ));
        cells.append(&rows);
        cells.append(&action_button(
            "desk-merge-cells-symbolic",
            "win.table-merge-cells",
            n_("Merge Cells"),
        ));
        cells.append(&action_button(
            "desk-function-symbolic",
            "win.function-browser",
            n_("Function Browser"),
        ));

        let chart = gtk::Box::builder().spacing(6).build();
        let kind = dropdown(&crate::chart_pane::KINDS, n_("Chart Type"));
        chart.append(&kind);
        let data = gtk::Button::builder()
            .label(tr("Edit Data"))
            .action_name("win.edit-chart-data")
            .build();
        chart.append(&data);
        let shows = gtk::Box::builder().css_classes(["linked"]).build();
        let title = gtk::ToggleButton::builder().label(tr("Title")).build();
        let legend = gtk::ToggleButton::builder().label(tr("Legend")).build();
        let three_d = gtk::ToggleButton::builder().label(tr("3D")).build();
        for b in [&title, &legend, &three_d] {
            shows.append(b);
        }
        chart.append(&shows);

        // Icons the theme lacks fall back to generic ones.
        for b in [&cells, &chart] {
            let mut child = b.first_child();
            while let Some(widget) = child {
                if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                    fallback_icon(button);
                }
                let mut inner = widget.first_child();
                while let Some(w) = inner {
                    if let Some(button) = w.downcast_ref::<gtk::Button>() {
                        fallback_icon(button);
                    }
                    inner = w.next_sibling();
                }
                child = widget.next_sibling();
            }
        }

        let this = Rc::new(Self {
            cells,
            chart,
            format,
            bold,
            italic,
            underline,
            text_color,
            align,
            fill,
            kind,
            title,
            legend,
            three_d,
            updating: Cell::new(false),
        });
        this.connect(&editor);
        this
    }

    /// The variant of cells.
    pub fn cells(&self) -> &gtk::Box {
        &self.cells
    }

    /// The variant of a chart.
    pub fn chart(&self) -> &gtk::Box {
        &self.chart
    }

    fn connect(self: &Rc<Self>, editor: &EditorOf) {
        let run = |this: &Rc<Self>, f: Run| {
            let weak = Rc::downgrade(this);
            let editor = editor.clone();
            move || {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    f(&this, &editor);
                }
            }
        };
        let f = run(
            self,
            Rc::new(|this, editor| {
                let symbol = editor.table_locale().currency.clone();
                editor.set_cell_format(format_at(this.format.selected(), None, false, &symbol));
            }),
        );
        self.format.connect_selected_notify(move |_| f());
        let texts: [(&gtk::ToggleButton, &str, TextChange); 3] = [
            (&self.bold, "Bold", |t, on| t.bold = on),
            (&self.italic, "Italic", |t, on| t.italic = on),
            (&self.underline, "Underline", |t, on| t.underline = on),
        ];
        for (button, name, change) in texts {
            let weak = Rc::downgrade(self);
            let editor = editor.clone();
            button.connect_toggled(move |button| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    let on = button.is_active();
                    editor.set_cell_text(name, |t| change(t, on));
                }
            });
        }
        let f = run(
            self,
            Rc::new(|this, editor| {
                let color = from_gdk(&this.text_color.rgba());
                editor.set_cell_text("Text Color", |t| t.color = Some(color));
            }),
        );
        self.text_color.connect_rgba_notify(move |_| f());
        for (button, (_, h_align, _)) in self.align.iter().zip(ALIGNMENTS) {
            let f = run(
                self,
                Rc::new(move |_, editor| {
                    editor.set_cell_style("Alignment", |s| s.h_align = h_align);
                }),
            );
            button.connect_clicked(move |_| f());
        }
        let f = run(
            self,
            Rc::new(|this, editor| {
                let fill = CellFill::Color(from_gdk(&this.fill.rgba()));
                editor.set_cell_style("Fill", |s| s.fill = Some(fill.clone()));
            }),
        );
        self.fill.connect_rgba_notify(move |_| f());

        let f = run(
            self,
            Rc::new(|this, editor| {
                if let Some(kind) = ChartKind::ALL.get(this.kind.selected() as usize) {
                    editor.change_chart("Chart Type", |c| c.set_kind(*kind));
                }
            }),
        );
        self.kind.connect_selected_notify(move |_| f());
        let f = run(
            self,
            Rc::new(|this, editor| {
                let on = this.title.is_active();
                editor.change_chart("Title", |c| {
                    c.title = on.then(|| c.title.clone().unwrap_or_else(|| tr("Title")));
                });
            }),
        );
        self.title.connect_toggled(move |_| f());
        let f = run(
            self,
            Rc::new(|this, editor| {
                let on = this.legend.is_active();
                editor.change_chart("Legend", |c| c.legend = on);
            }),
        );
        self.legend.connect_toggled(move |_| f());
        let f = run(
            self,
            Rc::new(|this, editor| {
                let on = this.three_d.is_active();
                editor.change_chart("3D", |c| c.three_d = on && c.kind.has_3d());
            }),
        );
        self.three_d.connect_toggled(move |_| f());
    }

    /// Shows the selected cells or chart; the name of the variant to
    /// show, if either is selected.
    pub fn show(&self, editor: &TextEditor) -> Option<&'static str> {
        self.updating.set(true);
        let shown = if let Some((table, area)) = editor.table_selection() {
            let cell = table.cell(area.row, area.col);
            self.format.set_selected(format_index(&cell.format));
            let style = &cell.style;
            self.bold.set_active(style.text.bold);
            self.italic.set_active(style.text.italic);
            self.underline.set_active(style.text.underline);
            let color = to_gdk(style.text.color.unwrap_or(Color::BLACK));
            if self.text_color.rgba() != color {
                self.text_color.set_rgba(&color);
            }
            for (button, (_, h_align, _)) in self.align.iter().zip(ALIGNMENTS) {
                button.set_active(style.h_align == h_align);
            }
            // A cell with no fill shows white, which is what it looks like.
            let fill = match &style.fill {
                Some(CellFill::Color(c)) => to_gdk(*c),
                _ => to_gdk(Color::WHITE),
            };
            if self.fill.rgba() != fill {
                self.fill.set_rgba(&fill);
            }
            Some(CELLS)
        } else if let Some(chart) = editor.selected_chart() {
            let index = ChartKind::ALL
                .iter()
                .position(|k| *k == chart.kind)
                .unwrap_or(0);
            self.kind.set_selected(index as u32);
            self.title.set_active(chart.title.is_some());
            self.legend.set_active(chart.legend);
            self.three_d.set_active(chart.three_d);
            self.three_d.set_sensitive(chart.kind.has_3d());
            Some(CHART)
        } else {
            None
        };
        self.updating.set(false);
        shown
    }
}

fn fallback_icon(button: &gtk::Button) {
    let Some(name) = button.icon_name() else {
        return;
    };
    let theme = gtk::IconTheme::for_display(&button.display());
    if theme.has_icon(&name) {
        return;
    }
    let fallback = match name.as_str() {
        "desk-add-row-symbolic" | "desk-add-column-symbolic" => "list-add-symbolic",
        "desk-merge-cells-symbolic" => "view-grid-symbolic",
        "desk-function-symbolic" => "accessories-calculator-symbolic",
        _ => return,
    };
    button.set_icon_name(fallback);
}
