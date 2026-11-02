// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A document window: the header bar, the document area, and the zoom
//! menu and counts at the bottom, as in Pages. It saves the document, asks
//! to save it before it closes, and shows it full screen.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use adw::prelude::*;
use desk_model::{CharAttrs, Document, LineStyle, ParaAttrs};
use gtk::{gdk, gio, glib};

use crate::editing::TextEditor;
use crate::fonts::FontsSlot;
use crate::formatting::{self, EditorOf, FormatBar, Inspector};
use crate::geometry;
use crate::i18n::{tr, trn};
use crate::preferences;
use crate::ruler::{self, Ruler};
use crate::view::DocumentView;
use crate::{AppInfo, Opener};

/// How far from the top of the screen the pointer brings the header bar
/// back in full screen, in pixels.
const REVEAL_EDGE: f64 = 4.0;

/// What an action of the window does.
type WindowAction = fn(&Rc<DocumentWindow>);

/// A window and the document it shows, if any.
pub struct DocumentWindow {
    window: adw::ApplicationWindow,
    info: AppInfo,
    opener: Option<&'static Opener>,
    title: adw::WindowTitle,
    toolbar_view: adw::ToolbarView,
    header: adw::HeaderBar,
    stack: gtk::Stack,
    view: DocumentView,
    zoom_button: gtk::MenuButton,
    counts_label: gtk::Label,
    warnings_button: gtk::Button,
    /// The file of the document; none for an untitled one.
    path: RefCell<Option<PathBuf>>,
    /// The name of the document, which is its file's unless untitled.
    name: RefCell<String>,
    warnings: RefCell<Vec<String>>,
    editor: Rc<RefCell<Option<TextEditor>>>,
    format_bar: Rc<FormatBar>,
    inspector: Rc<Inspector>,
    /// The Fonts window (INT-15), made when first shown.
    fonts: FontsSlot,
    split: adw::OverlaySplitView,
    format_revealer: gtk::Revealer,
    ruler: Ruler,
    vertical_ruler: Ruler,
    /// The corner where the rulers meet.
    ruler_corner: gtk::Box,
    /// Whether closing was asked for and answered, so that the window
    /// closes without asking again.
    closing: Cell<bool>,
}

thread_local! {
    /// The open windows, so that a file opened from outside goes to an
    /// empty window when there is one.
    static WINDOWS: RefCell<Vec<Rc<DocumentWindow>>> = const { RefCell::new(Vec::new()) };
}

/// The actions that need a document.
const DOCUMENT_ACTIONS: [&str; 16] = [
    "save",
    "save-as",
    "save-copy",
    "duplicate",
    "fullscreen",
    "select-all",
    "cut",
    "copy",
    "paste",
    "bold",
    "italic",
    "underline",
    "strikethrough",
    "align",
    "inspector",
    "fonts",
];

/// The character toggles of the format bar: an action and what it sets.
const TOGGLES: [(&str, &str); 4] = [
    ("bold", "Bold"),
    ("italic", "Italic"),
    ("underline", "Underline"),
    ("strikethrough", "Strikethrough"),
];

impl DocumentWindow {
    /// A new, empty window.
    pub fn new(app: &adw::Application, info: AppInfo, opener: Option<&'static Opener>) -> Rc<Self> {
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&main_menu(info, opener.is_some()))
            .primary(true)
            .tooltip_text(tr("Main Menu"))
            .build();

        let title = adw::WindowTitle::new(info.name, "");
        let header = adw::HeaderBar::builder().title_widget(&title).build();
        if opener.is_some() {
            let open = gtk::Button::builder()
                .label(tr("_Open"))
                .use_underline(true)
                .action_name("app.open")
                .tooltip_text(tr("Open a Document"))
                .build();
            header.pack_start(&open);
        }
        formatting::labelled(&menu_button, &tr("Main Menu"));
        header.pack_end(&menu_button);
        let leave_fullscreen = gtk::Button::builder()
            .icon_name("view-restore-symbolic")
            .tooltip_text(tr("Leave Fullscreen"))
            .action_name("win.fullscreen")
            .visible(false)
            .build();
        formatting::labelled(&leave_fullscreen, &tr("Leave Fullscreen"));
        header.pack_end(&leave_fullscreen);
        let warnings_button = gtk::Button::builder()
            .icon_name("dialog-warning-symbolic")
            .tooltip_text(tr("Document Warnings"))
            .action_name("win.warnings")
            .visible(false)
            .build();
        formatting::labelled(&warnings_button, &tr("Document Warnings"));
        header.pack_end(&warnings_button);

