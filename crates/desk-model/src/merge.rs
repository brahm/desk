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

use crate::{Document, Edit, Editor, Field, ModelError};

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

/// The datum of a merge field, by the name of its field and whether it is
/// a sender field; none leaves the field as it is.
pub type MergeValue<'a> = &'a dyn Fn(&str, bool) -> Option<String>;

/// The merge field filled with its datum, when `value` gives one.
fn filled(field: &Field, value: MergeValue<'_>) -> Option<Field> {
    let Field::Merge {
        name,
        sender,
        single_line,
        shown,
        origin,
    } = field
    else {
        return None;
    };
    let datum: Arc<str> = value(name, *sender)?.into();
    (shown.as_ref() != Some(&datum)).then(|| Field::Merge {
        name: name.clone(),
        sender: *sender,
        single_line: *single_line,
        shown: Some(datum),
        origin: origin.clone(),
    })
}

impl Edit {
    /// Fills the merge fields of the document, in the body, the headers
    /// and footers, the notes and the text of objects, with the data that
    /// `value` gives them, as the preview of Pages 15.4 shows a record
    /// (MAL-06).
    pub fn fill_merge_fields(&mut self, value: MergeValue<'_>) -> Result<(), ModelError> {
        self.map_fields(&|field| filled(field, value))
    }
}

/// The merged document of a mail merge (MAL-04): the body of `document`
/// once for each of `records` records, each from a new page, with the
/// data that `value` gives for the record. As in Pages 15.4 only the
/// sender fields stay fields there (MAL-01): the recipient fields of the
/// body become their text. The headers, footers and objects take the data
/// of the first record. The merged document tracks no changes and has no
/// source of its own.
pub fn merge_document(
    document: &Document,
    records: usize,
    value: &dyn Fn(usize, &str, bool) -> Option<String>,
) -> Result<Document, ModelError> {
    let mut editor = Editor::new(document.clone());
    // Settings first, so that the edit that merges tracks nothing.
    editor.edit("Merge", |edit| {
        let mut settings = edit.document().text_settings().clone();
        settings.tracking = crate::Tracking::Off;
        settings.merge_source = None;
        edit.set_text_settings(settings)
    })?;
    editor.edit("Merge", |edit| {
        let body = {
            let document = edit.document();
            document.copy_fragment(document.start(), document.end())?
        };
        edit.fill_merge_fields(&|name, sender| value(0, name, sender))?;
        let (start, end) = (edit.document().start(), edit.document().end());
        let mut at = edit.delete(start, end)?;
        for record in 0..records.max(1) {
            let mut copy = body.clone();
            for paragraph in &mut copy.paragraphs {
                for (text, _, attrs) in &mut paragraph.runs {
                    let Some(Some(field)) = &attrs.field else {
                        continue;
                    };
                    let field = filled(field, &|name, sender| value(record, name, sender))
                        .unwrap_or_else(|| field.clone());
                    if let Field::Merge { sender: false, .. } = field {
                        *text = field.text(1, 1);
                        attrs.field = None;
                    } else {
                        attrs.field = Some(Some(field));
                    }
                }
            }
            let first = edit.document().body().len();
            if record > 0 {
                at = edit.split_paragraph(edit.document().end())?;
            }
            edit.paste_fragment(at, &copy)?;
            if record > 0 {
                let start = edit
                    .document()
                    .body()
                    .get(first)
                    .expect("the copy has a paragraph")
                    .id();
                let start = crate::Position::new(start, 0);
                edit.set_paragraph_attrs(
                    start,
                    start,
                    &crate::ParaAttrs {
                        page_break_before: Some(true),
                        ..crate::ParaAttrs::default()
                    },
                )?;
            }
        }
        Ok(())
    })?;
    Ok(editor.document().clone())
}
