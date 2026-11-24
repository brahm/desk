// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cells dragged (TAB-26, PXV-17): moved or copied to another place of
//! the same table, into another table, or out of the table onto the page,
//! where they make a new table of their own.
//!
//! The cells take their content, their style and their comment along; the
//! cells they land on are replaced. A formula moved within its table keeps
//! pointing at the same cells; a formula copied, or taken to another
//! table, moves its relative references along, as a copy does (TAB-19).
//! Merged cells stay as they were in the source and are not made at the
//! destination.

use crate::cell::{Cell, CellText, Content};
use crate::table::{Area, Table};

/// A cell taken `rows` rows and `cols` columns away, with the relative
/// references of its formula moved along.
fn carried<T: CellText>(cell: &Cell<T>, rows: i64, cols: i64) -> Cell<T> {
    let mut cell = cell.clone();
    cell.move_references(cols, rows);
    cell
}

impl<T: CellText> Table<T> {
    /// A new table with only the cells of `area`, in the style of this
    /// one, with the widths of their columns, the heights of their rows
    /// and their lines, and the headers of this table that fall in it
    /// (PXV-17).
    pub fn sub_table(&self, area: Area) -> Self {
        let area = self.clamped(area);
        let mut out = Self::new(area.rows, area.cols);
        out.style = self.style.clone();
        out.options = self.options.clone();
        for (r, c) in area.cells() {
            let (nr, nc) = (r - area.row, c - area.col);
            *out.cell_mut(nr, nc) =
                carried(self.cell(r, c), -i64::from(area.row), -i64::from(area.col));
            out.set_h_border(nr, nc, self.h_border(r, c));
            out.set_v_border(nr, nc, self.v_border(r, c));
            if r + 1 == area.row_end() {
                out.set_h_border(nr + 1, nc, self.h_border(r + 1, c));
            }
            if c + 1 == area.col_end() {
                out.set_v_border(nr, nc + 1, self.v_border(r, c + 1));
            }
        }
        for c in 0..area.cols {
            out.set_width(c, self.width(area.col + c));
        }
        for r in 0..area.rows {
            out.set_height(r, self.height(area.row + r));
        }
        let headers = |start: u32, count: u32, header: u32| header.saturating_sub(start).min(count);
        let _ = out.set_header_rows(headers(area.row, area.rows, self.header_rows()));
        let _ = out.set_header_cols(headers(area.col, area.cols, self.header_cols()));
        let footer_start = self.rows() - self.footer_rows();
        let footers = area.row_end().saturating_sub(footer_start.max(area.row));
        let _ = out.set_footer_rows(footers.min(area.rows - out.header_rows()));
        out
    }

    /// Puts the cells of `area` of `from` into this table from `at`,
    /// adding rows and columns at the end where they do not reach, with
    /// the relative references of formulas moved along. The cells they
    /// landed on, which become the selection.
    pub fn put_cells(&mut self, at: (u32, u32), from: &Self, area: Area) -> Area {
        let area = from.clamped(area);
        let need_rows = at.0 + area.rows;
        let need_cols = at.1 + area.cols;
        self.add_rows_to_reach(at.0, need_rows);
        if need_cols > self.cols() {
            let _ = self.insert_cols(self.cols(), need_cols - self.cols());
        }
        let dr = i64::from(at.0) - i64::from(area.row);
        let dc = i64::from(at.1) - i64::from(area.col);
        for (r, c) in area.cells() {
            let (tr, tc) = ((i64::from(r) + dr) as u32, (i64::from(c) + dc) as u32);
            // A table at its largest leaves the rest out.
            if tr < self.rows() && tc < self.cols() && !self.is_covered(tr, tc) {
                *self.cell_mut(tr, tc) = carried(from.cell(r, c), dr, dc);
            }
        }
        Area::between(
            at,
            (
                (need_rows - 1).min(self.rows() - 1),
                (need_cols - 1).min(self.cols() - 1),
            ),
        )
    }

    /// Moves the cells of `area` so that its first cell goes to `to`, in
    /// this table, or copies them when `copying`: the cells they land on
    /// are replaced, and the cells they leave are emptied. Formulas that
    /// move keep pointing at the same cells. The cells they landed on.
    pub fn move_cells(&mut self, area: Area, to: (u32, u32), copying: bool) -> Area {
        let area = self.clamped(area);
        let source = self.clone();
        if !copying {
            for (r, c) in area.cells() {
                if !self.is_covered(r, c) {
                    *self.cell_mut(r, c) = Cell::default();
                }
            }
        }
        let landed = self.put_cells(to, &source, area);
        if !copying {
            // Moved formulas point at the same cells, as before.
            let (dr, dc) = (
                i64::from(to.0) - i64::from(area.row),
                i64::from(to.1) - i64::from(area.col),
            );
            for (r, c) in area.cells() {
                let (tr, tc) = ((i64::from(r) + dr) as u32, (i64::from(c) + dc) as u32);
                if tr < self.rows()
                    && tc < self.cols()
                    && let Content::Formula(f) = &source.cell(r, c).content
                    && !self.is_covered(tr, tc)
                {
                    self.cell_mut(tr, tc).content = Content::Formula(f.clone());
                }
            }
        }
        landed
    }