        let status = adw::StatusPage::builder()
            .icon_name(info.id)
            .title(info.name)
            .build();
        if opener.is_some() {
            status.set_description(Some(&tr(
                "Open an OpenDocument text or a Pages document to edit it.",
            )));
            let open = gtk::Button::builder()
                .label(tr("_Open a Document…"))
                .use_underline(true)
                .action_name("app.open")
                .halign(gtk::Align::Center)
                .css_classes(["pill", "suggested-action"])
                .build();
            status.set_child(Some(&open));
        } else {
            status.set_description(Some(&tr(
                "This app is at an early stage of development and cannot open documents yet.",
            )));
        }

        let view = DocumentView::new();
        let stack = gtk::Stack::new();
        stack.add_named(&status, Some("empty"));

        let zoom_button = gtk::MenuButton::builder()
            .label(geometry::zoom_label(geometry::DEFAULT_ZOOM))
            .menu_model(&zoom_menu())
            .direction(gtk::ArrowType::Up)
            .tooltip_text(tr("Zoom"))
            .css_classes(["flat"])
            .build();
        let counts_label = gtk::Label::builder()
            .css_classes(["dim-label", "caption", "numeric"])
            .hexpand(true)
            .xalign(1.0)
            .build();
        let bottom = gtk::Box::builder()
            .spacing(6)
            .margin_start(6)
            .margin_end(12)
            .margin_top(3)
            .margin_bottom(3)
            .build();
        bottom.append(&zoom_button);
        bottom.append(&counts_label);
        let bottom_bar = gtk::ActionBar::new();
        bottom_bar.pack_start(&bottom);
        bottom_bar.set_revealed(false);

        let editor: Rc<RefCell<Option<TextEditor>>> = Rc::new(RefCell::new(None));
        let editor_of = || -> EditorOf {
            let cell = editor.clone();
            Rc::new(move || cell.borrow().clone())
        };
        let format_bar = FormatBar::new(editor_of());
        let inspector = Inspector::new(editor_of());
        // The rulers over and beside the pages (INT-17).
        let ruler = Ruler::new(&view, editor_of(), false);
        let vertical_ruler = Ruler::new(&view, editor_of(), true);
        let ruler_corner = gtk::Box::builder()
            .width_request(ruler::THICKNESS)
            .height_request(ruler::THICKNESS)
            .css_classes(["desk-ruler"])
            .build();
        let pages = gtk::Grid::new();
        pages.attach(&ruler_corner, 0, 0, 1, 1);
        pages.attach(ruler.widget(), 1, 0, 1, 1);
        pages.attach(vertical_ruler.widget(), 0, 1, 1, 1);
        pages.attach(view.widget(), 1, 1, 1, 1);
        stack.add_named(&pages, Some("document"));
        let format_revealer = gtk::Revealer::builder()
            .child(format_bar.widget())
            .reveal_child(false)
            .build();
        let split = adw::OverlaySplitView::builder()
            .content(&stack)
            .sidebar(inspector.widget())
            .sidebar_position(gtk::PackType::End)
            .show_sidebar(false)
            .build();

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.add_top_bar(&format_revealer);
        toolbar_view.set_content(Some(&split));
        toolbar_view.add_bottom_bar(&bottom_bar);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(info.name)
            .default_width(900)
            .default_height(700)
            .content(&toolbar_view)
            .build();

