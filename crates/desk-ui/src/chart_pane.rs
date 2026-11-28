// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Chart, Axis and Series tabs of the Inspector for a selected chart
//! (GRF-01 to GRF-20): the kind, 3D, title, legend, colors and data of
//! the chart; the scale, gridlines and labels of its axes; and the
//! colors, value labels, trendlines and error bars of each series.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::chart::{
    Chart, ChartKind, Collection, ErrorAmount, ErrorBars, ErrorDirection, LabelPosition, Lighting,
    Scale, SeriesFill, Symbol, TrendKind, Trendline,
};
use desk_model::table::Num;

use crate::compact;
use crate::editing::TextEditor;
use crate::formatting::EditorOf;
use crate::i18n::{n_, tr};
use crate::table_pane::{from_gdk, to_gdk};

/// The kinds of the menu, in the order of [`ChartKind::ALL`].
pub(crate) const KINDS: [&str; 14] = [
    n_("Column"),
    n_("Stacked Column"),
    n_("Bar"),
    n_("Stacked Bar"),
    n_("Line"),
    n_("Area"),
    n_("Stacked Area"),
    n_("Pie"),
    n_("Donut"),
    n_("Scatter"),
    n_("Bubble"),
    n_("Radar"),
    n_("Mixed"),
    n_("2-Axis"),
];

const COLLECTIONS: [&str; 5] = [
    n_("Default"),
    n_("Pastel"),
    n_("Gray"),
    n_("Gradients"),
    n_("Textures"),
];

const TRENDS: [&str; 7] = [
    n_("None"),
    n_("Linear"),
    n_("Logarithmic"),
    n_("Polynomial"),
    n_("Power"),
    n_("Exponential"),
    n_("Moving Average"),
];

const ERRORS: [&str; 5] = [
    n_("None"),
    n_("Fixed Value"),
    n_("Percentage"),
    n_("Standard Deviation"),
    n_("Standard Error"),
];

fn trend_index(trend: Option<&Trendline>) -> u32 {
    match trend.map(|t| t.kind) {
        None => 0,
        Some(TrendKind::Linear) => 1,
        Some(TrendKind::Logarithmic) => 2,
        Some(TrendKind::Polynomial(_)) => 3,
        Some(TrendKind::Power) => 4,
        Some(TrendKind::Exponential) => 5,
        Some(TrendKind::MovingAverage(_)) => 6,
    }
}

fn trend_kind(index: u32, order: u8) -> Option<TrendKind> {
    Some(match index {
        1 => TrendKind::Linear,
        2 => TrendKind::Logarithmic,
        3 => TrendKind::Polynomial(order.clamp(2, 6)),
        4 => TrendKind::Power,
        5 => TrendKind::Exponential,
        6 => TrendKind::MovingAverage(order.max(2)),
        _ => return None,
    })
}

fn error_index(errors: Option<&ErrorBars>) -> (u32, f64) {
    match errors.map(|e| &e.amount) {
        None => (0, 1.0),
        Some(ErrorAmount::Fixed(n)) => (1, n.0),
        Some(ErrorAmount::Percent(n)) => (2, n.0),
        Some(ErrorAmount::StandardDeviation(n)) => (3, n.0),
        Some(ErrorAmount::StandardError) => (4, 1.0),
        Some(ErrorAmount::Custom(_)) => (1, 1.0),
    }
}

fn error_amount(index: u32, value: f64) -> Option<ErrorAmount> {
    Some(match index {
        1 => ErrorAmount::Fixed(Num(value)),
        2 => ErrorAmount::Percent(Num(value)),
        3 => ErrorAmount::StandardDeviation(Num(value)),
        4 => ErrorAmount::StandardError,
        _ => return None,
    })
}

