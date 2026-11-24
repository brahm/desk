// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The values of the cells, with the formulas computed (TAB-32).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use desk_formula::{CellRef, Context, ErrorKind, Expr, Formula, Value};

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

/// What the formulas of a table may do in one evaluation: cells of
/// ranges read and bytes of text made, a few seconds of work. A table
/// that a file fills with formulas over large ranges or long texts gives
/// `#VALUE!` past it rather than holding the app or filling its memory.
const MAX_WORK: u64 = 1 << 27;

/// The largest range a formula reads, as `desk-formula` bounds it.
const MAX_RANGE_CELLS: u64 = 1_000_000;

/// The functions that read where their cell or range is, not its values.
const POSITION_FUNCTIONS: [&str; 4] = ["ROW", "COLUMN", "ROWS", "COLUMNS"];

struct Evaluator<'a, T> {
    table: &'a Table<T>,
    cache: RefCell<HashMap<(u32, u32), Value>>,
    visiting: RefCell<Vec<(u32, u32)>>,
    now: f64,
    /// What is left of [`MAX_WORK`].
    work_left: std::cell::Cell<u64>,
}

/// A step of [`Evaluator::prepare`].
enum Step {
    /// Look at a cell and what it depends on.
    Enter(u32, u32),
    /// Compute a cell, once what it depends on is computed.
    Leave(u32, u32),
}