        let this = Rc::new(Self {
            window,
            info,
            opener,
            title,
            toolbar_view,
            header,
            stack,
            view,
            zoom_button,
            counts_label,
            warnings_button,
            path: RefCell::new(None),
            name: RefCell::new(String::new()),
            warnings: RefCell::new(Vec::new()),
            editor,
            format_bar,
            inspector,
            fonts: FontsSlot::default(),
            split,
            format_revealer,
            ruler,
            vertical_ruler,
            ruler_corner,
            closing: Cell::new(false),
        });
        this.add_actions();
        this.show_rulers();
        let weak = Rc::downgrade(&this);
        preferences::connect_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.show_rulers();
            }
        });
        {
            let label = this.zoom_button.downgrade();
            this.view.connect_zoom_changed(move |zoom| {
                if let Some(label) = label.upgrade() {
                    label.set_label(&geometry::zoom_label(zoom));
                }
            });
        }
        {
            let bar = bottom_bar.downgrade();
            this.stack.connect_visible_child_name_notify(move |stack| {
                if let Some(bar) = bar.upgrade() {
                    bar.set_revealed(stack.visible_child_name().as_deref() == Some("document"));
                }
            });
        }
        this.connect_fullscreen(&leave_fullscreen);
        this.connect_context_menu();
        this.window
            .set_help_overlay(Some(&crate::shortcuts::window(opener.is_some())));
        {
            let weak = Rc::downgrade(&this);
            this.window.connect_close_request(move |_| {
                weak.upgrade()
                    .map_or(glib::Propagation::Proceed, |this| this.close_request())
            });
        }
        WINDOWS.with(|w| w.borrow_mut().push(this.clone()));
        let weak = Rc::downgrade(&this);
        this.window.connect_destroy(move |_| {
            WINDOWS.with(|w| {
                w.borrow_mut()
                    .retain(|o| weak.upgrade().is_none_or(|me| !Rc::ptr_eq(o, &me)));
            });
        });
        this
    }

    /// The GTK window.
    pub fn window(&self) -> &adw::ApplicationWindow {
        &self.window
    }

    /// Whether the window shows no document.
    pub fn is_empty(&self) -> bool {
        self.editor.borrow().is_none()
    }

    /// An open window that shows no document, if any.
    pub fn find_empty() -> Option<Rc<Self>> {
        WINDOWS.with(|w| w.borrow().iter().find(|w| w.is_empty()).cloned())
    }

    fn editor(&self) -> Option<TextEditor> {
        self.editor.borrow().clone()
    }

    /// Whether closing would lose something: changes since the document
    /// was saved, or a document that has never been saved.
    fn needs_saving(&self) -> bool {
        self.editor()
            .is_some_and(|e| e.is_modified() || self.path.borrow().is_none())
    }

    /// Opens a document in this window, or says why it cannot.
    pub fn load(self: &Rc<Self>, opener: &Opener, path: &Path) {
        match crate::open_document(opener, path) {
            Ok(shown) => {
                *self.path.borrow_mut() = Some(path.to_owned());
                *self.name.borrow_mut() = display_name(path);
                self.show_document(shown.document);
                self.warnings_button.set_visible(!shown.warnings.is_empty());
                *self.warnings.borrow_mut() = shown.warnings;
                remember_recent(path);
                if shown.lossy {
                    self.show_warnings();
                }
            }
            Err(message) => {
                self.alert(
                    &tr("Cannot open {name}").replace("{name}", &display_name(path)),
                    &message,
                );
            }
        }
    }

    /// Shows a document that has no file yet, such as a duplicate.
    fn load_untitled(self: &Rc<Self>, document: Document, name: String) {
        *self.path.borrow_mut() = None;
        *self.name.borrow_mut() = name;
        self.show_document(document);
    }

    fn show_document(self: &Rc<Self>, document: Document) {
        let editor = TextEditor::new(&self.view, document);
        let weak = Rc::downgrade(self);
        editor.connect_changed(move || {
            if let Some(this) = weak.upgrade() {
                this.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        editor.connect_selection_changed(move || {
            if let Some(this) = weak.upgrade() {
                this.show_format();
            }
        });
        editor.set_show_invisibles(self.action_state("invisibles"));
        self.format_bar.set_styles(&editor.paragraph_styles());
        *self.editor.borrow_mut() = Some(editor);
        self.stack.set_visible_child_name("document");
        self.format_revealer.set_reveal_child(true);
        self.refresh();
        self.show_format();
    }

    /// Shows the formatting at the selection in the format bar, the
    /// Inspector and the states of the toggles.
    fn show_format(&self) {
        let Some(editor) = self.editor() else { return };
        let format = editor.format();
        self.format_bar.show(&format);
        self.inspector.show(&format);
        if let Some(fonts) = self.fonts.visible() {
            fonts.show(&format.character, &format.paragraph);
        }
        let setup = *editor.document().page_setup();
        self.ruler.show(setup, &format.paragraph);
        self.vertical_ruler.show(setup, &format.paragraph);
        let character = &format.character;
        for (name, on) in [
            ("bold", character.bold),
            ("italic", character.italic),
            ("underline", character.underline != LineStyle::None),
            ("strikethrough", character.strikethrough != LineStyle::None),
        ] {
            formatting::set_state(&self.window, name, on.to_variant());
        }
        formatting::set_state(
            &self.window,
            "align",
            formatting::alignment_name(format.paragraph.alignment).to_variant(),
        );
    }

    /// Shows the rulers that Show Ruler and the preferences ask for.
    fn show_rulers(&self) {
        let on = self.action_state("ruler");
        let vertical = on && preferences::get().vertical_ruler;
        self.ruler.widget().set_visible(on);
        self.vertical_ruler.widget().set_visible(vertical);
        self.ruler_corner.set_visible(vertical);
    }

    fn action_state(&self, name: &str) -> bool {
        self.window
            .lookup_action(name)
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
            .is_some_and(|a| formatting::state_of(&a))
    }

    /// Shows the name, whether the document has changes, the counts at the
    /// bottom, and which actions apply now.
    fn refresh(&self) {
        let editor = self.editor();
        let path = self.path.borrow().clone();
        let name = self.name.borrow().clone();
        if let Some(editor) = &editor {
            let edited = if self.needs_saving() { "• " } else { "" };
            self.title.set_title(&format!("{edited}{name}"));
            self.title
                .set_subtitle(&path.as_deref().map(folder_of).unwrap_or_default());
            self.window.set_title(Some(&format!("{edited}{name}")));
            self.view
                .access()
                .update_property(&[gtk::accessible::Property::Label(&name)]);
            self.counts_label
                .set_label(&counts(editor.word_count(), self.view.page_count()));
        }
        let has_document = editor.is_some();
        let modified = editor.as_ref().is_some_and(TextEditor::is_modified);
        for (action, enabled) in [
            ("undo", editor.as_ref().is_some_and(TextEditor::can_undo)),
            ("redo", editor.as_ref().is_some_and(TextEditor::can_redo)),
            ("revert", path.is_some() && modified),
            ("properties", path.is_some()),
        ] {
            self.enable(action, enabled);
        }
        for action in DOCUMENT_ACTIONS {
            self.enable(action, has_document);
        }
    }

    fn enable(&self, name: &str, enabled: bool) {
        if let Some(action) = self
            .window
            .lookup_action(name)
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        {
            action.set_enabled(enabled);
        }
    }

    fn show_warnings(&self) {
        crate::warnings::show(&self.window, &self.name.borrow(), &self.warnings.borrow());
    }

    fn alert(&self, heading: &str, body: &str) {
        let dialog = adw::AlertDialog::new(Some(heading), Some(body));
        dialog.add_response("close", &tr("_Close"));
        dialog.present(Some(&self.window));
    }

    /// Whether a file can be saved over as it is: one of the kind the app
    /// saves. Another, such as a Pages document, is saved as a new file.
    fn saves_in_place(&self, path: &Path) -> bool {
        self.opener.is_some_and(|opener| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(opener.saves.1))
        })
    }

    /// Writes the document to a file, and says why when it cannot.
    fn write_to(&self, path: &Path) -> bool {
        let (Some(opener), Some(editor)) = (self.opener, self.editor()) else {
            return false;
        };
        let pages = editor.pages();
        match crate::save_document(opener, &editor.document(), pages.first(), path) {
            Ok(()) => true,
            Err(message) => {
                self.alert(
                    &tr("Cannot save {name}").replace("{name}", &display_name(path)),
                    &message,
                );
                false
            }
        }
    }

    /// Saves the document to its file, or asks where when it has none
    /// that it can save over (DOC-09). Then runs `done` with whether it
    /// was saved.
    fn save(self: &Rc<Self>, done: impl FnOnce(bool) + 'static) {
        let path = self.path.borrow().clone();
        match path {
            Some(path) if self.saves_in_place(&path) => {
                let saved = self.write_to(&path);
                if saved {
                    self.saved_as(&path);
                }
                done(saved);
            }
            _ => self.save_as(done),
        }
    }

    /// Asks where to save the document, saves it there, and keeps editing
    /// that file.
    fn save_as(self: &Rc<Self>, done: impl FnOnce(bool) + 'static) {
        let weak = Rc::downgrade(self);
        self.ask_file(&tr("Save"), move |path| {
            let Some(this) = weak.upgrade() else { return };
            match path {
                Some(path) if this.write_to(&path) => {
                    this.saved_as(&path);
                    done(true);
                }
                _ => done(false),
            }
        });
    }

    /// Asks where to save a copy of the document, and saves it there,
    /// still editing the same file.
    fn save_copy(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.ask_file(&tr("Save a Copy"), move |path| {
            if let (Some(this), Some(path)) = (weak.upgrade(), path)
                && this.write_to(&path)
            {
                remember_recent(&path);
            }
        });
    }

    fn saved_as(&self, path: &Path) {
        *self.path.borrow_mut() = Some(path.to_owned());
        *self.name.borrow_mut() = display_name(path);
        if let Some(editor) = self.editor() {
            editor.mark_saved();
        }
        remember_recent(path);
        self.refresh();
    }

    /// The Save dialog, through the file chooser portal in Flatpak. It
    /// offers the name of the document with the extension of the kind the
    /// app saves, in its folder.
    fn ask_file(&self, title: &str, chosen: impl FnOnce(Option<PathBuf>) + 'static) {
        let Some(opener) = self.opener else { return };
        let (kind, extension) = opener.saves;
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&tr(kind)));
        filter.add_suffix(extension);
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let path = self.path.borrow().clone();
        let stem = path.as_deref().and_then(Path::file_stem).map_or_else(
            || self.name.borrow().clone(),
            |s| s.to_string_lossy().into_owned(),
        );
        let dialog = gtk::FileDialog::builder()
            .title(title)
            .modal(true)
            .filters(&filters)
            .default_filter(&filter)
            .initial_name(format!("{stem}.{extension}"))
            .build();
        if let Some(folder) = path.as_deref().and_then(Path::parent) {
            dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
        }
        dialog.save(Some(&self.window), gio::Cancellable::NONE, move |result| {
            chosen(
                result
                    .ok()
                    .and_then(|file| file.path())
                    .map(|path| with_extension(path, extension)),
            );
        });
    }

    /// Opens a copy of the document, untitled, in a new window (DOC-14).
    fn duplicate(&self) {
        let (Some(editor), Some(app)) = (
            self.editor(),
            self.window
                .application()
                .and_then(|a| a.downcast::<adw::Application>().ok()),
        ) else {
            return;
        };
        let stem = self
            .path
            .borrow()
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(
                || self.name.borrow().clone(),
                |s| s.to_string_lossy().into_owned(),
            );
        let copy = DocumentWindow::new(&app, self.info, self.opener);
        copy.load_untitled(editor.document(), format!("{stem} copy"));
        copy.view.set_zoom(self.view.zoom());
        copy.window.present();
    }

    /// Goes back to the document as it was last saved, after asking
    /// (DOC-12).
    fn revert(self: &Rc<Self>) {
        let (Some(opener), Some(path)) = (self.opener, self.path.borrow().clone()) else {
            return;
        };
        let dialog = adw::AlertDialog::new(
            Some(&tr("Revert to Saved?")),
            Some(
                &tr("The changes made to “{name}” since it was last saved will be lost.")
                    .replace("{name}", &self.name.borrow()),
            ),
        );
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("revert", &tr("_Revert"));
        dialog.set_response_appearance("revert", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.choose(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |response| {
                if let (Some(this), "revert") = (weak.upgrade(), response.as_str()) {
                    this.load(opener, &path);
                }
            },
        );
    }

    /// Asks to save the changes before the window closes (DOC-18).
    fn close_request(self: &Rc<Self>) -> glib::Propagation {
        if self.closing.get() || !self.needs_saving() {
            return glib::Propagation::Proceed;
        }
        let dialog = adw::AlertDialog::new(
            Some(&tr("Save Changes?")),
            Some(
                &tr("“{name}” has changes that are not saved. If you close it without saving, they are lost.")
                    .replace("{name}", &self.name.borrow()),
            ),
        );
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("discard", &tr("_Don’t Save"));
        dialog.add_response("save", &tr("_Save"));
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.choose(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |response| {
                let Some(this) = weak.upgrade() else { return };
                match response.as_str() {
                    "discard" => this.close_now(),
                    "save" => {
                        let weak = Rc::downgrade(&this);
                        this.save(move |saved| {
                            if let (true, Some(this)) = (saved, weak.upgrade()) {
                                this.close_now();
                            }
                        });
                    }
                    _ => {}
                }
            },
        );
        glib::Propagation::Stop
    }

    fn close_now(&self) {
        self.closing.set(true);
        self.window.close();
    }

    /// Full screen hides the header bar until the pointer reaches the top
    /// of the screen; the counts stay at the bottom, and Esc leaves it
    /// (DOC-07). A window always opens in the normal mode.
    fn connect_fullscreen(self: &Rc<Self>, leave: &gtk::Button) {
        let weak = Rc::downgrade(self);
        let leave = leave.downgrade();
        self.window.connect_fullscreened_notify(move |window| {
            let Some(this) = weak.upgrade() else { return };
            let fullscreen = window.is_fullscreen();
            this.toolbar_view.set_reveal_top_bars(!fullscreen);
            if let Some(leave) = leave.upgrade() {
                leave.set_visible(fullscreen);
            }
        });

        let motion = gtk::EventControllerMotion::new();
        let weak: Weak<Self> = Rc::downgrade(self);
        motion.connect_motion(move |_, _, y| {
            let Some(this) = weak.upgrade() else { return };
            if !this.window.is_fullscreen() {
                return;
            }
            let revealed = this.toolbar_view.reveals_top_bars();
            if !revealed && y <= REVEAL_EDGE {
                this.toolbar_view.set_reveal_top_bars(true);
            } else if revealed && y > f64::from(this.header.height()) + REVEAL_EDGE * 4.0 {
                this.toolbar_view.set_reveal_top_bars(false);
            }
        });
        self.window.add_controller(motion);

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let window = self.window.downgrade();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(window) = window.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if key == gdk::Key::Escape
                && modifiers.is_empty()
                && window.is_fullscreen()
                && window.visible_dialog().is_none()
            {
                window.unfullscreen();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.window.add_controller(keys);
    }

    fn add_actions(self: &Rc<Self>) {
        let simple: [(&str, WindowAction); 9] = [
            ("warnings", |this| this.show_warnings()),
            ("save", |this| this.save(|_| {})),
            ("save-as", |this| this.save_as(|_| {})),
            ("save-copy", Self::save_copy),
            ("duplicate", |this| this.duplicate()),
            ("revert", Self::revert),
            ("properties", |this| {
                if let Some(path) = this.path.borrow().as_deref() {
                    crate::properties::show(&this.window, path);
                }
            }),
            ("fullscreen", |this| {
                if this.window.is_fullscreen() {
                    this.window.unfullscreen();
                } else {
                    this.window.fullscreen();
                }
            }),
            ("close", |this| {
                this.window.close();
            }),
        ];
        for (name, run) in simple {
            let weak = Rc::downgrade(self);
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| {
                if let Some(this) = weak.upgrade() {
                    run(&this);
                }
            });
            self.window.add_action(&action);
        }

        let weak = Rc::downgrade(self);
        let zoom = gio::SimpleAction::new("zoom", Some(glib::VariantTy::STRING));
        zoom.connect_activate(move |_, parameter| {
            let (Some(this), Some(value)) =
                (weak.upgrade(), parameter.and_then(|p| p.get::<String>()))
            else {
                return;
            };
            let view = &this.view;
            let zoom = match value.as_str() {
                "in" => geometry::zoom_in(view.zoom()),
                "out" => geometry::zoom_out(view.zoom()),
                "fit-width" => view.fit_width_zoom(),
                "fit-page" => view.fit_page_zoom(),
                level => level.parse().unwrap_or(geometry::DEFAULT_ZOOM),
            };
            view.set_zoom(zoom);
        });
        self.window.add_action(&zoom);

        for (name, run) in [
            ("undo", TextEditor::undo as fn(&TextEditor)),
            ("redo", TextEditor::redo),
            ("cut", TextEditor::cut),
            ("copy", TextEditor::copy),
            ("paste", TextEditor::paste),
            ("select-all", TextEditor::select_all),
        ] {
            let weak = Rc::downgrade(self);
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| {
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    run(&editor);
                }
            });
            self.window.add_action(&action);
        }
        self.add_format_actions();
        self.refresh();
    }

    /// The toggles of the format bar, Show Invisibles and the Inspector,
    /// whose states follow the selection.
    fn add_format_actions(self: &Rc<Self>) {
        for (name, label) in TOGGLES {
            let action = gio::SimpleAction::new_stateful(name, None, &false.to_variant());
            let weak = Rc::downgrade(self);
            action.connect_change_state(move |action, value| {
                let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
                else {
                    return;
                };
                action.set_state(&on.to_variant());
                let Some(editor) = this.editor() else { return };
                let line = Some(if on {
                    LineStyle::Single
                } else {
                    LineStyle::None
                });
                let attrs = match name {
                    "bold" => CharAttrs {
                        bold: Some(on),
                        ..CharAttrs::default()
                    },
                    "italic" => CharAttrs {
                        italic: Some(on),
                        ..CharAttrs::default()
                    },
                    "underline" => CharAttrs {
                        underline: line,
                        ..CharAttrs::default()
                    },
                    _ => CharAttrs {
                        strikethrough: line,
                        ..CharAttrs::default()
                    },
                };
                editor.set_character_attrs(label, &attrs);
            });
            self.window.add_action(&action);
        }

        let align = gio::SimpleAction::new_stateful(
            "align",
            Some(glib::VariantTy::STRING),
            &"left".to_variant(),
        );
        let weak = Rc::downgrade(self);
        align.connect_change_state(move |action, value| {
            let (Some(this), Some(value)) = (weak.upgrade(), value) else {
                return;
            };
            let Some(alignment) = value
                .get::<String>()
                .and_then(|v| formatting::alignment_of(&v))
            else {
                return;
            };
            action.set_state(value);
            if let Some(editor) = this.editor() {
                editor.set_paragraph_attrs(
                    "Alignment",
                    &ParaAttrs {
                        alignment: Some(alignment),
                        ..ParaAttrs::default()
                    },
                );
            }
        });
        self.window.add_action(&align);

        let invisibles = gio::SimpleAction::new_stateful("invisibles", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        invisibles.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            if let Some(editor) = this.editor() {
                editor.set_show_invisibles(on);
            }
        });
        self.window.add_action(&invisibles);

        // The rulers show in a word processing document (INT-02).
        let ruler = gio::SimpleAction::new_stateful("ruler", None, &true.to_variant());
        let weak = Rc::downgrade(self);
        ruler.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            this.show_rulers();
        });
        self.window.add_action(&ruler);

        let inspector = gio::SimpleAction::new_stateful("inspector", None, &false.to_variant());
        let split = self.split.downgrade();
        inspector.connect_change_state(move |action, value| {
            let (Some(split), Some(on)) = (split.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            split.set_show_sidebar(on);
        });
        self.window.add_action(&inspector);

        let fonts = gio::SimpleAction::new("fonts", None);
        let weak = Rc::downgrade(self);
        fonts.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            let Some(editor) = this.editor() else { return };
            let cell = this.editor.clone();
            let window = this
                .fonts
                .present(&this.window, Rc::new(move || cell.borrow().clone()));
            let format = editor.format();
            window.show(&format.character, &format.paragraph);
        });
        self.window.add_action(&fonts);
        // The sidebar also closes by itself on a narrow window.
        let window = self.window.downgrade();
        self.split.connect_show_sidebar_notify(move |split| {
            if let Some(window) = window.upgrade() {
                formatting::set_state(&window, "inspector", split.shows_sidebar().to_variant());
            }
        });
    }

    /// A menu of the document at the pointer, on a secondary click or the
    /// Menu key (INT-21).
    fn connect_context_menu(&self) {
        let menu = gio::Menu::new();
        let edit = gio::Menu::new();
        edit.append(Some(&tr("Cu_t")), Some("win.cut"));
        edit.append(Some(&tr("_Copy")), Some("win.copy"));
        edit.append(Some(&tr("_Paste")), Some("win.paste"));
        edit.append(Some(&tr("Select _All")), Some("win.select-all"));
        menu.append_section(None, &edit);
        let format = gio::Menu::new();
        format.append(Some(&tr("_Bold")), Some("win.bold"));
        format.append(Some(&tr("_Italic")), Some("win.italic"));
        format.append(Some(&tr("_Underline")), Some("win.underline"));
        menu.append_section(None, &format);
        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(self.view.area());
        popover.set_has_arrow(false);
        popover.set_halign(gtk::Align::Start);

        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        let menu_popover = popover.downgrade();
        click.connect_pressed(move |gesture, _, x, y| {
            if let Some(popover) = menu_popover.upgrade() {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            }
        });
        self.view.area().add_controller(click);

        let keys = gtk::EventControllerKey::new();
        let menu_popover = popover.downgrade();
        let area = self.view.area().downgrade();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let menu_key = key == gdk::Key::Menu
                || (key == gdk::Key::F10 && modifiers == gdk::ModifierType::SHIFT_MASK);
            if let (true, Some(popover), Some(area)) =
                (menu_key, menu_popover.upgrade(), area.upgrade())
            {
                // At the middle of the visible area.
                let (width, height) = (area.width() / 2, area.height() / 3);
                popover.set_pointing_to(Some(&gdk::Rectangle::new(width, height, 1, 1)));
                popover.popup();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.view.area().add_controller(keys);
    }
}

