// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Desk Slides in the window of interface B (KINT-05 to KINT-13, KINT-15,
//! KGNM-01, KGNM-13 to KGNM-15).
//!
//! A presentation is a page layout document whose pages are its slides, so
//! the window of Desk Writer shows it with what Keynote adds: the slide
//! navigator, the outline and the master slides as pages of the
//! navigation sidebar; the presenter notes under the slide; the light
//! table in the place of the slide; the position as "Slide 3 of 12" with
//! the Notes button in the bottom bar; the Play button in the header bar;
//! the Slide tab of the Format pane when nothing is selected; and the Animation
//! level of the Inspector. Play, the master slides, transitions and builds
//! have their places here and work in later phases of Desk Slides.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use super::DocumentWindow;
use crate::editing::TextEditor;
use crate::formatting::{self, EditorOf};
use crate::i18n::{tr, trc};

/// The pages of the navigation sidebar of Desk Slides, and the actions
/// that show them (KGNM-13): the navigator takes the actions of the page
/// thumbnails of Desk Writer.
pub(super) const NAV_PAGES: [(&str, &str); 5] = [
    ("pages", "thumbnails"),
    ("topics", "slide-outline"),
    ("masters", "masters"),
    ("comments", "comments"),
    ("search", "search"),
];

/// The actions of Desk Slides that need a presentation.
pub(super) const ACTIONS: [&str; 10] = [
    "new-slide",
    "duplicate-slide",
    "delete-slide",
    "skip-slide",
    "presenter-notes",
    "light-table",
    "go-to-slide",
    "slide-outline",
    "masters",
    "edit-notes",
];

/// The width of the outline in the sidebar, in pixels.
pub(super) const TOPICS_WIDTH: f64 = 248.0;

/// The width of a slide on the light table, in pixels: the smallest, the
/// largest and the first (KINT-09).
const LIGHT_TABLE_SIZES: (f64, f64, f64) = (120.0, 360.0, 200.0);

/// How long the notes wait after the last key before they go into the
/// document, in milliseconds.
const NOTES_DELAY: u64 = 600;

/// The name of the level of the Inspector with the builds and the
/// transitions (KINT-14).
pub(super) const ANIMATION: &str = "animation";

/// What Desk Slides adds to the window.
pub(super) struct Slides {
    /// The presenter notes under the slide (KINT-05, KSLD-15).
    pub notes_revealer: gtk::Revealer,
    notes: gtk::TextView,
    /// The slide whose notes the field holds.
    notes_slide: Cell<Option<usize>>,
    /// Set while the field shows the notes of a slide, so that showing
    /// them does not change them.
    loading: Cell<bool>,
    /// Whether the field has changes that are not in the document yet.
    notes_due: Cell<bool>,
    /// The outline, a page of the navigation sidebar (KINT-08).
    pub topics: Rc<Topics>,
    /// The light table, in the place of the slide (KINT-09).
    pub light_table: Rc<LightTable>,
    /// The Slide group of the Format pane, with nothing selected
    /// (KINT-13).
    pub slide_group: crate::compact::Group,
    /// The size of the slides of the light table, in the bottom bar.
    pub size: gtk::Scale,
}

impl Slides {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let notes = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::WordChar)
            .top_margin(4)
            .bottom_margin(8)
            .left_margin(14)
            .right_margin(14)
            .accepts_tab(false)
            .build();
        formatting::labelled(&notes, &tr("Presenter Notes"));
        let scroller = gtk::ScrolledWindow::builder()
            .child(&notes)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(64)
            .max_content_height(160)
            .propagate_natural_height(true)
            .build();
        let title = gtk::Label::builder()
            .label(tr("Presenter Notes"))
            .xalign(0.0)
            .margin_start(14)
            .margin_top(6)
            .css_classes(["caption-heading", "dimmed"])
            .build();
        let column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["view"])
            .build();
        column.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        column.append(&title);
        column.append(&scroller);
        let notes_revealer = gtk::Revealer::builder()
            .child(&column)
            .reveal_child(true)
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .build();
        let size = gtk::Scale::with_range(
            gtk::Orientation::Horizontal,
            LIGHT_TABLE_SIZES.0,
            LIGHT_TABLE_SIZES.1,
            20.0,
        );
        size.set_value(LIGHT_TABLE_SIZES.2);
        size.set_width_request(140);
        size.set_draw_value(false);
        size.set_visible(false);
        size.set_tooltip_text(Some(&tr("Slide Size")));
        formatting::labelled(&size, &tr("Slide Size"));
        let slide_group = slide_group();
        let this = Rc::new(Self {
            notes_revealer,
            notes,
            notes_slide: Cell::new(None),
            loading: Cell::new(false),
            notes_due: Cell::new(false),
            topics: Topics::new(editor.clone()),
            light_table: LightTable::new(editor),
            slide_group,
            size,
        });
        let light_table = Rc::downgrade(&this.light_table);
        this.size.connect_value_changed(move |scale| {
            if let Some(table) = light_table.upgrade() {
                table.set_size(scale.value() as u32);
            }
        });
        this
    }
}