/// The groups of the tabs of a selected chart.
pub struct ChartGroups {
    /// The chart styles in view, with Copy, Paste and Create (W080).
    styles: compact::Group,
    /// Where the pictures of the styles go.
    style_holder: gtk::Box,
    /// What the pictures show: the styles and the chosen one.
    shown_styles: std::cell::RefCell<Option<(Vec<desk_model::ChartStyle>, Option<usize>)>>,
    paste_style: gtk::Button,
    editor: EditorOf,
    chart: compact::Group,
    scene: compact::Group,
    value_axis: compact::Group,
    category_axis: compact::Group,
    series: compact::Group,
    kind: compact::ComboRow,
    three_d: compact::SwitchRow,
    title_on: compact::SwitchRow,
    title: compact::EntryRow,
    legend: compact::SwitchRow,
    colors: compact::ComboRow,
    gap: compact::SpinRow,
    percentages: compact::SwitchRow,
    inner_radius: compact::SpinRow,
    corners: compact::SpinRow,
    outside_corners: compact::SwitchRow,
    share_x: compact::SwitchRow,
    radar_circular: compact::SwitchRow,
    bubble_scale: compact::SpinRow,
    by_diameter: compact::SwitchRow,
    negative_bubbles: compact::SwitchRow,
    depth: compact::SpinRow,
    lighting: compact::ComboRow,
    bevel: compact::SwitchRow,
    min_on: compact::SwitchRow,
    min: compact::SpinRow,
    max_on: compact::SwitchRow,
    max: compact::SpinRow,
    steps: compact::SpinRow,
    scale: compact::ComboRow,
    gridlines: compact::SwitchRow,
    value_labels: compact::SwitchRow,
    decimals: compact::SpinRow,
    axis_title: compact::EntryRow,
    category_labels: compact::SwitchRow,
    category_gridlines: compact::SwitchRow,
    category_title: compact::EntryRow,
    which: compact::ComboRow,
    color: gtk::ColorDialogButton,
    labels: compact::SwitchRow,
    position: compact::ComboRow,
    symbol: compact::ComboRow,
    trend: compact::ComboRow,
    order: compact::SpinRow,
    equation: compact::SwitchRow,
    r_squared: compact::SwitchRow,
    errors: compact::ComboRow,
    error_value: compact::SpinRow,
    updating: Cell<bool>,
}

impl ChartGroups {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let switch = |title: &str| compact::SwitchRow::builder().title(tr(title)).build();
        let spin = |title: &str, min: f64, max: f64, step: f64| {
            let row = compact::SpinRow::with_range(min, max, step);
            row.set_title(&tr(title));
            row
        };
        let combo = |title: &str, items: &[&str]| {
            let names: Vec<String> = items.iter().map(|s| tr(s)).collect();
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            compact::ComboRow::builder()
                .title(tr(title))
                .model(&gtk::StringList::new(&names))
                .build()
        };
        let kind = combo(n_("Chart Type"), &KINDS);
        let three_d = switch(n_("3D"));
        let title_on = switch(n_("Title"));
        let title = compact::EntryRow::builder().title(tr("Title Text")).build();
        let legend = switch(n_("Legend"));
        let colors = combo(n_("Chart Colors"), &COLLECTIONS);
        let gap = spin(n_("Gap Between Bars"), 0.0, 500.0, 5.0);
        gap.set_subtitle(&tr("Percent"));
        let percentages = switch(n_("Values as Percentages"));
        // The shapes of Pages 15.4 (W141, W142, W145).
        let inner_radius = spin(n_("Inner Radius"), 0.0, 95.0, 5.0);
        inner_radius.set_subtitle(&tr("Percent"));
        let corners = spin(n_("Rounded Corners"), 0.0, 50.0, 5.0);
        corners.set_subtitle(&tr("Percent"));
        let outside_corners = switch(n_("Outside Corners Only"));
        let share_x = switch(n_("Share X Values"));
        let radar_circular = switch(n_("Circular Grid"));
        let bubble_scale = spin(n_("Bubble Size"), 10.0, 300.0, 10.0);
        bubble_scale.set_subtitle(&tr("Percent"));
        let by_diameter = switch(n_("Size by Diameter"));
        let negative_bubbles = switch(n_("Show Negative Bubbles"));
        let edit = gtk::Button::builder()
            .label(tr("Edit Data"))
            .action_name("win.edit-chart-data")
            .valign(gtk::Align::Center)
            .build();
        let data = compact::ActionRow::builder()
            .title(tr("Chart Data"))
            .activatable_widget(&edit)
            .build();
        data.add_suffix(&edit);
        let chart = compact::Group::builder().title(tr("Chart")).build();
        chart.add(&kind);
        chart.add(&data);
        chart.add(&title_on);
        chart.add(&title);
        chart.add(&legend);
        chart.add(&colors);
        chart.add(&gap);
        chart.add(&corners);
        chart.add(&outside_corners);
        chart.add(&inner_radius);
        chart.add(&percentages);
        chart.add(&share_x);
        chart.add(&bubble_scale);
        chart.add(&by_diameter);
        chart.add(&negative_bubbles);
        chart.add(&radar_circular);
        chart.add(&three_d);
        let depth = spin(n_("Depth"), 10.0, 400.0, 10.0);
        depth.set_subtitle(&tr("Percent"));
        let lighting = combo(n_("Lighting"), &[n_("Soft"), n_("Strong"), n_("Flat")]);
        let bevel = switch(n_("Bevel Edges"));
        let scene = compact::Group::builder().title(tr("3D Scene")).build();
        scene.add(&depth);
        scene.add(&lighting);
        scene.add(&bevel);

