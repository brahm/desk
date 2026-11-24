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
    /// A cell that was deleted with its row or column: no rule that
    /// compares with it applies.
    Deleted,
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

impl Operand {
    fn map_cell(&self, f: &mut dyn FnMut(CellRef) -> Option<CellRef>) -> Self {
        match self {
            Self::Cell(r) => f(*r).map_or(Self::Deleted, Self::Cell),
            other => other.clone(),
        }
    }
}

impl Condition {
    /// The operands of the condition.
    fn operands(&self) -> Vec<&Operand> {
        match self {
            Self::Equal(o)
            | Self::NotEqual(o)
            | Self::Greater(o)
            | Self::GreaterOrEqual(o)
            | Self::Less(o)
            | Self::LessOrEqual(o) => vec![o],
            Self::Between(a, b) | Self::NotBetween(a, b) => vec![a, b],
            _ => Vec::new(),
        }
    }

    fn map_cells(&self, f: &mut dyn FnMut(CellRef) -> Option<CellRef>) -> Self {
        match self {
            Self::Equal(o) => Self::Equal(o.map_cell(f)),
            Self::NotEqual(o) => Self::NotEqual(o.map_cell(f)),
            Self::Greater(o) => Self::Greater(o.map_cell(f)),
            Self::GreaterOrEqual(o) => Self::GreaterOrEqual(o.map_cell(f)),
            Self::Less(o) => Self::Less(o.map_cell(f)),
            Self::LessOrEqual(o) => Self::LessOrEqual(o.map_cell(f)),
            Self::Between(a, b) => Self::Between(a.map_cell(f), b.map_cell(f)),
            Self::NotBetween(a, b) => Self::NotBetween(a.map_cell(f), b.map_cell(f)),
            other => other.clone(),
        }
    }
}

/// The rules with the cells their operands name changed by `f`, as a
/// formula's references are when rows or columns are added or deleted, or
/// the cell is copied or moved; a cell `f` gives none for was deleted.
/// The same rules when none names a cell.
pub(crate) fn map_rule_cells(
    rules: &std::sync::Arc<[Rule]>,
    f: &mut dyn FnMut(CellRef) -> Option<CellRef>,
) -> std::sync::Arc<[Rule]> {
    let names_cells = rules.iter().any(|rule| {
        rule.condition
            .operands()
            .iter()
            .any(|o| matches!(o, Operand::Cell(_)))
    });
    if !names_cells {
        return rules.clone();
    }
    rules
        .iter()
        .map(|rule| Rule {
            condition: rule.condition.map_cells(f),
            style: rule.style.clone(),
        })
        .collect()
}

