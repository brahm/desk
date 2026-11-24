// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A window of Desk Tables (NINT-02): the navigation sidebar as high as the
//! window at the start, with the tables and charts of the sheet shown
//! (NINT-03, NGNM-12, step 4 of window M+); the header bar; the formula
//! bar; the canvas of the sheet, with the zoom over it; the Inspector at
//! the end, whose Format pane holds what the format bar of interface B had
//! (step 2 of window M+); and the bottom bar, with the sheet switcher. Below 720 px the sidebar covers the canvas, and below 520
//! px the Inspector does too (NGNM-01).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::workbook::{FIRST_TABLE_ORIGIN, Grid, ItemKind, SheetError, SheetItem, Workbook};
use gtk::{gio, glib};

use super::bars::{BottomBar, FormulaBar};
use super::canvas::{self, Canvas, Selection};
use super::header::{self, Header};
use super::inspector::{self, Change, Inspector};
use crate::AppInfo;
use crate::formatting;
use crate::geometry;
use crate::i18n::{n_, tr};
use crate::nav_sidebar::{self, NavSidebar};

/// The width of the Inspector, in pixels, as in the mockups of interface B.
const INSPECTOR_WIDTH: f64 = 332.0;
/// The widths below which the window changes, in pixels (NGNM-01).
const OVERLAY_SIDEBAR: f64 = 720.0;
const OVERLAY_INSPECTOR: f64 = 520.0;
/// The width of the sidebar while it shows the search results.
const SEARCH_WIDTH: f64 = 260.0;
/// The width of the sidebar while it shows the table styles.
const STYLES_WIDTH: f64 = 220.0;

/// The pages of the navigation sidebar, the actions that show them, and
/// their titles and widths.
const NAV_PAGES: [(&str, &str, &str, f64); 5] = [
    (
        "sheets",
        "sheets",
        n_("Tables and Charts"),
        nav_sidebar::WIDTH,
    ),
    (
        "table-styles",
        "table-styles",
        n_("Table Styles"),
        STYLES_WIDTH,
    ),
    ("comments", "comments", n_("Comments"), nav_sidebar::WIDTH),
    ("search", "search", n_("Search Results"), SEARCH_WIDTH),
    ("formulas", "formula-list", n_("Formula List"), SEARCH_WIDTH),
];

/// The table styles of the Blank template of Numbers '09, in its order
/// (NTAB-30), which the sidebar lists.
const TABLE_STYLES: [&str; 10] = [
    n_("Basic"),
    n_("Basic (No Grid)"),
    n_("Gray"),
    n_("Gray Headers"),
    n_("Gray Fill"),
    n_("Beige"),
    n_("Ledger"),
    n_("Blue"),
    n_("Blue Headers"),
    n_("Blue Fill"),
];

/// The table that the Table button puts on a sheet: the first of the
/// tables of the Blank template, Headers, of 5 columns by 10 rows with a
/// header row and a header column (NTAB-05).
const NEW_TABLE: Grid = Grid {
    rows: 10,
    columns: 5,
    header_rows: 1,
    header_columns: 1,
    footer_rows: 0,
};

/// The space between a new table and what is above it, in points.
const NEW_TABLE_GAP: f64 = 36.0;

/// What an action of the window does.
type WindowAction = fn(&Rc<TablesWindow>);

/// The state of the document that undo brings back.
#[derive(Clone)]
struct Snapshot {
    /// The name of the change, for the menus.
    name: String,
    book: Workbook,
    current: usize,
    selection: Selection,
}

/// What the menu of the sidebar was opened on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MenuTarget {
    Sheet(usize),
    Item(usize),
}

/// A window and its document.
pub(super) struct TablesWindow {
    /// The window that holds the tab of the document (see tabs.rs).
    window: adw::ApplicationWindow,
    /// The window that holds the tab of the document.
    frame: RefCell<Rc<crate::tabs::Frame>>,
    /// The tab of the document, with its own breakpoints.
    root: adw::BreakpointBin,
    tab: std::cell::OnceCell<adw::TabPage>,
    /// The actions of the document (`win.`).
    actions: gio::SimpleActionGroup,
    info: AppInfo,
    title: adw::WindowTitle,
    book: RefCell<Workbook>,
    /// The sheet shown.
    current: Cell<usize>,
    selection: Cell<Selection>,
    undo: RefCell<Vec<Snapshot>>,
    redo: RefCell<Vec<Snapshot>>,
    /// Whether the document changed since it was made.
    modified: Cell<bool>,
    nav: Rc<NavSidebar>,
    nav_split: adw::OverlaySplitView,
    split: adw::OverlaySplitView,
    /// The sheets and the items of the sheet shown (NINT-03).
    sidebar: adw::Sidebar,
    items_section: adw::SidebarSection,
    menu_target: Cell<Option<MenuTarget>>,
    /// Whether the sidebar is being filled, when its selection changes
    /// without anyone choosing.
    filling: Cell<bool>,
    canvas: Rc<Canvas>,
    formula: FormulaBar,
    formula_revealer: gtk::Revealer,
    bottom: BottomBar,
    inspector: Rc<Inspector>,
    context_menu: gtk::PopoverMenu,
    /// The Undo and Redo of the context menu, named before it opens.
    history: gio::Menu,
}

thread_local! {
    /// The open windows, which live until they close.
    static WINDOWS: RefCell<Vec<Rc<TablesWindow>>> = const { RefCell::new(Vec::new()) };
    /// Whether the size of the screen was checked, which happens for the
    /// first window only (NGNM-14).
    static SCREEN_CHECKED: Cell<bool> = const { Cell::new(false) };
}

