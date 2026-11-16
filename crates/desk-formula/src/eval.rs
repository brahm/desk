// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Evaluating formulas.

use std::cmp::Ordering;

use crate::functions;
use crate::parse::{BinaryOp, CellRef, Expr, Formula, UnaryOp};
use crate::value::{ErrorKind, Value};

/// What a formula reads: the values of the cells of its table.
pub trait Context {
    /// The value of a cell; [`Value::Empty`] outside the table. A formula
    /// cell gives its result, and a cell whose formula depends on itself
    /// gives [`ErrorKind::Circular`].
    fn cell(&self, at: CellRef) -> Value;

    /// The cells that a header names: the body cells of the column under
    /// a header of a header row, or of the row beside a header of a
    /// header column; none when no header has that text (TAB-12).
    fn header(&self, name: &str) -> Option<(CellRef, CellRef)>;

    /// The date and time now, in days from 30 December 1899, for TODAY
    /// and NOW.
    fn now(&self) -> f64 {
        0.0
    }
}

/// Computes a formula.
pub fn evaluate(formula: &Formula, context: &dyn Context) -> Value {
    let eval = Eval { context };
    match eval.scalar(&formula.expr) {
        Value::Number(n) if !n.is_finite() => Value::Error(ErrorKind::Number),
        v => v,
    }
}

/// An argument of a function: one value, or the values of a range by
/// rows.
#[derive(Clone, Debug)]
pub(crate) enum Arg {
    Scalar(Value),
    Range { values: Vec<Value>, cols: usize },
}

impl Arg {
    /// The values, one for a scalar.
    pub(crate) fn values(&self) -> Vec<Value> {
        match self {
            Self::Scalar(v) => vec![v.clone()],
            Self::Range { values, .. } => values.clone(),
        }
    }

    pub(crate) fn is_range(&self) -> bool {
        matches!(self, Self::Range { .. })
    }

    /// The rows and columns.
    pub(crate) fn shape(&self) -> (usize, usize) {
        match self {
            Self::Scalar(_) => (1, 1),
            Self::Range { values, cols } => (values.len() / (*cols).max(1), *cols),
        }
    }

    /// The value at a row and a column, from 0.
    pub(crate) fn at(&self, row: usize, col: usize) -> Value {
        match self {
            Self::Scalar(v) => v.clone(),
            Self::Range { values, cols } => {
                if col >= *cols {
                    return Value::Error(ErrorKind::Reference);
                }
                values
                    .get(row * cols + col)
                    .cloned()
                    .unwrap_or(Value::Error(ErrorKind::Reference))
            }
        }
    }
}

pub(crate) struct Eval<'a> {
    pub(crate) context: &'a dyn Context,
}

impl Eval<'_> {
    fn range(&self, a: CellRef, b: CellRef) -> Arg {
        let (c0, c1) = (a.col.min(b.col), a.col.max(b.col));
        let (r0, r1) = (a.row.min(b.row), a.row.max(b.row));
        // A range larger than any table is a mistake; keep it bounded.
        if u64::from(c1 - c0 + 1) * u64::from(r1 - r0 + 1) > 1_000_000 {
            return Arg::Scalar(Value::Error(ErrorKind::Reference));
        }
        let mut values = Vec::new();
        for row in r0..=r1 {
            for col in c0..=c1 {
                values.push(self.context.cell(CellRef::new(col, row)));
            }
        }
        Arg::Range {
            values,
            cols: (c1 - c0 + 1) as usize,
        }
    }

    /// An argument, keeping ranges whole.
    pub(crate) fn arg(&self, expr: &Expr) -> Arg {
        match expr {
            Expr::Range(a, b) => self.range(*a, *b),
            Expr::Cell(c) => Arg::Range {
                values: vec![self.context.cell(*c)],
                cols: 1,
            },
            Expr::Header(name) => match self.context.header(name) {
                Some((a, b)) => self.range(a, b),
                None => Arg::Scalar(Value::Error(ErrorKind::Name)),
            },
            other => Arg::Scalar(self.scalar(other)),
        }
    }

    /// One value: a range of one cell gives that cell.
    pub(crate) fn scalar(&self, expr: &Expr) -> Value {
        match expr {
            Expr::Number(n) => Value::Number(*n),
            Expr::Text(t) => Value::Text(t.clone()),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Error(e) => Value::Error(*e),
            Expr::Cell(c) => self.context.cell(*c),
            Expr::Range(..) | Expr::Header(_) => match self.arg(expr) {
                Arg::Scalar(v) => v,
                Arg::Range { values, .. } if values.len() == 1 => values[0].clone(),
                Arg::Range { .. } => Value::Error(ErrorKind::Value),
            },
            Expr::Unary(op, a) => {
                let v = self.scalar(a);
                let n = match v.to_number() {
                    Ok(n) => n,
                    Err(e) => return Value::Error(e),
                };
                match op {
                    UnaryOp::Negate => match v {
                        Value::Duration(_) => Value::Duration(-n),
                        _ => Value::Number(-n),
                    },
                    UnaryOp::Plus => v,
                    UnaryOp::Percent => Value::Number(n / 100.0),
                }
            }
            Expr::Binary(op, a, b) => self.binary(*op, &self.scalar(a), &self.scalar(b)),
            Expr::Call(name, args) => match functions::function(name) {
                Some(f) => (f.run)(self, args),
                None => Value::Error(ErrorKind::Name),
            },
        }
    }

    fn binary(&self, op: BinaryOp, a: &Value, b: &Value) -> Value {
        if let Value::Error(e) = a {
            return Value::Error(*e);
        }
        if let Value::Error(e) = b {
            return Value::Error(*e);
        }
        match op {
            BinaryOp::Concat => match (a.to_text(), b.to_text()) {
                (Ok(x), Ok(y)) => Value::Text(x + &y),
                (Err(e), _) | (_, Err(e)) => Value::Error(e),
            },
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessOrEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterOrEqual => {
                let ord = compare(a, b);
                Value::Bool(match op {
                    BinaryOp::Equal => ord == Ordering::Equal,
                    BinaryOp::NotEqual => ord != Ordering::Equal,
                    BinaryOp::Less => ord == Ordering::Less,
                    BinaryOp::LessOrEqual => ord != Ordering::Greater,
                    BinaryOp::Greater => ord == Ordering::Greater,
                    _ => ord != Ordering::Less,
                })
            }
            _ => {
                let (x, y) = match (a.to_number(), b.to_number()) {
                    (Ok(x), Ok(y)) => (x, y),
                    (Err(e), _) | (_, Err(e)) => return Value::Error(e),
                };
                let n = match op {
                    BinaryOp::Add => x + y,
                    BinaryOp::Subtract => x - y,
                    BinaryOp::Multiply => x * y,
                    BinaryOp::Divide => {
                        if y == 0.0 {
                            return Value::Error(ErrorKind::DivisionByZero);
                        }
                        x / y
                    }
                    _ => {
                        let p = x.powf(y);
                        if !p.is_finite() {
                            return Value::Error(ErrorKind::Number);
                        }
                        p
                    }
                };
                // A date plus or minus a number of days is a date, and the
                // difference of two dates is a duration.
                match (op, a, b) {
                    (BinaryOp::Add | BinaryOp::Subtract, Value::Date(_), Value::Date(_)) => {
                        if op == BinaryOp::Subtract {
                            Value::Duration(n)
                        } else {
                            Value::Number(n)
                        }
                    }
                    (BinaryOp::Add | BinaryOp::Subtract, Value::Date(_), _)
                    | (BinaryOp::Add, _, Value::Date(_)) => Value::Date(n),
                    (
                        BinaryOp::Add | BinaryOp::Subtract,
                        Value::Duration(_),
                        Value::Duration(_),
                    ) => Value::Duration(n),
                    _ => Value::Number(n),
                }
            }
        }
    }
}

