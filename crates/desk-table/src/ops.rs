// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the commands of tables do to them: sorting, filling, merging and
//! splitting cells, clearing, sizes and converting text.

use std::cmp::Ordering;

use desk_formula::{Formula, Value};

use crate::cell::{Cell, CellText, Content, Num};
use crate::table::{Area, Region, Table, TableError};

/// Which way cells are filled (TAB-19).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillDirection {
    /// To the right.
    Right,
    /// To the left.
    Left,
    /// Down.
    Down,
    /// Up.
    Up,
}

/// How a cell is split (TAB-24).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    /// Into two rows.
    Rows,
    /// Into two columns.
    Columns,
}

/// The order of values in a sort: numbers first, then text, then true
/// and false, and empty cells last (TAB-15).
fn sort_order(a: &Value, b: &Value) -> Ordering {
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Number(_) | Value::Date(_) | Value::Duration(_) => 0,
            Value::Text(_) => 1,
            Value::Bool(_) => 2,
            Value::Error(_) => 3,
            Value::Empty => 4,
        }
    }
    match (a, b) {
        (Value::Text(x), Value::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ if rank(a) == 0 && rank(b) == 0 => {
            let x = a.to_number().unwrap_or(0.0);
            let y = b.to_number().unwrap_or(0.0);
            x.partial_cmp(&y).unwrap_or(Ordering::Equal)
        }
        _ => rank(a).cmp(&rank(b)),
    }
}

/// The names of months and weekdays that a fill continues, in Portuguese
/// and English, in full and short.
const SERIES: &[&[&str]] = &[
    &[
        "janeiro",
        "fevereiro",
        "março",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ],
    &[
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ],
    &[
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ],
    &[
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ],
    &[
        "domingo",
        "segunda-feira",
        "terça-feira",
        "quarta-feira",
        "quinta-feira",
        "sexta-feira",
        "sábado",
    ],
    &["dom", "seg", "ter", "qua", "qui", "sex", "sáb"],
    &[
        "sunday",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
    ],
    &["sun", "mon", "tue", "wed", "thu", "fri", "sat"],
];

/// Where a name is in a series.
fn position_in(series: usize, text: &str) -> Option<usize> {
    let lower = text.trim().to_lowercase();
    SERIES.get(series)?.iter().position(|n| *n == lower)
}

/// The first series that holds every name, and where each is in it. Short
/// names are shared: "jan" is Portuguese and English, so "Jan", "Feb" go
/// by the English names, where the first series holding "jan" alone,
/// the Portuguese, had no "feb".
fn series_of(texts: &[String]) -> Option<(usize, Vec<usize>)> {
    (0..SERIES.len()).find_map(|s| {
        let positions: Option<Vec<usize>> = texts.iter().map(|t| position_in(s, t)).collect();
        positions.map(|p| (s, p))
    })
}

/// A name of a series in the case of the example: capitalized or not.
fn in_case_of(name: &str, example: &str) -> String {
    if example.chars().next().is_some_and(char::is_uppercase) {
        let mut c = name.chars();
        c.next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default()
    } else {
        name.to_owned()
    }
}

impl<T: CellText> Table<T> {
    /// Sorts the body rows of an area by the values of a column, in the
    /// order of TAB-15; `rows` none sorts the whole body. Header and
    /// footer rows stay. Formulas are sorted by their values at `now`, in
    /// days from 30 December 1899, for TODAY and NOW.
    pub fn sort(&mut self, col: u32, ascending: bool, rows: Option<(u32, u32)>, now: f64) {
        if col >= self.cols() {
            return;
        }
        let (start, end) = rows.map_or((self.body_start(), self.body_end()), |(a, b)| {
            (a.max(self.body_start()), b.min(self.body_end()))
        });
        if end <= start + 1 {
            return;
        }
        let values = self.values(now);
        let mut order: Vec<u32> = (start..end).collect();
        order.sort_by(|a, b| {
            let (x, y) = (values.get(*a, col), values.get(*b, col));
            let o = sort_order(x, y);
            // Empty cells stay last either way.
            if !ascending && !matches!(x, Value::Empty) && !matches!(y, Value::Empty) {
                o.reverse()
            } else {
                o
            }
        });
        let cols = self.cols();
        let old: Vec<Cell<T>> = self.cells_mut().clone();
        let mut cells = old.clone();
        let heights = self.heights_mut().clone();
        let mut new_heights = heights.clone();
        for (i, from) in order.iter().enumerate() {
            let to = start + i as u32;
            for c in 0..cols {
                let mut cell = old[(from * cols + c) as usize].clone();
                // A formula and the rules move with their row.
                cell.move_references(0, i64::from(to) - i64::from(*from));
                cells[(to * cols + c) as usize] = cell;
            }
            new_heights[to as usize] = heights[*from as usize];
        }
        self.replace_cells(cells);
        *self.heights_mut() = new_heights;
    }

