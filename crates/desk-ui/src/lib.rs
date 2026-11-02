// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Interface shared by Desk Writer, Desk Tables and Desk Slides.
//!
//! The document area, the Inspector, the toolbars, the template chooser, the
//! font and color panels and the Media Browser, built with GTK 4 and
//! Libadwaita. This is the only library that depends on GTK. It re-exports
//! [`gtk`] and [`adw`] so that the apps build against the same versions.
//!
//! So far a window opens a document and shows its pages, with zoom and the
//! Warnings window, but cannot edit it yet.

pub use adw;
pub use gtk;

mod access;
mod area;
mod editing;
mod formatting;
pub mod geometry;
pub mod i18n;
mod preferences;
mod properties;
mod ruler;
mod shortcuts;
mod tab_stops;
pub mod text_nav;
mod view;
mod warnings;
mod window;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Once;

use adw::prelude::*;
use desk_text::{FontChoice, FontMatch, TextSystem};
use gtk::gio;

pub use desk_model::Document;
use i18n::tr;
pub use view::DocumentView;
use window::DocumentWindow;

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

/// A document that an app read, and what the reading left out.
pub struct Opened {
    /// The document.
    pub document: Document,
    /// What the import could not bring in, one sentence each, for the
    /// Warnings window (INT-22).
    pub warnings: Vec<String>,
}

/// How an app opens documents.
pub struct Opener {
    /// The kinds of document it opens, for the filters of the Open dialog:
    /// a name, and the patterns of the file names.
    pub filters: &'static [(&'static str, &'static [&'static str])],
    /// Reads a document.
    pub open: fn(&Path) -> Result<Opened, String>,
    /// The kind of file it saves: a name, and the extension of the file.
    pub saves: (&'static str, &'static str),
    /// Writes a document as the bytes of a file of that kind, with a
    /// preview of its first page as a PNG image when there is one.
    pub save: SaveFn,
}

/// Writes a document as the bytes of a file, with a preview of its first
/// page as a PNG image when there is one.
pub type SaveFn = fn(&Document, Option<&[u8]>) -> Result<Vec<u8>, String>;

/// The largest side of the preview saved in a document, in pixels.
const THUMBNAIL_SIZE: u32 = 256;

/// Writes a document to a file. It goes to a new file next to the old one,
/// which then takes its place, so that a failed save leaves the old file
/// as it was.
pub(crate) fn save_document(
    opener: &Opener,
    document: &Document,
    first_page: Option<&desk_render::DisplayList>,
    path: &Path,
) -> Result<(), String> {
    let thumbnail = first_page.and_then(|page| {
        desk_render::Painter::new()
            .and_then(|mut painter| painter.thumbnail_png(page, THUMBNAIL_SIZE))
            .ok()
    });
    let bytes = (opener.save)(document, thumbnail.as_deref())?;
    write_replacing(path, &bytes).map_err(|e| e.to_string())
}

