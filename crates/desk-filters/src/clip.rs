// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Objects on the clipboard between the apps of Desk (IOP-27 to IOP-29,
//! IOP-31, IOP-33): a page layout document of one page, written as ODT,
//! with the objects copied on it, their pictures, movies and sounds, and
//! the styles of their text. Any app of Desk reads it back and pastes
//! the objects, a whole slide or page among them, with their text styled
//! as it was.

use desk_model::{Anchor, Document, Editor, Object, PageSetup};

use crate::odt::{FilterError, read_odt, write_odt};

/// The media type of objects that an app of Desk puts on the clipboard.
pub const OBJECTS_TYPE: &str = "application/x-desk-objects";

/// Objects read from the clipboard, with the document whose styles their
/// text names.
#[derive(Clone, Debug)]
pub struct ObjectsClip {
    document: Document,
}

impl ObjectsClip {
    /// The objects, from back to front, each fixed on the page as it was
    /// on its own.
    pub fn objects(&self) -> Vec<Object> {
        self.document.objects().iter().cloned().collect()
    }

    /// The document they came in, whose styles their text names, for
    /// [`desk_model::Edit::adopt_object_styles`].
    pub fn document(&self) -> &Document {
        &self.document
    }
}

/// The bytes that put objects of a document on the clipboard: those
/// fixed on a page or in the text, which go on the page where they were,
/// on paper of the size of the document's.
pub fn write_objects(from: &Document, objects: &[Object]) -> Result<Vec<u8>, FilterError> {
    let mut editor = Editor::new(Document::new_page_layout());
    let setup = PageSetup {
        layout_pages: Some(1),
        ..*from.page_setup()
    };
    editor.edit("Copy", |edit| {
        edit.set_page_setup(setup)?;
        let objects: Vec<Object> = objects
            .iter()
            .map(|o| Object {
                anchor: Anchor::Page(0),
                ..o.clone()
            })
            .collect();
        let objects = edit.adopt_object_styles(from, &objects)?;
        edit.add_copies(&objects)?;
        Ok(())
    })?;
    write_odt(editor.document())
}

/// The objects that an app of Desk put on the clipboard.
pub fn read_objects(bytes: &[u8]) -> Result<ObjectsClip, FilterError> {
    Ok(ObjectsClip {
        document: read_odt(bytes)?,
    })
}
