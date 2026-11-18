// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of Desk Tables in interface B (docs/interface): the tables
//! and charts of the sheet in the navigation sidebar (NINT-02, NINT-03),
//! the header bar of the three apps (NINT-07), the formula bar, the canvas
//! of the sheet with its tables and the zoom over it, the Inspector with
//! Format, Data and Document (NINT-09), whose Format pane formats the
//! cells (NINT-08), and the bottom bar with the sheet switcher and the
//! instant calculations (NINT-04, NGNM-13, step 4 of window M+).
//!
//! The window shares the navigation sidebar, the command search and the
//! look of Desk Writer, and keeps its own parts here, so that each app can
//! grow without the other. The cells of the tables come from the table
//! engine of `desk-table`; until then the window shows the outline of each
//! table, and the Cell and Text tabs, the formula bar and the instant
//! calculations have their place in it and wait for their function.

mod bars;
mod canvas;
mod header;
mod inspector;
mod shortcuts;
mod window;

use adw::prelude::*;
use gtk::gio;

use crate::AppInfo;
use window::TablesWindow;

/// The shortcuts of the window, as GTK writes accelerators. The shortcuts
/// of interface B that are only of Desk Tables are here: Alt and the
/// arrows add rows and columns, and Ctrl+Shift+F shows the formula list
/// (docs/interface, decided on 2026-11-14).
pub(crate) const ACCELS: &[(&str, &[&str])] = &[
    ("app.quit", &["<Control>q"]),
    // Ctrl+T is a new tab in GNOME (D-024).
    ("app.new-document", &["<Control>n", "<Control>t"]),
    ("app.new-window", &["<Control><Shift>n"]),
    ("app.close-all", &["<Control><Alt>w"]),
    // Ctrl+W closes the tab and Ctrl+Shift+W the window, as in GNOME.
    ("win.close", &["<Control>w"]),
    ("window.close", &["<Control><Shift>w"]),
    ("win.duplicate", &["<Control><Shift>d"]),
    ("win.fullscreen", &["F11"]),
    ("win.inspector", &["<Control><Alt>i"]),
    ("win.sidebar", &["F9"]),
    ("win.formula-list", &["<Control><Shift>f"]),
    ("win.search", &["<Control><Alt>f"]),
    ("win.table-styles", &["<Control><Shift>t"]),
    ("win.shortcuts", &["<Control>question"]),
    ("win.command-search", &["<Control>slash"]),
    ("win.zoom('in')", &["<Control>plus", "<Control>equal"]),
    ("win.zoom('out')", &["<Control>minus"]),
    ("win.zoom('1')", &["<Control>0"]),
    ("win.undo", &["<Control>z"]),
    ("win.redo", &["<Control><Shift>z", "<Control>y"]),
    ("win.deselect-all", &["<Control><Shift>a"]),
    ("win.add-row-above", &["<Alt>Up"]),
    ("win.add-row-below", &["<Alt>Down"]),
    ("win.add-column-before", &["<Alt>Left"]),
    ("win.add-column-after", &["<Alt>Right"]),
    // Ctrl+PageUp and Ctrl+PageDown switch the document tabs, as in GNOME,
    // so the sheets take Alt (D-023).
    ("win.previous-sheet", &["<Alt>Page_Up"]),
    ("win.next-sheet", &["<Alt>Page_Down"]),
];

/// Runs Desk Tables until its last window closes, and returns its exit
/// code.
pub fn run(info: AppInfo) -> gtk::glib::ExitCode {
    crate::register_icons();
    let app = adw::Application::builder().application_id(info.id).build();
    app.connect_startup(move |app| {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::IconTheme::for_display(&display).add_resource_path(crate::ICONS);
        }
        gtk::Window::set_default_icon_name(info.id);
        crate::load_css();
        load_css();
        add_actions(app, info);
        for (action, keys) in ACCELS {
            app.set_accels_for_action(action, keys);
        }
    });
    app.connect_activate(move |app| {
        TablesWindow::new(app, info).show();
    });
    app.run()
}

