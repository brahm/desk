// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Inspector of Desk Tables (NINT-09): one utility pane at the end of
//! the window, with three levels. Format has the tabs of what is selected,
//! with a title that says what it is; Data has what Numbers called
//! Reorganize (sort, filter and categories); Document has the sheet and
//! the information of the document, as decided on 2026-11-14.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::workbook::{Grid, ItemKind, Sheet, SheetItem, Workbook};

use crate::i18n::{n_, tr};

/// The name of the level of the format.
pub(super) const FORMAT: &str = "format";
/// The name of the level of sorting, filtering and categories.
pub(super) const DATA: &str = "data";
/// The name of the level of the document.
pub(super) const DOCUMENT: &str = "document";

/// A change that the Inspector asks of the window.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Change {
    /// The name of the item selected.
    ItemName(String),
    /// Whether the name of the table shows over it.
    NameShown(bool),
    /// The outline of the grid of the table.
    Grid(Grid),
    /// The name of the sheet shown.
    SheetName(String),
}

type Changed = Rc<dyn Fn(Change)>;

/// The tabs of a level, chosen by an inline switcher that hides when there
/// is only one.
fn tabs(pages: &[(&str, &str, gtk::Widget)]) -> (gtk::Box, adw::ViewStack) {
    let stack = adw::ViewStack::builder()
        .vhomogeneous(false)
        .vexpand(true)
        .build();
    for (name, title, child) in pages {
        let content = gtk::ScrolledWindow::builder()
            .child(child)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        stack.add_titled(&content, Some(name), &tr(title));
    }
    let switcher = adw::InlineViewSwitcher::builder()
        .stack(&stack)
        .homogeneous(true)
        .css_classes(["flat"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .visible(pages.len() > 1)
        .build();
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    root.append(&switcher);
    root.append(&stack);
    (root, stack)
}

/// The box of the groups of a tab.
fn page() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build()
}

/// A dim line that says why a tab has nothing to set.
fn note(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build()
}

fn spin_row(title: &str, lower: f64, upper: f64) -> adw::SpinRow {
    adw::SpinRow::builder()
        .title(title)
        .adjustment(&gtk::Adjustment::new(lower, lower, upper, 1.0, 5.0, 0.0))
        .build()
}

/// The Inspector of a window.
pub(super) struct Inspector {
    root: gtk::Box,
    level: adw::ToggleGroup,
    title: gtk::Label,
    /// Says that nothing is selected that Format sets.
    nothing: gtk::Label,
    /// The tabs of a table.
    table: gtk::Box,
    item_name: adw::EntryRow,
    name_shown: adw::SwitchRow,
    header_rows: adw::SpinRow,
    header_columns: adw::SpinRow,
    footer_rows: adw::SpinRow,
    rows: adw::SpinRow,
    columns: adw::SpinRow,
    /// The tabs of Data, which need a table.
    data: gtk::Box,
    data_note: gtk::Label,
    sheet_name: adw::EntryRow,
    sheets: adw::ActionRow,
    tables: adw::ActionRow,
    charts: adw::ActionRow,
    /// Whether the controls are being set from the document, which is no
    /// change to ask for.
    updating: Cell<bool>,
}

impl Inspector {
    pub fn new(changed: impl Fn(Change) + 'static) -> Rc<Self> {
        let changed: Changed = Rc::new(changed);
        let level = adw::ToggleGroup::builder()
            .homogeneous(true)
            .css_classes(["round"])
            .margin_top(8)
            .margin_bottom(4)
            .margin_start(12)
            .margin_end(12)
            .build();
        for (name, label) in [
            (FORMAT, n_("Format")),
            (DATA, n_("Data")),
            (DOCUMENT, n_("Document")),
        ] {
            level.add(adw::Toggle::builder().name(name).label(tr(label)).build());
        }

        // Format: the title of the selection and its tabs.
        let title = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["heading"])
            .margin_start(14)
            .margin_top(12)
            .margin_bottom(2)
            .build();
        let nothing = note(&tr("Select a table to see its format."));
        nothing.set_margin_start(14);
        nothing.set_margin_end(14);
        nothing.set_margin_top(12);
        let item_name = adw::EntryRow::builder()
            .title(tr("Name"))
            .show_apply_button(true)
            .build();
        let name_shown = adw::SwitchRow::builder()
            .title(tr("Show Table Name"))
            .build();
        let name_group = adw::PreferencesGroup::new();
        name_group.add(&item_name);
        name_group.add(&name_shown);
        let header_rows = spin_row(&tr("Header Rows"), 0.0, 5.0);
        let header_columns = spin_row(&tr("Header Columns"), 0.0, 5.0);
        let footer_rows = spin_row(&tr("Footer Rows"), 0.0, 5.0);
        let headers = adw::PreferencesGroup::builder()
            .title(tr("Headers and Footer"))
            .build();
        headers.add(&header_rows);
        headers.add(&header_columns);
        headers.add(&footer_rows);
        let rows = spin_row(&tr("Rows"), 1.0, 9999.0);
        let columns = spin_row(&tr("Columns"), 1.0, 999.0);
        let size = adw::PreferencesGroup::builder()
            .title(tr("Rows and Columns"))
            .build();
        size.add(&rows);
        size.add(&columns);
        let table_page = page();
        table_page.append(&name_group);
        table_page.append(&headers);
        table_page.append(&size);
        let (table, _) = tabs(&[("table", n_("Table"), table_page.upcast())]);
        let format = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        format.append(&title);
        format.append(&nothing);
        format.append(&table);

        // Data: sort, filter and categories (NTAB-24 to NTAB-30).
        let data_pages: Vec<(&str, &str, gtk::Widget)> = [
            (
                "sort",
                n_("Sort"),
                n_("Sort the rows of the table by its columns."),
            ),
            (
                "filter",
                n_("Filter"),
                n_("Show only the rows that match rules."),
            ),
            (
                "categories",
                n_("Categories"),
                n_("Group the rows by the values of a column."),
            ),
        ]
        .into_iter()
        .map(|(name, title, what)| {
            let content = page();
            content.append(&note(&tr(what)));
            (name, title, content.upcast())
        })
        .collect();
        let (data_tabs, _) = tabs(&data_pages);
        data_tabs.set_sensitive(false);
        let data_note = note(&tr("Select a table to sort, filter and categorize it."));
        data_note.set_margin_start(14);
        data_note.set_margin_end(14);
        data_note.set_margin_top(12);
        let data = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        data.append(&data_note);
        data.append(&data_tabs);

        // Document: the sheet and the information.
        let sheet_name = adw::EntryRow::builder()
            .title(tr("Name"))
            .show_apply_button(true)
            .build();
        let sheet_group = adw::PreferencesGroup::builder().title(tr("Sheet")).build();
        sheet_group.add(&sheet_name);
        let sheet_page = page();
        sheet_page.append(&sheet_group);
        let count_row = |title: &str| {
            let row = adw::ActionRow::builder().title(title).build();
            row.add_css_class("property");
            row
        };
        let sheets = count_row(&tr("Sheets"));
        let tables = count_row(&tr("Tables"));
        let charts = count_row(&tr("Charts"));
        let counts = adw::PreferencesGroup::builder()
            .title(tr("Document"))
            .build();
        counts.add(&sheets);
        counts.add(&tables);
        counts.add(&charts);
        let info_page = page();
        info_page.append(&counts);
        let (document, _) = tabs(&[
            ("sheet", n_("Sheet"), sheet_page.upcast()),
            ("info", n_("Info"), info_page.upcast()),
        ]);

        let levels = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        levels.add_named(&format, Some(FORMAT));
        levels.add_named(&data, Some(DATA));
        levels.add_named(&document, Some(DOCUMENT));
        level.set_active_name(Some(FORMAT));
        {
            let levels = levels.downgrade();
            level.connect_active_name_notify(move |level| {
                if let (Some(levels), Some(name)) = (levels.upgrade(), level.active_name()) {
                    levels.set_visible_child_name(&name);
                }
            });
        }
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(332)
            .build();
        root.append(&level);
        root.append(&levels);

        let this = Rc::new(Self {
            root,
            level,
            title,
            nothing,
            table,
            item_name,
            name_shown,
            header_rows,
            header_columns,
            footer_rows,
            rows,
            columns,
            data,
            data_note,
            sheet_name,
            sheets,
            tables,
            charts,
            updating: Cell::new(false),
        });
        this.connect(&changed);
        this
    }

    fn connect(self: &Rc<Self>, changed: &Changed) {
        let weak = Rc::downgrade(self);
        let ask = move |change: Change| {
            weak.upgrade()
                .filter(|this| !this.updating.get())
                .map(|_| change)
        };
        let ask = Rc::new(ask);
        {
            let (ask, changed) = (ask.clone(), changed.clone());
            self.item_name.connect_apply(move |row| {
                if let Some(change) = ask(Change::ItemName(row.text().to_string())) {
                    changed(change);
                }
            });
        }
        {
            let (ask, changed) = (ask.clone(), changed.clone());
            self.name_shown.connect_active_notify(move |row| {
                if let Some(change) = ask(Change::NameShown(row.is_active())) {
                    changed(change);
                }
            });
        }
        {
            let (ask, changed) = (ask.clone(), changed.clone());
            self.sheet_name.connect_apply(move |row| {
                if let Some(change) = ask(Change::SheetName(row.text().to_string())) {
                    changed(change);
                }
            });
        }
        for row in [
            &self.header_rows,
            &self.header_columns,
            &self.footer_rows,
            &self.rows,
            &self.columns,
        ] {
            let (ask, changed) = (ask.clone(), changed.clone());
            let weak = Rc::downgrade(self);
            row.connect_value_notify(move |_| {
                let Some(this) = weak.upgrade() else { return };
                if let Some(change) = ask(Change::Grid(this.grid())) {
                    changed(change);
                }
            });
        }
    }

    /// The grid that the spin rows set.
    fn grid(&self) -> Grid {
        let value = |row: &adw::SpinRow| row.value().round() as u32;
        Grid {
            rows: value(&self.rows),
            columns: value(&self.columns),
            header_rows: value(&self.header_rows),
            header_columns: value(&self.header_columns),
            footer_rows: value(&self.footer_rows),
        }
    }

    /// The widget to put at the side of the window.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Shows a level: [`FORMAT`], [`DATA`] or [`DOCUMENT`].
    pub fn show_level(&self, name: &str) {
        self.level.set_active_name(Some(name));
    }

    /// Shows what the document and the selection have.
    pub fn show(&self, book: &Workbook, sheet: &Sheet, item: Option<&SheetItem>) {
        self.updating.set(true);
        let grid = match item.map(|i| &i.kind) {
            Some(ItemKind::Table(grid)) => Some(*grid),
            _ => None,
        };
        self.title.set_label(&match item.map(|i| &i.kind) {
            Some(ItemKind::Table(_)) => tr("Table"),
            Some(ItemKind::Chart) => tr("Chart"),
            None => tr("Sheet"),
        });
        self.nothing.set_visible(grid.is_none());
        self.table.set_visible(grid.is_some());
        self.data_note.set_visible(grid.is_none());
        self.data
            .last_child()
            .inspect(|tabs| tabs.set_visible(grid.is_some()));
        if let (Some(item), Some(grid)) = (item, grid) {
            self.item_name.set_text(&item.name);
            self.name_shown.set_active(item.name_shown);
            self.rows.set_value(f64::from(grid.rows));
            self.columns.set_value(f64::from(grid.columns));
            self.header_rows.set_value(f64::from(grid.header_rows));
            self.header_columns
                .set_value(f64::from(grid.header_columns));
            self.footer_rows.set_value(f64::from(grid.footer_rows));
        }
        if !self.sheet_name.has_focus() || self.sheet_name.text() != sheet.name {
            self.sheet_name.set_text(&sheet.name);
        }
        let (tables, charts) = book.counts();
        for (row, n) in [
            (&self.sheets, book.len()),
            (&self.tables, tables),
            (&self.charts, charts),
        ] {
            row.set_subtitle(&crate::bottom_bar::grouped(n));
        }
        self.updating.set(false);
    }
}
