// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The source of a mail merge (W105, MAL-04, MAL-05): where the data of
//! the merge fields of a document come from, as Populate With of Pages
//! 15.4 chooses it.
//!
//! A source is the contacts of the desktop, a vCard file, or a table of a
//! spreadsheet, whose column headers are the names of its fields. The
//! document keeps only where its data are; `desk-contacts` reads them.

use std::sync::Arc;

/// Where the data of a mail merge come from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MergeSource {
    /// The contacts of the desktop, from Evolution Data Server, or of a
    /// group of them by its name.
    Contacts {
        /// The group, as Pages 15.4 offers the groups of Contacts; none
        /// for every contact.
        group: Option<Arc<str>>,
    },
    /// A vCard file, of one card or several.
    VCard {
        /// The path of the file, as it was chosen.
        file: Arc<str>,
    },
    /// A table of a spreadsheet: an ODS of Desk Tables, or CSV.
    Spreadsheet {
        /// The path of the file, as it was chosen.
        file: Arc<str>,
        /// The table of a spreadsheet of several; none for the first.
        table: Option<Arc<str>>,
    },
}

impl MergeSource {
    /// The file of the source, when it is one.
    pub fn file(&self) -> Option<&str> {
        match self {
            Self::Contacts { .. } => None,
            Self::VCard { file } | Self::Spreadsheet { file, .. } => Some(file),
        }
    }
}
