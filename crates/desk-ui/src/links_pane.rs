// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Links, bookmarks and tables of contents in the Inspector (EST-22,
//! EST-23, EST-25, EST-26).
//!
//! The Links group shows where the text at the cursor links to, whether
//! the links of the document are followed on a click, and its bookmarks,
//! each of which can be gone to, renamed or taken away. The Table of
//! Contents group chooses the paragraph styles that tables list, and
//! whether their entries show page numbers.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use desk_model::{StyleId, TocLevel, TocSettings};

use crate::editing::Format;
use crate::formatting::{EditorOf, labelled};
use crate::i18n::tr;

/// An icon button for the end of a row.
fn icon_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .valign(gtk::Align::Center)
        .tooltip_text(label)
        .css_classes(["flat"])
        .build();
    labelled(&button, label);
    button
}

/// The link at the cursor, and the names of the bookmarks.
type Shown = (Option<Arc<str>>, Vec<Arc<str>>);

/// The Links and Bookmarks group of the Inspector.
pub struct LinksGroup {
    group: adw::PreferencesGroup,
    link: adw::EntryRow,
    inactive: adw::SwitchRow,
    bookmarks: adw::PreferencesGroup,
    rows: RefCell<Vec<adw::EntryRow>>,
    /// The link at the cursor and the names of the bookmarks shown.
    shown: RefCell<Shown>,
    updating: Cell<bool>,
    editor: EditorOf,
}

impl LinksGroup {
    /// The group for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let link = adw::EntryRow::builder()
            .title(tr("Link at the Cursor"))
            .show_apply_button(true)
            .build();
        let open = icon_button("go-next-symbolic", &tr("Follow Link"));
        let remove = icon_button("user-trash-symbolic", &tr("Remove Link"));
        link.add_suffix(&open);
        link.add_suffix(&remove);
        let inactive = adw::SwitchRow::builder()
            .title(tr("Make Links Inactive"))
            .subtitle(tr(
                "A click puts the cursor in a link instead of following it",
            ))
            .build();
        let group = adw::PreferencesGroup::builder()
            .title(tr("Hyperlinks"))
            .description(tr(
                "Insert ▸ Hyperlink links the selection. A link to a bookmark starts with #.",
            ))
            .build();
        group.add(&link);
        group.add(&inactive);
        let bookmarks = adw::PreferencesGroup::builder()
            .title(tr("Bookmarks"))
            .description(tr(
                "Insert ▸ Bookmark marks the selection. An empty name takes a bookmark away.",
            ))
            .build();
        let this = Rc::new(Self {
            group,
            link,
            inactive,
            bookmarks,
            rows: RefCell::new(Vec::new()),
            shown: RefCell::new(Default::default()),
            updating: Cell::new(false),
            editor,
        });
        let weak = Rc::downgrade(&this);
        this.link.connect_apply(move |row| {
            if let Some(this) = weak.upgrade()
                && let Some(editor) = (this.editor)()
            {
                editor.set_link(&row.text());
            }
        });
        let weak = Rc::downgrade(&this);
        open.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade()
                && let Some(link) = this.shown.borrow().0.clone()
                && let Some(editor) = (this.editor)()
            {
                editor.open_link(&link);
            }
        });
        let weak = Rc::downgrade(&this);
        remove.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade()
                && let Some(editor) = (this.editor)()
            {
                editor.set_link("");
            }
        });
        let weak = Rc::downgrade(&this);
        this.inactive.connect_active_notify(move |row| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
                && let Some(editor) = (this.editor)()
            {
                let inactive = row.is_active();
                editor.change_text_settings("Links", |text| text.links_inactive = inactive);
            }
        });
        this
    }

    /// The groups.
    pub fn groups(&self) -> [&adw::PreferencesGroup; 2] {
        [&self.group, &self.bookmarks]
    }

    /// Shows the link at the cursor, and the bookmarks of the document.
    pub fn show(self: &Rc<Self>, format: &Format) {
        self.updating.set(true);
        let inactive = format.text_settings.links_inactive;
        if self.inactive.is_active() != inactive {
            self.inactive.set_active(inactive);
        }
        self.link.set_visible(format.link.is_some());
        if let Some(link) = &format.link
            && self.link.text() != **link
        {
            self.link.set_text(link);
        }
        let names_changed = self.shown.borrow().1 != format.bookmarks;
        *self.shown.borrow_mut() = (format.link.clone(), format.bookmarks.clone());
        if names_changed {
            for row in self.rows.borrow_mut().drain(..) {
                self.bookmarks.remove(&row);
            }
            for name in &format.bookmarks {
                let row = self.bookmark_row(name);
                self.bookmarks.add(&row);
                self.rows.borrow_mut().push(row);
            }
        }
        self.bookmarks.set_visible(!format.bookmarks.is_empty());
        self.updating.set(false);
    }

    fn bookmark_row(self: &Rc<Self>, name: &Arc<str>) -> adw::EntryRow {
        let row = adw::EntryRow::builder()
            .title(tr("Bookmark"))
            .text(&**name)
            .show_apply_button(true)
            .build();
        let go = icon_button("go-next-symbolic", &tr("Go to Bookmark"));
        let remove = icon_button("user-trash-symbolic", &tr("Remove Bookmark"));
        row.add_suffix(&go);
        row.add_suffix(&remove);
        let editor = self.editor.clone();
        let old = name.clone();
        row.connect_apply(move |row| {
            if let Some(editor) = editor() {
                editor.rename_bookmark(&old, &row.text());
            }
        });
        let editor = self.editor.clone();
        let old = name.clone();
        go.connect_clicked(move |_| {
            if let Some(editor) = editor() {
                editor.go_to_bookmark(&old);
                editor.focus();
            }
        });
        let editor = self.editor.clone();
        let old = name.clone();
        remove.connect_clicked(move |_| {
            if let Some(editor) = editor() {
                editor.rename_bookmark(&old, "");
            }
        });
        row
    }
}

