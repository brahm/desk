// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The functions of formulas, as the Function Browser lists them
//! (TAB-32).

use std::cmp::Ordering;

use crate::eval::{Eval, MAX_TEXT, compare};
use crate::parse::Expr;
use crate::value::{ErrorKind, Value, date_from_days, days_from_date};

type Result<T> = std::result::Result<T, ErrorKind>;

/// A function of formulas.
pub struct FunctionInfo {
    /// The name, in capitals.
    pub name: &'static str,
    /// The category of the Function Browser: Data e hora, Engenharia,
    /// Financeira, Lógica, Numérica, Consulta, Estatística, Texto or
    /// Trigonométrica, by their keys `date`, `financial`, `logical`,
    /// `numeric`, `lookup`, `statistical`, `text` and `trigonometric`.
    pub category: &'static str,
    /// How it is written, with its arguments.
    pub syntax: &'static str,
    /// What it gives, in one sentence.
    pub summary: &'static str,
    pub(crate) run: fn(&Eval<'_>, &[Expr]) -> Value,
}

impl std::fmt::Debug for FunctionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionInfo")
            .field("name", &self.name)
            .finish()
    }
}

/// The function with a name, in any case.
pub fn function(name: &str) -> Option<&'static FunctionInfo> {
    let upper = name.to_ascii_uppercase();
    FUNCTIONS.iter().find(|f| f.name == upper)
}

fn wrap(r: Result<Value>) -> Value {
    match r {
        Ok(Value::Number(n)) if !n.is_finite() => Value::Error(ErrorKind::Number),
        Ok(v) => v,
        Err(e) => Value::Error(e),
    }
}

fn count(args: &[Expr], min: usize, max: usize) -> Result<()> {
    if args.len() < min || args.len() > max {
        Err(ErrorKind::Value)
    } else {
        Ok(())
    }
}

fn num(e: &Eval<'_>, x: &Expr) -> Result<f64> {
    e.scalar(x).to_number()
}

fn text(e: &Eval<'_>, x: &Expr) -> Result<String> {
    e.scalar(x).to_text()
}

fn boolean(e: &Eval<'_>, x: &Expr) -> Result<bool> {
    e.scalar(x).to_bool()
}

/// The numbers of the arguments, as SUM takes them: in ranges only the
/// numbers count, and other arguments must read as numbers.
fn numbers(e: &Eval<'_>, args: &[Expr]) -> Result<Vec<f64>> {
    let mut out = Vec::new();
    for a in args {
        let arg = e.arg(a);
        if arg.is_range() {
            for v in arg.values() {
                match v {
                    Value::Number(n) | Value::Date(n) | Value::Duration(n) => out.push(n),
                    Value::Error(err) => return Err(err),
                    _ => {}
                }
            }
        } else {
            let first = arg.values().into_iter().next();
            out.push(first.ok_or(ErrorKind::Value)?.to_number()?);
        }
    }
    Ok(out)
}

/// The values of the arguments, as COUNTA takes them.
fn all_values(e: &Eval<'_>, args: &[Expr]) -> Vec<Value> {
    args.iter().flat_map(|a| e.arg(a).values()).collect()
}

/// The numbers of the arguments with text and true and false counted, as
/// AVERAGEA takes them.
fn numbers_a(e: &Eval<'_>, args: &[Expr]) -> Result<Vec<f64>> {
    let mut out = Vec::new();
    for v in all_values(e, args) {
        match v {
            Value::Empty => {}
            Value::Text(_) => out.push(0.0),
            Value::Error(err) => return Err(err),
            v => out.push(v.to_number()?),
        }
    }
    Ok(out)
}

/// An argument; a missing one is an error of value, though the count of
/// arguments was checked before.
fn at(a: &[Expr], i: usize) -> Result<&Expr> {
    a.get(i).ok_or(ErrorKind::Value)
}

fn int(e: &Eval<'_>, x: &Expr) -> Result<i64> {
    Ok(num(e, x)?.trunc() as i64)
}

/// A number cut to 15 significant digits, as many as spreadsheets keep:
/// the decimal digits a binary number misses come back, as 2.675 is
/// stored a little under it, and 0.1 + 0.2 a little over 0.3.
fn significant(n: f64) -> f64 {
    if !n.is_finite() || n == 0.0 {
        return n;
    }
    format!("{n:.14e}").parse().unwrap_or(n)
}

fn round_to(n: f64, digits: i64, mode: fn(f64) -> f64) -> f64 {
    let factor = 10f64.powi(digits.clamp(-15, 15) as i32);
    let scaled = n * factor;
    // A number so large has no such digits to round.
    if !scaled.is_finite() {
        return n;
    }
    // Relative to the number, not to its digits: rounding to a fixed
    // number of decimals first pushed 1.9999999999 up to 2 before TRUNC.
    mode(significant(scaled)) / factor
}

fn round_half_away(x: f64) -> f64 {
    x.round()
}

fn mean(v: &[f64]) -> Result<f64> {
    if v.is_empty() {
        return Err(ErrorKind::DivisionByZero);
    }
    Ok(v.iter().sum::<f64>() / v.len() as f64)
}

fn variance(v: &[f64], sample: bool) -> Result<f64> {
    let n = v.len();
    if n == 0 || (sample && n < 2) {
        return Err(ErrorKind::DivisionByZero);
    }
    let m = mean(v)?;
    let ss: f64 = v.iter().map(|x| (x - m).powi(2)).sum();
    Ok(ss / if sample { (n - 1) as f64 } else { n as f64 })
}

/// Whether a value meets a condition of COUNTIF and SUMIF, such as
/// `">5"`, `"<>x"`, `"ab*"` or a value.
fn matches(value: &Value, criterion: &Value) -> bool {
    let (op, operand) = match criterion {
        Value::Text(t) => {
            let ops = [">=", "<=", "<>", "=", ">", "<"];
            match ops.iter().find(|op| t.starts_with(**op)) {
                Some(op) => (*op, Value::Text(t[op.len()..].to_owned())),
                None => ("=", criterion.clone()),
            }
        }
        _ => ("=", criterion.clone()),
    };
    // The operand as a number when it reads as one.
    let operand = match &operand {
        Value::Text(t) => t
            .trim()
            .parse::<f64>()
            .map_or_else(|_| operand.clone(), Value::Number),
        _ => operand,
    };
    if let (Value::Text(pattern), "=" | "<>") = (&operand, op) {
        let hit = match value {
            Value::Text(t) => wildcard(&pattern.to_lowercase(), &t.to_lowercase()),
            Value::Empty => pattern.is_empty(),
            _ => false,
        };
        return if op == "=" { hit } else { !hit };
    }
    if matches!(value, Value::Empty) && op != "<>" {
        return false;
    }
    if operand.is_numeric() != value.is_numeric() {
        return op == "<>";
    }
    let ord = compare(value, &operand);
    match op {
        "=" => ord == Ordering::Equal,
        "<>" => ord != Ordering::Equal,
        ">" => ord == Ordering::Greater,
        "<" => ord == Ordering::Less,
        ">=" => ord != Ordering::Less,
        _ => ord != Ordering::Greater,
    }
}

/// Whether text matches a pattern with `*` for any text and `?` for one
/// character.
fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    wildcard_chars(&p, &t)
}

/// [`wildcard`] on characters: in time of the pattern by the text at most,
/// going back only to the last `*`.
fn wildcard_chars(p: &[char], t: &[char]) -> bool {
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while let Some(&tc) = t.get(ti) {
        match p.get(pi) {
            Some(&pc) if pc == '?' || pc == tc => {
                pi += 1;
                ti += 1;
            }
            Some('*') => {
                star = Some(pi);
                mark = ti;
                pi += 1;
            }
            _ => match star {
                Some(s) => {
                    pi = s + 1;
                    mark += 1;
                    ti = mark;
                }
                None => return false,
            },
        }
    }
    while p.get(pi) == Some(&'*') {
        pi += 1;
    }
    pi == p.len()
}