/// The main menu of the header bar.
fn main_menu(info: AppInfo, opens: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    let file = gio::Menu::new();
    file.append(Some(&tr("_New Window")), Some("app.new-window"));
    if opens {
        file.append(Some(&tr("_Open…")), Some("app.open"));
    }
    menu.append_section(None, &file);
    if opens {
        let save = gio::Menu::new();
        save.append(Some(&tr("_Save")), Some("win.save"));
        save.append(Some(&tr("Save _As…")), Some("win.save-as"));
        save.append(Some(&tr("Save a _Copy…")), Some("win.save-copy"));
        save.append(Some(&tr("_Duplicate")), Some("win.duplicate"));
        save.append(Some(&tr("_Revert to Saved")), Some("win.revert"));
        menu.append_section(None, &save);
        let document = gio::Menu::new();
        document.append(Some(&tr("Show _Fonts")), Some("win.fonts"));
        document.append(Some(&tr("Show _Ruler")), Some("win.ruler"));
        document.append(Some(&tr("_Fullscreen")), Some("win.fullscreen"));
        document.append(Some(&tr("Document _Info")), Some("win.properties"));
        menu.append_section(None, &document);
    }
    let close = gio::Menu::new();
    close.append(Some(&tr("C_lose")), Some("win.close"));
    close.append(Some(&tr("Close _All Windows")), Some("app.close-all"));
    menu.append_section(None, &close);
    let help = gio::Menu::new();
    help.append(Some(&tr("_Preferences")), Some("app.preferences"));
    help.append(
        Some(&tr("_Keyboard Shortcuts")),
        Some("win.show-help-overlay"),
    );
    help.append(
        Some(&tr("_About {app}").replace("{app}", info.name)),
        Some("app.about"),
    );
    menu.append_section(None, &help);
    menu
}

