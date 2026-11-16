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
pub(crate) const KINDS: [&str; 11] = [
    n_("Column"),
    n_("Stacked Column"),
    n_("Bar"),
    n_("Stacked Bar"),
    n_("Line"),
    n_("Area"),
    n_("Stacked Area"),
    n_("Pie"),
    n_("Scatter"),
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
        chart.add(&percentages);
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

        let this = Rc::new(Self {
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

    /// The groups: Chart and 3D Scene for the Chart tab, the axes for
    /// the Axis tab and Series for the Series tab.
    pub fn groups(&self) -> [&compact::Group; 5] {
        [
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
        let switches: [(&compact::SwitchRow, &'static str, Change); 14] = [
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
        let spins: [(&compact::SpinRow, &'static str, Change); 8] = [
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
    pub fn show(&self, editor: &TextEditor) {
        let Some(chart) = editor.selected_chart() else {
            return;
        };
        self.updating.set(true);
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
        let bars = !matches!(
            chart.kind,
            ChartKind::Line
                | ChartKind::Pie
                | ChartKind::Scatter
                | ChartKind::Area
                | ChartKind::StackedArea
        );
        self.gap.set_visible(bars);
        set_spin(&self.gap, f64::from(chart.bar_gap));
        let pie = chart.kind == ChartKind::Pie;
        self.percentages.set_visible(pie);
        set_switch(&self.percentages, chart.pie.percentages);
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
        // The series, by name: of a scatter chart, the y of each pair.
        let scatter = chart.kind == ChartKind::Scatter;
        let names: Vec<String> = if scatter {
            (0..chart.series_count() / 2)
                .map(|s| chart.series_name(2 * s + 1).to_owned())
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
        let index = if scatter { 2 * shown + 1 } else { shown };
        let style = chart.style(index);
        self.color.set_rgba(&to_gdk(style.color()));
        set_switch(&self.labels, style.labels.show);
        self.position.set_selected(match style.labels.position {
            LabelPosition::Outside => 0,
            LabelPosition::InsideEnd => 1,
            LabelPosition::Middle => 2,
            LabelPosition::InsideBase => 3,
        });
        let lines = matches!(chart.kind, ChartKind::Line | ChartKind::Scatter)
            || matches!(chart.kind, ChartKind::Mixed | ChartKind::TwoAxis);
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

/// The style of a series, made when the chart has none of its own yet.
fn series_style(chart: &mut Chart, s: usize) -> &mut desk_model::chart::SeriesStyle {
    let s = if chart.kind == ChartKind::Scatter {
        2 * s + 1
    } else {
        s
    };
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
