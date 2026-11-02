// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Keyboard Shortcuts window, which Help lists in Pages (INT-21).

use std::fmt::Write;

/// A section: its title, whether it needs a document, and each shortcut's
/// keys as GTK writes accelerators, with what it does.
type Section = (&'static str, bool, &'static [(&'static str, &'static str)]);

/// The shortcuts, by section.
const SECTIONS: &[Section] = &[
    (
        "Documents",
        true,
        &[
            ("<Control>o", "Open a document"),
            ("<Control>s", "Save"),
            ("<Control><Shift>s", "Save As"),
            ("<Control><Shift>d", "Duplicate"),
            ("<Alt>Return", "Document info"),
        ],
    ),
    (
        "Windows",
        false,
        &[
            ("<Control>n", "New window"),
            ("<Control>w", "Close the window"),
            ("<Control><Alt>w", "Close all windows"),
            ("<Control>q", "Quit"),
            ("F11", "Fullscreen"),
            ("<Control>question", "Keyboard shortcuts"),
        ],
    ),
    (
        "Editing",
        true,
        &[
            ("<Control>z", "Undo"),
            ("<Control><Shift>z", "Redo"),
            ("<Control>x", "Cut"),
            ("<Control>c", "Copy"),
            ("<Control>v", "Paste"),
            ("<Control>a", "Select all"),
            ("<Shift>Return", "Line break"),
            ("<Control>BackSpace", "Delete the word before"),
        ],
    ),
    (
        "Moving and Selecting",
        true,
        &[
            ("<Control>Left <Control>Right", "Move by word"),
            ("<Control>Up <Control>Down", "Move by paragraph"),
            ("Home End", "Start or end of the line"),
            ("<Control>Home <Control>End", "Start or end of the document"),
            ("<Shift>Left <Shift>Right", "Select while moving"),
            ("Menu <Shift>F10", "Context menu"),
        ],
    ),
    (
        "Formatting",
        true,
        &[
            ("<Control>b", "Bold"),
            ("<Control>i", "Italic"),
            ("<Control>u", "Underline"),
            ("<Control>braceleft", "Align left"),
            ("<Control>bar", "Center"),
            ("<Control>braceright", "Align right"),
            ("<Control><Shift>i", "Show invisibles"),
            ("<Control><Alt>i", "Inspector"),
        ],
    ),
    (
        "View",
        false,
        &[
            ("<Control>plus", "Zoom in"),
            ("<Control>minus", "Zoom out"),
            ("<Control>0", "Actual size"),
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
            escape(title)
        );
        for (keys, what) in *shortcuts {
            let _ = write!(
                ui,
                "<child><object class=\"GtkShortcutsShortcut\">\
                 <property name=\"accelerator\">{}</property>\
                 <property name=\"title\">{}</property></object></child>",
                escape(keys),
                escape(what)
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