    /// Fills cells from the selected ones in a direction, up to `count`
    /// rows or columns beyond them, as dragging the fill handle does
    /// (TAB-19): a sequence of numbers, dates, months or weekdays when the
    /// source has one, or the source repeated otherwise. Formulas move,
    /// and the format and fill come along; comments do not.
    pub fn fill(&mut self, source: Area, direction: FillDirection, count: u32) {
        let vertical = matches!(direction, FillDirection::Down | FillDirection::Up);
        let lines = if vertical { source.cols } else { source.rows };
        let len = if vertical { source.rows } else { source.cols };
        for line in 0..lines {
            // The cells of the source along the direction, in order.
            let src: Vec<(u32, u32)> = (0..len)
                .map(|k| {
                    if vertical {
                        (source.row + k, source.col + line)
                    } else {
                        (source.row + line, source.col + k)
                    }
                })
                .collect();
            let cells: Vec<Cell<T>> = src.iter().map(|(r, c)| self.cell(*r, *c).clone()).collect();
            let backward = matches!(direction, FillDirection::Up | FillDirection::Left);
            let step = sequence_step(&cells);
            for k in 1..=count {
                let (r, c) = match direction {
                    FillDirection::Down => (source.row_end() - 1 + k, source.col + line),
                    FillDirection::Up => match source.row.checked_sub(k) {
                        Some(r) => (r, source.col + line),
                        None => break,
                    },
                    FillDirection::Right => (source.row + line, source.col_end() - 1 + k),
                    FillDirection::Left => match source.col.checked_sub(k) {
                        Some(c) => (source.row + line, c),
                        None => break,
                    },
                };
                if r >= self.rows() || c >= self.cols() {
                    break;
                }
                let from_index = if backward {
                    (len - 1 - (k - 1) % len) as usize
                } else {
                    ((k - 1) % len) as usize
                };
                let from = &cells[from_index];
                let (fr, fc) = src[from_index];
                let mut cell = from.clone();
                cell.comment = None;
                let offset = |n: u32| if backward { -(n as f64) } else { n as f64 };
                cell.content = match (&step, &from.content) {
                    (Some(Step::Number(d)), Content::Number(_)) => {
                        let last = number_of(&cells[if backward { 0 } else { cells.len() - 1 }]);
                        Content::Number(Num(last + d * offset(k)))
                    }
                    (Some(Step::Date(d)), Content::Date(_)) => {
                        let last = number_of(&cells[if backward { 0 } else { cells.len() - 1 }]);
                        Content::Date(Num(last + d * offset(k)))
                    }
                    (Some(Step::Named(s, d)), Content::Text(t)) => {
                        let names = SERIES[*s];
                        let last_text =
                            match &cells[if backward { 0 } else { cells.len() - 1 }].content {
                                Content::Text(t) => t.plain(),
                                _ => String::new(),
                            };
                        let last = position_in(*s, &last_text).unwrap_or(0) as i64;
                        let n = names.len() as i64;
                        let pos = (last + d * offset(k) as i64).rem_euclid(n) as usize;
                        Content::Text(T::from_plain(&in_case_of(names[pos], &t.plain())))
                    }
                    (_, Content::Formula(f)) => match Formula::parse(f) {
                        Ok(parsed) => Content::Formula(
                            parsed
                                .moved(i64::from(c) - i64::from(fc), i64::from(r) - i64::from(fr))
                                .to_string(),
                        ),
                        Err(_) => from.content.clone(),
                    },
                    _ => from.content.clone(),
                };
                cell.rules = crate::conditional::map_rule_cells(&cell.rules, &mut |at| {
                    at.moved(i64::from(c) - i64::from(fc), i64::from(r) - i64::from(fr))
                });
                *self.cell_mut(r, c) = cell;
            }
        }
    }