/// The style of the parts that only Desk Tables has: the instant
/// calculations at the bottom, as chips.
fn load_css() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        ".desk-calc-chip { padding: 2px 8px; border-radius: 99px; \
         background-color: color-mix(in srgb, var(--window-fg-color) 8%, transparent); }\n\
         .desk-formula-bar entry { font-family: monospace; }",
    );
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn add_actions(app: &adw::Application, info: AppInfo) {
    let about = gio::ActionEntry::builder("about")
        .activate(move |app: &adw::Application, _, _| crate::show_about(app, info))
        .build();
    let close_all = |app: &adw::Application| {
        for window in app.windows() {
            window.close();
        }
    };
    let quit = gio::ActionEntry::builder("quit")
        .activate(move |app: &adw::Application, _, _| close_all(app))
        .build();
    let close_windows = gio::ActionEntry::builder("close-all")
        .activate(move |app: &adw::Application, _, _| close_all(app))
        .build();
    // Desk Tables does not open documents yet: a new window and a new
    // document both start from the Blank template, one in a window of its
    // own and the other in a tab of the window in front.
    let new_window = gio::ActionEntry::builder("new-window")
        .activate(move |app: &adw::Application, _, _| {
            TablesWindow::new_window(app, info).show();
        })
        .build();
    let new_document = gio::ActionEntry::builder("new-document")
        .activate(move |app: &adw::Application, _, _| {
            TablesWindow::new(app, info).show();
        })
        .build();
    // The preferences that Desk Tables shares with the other apps (T018).
    let preferences = gio::ActionEntry::builder("preferences")
        .activate(move |app: &adw::Application, _, _| {
            if let Some(window) = app.active_window() {
                crate::preferences::show_for(&window, true);
            }
        })
        .build();
    app.add_action_entries([
        about,
        quit,
        close_windows,
        new_window,
        new_document,
        preferences,
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ngnm_11_every_shortcut_is_one_gtk_knows() {
        for (action, keys) in ACCELS {
            assert!(gio::Action::parse_detailed_name(action).is_ok(), "{action}");
            for key in *keys {
                // The modifiers, then a key that GDK knows by name.
                for modifier in key.split('>').filter(|m| m.starts_with('<')) {
                    assert!(["<Control", "<Shift", "<Alt"].contains(&modifier), "{key}");
                }
                let name = key.rsplit('>').next().unwrap();
                assert!(gtk::gdk::Key::from_name(name).is_some(), "{key}");
            }
        }
    }

    #[test]
    fn nint_07_the_shortcuts_decided_for_tables_are_set() {
        let keys = |action: &str| {
            ACCELS
                .iter()
                .find(|(a, _)| *a == action)
                .map(|(_, k)| k.to_vec())
                .unwrap_or_default()
        };
        // Decided on 2026-11-14 and approved again on 2026-11-15.
        assert_eq!(keys("win.formula-list"), ["<Control><Shift>f"]);
        assert_eq!(keys("win.add-row-above"), ["<Alt>Up"]);
        assert_eq!(keys("win.add-row-below"), ["<Alt>Down"]);
        assert_eq!(keys("win.add-column-before"), ["<Alt>Left"]);
        assert_eq!(keys("win.add-column-after"), ["<Alt>Right"]);
        assert_eq!(keys("win.command-search"), ["<Control>slash"]);
        assert_eq!(keys("win.sidebar"), ["F9"]);
        assert_eq!(keys("win.inspector"), ["<Control><Alt>i"]);
        // Ctrl+Alt+F shows the search results, as in Desk Writer.
        assert_eq!(keys("win.search"), ["<Control><Alt>f"]);
        // No two actions share a key.
        let mut all: Vec<&str> = ACCELS.iter().flat_map(|(_, k)| k.iter().copied()).collect();
        all.sort_unstable();
        let count = all.len();
        all.dedup();
        assert_eq!(all.len(), count);
    }
}
