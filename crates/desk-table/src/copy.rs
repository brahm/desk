// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cells copied and pasted as text (matrix row IOP-19).
//!
//! Copied cells go to the clipboard as lines of values separated by tabs,
//! the form that spreadsheets and other tables read; each value is the
//! text the cell shows, in its format. Text of that form pasted into
//! selected cells replaces them from the cell in focus, as iWork does with
//! cells copied from Numbers, and the table grows when the text is larger
//! than the room left. A format of the suite that also keeps formulas and
//! styles, and HTML for other programs, are left for phase X1.

use crate::{Area, CellText, Locale, MAX_CELLS, Table, Values};

/// The cells a table may grow to when text is pasted into it.
const MAX_PASTED_CELLS: u64 = 1 << 20;

impl<T: CellText> Table<T> {
    /// The cells of `area` as text: a line per row, the values of a row
    /// separated by tabs, each as the cell shows it.
    pub fn area_text(&self, area: Area, values: &Values, locale: &Locale) -> String {
        let mut out = String::new();
        for r in area.row..area.row_end() {
            let row: Vec<String> = (area.col..area.col_end())
                .map(|c| self.shown(values, r, c, locale).text)
                .collect();
            out.push_str(&row.join("\t"));
            out.push('\n');
        }
        out
    }

    /// Types `text`, lines of values separated by tabs, into the cells
    /// from `at`, adding rows and columns at the end of the table where it
    /// does not reach. The cells that the text filled, which become the
    /// selection.
    ///
    /// Each value is typed as if the user typed it, so numbers, dates and
    /// formulas are read by `locale`. Line ends of other systems (`\r\n`)
    /// are taken as well.
    pub fn paste_text(&mut self, at: (u32, u32), text: &str, locale: &Locale) -> Area {
        let (r0, c0) = at;
        let lines: Vec<Vec<&str>> = text
            .trim_end_matches(['\n', '\r'])
            .split('\n')
            .map(|line| line.trim_end_matches('\r').split('\t').collect())
            .collect();
        let width = lines.iter().map(Vec::len).max().unwrap_or(1).max(1) as u32;
        // The rows and columns are added at once: one at a time, each moved
        // the whole table, and a long text pasted took minutes. A table
        // grows to a million cells at most, so that a short text of many
        // lines and tabs cannot ask for billions.
        let (rows, cols) = (self.rows(), self.cols());
        let most = MAX_PASTED_CELLS.max(u64::from(rows) * u64::from(cols));
        let mut new_rows = r0.saturating_add(lines.len() as u32).clamp(rows, MAX_CELLS);
        let mut new_cols = c0.saturating_add(width).clamp(cols, MAX_CELLS);
        if u64::from(new_rows) * u64::from(new_cols) > most {
            let fit = |n: u32| u32::try_from(most / u64::from(n)).unwrap_or(MAX_CELLS);
            new_cols = fit(new_rows).clamp(cols, new_cols);
            new_rows = fit(new_cols).clamp(rows, new_rows);
        }
        self.add_rows_to_reach(r0, new_rows);
        if new_cols > cols {
            let _ = self.insert_cols(cols, new_cols - cols);
        }
        for (i, values) in lines.iter().enumerate() {
            let r = r0.saturating_add(i as u32);
            for (j, value) in values.iter().enumerate() {
                let c = c0.saturating_add(j as u32);
                // A table at its largest leaves the rest out.
                if r < self.rows() && c < self.cols() {
                    self.type_text(r, c, value, locale);
                }
            }
        }
        Area::between(
            (r0, c0),
            (
                (r0.saturating_add(lines.len() as u32) - 1).min(self.rows() - 1),
                (c0.saturating_add(width) - 1).min(self.cols() - 1),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Content, Num};

    #[test]
    fn iop_19_cells_are_copied_as_the_values_they_show() {
        let pt = Locale::pt_br();
        let mut t: Table<String> = Table::new(3, 3);
        t.type_text(1, 1, "10", &pt);
        t.type_text(1, 2, "2,5", &pt);
        t.type_text(2, 1, "=B2*2", &pt);
        t.type_text(2, 2, "Sul", &pt);
        let values = t.values(0.0);
        let text = t.area_text(Area::between((1, 1), (2, 2)), &values, &pt);
        // The formula goes as its value, the number in the locale.
        assert_eq!(text, "10\t2,5\n20\tSul\n");
    }

    #[test]
    fn iop_19_pasted_text_replaces_cells_from_the_focus_and_grows_the_table() {
        let pt = Locale::pt_br();
        let mut t: Table<String> = Table::new(3, 3);
        t.type_text(1, 1, "velho", &pt);
        let area = t.paste_text((1, 1), "1\t2\t3\r\n4\t5\t6\r\n7\r\n", &pt);
        assert_eq!(area, Area::between((1, 1), (3, 3)));
        assert_eq!((t.rows(), t.cols()), (4, 4));
        assert_eq!(t.cell(1, 1).content, Content::Number(Num(1.0)));
        assert_eq!(t.cell(2, 3).content, Content::Number(Num(6.0)));
        assert_eq!(t.cell(3, 1).content, Content::Number(Num(7.0)));
        // A short line leaves the cells after it as they were.
        assert_eq!(t.cell(3, 2).content, Content::Empty);
    }

    #[test]
    fn a_short_text_of_many_lines_and_tabs_grows_the_table_to_a_million_cells() {
        let pt = Locale::pt_br();
        let mut t: Table<String> = Table::new(2, 2);
        // 60,000 tabs on the first line and 60,000 lines.
        let text = format!("{}{}", "x\t".repeat(60_000), "\ny".repeat(60_000));
        let area = t.paste_text((0, 0), &text, &pt);
        assert!(u64::from(t.rows()) * u64::from(t.cols()) <= MAX_PASTED_CELLS);
        assert_eq!((t.rows(), t.cols()), (60_001, 17));
        assert_eq!(area, Area::between((0, 0), (t.rows() - 1, t.cols() - 1)));
        assert_eq!(t.cell(1, 0).content, Content::Text("y".into()));
    }

    #[test]
    fn text_pasted_past_the_body_goes_before_the_footer() {
        let pt = Locale::pt_br();
        let mut t: Table<String> = Table::new(4, 2);
        t.type_text(3, 1, "=SUM(B2:B3)", &pt);
        t.set_footer_rows(1).unwrap();
        let area = t.paste_text((2, 1), "1\n2\n3\n", &pt);
        assert_eq!(area, Area::between((2, 1), (4, 1)));
        assert_eq!(t.footer_rows(), 1);
        assert_eq!(t.rows(), 6);
        assert_eq!(t.cell(4, 1).content, Content::Number(Num(3.0)));
        // The totals stay the footer, not written over.
        assert_eq!(t.cell(5, 1).content, Content::Formula("=SUM(B2:B3)".into()));
    }

    #[test]
    fn iop_19_what_is_copied_pastes_back_the_same() {
        let pt = Locale::pt_br();
        let mut from: Table<String> = Table::new(3, 2);
        from.type_text(1, 0, "Norte", &pt);
        from.type_text(1, 1, "12,5", &pt);
        from.type_text(2, 0, "Sul", &pt);
        from.type_text(2, 1, "7", &pt);
        let area = Area::between((1, 0), (2, 1));
        let text = from.area_text(area, &from.values(0.0), &pt);
        let mut to: Table<String> = Table::new(3, 2);
        to.paste_text((1, 0), &text, &pt);
        for (r, c) in area.cells() {
            assert_eq!(to.cell(r, c).content, from.cell(r, c).content, "{r} {c}");
        }
    }
}