/// What checking `rules` against `value` costs, apart from the cells they
/// compare with: the texts compared, which are put in lower case, for
/// each rule.
pub(crate) fn text_work(rules: &[Rule], value: &Value) -> u64 {
    let own = match value {
        Value::Text(t) => t.len() as u64,
        _ => 0,
    };
    rules
        .iter()
        .map(|rule| {
            let theirs: usize = match &rule.condition {
                Condition::TextIs(t)
                | Condition::TextIsNot(t)
                | Condition::StartsWith(t)
                | Condition::EndsWith(t)
                | Condition::Contains(t)
                | Condition::DoesNotContain(t) => t.len(),
                other => other
                    .operands()
                    .iter()
                    .map(|o| match o {
                        Operand::Text(t) => t.len(),
                        _ => 0,
                    })
                    .sum(),
            };
            1 + own + theirs as u64
        })
        .fold(0, u64::saturating_add)
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
        // A rule whose cell was deleted no longer applies, "not equal"
        // included.
        if self
            .operands()
            .iter()
            .any(|o| matches!(o, Operand::Deleted))
        {
            return false;
        }
        let operand = |o: &Operand| match o {
            Operand::Number(n) => Value::Number(n.0),
            Operand::Text(t) => Value::Text(t.clone()),
            Operand::Cell(r) => cell(*r),
            Operand::Deleted => Value::Error(desk_formula::ErrorKind::Reference),
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

    #[test]
    fn a_row_added_at_the_top_copies_the_rules_of_the_first_once() {
        // The rules of the row copied were moved down with it, then moved
        // again: the new first row compared with the cell below the one
        // it should (review of the pull request of RV batch 2).
        use crate::{Locale, Table};
        let mut t: Table<String> = Table::new(3, 1);
        t.set_header_rows(0).unwrap();
        t.type_text(1, 0, "5", &Locale::en_us());
        let rule = Rule {
            condition: Condition::Greater(Operand::Cell(CellRef::new(0, 1))),
            style: RuleStyle::default(),
        };
        t.cell_mut(0, 0).rules = std::sync::Arc::from([rule]);
        t.insert_rows(0, 2).unwrap();
        let operand = |r| match &t.cell(r, 0).rules[0].condition {
            Condition::Greater(o) => o.clone(),
            other => panic!("{other:?}"),
        };
        // The first row as it was compares with its old neighbour, now
        // row 4; the new rows each with the row below them.
        assert_eq!(operand(2), Operand::Cell(CellRef::new(0, 3)));
        assert_eq!(operand(0), Operand::Cell(CellRef::new(0, 1)));
        assert_eq!(operand(1), Operand::Cell(CellRef::new(0, 2)));
    }

    #[test]
    fn the_cell_a_rule_compares_with_follows_rows_added_and_deleted() {
        // A rule "greater than A1" still read row 1 once a row was added
        // above, so it compared with the new empty row (review RV,
        // batch 2).
        use crate::{Locale, Table};
        let en = Locale::en_us();
        let mut t: Table<String> = Table::new(5, 2);
        t.type_text(0, 0, "10", &en);
        t.type_text(4, 1, "20", &en);
        let rule = Rule {
            condition: Condition::Greater(Operand::Cell(CellRef::new(0, 0))),
            style: RuleStyle {
                bold: Some(true),
                ..RuleStyle::default()
            },
        };
        t.cell_mut(4, 1).rules = std::sync::Arc::from([rule]);
        t.insert_rows(0, 1).unwrap();
        let operand = |t: &Table<String>, r, c| match &t.cell(r, c).rules[0].condition {
            Condition::Greater(o) => o.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(operand(&t, 5, 1), Operand::Cell(CellRef::new(0, 1)));
        let v = t.values(0.0);
        assert_eq!(t.rule_style(&v, 5, 1).bold, Some(true));
        // Copied down a row, a relative reference moves with it.
        t.fill(crate::Area::cell(5, 1), crate::FillDirection::Up, 1);
        assert_eq!(operand(&t, 4, 1), Operand::Cell(CellRef::new(0, 0)));
        // Deleted with its row, the rule no longer applies.
        t.delete_rows(1, 1).unwrap();
        assert_eq!(operand(&t, 4, 1), Operand::Deleted);
        let v = t.values(0.0);
        assert_eq!(t.rule_style(&v, 4, 1).bold, None);
        // Rows and columns added in the middle copy the rules relatively,
        // from where their references were (B-053).
        let mut t: Table<String> = Table::new(6, 3);
        t.set_header_rows(0).unwrap();
        t.set_header_cols(0).unwrap();
        let rule = |r: CellRef| Rule {
            condition: Condition::Greater(Operand::Cell(r)),
            style: RuleStyle::default(),
        };
        // Below its cell, and fixed at A1.
        t.cell_mut(1, 1).rules = std::sync::Arc::from([
            rule(CellRef::new(1, 3)),
            rule(CellRef::parse("$A$1").unwrap()),
        ]);
        t.insert_rows(2, 2).unwrap();
        let operands = |t: &Table<String>, r, c| -> Vec<Operand> {
            t.cell(r, c)
                .rules
                .iter()
                .map(|x| match &x.condition {
                    Condition::Greater(o) => o.clone(),
                    other => panic!("{other:?}"),
                })
                .collect()
        };
        // The original moves with the rows; each copy keeps the distance.
        assert_eq!(operands(&t, 1, 1)[0], Operand::Cell(CellRef::new(1, 5)));
        assert_eq!(operands(&t, 2, 1)[0], Operand::Cell(CellRef::new(1, 4)));
        assert_eq!(operands(&t, 3, 1)[0], Operand::Cell(CellRef::new(1, 5)));
        assert_eq!(
            operands(&t, 3, 1)[1],
            Operand::Cell(CellRef::parse("$A$1").unwrap())
        );
        // A column copies the rule of the one before it, to its left.
        let mut t: Table<String> = Table::new(1, 3);
        t.set_header_rows(0).unwrap();
        t.set_header_cols(0).unwrap();
        t.cell_mut(0, 1).rules = std::sync::Arc::from([rule(CellRef::new(0, 0))]);
        t.insert_cols(2, 1).unwrap();
        assert_eq!(operands(&t, 0, 2)[0], Operand::Cell(CellRef::new(1, 0)));
        // Nor does "not equal", which every value would meet.
        let deleted = Condition::NotEqual(Operand::Deleted);
        assert!(!deleted.holds(&Value::Number(1.0), &|_| Value::Empty, 0.0));
        let between = Condition::Between(Operand::Number(Num(0.0)), Operand::Deleted);
        assert!(!between.holds(&Value::Number(1.0), &|_| Value::Empty, 0.0));
    }
}
