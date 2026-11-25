// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The table styles of the selected table (PXV-14, TAB-04): giving it a
//! style, Create Table Style, and Copy and Paste Table Style, which pass
//! the look of one table to another, in this document or another one.

use std::cell::RefCell;

use desk_model::{StyleId, TableLook, TableStyleEntry};

use super::TextEditor;

thread_local! {
    /// The look that Copy Table Style took, for Paste Table Style in any
    /// window.
    static COPIED: RefCell<Option<TableLook>> = const { RefCell::new(None) };
}

impl TextEditor {
    /// The table styles of the document, in order.
    pub fn table_styles(&self) -> Vec<TableStyleEntry> {
        self.document().table_styles().iter().cloned().collect()
    }

    /// The table style that the selected table looks like, if any.
    pub fn current_table_style(&self) -> Option<StyleId> {
        let (table, _) = self.table_selection()?;
        let look = TableLook::of(&table);
        self.document().table_styles().matching(&look).map(|s| s.id)
    }

    /// Gives the selected table the look of a table style.
    pub fn apply_table_style(&self, style: StyleId) {
        let Some(entry) = self.document().table_styles().get(style).cloned() else {
            return;
        };
        self.rearrange_table("Table Style", |table, _| entry.look.apply_to(table));
    }

    /// Takes the look of the selected table, for Paste Table Style.
    pub fn copy_table_style(&self) {
        if let Some((table, _)) = self.table_selection() {
            COPIED.with(|c| *c.borrow_mut() = Some(TableLook::of(&table)));
        }
    }

    /// Whether Copy Table Style took a look.
    pub fn can_paste_table_style(&self) -> bool {
        COPIED.with(|c| c.borrow().is_some())
    }

    /// Gives the selected table the look that Copy Table Style took.
    pub fn paste_table_style(&self) {
        let Some(look) = COPIED.with(|c| c.borrow().clone()) else {
            return;
        };
        self.rearrange_table("Paste Table Style", |table, _| look.apply_to(table));
    }

    /// Adds a table style with the look of the selected table, after the
    /// others, as Create Table Style does.
    pub fn create_table_style(&self) {
        if self.table_selection().is_none() {
            return;
        }
        let name = crate::i18n::tr("Table Style {number}");
        self.arrange_objects("Create Table Style", |edit, ids| {
            edit.create_table_style(ids[0], &name)?;
            Ok(None)
        });
    }
}