    /// Merges the cells of an area into one (TAB-23). They must all be of
    /// one region. The texts are joined, with tabs across and line breaks
    /// down, as in Pages 15.4, and the merged cell takes the look of the
    /// top left cell. Numbers, dates and formulas joined with text are
    /// written as they are typed in `locale`.
    pub fn merge(&mut self, area: Area, locale: &crate::Locale) -> Result<(), TableError> {
        if area.row_end() > self.rows() || area.col_end() > self.cols() {
            return Err(TableError::OutOfRange);
        }
        if area.rows * area.cols < 2 {
            return Ok(());
        }
        let region = self.region(area.row, area.col);
        let region_of = |r, c| match self.region(r, c) {
            Region::HeaderColumn | Region::Body
                if region == Region::Body || region == Region::HeaderColumn =>
            {
                self.region(r, c) == region
            }
            other => other == region,
        };
        if !area.cells().all(|(r, c)| region_of(r, c)) {
            return Err(TableError::MixedRegions);
        }
        // Merged cells inside the area are taken in; ones that cross its
        // edge make it fail.
        let mut whole = area;
        for m in self.merges() {
            if m.overlaps(&area) {
                if !(area.contains(m.row, m.col) && area.contains(m.row_end() - 1, m.col_end() - 1))
                {
                    return Err(TableError::CutsMerge);
                }
                whole = whole.union(m);
            }
        }
        let mut row_texts = Vec::new();
        let mut first_value: Option<Content<T>> = None;
        for r in whole.row..whole.row_end() {
            let mut parts = Vec::new();
            for c in whole.col..whole.col_end() {
                match &self.cell(r, c).content {
                    Content::Empty => {}
                    Content::Text(t) => parts.push(t.clone()),
                    other => {
                        if first_value.is_none() {
                            first_value = Some(other.clone());
                        }
                        parts.push(T::from_plain(&self.source_text(r, c, locale)));
                    }
                }
            }
            if !parts.is_empty() {
                row_texts.push(T::join(&parts, true));
            }
        }
        let content = match (row_texts.len(), first_value) {
            (0, _) => Content::Empty,
            (1, Some(v)) if matches!(self.count_filled(whole), 1) => v,
            (1, _) => Content::Text(row_texts.remove(0)),
            _ => Content::Text(T::join(&row_texts, false)),
        };
        for (r, c) in whole.cells().collect::<Vec<_>>() {
            if (r, c) != (whole.row, whole.col) {
                let cleared = self.cell(r, c).cleared();
                *self.cell_mut(r, c) = Cell {
                    comment: None,
                    ..cleared
                };
            }
        }
        self.cell_mut(whole.row, whole.col).content = content;
        let merges = self.merges_mut();
        merges.retain(|m| !m.overlaps(&whole));
        merges.push(whole);
        Ok(())
    }

    fn count_filled(&self, area: Area) -> usize {
        area.cells()
            .filter(|(r, c)| !self.cell(*r, *c).content.is_empty())
            .count()
    }

    /// Splits merged cells back into their cells.
    pub fn unmerge(&mut self, row: u32, col: u32) {
        self.merges_mut().retain(|m| !m.contains(row, col));
    }