    /// Empties the cells of `area`, as the cells left by a drag to
    /// another table.
    pub fn clear_cells(&mut self, area: Area) {
        let area = self.clamped(area);
        for (r, c) in area.cells() {
            if !self.is_covered(r, c) {
                *self.cell_mut(r, c) = Cell::default();
            }
        }
    }

    /// The part of `area` inside the table.
    fn clamped(&self, area: Area) -> Area {
        let row = area.row.min(self.rows() - 1);
        let col = area.col.min(self.cols() - 1);
        Area {
            row,
            col,
            rows: area.rows.clamp(1, self.rows() - row),
            cols: area.cols.clamp(1, self.cols() - col),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Locale, Num};

    fn table() -> Table<String> {
        let pt = Locale::pt_br();
        let mut t: Table<String> = Table::new(4, 3);
        t.type_text(0, 1, "Vendas", &pt);
        t.type_text(1, 0, "Norte", &pt);
        t.type_text(1, 1, "10", &pt);
        t.type_text(2, 0, "Sul", &pt);
        t.type_text(2, 1, "20", &pt);
        t.type_text(3, 1, "=B2+B3", &pt);
        t
    }

    #[test]
    fn tab_26_dragging_moves_cells_and_replaces_where_they_land() {
        let mut t = table();
        let landed = t.move_cells(Area::between((1, 1), (2, 1)), (1, 2), false);
        assert_eq!(landed, Area::between((1, 2), (2, 2)));
        assert_eq!(t.cell(1, 2).content, Content::Number(Num(10.0)));
        assert_eq!(t.cell(2, 2).content, Content::Number(Num(20.0)));
        assert!(t.cell(1, 1).content.is_empty() && t.cell(2, 1).content.is_empty());
        // A formula moved keeps pointing at the same cells.
        let landed = t.move_cells(Area::between((3, 1), (3, 1)), (3, 0), false);
        assert_eq!(landed, Area::between((3, 0), (3, 0)));
        assert_eq!(t.cell(3, 0).content, Content::Formula("=B2+B3".into()));
    }

    #[test]
    fn tab_26_alt_dragging_copies_cells() {
        let mut t = table();
        t.move_cells(Area::between((1, 1), (3, 1)), (1, 2), true);
        assert_eq!(t.cell(1, 1).content, Content::Number(Num(10.0)));
        assert_eq!(t.cell(1, 2).content, Content::Number(Num(10.0)));
        // A copied formula moves its references along.
        assert_eq!(t.cell(3, 2).content, Content::Formula("=C2+C3".into()));
    }

    #[test]
    fn tab_26_cells_dragged_to_another_table_make_room_there() {
        let from = table();
        let mut to: Table<String> = Table::new(2, 2);
        let landed = to.put_cells((1, 1), &from, Area::between((1, 0), (2, 1)));
        assert_eq!(landed, Area::between((1, 1), (2, 2)));
        assert_eq!((to.rows(), to.cols()), (3, 3));
        assert_eq!(to.cell(2, 1).content, Content::Text("Sul".into()));
    }

    #[test]
    fn cells_dragged_past_the_body_go_before_the_footer() {
        let from = table();
        let mut to: Table<String> = Table::new(3, 2);
        to.type_text(2, 1, "Total", &Locale::pt_br());
        to.set_footer_rows(1).unwrap();
        // Three rows from the one row of the body: the table grows.
        to.put_cells((1, 0), &from, Area::between((1, 0), (3, 1)));
        assert_eq!((to.rows(), to.footer_rows()), (5, 1));
        assert_eq!(to.cell(2, 0).content, Content::Text("Sul".into()));
        assert_eq!(to.cell(4, 1).content, Content::Text("Total".into()));
    }

    #[test]
    fn pxv_17_dragged_cells_make_a_table_of_their_own() {
        let mut from = table();
        from.style.alternating = Some(crate::Color::rgb(1, 2, 3));
        from.set_width(1, 50_000);
        let t = from.sub_table(Area::between((0, 1), (2, 1)));
        assert_eq!((t.rows(), t.cols()), (3, 1));
        assert_eq!(t.cell(0, 0).content, Content::Text("Vendas".into()));
        assert_eq!(t.cell(2, 0).content, Content::Number(Num(20.0)));
        assert_eq!(t.width(0), 50_000);
        assert_eq!(t.style, from.style);
        // The header row of the source is the header of the new table;
        // the header column was left behind.
        assert_eq!((t.header_rows(), t.header_cols()), (1, 0));
    }
}
