// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Interface shared by Desk Writer, Desk Tables and Desk Slides.
//!
//! The document area, the Inspector, the toolbars, the template chooser, the
//! font and color panels and the Media Browser, built with GTK 4 and
//! Libadwaita. This is the only library that depends on GTK. It re-exports
//! [`gtk`] and [`adw`] so that the apps build against the same versions.
//!
//! For now it only opens an empty window for each app.

pub use adw;
pub use gtk;

use std::sync::Once;

use adw::prelude::*;
use gtk::gio;

/// Resource path of the icons built into the apps, which follow the layout
/// of the `hicolor` icon theme.
const ICONS: &str = "/br/com/brahm/Desk/icons";

/// What the shared interface needs to know about an app.
#[derive(Debug, Clone, Copy)]
pub struct AppInfo {
    /// Application ID, which names the app on D-Bus and in Flatpak, and
    /// names its icons.
    pub id: &'static str,
    /// Name shown to people, such as "Desk Writer".
    pub name: &'static str,
    /// Version shown in the About dialog.
    pub version: &'static str,
}

/// Runs the app until its last window closes, and returns its exit code.
pub fn run(info: AppInfo) -> gtk::glib::ExitCode {
    register_icons();
    let app = adw::Application::builder().application_id(info.id).build();

    app.connect_startup(move |app| {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::IconTheme::for_display(&display).add_resource_path(ICONS);
        }
        gtk::Window::set_default_icon_name(info.id);
        add_actions(app, info);
        app.set_accels_for_action("app.quit", &["<Control>q"]);
        app.set_accels_for_action("window.close", &["<Control>w"]);
    });
    app.connect_activate(move |app| new_window(app, info).present());

    app.run()
}

/// Whether `name` is one of the icons built into the apps: a full color app
/// icon, named after the application ID, or its symbolic version, whose name
/// ends in `-symbolic`.
pub fn has_icon(name: &str) -> bool {
    register_icons();
    ["scalable", "symbolic"].iter().any(|dir| {
        let path = format!("{ICONS}/{dir}/apps/{name}.svg");
        gio::resources_get_info(&path, gio::ResourceLookupFlags::NONE).is_ok()
    })
}

/// Registers the icons that the build script compiled into the app.
fn register_icons() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        gio::resources_register_include!("icons.gresource")
            .expect("the build script makes a valid resource bundle");
    });
}

fn add_actions(app: &adw::Application, info: AppInfo) {
    let about = gio::ActionEntry::builder("about")
        .activate(move |app: &adw::Application, _, _| show_about(app, info))
        .build();
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| app.quit())
        .build();
    app.add_action_entries([about, quit]);
}

fn new_window(app: &adw::Application, info: AppInfo) -> adw::ApplicationWindow {
    let menu = gio::Menu::new();
    menu.append(Some(&format!("_About {}", info.name)), Some("app.about"));

    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .primary(true)
        .tooltip_text("Main Menu")
        .build();

    let header_bar = adw::HeaderBar::new();
    header_bar.pack_end(&menu_button);

    let status_page = adw::StatusPage::builder()
        .icon_name(info.id)
        .title(info.name)
        .description("This app is at an early stage of development and cannot open documents yet.")
        .build();

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header_bar);
    toolbar_view.set_content(Some(&status_page));

    adw::ApplicationWindow::builder()
        .application(app)
        .title(info.name)
        .default_width(800)
        .default_height(600)
        .content(&toolbar_view)
        .build()
}

fn show_about(app: &adw::Application, info: AppInfo) {
    let dialog = adw::AboutDialog::builder()
        .application_name(info.name)
        .application_icon(info.id)
        .version(info.version)
        .developer_name("Desk contributors")
        .copyright("© 2026 Desk contributors")
        .license_type(gtk::License::Gpl30)
        .website("https://github.com/brahm/desk")
        .issue_url("https://github.com/brahm/desk/issues")
        .build();
    dialog.present(app.active_window().as_ref());
}