/// The counts at the bottom of the window.
fn counts(words: usize, pages: usize) -> String {
    let words = trn("{n} word", "{n} words", words as u64).replace("{n}", &words.to_string());
    let pages = trn("{n} page", "{n} pages", pages as u64).replace("{n}", &pages.to_string());
    format!("{words} · {pages}")
}

/// A path with the extension of the kind of file saved, unless it has it.
fn with_extension(path: PathBuf, extension: &str) -> PathBuf {
    let has = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(extension));
    if has {
        path
    } else {
        let mut name = path.file_name().unwrap_or_default().to_owned();
        name.push(".");
        name.push(extension);
        path.with_file_name(name)
    }
}

/// The zoom menu at the bottom of the window (INT-07).
fn zoom_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let levels = gio::Menu::new();
    for level in geometry::ZOOM_LEVELS {
        let item = gio::MenuItem::new(Some(&geometry::zoom_label(level)), None);
        item.set_action_and_target_value(Some("win.zoom"), Some(&level.to_string().to_variant()));
        levels.append_item(&item);
    }
    menu.append_section(None, &levels);
    let fit = gio::Menu::new();
    for (label, target) in [
        (tr("Fit _Width"), "fit-width"),
        (tr("Fit _Page"), "fit-page"),
    ] {
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.zoom"), Some(&target.to_variant()));
        fit.append_item(&item);
    }
    menu.append_section(None, &fit);
    menu
}

