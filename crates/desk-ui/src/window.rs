// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A document window: the header bar, the document area, and the zoom
//! menu and counts at the bottom, as in Pages. It saves the document, asks
//! to save it before it closes, and shows it full screen.
//!
//! The window has the skeleton of interface B (docs/interface): the
//! navigation sidebar as high as the window at the start, with the page
//! thumbnails (GNM-12), and the Inspector as a utility pane at the end.
//! Below 720 px, as in half of the smallest screen Desk supports, the
//! sidebar covers the pages instead of pushing them (GNM-01).

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
use crate::i18n::{n_, tr};
use crate::nav_sidebar::{self, NavSidebar};
use crate::preferences;
use crate::ruler::{self, Ruler};
use crate::spelling::MistakeKind;
use crate::styles_drawer::StylesDrawer;
use crate::view::DocumentView;
use crate::writing_tools::{self, Lookup};
use crate::{AppInfo, Opener};

pub(crate) use objects::OBJECT_ACCELS;

mod command_search;
mod format_actions;
mod header;
mod objects;
mod rename;

pub(crate) use objects::arrange_menu;

/// How far from the top of the screen the pointer brings the header bar
/// back in full screen, in pixels.
const REVEAL_EDGE: f64 = 4.0;

/// The width of the Inspector, in pixels, as in the mockups of interface B.
const INSPECTOR_WIDTH: f64 = 332.0;
/// The width of the sidebar while it shows the styles, as the Styles
/// drawer had (INT-18).
const STYLES_WIDTH: f64 = 220.0;
/// The width of the sidebar while it shows the search results (TXT-13).
const SEARCH_WIDTH: f64 = 260.0;
/// The pages of the navigation sidebar, and the actions that show them.
const NAV_PAGES: [(&str, &str); 4] = [
    ("pages", "thumbnails"),
    ("styles", "styles-drawer"),
    ("comments", "comments"),
    ("search", "search"),
];

