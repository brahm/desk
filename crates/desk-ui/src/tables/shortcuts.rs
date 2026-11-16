// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Keyboard Shortcuts dialog of Desk Tables (NINT-16), with the
//! shortcuts of interface B that are only of Desk Tables.

use crate::i18n::{n_, tr};

/// A section: its title, and each shortcut's keys as GTK writes
/// accelerators, with what it does.
type Section = (&'static str, &'static [(&'static str, &'static str)]);

const SECTIONS: &[Section] = &[
    (
        n_("Windows"),
        &[
            ("<Control>n <Control>t", n_("New document")),
            ("<Control><Shift>n", n_("New window")),
            ("<Control><Shift>d", n_("Duplicate")),
            ("<Control>w", n_("Close the document")),
            ("<Control><Shift>w", n_("Close the window")),
            ("<Control><Alt>w", n_("Close all windows")),
            ("<Control>q", n_("Quit")),
            ("F11", n_("Fullscreen")),
            ("<Control>question", n_("Keyboard shortcuts")),
            ("F10", n_("Main menu")),
            ("<Control>slash", n_("Search commands")),
        ],
    ),
    (
        n_("Documents"),
        &[
            (
                "<Control>Page_Up <Control>Page_Down",
                n_("Previous or next document"),
            ),
            ("<Alt>1 <Alt>9", n_("First to ninth document")),
            (
                "<Control>Tab <Control><Shift>Tab",
                n_("Next or previous document"),
            ),
        ],
    ),
    (
        n_("Editing"),
        &[
            ("<Control>z", n_("Undo")),
            ("<Control><Shift>z", n_("Redo")),
            ("<Control><Shift>a", n_("Deselect all")),
            ("Delete", n_("Delete the table selected")),
        ],
    ),
    (
        n_("Sheets and Tables"),
        &[
            ("<Alt>Page_Up <Alt>Page_Down", n_("Previous or next sheet")),
            ("Left Right Up Down", n_("Move to another cell")),
            ("Tab <Shift>Tab", n_("Next or previous cell in the row")),
            ("Return", n_("Next cell in the column")),
            (
                "<Shift>Left <Shift>Right <Shift>Up <Shift>Down",
                n_("Select cells"),
            ),
            ("<Alt>Up", n_("Add a row above")),
            ("<Alt>Down", n_("Add a row below")),
            ("<Alt>Left", n_("Add a column before")),
            ("<Alt>Right", n_("Add a column after")),
            ("Escape", n_("Select the table, then nothing")),
            ("Menu <Shift>F10", n_("Context menu")),
        ],
    ),
    (
        n_("View"),
        &[
            ("F9", n_("Show the sidebar")),
            (
                "<Control><Shift>t",
                n_("Show the table styles in the sidebar"),
            ),
            (
                "<Control><Alt>f",
                n_("Show the search results in the sidebar"),
            ),
            ("<Control><Shift>f", n_("Show the formula list")),
            ("<Control><Alt>i", n_("Inspector")),
            ("<Control>plus", n_("Zoom in")),
            ("<Control>minus", n_("Zoom out")),
            ("<Control>0", n_("Actual size")),
        ],
    ),
];

/// The Keyboard Shortcuts dialog, for `win.shortcuts`.
pub(super) fn dialog() -> adw::ShortcutsDialog {
    let dialog = adw::ShortcutsDialog::new();
    for (title, shortcuts) in SECTIONS {
        let section = adw::ShortcutsSection::new(Some(&tr(title)));
        for (keys, what) in *shortcuts {
            section.add(adw::ShortcutsItem::new(&tr(what), keys));
        }
        dialog.add(section);
    }
    dialog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nint_16_every_shortcut_names_keys_gdk_knows() {
        for (_, shortcuts) in SECTIONS {
            for (keys, _) in *shortcuts {
                for key in keys.split(' ') {
                    let name = key.rsplit('>').next().unwrap();
                    assert!(gtk::gdk::Key::from_name(name).is_some(), "{key}");
                }
            }
        }
    }

    #[test]
    fn nint_16_the_dialog_lists_what_the_window_binds() {
        // Each shortcut of the window that the dialog should show is in it.
        let listed: Vec<&str> = SECTIONS
            .iter()
            .flat_map(|(_, s)| s.iter().flat_map(|(keys, _)| keys.split(' ')))
            .collect();
        for (_, keys) in super::super::ACCELS {
            let key = keys[0];
            if ["<Control>equal", "<Control>y"].contains(&key) {
                continue;
            }
            assert!(listed.contains(&key), "{key}");
        }
    }
}
