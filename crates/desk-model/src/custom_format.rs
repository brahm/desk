// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The custom formats of the cells of a document (PXV-18, TAB-31): Create
//! Custom Format in the Data Format menu of Pages 15.4 keeps one under a
//! name, which the menu then lists; Edit Custom Format changes, renames,
//! moves or deletes it. As the owner's Mac showed on 2026-11-21, changing
//! or renaming a format changes it in every cell that has it, and
//! deleting it gives those cells the Automatic format.

use std::sync::Arc;

use crate::ObjectKind;
use crate::error::ModelError;
use crate::table::{CustomFormat, Format};
use crate::transaction::Edit;

impl crate::Document {
    /// The custom formats, in the order of the Data Format menu.
    pub fn custom_formats(&self) -> &[CustomFormat] {
        &self.text_settings().custom_formats
    }
}

impl Edit {
    /// Sets the custom formats.
    pub fn set_custom_formats(&mut self, formats: Vec<CustomFormat>) -> Result<(), ModelError> {
        let settings = crate::TextSettings {
            custom_formats: formats,
            ..self.document().text_settings().clone()
        };
        self.set_text_settings(settings)
    }

    /// Keeps a custom format: in place of the one at `index`, or after the
    /// others. Where it is.
    pub fn save_custom_format(
        &mut self,
        index: Option<usize>,
        format: CustomFormat,
    ) -> Result<usize, ModelError> {
        let mut formats = self.document().custom_formats().to_vec();
        let at = match index {
            Some(i) if i < formats.len() => {
                let old = std::mem::replace(&mut formats[i], format.clone());
                self.reformat_cells(&old, &Format::Custom(format))?;
                i
            }
            _ => {
                formats.push(format);
                formats.len() - 1
            }
        };
        self.set_custom_formats(formats)?;
        Ok(at)
    }

    /// Moves the custom format at `index` one place up, or down when
    /// `down`, in the menu. Where it is.
    pub fn move_custom_format(&mut self, index: usize, down: bool) -> Result<usize, ModelError> {
        let mut formats = self.document().custom_formats().to_vec();
        let to = if down {
            index + 1
        } else {
            index.wrapping_sub(1)
        };
        if index >= formats.len() || to >= formats.len() {
            return Ok(index);
        }
        formats.swap(index, to);
        self.set_custom_formats(formats)?;
        Ok(to)
    }

    /// Takes out a custom format; the cells that have it take the
    /// Automatic format.
    pub fn delete_custom_format(&mut self, index: usize) -> Result<(), ModelError> {
        let mut formats = self.document().custom_formats().to_vec();
        if index < formats.len() {
            let old = formats.remove(index);
            self.reformat_cells(&old, &Format::Automatic)?;
        }
        self.set_custom_formats(formats)
    }

    /// Gives the cells of every table that have the custom format `old`
    /// the format `new`.
    fn reformat_cells(&mut self, old: &CustomFormat, new: &Format) -> Result<(), ModelError> {
        let old = Format::Custom(old.clone());
        let objects: Vec<crate::Object> = self.document().objects().iter().cloned().collect();
        for mut object in objects {
            let ObjectKind::Table(table) = &object.kind else {
                continue;
            };
            let mut changed = (**table).clone();
            let mut any = false;
            for r in 0..changed.rows() {
                for c in 0..changed.cols() {
                    let cell = changed.cell_mut(r, c);
                    if cell.format == old {
                        cell.format = new.clone();
                        any = true;
                    }
                }
            }
            if any {
                object.kind = ObjectKind::Table(Arc::new(changed));
                self.replace_object(object)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::table::CustomFormat;
    use crate::{Document, Editor};

    fn format(name: &str, pattern: &str) -> CustomFormat {
        CustomFormat {
            name: name.into(),
            pattern: pattern.into(),
        }
    }

    #[test]
    fn pxv_18_tab_31_custom_formats_are_kept_changed_moved_and_deleted() {
        let mut editor = Editor::new(Document::new_word_processing());
        assert!(editor.document().custom_formats().is_empty());
        let kg = format("Kilograms", "#,##0.0 \"kg\"");
        let day = format("Day", "dd/MM");
        editor
            .edit("Create Custom Format", |e| {
                e.save_custom_format(None, kg.clone())?;
                e.save_custom_format(None, day.clone())
            })
            .unwrap();
        assert_eq!(
            editor.document().custom_formats(),
            [kg.clone(), day.clone()]
        );
        // Edit Custom Format renames one in its place.
        let grams = format("Grams", "0 \"g\"");
        let at = editor
            .edit("Edit Custom Format", |e| {
                e.save_custom_format(Some(0), grams.clone())
            })
            .unwrap();
        assert_eq!(at, 0);
        assert_eq!(
            editor.document().custom_formats(),
            [grams.clone(), day.clone()]
        );
        // Moving one down, and past the end, which does nothing.
        let at = editor
            .edit("Move", |e| e.move_custom_format(0, true))
            .unwrap();
        assert_eq!(at, 1);
        assert_eq!(
            editor.document().custom_formats(),
            [day.clone(), grams.clone()]
        );
        editor
            .edit("Move", |e| e.move_custom_format(1, true).map(|_| ()))
            .unwrap();
        assert_eq!(editor.document().custom_formats(), [day.clone(), grams]);
        editor
            .edit("Delete", |e| e.delete_custom_format(1))
            .unwrap();
        assert_eq!(editor.document().custom_formats(), [day]);
        editor.undo().unwrap();
        assert_eq!(editor.document().custom_formats().len(), 2);
    }

    #[test]
    fn tab_31_a_format_changes_in_its_cells_and_deleting_it_makes_them_automatic() {
        use crate::table::Format;
        use crate::{Frame, Object, ObjectKind, Table};
        use std::sync::Arc;
        let mut editor = Editor::new(Document::new_word_processing());
        let kilo = format("Kilo", "#,##0.00K \" kg\"");
        let id = editor
            .edit("Create Custom Format", |e| {
                e.save_custom_format(None, kilo.clone())?;
                let mut table = Table::new(3, 2);
                table.cell_mut(1, 1).format = Format::Custom(kilo.clone());
                table.cell_mut(2, 1).format = Format::Custom(kilo.clone());
                let id = e.new_id();
                e.add_object(Object::new(
                    id,
                    ObjectKind::Table(Arc::new(table)),
                    Frame::from_pt(0.0, 0.0, 200.0, 60.0),
                ))?;
                Ok(id)
            })
            .unwrap();
        let formats = |editor: &Editor| match &editor.document().objects().get(id).unwrap().kind {
            ObjectKind::Table(t) => (t.cell(1, 1).format.clone(), t.cell(2, 1).format.clone()),
            _ => unreachable!(),
        };
        // Editing and renaming it changes it in every cell, under the new
        // name, as Pages 15.4 does.
        let weight = format("Weight", "#,##0.00K \" kg net\"");
        editor
            .edit("Edit Custom Format", |e| {
                e.save_custom_format(Some(0), weight.clone()).map(|_| ())
            })
            .unwrap();
        let custom = Format::Custom(weight);
        assert_eq!(formats(&editor), (custom.clone(), custom));
        // Deleting it gives the cells the Automatic format.
        editor
            .edit("Delete", |e| e.delete_custom_format(0))
            .unwrap();
        assert_eq!(formats(&editor), (Format::Automatic, Format::Automatic));
        assert!(editor.document().custom_formats().is_empty());
    }
}
