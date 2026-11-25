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
mod auto_numbers;
mod bullet_pictures;
mod chart_data;
mod chart_pane;
mod chart_styles;
mod color_popover;
mod comments_page;
mod compact;
mod contents_page;
mod custom_format;
mod document_controls;
mod drop_cap_pane;
mod drop_cap_styles;
mod editing;
mod effects;
mod eyedropper;
mod fields;
mod fill_rows;
mod find;
mod fonts;
mod format_pane;
mod formatting;
mod function_browser;
pub mod geometry;
mod gradient_bar;
mod graphic_pane;
mod headers_pane;
pub mod i18n;
mod inspector;
mod language_region;
mod layout_pane;
mod links;
mod links_pane;
mod lists_pane;
mod media;
mod media_browser;
mod media_pane;
mod nav_sidebar;
mod notes_pane;
mod object_styles_pane;
mod object_text_pane;
mod objects_pane;
mod outline_view;
mod picture_pane;
mod preferences;
mod properties;
mod recovery;
mod review_bar;
mod ruler;
mod shape_pane;
mod shortcuts;
mod spelling;
mod spelling_window;
mod styles_drawer;
mod tab_stops;
mod table_pane;
mod table_styles;
mod tables;
mod tabs;
pub mod text_nav;
mod thumbnails;
mod undo_names;
mod versions;
mod view;
mod warnings;
mod window;
mod writing_tools;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Once;

use adw::prelude::*;
use desk_text::{FontChoice, FontMatch, TextSystem};
use gtk::gio;

pub use desk_model::{Document, Fragment, Object};
use i18n::{tr, trn};
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

/// Whether an app is Desk Slides, whose window shows presentations
/// (KINT-05).
pub(crate) fn is_slides(info: AppInfo) -> bool {
    info.id.ends_with(".Slides")
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
    /// How it puts objects on the clipboard for the other apps of Desk,
    /// and reads theirs (IOP-27, IOP-28).
    pub objects: Option<ObjectsFormat>,
}

/// Objects and styled text on the clipboard between the apps of Desk
/// (IOP-17, IOP-25, IOP-27, IOP-28).
#[derive(Clone, Copy)]
pub struct ObjectsFormat {
    /// The media type of the bytes.
    pub media_type: &'static str,
    /// The bytes of objects of a document.
    pub write: fn(&Document, &[Object]) -> Result<Vec<u8>, String>,
    /// A document with the objects of the bytes, whose styles their text
    /// names.
    pub read: fn(&[u8]) -> Result<Document, String>,
    /// The media type of text copied with its styles (IOP-17, IOP-25).
    pub text_type: &'static str,
    /// The bytes of text copied with its styles.
    pub write_text: fn(&desk_model::Fragment) -> Result<Vec<u8>, String>,
    /// The text with its styles of the bytes.
    pub read_text: fn(&[u8]) -> Result<desk_model::Fragment, String>,
    /// The media types of formatted text of other programs that the app
    /// reads, the better first (TXT-04).
    pub foreign_text_types: &'static [&'static str],
    /// The formatted text of another program, by its media type.
    pub read_foreign_text: fn(&str, &[u8]) -> Result<desk_model::Fragment, String>,
    /// What puts text copied with its styles on the clipboard for other
    /// programs: media types with their bytes, the better first (TXT-04).
    pub write_foreign_text: ForeignTextFn,
    /// Whether the app keeps the named styles of text pasted in it; one
    /// that does not, as Numbers and Keynote, keeps their look only.
    pub named_styles: bool,
}

