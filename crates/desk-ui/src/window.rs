// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A document window: the header bar, the document area, and the zoom
//! and the count over the document (window M+). It saves the document, asks
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
use crate::formatting::{self, EditorOf, Inspector};
use crate::geometry;
use crate::i18n::{n_, tr, trc};
use crate::nav_sidebar::{self, NavSidebar};
use crate::preferences;
use crate::ruler::{self, Ruler};
use crate::spelling::MistakeKind;
use crate::styles_drawer::StylesDrawer;
use crate::tabs::{Frame, FramePage};
use crate::view::DocumentView;
use crate::writing_tools::{self, Lookup};
use crate::{AppInfo, Opener};

pub(crate) use objects::OBJECT_ACCELS;
pub(crate) use tables::TABLE_ACCELS;

pub(crate) mod command_search;
mod format_actions;
mod header;
mod objects;
mod rename;
mod slides;
mod tables;
mod versions;

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
const NAV_PAGES: [(&str, &str); 5] = [
    ("pages", "thumbnails"),
    ("contents", "contents"),
    ("styles", "styles-drawer"),
    ("comments", "comments"),
    ("search", "search"),
];

/// The widths below which the window changes, in pixels (GNM-01): the
/// navigation sidebar covers the pages, and the Inspector covers them
/// too. Desk asks for a screen of 1280 × 720 at least, and a window can
/// take half of one.
const OVERLAY_SIDEBAR: f64 = 720.0;
const OVERLAY_INSPECTOR: f64 = 520.0;

/// What an action of the window does.
type WindowAction = fn(&Rc<DocumentWindow>);
/// What an action of the menus does to the editor of a window.
type EditorAction = fn(&TextEditor);

/// A window and the document it shows, if any.
pub struct DocumentWindow {
    /// The window that holds the tab of the document, shared with the
    /// other documents of that window.
    window: adw::ApplicationWindow,
    frame: Rc<Frame>,
    /// The tab of the document: everything the document shows, with its
    /// own breakpoints, since the window has the tabs of other documents.
    root: adw::BreakpointBin,
    tab: std::cell::OnceCell<adw::TabPage>,
    /// The actions of the document (`win.`), which the window answers
    /// while its tab is in sight (see tabs.rs).
    actions: gio::SimpleActionGroup,
    info: AppInfo,
    opener: Option<&'static Opener>,
    title: adw::WindowTitle,
    toolbar_view: adw::ToolbarView,
    header: adw::HeaderBar,
    stack: gtk::Stack,
    view: DocumentView,
    /// Zoom, the position and the count (GNM-13).
    controls: crate::document_controls::Controls,
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
    /// The table of contents in the sidebar of Desk Writer (W011).
    contents: Rc<crate::contents_page::ContentsPage>,
    /// The comments in the sidebar of Desk Writer (W149).
    comments: Rc<crate::comments_page::CommentsPage>,
    /// The bubble of a comment at its text, the popover it is in, and the
    /// comment and the text it points at (W149).
    bubble: Rc<crate::comments_page::CommentsPage>,
    bubble_popover: gtk::Popover,
    bubble_at: Cell<Option<(Option<desk_model::NodeId>, crate::text_nav::SpanRect)>>,
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
    /// Table, whose popover shows the table styles (PXV-14).
    table_button: gtk::MenuButton,
    /// Chart, whose popover shows the chart styles (W080).
    chart_button: gtk::MenuButton,
    /// The Media Browser, made when first shown (INT-14).
    media_browser: std::cell::OnceCell<Rc<crate::media_browser::MediaBrowser>>,
    ruler: Ruler,
    vertical_ruler: Ruler,
    /// The corner where the rulers meet.
    ruler_corner: gtk::Box,
    /// The recovery draft of the document, once one is written.
    draft: RefCell<Option<crate::recovery::Draft>>,
    /// What Desk Slides adds to the window; none in Desk Writer.
    slides: Option<Rc<slides::Slides>>,
    /// Whether the document changed since the last draft.
    draft_due: Cell<bool>,
    /// The notices over the pages, such as what Screen View hides.
    toasts: adw::ToastOverlay,
    /// How the pages showed before Screen View: the zoom, how it
    /// followed the window, and whether the thumbnails showed (PXV-01).
    before_screen: Cell<Option<(f64, Option<geometry::Fit>, bool)>>,
    /// The save that waits for a pause in the editing (DOC-11).
    autosave_source: RefCell<Option<glib::SourceId>>,
    /// Whether the file as it was opened is kept as a version yet.
    opened_kept: Cell<bool>,
}

/// How long a pause in the editing is before the document saves itself
/// (DOC-11).
const AUTOSAVE_SECONDS: u32 = 2;

thread_local! {
    /// The open windows, so that a file opened from outside goes to an
    /// empty window when there is one.
    static WINDOWS: RefCell<Vec<Rc<DocumentWindow>>> = const { RefCell::new(Vec::new()) };
}