impl TablesWindow {
    /// The window that holds the tab of the document now.
    fn frame(&self) -> Rc<crate::tabs::Frame> {
        self.frame.borrow().clone()
    }

    /// A new window with a document of the Blank template.
    /// A new document in a tab of the window in front, or in a new window
    /// when the app has none.
    pub fn new(app: &adw::Application, info: AppInfo) -> Rc<Self> {
        let frame = crate::tabs::Frame::active(app).unwrap_or_else(|| Self::new_frame(app, info));
        Self::new_in(&frame, info)
    }

    /// A new document in a new window (New Window).
    pub fn new_window(app: &adw::Application, info: AppInfo) -> Rc<Self> {
        let frame = Self::new_frame(app, info);
        Self::new_in(&frame, info)
    }

    fn new_frame(app: &adw::Application, info: AppInfo) -> Rc<crate::tabs::Frame> {
        let frame = crate::tabs::Frame::new(app, info.name);
        let weak = Rc::downgrade(&frame);
        frame.set_new_tab(move || {
            let frame = weak.upgrade()?;
            Self::new_in(&frame, info).tab.get().cloned()
        });
        frame
    }

    /// A new document, from the Blank template, in a new tab of `frame`.
    fn new_in(frame: &Rc<crate::tabs::Frame>, info: AppInfo) -> Rc<Self> {
        Self::with_book(frame, info, Workbook::blank(&tr("Sheet"), &tr("Table")))
    }

    /// A new tab of `frame` with a document.
    fn with_book(frame: &Rc<crate::tabs::Frame>, info: AppInfo, book: Workbook) -> Rc<Self> {
        let Header {
            header,
            title,
            insert_button,
            objects,
        } = Header::new(info);
        let leave_fullscreen = gtk::Button::builder()
            .icon_name("view-restore-symbolic")
            .tooltip_text(tr("Leave Fullscreen"))
            .action_name("win.fullscreen")
            .visible(false)
            .build();
        formatting::labelled(&leave_fullscreen, &tr("Leave Fullscreen"));
        header.pack_end(&leave_fullscreen);
        let screen_banner = adw::Banner::builder()
            .title(tr(
                "The screen is smaller than 1280 × 720, so parts of the window might not fit",
            ))
            .button_label(tr("_Close"))
            .build();
        screen_banner.connect_button_clicked(|banner| banner.set_revealed(false));

        let formula = FormulaBar::new();
        let formula_revealer = gtk::Revealer::builder()
            .child(&formula.bar)
            .reveal_child(true)
            .build();
        let canvas = Canvas::new();
        let bottom = BottomBar::new();

        // The sheets, and the tables and charts of the sheet shown.
        let sidebar = adw::Sidebar::new();
        // In window M+ the sheets are in the switcher of the bottom bar,
        // and the sidebar lists the tables and charts of the sheet in sight
        // (NGNM-12, NINT-03).
        let items_section = adw::SidebarSection::new();
        items_section.set_menu_model(Some(&header::item_menu()));
        sidebar.append(items_section.clone());
        let nav = NavSidebar::new(
            &tr("Add Table or Chart"),
            header::add_item_menu().upcast_ref(),
        );
        for (name, _, title, width) in NAV_PAGES {
            let child: gtk::Widget = match name {
                "sheets" => sidebar.clone().upcast(),
                "table-styles" => table_styles_page().upcast(),
                "comments" => empty_page(
                    "desk-comment-symbolic",
                    &tr("No Comments"),
                    &tr("Comments on the cells and the objects of the sheets show here."),
                ),
                "search" => empty_page(
                    "edit-find-symbolic",
                    &tr("No Results"),
                    &tr("What a search finds in the sheets shows here."),
                ),
                _ => empty_page(
                    "desk-function-symbolic",
                    &tr("No Formulas"),
                    &tr("The formulas of the document show here, with their results."),
                ),
            };
            nav.add_page(name, &tr(title), width, &child);
        }

        let inspector_slot: Rc<RefCell<Option<std::rc::Weak<Self>>>> = Rc::new(RefCell::new(None));
        let inspector = {
            let slot = inspector_slot.clone();
            Inspector::new(move |change| {
                let this = slot.borrow().as_ref().and_then(std::rc::Weak::upgrade);
                if let Some(this) = this {
                    this.inspector_changed(change);
                }
            })
        };
        // The zoom floats over the sheet (NINT-04).
        let canvas_overlay = gtk::Overlay::builder().child(canvas.widget()).build();
        canvas_overlay.add_overlay(&bottom.zoom_pill);
        let split = adw::OverlaySplitView::builder()
            .content(&canvas_overlay)
            .sidebar(inspector.widget())
            .min_sidebar_width(INSPECTOR_WIDTH)
            .max_sidebar_width(INSPECTOR_WIDTH)
            .sidebar_position(gtk::PackType::End)
            .show_sidebar(false)
            .build();
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        // The tabs of the open documents, once two are open.
        toolbar_view.add_top_bar(
            &adw::TabBar::builder()
                .view(frame.view())
                .autohide(true)
                .build(),
        );
        toolbar_view.add_top_bar(&screen_banner);
        toolbar_view.add_top_bar(&formula_revealer);
        toolbar_view.set_content(Some(&split));
        toolbar_view.add_bottom_bar(&bottom.bar);
        let nav_split = adw::OverlaySplitView::builder()
            .content(&toolbar_view)
            .sidebar(nav.widget())
            .sidebar_position(gtk::PackType::Start)
            .min_sidebar_width(nav_sidebar::WIDTH)
            .max_sidebar_width(nav_sidebar::WIDTH)
            .show_sidebar(true)
            .build();
        let window = frame.window().clone();
        let root = adw::BreakpointBin::builder()
            .child(&nav_split)
            .width_request(360)
            .height_request(294)
            .build();

        // The breakpoints belong to the tab. Of the breakpoints that match,
        // the bin applies the one added
        // last, so each one sets what the wider ones set too (NGNM-01).
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
            if width <= OVERLAY_SIDEBAR {
                // The buttons of the objects go into the Insert menu.
                breakpoint.add_setter(&objects, "visible", Some(&false.to_value()));
                breakpoint.add_setter(
                    &insert_button,
                    "menu-model",
                    Some(&header::insert_menu(true).to_value()),
                );
            }
            breakpoints.push(breakpoint);
        }
        for breakpoint in breakpoints {
            root.add_breakpoint(breakpoint);
        }

