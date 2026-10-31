// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Import and export filters.
//!
//! Maps the Desk document model to and from ODF, through `desk-odf`, and
//! from iWork '09 and '08 documents, through `desk-iwork`. It also reads and
//! writes Word, Excel, CSV, PowerPoint, RTF and plain text files, and
//! exports ePub.
//!
//! So far it reads and writes ODF text documents, in the [`odt`] module.

pub mod odt;

pub use odt::{FilterError, read_odt, write_odt};