/// Writes text copied with its styles as other programs read it: media
/// types with their bytes.
pub type ForeignTextFn = fn(&desk_model::Fragment) -> Result<Vec<(&'static str, Vec<u8>)>, String>;

static OBJECTS_FORMAT: std::sync::OnceLock<ObjectsFormat> = std::sync::OnceLock::new();

/// How objects go on the clipboard between the apps of Desk, if the app
/// says.
pub(crate) fn objects_format() -> Option<ObjectsFormat> {
    OBJECTS_FORMAT.get().copied()
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

/// Writes a file whole or not at all: into a new file beside it, which
/// then takes its place.
///
/// The new file has a name no one can guess and is made, never opened if
/// it exists, so that in a folder others can write to a link put in its
/// place does not take the bytes or the permissions elsewhere; it is on
/// the disk before it takes the place of the file, so that a crash leaves
/// one or the other whole (review RV, batch 5).
pub(crate) fn write_replacing(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let (temporary, file) = temporary_beside(path)?;
    let result = Ok(file)
        .and_then(|mut file: std::fs::File| {
            file.write_all(bytes)?;
            // Keep the permissions of the file it replaces, or those of a
            // new file.
            let permissions = match std::fs::metadata(path) {
                Ok(old) => old.permissions(),
                Err(_) => std::os::unix::fs::PermissionsExt::from_mode(0o666 & !umask()),
            };
            file.set_permissions(permissions)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary, path));
    match result {
        Ok(()) => {
            if let Some(folder) = path.parent().and_then(|p| std::fs::File::open(p).ok()) {
                let _ = folder.sync_all();
            }
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// A new file beside `path`, of a name no one can guess that fits the
/// limit of a file name with the name of `path` in it, made only if no
/// file has that name.
fn temporary_beside(path: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    use std::os::unix::fs::OpenOptionsExt;
    // Names of up to 255 bytes, as most file systems take: the name of the
    // file is cut short to leave room for the dot and the suffix
    // (second review of RV batch 5).
    const NAME_MAX: usize = 255;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut kept = NAME_MAX - ".".len() - ".desk-".len() - 8;
    while !name.is_char_boundary(kept.min(name.len())) {
        kept -= 1;
    }
    let short = &name[..kept.min(name.len())];
    let mut tries = 0;
    loop {
        let token = gtk::glib::random_int();
        let temporary = path.with_file_name(format!(".{short}.desk-{token:08x}"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && tries < 16 => {
                tries += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// The permissions that new files leave out, as the process set them.
fn umask() -> u32 {
    // Read from /proc, as setting it to read it races with other threads.
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|l| l.strip_prefix("Umask:"))
                .and_then(|v| u32::from_str_radix(v.trim(), 8).ok())
        })
        .unwrap_or(0o022)
}

/// A warning of the Warnings window (INT-22).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Warning {
    /// What it says.
    pub text: String,
    /// The family of a missing font that it is about, which can be
    /// replaced.
    pub font: Option<String>,
}

/// A document laid out for a window.
pub(crate) struct Shown {
    pub document: Document,
    pub warnings: Vec<Warning>,
    /// Whether something was left out, rather than only fonts replaced by
    /// others with the same metrics; then the Warnings window opens by
    /// itself, as in Pages.
    pub lossy: bool,
    /// Whether the reader left out content of the file, which saving over
    /// it would lose; a font replaced by another loses nothing (D-031).
    pub left_out: bool,
}

thread_local! {
    /// The fonts of the system, found once for all windows.
    static TEXT: RefCell<TextSystem> = RefCell::new(TextSystem::new());
}

/// Reads and lays out a document.
pub(crate) fn open_document(opener: &Opener, path: &Path) -> Result<Shown, String> {
    let Opened { document, warnings } = (opener.open)(&package_of(path))?;
    let left_out = !warnings.is_empty();
    let mut lossy = left_out;
    let mut warnings: Vec<Warning> = warnings
        .into_iter()
        .map(|text| Warning { text, font: None })
        .collect();
    TEXT.with(|text| {
        let mut system = text.borrow_mut();
        system.forget_choices();
        desk_layout::paginate(&document, &mut system);
        for choice in system.substitutions() {
            lossy |= choice.kind != FontMatch::Metric;
            warnings.push(Warning {
                text: substitution(&choice),
                font: Some(choice.requested.to_string()),
            });
        }
    });
    Ok(Shown {
        document,
        warnings,
        lossy,
        left_out,
    })
}

/// Calls `f` with the fonts of the system.
pub(crate) fn with_text_system<T>(f: impl FnOnce(&mut TextSystem) -> T) -> T {
    TEXT.with(|text| f(&mut text.borrow_mut()))
}

/// Lays out a document on pages, with the fonts of the system.
pub(crate) fn lay_out(document: &Document) -> desk_layout::DocumentLayout {
    TEXT.with(|text| desk_layout::paginate(document, &mut text.borrow_mut()))
}

/// Lays out a document in Screen View, `width` points wide at a Text
/// Size of `zoom` (PXV-01).
pub(crate) fn lay_out_screen(
    document: &Document,
    width: f64,
    zoom: f64,
) -> desk_layout::DocumentLayout {
    TEXT.with(|text| desk_layout::paginate_screen(document, width, zoom, &mut text.borrow_mut()))
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

pub use tables::run as run_tables;

/// Runs the app until its last window closes, and returns its exit code.
/// An app that opens documents passes how it does.
pub fn run(info: AppInfo, opener: Option<&'static Opener>) -> gtk::glib::ExitCode {
    register_icons();
    if let Some(format) = opener.and_then(|o| o.objects) {
        let _ = OBJECTS_FORMAT.set(format);
    }
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
        app.set_accels_for_action("app.new-window", &["<Control><Shift>n"]);
        app.set_accels_for_action("app.preferences", &["<Control>comma"]);
        app.set_accels_for_action("win.ruler", &["<Control>r"]);
        // Ctrl+W closes the tab and Ctrl+Shift+W the window, as in GNOME.
        app.set_accels_for_action("win.close", &["<Control>w"]);
        app.set_accels_for_action("window.close", &["<Control><Shift>w"]);
        app.set_accels_for_action("app.close-all", &["<Control><Alt>w"]);
        if opener.is_some() {
            app.set_accels_for_action("app.open", &["<Control>o"]);
            // Ctrl+T is a new tab in GNOME (D-024), and documents open in
            // tabs.
            app.set_accels_for_action("app.new-document", &["<Control>n", "<Control>t"]);
            app.set_accels_for_action("win.save", &["<Control>s"]);
            app.set_accels_for_action("win.save-as", &["<Control><Shift>s"]);
            app.set_accels_for_action("win.duplicate", &["<Control><Shift>d"]);
            app.set_accels_for_action("win.properties", &["<Alt>Return"]);
        }
        app.set_accels_for_action("win.fullscreen", &["F11"]);
        if is_slides(info) {
            // The shortcuts of Desk Slides approved on 14/11/2026: Ctrl+M
            // for a new slide, Ctrl+Shift+H to skip it and F5 to play
            // (KGNM-11). Ctrl+N starts a presentation too, until it can
            // be saved.
            app.set_accels_for_action("win.new-slide", &["<Control>m"]);
            app.set_accels_for_action("win.skip-slide", &["<Control><Shift>h"]);
            app.set_accels_for_action("win.play", &["F5"]);
            if opener.is_none() {
                app.set_accels_for_action("app.new-tab", &["<Control>n", "<Control>t"]);
            }
        } else {
            // ⇧⌘H in Pages 15.4 (PXV-19); Desk Slides has Ctrl+Shift+H
            // for Skip Slide.
            app.set_accels_for_action("win.insert-highlight", &["<Control><Shift>h"]);
        }
        for (action, keys) in [
            ("win.bold", &["<Control>b"][..]),
            ("win.italic", &["<Control>i"]),
            ("win.underline", &["<Control>u"]),
            ("win.align('left')", &["<Control>braceleft"]),
            ("win.align('center')", &["<Control>bar"]),
            ("win.align('right')", &["<Control>braceright"]),
            ("win.invisibles", &["<Control><Shift>i"]),
            ("win.show-layout", &["<Control><Shift>l"]),
            ("win.screen-view", &["<Control><Shift>r"]),
            ("win.styles-drawer", &["<Control><Shift>t"]),
            ("win.copy-paragraph-style", &["<Control><Alt>c"]),
            ("win.copy-character-style", &["<Control><Shift><Alt>c"]),
            ("win.paste-style", &["<Control><Alt>v"]),
            // Ctrl+Shift+V is Paste Special in GNOME.
            (
                "win.paste-match-style",
                &["<Control><Shift>v", "<Control><Shift><Alt>v"],
            ),
            ("win.inspector", &["<Control><Alt>i"]),
            ("win.sidebar", &["F9"]),
            ("win.find", &["<Control>f"]),
            ("win.find-replace", &["<Control>h"]),
            ("win.find-next", &["<Control>g"]),
            ("win.find-previous", &["<Control><Shift>g"]),
            ("win.use-selection-for-find", &["<Control>e"]),
            ("win.jump-to-selection", &["<Control>j"]),
            ("win.search", &["<Control><Alt>f"]),
            ("win.colors", &["<Control><Shift>c"]),
            ("win.check-spelling", &["<Control>semicolon"]),
            ("win.spelling", &["<Control>colon"]),
            ("win.shortcuts", &["<Control>question"]),
            ("win.command-search", &["<Control>slash"]),
            // ⇧⌘K, ⌥⌘K and ⇧⌥⌘K in Pages 15.4 (W149).
            ("win.insert-comment", &["<Control><Shift>k"]),
            ("win.next-comment", &["<Control><Alt>k"]),
            ("win.previous-comment", &["<Control><Shift><Alt>k"]),
        ] {
            app.set_accels_for_action(action, keys);
        }
        for (action, keys) in window::OBJECT_ACCELS {
            app.set_accels_for_action(action, keys);
        }
        for (action, keys) in window::TABLE_ACCELS {
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
        app.set_accels_for_action("win.deselect-all", &["<Control><Shift>a"]);
        app.set_accels_for_action("win.special-characters", &["<Control>period"]);
    });
    app.connect_activate(move |app| {
        let window = DocumentWindow::new(app, info, opener);
        window.show();
        offer_recovery(app, info, opener, &window);
    });
    // Files opened from outside: by double click, from the recent files or
    // by dropping them on the icon of the app (DOC-03).
    app.connect_open(move |app, files, _| {
        let Some(opener) = opener else { return };
        for file in files {
            if let Some(path) = file.path() {
                open_in_window(app, info, opener, &path);
            }
        }
        if let Some(window) = DocumentWindow::last() {
            offer_recovery(app, info, Some(opener), &window);
        }
    });

    // The files written for playing movies and sounds go with the app.
    app.connect_shutdown(|_| media::clean_up());
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
        // The colors are the named ones of Adwaita, which follow the
        // accent, the dark style and high contrast. The section chosen in
        // the thumbnails has a thin outline and no fill, as in Pages
        // (EST-17), in the accent color in place of its yellow. The
        // controls over the document are pills of the osd style (GNM-13).
        ".desk-document { background-color: color-mix(in srgb, var(--window-fg-color) 10%, transparent); }\n\
         .desk-ruler { background-color: var(--headerbar-bg-color); \
         box-shadow: inset 0 -1px color-mix(in srgb, var(--window-fg-color) 15%, transparent); }\n\
         .desk-thumb-section { padding: 6px; border: 1px solid transparent; }\n\
         .desk-thumb-section.selected { border-color: var(--accent-color); }\n\
         .desk-compact-group { padding-top: 3px; padding-bottom: 3px; }\n\
         .desk-compact-row { min-height: 30px; }\n\
         .desk-inspector-tabs toggle { padding: 0 4px; min-width: 0; }\n\
         .desk-inspector-tabs indicatorbin { padding: 0; margin: 0; }\n\
         .desk-compact-row spinbutton, .desk-compact-row dropdown > button, \
         .desk-compact-row entry { min-height: 26px; }\n\
         .desk-osd-pill { border-radius: 999px; padding: 2px 6px; }\n\
         .desk-template { padding: 9px; }\n\
         .desk-template-page { background-color: white; border-radius: 3px; \
         box-shadow: 0 1px 3px 1px color-mix(in srgb, black 20%, transparent); }\n\
         .desk-template-layout { background-image: linear-gradient(to bottom, \
         transparent 18px, color-mix(in srgb, var(--accent-bg-color) 25%, transparent) 18px, \
         color-mix(in srgb, var(--accent-bg-color) 25%, transparent) 80px, transparent 80px); }",
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
    window.show();
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
            DocumentWindow::new_window(app, info, opener).show();
        })
        .build();
    // A new document in a tab of the window in front: Desk Slides, until
    // it opens files, starts a presentation in it.
    let new_tab = gio::ActionEntry::builder("new-tab")
        .activate(move |app: &adw::Application, _, _| {
            DocumentWindow::new(app, info, opener).show();
        })
        .build();
    // Ctrl+N starts a document from the Blank template at once; a new
    // window shows the template gallery (INT-01).
    let new_document = gio::ActionEntry::builder("new-document")
        .activate(move |app: &adw::Application, _, _| {
            let window = DocumentWindow::new(app, info, opener);
            window.new_from_blank();
            window.show();
        })
        .build();
    let preferences = gio::ActionEntry::builder("preferences")
        .activate(move |app: &adw::Application, _, _| {
            if let Some(window) = app.active_window() {
                preferences::show(&window);
            }
        })
        .build();
    app.add_action_entries([
        about,
        quit,
        close_windows,
        new_window,
        new_tab,
        new_document,
        preferences,
    ]);
    if let Some(opener) = opener {
        let open = gio::ActionEntry::builder("open")
            .activate(move |app: &adw::Application, _, _| show_open_dialog(app, info, opener))
            .build();
        // Open Recent in the main menu names a document by its URI.
        let open_recent = gio::ActionEntry::builder("open-recent")
            .parameter_type(Some(gtk::glib::VariantTy::STRING))
            .activate(move |app: &adw::Application, _, uri| {
                if let Some(path) = uri
                    .and_then(|u| u.get::<String>())
                    .and_then(|u| gio::File::for_uri(&u).path())
                {
                    open_in_window(app, info, opener, &path);
                }
            })
            .build();
        app.add_action_entries([open, open_recent]);
    }
}

thread_local! {
    /// Whether the drafts that a crash left were offered already.
    static RECOVERY_OFFERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Offers back, once, the documents whose recovery drafts a crash left:
/// Recover opens each in a window as a new document, Discard drops them.
fn offer_recovery(
    app: &adw::Application,
    info: AppInfo,
    opener: Option<&'static Opener>,
    over: &std::rc::Rc<DocumentWindow>,
) {
    if opener.is_none() || RECOVERY_OFFERED.with(|o| o.replace(true)) {
        return;
    }
    let drafts = recovery::leftovers();
    if drafts.is_empty() {
        return;
    }
    let names: Vec<String> = drafts
        .iter()
        .map(|d| d.name())
        .map(|n| if n.is_empty() { tr("Untitled") } else { n })
        .collect();
    let dialog = adw::AlertDialog::new(
        Some(&tr("Recover Unsaved Changes?")),
        Some(
            &trn(
                "{app} closed unexpectedly. The changes to this document were kept: {names}.",
                "{app} closed unexpectedly. The changes to these documents were kept: {names}.",
                drafts.len() as u64,
            )
            .replace("{app}", info.name)
            .replace("{names}", &names.join(", ")),
        ),
    );
    dialog.add_response("discard", &tr("_Discard"));
    dialog.add_response("recover", &tr("_Recover"));
    dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
    dialog.set_response_appearance("recover", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("recover"));
    dialog.set_close_response("recover");
    let app = app.clone();
    let first = over.clone();
    dialog.choose(
        Some(&over.window()),
        gio::Cancellable::NONE,
        move |response| {
            if response == "discard" {
                for draft in &drafts {
                    draft.remove();
                }
                return;
            }
            for draft in &drafts {
                let window = if first.is_empty() {
                    first.clone()
                } else {
                    DocumentWindow::new(&app, info, opener)
                };
                if window.recover(draft) {
                    window.show();
                }
            }
        },
    );
}

/// The Open dialog, through the file chooser portal in Flatpak (DOC-03).
fn show_open_dialog(app: &adw::Application, info: AppInfo, opener: &'static Opener) {
    let (filters, all) = document_filters(opener);
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

/// The filters of the documents that an app opens, and the one of all of
/// them, which comes first.
pub(crate) fn document_filters(opener: &Opener) -> (gio::ListStore, gtk::FileFilter) {
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
    (filters, all)
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

/// Runs a test that makes widgets on the one thread that GTK lives on in
/// the tests, as GTK takes one thread only; none when there is no display
/// to start GTK on.
#[cfg(test)]
pub(crate) fn with_gtk<R: Send + 'static>(
    f: impl FnOnce() -> R + Send + std::panic::UnwindSafe + 'static,
) -> Option<R> {
    use std::sync::mpsc;
    type Job = Box<dyn FnOnce() + Send>;
    static GTK: std::sync::OnceLock<Option<std::sync::Mutex<mpsc::Sender<Job>>>> =
        std::sync::OnceLock::new();
    let sender = GTK.get_or_init(|| {
        let (jobs, queue) = mpsc::channel::<Job>();
        let (started, start) = mpsc::channel();
        std::thread::spawn(move || {
            let ok = gtk::init().is_ok();
            let _ = started.send(ok);
            if ok {
                for job in queue {
                    job();
                }
            }
        });
        start
            .recv()
            .unwrap_or(false)
            .then(|| std::sync::Mutex::new(jobs))
    });
    let sender = sender.as_ref()?;
    let (done, result) = mpsc::channel();
    let job: Job = Box::new(move || {
        let _ = done.send(std::panic::catch_unwind(f));
    });
    sender.lock().unwrap().send(job).unwrap();
    match result.recv().unwrap() {
        Ok(r) => Some(r),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_written_whole_without_following_links() {
        // Review RV, batch 5: the file written first had a name anyone
        // could guess, and a link put there sent the bytes elsewhere.
        use std::os::unix::fs::PermissionsExt;
        let folder = std::env::temp_dir().join(format!("desk-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("a.odt");
        let elsewhere = folder.join("elsewhere");
        std::fs::write(&elsewhere, b"theirs").unwrap();
        // A link at the name the old code used.
        std::os::unix::fs::symlink(
            &elsewhere,
            folder.join(format!(".a.odt.desk-{}", std::process::id())),
        )
        .unwrap();
        write_replacing(&path, b"first").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        write_replacing(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert_eq!(std::fs::read(&elsewhere).unwrap(), b"theirs");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        // Nothing is left beside it but the link.
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 3);
        // A name as long as a file system takes still saves (second review
        // of RV batch 5).
        let long = folder.join(format!("{}.odt", "é".repeat(125)));
        write_replacing(&long, b"long").unwrap();
        assert_eq!(std::fs::read(&long).unwrap(), b"long");
        std::fs::remove_dir_all(&folder).unwrap();
    }

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
