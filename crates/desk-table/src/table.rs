// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The table: its grid of cells, the sizes of its rows and columns, its
//! headers and footer, merged cells and borders.

use desk_formula::{Axis, Formula};

use crate::cell::{Cell, CellText, Color, Content};

/// The most header rows, header columns or footer rows a table has
/// (TAB-12).
pub const MAX_HEADERS: u32 = 5;

/// The most rows or columns a table has.
pub const MAX_CELLS: u32 = 65_535;

/// A rectangle of cells.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Area {
    /// The first row.
    pub row: u32,
    /// The first column.
    pub col: u32,
    /// How many rows.
    pub rows: u32,
    /// How many columns.
    pub cols: u32,
}

impl Area {
    /// One cell.
    pub const fn cell(row: u32, col: u32) -> Self {
        Self {
            row,
            col,
            rows: 1,
            cols: 1,
        }
    }

    /// The area between two cells, both included, in any order.
    pub fn between(a: (u32, u32), b: (u32, u32)) -> Self {
        Self {
            row: a.0.min(b.0),
            col: a.1.min(b.1),
            rows: a.0.abs_diff(b.0) + 1,
            cols: a.1.abs_diff(b.1) + 1,
        }
    }

    /// The row after the last.
    pub fn row_end(&self) -> u32 {
        self.row + self.rows
    }

    /// The column after the last.
    pub fn col_end(&self) -> u32 {
        self.col + self.cols
    }

    /// Whether a cell is in the area.
    pub fn contains(&self, row: u32, col: u32) -> bool {
        row >= self.row && row < self.row_end() && col >= self.col && col < self.col_end()
    }

    /// Whether two areas share a cell.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.row < other.row_end()
            && other.row < self.row_end()
            && self.col < other.col_end()
            && other.col < self.col_end()
    }

    /// The smallest area holding both.
    pub fn union(&self, other: &Self) -> Self {
        let row = self.row.min(other.row);
        let col = self.col.min(other.col);
        Self {
            row,
            col,
            rows: self.row_end().max(other.row_end()) - row,
            cols: self.col_end().max(other.col_end()) - col,
        }
    }

    /// The cells, by rows.
    pub fn cells(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        (self.row..self.row_end())
            .flat_map(move |r| (self.col..self.col_end()).map(move |c| (r, c)))
    }
}

/// The part of the table a cell is in (TAB-12, TAB-23).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Region {
    /// A header row, the corner included.
    HeaderRow,
    /// A header column, below the header rows.
    HeaderColumn,
    /// The body.
    Body,
    /// A footer row.
    Footer,
}

/// How a border line is drawn.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum LineStyle {
    /// Solid.
    #[default]
    Solid,
    /// Dashed.
    Dashed,
    /// Dotted.
    Dotted,
}

/// A border of a cell (TAB-25).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Border {
    /// The width, in thousandths of a point.
    pub width: u32,
    /// The color.
    pub color: Color,
    /// The style.
    pub style: LineStyle,
}

impl Border {
    /// A solid line.
    pub const fn solid(width: u32, color: Color) -> Self {
        Self {
            width,
            color,
            style: LineStyle::Solid,
        }
    }
}

/// The look of the parts of a table (TAB-04, TAB-14).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TableStyle {
    /// The fill of the header rows.
    pub header_row_fill: Option<Color>,
    /// The fill of the header columns.
    pub header_column_fill: Option<Color>,
    /// The fill of the footer rows.
    pub footer_fill: Option<Color>,
    /// The fill of the body.
    pub body_fill: Option<Color>,
    /// The color of every other body row, when rows alternate (TAB-14).
    pub alternating: Option<Color>,
    /// The font family of the cells.
    pub font: String,
    /// The size of the text, in thousandths of a point.
    pub font_size: u32,
    /// The room between the borders and the text, in thousandths of a
    /// point.
    pub padding: u32,
}

impl Default for TableStyle {
    fn default() -> Self {
        Self {
            header_row_fill: Some(Color::rgb(0xd5, 0xdb, 0xe3)),
            header_column_fill: Some(Color::rgb(0xe6, 0xea, 0xef)),
            footer_fill: Some(Color::rgb(0xe6, 0xea, 0xef)),
            body_fill: Some(Color::WHITE),
            alternating: None,
            font: "Helvetica Neue".to_owned(),
            font_size: 10_000,
            padding: 4_000,
        }
    }
}

/// What Tab does in the last cell (TAB-08).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum LastCellTab {
    /// Adds a row.
    #[default]
    AddRow,
    /// Adds a column.
    AddColumn,
    /// Goes back to the first cell.
    Wrap,
}