fn date_parts(e: &Eval<'_>, x: &Expr) -> Result<(i32, u32, u32, f64)> {
    let n = num(e, x)?;
    let (y, m, d) = date_from_days(n.floor() as i64);
    Ok((y, m, d, n - n.floor()))
}

fn lookup_position(needle: &Value, hay: &[Value], exact: bool) -> Option<usize> {
    if exact {
        return hay.iter().position(|v| match (needle, v) {
            (Value::Text(p), Value::Text(t)) => wildcard(&p.to_lowercase(), &t.to_lowercase()),
            _ => compare(v, needle) == Ordering::Equal && !matches!(v, Value::Empty),
        });
    }
    // The largest value not above the one looked for, in a sorted list.
    let mut found = None;
    for (i, v) in hay.iter().enumerate() {
        if matches!(v, Value::Empty) {
            continue;
        }
        if compare(v, needle) != Ordering::Greater {
            found = Some(i);
        } else {
            break;
        }
    }
    found
}

macro_rules! f {
    ($name:literal, $cat:literal, $syntax:literal, $summary:literal, $run:expr) => {
        FunctionInfo {
            name: $name,
            category: $cat,
            syntax: $syntax,
            summary: $summary,
            run: |e, a| wrap(($run)(e, a)),
        }
    };
}

