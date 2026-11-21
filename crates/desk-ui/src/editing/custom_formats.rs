// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The custom formats of the document (PXV-18, TAB-31): Create and Edit
//! Custom Format keep one and give it to the selected cells, as one step
//! of the history; a format is also moved in the menu or deleted. Changing
//! a format changes it in every cell that has it, and deleting it gives
//! those cells the Automatic format, as in Pages 15.4.

use std::sync::Arc;

use desk_model::ObjectKind;
use desk_model::table::{CustomFormat, Format};

use super::TextEditor;

impl TextEditor {
    /// The custom formats of the document, in the order of the menu.
    pub fn custom_formats(&self) -> Vec<CustomFormat> {
        self.document().custom_formats().to_vec()
    }

    /// Keeps a custom format, in place of the one at `index` or after the
    /// others, and gives it to the selected cells.
    pub fn save_custom_format(&self, index: Option<usize>, format: CustomFormat) {
        let Some((table, area)) = self.table_selection() else {
            return;
        };
        if self.selection_locked() {
            return;
        }
        let id = self.selected_objects()[0];
        let mut changed = (*table).clone();
        for (r, c) in area.cells() {
            changed.cell_mut(r, c).format = Format::Custom(format.clone());
        }
        let (w, h) = self.table_size(&changed);
        let custom = Format::Custom(format.clone());
        let name = if index.is_some() {
            "Edit Custom Format"
        } else {
            "Create Custom Format"
        };
        self.arrange_objects(name, move |edit, _| {
            // Saving the format changes it in the other cells that have
            // it, this table's too.
            edit.save_custom_format(index, format)?;
            let Some(mut object) = edit.document().objects().get(id).cloned() else {
                return Ok(None);
            };
            let ObjectKind::Table(saved) = &object.kind else {
                return Ok(None);
            };
            let mut changed = (**saved).clone();
            for (r, c) in area.cells() {
                changed.cell_mut(r, c).format = custom.clone();
            }
            object.kind = ObjectKind::Table(Arc::new(changed));
            object.frame.width = desk_model::Length::from_pt(w);
            object.frame.height = desk_model::Length::from_pt(h);
            edit.replace_object(object)?;
            Ok(None)
        });
        self.show_cells();
        self.selection_changed();
    }

    /// Moves a custom format one place up in the menu, or down.
    pub fn move_custom_format(&self, index: usize, down: bool) {
        self.arrange_objects("Move Custom Format", move |edit, _| {
            edit.move_custom_format(index, down)?;
            Ok(None)
        });
        self.selection_changed();
    }

    /// Takes a custom format out of the menu; the cells that have it take
    /// the Automatic format.
    pub fn delete_custom_format(&self, index: usize) {
        self.arrange_objects("Delete Custom Format", move |edit, _| {
            edit.delete_custom_format(index)?;
            Ok(None)
        });
        self.selection_changed();
    }
}