impl<T: CellText> Evaluator<'_, T> {
    /// The header cells that finding a header by its name looks at.
    fn header_cost(&self) -> u64 {
        let t = self.table;
        u64::from(t.header_rows()) * u64::from(t.cols())
            + u64::from(t.body_end().saturating_sub(t.body_start())) * u64::from(t.header_cols())
    }

    fn is_pending_formula(&self, row: u32, col: u32) -> bool {
        row < self.table.rows()
            && col < self.table.cols()
            && matches!(self.table.cell(row, col).content, Content::Formula(_))
            && !self.cache.borrow().contains_key(&(row, col))
    }

    /// The formula cells not yet computed that a formula refers to. The
    /// cells looked at are work: once it is spent, the formulas left give
    /// `#VALUE!` when they are computed, without looking further.
    fn dependencies(&self, expr: &Expr, out: &mut Vec<(u32, u32)>) {
        let mut rect = |a: CellRef, b: CellRef| {
            let (rows, cols) = (
                u64::from(a.row.abs_diff(b.row)) + 1,
                u64::from(a.col.abs_diff(b.col)) + 1,
            );
            // A range the formula refuses to read has nothing to prepare.
            if rows * cols > MAX_RANGE_CELLS {
                return;
            }
            // Only the part inside the table is looked at. A refused
            // charge leaves the work for others: the formula then refuses
            // the range too, as it costs at least as much.
            let last_row = a.row.max(b.row).min(self.table.rows().saturating_sub(1));
            let last_col = a.col.max(b.col).min(self.table.cols().saturating_sub(1));
            let (first_row, first_col) = (a.row.min(b.row), a.col.min(b.col));
            if first_row > last_row || first_col > last_col {
                return;
            }
            let inside = u64::from(last_row - first_row + 1) * u64::from(last_col - first_col + 1);
            if !self.spend(inside) {
                return;
            }
            for r in first_row..=last_row {
                for c in first_col..=last_col {
                    if self.is_pending_formula(r, c) {
                        out.push((r, c));
                    }
                }
            }
        };
        match expr {
            Expr::Cell(a) => rect(*a, *a),
            Expr::Range(a, b) => rect(*a, *b),
            Expr::Header(name) => {
                if self.spend(self.header_cost())
                    && let Some((a, b)) = header_range(self.table, name)
                {
                    rect(a, b);
                }
            }
            Expr::Unary(_, e) => self.dependencies(e, out),
            Expr::Binary(_, a, b) => {
                self.dependencies(a, out);
                self.dependencies(b, out);
            }
            // The place of a cell or a range is not its value: =ROW(A2)
            // in A2 is not circular.
            Expr::Call(name, args)
                if POSITION_FUNCTIONS.contains(&name.as_str())
                    && args
                        .iter()
                        .all(|a| matches!(a, Expr::Cell(_) | Expr::Range(..))) => {}
            Expr::Call(_, args) => {
                for a in args {
                    self.dependencies(a, out);
                }
            }
            Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) => {}
        }
    }

    /// Computes the formulas a cell depends on, the deepest first, with a
    /// stack of its own: computed by recursion, a chain of formulas that
    /// each refer to the next, as a running balance from the bottom up,
    /// overflowed the stack. The cells of a cycle are circular.
    fn prepare(&self, row: u32, col: u32) {
        let mut steps = vec![Step::Enter(row, col)];
        let mut path: Vec<(u32, u32)> = Vec::new();
        let mut on_path: HashSet<(u32, u32)> = HashSet::new();
        while let Some(step) = steps.pop() {
            match step {
                Step::Enter(r, c) => {
                    if !self.is_pending_formula(r, c) {
                        continue;
                    }
                    if on_path.contains(&(r, c)) {
                        // Every cell from this one up the path is in the
                        // cycle.
                        let start = path.iter().rposition(|&p| p == (r, c)).unwrap_or(0);
                        let mut cache = self.cache.borrow_mut();
                        for &p in path.get(start..).unwrap_or_default() {
                            cache.insert(p, Value::Error(ErrorKind::Circular));
                        }
                        continue;
                    }
                    steps.push(Step::Leave(r, c));
                    path.push((r, c));
                    on_path.insert((r, c));
                    if let Content::Formula(text) = &self.table.cell(r, c).content
                        && let Ok(f) = Formula::parse(text)
                    {
                        let mut deps = Vec::new();
                        self.dependencies(&f.expr, &mut deps);
                        steps.extend(deps.into_iter().map(|(r, c)| Step::Enter(r, c)));
                    }
                }
                Step::Leave(r, c) => {
                    path.pop();
                    on_path.remove(&(r, c));
                    self.value(r, c);
                }
            }
        }
    }

    fn value(&self, row: u32, col: u32) -> Value {
        if row >= self.table.rows() || col >= self.table.cols() {
            return Value::Empty;
        }
        if let Some(v) = self.cache.borrow().get(&(row, col)) {
            return v.clone();
        }
        // A formula asked for from outside another one: what it depends
        // on first, without recursion.
        if self.visiting.borrow().is_empty() && self.is_pending_formula(row, col) {
            self.visiting.borrow_mut().push((row, col));
            self.prepare(row, col);
            self.visiting.borrow_mut().pop();
            if let Some(v) = self.cache.borrow().get(&(row, col)) {
                return v.clone();
            }
        }
        let cell = self.table.cell(row, col);
        let v = match &cell.content {
            Content::Empty => Value::Empty,
            Content::Text(t) => Value::Text(t.plain()),
            Content::Number(n) => Value::Number(n.0),
            Content::Bool(b) => Value::Bool(*b),
            Content::Date(d) => Value::Date(d.0),
            Content::Duration(d) => Value::Duration(d.0),
            // With the work spent, a formula is not computed: what it
            // refers to may not have been prepared, and computing it
            // would recurse.
            Content::Formula(_) if self.work_left.get() == 0 => Value::Error(ErrorKind::Value),
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
        if !self.spend(self.header_cost()) {
            return None;
        }
        header_range(self.table, name)
    }

    fn now(&self) -> f64 {
        self.now
    }

    /// A charge refused leaves the work as it was, for the formulas
    /// that ask for less.
    fn spend(&self, work: u64) -> bool {
        match self.work_left.get().checked_sub(work) {
            Some(left) => {
                self.work_left.set(left);
                true
            }
            None => false,
        }
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
        self.values_within(now, MAX_WORK)
    }

    /// [`Table::values`] with `work` for the formulas to do.
    fn values_within(&self, now: f64, work: u64) -> Values {
        let eval = Evaluator {
            table: self,
            cache: RefCell::new(HashMap::new()),
            visiting: RefCell::new(Vec::new()),
            now,
            work_left: std::cell::Cell::new(work),
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

    #[test]
    fn formulas_stop_when_their_work_is_spent() {
        let en = Locale::en_us();
        let mut t: Table<String> = Table::new(40, 2);
        for r in 0..40 {
            t.type_text(r, 0, "1", &en);
            // Each reads 65,535 cells, most of them past the table.
            t.type_text(r, 1, "=SUM(A1:A65535)", &en);
        }
        let v = t.values_within(0.0, 1 << 20);
        assert_eq!(v.get(0, 1), &Value::Number(40.0));
        assert_eq!(v.get(39, 1), &Value::Error(ErrorKind::Value));
        // A table of the size people make is far from the limit.
        let v = t.values(0.0);
        assert_eq!(v.get(39, 1), &Value::Number(40.0));
    }

    #[test]
    fn a_range_refused_leaves_the_work_to_the_other_formulas() {
        let en = Locale::en_us();
        let mut t: Table<String> = Table::new(3, 2);
        t.type_text(0, 0, "2", &en);
        // Larger than any range a formula reads.
        t.type_text(0, 1, "=SUM(A1:ZZZ60000)", &en);
        t.type_text(1, 1, "=A1*3", &en);
        // Larger than the work left, but not than the limit of a range.
        t.type_text(2, 1, "=SUM(A1:A60000)", &en);
        let v = t.values_within(0.0, 1_000);
        assert_eq!(v.get(0, 1), &Value::Error(ErrorKind::Reference));
        assert_eq!(v.get(2, 1), &Value::Error(ErrorKind::Value));
        assert_eq!(v.get(1, 1), &Value::Number(6.0));
    }

    #[test]
    fn the_place_of_its_own_cell_is_not_circular() {
        let en = Locale::en_us();
        let mut t: Table<String> = Table::new(3, 1);
        t.type_text(1, 0, "=ROW(A2)-1", &en);
        t.type_text(2, 0, "=ROWS(A1:A3)+COLUMN(A3)", &en);
        let v = t.values(0.0);
        assert_eq!(v.get(1, 0), &Value::Number(1.0));
        assert_eq!(v.get(2, 0), &Value::Number(4.0));
    }

    #[test]
    fn long_texts_made_by_formulas_are_bounded() {
        let en = Locale::en_us();
        let mut t: Table<String> = Table::new(30, 2);
        t.type_text(0, 0, "=REPT(\"ab\",10000)", &en);
        // Each doubles the text before it.
        for r in 1..30 {
            t.type_text(r, 0, &format!("=A{r}&A{r}"), &en);
        }
        t.type_text(0, 1, "=SUBSTITUTE(A1,\"a\",A1)", &en);
        t.type_text(1, 1, "=CONCATENATE(A1,A1)", &en);
        let v = t.values(0.0);
        assert!(matches!(v.get(0, 0), Value::Text(s) if s.len() == 20_000));
        assert_eq!(v.get(1, 0), &Value::Error(ErrorKind::Value));
        assert_eq!(v.get(29, 0), &Value::Error(ErrorKind::Value));
        assert_eq!(v.get(0, 1), &Value::Error(ErrorKind::Value));
        assert_eq!(v.get(1, 1), &Value::Error(ErrorKind::Value));
        // Texts within the limit, many times over, spend the work.
        let mut t: Table<String> = Table::new(100, 1);
        for r in 0..100 {
            t.type_text(r, 0, "=REPT(\"a\",30000)", &en);
        }
        let v = t.values_within(0.0, 1 << 20);
        assert!(matches!(v.get(0, 0), Value::Text(_)));
        assert_eq!(v.get(99, 0), &Value::Error(ErrorKind::Value));
    }

    #[test]
    fn the_longest_sum_a_formula_holds_is_computed_and_moved() {
        // In a thread with the small stack of the tests, without
        // optimization.
        let en = Locale::en_us();
        let mut t: Table<String> = Table::new(3, 1);
        t.type_text(0, 0, "1", &en);
        let sum = format!("=A1{}", "+A1".repeat(1_020));
        t.type_text(1, 0, &sum, &en);
        assert_eq!(t.values(0.0).get(1, 0), &Value::Number(1_021.0));
        t.insert_rows(0, 1).unwrap();
        assert!(t.source_text(2, 0, &en).starts_with("=A2+A2"));
    }

    #[test]
    fn a_long_chain_of_formulas_is_computed_without_recursion() {
        // A1 = A2 + 1, A2 = A3 + 1 and so on, a running balance from the
        // bottom up, overflowed the stack (review RV, batch 2).
        let en = Locale::en_us();
        let n = 20_000;
        let mut t: Table<String> = Table::new(n, 1);
        for r in 0..n - 1 {
            t.type_text(r, 0, &format!("=A{}+1", r + 2), &en);
        }
        t.type_text(n - 1, 0, "1", &en);
        let v = t.values(0.0);
        assert_eq!(v.get(0, 0), &Value::Number(f64::from(n)));
        // A chain that closes on itself is circular in every cell.
        t.type_text(n - 1, 0, "=A1", &en);
        let v = t.values(0.0);
        assert_eq!(v.get(0, 0), &Value::Error(ErrorKind::Circular));
        assert_eq!(v.get(n - 1, 0), &Value::Error(ErrorKind::Circular));
    }
}
