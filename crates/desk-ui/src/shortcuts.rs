// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Keyboard Shortcuts window, which Help lists in Pages (INT-21).

use crate::i18n::{n_, tr};
use std::fmt::Write;

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
            ("<Control>n", n_("New window")),
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
            ("<Control>v", n_("Paste")),
            ("<Control>a", n_("Select all")),
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
            ("<Control>r", n_("Show ruler")),
        ],
    ),
    (
        n_("View"),
        false,
        &[
            ("<Control>plus", n_("Zoom in")),
            ("<Control>minus", n_("Zoom out")),
            ("<Control>0", n_("Actual size")),
        ],
    ),
];

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The UI definition of the window. An app that opens no documents leaves
/// out what needs one.
fn definition(opens: bool) -> String {
    let mut ui = String::from(
        "<interface><object class=\"GtkShortcutsWindow\" id=\"shortcuts\">\
         <property name=\"modal\">1</property>\
         <child><object class=\"GtkShortcutsSection\">\
         <property name=\"section-name\">main</property>",
    );
    for (title, needs_document, shortcuts) in SECTIONS {
        if *needs_document && !opens {
            continue;
        }
        let _ = write!(
            ui,
            "<child><object class=\"GtkShortcutsGroup\">\
             <property name=\"title\">{}</property>",
            escape(&tr(title))
        );
        for (keys, what) in *shortcuts {
            let _ = write!(
                ui,
                "<child><object class=\"GtkShortcutsShortcut\">\
                 <property name=\"accelerator\">{}</property>\
                 <property name=\"title\">{}</property></object></child>",
                escape(keys),
                escape(&tr(what))
            );
        }
        ui.push_str("</object></child>");
    }
    ui.push_str("</object></child></object></interface>");
    ui
}

/// The Keyboard Shortcuts window, for `win.show-help-overlay`.
#[allow(deprecated, reason = "GtkShortcutsWindow is current in GTK 4.14")]
pub fn window(opens: bool) -> gtk::ShortcutsWindow {
    gtk::Builder::from_string(&definition(opens))
        .object("shortcuts")
        .expect("the definition names the window")
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
        assert!(definition(true).contains("Bold"));
        assert!(!definition(false).contains("Bold"));
    }
}
