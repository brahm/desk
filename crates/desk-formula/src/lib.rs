// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Formula engine of the Desk tables.
//!
//! This crate parses formulas, tracks the dependencies between cells and
//! evaluates functions. It reads cell values through a trait that
//! `desk-table` implements, so it does not need the table model.
//!
//! A formula is written as in iWork: `=SUM(B2:B5)*2`, with cells named by
//! column letters and row numbers from 1, ranges with a colon, `$` before
//! a column or a row that does not move when the formula is copied, and
//! the text of a header cell in place of a range, as `=SUM(2006)` adds the
//! column under the header 2006 (TAB-32). Arguments are separated by
//! commas or semicolons.
//!
//! Like the other format libraries, this crate is licensed under the
//! MPL-2.0 so that other programs can reuse it. For the same reason it must
//! not depend on the other Desk crates, which are GPL-3.0-or-later. The CI
//! checks this rule.

#![forbid(unsafe_code)]

mod eval;
mod functions;
mod odf;
mod parse;
mod value;

pub use eval::{Context, evaluate};
pub use functions::{FUNCTIONS, FunctionInfo, function};
pub use parse::{Axis, BinaryOp, CellRef, Expr, Formula, MAX_NESTING, ParseError, UnaryOp};
pub use value::{ErrorKind, Value, date_from_days, days_from_date};