/// The functions, by category and name.
pub static FUNCTIONS: &[FunctionInfo] = &[
    // Date and time.
    f!(
        "DATE",
        "date",
        "DATE(ano, mês, dia)",
        "A date from its year, month and day.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 3)?;
            Ok(Value::Date(days_from_date(
                int(e, at(a, 0)?)? as i32,
                int(e, at(a, 1)?)? as i32,
                int(e, at(a, 2)?)? as i32,
            ) as f64))
        }
    ),
    f!(
        "TIME",
        "date",
        "TIME(hora, minuto, segundo)",
        "A time of day.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 3)?;
            let s = num(e, at(a, 0)?)? * 3600.0 + num(e, at(a, 1)?)? * 60.0 + num(e, at(a, 2)?)?;
            Ok(Value::Date((s / 86_400.0).rem_euclid(1.0)))
        }
    ),
    f!(
        "TODAY",
        "date",
        "TODAY()",
        "Today's date.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 0, 0)?;
            Ok(Value::Date(e.context.now().floor()))
        }
    ),
    f!(
        "NOW",
        "date",
        "NOW()",
        "The date and time now.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 0, 0)?;
            Ok(Value::Date(e.context.now()))
        }
    ),
    f!(
        "YEAR",
        "date",
        "YEAR(data)",
        "The year of a date.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(f64::from(date_parts(e, at(a, 0)?)?.0)))
        }
    ),
    f!(
        "MONTH",
        "date",
        "MONTH(data)",
        "The month of a date, from 1 to 12.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(f64::from(date_parts(e, at(a, 0)?)?.1)))
        }
    ),
    f!(
        "DAY",
        "date",
        "DAY(data)",
        "The day of the month of a date.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(f64::from(date_parts(e, at(a, 0)?)?.2)))
        }
    ),
    f!(
        "HOUR",
        "date",
        "HOUR(hora)",
        "The hour of a time, from 0 to 23.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(
                (date_parts(e, at(a, 0)?)?.3 * 24.0 + 1e-9).floor(),
            ))
        }
    ),
    f!(
        "MINUTE",
        "date",
        "MINUTE(hora)",
        "The minutes of a time.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(
                ((date_parts(e, at(a, 0)?)?.3 * 1440.0 + 1e-7).floor()) % 60.0,
            ))
        }
    ),
    f!(
        "SECOND",
        "date",
        "SECOND(hora)",
        "The seconds of a time.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(
                ((date_parts(e, at(a, 0)?)?.3 * 86_400.0).round()) % 60.0,
            ))
        }
    ),
    f!(
        "WEEKDAY",
        "date",
        "WEEKDAY(data, [tipo])",
        "The day of the week, from 1 for Sunday.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let d = num(e, at(a, 0)?)?.floor() as i64;
            let sunday_based = (d + 6).rem_euclid(7) + 1;
            let kind = if a.len() > 1 { int(e, at(a, 1)?)? } else { 1 };
            Ok(Value::Number(match kind {
                1 => sunday_based,
                2 => (d + 5).rem_euclid(7) + 1,
                3 => (d + 5).rem_euclid(7),
                _ => return Err(ErrorKind::Number),
            } as f64))
        }
    ),
    f!(
        "DAYS360",
        "date",
        "DAYS360(início, fim)",
        "The days between two dates in a year of 360 days.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 3)?;
            let (y1, m1, mut d1, _) = date_parts(e, at(a, 0)?)?;
            let (y2, m2, mut d2, _) = date_parts(e, at(a, 1)?)?;
            d1 = d1.min(30);
            if d2 == 31 && d1 == 30 {
                d2 = 30;
            }
            Ok(Value::Number(f64::from(
                (y2 - y1) * 360 + (m2 as i32 - m1 as i32) * 30 + (d2 as i32 - d1 as i32),
            )))
        }
    ),
    f!(
        "DATEDIF",
        "date",
        "DATEDIF(início, fim, unidade)",
        "The years, months or days between two dates.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 3)?;
            let s = num(e, at(a, 0)?)?.floor();
            let t = num(e, at(a, 1)?)?.floor();
            if t < s {
                return Err(ErrorKind::Number);
            }
            let (y1, m1, d1) = date_from_days(s as i64);
            let (y2, m2, d2) = date_from_days(t as i64);
            let months = (y2 - y1) * 12 + m2 as i32 - m1 as i32 - i32::from(d2 < d1);
            Ok(Value::Number(
                match text(e, at(a, 2)?)?.to_uppercase().as_str() {
                    "D" => t - s,
                    "M" => f64::from(months),
                    "Y" => f64::from(months / 12),
                    "YM" => f64::from(months % 12),
                    "MD" => {
                        let base =
                            days_from_date(y2, m2 as i32 - i32::from(d2 < d1), d1 as i32) as f64;
                        t - base
                    }
                    _ => return Err(ErrorKind::Number),
                },
            ))
        }
    ),
    f!(
        "EDATE",
        "date",
        "EDATE(data, meses)",
        "The date some months before or after.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            let (y, m, d, _) = date_parts(e, at(a, 0)?)?;
            let months = int(e, at(a, 1)?)?;
            let target_first = days_from_date(y, m as i32 + months as i32, 1);
            let (ty, tm, _) = date_from_days(target_first);
            let last = date_from_days(days_from_date(ty, tm as i32 + 1, 1) - 1).2;
            Ok(Value::Date(
                days_from_date(ty, tm as i32, d.min(last) as i32) as f64,
            ))
        }
    ),
    // Financial.
    f!(
        "PMT",
        "financial",
        "PMT(taxa, períodos, valor-presente, [valor-futuro], [quando])",
        "The payment of a loan for each period.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 5)?;
            let rate = num(e, at(a, 0)?)?;
            let n = num(e, at(a, 1)?)?;
            let pv = num(e, at(a, 2)?)?;
            let fv = if a.len() > 3 { num(e, at(a, 3)?)? } else { 0.0 };
            let when = if a.len() > 4 { num(e, at(a, 4)?)? } else { 0.0 };
            if n == 0.0 {
                return Err(ErrorKind::Number);
            }
            if rate == 0.0 {
                return Ok(Value::Number(-(pv + fv) / n));
            }
            let f = (1.0 + rate).powf(n);
            Ok(Value::Number(
                -(rate * (pv * f + fv)) / ((1.0 + rate * when) * (f - 1.0)),
            ))
        }
    ),
    f!(
        "FV",
        "financial",
        "FV(taxa, períodos, pagamento, [valor-presente], [quando])",
        "The future value of an investment.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 5)?;
            let rate = num(e, at(a, 0)?)?;
            let n = num(e, at(a, 1)?)?;
            let pmt = num(e, at(a, 2)?)?;
            let pv = if a.len() > 3 { num(e, at(a, 3)?)? } else { 0.0 };
            let when = if a.len() > 4 { num(e, at(a, 4)?)? } else { 0.0 };
            if rate == 0.0 {
                return Ok(Value::Number(-(pv + pmt * n)));
            }
            let f = (1.0 + rate).powf(n);
            Ok(Value::Number(
                -(pv * f + pmt * (1.0 + rate * when) * (f - 1.0) / rate),
            ))
        }
    ),
    f!(
        "PV",
        "financial",
        "PV(taxa, períodos, pagamento, [valor-futuro], [quando])",
        "The present value of an investment.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 5)?;
            let rate = num(e, at(a, 0)?)?;
            let n = num(e, at(a, 1)?)?;
            let pmt = num(e, at(a, 2)?)?;
            let fv = if a.len() > 3 { num(e, at(a, 3)?)? } else { 0.0 };
            let when = if a.len() > 4 { num(e, at(a, 4)?)? } else { 0.0 };
            if rate == 0.0 {
                return Ok(Value::Number(-(fv + pmt * n)));
            }
            let f = (1.0 + rate).powf(n);
            Ok(Value::Number(
                -(fv + pmt * (1.0 + rate * when) * (f - 1.0) / rate) / f,
            ))
        }
    ),
    f!(
        "NPV",
        "financial",
        "NPV(taxa, fluxo, ...)",
        "The net present value of cash flows.",
        |e: &Eval<'_>, a: &[Expr]| {
            if a.len() < 2 {
                return Err(ErrorKind::Value);
            }
            let rate = num(e, at(a, 0)?)?;
            let flows = numbers(e, a.get(1..).unwrap_or_default())?;
            Ok(Value::Number(
                flows
                    .iter()
                    .enumerate()
                    .map(|(i, v)| v / (1.0 + rate).powi(i as i32 + 1))
                    .sum(),
            ))
        }
    ),
    // Logical.
    f!(
        "IF",
        "logical",
        "IF(condição, se-verdadeiro, [se-falso])",
        "One value or another, as a condition is true.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 3)?;
            if boolean(e, at(a, 0)?)? {
                Ok(e.scalar(at(a, 1)?))
            } else if a.len() > 2 {
                Ok(e.scalar(at(a, 2)?))
            } else {
                Ok(Value::Bool(false))
            }
        }
    ),
    f!(
        "IFERROR",
        "logical",
        "IFERROR(valor, se-erro)",
        "A value, or another when it is an error.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            match e.scalar(at(a, 0)?) {
                Value::Error(_) => Ok(e.scalar(at(a, 1)?)),
                v => Ok(v),
            }
        }
    ),
    f!(
        "AND",
        "logical",
        "AND(expressão, ...)",
        "Whether all the conditions are true.",
        |e: &Eval<'_>, a: &[Expr]| {
            let vals = all_values(e, a);
            let mut out = true;
            for v in vals
                .iter()
                .filter(|v| !matches!(v, Value::Empty | Value::Text(_)))
            {
                out &= v.to_bool()?;
            }
            Ok(Value::Bool(out))
        }
    ),
    f!(
        "OR",
        "logical",
        "OR(expressão, ...)",
        "Whether any of the conditions is true.",
        |e: &Eval<'_>, a: &[Expr]| {
            let vals = all_values(e, a);
            let mut out = false;
            for v in vals
                .iter()
                .filter(|v| !matches!(v, Value::Empty | Value::Text(_)))
            {
                out |= v.to_bool()?;
            }
            Ok(Value::Bool(out))
        }
    ),
    f!(
        "NOT",
        "logical",
        "NOT(expressão)",
        "The opposite of a condition.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Bool(!boolean(e, at(a, 0)?)?))
        }
    ),
    f!(
        "TRUE",
        "logical",
        "TRUE()",
        "True.",
        |_e: &Eval<'_>, a: &[Expr]| {
            count(a, 0, 0)?;
            Ok(Value::Bool(true))
        }
    ),
    f!(
        "FALSE",
        "logical",
        "FALSE()",
        "False.",
        |_e: &Eval<'_>, a: &[Expr]| {
            count(a, 0, 0)?;
            Ok(Value::Bool(false))
        }
    ),
    f!(
        "ISBLANK",
        "logical",
        "ISBLANK(célula)",
        "Whether a cell is empty.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Bool(matches!(e.scalar(at(a, 0)?), Value::Empty)))
        }
    ),
    f!(
        "ISERROR",
        "logical",
        "ISERROR(valor)",
        "Whether a value is an error.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Bool(matches!(e.scalar(at(a, 0)?), Value::Error(_))))
        }
    ),
    f!(
        "ISNUMBER",
        "logical",
        "ISNUMBER(valor)",
        "Whether a value is a number.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Bool(e.scalar(at(a, 0)?).is_numeric()))
        }
    ),
    f!(
        "ISTEXT",
        "logical",
        "ISTEXT(valor)",
        "Whether a value is text.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Bool(matches!(e.scalar(at(a, 0)?), Value::Text(_))))
        }
    ),
    f!(
        "ISEVEN",
        "logical",
        "ISEVEN(número)",
        "Whether a number is even.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Bool(int(e, at(a, 0)?)? % 2 == 0))
        }
    ),
    f!(
        "ISODD",
        "logical",
        "ISODD(número)",
        "Whether a number is odd.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Bool(int(e, at(a, 0)?)? % 2 != 0))
        }
    ),
    // Numeric.
    f!(
        "ABS",
        "numeric",
        "ABS(número)",
        "The number without its sign.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.abs()))
        }
    ),
    f!(
        "CEILING",
        "numeric",
        "CEILING(número, [múltiplo])",
        "The number rounded up to a multiple.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let n = num(e, at(a, 0)?)?;
            let m = if a.len() > 1 { num(e, at(a, 1)?)? } else { 1.0 };
            if m == 0.0 {
                return Ok(Value::Number(0.0));
            }
            Ok(Value::Number((n / m - 1e-12).ceil() * m))
        }
    ),
    f!(
        "FLOOR",
        "numeric",
        "FLOOR(número, [múltiplo])",
        "The number rounded down to a multiple.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let n = num(e, at(a, 0)?)?;
            let m = if a.len() > 1 { num(e, at(a, 1)?)? } else { 1.0 };
            if m == 0.0 {
                return Err(ErrorKind::DivisionByZero);
            }
            Ok(Value::Number((n / m + 1e-12).floor() * m))
        }
    ),
    f!(
        "EVEN",
        "numeric",
        "EVEN(número)",
        "The number rounded away from 0 to an even number.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let n = num(e, at(a, 0)?)?;
            let r = (n.abs() / 2.0).ceil() * 2.0;
            Ok(Value::Number(r.copysign(n)))
        }
    ),
    f!(
        "ODD",
        "numeric",
        "ODD(número)",
        "The number rounded away from 0 to an odd number.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let n = num(e, at(a, 0)?)?;
            let r = ((n.abs() - 1.0) / 2.0).ceil().max(0.0) * 2.0 + 1.0;
            Ok(Value::Number(r.copysign(n)))
        }
    ),
    f!(
        "EXP",
        "numeric",
        "EXP(número)",
        "e raised to a number.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.exp()))
        }
    ),
    f!(
        "FACT",
        "numeric",
        "FACT(número)",
        "The factorial of a number.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let n = int(e, at(a, 0)?)?;
            if !(0..=170).contains(&n) {
                return Err(ErrorKind::Number);
            }
            Ok(Value::Number((1..=n).map(|i| i as f64).product()))
        }
    ),
    f!(
        "INT",
        "numeric",
        "INT(número)",
        "The number rounded down to a whole number.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.floor()))
        }
    ),
    f!(
        "LN",
        "numeric",
        "LN(número)",
        "The natural logarithm.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let n = num(e, at(a, 0)?)?;
            if n <= 0.0 {
                return Err(ErrorKind::Number);
            }
            Ok(Value::Number(n.ln()))
        }
    ),
    f!(
        "LOG",
        "numeric",
        "LOG(número, [base])",
        "The logarithm in a base, 10 when none is given.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let n = num(e, at(a, 0)?)?;
            let b = if a.len() > 1 {
                num(e, at(a, 1)?)?
            } else {
                10.0
            };
            if n <= 0.0 || b <= 0.0 || b == 1.0 {
                return Err(ErrorKind::Number);
            }
            Ok(Value::Number(n.log(b)))
        }
    ),
    f!(
        "LOG10",
        "numeric",
        "LOG10(número)",
        "The logarithm in base 10.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let n = num(e, at(a, 0)?)?;
            if n <= 0.0 {
                return Err(ErrorKind::Number);
            }
            Ok(Value::Number(n.log10()))
        }
    ),
    f!(
        "MOD",
        "numeric",
        "MOD(dividendo, divisor)",
        "The remainder of a division, with the sign of the divisor.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            let n = num(e, at(a, 0)?)?;
            let d = num(e, at(a, 1)?)?;
            if d == 0.0 {
                return Err(ErrorKind::DivisionByZero);
            }
            Ok(Value::Number(n - d * (n / d).floor()))
        }
    ),
    f!(
        "PI",
        "numeric",
        "PI()",
        "The number π.",
        |_e: &Eval<'_>, a: &[Expr]| {
            count(a, 0, 0)?;
            Ok(Value::Number(std::f64::consts::PI))
        }
    ),
    f!(
        "POWER",
        "numeric",
        "POWER(número, expoente)",
        "A number raised to a power.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.powf(num(e, at(a, 1)?)?)))
        }
    ),
    f!(
        "PRODUCT",
        "numeric",
        "PRODUCT(número, ...)",
        "The product of the numbers.",
        |e: &Eval<'_>, a: &[Expr]| { Ok(Value::Number(numbers(e, a)?.iter().product())) }
    ),
    f!(
        "QUOTIENT",
        "numeric",
        "QUOTIENT(dividendo, divisor)",
        "The whole part of a division.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            let d = num(e, at(a, 1)?)?;
            if d == 0.0 {
                return Err(ErrorKind::DivisionByZero);
            }
            Ok(Value::Number((num(e, at(a, 0)?)? / d).trunc()))
        }
    ),
    f!(
        "ROUND",
        "numeric",
        "ROUND(número, casas)",
        "The number rounded to some decimal places.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let d = if a.len() > 1 { int(e, at(a, 1)?)? } else { 0 };
            Ok(Value::Number(round_to(
                num(e, at(a, 0)?)?,
                d,
                round_half_away,
            )))
        }
    ),
    f!(
        "ROUNDDOWN",
        "numeric",
        "ROUNDDOWN(número, casas)",
        "The number rounded toward 0.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let d = if a.len() > 1 { int(e, at(a, 1)?)? } else { 0 };
            Ok(Value::Number(round_to(num(e, at(a, 0)?)?, d, f64::trunc)))
        }
    ),
    f!(
        "ROUNDUP",
        "numeric",
        "ROUNDUP(número, casas)",
        "The number rounded away from 0.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let d = if a.len() > 1 { int(e, at(a, 1)?)? } else { 0 };
            Ok(Value::Number(round_to(num(e, at(a, 0)?)?, d, |x| {
                x.abs().ceil().copysign(x)
            })))
        }
    ),
    f!(
        "SIGN",
        "numeric",
        "SIGN(número)",
        "1, 0 or -1, by the sign of the number.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let n = num(e, at(a, 0)?)?;
            Ok(Value::Number(if n > 0.0 {
                1.0
            } else if n < 0.0 {
                -1.0
            } else {
                0.0
            }))
        }
    ),
    f!(
        "SQRT",
        "numeric",
        "SQRT(número)",
        "The square root.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let n = num(e, at(a, 0)?)?;
            if n < 0.0 {
                return Err(ErrorKind::Number);
            }
            Ok(Value::Number(n.sqrt()))
        }
    ),
    f!(
        "SUM",
        "numeric",
        "SUM(número, ...)",
        "The sum of the numbers.",
        |e: &Eval<'_>, a: &[Expr]| {
            let v = numbers(e, a)?;
            let s: f64 = v.iter().sum();
            Ok(Value::Number(significant(s)))
        }
    ),
    f!(
        "SUMIF",
        "numeric",
        "SUMIF(valores-teste, condição, [valores-soma])",
        "The sum of the cells that meet a condition.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 3)?;
            let test = e.arg(at(a, 0)?).values();
            let crit = e.scalar(at(a, 1)?);
            let sum = if a.len() > 2 {
                e.arg(at(a, 2)?).values()
            } else {
                test.clone()
            };
            let mut s = 0.0;
            for (i, v) in test.iter().enumerate() {
                if matches(v, &crit)
                    && let Some(Value::Number(n) | Value::Date(n) | Value::Duration(n)) = sum.get(i)
                {
                    s += n;
                }
            }
            Ok(Value::Number(s))
        }
    ),
    f!(
        "SUMPRODUCT",
        "numeric",
        "SUMPRODUCT(intervalo, intervalo, ...)",
        "The sum of the products of the matching cells.",
        |e: &Eval<'_>, a: &[Expr]| {
            if a.is_empty() {
                return Err(ErrorKind::Value);
            }
            let ranges: Vec<Vec<Value>> = a.iter().map(|x| e.arg(x).values()).collect();
            let n = ranges.first().map_or(0, Vec::len);
            if ranges.iter().any(|r| r.len() != n) {
                return Err(ErrorKind::Value);
            }
            let mut s = 0.0;
            for i in 0..n {
                let mut p = 1.0;
                for r in &ranges {
                    p *= match r.get(i).ok_or(ErrorKind::Value)? {
                        Value::Number(x) | Value::Date(x) | Value::Duration(x) => *x,
                        Value::Error(err) => return Err(*err),
                        _ => 0.0,
                    };
                }
                s += p;
            }
            Ok(Value::Number(s))
        }
    ),
    f!(
        "SUMSQ",
        "numeric",
        "SUMSQ(número, ...)",
        "The sum of the squares of the numbers.",
        |e: &Eval<'_>, a: &[Expr]| {
            Ok(Value::Number(numbers(e, a)?.iter().map(|x| x * x).sum()))
        }
    ),
    f!(
        "TRUNC",
        "numeric",
        "TRUNC(número, [casas])",
        "The number cut to some decimal places.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let d = if a.len() > 1 { int(e, at(a, 1)?)? } else { 0 };
            Ok(Value::Number(round_to(num(e, at(a, 0)?)?, d, f64::trunc)))
        }
    ),
    // Statistical.
    f!(
        "AVERAGE",
        "statistical",
        "AVERAGE(número, ...)",
        "The mean of the numbers.",
        |e: &Eval<'_>, a: &[Expr]| { Ok(Value::Number(mean(&numbers(e, a)?)?)) }
    ),
    f!(
        "AVERAGEA",
        "statistical",
        "AVERAGEA(valor, ...)",
        "The mean of the values, with text as 0.",
        |e: &Eval<'_>, a: &[Expr]| { Ok(Value::Number(mean(&numbers_a(e, a)?)?)) }
    ),
    f!(
        "AVERAGEIF",
        "statistical",
        "AVERAGEIF(valores-teste, condição, [valores-média])",
        "The mean of the cells that meet a condition.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 3)?;
            let test = e.arg(at(a, 0)?).values();
            let crit = e.scalar(at(a, 1)?);
            let vals = if a.len() > 2 {
                e.arg(at(a, 2)?).values()
            } else {
                test.clone()
            };
            let picked: Vec<f64> = test
                .iter()
                .enumerate()
                .filter(|(_, v)| matches(v, &crit))
                .filter_map(|(i, _)| match vals.get(i) {
                    Some(Value::Number(n) | Value::Date(n) | Value::Duration(n)) => Some(*n),
                    _ => None,
                })
                .collect();
            Ok(Value::Number(mean(&picked)?))
        }
    ),
    f!(
        "COUNT",
        "statistical",
        "COUNT(valor, ...)",
        "How many of the values are numbers.",
        |e: &Eval<'_>, a: &[Expr]| {
            Ok(Value::Number(
                all_values(e, a).iter().filter(|v| v.is_numeric()).count() as f64,
            ))
        }
    ),
    f!(
        "COUNTA",
        "statistical",
        "COUNTA(valor, ...)",
        "How many of the values are not empty.",
        |e: &Eval<'_>, a: &[Expr]| {
            Ok(Value::Number(
                all_values(e, a)
                    .iter()
                    .filter(|v| !matches!(v, Value::Empty))
                    .count() as f64,
            ))
        }
    ),
    f!(
        "COUNTBLANK",
        "statistical",
        "COUNTBLANK(intervalo)",
        "How many cells are empty.",
        |e: &Eval<'_>, a: &[Expr]| {
            Ok(Value::Number(
                all_values(e, a)
                    .iter()
                    .filter(|v| matches!(v, Value::Empty))
                    .count() as f64,
            ))
        }
    ),
    f!(
        "COUNTIF",
        "statistical",
        "COUNTIF(valores-teste, condição)",
        "How many cells meet a condition.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            let crit = e.scalar(at(a, 1)?);
            Ok(Value::Number(
                e.arg(at(a, 0)?)
                    .values()
                    .iter()
                    .filter(|v| matches(v, &crit))
                    .count() as f64,
            ))
        }
    ),
    f!(
        "LARGE",
        "statistical",
        "LARGE(intervalo, posição)",
        "The nth largest number.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            let mut v = numbers(e, a.get(..1).ok_or(ErrorKind::Value)?)?;
            let k = int(e, at(a, 1)?)?;
            v.sort_by(|x, y| y.partial_cmp(x).unwrap_or(Ordering::Equal));
            usize::try_from(k - 1)
                .ok()
                .and_then(|i| v.get(i))
                .map(|n| Value::Number(*n))
                .ok_or(ErrorKind::Number)
        }
    ),
    f!(
        "SMALL",
        "statistical",
        "SMALL(intervalo, posição)",
        "The nth smallest number.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            let mut v = numbers(e, a.get(..1).ok_or(ErrorKind::Value)?)?;
            let k = int(e, at(a, 1)?)?;
            v.sort_by(|x, y| x.partial_cmp(y).unwrap_or(Ordering::Equal));
            usize::try_from(k - 1)
                .ok()
                .and_then(|i| v.get(i))
                .map(|n| Value::Number(*n))
                .ok_or(ErrorKind::Number)
        }
    ),
    f!(
        "MAX",
        "statistical",
        "MAX(número, ...)",
        "The largest number.",
        |e: &Eval<'_>, a: &[Expr]| {
            let v = numbers(e, a)?;
            Ok(Value::Number(if v.is_empty() {
                0.0
            } else {
                v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            }))
        }
    ),
    f!(
        "MAXA",
        "statistical",
        "MAXA(valor, ...)",
        "The largest value, with text as 0.",
        |e: &Eval<'_>, a: &[Expr]| {
            let v = numbers_a(e, a)?;
            Ok(Value::Number(if v.is_empty() {
                0.0
            } else {
                v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            }))
        }
    ),
    f!(
        "MIN",
        "statistical",
        "MIN(número, ...)",
        "The smallest number.",
        |e: &Eval<'_>, a: &[Expr]| {
            let v = numbers(e, a)?;
            Ok(Value::Number(if v.is_empty() {
                0.0
            } else {
                v.iter().copied().fold(f64::INFINITY, f64::min)
            }))
        }
    ),
    f!(
        "MINA",
        "statistical",
        "MINA(valor, ...)",
        "The smallest value, with text as 0.",
        |e: &Eval<'_>, a: &[Expr]| {
            let v = numbers_a(e, a)?;
            Ok(Value::Number(if v.is_empty() {
                0.0
            } else {
                v.iter().copied().fold(f64::INFINITY, f64::min)
            }))
        }
    ),
    f!(
        "MEDIAN",
        "statistical",
        "MEDIAN(número, ...)",
        "The middle number.",
        |e: &Eval<'_>, a: &[Expr]| {
            let mut v = numbers(e, a)?;
            if v.is_empty() {
                return Err(ErrorKind::Number);
            }
            v.sort_by(|x, y| x.partial_cmp(y).unwrap_or(Ordering::Equal));
            let (low, high) = v.split_at(v.len() / 2);
            let middle = high.first().copied().ok_or(ErrorKind::Number)?;
            Ok(Value::Number(match low.last() {
                Some(below) if v.len() % 2 == 0 => (below + middle) / 2.0,
                _ => middle,
            }))
        }
    ),
    f!(
        "MODE",
        "statistical",
        "MODE(número, ...)",
        "The number that comes most often.",
        |e: &Eval<'_>, a: &[Expr]| {
            let v = numbers(e, a)?;
            let mut best: Option<(f64, usize)> = None;
            for x in &v {
                let c = v.iter().filter(|y| *y == x).count();
                if c > 1 && best.is_none_or(|(_, bc)| c > bc) {
                    best = Some((*x, c));
                }
            }
            best.map(|(x, _)| Value::Number(x))
                .ok_or(ErrorKind::NotAvailable)
        }
    ),
    f!(
        "RANK",
        "statistical",
        "RANK(número, intervalo, [ordem])",
        "The position of a number in a list.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 3)?;
            let n = num(e, at(a, 0)?)?;
            let v = numbers(e, a.get(1..2).ok_or(ErrorKind::Value)?)?;
            let ascending = a.len() > 2 && boolean(e, at(a, 2)?)?;
            if !v.contains(&n) {
                return Err(ErrorKind::NotAvailable);
            }
            let above = v
                .iter()
                .filter(|x| if ascending { **x < n } else { **x > n })
                .count();
            Ok(Value::Number((above + 1) as f64))
        }
    ),
    f!(
        "STDEV",
        "statistical",
        "STDEV(número, ...)",
        "The standard deviation of a sample.",
        |e: &Eval<'_>, a: &[Expr]| { Ok(Value::Number(variance(&numbers(e, a)?, true)?.sqrt())) }
    ),
    f!(
        "STDEVP",
        "statistical",
        "STDEVP(número, ...)",
        "The standard deviation of a population.",
        |e: &Eval<'_>, a: &[Expr]| { Ok(Value::Number(variance(&numbers(e, a)?, false)?.sqrt())) }
    ),
    f!(
        "VAR",
        "statistical",
        "VAR(número, ...)",
        "The variance of a sample.",
        |e: &Eval<'_>, a: &[Expr]| { Ok(Value::Number(variance(&numbers(e, a)?, true)?)) }
    ),
    f!(
        "VARP",
        "statistical",
        "VARP(número, ...)",
        "The variance of a population.",
        |e: &Eval<'_>, a: &[Expr]| { Ok(Value::Number(variance(&numbers(e, a)?, false)?)) }
    ),
    // Lookup.
    f!(
        "CHOOSE",
        "lookup",
        "CHOOSE(índice, valor, ...)",
        "The value at a position in a list.",
        |e: &Eval<'_>, a: &[Expr]| {
            if a.len() < 2 {
                return Err(ErrorKind::Value);
            }
            let i = int(e, at(a, 0)?)?;
            usize::try_from(i)
                .ok()
                .filter(|i| *i >= 1)
                .and_then(|i| a.get(i))
                .map(|x| e.scalar(x))
                .ok_or(ErrorKind::Value)
        }
    ),
    f!(
        "INDEX",
        "lookup",
        "INDEX(intervalo, linha, [coluna])",
        "The value at a row and a column of a range.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 3)?;
            let r = e.arg(at(a, 0)?);
            let (rows, cols) = r.shape();
            let mut row = int(e, at(a, 1)?)?;
            let mut col = if a.len() > 2 { int(e, at(a, 2)?)? } else { 1 };
            if rows == 1 && a.len() == 2 {
                col = row;
                row = 1;
            }
            if row < 1 || col < 1 || row as usize > rows || col as usize > cols {
                return Err(ErrorKind::Reference);
            }
            Ok(r.at(row as usize - 1, col as usize - 1))
        }
    ),
    f!(
        "MATCH",
        "lookup",
        "MATCH(valor, intervalo, [tipo])",
        "The position of a value in a range.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 3)?;
            let needle = e.scalar(at(a, 0)?);
            let hay = e.arg(at(a, 1)?).values();
            let kind = if a.len() > 2 { int(e, at(a, 2)?)? } else { 1 };
            let pos = match kind {
                0 => lookup_position(&needle, &hay, true),
                -1 => hay
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| compare(v, &needle) != Ordering::Less)
                    .map(|(i, _)| i)
                    .next_back(),
                _ => lookup_position(&needle, &hay, false),
            };
            pos.map(|p| Value::Number((p + 1) as f64))
                .ok_or(ErrorKind::NotAvailable)
        }
    ),
    f!(
        "VLOOKUP",
        "lookup",
        "VLOOKUP(valor, intervalo, coluna, [aproximado])",
        "A value from a column, in the row where the first column has a value.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 4)?;
            let needle = e.scalar(at(a, 0)?);
            let r = e.arg(at(a, 1)?);
            let col = int(e, at(a, 2)?)?;
            let approx = a.len() < 4 || boolean(e, at(a, 3)?)?;
            let (rows, cols) = r.shape();
            if col < 1 || col as usize > cols {
                return Err(ErrorKind::Reference);
            }
            let first: Vec<Value> = (0..rows).map(|i| r.at(i, 0)).collect();
            let row = lookup_position(&needle, &first, !approx).ok_or(ErrorKind::NotAvailable)?;
            Ok(r.at(row, col as usize - 1))
        }
    ),
    f!(
        "HLOOKUP",
        "lookup",
        "HLOOKUP(valor, intervalo, linha, [aproximado])",
        "A value from a row, in the column where the first row has a value.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 4)?;
            let needle = e.scalar(at(a, 0)?);
            let r = e.arg(at(a, 1)?);
            let row = int(e, at(a, 2)?)?;
            let approx = a.len() < 4 || boolean(e, at(a, 3)?)?;
            let (rows, cols) = r.shape();
            if row < 1 || row as usize > rows {
                return Err(ErrorKind::Reference);
            }
            let first: Vec<Value> = (0..cols).map(|i| r.at(0, i)).collect();
            let col = lookup_position(&needle, &first, !approx).ok_or(ErrorKind::NotAvailable)?;
            Ok(r.at(row as usize - 1, col))
        }
    ),
    f!(
        "ROWS",
        "lookup",
        "ROWS(intervalo)",
        "How many rows a range has.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(e.arg(at(a, 0)?).shape().0 as f64))
        }
    ),
    f!(
        "COLUMNS",
        "lookup",
        "COLUMNS(intervalo)",
        "How many columns a range has.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(e.arg(at(a, 0)?).shape().1 as f64))
        }
    ),
    f!(
        "ROW",
        "lookup",
        "ROW(célula)",
        "The number of the row of a cell.",
        |_e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            match at(a, 0)? {
                Expr::Cell(c) | Expr::Range(c, _) => Ok(Value::Number(f64::from(c.row + 1))),
                _ => Err(ErrorKind::Value),
            }
        }
    ),
    f!(
        "COLUMN",
        "lookup",
        "COLUMN(célula)",
        "The number of the column of a cell.",
        |_e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            match at(a, 0)? {
                Expr::Cell(c) | Expr::Range(c, _) => Ok(Value::Number(f64::from(c.col + 1))),
                _ => Err(ErrorKind::Value),
            }
        }
    ),
    // Text.
    f!(
        "CHAR",
        "text",
        "CHAR(código)",
        "The character of a code.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let c = u32::try_from(int(e, at(a, 0)?)?)
                .ok()
                .and_then(char::from_u32)
                .ok_or(ErrorKind::Value)?;
            Ok(Value::Text(c.to_string()))
        }
    ),
    f!(
        "CODE",
        "text",
        "CODE(texto)",
        "The code of the first character.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let c = text(e, at(a, 0)?)?.chars().next().ok_or(ErrorKind::Value)?;
            Ok(Value::Number(f64::from(u32::from(c))))
        }
    ),
    f!(
        "CLEAN",
        "text",
        "CLEAN(texto)",
        "The text without the characters that do not print.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Text(
                text(e, at(a, 0)?)?
                    .chars()
                    .filter(|c| !c.is_control())
                    .collect(),
            ))
        }
    ),
    f!(
        "CONCATENATE",
        "text",
        "CONCATENATE(texto, ...)",
        "The texts joined.",
        |e: &Eval<'_>, a: &[Expr]| {
            let mut s = String::new();
            for x in a {
                s += &text(e, x)?;
                if s.len() > MAX_TEXT {
                    return Err(ErrorKind::Value);
                }
            }
            Ok(Value::Text(s))
        }
    ),
    f!(
        "EXACT",
        "text",
        "EXACT(texto, texto)",
        "Whether two texts are the same, in the same case.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            Ok(Value::Bool(text(e, at(a, 0)?)? == text(e, at(a, 1)?)?))
        }
    ),
    f!(
        "FIND",
        "text",
        "FIND(procurar, texto, [início])",
        "Where a text starts in another, in the same case.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 3)?;
            find(
                &text(e, at(a, 0)?)?,
                &text(e, at(a, 1)?)?,
                if a.len() > 2 { int(e, at(a, 2)?)? } else { 1 },
                false,
            )
        }
    ),
    f!(
        "SEARCH",
        "text",
        "SEARCH(procurar, texto, [início])",
        "Where a text starts in another, in any case.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 3)?;
            find(
                &text(e, at(a, 0)?)?,
                &text(e, at(a, 1)?)?,
                if a.len() > 2 { int(e, at(a, 2)?)? } else { 1 },
                true,
            )
        }
    ),
    f!(
        "FIXED",
        "text",
        "FIXED(número, [casas], [sem-separador])",
        "The number as text with some decimal places.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 3)?;
            let d = if a.len() > 1 {
                int(e, at(a, 1)?)?.clamp(0, 15) as usize
            } else {
                2
            };
            let plain = a.len() > 2 && boolean(e, at(a, 2)?)?;
            let s = format!("{:.*}", d, num(e, at(a, 0)?)?);
            Ok(Value::Text(if plain { s } else { group_thousands(&s) }))
        }
    ),
    f!(
        "LEFT",
        "text",
        "LEFT(texto, [quantos])",
        "The first characters of a text.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let n = if a.len() > 1 { int(e, at(a, 1)?)? } else { 1 };
            let n = usize::try_from(n).map_err(|_| ErrorKind::Value)?;
            Ok(Value::Text(text(e, at(a, 0)?)?.chars().take(n).collect()))
        }
    ),
    f!(
        "RIGHT",
        "text",
        "RIGHT(texto, [quantos])",
        "The last characters of a text.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 2)?;
            let n = if a.len() > 1 { int(e, at(a, 1)?)? } else { 1 };
            let n = usize::try_from(n).map_err(|_| ErrorKind::Value)?;
            let t: Vec<char> = text(e, at(a, 0)?)?.chars().collect();
            let from = t.len().saturating_sub(n);
            Ok(Value::Text(t.iter().skip(from).collect()))
        }
    ),
    f!(
        "MID",
        "text",
        "MID(texto, início, quantos)",
        "Some characters from the middle of a text.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 3)?;
            let start = usize::try_from(int(e, at(a, 1)?)? - 1).map_err(|_| ErrorKind::Value)?;
            let n = usize::try_from(int(e, at(a, 2)?)?).map_err(|_| ErrorKind::Value)?;
            Ok(Value::Text(
                text(e, at(a, 0)?)?.chars().skip(start).take(n).collect(),
            ))
        }
    ),
    f!(
        "LEN",
        "text",
        "LEN(texto)",
        "How many characters a text has.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(text(e, at(a, 0)?)?.chars().count() as f64))
        }
    ),
    f!(
        "LOWER",
        "text",
        "LOWER(texto)",
        "The text in lower case.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Text(text(e, at(a, 0)?)?.to_lowercase()))
        }
    ),
    f!(
        "UPPER",
        "text",
        "UPPER(texto)",
        "The text in capitals.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Text(text(e, at(a, 0)?)?.to_uppercase()))
        }
    ),
    f!(
        "PROPER",
        "text",
        "PROPER(texto)",
        "The text with the first letter of each word in capitals.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            let mut out = String::new();
            let mut start = true;
            for c in text(e, at(a, 0)?)?.chars() {
                if c.is_alphabetic() {
                    if start {
                        out.extend(c.to_uppercase());
                    } else {
                        out.extend(c.to_lowercase());
                    }
                    start = false;
                } else {
                    out.push(c);
                    start = true;
                }
            }
            Ok(Value::Text(out))
        }
    ),
    f!(
        "REPT",
        "text",
        "REPT(texto, vezes)",
        "The text repeated.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 2, 2)?;
            let n = usize::try_from(int(e, at(a, 1)?)?).map_err(|_| ErrorKind::Value)?;
            let t = text(e, at(a, 0)?)?;
            if t.len().saturating_mul(n) > MAX_TEXT {
                return Err(ErrorKind::Value);
            }
            Ok(Value::Text(t.repeat(n)))
        }
    ),
    f!(
        "SUBSTITUTE",
        "text",
        "SUBSTITUTE(texto, procurar, substituir, [ocorrência])",
        "The text with a part replaced.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 3, 4)?;
            let t = text(e, at(a, 0)?)?;
            let old = text(e, at(a, 1)?)?;
            let new = text(e, at(a, 2)?)?;
            if old.is_empty() {
                return Ok(Value::Text(t));
            }
            // A long text with a long replacement for each character
            // would be gigabytes: its length is known first.
            if new.len() > old.len() {
                let times = if a.len() > 3 {
                    1
                } else {
                    t.matches(&old).count()
                };
                let len = t.len() + times.saturating_mul(new.len() - old.len());
                if len > MAX_TEXT {
                    return Err(ErrorKind::Value);
                }
            }
            if a.len() > 3 {
                let k = usize::try_from(int(e, at(a, 3)?)?).map_err(|_| ErrorKind::Value)?;
                if let Some((i, _)) = t.match_indices(&old).nth(k.saturating_sub(1)) {
                    return Ok(Value::Text(format!(
                        "{}{}{}",
                        &t[..i],
                        new,
                        &t[i + old.len()..]
                    )));
                }
                return Ok(Value::Text(t));
            }
            Ok(Value::Text(t.replace(&old, &new)))
        }
    ),
    f!(
        "TRIM",
        "text",
        "TRIM(texto)",
        "The text without extra spaces.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Text(
                text(e, at(a, 0)?)?
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            ))
        }
    ),
    f!(
        "VALUE",
        "text",
        "VALUE(texto)",
        "The number that a text writes.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(Value::Text(text(e, at(a, 0)?)?).to_number()?))
        }
    ),
    // Trigonometric.
    f!(
        "SIN",
        "trigonometric",
        "SIN(ângulo)",
        "The sine of an angle in radians.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.sin()))
        }
    ),
    f!(
        "COS",
        "trigonometric",
        "COS(ângulo)",
        "The cosine of an angle in radians.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.cos()))
        }
    ),
    f!(
        "TAN",
        "trigonometric",
        "TAN(ângulo)",
        "The tangent of an angle in radians.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.tan()))
        }
    ),
    f!(
        "ATAN",
        "trigonometric",
        "ATAN(número)",
        "The angle of a tangent, in radians.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.atan()))
        }
    ),
    f!(
        "DEGREES",
        "trigonometric",
        "DEGREES(radianos)",
        "An angle in degrees.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.to_degrees()))
        }
    ),
    f!(
        "RADIANS",
        "trigonometric",
        "RADIANS(graus)",
        "An angle in radians.",
        |e: &Eval<'_>, a: &[Expr]| {
            count(a, 1, 1)?;
            Ok(Value::Number(num(e, at(a, 0)?)?.to_radians()))
        }
    ),
];