/// The order of two values: numbers before text, text before true and
/// false, and empty as 0 or as empty text.
pub(crate) fn compare(a: &Value, b: &Value) -> Ordering {
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Number(_) | Value::Date(_) | Value::Duration(_) | Value::Empty => 0,
            Value::Text(_) => 1,
            Value::Bool(_) => 2,
            Value::Error(_) => 3,
        }
    }
    match (a, b) {
        (Value::Empty, Value::Text(t)) => "".cmp(t.to_lowercase().as_str()),
        (Value::Text(t), Value::Empty) => t.to_lowercase().as_str().cmp(""),
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A table for the tests: cells by name, with values or formulas.
    #[derive(Default)]
    pub(crate) struct Sheet {
        pub(crate) cells: HashMap<CellRef, Value>,
        pub(crate) headers: HashMap<String, (CellRef, CellRef)>,
    }

    impl Sheet {
        pub(crate) fn set(&mut self, name: &str, v: Value) {
            self.cells.insert(CellRef::parse(name).unwrap(), v);
        }
    }

    impl Context for Sheet {
        fn cell(&self, at: CellRef) -> Value {
            self.cells
                .get(&CellRef::new(at.col, at.row))
                .cloned()
                .unwrap_or_default()
        }
        fn header(&self, name: &str) -> Option<(CellRef, CellRef)> {
            self.headers.get(name).copied()
        }
        fn now(&self) -> f64 {
            45_000.5
        }
    }

    pub(crate) fn run(sheet: &Sheet, text: &str) -> Value {
        evaluate(&Formula::parse(text).unwrap(), sheet)
    }

    #[test]
    fn tab_32_arithmetic_and_comparisons() {
        let mut s = Sheet::default();
        s.set("A1", Value::Number(2.0));
        s.set("A2", Value::Text("3".into()));
        assert_eq!(run(&s, "=A1*A2+1"), Value::Number(7.0));
        assert_eq!(run(&s, "=A1/0"), Value::Error(ErrorKind::DivisionByZero));
        assert_eq!(run(&s, "=2^10"), Value::Number(1024.0));
        assert_eq!(run(&s, "=50%*A1"), Value::Number(1.0));
        assert_eq!(run(&s, "=A1&\"x\""), Value::Text("2x".into()));
        assert_eq!(run(&s, "=A1<A2"), Value::Bool(true));
        assert_eq!(run(&s, "=\"abc\"=\"ABC\""), Value::Bool(true));
        assert_eq!(run(&s, "=A9+1"), Value::Number(1.0));
        assert_eq!(run(&s, "=NOPE(1)"), Value::Error(ErrorKind::Name));
    }

    #[test]
    fn tab_12_headers_name_ranges() {
        let mut s = Sheet::default();
        s.set("B2", Value::Number(10.0));
        s.set("B3", Value::Number(5.0));
        s.headers
            .insert("2006".into(), (CellRef::new(1, 1), CellRef::new(1, 2)));
        assert_eq!(run(&s, "=SUM('2006')"), Value::Number(15.0));
        assert_eq!(run(&s, "=SUM(Nada)"), Value::Error(ErrorKind::Name));
    }
}