        let history = gio::Menu::new();
        let context_menu = gtk::PopoverMenu::builder()
            .menu_model(&header::table_menu(&history))
            .has_arrow(false)
            .halign(gtk::Align::Start)
            .build();
        context_menu.set_parent(canvas.area());

        let this = Rc::new(Self {
            window,
            frame: RefCell::new(frame.clone()),
            root,
            tab: std::cell::OnceCell::new(),
            actions: gio::SimpleActionGroup::new(),
            info,
            title,
            book: RefCell::new(book),
            current: Cell::new(0),
            selection: Cell::new(Selection::None),
            undo: RefCell::new(Vec::new()),
            redo: RefCell::new(Vec::new()),
            modified: Cell::new(false),
            nav,
            nav_split,
            split,
            sidebar,
            items_section,
            menu_target: Cell::new(None),
            filling: Cell::new(false),
            canvas,
            formula,
            formula_revealer,
            bottom,
            inspector,
            context_menu,
            history,
        });
        *inspector_slot.borrow_mut() = Some(Rc::downgrade(&this));
        this.add_actions();
        this.connect_sidebar();
        this.connect_canvas();
        this.connect_fullscreen(&leave_fullscreen);
        warn_small_screen(&this.window, &screen_banner);
        this.refresh();
        // The first table of a new document has the focus, in its first
        // cell, ready to type in, as in Numbers.
        if this
            .book
            .borrow()
            .sheet(0)
            .is_some_and(|s| !s.items.is_empty())
        {
            this.select(Selection::Cells {
                item: 0,
                anchor: (0, 0),
                focus: (0, 0),
            });
        }
        this.canvas.focus();
        WINDOWS.with(|w| w.borrow_mut().push(this.clone()));
        let tab = frame.add(&this.root, this.clone());
        this.tab.set(tab).ok();
        this.refresh();
        this.canvas.focus();
        this
    }

    /// Brings the tab of the document and its window to the front.
    pub fn show(&self) {
        if let Some(tab) = self.tab.get() {
            self.frame().view().set_selected_page(tab);
        }
        self.window.present();
    }

    /// Shows the document, the sheet and the selection everywhere.
    fn refresh(&self) {
        let book = self.book.borrow();
        let current = self.current.get().min(book.len() - 1);
        self.current.set(current);
        let sheet = book.sheet(current).expect("the sheet shown exists");
        let selection = self.selection.get();
        let item = selection.item().and_then(|i| sheet.items.get(i));
        self.canvas.show(sheet, selection);
        self.fill_sidebar(&book, current);
        self.inspector.show(&book, sheet, item);
        self.bottom.show_count(selection.cells());
        // The cell in focus, or the table, in the formula bar.
        let reference = match (selection, item) {
            (Selection::Cells { .. }, Some(_)) => selection
                .range()
                .map(|(first, last)| canvas::range_name(first, last))
                .unwrap_or_default(),
            (_, Some(item)) => item.name.clone(),
            _ => String::new(),
        };
        self.formula.show_reference(&reference);
        let name = tr("Untitled");
        self.title.set_title(&name);
        self.title.set_subtitle(&if self.modified.get() {
            tr("Edited")
        } else {
            String::new()
        });
        if let Some(tab) = self.tab.get() {
            tab.set_title(&name);
            tab.set_tooltip(&name);
        }
        drop(book);
        self.update_actions();
    }

    /// Fills the sidebar: the sheets, then the items of the sheet shown
    /// under its name (NINT-03).
    fn fill_sidebar(&self, book: &Workbook, current: usize) {
        self.filling.set(true);
        let names: Vec<String> = book
            .sheets()
            .iter()
            .map(|sheet| sheet.name.clone())
            .collect();
        self.bottom.show_sheets(&names, current);
        self.items_section.remove_all();
        let sheet = book.sheet(current).expect("the sheet shown exists");
        self.items_section
            .set_title(Some(&tr("In “{sheet}”").replace("{sheet}", &sheet.name)));
        for item in &sheet.items {
            let icon = match item.kind {
                ItemKind::Table(_) => "desk-table-symbolic",
                ItemKind::Chart => "desk-chart-symbolic",
            };
            self.items_section.append(
                adw::SidebarItem::builder()
                    .title(&item.name)
                    .icon_name(icon)
                    .build(),
            );
        }
        // The item selected is lit.
        self.sidebar.set_selected(
            self.selection
                .get()
                .item()
                .map_or(gtk::INVALID_LIST_POSITION, |item| item as u32),
        );
        self.filling.set(false);
    }

    fn connect_sidebar(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.sidebar.connect_activated(move |_, index| {
            let Some(this) = weak.upgrade() else { return };
            if this.filling.get() {
                return;
            }
            // Choosing a table or a chart selects it and shows it.
            let item = index as usize;
            this.select(Selection::Item(item));
            this.canvas.scroll_to(item);
            this.canvas.focus();
        });
        // The switcher of the sheets shows the sheet chosen (NGNM-13).
        let weak = Rc::downgrade(self);
        self.bottom.sheets.connect_active_name_notify(move |group| {
            let Some(this) = weak.upgrade() else { return };
            if this.filling.get() {
                return;
            }
            if let Some(index) = group.active_name().and_then(|n| n.parse::<usize>().ok())
                && index != this.current.get()
            {
                this.show_sheet(index);
            }
        });
        // A secondary click on a sheet opens its menu (NINT-16).
        let menu = gtk::PopoverMenu::from_model(Some(&header::sheet_menu()));
        menu.set_parent(&self.bottom.sheets);
        menu.set_has_arrow(false);
        menu.set_position(gtk::PositionType::Top);
        let click = gtk::GestureClick::builder()
            .button(gtk::gdk::BUTTON_SECONDARY)
            .build();
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |gesture, _, x, y| {
            let Some(this) = weak.upgrade() else { return };
            let group = &this.bottom.sheets;
            // The toggle under the pointer is a child of the group, in the
            // order of the sheets, with a separator between two toggles.
            let mut picked = group.pick(x, y, gtk::PickFlags::DEFAULT);
            while let Some(widget) = picked.clone() {
                if widget.parent().as_ref() == Some(group.upcast_ref()) {
                    break;
                }
                picked = widget.parent();
            }
            let Some(picked) = picked else { return };
            let mut index = 0;
            let mut child = group.first_child();
            while let Some(widget) = child {
                if widget == picked {
                    break;
                }
                if !widget.is::<gtk::Separator>() && !widget.is::<gtk::Popover>() {
                    index += 1;
                }
                child = widget.next_sibling();
            }
            this.menu_target.set(Some(MenuTarget::Sheet(index)));
            menu.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            menu.popup();
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        self.bottom.sheets.add_controller(click);
        let weak = Rc::downgrade(self);
        self.sidebar.connect_setup_menu(move |_, item| {
            let Some(this) = weak.upgrade() else { return };
            let target = item.and_then(|item| {
                let position = |section: &adw::SidebarSection| {
                    (0..section.items().n_items()).find(|&i| section.item(i).as_ref() == Some(item))
                };
                position(&this.items_section).map(|i| MenuTarget::Item(i as usize))
            });
            this.menu_target.set(target);
        });
    }

    fn connect_canvas(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.canvas.connect_selected(move |selection| {
            if let Some(this) = weak.upgrade() {
                this.selection.set(selection);
                this.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        self.canvas.connect_context_menu(move |x, y| {
            if let Some(this) = weak.upgrade() {
                let name =
                    |stack: &RefCell<Vec<Snapshot>>| stack.borrow().last().map(|s| s.name.clone());
                header::fill_history(
                    &this.history,
                    name(&this.undo).as_deref(),
                    name(&this.redo).as_deref(),
                );
                this.context_menu
                    .set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                this.context_menu.popup();
            }
        });
    }

    /// Shows the sheet at `index`, with nothing selected on it.
    fn show_sheet(&self, index: usize) {
        if index >= self.book.borrow().len() {
            return;
        }
        self.current.set(index);
        self.selection.set(Selection::None);
        self.menu_target.set(None);
        self.refresh();
    }

    fn select(&self, selection: Selection) {
        self.selection.set(selection);
        self.refresh();
    }

    /// The sheet that a command of the sidebar's menu is for: the one it
    /// was opened on, or the one shown.
    fn target_sheet(&self) -> usize {
        match self.menu_target.take() {
            Some(MenuTarget::Sheet(index)) => index,
            _ => self.current.get(),
        }
    }

    /// The item that a command is for: the one the sidebar's menu was
    /// opened on, or the one selected.
    fn target_item(&self) -> Option<usize> {
        match self.menu_target.take() {
            Some(MenuTarget::Item(index)) => Some(index),
            _ => self.selection.get().item(),
        }
    }

    /// Changes the document as one step that undo takes back (NDOC-08).
    /// The change returns the sheet to show and the selection, or why it
    /// could not be made, which leaves the document as it was.
    fn change(
        &self,
        name: &str,
        f: impl FnOnce(&mut Workbook, usize, Selection) -> Result<(usize, Selection), SheetError>,
    ) -> bool {
        let before = self.snapshot(name);
        let mut book = self.book.borrow().clone();
        match f(&mut book, self.current.get(), self.selection.get()) {
            Ok((current, selection)) => {
                *self.book.borrow_mut() = book;
                self.current.set(current);
                self.selection.set(selection);
                self.undo.borrow_mut().push(before);
                self.redo.borrow_mut().clear();
                self.modified.set(true);
                self.refresh();
                true
            }
            Err(_) => {
                self.window.error_bell();
                false
            }
        }
    }

    fn snapshot(&self, name: &str) -> Snapshot {
        Snapshot {
            name: name.to_owned(),
            book: self.book.borrow().clone(),
            current: self.current.get(),
            selection: self.selection.get(),
        }
    }

    fn restore(&self, snapshot: Snapshot) {
        *self.book.borrow_mut() = snapshot.book;
        self.current.set(snapshot.current);
        self.selection.set(snapshot.selection);
        self.modified.set(true);
        self.refresh();
    }

    fn undo(&self) {
        let Some(snapshot) = self.undo.borrow_mut().pop() else {
            return;
        };
        let now = self.snapshot(&snapshot.name);
        self.redo.borrow_mut().push(now);
        self.restore(snapshot);
    }

    fn redo(&self) {
        let Some(snapshot) = self.redo.borrow_mut().pop() else {
            return;
        };
        let now = self.snapshot(&snapshot.name);
        self.undo.borrow_mut().push(now);
        self.restore(snapshot);
    }

    /// Applies what the Inspector asks.
    fn inspector_changed(&self, change: Change) {
        match change {
            Change::SheetName(name) => {
                let index = self.current.get();
                self.rename_sheet(index, &name);
            }
            Change::ItemName(name) => {
                self.change(&tr("Rename Table"), |book, current, selection| {
                    let item = selection.item().ok_or(SheetError::OutOfRange)?;
                    book.sheet_mut(current)
                        .ok_or(SheetError::OutOfRange)?
                        .rename_item(item, &name)?;
                    Ok((current, selection))
                });
            }
            Change::NameShown(shown) => self.set_name_shown(shown),
            Change::Grid(grid) => {
                self.change(&tr("Change Table"), |book, current, selection| {
                    let item = item_mut(book, current, selection.item())?;
                    let ItemKind::Table(old) = &mut item.kind else {
                        return Err(SheetError::OutOfRange);
                    };
                    if *old == grid {
                        return Err(SheetError::OutOfRange);
                    }
                    *old = fitted(grid);
                    let grid = *old;
                    Ok((current, clamped(selection, &grid)))
                });
            }
        }
    }

    fn rename_sheet(&self, index: usize, name: &str) {
        if self
            .book
            .borrow()
            .sheet(index)
            .is_some_and(|s| s.name == name.trim())
        {
            return;
        }
        let selection = self.selection.get();
        self.change(&tr("Rename Sheet"), |book, current, _| {
            book.rename_sheet(index, name)?;
            Ok((current, selection))
        });
    }

    fn set_name_shown(&self, shown: bool) {
        self.change(&tr("Show Table Name"), |book, current, selection| {
            let item = item_mut(book, current, selection.item())?;
            if item.name_shown == shown {
                return Err(SheetError::OutOfRange);
            }
            item.name_shown = shown;
            Ok((current, selection))
        });
    }

    /// Asks for a new name of a sheet, in a dialog (NTAB-04).
    fn ask_sheet_name(self: &Rc<Self>, index: usize) {
        let Some(name) = self.book.borrow().sheet(index).map(|s| s.name.clone()) else {
            return;
        };
        let dialog = adw::AlertDialog::new(Some(&tr("Rename Sheet")), None);
        let entry = gtk::Entry::builder()
            .text(&name)
            .activates_default(true)
            .build();
        formatting::labelled(&entry, &tr("Name"));
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("rename", &tr("_Rename"));
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        let field = entry.clone();
        dialog.choose(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |response| {
                if let (Some(this), "rename") = (weak.upgrade(), response.as_str()) {
                    this.rename_sheet(index, &field.text());
                }
            },
        );
        entry.grab_focus();
    }

    /// Adds a row or a column next to the cell in focus (NTAB-06): Alt and
    /// the arrows, as decided for Desk Tables.
    fn add_line(&self, row: bool, after: bool) {
        let name = match (row, after) {
            (true, false) => tr("Add Row Above"),
            (true, true) => tr("Add Row Below"),
            (false, false) => tr("Add Column Before"),
            (false, true) => tr("Add Column After"),
        };
        self.change(&name, |book, current, selection| {
            let Selection::Cells {
                item,
                anchor,
                focus,
            } = selection
            else {
                return Err(SheetError::OutOfRange);
            };
            let entry = item_mut(book, current, Some(item))?;
            let ItemKind::Table(grid) = &mut entry.kind else {
                return Err(SheetError::OutOfRange);
            };
            let shift = |(r, c): (u32, u32)| if row { (r + 1, c) } else { (r, c + 1) };
            if row {
                grid.rows += 1;
            } else {
                grid.columns += 1;
            }
            // The cells move with what was in them: a line added before the
            // cell in focus pushes it on.
            let selection = if after {
                selection
            } else {
                Selection::Cells {
                    item,
                    anchor: shift(anchor),
                    focus: shift(focus),
                }
            };
            Ok((current, selection))
        });
    }

    /// Puts a new table on the sheet shown, under what is there, and
    /// selects its first cell (NTAB-05).
    fn insert_table(&self) {
        let stem = tr("Table");
        self.change(&tr("Insert Table"), |book, current, _| {
            let sheet = book.sheet_mut(current).ok_or(SheetError::OutOfRange)?;
            let name = (1..)
                .map(|n| format!("{stem} {n}"))
                .find(|name| {
                    !sheet
                        .items
                        .iter()
                        .any(|i| i.name.to_lowercase() == name.to_lowercase())
                })
                .expect("some number is free");
            let top = if sheet.items.is_empty() {
                FIRST_TABLE_ORIGIN.1
            } else {
                sheet.extent().1 + NEW_TABLE_GAP
            };
            sheet.items.push(SheetItem::table(
                &name,
                NEW_TABLE,
                (FIRST_TABLE_ORIGIN.0, top),
            ));
            let item = sheet.items.len() - 1;
            Ok((
                current,
                Selection::Cells {
                    item,
                    anchor: (0, 0),
                    focus: (0, 0),
                },
            ))
        });
        if let Some(item) = self.selection.get().item() {
            self.canvas.scroll_to(item);
        }
        self.canvas.focus();
    }

    /// Sets which actions can run, and the states that follow the
    /// selection.
    fn update_actions(&self) {
        let book = self.book.borrow();
        let current = self.current.get();
        let selection = self.selection.get();
        let item = book
            .sheet(current)
            .and_then(|s| selection.item().and_then(|i| s.items.get(i)));
        let cells = matches!(selection, Selection::Cells { .. });
        let table = item.is_some_and(SheetItem::is_table);
        for (name, enabled) in [
            ("undo", !self.undo.borrow().is_empty()),
            ("redo", !self.redo.borrow().is_empty()),
            ("delete-sheet", book.len() > 1),
            ("previous-sheet", current > 0),
            ("next-sheet", current + 1 < book.len()),
            ("add-row-above", cells),
            ("add-row-below", cells),
            ("add-column-before", cells),
            ("add-column-after", cells),
            ("show-table-name", table),
            ("deselect-all", selection != Selection::None),
        ] {
            if let Some(action) = self
                .actions
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(enabled);
            }
        }
        formatting::set_state(
            &self.actions,
            "show-table-name",
            item.is_some_and(|i| i.name_shown).to_variant(),
        );
    }

    fn add_actions(self: &Rc<Self>) {
        let simple: [(&str, WindowAction); 17] = [
            ("undo", |this| this.undo()),
            ("redo", |this| this.redo()),
            ("add-sheet", |this| {
                let (sheet, table) = (tr("Sheet"), tr("Table"));
                this.change(&tr("Add Sheet"), |book, current, _| {
                    let index = book.add_sheet(Some(current), &sheet, &table);
                    Ok((index, Selection::None))
                });
            }),
            ("duplicate-sheet", |this| {
                let index = this.target_sheet();
                let copy = tr("(copy)");
                this.change(&tr("Duplicate Sheet"), |book, _, _| {
                    let index = book.duplicate_sheet(index, &copy)?;
                    Ok((index, Selection::None))
                });
            }),
            ("delete-sheet", |this| {
                let index = this.target_sheet();
                this.change(&tr("Delete Sheet"), |book, current, selection| {
                    book.delete_sheet(index)?;
                    if index == current {
                        Ok((index.min(book.len() - 1), Selection::None))
                    } else {
                        Ok((current - usize::from(index < current), selection))
                    }
                });
            }),
            ("rename-sheet", |this| {
                let index = this.target_sheet();
                this.ask_sheet_name(index);
            }),
            ("delete-item", |this| {
                let Some(index) = this.target_item() else {
                    return;
                };
                this.change(&tr("Delete"), |book, current, _| {
                    let sheet = book.sheet_mut(current).ok_or(SheetError::OutOfRange)?;
                    if index >= sheet.items.len() {
                        return Err(SheetError::OutOfRange);
                    }
                    sheet.items.remove(index);
                    Ok((current, Selection::None))
                });
            }),
            ("insert-table", |this| this.insert_table()),
            ("add-row-above", |this| this.add_line(true, false)),
            ("add-row-below", |this| this.add_line(true, true)),
            ("add-column-before", |this| this.add_line(false, false)),
            ("add-column-after", |this| this.add_line(false, true)),
            ("previous-sheet", |this| {
                let current = this.current.get();
                if current > 0 {
                    this.show_sheet(current - 1);
                }
            }),
            ("next-sheet", |this| this.show_sheet(this.current.get() + 1)),
            ("deselect-all", |this| this.select(Selection::None)),
            ("show-in-inspector", |this| {
                this.split.set_show_sidebar(true);
                this.inspector.show_level(inspector::FORMAT);
            }),
            ("duplicate", |this| {
                // A copy without a title, opened beside the original
                // (NDOC-11).
                let copy = Self::with_book(&this.frame(), this.info, this.book.borrow().clone());
                copy.show();
            }),
        ];
        for (name, f) in simple {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(this) = weak.upgrade() {
                    f(&this);
                }
            });
            self.actions.add_action(&action);
        }

        // Moves a sheet up or down in the list (NTAB-03).
        let move_sheet = gio::SimpleAction::new("move-sheet", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        move_sheet.connect_activate(move |_, value| {
            let (Some(this), Some(way)) = (weak.upgrade(), value.and_then(|v| v.get::<String>()))
            else {
                return;
            };
            let index = this.target_sheet();
            let to = if way == "up" {
                index.checked_sub(1)
            } else {
                Some(index + 1)
            };
            let Some(to) = to else {
                this.window.error_bell();
                return;
            };
            this.change(&tr("Move Sheet"), |book, current, selection| {
                book.move_sheet(index, to)?;
                // The sheet shown stays shown, wherever it went.
                let current = if current == index {
                    to
                } else if current == to {
                    index
                } else {
                    current
                };
                Ok((current, selection))
            });
        });
        self.actions.add_action(&move_sheet);

        // Moves a table or a chart in the list, or to another sheet,
        // without moving it on the sheet (NTAB-03).
        let move_item = gio::SimpleAction::new("move-item", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        move_item.connect_activate(move |_, value| {
            let (Some(this), Some(way)) = (weak.upgrade(), value.and_then(|v| v.get::<String>()))
            else {
                return;
            };
            let Some(index) = this.target_item() else {
                return;
            };
            this.change(&tr("Move"), |book, current, _| match way.as_str() {
                "up" | "down" => {
                    let to = if way == "up" {
                        index.checked_sub(1).ok_or(SheetError::OutOfRange)?
                    } else {
                        index + 1
                    };
                    book.sheet_mut(current)
                        .ok_or(SheetError::OutOfRange)?
                        .move_item(index, to)?;
                    Ok((current, Selection::Item(to)))
                }
                _ => {
                    let to = if way == "previous-sheet" {
                        current.checked_sub(1).ok_or(SheetError::OutOfRange)?
                    } else {
                        current + 1
                    };
                    let moved = book.move_item_to_sheet(current, index, to)?;
                    Ok((to, Selection::Item(moved)))
                }
            });
        });
        self.actions.add_action(&move_item);

        // Show Table Name, over the table (NTAB-09).
        let show_name =
            gio::SimpleAction::new_stateful("show-table-name", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        show_name.connect_change_state(move |_, value| {
            if let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>())) {
                this.set_name_shown(on);
            }
        });
        self.actions.add_action(&show_name);

        // The zoom (NINT-04).
        let zoom = gio::SimpleAction::new("zoom", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        zoom.connect_activate(move |_, value| {
            let (Some(this), Some(value)) = (weak.upgrade(), value.and_then(|v| v.get::<String>()))
            else {
                return;
            };
            let level = match value.as_str() {
                // Zoom to Selection (B-027).
                "selection" => {
                    match this.canvas.zoom_to_selection() {
                        Some(zoom) => {
                            crate::document_controls::show_zoom(&this.bottom.zoom, zoom);
                        }
                        None => this.window.error_bell(),
                    }
                    return;
                }
                "in" => geometry::zoom_in(this.canvas.zoom()),
                "out" => geometry::zoom_out(this.canvas.zoom()),
                level => level.parse().unwrap_or(geometry::DEFAULT_ZOOM),
            };
            this.canvas.set_zoom(level);
            crate::document_controls::show_zoom(&this.bottom.zoom, level);
        });
        self.actions.add_action(&zoom);

        // The bars and the panes the window shows.
        for (name, initial) in [
            ("sidebar", true),
            ("inspector", false),
            ("formula-bar", true),
        ] {
            let action = gio::SimpleAction::new_stateful(name, None, &initial.to_variant());
            let weak = Rc::downgrade(self);
            action.connect_change_state(move |action, value| {
                let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
                else {
                    return;
                };
                action.set_state(&on.to_variant());
                match action.name().as_str() {
                    "sidebar" => this.nav_split.set_show_sidebar(on),
                    "inspector" => this.split.set_show_sidebar(on),
                    _ => this.formula_revealer.set_reveal_child(on),
                }
            });
            self.actions.add_action(&action);
        }
        // The panes follow the split views, which a narrow window closes.
        let weak = Rc::downgrade(self);
        self.nav_split.connect_show_sidebar_notify(move |split| {
            if let Some(this) = weak.upgrade() {
                formatting::set_state(&this.actions, "sidebar", split.shows_sidebar().to_variant());
                this.sync_nav();
            }
        });
        let weak = Rc::downgrade(self);
        self.split.connect_show_sidebar_notify(move |split| {
            if let Some(this) = weak.upgrade() {
                formatting::set_state(
                    &this.actions,
                    "inspector",
                    split.shows_sidebar().to_variant(),
                );
            }
        });

        // The pages of the sidebar: the sheets, the table styles, the
        // comments, the search results and the formula list.
        for (page, name, _, _) in NAV_PAGES {
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

        let shortcuts = gio::SimpleAction::new("shortcuts", None);
        let window = self.window.downgrade();
        shortcuts.connect_activate(move |_, _| {
            if let Some(window) = window.upgrade() {
                super::shortcuts::dialog().present(Some(&window));
            }
        });
        self.actions.add_action(&shortcuts);

        let search = gio::SimpleAction::new("command-search", None);
        let weak = Rc::downgrade(self);
        search.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            let menus: Vec<(String, gio::MenuModel)> = [
                (tr("Main Menu"), header::main_menu(this.info)),
                (tr("Main Menu"), header::more_commands()),
                (tr("View"), header::view_menu()),
                (tr("Add Sheet"), header::add_sheet_menu()),
                (tr("Insert"), header::insert_menu(true)),
                (tr("Sheet"), header::sheet_menu()),
                (tr("Table"), header::table_menu(&gio::Menu::new())),
            ]
            .into_iter()
            .map(|(name, menu)| (name, menu.upcast()))
            .collect();
            crate::window::command_search::present(&this.window, this.actions.upcast_ref(), &menus);
        });
        self.actions.add_action(&search);
        self.sync_nav();
    }

    /// Sets the states of the actions of the sidebar pages, and the width
    /// of the sidebar, from the page it shows.
    fn sync_nav(&self) {
        let shown = self.nav_split.shows_sidebar();
        let page = self.nav.page();
        for (name, action, _, _) in NAV_PAGES {
            formatting::set_state(&self.actions, action, (shown && page == name).to_variant());
        }
        let width = self.nav.width(&page);
        self.nav_split.set_min_sidebar_width(width);
        self.nav_split.set_max_sidebar_width(width);
    }

    /// Full screen (NINT-06): F11 goes in and out, and so does Escape out
    /// of it. The sidebar and the Inspector hide, and come back after.
    fn connect_fullscreen(self: &Rc<Self>, leave: &gtk::Button) {
        // Ctrl+W closes the tab of the document.
        let close = gio::SimpleAction::new("close", None);
        let weak = Rc::downgrade(self);
        close.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.frame().close(&this.root);
            }
        });
        self.actions.add_action(&close);
        let overview = gio::SimpleAction::new("tab-overview", None);
        let weak = Rc::downgrade(self);
        overview.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.frame().show_overview();
            }
        });
        self.actions.add_action(&overview);
        let action = gio::SimpleAction::new_stateful("fullscreen", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        action.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            this.window.set_fullscreened(on);
        });
        self.actions.add_action(&action);
        let leave = leave.downgrade();
        let weak = Rc::downgrade(self);
        let before = Rc::new(Cell::new(false));
        self.window.connect_fullscreened_notify(move |window| {
            let Some(this) = weak.upgrade() else { return };
            let on = window.is_fullscreen();
            formatting::set_state(window, "fullscreen", on.to_variant());
            if let Some(leave) = leave.upgrade() {
                leave.set_visible(on);
            }
            if on {
                before.set(this.nav_split.shows_sidebar());
                this.nav_split.set_show_sidebar(false);
            } else if before.get() {
                this.nav_split.set_show_sidebar(true);
            }
        });
        let escape = gtk::ShortcutController::new();
        escape.set_scope(gtk::ShortcutScope::Managed);
        let weak = self.window.downgrade();
        escape.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("Escape"),
            Some(gtk::CallbackAction::new(move |_, _| {
                match weak
                    .upgrade()
                    .filter(gtk::prelude::GtkWindowExt::is_fullscreen)
                {
                    Some(window) => {
                        window.unfullscreen();
                        glib::Propagation::Stop
                    }
                    None => glib::Propagation::Proceed,
                }
            })),
        ));
        self.window.add_controller(escape);
    }
}

