// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables and charts on the pages (TAB-01 to TAB-32, GRF-01 to GRF-20):
//! inserting them, selecting cells of a table, typing in them and
//! changing them as one step of the history each.
//!
//! A click on a selected table selects the cell under it, and a double
//! click edits it, as in iWork. With cells selected, the arrows move
//! between them, Shift extends the selection, Tab and Return go to the
//! next cell, and typing replaces what the cell holds. The text of a cell
//! is edited in a field laid over it, which shows the formula of a cell
//! that has one and suggests the values of its column.

use std::sync::Arc;

use desk_model::chart::{Chart, ChartKind};
use desk_model::table::{
    Area, BorderSet, CellStyle, FillDirection, Format, Landing, Locale, Step, TextStyle,
};
use desk_model::{Anchor, NodeId, Object, ObjectKind, Table, Wrap};
use gtk::prelude::*;
use gtk::{gdk, glib};

use super::TextEditor;
use super::objects::find;

/// The cells selected in a table: the cell where the selection started
/// and the one it reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CellSelection {
    /// The table.
    pub id: NodeId,
    /// The first cell, by row and column.
    pub anchor: (u32, u32),
    /// The cell that the keyboard moves.
    pub focus: (u32, u32),
}

impl CellSelection {
    /// The cells, grown to take in whole merged cells (TAB-07).
    pub fn area(&self, table: &Table) -> Area {
        table.selection_area(self.anchor, self.focus)
    }
}

/// The field that edits the text of a cell, over it.
pub(crate) struct CellEditor {
    popover: gtk::Popover,
    entry: gtk::Entry,
    /// The cell being edited.
    cell: Option<(NodeId, u32, u32)>,
    /// Set while the field's text is being set, so that it suggests
    /// nothing then.
    filling: bool,
    /// The text of the field when it last changed, to find what was typed.
    last: String,
}

/// The one character typed into `old` to make `new`, and where.
fn typed_char(old: &str, new: &str) -> Option<(usize, char)> {
    let at = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len(), |((i, _), _)| i);
    let c = new.get(at..)?.chars().next()?;
    (new.len() == old.len() + c.len_utf8() && new[at + c.len_utf8()..] == old[at..])
        .then_some((at, c))
}

/// What the text before the cursor of a cell becomes when `typed` is
/// typed after it, with the corrections of the preferences that all the
/// apps share (TXT-06, TXT-07, W064): smart quotes and dashes, the table of
/// replacements, capitals, fractions. A formula is left as typed, and so
/// is a correction that raises text, which a cell cannot show.
pub(crate) fn correct_in_cell(
    before: &str,
    typed: char,
    preferences: &crate::preferences::Preferences,
) -> Option<String> {
    if before.starts_with('=') {
        return None;
    }
    if matches!(typed, '"' | '\'') && preferences.smart_quotes {
        let mut out = before.to_owned();
        out.push(preferences.quotes.smart_quote(before, typed));
        return Some(out);
    }
    if preferences.smart_quotes && desk_model::smart_dash(before, typed) {
        let mut out = before[..before.len() - 1].to_owned();
        out.push('\u{2014}');
        return Some(out);
    }
    if !desk_model::ends_word(typed) {
        return None;
    }
    let correction = preferences.autocorrect.correct(before)?;
    if correction.superscript.is_some() {
        return None;
    }
    let mut out = before.to_owned();
    out.replace_range(correction.range, &correction.text);
    out.push(typed);
    Some(out)
}

impl TextEditor {
    /// The cells selected, while the only object selected is a table.
    /// None in a locked table, whose cells cannot be edited (PXV-16).
    pub fn selected_cells(&self) -> Option<CellSelection> {
        let state = self.state.borrow();
        let cells = state.cells?;
        let locked = state
            .editor
            .document()
            .objects()
            .get(cells.id)
            .is_some_and(|o| o.locked);
        (state.objects == [cells.id] && !locked).then_some(cells)
    }

    /// Whether the selected object is locked: a locked table or chart is
    /// moved, resized and edited only once unlocked (PXV-16).
    pub fn selection_locked(&self) -> bool {
        let state = self.state.borrow();
        let [id] = state.objects[..] else {
            return false;
        };
        state
            .editor
            .document()
            .objects()
            .get(id)
            .is_some_and(|o| o.locked)
    }

    /// The selected table and its selected cells, or all its cells when
    /// none are selected.
    pub fn table_selection(&self) -> Option<(Arc<Table>, Area)> {
        let state = self.state.borrow();
        let [id] = state.objects[..] else {
            return None;
        };
        let object = state.editor.document().objects().get(id)?;
        let ObjectKind::Table(table) = &object.kind else {
            return None;
        };
        let area = match state.cells.filter(|c| c.id == id) {
            Some(cells) => cells.area(table),
            None => Area {
                row: 0,
                col: 0,
                rows: table.rows(),
                cols: table.cols(),
            },
        };
        Some((table.clone(), area))
    }