/// The page of the Masters of the navigation sidebar. The master slides
/// come with the themes, in phase 3 of Desk Slides, so it says so.
pub(super) fn masters_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name("desk-slide-layout-symbolic")
        .title(tr("No Master Slides"))
        .description(tr(
            "The master slides of the theme show here. Every slide is blank for now.",
        ))
        .css_classes(["compact"])
        .build()
}

/// The Animation level of the Inspector (KINT-14). The builds and the
/// transitions come in later phases of Desk Slides, so it says so.
pub(super) fn animation_page() -> gtk::ScrolledWindow {
    let page = adw::StatusPage::builder()
        .icon_name("desk-animate-symbolic")
        .title(tr("No Animations Yet"))
        .description(tr(
            "The transition of the slide and the builds of the selected object show here.",
        ))
        .css_classes(["compact"])
        .vexpand(true)
        .build();
    gtk::ScrolledWindow::builder()
        .child(&page)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build()
}

/// The menu of the Add button of the sidebar (KGNM-13): a new slide. The
/// master slides of the theme join it in phase 3.
pub(super) fn add_slide_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("_Blank Slide")), Some("win.new-slide"));
    menu
}

/// The menu of the Play button (KINT-12): it plays in phase 4, and
/// rehearses and shows the presenter display in phase 6, so its items are
/// off until then.
fn play_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let play = gio::Menu::new();
    play.append(Some(&tr("Play from _Start")), Some("win.play"));
    play.append(
        Some(&tr("Play from _This Slide")),
        Some("win.play-from-slide"),
    );
    menu.append_section(None, &play);
    let rehearse = gio::Menu::new();
    rehearse.append(Some(&tr("_Rehearse Slideshow")), Some("win.rehearse"));
    rehearse.append(Some(&tr("Re_cord Slideshow…")), Some("win.record"));
    menu.append_section(None, &rehearse);
    let screens = gio::Menu::new();
    screens.append(
        Some(&tr("_Presenter Display")),
        Some("win.presenter-display"),
    );
    screens.append(Some(&tr("Play in a _Window")), Some("win.play-in-window"));
    menu.append_section(None, &screens);
    menu
}

/// The Play button of the header bar, whose arrow opens the options of
/// the presentation (KINT-05, KINT-12).
pub(super) fn play_button() -> adw::SplitButton {
    let button = adw::SplitButton::builder()
        .icon_name("media-playback-start-symbolic")
        .action_name("win.play")
        .menu_model(&play_menu())
        .tooltip_text(tr("Play"))
        .dropdown_tooltip(tr("Slideshow Options"))
        .build();
    formatting::labelled(button.upcast_ref::<gtk::Widget>(), &tr("Play"));
    button
}