/// The Table of Contents group of the Inspector.
pub struct TocGroup {
    group: adw::PreferencesGroup,
    page_links: adw::SwitchRow,
    styles: adw::ExpanderRow,
    /// For each paragraph style, whether it is listed, and whether its
    /// entries show their page number: Pages' "#" column.
    rows: RefCell<Vec<(StyleId, adw::SwitchRow, gtk::ToggleButton)>>,
    shown: RefCell<(TocSettings, Vec<(StyleId, String)>)>,
    updating: Cell<bool>,
    editor: EditorOf,
}

impl TocGroup {
    /// The group for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let page_links = adw::SwitchRow::builder()
            .title(tr("Make Page Numbers Links"))
            .active(true)
            .build();
        let styles = adw::ExpanderRow::builder()
            .title(tr("Styles to Include"))
            .build();
        let update = gtk::Button::builder()
            .label(tr("Update Now"))
            .valign(gtk::Align::Center)
            .build();
        let group = adw::PreferencesGroup::builder()
            .title(tr("Table of Contents"))
            .description(tr(
                "Insert ▸ Table of Contents adds one before the paragraph of the cursor.",
            ))
            .header_suffix(&update)
            .build();
        group.add(&styles);
        group.add(&page_links);
        let this = Rc::new(Self {
            group,
            page_links,
            styles,
            rows: RefCell::new(Vec::new()),
            shown: RefCell::new(Default::default()),
            updating: Cell::new(false),
            editor,
        });
        let weak = Rc::downgrade(&this);
        update.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade()
                && let Some(editor) = (this.editor)()
            {
                editor.update_tables_of_contents();
            }
        });
        let weak = Rc::downgrade(&this);
        this.page_links.connect_active_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.changed();
            }
        });
        this
    }

    /// The group.
    pub fn widget(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    /// Shows what tables of contents list.
    pub fn show(self: &Rc<Self>, format: &Format) {
        let styles = (self.editor)()
            .map(|e| e.paragraph_styles())
            .unwrap_or_default();
        let toc = format.text_settings.toc.clone();
        self.updating.set(true);
        if self.shown.borrow().1 != styles {
            for (_, row, _) in self.rows.borrow_mut().drain(..) {
                self.styles.remove(&row);
            }
            for (id, name) in &styles {
                let row = adw::SwitchRow::builder().title(name).build();
                let number = gtk::ToggleButton::builder()
                    .label("#")
                    .tooltip_text(tr("Show Page Number"))
                    .valign(gtk::Align::Center)
                    .active(true)
                    .css_classes(["flat"])
                    .build();
                row.add_suffix(&number);
                let weak = Rc::downgrade(self);
                row.connect_active_notify(move |_| {
                    if let Some(this) = weak.upgrade()
                        && !this.updating.get()
                    {
                        this.changed();
                    }
                });
                let weak = Rc::downgrade(self);
                number.connect_toggled(move |_| {
                    if let Some(this) = weak.upgrade()
                        && !this.updating.get()
                    {
                        this.changed();
                    }
                });
                self.styles.add_row(&row);
                self.rows.borrow_mut().push((*id, row, number));
            }
        }
        for (id, row, number) in self.rows.borrow().iter() {
            let level = toc.level(*id);
            if row.is_active() != level.is_some() {
                row.set_active(level.is_some());
            }
            let shown = level.is_none_or(|l| l.page_number);
            if number.is_active() != shown {
                number.set_active(shown);
            }
            number.set_sensitive(level.is_some());
        }
        let links = toc.levels.is_empty() || toc.page_links;
        if self.page_links.is_active() != links {
            self.page_links.set_active(links);
        }
        let listed = toc.levels.len();
        self.styles.set_subtitle(&if listed == 0 {
            tr("Headings, once a table is inserted")
        } else {
            tr("{n} styles").replace("{n}", &listed.to_string())
        });
        *self.shown.borrow_mut() = (toc, styles);
        self.updating.set(false);
    }

    fn changed(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let (old, styles) = self.shown.borrow().clone();
        let body = editor.styles().default_paragraph();
        let levels = self
            .rows
            .borrow()
            .iter()
            .filter(|(_, row, _)| row.is_active())
            .map(|(id, _, number)| {
                let entry = old.level(*id).map_or_else(
                    || {
                        let name = styles
                            .iter()
                            .find(|(s, _)| s == id)
                            .map(|(_, n)| format!("TOC {n}"))
                            .unwrap_or_default();
                        styles
                            .iter()
                            .find(|(_, n)| *n == name)
                            .map_or(body, |(s, _)| *s)
                    },
                    |l| l.entry,
                );
                TocLevel {
                    style: *id,
                    entry,
                    page_number: number.is_active(),
                }
            })
            .collect();
        let toc = TocSettings {
            levels,
            page_links: self.page_links.is_active(),
        };
        if toc != old {
            editor.set_toc(toc);
        }
    }
}
