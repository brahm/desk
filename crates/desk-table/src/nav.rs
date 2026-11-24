// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Selecting cells and moving among them with the keyboard (matrix rows
//! TAB-07 and TAB-08).
//!
//! The rules are those of the table on page 183 of the Pages '09 guide,
//! which the current Pages keeps: the arrows move one cell, Tab and
//! Shift-Tab go through the cells in reading order, and Tab in the last
//! cell does what the option of the table says ([`LastCellTab`]). A merged
//! cell counts as one cell: a move leaves it from its edge, and a
//! selection that touches it takes it whole.
//!
//! These are pure functions of the table, so that the three apps answer
//! the keys the same way and the rules can be tested without a window; the
//! apps only turn keys into [`Step`]s and act on the [`Landing`].

use crate::{Area, CellText, LastCellTab, Table};

/// A move of the cell in focus, as a key asks for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Up arrow, or Shift-Return.
    Up,
    /// Down arrow, or Return when it moves to the next cell.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Tab: the next cell in reading order.
    Next,
    /// Shift-Tab: the previous cell in reading order.
    Previous,
}

/// Where a [`Step`] lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    /// On this cell, by row and column: the first cell of a merged cell.
    Cell(u32, u32),
    /// Tab in the last cell, when the table adds a row: the app adds one
    /// at the end and goes to its first cell.
    AddRow,
    /// Tab in the last cell, when the table adds a column: the app adds
    /// one at the end and goes to its cell in the same row.
    AddColumn,
}

impl<T: CellText> Table<T> {
    /// The cells selected from `anchor` to `focus`, grown until no merged
    /// cell is cut by the edge of the selection (TAB-07).
    ///
    /// Growing once is not enough: taking in one merged cell can reach
    /// another, so it repeats until nothing changes.
    pub fn selection_area(&self, anchor: (u32, u32), focus: (u32, u32)) -> Area {
        let mut area = Area::between(anchor, focus);
        loop {
            let mut grown = area;
            for (r, c) in area.cells() {
                if let Some(merge) = self.merge_at(r, c) {
                    grown = grown.union(&merge);
                }
            }
            if grown == area {
                return area;
            }
            area = grown;
        }
    }

    /// Where `step` takes the cell in focus at `from` (TAB-08).
    ///
    /// The arrows stop at the edges of the table. Tab goes right and then
    /// to the first cell of the next row; Shift-Tab the other way, and
    /// stops at the first cell. Tab in the last cell lands on
    /// [`Landing::AddRow`] or [`Landing::AddColumn`], or wraps to the
    /// first cell, as `options.last_cell_tab` says.
    pub fn step(&self, from: (u32, u32), step: Step) -> Landing {
        let (rows, cols) = (self.rows(), self.cols());
        let (mut r, mut c) = from;
        // Out of a merged cell, from its edge.
        let span = self.span(r, c);
        match step {
            Step::Up => r = span.row.saturating_sub(1),
            Step::Down => r = span.row_end().min(rows - 1),
            Step::Left => c = span.col.saturating_sub(1),
            Step::Right => c = span.col_end().min(cols - 1),
            Step::Next => {
                if span.col_end() < cols {
                    c = span.col_end();
                } else if r + 1 < rows {
                    (r, c) = (r + 1, 0);
                } else {
                    match self.options.last_cell_tab {
                        LastCellTab::AddRow => return Landing::AddRow,
                        LastCellTab::AddColumn => return Landing::AddColumn,
                        LastCellTab::Wrap => (r, c) = (0, 0),
                    }
                }
            }
            Step::Previous => {
                if span.col > 0 {
                    c = span.col - 1;
                } else if r > 0 {
                    (r, c) = (r - 1, cols - 1);
                }
            }
        }
        // Onto a merged cell, at its first cell.
        let span = self.span(r, c);
        Landing::Cell(span.row, span.col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: u32, cols: u32) -> Table<String> {
        Table::new(rows, cols)
    }

    #[test]
    fn tab_07_a_selection_takes_in_whole_merged_cells() {
        let mut t = table(5, 5);
        // B2:C3 merged, and C4:D4, which a selection that takes in C3
        // must then reach.
        t.merge(Area::between((1, 1), (2, 2)), &crate::Locale::en_us())
            .unwrap();
        t.merge(Area::between((3, 2), (3, 3)), &crate::Locale::en_us())
            .unwrap();
        assert_eq!(
            t.selection_area((0, 0), (0, 3)),
            Area::between((0, 0), (0, 3))
        );
        assert_eq!(
            t.selection_area((0, 0), (1, 1)),
            Area::between((0, 0), (2, 2))
        );
        // From A4 to C3: the first merge reaches row 2 and column 2, then
        // the second takes in column 3.
        assert_eq!(
            t.selection_area((3, 0), (2, 2)),
            Area::between((1, 0), (3, 3))
        );
        // The anchor and the focus can be in any order.
        assert_eq!(
            t.selection_area((2, 2), (3, 0)),
            t.selection_area((3, 0), (2, 2))
        );
    }

    #[test]
    fn tab_08_arrows_stop_at_the_edges() {
        let t = table(3, 3);
        assert_eq!(t.step((0, 0), Step::Up), Landing::Cell(0, 0));
        assert_eq!(t.step((0, 0), Step::Left), Landing::Cell(0, 0));
        assert_eq!(t.step((2, 2), Step::Down), Landing::Cell(2, 2));
        assert_eq!(t.step((2, 2), Step::Right), Landing::Cell(2, 2));
        assert_eq!(t.step((1, 1), Step::Down), Landing::Cell(2, 1));
        assert_eq!(t.step((1, 1), Step::Right), Landing::Cell(1, 2));
    }

    #[test]
    fn tab_08_tab_goes_in_reading_order() {
        let t = table(2, 3);
        assert_eq!(t.step((0, 1), Step::Next), Landing::Cell(0, 2));
        assert_eq!(t.step((0, 2), Step::Next), Landing::Cell(1, 0));
        assert_eq!(t.step((1, 0), Step::Previous), Landing::Cell(0, 2));
        // Shift-Tab stops at the first cell.
        assert_eq!(t.step((0, 0), Step::Previous), Landing::Cell(0, 0));
    }

    #[test]
    fn tab_08_tab_in_the_last_cell_follows_the_option_of_the_table() {
        let mut t = table(2, 2);
        assert_eq!(t.step((1, 1), Step::Next), Landing::AddRow);
        t.options.last_cell_tab = LastCellTab::AddColumn;
        assert_eq!(t.step((1, 1), Step::Next), Landing::AddColumn);
        t.options.last_cell_tab = LastCellTab::Wrap;
        assert_eq!(t.step((1, 1), Step::Next), Landing::Cell(0, 0));
    }

    #[test]
    fn tab_08_moves_leave_and_enter_merged_cells_by_their_edges() {
        let mut t = table(4, 4);
        t.merge(Area::between((1, 1), (2, 2)), &crate::Locale::en_us())
            .unwrap();
        // Into the merged cell: at its first cell, from any side.
        assert_eq!(t.step((0, 2), Step::Down), Landing::Cell(1, 1));
        assert_eq!(t.step((2, 0), Step::Right), Landing::Cell(1, 1));
        // Out of it: from its far edge, not from the cell in focus.
        assert_eq!(t.step((1, 1), Step::Down), Landing::Cell(3, 1));
        assert_eq!(t.step((1, 1), Step::Right), Landing::Cell(1, 3));
        assert_eq!(t.step((1, 1), Step::Next), Landing::Cell(1, 3));
        assert_eq!(t.step((1, 3), Step::Previous), Landing::Cell(1, 1));
    }
}