/// The View menu of Desk Slides, under the arrow of the Sidebar button
/// (KINT-05, KGNM-10): the pages of the sidebar, the light table, what the
/// window shows, and the zoom.
pub(super) fn view_menu() -> gio::Menu {
    let panes = gio::Menu::new();
    panes.append(Some(&tr("_Navigator")), Some("win.thumbnails"));
    panes.append(Some(&tr("_Outline")), Some("win.slide-outline"));
    panes.append(Some(&tr("_Master Slides")), Some("win.masters"));
    panes.append(Some(&tr("_Comments")), Some("win.comments"));
    panes.append(Some(&tr("Search _Results")), Some("win.search"));
    panes.append(Some(&tr("Spe_lling and Grammar")), Some("win.spelling"));
    let slides = gio::Menu::new();
    slides.append(Some(&tr("Light _Table")), Some("win.light-table"));
    slides.append(Some(&tr("Presenter _Notes")), Some("win.presenter-notes"));
    let document = gio::Menu::new();
    document.append(Some(&tr("_Ruler")), Some("win.ruler"));
    document.append(Some(&tr("_Invisibles")), Some("win.invisibles"));
    let window = gio::Menu::new();
    window.append(Some(&tr("_Fullscreen")), Some("win.fullscreen"));
    window.append_submenu(Some(&tr("_Zoom")), &crate::bottom_bar::zoom_menu());
    let menu = gio::Menu::new();
    menu.append_section(Some(&tr("Sidebar")), &panes);
    menu.append_section(None, &slides);
    menu.append_section(None, &document);
    menu.append_section(None, &window);
    menu
}

/// The Insert menu of Desk Slides (KGNM-10): a new slide, the fields that
/// a slide can show, links and special characters; with `objects`, the
/// objects too, as a window narrower than 720 px shows them.
pub(super) fn insert_menu(objects: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if objects {
        let items = gio::Menu::new();
        items.append(Some(&tr("_Table")), Some("win.insert-table"));
        items.append(Some(&tr("C_hart")), Some("win.insert-chart"));
        items.append(Some(&tr("Te_xt Box")), Some("win.insert-text-box"));
        items.append_submenu(Some(&tr("S_hape")), &super::header::shape_menu());
        items.append(
            Some(&tr("_Image, Movie or Sound…")),
            Some("win.insert-picture"),
        );
        items.append(Some(&tr("Co_mment")), Some("win.insert-comment"));
        menu.append_section(None, &items);
    }
    let slides = gio::Menu::new();
    slides.append(Some(&tr("_New Slide")), Some("win.new-slide"));
    menu.append_section(None, &slides);
    let fields = gio::Menu::new();
    fields.append(Some(&tr("Slide _Number")), Some("win.insert-page-number"));
    fields.append(Some(&tr("Slide Co_unt")), Some("win.insert-page-count"));
    fields.append(Some(&tr("_Date")), Some("win.insert-date"));
    fields.append(Some(&tr("_File Name")), Some("win.insert-file-name"));
    let inserts = gio::Menu::new();
    inserts.append_submenu(Some(&tr("_Field")), &fields);
    inserts.append(Some(&tr("_Hyperlink")), Some("win.insert-link"));
    menu.append_section(None, &inserts);
    let other = gio::Menu::new();
    other.append(
        Some(&tr("Special _Characters…")),
        Some("win.special-characters"),
    );
    other.append(
        Some(&tr("Add _Linked Text Box")),
        Some("win.add-linked-text-box"),
    );
    other.append(Some(&tr("C_hoose…")), Some("win.insert-picture"));
    menu.append_section(None, &other);
    menu
}

/// The menu of a slide: in the navigator, on the light table and under
/// Search Commands (KINT-07, KINT-09, KGNM-10).
pub(super) fn slide_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let add = gio::Menu::new();
    add.append(Some(&tr("_New Slide")), Some("win.new-slide"));
    add.append(Some(&tr("_Duplicate Slide")), Some("win.duplicate-slide"));
    add.append(Some(&tr("_Delete Slide")), Some("win.delete-slide"));
    menu.append_section(None, &add);
    let skip = gio::Menu::new();
    skip.append(Some(&tr("_Skip Slide")), Some("win.skip-slide"));
    menu.append_section(None, &skip);
    menu
}

/// The Slide group of the Format pane, with nothing selected (KINT-13):
/// its master slide, its background, its transition and Skip This Slide,
/// as the format bar of the slide had them in interface B. The master
/// slides come in phase 3, the background of a slide with the rest of
/// phase 1 and the transitions in phase 4, so their rows are off until
/// then; the Slide menu stays for Search Commands.
fn slide_group() -> crate::compact::Group {
    let group = crate::compact::Group::builder().title(tr("Slide")).build();
    for (title, value) in [
        (tr("Master Slide"), tr("Blank")),
        (tr("Slide Background"), tr("None")),
        (tr("Transition"), trc("transition", "None")),
    ] {
        let row = crate::compact::ActionRow::builder().title(title).build();
        row.add_suffix(
            &gtk::Label::builder()
                .label(value)
                .css_classes(["dimmed"])
                .build(),
        );
        row.set_sensitive(false);
        group.add(&row);
    }
    let skip = crate::compact::SwitchRow::builder()
        .title(tr("Skip This Slide"))
        .subtitle(tr("Leaves it out of the slideshow"))
        .build();
    skip.set_action_name(Some("win.skip-slide"));
    group.add(&skip);
    group
}

