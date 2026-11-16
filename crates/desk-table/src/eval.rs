// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The values of the cells, with the formulas computed (TAB-32).

use std::cell::RefCell;
use std::collections::HashMap;

use desk_formula::{CellRef, Context, ErrorKind, Formula, Value};

use crate::cell::{CellText, Content};
use crate::conditional::{self, RuleStyle};
use crate::format::{Locale, Shown};
use crate::table::Table;

/// The values of the cells of a table, computed once.
#[derive(Clone, Debug, PartialEq)]
pub struct Values {
    cols: u32,
    values: Vec<Value>,
    now: f64,
}

impl Values {
    /// The value of a cell; empty outside the table.
    pub fn get(&self, row: u32, col: u32) -> &Value {
        static EMPTY: Value = Value::Empty;
        if col >= self.cols {
            return &EMPTY;
        }
        self.values
            .get((row * self.cols + col) as usize)
            .unwrap_or(&EMPTY)
    }
}

struct Evaluator<'a, T> {
    table: &'a Table<T>,
    cache: RefCell<HashMap<(u32, u32), Value>>,
    visiting: RefCell<Vec<(u32, u32)>>,
    now: f64,
}

impl<T: CellText> Evaluator<'_, T> {
    fn value(&self, row: u32, col: u32) -> Value {
        if row >= self.table.rows() || col >= self.table.cols() {
            return Value::Empty;
        }
        if let Some(v) = self.cache.borrow().get(&(row, col)) {
            return v.clone();
        }
        let cell = self.table.cell(row, col);
        let v = match &cell.content {
            Content::Empty => Value::Empty,
            Content::Text(t) => Value::Text(t.plain()),
            Content::Number(n) => Value::Number(n.0),
            Content::Bool(b) => Value::Bool(*b),
            Content::Date(d) => Value::Date(d.0),
            Content::Duration(d) => Value::Duration(d.0),
            Content::Formula(text) => {
                if self.visiting.borrow().contains(&(row, col)) {
                    return Value::Error(ErrorKind::Circular);
                }
                self.visiting.borrow_mut().push((row, col));
                let v = match Formula::parse(text) {
                    Ok(f) => desk_formula::evaluate(&f, self),
                    Err(_) => Value::Error(ErrorKind::Syntax),
                };
                self.visiting.borrow_mut().pop();
                v
            }
        };
        self.cache.borrow_mut().insert((row, col), v.clone());
        v
    }
}

impl<T: CellText> Context for Evaluator<'_, T> {
    fn cell(&self, at: CellRef) -> Value {
        self.value(at.row, at.col)
    }

    fn header(&self, name: &str) -> Option<(CellRef, CellRef)> {
        header_range(self.table, name)
    }

    fn now(&self) -> f64 {
        self.now
    }
}

/// The body cells that the text of a header names: the column under a
/// header of a header row, or the row beside a header of a header column
/// (TAB-12).
pub fn header_range<T: CellText>(table: &Table<T>, name: &str) -> Option<(CellRef, CellRef)> {
    let wanted = name.trim().to_lowercase();
    let text = |r: u32, c: u32| match &table.cell(r, c).content {
        Content::Text(t) => Some(t.plain().trim().to_lowercase()),
        Content::Number(n) => Some(desk_formula::Value::Number(n.0).to_text().ok()?),
        _ => None,
    };
    let (start, end) = (table.body_start(), table.body_end());
    for r in 0..table.header_rows() {
        for c in 0..table.cols() {
            if text(r, c).as_deref() == Some(wanted.as_str()) && end > start {
                return Some((CellRef::new(c, start), CellRef::new(c, end - 1)));
            }
        }
    }
    let first_col = table.header_cols();
    for r in start..end {
        for c in 0..table.header_cols() {
            if text(r, c).as_deref() == Some(wanted.as_str()) && table.cols() > first_col {
                return Some((
                    CellRef::new(first_col, r),
                    CellRef::new(table.cols() - 1, r),
                ));
            }
        }
    }
    None
}

impl<T: CellText> Table<T> {
    /// The values of all the cells, with the formulas computed; `now` is
    /// the date and time for TODAY and NOW, in days from 30 December 1899.
    pub fn values(&self, now: f64) -> Values {
        let eval = Evaluator {
            table: self,
            cache: RefCell::new(HashMap::new()),
            visiting: RefCell::new(Vec::new()),
            now,
        };
        let mut values = Vec::with_capacity((self.rows() * self.cols()) as usize);
        for r in 0..self.rows() {
            for c in 0..self.cols() {
                values.push(eval.value(r, c));
            }
        }
        Values {
            cols: self.cols(),
            values,
            now,
        }
    }

    /// A cell as it shows: its value in its format.
    pub fn shown(&self, values: &Values, row: u32, col: u32, locale: &Locale) -> Shown {
        self.cell(row, col)
            .format
            .show(values.get(row, col), locale)
    }

    /// What the rules of conditional formatting of a cell change in it
    /// (TAB-21).
    pub fn rule_style(&self, values: &Values, row: u32, col: u32) -> RuleStyle {
        let cell = self.cell(row, col);
        if cell.rules.is_empty() {
            return RuleStyle::default();
        }
        conditional::apply(
            &cell.rules,
            values.get(row, col),
            &|r: CellRef| values.get(r.row, r.col).clone(),
            values.now,
        )
    }

    /// The cells whose rules of conditional formatting are the same as
    /// those of a cell: "Select all cells with the same rules" (TAB-21).
    pub fn same_rules(&self, row: u32, col: u32) -> Vec<(u32, u32)> {
        let rules = &self.cell(row, col).rules;
        let mut out = Vec::new();
        for r in 0..self.rows() {
            for c in 0..self.cols() {
                if &self.cell(r, c).rules == rules {
                    out.push((r, c));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Locale;

    #[test]
    fn tab_32_formulas_compute_and_cycles_are_errors() {
        let en = Locale::en_us();
        let mut t: Table<String> = Table::new(4, 3);
        t.type_text(0, 1, "2006", &en);
        t.type_text(1, 1, "10", &en);
        t.type_text(2, 1, "5", &en);
        t.type_text(3, 1, "=SUM('2006')", &en);
        t.type_text(3, 2, "=C4+1", &en);
        t.set_footer_rows(1).unwrap();
        t.type_text(1, 2, "=B2*TODAY()", &en);
        let v = t.values(2.0);
        assert_eq!(v.get(3, 1), &Value::Number(15.0));
        assert_eq!(v.get(3, 2), &Value::Error(ErrorKind::Circular));
        assert_eq!(v.get(1, 2), &Value::Number(20.0));
        assert_eq!(t.shown(&v, 3, 1, &en).text, "15");
    }
}