    /// Splits a cell into two rows or two columns of the same size
    /// (TAB-24). Merged cells of two or more rows or columns are split in
    /// the middle; a single cell gets a new row or column beside it, and
    /// the other cells of that row or column are merged across it so they
    /// stay as they were. The text stays in the first part.
    pub fn split(&mut self, row: u32, col: u32, how: Split) -> Result<(), TableError> {
        if row >= self.rows() || col >= self.cols() {
            return Err(TableError::OutOfRange);
        }
        let span = self.span(row, col);
        let len = match how {
            Split::Rows => span.rows,
            Split::Columns => span.cols,
        };
        if len >= 2 {
            let first = len.div_ceil(2);
            self.merges_mut().retain(|m| *m != span);
            let (a, b) = match how {
                Split::Rows => (
                    Area {
                        rows: first,
                        ..span
                    },
                    Area {
                        row: span.row + first,
                        rows: len - first,
                        ..span
                    },
                ),
                Split::Columns => (
                    Area {
                        cols: first,
                        ..span
                    },
                    Area {
                        col: span.col + first,
                        cols: len - first,
                        ..span
                    },
                ),
            };
            for part in [a, b] {
                if part.rows * part.cols > 1 {
                    self.merges_mut().push(part);
                }
            }
            return Ok(());
        }
        match how {
            Split::Rows => {
                let at = span.row_end();
                let h = self.height(span.row);
                self.insert_rows(at, 1)?;
                self.set_height(span.row, h / 2);
                self.set_height(at, h / 2);
                for c in 0..self.cols() {
                    if c >= span.col && c < span.col_end() {
                        continue;
                    }
                    // A cell beside the split one grows over the new row
                    // when it ends with it. One that went past it was
                    // grown by the insertion already.
                    let s = self.span(span.row, c);
                    if s.row_end() == at {
                        let grown = Area {
                            rows: s.rows + 1,
                            ..s
                        };
                        self.merges_mut().retain(|m| *m != s);
                        self.merges_mut().push(grown);
                    }
                }
                for c in span.col..span.col_end() {
                    let fresh = self.cell(at, c).cleared();
                    *self.cell_mut(at, c) = fresh;
                }
            }
            Split::Columns => {
                let at = span.col_end();
                let w = self.width(span.col);
                self.insert_cols(at, 1)?;
                self.set_width(span.col, w / 2);
                self.set_width(at, w / 2);
                for r in 0..self.rows() {
                    if r >= span.row && r < span.row_end() {
                        continue;
                    }
                    let s = self.span(r, span.col);
                    if s.col_end() == at {
                        let grown = Area {
                            cols: s.cols + 1,
                            ..s
                        };
                        self.merges_mut().retain(|m| *m != s);
                        self.merges_mut().push(grown);
                    }
                }
            }
        }
        Ok(())
    }

    /// Clears what an area holds: only the content, or with "Clear All"
    /// also the fill, format, rules and comments (TAB-16).
    pub fn clear(&mut self, area: Area, all: bool) {
        for (r, c) in area.cells().collect::<Vec<_>>() {
            if r >= self.rows() || c >= self.cols() {
                continue;
            }
            if all {
                // In the tracked change it was in, which keeps what it
                // was (REV-05).
                let tracked = self.cell(r, c).tracked.clone();
                *self.cell_mut(r, c) = Cell {
                    tracked,
                    ..Cell::default()
                };
            } else {
                self.cell_mut(r, c).content = Content::Empty;
            }
        }
    }

    /// Gives some rows the same height, their average, as "Distribute
    /// Rows Evenly" (TAB-13); the rows need not be next to each other.
    pub fn distribute_rows(&mut self, rows: &[u32]) {
        let valid: Vec<u32> = rows.iter().copied().filter(|r| *r < self.rows()).collect();
        if valid.is_empty() {
            return;
        }
        let total: u64 = valid.iter().map(|r| u64::from(self.height(*r))).sum();
        let each = (total / valid.len() as u64) as u32;
        for r in valid {
            self.set_height(r, each);
        }
    }