/// The options of a table (TAB-08, TAB-12, TAB-17).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TableOptions {
    /// Whether the name shows above the table.
    pub show_name: bool,
    /// Whether the name shows centered, as the title of a table of Pages
    /// 15, rather than at the left, as in Desk Tables (PXV-15).
    pub name_centered: bool,
    /// Whether a line goes around the name: Outline Table Title of Pages
    /// 15 (PXV-15).
    pub name_outline: bool,
    /// Whether the header rows repeat on each page, column or box that
    /// the table goes on to (TAB-12).
    pub repeat_headers: bool,
    /// Whether Return goes to the next cell, rather than to a new line
    /// in the cell (TAB-08).
    pub return_moves: bool,
    /// What Tab does in the last cell.
    pub last_cell_tab: LastCellTab,
    /// Whether typing suggests values of the same column (TAB-17).
    pub autocomplete: bool,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            show_name: false,
            name_centered: false,
            name_outline: false,
            repeat_headers: true,
            return_moves: true,
            last_cell_tab: LastCellTab::AddRow,
            autocomplete: true,
        }
    }
}

/// Why a change of a table was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    /// A row or column out of the table.
    OutOfRange,
    /// Merged cells must be a rectangle of cells of one region (TAB-23).
    MixedRegions,
    /// The change would leave a table with no row or no column, or with
    /// more headers than allowed.
    BadSize,
    /// The area cuts through merged cells.
    CutsMerge,
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OutOfRange => "out of the table",
            Self::MixedRegions => "cells of different parts of the table",
            Self::BadSize => "bad size",
            Self::CutsMerge => "cuts through merged cells",
        })
    }
}

impl std::error::Error for TableError {}

/// A table.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Table<T> {
    /// The name, as Table 1, which formulas of other tables use.
    pub name: String,
    /// The widths of the columns, in thousandths of a point.
    widths: Vec<u32>,
    /// The least heights of the rows, in thousandths of a point; a row
    /// grows to fit its text.
    heights: Vec<u32>,
    /// The cells, by rows.
    cells: Vec<Cell<T>>,
    header_rows: u32,
    header_cols: u32,
    footer_rows: u32,
    /// The merged cells.
    merges: Vec<Area>,
    /// The lines above each row and below the last, for each column:
    /// `(rows + 1) × cols`.
    h_borders: Vec<Option<Border>>,
    /// The lines left of each column and right of the last, for each
    /// row: `rows × (cols + 1)`.
    v_borders: Vec<Option<Border>>,
    /// The look of its parts.
    pub style: TableStyle,
    /// The options.
    pub options: TableOptions,
}

/// The default border of new tables: a thin gray line.
pub const DEFAULT_BORDER: Border = Border::solid(1_000, Color::rgb(0xa6, 0xa6, 0xa6));

impl<T: CellText> Table<T> {
    /// A table of empty cells, with one header row and one header column
    /// when it is at least 2 by 2, as iWork makes them, columns of 1 inch
    /// and rows of 20 points, and thin borders all around.
    pub fn new(rows: u32, cols: u32) -> Self {
        let rows = rows.clamp(1, MAX_CELLS);
        let cols = cols.clamp(1, MAX_CELLS);
        // In usize: the product of the largest sizes only just fits u32.
        let (r, c) = (rows as usize, cols as usize);
        let n = r * c;
        Self {
            name: String::new(),
            widths: vec![72_000; cols as usize],
            heights: vec![20_000; rows as usize],
            cells: vec![Cell::default(); n],
            header_rows: u32::from(rows > 1),
            header_cols: u32::from(cols > 1),
            footer_rows: 0,
            merges: Vec::new(),
            h_borders: vec![Some(DEFAULT_BORDER); (r + 1) * c],
            v_borders: vec![Some(DEFAULT_BORDER); r * (c + 1)],
            style: TableStyle::default(),
            options: TableOptions::default(),
        }
    }

