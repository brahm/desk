// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Reader for Apple iWork '09 and '08 documents.
//!
//! This crate reads the XML inside Pages, Numbers and Keynote documents
//! saved by iWork '08 and '09 into a typed tree that follows the files
//! closely. The mapping from that tree to the Desk document model lives in
//! `desk-filters`.
//!
//! Like the other format libraries, this crate is licensed under the
//! MPL-2.0 so that other programs can reuse it. For the same reason it must
//! not depend on the other Desk crates, which are GPL-3.0-or-later. The CI
//! checks this rule.

#![forbid(unsafe_code)]