/// The actions that need a document.
const DOCUMENT_ACTIONS: [&str; 81] = [
    "outline",
    "screen-view",
    "text-size",
    "contents",
    "edit-toc",
    "go-to-page-end",
    "sidebar",
    "comments",
    "colors",
    "navigate",
    "thumbnails",
    "delete-page",
    "special-characters",
    "make-upper-case",
    "make-lower-case",
    "capitalize",
    "convert-quotes",
    "language-region",
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
    "insert-comment",
    "next-comment",
    "previous-comment",
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

/// The character toggles of the Format pane and the keyboard: an action
/// and what it sets.
const TOGGLES: [(&str, &str); 4] = [
    ("bold", "Bold"),
    ("italic", "Italic"),
    ("underline", "Underline"),
    ("strikethrough", "Strikethrough"),
];

impl DocumentWindow {
    /// A new, empty document in a tab of the window in front, or in a new
    /// window when the app has none.
    pub fn new(app: &adw::Application, info: AppInfo, opener: Option<&'static Opener>) -> Rc<Self> {
        let frame = Frame::active(app).unwrap_or_else(|| Self::new_frame(app, info, opener));
        Self::new_in(&frame, info, opener)
    }

    /// A new, empty document in a new window (New Window).
    pub fn new_window(
        app: &adw::Application,
        info: AppInfo,
        opener: Option<&'static Opener>,
    ) -> Rc<Self> {
        let frame = Self::new_frame(app, info, opener);
        Self::new_in(&frame, info, opener)
    }

    /// A new window of the app, whose overview opens a new document.
    fn new_frame(
        app: &adw::Application,
        info: AppInfo,
        opener: Option<&'static Opener>,
    ) -> Rc<Frame> {
        let frame = Frame::new(app, info.name);
        let weak = Rc::downgrade(&frame);
        frame.set_new_tab(move || {
            let frame = weak.upgrade()?;
            let document = Self::new_in(&frame, info, opener);
            if opener.is_some() {
                document.new_from_blank();
            }
            document.tab.get().cloned()
        });
        frame
    }

    /// A new, empty document in a new tab of `frame`.
    fn new_in(frame: &Rc<Frame>, info: AppInfo, opener: Option<&'static Opener>) -> Rc<Self> {
        let is_slides = crate::is_slides(info);
        // The controls over the document, and Go to Page in the View menu
        // (step 3 of M+).
        let controls = crate::document_controls::Controls::new_for(is_slides);
        let header::Header {
            header,
            title,
            document_buttons,
            insert_button,
            objects: object_buttons,
            media_button,
            table_button,
            chart_button,
            find_toggle,
            menu_button,
        } = header::Header::new(info, opener.is_some(), &controls.navigation);
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

        if let Some(popover) = menu_button.popover().and_downcast::<gtk::PopoverMenu>() {
            popover.add_child(&controls.go_to, "go-to-page");
        }

        let editor: Rc<RefCell<Option<TextEditor>>> = Rc::new(RefCell::new(None));
        let editor_of = || -> EditorOf {
            let cell = editor.clone();
            Rc::new(move || cell.borrow().clone())
        };
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
        // Desk Slides has the presenter notes under the slide, and the
        // light table in its place (KINT-05, KINT-09).
        let slides = is_slides.then(|| slides::Slides::new(editor_of()));
        let toasts = adw::ToastOverlay::new();
        if let Some(slides) = &slides {
            let column = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .build();
            pages.set_vexpand(true);
            column.append(&pages);
            column.append(&slides.notes_revealer);
            stack.add_named(&column, Some("document"));
            stack.add_named(slides.light_table.widget(), Some("light-table"));
            controls.add_to_zoom(&slides.size);
        } else {
            toasts.set_child(Some(&pages));
            stack.add_named(&toasts, Some("document"));
        }
        // The document as an outline (TXT-14).
        let outline = crate::outline_view::OutlineView::new(editor_of());
        stack.add_named(outline.widget(), Some("outline"));
        // The page thumbnails, in the navigation sidebar (INT-19, EST-17,
        // GNM-12).
        let thumbnails = crate::thumbnails::Thumbnails::new(&view, editor_of());
        let contents = crate::contents_page::ContentsPage::new(editor_of());
        let comments = crate::comments_page::CommentsPage::new(editor_of());
        let bubble = crate::comments_page::CommentsPage::bubble(editor_of());
        let bubble_popover = gtk::Popover::builder()
            .child(bubble.widget())
            .position(gtk::PositionType::Bottom)
            .css_classes(["desk-comment-popover"])
            .build();
        bubble_popover.set_parent(view.area());
        let nav = if let Some(slides) = &slides {
            // The navigator, the outline and the master slides of Keynote
            // (KINT-07, KINT-08, KGNM-13).
            let nav = NavSidebar::new(&tr("Add Slide"), slides::add_slide_menu().upcast_ref());
            nav.add_page(
                "pages",
                &tr("Navigator"),
                nav_sidebar::WIDTH,
                thumbnails.widget(),
            );
            nav.add_page(
                "topics",
                &trc("slides", "Outline"),
                slides::TOPICS_WIDTH,
                slides.topics.widget(),
            );
            nav.add_page(
                "masters",
                &tr("Master Slides"),
                nav_sidebar::WIDTH,
                &slides::masters_page(),
            );
            nav
        } else {
            let nav = NavSidebar::new(&tr("Add Page"), add_page_menu().upcast_ref());
            nav.add_page(
                "pages",
                &tr("Pages"),
                nav_sidebar::WIDTH,
                thumbnails.widget(),
            );
            nav.add_page(
                "contents",
                &tr("Table of Contents"),
                STYLES_WIDTH,
                contents.widget(),
            );
            nav.add_page("styles", &tr("Styles"), STYLES_WIDTH, drawer.widget());
            nav
        };
        // The styles that were the Styles drawer of Pages (INT-18), the
        // comments, and the search results that were its search sidebar
        // (TXT-13), as pages of the sidebar of interface B.
        // The comments of Desk Slides come with phase S2 (S158).
        let comments_widget: gtk::Widget = if slides.is_some() {
            comments_page().upcast()
        } else {
            comments.widget().clone().upcast()
        };
        nav.add_page(
            "comments",
            &tr("Comments"),
            nav_sidebar::WIDTH,
            &comments_widget,
        );
        nav.add_page(
            "search",
            &tr("Search Results"),
            SEARCH_WIDTH,
            search_sidebar.widget(),
        );
        if slides.is_some() {
            // The Slide tab of the Format pane when nothing is selected
            // (KINT-13), and the Animation level of the Inspector
            // (KINT-14).
            if let Some(slides) = &slides {
                inspector.add_context(
                    crate::inspector::Context::Slide,
                    vec![crate::inspector::Tab::new(
                        "slide",
                        n_("Slide"),
                        &[&slides.slide_group],
                    )],
                );
            }
            inspector.add_level(
                slides::ANIMATION,
                &tr("Animation"),
                &slides::animation_page(),
            );
        }
        let side = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        side.add_named(inspector.widget(), Some("inspector"));
        // The Inspector is a utility pane of a fixed width at the end.
        // The zoom and the count float over the document (GNM-13).
        let document_overlay = gtk::Overlay::builder().child(&stack).build();
        controls.attach(&document_overlay);
        let split = adw::OverlaySplitView::builder()
            .content(&document_overlay)
            .sidebar(&side)
            .min_sidebar_width(INSPECTOR_WIDTH)
            .max_sidebar_width(INSPECTOR_WIDTH)
            .sidebar_position(gtk::PackType::End)
            .show_sidebar(false)
            .build();
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        // The tabs of the open documents, under the header bar; it shows
        // once two documents are open.
        toolbar_view.add_top_bar(
            &adw::TabBar::builder()
                .view(frame.view())
                .autohide(true)
                .build(),
        );
        toolbar_view.add_top_bar(&screen_banner);
        toolbar_view.add_top_bar(&warnings_banner);
        let find = FindWindow::new(editor_of(), search.clone());
        toolbar_view.add_top_bar(find.widget());
        toolbar_view.set_content(Some(&split));

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

        let window = frame.window().clone();
        let root = adw::BreakpointBin::builder()
            .child(&nav_split)
            .width_request(360)
            .height_request(294)
            .build();

        // The breakpoints belong to the tab, which is as wide as the window.
        // Of the breakpoints that match, the bin applies the one added
        // last, so each one sets what the wider ones set too. Below 720,
        // as when the window takes half of a screen
        // of 1280 × 720, the smallest Desk supports, the sidebars cover the
        // pages instead of pushing them; below 520 the Inspector covers
        // them too, so that the pages keep some room (GNM-01).
        let mut breakpoints = Vec::new();
        for (width, collapsed) in [
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
                let menu = if is_slides {
                    slides::insert_menu(true)
                } else {
                    header::insert_menu(true)
                };
                breakpoint.add_setter(&insert_button, "menu-model", Some(&menu.to_value()));
                // The count goes over the zoom, at the end.
                controls.stack_in(&breakpoint);
            }
            breakpoints.push(breakpoint);
        }
        for breakpoint in breakpoints {
            root.add_breakpoint(breakpoint);
        }

        let this = Rc::new(Self {
            window,
            frame: frame.clone(),
            root,
            tab: std::cell::OnceCell::new(),
            actions: gio::SimpleActionGroup::new(),
            info,
            opener,
            title,
            toolbar_view,
            header,
            stack,
            view,
            controls,
            counted: Cell::new(None),
            warnings_banner,
            path: RefCell::new(None),
            name: RefCell::new(String::new()),
            warnings: RefCell::new(Vec::new()),
            editor,
            inspector,
            drawer,
            fonts: FontsSlot::default(),
            split,
            search,
            find,
            search_sidebar,
            thumbnails,
            contents,
            comments,
            bubble,
            bubble_popover,
            bubble_at: Cell::new(None),
            outline,
            nav,
            nav_split,
            nav_before_fullscreen: Cell::new(false),
            side,
            spelling: std::cell::OnceCell::new(),
            media_button,
            table_button,
            chart_button,
            media_browser: std::cell::OnceCell::new(),
            ruler,
            vertical_ruler,
            ruler_corner,
            draft: RefCell::new(None),
            draft_due: Cell::new(false),
            toasts,
            before_screen: Cell::new(None),
            autosave_source: RefCell::new(None),
            opened_kept: Cell::new(false),
            slides,
        });
        this.add_actions();
        this.add_text_format_actions();
        this.connect_header(&find_toggle);
        this.add_rename();
        this.add_slides_actions();
        warn_small_screen(&this.window, &screen_banner);
        this.show_rulers();
        {
            let current = preferences::get();
            this.view.set_zoom(f64::from(current.zoom) / 100.0);
            this.controls.show_zoom(this.view.zoom());
            this.show_window_preferences(&current);
        }
        let weak = Rc::downgrade(&this);
        preferences::connect_changed(move |p| {
            if let Some(this) = weak.upgrade() {
                this.show_rulers();
                this.show_window_preferences(p);
                formatting::set_state(
                    &this.actions,
                    "check-spelling-while-typing",
                    p.check_spelling.to_variant(),
                );
                formatting::set_state(
                    &this.actions,
                    "proofread-while-typing",
                    p.proofread.to_variant(),
                );
                if let Some(editor) = this.editor() {
                    editor.update_squiggles();
                }
            }
        });
        {
            let weak = Rc::downgrade(&this);
            this.view.connect_zoom_changed(move |zoom| {
                if let Some(this) = weak.upgrade() {
                    this.controls.show_zoom(zoom);
                }
            });
        }
        {
            // The page in sight shows in the position and in Go to Page.
            let weak = Rc::downgrade(&this);
            this.view.connect_moved(move || {
                if let Some(this) = weak.upgrade() {
                    this.show_position();
                    this.place_bubble();
                }
            });
            // The bubble of a comment goes when it has nothing to show, and
            // gives the keyboard back to the document when it goes.
            let weak = Rc::downgrade(&this);
            this.bubble.connect_empty(move || {
                if let Some(this) = weak.upgrade() {
                    this.bubble_popover.popdown();
                }
            });
            let weak = Rc::downgrade(&this);
            this.bubble_popover.connect_closed(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.bubble.set_shown(false);
                    this.bubble_at.set(None);
                    if let Some(editor) = this.editor() {
                        editor.focus();
                    }
                }
            });
            let weak = Rc::downgrade(&this);
            this.controls.page_entry.connect_activate(move |entry| {
                let Some(this) = weak.upgrade() else {
                    return;
                };
                let count = this.view.page_count();
                if let (Ok(page), Some(editor)) =
                    (entry.text().trim().parse::<usize>(), this.editor())
                    && (1..=count).contains(&page)
                {
                    if let Some(popover) = entry
                        .ancestor(gtk::Popover::static_type())
                        .and_downcast::<gtk::Popover>()
                    {
                        popover.popdown();
                    }
                    editor.go_to_page(page - 1);
                    editor.focus();
                } else {
                    this.window.error_bell();
                }
            });
            // Go to Page shows the page in sight, ready to be typed over.
            let weak = Rc::downgrade(&this);
            this.controls.page_entry.connect_map(move |entry| {
                if let Some(this) = weak.upgrade() {
                    if let Some(page) = this.view.current_page() {
                        entry.set_text(&(page + 1).to_string());
                    }
                    entry.select_region(0, -1);
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.stack.connect_visible_child_name_notify(move |stack| {
                let document = stack.visible_child_name().as_deref() != Some("empty");
                if let Some(this) = weak.upgrade() {
                    this.controls.set_shown(document);
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
                    if is_slides {
                        crate::shortcuts::slides_dialog().present(Some(&window));
                    } else {
                        crate::shortcuts::dialog(opens).present(Some(&window));
                    }
                }
            });
            this.actions.add_action(&shortcuts);
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
        // A window of Desk Slides starts a presentation at once, until the
        // theme chooser comes in phase 3.
        if is_slides && opener.is_none() {
            this.show_new_presentation();
        }
        WINDOWS.with(|w| w.borrow_mut().push(this.clone()));
        let tab = frame.add(&this.root, this.clone());
        this.tab.set(tab).ok();
        this.show_title(info.name);
        this
    }

    /// Starts the document from the Blank template (Ctrl+N, INT-01).
    pub fn new_from_blank(&self) {
        gio::prelude::ActionGroupExt::activate_action(
            &self.actions,
            "new-from-template",
            Some(&"blank".to_variant()),
        );
    }

    /// Brings the tab of the document and its window to the front.
    pub fn show(&self) {
        if let Some(tab) = self.tab.get() {
            self.frame.view().set_selected_page(tab);
        }
        self.window.present();
    }

    /// The name of the document on its tab, which the window shows as its
    /// title while the tab is in sight.
    fn show_title(&self, title: &str) {
        if let Some(tab) = self.tab.get() {
            tab.set_title(title);
            tab.set_tooltip(title);
        }
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
    /// was saved, or a document that has never been saved. A document
    /// that saves itself loses nothing (DOC-11).
    fn needs_saving(&self) -> bool {
        // Desk Slides saves presentations with phase 1 of its plan.
        self.opener.is_some()
            && !self.autosaves()
            && self
                .editor()
                .is_some_and(|e| e.is_modified() || self.path.borrow().is_none())
    }

    /// Whether the document saves itself: it has a file of the kind the
    /// app saves (DOC-11, D-001). A new document, or one of another kind
    /// such as a Pages document, waits for Save As, and keeps a recovery
    /// draft until then.
    fn autosaves(&self) -> bool {
        self.opener.is_some()
            && self
                .path
                .borrow()
                .as_deref()
                .is_some_and(|p| self.saves_in_place(p))
    }

    /// Saves the document a moment after the editing pauses (DOC-11).
    fn schedule_autosave(self: &Rc<Self>) {
        if !self.autosaves() {
            return;
        }
        if let Some(source) = self.autosave_source.borrow_mut().take() {
            source.remove();
        }
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_seconds_local_once(AUTOSAVE_SECONDS, move || {
            if let Some(this) = weak.upgrade() {
                // The source has run: forget it before saving.
                this.autosave_source.borrow_mut().take();
                this.autosave();
            }
        });
        *self.autosave_source.borrow_mut() = Some(source);
    }

    /// Saves the changes of a document that saves itself, keeping a
    /// version of the file first when one is due, and says whether the
    /// file holds the document now.
    fn autosave(&self) -> bool {
        if let Some(source) = self.autosave_source.borrow_mut().take() {
            source.remove();
        }
        let Some(path) = self.path.borrow().clone() else {
            return false;
        };
        if !self.autosaves() {
            return false;
        }
        if !self.editor().is_some_and(|e| e.is_modified()) {
            return true;
        }
        self.keep_version_before_writing(&path);
        if !self.write_to(&path) {
            return false;
        }
        if let Some(editor) = self.editor() {
            editor.mark_saved();
        }
        self.refresh();
        true
    }

    /// Keeps the file as it is as a version before it is written over:
    /// the first time after the document opened, then once an hour, and
    /// thins the old versions (DOC-11).
    fn keep_version_before_writing(&self, path: &Path) {
        if !path.exists() {
            return;
        }
        let store = crate::versions::Store::for_document(path);
        let now = crate::versions::now();
        let kind = if !self.opened_kept.get() {
            Some(crate::versions::Kind::Opened)
        } else if store
            .latest()
            .is_none_or(|v| now - v.time >= crate::versions::HOURLY_SECONDS)
        {
            Some(crate::versions::Kind::Automatic)
        } else {
            None
        };
        if let Some(kind) = kind
            && store.keep(path, kind, now).is_ok()
            && kind == crate::versions::Kind::Opened
        {
            self.opened_kept.set(true);
        }
        store.thin(now, crate::versions::local_offset());
    }

    /// Opens a document in this window, or says why it cannot.
    pub fn load(self: &Rc<Self>, opener: &Opener, path: &Path) {
        match crate::open_document(opener, path) {
            Ok(shown) => {
                if let Some(source) = self.autosave_source.borrow_mut().take() {
                    source.remove();
                }
                self.opened_kept.set(false);
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
        // A new document opens in Screen View, if the preferences ask
        // (PXV-01).
        if preferences::get().screen_view && self.slides.is_none() {
            self.actions
                .change_action_state("screen-view", &true.to_variant());
        }
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
        if let Some(action) = self.actions.lookup_action("page-layout") {
            action.activate(Some(&layout.to_variant()));
        }
        match view.zoom {
            desk_model::ViewZoom::FitWidth => self.view.set_fit(Some(geometry::Fit::Width)),
            desk_model::ViewZoom::FitPage => self.view.set_fit(Some(geometry::Fit::Page)),
            desk_model::ViewZoom::Percent(p) => self.view.set_zoom(f64::from(p) / 100.0),
        }
        self.outline.set_options(view.outline_options);
        if let Some(action) = self.actions.lookup_action("outline") {
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
                this.schedule_autosave();
                this.refresh();
                this.search_sidebar.document_changed();
                this.thumbnails.refresh();
                this.contents.refresh();
                this.comments.refresh();
                this.bubble.refresh();
                this.outline.document_changed();
                this.slides_changed();
            }
        });
        let weak = Rc::downgrade(self);
        editor.connect_comment_clicked(move |id| {
            if let Some(this) = weak.upgrade() {
                this.show_bubble(id);
            }
        });
        let weak = Rc::downgrade(self);
        editor.connect_selection_changed(move || {
            if let Some(this) = weak.upgrade() {
                this.show_format();
                this.show_count();
                if this.nav_shows("comments") {
                    this.comments
                        .show_current(this.editor().and_then(|e| e.comment_at_cursor()));
                }
            }
        });
        editor.set_show_invisibles(self.action_state("invisibles"));
        editor.set_show_connection_lines(self.action_state("connection-lines"));
        editor.set_show_layout(self.action_state("show-layout"));
        let screen = self.action_state("screen-view");
        *self.editor.borrow_mut() = Some(editor);
        self.stack.set_visible_child_name("document");
        // A document saved as an outline opens as one (DOC-16).
        if self.action_state("outline") {
            self.show_outline(true);
        }
        // Screen View stays on for the next document, if it can show it
        // (PXV-01).
        if screen && let Some(editor) = self.editor() {
            if editor.is_page_layout() || editor.is_presentation() {
                self.actions
                    .change_action_state("screen-view", &false.to_variant());
            } else {
                editor.set_screen_view(true);
                self.show_rulers();
            }
        }
        // A page layout document opens with its thumbnails and both
        // rulers, as in Pages (INT-03).
        let presentation = self.editor().is_some_and(|e| e.is_presentation());
        if presentation {
            // A slide fills the window, as in Keynote, without rulers.
            self.view.set_fit(Some(geometry::Fit::Page));
            self.actions
                .change_action_state("ruler", &false.to_variant());
            self.show_rulers();
        }
        if self.editor().is_some_and(|e| e.is_page_layout()) {
            if !presentation {
                self.actions
                    .change_action_state("ruler", &true.to_variant());
            }
            self.show_rulers();
            // The sidebar opens once the window has its size.
            let weak = Rc::downgrade(self);
            glib::idle_add_local_once(move || {
                if let Some(this) = weak.upgrade() {
                    this.actions
                        .change_action_state("thumbnails", &true.to_variant());
                }
            });
        }
        self.refresh();
        self.show_format();
    }

    /// Shows the Inspector at a tab of its Format pane, for a command
    /// whose control lives there, such as Fonts and Colors (D-024).
    fn reveal_format(&self, tab: &str) {
        self.actions
            .change_action_state("inspector", &true.to_variant());
        self.inspector.show_tab(tab);
    }

    /// Shows the formatting at the selection in the Inspector and the
    /// states of the toggles.
    fn show_format(&self) {
        let Some(editor) = self.editor() else { return };
        let format = editor.format();
        self.show_format_states(&editor, &format);
        self.inspector.show(&format);
        self.show_slide_format(&editor);
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
            formatting::set_state(&self.actions, name, on.to_variant());
        }
        formatting::set_state(
            &self.actions,
            "align",
            formatting::alignment_name(format.paragraph.alignment).to_variant(),
        );
        formatting::set_state(
            &self.actions,
            "columns",
            i32::from(format.layout.columns.count).to_variant(),
        );
    }

    /// Shows what the preferences say about the window: the count over
    /// the document, the color of invisibles and that of the guides.
    fn show_window_preferences(&self, preferences: &preferences::Preferences) {
        self.controls.set_counted(preferences.show_counts);
        let count = crate::document_controls::Count::from_name(&preferences.count);
        formatting::set_state(&self.actions, "count", count.name().to_variant());
        self.show_count();
        self.view.set_invisibles_color(preferences.invisibles_color);
        self.view.set_guide_color(preferences.guide_color);
    }

    /// Shows the rulers that Show Ruler and the preferences ask for.
    fn show_rulers(&self) {
        // Screen View has no rulers (PXV-01).
        let screen = self.editor().is_some_and(|e| e.screen_view());
        let on = self.action_state("ruler") && !screen;
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
        formatting::set_state(&self.actions, "sidebar", shown.to_variant());
        let page = self.nav.page();
        let width = self.nav.width(&page);
        self.nav_split.set_min_sidebar_width(width);
        self.nav_split.set_max_sidebar_width(width);
        for &(name, action) in self.nav_pages() {
            let on = shown && page == name;
            let was = self.action_state(action);
            formatting::set_state(&self.actions, action, on.to_variant());
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
                "contents" => self.contents.set_shown(on),
                "comments" if self.slides.is_none() => self.comments.set_shown(on),
                "styles" => self.show_format(),
                "topics" => {
                    if let Some(slides) = &self.slides {
                        slides.topics.set_shown(on);
                    }
                }
                _ => {}
            }
        }
    }

    /// Shows over the foot of the document the count of the document, or
    /// of the selection when text is selected (DOC-20).
    fn show_count(&self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let kind = self
            .window
            .lookup_action("count")
            .and_then(|a| a.state())
            .and_then(|s| s.get::<String>())
            .map_or(crate::document_controls::Count::Words, |n| {
                crate::document_controls::Count::from_name(&n)
            });
        let all = self.counted.get().unwrap_or_else(|| {
            let all = editor.main_statistics();
            self.counted.set(Some(all));
            all
        });
        self.controls
            .show_count(&crate::document_controls::count_label(
                kind,
                &all,
                editor.selection_statistics().as_ref(),
                self.view.page_count(),
            ));
    }

    /// The pages of the navigation sidebar, and the actions that show
    /// them.
    fn nav_pages(&self) -> &'static [(&'static str, &'static str)] {
        if self.slides.is_some() {
            &slides::NAV_PAGES
        } else {
            &NAV_PAGES
        }
    }

    /// Shows the number of pages after Go to Page (INT-19).
    fn show_position(&self) {
        if self.slides.is_some() {
            self.show_slide();
            return;
        }
        self.controls.show_page_count(self.view.page_count().max(1));
    }

    fn action_state(&self, name: &str) -> bool {
        self.actions
            .lookup_action(name)
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
            .is_some_and(|a| formatting::state_of(&a))
    }

    /// Shows the name, whether the document has changes, the count over
    /// the document, and which actions apply now.
    fn refresh(&self) {
        let editor = self.editor();
        let path = self.path.borrow().clone();
        let name = self.name.borrow().clone();
        if let Some(editor) = &editor {
            let text = self.inspector.text();
            let paragraphs = text.set_styles(&editor.paragraph_styles());
            let characters = text.set_character_styles(&editor.character_styles());
            if paragraphs || characters {
                self.show_format();
            }
            let edited = if self.needs_saving() { "• " } else { "" };
            self.title.set_title(&format!("{edited}{name}"));
            self.title
                .set_subtitle(&path.as_deref().map(folder_of).unwrap_or_default());
            self.show_title(&format!("{edited}{name}"));
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
            (
                "revert",
                self.autosaves() && (modified || self.opened_kept.get()),
            ),
            ("versions", self.autosaves()),
            ("properties", path.is_some()),
        ] {
            self.enable(action, enabled);
        }
        for action in DOCUMENT_ACTIONS
            .iter()
            .chain(&objects::OBJECT_ACTIONS)
            .chain(&tables::TABLE_ACTIONS)
            .chain(&format_actions::FORMAT_BAR_ACTIONS)
        {
            self.enable(action, has_document);
        }
        if self.slides.is_some() {
            for action in slides::ACTIONS {
                self.enable(action, has_document);
            }
        }
        // Paste Style is off until a style is copied, as in Pages.
        let copied = COPIED_STYLE.with(|c| c.borrow().is_some());
        self.enable("paste-style", has_document && copied);
        // Screen View is for word processing documents, and while it is
        // on the pages, their thumbnails and their layout do not show
        // (PXV-01).
        let screen = editor.as_ref().is_some_and(TextEditor::screen_view);
        let words = editor
            .as_ref()
            .is_some_and(|e| !e.is_page_layout() && !e.is_presentation());
        self.enable("screen-view", has_document && words);
        self.enable("text-size", has_document && screen);
        // Screen View has no pages to go to, and shows no page count, as
        // in Pages 15.4 (PXV-01).
        for action in [
            "thumbnails",
            "show-layout",
            "ruler",
            "page-layout",
            "go-to-page-end",
        ] {
            self.enable(action, has_document && !screen);
        }
        self.controls.go_to.set_sensitive(!screen);
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
    ///
    /// A document that saves itself saves its changes now and keeps a
    /// version of what it holds, as Cmd-S of iWork does (DOC-11).
    fn save(self: &Rc<Self>, done: impl FnOnce(bool) + 'static) {
        let path = self.path.borrow().clone();
        match path {
            Some(path) if self.autosaves() => {
                let saved = self.autosave();
                if saved {
                    let _ = crate::versions::Store::for_document(&path).keep(
                        &path,
                        crate::versions::Kind::Saved,
                        crate::versions::now(),
                    );
                    self.drop_draft();
                    remember_recent(&path);
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
        // A document that saves itself needs no draft (DOC-11).
        if self.autosaves() || !self.draft_due.get() || !editor.is_modified() {
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
        // The file is new: its first version is what was saved now.
        self.opened_kept.set(false);
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
        window.show();
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

    /// Shows the text in one flow as wide as the window, or as pages
    /// again as they showed before (PXV-01).
    fn show_screen_view(&self, on: bool) {
        let Some(editor) = self.editor() else {
            return;
        };
        if on && editor.screen_view() {
            return;
        }
        if on {
            if editor.is_page_layout() || editor.is_presentation() {
                formatting::set_state(&self.actions, "screen-view", false.to_variant());
                return;
            }
            if self.action_state("outline") {
                self.actions
                    .change_action_state("outline", &false.to_variant());
            }
            let thumbnails = self.action_state("thumbnails");
            self.before_screen
                .set(Some((self.view.zoom(), self.view.fit(), thumbnails)));
            if thumbnails {
                self.actions
                    .change_action_state("thumbnails", &false.to_variant());
            }
            // The text starts at Text Size ▸ Auto.
            self.view
                .set_zoom(geometry::text_size_auto(f64::from(self.window.width())));
            editor.set_screen_view(true);
            // Pages 15.4 always says so, and says what it hides when it
            // hides something (PXV-01).
            let notice = if editor.screen_hides_content() {
                tr("Some content is hidden in Screen View")
            } else {
                tr("Screen View is on")
            };
            self.toasts.add_toast(adw::Toast::new(&notice));
        } else {
            editor.set_screen_view(false);
            self.toasts
                .add_toast(adw::Toast::new(&tr("Screen View is off")));
            if let Some((zoom, fit, thumbnails)) = self.before_screen.take() {
                match fit {
                    Some(fit) => self.view.set_fit(Some(fit)),
                    None => self.view.set_zoom(zoom),
                }
                if thumbnails {
                    self.actions
                        .change_action_state("thumbnails", &true.to_variant());
                }
            }
        }
        self.show_rulers();
        self.refresh();
        editor.show_selection();
        editor.focus();
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

    /// Goes back to the document as it was when it opened, as Revert To
    /// ▸ Last Opened of iWork 15 (DOC-11, DOC-12). Nothing is lost: what
    /// the document holds now is kept as a version first.
    fn revert(self: &Rc<Self>) {
        let (Some(opener), Some(path)) = (self.opener, self.path.borrow().clone()) else {
            return;
        };
        if !self.opened_kept.get() {
            // The file is still as it was opened.
            self.load(opener, &path);
            return;
        }
        let opened = crate::versions::Store::for_document(&path)
            .list()
            .into_iter()
            .find(|v| v.kind == crate::versions::Kind::Opened);
        if let Some(version) = opened {
            self.restore(&version);
        }
    }

    /// Puts an older version in place of the document, keeping what the
    /// document holds now as a version first (DOC-11).
    pub(crate) fn restore(self: &Rc<Self>, version: &crate::versions::Version) {
        let (Some(opener), Some(path)) = (self.opener, self.path.borrow().clone()) else {
            return;
        };
        if !self.autosave() {
            return;
        }
        let store = crate::versions::Store::for_document(&path);
        if path.exists() {
            let _ = store.keep(
                &path,
                crate::versions::Kind::Automatic,
                crate::versions::now(),
            );
        }
        match crate::open_document(opener, &version.path) {
            Ok(shown) => {
                self.show_document(shown.document);
                if self.write_to(&path)
                    && let Some(editor) = self.editor()
                {
                    editor.mark_saved();
                }
                self.refresh();
            }
            Err(message) => self.alert(&tr("Cannot Restore the Version"), &message),
        }
    }

    /// Opens an older version in a new window, as an untitled copy, to
    /// take what is needed from it (DOC-11).
    pub(crate) fn open_version_copy(&self, version: &crate::versions::Version, name: String) {
        let (Some(opener), Some(app)) = (
            self.opener,
            self.window
                .application()
                .and_then(|a| a.downcast::<adw::Application>().ok()),
        ) else {
            return;
        };
        match crate::open_document(opener, &version.path) {
            Ok(shown) => {
                let copy = DocumentWindow::new(&app, self.info, self.opener);
                copy.load_untitled(shown.document, name);
                copy.window.present();
            }
            Err(message) => self.alert(&tr("Cannot Open the Version"), &message),
        }
    }

    /// Asks to save the changes before the tab closes (DOC-18), and says
    /// through `done` whether it closes.
    fn ask_to_close(self: &Rc<Self>, done: Box<dyn FnOnce(bool)>) {
        // A document that saves itself saves what is left, and closes
        // without asking (DOC-11).
        if self.autosaves() {
            let saved = self.autosave();
            if saved {
                self.drop_draft();
            }
            done(saved);
            return;
        }
        if !self.needs_saving() {
            self.drop_draft();
            done(true);
            return;
        }
        // The tab must be in sight for its dialog to make sense.
        if let Some(tab) = self.tab.get() {
            self.frame.view().set_selected_page(tab);
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
                let Some(this) = weak.upgrade() else {
                    done(false);
                    return;
                };
                match response.as_str() {
                    "discard" => {
                        this.drop_draft();
                        done(true);
                    }
                    "save" => {
                        let weak = Rc::downgrade(&this);
                        this.save(move |saved| {
                            if let (true, Some(this)) = (saved, weak.upgrade()) {
                                this.drop_draft();
                            }
                            done(saved);
                        });
                    }
                    _ => done(false),
                }
            },
        );
    }

    /// Full screen hides the header bar until the pointer reaches the top
    /// of the screen; the zoom and the count stay over the document, and Esc leaves it
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

        // The table styles of the document, drawn again each time the
        // popover of the Table button opens; a click inserts a table of
        // the style (PXV-14).
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        holder.set_margin_top(6);
        holder.set_margin_bottom(6);
        holder.set_margin_start(6);
        holder.set_margin_end(6);
        let popover = gtk::Popover::builder().child(&holder).build();
        self.table_button.set_popover(Some(&popover));
        let weak = Rc::downgrade(self);
        popover.connect_show(move |popover| {
            let Some(editor) = weak.upgrade().and_then(|this| this.editor()) else {
                return;
            };
            while let Some(child) = holder.first_child() {
                holder.remove(&child);
            }
            let weak_popover = popover.downgrade();
            let target = editor.clone();
            let grid = crate::table_styles::grid(&editor.table_styles(), None, move |style| {
                if let Some(popover) = weak_popover.upgrade() {
                    popover.popdown();
                }
                target.insert_table_of(Some(style));
            });
            holder.append(&grid);
        });

        // The chart styles of the document, in the same way: a click
        // inserts a column chart of the style (W080).
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        holder.set_margin_top(6);
        holder.set_margin_bottom(6);
        holder.set_margin_start(6);
        holder.set_margin_end(6);
        let popover = gtk::Popover::builder().child(&holder).build();
        self.chart_button.set_popover(Some(&popover));
        let weak = Rc::downgrade(self);
        popover.connect_show(move |popover| {
            let Some(editor) = weak.upgrade().and_then(|this| this.editor()) else {
                return;
            };
            while let Some(child) = holder.first_child() {
                holder.remove(&child);
            }
            let weak_popover = popover.downgrade();
            let target = editor.clone();
            let grid = crate::chart_styles::grid(&editor.chart_styles(), None, move |style| {
                if let Some(popover) = weak_popover.upgrade() {
                    popover.popdown();
                }
                target.insert_chart_of(desk_model::chart::ChartKind::Column, Some(style));
            });
            holder.append(&grid);
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
            let (view, insert) = if this.slides.is_some() {
                (
                    slides::view_menu(&this.controls.navigation),
                    slides::insert_menu(true),
                )
            } else {
                (
                    header::view_menu(&this.controls.navigation),
                    header::insert_menu(true),
                )
            };
            let mut menus: Vec<(String, gio::MenuModel)> = [
                (
                    tr("Main Menu"),
                    header::main_menu(this.info, opens, &gio::Menu::new()),
                ),
                (tr("Main Menu"), header::more_commands(opens)),
                (tr("View"), view),
                (tr("Insert"), insert),
                (tr("Edit"), edit_menu()),
                (tr("Format"), format_menu()),
                (tr("Arrange"), objects::arrange_menu()),
            ]
            .into_iter()
            .map(|(name, menu)| (name, menu.upcast()))
            .chain(this.inspector.text().search_menus())
            .chain([(tr("Objects"), crate::format_pane::object_menu().upcast())])
            .collect();
            if this.slides.is_some() {
                menus.push((tr("Slide"), slides::slide_menu().upcast()));
            }
            command_search::present(&this.window, this.actions.upcast_ref(), &menus);
        });
        self.actions.add_action(&search);
    }

    fn add_actions(self: &Rc<Self>) {
        let simple: [(&str, WindowAction); 13] = [
            ("warnings", |this| this.show_warnings()),
            // Edit in the table of contents of the sidebar: the styles it
            // lists are in the Document tab of the Inspector (W011).
            ("edit-toc", |this| {
                this.actions
                    .change_action_state("inspector", &true.to_variant());
                this.inspector.show_toc_settings();
            }),
            ("versions", |this| this.browse_versions()),
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
            ("language-region", |this| {
                if let Some(editor) = this.editor() {
                    crate::language_region::present(&this.window, &editor);
                }
            }),
            ("fullscreen", |this| {
                if this.window.is_fullscreen() {
                    this.window.unfullscreen();
                } else {
                    this.window.fullscreen();
                }
            }),
            // Ctrl+W closes the tab, as in GNOME.
            ("close", |this| this.frame.close(&this.root)),
            ("tab-overview", |this| this.frame.show_overview()),
        ];
        let template = gio::SimpleAction::new("new-from-template", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        template.connect_activate(move |_, target| {
            let (Some(this), Some(target)) = (weak.upgrade(), target.and_then(|t| t.str())) else {
                return;
            };
            this.load_untitled(from_template(target), tr("Untitled"));
        });
        self.actions.add_action(&template);
        for (name, run) in simple {
            let weak = Rc::downgrade(self);
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| {
                if let Some(this) = weak.upgrade() {
                    run(&this);
                }
            });
            self.actions.add_action(&action);
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
            // In Screen View the zoom sets the size of the text, and the
            // flow is always as wide as the window (PXV-01).
            let screen = this.editor().is_some_and(|e| e.screen_view());
            let value = match value.as_str() {
                "fit-width" | "fit-page" | "selection" | "content" if screen => {
                    geometry::DEFAULT_ZOOM.to_string()
                }
                _ => value,
            };
            let zoom = match value.as_str() {
                "in" => geometry::zoom_in(view.zoom()),
                "out" => geometry::zoom_out(view.zoom()),
                // The zoom then follows the window (INT-08).
                "fit-width" => return view.set_fit(Some(geometry::Fit::Width)),
                "fit-page" => return view.set_fit(Some(geometry::Fit::Page)),
                // Zoom to Selection and Zoom to Fit Content (B-027).
                "selection" => {
                    match view.selection_bounds() {
                        Some((page, bounds)) => view.zoom_to_box(page, bounds),
                        None => this.window.error_bell(),
                    }
                    return;
                }
                "content" => {
                    if let Some(page) = view.current_page() {
                        let beyond = this.editor().and_then(|e| e.objects_bounds(page));
                        view.zoom_to_content(page, beyond);
                    }
                    return;
                }
                level => level.parse().unwrap_or(geometry::DEFAULT_ZOOM),
            };
            view.set_zoom(zoom);
        });
        self.actions.add_action(&zoom);

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
        self.actions.add_action(&layout);

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
            ("make-upper-case", |e: &TextEditor| {
                e.change_case(desk_model::CaseChange::Upper);
            }),
            ("make-lower-case", |e: &TextEditor| {
                e.change_case(desk_model::CaseChange::Lower);
            }),
            ("capitalize", |e: &TextEditor| {
                e.change_case(desk_model::CaseChange::Capitalize);
            }),
            ("convert-quotes", TextEditor::convert_quotes),
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
            self.actions.add_action(&action);
        }
        self.add_format_actions();
        self.refresh();
    }

    /// The toggles of the Format pane, Show Invisibles and the Inspector,
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
            self.actions.add_action(&action);
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
        self.actions.add_action(&align);

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
        self.actions.add_action(&invisibles);

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
        self.actions.add_action(&lines);

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
            this.actions.change_action_state("ruler", &on.to_variant());
        });
        self.actions.add_action(&layout);

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
        self.actions.add_action(&ruler);

        // The previous and the next place, in the View menu, and what
        // they go to (INT-19).
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
            this.controls.show_navigate(kind);
        });
        self.actions.add_action(&navigate_by);
        // What the count over the document counts (DOC-20), kept in the
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
            let kind = crate::document_controls::Count::from_name(&value);
            action.set_state(&kind.name().to_variant());
            this.show_count();
            let mut current = preferences::get();
            if current.count != kind.name() {
                current.count = kind.name().to_owned();
                preferences::set(current);
            }
        });
        self.actions.add_action(&count);
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
        self.actions.add_action(&navigate);
        // First Page and Last Page (B-028).
        let ends = gio::SimpleAction::new("go-to-page-end", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        ends.connect_activate(move |_, parameter| {
            let (Some(this), Some(end)) =
                (weak.upgrade(), parameter.and_then(|p| p.get::<String>()))
            else {
                return;
            };
            if let Some(editor) = this.editor() {
                let page = match end.as_str() {
                    "first" => 0,
                    _ => this.view.page_count().saturating_sub(1),
                };
                editor.go_to_page(page);
                editor.focus();
            }
        });
        self.actions.add_action(&ends);

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
        self.actions.add_action(&sidebar);
        for &(page, name) in self.nav_pages() {
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
            self.actions.add_action(&action);
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
            if on {
                this.actions
                    .change_action_state("screen-view", &false.to_variant());
            }
            this.show_outline(on);
        });
        self.actions.add_action(&outline);

        // View ▸ Screen View: the text in one flow as wide as the window,
        // without pages (PXV-01).
        let screen = gio::SimpleAction::new_stateful("screen-view", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        screen.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            this.show_screen_view(on);
        });
        self.actions.add_action(&screen);

        // View ▸ Text Size, in Screen View: Auto, Bigger and Smaller.
        let text_size = gio::SimpleAction::new("text-size", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        text_size.connect_activate(move |_, parameter| {
            let (Some(this), Some(value)) =
                (weak.upgrade(), parameter.and_then(|p| p.get::<String>()))
            else {
                return;
            };
            let zoom = this.view.zoom();
            this.view.set_zoom(match value.as_str() {
                "bigger" => geometry::text_bigger(zoom),
                "smaller" => geometry::text_smaller(zoom),
                _ => geometry::text_size_auto(f64::from(this.window.width())),
            });
        });
        self.actions.add_action(&text_size);

        // Edit ▸ Delete Page, after asking (TXT-02).
        let delete_page = gio::SimpleAction::new("delete-page", None);
        let weak = Rc::downgrade(self);
        delete_page.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.delete_page();
            }
        });
        self.actions.add_action(&delete_page);

        let inspector = gio::SimpleAction::new_stateful("inspector", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        inspector.connect_change_state(move |_, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            this.show_side("inspector", on);
        });
        self.actions.add_action(&inspector);

        let fonts = gio::SimpleAction::new("fonts", None);
        let weak = Rc::downgrade(self);
        fonts.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            let Some(editor) = this.editor() else { return };
            let cell = this.editor.clone();
            let editor_of: crate::formatting::EditorOf = Rc::new(move || cell.borrow().clone());
            // The family of a selected shape's text (OBJ-13), in its Text
            // tab, or of selected text, in the Style tab of the Format pane
            // (step 2 of window M+): the pane shows first, so that the
            // popover has its row to open on.
            let object = editor.has_selected_objects();
            this.reveal_format(if object { "text" } else { "style" });
            let button = if object {
                this.inspector.object_font_button()
            } else {
                this.inspector.text_font_button()
            };
            let weak = Rc::downgrade(&this);
            when_shown(button, move |button| {
                let Some(this) = weak.upgrade() else { return };
                let Some(editor) = this.editor() else { return };
                let window = this.fonts.present(button, editor_of);
                let format = editor.format();
                window.show(&format.character, &format.paragraph);
            });
        });
        self.actions.add_action(&fonts);
        let colors = gio::SimpleAction::new("colors", None);
        let weak = Rc::downgrade(self);
        colors.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            let object = this
                .editor()
                .is_some_and(|editor| editor.has_selected_objects());
            this.reveal_format(if object { "text" } else { "style" });
            when_shown(this.inspector.text_color_button(object), |button| {
                button.popup()
            });
        });
        self.actions.add_action(&colors);

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
            self.actions.add_action(&copy_style);
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
        self.actions.add_action(&paste_style);

        self.add_find_actions();
        self.add_object_actions();
        self.add_table_actions();
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
            self.actions.add_action(&action);
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
            self.actions.add_action(&action);
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
                if layout != format.layout || !editor.selection().is_empty() {
                    editor.set_columns("Columns", layout);
                }
                action.set_state(&i32::from(count).to_variant());
            }
        });
        self.actions.add_action(&columns);

        let auto = gio::SimpleAction::new("insert-auto-page-numbers", None);
        let weak = Rc::downgrade(self);
        auto.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade()
                && let Some(editor) = this.editor()
            {
                crate::auto_numbers::present(&this.window, &editor);
            }
        });
        self.actions.add_action(&auto);

        let linked = gio::SimpleAction::new("open-linked", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        linked.connect_activate(move |_, link| {
            if let (Some(this), Some(link)) = (weak.upgrade(), link.and_then(|l| l.str())) {
                this.open_linked(link);
            }
        });
        self.actions.add_action(&linked);

        let note = gio::SimpleAction::new("insert-note", None);
        let weak = Rc::downgrade(self);
        note.connect_activate(move |_, _| {
            if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                editor.insert_note();
            }
        });
        self.actions.add_action(&note);
        // From note to note, each of which the Inspector shows (EST-15).
        for (name, forward) in [("next-note", true), ("previous-note", false)] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    editor.go_to_note(forward);
                }
            });
            self.actions.add_action(&action);
        }

        self.add_comment_actions();
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
            self.actions.add_action(&action);
        }

        let import = gio::SimpleAction::new("import-styles", None);
        let weak = Rc::downgrade(self);
        import.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.import_styles();
            }
        });
        self.actions.add_action(&import);
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
        formatting::set_state(&self.actions, "inspector", on.to_variant());
    }

    /// Comment, on the selection or the word at the cursor, which the
    /// Comments page of the sidebar writes, and Next and Previous Comment
    /// (W149). Desk Slides has its comments with phase S2 (S158).
    fn add_comment_actions(self: &Rc<Self>) {
        if self.slides.is_some() {
            return;
        }
        let insert = gio::SimpleAction::new("insert-comment", None);
        let weak = Rc::downgrade(self);
        insert.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            let Some(range) = this.editor().and_then(|e| e.comment_target()) else {
                this.window.error_bell();
                return;
            };
            // A cell has one comment: Comment goes to it.
            if let Some(id) = this.editor().and_then(|e| e.target_comment(range)) {
                if this.nav_shows("comments") {
                    this.comments.show_current(Some(id));
                } else {
                    this.show_bubble(id);
                }
                return;
            }
            // In the Comments page when it shows, or else in a bubble at
            // the text, as Pages 15.4 writes it.
            if this.nav_shows("comments") {
                this.comments.start_comment(range);
            } else if let Some(at) = this.editor().and_then(|e| e.target_anchor(range)) {
                this.bubble.start_comment(range);
                this.show_bubble_at(at, None);
            } else {
                this.show_nav_page("comments");
                this.comments.start_comment(range);
            }
        });
        self.actions.add_action(&insert);
        for (name, forward) in [("next-comment", true), ("previous-comment", false)] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                let Some(this) = weak.upgrade() else { return };
                match this.editor().and_then(|e| e.go_to_comment(forward)) {
                    Some(id) if !this.nav_shows("comments") => this.show_bubble(id),
                    Some(_) => {}
                    None => this.window.error_bell(),
                }
            });
            self.actions.add_action(&action);
        }
    }

    /// Shows the bubble of a comment at its text (W149).
    fn show_bubble(&self, id: desk_model::NodeId) {
        let Some(at) = self.editor().and_then(|e| e.comment_anchor(id)) else {
            return;
        };
        self.bubble.show_comment(id);
        self.show_bubble_at(at, Some(id));
    }

    /// Points the bubble at a piece of text on a page and shows it: that
    /// of a comment, or of a new one.
    fn show_bubble_at(&self, at: crate::text_nav::SpanRect, id: Option<desk_model::NodeId>) {
        self.bubble_at.set(Some((id, at)));
        self.place_bubble();
        self.bubble.set_shown(true);
        self.bubble_popover.popup();
    }

    /// Points the bubble at its text again, after the pages moved.
    fn place_bubble(&self) {
        let Some((id, mut at)) = self.bubble_at.get() else {
            return;
        };
        if let Some(id) = id
            && let Some(now) = self.editor().and_then(|e| e.comment_anchor(id))
        {
            at = now;
            self.bubble_at.set(Some((Some(id), at)));
        }
        let (Some((x0, y0)), Some((x1, y1))) = (
            self.view.to_area(at.page, at.x0, at.y0),
            self.view.to_area(at.page, at.x1, at.y1),
        ) else {
            return;
        };
        let rect = gtk::gdk::Rectangle::new(
            x0 as i32,
            y0 as i32,
            ((x1 - x0) as i32).max(1),
            ((y1 - y0) as i32).max(1),
        );
        self.bubble_popover.set_pointing_to(Some(&rect));
    }

    /// Shows a page of the navigation sidebar, by name.
    fn show_nav_page(&self, page: &str) {
        let action = self
            .nav_pages()
            .iter()
            .find(|(p, _)| *p == page)
            .map(|(_, action)| *action);
        if let Some(action) = action {
            self.actions.change_action_state(action, &true.to_variant());
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
        self.actions.add_action(&number);
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
            let Some(locale) = this.editor().map(|editor| editor.locale()) else {
                return;
            };
            change(&this, &|field| match field {
                Field::DateTime { update, .. } => Field::DateTime {
                    pattern: pattern.into(),
                    shown: crate::fields::format_date(pattern, &locale)
                        .unwrap_or_default()
                        .into(),
                    update,
                },
                other => other,
            });
        });
        self.actions.add_action(&date);
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
            self.actions.add_action(&action);
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
            self.actions.add_action(&action);
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
        self.actions.add_action(&recent);
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
            self.actions.add_action(&action);
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
            self.actions.add_action(&action);
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
        self.actions.add_action(&correct);
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
            self.actions.add_action(&action);
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
        tools.append_submenu(Some(&tr("T_ransformations")), &transformations_menu());
        tools.append_submenu(Some(&tr("Su_bstitutions")), &substitutions_menu());
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
impl FramePage for DocumentWindow {
    fn actions(&self) -> gio::ActionGroup {
        self.actions.clone().upcast()
    }