    /// Gives some columns the same width, their average (TAB-13).
    pub fn distribute_cols(&mut self, cols: &[u32]) {
        let valid: Vec<u32> = cols.iter().copied().filter(|c| *c < self.cols()).collect();
        if valid.is_empty() {
            return;
        }
        let total: u64 = valid.iter().map(|c| u64::from(self.width(*c))).sum();
        let each = (total / valid.len() as u64) as u32;
        for c in valid {
            self.set_width(c, each);
        }
    }

    /// Makes the table as wide as `width`, keeping the proportions of the
    /// columns, as dragging its handle does (TAB-03).
    pub fn scale_width(&mut self, width: u32) {
        let total = u64::from(self.total_width().max(1));
        let widths = self.widths_mut();
        for w in widths.iter_mut() {
            *w = ((u64::from(*w) * u64::from(width) / total) as u32).max(6_000);
        }
    }

    /// The table as text, as "Convert Table to Text" makes it: a tab
    /// between cells and a line between rows (TAB-05), with formulas as
    /// their values at `now`.
    pub fn to_text(&self, locale: &crate::Locale, now: f64) -> String {
        let values = self.values(now);
        let mut lines = Vec::new();
        for r in 0..self.rows() {
            let row: Vec<String> = (0..self.cols())
                .map(|c| match &self.cell(r, c).content {
                    Content::Text(t) => t.plain().replace(['\n', '\t'], " "),
                    _ => self.shown(&values, r, c, locale).text,
                })
                .collect();
            lines.push(row.join("\t"));
        }
        lines.join("\n")
    }

    /// A table from text, as "Convert Text to Table" makes it: a row for
    /// each line and a column for each tab (TAB-05).
    pub fn from_text(text: &str, locale: &crate::Locale) -> Self {
        let rows: Vec<Vec<String>> = text
            .lines()
            .map(|l| l.split('\t').map(str::to_owned).collect())
            .collect();
        Self::from_rows(&rows, locale)
    }
}

enum Step {
    Number(f64),
    Date(f64),
    Named(usize, i64),
}

fn number_of<T>(cell: &Cell<T>) -> f64 {
    match &cell.content {
        Content::Number(n) | Content::Date(n) | Content::Duration(n) => n.0,
        _ => 0.0,
    }
}

