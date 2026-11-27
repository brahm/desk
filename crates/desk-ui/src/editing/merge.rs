// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Mail merge in the editor (W105): merge fields put at the cursor
//! (MAL-07), the source of the document chosen (MAL-04, MAL-05), and the
//! preview, which shows the fields filled with the data of a record and
//! leaves the document as it is (MAL-04).

use std::rc::Rc;

use desk_model::{Document, Editor, Field, MergeSource};

use super::TextEditor;

/// The datum of a merge field of the record previewed, by the name of
/// its field and whether it is a sender field.
pub type MergePreview = Rc<dyn Fn(&str, bool) -> Option<String>>;

/// A copy of the document with its merge fields filled by the preview;
/// none when the fields cannot be filled.
pub(super) fn filled(document: &Document, preview: &MergePreview) -> Option<Document> {
    let mut editor = Editor::new(document.clone());
    editor
        .edit("Preview", |edit| {
            edit.fill_merge_fields(&|name, sender| preview(name, sender))
        })
        .ok()?;
    Some(editor.document().clone())
}

impl TextEditor {
    /// Puts a merge field at the cursor, of a recipient or of the sender
    /// (MAL-07).
    pub fn insert_merge_field(&self, name: &str, sender: bool) {
        self.insert_field(Field::Merge {
            name: name.into(),
            sender,
            single_line: false,
            shown: None,
            origin: None,
        });
    }

    /// Chooses where the data of the merge fields come from, or none
    /// (MAL-04): Populate With.
    pub fn set_merge_source(&self, source: Option<MergeSource>) {
        self.format_edit("Mail Merge Source", move |edit| {
            let mut settings = edit.document().text_settings().clone();
            settings.merge_source = source;
            edit.set_text_settings(settings)
        });
    }

    /// Shows the merge fields filled with the data of a record, or as
    /// they are, without changing the document (MAL-04).
    pub fn set_merge_preview(&self, preview: Option<MergePreview>) {
        let was = self.state.borrow().merge_preview.is_some();
        if !was && preview.is_none() {
            return;
        }
        // The preview fills the document, not a header or note being
        // edited on its own.
        if self.story().is_some() {
            self.leave_story(None);
        }
        self.state.borrow_mut().merge_preview = preview;
        self.lay_out_again();
    }

    /// Whether the preview of a mail merge shows; an edit ends it.
    pub fn merge_previewing(&self) -> bool {
        self.state.borrow().merge_preview.is_some()
    }
}