/// The item of the sheet `current` at `index`, to change.
fn item_mut(
    book: &mut Workbook,
    current: usize,
    index: Option<usize>,
) -> Result<&mut SheetItem, SheetError> {
    book.sheet_mut(current)
        .and_then(|sheet| sheet.items.get_mut(index?))
        .ok_or(SheetError::OutOfRange)
}

/// A grid whose headers and footers fit in it, with a body row and a body
/// column left.
fn fitted(mut grid: Grid) -> Grid {
    grid.rows = grid.rows.max(1);
    grid.columns = grid.columns.max(1);
    grid.header_columns = grid.header_columns.min(grid.columns - 1).min(5);
    let rows = grid.rows - 1;
    grid.header_rows = grid.header_rows.min(rows).min(5);
    grid.footer_rows = grid.footer_rows.min(rows - grid.header_rows).min(5);
    grid
}

/// A selection of cells that stays inside a grid that shrank.
fn clamped(selection: Selection, grid: &Grid) -> Selection {
    let clamp = |(row, column): (u32, u32)| (row.min(grid.rows - 1), column.min(grid.columns - 1));
    match selection {
        Selection::Cells {
            item,
            anchor,
            focus,
        } => Selection::Cells {
            item,
            anchor: clamp(anchor),
            focus: clamp(focus),
        },
        other => other,
    }
}

