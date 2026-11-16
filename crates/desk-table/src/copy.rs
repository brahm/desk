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

use crate::{Area, CellText, Locale, Table, Values};

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
        let lines: Vec<&str> = text.trim_end_matches(['\n', '\r']).split('\n').collect();
        let mut width = 1;
        for (i, line) in lines.iter().enumerate() {
            let r = r0 + i as u32;
            if r >= self.rows() {
                let _ = self.insert_rows(self.rows(), 1);
            }
            let values: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
            width = width.max(values.len() as u32);
            for (j, value) in values.iter().enumerate() {
                let c = c0 + j as u32;
                if c >= self.cols() {
                    let _ = self.insert_cols(self.cols(), 1);
                }
                // A table at its largest leaves the rest out.
                if r < self.rows() && c < self.cols() {
                    self.type_text(r, c, value, locale);
                }
            }
        }
        Area::between(
            (r0, c0),
            (
                (r0 + lines.len() as u32 - 1).min(self.rows() - 1),
                (c0 + width - 1).min(self.cols() - 1),
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
