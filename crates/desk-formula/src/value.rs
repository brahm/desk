// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The values that cells hold and formulas give.

use std::fmt;

/// A value of a cell or the result of a formula.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    /// Nothing: an empty cell.
    #[default]
    Empty,
    /// A number.
    Number(f64),
    /// Text.
    Text(String),
    /// True or false.
    Bool(bool),
    /// A date and time, in days from 30 December 1899, with the time of
    /// day as the fraction, as spreadsheets count them.
    Date(f64),
    /// A length of time, in days.
    Duration(f64),
    /// An error.
    Error(ErrorKind),
}

/// What went wrong in a formula, as a cell shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// A division by zero: `#DIV/0!`.
    DivisionByZero,
    /// A value of the wrong type, as text where a number goes: `#VALUE!`.
    Value,
    /// A reference to a cell that is not there: `#REF!`.
    Reference,
    /// A name that is neither a function nor a header: `#NAME?`.
    Name,
    /// A number out of range, as the square root of -1: `#NUM!`.
    Number,
    /// A value that was looked for and not found: `#N/A`.
    NotAvailable,
    /// A formula that depends on itself.
    Circular,
    /// A formula that could not be read.
    Syntax,
}

impl ErrorKind {
    /// The error as a cell shows it.
    pub fn code(self) -> &'static str {
        match self {
            Self::DivisionByZero => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Reference => "#REF!",
            Self::Name => "#NAME?",
            Self::Number => "#NUM!",
            Self::NotAvailable => "#N/A",
            Self::Circular => "#CIRC!",
            Self::Syntax => "#ERROR!",
        }
    }

    /// The error from its code, as [`ErrorKind::code`] writes it.
    pub fn from_code(code: &str) -> Option<Self> {
        Some(match code.to_ascii_uppercase().as_str() {
            "#DIV/0!" => Self::DivisionByZero,
            "#VALUE!" => Self::Value,
            "#REF!" => Self::Reference,
            "#NAME?" => Self::Name,
            "#NUM!" => Self::Number,
            "#N/A" => Self::NotAvailable,
            "#CIRC!" => Self::Circular,
            "#ERROR!" => Self::Syntax,
            _ => return None,
        })
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl Value {
    /// The value as a number, as arithmetic takes it: empty is 0, true is
    /// 1, and text that reads as a number is that number.
    pub fn to_number(&self) -> Result<f64, ErrorKind> {
        match self {
            Self::Empty => Ok(0.0),
            Self::Number(n) | Self::Date(n) | Self::Duration(n) => Ok(*n),
            Self::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Self::Text(t) => {
                let t = t.trim();
                if t.is_empty() {
                    return Ok(0.0);
                }
                if let Some(p) = t.strip_suffix('%') {
                    return p
                        .trim()
                        .parse::<f64>()
                        .map(|n| n / 100.0)
                        .map_err(|_| ErrorKind::Value);
                }
                t.parse::<f64>().map_err(|_| ErrorKind::Value)
            }
            Self::Error(e) => Err(*e),
        }
    }

    /// The value as true or false: numbers other than 0 are true.
    pub fn to_bool(&self) -> Result<bool, ErrorKind> {
        match self {
            Self::Empty => Ok(false),
            Self::Bool(b) => Ok(*b),
            Self::Number(n) | Self::Date(n) | Self::Duration(n) => Ok(*n != 0.0),
            Self::Text(t) => match t.trim().to_ascii_uppercase().as_str() {
                "TRUE" | "VERDADEIRO" => Ok(true),
                "FALSE" | "FALSO" => Ok(false),
                _ => Err(ErrorKind::Value),
            },
            Self::Error(e) => Err(*e),
        }
    }

    /// The value as text, as `&` joins it: numbers in their shortest
    /// form.
    pub fn to_text(&self) -> Result<String, ErrorKind> {
        match self {
            Self::Empty => Ok(String::new()),
            Self::Number(n) | Self::Duration(n) => Ok(format_number(*n)),
            Self::Date(n) => {
                let (y, m, d) = date_from_days(n.floor() as i64);
                Ok(format!("{d:02}/{m:02}/{y:04}"))
            }
            Self::Text(t) => Ok(t.clone()),
            Self::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.to_owned()),
            Self::Error(e) => Err(*e),
        }
    }

    /// Whether the value is a number, a date or a duration.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Number(_) | Self::Date(_) | Self::Duration(_))
    }
}

/// A number in its shortest form, without a trailing `.0`, with at most
/// 15 significant digits as spreadsheets keep.
pub(crate) fn format_number(n: f64) -> String {
    if n.is_nan() {
        return ErrorKind::Number.code().to_owned();
    }
    if n == n.trunc() && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let s = format!("{:.15}", n);
    let precise: f64 = format!("{:.*e}", 14, n).parse().unwrap_or(n);
    let mut out = format!("{precise}");
    if out.len() > s.len() {
        out = s.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    out
}

/// A number as a formula writes it: in full, so that it reads back the
/// same. Very small and very large numbers take an exponent; the 15
/// digits of [`format_number`] are for showing a value, and a formula
/// written back with them lost a small constant, as `1E-16` became `0`.
pub(crate) fn formula_number(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    if n.is_finite() && (1e-5..1e15).contains(&n.abs()) {
        format!("{n}")
    } else {
        format!("{n:E}")
    }
}

/// The year, month and day of a day counted from 30 December 1899.
pub fn date_from_days(days: i64) -> (i32, u32, u32) {
    // Far enough for any year an i32 holds, and short of overflow below.
    let days = days.clamp(-(1 << 39), 1 << 39);
    // Days from 1 March of year 0, by the algorithm of Howard Hinnant.
    let z = days - 25_569 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// The day counted from 30 December 1899 of a year, month and day. A
/// month past 12 or a day past the end of the month goes on into the next.
pub fn days_from_date(year: i32, month: i32, day: i32) -> i64 {
    let total_months = i64::from(year) * 12 + i64::from(month) - 1;
    let y = total_months.div_euclid(12);
    let m = total_months.rem_euclid(12) + 1;
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468 + 25_569 + i64::from(day) - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_far_from_any_year_do_not_overflow() {
        // YEAR(1E300) reached here as i64::MAX, and the sum overflowed.
        let (y, m, d) = date_from_days(i64::MAX);
        assert!(y > 1_000_000_000 && (1..=12).contains(&m) && (1..=31).contains(&d));
        assert!(date_from_days(i64::MIN).0 < -1_000_000_000);
    }

    #[test]
    fn dates_go_both_ways() {
        assert_eq!(days_from_date(1899, 12, 30), 0);
        assert_eq!(days_from_date(1900, 1, 1), 2);
        assert_eq!(days_from_date(2009, 1, 6), 39_819);
        assert_eq!(date_from_days(39_819), (2009, 1, 6));
        assert_eq!(days_from_date(2024, 13, 1), days_from_date(2025, 1, 1));
        for d in [-1000, 0, 1, 59, 60, 61, 40_000, 80_000] {
            let (y, m, day) = date_from_days(d);
            assert_eq!(days_from_date(y, m as i32, day as i32), d);
        }
    }

    #[test]
    fn numbers_print_short() {
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(0.1 + 0.2), "0.3");
        assert_eq!(format_number(-2.5), "-2.5");
    }
}