/// The step of the sequence that cells start: the same step between
/// numbers, dates or names of months and weekdays; one cell with a month
/// or weekday steps by one, and one number repeats (TAB-19).
fn sequence_step<T: CellText>(cells: &[Cell<T>]) -> Option<Step> {
    if cells
        .iter()
        .all(|c| matches!(c.content, Content::Number(_)))
        && cells.len() >= 2
    {
        let v: Vec<f64> = cells.iter().map(number_of).collect();
        let d = v[1] - v[0];
        if v.windows(2).all(|w| ((w[1] - w[0]) - d).abs() < 1e-9) {
            return Some(Step::Number(d));
        }
        return None;
    }
    if cells.iter().all(|c| matches!(c.content, Content::Date(_))) {
        if cells.len() == 1 {
            return Some(Step::Date(1.0));
        }
        let v: Vec<f64> = cells.iter().map(number_of).collect();
        let d = v[1] - v[0];
        if v.windows(2).all(|w| ((w[1] - w[0]) - d).abs() < 1e-9) {
            return Some(Step::Date(d));
        }
        return None;
    }
    let texts: Option<Vec<String>> = cells
        .iter()
        .map(|c| match &c.content {
            Content::Text(t) => Some(t.plain()),
            _ => None,
        })
        .collect();
    let (series, positions) = series_of(&texts?)?;
    if positions.len() == 1 {
        return Some(Step::Named(series, 1));
    }
    let n = SERIES.get(series)?.len() as i64;
    let d = (*positions.get(1)? as i64 - *positions.first()? as i64).rem_euclid(n);
    if positions
        .windows(2)
        .all(|w| (w[1] as i64 - w[0] as i64).rem_euclid(n) == d)
    {
        Some(Step::Named(series, d))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Locale;

    fn table(rows: &[&[&str]]) -> Table<String> {
        let rows: Vec<Vec<String>> = rows
            .iter()
            .map(|r| r.iter().map(|s| (*s).to_owned()).collect())
            .collect();
        Table::from_rows(&rows, &Locale::en_us())
    }

    fn texts(t: &Table<String>, col: u32) -> Vec<String> {
        (0..t.rows())
            .map(|r| t.source_text(r, col, &Locale::en_us()))
            .collect()
    }

    #[test]
    fn tab_15_sort_puts_numbers_text_booleans_then_empty() {
        let mut t = table(&[&["Nome"], &["b"], &[""], &["TRUE"], &["10"], &["a"], &["2"]]);
        t.set_header_rows(1).unwrap();
        t.sort(0, true, None, 0.0);
        assert_eq!(texts(&t, 0), ["Nome", "2", "10", "a", "b", "TRUE", ""]);
        t.sort(0, false, None, 0.0);
        assert_eq!(texts(&t, 0), ["Nome", "TRUE", "b", "a", "10", "2", ""]);
    }

    #[test]
    fn sort_takes_today_as_the_day_it_is() {
        let mut t = table(&[&["Dia"], &["=TODAY()"], &["1000"]]);
        t.set_header_rows(1).unwrap();
        // On day 45,000 TODAY is larger than 1,000, not 0.
        t.sort(0, true, None, 45_000.0);
        assert_eq!(texts(&t, 0), ["Dia", "1000", "=TODAY()"]);
    }

    #[test]
    fn tab_19_fill_continues_sequences() {
        let mut t = table(&[
            &["1", "jan", "=A1*2", "x", "Segunda-feira"],
            &["3", "", "", "", ""],
            &["", "", "", "", ""],
            &["", "", "", "", ""],
        ]);
        t.fill(Area::between((0, 0), (1, 0)), FillDirection::Down, 2);
        assert_eq!(texts(&t, 0), ["1", "3", "5", "7"]);
        t.fill(Area::cell(0, 1), FillDirection::Down, 3);
        assert_eq!(texts(&t, 1), ["jan", "fev", "mar", "abr"]);
        t.fill(Area::cell(0, 2), FillDirection::Down, 2);
        assert_eq!(texts(&t, 2), ["=A1*2", "=A2*2", "=A3*2", ""]);
        t.fill(Area::cell(0, 3), FillDirection::Down, 1);
        assert_eq!(texts(&t, 3), ["x", "x", "", ""]);
        t.fill(Area::cell(0, 4), FillDirection::Down, 1);
        assert_eq!(texts(&t, 4)[1], "Terça-feira");
        // English short months go on in English: "jan" is Portuguese too,
        // and the first series holding it has no "feb" (review RV,
        // batch 2).
        let mut t = table(&[&["Jan"], &["Feb"], &[""], &[""]]);
        t.fill(Area::between((0, 0), (1, 0)), FillDirection::Down, 2);
        assert_eq!(texts(&t, 0), ["Jan", "Feb", "Mar", "Apr"]);
    }

    #[test]
    fn tab_23_merge_joins_text_of_one_region() {
        let mut t = table(&[&["a", "b"], &["c", ""], &["", ""]]);
        t.set_header_rows(1).unwrap();
        assert_eq!(
            t.merge(Area::between((0, 0), (1, 1)), &Locale::en_us()),
            Err(TableError::MixedRegions)
        );
        t.merge(Area::between((1, 0), (2, 1)), &Locale::en_us())
            .unwrap();
        assert_eq!(t.span(2, 1), Area::between((1, 0), (2, 1)));
        assert!(t.is_covered(2, 1));
        assert_eq!(t.source_text(1, 0, &Locale::en_us()), "c");
        let mut u = table(&[&["a", "b"], &["c", "d"]]);
        u.merge(Area::between((0, 0), (1, 1)), &Locale::en_us())
            .unwrap();
        assert_eq!(u.source_text(0, 0, &Locale::en_us()), "a\tb\u{2028}c\td");
    }

    #[test]
    fn merge_writes_numbers_as_they_are_typed_in_the_locale() {
        let pt = Locale::pt_br();
        let mut t: Table<String> = Table::new(2, 2);
        t.set_header_rows(0).unwrap();
        t.set_header_cols(0).unwrap();
        t.type_text(0, 0, "Preço", &pt);
        t.type_text(0, 1, "2,5", &pt);
        t.merge(Area::between((0, 0), (0, 1)), &pt).unwrap();
        assert_eq!(t.source_text(0, 0, &pt), "Preço\t2,5");
    }

    #[test]
    fn tab_24_split_cells() {
        let mut t = table(&[&["a", "b"], &["c", "d"]]);
        t.split(0, 0, Split::Columns).unwrap();
        assert_eq!(t.cols(), 3);
        assert_eq!(t.source_text(0, 0, &Locale::en_us()), "a");
        assert_eq!(t.span(1, 0), Area::between((1, 0), (1, 1)));
        assert_eq!(t.width(0), 36_000);
        t.split(1, 0, Split::Columns).unwrap();
        assert_eq!(t.span(1, 0), Area::cell(1, 0));
        t.split(0, 2, Split::Rows).unwrap();
        assert_eq!(t.rows(), 3);
        assert_eq!(t.span(0, 0), Area::between((0, 0), (1, 0)));
    }

    #[test]
    fn splitting_a_cell_keeps_the_merged_cells_beside_it() {
        // Splitting one cell broke the merges beside it: one that crossed
        // the split row shrank, one that ended on it did not grow
        // (review RV, batch 2).
        let rows: Vec<Vec<&str>> = vec![vec!["", "", ""]; 5];
        let rows: Vec<&[&str]> = rows.iter().map(Vec::as_slice).collect();
        let mut t = table(&rows);
        t.merge(Area::between((1, 2), (3, 2)), &Locale::en_us())
            .unwrap();
        t.split(1, 1, Split::Rows).unwrap();
        assert_eq!(t.span(1, 2), Area::between((1, 2), (4, 2)));
        assert_eq!(t.span(1, 0), Area::between((1, 0), (2, 0)));
        let mut t = table(&rows);
        t.merge(Area::between((1, 2), (2, 2)), &Locale::en_us())
            .unwrap();
        t.split(2, 1, Split::Rows).unwrap();
        assert_eq!(t.span(1, 2), Area::between((1, 2), (3, 2)));
        let mut t = table(&rows);
        t.merge(Area::between((0, 0), (0, 1)), &Locale::en_us())
            .unwrap();
        t.split(0, 2, Split::Columns).unwrap();
        assert_eq!(t.span(0, 0), Area::between((0, 0), (0, 1)));
        assert_eq!(t.span(1, 2), Area::between((1, 2), (1, 3)));
    }

    #[test]
    fn tab_05_text_and_table() {
        let en = Locale::en_us();
        let t: Table<String> = Table::from_text("a\tb\n1\t2", &en);
        assert_eq!((t.rows(), t.cols()), (2, 2));
        assert_eq!(t.to_text(&en, 0.0), "a\tb\n1\t2");
    }

    #[test]
    fn tab_13_tab_16_sizes_and_clearing() {
        let mut t = table(&[&["1", "2"], &["3", "4"]]);
        t.set_height(0, 10_000);
        t.set_height(1, 30_000);
        t.distribute_rows(&[0, 1]);
        assert_eq!((t.height(0), t.height(1)), (20_000, 20_000));
        t.cell_mut(0, 0).comment = std::num::NonZeroU64::new(7);
        t.clear(Area::cell(0, 0), false);
        assert!(t.cell(0, 0).comment.is_some());
        t.clear(Area::cell(0, 0), true);
        assert!(t.cell(0, 0).comment.is_none());
        t.scale_width(288_000);
        assert_eq!(t.total_width(), 288_000);
        // Widths from a file past the largest sum stop at it.
        t.set_width(0, u32::MAX);
        t.set_width(1, u32::MAX);
        assert_eq!(t.total_width(), u32::MAX);
    }
}
