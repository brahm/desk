// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A document window: the header bar, the document area, and the zoom
//! menu and counts at the bottom, as in Pages. It saves the document, asks
//! to save it before it closes, and shows it full screen.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use adw::prelude::*;
use desk_model::{CharAttrs, Document, Field, LineStyle, ParaAttrs};
use gtk::{gdk, gio, glib};

use crate::editing::{BreakKind, FieldKind, TextEditor};
use crate::find::{self, FindWindow, Search, SearchSidebar};
use crate::fonts::FontsSlot;
use crate::formatting::{self, EditorOf, FormatBar, Inspector};
use crate::geometry;
use crate::i18n::{n_, tr, trn};
use crate::preferences;
use crate::ruler::{self, Ruler};
use crate::spelling::MistakeKind;
use crate::styles_drawer::StylesDrawer;
use crate::view::DocumentView;
use crate::writing_tools::{self, Lookup};
use crate::{AppInfo, Opener};

pub(crate) use objects::OBJECT_ACCELS;

mod objects;

/// How far from the top of the screen the pointer brings the header bar
/// back in full screen, in pixels.
const REVEAL_EDGE: f64 = 4.0;

/// What an action of the window does.
type WindowAction = fn(&Rc<DocumentWindow>);
/// What an action of the menus does to the editor of a window.
type EditorAction = fn(&TextEditor);

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
    /// Says that the document has warnings, with a button to see them.
    warnings_banner: adw::Banner,
    /// The file of the document; none for an untitled one.
    path: RefCell<Option<PathBuf>>,
    /// The name of the document, which is its file's unless untitled.
    name: RefCell<String>,
    warnings: RefCell<Vec<crate::Warning>>,
    editor: Rc<RefCell<Option<TextEditor>>>,
    format_bar: Rc<FormatBar>,
    inspector: Rc<Inspector>,
    /// The Styles drawer (INT-18).
    drawer: Rc<StylesDrawer>,
    drawer_split: adw::OverlaySplitView,
    /// The Fonts window (INT-15), made when first shown.
    fonts: FontsSlot,
    split: adw::OverlaySplitView,
    /// The search of the window (TXT-12, TXT-13).
    search: Rc<Search>,
    /// The Find & Replace window, made when first shown.
    find: Rc<FindWindow>,
    search_sidebar: Rc<SearchSidebar>,
    search_split: adw::OverlaySplitView,
    navigate_by: gtk::MenuButton,
    page_entry: gtk::Entry,
    thumbnails: Rc<crate::thumbnails::Thumbnails>,
    outline: Rc<crate::outline_view::OutlineView>,
    thumbnails_split: adw::OverlaySplitView,
    /// The pages of the right sidebar: the Inspector, the Media Browser
    /// and Spelling and Grammar, one at a time.
    side: gtk::Stack,
    /// Spelling and Grammar, made when first shown.
    spelling: std::cell::OnceCell<Rc<crate::spelling_window::SpellingWindow>>,
    /// The Media Browser, made when first shown (INT-14).
    media_browser: std::cell::OnceCell<Rc<crate::media_browser::MediaBrowser>>,
    format_revealer: gtk::Revealer,
    ruler: Ruler,
    vertical_ruler: Ruler,
    /// The corner where the rulers meet.
    ruler_corner: gtk::Box,
    /// Whether closing was asked for and answered, so that the window
    /// closes without asking again.
    closing: Cell<bool>,
    /// The recovery draft of the document, once one is written.
    draft: RefCell<Option<crate::recovery::Draft>>,
    /// Whether the document changed since the last draft.
    draft_due: Cell<bool>,
}

thread_local! {
    /// The open windows, so that a file opened from outside goes to an
    /// empty window when there is one.
    static WINDOWS: RefCell<Vec<Rc<DocumentWindow>>> = const { RefCell::new(Vec::new()) };
}

/// The actions that need a document.
const DOCUMENT_ACTIONS: [&str; 63] = [
    "outline",
    "colors",
    "format-bar",
    "navigate",
    "thumbnails",
    "delete-page",
    "special-characters",
    "save",
    "save-as",
    "save-copy",
    "duplicate",
    "fullscreen",
    "select-all",
    "cut",
    "copy",
    "paste",
    "paste-match-style",
    "bold",
    "italic",
    "underline",
    "strikethrough",
    "align",
    "inspector",
    "fonts",
    "styles-drawer",
    "copy-paragraph-style",
    "copy-character-style",
    "import-styles",
    "insert-page-break",
    "insert-column-break",
    "insert-layout-break",
    "insert-section-break",
    "insert-page-number",
    "insert-auto-page-numbers",
    "columns",
    "insert-page-count",
    "insert-date",
    "insert-file-name",
    "insert-note",
    "next-note",
    "previous-note",
    "insert-toc",
    "update-toc",
    "insert-link",
    "insert-bookmark",
    "find",
    "find-next",
    "find-previous",
    "use-selection-for-find",
    "jump-to-selection",
    "search",
    "recent-search",
    "look-up-dictionary",
    "look-up-web",
    "look-up-wikipedia",
    "statistics",
    "check-spelling",
    "spelling",
    "correct-spelling",
    "ignore-spelling",
    "learn-spelling",
    "never-hyphenate",
    "allow-hyphenation",
];