fn write_replacing(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.desk-{}", std::process::id()));
    let result = std::fs::write(&temporary, bytes).and_then(|()| {
        // Keep the permissions of the file it replaces.
        if let Ok(old) = std::fs::metadata(path) {
            std::fs::set_permissions(&temporary, old.permissions())?;
        }
        std::fs::rename(&temporary, path)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// A document laid out for a window.
pub(crate) struct Shown {
    pub document: Document,
    pub warnings: Vec<String>,
    /// Whether something was left out, rather than only fonts replaced by
    /// others with the same metrics; then the Warnings window opens by
    /// itself, as in Pages.
    pub lossy: bool,
}

thread_local! {
    /// The fonts of the system, found once for all windows.
    static TEXT: RefCell<TextSystem> = RefCell::new(TextSystem::new());
}

/// Reads and lays out a document.
pub(crate) fn open_document(opener: &Opener, path: &Path) -> Result<Shown, String> {
    let Opened {
        document,
        mut warnings,
    } = (opener.open)(&package_of(path))?;
    let mut lossy = !warnings.is_empty();
    TEXT.with(|text| {
        let mut system = text.borrow_mut();
        system.forget_choices();
        desk_layout::paginate(&document, &mut system);
        for choice in system.substitutions() {
            lossy |= choice.kind != FontMatch::Metric;
            warnings.push(substitution(&choice));
        }
    });
    Ok(Shown {
        document,
        warnings,
        lossy,
    })
}

/// Lays out a document on pages, with the fonts of the system.
pub(crate) fn lay_out(document: &Document) -> desk_layout::DocumentLayout {
    TEXT.with(|text| desk_layout::paginate(document, &mut text.borrow_mut()))
}

/// The document a path names. A Pages document can be a package folder,
/// which the Open dialog cannot pick, so picking the index inside it opens
/// the package.
pub fn package_of(path: &Path) -> PathBuf {
    let is_index = path
        .file_name()
        .is_some_and(|n| n == "index.xml.gz" || n == "index.xml");
    match path.parent() {
        Some(parent)
            if is_index
                && parent.extension().is_some_and(|e| {
                    e.eq_ignore_ascii_case("pages") || e.eq_ignore_ascii_case("template")
                }) =>
        {
            parent.to_owned()
        }
        _ => path.to_owned(),
    }
}

/// A font substitution, in one sentence (DOC-05).
pub fn substitution(choice: &FontChoice) -> String {
    let sentence = match choice.kind {
        FontMatch::Metric => tr(
            "The font {requested} is not installed, so {used} is used, which has the same metrics.",
        ),
        FontMatch::Style => tr(
            "The font {requested} is not installed, so {used} is used, which looks alike; lines may break elsewhere.",
        ),
        FontMatch::Fallback | FontMatch::Installed => tr(
            "The font {requested} is not installed, so {used} is used; lines may break elsewhere.",
        ),
    };
    sentence
        .replace("{requested}", &choice.requested)
        .replace("{used}", &choice.used)
}

/// Runs the app until its last window closes, and returns its exit code.
/// An app that opens documents passes how it does.
pub fn run(info: AppInfo, opener: Option<&'static Opener>) -> gtk::glib::ExitCode {
    register_icons();
    let mut flags = gio::ApplicationFlags::empty();
    if opener.is_some() {
        flags |= gio::ApplicationFlags::HANDLES_OPEN;
    }
    let app = adw::Application::builder()
        .application_id(info.id)
        .flags(flags)
        .build();

    app.connect_startup(move |app| {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::IconTheme::for_display(&display).add_resource_path(ICONS);
        }
        gtk::Window::set_default_icon_name(info.id);
        load_css();
        add_actions(app, info, opener);
        app.set_accels_for_action("app.quit", &["<Control>q"]);
        app.set_accels_for_action("app.new-window", &["<Control>n"]);
        app.set_accels_for_action("app.preferences", &["<Control>comma"]);
        app.set_accels_for_action("win.ruler", &["<Control>r"]);
        app.set_accels_for_action("window.close", &["<Control>w"]);
        app.set_accels_for_action("app.close-all", &["<Control><Alt>w"]);
        if opener.is_some() {
            app.set_accels_for_action("app.open", &["<Control>o"]);
            app.set_accels_for_action("win.save", &["<Control>s"]);
            app.set_accels_for_action("win.save-as", &["<Control><Shift>s"]);
            app.set_accels_for_action("win.duplicate", &["<Control><Shift>d"]);
            app.set_accels_for_action("win.properties", &["<Alt>Return"]);
        }
        app.set_accels_for_action("win.fullscreen", &["F11", "<Control><Alt>f"]);
        for (action, keys) in [
            ("win.bold", &["<Control>b"][..]),
            ("win.italic", &["<Control>i"]),
            ("win.underline", &["<Control>u"]),
            ("win.align('left')", &["<Control>braceleft"]),
            ("win.align('center')", &["<Control>bar"]),
            ("win.align('right')", &["<Control>braceright"]),
            ("win.invisibles", &["<Control><Shift>i"]),
            ("win.inspector", &["<Control><Alt>i"]),
            ("win.show-help-overlay", &["<Control>question"]),
        ] {
            app.set_accels_for_action(action, keys);
        }
        app.set_accels_for_action("win.zoom('in')", &["<Control>plus", "<Control>equal"]);
        app.set_accels_for_action("win.zoom('out')", &["<Control>minus"]);
        app.set_accels_for_action("win.zoom('1')", &["<Control>0"]);
        app.set_accels_for_action("win.undo", &["<Control>z"]);
        app.set_accels_for_action("win.redo", &["<Control><Shift>z", "<Control>y"]);
        app.set_accels_for_action("win.cut", &["<Control>x"]);
        app.set_accels_for_action("win.copy", &["<Control>c"]);
        app.set_accels_for_action("win.paste", &["<Control>v"]);
        app.set_accels_for_action("win.select-all", &["<Control>a"]);
    });
    app.connect_activate(move |app| DocumentWindow::new(app, info, opener).window().present());
    // Files opened from outside: by double click, from the recent files or
    // by dropping them on the icon of the app (DOC-03).
    app.connect_open(move |app, files, _| {
        let Some(opener) = opener else { return };
        for file in files {
            if let Some(path) = file.path() {
                open_in_window(app, info, opener, &path);
            }
        }
    });

    app.run()
}

/// The style of the document area: the pages stand out on a darker
/// backdrop, in the light and the dark style, under rulers that look like
/// the bars of the window.
fn load_css() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        ".desk-document { background-color: alpha(@window_fg_color, 0.1); }\n\
         .desk-ruler { background-color: @headerbar_bg_color; \
         box-shadow: inset 0 -1px alpha(@window_fg_color, 0.15); }",
    );
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// Opens a document in an empty window, or in a new one.
fn open_in_window(app: &adw::Application, info: AppInfo, opener: &'static Opener, path: &Path) {
    let window = DocumentWindow::find_empty()
        .unwrap_or_else(|| DocumentWindow::new(app, info, Some(opener)));
    window.window().present();
    window.load(opener, path);
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

fn add_actions(app: &adw::Application, info: AppInfo, opener: Option<&'static Opener>) {
    let about = gio::ActionEntry::builder("about")
        .activate(move |app: &adw::Application, _, _| show_about(app, info))
        .build();
    // Each window asks to save its changes as it closes, and the app ends
    // with the last one (DOC-18).
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
    let new_window = gio::ActionEntry::builder("new-window")
        .activate(move |app: &adw::Application, _, _| {
            DocumentWindow::new(app, info, opener).window().present();
        })
        .build();
    let preferences = gio::ActionEntry::builder("preferences")
        .activate(move |app: &adw::Application, _, _| {
            if let Some(window) = app.active_window() {
                preferences::show(&window);
            }
        })
        .build();
    app.add_action_entries([about, quit, close_windows, new_window, preferences]);
    if let Some(opener) = opener {
        let open = gio::ActionEntry::builder("open")
            .activate(move |app: &adw::Application, _, _| show_open_dialog(app, info, opener))
            .build();
        app.add_action_entries([open]);
    }
}

/// The Open dialog, through the file chooser portal in Flatpak (DOC-03).
fn show_open_dialog(app: &adw::Application, info: AppInfo, opener: &'static Opener) {
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    let all = gtk::FileFilter::new();
    all.set_name(Some(&tr("All Documents")));
    for (name, patterns) in opener.filters {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&tr(name)));
        for pattern in *patterns {
            filter.add_suffix(pattern);
            all.add_suffix(pattern);
        }
        filters.append(&filter);
    }
    // The index inside a package of Pages, which opens the package.
    all.add_pattern("index.xml.gz");
    filters.insert(0, &all);
    let dialog = gtk::FileDialog::builder()
        .title(tr("Open a Document"))
        .modal(true)
        .filters(&filters)
        .default_filter(&all)
        .build();
    let app = app.clone();
    let parent = app.active_window();
    dialog.open_multiple(parent.as_ref(), gio::Cancellable::NONE, move |result| {
        let Ok(files) = result else { return };
        for file in files.iter::<gio::File>().flatten() {
            if let Some(path) = file.path() {
                open_in_window(&app, info, opener, &path);
            }
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_03_the_index_of_a_package_opens_the_package() {
        assert_eq!(
            package_of(Path::new("/d/Report.pages/index.xml.gz")),
            PathBuf::from("/d/Report.pages")
        );
        assert_eq!(
            package_of(Path::new("/d/Report.pages")),
            PathBuf::from("/d/Report.pages")
        );
        assert_eq!(
            package_of(Path::new("/d/other/index.xml.gz")),
            PathBuf::from("/d/other/index.xml.gz")
        );
    }

    #[test]
    fn doc_09_saving_replaces_the_file_and_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join(format!("desk-ui-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Letter.odt");
        std::fs::write(&path, b"old").unwrap();
        write_replacing(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["Letter.odt"]);
        // A folder that does not exist fails, and the error says why.
        assert!(write_replacing(&dir.join("missing/Letter.odt"), b"new").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