/// A page of the sidebar with nothing in it yet.
fn empty_page(icon: &str, title: &str, description: &str) -> gtk::Widget {
    adw::StatusPage::builder()
        .icon_name(icon)
        .title(title)
        .description(description)
        .css_classes(["compact"])
        .build()
        .upcast()
}

/// The table styles of the document, as the Styles pane of Numbers listed
/// them (NTAB-30).
fn table_styles_page() -> gtk::ScrolledWindow {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .build();
    formatting::labelled(&list, &tr("Table Styles"));
    for style in TABLE_STYLES {
        let row = adw::ActionRow::builder()
            .title(tr(style))
            .sensitive(false)
            .build();
        list.append(&row);
    }
    gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build()
}

/// Says once, over the first window, that the screen is smaller than Desk
/// asks for (NGNM-14).
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

impl crate::tabs::FramePage for TablesWindow {
    fn actions(&self) -> gio::ActionGroup {
        self.actions.clone().upcast()
    }

    fn moved_to(&self, frame: &Rc<crate::tabs::Frame>) {
        *self.frame.borrow_mut() = frame.clone();
    }

    /// Desk Tables does not save documents yet, so a tab closes at once.
    fn ask_close(self: Rc<Self>, done: Box<dyn FnOnce(bool)>) {
        done(true);
    }