fn find(needle: &str, hay: &str, start: i64, any_case: bool) -> Result<Value> {
    let (needle, hay) = if any_case {
        (needle.to_lowercase(), hay.to_lowercase())
    } else {
        (needle.to_owned(), hay.to_owned())
    };
    let start = usize::try_from(start - 1).map_err(|_| ErrorKind::Value)?;
    let chars: Vec<(usize, char)> = hay.char_indices().collect();
    let byte_start = match chars.get(start) {
        Some(&(b, _)) => b,
        None if start == chars.len() => hay.len(),
        None => return Err(ErrorKind::Value),
    };
    let found = if any_case && (needle.contains('*') || needle.contains('?')) {
        // Some text from i matches when the pattern, then any text, matches
        // all that follows i.
        let pattern: Vec<char> = needle.chars().chain(['*']).collect();
        let text: Vec<char> = chars.iter().map(|c| c.1).collect();
        (start..text.len()).find(|&i| {
            text.get(i..)
                .is_some_and(|rest| wildcard_chars(&pattern, rest))
        })
    } else {
        hay[byte_start..]
            .find(&needle)
            .map(|b| hay[..byte_start + b].chars().count())
    };
    found
        .map(|i| Value::Number((i + 1) as f64))
        .ok_or(ErrorKind::Value)
}