    fn ask_close(self: Rc<Self>, done: Box<dyn FnOnce(bool)>) {
        self.ask_to_close(done);
    }

    fn closed(&self) {
        let _ = WINDOWS.try_with(|w| {
            if let Ok(mut w) = w.try_borrow_mut() {
                w.retain(|o| !std::ptr::eq(Rc::as_ptr(o), self));
            }
        });
    }
}

/// Runs `f` on `widget` once it shows: at once on an idle if it is mapped,
/// else when it maps, as a row of the Format pane does after the pane or
/// its tab opens. A popover can only open on a widget that shows.
fn when_shown<W: IsA<gtk::Widget> + Clone>(widget: &W, f: impl FnOnce(&W) + 'static) {
    let later = {
        let widget = widget.clone();
        move || {
            glib::idle_add_local_once(move || f(&widget));
        }
    };
    if widget.is_mapped() {
        later();
        return;
    }
    let later = std::cell::Cell::new(Some(later));
    let handler: Rc<std::cell::Cell<Option<glib::SignalHandlerId>>> = Rc::default();
    let id = {
        let handler = handler.clone();
        widget
            .upcast_ref::<gtk::Widget>()
            .connect_map(move |widget| {
                if let Some(id) = handler.take() {
                    widget.disconnect(id);
                }
                if let Some(later) = later.take() {
                    later();
                }
            })
    };
    handler.set(Some(id));
}

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
            let locale = editor.map(TextEditor::locale).unwrap_or_default();
            for pattern in crate::fields::DATE_FORMATS {
                let shown =
                    crate::fields::format_date(pattern, &locale).unwrap_or_else(|| pattern.into());
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

/// A new document from a template of the gallery, in the Default Font of
/// the preferences when it is set (PXV-03).
fn from_template(target: &str) -> Document {
    let mut document = match target {
        "layout" => Document::new_page_layout(),
        _ => Document::new_word_processing(),
    };
    let preferences = crate::preferences::get();
    if preferences.set_default_font {
        document.set_default_font(&preferences.default_font, preferences.default_font_size);
    }
    document
}

/// The Comments page of the sidebar of Desk Slides, whose comments come
/// with phase S2 of the roadmap (S158), so it says there are none.
fn comments_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name("desk-comment-symbolic")
        .title(tr("No Comments"))
        .description(tr("The comments of the document show here."))
        .css_classes(["compact"])
        .build()
}

