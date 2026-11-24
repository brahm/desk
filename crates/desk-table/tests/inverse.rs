// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests: adding rows or columns and deleting them again gives
//! the table back, with its formulas and the rules of its cells (review
//! RV, batch 3).

use std::sync::Arc;

use desk_table::{CellRef, Condition, Locale, Operand, Rule, RuleStyle, Table};
use proptest::prelude::*;

/// A cell and what it holds: a number, a formula or a rule that compares
/// with another cell, relative or fixed.
#[derive(Clone, Debug)]
enum Fill {
    Number(u8),
    Formula(u8, u8, bool),
    Rule(u8, u8, bool),
}

fn fill() -> impl Strategy<Value = Fill> {
    prop_oneof![
        any::<u8>().prop_map(Fill::Number),
        (0..6u8, 0..6u8, any::<bool>()).prop_map(|(r, c, a)| Fill::Formula(r, c, a)),
        (0..6u8, 0..6u8, any::<bool>()).prop_map(|(r, c, a)| Fill::Rule(r, c, a)),
    ]
}

fn table(fills: &[(u8, u8, Fill)]) -> Table<String> {
    let en = Locale::en_us();
    let mut t: Table<String> = Table::new(6, 6);
    for (r, c, f) in fills {
        let (r, c) = (u32::from(*r) % 6, u32::from(*c) % 6);
        let name = |row: u8, col: u8, fixed: bool| {
            let col = char::from(b'A' + col % 6);
            let row = row % 6 + 1;
            if fixed {
                format!("${col}${row}")
            } else {
                format!("{col}{row}")
            }
        };
        match f {
            Fill::Number(n) => t.type_text(r, c, &n.to_string(), &en),
            Fill::Formula(row, col, fixed) => {
                t.type_text(r, c, &format!("={}+1", name(*row, *col, *fixed)), &en);
            }
            Fill::Rule(row, col, fixed) => {
                let at = CellRef::parse(&name(*row, *col, *fixed)).unwrap();
                t.cell_mut(r, c).rules = Arc::from([Rule {
                    condition: Condition::Greater(Operand::Cell(at)),
                    style: RuleStyle::default(),
                }]);
            }
        }
    }
    t
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn rows_added_and_deleted_give_the_table_back(
        fills in prop::collection::vec((0..6u8, 0..6u8, fill()), 0..20),
        at in 0..=6u32,
        count in 1..4u32,
    ) {
        let before = table(&fills);
        let mut t = before.clone();
        t.insert_rows(at, count).unwrap();
        t.delete_rows(at, count).unwrap();
        prop_assert_eq!(t, before);
    }

    #[test]
    fn columns_added_and_deleted_give_the_table_back(
        fills in prop::collection::vec((0..6u8, 0..6u8, fill()), 0..20),
        at in 0..=6u32,
        count in 1..4u32,
    ) {
        let before = table(&fills);
        let mut t = before.clone();
        t.insert_cols(at, count).unwrap();
        t.delete_cols(at, count).unwrap();
        prop_assert_eq!(t, before);
    }

    #[test]
    fn rules_copied_into_new_rows_keep_their_distance(
        row in 0..6u8, col in 0..6u8, fixed: bool,
        at in 1..=5u32,
        count in 1..4u32,
    ) {
        // A rule in each cell of the row before the new ones.
        let model = at - 1;
        let mut t = table(&[]);
        let target = CellRef::parse(&if fixed {
            format!("${}${}", char::from(b'A' + col), row + 1)
        } else {
            format!("{}{}", char::from(b'A' + col), row + 1)
        }).unwrap();
        t.cell_mut(model, 0).rules = Arc::from([Rule {
            condition: Condition::Greater(Operand::Cell(target)),
            style: RuleStyle::default(),
        }]);
        t.insert_rows(at, count).unwrap();
        // Where the target is once the rows are added.
        let moved = if target.row >= at { target.row + count } else { target.row };
        for k in 0..count {
            let Condition::Greater(Operand::Cell(r)) = &t.cell(at + k, 0).rules[0].condition else {
                panic!("no rule copied");
            };
            let expected = if fixed {
                moved
            } else {
                target.row + k + 1
            };
            prop_assert_eq!(r.row, expected);
        }
    }
}
