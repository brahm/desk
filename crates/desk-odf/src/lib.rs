// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! OpenDocument (ODF) package reader and writer.
//!
//! This crate reads and writes ODF packages: the zip container, its
//! manifest and the XML streams inside it. It also holds the schema of the
//! `desk:` extensions, which keep what standard ODF cannot hold. It knows
//! nothing about the Desk document model; the mapping between ODF and the
//! model lives in `desk-filters`.
//!
//! Like the other format libraries, this crate is licensed under the
//! MPL-2.0 so that other programs can reuse it. For the same reason it must
//! not depend on the other Desk crates, which are GPL-3.0-or-later. The CI
//! checks this rule.

#![forbid(unsafe_code)]