/// The outline of the presentation, a page of the navigation sidebar
/// (KINT-08): the title and the lines of each slide, from its text. A
/// click on a slide shows it. The text is edited on the slides until the
/// placeholders of the master slides come, in phase 3.
pub(super) struct Topics {
    root: gtk::ScrolledWindow,
    list: gtk::ListBox,
    editor: EditorOf,
    shown: Cell<bool>,
    pending: Cell<bool>,
}

impl Topics {
    fn new(editor: EditorOf) -> Rc<Self> {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        list.update_property(&[gtk::accessible::Property::Label(&trc("slides", "Outline"))]);
        let root = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        let this = Rc::new(Self {
            root,
            list,
            editor,
            shown: Cell::new(false),
            pending: Cell::new(false),
        });
        let weak = Rc::downgrade(&this);
        this.list.connect_row_activated(move |_, row| {
            if let Some(this) = weak.upgrade()
                && let Some(editor) = (this.editor)()
                && let Ok(slide) = usize::try_from(row.index())
            {
                editor.go_to_page(slide);
            }
        });
        this
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    /// Shows or hides it; it is made only while it shows.
    pub fn set_shown(self: &Rc<Self>, shown: bool) {
        self.shown.set(shown);
        if shown {
            self.refresh();
        }
    }

    /// Makes it again soon, after the document changed.
    pub fn refresh(self: &Rc<Self>) {
        if !self.shown.get() || self.pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            if let Some(this) = weak.upgrade() {
                this.pending.set(false);
                this.rebuild();
            }
        });
    }

    fn rebuild(&self) {
        self.list.remove_all();
        let Some(editor) = (self.editor)() else {
            return;
        };
        let document = editor.document();
        for (index, topics) in document.slide_topics().into_iter().enumerate() {
            let number = gtk::Label::builder()
                .label((index + 1).to_string())
                .valign(gtk::Align::Start)
                .width_chars(2)
                .xalign(1.0)
                .css_classes(["numeric", "dimmed"])
                .build();
            let skipped = editor.slide_skipped(index);
            let title = gtk::Label::builder()
                .label(if topics.title.is_empty() {
                    tr("Untitled Slide")
                } else {
                    topics.title.clone()
                })
                .xalign(0.0)
                .wrap(true)
                .css_classes(["heading"])
                .build();
            if topics.title.is_empty() || skipped {
                title.add_css_class("dimmed");
            }
            let column = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(2)
                .hexpand(true)
                .build();
            column.append(&title);
            for line in &topics.lines {
                column.append(
                    &gtk::Label::builder()
                        .label(format!("• {line}"))
                        .xalign(0.0)
                        .wrap(true)
                        .css_classes(["caption"])
                        .build(),
                );
            }
            let row = gtk::Box::builder()
                .spacing(8)
                .margin_top(4)
                .margin_bottom(4)
                .build();
            row.append(&number);
            row.append(&column);
            let name = tr("Slide {n}").replace("{n}", &(index + 1).to_string());
            let item = gtk::ListBoxRow::builder().child(&row).build();
            item.update_property(&[gtk::accessible::Property::Label(&name)]);
            self.list.append(&item);
        }
        let current = i32::try_from(editor.current_slide()).unwrap_or(0);
        if let Some(row) = self.list.row_at_index(current) {
            self.list.select_row(Some(&row));
        }
    }
}

/// What shows a slide, by index.
type OpenSlide = Box<dyn Fn(usize)>;

/// What a command of a slide does.
type SlideAction = fn(&Rc<DocumentWindow>, &TextEditor);