/// The menu of the Add button of the navigation sidebar (GNM-12): a blank
/// page, which in a page layout document is a page of its own, and a
/// section break. The pages of the template join them with the templates
/// of the gallery (INT-06).
fn add_page_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("_Blank Page")), Some("win.insert-page-break"));
    menu.append(
        Some(&tr("_Section Break")),
        Some("win.insert-section-break"),
    );
    menu
}

/// The Format menu that Search Commands finds and the context menu
/// shows: fonts, colors and styles.
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

/// Select All and Deselect All, for the command search (VMI-01), the
/// transformations of the case (W054) and converting quotes (PXV-08).
pub(crate) fn edit_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("Select _All")), Some("win.select-all"));
    menu.append(Some(&tr("_Deselect All")), Some("win.deselect-all"));
    menu.append_section(None, &transformations_menu());
    menu.append_section(None, &substitutions_menu());
    menu
}

/// What Edit > Substitutions of Pages 15.4 does with the panel of macOS
/// for quotes already typed (PXV-08). GNOME has no such panel, so it is a
/// command: on the selection, or on the whole text when nothing is
/// selected.
fn substitutions_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("Convert _Quotes")), Some("win.convert-quotes"));
    menu
}

/// Edit > Transformations of Pages 15.4, which change the case of the
/// characters themselves (W054), in the context menu of the text and in
/// the command search.
fn transformations_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("Make _Upper Case")), Some("win.make-upper-case"));
    menu.append(Some(&tr("Make _Lower Case")), Some("win.make-lower-case"));
    menu.append(Some(&tr("_Capitalize")), Some("win.capitalize"));
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
