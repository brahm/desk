// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Contacts and records for merge fields (W105).
//!
//! Reads the records of a mail merge from its source: the cards of a vCard
//! file, or the rows of a table of a spreadsheet (ODS of Desk Tables, or
//! CSV) whose column headers name its fields. The fields of a card are
//! those of Pages (MAL-09), parts of the name, nickname, job title,
//! department, company, phones, emails, URLs, birthday, addresses and
//! note, and a field of the name of a kind takes its first value.
//!
//! The source of a document names its file; that name comes from the
//! document, so this crate opens nothing itself: the caller opens the
//! file, after the person chose it or agreed to it, and this crate reads
//! at most [`MAX_SOURCE_BYTES`] of it.

#![deny(clippy::indexing_slicing, clippy::unwrap_used)]
#![cfg_attr(test, allow(clippy::indexing_slicing, clippy::unwrap_used))]

pub mod csv;
pub mod fields;
mod records;
pub mod vcard;

pub use fields::{CONTACT_FIELDS, canonical};
pub use records::{MAX_SOURCE_BYTES, Records, SourceError, decode, read_source};