/// The light table: the slides bigger, in a grid, to see the whole
/// presentation and put its slides in order (KINT-09). A click chooses a
/// slide, a double click shows it, a drag moves it and Ctrl copies it; the
/// menu of a slide adds, duplicates, deletes and skips slides.
pub(super) struct LightTable {
    root: gtk::ScrolledWindow,
    flow: gtk::FlowBox,
    editor: EditorOf,
    size: Cell<u32>,
    shown: Cell<bool>,
    pending: Cell<bool>,
    /// The slide chosen.
    selected: Cell<usize>,
    /// What a double click does: show the slide.
    on_open: RefCell<Option<OpenSlide>>,
}

impl LightTable {
    fn new(editor: EditorOf) -> Rc<Self> {
        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .homogeneous(true)
            .max_children_per_line(20)
            .row_spacing(18)
            .column_spacing(18)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .valign(gtk::Align::Start)
            .activate_on_single_click(false)
            .build();
        flow.update_property(&[gtk::accessible::Property::Label(&tr("Light Table"))]);
        let root = gtk::ScrolledWindow::builder()
            .child(&flow)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .css_classes(["desk-document"])
            .build();
        let this = Rc::new(Self {
            root,
            flow,
            editor,
            size: Cell::new(LIGHT_TABLE_SIZES.2 as u32),
            shown: Cell::new(false),
            pending: Cell::new(false),
            selected: Cell::new(0),
            on_open: RefCell::new(None),
        });
        let weak = Rc::downgrade(&this);
        this.flow.connect_child_activated(move |_, child| {
            if let Some(this) = weak.upgrade()
                && let Ok(slide) = usize::try_from(child.index())
                && let Some(open) = this.on_open.borrow().as_ref()
            {
                open(slide);
            }
        });
        let weak = Rc::downgrade(&this);
        this.flow.connect_selected_children_changed(move |flow| {
            if let Some(this) = weak.upgrade()
                && let Some(child) = flow.selected_children().first()
                && let Ok(slide) = usize::try_from(child.index())
            {
                this.selected.set(slide);
            }
        });
        // The menu of a slide, with the right button.
        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        let weak = Rc::downgrade(&this);
        click.connect_pressed(move |gesture, _, x, y| {
            let Some(this) = weak.upgrade() else { return };
            if let Some(child) = this.flow.child_at_pos(x as i32, y as i32) {
                this.flow.select_child(&child);
            }
            let popover = gtk::PopoverMenu::from_model(Some(&slide_menu()));
            popover.set_parent(&this.flow);
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.set_has_arrow(false);
            popover.connect_closed(|popover| {
                let popover = popover.clone();
                glib::idle_add_local_once(move || popover.unparent());
            });
            popover.popup();
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        this.flow.add_controller(click);
        this
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    /// Calls `f` with the slide that a double click shows.
    pub fn connect_open(&self, f: impl Fn(usize) + 'static) {
        *self.on_open.borrow_mut() = Some(Box::new(f));
    }

    /// The slide chosen.
    pub fn selected(&self) -> usize {
        self.selected.get()
    }

    /// Chooses a slide.
    pub fn select(&self, slide: usize) {
        self.selected.set(slide);
        if let Some(child) = self
            .flow
            .child_at_index(i32::try_from(slide).unwrap_or(i32::MAX))
        {
            self.flow.select_child(&child);
            child.grab_focus();
        }
    }

    /// Whether it shows.
    pub fn is_shown(&self) -> bool {
        self.shown.get()
    }

    /// Shows or hides it; it draws only while it shows.
    pub fn set_shown(self: &Rc<Self>, shown: bool) {
        self.shown.set(shown);
        if shown {
            self.rebuild();
        }
    }

    /// The width of a slide, in pixels.
    fn set_size(self: &Rc<Self>, size: u32) {
        if self.size.replace(size) != size {
            self.refresh();
        }
    }

    /// Draws it again soon, after the document changed.
    pub fn refresh(self: &Rc<Self>) {
        if !self.shown.get() || self.pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(200), move || {
            if let Some(this) = weak.upgrade() {
                this.pending.set(false);
                this.rebuild();
            }
        });
    }

    fn rebuild(self: &Rc<Self>) {
        self.flow.remove_all();
        let Some(editor) = (self.editor)() else {
            return;
        };
        let pages = editor.pages();
        let mut painter = desk_render::Painter::new().ok();
        let size = self.size.get();
        for (index, page) in pages.iter().enumerate() {
            let picture = gtk::Picture::builder()
                .can_shrink(false)
                .css_classes(["card"])
                .halign(gtk::Align::Center)
                .build();
            if let Some(png) = painter
                .as_mut()
                .and_then(|p| p.thumbnail_png(page, size).ok())
                && let Ok(texture) = gdk::Texture::from_bytes(&glib::Bytes::from_owned(png))
            {
                picture.set_paintable(Some(&texture));
            }
            let skipped = editor.slide_skipped(index);
            if skipped {
                picture.set_opacity(0.4);
            }
            let number = (index + 1).to_string();
            let caption = gtk::Box::builder()
                .spacing(4)
                .halign(gtk::Align::Center)
                .build();
            caption.append(
                &gtk::Label::builder()
                    .label(&number)
                    .css_classes(["caption", "numeric"])
                    .build(),
            );
            if skipped {
                caption.append(
                    &gtk::Image::builder()
                        .icon_name("desk-skip-slide-symbolic")
                        .tooltip_text(tr("Skipped"))
                        .css_classes(["dimmed"])
                        .build(),
                );
            }
            let item = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .build();
            item.append(&picture);
            item.append(&caption);
            let name = slide_name(index, skipped);
            picture.set_alternative_text(Some(&name));
            let child = gtk::FlowBoxChild::builder().child(&item).build();
            child.update_property(&[gtk::accessible::Property::Label(&name)]);
            self.connect_drag(&child, index);
            self.flow.append(&child);
        }
        let selected = self.selected.get().min(pages.len().saturating_sub(1));
        self.selected.set(selected);
        if let Some(child) = self
            .flow
            .child_at_index(i32::try_from(selected).unwrap_or(0))
        {
            self.flow.select_child(&child);
        }
    }

    /// A drag of a slide moves it before or after the slide it drops on,
    /// by the half it drops on, or copies it there with Ctrl.
    fn connect_drag(self: &Rc<Self>, child: &gtk::FlowBoxChild, index: usize) {
        let drag = gtk::DragSource::builder()
            .actions(gdk::DragAction::MOVE | gdk::DragAction::COPY)
            .build();
        drag.connect_prepare(move |_, _, _| {
            Some(gdk::ContentProvider::for_value(
                &format!("desk-slide:{index}").to_value(),
            ))
        });
        child.add_controller(drag);
        let drop = gtk::DropTarget::new(
            glib::Type::STRING,
            gdk::DragAction::MOVE | gdk::DragAction::COPY,
        );
        let weak = Rc::downgrade(self);
        let target = child.downgrade();
        drop.connect_drop(move |drop, value, x, _| {
            let (Some(this), Some(target)) = (weak.upgrade(), target.upgrade()) else {
                return false;
            };
            let Some(from) = value
                .get::<String>()
                .ok()
                .and_then(|v| v.strip_prefix("desk-slide:")?.parse::<usize>().ok())
            else {
                return false;
            };
            let before = if x < f64::from(target.width()) / 2.0 {
                index
            } else {
                index + 1
            };
            let copy = drop
                .current_event_state()
                .contains(gdk::ModifierType::CONTROL_MASK);
            if let Some(editor) = (this.editor)() {
                editor.move_sections(&[from], before, copy);
                let moved = if copy || before <= from {
                    before
                } else {
                    before - 1
                };
                this.selected.set(moved);
            }
            true
        });
        child.add_controller(drop);
    }
}

/// The name of a slide for the screen reader.
fn slide_name(index: usize, skipped: bool) -> String {
    let name = tr("Slide {n}").replace("{n}", &(index + 1).to_string());
    if skipped {
        tr("{slide}, skipped").replace("{slide}", &name)
    } else {
        name
    }
}

impl DocumentWindow {
    /// What Desk Slides adds to the window, if this is its window.
    fn slides(&self) -> Option<&Rc<Slides>> {
        self.slides.as_ref()
    }

    /// The slide that the commands of a slide act on: the one chosen on
    /// the light table while it shows, or the one in sight.
    fn chosen_slide(&self, editor: &TextEditor) -> usize {
        match self.slides() {
            Some(slides) if slides.light_table.is_shown() => slides
                .light_table
                .selected()
                .min(editor.slide_count().saturating_sub(1)),
            _ => editor.current_slide(),
        }
    }

    /// Shows a new presentation, as a new window of Desk Slides does
    /// until the theme chooser comes, in phase 3 (KDOC-01).
    pub(super) fn show_new_presentation(self: &Rc<Self>) {
        self.load_untitled(
            desk_model::Document::new_presentation(),
            tr("Untitled Presentation"),
        );
    }

    /// The actions of Desk Slides (KINT-05 to KINT-13), and Ctrl+M,
    /// Ctrl+Shift+H and F5, approved on 14/11/2026.
    pub(super) fn add_slides_actions(self: &Rc<Self>) {
        let Some(slides) = self.slides().cloned() else {
            return;
        };
        let simple: [(&str, SlideAction); 3] = [
            ("new-slide", |this, editor| {
                if let Some(slides) = this.slides()
                    && slides.light_table.is_shown()
                {
                    let slide = this.chosen_slide(editor);
                    editor.go_to_page(slide);
                    editor.new_slide();
                    slides.light_table.selected.set(slide + 1);
                } else {
                    editor.new_slide();
                }
            }),
            ("duplicate-slide", |this, editor| {
                let slide = this.chosen_slide(editor);
                editor.duplicate_sections(&[slide]);
                if let Some(slides) = this.slides() {
                    slides.light_table.selected.set(slide + 1);
                }
            }),
            ("delete-slide", |this, editor| {
                let slide = this.chosen_slide(editor);
                editor.delete_page(slide);
            }),
        ];
        for (name, run) in simple {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(this) = weak.upgrade()
                    && let Some(editor) = this.editor()
                {
                    run(&this, &editor);
                }
            });
            self.actions.add_action(&action);
        }

        // Skip This Slide (KSLD-06), in the Slide tab of the Format pane
        // and the menu of a slide.
        let skip = gio::SimpleAction::new_stateful("skip-slide", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        skip.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            if let Some(editor) = this.editor() {
                let slide = this.chosen_slide(&editor);
                if editor.slide_skipped(slide) != on {
                    editor.set_slides_skipped(&[slide], on);
                }
            }
        });
        self.actions.add_action(&skip);

