// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Table and Chart commands of the window (TAB-01 to TAB-32, GRF-01
//! to GRF-20): inserting them, the rows, columns and cells of the
//! selected table, the data of the selected chart and the Function
//! Browser.

use std::rc::Rc;

use desk_model::chart::ChartKind;
use desk_model::table::FillDirection;
use gtk::prelude::*;
use gtk::{gio, glib};

use super::DocumentWindow;
use crate::editing::TextEditor;

/// The actions of tables and charts, which need a document.
pub(super) const TABLE_ACTIONS: [&str; 20] = [
    "insert-table",
    "insert-chart",
    "insert-chart-kind",
    "table-add-row-above",
    "table-add-row-below",
    "table-add-column-before",
    "table-add-column-after",
    "table-delete-row",
    "table-delete-column",
    "table-merge-cells",
    "table-unmerge-cells",
    "table-sort-ascending",
    "table-sort-descending",
    "table-fill-down",
    "table-fill-right",
    "table-clear-cells",
    "table-distribute-rows",
    "table-distribute-columns",
    "edit-chart-data",
    "function-browser",
];

/// The keys of the table commands. Those of Numbers for rows and
/// columns, with Alt and the arrows, move objects in Desk.
pub(crate) const TABLE_ACCELS: [(&str, &[&str]); 1] =
    [("win.function-browser", &["<Control><Shift>F3"])];

/// The kind of chart a name of the Insert menu stands for.
pub(crate) fn chart_kind(name: &str) -> (ChartKind, bool) {
    let three_d = name.ends_with("-3d");
    let kind = match name.trim_end_matches("-3d") {
        "stacked-column" => ChartKind::StackedColumn,
        "bar" => ChartKind::Bar,
        "stacked-bar" => ChartKind::StackedBar,
        "line" => ChartKind::Line,
        "area" => ChartKind::Area,
        "stacked-area" => ChartKind::StackedArea,
        "pie" => ChartKind::Pie,
        "donut" => ChartKind::Donut,
        "scatter" => ChartKind::Scatter,
        "bubble" => ChartKind::Bubble,
        "radar" => ChartKind::Radar,
        "mixed" => ChartKind::Mixed,
        "two-axis" => ChartKind::TwoAxis,
        _ => ChartKind::Column,
    };
    (kind, three_d && kind.has_3d())
}

type EditorAction = fn(&TextEditor);

impl DocumentWindow {
    pub(super) fn add_table_actions(self: &Rc<Self>) {
        let simple: [(&str, EditorAction); 17] = [
            ("insert-table", TextEditor::insert_table),
            ("insert-chart", |e| e.insert_chart(ChartKind::Column)),
            ("table-add-row-above", |e| e.add_rows(false)),
            ("table-add-row-below", |e| e.add_rows(true)),
            ("table-add-column-before", |e| e.add_cols(false)),
            ("table-add-column-after", |e| e.add_cols(true)),
            ("table-delete-row", TextEditor::delete_rows),
            ("table-delete-column", TextEditor::delete_cols),
            ("table-merge-cells", TextEditor::merge_cells),
            ("table-unmerge-cells", TextEditor::unmerge_cells),
            ("table-sort-ascending", |e| e.sort_rows(true)),
            ("table-sort-descending", |e| e.sort_rows(false)),
            ("table-fill-down", |e| e.fill_cells(FillDirection::Down)),
            ("table-fill-right", |e| e.fill_cells(FillDirection::Right)),
            ("table-clear-cells", |e| e.clear_cells(true)),
            ("table-distribute-rows", |e| e.distribute_cells(true)),
            ("table-distribute-columns", |e| e.distribute_cells(false)),
        ];
        for (name, run) in simple {
            let weak = Rc::downgrade(self);
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| {
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    run(&editor);
                }
            });
            self.actions.add_action(&action);
        }
        let weak = Rc::downgrade(self);
        let action = gio::SimpleAction::new("insert-chart-kind", Some(glib::VariantTy::STRING));
        action.connect_activate(move |_, parameter| {
            let Some(value) = parameter.and_then(|p| p.get::<String>()) else {
                return;
            };
            if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                let (kind, three_d) = chart_kind(&value);
                editor.insert_chart(kind);
                if three_d {
                    editor.change_chart("3D", |chart| chart.three_d = true);
                }
            }
        });
        self.actions.add_action(&action);
        let weak = Rc::downgrade(self);
        let action = gio::SimpleAction::new("edit-chart-data", None);
        action.connect_activate(move |_, _| {
            if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                editor.edit_chart_data();
            }
        });
        self.actions.add_action(&action);
        let weak = Rc::downgrade(self);
        let action = gio::SimpleAction::new("function-browser", None);
        action.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            let Some(editor) = this.editor() else { return };
            let window = this.window().clone().upcast::<gtk::Window>();
            crate::function_browser::open(Some(&window), move |text| {
                if !editor.insert_in_cell(text) && editor.selected_cells().is_some() {
                    editor.edit_cell(Some(&format!("={text}")));
                }
            });
        });
        self.actions.add_action(&action);
    }
}

/// The Chart submenu of the Insert menu: the kinds in 2D and in 3D.
pub(crate) fn chart_menu() -> gio::Menu {
    use crate::i18n::tr;
    let menu = gio::Menu::new();
    let two = gio::Menu::new();
    let three = gio::Menu::new();
    for (name, label) in [
        ("column", tr("Column")),
        ("stacked-column", tr("Stacked Column")),
        ("bar", tr("Bar")),
        ("stacked-bar", tr("Stacked Bar")),
        ("line", tr("Line")),
        ("area", tr("Area")),
        ("stacked-area", tr("Stacked Area")),
        ("pie", tr("Pie")),
        ("donut", tr("Donut")),
        ("scatter", tr("Scatter")),
        ("bubble", tr("Bubble")),
        ("radar", tr("Radar")),
        ("mixed", tr("Mixed")),
        ("two-axis", tr("2-Axis")),
    ] {
        two.append(
            Some(&label),
            Some(&format!("win.insert-chart-kind('{name}')")),
        );
        if chart_kind(name).0.has_3d() {
            three.append(
                Some(&format!("{label} 3D")),
                Some(&format!("win.insert-chart-kind('{name}-3d')")),
            );
        }
    }
    menu.append_section(Some(&tr("2D")), &two);
    menu.append_section(Some(&tr("3D")), &three);
    menu
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grf_01_every_kind_has_a_name_in_the_menu() {
        for kind in ChartKind::ALL.iter() {
            let names = [
                "column",
                "stacked-column",
                "bar",
                "stacked-bar",
                "line",
                "area",
                "stacked-area",
                "pie",
                "donut",
                "scatter",
                "bubble",
                "radar",
                "mixed",
                "two-axis",
            ];
            assert!(names.iter().any(|n| chart_kind(n).0 == *kind));
        }
        assert_eq!(chart_kind("pie-3d"), (ChartKind::Pie, true));
        assert_eq!(chart_kind("scatter-3d"), (ChartKind::Scatter, false));
        assert_eq!(chart_kind("donut-3d"), (ChartKind::Donut, false));
    }
}
