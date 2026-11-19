// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The table styles of a document: named looks of tables, which the Table
//! button shows when inserting a table and the Table tab of the Format
//! pane shows for the selected one (PXV-14, TAB-04).
//!
//! A table style is the fills of the headers, the footers and the body,
//! the color of every other row, the font, the room in the cells and the
//! lines of the cells. Applying it gives the table that look, keeping its
//! cells and their own formatting. A new document has six, which
//! Create Table Style adds to from a table; Copy Table Style and Paste
//! Table Style pass the look of one table to another.

use std::sync::Arc;

use crate::error::ModelError;
use crate::id::NodeId;
use crate::object::ObjectKind;
use crate::op::Op;
use crate::style::StyleId;
use crate::transaction::Edit;
use desk_table::{Area, Border, BorderSet, Color, DEFAULT_BORDER, TableStyle};

/// The look of a table, as a table style keeps it and Copy Table Style
/// takes it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableLook {
    /// The fills, the font and the room in the cells.
    pub style: TableStyle,
    /// The lines of the cells, all alike, or none.
    pub border: Option<Border>,
}

impl Default for TableLook {
    fn default() -> Self {
        Self {
            style: TableStyle::default(),
            border: Some(DEFAULT_BORDER),
        }
    }
}

impl TableLook {
    /// The look of a table: its style, and the line above its first body
    /// cell, or the first line it has, as the lines of all its cells.
    pub fn of(table: &crate::Table) -> Self {
        let (rows, cols) = (table.rows(), table.cols());
        let body = (
            table.header_rows().min(rows.saturating_sub(1)),
            table.header_cols().min(cols.saturating_sub(1)),
        );
        let border = table.h_border(body.0, body.1).or_else(|| {
            (0..=rows)
                .flat_map(|r| (0..cols).map(move |c| (r, c)))
                .find_map(|(r, c)| table.h_border(r, c))
        });
        Self {
            style: table.style.clone(),
            border,
        }
    }

    /// Gives a table this look, keeping its cells.
    pub fn apply_to(&self, table: &mut crate::Table) {
        table.style = self.style.clone();
        let area = Area {
            row: 0,
            col: 0,
            rows: table.rows(),
            cols: table.cols(),
        };
        table.set_borders(area, BorderSet::All, self.border);
    }
}

/// A table style of the document.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableStyleEntry {
    /// The identifier.
    pub id: StyleId,
    /// The name, which the user may change.
    pub name: Arc<str>,
    /// The look.
    pub look: TableLook,
}

/// The table styles of a document, in their order on the pages of the
/// Table button and the Table tab.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TableStyles(Vec<TableStyleEntry>);

/// A look of the library: the fills of the header row, the header column
/// and the body, the color of every other row, and the color of the lines.
fn library_look(
    header_row: Option<Color>,
    header_column: Option<Color>,
    alternating: Option<Color>,
    line: Option<Color>,
) -> TableLook {
    TableLook {
        style: TableStyle {
            header_row_fill: header_row,
            header_column_fill: header_column,
            footer_fill: header_column,
            alternating,
            ..TableStyle::default()
        },
        border: line.map(|color| Border::solid(1_000, color)),
    }
}