        let min_on = switch(n_("Fixed Minimum"));
        let min = spin(n_("Minimum"), -1e12, 1e12, 1.0);
        let max_on = switch(n_("Fixed Maximum"));
        let max = spin(n_("Maximum"), -1e12, 1e12, 1.0);
        let steps = spin(n_("Steps"), 0.0, 50.0, 1.0);
        steps.set_subtitle(&tr("0 for automatic"));
        let scale = combo(
            n_("Scale"),
            &[n_("Linear"), n_("Logarithmic"), n_("Percentage")],
        );
        let gridlines = switch(n_("Gridlines"));
        let value_labels = switch(n_("Value Labels"));
        let decimals = spin(n_("Decimals"), -1.0, 10.0, 1.0);
        decimals.set_subtitle(&tr("-1 for automatic"));
        let axis_title = compact::EntryRow::builder().title(tr("Axis Title")).build();
        let value_axis = compact::Group::builder().title(tr("Value Axis")).build();
        for row in [&min_on, &max_on] {
            value_axis.add(row);
        }
        value_axis.add(&min);
        value_axis.add(&max);
        value_axis.add(&steps);
        value_axis.add(&scale);
        value_axis.add(&gridlines);
        value_axis.add(&value_labels);
        value_axis.add(&decimals);
        value_axis.add(&axis_title);
        let category_labels = switch(n_("Category Labels"));
        let category_gridlines = switch(n_("Gridlines"));
        let category_title = compact::EntryRow::builder().title(tr("Axis Title")).build();
        let category_axis = compact::Group::builder().title(tr("Category Axis")).build();
        category_axis.add(&category_labels);
        category_axis.add(&category_gridlines);
        category_axis.add(&category_title);

        let which = compact::ComboRow::builder()
            .title(tr("Series"))
            .model(&gtk::StringList::new(&[]))
            .build();
        let color = gtk::ColorDialogButton::builder()
            .dialog(&gtk::ColorDialog::new())
            .valign(gtk::Align::Center)
            .build();
        which.add_suffix(&color);
        let labels = switch(n_("Value Labels"));
        let position = combo(
            n_("Label Position"),
            &[
                n_("Outside"),
                n_("Inside End"),
                n_("Middle"),
                n_("Inside Base"),
            ],
        );
        let symbol = combo(
            n_("Data Symbol"),
            &[
                n_("None"),
                n_("Circle"),
                n_("Triangle"),
                n_("Square"),
                n_("Diamond"),
            ],
        );
        let trend = combo(n_("Trendline"), &TRENDS);
        let order = spin(n_("Order or Period"), 2.0, 6.0, 1.0);
        let equation = switch(n_("Show Equation"));
        let r_squared = switch(n_("Show R²"));
        let errors = combo(n_("Error Bars"), &ERRORS);
        let error_value = spin(n_("Amount"), 0.0, 1e9, 1.0);
        error_value.set_digits(2);
        let series = compact::Group::builder().title(tr("Series")).build();
        series.add(&which);
        series.add(&labels);
        series.add(&position);
        series.add(&symbol);
        series.add(&trend);
        series.add(&order);
        series.add(&equation);
        series.add(&r_squared);
        series.add(&errors);
        series.add(&error_value);