    /// A table with the values of a grid of text, by rows, without
    /// headers.
    pub fn from_rows(rows: &[Vec<String>], locale: &crate::Locale) -> Self {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1) as u32;
        let mut table = Self::new(rows.len().max(1) as u32, cols);
        table.header_rows = 0;
        table.header_cols = 0;
        for (r, row) in rows.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                table.type_text(r as u32, c as u32, text, locale);
            }
        }
        table
    }

    /// Whether the parts of the table agree with its size: as many cells
    /// and lines as rows and columns call for, headers and footers that
    /// fit, and merges inside it. A table made by these methods always
    /// does; one read from a file may not, and indexing it would panic.
    pub fn is_consistent(&self) -> bool {
        let (rows, cols) = (self.heights.len() as u64, self.widths.len() as u64);
        let fits = |n: u64| (1..=u64::from(MAX_CELLS)).contains(&n);
        fits(rows)
            && fits(cols)
            && self.cells.len() as u64 == rows * cols
            && self.h_borders.len() as u64 == (rows + 1) * cols
            && self.v_borders.len() as u64 == rows * (cols + 1)
            && self.header_rows <= MAX_HEADERS
            && self.header_cols <= MAX_HEADERS
            && u64::from(self.header_rows) + u64::from(self.footer_rows) <= rows
            && u64::from(self.header_cols) <= cols
            && self.merges.iter().all(|m| {
                m.rows > 0
                    && m.cols > 0
                    && u64::from(m.row) + u64::from(m.rows) <= rows
                    && u64::from(m.col) + u64::from(m.cols) <= cols
            })
    }

    /// How many rows.
    pub fn rows(&self) -> u32 {
        self.heights.len() as u32
    }

    /// How many columns.
    pub fn cols(&self) -> u32 {
        self.widths.len() as u32
    }

    /// How many header rows.
    pub fn header_rows(&self) -> u32 {
        self.header_rows
    }

    /// How many header columns.
    pub fn header_cols(&self) -> u32 {
        self.header_cols
    }

    /// How many footer rows.
    pub fn footer_rows(&self) -> u32 {
        self.footer_rows
    }

    /// The first row of the body.
    pub fn body_start(&self) -> u32 {
        self.header_rows
    }

    /// The row after the body, where the footer starts.
    pub fn body_end(&self) -> u32 {
        self.rows() - self.footer_rows
    }

    fn index(&self, row: u32, col: u32) -> usize {
        (row * self.cols() + col) as usize
    }

    /// A cell.
    ///
    /// # Panics
    ///
    /// When the cell is out of the table.
    pub fn cell(&self, row: u32, col: u32) -> &Cell<T> {
        assert!(
            row < self.rows() && col < self.cols(),
            "cell out of the table"
        );
        &self.cells[self.index(row, col)]
    }

    /// A cell, if it is in the table.
    pub fn get(&self, row: u32, col: u32) -> Option<&Cell<T>> {
        (row < self.rows() && col < self.cols()).then(|| &self.cells[self.index(row, col)])
    }

    /// A cell to change.
    ///
    /// # Panics
    ///
    /// When the cell is out of the table.
    pub fn cell_mut(&mut self, row: u32, col: u32) -> &mut Cell<T> {
        assert!(
            row < self.rows() && col < self.cols(),
            "cell out of the table"
        );
        let i = self.index(row, col);
        &mut self.cells[i]
    }

    /// Sets what a cell holds.
    pub fn set_content(&mut self, row: u32, col: u32, content: Content<T>) {
        if row < self.rows() && col < self.cols() {
            self.cell_mut(row, col).content = content;
        }
    }

    /// Puts typed text in a cell, as iWork reads it (TAB-18): a formula,
    /// a number, a date or text, with the format it was typed in when the
    /// cell's format is Automatic.
    pub fn type_text(&mut self, row: u32, col: u32, text: &str, locale: &crate::Locale) {
        if row >= self.rows() || col >= self.cols() {
            return;
        }
        let format = self.cell(row, col).format.clone();
        let (content, new_format) = crate::format::parse_input(text, &format, locale);
        let cell = self.cell_mut(row, col);
        cell.content = content;
        if let Some(f) = new_format {
            cell.format = f;
        }
    }

    /// The width of a column, in thousandths of a point.
    pub fn width(&self, col: u32) -> u32 {
        self.widths[col as usize]
    }

    /// The least height of a row, in thousandths of a point.
    pub fn height(&self, row: u32) -> u32 {
        self.heights[row as usize]
    }

    /// Sets the width of a column, at least 6 points (TAB-13).
    pub fn set_width(&mut self, col: u32, width: u32) {
        if let Some(w) = self.widths.get_mut(col as usize) {
            *w = width.max(6_000);
        }
    }

    /// Sets the least height of a row, at least 6 points (TAB-13).
    pub fn set_height(&mut self, row: u32, height: u32) {
        if let Some(h) = self.heights.get_mut(row as usize) {
            *h = height.max(6_000);
        }
    }

    /// The width of the table.
    pub fn total_width(&self) -> u32 {
        self.widths.iter().sum()
    }

    /// The part of the table a cell is in.
    pub fn region(&self, row: u32, col: u32) -> Region {
        if row < self.header_rows {
            Region::HeaderRow
        } else if row >= self.body_end() {
            Region::Footer
        } else if col < self.header_cols {
            Region::HeaderColumn
        } else {
            Region::Body
        }
    }

    /// Sets how many header rows the table has, up to 5, and no more than
    /// leave a row of body (TAB-12).
    pub fn set_header_rows(&mut self, n: u32) -> Result<(), TableError> {
        if n > MAX_HEADERS || n + self.footer_rows > self.rows() {
            return Err(TableError::BadSize);
        }
        self.header_rows = n;
        Ok(())
    }

    /// Sets how many header columns the table has, up to 5.
    pub fn set_header_cols(&mut self, n: u32) -> Result<(), TableError> {
        if n > MAX_HEADERS || n > self.cols() {
            return Err(TableError::BadSize);
        }
        self.header_cols = n;
        Ok(())
    }

    /// Sets how many footer rows the table has, up to 5.
    pub fn set_footer_rows(&mut self, n: u32) -> Result<(), TableError> {
        if n > MAX_HEADERS || n + self.header_rows > self.rows() {
            return Err(TableError::BadSize);
        }
        self.footer_rows = n;
        Ok(())
    }

    /// The merged cells.
    pub fn merges(&self) -> &[Area] {
        &self.merges
    }

    /// The merged cells that a cell is part of.
    pub fn merge_at(&self, row: u32, col: u32) -> Option<Area> {
        self.merges.iter().copied().find(|m| m.contains(row, col))
    }

    /// Whether a cell is hidden under the first cell of merged cells.
    pub fn is_covered(&self, row: u32, col: u32) -> bool {
        self.merge_at(row, col)
            .is_some_and(|m| (m.row, m.col) != (row, col))
    }

    /// The area that a cell shows on: its merged cells, or itself.
    pub fn span(&self, row: u32, col: u32) -> Area {
        self.merge_at(row, col).unwrap_or(Area::cell(row, col))
    }

    pub(crate) fn merges_mut(&mut self) -> &mut Vec<Area> {
        &mut self.merges
    }

    /// The line above a row (or below the last, for `row == rows`) over a
    /// column.
    pub fn h_border(&self, row: u32, col: u32) -> Option<Border> {
        self.h_borders
            .get((row * self.cols() + col) as usize)
            .copied()
            .flatten()
    }

    /// The line left of a column (or right of the last) beside a row.
    pub fn v_border(&self, row: u32, col: u32) -> Option<Border> {
        self.v_borders
            .get((row * (self.cols() + 1) + col) as usize)
            .copied()
            .flatten()
    }

    /// Sets the line above a row over a column (TAB-25).
    pub fn set_h_border(&mut self, row: u32, col: u32, border: Option<Border>) {
        let i = (row * self.cols() + col) as usize;
        if let Some(b) = self.h_borders.get_mut(i) {
            *b = border;
        }
    }

    /// Sets the line left of a column beside a row (TAB-25).
    pub fn set_v_border(&mut self, row: u32, col: u32, border: Option<Border>) {
        let i = (row * (self.cols() + 1) + col) as usize;
        if let Some(b) = self.v_borders.get_mut(i) {
            *b = border;
        }
    }

    /// Which borders of an area to set.
    pub fn set_borders(&mut self, area: Area, which: BorderSet, border: Option<Border>) {
        for (r, c) in area.cells().collect::<Vec<_>>() {
            let top = r == area.row;
            let bottom = r + 1 == area.row_end();
            let left = c == area.col;
            let right = c + 1 == area.col_end();
            let outer = matches!(which, BorderSet::All | BorderSet::Outside);
            let inner = matches!(which, BorderSet::All | BorderSet::Inside);
            if (top && (outer || which == BorderSet::Top)) || (!top && inner) {
                self.set_h_border(r, c, border);
            }
            if bottom && (outer || which == BorderSet::Bottom) {
                self.set_h_border(r + 1, c, border);
            }
            if (left && (outer || which == BorderSet::Left)) || (!left && inner) {
                self.set_v_border(r, c, border);
            }
            if right && (outer || which == BorderSet::Right) {
                self.set_v_border(r, c + 1, border);
            }
        }
    }

    /// Adds `count` rows before row `at` (`at == rows` adds them at the
    /// end), looking like the row before them, and moves the references
    /// of formulas (TAB-10). A body column whose cells above all hold the
    /// same formula gets it in the new rows too.
    pub fn insert_rows(&mut self, at: u32, count: u32) -> Result<(), TableError> {
        if at > self.rows() || count == 0 || self.rows() + count > MAX_CELLS {
            return Err(TableError::OutOfRange);
        }
        let cols = self.cols();
        let model_row = if at > 0 { at - 1 } else { 0 };
        let repeated: Vec<Option<String>> =
            (0..cols).map(|c| self.repeated_formula(at, c)).collect();
        self.shift_formulas(Axis::Rows, at, i64::from(count));
        let mut new_cells = Vec::with_capacity((count * cols) as usize);
        for k in 0..count {
            for c in 0..cols {
                let mut cell = self.cell(model_row, c).cleared();
                cell.comment = None;
                cell.move_references(0, i64::from(at + k) - i64::from(model_row));
                if let Some(f) = &repeated[c as usize]
                    && let Ok(formula) = Formula::parse(f)
                {
                    cell.content = Content::Formula(formula.moved(0, i64::from(k) + 1).to_string());
                }
                new_cells.push(cell);
            }
        }
        let i = (at * cols) as usize;
        self.cells.splice(i..i, new_cells);
        let h = self.heights[model_row as usize];
        self.heights.splice(
            at as usize..at as usize,
            std::iter::repeat_n(h, count as usize),
        );
        // Borders: the new rows take the lines of the row they copy.
        let above: Vec<Option<Border>> = (0..cols)
            .map(|c| self.h_border(at.max(1).min(self.rows()), c))
            .collect();
        let hi = (at * cols) as usize;
        let new_h: Vec<Option<Border>> = (0..count).flat_map(|_| above.clone()).collect();
        self.h_borders.splice(hi..hi, new_h);
        let side: Vec<Option<Border>> = (0..=cols).map(|c| self.v_border(model_row, c)).collect();
        let vi = (at * (cols + 1)) as usize;
        let new_v: Vec<Option<Border>> = (0..count).flat_map(|_| side.clone()).collect();
        self.v_borders.splice(vi..vi, new_v);
        for m in &mut self.merges {
            if m.row >= at {
                m.row += count;
            } else if m.row_end() > at {
                m.rows += count;
            }
        }
        if at < self.header_rows {
            self.header_rows = (self.header_rows + count).min(MAX_HEADERS);
        } else if self.footer_rows > 0 && at > self.rows() - count - self.footer_rows {
            // Inside the footer, or after it at the end: the footer takes
            // the new rows. At the end they would otherwise become the
            // footer, and the footer rows they push up, as a row of
            // totals, the body.
            self.footer_rows = (self.footer_rows + count).min(MAX_HEADERS);
        }
        Ok(())
    }

    /// The formula that the cells of a column repeat down to row `at`,
    /// copied relatively, if every body cell above has it (TAB-10).
    fn repeated_formula(&self, at: u32, col: u32) -> Option<String> {
        let start = self.header_rows;
        if at <= start || at > self.body_end() {
            return None;
        }
        let last = Formula::parse(self.cell(at - 1, col).content.formula()?).ok()?;
        for r in start..at - 1 {
            let f = Formula::parse(self.cell(r, col).content.formula()?).ok()?;
            if f.moved(0, i64::from(at - 1 - r)) != last {
                return None;
            }
        }
        Some(last.to_string())
    }

    /// Adds `count` columns before column `at` (TAB-10).
    pub fn insert_cols(&mut self, at: u32, count: u32) -> Result<(), TableError> {
        if at > self.cols() || count == 0 || self.cols() + count > MAX_CELLS {
            return Err(TableError::OutOfRange);
        }
        let old_cols = self.cols();
        let model_col = if at > 0 { at - 1 } else { 0 };
        self.shift_formulas(Axis::Columns, at, i64::from(count));
        let rows = self.rows();
        let mut cells = Vec::with_capacity(((old_cols + count) * rows) as usize);
        for r in 0..rows {
            for c in 0..old_cols {
                if c == at {
                    for _ in 0..count {
                        let mut cell = self.cell(r, model_col).cleared();
                        cell.comment = None;
                        cells.push(cell);
                    }
                }
                cells.push(self.cell(r, c).clone());
            }
            if at == old_cols {
                for _ in 0..count {
                    let mut cell = self.cell(r, model_col).cleared();
                    cell.comment = None;
                    cells.push(cell);
                }
            }
        }
        let mut h = Vec::with_capacity(((rows + 1) * (old_cols + count)) as usize);
        for r in 0..=rows {
            for c in 0..old_cols {
                if c == at {
                    for _ in 0..count {
                        h.push(self.h_border(r, model_col));
                    }
                }
                h.push(self.h_border(r, c));
            }
            if at == old_cols {
                for _ in 0..count {
                    h.push(self.h_border(r, model_col));
                }
            }
        }
        let mut v = Vec::with_capacity((rows * (old_cols + count + 1)) as usize);
        for r in 0..rows {
            for c in 0..=old_cols {
                if c == at {
                    for _ in 0..count {
                        v.push(self.v_border(r, at.min(old_cols)));
                    }
                }
                v.push(self.v_border(r, c));
            }
        }
        self.cells = cells;
        self.h_borders = h;
        self.v_borders = v;
        let w = self.widths[model_col as usize];
        self.widths.splice(
            at as usize..at as usize,
            std::iter::repeat_n(w, count as usize),
        );
        for m in &mut self.merges {
            if m.col >= at {
                m.col += count;
            } else if m.col_end() > at {
                m.cols += count;
            }
        }
        if at < self.header_cols {
            self.header_cols = (self.header_cols + count).min(MAX_HEADERS);
        }
        Ok(())
    }

    /// Deletes `count` rows from row `at` (TAB-11). References to them
    /// become `#REF!`.
    pub fn delete_rows(&mut self, at: u32, count: u32) -> Result<(), TableError> {
        if count == 0 || at + count > self.rows() || count >= self.rows() {
            return Err(TableError::OutOfRange);
        }
        let cols = self.cols();
        let end = at + count;
        let headers_gone = self.header_rows.min(end).saturating_sub(at);
        let footer_start = self.body_end();
        let footers_gone = end.min(self.rows()).saturating_sub(at.max(footer_start));
        self.cells
            .drain((at * cols) as usize..(end * cols) as usize);
        self.heights.drain(at as usize..end as usize);
        self.h_borders
            .drain((at * cols) as usize..(end * cols) as usize);
        self.v_borders
            .drain((at * (cols + 1)) as usize..(end * (cols + 1)) as usize);
        self.merges = shrink_merges(
            &self.merges,
            |m| (m.row, m.rows),
            |m, v, n| {
                m.row = v;
                m.rows = n;
            },
            at,
            count,
        );
        self.header_rows -= headers_gone;
        self.footer_rows -= footers_gone;
        self.shift_formulas(Axis::Rows, at, -i64::from(count));
        Ok(())
    }

    /// Deletes `count` columns from column `at` (TAB-11).
    pub fn delete_cols(&mut self, at: u32, count: u32) -> Result<(), TableError> {
        if count == 0 || at + count > self.cols() || count >= self.cols() {
            return Err(TableError::OutOfRange);
        }
        let old_cols = self.cols();
        let end = at + count;
        let rows = self.rows();
        let keep = |c: u32| c < at || c >= end;
        let mut cells = Vec::new();
        for r in 0..rows {
            for c in (0..old_cols).filter(|c| keep(*c)) {
                cells.push(self.cell(r, c).clone());
            }
        }
        let mut h = Vec::new();
        for r in 0..=rows {
            for c in (0..old_cols).filter(|c| keep(*c)) {
                h.push(self.h_border(r, c));
            }
        }
        let mut v = Vec::new();
        for r in 0..rows {
            for c in (0..=old_cols).filter(|c| *c <= at || *c > end) {
                v.push(self.v_border(r, c));
            }
        }
        self.cells = cells;
        self.h_borders = h;
        self.v_borders = v;
        self.widths.drain(at as usize..end as usize);
        self.merges = shrink_merges(
            &self.merges,
            |m| (m.col, m.cols),
            |m, v, n| {
                m.col = v;
                m.cols = n;
            },
            at,
            count,
        );
        let headers_gone = self.header_cols.min(end).saturating_sub(at);
        self.header_cols -= headers_gone;
        self.shift_formulas(Axis::Columns, at, -i64::from(count));
        Ok(())
    }

    fn shift_formulas(&mut self, axis: Axis, at: u32, count: i64) {
        // A cell a rule compares with moves as a formula's reference does,
        // and is deleted with its row or column.
        let mut shift = |r: desk_formula::CellRef| match (Formula {
            expr: desk_formula::Expr::Cell(r),
        })
        .shifted(axis, at, count)
        .expr
        {
            desk_formula::Expr::Cell(r) => Some(r),
            _ => None,
        };
        for cell in &mut self.cells {
            if let Content::Formula(text) = &cell.content
                && let Ok(f) = Formula::parse(text)
            {
                cell.content = Content::Formula(f.shifted(axis, at, count).to_string());
            }
            cell.rules = crate::conditional::map_rule_cells(&cell.rules, &mut shift);
        }
    }

    /// The cells by rows, for the crate's operations.
    pub(crate) fn cells_mut(&mut self) -> &mut Vec<Cell<T>> {
        &mut self.cells
    }

    pub(crate) fn heights_mut(&mut self) -> &mut Vec<u32> {
        &mut self.heights
    }

    pub(crate) fn widths_mut(&mut self) -> &mut Vec<u32> {
        &mut self.widths
    }

    /// The text of a cell without formatting, or the formula, as the
    /// editor of a cell starts with.
    pub fn source_text(&self, row: u32, col: u32, locale: &crate::Locale) -> String {
        let cell = self.cell(row, col);
        match &cell.content {
            Content::Empty => String::new(),
            Content::Text(t) => t.plain(),
            Content::Formula(f) => f.clone(),
            Content::Bool(b) => {
                crate::Format::Automatic
                    .show(&desk_formula::Value::Bool(*b), locale)
                    .text
            }
            Content::Number(n) => match &cell.format {
                crate::Format::Percent { .. } => {
                    let s = crate::Format::Automatic
                        .show(&desk_formula::Value::Number(n.0 * 100.0), locale)
                        .text;
                    format!("{s}%")
                }
                _ => {
                    crate::Format::Automatic
                        .show(&desk_formula::Value::Number(n.0), locale)
                        .text
                }
            },
            Content::Date(d) => {
                crate::Format::Automatic
                    .show(&desk_formula::Value::Date(d.0), locale)
                    .text
            }
            Content::Duration(d) => {
                crate::Format::Automatic
                    .show(&desk_formula::Value::Duration(d.0), locale)
                    .text
            }
        }
    }

    /// All the values typed in a column of the body, for the suggestions
    /// of typing (TAB-17), in order, without repeats.
    pub fn column_texts(&self, col: u32) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for r in self.body_start()..self.body_end() {
            if let Content::Text(t) = &self.cell(r, col).content {
                let p = t.plain();
                if !p.is_empty() && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out
    }

    /// The suggestion of typing for a cell: the one value of its column
    /// that starts with what is typed, in any case (TAB-17).
    pub fn suggestion(&self, row: u32, col: u32, typed: &str) -> Option<String> {
        if !self.options.autocomplete || typed.is_empty() {
            return None;
        }
        let lower = typed.to_lowercase();
        // The texts of the other cells of the column, by row: the list of
        // the column skips empty cells and repeats, so its index is no
        // row (review RV, batch 2).
        let mut found: Vec<String> = Vec::new();
        for r in self.body_start()..self.body_end() {
            if r == row {
                continue;
            }
            if let Content::Text(t) = &self.cell(r, col).content {
                let t = t.plain();
                if t.to_lowercase().starts_with(&lower)
                    && t.len() > typed.len()
                    && !found.contains(&t)
                {
                    found.push(t);
                }
            }
        }
        match found.as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        }
    }

    /// The table with the cells of a column moved, for operations.
    pub(crate) fn replace_cells(&mut self, cells: Vec<Cell<T>>) {
        debug_assert_eq!(cells.len(), self.cells.len());
        self.cells = cells;
    }

    /// The comments, by cell, row by row.
    pub fn comments(&self) -> Vec<((u32, u32), std::num::NonZeroU64)> {
        let mut out = Vec::new();
        for r in 0..self.rows() {
            for c in 0..self.cols() {
                if let Some(id) = self.cell(r, c).comment {
                    out.push(((r, c), id));
                }
            }
        }
        out
    }
}