/// A number written with commas between the thousands.
fn group_thousands(s: &str) -> String {
    let (sign, rest) = s.strip_prefix('-').map_or(("", s), |r| ("-", r));
    let (whole, frac) = rest
        .split_once('.')
        .map_or((rest, None), |(w, f)| (w, Some(f)));
    let mut out = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    let mut s = format!("{sign}{out}");
    if let Some(f) = frac {
        s.push('.');
        s.push_str(f);
    }
    s
}

#[cfg(test)]
mod tests {
    use crate::eval::tests::{Sheet, run};
    use crate::value::{ErrorKind, Value};

    fn sheet() -> Sheet {
        let mut s = Sheet::default();
        for (i, v) in [3.0, 1.0, 4.0, 1.0, 5.0].iter().enumerate() {
            s.set(&format!("A{}", i + 1), Value::Number(*v));
        }
        s.set("B1", Value::Text("maçã".into()));
        s.set("B2", Value::Text("banana".into()));
        s.set("B3", Value::Text("Maçã".into()));
        s
    }

    #[test]
    fn tab_32_statistics() {
        let s = sheet();
        assert_eq!(run(&s, "=SUM(A1:A5)"), Value::Number(14.0));
        assert_eq!(run(&s, "=AVERAGE(A1:A5)"), Value::Number(2.8));
        assert_eq!(run(&s, "=MAX(A1:A5)"), Value::Number(5.0));
        assert_eq!(run(&s, "=MIN(A1:A5)"), Value::Number(1.0));
        assert_eq!(run(&s, "=MEDIAN(A1:A5)"), Value::Number(3.0));
        assert_eq!(run(&s, "=COUNT(A1:B5)"), Value::Number(5.0));
        assert_eq!(run(&s, "=COUNTA(A1:B5)"), Value::Number(8.0));
        assert_eq!(run(&s, "=COUNTIF(A1:A5,\">2\")"), Value::Number(3.0));
        assert_eq!(run(&s, "=COUNTIF(B1:B5,\"maçã\")"), Value::Number(2.0));
        assert_eq!(run(&s, "=COUNTIF(B1:B5,\"b*\")"), Value::Number(1.0));
        assert_eq!(run(&s, "=SUMIF(A1:A5,1)"), Value::Number(2.0));
        assert_eq!(run(&s, "=MODE(A1:A5)"), Value::Number(1.0));
        assert_eq!(run(&s, "=LARGE(A1:A5,2)"), Value::Number(4.0));
        assert_eq!(run(&s, "=RANK(4,A1:A5)"), Value::Number(2.0));
        assert_eq!(run(&s, "=SUM(0.1,0.2)"), Value::Number(0.3));
        assert_eq!(
            run(&s, "=AVERAGE(B1:B2)"),
            Value::Error(ErrorKind::DivisionByZero)
        );
    }

