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
    n_("Add Linked Text Box"),
    n_("Breaks"),
    n_("Character Style"),
    n_("Columns"),
    n_("Delete"),
    n_("Delete Style"),
    n_("Draw Shape"),
    n_("Duplicate"),
    n_("Edit Note"),
    n_("Edit Text"),
    n_("Edit Text Box"),
    n_("Field"),
    n_("Guides"),
    n_("Header"),
    n_("Hyphenation"),
    n_("Import Styles"),
    n_("Insert Break"),
    n_("Insert Connection Line"),
    n_("Insert Field"),
    n_("Insert Image"),
    n_("Insert Link"),
    n_("Insert Note"),
    n_("Insert Shape"),
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
    n_("Shape"),
    n_("Style Shortcut"),
    n_("Styles"),
    n_("Text"),
    n_("Typing"),
];

/// The name of a change in the language of the interface, if it has one
/// to show. The name may have been kept translated already.
fn shown(name: &str) -> Option<String> {
    NAMES
        .iter()
        .find(|known| **known == name || tr(known) == name)
        .map(|known| tr(known))
}

/// The label of Undo for the change it takes back.
pub fn undo_label(name: Option<&str>) -> String {
    match name.and_then(shown) {
        Some(name) => tr("_Undo {action}").replace("{action}", &name),
        None => tr("_Undo"),
    }
}

/// The label of Redo for the change it brings back.
pub fn redo_label(name: Option<&str>) -> String {
    match name.and_then(shown) {
        Some(name) => tr("_Redo {action}").replace("{action}", &name),
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