        // The presenter notes under the slide (KINT-05).
        let notes = gio::SimpleAction::new_stateful("presenter-notes", None, &true.to_variant());
        let revealer = slides.notes_revealer.downgrade();
        notes.connect_change_state(move |action, value| {
            let Some(on) = value.and_then(|v| v.get::<bool>()) else {
                return;
            };
            action.set_state(&on.to_variant());
            if let Some(revealer) = revealer.upgrade() {
                revealer.set_reveal_child(on);
            }
        });
        self.actions.add_action(&notes);
        // Puts the cursor in the notes, opening them.
        let edit_notes = gio::SimpleAction::new("edit-notes", None);
        let weak = Rc::downgrade(self);
        edit_notes.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade()
                && let Some(slides) = this.slides()
            {
                this.actions
                    .change_action_state("presenter-notes", &true.to_variant());
                slides.notes.grab_focus();
            }
        });
        self.actions.add_action(&edit_notes);

        // The light table, in the place of the slide (KINT-09).
        let light_table = gio::SimpleAction::new_stateful("light-table", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        light_table.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            this.show_light_table(on);
        });
        self.actions.add_action(&light_table);
        let weak = Rc::downgrade(self);
        slides.light_table.connect_open(move |slide| {
            if let Some(this) = weak.upgrade() {
                this.actions
                    .change_action_state("light-table", &false.to_variant());
                if let Some(editor) = this.editor() {
                    editor.go_to_page(slide);
                    editor.focus();
                }
            }
        });

        // Go To ▸ First and Last Slide (KINT-11).
        let go_to = gio::SimpleAction::new("go-to-slide", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        go_to.connect_activate(move |_, value| {
            let (Some(this), Some(value)) = (weak.upgrade(), value.and_then(|v| v.get::<String>()))
            else {
                return;
            };
            if let Some(editor) = this.editor() {
                let slide = match value.as_str() {
                    "first" => 0,
                    _ => editor.slide_count().saturating_sub(1),
                };
                if let Some(slides) = this.slides()
                    && slides.light_table.is_shown()
                {
                    slides.light_table.select(slide);
                } else {
                    editor.go_to_page(slide);
                }
            }
        });
        self.actions.add_action(&go_to);

        // The notes go into the document a moment after the last key, as
        // one action of the history for each slide.
        let weak = Rc::downgrade(self);
        slides.notes.buffer().connect_changed(move |_| {
            let Some(this) = weak.upgrade() else { return };
            let Some(slides) = this.slides() else { return };
            if slides.loading.get() || slides.notes_due.replace(true) {
                return;
            }
            let weak = Rc::downgrade(&this);
            glib::timeout_add_local_once(Duration::from_millis(NOTES_DELAY), move || {
                if let Some(this) = weak.upgrade() {
                    this.keep_notes();
                }
            });
        });
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        focus.connect_leave(move |_| {
            if let Some(this) = weak.upgrade() {
                this.keep_notes();
            }
        });
        slides.notes.add_controller(focus);
        // The notes and the position follow the slide in sight.
        let weak = Rc::downgrade(self);
        self.view.connect_moved(move || {
            if let Some(this) = weak.upgrade() {
                this.show_slide();
            }
        });
    }

    /// Puts the notes of the field into the document.
    fn keep_notes(&self) {
        let Some(slides) = self.slides() else { return };
        if !slides.notes_due.replace(false) {
            return;
        }
        let (Some(editor), Some(slide)) = (self.editor(), slides.notes_slide.get()) else {
            return;
        };
        let buffer = slides.notes.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        editor.set_slide_notes(slide, &text);
    }

    /// Shows the light table in the place of the slide, with the size of
    /// its slides in the place of the zoom, or the slide again.
    fn show_light_table(&self, on: bool) {
        let Some(slides) = self.slides() else { return };
        let Some(editor) = self.editor() else { return };
        if on {
            self.keep_notes();
            slides.light_table.selected.set(editor.current_slide());
            slides.light_table.set_shown(true);
            self.stack.set_visible_child_name("light-table");
            slides.light_table.select(editor.current_slide());
        } else {
            slides.light_table.set_shown(false);
            self.stack.set_visible_child_name("document");
        }
        slides.size.set_visible(on);
        self.bottom.zoom.set_visible(!on);
        slides.notes_revealer.set_visible(!on);
    }

    /// Shows what goes with the slide in sight: its notes, its position
    /// and whether it is skipped.
    pub(super) fn show_slide(&self) {
        let (Some(slides), Some(editor)) = (self.slides(), self.editor()) else {
            return;
        };
        let slide = editor.current_slide();
        if slides.notes_slide.get() != Some(slide) {
            self.keep_notes();
        }
        let notes = editor.slide_notes(slide);
        let buffer = slides.notes.buffer();
        let shown = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        // Notes being typed stay as they are until they go into the
        // document.
        if (slides.notes_slide.get() != Some(slide) || !slides.notes_due.get())
            && shown.as_str() != notes
        {
            slides.loading.set(true);
            buffer.set_text(&notes);
            slides.loading.set(false);
        }
        slides.notes_slide.set(Some(slide));
        let skipped = editor.slide_skipped(self.chosen_slide(&editor));
        formatting::set_state(&self.actions, "skip-slide", skipped.to_variant());
        self.bottom
            .show_slide_position(slide + 1, editor.slide_count().max(1));
    }

    /// Shows the Slide tab of the Format pane when nothing is selected
    /// (KINT-13).
    pub(super) fn show_slide_format(&self, editor: &TextEditor) {
        if self.slides.is_none() {
            return;
        }
        if editor.nothing_selected() {
            self.inspector
                .show_context(crate::inspector::Context::Slide);
        }
        self.show_slide();
    }

    /// Draws again what shows the slides, after the document changed.
    pub(super) fn slides_changed(&self) {
        let Some(slides) = self.slides() else { return };
        slides.topics.refresh();
        slides.light_table.refresh();
        self.show_slide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kgnm_13_the_sidebar_has_the_pages_of_slides() {
        let names: Vec<&str> = NAV_PAGES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["pages", "topics", "masters", "comments", "search"]);
        for (_, action) in NAV_PAGES {
            assert!(
                ["thumbnails", "comments", "search"].contains(&action) || ACTIONS.contains(&action),
                "{action}"
            );
        }
    }

    #[test]
    fn kint_09_a_slide_is_named_with_its_number() {
        assert_eq!(slide_name(2, false), "Slide 3");
        assert_eq!(slide_name(0, true), "Slide 1, skipped");
    }
}
