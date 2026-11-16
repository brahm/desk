// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Conditional formatting: rules that change how a cell looks by its
//! value (TAB-21).

use desk_formula::{CellRef, Value};

use crate::cell::{Color, Num};

/// What a rule compares the value of a cell with.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Operand {
    /// A number.
    Number(Num),
    /// Text.
    Text(String),
    /// The value of a cell of the same table.
    Cell(#[serde(with = "cell_ref")] CellRef),
}

/// A cell reference, saved as its column, row and whether each stays.
mod cell_ref {
    use desk_formula::CellRef;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(r: &CellRef, s: S) -> Result<S::Ok, S::Error> {
        (r.col, r.row, r.col_abs, r.row_abs).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<CellRef, D::Error> {
        let (col, row, col_abs, row_abs) = <(u32, u32, bool, bool)>::deserialize(d)?;
        Ok(CellRef {
            col,
            row,
            col_abs,
            row_abs,
        })
    }
}

/// When a rule applies.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Condition {
    /// The value equals the operand.
    Equal(Operand),
    /// The value does not equal the operand.
    NotEqual(Operand),
    /// The value is greater than the operand.
    Greater(Operand),
    /// The value is greater than or equal to the operand.
    GreaterOrEqual(Operand),
    /// The value is less than the operand.
    Less(Operand),
    /// The value is less than or equal to the operand.
    LessOrEqual(Operand),
    /// The value is between the two, both included.
    Between(Operand, Operand),
    /// The value is not between the two.
    NotBetween(Operand, Operand),
    /// The text is.
    TextIs(String),
    /// The text is not.
    TextIsNot(String),
    /// The text starts with.
    StartsWith(String),
    /// The text ends with.
    EndsWith(String),
    /// The text contains.
    Contains(String),
    /// The text does not contain.
    DoesNotContain(String),
    /// The date is today.
    Today,
    /// The date is before today.
    BeforeToday,
    /// The date is after today.
    AfterToday,
    /// The date is in the 7 days up to today.
    LastSevenDays,
    /// The cell is empty.
    Empty,
}

/// What a rule changes: the parts that are set (TAB-21).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RuleStyle {
    /// The color of the text.
    pub text_color: Option<Color>,
    /// Bold.
    pub bold: Option<bool>,
    /// Italic.
    pub italic: Option<bool>,
    /// Underline.
    pub underline: Option<bool>,
    /// Strikethrough.
    pub strike: Option<bool>,
    /// The fill of the cell.
    pub fill: Option<Color>,
}

impl RuleStyle {
    /// The parts of this style that `other` does not set yet, added to
    /// it: with several rules, the first that sets an attribute wins.
    pub fn fill_in(&self, other: &mut Self) {
        other.text_color = other.text_color.or(self.text_color);
        other.bold = other.bold.or(self.bold);
        other.italic = other.italic.or(self.italic);
        other.underline = other.underline.or(self.underline);
        other.strike = other.strike.or(self.strike);
        other.fill = other.fill.or(self.fill);
    }
}

/// A rule of conditional formatting.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Rule {
    /// When it applies.
    pub condition: Condition,
    /// What it changes.
    pub style: RuleStyle,
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) | Value::Date(n) | Value::Duration(n) => Some(*n),
        _ => None,
    }
}