thread_local! {
    /// The style that Copy Style took, which Paste Style puts on text of
    /// any window (STY-08).
    static COPIED_STYLE: RefCell<Option<desk_model::CopiedStyle>> = const { RefCell::new(None) };
}

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
        // The View and Insert menus at the start, and the toggles of the
        // right sidebar at the end, for a document only.
        let view_button = gtk::MenuButton::builder()
            .label(tr("_View"))
            .use_underline(true)
            .menu_model(&view_menu())
            .tooltip_text(tr("View"))
            .visible(false)
            .build();
        view_button.set_always_show_arrow(true);
        let insert_button = gtk::MenuButton::builder()
            .label(tr("_Insert"))
            .use_underline(true)
            .menu_model(&insert_menu())
            .tooltip_text(tr("Insert"))
            .visible(false)
            .build();
        insert_button.set_always_show_arrow(true);
        let media_toggle = gtk::ToggleButton::builder()
            .icon_name("folder-pictures-symbolic")
            .action_name("win.media-browser")
            .tooltip_text(tr("Media Browser"))
            .visible(false)
            .build();
        formatting::labelled(&media_toggle, &tr("Media Browser"));
        let inspector_toggle = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-right-symbolic")
            .action_name("win.inspector")
            .tooltip_text(tr("Inspector"))
            .visible(false)
            .build();
        formatting::labelled(&inspector_toggle, &tr("Inspector"));
        let document_buttons: [gtk::Widget; 4] = [
            view_button.clone().upcast(),
            insert_button.clone().upcast(),
            media_toggle.clone().upcast(),
            inspector_toggle.clone().upcast(),
        ];
        header.pack_start(&view_button);
        header.pack_start(&insert_button);
        header.pack_end(&inspector_toggle);
        header.pack_end(&media_toggle);
        let leave_fullscreen = gtk::Button::builder()
            .icon_name("view-restore-symbolic")
            .tooltip_text(tr("Leave Fullscreen"))
            .action_name("win.fullscreen")
            .visible(false)
            .build();
        formatting::labelled(&leave_fullscreen, &tr("Leave Fullscreen"));
        header.pack_end(&leave_fullscreen);
        // Full screen has its own background, as Pages does (DOC-07).
        let backdrop = gtk::ColorDialogButton::builder()
            .dialog(&gtk::ColorDialog::builder().title(tr("Background")).build())
            .rgba(&gdk::RGBA::new(0.25, 0.25, 0.25, 1.0))
            .tooltip_text(tr("Background"))
            .valign(gtk::Align::Center)
            .visible(false)
            .build();
        formatting::labelled(&backdrop, &tr("Background"));
        header.pack_end(&backdrop);
        let warnings_banner = adw::Banner::builder()
            .title(tr("Some of the document might not show as it was made"))
            .button_label(tr("_See Warnings"))
            .action_name("win.warnings")
            .build();

        let status = adw::StatusPage::builder()
            .icon_name(info.id)
            .title(info.name)
            .build();
        if opener.is_some() {
            // The template gallery, as the window shows when it has no
            // document (INT-01, INT-03).
            status.set_icon_name(None);
            status.set_title(&tr("Choose a Template"));
            status.set_description(Some(&tr(
                "Start a new document, or open an OpenDocument text or a Pages document.",
            )));
            status.set_child(Some(&template_gallery()));
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
        // The arrows, what they go to, and Go to Page (INT-19).
        let previous = gtk::Button::builder()
            .icon_name("go-up-symbolic")
            .tooltip_text(tr("Previous"))
            .action_name("win.navigate")
            .action_target(&false.to_variant())
            .css_classes(["flat"])
            .build();
        let next = gtk::Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text(tr("Next"))
            .action_name("win.navigate")
            .action_target(&true.to_variant())
            .css_classes(["flat"])
            .build();
        let navigate_by = gtk::MenuButton::builder()
            .label(tr("Page"))
            .menu_model(&navigate_menu())
            .direction(gtk::ArrowType::Up)
            .tooltip_text(tr("Go To"))
            .css_classes(["flat"])
            .build();
        let page_entry = gtk::Entry::builder()
            .width_chars(4)
            .max_width_chars(5)
            .input_purpose(gtk::InputPurpose::Digits)
            .xalign(0.5)
            .tooltip_text(tr("Go to Page"))
            .valign(gtk::Align::Center)
            .build();
        crate::formatting::labelled(&page_entry, &tr("Go to Page"));
        bottom.append(&previous);
        bottom.append(&next);
        bottom.append(&navigate_by);
        bottom.append(&page_entry);
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
        let drawer = StylesDrawer::new(editor_of());
        let search = Rc::new(Search::default());
        let search_sidebar = SearchSidebar::new(editor_of(), search.clone());
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
        // The document as an outline (TXT-14).
        let outline = crate::outline_view::OutlineView::new(editor_of());
        stack.add_named(outline.widget(), Some("outline"));
        // The format bar scrolls when the window is narrower than it.
        let format_scroller = gtk::ScrolledWindow::builder()
            .child(format_bar.widget())
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .build();
        let format_revealer = gtk::Revealer::builder()
            .child(&format_scroller)
            .reveal_child(false)
            .build();
        // The page thumbnails, beside the pages (INT-19, EST-17).
        let thumbnails = crate::thumbnails::Thumbnails::new(&view, editor_of());
        let thumbnails_split = adw::OverlaySplitView::builder()
            .content(&stack)
            .sidebar(thumbnails.widget())
            .sidebar_position(gtk::PackType::Start)
            .show_sidebar(false)
            .build();
        let search_split = adw::OverlaySplitView::builder()
            .content(&thumbnails_split)
            .sidebar(search_sidebar.widget())
            .sidebar_position(gtk::PackType::End)
            .show_sidebar(false)
            .build();
        let side = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        side.add_named(inspector.widget(), Some("inspector"));
        let split = adw::OverlaySplitView::builder()
            .content(&search_split)
            .sidebar(&side)
            .min_sidebar_width(300.0)
            .sidebar_position(gtk::PackType::End)
            .show_sidebar(false)
            .build();
        let drawer_split = adw::OverlaySplitView::builder()
            .content(&split)
            .sidebar(drawer.widget())
            .sidebar_position(gtk::PackType::Start)
            .show_sidebar(false)
            .build();

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.add_top_bar(&warnings_banner);
        toolbar_view.add_top_bar(&format_revealer);
        let find = FindWindow::new(editor_of(), search.clone());
        toolbar_view.add_top_bar(find.widget());
        toolbar_view.set_content(Some(&drawer_split));
        toolbar_view.add_bottom_bar(&bottom_bar);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(info.name)
            .default_width(900)
            .default_height(700)
            .content(&toolbar_view)
            .build();

        // On a narrow window the sidebars cover the pages instead of
        // pushing them, and the format bar keeps what matters most.
        let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            860.0,
            adw::LengthUnit::Sp,
        ));
        for split in [&thumbnails_split, &search_split, &split, &drawer_split] {
            narrow.add_setter(split, "collapsed", Some(&true.to_value()));
        }
        {
            let bar = format_bar.clone();
            narrow.connect_apply(move |_| bar.set_compact(true));
            let bar = format_bar.clone();
            narrow.connect_unapply(move |_| bar.set_compact(false));
        }
        window.add_breakpoint(narrow);
        // One sidebar a side when they cover the pages.
        for (a, b) in [(&thumbnails_split, &drawer_split), (&search_split, &split)] {
            for (one, other) in [(a, b), (b, a)] {
                let other = other.downgrade();
                one.connect_show_sidebar_notify(move |one| {
                    if let Some(other) = other.upgrade()
                        && one.is_collapsed()
                        && one.shows_sidebar()
                    {
                        other.set_show_sidebar(false);
                    }
                });
            }
        }
        window.set_width_request(360);
        window.set_height_request(294);

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
            warnings_banner,
            path: RefCell::new(None),
            name: RefCell::new(String::new()),
            warnings: RefCell::new(Vec::new()),
            editor,
            format_bar,
            inspector,
            drawer,
            drawer_split,
            fonts: FontsSlot::default(),
            split,
            search,
            find,
            search_sidebar,
            search_split,
            navigate_by,
            page_entry,
            thumbnails,
            thumbnails_split,
            outline,
            side,
            spelling: std::cell::OnceCell::new(),
            media_browser: std::cell::OnceCell::new(),
            format_revealer,
            ruler,
            vertical_ruler,
            ruler_corner,
            closing: Cell::new(false),
            draft: RefCell::new(None),
            draft_due: Cell::new(false),
        });
        this.add_actions();
        this.show_rulers();
        {
            let current = preferences::get();
            this.view.set_zoom(f64::from(current.zoom) / 100.0);
            this.zoom_button
                .set_label(&geometry::zoom_label(this.view.zoom()));
            this.show_window_preferences(&current);
        }
        let weak = Rc::downgrade(&this);
        preferences::connect_changed(move |p| {
            if let Some(this) = weak.upgrade() {
                this.show_rulers();
                this.show_window_preferences(p);
                formatting::set_state(
                    &this.window,
                    "check-spelling-while-typing",
                    p.check_spelling.to_variant(),
                );
                formatting::set_state(
                    &this.window,
                    "proofread-while-typing",
                    p.proofread.to_variant(),
                );
                if let Some(editor) = this.editor() {
                    editor.update_squiggles();
                }
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
            // The page in sight shows in Go to Page.
            let weak = Rc::downgrade(&this);
            this.view.connect_moved(move || {
                if let Some(this) = weak.upgrade()
                    && !this.page_entry.has_focus()
                    && let Some(page) = this.view.current_page()
                {
                    this.page_entry.set_text(&(page + 1).to_string());
                }
            });
            let weak = Rc::downgrade(&this);
            this.page_entry.connect_activate(move |entry| {
                let Some(this) = weak.upgrade() else {
                    return;
                };
                let count = this.view.page_count();
                if let (Ok(page), Some(editor)) =
                    (entry.text().trim().parse::<usize>(), this.editor())
                    && (1..=count).contains(&page)
                {
                    editor.go_to_page(page - 1);
                    editor.focus();
                }
            });
        }
        {
            let bar = bottom_bar.downgrade();
            this.stack.connect_visible_child_name_notify(move |stack| {
                let document = stack.visible_child_name().as_deref() != Some("empty");
                if let Some(bar) = bar.upgrade() {
                    bar.set_revealed(document);
                }
                for button in &document_buttons {
                    button.set_visible(document);
                }
            });
        }
        this.connect_fullscreen(&leave_fullscreen, &backdrop);
        this.connect_context_menu();
        {
            let window = this.window.downgrade();
            let opens = opener.is_some();
            let shortcuts = gio::SimpleAction::new("shortcuts", None);
            shortcuts.connect_activate(move |_, _| {
                if let Some(window) = window.upgrade() {
                    crate::shortcuts::dialog(opens).present(Some(&window));
                }
            });
            this.window.add_action(&shortcuts);
        }
        {
            let weak = Rc::downgrade(&this);
            this.window.connect_close_request(move |_| {
                weak.upgrade().map_or(glib::Propagation::Proceed, |this| {
                    let propagation = this.close_request();
                    if propagation == glib::Propagation::Proceed {
                        this.drop_draft();
                    }
                    propagation
                })
            });
        }
        {
            let weak = Rc::downgrade(&this);
            glib::timeout_add_seconds_local(crate::recovery::INTERVAL_SECONDS, move || {
                let Some(this) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                this.keep_draft();
                glib::ControlFlow::Continue
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.window.connect_visible_dialog_notify(move |_| {
                // Once the dialog has gone.
                let weak = weak.clone();
                glib::idle_add_local_once(move || {
                    if let Some(this) = weak.upgrade() {
                        this.refresh();
                    }
                });
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

    /// The window opened last.
    pub fn last() -> Option<Rc<Self>> {
        WINDOWS.with(|w| w.borrow().last().cloned())
    }

    fn editor(&self) -> Option<TextEditor> {
        self.editor.borrow().clone()
    }

    /// The file of the document as fields of the file name show it: its
    /// path, or its name until it has one.
    fn file(&self) -> String {
        self.path.borrow().as_deref().map_or_else(
            || self.name.borrow().clone(),
            |p| p.to_string_lossy().into_owned(),
        )
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
                // A banner says when something may not show as it was made;
                // fonts with the same metrics in place of others are only
                // in Document Warnings of the main menu.
                self.warnings_banner.set_revealed(shown.lossy);
                *self.warnings.borrow_mut() = shown.warnings;
                self.enable_warnings();
                remember_recent(path);
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

    /// How the document shows now.
    fn current_view(&self) -> desk_model::ViewSettings {
        let zoom = match self.view.fit() {
            Some(geometry::Fit::Width) => desk_model::ViewZoom::FitWidth,
            Some(geometry::Fit::Page) => desk_model::ViewZoom::FitPage,
            None => desk_model::ViewZoom::Percent((self.view.zoom() * 100.0).round() as u16),
        };
        desk_model::ViewSettings {
            zoom,
            two_pages: self.view.columns() == 2,
            outline: self.action_state("outline"),
            outline_options: self.outline.options(),
        }
    }

    /// Shows a document as it showed when it was saved.
    fn show_view(&self, view: desk_model::ViewSettings) {
        let layout = if view.two_pages { "two" } else { "one" };
        if let Some(action) = self.window.lookup_action("page-layout") {
            action.activate(Some(&layout.to_variant()));
        }
        match view.zoom {
            desk_model::ViewZoom::FitWidth => self.view.set_fit(Some(geometry::Fit::Width)),
            desk_model::ViewZoom::FitPage => self.view.set_fit(Some(geometry::Fit::Page)),
            desk_model::ViewZoom::Percent(p) => self.view.set_zoom(f64::from(p) / 100.0),
        }
        self.outline.set_options(view.outline_options);
        if let Some(action) = self.window.lookup_action("outline") {
            action.change_state(&view.outline.to_variant());
        }
    }

    fn show_document(self: &Rc<Self>, document: Document) {
        if let Some(view) = document.view() {
            self.show_view(view);
        }
        let file = self.file();
        let document = crate::fields::opened(document, &file);
        let editor = TextEditor::new(&self.view, document);
        editor.set_file_name(&file);
        let weak = Rc::downgrade(self);
        editor.connect_changed(move || {
            if let Some(this) = weak.upgrade() {
                this.draft_due.set(true);
                this.refresh();
                this.search_sidebar.document_changed();
                this.thumbnails.refresh();
                this.outline.document_changed();
            }
        });
        let weak = Rc::downgrade(self);
        editor.connect_selection_changed(move || {
            if let Some(this) = weak.upgrade() {
                this.show_format();
            }
        });
        editor.set_show_invisibles(self.action_state("invisibles"));
        editor.set_show_layout(self.action_state("show-layout"));
        *self.editor.borrow_mut() = Some(editor);
        self.stack.set_visible_child_name("document");
        // A document saved as an outline opens as one (DOC-16).
        if self.action_state("outline") {
            self.show_outline(true);
        }
        // A page layout document opens with its thumbnails and both
        // rulers, as in Pages (INT-03).
        if self.editor().is_some_and(|e| e.is_page_layout()) {
            self.window.change_action_state("ruler", &true.to_variant());
            self.show_rulers();
            // The sidebar opens once the window has its size.
            let weak = Rc::downgrade(self);
            glib::idle_add_local_once(move || {
                if let Some(this) = weak.upgrade() {
                    this.window
                        .change_action_state("thumbnails", &true.to_variant());
                }
            });
        }
        self.format_revealer
            .set_reveal_child(self.action_state("format-bar"));
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
        if self.drawer_split.shows_sidebar() {
            self.drawer.show(&editor);
        }
        if let Some(fonts) = self.fonts.visible() {
            fonts.show(&format.character, &format.paragraph);
        }
        let setup = *editor.document().page_setup();
        self.ruler.show(setup, format.layout, &format.paragraph);
        self.vertical_ruler
            .show(setup, format.layout, &format.paragraph);
        let character = &format.character;
        for (name, on) in [
            ("bold", character.weight.is_bold()),
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
        formatting::set_state(
            &self.window,
            "columns",
            i32::from(format.layout.columns.count).to_variant(),
        );
    }

    /// Shows what the preferences say about the window: the counts at
    /// the bottom, the color of invisibles and that of the guides.
    fn show_window_preferences(&self, preferences: &preferences::Preferences) {
        self.counts_label.set_visible(preferences.show_counts);
        self.view.set_invisibles_color(preferences.invisibles_color);
        self.view.set_guide_color(preferences.guide_color);
    }

    /// Shows the rulers that Show Ruler and the preferences ask for.
    fn show_rulers(&self) {
        let on = self.action_state("ruler");
        let page_layout = self.editor().is_some_and(|e| e.is_page_layout());
        let vertical = on && (page_layout || preferences::get().vertical_ruler);
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
            if self.format_bar.set_styles(&editor.paragraph_styles()) {
                self.show_format();
            }
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
        // A dialog over the window keeps the keys of editing, such as
        // Ctrl+A in a field, from reaching the document.
        let free = self.window.visible_dialog().is_none();
        let has_document = editor.is_some() && free;
        let modified = editor.as_ref().is_some_and(TextEditor::is_modified);
        for (action, enabled) in [
            (
                "undo",
                free && editor.as_ref().is_some_and(TextEditor::can_undo),
            ),
            (
                "redo",
                free && editor.as_ref().is_some_and(TextEditor::can_redo),
            ),
            ("revert", path.is_some() && modified),
            ("properties", path.is_some()),
        ] {
            self.enable(action, enabled);
        }
        for action in DOCUMENT_ACTIONS.iter().chain(&objects::OBJECT_ACTIONS) {
            self.enable(action, has_document);
        }
        // Paste Style is off until a style is copied, as in Pages.
        let copied = COPIED_STYLE.with(|c| c.borrow().is_some());
        self.enable("paste-style", has_document && copied);
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

    fn show_warnings(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let replace = move |from: &str, to: &str| {
            let Some(this) = weak.upgrade() else {
                return false;
            };
            let Some(editor) = this.editor() else {
                return false;
            };
            editor.replace_font(from, to);
            this.warnings
                .borrow_mut()
                .retain(|w| w.font.as_deref() != Some(from));
            if this.warnings.borrow().is_empty() {
                this.warnings_banner.set_revealed(false);
            }
            this.enable_warnings();
            true
        };
        crate::warnings::show(
            &self.window,
            &self.name.borrow(),
            &self.warnings.borrow(),
            Rc::new(replace),
        );
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
        // Tables of contents are up to date in what is kept, as Pages
        // updates them when a document closes (EST-22).
        editor.update_tables_of_contents();
        let pages = editor.pages();
        let mut document = crate::fields::saved(editor.document(), &path.to_string_lossy());
        // How it shows, to reopen the same way (INT-08, DOC-16).
        document.set_view(Some(self.current_view()));
        // A picture of the first page, unless the preferences leave it out
        // (DOC-09).
        let preview = pages
            .first()
            .filter(|_| crate::preferences::get().include_preview);
        match crate::save_document(opener, &document, preview, path) {
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

    /// Writes the recovery draft of the document, when it has changes
    /// that are not saved nor in the draft yet.
    fn keep_draft(&self) {
        let (Some(opener), Some(editor)) = (self.opener, self.editor()) else {
            return;
        };
        if !self.draft_due.get() || !editor.is_modified() {
            return;
        }
        let draft = self
            .draft
            .borrow_mut()
            .get_or_insert_with(|| crate::recovery::Draft::new(opener.saves.1))
            .clone();
        let written = draft.name_file(&self.name.borrow()).is_ok()
            && crate::save_document(opener, &editor.document(), None, &draft.path).is_ok();
        if written {
            self.draft_due.set(false);
        }
    }

    /// Document Warnings is there when the document has warnings.
    fn enable_warnings(&self) {
        if let Some(action) = self
            .window
            .lookup_action("warnings")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(!self.warnings.borrow().is_empty());
        }
    }

    /// Drops the recovery draft, when what it kept is saved or given up.
    fn drop_draft(&self) {
        if let Some(draft) = self.draft.borrow_mut().take() {
            draft.remove();
        }
        self.draft_due.set(false);
    }

    /// Shows a document that a recovery draft kept, as a new document
    /// with the name it had.
    pub fn recover(self: &Rc<Self>, draft: &crate::recovery::Draft) -> bool {
        let Some(opener) = self.opener else {
            return false;
        };
        match crate::open_document(opener, &draft.path) {
            Ok(shown) => {
                let name = draft.name();
                let name = if name.is_empty() {
                    tr("Untitled")
                } else {
                    name
                };
                self.load_untitled(
                    shown.document,
                    tr("{name} (recovered)").replace("{name}", &name),
                );
                draft.remove();
                true
            }
            Err(_) => false,
        }
    }

    fn saved_as(&self, path: &Path) {
        self.drop_draft();
        *self.path.borrow_mut() = Some(path.to_owned());
        *self.name.borrow_mut() = display_name(path);
        if let Some(editor) = self.editor() {
            editor.set_file_name(&self.file());
            editor.refresh_fields();
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

    /// Asks for a document, then for which of its styles to bring into
    /// this one, and whether they replace the styles with their names
    /// (STY-10).
    fn import_styles(self: &Rc<Self>) {
        let Some(opener) = self.opener else { return };
        let (filters, all) = crate::document_filters(opener);
        let dialog = gtk::FileDialog::builder()
            .title(tr("Import Styles"))
            .modal(true)
            .filters(&filters)
            .default_filter(&all)
            .build();
        let weak = Rc::downgrade(self);
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |result| {
            let (Some(this), Ok(file)) = (weak.upgrade(), result) else {
                return;
            };
            let Some(path) = file.path() else { return };
            match (opener.open)(&crate::package_of(&path)) {
                Ok(opened) => this.choose_styles(opened.document, &display_name(&path)),
                Err(message) => this.alert(
                    &tr("Cannot open {name}").replace("{name}", &display_name(&path)),
                    &message,
                ),
            }
        });
    }

    /// Asks which styles of a document to import.
    fn choose_styles(self: &Rc<Self>, source: Document, name: &str) {
        let dialog = adw::AlertDialog::new(
            Some(&tr("Import Styles")),
            Some(&tr("Choose the styles of “{name}” to import.").replace("{name}", name)),
        );
        // The styles in groups by their kind, none chosen at first, as in
        // Pages: Import stays off until one is.
        let groups = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        let mut checks = Vec::new();
        for (family, heading) in [
            (desk_model::StyleFamily::Paragraph, tr("Paragraph Styles")),
            (desk_model::StyleFamily::Character, tr("Character Styles")),
            (desk_model::StyleFamily::List, tr("List Styles")),
        ] {
            let styles = crate::styles_drawer::sorted(source.styles(), family);
            if styles.is_empty() {
                continue;
            }
            let label = gtk::Label::builder()
                .label(heading)
                .xalign(0.0)
                .css_classes(["heading"])
                .margin_top(if checks.is_empty() { 0 } else { 12 })
                .build();
            let list = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .css_classes(["boxed-list"])
                .build();
            for style in styles {
                let check = gtk::CheckButton::new();
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(style.name()))
                    .activatable_widget(&check)
                    .build();
                row.add_prefix(&check);
                list.append(&row);
                checks.push((style.id(), check));
            }
            groups.append(&label);
            groups.append(&list);
        }
        let checks = Rc::new(checks);
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(320)
            .child(&groups)
            .build();
        let select_all = gtk::Button::builder()
            .label(tr("_Select All"))
            .use_underline(true)
            .build();
        let deselect_all = gtk::Button::builder()
            .label(tr("_Deselect All"))
            .use_underline(true)
            .build();
        for (button, active) in [(&select_all, true), (&deselect_all, false)] {
            let checks = Rc::clone(&checks);
            button.connect_clicked(move |_| {
                for (_, check) in checks.iter() {
                    check.set_active(active);
                }
            });
        }
        let buttons = gtk::Box::builder()
            .spacing(6)
            .halign(gtk::Align::End)
            .margin_top(12)
            .build();
        buttons.append(&select_all);
        buttons.append(&deselect_all);
        let replace = adw::SwitchRow::builder()
            .title(tr("Replace Duplicates"))
            .subtitle(tr("Styles with the same names take the imported ones"))
            .build();
        let options = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .margin_top(12)
            .build();
        options.append(&replace);
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        content.append(&scrolled);
        content.append(&buttons);
        content.append(&options);
        dialog.set_extra_child(Some(&content));
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("import", &tr("_Import"));
        dialog.set_response_appearance("import", adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled("import", false);
        dialog.set_default_response(Some("import"));
        dialog.set_close_response("cancel");
        for (_, check) in checks.iter() {
            let (dialog, checks) = (dialog.downgrade(), Rc::downgrade(&checks));
            check.connect_toggled(move |_| {
                if let (Some(dialog), Some(checks)) = (dialog.upgrade(), checks.upgrade()) {
                    let any = checks.iter().any(|(_, c)| c.is_active());
                    dialog.set_response_enabled("import", any);
                }
            });
        }
        let weak = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            let (Some(this), "import") = (weak.upgrade(), response) else {
                return;
            };
            let Some(editor) = this.editor() else { return };
            let ids: Vec<_> = checks
                .iter()
                .filter(|(_, check)| check.is_active())
                .map(|(id, _)| *id)
                .collect();
            if ids.is_empty() {
                return;
            }
            let replace = replace.is_active();
            if let Err(error) = editor.style_edit(&tr("Import Styles"), |edit, _, _| {
                edit.import_styles(&source, &ids, replace)
            }) {
                this.alert(&tr("Cannot Import the Styles"), &error.to_string());
            }
        });
        let weak = Rc::downgrade(self);
        dialog.connect_closed(move |_| {
            if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                editor.focus();
            }
        });
        dialog.present(Some(&self.window));
    }

    /// Opens the document that a link points to, in its window if it is
    /// open already, and goes to its bookmark (EST-25).
    fn open_linked(self: &Rc<Self>, link: &str) {
        let crate::links::Target::Document { file, bookmark } = crate::links::Target::parse(link)
        else {
            return;
        };
        let folder = self
            .path
            .borrow()
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_owned);
        let Some((path, bookmark)) =
            crate::links::document_file(&file, &bookmark, folder.as_deref())
        else {
            self.alert(
                &tr("Cannot Follow Link"),
                &tr("Save this document first: the link names a file from its folder."),
            );
            return;
        };
        let open = WINDOWS.with(|w| {
            w.borrow()
                .iter()
                .find(|w| w.path.borrow().as_deref() == Some(path.as_path()))
                .cloned()
        });
        let window = match open {
            Some(window) => window,
            None => {
                let (Some(opener), Some(app)) = (
                    self.opener,
                    self.window
                        .application()
                        .and_then(|a| a.downcast::<adw::Application>().ok()),
                ) else {
                    return;
                };
                let window = Self::new(&app, self.info, Some(opener));
                window.load(opener, &path);
                window
            }
        };
        window.window.present();
        if !bookmark.is_empty()
            && let Some(editor) = window.editor()
        {
            editor.go_to_bookmark(&bookmark);
        }
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
        copy.load_untitled(editor.document(), copy_name(&stem));
        copy.view.set_zoom(self.view.zoom());
        copy.window.present();
    }

    /// Shows the document as an outline, or as pages (TXT-14).
    fn show_outline(&self, on: bool) {
        if self.editor().is_none() {
            return;
        }
        self.stack
            .set_visible_child_name(if on { "outline" } else { "document" });
        self.outline.set_shown(on);
        if !on && let Some(editor) = self.editor() {
            editor.show_selection();
            editor.focus();
        }
    }

    /// Deletes the page the cursor is on, after asking (TXT-02).
    fn delete_page(self: &Rc<Self>) {
        let Some(editor) = self.editor() else {
            return;
        };
        let Some(page) = editor.cursor_page() else {
            return;
        };
        let dialog = adw::AlertDialog::new(
            Some(&tr("Delete Page?")),
            Some(
                &tr("Everything on page {n} will be deleted.")
                    .replace("{n}", &(page + 1).to_string()),
            ),
        );
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("delete", &tr("_Delete"));
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.choose(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |response| {
                if let (Some(this), "delete") = (weak.upgrade(), response.as_str())
                    && let Some(editor) = this.editor()
                {
                    editor.delete_page(page);
                }
            },
        );
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
        // The dialog of GNOME apps, rather than Don't Save of Pages.
        let dialog = adw::AlertDialog::new(
            Some(&tr("Save Changes?")),
            Some(
                &tr("“{name}” contains unsaved changes. Changes which are not saved will be permanently lost.")
                    .replace("{name}", &self.name.borrow()),
            ),
        );
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("discard", &tr("_Discard"));
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
    fn connect_fullscreen(self: &Rc<Self>, leave: &gtk::Button, backdrop: &gtk::ColorDialogButton) {
        let weak = Rc::downgrade(self);
        let leave = leave.downgrade();
        let button = backdrop.downgrade();
        self.window.connect_fullscreened_notify(move |window| {
            let Some(this) = weak.upgrade() else { return };
            let fullscreen = window.is_fullscreen();
            this.toolbar_view.set_reveal_top_bars(!fullscreen);
            if let Some(leave) = leave.upgrade() {
                leave.set_visible(fullscreen);
            }
            if let Some(button) = button.upgrade() {
                button.set_visible(fullscreen);
                this.view
                    .set_backdrop(fullscreen.then(|| formatting::from_gdk(&button.rgba())));
            }
        });
        let weak = Rc::downgrade(self);
        backdrop.connect_rgba_notify(move |button| {
            let Some(this) = weak.upgrade() else { return };
            if this.window.is_fullscreen() {
                this.view
                    .set_backdrop(Some(formatting::from_gdk(&button.rgba())));
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
        let template = gio::SimpleAction::new("new-from-template", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        template.connect_activate(move |_, target| {
            let (Some(this), Some(target)) = (weak.upgrade(), target.and_then(|t| t.str())) else {
                return;
            };
            this.load_untitled(from_template(target), tr("Untitled"));
        });
        self.window.add_action(&template);
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
        self.enable_warnings();

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
                // The zoom then follows the window (INT-08).
                "fit-width" => return view.set_fit(Some(geometry::Fit::Width)),
                "fit-page" => return view.set_fit(Some(geometry::Fit::Page)),
                level => level.parse().unwrap_or(geometry::DEFAULT_ZOOM),
            };
            view.set_zoom(zoom);
        });
        self.window.add_action(&zoom);

        // One page to a row, or two side by side (INT-08).
        let weak = Rc::downgrade(self);
        let layout = gio::SimpleAction::new_stateful(
            "page-layout",
            Some(glib::VariantTy::STRING),
            &"one".to_variant(),
        );
        layout.connect_activate(move |action, parameter| {
            let (Some(this), Some(value)) =
                (weak.upgrade(), parameter.and_then(|p| p.get::<String>()))
            else {
                return;
            };
            action.set_state(&value.to_variant());
            this.view.set_columns(if value == "two" { 2 } else { 1 });
        });
        self.window.add_action(&layout);

        for (name, run) in [
            ("undo", TextEditor::undo as fn(&TextEditor)),
            ("redo", TextEditor::redo),
            ("cut", TextEditor::cut),
            ("copy", TextEditor::copy),
            ("paste", TextEditor::paste),
            ("paste-match-style", TextEditor::paste_matching_style),
            ("select-all", TextEditor::select_all),
            ("special-characters", TextEditor::special_characters),
        ] {
            let weak = Rc::downgrade(self);
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| {
                let Some(this) = weak.upgrade() else { return };
                // Pages selected in the thumbnails take Cut, Copy and Paste.
                if matches!(name, "cut" | "copy" | "paste") && this.thumbnails.take_shortcut(name) {
                    return;
                }
                if let Some(editor) = this.editor() {
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
                    // A light face turned bold is bold, and a black face
                    // stays as it is.
                    "bold" => CharAttrs {
                        weight: Some(editor.format().character.weight.with_bold(on)),
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

        // The outlines of the body, the columns, the headers, the footers
        // and the text boxes (INT-09).
        let layout = gio::SimpleAction::new_stateful("show-layout", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        layout.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            if let Some(editor) = this.editor() {
                editor.set_show_layout(on);
            }
            // As in Pages, Show Layout shows the rulers too, and Hide
            // Layout hides them.
            this.window.change_action_state("ruler", &on.to_variant());
        });
        self.window.add_action(&layout);

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

        // The arrows at the bottom, and what they go to (INT-19).
        let navigate_by = gio::SimpleAction::new_stateful(
            "navigate-by",
            Some(glib::VariantTy::STRING),
            &"page".to_variant(),
        );
        let weak = Rc::downgrade(self);
        navigate_by.connect_activate(move |action, parameter| {
            let (Some(this), Some(value)) =
                (weak.upgrade(), parameter.and_then(|p| p.get::<String>()))
            else {
                return;
            };
            action.set_state(&value.to_variant());
            let kind = crate::editing::Navigate::from_name(&value);
            this.navigate_by.set_label(&navigate_label(kind));
        });
        self.window.add_action(&navigate_by);
        let navigate = gio::SimpleAction::new("navigate", Some(glib::VariantTy::BOOLEAN));
        let weak = Rc::downgrade(self);
        navigate.connect_activate(move |_, parameter| {
            let (Some(this), Some(forward)) =
                (weak.upgrade(), parameter.and_then(|p| p.get::<bool>()))
            else {
                return;
            };
            let kind = this
                .window
                .lookup_action("navigate-by")
                .and_then(|a| a.state())
                .and_then(|s| s.get::<String>())
                .map_or(crate::editing::Navigate::Page, |n| {
                    crate::editing::Navigate::from_name(&n)
                });
            if let Some(editor) = this.editor()
                && !editor.navigate(kind, forward)
            {
                this.window.error_bell();
            }
        });
        self.window.add_action(&navigate);

        // The page thumbnails (INT-19).
        let thumbnails = gio::SimpleAction::new_stateful("thumbnails", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        thumbnails.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            this.thumbnails_split.set_show_sidebar(on);
            this.thumbnails.set_shown(on, this.window.upcast_ref());
        });
        self.window.add_action(&thumbnails);

        // The document as an outline, or as pages (TXT-14).
        let outline = gio::SimpleAction::new_stateful("outline", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        outline.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            this.show_outline(on);
        });
        self.window.add_action(&outline);

        // Edit ▸ Delete Page, after asking (TXT-02).
        let delete_page = gio::SimpleAction::new("delete-page", None);
        let weak = Rc::downgrade(self);
        delete_page.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.delete_page();
            }
        });
        self.window.add_action(&delete_page);

        let inspector = gio::SimpleAction::new_stateful("inspector", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        inspector.connect_change_state(move |_, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            this.show_side("inspector", on);
        });
        self.window.add_action(&inspector);

        let fonts = gio::SimpleAction::new("fonts", None);
        let weak = Rc::downgrade(self);
        fonts.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            let Some(editor) = this.editor() else { return };
            let cell = this.editor.clone();
            this.format_revealer.set_reveal_child(true);
            let window = this.fonts.present(
                this.format_bar.font_button(),
                Rc::new(move || cell.borrow().clone()),
            );
            let format = editor.format();
            window.show(&format.character, &format.paragraph);
        });
        self.window.add_action(&fonts);
        let colors = gio::SimpleAction::new("colors", None);
        let weak = Rc::downgrade(self);
        colors.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.format_bar.choose_color();
            }
        });
        self.window.add_action(&colors);
        // The format bar can be hidden for more room (Ctrl+Shift+R).
        let format_bar = gio::SimpleAction::new_stateful("format-bar", None, &true.to_variant());
        let revealer = self.format_revealer.downgrade();
        let stack = self.stack.downgrade();
        format_bar.connect_change_state(move |action, value| {
            let (Some(revealer), Some(stack), Some(on)) = (
                revealer.upgrade(),
                stack.upgrade(),
                value.and_then(|v| v.get::<bool>()),
            ) else {
                return;
            };
            action.set_state(&on.to_variant());
            revealer.set_reveal_child(on && stack.visible_child_name().as_deref() != Some("empty"));
        });
        self.window.add_action(&format_bar);

        let drawer = gio::SimpleAction::new_stateful("styles-drawer", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        drawer.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            this.drawer_split.set_show_sidebar(on);
            this.show_format();
        });
        self.window.add_action(&drawer);
        let window = self.window.downgrade();
        self.drawer_split.connect_show_sidebar_notify(move |split| {
            if let Some(window) = window.upgrade() {
                formatting::set_state(&window, "styles-drawer", split.shows_sidebar().to_variant());
            }
        });

        // Pages copies the style of the paragraph and that of the
        // characters apart, and pastes only what was copied (STY-08).
        for (name, parts) in [
            ("copy-paragraph-style", desk_model::StyleParts::Paragraph),
            ("copy-character-style", desk_model::StyleParts::Character),
        ] {
            let copy_style = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            copy_style.connect_activate(move |_, _| {
                if let Some(copied) = weak
                    .upgrade()
                    .and_then(|this| this.editor())
                    .and_then(|editor| editor.copy_style())
                {
                    COPIED_STYLE.with(|c| *c.borrow_mut() = Some(copied.only(parts)));
                    let windows = WINDOWS.with(|w| w.borrow().clone());
                    for window in windows {
                        window.refresh();
                    }
                }
            });
            self.window.add_action(&copy_style);
        }
        let paste_style = gio::SimpleAction::new("paste-style", None);
        let weak = Rc::downgrade(self);
        paste_style.connect_activate(move |_, _| {
            let Some(editor) = weak.upgrade().and_then(|this| this.editor()) else {
                return;
            };
            if let Some(copied) = COPIED_STYLE.with(|c| c.borrow().clone()) {
                editor.paste_style(&copied);
            }
        });
        self.window.add_action(&paste_style);

        self.add_find_actions();
        self.add_object_actions();
        self.add_spelling_actions();

        for (name, kind) in [
            ("insert-page-break", BreakKind::Page),
            ("insert-column-break", BreakKind::Column),
            ("insert-layout-break", BreakKind::Layout),
            ("insert-section-break", BreakKind::Section),
        ] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    editor.insert_break(kind);
                }
            });
            self.window.add_action(&action);
        }

        for (name, kind) in [
            ("insert-page-number", FieldKind::PageNumber),
            ("insert-page-count", FieldKind::PageCount),
            ("insert-date", FieldKind::Date),
            ("insert-file-name", FieldKind::FileName),
        ] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    editor.insert_field(editor.new_field(kind));
                }
            });
            self.window.add_action(&action);
        }

        // The number of columns of the layout at the cursor, which keep
        // their gap (EST-07).
        let columns = gio::SimpleAction::new_stateful(
            "columns",
            Some(glib::VariantTy::INT32),
            &1i32.to_variant(),
        );
        let weak = Rc::downgrade(self);
        columns.connect_activate(move |action, count| {
            let (Some(this), Some(count)) = (weak.upgrade(), count.and_then(|c| c.get::<i32>()))
            else {
                return;
            };
            if let Some(editor) = this.editor() {
                let format = editor.format();
                let gap = crate::layout_pane::gap(&format.setup, &format.layout);
                let count = count.clamp(1, desk_model::MAX_COLUMNS as i32) as u8;
                let layout =
                    crate::layout_pane::with_columns(&format.setup, &format.layout, count, gap);
                if layout != format.layout {
                    editor.set_layout("Columns", layout);
                }
                action.set_state(&i32::from(count).to_variant());
            }
        });
        self.window.add_action(&columns);

        let auto = gio::SimpleAction::new("insert-auto-page-numbers", None);
        let weak = Rc::downgrade(self);
        auto.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade()
                && let Some(editor) = this.editor()
            {
                crate::auto_numbers::present(&this.window, &editor);
            }
        });
        self.window.add_action(&auto);

        let linked = gio::SimpleAction::new("open-linked", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        linked.connect_activate(move |_, link| {
            if let (Some(this), Some(link)) = (weak.upgrade(), link.and_then(|l| l.str())) {
                this.open_linked(link);
            }
        });
        self.window.add_action(&linked);

        let note = gio::SimpleAction::new("insert-note", None);
        let weak = Rc::downgrade(self);
        note.connect_activate(move |_, _| {
            if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                editor.insert_note();
            }
        });
        self.window.add_action(&note);
        // From note to note, each of which the Inspector shows (EST-15).
        for (name, forward) in [("next-note", true), ("previous-note", false)] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    editor.go_to_note(forward);
                }
            });
            self.window.add_action(&action);
        }

        self.add_field_actions();

        // Tables of contents, links and bookmarks (EST-22, EST-25,
        // EST-26).
        let structure: [(&str, EditorAction); 4] = [
            ("insert-toc", TextEditor::insert_table_of_contents),
            ("update-toc", TextEditor::update_tables_of_contents),
            ("insert-link", TextEditor::insert_link),
            ("insert-bookmark", TextEditor::insert_bookmark),
        ];
        for (name, run) in structure {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    run(&editor);
                }
            });
            self.window.add_action(&action);
        }

        let import = gio::SimpleAction::new("import-styles", None);
        let weak = Rc::downgrade(self);
        import.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.import_styles();
            }
        });
        self.window.add_action(&import);
        // The sidebar also closes by itself on a narrow window.
        let weak = Rc::downgrade(self);
        self.split.connect_show_sidebar_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.sync_side();
            }
        });
    }

    /// Shows `page` in the right sidebar, or hides the sidebar if it shows
    /// that page and `on` is false. Media and spelling are made the first
    /// time they are asked for.
    fn show_side(self: &Rc<Self>, page: &str, on: bool) {
        if self.side.child_by_name(page).is_none() {
            match page {
                "media" => {
                    let browser = self.media_browser();
                    self.side.add_named(browser.widget(), Some("media"));
                }
                "spelling" => {
                    let cell = self.editor.clone();
                    let spelling = self
                        .spelling
                        .get_or_init(|| {
                            crate::spelling_window::SpellingWindow::new(Rc::new(move || {
                                cell.borrow().clone()
                            }))
                        })
                        .clone();
                    self.side.add_named(spelling.widget(), Some("spelling"));
                }
                _ => {}
            }
        }
        if on {
            self.side.set_visible_child_name(page);
            self.split.set_show_sidebar(true);
        } else if self.side.visible_child_name().as_deref() == Some(page) {
            self.split.set_show_sidebar(false);
        }
        self.sync_side();
    }

    /// Puts the states of the toggles of the right sidebar in step with
    /// what it shows.
    fn sync_side(&self) {
        let shown = self.split.shows_sidebar();
        let page = self.side.visible_child_name();
        for (action, name) in [("inspector", "inspector"), ("media-browser", "media")] {
            let on = shown && page.as_deref() == Some(name);
            formatting::set_state(&self.window, action, on.to_variant());
        }
    }

    /// The formats of the field at the cursor, from its context menu
    /// (EST-27).
    fn add_field_actions(self: &Rc<Self>) {
        let change = |this: &Rc<Self>, f: &dyn Fn(Field) -> Field| {
            if let Some(editor) = this.editor()
                && let Some((_, field)) = editor.field_at_cursor()
            {
                editor.set_field_at_cursor(f(field));
            }
        };
        let number = gio::SimpleAction::new_stateful(
            "field-number-format",
            Some(glib::VariantTy::STRING),
            &"decimal".to_variant(),
        );
        let weak = Rc::downgrade(self);
        number.connect_activate(move |_, value| {
            let (Some(this), Some(name)) = (weak.upgrade(), value.and_then(|v| v.str())) else {
                return;
            };
            let format = crate::fields::number_format(name);
            change(&this, &|field| match field {
                Field::PageNumber { .. } => Field::PageNumber { format },
                Field::PageCount { .. } => Field::PageCount { format },
                other => other,
            });
        });
        self.window.add_action(&number);
        let date = gio::SimpleAction::new_stateful(
            "field-date-format",
            Some(glib::VariantTy::STRING),
            &crate::fields::DATE_PATTERN.to_variant(),
        );
        let weak = Rc::downgrade(self);
        date.connect_activate(move |_, value| {
            let (Some(this), Some(pattern)) = (weak.upgrade(), value.and_then(|v| v.str())) else {
                return;
            };
            change(&this, &|field| match field {
                Field::DateTime { update, .. } => Field::DateTime {
                    pattern: pattern.into(),
                    shown: crate::fields::format_date(pattern)
                        .unwrap_or_default()
                        .into(),
                    update,
                },
                other => other,
            });
        });
        self.window.add_action(&date);
        type Toggle = fn(Field) -> Field;
        let toggles: [(&str, Toggle); 3] = [
            ("field-date-update", |field| match field {
                Field::DateTime {
                    pattern,
                    shown,
                    update,
                } => Field::DateTime {
                    pattern,
                    shown,
                    update: !update,
                },
                other => other,
            }),
            ("field-file-path", |field| match field {
                Field::FileName {
                    shown,
                    path,
                    extension,
                } => Field::FileName {
                    shown,
                    path: !path,
                    extension,
                },
                other => other,
            }),
            ("field-file-extension", |field| match field {
                Field::FileName {
                    shown,
                    path,
                    extension,
                } => Field::FileName {
                    shown,
                    path,
                    extension: !extension,
                },
                other => other,
            }),
        ];
        for (name, toggle) in toggles {
            let action = gio::SimpleAction::new_stateful(name, None, &false.to_variant());
            let weak = Rc::downgrade(self);
            action.connect_activate(move |action, _| {
                if let Some(this) = weak.upgrade() {
                    let on = action
                        .state()
                        .and_then(|s| s.get::<bool>())
                        .unwrap_or(false);
                    action.set_state(&(!on).to_variant());
                    change(&this, &toggle);
                }
            });
            self.window.add_action(&action);
        }
    }

    /// Find & Replace, the Search sidebar and the Writing Tools (TXT-12,
    /// TXT-13, INT-20).
    fn add_find_actions(self: &Rc<Self>) {
        let simple: [(&str, WindowAction); 9] = [
            ("find", |this| {
                this.find.present();
            }),
            ("find-next", |this| {
                if let Some(editor) = this.editor() {
                    find::find_next(&editor, &this.search, false);
                }
            }),
            ("find-previous", |this| {
                if let Some(editor) = this.editor() {
                    find::find_next(&editor, &this.search, true);
                }
            }),
            ("use-selection-for-find", |this| {
                if let Some(editor) = this.editor() {
                    let text = editor.selected_text();
                    if !text.is_empty() && !text.contains('\n') {
                        this.search.query.borrow_mut().text = text;
                    }
                }
            }),
            ("jump-to-selection", |this| {
                if let Some(editor) = this.editor() {
                    editor.show_selection();
                }
            }),
            ("look-up-dictionary", |this| {
                this.look_up(Lookup::Dictionary)
            }),
            ("look-up-web", |this| this.look_up(Lookup::Web)),
            ("look-up-wikipedia", |this| this.look_up(Lookup::Wikipedia)),
            ("statistics", |this| {
                if let Some(editor) = this.editor() {
                    let (all, selected) = editor.statistics();
                    let (lines, selected_lines) = editor.line_counts();
                    writing_tools::show_statistics(
                        &this.window,
                        (&all, lines),
                        selected.as_ref().map(|s| (s, selected_lines.unwrap_or(0))),
                        this.view.page_count(),
                    );
                }
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

        let search = gio::SimpleAction::new_stateful("search", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        search.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            this.search_split.set_show_sidebar(on);
            this.search_sidebar.set_shown(on);
            if !on && let Some(editor) = this.editor() {
                editor.focus();
            }
        });
        self.window.add_action(&search);
        let weak = Rc::downgrade(self);
        self.search_split.connect_show_sidebar_notify(move |split| {
            if let Some(this) = weak.upgrade() {
                let on = split.shows_sidebar();
                formatting::set_state(&this.window, "search", on.to_variant());
                this.search_sidebar.set_shown(on);
            }
        });
        let recent = gio::SimpleAction::new("recent-search", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        recent.connect_activate(move |_, parameter| {
            if let (Some(this), Some(text)) =
                (weak.upgrade(), parameter.and_then(|p| p.get::<String>()))
            {
                this.search_sidebar.search_for(&text);
            }
        });
        self.window.add_action(&recent);
    }

    /// Spelling and proofreading (TXT-10, TXT-11).
    fn add_spelling_actions(self: &Rc<Self>) {
        let simple: [(&str, WindowAction); 4] = [
            ("check-spelling", |this| {
                if let Some(editor) = this.editor() {
                    editor.next_mistake(true, preferences::get().proofread);
                }
            }),
            ("spelling", |this| {
                this.show_side("spelling", true);
                if let Some(spelling) = this.spelling.get() {
                    spelling.present();
                }
            }),
            ("ignore-spelling", |this| {
                if let Some(editor) = this.editor() {
                    let word = editor.selected_text();
                    crate::spelling::ignore(&word);
                    editor.update_squiggles();
                }
            }),
            ("learn-spelling", |this| {
                if let Some(editor) = this.editor() {
                    let word = editor.selected_text();
                    crate::spelling::learn(&word);
                    editor.update_squiggles();
                }
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
        for (name, never) in [("never-hyphenate", true), ("allow-hyphenation", false)] {
            let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, parameter| {
                let (Some(this), Some(word)) =
                    (weak.upgrade(), parameter.and_then(|p| p.get::<String>()))
                else {
                    return;
                };
                if let Some(editor) = this.editor() {
                    let word = word.to_lowercase();
                    editor.change_text_settings("Hyphenation", |settings| {
                        if never {
                            settings.never_hyphenate.insert(word);
                        } else {
                            settings.never_hyphenate.remove(&word);
                        }
                    });
                }
            });
            self.window.add_action(&action);
        }
        let correct = gio::SimpleAction::new("correct-spelling", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        correct.connect_activate(move |_, parameter| {
            if let (Some(editor), Some(text)) = (
                weak.upgrade().and_then(|this| this.editor()),
                parameter.and_then(|p| p.get::<String>()),
            ) {
                editor.correct(&text);
            }
        });
        self.window.add_action(&correct);
        let current = preferences::get();
        for (name, state, set) in [
            (
                "check-spelling-while-typing",
                current.check_spelling,
                (|p, on| p.check_spelling = on) as fn(&mut preferences::Preferences, bool),
            ),
            ("proofread-while-typing", current.proofread, |p, on| {
                p.proofread = on
            }),
        ] {
            let action = gio::SimpleAction::new_stateful(name, None, &state.to_variant());
            action.connect_change_state(move |action, value| {
                if let Some(on) = value.and_then(|v| v.get::<bool>()) {
                    action.set_state(&on.to_variant());
                    let mut preferences = preferences::get();
                    set(&mut preferences, on);
                    preferences::set(preferences);
                }
            });
            self.window.add_action(&action);
        }
    }

    /// Looks the selection, or the word at the cursor, up (INT-20).
    fn look_up(&self, lookup: Lookup) {
        if let Some(editor) = self.editor() {
            writing_tools::look_up(&self.window, lookup, &editor.lookup_text());
        }
    }

    /// A menu of the document at the pointer, on a secondary click or the
    /// Menu key (INT-21).
    fn connect_context_menu(&self) {
        let menu = gio::Menu::new();
        // The guesses for a misspelled word or a mistake, filled when the
        // menu opens on one (TXT-10, TXT-11).
        let guesses = gio::Menu::new();
        menu.append_section(None, &guesses);
        // Never Hyphenate for the word at the cursor (TXT-09).
        let hyphenation = gio::Menu::new();
        menu.append_section(None, &hyphenation);
        // The format of the field at the cursor (EST-27).
        let field = gio::Menu::new();
        menu.append_section(None, &field);
        // Undo and Redo, with the name of the change, filled when the menu
        // opens (DOC-12).
        let undo = gio::Menu::new();
        menu.append_section(None, &undo);
        let edit = gio::Menu::new();
        edit.append(Some(&tr("Cu_t")), Some("win.cut"));
        edit.append(Some(&tr("_Copy")), Some("win.copy"));
        edit.append(Some(&tr("_Paste")), Some("win.paste"));
        edit.append(Some(&tr("Dupl_icate")), Some("win.duplicate-objects"));
        edit.append(
            Some(&tr("Paste and _Match Style")),
            Some("win.paste-match-style"),
        );
        edit.append(Some(&tr("Select _All")), Some("win.select-all"));
        menu.append_section(None, &edit);
        let format = gio::Menu::new();
        format.append(Some(&tr("_Bold")), Some("win.bold"));
        format.append(Some(&tr("_Italic")), Some("win.italic"));
        format.append(Some(&tr("_Underline")), Some("win.underline"));
        menu.append_section(None, &format);
        let style = gio::Menu::new();
        style.append(
            Some(&tr("Copy _Paragraph Style")),
            Some("win.copy-paragraph-style"),
        );
        style.append(
            Some(&tr("Copy C_haracter Style")),
            Some("win.copy-character-style"),
        );
        style.append(Some(&tr("Paste Sty_le")), Some("win.paste-style"));
        menu.append_section(None, &style);
        let tools = gio::Menu::new();
        tools.append_submenu(Some(&tr("_Writing Tools")), &writing_tools_menu());
        tools.append_submenu(Some(&tr("Spe_lling and Grammar")), &spelling_menu());
        tools.append_submenu(Some(&tr("_Arrange")), &objects::arrange_menu());
        menu.append_section(None, &tools);
        let page = gio::Menu::new();
        page.append(Some(&tr("Delete _Page…")), Some("win.delete-page"));
        menu.append_section(None, &page);
        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(self.view.area());
        popover.set_has_arrow(false);
        popover.set_halign(gtk::Align::Start);

        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        let menu_popover = popover.downgrade();
        let editor = self.editor.clone();
        let menu_guesses = guesses.clone();
        let menu_hyphenation = hyphenation.clone();
        let menu_field = field.clone();
        let menu_undo = undo.clone();
        let window = self.window.downgrade();
        click.connect_pressed(move |gesture, _, x, y| {
            if let Some(popover) = menu_popover.upgrade() {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                let editor = editor.borrow().clone();
                fill_guesses(
                    &menu_guesses,
                    editor.as_ref().and_then(|e| e.mistake_at(x, y)),
                );
                fill_hyphenation(&menu_hyphenation, editor.as_ref());
                fill_undo(&menu_undo, editor.as_ref());
                if let Some(editor) = &editor {
                    editor.select_field_at(x, y);
                }
                if let Some(window) = window.upgrade() {
                    fill_field(&menu_field, &window, editor.as_ref());
                }
                popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            }
        });
        self.view.area().add_controller(click);

        let keys = gtk::EventControllerKey::new();
        let menu_popover = popover.downgrade();
        let area = self.view.area().downgrade();
        let editor = self.editor.clone();
        let window = self.window.downgrade();
        keys.connect_key_pressed(move |_, key, keycode, modifiers| {
            // The shortcuts of styles: Ctrl+1 to Ctrl+8 in place of F1 to
            // F8 of Pages, with Shift to drop the formatting set over the
            // style in place of Option, since GNOME keeps F1 for Help and
            // Alt with the function keys for the windows (STY-04).
            let control = gdk::ModifierType::CONTROL_MASK;
            let shift = control | gdk::ModifierType::SHIFT_MASK;
            if (modifiers == control || modifiers == shift)
                && let Some(n) = style_key(key, keycode)
                && let Some(editor) = editor.borrow().clone()
                && let Some(style) = editor.styles().by_shortcut(n).map(desk_model::Style::id)
            {
                editor.apply_style(style, modifiers == shift);
                return glib::Propagation::Stop;
            }
            let menu_key = key == gdk::Key::Menu
                || (key == gdk::Key::F10 && modifiers == gdk::ModifierType::SHIFT_MASK);
            if let (true, Some(popover), Some(area)) =
                (menu_key, menu_popover.upgrade(), area.upgrade())
            {
                let current = editor.borrow().clone();
                let preferences = preferences::get();
                fill_guesses(
                    &guesses,
                    current.as_ref().and_then(|e| {
                        e.selected_mistake(preferences.check_spelling, preferences.proofread)
                    }),
                );
                fill_hyphenation(&hyphenation, current.as_ref());
                fill_undo(&undo, current.as_ref());
                if let Some(window) = window.upgrade() {
                    fill_field(&field, &window, current.as_ref());
                }
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

/// Fills the section of the context menu with Undo and Redo, named by the
/// changes they take back and bring back, as in Pages (DOC-12).
fn fill_undo(menu: &gio::Menu, editor: Option<&TextEditor>) {
    menu.remove_all();
    let undo = editor.and_then(TextEditor::undo_name);
    let redo = editor.and_then(TextEditor::redo_name);
    menu.append(
        Some(&crate::undo_names::undo_label(undo.as_deref())),
        Some("win.undo"),
    );
    menu.append(
        Some(&crate::undo_names::redo_label(redo.as_deref())),
        Some("win.redo"),
    );
}

/// Fills the section of the context menu with the guesses for a mistake,
/// or empties it.
fn fill_guesses(menu: &gio::Menu, found: Option<(crate::spelling::Mistake, String)>) {
    menu.remove_all();
    let Some((mistake, text)) = found else {
        return;
    };
    if let MistakeKind::Proofreading(_) = mistake.kind {
        // What is wrong, as an item that does nothing.
        menu.append(Some(&mistake.description()), Some("win.none"));
    }
    let guesses = mistake.suggestions(&text);
    if guesses.is_empty() {
        menu.append(Some(&tr("No Guesses Found")), Some("win.none"));
    }
    for guess in guesses.iter().take(6) {
        let label = if guess.is_empty() {
            tr("Delete")
        } else {
            // Underscores would be mnemonics.
            guess.replace('_', "__")
        };
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.correct-spelling"), Some(&guess.to_variant()));
        menu.append_item(&item);
    }
    if let MistakeKind::Spelling { .. } = mistake.kind {
        menu.append(Some(&tr("_Ignore Spelling")), Some("win.ignore-spelling"));
        menu.append(Some(&tr("_Learn Spelling")), Some("win.learn-spelling"));
    }
}

/// Fills the section of the context menu with Never Hyphenate, or Allow
/// Hyphenation for a word that is never hyphenated, when the document is
/// hyphenated and the cursor is in a word (TXT-09).
fn fill_hyphenation(menu: &gio::Menu, editor: Option<&TextEditor>) {
    menu.remove_all();
    let Some(editor) = editor else { return };
    let Some(word) = editor.word_at_cursor() else {
        return;
    };
    let settings = editor.format().text_settings;
    if !settings.hyphenation {
        return;
    }
    let (label, action) = if settings.may_hyphenate(&word) {
        (tr("Never _Hyphenate"), "win.never-hyphenate")
    } else {
        (tr("Allow _Hyphenation"), "win.allow-hyphenation")
    };
    let item = gio::MenuItem::new(Some(&label), None);
    item.set_action_and_target_value(Some(action), Some(&word.to_variant()));
    menu.append_item(&item);
}

/// Fills the section of the context menu with the format of the field at
/// the cursor, and sets the states of its actions (EST-27): the format of
/// a number of pages, of a date and whether it updates on open, and
/// whether a file name shows its folder and extension.
fn fill_field(menu: &gio::Menu, window: &adw::ApplicationWindow, editor: Option<&TextEditor>) {
    menu.remove_all();
    let Some((_, field)) = editor.and_then(TextEditor::field_at_cursor) else {
        return;
    };
    let state = |name: &str, value: glib::Variant| {
        formatting::set_state(window, name, value);
    };
    match &field {
        Field::PageNumber { format } | Field::PageCount { format } => {
            state(
                "field-number-format",
                crate::fields::number_format_name(*format).to_variant(),
            );
            let formats = gio::Menu::new();
            for (_, name, label) in crate::fields::NUMBER_FORMATS {
                let item = gio::MenuItem::new(Some(label), None);
                item.set_action_and_target_value(
                    Some("win.field-number-format"),
                    Some(&name.to_variant()),
                );
                formats.append_item(&item);
            }
            menu.append_submenu(Some(&tr("Number _Format")), &formats);
        }
        Field::DateTime {
            pattern, update, ..
        } => {
            state("field-date-format", pattern.to_variant());
            state("field-date-update", update.to_variant());
            let formats = gio::Menu::new();
            for pattern in crate::fields::DATE_FORMATS {
                let shown = crate::fields::format_date(pattern).unwrap_or_else(|| pattern.into());
                let item = gio::MenuItem::new(Some(&shown.replace('_', "__")), None);
                item.set_action_and_target_value(
                    Some("win.field-date-format"),
                    Some(&pattern.to_variant()),
                );
                formats.append_item(&item);
            }
            menu.append_submenu(Some(&tr("Date and Time _Format")), &formats);
            menu.append(
                Some(&tr("_Automatically Update on Open")),
                Some("win.field-date-update"),
            );
        }
        Field::FileName {
            path, extension, ..
        } => {
            state("field-file-path", path.to_variant());
            state("field-file-extension", extension.to_variant());
            menu.append(
                Some(&tr("Show _Directory Path")),
                Some("win.field-file-path"),
            );
            menu.append(
                Some(&tr("Always Show File_name Extension")),
                Some("win.field-file-extension"),
            );
        }
        Field::NoteMark { .. } | Field::PageOf { .. } | Field::ObjectMark { .. } => {}
    }
}

/// The number of a style shortcut, from 1 to 8, for the key of a digit
/// on the row of digits or the keypad. With Shift the key is a symbol on
/// most layouts, so the key is read again without modifiers (STY-04).
fn style_key(key: gdk::Key, keycode: u32) -> Option<u8> {
    let digit = |key: gdk::Key| {
        key.to_unicode()
            .and_then(|c| c.to_digit(10))
            .filter(|d| (1..=8).contains(d))
            .map(|d| d as u8)
    };
    digit(key).or_else(|| {
        let display = gdk::Display::default()?;
        let (plain, ..) = display.translate_key(keycode, gdk::ModifierType::empty(), 0)?;
        digit(plain)
    })
}

/// The main menu of the header bar.
fn main_menu(info: AppInfo, opens: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    let file = gio::Menu::new();
    if opens {
        file.append(Some(&tr("_New Document")), Some("app.new-document"));
    }
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
        document.append(Some(&tr("Document _Info")), Some("win.properties"));
        document.append(Some(&tr("Document _Warnings")), Some("win.warnings"));
        document.append(Some(&tr("_Import Styles…")), Some("win.import-styles"));
        menu.append_section(None, &document);
    }
    let close = gio::Menu::new();
    close.append(Some(&tr("C_lose")), Some("win.close"));
    close.append(Some(&tr("Close _All Windows")), Some("app.close-all"));
    menu.append_section(None, &close);
    let help = gio::Menu::new();
    help.append(Some(&tr("_Preferences")), Some("app.preferences"));
    help.append(Some(&tr("_Keyboard Shortcuts")), Some("win.shortcuts"));
    help.append(
        Some(&tr("_About {app}").replace("{app}", info.name)),
        Some("app.about"),
    );
    menu.append_section(None, &help);
    menu
}

/// The name of an untitled copy of a document, as Duplicate gives it
/// (DOC-14).
fn copy_name(name: &str) -> String {
    tr("{name} copy").replace("{name}", name)
}

/// The templates of the gallery: their action targets and names.
const TEMPLATES: [(&str, &str); 2] = [("blank", n_("Blank")), ("layout", n_("Blank Layout"))];

/// The template gallery of an empty window: a card for each template,
/// and Open.
fn template_gallery() -> gtk::Box {
    let cards = gtk::Box::builder()
        .spacing(18)
        .homogeneous(true)
        .halign(gtk::Align::Center)
        .build();
    for (target, name) in TEMPLATES {
        let page = gtk::Box::builder()
            .width_request(120)
            .height_request(170)
            .css_classes(["desk-template-page"])
            .build();
        if target == "layout" {
            page.add_css_class("desk-template-layout");
        }
        let label = gtk::Label::new(Some(&tr(name)));
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(9)
            .build();
        content.append(&page);
        content.append(&label);
        let button = gtk::Button::builder()
            .child(&content)
            .action_name("win.new-from-template")
            .action_target(&target.to_variant())
            .css_classes(["flat", "desk-template"])
            .build();
        formatting::labelled(&button, &tr(name));
        cards.append(&button);
    }
    let open = gtk::Button::builder()
        .label(tr("_Open a Document…"))
        .use_underline(true)
        .action_name("app.open")
        .halign(gtk::Align::Center)
        .css_classes(["pill"])
        .build();
    let gallery = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .build();
    gallery.append(&cards);
    gallery.append(&open);
    gallery
}

/// A new document from a template of the gallery.
fn from_template(target: &str) -> Document {
    match target {
        "layout" => Document::new_page_layout(),
        _ => Document::new_word_processing(),
    }
}

/// The View menu of the header bar: what the window shows.
fn view_menu() -> gio::Menu {
    let panes = gio::Menu::new();
    panes.append(Some(&tr("Page _Thumbnails")), Some("win.thumbnails"));
    panes.append(Some(&tr("_Styles Drawer")), Some("win.styles-drawer"));
    panes.append(Some(&tr("Se_arch Sidebar")), Some("win.search"));
    panes.append(Some(&tr("Spe_lling and Grammar")), Some("win.spelling"));
    let document = gio::Menu::new();
    document.append(Some(&tr("As _Outline")), Some("win.outline"));
    document.append(Some(&tr("_Ruler")), Some("win.ruler"));
    document.append(Some(&tr("_Layout")), Some("win.show-layout"));
    document.append(Some(&tr("_Invisibles")), Some("win.invisibles"));
    document.append(Some(&tr("_Format Bar")), Some("win.format-bar"));
    let window = gio::Menu::new();
    window.append_submenu(Some(&tr("_Zoom")), &zoom_menu());
    window.append(Some(&tr("_Fullscreen")), Some("win.fullscreen"));
    let menu = gio::Menu::new();
    menu.append_section(None, &panes);
    menu.append_section(None, &document);
    menu.append_section(None, &window);
    menu
}

/// The Format menu at the end of the format bar: fonts, colors and
/// styles.
pub(crate) fn format_menu() -> gio::Menu {
    let fonts = gio::Menu::new();
    fonts.append(Some(&tr("_Fonts…")), Some("win.fonts"));
    fonts.append(Some(&tr("_Colors…")), Some("win.colors"));
    fonts.append(
        Some(&tr("Special _Characters…")),
        Some("win.special-characters"),
    );
    let styles = gio::Menu::new();
    styles.append(
        Some(&tr("Copy _Paragraph Style")),
        Some("win.copy-paragraph-style"),
    );
    styles.append(
        Some(&tr("Copy C_haracter Style")),
        Some("win.copy-character-style"),
    );
    styles.append(Some(&tr("Paste Sty_le")), Some("win.paste-style"));
    styles.append(
        Some(&tr("Paste and _Match Style")),
        Some("win.paste-match-style"),
    );
    let menu = gio::Menu::new();
    menu.append_section(None, &fonts);
    menu.append_section(None, &styles);
    menu
}

/// The breaks, fields, notes, tables of contents, links and bookmarks of
/// the Insert menu (EST-04, EST-08, EST-09, EST-13, EST-15, EST-16,
/// EST-22, EST-25, EST-26, EST-27).
fn insert_menu() -> gio::Menu {
    let breaks = gio::Menu::new();
    breaks.append(Some(&tr("_Page Break")), Some("win.insert-page-break"));
    breaks.append(Some(&tr("_Column Break")), Some("win.insert-column-break"));
    breaks.append(Some(&tr("_Layout Break")), Some("win.insert-layout-break"));
    breaks.append(
        Some(&tr("_Section Break")),
        Some("win.insert-section-break"),
    );
    let fields = gio::Menu::new();
    fields.append(
        Some(&tr("_Auto Page Numbers…")),
        Some("win.insert-auto-page-numbers"),
    );
    fields.append(Some(&tr("Page _Number")), Some("win.insert-page-number"));
    fields.append(Some(&tr("Page Co_unt")), Some("win.insert-page-count"));
    fields.append(Some(&tr("_Date")), Some("win.insert-date"));
    fields.append(Some(&tr("_File Name")), Some("win.insert-file-name"));
    let notes = gio::Menu::new();
    notes.append(Some(&tr("Foot_note")), Some("win.insert-note"));
    notes.append(Some(&tr("Ne_xt Note")), Some("win.next-note"));
    notes.append(Some(&tr("Pre_vious Note")), Some("win.previous-note"));
    let structure = gio::Menu::new();
    structure.append(Some(&tr("_Table of Contents")), Some("win.insert-toc"));
    structure.append(
        Some(&tr("Update Table of C_ontents")),
        Some("win.update-toc"),
    );
    structure.append(Some(&tr("_Hyperlink")), Some("win.insert-link"));
    structure.append(Some(&tr("_Bookmark")), Some("win.insert-bookmark"));
    let menu = gio::Menu::new();
    menu.append_section(None, &breaks);
    menu.append_section(None, &fields);
    menu.append_section(None, &notes);
    menu.append_section(None, &structure);
    menu.append_section(None, &objects::shapes_menu());
    menu
}

/// Spelling and Grammar, in the context menu (TXT-10, TXT-11).
fn spelling_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(
        Some(&tr("Show _Spelling and Grammar")),
        Some("win.spelling"),
    );
    menu.append(Some(&tr("_Check Document Now")), Some("win.check-spelling"));
    let typing = gio::Menu::new();
    typing.append(
        Some(&tr("Check Spelling While _Typing")),
        Some("win.check-spelling-while-typing"),
    );
    typing.append(
        Some(&tr("Check _Grammar While Typing")),
        Some("win.proofread-while-typing"),
    );
    menu.append_section(None, &typing);
    menu
}

/// The Writing Tools of the context menu (INT-20).
fn writing_tools_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(
        Some(&tr("Look Up in _Dictionary and Thesaurus")),
        Some("win.look-up-dictionary"),
    );
    menu.append(Some(&tr("Search the _Web")), Some("win.look-up-web"));
    menu.append(
        Some(&tr("Search in Wi_kipedia")),
        Some("win.look-up-wikipedia"),
    );
    menu.append(Some(&tr("Show _Statistics")), Some("win.statistics"));
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

/// The name of what the arrows go to, on the button that chooses it.
fn navigate_label(kind: crate::editing::Navigate) -> String {
    use crate::editing::Navigate;
    match kind {
        Navigate::Page => tr("Page"),
        Navigate::Section => tr("Section"),
        Navigate::Link => tr("Link"),
        Navigate::Bookmark => tr("Bookmark"),
        Navigate::Style => tr("Style"),
    }
}

/// The menu of what the arrows at the bottom go to (INT-19).
fn navigate_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    for kind in crate::editing::Navigate::ALL {
        let item = gio::MenuItem::new(Some(&navigate_label(kind)), None);
        item.set_action_and_target_value(Some("win.navigate-by"), Some(&kind.name().to_variant()));
        menu.append_item(&item);
    }
    menu
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
    let layout = gio::Menu::new();
    for (label, target) in [(tr("_One Page"), "one"), (tr("_Two Pages"), "two")] {
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.page-layout"), Some(&target.to_variant()));
        layout.append_item(&item);
    }
    menu.append_section(None, &layout);
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
    fn doc_14_a_copy_is_named_after_the_original() {
        assert_eq!(copy_name("Letter"), "Letter copy");
    }

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
