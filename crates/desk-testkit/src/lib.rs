// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Test support for Desk.
//!
//! Loads the reference corpus that iWork '09 produces on the reference
//! virtual machine, compares what Desk renders with the PDF files that iWork
//! exported, and reports parity with the feature matrix.
//!
//! The corpus lives in `corpus/` at the root of the repository:
//! `matrix.tsv` is the parity matrix ([`matrix`]), and each folder under
//! `corpus/writer` is a case ([`case`]). The `desk-parity` program runs
//! every case, compares it with Pages ([`compare`]) and writes the report
//! ([`report`]).

pub mod case;
pub mod compare;
pub mod matrix;
pub mod report;

pub use case::{Case, CaseError, CaseSpec};
pub use matrix::Row;
pub use report::{CaseResult, Outcome, Report, Status};