    #[test]
    fn tab_32_text_logic_and_lookup() {
        let s = sheet();
        assert_eq!(
            run(&s, "=IF(A1>2,\"sim\",\"não\")"),
            Value::Text("sim".into())
        );
        assert_eq!(run(&s, "=IFERROR(1/0,7)"), Value::Number(7.0));
        assert_eq!(run(&s, "=AND(TRUE,A1>1)"), Value::Bool(true));
        assert_eq!(
            run(&s, "=CONCATENATE(\"a\",1,\"b\")"),
            Value::Text("a1b".into())
        );
        assert_eq!(run(&s, "=LEFT(\"maçã\",3)"), Value::Text("maç".into()));
        assert_eq!(run(&s, "=MID(\"abcdef\",2,3)"), Value::Text("bcd".into()));
        assert_eq!(run(&s, "=FIND(\"c\",\"abcabc\",4)"), Value::Number(6.0));
        assert_eq!(run(&s, "=SEARCH(\"B\",\"abc\")"), Value::Number(2.0));
        assert_eq!(
            run(&s, "=PROPER(\"olá mundo\")"),
            Value::Text("Olá Mundo".into())
        );
        assert_eq!(run(&s, "=TRIM(\"  a   b \")"), Value::Text("a b".into()));
        assert_eq!(run(&s, "=FIXED(1234.567,1)"), Value::Text("1,234.6".into()));
        assert_eq!(run(&s, "=INDEX(A1:A5,3)"), Value::Number(4.0));
        assert_eq!(run(&s, "=MATCH(4,A1:A5,0)"), Value::Number(3.0));
        assert_eq!(
            run(&s, "=VLOOKUP(\"banana\",B1:B3,1,FALSE)"),
            Value::Text("banana".into())
        );
        assert_eq!(run(&s, "=ROUND(2.675,2)"), Value::Number(2.68));
        assert_eq!(run(&s, "=ROUND(-2.5,0)"), Value::Number(-3.0));
        assert_eq!(run(&s, "=MOD(-3,2)"), Value::Number(1.0));
        assert_eq!(run(&s, "=CEILING(2.1,0.5)"), Value::Number(2.5));
    }