        let style_holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let style_buttons = gtk::Box::builder()
            .spacing(6)
            .homogeneous(true)
            .margin_top(6)
            .build();
        let style_button = |label: &str| {
            let button = gtk::Button::with_label(&tr(label));
            style_buttons.append(&button);
            button
        };
        let copy_style = style_button(n_("Copy Style"));
        let paste_style = style_button(n_("Paste Style"));
        let create_style = style_button(n_("Create Style"));
        copy_style.set_tooltip_text(Some(&tr("Copy Chart Style")));
        paste_style.set_tooltip_text(Some(&tr("Paste Chart Style")));
        create_style.set_tooltip_text(Some(&tr("Create Chart Style")));
        let styles_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        styles_box.append(&style_holder);
        styles_box.append(&style_buttons);
        let styles = compact::Group::builder().title(tr("Chart Styles")).build();
        styles.add_widget(&styles_box);
        for (button, run) in [
            (&copy_style, TextEditor::copy_chart_style as fn(&TextEditor)),
            (&paste_style, TextEditor::paste_chart_style),
            (&create_style, TextEditor::create_chart_style),
        ] {
            let editor = editor.clone();
            button.connect_clicked(move |_| {
                if let Some(editor) = editor() {
                    run(&editor);
                }
            });
        }

