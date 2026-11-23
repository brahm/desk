// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The tables of a Pages document (`sf:tabular-info`, with its
//! `sf:tabular-model`), as the documents that the project made with Pages
//! '09 save them (B-048, TAB-01).
//!
//! The grid (`sf:grid`) gives the number of rows and columns, the width
//! of each column and the height of each row. Its cells (`sf:datasource`)
//! come row by row, one element for each: `sf:t` text, `sf:n` a number,
//! `sf:d` a date, `sf:du` a length of time, `sf:f` a formula with its
//! result, `sf:g` an empty cell and `sf:s` a cell that a merged cell
//! covers. A cell is in the column after the one before it, unless its
//! `sf:ct` says how many columns to go, as a byte: 254 is two columns
//! back, and a cell in a column at or before the one of the cell before it
//! is in the next row. The cells after the last are empty.

use crate::ns::{SF, SFA, SL};
use crate::sf::Ids;
use crate::xml::Element;

/// Seconds from 1 January 2001, from which Pages counts dates, to the
/// days from 30 December 1899, from which tables count them.
const DAYS_TO_2001: f64 = 36_892.0;

/// What a cell of a table holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CellContent {
    /// Nothing (`sf:g`), or a cell that a merged cell covers (`sf:s`).
    #[default]
    Empty,
    /// Text (`sf:t`), its paragraphs joined by line feeds.
    Text(String),
    /// A number (`sf:n`).
    Number(f64),
    /// A date (`sf:d`), in days from 30 December 1899.
    Date(f64),
    /// A length of time (`sf:du`), in days.
    Duration(f64),
    /// A formula (`sf:f`), as written, with the `=`.
    Formula(String),
}

/// A cell of a table that holds something or starts a merged cell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableCell {
    /// The row, from 0.
    pub row: u32,
    /// The column, from 0.
    pub col: u32,
    /// What it holds.
    pub content: CellContent,
    /// `sf:row-span`: how many rows it takes, merged.
    pub rows: u32,
    /// `sf:col-span`: how many columns it takes, merged.
    pub cols: u32,
}

/// A table (`sf:tabular-model`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// `sf:name`.
    pub name: String,
    /// `sf:name-is-visible`.
    pub name_shown: bool,
    /// `sf:num-header-rows`.
    pub header_rows: u32,
    /// `sf:num-header-columns`.
    pub header_cols: u32,
    /// `sf:num-footer-rows`.
    pub footer_rows: u32,
    /// The width of each column, in points.
    pub widths: Vec<f64>,
    /// The height of each row, in points.
    pub heights: Vec<f64>,
    /// The cells that hold something or are merged, row by row.
    pub cells: Vec<TableCell>,
    /// The cells with comments (`sl:cell-annotations`): the row, the
    /// column and the object ID that the comment names.
    pub comments: Vec<(u32, u32, String)>,
    /// The elements of cells that the reader does not read, by name.
    pub skipped: Vec<String>,
}

impl Table {
    /// The number of rows.
    pub fn rows(&self) -> u32 {
        self.heights.len() as u32
    }

    /// The number of columns.
    pub fn cols(&self) -> u32 {
        self.widths.len() as u32
    }
}

/// The most rows or columns a table is read with, so that a damaged file
/// cannot ask for a grid that does not fit in memory.
const MAX_LINES: usize = 10_000;

/// The most cells a table is read with, for the same reason; a table of
/// Pages '09 in the text of a document is far smaller.
const MAX_GRID: usize = 100_000;