    #[test]
    fn search_with_wildcards_finds_the_first_match_quickly() {
        let s = sheet();
        assert_eq!(run(&s, "=SEARCH(\"b?d\",\"abcdbxd\")"), Value::Number(2.0));
        assert_eq!(run(&s, "=SEARCH(\"c*x\",\"abcdbxd\")"), Value::Number(3.0));
        assert_eq!(run(&s, "=SEARCH(\"*\",\"abc\",2)"), Value::Number(2.0));
        assert_eq!(
            run(&s, "=SEARCH(\"z*\",\"abc\")"),
            Value::Error(ErrorKind::Value)
        );
        // Text of a few thousand characters took minutes when every start
        // tried every length.
        let long = "a".repeat(4000);
        let formula = format!("=SEARCH(\"*b\",\"{long}\")");
        assert_eq!(run(&s, &formula), Value::Error(ErrorKind::Value));
    }

    #[test]
    fn median_and_right_take_the_middle_and_the_end() {
        let s = sheet();
        assert_eq!(run(&s, "=MEDIAN(3,1,2)"), Value::Number(2.0));
        assert_eq!(run(&s, "=MEDIAN(4,1,2,3)"), Value::Number(2.5));
        assert_eq!(run(&s, "=MEDIAN(7)"), Value::Number(7.0));
        assert_eq!(run(&s, "=RIGHT(\"maçã\",2)"), Value::Text("çã".into()));
        assert_eq!(run(&s, "=RIGHT(\"ab\",5)"), Value::Text("ab".into()));
        assert_eq!(run(&s, "=SUMPRODUCT(A1:A2,A1:A2)"), run(&s, "=A1*A1+A2*A2"));
        assert_eq!(run(&s, "=CHOOSE(2,\"a\",\"b\")"), Value::Text("b".into()));
        assert_eq!(
            run(&s, "=CHOOSE(3,\"a\",\"b\")"),
            Value::Error(ErrorKind::Value)
        );
    }

