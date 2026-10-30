// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Formula engine of the Desk tables.
//!
//! This crate parses formulas, tracks the dependencies between cells and
//! evaluates functions. It reads cell values through a trait that
//! `desk-table` implements, so it does not need the table model.
//!
//! Like the other format libraries, this crate is licensed under the
//! MPL-2.0 so that other programs can reuse it. For the same reason it must
//! not depend on the other Desk crates, which are GPL-3.0-or-later. The CI
//! checks this rule.

#![forbid(unsafe_code)]