/// Which borders of an area a change sets (TAB-25).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderSet {
    /// Every line, outside and between the cells.
    All,
    /// The outline.
    Outside,
    /// The lines between the cells.
    Inside,
    /// The top line.
    Top,
    /// The bottom line.
    Bottom,
    /// The left line.
    Left,
    /// The right line.
    Right,
}

/// The merges after `count` rows or columns are deleted from `at`: those
/// all inside the deleted ones go, and the rest shrink or move.
fn shrink_merges(
    merges: &[Area],
    get: impl Fn(&Area) -> (u32, u32),
    set: impl Fn(&mut Area, u32, u32),
    at: u32,
    count: u32,
) -> Vec<Area> {
    let end = at + count;
    let mut out = Vec::new();
    for m in merges {
        let (start, len) = get(m);
        let m_end = start + len;
        let overlap = m_end.min(end).saturating_sub(start.max(at));
        let new_len = len - overlap;
        let new_start = if start >= end {
            start - count
        } else if start >= at {
            at
        } else {
            start
        };
        if new_len == 0 {
            continue;
        }
        let mut merged = *m;
        set(&mut merged, new_start, new_len);
        if merged.rows * merged.cols > 1 {
            out.push(merged);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Locale;

    fn t(rows: u32, cols: u32) -> Table<String> {
        Table::new(rows, cols)
    }

    #[test]
    fn tables_made_here_are_consistent_and_broken_ones_are_not() {
        let mut table = t(3, 2);
        assert!(table.is_consistent());
        table
            .merge(Area::between((1, 1), (2, 1)), &crate::Locale::en_us())
            .unwrap();
        assert!(table.is_consistent());
        let mut short = t(3, 2);
        short.cells.pop();
        assert!(!short.is_consistent());
        let mut lines = t(3, 2);
        lines.v_borders.pop();
        assert!(!lines.is_consistent());
        let mut outside = t(3, 2);
        outside.merges.push(Area::between((2, 1), (3, 1)));
        assert!(!outside.is_consistent());
        let mut headers = t(3, 2);
        headers.footer_rows = 3;
        assert!(!headers.is_consistent());
    }

    #[test]
    fn tab_01_new_tables_have_headers() {
        let table = t(4, 3);
        assert_eq!((table.rows(), table.cols()), (4, 3));
        assert_eq!((table.header_rows(), table.header_cols()), (1, 1));
        assert_eq!(table.region(0, 2), Region::HeaderRow);
        assert_eq!(table.region(2, 0), Region::HeaderColumn);
        assert_eq!(table.region(2, 2), Region::Body);
    }

    #[test]
    fn tab_10_new_rows_repeat_formulas_and_move_references() {
        let en = Locale::en_us();
        let mut table = t(4, 3);
        table.type_text(1, 1, "1", &en);
        table.type_text(2, 1, "2", &en);
        table.type_text(1, 2, "=B2*2", &en);
        table.type_text(2, 2, "=B3*2", &en);
        table.type_text(3, 2, "=SUM(C2:C3)", &en);
        table.set_footer_rows(1).unwrap();
        table.insert_rows(3, 1).unwrap();
        assert_eq!(table.rows(), 5);
        assert_eq!(table.cell(3, 2).content, Content::Formula("=B4*2".into()));
        assert_eq!(
            table.cell(4, 2).content,
            Content::Formula("=SUM(C2:C3)".into())
        );
        table.insert_rows(2, 1).unwrap();
        assert_eq!(
            table.cell(5, 2).content,
            Content::Formula("=SUM(C2:C4)".into())
        );
        assert_eq!(table.footer_rows(), 1);
        // Rows added at the end of a table with a footer join the footer:
        // the totals stayed in the footer, not the new empty row (review
        // RV, batch 2).
        let rows = table.rows();
        table.insert_rows(rows, 1).unwrap();
        assert_eq!(table.footer_rows(), 2);
        assert_eq!(table.region(rows - 1, 2), Region::Footer);
    }

    #[test]
    fn tab_11_deleting_rows_and_columns() {
        let en = Locale::en_us();
        let mut table = t(4, 4);
        table.type_text(3, 3, "=B2+C3", &en);
        table.delete_rows(1, 1).unwrap();
        assert_eq!(
            table.cell(2, 3).content,
            Content::Formula("=#REF!+C2".into())
        );
        table.delete_cols(2, 1).unwrap();
        assert_eq!(table.cols(), 3);
        assert_eq!(
            table.cell(2, 2).content,
            Content::Formula("=#REF!+#REF!".into())
        );
        assert_eq!(table.delete_rows(0, 3), Err(TableError::OutOfRange));
    }

    #[test]
    fn tab_12_headers_up_to_five() {
        let mut table = t(8, 3);
        assert_eq!(table.set_header_rows(6), Err(TableError::BadSize));
        table.set_header_rows(5).unwrap();
        table.set_footer_rows(3).unwrap();
        assert_eq!(table.region(7, 1), Region::Footer);
        assert_eq!(table.set_footer_rows(4), Err(TableError::BadSize));
    }

    #[test]
    fn tab_25_borders_of_an_area() {
        let mut table = t(3, 3);
        let red = Border::solid(2_000, Color::rgb(255, 0, 0));
        table.set_borders(Area::between((0, 0), (1, 1)), BorderSet::Outside, Some(red));
        assert_eq!(table.h_border(0, 0), Some(red));
        assert_eq!(table.h_border(2, 1), Some(red));
        assert_eq!(table.v_border(1, 2), Some(red));
        assert_eq!(table.h_border(1, 0), Some(DEFAULT_BORDER));
        table.set_borders(Area::cell(2, 2), BorderSet::All, None);
        assert_eq!(table.v_border(2, 3), None);
        table.insert_cols(1, 1).unwrap();
        assert_eq!(table.cols(), 4);
        assert_eq!(table.h_border(0, 0), Some(red));
    }

    #[test]
    fn tab_17_suggestions_from_the_column() {
        let en = Locale::en_us();
        let mut table = t(4, 2);
        table.type_text(1, 1, "Maçã", &en);
        table.type_text(2, 1, "Banana", &en);
        assert_eq!(table.suggestion(3, 1, "ma"), Some("Maçã".into()));
        assert_eq!(table.suggestion(3, 1, "x"), None);
        // An empty cell above: the suggestion still comes, from the row
        // below (review RV, batch 2). The cell typed in is not its own
        // suggestion.
        let mut other = t(4, 2);
        other.type_text(2, 1, "Maçã", &en);
        assert_eq!(other.suggestion(1, 1, "ma"), Some("Maçã".into()));
        assert_eq!(other.suggestion(2, 1, "ma"), None);
        table.options.autocomplete = false;
        assert_eq!(table.suggestion(3, 1, "ma"), None);
    }
}