    /// The selected chart.
    pub fn selected_chart(&self) -> Option<Arc<Chart>> {
        let state = self.state.borrow();
        let [id] = state.objects[..] else {
            return None;
        };
        match &state.editor.document().objects().get(id)?.kind {
            ObjectKind::Chart(chart) => Some(chart.clone()),
            _ => None,
        }
    }

    /// The language and region that tables and date fields write numbers
    /// and dates in (PXV-09).
    pub fn locale(&self) -> Locale {
        desk_layout::document_locale(self.state.borrow().editor.document())
    }

    /// Puts a new table in the middle of the page shown and selects its
    /// first cell (TAB-01): five rows and four columns with a header row
    /// and a header column, as iWork makes them.
    pub fn insert_table(&self) {
        self.insert_table_of(None);
    }

    /// Puts a new table in the look of a table style, as a click on the
    /// style in the popover of the Table button does (PXV-14), or in the
    /// look of new tables.
    pub fn insert_table_of(&self, style: Option<desk_model::StyleId>) {
        let mut table = Table::new(5, 4);
        if let Some(entry) = style.and_then(|s| self.document().table_styles().get(s).cloned()) {
            entry.look.apply_to(&mut table);
        }
        self.add_table(table, None, (1, 1), "Insert Table", |_| Ok(()));
    }

