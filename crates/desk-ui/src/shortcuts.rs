// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Keyboard Shortcuts dialog, which Help lists in Pages (INT-21).

use crate::i18n::{n_, tr};

/// A section: its title, whether it needs a document, and each shortcut's
/// keys as GTK writes accelerators, with what it does.
type Section = (&'static str, bool, &'static [(&'static str, &'static str)]);

/// The shortcuts, by section.
const SECTIONS: &[Section] = &[
    (
        n_("Documents"),
        true,
        &[
            ("<Control>o", n_("Open a document")),
            ("<Control>s", n_("Save")),
            ("<Control><Shift>s", n_("Save As")),
            ("<Control><Shift>d", n_("Duplicate")),
            ("<Alt>Return", n_("Document info")),
        ],
    ),
    (
        n_("Windows"),
        false,
        &[
            ("<Control>n", n_("New document")),
            ("<Control><Shift>n", n_("New window")),
            ("<Control>w", n_("Close the window")),
            ("<Control><Alt>w", n_("Close all windows")),
            ("<Control>q", n_("Quit")),
            ("F11", n_("Fullscreen")),
            ("<Control>comma", n_("Preferences")),
            ("<Control>question", n_("Keyboard shortcuts")),
        ],
    ),
    (
        n_("Editing"),
        true,
        &[
            ("<Control>z", n_("Undo")),
            ("<Control><Shift>z", n_("Redo")),
            ("<Control>x", n_("Cut")),
            ("<Control>c", n_("Copy")),
            ("<Control>d", n_("Duplicate")),
            ("<Control>v", n_("Paste")),
            (
                "<Control><Shift>v <Control><Shift><Alt>v",
                n_("Paste and match style"),
            ),
            ("<Control>a", n_("Select all")),
            ("<Control>period", n_("Special characters")),
            ("<Shift>Return", n_("Line break")),
            ("<Control>BackSpace", n_("Delete the word before")),
        ],
    ),
    (
        n_("Moving and Selecting"),
        true,
        &[
            ("<Control>Left <Control>Right", n_("Move by word")),
            ("<Control>Up <Control>Down", n_("Move by paragraph")),
            ("Home End", n_("Start or end of the line")),
            (
                "<Control>Home <Control>End",
                n_("Start or end of the document"),
            ),
            ("<Shift>Left <Shift>Right", n_("Select while moving")),
            (
                "<Alt><Shift>Right <Alt><Shift>Left",
                n_("Change the level of a list item alone"),
            ),
            ("Menu <Shift>F10", n_("Context menu")),
        ],
    ),
    (
        n_("Formatting"),
        true,
        &[
            ("<Control>b", n_("Bold")),
            ("<Control>i", n_("Italic")),
            ("<Control>u", n_("Underline")),
            ("<Control>braceleft", n_("Align left")),
            ("<Control>bar", n_("Center")),
            ("<Control>braceright", n_("Align right")),
            ("<Control><Shift>i", n_("Show invisibles")),
            ("<Control><Alt>i", n_("Inspector")),
            ("F9", n_("Page Thumbnails")),
            ("<Control>t", n_("Show fonts")),
            ("<Control><Shift>c", n_("Show colors")),
            ("<Control><Shift>t", n_("Show the Styles drawer")),
            ("<Control><Alt>c", n_("Copy paragraph style")),
            ("<Control><Shift><Alt>c", n_("Copy character style")),
            ("<Control><Alt>v", n_("Paste style")),
            (
                "<Control>1 <Control>2 <Control>3 <Control>4 <Control>5 <Control>6 <Control>7 <Control>8",
                n_("Apply the style of the key"),
            ),
            (
                "<Control><Shift>1",
                n_("Apply a style and drop the formatting over it"),
            ),
            ("<Control>r", n_("Show ruler")),
            ("<Control><Shift>l", n_("Show layout")),
            ("<Control><Shift>r", n_("Show the format bar")),
        ],
    ),
    (
        n_("Arranging"),
        true,
        &[
            ("<Control><Alt>p", n_("Choose an image, movie or sound")),
            ("<Control><Shift>bracketright", n_("Bring to front")),
            ("<Control>bracketright", n_("Bring forward")),
            ("<Control>bracketleft", n_("Send backward")),
            ("<Control><Shift>bracketleft", n_("Send to back")),
            ("<Control><Alt>g", n_("Group")),
            ("<Control><Shift><Alt>g", n_("Ungroup")),
            ("<Control>l", n_("Lock")),
            ("<Control><Shift><Alt>l", n_("Unlock")),
            ("Left Right Up Down", n_("Nudge the selected objects")),
            ("Escape", n_("Deselect the objects")),
        ],
    ),
    (
        n_("Finding"),
        true,
        &[
            ("<Control>f", n_("Find & Replace")),
            ("<Control>h", n_("Replace")),
            ("<Control>g", n_("Find next")),
            ("<Control><Shift>g", n_("Find previous")),
            ("<Control>e", n_("Use selection for Find")),
            ("<Control>j", n_("Jump to the selection")),
            ("<Control><Alt>f", n_("Search sidebar")),
            ("<Control>semicolon", n_("Find the next misspelled word")),
            ("<Control>colon", n_("Spelling and Grammar")),
        ],
    ),
    (
        n_("View"),
        false,
        &[
            ("<Control>plus", n_("Zoom in")),
            ("<Control>minus", n_("Zoom out")),
            ("<Control>0", n_("Actual size")),
            ("F11", n_("Fullscreen")),
        ],
    ),
];

/// The Keyboard Shortcuts dialog, for `win.shortcuts`. An app that opens
/// no documents leaves out what needs one.
pub fn dialog(opens: bool) -> adw::ShortcutsDialog {
    let dialog = adw::ShortcutsDialog::new();
    for (title, needs_document, shortcuts) in SECTIONS {
        if *needs_document && !opens {
            continue;
        }
        let section = adw::ShortcutsSection::new(Some(&tr(title)));
        for (keys, what) in *shortcuts {
            section.add(adw::ShortcutsItem::new(&tr(what), keys));
        }
        dialog.add(section);
    }
    dialog
}

/// The titles of the shortcuts the dialog lists, for the tests.
#[cfg(test)]
fn titles(opens: bool) -> Vec<&'static str> {
    SECTIONS
        .iter()
        .filter(|(_, needs_document, _)| opens || !needs_document)
        .flat_map(|(_, _, shortcuts)| shortcuts.iter().map(|(_, what)| *what))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_21_every_shortcut_names_keys_gdk_knows() {
        for (_, _, shortcuts) in SECTIONS {
            for (keys, _) in *shortcuts {
                for key in keys.split(' ') {
                    // The modifiers, then a key that GDK knows by name.
                    let name = key.rsplit('>').next().unwrap();
                    for modifier in key.split('>').filter(|m| m.starts_with('<')) {
                        assert!(["<Control", "<Shift", "<Alt"].contains(&modifier), "{key}");
                    }
                    assert!(gtk::gdk::Key::from_name(name).is_some(), "{key}");
                }
            }
        }
        assert!(titles(true).contains(&"Bold"));
        assert!(!titles(false).contains(&"Bold"));
    }
}