/// The widths below which the window changes, in pixels (GNM-01): the
/// format bar leaves out its secondary controls, the navigation sidebar
/// covers the pages, and the Inspector covers them too. Desk asks for a
/// screen of 1280 × 720 at least, and a window can take half of one.
const COMPACT_FORMAT_BAR: f64 = 1000.0;
const OVERLAY_SIDEBAR: f64 = 720.0;
const OVERLAY_INSPECTOR: f64 = 520.0;

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
    /// Zoom, the position and the count (GNM-13).
    bottom: crate::bottom_bar::BottomBar,
    /// The counts of the document as it last changed, which a change of
    /// the selection does not count again.
    counted: Cell<Option<desk_model::Statistics>>,
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
    /// The styles, a page of the navigation sidebar (INT-18).
    drawer: Rc<StylesDrawer>,
    /// The Fonts window (INT-15), made when first shown.
    fonts: FontsSlot,
    split: adw::OverlaySplitView,
    /// The search of the window (TXT-12, TXT-13).
    search: Rc<Search>,
    /// The Find & Replace window, made when first shown.
    find: Rc<FindWindow>,
    /// The search results, a page of the navigation sidebar (TXT-13).
    search_sidebar: Rc<SearchSidebar>,
    thumbnails: Rc<crate::thumbnails::Thumbnails>,
    outline: Rc<crate::outline_view::OutlineView>,
    /// The navigation sidebar, with the page thumbnails (GNM-12).
    nav: Rc<NavSidebar>,
    nav_split: adw::OverlaySplitView,
    /// Whether the navigation sidebar showed before full screen hid it.
    nav_before_fullscreen: Cell<bool>,
    /// The pages of the right sidebar: the Inspector and Spelling and
    /// Grammar, one at a time.
    side: gtk::Stack,
    /// Spelling and Grammar, made when first shown.
    spelling: std::cell::OnceCell<Rc<crate::spelling_window::SpellingWindow>>,
    /// Media in the header bar, whose popover holds the Media Browser
    /// (INT-14).
    media_button: gtk::MenuButton,
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
const DOCUMENT_ACTIONS: [&str; 69] = [
    "outline",
    "sidebar",
    "comments",
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
    "deselect-all",
    "rename",
    "connection-lines",
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
    "find-replace",
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
        let header::Header {
            header,
            title,
            document_buttons,
            insert_button,
            objects: object_buttons,
            media_button,
            find_toggle,
        } = header::Header::new(info, opener.is_some());
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
        // Says once, over the first window, that the screen is smaller
        // than Desk asks for (GNM-14).
        let screen_banner = adw::Banner::builder()
            .title(tr(
                "The screen is smaller than 1280 × 720, so parts of the window might not fit",
            ))
            .button_label(tr("_Close"))
            .build();
        screen_banner.connect_button_clicked(|banner| banner.set_revealed(false));
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
        // The template gallery is wider than the pages need to be, and
        // must not keep the Inspector from fitting beside them (GNM-01).
        let stack = gtk::Stack::builder().hhomogeneous(false).build();
        stack.add_named(&status, Some("empty"));

        let bottom = crate::bottom_bar::BottomBar::new();
        let bottom_bar = bottom.bar.clone();

        let editor: Rc<RefCell<Option<TextEditor>>> = Rc::new(RefCell::new(None));
        let editor_of = || -> EditorOf {
            let cell = editor.clone();
            Rc::new(move || cell.borrow().clone())
        };
        let format_bar = FormatBar::new(editor_of());
        let inspector = Inspector::new(editor_of());
        format_bar.set_link_popover(inspector.link_popover());
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
        // The page thumbnails, in the navigation sidebar (INT-19, EST-17,
        // GNM-12).
        let thumbnails = crate::thumbnails::Thumbnails::new(&view, editor_of());
        let nav = NavSidebar::new(&tr("Add Page"), add_page_menu().upcast_ref());
        nav.add_page(
            "pages",
            &tr("Pages"),
            nav_sidebar::WIDTH,
            thumbnails.widget(),
        );
        // The styles that were the Styles drawer of Pages (INT-18), the
        // comments, and the search results that were its search sidebar
        // (TXT-13), as pages of the sidebar of interface B.
        nav.add_page("styles", &tr("Styles"), STYLES_WIDTH, drawer.widget());
        nav.add_page(
            "comments",
            &tr("Comments"),
            nav_sidebar::WIDTH,
            &comments_page(),
        );
        nav.add_page(
            "search",
            &tr("Search Results"),
            SEARCH_WIDTH,
            search_sidebar.widget(),
        );
        let side = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        side.add_named(inspector.widget(), Some("inspector"));
        // The Inspector is a utility pane of a fixed width at the end.
        let split = adw::OverlaySplitView::builder()
            .content(&stack)
            .sidebar(&side)
            .min_sidebar_width(INSPECTOR_WIDTH)
            .max_sidebar_width(INSPECTOR_WIDTH)
            .sidebar_position(gtk::PackType::End)
            .show_sidebar(false)
            .build();
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.add_top_bar(&screen_banner);
        toolbar_view.add_top_bar(&warnings_banner);
        toolbar_view.add_top_bar(&format_revealer);
        let find = FindWindow::new(editor_of(), search.clone());
        toolbar_view.add_top_bar(find.widget());
        toolbar_view.set_content(Some(&split));
        toolbar_view.add_bottom_bar(&bottom_bar);

        // The navigation sidebar is as high as the window, beside the
        // header bar and not under it.
        let nav_split = adw::OverlaySplitView::builder()
            .content(&toolbar_view)
            .sidebar(nav.widget())
            .sidebar_position(gtk::PackType::Start)
            .min_sidebar_width(nav_sidebar::WIDTH)
            .max_sidebar_width(nav_sidebar::WIDTH)
            .show_sidebar(false)
            .build();

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(info.name)
            .default_width(1280)
            .default_height(720)
            .content(&nav_split)
            .build();

        // Of the breakpoints that match, the window applies the one added
        // last, so each one sets what the wider ones set too. Below about
        // a thousand pixels the format bar leaves out its secondary
        // controls; below 720, as when the window takes half of a screen
        // of 1280 × 720, the smallest Desk supports, the sidebars cover the
        // pages instead of pushing them; below 520 the Inspector covers
        // them too, so that the pages keep some room (GNM-01).
        let mut breakpoints = Vec::new();
        for (width, collapsed) in [
            (COMPACT_FORMAT_BAR, &[][..]),
            (OVERLAY_SIDEBAR, &[&nav_split][..]),
            (OVERLAY_INSPECTOR, &[&nav_split, &split][..]),
        ] {
            let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
                adw::BreakpointConditionLengthType::MaxWidth,
                width,
                adw::LengthUnit::Sp,
            ));
            for split in collapsed {
                breakpoint.add_setter(*split, "collapsed", Some(&true.to_value()));
            }
            if width == OVERLAY_SIDEBAR {
                // The buttons of the objects go into the Insert menu
                // (INT-11).
                breakpoint.add_setter(&object_buttons, "visible", Some(&false.to_value()));
                breakpoint.add_setter(
                    &insert_button,
                    "menu-model",
                    Some(&header::insert_menu(true).to_value()),
                );
            }
            let bar = format_bar.clone();
            breakpoint.connect_apply(move |_| bar.set_compact(true));
            let bar = format_bar.clone();
            breakpoint.connect_unapply(move |_| bar.set_compact(false));
            breakpoints.push(breakpoint);
        }
        for breakpoint in breakpoints {
            window.add_breakpoint(breakpoint);
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
            bottom,
            counted: Cell::new(None),
            warnings_banner,
            path: RefCell::new(None),
            name: RefCell::new(String::new()),
            warnings: RefCell::new(Vec::new()),
            editor,
            format_bar,
            inspector,
            drawer,
            fonts: FontsSlot::default(),
            split,
            search,
            find,
            search_sidebar,
            thumbnails,
            outline,
            nav,
            nav_split,
            nav_before_fullscreen: Cell::new(false),
            side,
            spelling: std::cell::OnceCell::new(),
            media_button,
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
        this.add_format_bar_actions();
        this.connect_header(&find_toggle);
        this.add_rename();
        warn_small_screen(&this.window, &screen_banner);
        this.show_rulers();
        {
            let current = preferences::get();
            this.view.set_zoom(f64::from(current.zoom) / 100.0);
            this.bottom
                .zoom
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
            let label = this.bottom.zoom.downgrade();
            this.view.connect_zoom_changed(move |zoom| {
                if let Some(label) = label.upgrade() {
                    label.set_label(&geometry::zoom_label(zoom));
                }
            });
        }
        {
            // The page in sight shows in the position and in Go to Page.
            let weak = Rc::downgrade(&this);
            this.view.connect_moved(move || {
                if let Some(this) = weak.upgrade() {
                    this.show_position();
                }
            });
            let weak = Rc::downgrade(&this);
            this.bottom.page_entry.connect_activate(move |entry| {
                let Some(this) = weak.upgrade() else {
                    return;
                };
                let count = this.view.page_count();
                if let (Ok(page), Some(editor)) =
                    (entry.text().trim().parse::<usize>(), this.editor())
                    && (1..=count).contains(&page)
                {
                    this.bottom.position.popdown();
                    editor.go_to_page(page - 1);
                    editor.focus();
                } else {
                    this.window.error_bell();
                }
            });
            // Go to Page opens with the page in sight, ready to be typed
            // over.
            let weak = Rc::downgrade(&this);
            if let Some(popover) = this.bottom.position.popover() {
                popover.connect_show(move |_| {
                    if let Some(this) = weak.upgrade() {
                        let entry = &this.bottom.page_entry;
                        if let Some(page) = this.view.current_page() {
                            entry.set_text(&(page + 1).to_string());
                        }
                        entry.grab_focus();
                        entry.select_region(0, -1);
                    }
                });
            }
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
            // The list is gone when the app ends with windows in it.
            let _ = WINDOWS.try_with(|w| {
                if let Ok(mut w) = w.try_borrow_mut() {
                    w.retain(|o| weak.upgrade().is_none_or(|me| !Rc::ptr_eq(o, &me)));
                }
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
                self.warnings_banner
                    .set_revealed(shown.lossy && crate::preferences::get().show_warnings);
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
                this.show_count();
            }
        });
        editor.set_show_invisibles(self.action_state("invisibles"));
        editor.set_show_connection_lines(self.action_state("connection-lines"));
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
        self.show_format_bar(&editor, &format);
        self.inspector.show(&format);
        if self.nav_shows("styles") {
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
        self.bottom.count.set_visible(preferences.show_counts);
        let count = crate::bottom_bar::Count::from_name(&preferences.count);
        formatting::set_state(&self.window, "count", count.name().to_variant());
        self.show_count();
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

    /// Whether the navigation sidebar shows the page called `page`.
    fn nav_shows(&self, page: &str) -> bool {
        self.nav_split.shows_sidebar() && self.nav.page() == page
    }

    /// Puts the actions of the pages of the navigation sidebar, its width
    /// and what its pages show in step with what it shows.
    fn sync_nav(&self) {
        let shown = self.nav_split.shows_sidebar();
        formatting::set_state(&self.window, "sidebar", shown.to_variant());
        let page = self.nav.page();
        let width = self.nav.width(&page);
        self.nav_split.set_min_sidebar_width(width);
        self.nav_split.set_max_sidebar_width(width);
        for (name, action) in NAV_PAGES {
            let on = shown && page == name;
            let was = self.action_state(action);
            formatting::set_state(&self.window, action, on.to_variant());
            if on == was {
                continue;
            }
            match name {
                "pages" => self.thumbnails.set_shown(on, self.window.upcast_ref()),
                "search" => {
                    self.search_sidebar.set_shown(on);
                    if !on && let Some(editor) = self.editor() {
                        editor.focus();
                    }
                }
                "styles" => self.show_format(),
                _ => {}
            }
        }
    }

    /// Shows at the end of the bottom bar the count of the document, or of
    /// the selection when text is selected (DOC-20).
    fn show_count(&self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let kind = self
            .window
            .lookup_action("count")
            .and_then(|a| a.state())
            .and_then(|s| s.get::<String>())
            .map_or(crate::bottom_bar::Count::Words, |n| {
                crate::bottom_bar::Count::from_name(&n)
            });
        let all = self.counted.get().unwrap_or_else(|| {
            let all = editor.main_statistics();
            self.counted.set(Some(all));
            all
        });
        self.bottom.show_count(&crate::bottom_bar::count_label(
            kind,
            &all,
            editor.selection_statistics().as_ref(),
            self.view.page_count(),
        ));
    }

    /// Shows in the middle of the bottom bar the page in sight (INT-19).
    fn show_position(&self) {
        let count = self.view.page_count().max(1);
        let page = self.view.current_page().map_or(1, |p| p + 1).min(count);
        self.bottom.show_position(page, count);
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
            let paragraphs = self.format_bar.set_styles(&editor.paragraph_styles());
            let characters = self
                .format_bar
                .set_character_styles(&editor.character_styles());
            if paragraphs || characters {
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
            self.counted.set(Some(editor.main_statistics()));
            self.show_count();
            self.show_position();
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
        for action in DOCUMENT_ACTIONS
            .iter()
            .chain(&objects::OBJECT_ACTIONS)
            .chain(&format_actions::FORMAT_BAR_ACTIONS)
        {
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
        let weak = Rc::downgrade(self);
        let clear = move || {
            if let Some(this) = weak.upgrade() {
                this.warnings.borrow_mut().clear();
                this.warnings_banner.set_revealed(false);
                this.enable_warnings();
            }
        };
        crate::warnings::show(
            &self.window,
            &self.name.borrow(),
            &self.warnings.borrow(),
            Rc::new(replace),
            Rc::new(clear),
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
        let Some((first, last)) = editor.deleted_pages(page) else {
            return;
        };
        // Pages deletes the section of the page and names its pages.
        let (heading, body) = if first == last {
            (
                tr("Delete Page?"),
                tr("Everything on page {n} will be deleted.")
                    .replace("{n}", &(first + 1).to_string()),
            )
        } else {
            (
                tr("Delete Pages?"),
                tr("Pages {first} to {last}, the section of this page, will be deleted.")
                    .replace("{first}", &(first + 1).to_string())
                    .replace("{last}", &(last + 1).to_string()),
            )
        };
        let dialog = adw::AlertDialog::new(Some(&heading), Some(&body));
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
            // The navigation sidebar leaves room for the pages in full
            // screen and comes back after (GNM-12).
            if fullscreen {
                this.nav_before_fullscreen
                    .set(this.nav_split.shows_sidebar());
                this.nav_split.set_show_sidebar(false);
            } else if this.nav_before_fullscreen.replace(false) {
                this.nav_split.set_show_sidebar(true);
            }
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

    /// Puts Find and Media in the header bar to work, and adds Search
    /// Commands (INT-14, GNM-10).
    fn connect_header(self: &Rc<Self>, find_toggle: &gtk::ToggleButton) {
        // Find is down while the find bar shows.
        self.find
            .widget()
            .bind_property("search-mode-enabled", find_toggle, "active")
            .sync_create()
            .build();
        let weak = Rc::downgrade(self);
        find_toggle.connect_clicked(move |toggle| {
            if let Some(this) = weak.upgrade() {
                if toggle.is_active() {
                    this.find.present();
                } else {
                    this.find.widget().set_search_mode(false);
                }
            }
        });

        // The Media Browser goes in the popover when it first opens.
        let (popover, holder) = header::media_popover();
        self.media_button.set_popover(Some(&popover));
        let weak = Rc::downgrade(self);
        popover.connect_show(move |_| {
            if let Some(this) = weak.upgrade()
                && holder.first_child().is_none()
            {
                let browser = this.media_browser();
                browser.widget().set_vexpand(true);
                holder.append(browser.widget());
            }
        });

        let search = gio::SimpleAction::new("command-search", None);
        let weak = Rc::downgrade(self);
        search.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            let opens = this.opener.is_some();
            let menus: Vec<(String, gio::MenuModel)> = [
                (tr("Main Menu"), header::main_menu(this.info, opens)),
                (tr("Main Menu"), header::more_commands(opens)),
                (tr("View"), header::view_menu()),
                (tr("Insert"), header::insert_menu(true)),
                (tr("Edit"), edit_menu()),
                (tr("Format"), format_menu()),
                (tr("Arrange"), objects::arrange_menu()),
            ]
            .into_iter()
            .map(|(name, menu)| (name, menu.upcast()))
            .chain(this.format_bar.more_menus())
            .collect();
            command_search::present(&this.window, &menus);
        });
        self.window.add_action(&search);
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
            ("deselect-all", TextEditor::deselect_all),
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

        // The lines between linked text boxes, which show until hidden
        // (VMI-03).
        let lines = gio::SimpleAction::new_stateful("connection-lines", None, &true.to_variant());
        let weak = Rc::downgrade(self);
        lines.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            if let Some(editor) = this.editor() {
                editor.set_show_connection_lines(on);
            }
        });
        self.window.add_action(&lines);

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
            this.bottom.show_navigate(kind);
        });
        self.window.add_action(&navigate_by);
        // What the count at the bottom counts (DOC-20), kept in the
        // preferences for every window.
        let count = gio::SimpleAction::new_stateful(
            "count",
            Some(glib::VariantTy::STRING),
            &preferences::get().count.to_variant(),
        );
        let weak = Rc::downgrade(self);
        count.connect_activate(move |action, parameter| {
            let (Some(this), Some(value)) =
                (weak.upgrade(), parameter.and_then(|p| p.get::<String>()))
            else {
                return;
            };
            let kind = crate::bottom_bar::Count::from_name(&value);
            action.set_state(&kind.name().to_variant());
            this.show_count();
            let mut current = preferences::get();
            if current.count != kind.name() {
                current.count = kind.name().to_owned();
                preferences::set(current);
            }
        });
        self.window.add_action(&count);
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

        // The sidebar, with the page it shows (GNM-12), and its pages:
        // the page thumbnails (INT-19), the styles (INT-18), the comments
        // and the search results (TXT-13).
        let sidebar = gio::SimpleAction::new_stateful("sidebar", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        sidebar.connect_change_state(move |_, value| {
            if let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>())) {
                this.nav_split.set_show_sidebar(on);
            }
        });
        self.window.add_action(&sidebar);
        for (page, name) in NAV_PAGES {
            let action = gio::SimpleAction::new_stateful(name, None, &false.to_variant());
            let weak = Rc::downgrade(self);
            action.connect_change_state(move |_, value| {
                let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
                else {
                    return;
                };
                if on {
                    this.nav.set_page(page);
                    this.nav_split.set_show_sidebar(true);
                } else if this.nav.page() == page {
                    this.nav_split.set_show_sidebar(false);
                }
                this.sync_nav();
            });
            self.window.add_action(&action);
        }
        let weak = Rc::downgrade(self);
        self.nav.connect_page_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.sync_nav();
            }
        });
        let weak = Rc::downgrade(self);
        self.nav_split.connect_show_sidebar_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.sync_nav();
            }
        });

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
            let editor_of: crate::formatting::EditorOf = Rc::new(move || cell.borrow().clone());
            let window = match this.inspector.object_font_button() {
                // The family of a selected shape's text (OBJ-13).
                Some(button) if editor.has_selected_objects() => {
                    this.fonts.present(button, editor_of)
                }
                _ => {
                    this.format_revealer.set_reveal_child(true);
                    this.fonts.present(this.format_bar.font_button(), editor_of)
                }
            };
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
    /// that page and `on` is false. Spelling is made the first time it is
    /// asked for.
    fn show_side(self: &Rc<Self>, page: &str, on: bool) {
        if page == "spelling" && self.side.child_by_name(page).is_none() {
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
        let on = shown && page.as_deref() == Some("inspector");
        formatting::set_state(&self.window, "inspector", on.to_variant());
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
        let simple: [(&str, WindowAction); 10] = [
            ("find", |this| {
                this.find.present();
            }),
            ("find-replace", |this| {
                this.find.present_replace();
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
        edit.append(Some(&tr("_Deselect All")), Some("win.deselect-all"));
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

/// The menu of the Add button of the navigation sidebar (GNM-12): a blank
/// page, which in a page layout document is a page of its own, and a
/// section break. The pages of the template join them with the templates
/// of the gallery (INT-06).
/// The Comments page of the sidebar. Desk has no comments yet (REV-08,
/// phase 8 of the plan), so it says so.
fn comments_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name("desk-comment-symbolic")
        .title(tr("No Comments"))
        .description(tr("The comments of the document show here."))
        .css_classes(["compact"])
        .build()
}

fn add_page_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("_Blank Page")), Some("win.insert-page-break"));
    menu.append(
        Some(&tr("_Section Break")),
        Some("win.insert-section-break"),
    );
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

/// Select All and Deselect All, for the command search (VMI-01).
pub(crate) fn edit_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("Select _All")), Some("win.select-all"));
    menu.append(Some(&tr("_Deselect All")), Some("win.deselect-all"));
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

thread_local! {
    /// Whether a window of the app looked at the size of the screen.
    static SCREEN_CHECKED: Cell<bool> = const { Cell::new(false) };
}

/// Shows `banner` when the first window of the app appears on a screen
/// smaller than 1280 × 720 (GNM-14). Desk works there, but parts of a
/// window might not fit; the banner does not return until the app opens
/// again.
fn warn_small_screen(window: &adw::ApplicationWindow, banner: &adw::Banner) {
    let banner = banner.downgrade();
    window.connect_map(move |window| {
        if SCREEN_CHECKED.with(|c| c.replace(true)) {
            return;
        }
        let monitor = window
            .surface()
            .and_then(|surface| WidgetExt::display(window).monitor_at_surface(&surface));
        if let (Some(monitor), Some(banner)) = (monitor, banner.upgrade()) {
            let area = monitor.geometry();
            if header::is_small_screen(area.width(), area.height()) {
                banner.set_revealed(true);
            }
        }
    });
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
}