impl Condition {
    /// Whether a value meets the condition, with `cell` giving the values
    /// of the cells that the operands name and `today` the day now.
    pub fn holds(&self, value: &Value, cell: &dyn Fn(CellRef) -> Value, today: f64) -> bool {
        let operand = |o: &Operand| match o {
            Operand::Number(n) => Value::Number(n.0),
            Operand::Text(t) => Value::Text(t.clone()),
            Operand::Cell(r) => cell(*r),
        };
        let cmp = |o: &Operand| -> Option<std::cmp::Ordering> {
            let other = operand(o);
            match (number(value), number(&other)) {
                (Some(a), Some(b)) => a.partial_cmp(&b),
                _ => match (value, &other) {
                    (Value::Text(a), Value::Text(b)) => {
                        Some(a.to_lowercase().cmp(&b.to_lowercase()))
                    }
                    _ => None,
                },
            }
        };
        use std::cmp::Ordering::*;
        let text = match value {
            Value::Text(t) => Some(t.to_lowercase()),
            _ => None,
        };
        let day = number(value).map(f64::floor);
        let today = today.floor();
        match self {
            Self::Equal(o) => cmp(o) == Some(Equal),
            Self::NotEqual(o) => cmp(o) != Some(Equal) && !matches!(value, Value::Empty),
            Self::Greater(o) => cmp(o) == Some(Greater),
            Self::GreaterOrEqual(o) => matches!(cmp(o), Some(Greater | Equal)),
            Self::Less(o) => cmp(o) == Some(Less),
            Self::LessOrEqual(o) => matches!(cmp(o), Some(Less | Equal)),
            Self::Between(a, b) | Self::NotBetween(a, b) => {
                let (x, y) = (cmp(a), cmp(b));
                let inside = matches!(x, Some(Greater | Equal)) && matches!(y, Some(Less | Equal))
                    || matches!(x, Some(Less | Equal)) && matches!(y, Some(Greater | Equal));
                let known = x.is_some() && y.is_some();
                if matches!(self, Self::Between(..)) {
                    known && inside
                } else {
                    known && !inside
                }
            }
            Self::TextIs(t) => text.is_some_and(|v| v == t.to_lowercase()),
            Self::TextIsNot(t) => text.is_some_and(|v| v != t.to_lowercase()),
            Self::StartsWith(t) => text.is_some_and(|v| v.starts_with(&t.to_lowercase())),
            Self::EndsWith(t) => text.is_some_and(|v| v.ends_with(&t.to_lowercase())),
            Self::Contains(t) => text.is_some_and(|v| v.contains(&t.to_lowercase())),
            Self::DoesNotContain(t) => text.is_some_and(|v| !v.contains(&t.to_lowercase())),
            Self::Today => matches!(value, Value::Date(_)) && day == Some(today),
            Self::BeforeToday => matches!(value, Value::Date(_)) && day.is_some_and(|d| d < today),
            Self::AfterToday => matches!(value, Value::Date(_)) && day.is_some_and(|d| d > today),
            Self::LastSevenDays => {
                matches!(value, Value::Date(_))
                    && day.is_some_and(|d| d <= today && d > today - 7.0)
            }
            Self::Empty => matches!(value, Value::Empty),
        }
    }
}

/// The look that the rules give a value: for each attribute, that of the
/// first rule that applies and sets it.
pub fn apply(
    rules: &[Rule],
    value: &Value,
    cell: &dyn Fn(CellRef) -> Value,
    today: f64,
) -> RuleStyle {
    let mut out = RuleStyle::default();
    for rule in rules {
        if rule.condition.holds(value, cell, today) {
            rule.style.fill_in(&mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_21_first_rule_wins_for_each_attribute() {
        let red = Color::rgb(255, 0, 0);
        let blue = Color::rgb(0, 0, 255);
        let rules = [
            Rule {
                condition: Condition::Greater(Operand::Number(Num(10.0))),
                style: RuleStyle {
                    text_color: Some(red),
                    ..RuleStyle::default()
                },
            },
            Rule {
                condition: Condition::Between(
                    Operand::Number(Num(5.0)),
                    Operand::Cell(CellRef::new(0, 0)),
                ),
                style: RuleStyle {
                    text_color: Some(blue),
                    bold: Some(true),
                    ..RuleStyle::default()
                },
            },
        ];
        let cell = |_r: CellRef| Value::Number(100.0);
        let s = apply(&rules, &Value::Number(20.0), &cell, 0.0);
        assert_eq!(s.text_color, Some(red));
        assert_eq!(s.bold, Some(true));
        let s = apply(&rules, &Value::Number(1.0), &cell, 0.0);
        assert_eq!(s, RuleStyle::default());
        assert!(Condition::Contains("ab".into()).holds(&Value::Text("xABy".into()), &cell, 0.0));
        assert!(Condition::Today.holds(&Value::Date(45_000.3), &cell, 45_000.9));
    }
}