/// The six table styles of a new document, by name: the look of new
/// tables first, then plain, blue, green, orange and striped.
pub fn library() -> [(&'static str, TableLook); 6] {
    let rgb = Color::rgb;
    [
        ("Basic", TableLook::default()),
        (
            "Plain",
            library_look(None, None, None, Some(rgb(0xd0, 0xd0, 0xd0))),
        ),
        (
            "Blue",
            library_look(
                Some(rgb(0xc6, 0xdb, 0xf3)),
                Some(rgb(0xdd, 0xe9, 0xf7)),
                None,
                Some(rgb(0x8f, 0xb3, 0xdd)),
            ),
        ),
        (
            "Green",
            library_look(
                Some(rgb(0xcf, 0xe8, 0xd2)),
                Some(rgb(0xe3, 0xf2, 0xe4)),
                None,
                Some(rgb(0x8c, 0xc5, 0x96)),
            ),
        ),
        (
            "Orange",
            library_look(
                Some(rgb(0xfb, 0xdc, 0xc0)),
                Some(rgb(0xfd, 0xe9, 0xd7)),
                None,
                Some(rgb(0xe8, 0xa7, 0x6d)),
            ),
        ),
        (
            "Striped",
            library_look(
                Some(rgb(0xd9, 0xd9, 0xd9)),
                None,
                Some(rgb(0xf2, 0xf2, 0xf2)),
                Some(rgb(0xbf, 0xbf, 0xbf)),
            ),
        ),
    ]
}

impl TableStyles {
    /// The six styles of a new document.
    pub fn library(mut new_id: impl FnMut() -> NodeId) -> Self {
        Self(
            library()
                .into_iter()
                .map(|(name, look)| TableStyleEntry {
                    id: new_id(),
                    name: name.into(),
                    look,
                })
                .collect(),
        )
    }

    /// The styles from a list, as a file keeps them.
    pub fn from_entries(entries: Vec<TableStyleEntry>) -> Self {
        Self(entries)
    }

    /// The styles, in order.
    pub fn iter(&self) -> impl Iterator<Item = &TableStyleEntry> {
        self.0.iter()
    }

    /// How many there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// A style by its identifier.
    pub fn get(&self, id: StyleId) -> Option<&TableStyleEntry> {
        self.0.iter().find(|s| s.id == id)
    }

    /// The first style with the look of a table, which the Table tab
    /// shows as chosen.
    pub fn matching(&self, look: &TableLook) -> Option<&TableStyleEntry> {
        self.0.iter().find(|s| s.look == *look)
    }
}

impl crate::Document {
    /// The table styles.
    pub fn table_styles(&self) -> &TableStyles {
        &self.table_styles
    }
}

impl Edit {
    /// Sets every table style at once.
    pub fn set_table_styles(&mut self, styles: TableStyles) -> Result<(), ModelError> {
        if *self.document().table_styles() == styles {
            return Ok(());
        }
        self.apply(Op::SetTableStyles { styles }).map(drop)
    }

    /// Gives tables a look, keeping their cells. Locked tables and other
    /// objects stay as they are.
    pub fn apply_table_look(&mut self, ids: &[NodeId], look: &TableLook) -> Result<(), ModelError> {
        for id in ids {
            let Some(mut object) = self.document().objects().get(*id).cloned() else {
                continue;
            };
            let ObjectKind::Table(table) = &object.kind else {
                continue;
            };
            if object.locked {
                continue;
            }
            let mut table = (**table).clone();
            look.apply_to(&mut table);
            object.kind = ObjectKind::Table(Arc::new(table));
            self.replace_object(object)?;
        }
        Ok(())
    }

    /// Gives tables the look of a table style.
    pub fn apply_table_style(&mut self, ids: &[NodeId], style: StyleId) -> Result<(), ModelError> {
        let look = self
            .document()
            .table_styles()
            .get(style)
            .map(|s| s.look.clone())
            .ok_or(ModelError::UnknownStyle(style))?;
        self.apply_table_look(ids, &look)
    }

    /// Adds a table style with the look of a table, after the others, as
    /// Create Table Style does. Its name is `name` with the first number
    /// for `{number}` that no other style has, as "Table Style 1". The new
    /// style, or none when `id` is no table.
    pub fn create_table_style(
        &mut self,
        id: NodeId,
        name: &str,
    ) -> Result<Option<StyleId>, ModelError> {
        let Some(ObjectKind::Table(table)) =
            self.document().objects().get(id).map(|o| o.kind.clone())
        else {
            return Ok(None);
        };
        let mut styles = self.document().table_styles().clone();
        let taken = |n: &str| styles.iter().any(|s| &*s.name == n);
        let name = (1..)
            .map(|n| name.replace("{number}", &n.to_string()))
            .find(|n| !taken(n))
            .unwrap_or_default();
        let style = self.new_id();
        styles.0.push(TableStyleEntry {
            id: style,
            name: name.into(),
            look: TableLook::of(&table),
        });
        self.set_table_styles(styles)?;
        Ok(Some(style))
    }

    /// Renames a table style.
    pub fn rename_table_style(&mut self, style: StyleId, name: &str) -> Result<(), ModelError> {
        let mut styles = self.document().table_styles().clone();
        let entry = styles
            .0
            .iter_mut()
            .find(|s| s.id == style)
            .ok_or(ModelError::UnknownStyle(style))?;
        entry.name = name.into();
        self.set_table_styles(styles)
    }

    /// Takes out a table style; the tables keep their look.
    pub fn delete_table_style(&mut self, style: StyleId) -> Result<(), ModelError> {
        let mut styles = self.document().table_styles().clone();
        let before = styles.len();
        styles.0.retain(|s| s.id != style);
        if styles.len() == before {
            return Err(ModelError::UnknownStyle(style));
        }
        self.set_table_styles(styles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{Frame, Object};
    use crate::{Document, Editor};

    #[test]
    fn pxv_14_tab_04_tables_take_table_styles() {
        let mut editor = Editor::new(Document::new_word_processing());
        let styles: Vec<StyleId> = editor
            .document()
            .table_styles()
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(styles.len(), 6);
        let add = |editor: &mut Editor| {
            editor
                .edit("Insert Table", |edit| {
                    let id = edit.new_id();
                    edit.add_object(Object::new(
                        id,
                        ObjectKind::Table(Arc::new(crate::Table::new(3, 3))),
                        Frame::from_pt(0.0, 0.0, 216.0, 60.0),
                    ))?;
                    Ok(id)
                })
                .unwrap()
        };
        let (a, b) = (add(&mut editor), add(&mut editor));
        let look = |editor: &Editor, id| match &editor.document().objects().get(id).unwrap().kind {
            ObjectKind::Table(t) => TableLook::of(t),
            _ => unreachable!(),
        };
        // A new table has the look of the first style.
        assert_eq!(
            editor
                .document()
                .table_styles()
                .matching(&look(&editor, a))
                .map(|s| s.id),
            Some(styles[0])
        );
        // Applying the blue style gives its fills and lines to every cell.
        editor
            .edit("Table Style", |e| e.apply_table_style(&[a], styles[2]))
            .unwrap();
        let blue = editor
            .document()
            .table_styles()
            .get(styles[2])
            .unwrap()
            .look
            .clone();
        assert_eq!(look(&editor, a), blue);
        // Copy and Paste Table Style pass the look to another table.
        let copied = look(&editor, a);
        editor
            .edit("Paste Table Style", |e| e.apply_table_look(&[b], &copied))
            .unwrap();
        assert_eq!(look(&editor, b), blue);
        // Create Table Style adds one with the look of a table, named
        // with the first free number.
        let mut striped = look(&editor, b);
        striped.style.alternating = Some(Color::rgb(1, 2, 3));
        editor
            .edit("Look", |e| e.apply_table_look(&[b], &striped))
            .unwrap();
        let made = editor
            .edit("Create Table Style", |e| {
                e.create_table_style(b, "Table Style {number}")
            })
            .unwrap()
            .unwrap();
        let entry = editor.document().table_styles().get(made).unwrap();
        assert_eq!(&*entry.name, "Table Style 1");
        assert_eq!(entry.look, striped);
        editor
            .edit("Rename", |e| e.rename_table_style(made, "Mine"))
            .unwrap();
        editor
            .edit("Delete", |e| e.delete_table_style(made))
            .unwrap();
        assert_eq!(editor.document().table_styles().len(), 6);
        // Undo brings the style back.
        editor.undo().unwrap();
        assert_eq!(editor.document().table_styles().len(), 7);
    }
}
