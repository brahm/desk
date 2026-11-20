// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The names of the changes that Undo and Redo take back and bring back,
//! as Pages shows them in its menus: "Undo Typing", "Redo Paste" (DOC-12).

use crate::i18n::{n_, tr};

/// The changes with a name to show, marked for translation. A change
/// whose name is not here shows as plain Undo or Redo, so that a name is
/// never shown in another language than the menu.
const NAMES: &[&str] = &[
    n_("Add"),
    n_("Add Column"),
    n_("Add Linked Text Box"),
    n_("Add Row"),
    n_("Add to Thread"),
    n_("Breaks"),
    n_("Capitalize"),
    n_("Change Language and Region"),
    n_("Character Style"),
    n_("Columns"),
    n_("Convert Quotes"),
    n_("Create New Thread"),
    n_("Chart Style"),
    n_("Create Chart Style"),
    n_("Create Table Style"),
    n_("Data Format"),
    n_("Delete"),
    n_("Delete Column"),
    n_("Delete Row"),
    n_("Delete Style"),
    n_("Stop Skipping Slide"),
    n_("Draw Shape"),
    n_("Layout Order"),
    n_("Outline Table Title"),
    n_("Move Cells"),
    n_("Copy Cells"),
    n_("Create Custom Format"),
    n_("Edit Custom Format"),
    n_("Move Custom Format"),
    n_("Delete Custom Format"),
    n_("Paste Chart Style"),
    n_("Paste Table Style"),
    n_("Remove from Thread"),
    n_("Start Thread"),
    n_("Table Style"),
    n_("Table Title"),
    n_("Drop Cap"),
    n_("Drop Cap Style"),
    n_("Duplicate"),
    n_("Edit Chart Data"),
    n_("Edit Note"),
    n_("Edit Presenter Notes"),
    n_("Edit Text"),
    n_("Edit Text Box"),
    n_("Field"),
    n_("Fill"),
    n_("Guides"),
    n_("Header"),
    n_("Hyphenation"),
    n_("Import Styles"),
    n_("Insert Break"),
    n_("Insert Chart"),
    n_("Insert Connection Line"),
    n_("Insert Field"),
    n_("Insert Image"),
    n_("Insert Link"),
    n_("Insert Note"),
    n_("Insert Shape"),
    n_("Insert Table"),
    n_("Insert Table of Contents"),
    n_("Insert Text Box"),
    n_("Instant Alpha"),
    n_("Language"),
    n_("Ligatures"),
    n_("Links"),
    n_("List"),
    n_("List Level"),
    n_("List Start"),
    n_("List Style"),
    n_("Make Lower Case"),
    n_("Make Upper Case"),
    n_("Merge Cells"),
    n_("Move"),
    n_("Move Topic"),
    n_("New Style"),
    n_("Note"),
    n_("Notes"),
    n_("Nudge"),
    n_("Outline Level"),
    n_("Paragraph Style"),
    n_("Paste"),
    n_("Paste Style"),
    n_("Redefine Style"),
    n_("Rename Style"),
    n_("Replace"),
    n_("Replace All"),
    n_("Replace Font"),
    n_("Replace Style"),
    n_("Revert to Defined Style"),
    n_("Set as Default Text Box Appearance"),
    n_("Shape"),
    n_("Skip Slide"),
    n_("Sort"),
    n_("Style Shortcut"),
    n_("Styles"),
    n_("Text"),
    n_("Typing"),
    n_("Unmerge Cells"),
    n_("Add Sheet"),
    n_("Change Table"),
    n_("Delete Sheet"),
    n_("Duplicate Sheet"),
    n_("Move Sheet"),
    n_("Rename Sheet"),
    n_("Rename Table"),
    n_("Show Table Name"),
    n_("Add Row Above"),
    n_("Add Row Below"),
    n_("Add Column Before"),
    n_("Add Column After"),
];

/// The name of a change in the language of the interface, if it has one
/// to show. The name may have been kept translated already.
fn shown(name: &str) -> Option<String> {
    NAMES
        .iter()
        .find(|known| **known == name || tr(known) == name)
        .map(|known| tr(known))
}

/// A label of the form `format`, with the name of a change in place of
/// `{action}`. In English the name keeps its capitals ("Undo Typing"); a
/// translation follows the capitalization of GNOME in its language, with
/// only the first word capitalized ("Desfazer digitação").
fn label(format: &str, name: &str) -> String {
    let translated = tr(format);
    let name = if translated == format {
        name.to_owned()
    } else {
        let mut chars = name.chars();
        chars
            .next()
            .map(|first| first.to_lowercase().chain(chars).collect())
            .unwrap_or_default()
    };
    translated.replace("{action}", &name)
}

/// The label of Undo for the change it takes back.
pub fn undo_label(name: Option<&str>) -> String {
    match name.and_then(shown) {
        Some(name) => label("_Undo {action}", &name),
        None => tr("_Undo"),
    }
}

/// The label of Redo for the change it brings back.
pub fn redo_label(name: Option<&str>) -> String {
    match name.and_then(shown) {
        Some(name) => label("_Redo {action}", &name),
        None => tr("_Redo"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_12_undo_and_redo_name_the_change() {
        assert_eq!(undo_label(Some("Typing")), "_Undo Typing");
        assert_eq!(redo_label(Some("Paste")), "_Redo Paste");
        // A change without a name to show, or none at all.
        assert_eq!(undo_label(Some("Nudge 7")), "_Undo");
        assert_eq!(redo_label(None), "_Redo");
    }
}