/// Reads a table from its `sf:tabular-info` or `sf:tabular-model`.
pub(super) fn read(ids: &Ids<'_>, element: &Element) -> Option<Table> {
    let element = ids.deref(element);
    let model = if element.name.is(SF, "tabular-model") {
        element
    } else {
        ids.deref(element.child(SF, "tabular-model")?)
    };
    let grid = model.child(SF, "grid")?;
    let number = |e: &Element, name: &str| -> Option<f64> {
        e.attr(SF, name)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
    };
    let count = |name: &str| -> u32 {
        model
            .attr(SF, name)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    };
    let widths: Vec<f64> = grid
        .child(SF, "columns")?
        .children_named(SF, "grid-column")
        .take(MAX_LINES)
        .map(|c| number(c, "width").unwrap_or(72.0).max(1.0))
        .collect();
    let heights: Vec<f64> = grid
        .child(SF, "rows")?
        .children_named(SF, "grid-row")
        .take(MAX_LINES)
        .map(|r| number(r, "height").unwrap_or(18.0).max(1.0))
        .collect();
    if widths.is_empty() || heights.is_empty() || widths.len() * heights.len() > MAX_GRID {
        return None;
    }
    let mut table = Table {
        name: model.attr(SF, "name").unwrap_or_default().to_owned(),
        name_shown: model.attr(SF, "name-is-visible") == Some("true"),
        header_rows: count("num-header-rows"),
        header_cols: count("num-header-columns"),
        footer_rows: count("num-footer-rows"),
        widths,
        heights,
        ..Table::default()
    };
    let (rows, cols) = (table.rows(), table.cols());
    let (mut row, mut col): (u32, i64) = (0, 0);
    for (n, cell) in grid
        .child(SF, "datasource")
        .into_iter()
        .flat_map(Element::elements)
        .enumerate()
    {
        let step = match cell
            .attr(SF, "ct")
            .and_then(|v| v.trim().parse::<u8>().ok())
        {
            Some(byte) => i64::from(byte as i8),
            None if n == 0 => 0,
            None => 1,
        };
        let next = col + step;
        if n > 0 && next <= col {
            row += 1;
        }
        if next < 0 || next >= i64::from(cols) {
            table.skipped.push(cell.name.local.clone());
            break;
        }
        col = next;
        if row >= rows {
            break;
        }
        let content = match cell.name.local.as_str() {
            "g" | "s" => CellContent::Empty,
            "t" => CellContent::Text(cell_text(cell)),
            "n" => CellContent::Number(number(cell, "v").unwrap_or(0.0)),
            "d" => CellContent::Date(
                number(cell, "cell-date").unwrap_or(0.0) / 86_400.0 + DAYS_TO_2001,
            ),
            "du" => CellContent::Duration(number(cell, "du").unwrap_or(0.0) / 86_400.0),
            "f" => match cell
                .child(SF, "fo")
                .and_then(|f| f.attr(SF, "fs"))
                .filter(|f| f.starts_with('='))
            {
                Some(formula) => CellContent::Formula(formula.to_owned()),
                None => {
                    table.skipped.push("f".to_owned());
                    CellContent::Empty
                }
            },
            other => {
                table.skipped.push(other.to_owned());
                CellContent::Empty
            }
        };
        let span = |name: &str| -> u32 {
            cell.attr(SF, name)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(1)
                .max(1)
        };
        let (rows_spanned, cols_spanned) = (span("row-span"), span("col-span"));
        if content != CellContent::Empty || rows_spanned > 1 || cols_spanned > 1 {
            table.cells.push(TableCell {
                row,
                col: col as u32,
                content,
                rows: rows_spanned.min(rows - row),
                cols: cols_spanned.min(cols - col as u32),
            });
        }
    }
    for a in model
        .child(SL, "cell-annotations")
        .into_iter()
        .flat_map(|c| c.children_named(SL, "a-cell"))
    {
        let at = |name: &str| a.attr(SF, name).and_then(|v| v.trim().parse::<u32>().ok());
        if let (Some(r), Some(c), Some(id)) = (at("row"), at("column"), a.attr(SFA, "ID"))
            && r < rows
            && c < cols
        {
            table.comments.push((r, c, id.to_owned()));
        }
    }
    Some(table)
}

/// The text of a text cell: its plain text (`sf:ct sfa:s`), or the
/// paragraphs of its formatted text (`sf:so`), joined by line feeds.
fn cell_text(cell: &Element) -> String {
    if let Some(text) = cell.child(SF, "ct").and_then(|c| c.attr(SFA, "s")) {
        return text.to_owned();
    }
    let Some(body) = cell.child(SF, "so").and_then(|s| s.child(SF, "text-body")) else {
        return String::new();
    };
    body.descendants()
        .filter(|e| e.name.is(SF, "p"))
        .map(Element::text)
        .collect::<Vec<_>>()
        .join("\n")
}
