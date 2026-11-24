// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Formulas typed in a cell or read from a file (TAB-32): any text must
//! read without a panic; a formula read must be written as text and as
//! OpenFormula that read back the same; and a table must evaluate it.

#![no_main]

use desk_formula::Formula;
use desk_table::{Locale, Table};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(formula) = Formula::parse(&text) {
        let written = formula.to_string();
        let again = Formula::parse(&written).expect("a formula written reads");
        assert_eq!(again, formula, "written as {written}");
        let odf = formula.to_odf();
        let again = Formula::parse_odf(&odf).expect("a formula in ODF reads");
        assert_eq!(again, formula, "written as {odf}");
    }
    let _ = Formula::parse_odf(&text);
    // Typed in a cell of a small table, beside numbers it may refer to.
    let pt = Locale::pt_br();
    let mut table: Table<String> = Table::new(3, 3);
    table.type_text(1, 0, "2,5", &pt);
    table.type_text(2, 0, "=A2*2", &pt);
    table.type_text(1, 1, &text, &pt);
    let _ = table.values(45_000.0);
});