        let this = Rc::new(Self {
            styles,
            style_holder,
            shown_styles: std::cell::RefCell::new(None),
            paste_style,
            editor: editor.clone(),
            chart,
            scene,
            value_axis,
            category_axis,
            series,
            kind,
            three_d,
            title_on,
            title,
            legend,
            colors,
            gap,
            percentages,
            inner_radius,
            corners,
            outside_corners,
            share_x,
            radar_circular,
            bubble_scale,
            by_diameter,
            negative_bubbles,
            depth,
            lighting,
            bevel,
            min_on,
            min,
            max_on,
            max,
            steps,
            scale,
            gridlines,
            value_labels,
            decimals,
            axis_title,
            category_labels,
            category_gridlines,
            category_title,
            which,
            color,
            labels,
            position,
            symbol,
            trend,
            order,
            equation,
            r_squared,
            errors,
            error_value,
            updating: Cell::new(false),
        });
        this.connect(&editor);
        this
    }

    /// The groups: Chart Styles, Chart and 3D Scene for the Chart tab,
    /// the axes for the Axis tab and Series for the Series tab.
    pub fn groups(&self) -> [&compact::Group; 6] {
        [
            &self.styles,
            &self.chart,
            &self.scene,
            &self.value_axis,
            &self.category_axis,
            &self.series,
        ]
    }

    /// The series the Series group shows.
    fn series_index(&self) -> usize {
        let i = self.which.selected();
        if i == gtk::INVALID_LIST_POSITION {
            0
        } else {
            i as usize
        }
    }

    fn connect(self: &Rc<Self>, editor: &EditorOf) {
        type Change = fn(&ChartGroups, &mut Chart);
        let make = |this: &Rc<Self>, name: &'static str, f: Change| {
            let weak = Rc::downgrade(this);
            let editor = editor.clone();
            move || {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    editor.change_chart(name, |chart| f(&this, chart));
                    this.show(&editor);
                }
            }
        };
        let combos: [(&compact::ComboRow, &'static str, Change); 8] = [
            (&self.kind, "Chart Type", |this, c| {
                if let Some(kind) = ChartKind::ALL.get(this.kind.selected() as usize) {
                    c.set_kind(*kind);
                }
            }),
            (&self.colors, "Chart Colors", |this, c| {
                if let Some(collection) = Collection::ALL.get(this.colors.selected() as usize) {
                    c.apply_collection(*collection);
                }
            }),
            (&self.lighting, "Lighting", |this, c| {
                c.scene.lighting = match this.lighting.selected() {
                    1 => Lighting::Strong,
                    2 => Lighting::Flat,
                    _ => Lighting::Soft,
                };
            }),
            (&self.scale, "Scale", |this, c| {
                c.value_axis.scale = match this.scale.selected() {
                    1 => Scale::Logarithmic,
                    2 => Scale::Percent,
                    _ => Scale::Linear,
                };
            }),
            (&self.position, "Value Labels", |this, c| {
                let s = this.series_index();
                series_style(c, s).labels.position = match this.position.selected() {
                    1 => LabelPosition::InsideEnd,
                    2 => LabelPosition::Middle,
                    3 => LabelPosition::InsideBase,
                    _ => LabelPosition::Outside,
                };
            }),
            (&self.symbol, "Data Symbol", |this, c| {
                let s = this.series_index();
                series_style(c, s).symbol = match this.symbol.selected() {
                    1 => Symbol::Circle,
                    2 => Symbol::Triangle,
                    3 => Symbol::Square,
                    4 => Symbol::Diamond,
                    _ => Symbol::None,
                };
            }),
            (&self.trend, "Trendline", set_trend),
            (&self.errors, "Error Bars", set_errors),
        ];
        for (row, name, f) in combos {
            let f = make(self, name, f);
            row.connect_selected_notify(move |_| f());
        }
        let switches: [(&compact::SwitchRow, &'static str, Change); 19] = [
            (&self.outside_corners, "Rounded Corners", |this, c| {
                c.outside_corners_only = this.outside_corners.is_active();
            }),
            (&self.share_x, "Share X Values", |this, c| {
                c.share_x = this.share_x.is_active();
            }),
            (&self.radar_circular, "Radar Grid", |this, c| {
                c.radar_circular = this.radar_circular.is_active();
            }),
            (&self.by_diameter, "Bubble Size", |this, c| {
                c.bubble.by_diameter = this.by_diameter.is_active();
            }),
            (&self.negative_bubbles, "Bubble Size", |this, c| {
                c.bubble.negative = this.negative_bubbles.is_active();
            }),
            (&self.three_d, "3D", |this, c| {
                c.three_d = this.three_d.is_active()
            }),
            (&self.title_on, "Title", |this, c| {
                c.title = this.title_on.is_active().then(|| {
                    let t = this.title.text().to_string();
                    if t.is_empty() { tr("Title") } else { t }
                });
            }),
            (&self.legend, "Legend", |this, c| {
                c.legend = this.legend.is_active()
            }),
            (&self.percentages, "Percentages", |this, c| {
                c.pie.percentages = this.percentages.is_active();
            }),
            (&self.bevel, "Bevel Edges", |this, c| {
                c.scene.bevel = this.bevel.is_active()
            }),
            (&self.min_on, "Axis Scale", |this, c| {
                c.value_axis.min = this.min_on.is_active().then(|| Num(this.min.value()));
            }),
            (&self.max_on, "Axis Scale", |this, c| {
                c.value_axis.max = this.max_on.is_active().then(|| Num(this.max.value()));
            }),
            (&self.gridlines, "Gridlines", |this, c| {
                c.value_axis.gridlines = this.gridlines.is_active();
            }),
            (&self.value_labels, "Axis Labels", |this, c| {
                c.value_axis.labels = this.value_labels.is_active();
            }),
            (&self.category_labels, "Axis Labels", |this, c| {
                c.category_axis.labels = this.category_labels.is_active();
            }),
            (&self.category_gridlines, "Gridlines", |this, c| {
                c.category_axis.gridlines = this.category_gridlines.is_active();
            }),
            (&self.labels, "Value Labels", |this, c| {
                let s = this.series_index();
                series_style(c, s).labels.show = this.labels.is_active();
            }),
            (&self.equation, "Trendline", set_trend),
            (&self.r_squared, "Trendline", set_trend),
        ];
        for (row, name, f) in switches {
            let f = make(self, name, f);
            row.connect_active_notify(move |_| f());
        }
        let spins: [(&compact::SpinRow, &'static str, Change); 11] = [
            (&self.inner_radius, "Inner Radius", |this, c| {
                c.inner_radius = this.inner_radius.value() as u8;
            }),
            (&self.corners, "Rounded Corners", |this, c| {
                c.corner_radius = this.corners.value() as u8;
            }),
            (&self.bubble_scale, "Bubble Size", |this, c| {
                c.bubble.scale = this.bubble_scale.value() as u16;
            }),
            (&self.gap, "Gap Between Bars", |this, c| {
                c.bar_gap = this.gap.value() as u16
            }),
            (&self.depth, "Depth", |this, c| {
                c.scene.depth = this.depth.value() as u16
            }),
            (&self.min, "Axis Scale", |this, c| {
                if this.min_on.is_active() {
                    c.value_axis.min = Some(Num(this.min.value()));
                }
            }),
            (&self.max, "Axis Scale", |this, c| {
                if this.max_on.is_active() {
                    c.value_axis.max = Some(Num(this.max.value()));
                }
            }),
            (&self.steps, "Axis Scale", |this, c| {
                let n = this.steps.value() as u8;
                c.value_axis.steps = (n > 0).then_some(n);
            }),
            (&self.decimals, "Number Format", |this, c| {
                let d = this.decimals.value();
                c.value_axis.number.decimals = (d >= 0.0).then_some(d as u8);
            }),
            (&self.order, "Trendline", set_trend),
            (&self.error_value, "Error Bars", set_errors),
        ];
        for (row, name, f) in spins {
            let f = make(self, name, f);
            row.connect_value_notify(move |_| f());
        }
        let entries: [(&compact::EntryRow, &'static str, Change); 3] = [
            (&self.title, "Title", |this, c| {
                let t = this.title.text().to_string();
                c.title = (!t.is_empty()).then_some(t);
            }),
            (&self.axis_title, "Axis Title", |this, c| {
                let t = this.axis_title.text().to_string();
                c.value_axis.title = (!t.is_empty()).then_some(t);
            }),
            (&self.category_title, "Axis Title", |this, c| {
                let t = this.category_title.text().to_string();
                c.category_axis.title = (!t.is_empty()).then_some(t);
            }),
        ];
        for (row, name, f) in entries {
            let f = make(self, name, f);
            row.connect_apply(move |_| f());
        }
        {
            let f = make(self, "Series Color", |this, c| {
                let s = this.series_index();
                series_style(c, s).fill = SeriesFill::Color(from_gdk(&this.color.rgba()));
            });
            self.color.connect_rgba_notify(move |_| f());
        }
        // Another series shows its settings.
        let weak = Rc::downgrade(self);
        let editor = editor.clone();
        self.which.connect_selected_notify(move |_| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = editor() {
                this.show(&editor);
            }
        });
    }

    /// Shows the selected chart.
    /// The pictures of the chart styles, with the one the chart looks
    /// like chosen; drawn again only when they change.
    fn show_styles(&self, editor: &TextEditor) {
        self.paste_style
            .set_sensitive(editor.can_paste_chart_style());
        let shown = (editor.chart_styles(), editor.current_chart_style());
        if self.shown_styles.borrow().as_ref() == Some(&shown) {
            return;
        }
        while let Some(child) = self.style_holder.first_child() {
            self.style_holder.remove(&child);
        }
        let editor_of = self.editor.clone();
        let grid = crate::chart_styles::grid(&shown.0, shown.1, move |style| {
            if let Some(editor) = editor_of() {
                editor.apply_chart_style(style);
            }
        });
        self.style_holder.append(&grid);
        *self.shown_styles.borrow_mut() = Some(shown);
    }

    /// Shows the selected chart.
    pub fn show(&self, editor: &TextEditor) {
        let Some(chart) = editor.selected_chart() else {
            return;
        };
        self.updating.set(true);
        // A locked chart is not edited either (PXV-16).
        let editable = !editor.selection_locked();
        for group in self.groups() {
            group.set_sensitive(editable);
        }
        self.show_styles(editor);
        let set_spin = |row: &compact::SpinRow, v: f64| {
            if (row.value() - v).abs() > 1e-9 {
                row.set_value(v);
            }
        };
        let set_switch = |row: &compact::SwitchRow, on: bool| {
            if row.is_active() != on {
                row.set_active(on);
            }
        };
        let set_entry = |row: &compact::EntryRow, text: &str| {
            if row.text() != text {
                row.set_text(text);
            }
        };
        let kind = ChartKind::ALL
            .iter()
            .position(|k| *k == chart.kind)
            .unwrap_or(0);
        self.kind.set_selected(kind as u32);
        set_switch(&self.three_d, chart.three_d);
        self.three_d.set_sensitive(chart.kind.has_3d());
        self.scene.set_visible(chart.three_d);
        set_switch(&self.title_on, chart.title.is_some());
        set_entry(&self.title, chart.title.as_deref().unwrap_or_default());
        set_switch(&self.legend, chart.legend);
        let bars =
            chart.kind.has_bars() || matches!(chart.kind, ChartKind::Mixed | ChartKind::TwoAxis);
        self.gap.set_visible(bars);
        set_spin(&self.gap, f64::from(chart.bar_gap));
        let pie = chart.kind.is_round();
        self.percentages.set_visible(pie);
        set_switch(&self.percentages, chart.pie.percentages);
        self.corners.set_visible(chart.kind.has_bars());
        set_spin(&self.corners, f64::from(chart.corner_radius));
        self.outside_corners
            .set_visible(chart.kind.has_bars() && chart.corner_radius > 0);
        set_switch(&self.outside_corners, chart.outside_corners_only);
        self.inner_radius
            .set_visible(chart.kind == ChartKind::Donut);
        set_spin(&self.inner_radius, f64::from(chart.inner_radius));
        self.share_x.set_visible(chart.kind.has_x_values());
        set_switch(&self.share_x, chart.share_x);
        let bubble = chart.kind == ChartKind::Bubble;
        for row in [&self.by_diameter, &self.negative_bubbles] {
            row.set_visible(bubble);
        }
        self.bubble_scale.set_visible(bubble);
        set_spin(&self.bubble_scale, f64::from(chart.bubble.scale));
        set_switch(&self.by_diameter, chart.bubble.by_diameter);
        set_switch(&self.negative_bubbles, chart.bubble.negative);
        self.radar_circular
            .set_visible(chart.kind == ChartKind::Radar);
        set_switch(&self.radar_circular, chart.radar_circular);
        set_spin(&self.depth, f64::from(chart.scene.depth));
        self.lighting.set_selected(match chart.scene.lighting {
            Lighting::Soft => 0,
            Lighting::Strong => 1,
            Lighting::Flat => 2,
        });
        set_switch(&self.bevel, chart.scene.bevel);
        self.value_axis.set_visible(!pie);
        self.category_axis.set_visible(!pie);
        let axis = &chart.value_axis;
        set_switch(&self.min_on, axis.min.is_some());
        set_switch(&self.max_on, axis.max.is_some());
        if let Some(min) = axis.min {
            set_spin(&self.min, min.0);
        }
        if let Some(max) = axis.max {
            set_spin(&self.max, max.0);
        }
        self.min.set_sensitive(axis.min.is_some());
        self.max.set_sensitive(axis.max.is_some());
        set_spin(&self.steps, f64::from(axis.steps.unwrap_or(0)));
        self.scale.set_selected(match axis.scale {
            Scale::Linear => 0,
            Scale::Logarithmic => 1,
            Scale::Percent => 2,
        });
        set_switch(&self.gridlines, axis.gridlines);
        set_switch(&self.value_labels, axis.labels);
        set_spin(&self.decimals, axis.number.decimals.map_or(-1.0, f64::from));
        set_entry(&self.axis_title, axis.title.as_deref().unwrap_or_default());
        let category = &chart.category_axis;
        set_switch(&self.category_labels, category.labels);
        set_switch(&self.category_gridlines, category.gridlines);
        set_entry(
            &self.category_title,
            category.title.as_deref().unwrap_or_default(),
        );
        // The series, by name: of a scatter or a bubble chart, those of
        // the y values that it plots (W142).
        let names: Vec<String> = if chart.kind.has_x_values() {
            chart
                .point_series()
                .iter()
                .map(|ps| chart.series_name(ps.y).to_owned())
                .collect()
        } else {
            (0..chart.series_count())
                .map(|s| chart.series_name(s).to_owned())
                .collect()
        };
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let shown = self.series_index().min(names.len().saturating_sub(1));
        let model = gtk::StringList::new(&names);
        self.which.set_model(Some(&model));
        self.which.set_selected(shown as u32);
        let style = chart.style(look_of(&chart, shown));
        self.color.set_rgba(&to_gdk(style.color()));
        set_switch(&self.labels, style.labels.show);
        self.position.set_selected(match style.labels.position {
            LabelPosition::Outside => 0,
            LabelPosition::InsideEnd => 1,
            LabelPosition::Middle => 2,
            LabelPosition::InsideBase => 3,
        });
        let lines = matches!(
            chart.kind,
            ChartKind::Line
                | ChartKind::Scatter
                | ChartKind::Radar
                | ChartKind::Mixed
                | ChartKind::TwoAxis
        );
        self.symbol.set_visible(lines);
        self.symbol.set_selected(match style.symbol {
            Symbol::None => 0,
            Symbol::Circle => 1,
            Symbol::Triangle => 2,
            Symbol::Square => 3,
            Symbol::Diamond => 4,
        });
        let trends = chart.kind.has_trendlines();
        self.trend.set_visible(trends);
        self.trend.set_selected(trend_index(style.trend.as_ref()));
        let order = match style.trend.as_ref().map(|t| t.kind) {
            Some(TrendKind::Polynomial(n) | TrendKind::MovingAverage(n)) => f64::from(n),
            _ => 2.0,
        };
        set_spin(&self.order, order);
        let has_order = matches!(
            style.trend.as_ref().map(|t| t.kind),
            Some(TrendKind::Polynomial(_) | TrendKind::MovingAverage(_))
        );
        self.order.set_visible(trends && has_order);
        let has_trend = trends && style.trend.is_some();
        self.equation.set_visible(has_trend);
        self.r_squared.set_visible(has_trend);
        set_switch(
            &self.equation,
            style.trend.as_ref().is_some_and(|t| t.show_equation),
        );
        set_switch(
            &self.r_squared,
            style.trend.as_ref().is_some_and(|t| t.show_r_squared),
        );
        let errors = chart.kind.has_error_bars();
        self.errors.set_visible(errors);
        let (error, value) = error_index(style.errors.as_ref());
        self.errors.set_selected(error);
        set_spin(&self.error_value, value);
        self.error_value
            .set_visible(errors && matches!(error, 1..=3));
        self.updating.set(false);
    }
}

/// The series whose look the nth series of the Series group shows: of a
/// scatter or a bubble chart, that of its y values (W142).
fn look_of(chart: &Chart, shown: usize) -> usize {
    if chart.kind.has_x_values() {
        chart.point_series().get(shown).map_or(shown, |ps| ps.y)
    } else {
        shown
    }
}

/// The style of a series, made when the chart has none of its own yet.
fn series_style(chart: &mut Chart, s: usize) -> &mut desk_model::chart::SeriesStyle {
    let s = look_of(chart, s);
    while chart.series.len() <= s {
        let n = chart.series.len();
        chart.series.push(chart.style(n));
    }
    &mut chart.series[s]
}

fn set_trend(this: &ChartGroups, chart: &mut Chart) {
    let s = this.series_index();
    let order = this.order.value() as u8;
    let kind = trend_kind(this.trend.selected(), order);
    let equation = this.equation.is_active();
    let r_squared = this.r_squared.is_active();
    let style = series_style(chart, s);
    style.trend = kind.map(|kind| Trendline {
        kind,
        show_equation: equation,
        show_r_squared: r_squared,
        label: String::new(),
        color: None,
    });
}

fn set_errors(this: &ChartGroups, chart: &mut Chart) {
    let s = this.series_index();
    let amount = error_amount(this.errors.selected(), this.error_value.value());
    let style = series_style(chart, s);
    style.errors = amount.map(|amount| ErrorBars {
        direction: ErrorDirection::Both,
        amount,
        along_x: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grf_13_grf_14_the_menus_come_back_to_their_place() {
        for i in 0..TRENDS.len() as u32 {
            let kind = trend_kind(i, 3);
            let line = kind.map(|kind| Trendline {
                kind,
                show_equation: false,
                show_r_squared: false,
                label: String::new(),
                color: None,
            });
            assert_eq!(trend_index(line.as_ref()), i);
        }
        for i in 0..ERRORS.len() as u32 {
            let bars = error_amount(i, 2.0).map(|amount| ErrorBars {
                direction: ErrorDirection::Both,
                amount,
                along_x: false,
            });
            assert_eq!(error_index(bars.as_ref()).0, i);
        }
        assert_eq!(KINDS.len(), ChartKind::ALL.len());
    }
}