    #[test]
    fn tab_32_dates_and_money() {
        let s = sheet();
        assert_eq!(run(&s, "=DATE(2009,1,6)"), Value::Date(39_819.0));
        assert_eq!(run(&s, "=YEAR(DATE(2009,1,6))"), Value::Number(2009.0));
        assert_eq!(run(&s, "=WEEKDAY(DATE(2009,1,6))"), Value::Number(3.0));
        assert_eq!(
            run(&s, "=DATE(2009,1,6)-DATE(2009,1,1)"),
            Value::Duration(5.0)
        );
        assert_eq!(run(&s, "=DATE(2009,1,6)+1"), Value::Date(39_820.0));
        assert_eq!(
            run(&s, "=DATEDIF(DATE(2000,3,15),DATE(2009,1,6),\"Y\")"),
            Value::Number(8.0)
        );
        assert_eq!(run(&s, "=EDATE(DATE(2009,1,31),1)"), Value::Date(39_872.0));
        assert_eq!(run(&s, "=HOUR(TIME(13,30,0))"), Value::Number(13.0));
        let Value::Number(p) = run(&s, "=PMT(0.01,12,1000)") else {
            panic!()
        };
        assert!((p + 88.848_788_6).abs() < 1e-6);
    }

    #[test]
    fn rounding_keeps_the_digits_of_the_number() {
        // Rounding to nine decimals first carried numbers close to an
        // integer over it, the product overflowed for large numbers, and
        // SUM lost totals under 1E-12 (review RV, batch 2).
        let s = Sheet::default();
        assert_eq!(run(&s, "=TRUNC(1.9999999999)"), Value::Number(1.0));
        assert_eq!(run(&s, "=ROUNDUP(2.0000000001,0)"), Value::Number(3.0));
        assert_eq!(run(&s, "=ROUNDDOWN(-1.9999999999,0)"), Value::Number(-1.0));
        assert_eq!(run(&s, "=ROUND(1E300,0)"), Value::Number(1e300));
        assert_eq!(run(&s, "=ROUND(1E300,2)"), Value::Number(1e300));
        assert_eq!(run(&s, "=SUM(1E-13)"), Value::Number(1e-13));
        assert_eq!(run(&s, "=ROUND(2.675,2)"), Value::Number(2.68));
        assert_eq!(run(&s, "=ROUND(-2.5,0)"), Value::Number(-3.0));
        assert_eq!(run(&s, "=ROUNDUP(0.1*3,1)"), Value::Number(0.3));
    }
}