    /// The name of the next table: Table and the number after those of
    /// the document.
    pub(super) fn next_table_name(&self) -> String {
        let count = self
            .document()
            .objects()
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Table(_)))
            .count();
        format!("{} {}", crate::i18n::tr("Table"), count + 1)
    }

    /// Puts a new table on the page, centered on `at` or in the middle of
    /// the page shown, and selects it with the cell `cell` chosen. `also`
    /// makes the rest of the step `name`, such as emptying the cells the
    /// table was made from.
    pub(super) fn add_table(
        &self,
        mut table: Table,
        at: Option<(usize, f64, f64)>,
        cell: (u32, u32),
        name: &str,
        also: impl FnOnce(&mut desk_model::Edit) -> Result<(), desk_model::ModelError>,
    ) {
        table.name = self.next_table_name();
        let size = self.table_size(&table);
        let (page, frame) = self.placed_frame(|_| size, at);
        self.leave_story(None);
        self.close_cell_editor(false);
        let selection = self.selection();
        let result = self.state.borrow_mut().editor.edit(name, |edit| {
            let id = edit.new_id();
            let mut object = Object::new(id, ObjectKind::Table(Arc::new(table)), frame);
            object.anchor = Anchor::Page(page as u32);
            object.wrap = Wrap::AboveAndBelow;
            edit.add_object(object)?;
            also(edit)?;
            Ok(id)
        });
        match result {
            Ok(id) => {
                self.state.borrow_mut().cells = Some(CellSelection {
                    id,
                    anchor: cell,
                    focus: cell,
                });
                self.select_new_object(selection, id);
            }
            Err(error) => glib::g_warning!("desk", "cannot insert a table: {error}"),
        }
    }

    /// Puts a new chart of a kind in the middle of the page shown, with
    /// the data of iWork, and selects it (GRF-01, GRF-03).
    pub fn insert_chart(&self, kind: ChartKind) {
        self.insert_chart_of(kind, None);
    }

    /// Puts a new chart of a kind in the look of a chart style, as a click
    /// on the style in the popover of the Chart button does (W080), or in
    /// the look of new charts.
    pub fn insert_chart_of(&self, kind: ChartKind, style: Option<usize>) {
        let mut chart = Chart::new(kind);
        if let Some(style) = style.and_then(|s| self.document().chart_styles().get(s).cloned()) {
            style.apply_to(&mut chart);
        }
        // The names of the sample data, in the language of the app.
        for (i, row) in chart.data.rows.iter_mut().enumerate() {
            *row = format!("{} {}", crate::i18n::tr("Region"), i + 1);
        }
        let (page, frame) =
            self.placed_frame(|(w, _)| (w.min(360.0), w.min(360.0) * 2.0 / 3.0), None);
        self.leave_story(None);
        self.close_cell_editor(false);
        let selection = self.selection();
        let result = self.state.borrow_mut().editor.edit("Insert Chart", |edit| {
            let id = edit.new_id();
            let mut object = Object::new(id, ObjectKind::Chart(Arc::new(chart)), frame);
            object.anchor = Anchor::Page(page as u32);
            object.wrap = Wrap::AboveAndBelow;
            edit.add_object(object)?;
            Ok(id)
        });
        match result {
            Ok(id) => self.select_new_object(selection, id),
            Err(error) => glib::g_warning!("desk", "cannot insert a chart: {error}"),
        }
    }

    /// The size a table takes on the page, in points.
    /// A table resized by a handle of its frame: its columns and rows
    /// grow or shrink in proportion, and the frame takes the size of the
    /// table (TAB-03).
    pub(super) fn fit_table_to_frame(&self, object: &mut Object, before: &Object) {
        let ObjectKind::Table(table) = &object.kind else {
            return;
        };
        if object.frame.width == before.frame.width && object.frame.height == before.frame.height {
            return;
        }
        let (old_w, old_h) = (before.frame.width.to_pt(), before.frame.height.to_pt());
        let (new_w, new_h) = (object.frame.width.to_pt(), object.frame.height.to_pt());
        let mut table = (**table).clone();
        if old_w > 0.0 && (new_w - old_w).abs() > 0.01 {
            table.scale_width((new_w * 1000.0).max(6_000.0) as u32);
        }
        if old_h > 0.0 && (new_h - old_h).abs() > 0.01 {
            let ratio = new_h / old_h;
            for r in 0..table.rows() {
                table.set_height(r, (f64::from(table.height(r)) * ratio) as u32);
            }
        }
        let (w, h) = self.table_size(&table);
        object.frame.width = desk_model::Length::from_pt(w);
        object.frame.height = desk_model::Length::from_pt(h);
        object.kind = ObjectKind::Table(std::sync::Arc::new(table));
    }

    pub(super) fn table_size(&self, table: &Table) -> (f64, f64) {
        let document = self.document();
        let (w, h) =
            crate::with_text_system(|system| desk_layout::table_size(&document, table, system));
        (f64::from(w), f64::from(h))
    }

    /// Changes the selected table as one step of the history, and the
    /// selected cells with it; the frame takes the size of the table.
    pub fn change_table(&self, name: &str, change: impl FnOnce(&mut Table, &mut CellSelection)) {
        self.edit_table(name, Some(|_: &mut Table| {}), change);
    }

    /// Changes the rows, columns, borders, merges or order of the selected
    /// table, or moves or fills its cells, as one step of the history:
    /// none of it is a tracked change of its cells, as Pages '09 tracked
    /// none (REV-05).
    pub fn rearrange_table(&self, name: &str, change: impl FnOnce(&mut Table, &mut CellSelection)) {
        self.edit_table(name, None::<fn(&mut Table)>, change);
    }

    /// Changes the selected table with `change` in one step of the
    /// history: tracked when `arrange` is given, which first rearranges
    /// the table without being tracked, as a paste adds the rows and
    /// columns it needs before it writes the cells (REV-05).
    fn edit_table(
        &self,
        name: &str,
        arrange: Option<impl FnOnce(&mut Table)>,
        change: impl FnOnce(&mut Table, &mut CellSelection),
    ) {
        let Some((table, _)) = self.table_selection() else {
            return;
        };
        if self.selection_locked() {
            return;
        }
        let id = self.selected_objects()[0];
        let mut cells = self
            .state
            .borrow()
            .cells
            .filter(|c| c.id == id)
            .unwrap_or(CellSelection {
                id,
                anchor: (0, 0),
                focus: (table.rows() - 1, table.cols() - 1),
            });
        let tracked = arrange.is_some();
        let mut arranged = (*table).clone();
        if let Some(arrange) = arrange {
            arrange(&mut arranged);
        }
        let mut changed = arranged.clone();
        change(&mut changed, &mut cells);
        if changed == *table {
            if self.state.borrow().cells != Some(cells) {
                self.state.borrow_mut().cells = Some(cells);
                self.show_cells();
                self.selection_changed();
            }
            return;
        }
        let clamp = |(r, c): (u32, u32)| {
            (
                r.min(changed.rows().saturating_sub(1)),
                c.min(changed.cols().saturating_sub(1)),
            )
        };
        cells.anchor = clamp(cells.anchor);
        cells.focus = clamp(cells.focus);
        let (w, h) = self.table_size(&changed);
        let arranged = (arranged != *table).then(|| Arc::new(arranged));
        let changed = Arc::new(changed);
        let aside = crate::preferences::get().move_objects_aside;
        self.arrange_objects(name, move |edit, _| {
            let Some(mut object) = edit.document().objects().get(id).cloned() else {
                return Ok(None);
            };
            let old = object.frame;
            if let Some(arranged) = arranged {
                let mut object = object.clone();
                object.kind = ObjectKind::Table(arranged);
                edit.untracked(|edit| edit.replace_object(object))?;
            }
            object.kind = ObjectKind::Table(changed);
            object.frame.width = desk_model::Length::from_pt(w);
            object.frame.height = desk_model::Length::from_pt(h);
            if tracked {
                edit.replace_object(object)?;
            } else {
                edit.untracked(|edit| edit.replace_object(object))?;
            }
            if aside {
                edit.move_aside(id, old)?;
            }
            Ok(None)
        });
        self.state.borrow_mut().cells = Some(cells);
        self.show_cells();
        self.selection_changed();
    }

    /// Changes the selected chart as one step of the history.
    pub fn change_chart(&self, name: &str, change: impl FnOnce(&mut Chart)) {
        let Some(chart) = self.selected_chart() else {
            return;
        };
        if self.selection_locked() {
            return;
        }
        let mut changed = (*chart).clone();
        change(&mut changed);
        if changed == *chart {
            return;
        }
        let id = self.selected_objects()[0];
        let changed = Arc::new(changed);
        self.arrange_objects(name, move |edit, _| {
            let Some(mut object) = edit.document().objects().get(id).cloned() else {
                return Ok(None);
            };
            object.kind = ObjectKind::Chart(changed);
            edit.replace_object(object)?;
            Ok(None)
        });
    }

    /// Changes each selected cell.
    pub fn change_cells(
        &self,
        name: &str,
        mut change: impl FnMut(&mut desk_model::table::Cell<String>),
    ) {
        self.change_table(name, |table, cells| {
            for (r, c) in cells.area(table).cells() {
                change(table.cell_mut(r, c));
            }
        });
    }

    /// Sets how the selected cells write their values (TAB-28).
    pub fn set_cell_format(&self, format: Format) {
        self.change_cells("Data Format", |cell| cell.format = format.clone());
    }

    /// Sets the look of the selected cells.
    pub fn set_cell_style(&self, name: &str, mut change: impl FnMut(&mut CellStyle)) {
        self.change_cells(name, |cell| change(&mut cell.style));
    }

    /// Sets the formatting of the text of the selected cells.
    pub fn set_cell_text(&self, name: &str, mut change: impl FnMut(&mut TextStyle)) {
        self.change_cells(name, |cell| change(&mut cell.style.text));
    }

    /// Sets the borders of the selected cells (TAB-25).
    pub fn set_cell_borders(&self, which: BorderSet, border: Option<desk_model::table::Border>) {
        self.rearrange_table("Borders", |table, cells| {
            let area = cells.area(table);
            table.set_borders(area, which, border);
        });
    }

    /// Adds rows above or below the selected ones (TAB-10).
    pub fn add_rows(&self, below: bool) {
        self.rearrange_table("Add Row", |table, cells| {
            let area = cells.area(table);
            let at = if below { area.row_end() } else { area.row };
            if table.insert_rows(at, area.rows).is_ok() && !below {
                cells.anchor.0 += area.rows;
                cells.focus.0 += area.rows;
            }
        });
    }

    /// Adds columns before or after the selected ones (TAB-10).
    pub fn add_cols(&self, after: bool) {
        self.rearrange_table("Add Column", |table, cells| {
            let area = cells.area(table);
            let at = if after { area.col_end() } else { area.col };
            if table.insert_cols(at, area.cols).is_ok() && !after {
                cells.anchor.1 += area.cols;
                cells.focus.1 += area.cols;
            }
        });
    }

    /// Deletes the selected rows (TAB-11).
    pub fn delete_rows(&self) {
        self.rearrange_table("Delete Row", |table, cells| {
            let area = cells.area(table);
            let _ = table.delete_rows(area.row, area.rows);
        });
    }

    /// Deletes the selected columns (TAB-11).
    pub fn delete_cols(&self) {
        self.rearrange_table("Delete Column", |table, cells| {
            let area = cells.area(table);
            let _ = table.delete_cols(area.col, area.cols);
        });
    }

    /// Merges the selected cells, or unmerges a merged cell (TAB-23, TAB-24).
    pub fn merge_cells(&self) {
        let locale = self.locale();
        self.rearrange_table("Merge Cells", |table, cells| {
            let area = cells.area(table);
            if area.rows == 1 && area.cols == 1 {
                return;
            }
            if table.merge_at(area.row, area.col) == Some(area) {
                table.unmerge(area.row, area.col);
            } else if table.merge(area, &locale).is_ok() {
                cells.anchor = (area.row, area.col);
                cells.focus = cells.anchor;
            }
        });
    }

    /// Unmerges the selected merged cells (TAB-24).
    pub fn unmerge_cells(&self) {
        self.rearrange_table("Unmerge Cells", |table, cells| {
            for (r, c) in cells.area(table).cells() {
                if table
                    .merge_at(r, c)
                    .is_some_and(|m| (m.row, m.col) == (r, c))
                {
                    table.unmerge(r, c);
                }
            }
        });
    }

    /// Sorts the body rows by the column of the cell selected (TAB-15).
    pub fn sort_rows(&self, ascending: bool) {
        self.rearrange_table("Sort", |table, cells| {
            let area = cells.area(table);
            let rows = (area.rows > 1).then(|| (area.row, area.row_end()));
            table.sort(cells.focus.1, ascending, rows, desk_layout::today());
        });
    }

    /// Fills the selected cells from the first row or column of them
    /// (TAB-19).
    pub fn fill_cells(&self, direction: FillDirection) {
        self.rearrange_table("Fill", |table, cells| {
            let area = cells.area(table);
            let (source, count) = match direction {
                FillDirection::Down => (Area { rows: 1, ..area }, area.rows - 1),
                FillDirection::Right => (Area { cols: 1, ..area }, area.cols - 1),
                FillDirection::Up => (
                    Area {
                        row: area.row_end() - 1,
                        rows: 1,
                        ..area
                    },
                    area.rows - 1,
                ),
                FillDirection::Left => (
                    Area {
                        col: area.col_end() - 1,
                        cols: 1,
                        ..area
                    },
                    area.cols - 1,
                ),
            };
            if count > 0 {
                table.fill(source, direction, count);
            }
        });
    }

    /// Empties the selected cells; `all` takes their formatting too.
    pub fn clear_cells(&self, all: bool) {
        self.change_table("Clear", |table, cells| {
            let area = cells.area(table);
            table.clear(area, all);
        });
    }

    /// Makes the selected rows or columns as high or wide as each other
    /// (TAB-13).
    pub fn distribute_cells(&self, rows: bool) {
        self.rearrange_table("Distribute", |table, cells| {
            let area = cells.area(table);
            if rows {
                let rows: Vec<u32> = (area.row..area.row_end()).collect();
                table.distribute_rows(&rows);
            } else {
                let cols: Vec<u32> = (area.col..area.col_end()).collect();
                table.distribute_cols(&cols);
            }
        });
    }

    /// The rectangles of the selected cells, on their page.
    pub(super) fn cell_rects(&self) -> Option<Vec<crate::text_nav::SpanRect>> {
        let cells = self.selected_cells()?;
        let state = self.state.borrow();
        let (page, placed) = find(&state.layout, cells.id)?;
        let layout = placed.table.as_ref()?;
        let ObjectKind::Table(table) = &placed.object.kind else {
            return None;
        };
        let area = cells.area(table);
        let (x0, y0) = (f64::from(placed.x), f64::from(placed.y));
        let first = layout.cell_rect(area.row, area.col)?;
        let last = layout.cell_rect(area.row_end() - 1, area.col_end() - 1)?;
        Some(vec![crate::text_nav::SpanRect {
            page,
            x0: x0 + f64::from(first.0),
            y0: y0 + f64::from(first.1),
            x1: x0 + f64::from(last.0 + last.2),
            y1: y0 + f64::from(last.1 + last.3),
        }])
    }

    /// Shows the selected cells.
    pub(super) fn show_cells(&self) {
        if let Some(rects) = self.cell_rects() {
            self.view.set_marks(rects, None);
        } else {
            self.show_selection();
        }
        self.describe_table_or_chart();
    }

    /// Tells the screen reader what is selected in a table or which chart
    /// is (GNM-04): the description of the page area, which changes with
    /// the cell, and an announcement of it.
    pub(super) fn describe_table_or_chart(&self) {
        let text = if let Some((table, area)) = self.table_selection() {
            let locale = self.locale();
            let values = table.values(desk_layout::today());
            match self.selected_cells() {
                Some(cells) => {
                    let (r, c) = cells.focus;
                    let name = format!("{}{}", desk_model::table::CellRef::column_name(c), r + 1);
                    let shown = table.shown(&values, r, c, &locale).text;
                    let cells = area.rows * area.cols;
                    if cells > 1 {
                        format!(
                            "{}, {name}: {shown} ({})",
                            table.name,
                            crate::i18n::tr("%d cells selected").replace("%d", &cells.to_string())
                        )
                    } else {
                        format!("{}, {name}: {shown}", table.name)
                    }
                }
                None => format!(
                    "{}, {}",
                    table.name,
                    crate::i18n::tr("%r rows, %c columns")
                        .replace("%r", &table.rows().to_string())
                        .replace("%c", &table.cols().to_string())
                ),
            }
        } else if let Some(chart) = self.selected_chart() {
            let title = chart.title.clone().unwrap_or_default();
            let series =
                crate::i18n::tr("%d series").replace("%d", &chart.series_count().to_string());
            [crate::i18n::tr("Chart"), title, series]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            String::new()
        };
        let area = self.view.area();
        if text.is_empty() {
            area.reset_property(gtk::AccessibleProperty::Description);
        } else {
            area.update_property(&[gtk::accessible::Property::Description(&text)]);
            area.announce(&text, gtk::AccessibleAnnouncementPriority::Medium);
        }
    }

    /// Answers a press on a table or a chart: on a selected table, it
    /// selects the cell under it, and with Shift the cells up to it; a
    /// double click edits the cell, or the data of a chart. Whether it
    /// took the press.
    pub(super) fn press_table(
        &self,
        id: NodeId,
        page: usize,
        x: f32,
        y: f32,
        presses: i32,
        shift: bool,
    ) -> bool {
        let (selected, cell, chart) = {
            let state = self.state.borrow();
            let Some((p, placed)) = find(&state.layout, id) else {
                return false;
            };
            // A locked table or chart is only selected as an object.
            if p != page || placed.object.locked {
                return false;
            }
            let selected = state.objects == [id];
            let cell = placed
                .table
                .as_ref()
                .and_then(|t| t.cell_at(x - placed.x, y - placed.y));
            (
                selected,
                cell,
                matches!(placed.object.kind, ObjectKind::Chart(_)),
            )
        };
        if chart {
            if presses > 1 {
                self.select_objects(vec![id]);
                self.edit_chart_data();
                return true;
            }
            return false;
        }
        let Some(cell) = cell else {
            return false;
        };
        // A press inside the selected cells may start dragging them
        // (TAB-26); without a drag, it selects the cell on release.
        if selected
            && presses == 1
            && !shift
            && let Some(cells) = self.selected_cells().filter(|c| c.id == id)
            && let Some((table, _)) = self.table_selection()
            && cells.area(&table).contains(cell.0, cell.1)
        {
            self.close_cell_editor(true);
            self.start_cell_drag(id, cells.area(&table), cell);
            return true;
        }
        if !selected && presses < 2 {
            self.close_cell_editor(true);
            self.state.borrow_mut().cells = None;
            return false;
        }
        self.close_cell_editor(true);
        let mut cells = self
            .selected_cells()
            .filter(|c| c.id == id)
            .unwrap_or(CellSelection {
                id,
                anchor: cell,
                focus: cell,
            });
        if shift {
            cells.focus = cell;
        } else {
            cells.anchor = cell;
            cells.focus = cell;
        }
        {
            let mut state = self.state.borrow_mut();
            state.objects = vec![id];
            state.cells = Some(cells);
        }
        self.show_objects();
        self.show_cells();
        self.selection_changed();
        if presses > 1 {
            self.edit_cell(None);
        }
        true
    }

    /// Answers a key while cells are selected. Whether it took it.
    pub(super) fn table_key(&self, key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
        let Some(cells) = self.selected_cells() else {
            return false;
        };
        let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
        let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        if control || modifiers.contains(gdk::ModifierType::ALT_MASK) {
            return false;
        }
        let step = match key {
            gdk::Key::Up => Step::Up,
            gdk::Key::Down => Step::Down,
            gdk::Key::Left => Step::Left,
            gdk::Key::Right => Step::Right,
            gdk::Key::Tab | gdk::Key::KP_Tab => Step::Next,
            gdk::Key::ISO_Left_Tab => Step::Previous,
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if shift {
                    Step::Up
                } else {
                    // Return edits the cell; in the field it goes down.
                    self.edit_cell(None);
                    return true;
                }
            }
            gdk::Key::Escape => {
                // Back to the table as an object.
                self.state.borrow_mut().cells = None;
                self.show_selection();
                self.show_objects();
                self.selection_changed();
                return true;
            }
            gdk::Key::Delete | gdk::Key::BackSpace | gdk::Key::KP_Delete => {
                self.clear_cells(false);
                return true;
            }
            _ => return false,
        };
        let extend = shift && matches!(step, Step::Up | Step::Down | Step::Left | Step::Right);
        self.move_cells(cells, step, extend);
        true
    }

    /// Moves the selected cell, or extends the selection, by the rules of
    /// [`Table::step`] (TAB-08), adding a row or a column when Tab leaves
    /// the last cell of a table that says so.
    fn move_cells(&self, cells: CellSelection, step: Step, extend: bool) {
        let Some((table, _)) = self.table_selection() else {
            return;
        };
        let cols = table.cols();
        let at = match table.step(cells.focus, step) {
            Landing::Cell(r, c) => (r, c),
            Landing::AddRow => {
                // A new row of the body, before the footer if there is one.
                self.rearrange_table("Add Row", |table, cells| {
                    let at = table.body_end();
                    if table.insert_rows(at, 1).is_ok() {
                        cells.anchor = (at, 0);
                        cells.focus = (at, 0);
                    }
                });
                return;
            }
            Landing::AddColumn => {
                let r = cells.focus.0;
                self.rearrange_table("Add Column", |table, cells| {
                    if table.insert_cols(cols, 1).is_ok() {
                        cells.anchor = (r, cols);
                        cells.focus = (r, cols);
                    }
                });
                return;
            }
        };
        let moved = CellSelection {
            id: cells.id,
            anchor: if extend { cells.anchor } else { at },
            focus: at,
        };
        self.state.borrow_mut().cells = Some(moved);
        self.show_cells();
        self.selection_changed();
    }

    /// Answers text typed while cells are selected: it replaces what the
    /// focused cell holds. Whether it took it.
    pub(super) fn type_in_cell(&self, text: &str) -> bool {
        if self.selected_cells().is_none() {
            return false;
        }
        self.edit_cell(Some(text));
        true
    }

    /// Edits the focused cell in the field over it: with its text, or
    /// with `typed` in place of it.
    pub fn edit_cell(&self, typed: Option<&str>) {
        let Some(cells) = self.selected_cells() else {
            return;
        };
        let Some((table, _)) = self.table_selection() else {
            return;
        };
        let (r, c) = cells.focus;
        let rect = {
            let state = self.state.borrow();
            find(&state.layout, cells.id).and_then(|(page, placed)| {
                let (x, y, w, h) = placed.table.as_ref()?.cell_rect(r, c)?;
                Some((page, placed.x + x, placed.y + y, w, h))
            })
        };
        let Some((page, x, y, w, h)) = rect else {
            return;
        };
        let Some((ax, ay)) = self.view.to_area(page, f64::from(x), f64::from(y)) else {
            return;
        };
        let scale = self.view.scale();
        let text = match typed {
            Some(t) => t.to_owned(),
            None => table.source_text(r, c, &self.locale()),
        };
        self.ensure_cell_editor();
        let state = self.state.borrow();
        let Some(editor) = &state.cell_editor else {
            return;
        };
        let popover = editor.popover.clone();
        let entry = editor.entry.clone();
        drop(state);
        if let Some(e) = self.state.borrow_mut().cell_editor.as_mut() {
            e.cell = Some((cells.id, r, c));
            e.filling = true;
        }
        entry.set_text(&text);
        entry.set_width_request((f64::from(w) * scale).max(80.0) as i32);
        entry.set_height_request((f64::from(h) * scale).max(24.0) as i32);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(
            ax as i32,
            ay as i32 - 1,
            (f64::from(w) * scale) as i32,
            1,
        )));
        popover.popup();
        entry.grab_focus();
        entry.set_position(-1);
        if let Some(e) = self.state.borrow_mut().cell_editor.as_mut() {
            e.filling = false;
        }
    }

    /// Makes the field that edits cells, the first time.
    fn ensure_cell_editor(&self) {
        if self.state.borrow().cell_editor.is_some() {
            return;
        }
        let entry = gtk::Entry::builder().has_frame(false).build();
        entry.add_css_class("desk-cell-editor");
        let popover = gtk::Popover::builder()
            .child(&entry)
            .has_arrow(false)
            .position(gtk::PositionType::Bottom)
            .autohide(true)
            .build();
        popover.add_css_class("desk-cell-editor");
        popover.set_parent(self.view.area());
        let weak = self.downgrade();
        entry.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.finish_cell(Some(Step::Down));
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let step = match key {
                gdk::Key::Tab | gdk::Key::KP_Tab => Step::Next,
                gdk::Key::ISO_Left_Tab => Step::Previous,
                gdk::Key::Up => Step::Up,
                gdk::Key::Down => Step::Down,
                gdk::Key::Return | gdk::Key::KP_Enter
                    if modifiers.contains(gdk::ModifierType::SHIFT_MASK) =>
                {
                    Step::Up
                }
                gdk::Key::Escape => {
                    this.close_cell_editor(false);
                    this.focus();
                    return glib::Propagation::Stop;
                }
                _ => return glib::Propagation::Proceed,
            };
            this.finish_cell(Some(step));
            glib::Propagation::Stop
        });
        entry.add_controller(keys);
        // The values of the column, as the text is typed (TAB-17), and
        // the corrections of typing (W064).
        let weak = self.downgrade();
        entry.connect_changed(move |entry| {
            let Some(this) = weak.upgrade() else { return };
            this.correct_cell(entry);
            this.suggest(entry);
        });
        let weak = self.downgrade();
        popover.connect_closed(move |_| {
            if let Some(this) = weak.upgrade() {
                // A click elsewhere keeps what was typed.
                glib::idle_add_local_once(move || this.finish_cell(None));
            }
        });
        self.state.borrow_mut().cell_editor = Some(CellEditor {
            popover,
            entry,
            cell: None,
            filling: false,
            last: String::new(),
        });
    }

    /// Makes the corrections of typing on the character just typed in the
    /// field, once the field has taken it.
    fn correct_cell(&self, entry: &gtk::Entry) {
        let text = entry.text().to_string();
        let (old, filling) = match self.state.borrow_mut().cell_editor.as_mut() {
            Some(e) => (std::mem::replace(&mut e.last, text.clone()), e.filling),
            None => return,
        };
        if filling {
            return;
        }
        let Some((at, typed)) = typed_char(&old, &text) else {
            return;
        };
        let Some(corrected) = correct_in_cell(&text[..at], typed, &crate::preferences::get())
        else {
            return;
        };
        let rest = text[at + typed.len_utf8()..].to_owned();
        let weak = self.downgrade();
        let entry = entry.clone();
        glib::idle_add_local_once(move || {
            let Some(this) = weak.upgrade() else { return };
            if entry.text() != text {
                return;
            }
            if let Some(e) = this.state.borrow_mut().cell_editor.as_mut() {
                e.filling = true;
            }
            entry.set_text(&format!("{corrected}{rest}"));
            entry.set_position(corrected.chars().count() as i32);
            if let Some(e) = this.state.borrow_mut().cell_editor.as_mut() {
                e.filling = false;
                e.last = entry.text().to_string();
            }
        });
    }

    /// Completes the text typed with a value of the column that starts
    /// with it, the rest selected.
    fn suggest(&self, entry: &gtk::Entry) {
        let (cell, filling) = match &self.state.borrow().cell_editor {
            Some(e) => (e.cell, e.filling),
            None => return,
        };
        let Some((_, r, c)) = cell else { return };
        if filling {
            return;
        }
        let text = entry.text().to_string();
        if text.starts_with('=') || entry.position() != text.chars().count() as i32 {
            return;
        }
        let Some((table, _)) = self.table_selection() else {
            return;
        };
        let Some(suggestion) = table.suggestion(r, c, &text) else {
            return;
        };
        if suggestion.chars().count() <= text.chars().count() {
            return;
        }
        if let Some(e) = self.state.borrow_mut().cell_editor.as_mut() {
            e.filling = true;
        }
        let start = text.chars().count() as i32;
        entry.set_text(&suggestion);
        entry.select_region(start, -1);
        if let Some(e) = self.state.borrow_mut().cell_editor.as_mut() {
            e.filling = false;
        }
    }

    /// Puts the text of the field in its cell, as one step, and moves.
    fn finish_cell(&self, step: Option<Step>) {
        let (cell, text, popover) = {
            let mut state = self.state.borrow_mut();
            let Some(editor) = state.cell_editor.as_mut() else {
                return;
            };
            let Some(cell) = editor.cell.take() else {
                return;
            };
            (
                cell,
                editor.entry.text().to_string(),
                editor.popover.clone(),
            )
        };
        popover.popdown();
        let (id, r, c) = cell;
        if self.selected_objects() != [id] {
            return;
        }
        let locale = self.locale();
        let before = self
            .table_selection()
            .map(|(t, _)| t.source_text(r, c, &locale))
            .unwrap_or_default();
        if before != text {
            self.change_table("Typing", |table, _| {
                table.type_text(r, c, &text, &locale);
            });
        }
        self.focus();
        if let (Some(step), Some(cells)) = (step, self.selected_cells()) {
            self.move_cells(cells, step, false);
        }
    }

    /// Closes the field that edits a cell, keeping what was typed or not.
    pub(super) fn close_cell_editor(&self, keep: bool) {
        if keep {
            self.finish_cell(None);
            return;
        }
        let popover = {
            let mut state = self.state.borrow_mut();
            let Some(editor) = state.cell_editor.as_mut() else {
                return;
            };
            editor.cell = None;
            editor.popover.clone()
        };
        popover.popdown();
    }

    /// Puts text at the cursor of the field that edits a cell, as the
    /// Function Browser does; whether a cell is being edited.
    pub fn insert_in_cell(&self, text: &str) -> bool {
        let entry = {
            let state = self.state.borrow();
            match &state.cell_editor {
                Some(e) if e.cell.is_some() => e.entry.clone(),
                _ => return false,
            }
        };
        let mut at = entry.position();
        entry.insert_text(text, &mut at);
        entry.set_position(at);
        true
    }

    /// Opens the Chart Data Editor of the selected chart (GRF-02).
    pub fn edit_chart_data(&self) {
        let Some(chart) = self.selected_chart() else {
            return;
        };
        let parent = self.view.area().root().and_downcast::<gtk::Window>();
        let weak = self.downgrade();
        let locale = self.locale();
        crate::chart_data::open(
            parent.as_ref(),
            &chart,
            locale,
            move |data, series_in_rows| {
                if let Some(this) = weak.upgrade() {
                    this.change_chart("Edit Chart Data", |chart| {
                        chart.data = data;
                        chart.series_in_rows = series_in_rows;
                    });
                }
            },
        );
    }

    /// The text of the selected cells as they show, by tabs and lines,
    /// as Copy puts it for other programs; none when no cells are
    /// selected.
    pub fn cells_text(&self) -> Option<String> {
        let (table, area) = self.table_selection()?;
        self.selected_cells()?;
        let values = table.values(desk_layout::today());
        Some(table.area_text(area, &values, &self.locale()))
    }

    /// Pastes text of tabs and lines into the cells from the focused one,
    /// as iWork does with cells copied from a spreadsheet (IOP-19).
    pub fn paste_in_cells(&self, text: &str) -> bool {
        let Some(cells) = self.selected_cells() else {
            return false;
        };
        let locale = self.locale();
        let text = text.to_owned();
        let grow = text.clone();
        self.edit_table(
            "Paste",
            Some(move |table: &mut Table| {
                table.grow_for_text(cells.focus, &grow);
            }),
            |table, sel| {
                let area = table.paste_text(cells.focus, &text, &locale);
                sel.anchor = (area.row, area.col);
                sel.focus = (area.row_end() - 1, area.col_end() - 1);
            },
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;

    #[test]
    fn w064_cells_take_the_corrections_of_typing() {
        assert_eq!(typed_char("ab", "abc"), Some((2, 'c')));
        assert_eq!(typed_char("ac", "abc"), Some((1, 'b')));
        assert_eq!(typed_char("abc", "ab"), None);
        assert_eq!(typed_char("ab", "abcd"), None);
        let mut preferences = Preferences::default();
        assert_eq!(
            correct_in_cell("He said ", '"', &preferences).as_deref(),
            Some("He said “")
        );
        // A new install replaces nothing, as Pages.
        assert_eq!(correct_in_cell("(c)", ' ', &preferences), None);
        preferences.autocorrect.replace = true;
        preferences.autocorrect.fractions = true;
        preferences.autocorrect.ordinals = true;
        assert_eq!(
            correct_in_cell("Desk (c)", ' ', &preferences).as_deref(),
            Some("Desk © ")
        );
        assert_eq!(
            correct_in_cell("1/2", '.', &preferences).as_deref(),
            Some("½.")
        );
        // A cell cannot raise the suffix, nor change a formula.
        assert_eq!(correct_in_cell("1st", ' ', &preferences), None);
        assert_eq!(correct_in_cell("=\"a", '"', &preferences), None);
        assert_eq!(
            correct_in_cell("a-", '-', &preferences).as_deref(),
            Some("a—")
        );
        preferences.smart_quotes = false;
        assert_eq!(correct_in_cell("a", '\'', &preferences), None);
    }
}