    fn closed(&self) {
        let _ = WINDOWS.try_with(|w| {
            if let Ok(mut w) = w.try_borrow_mut() {
                w.retain(|o| !std::ptr::eq(Rc::as_ptr(o), self));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntab_06_headers_and_footers_fit_in_the_table() {
        let grid = fitted(Grid {
            rows: 3,
            columns: 2,
            header_rows: 5,
            header_columns: 5,
            footer_rows: 5,
        });
        assert_eq!((grid.header_rows, grid.footer_rows), (2, 0));
        assert_eq!(grid.header_columns, 1);
        let grid = fitted(Grid {
            rows: 0,
            columns: 0,
            ..Grid::BLANK
        });
        assert_eq!((grid.rows, grid.columns), (1, 1));
        assert_eq!((grid.header_rows, grid.header_columns), (0, 0));
    }

    #[test]
    fn ntab_06_the_selection_stays_in_a_table_that_shrinks() {
        let selection = Selection::Cells {
            item: 0,
            anchor: (9, 4),
            focus: (2, 2),
        };
        let grid = Grid {
            rows: 5,
            columns: 3,
            ..NEW_TABLE
        };
        assert_eq!(
            clamped(selection, &grid),
            Selection::Cells {
                item: 0,
                anchor: (4, 2),
                focus: (2, 2)
            }
        );
    }

    #[test]
    fn ntab_05_the_new_table_is_the_first_of_the_template() {
        assert_eq!((NEW_TABLE.columns, NEW_TABLE.rows), (5, 10));
        assert_eq!((NEW_TABLE.header_rows, NEW_TABLE.header_columns), (1, 1));
        assert_eq!(TABLE_STYLES.len(), 10);
    }
}
