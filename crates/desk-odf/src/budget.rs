// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! What the readers of one document may expand. ODF writes a row or a
//! cell once for many the same (`table:number-rows-repeated`,
//! `table:number-columns-repeated`), so a few hundred bytes can stand for
//! millions of cells, each holding a copy of whatever the one written
//! holds. The limits of rows and columns hold for each table; these hold
//! for the document as a whole, and count what each copy weighs.

use crate::xml::{Element, Node};

/// The most cells read in the tables of one document, all together.
pub const MAX_DOCUMENT_CELLS: usize = 1 << 20;

/// The most bytes of content the tables and charts of one document may
/// copy: text, attributes and elements, counted as they were written in
/// the stream, for each copy.
pub const MAX_DOCUMENT_BYTES: usize = 64 << 20;

/// The most cells read in the data tables of the charts of one document.
pub const MAX_CHART_CELLS: usize = 1 << 20;

/// The most values the series and categories of the charts of one
/// document read from their data tables.
pub const MAX_CHART_VALUES: usize = 1 << 23;

/// What one copy of a row or a cell costs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cost {
    pub cells: usize,
    pub bytes: usize,
}

#[derive(Clone, Copy)]
struct Left {
    cells: usize,
    bytes: usize,
    chart_cells: usize,
    chart_values: usize,
}

const FULL: Left = Left {
    cells: MAX_DOCUMENT_CELLS,
    bytes: MAX_DOCUMENT_BYTES,
    chart_cells: MAX_CHART_CELLS,
    chart_values: MAX_CHART_VALUES,
};

thread_local! {
    /// What the document read now may still expand.
    static LEFT: std::cell::Cell<Left> = const { std::cell::Cell::new(FULL) };
}

/// Starts the count of a new document.
pub(crate) fn start_document() {
    LEFT.set(FULL);
}

/// What the tables of the document may still read, as a cost. The cost
/// of reading something is what is left before less what is left after.
pub(crate) fn table_left() -> Cost {
    let left = LEFT.get();
    Cost {
        cells: left.cells,
        bytes: left.bytes,
    }
}

/// The cost spent between two readings of [`table_left`].
pub(crate) fn spent(before: Cost, after: Cost) -> Cost {
    Cost {
        cells: before.cells.saturating_sub(after.cells),
        bytes: before.bytes.saturating_sub(after.bytes),
    }
}

fn copies(wanted: usize, cost: Cost, cells: usize, bytes: usize) -> usize {
    let by_cells = cells.checked_div(cost.cells).unwrap_or(usize::MAX);
    let by_bytes = bytes.checked_div(cost.bytes).unwrap_or(usize::MAX);
    wanted.min(by_cells).min(by_bytes)
}

/// How many of `wanted` copies that cost `cost` each the tables of the
/// document may still read, which are then counted.
pub(crate) fn take_table(wanted: usize, cost: Cost) -> usize {
    let mut left = LEFT.get();
    let n = copies(wanted, cost, left.cells, left.bytes);
    left.cells -= n * cost.cells;
    left.bytes -= n * cost.bytes;
    LEFT.set(left);
    n
}

/// How many of `wanted` copies that cost `cost` each the charts of the
/// document may still read into their data tables, which are then
/// counted. The bytes are those the tables count too.
pub(crate) fn take_chart(wanted: usize, cost: Cost) -> usize {
    let mut left = LEFT.get();
    let n = copies(wanted, cost, left.chart_cells, left.bytes);
    left.chart_cells -= n * cost.cells;
    left.bytes -= n * cost.bytes;
    LEFT.set(left);
    n
}

/// The values the charts of the document may still read from their data
/// tables.
pub(crate) fn chart_values_left() -> usize {
    LEFT.get().chart_values
}

/// Counts values read, and text copied with them.
pub(crate) fn spend_chart_values(values: usize, bytes: usize) {
    let mut left = LEFT.get();
    left.chart_values = left.chart_values.saturating_sub(values);
    left.bytes = left.bytes.saturating_sub(bytes);
    LEFT.set(left);
}

/// What an element weighs: the bytes of its names, attributes and text,
/// and of everything inside.
pub(crate) fn weight(e: &Element) -> usize {
    let own = e.name.local.len()
        + e.attrs
            .iter()
            .map(|(name, value)| name.local.len() + value.len() + 4)
            .sum::<usize>()
        + 8;
    own + e
        .children
        .iter()
        .map(|node| match node {
            Node::Element(child) => weight(child),
            Node::Text(text) => text.len(),
        })
        .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_are_bounded_by_cells_and_by_bytes() {
        start_document();
        let heavy = Cost {
            cells: 1,
            bytes: 1 << 20,
        };
        assert_eq!(take_table(1000, heavy), 64);
        assert_eq!(take_table(1, heavy), 0);
        start_document();
        let light = Cost { cells: 1, bytes: 0 };
        assert_eq!(take_table(usize::MAX, light), MAX_DOCUMENT_CELLS);
        assert_eq!(take_chart(usize::MAX, light), MAX_CHART_CELLS);
        start_document();
        assert_eq!(take_table(7, Cost::default()), 7);
    }
}