/// The name of a document as the title of its window. A package of Pages
/// is a folder; its name is the document's.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The folder of a document, with the home folder as `~`.
fn folder_of(path: &Path) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    let home = glib::home_dir();
    match parent.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => parent.display().to_string(),
    }
}

/// Adds a document to the recent files of the desktop (DOC-03).
fn remember_recent(path: &Path) {
    let file = gio::File::for_path(path);
    gtk::RecentManager::default().add_item(&file.uri());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_09_a_saved_file_gets_the_extension_of_its_kind() {
        assert_eq!(
            with_extension(PathBuf::from("/d/Letter"), "odt"),
            PathBuf::from("/d/Letter.odt")
        );
        assert_eq!(
            with_extension(PathBuf::from("/d/Letter.ODT"), "odt"),
            PathBuf::from("/d/Letter.ODT")
        );
        // A Pages document saved again becomes a new ODT next to it.
        assert_eq!(
            with_extension(PathBuf::from("/d/Report.pages"), "odt"),
            PathBuf::from("/d/Report.pages.odt")
        );
    }

    #[test]
    fn doc_07_the_counts_name_words_and_pages() {
        assert_eq!(counts(1, 1), "1 word · 1 page");
        assert_eq!(counts(0, 3), "0 words · 3 pages");
    }
}
