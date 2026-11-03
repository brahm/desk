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
use crate::links::Target;

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
    /// What the link at the cursor points to: a web page, an e-mail
    /// message, a document or a bookmark (EST-25).
    kind: adw::ComboRow,
    /// Its address, whom the message is to, the file or the bookmark.
    link: adw::EntryRow,
    /// The subject of the message, or the bookmark of the document.
    second: adw::EntryRow,
    choose: gtk::Button,
    inactive: adw::SwitchRow,
    bookmarks: adw::PreferencesGroup,
    /// Whether the bookmarks are listed by name, rather than in the order
    /// of their pages (EST-26).
    by_name: gtk::DropDown,
    rows: RefCell<Vec<adw::EntryRow>>,
    /// The link at the cursor and the names of the bookmarks shown.
    shown: RefCell<Shown>,
    updating: Cell<bool>,
    editor: EditorOf,
}

impl LinksGroup {
    /// The group for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let kinds = [
            tr("Webpage"),
            tr("Email Message"),
            tr("Document"),
            tr("Bookmark"),
        ];
        let kinds: Vec<&str> = kinds.iter().map(String::as_str).collect();
        let kind = adw::ComboRow::builder()
            .title(tr("Link To"))
            .model(&gtk::StringList::new(&kinds))
            .build();
        let open = icon_button("go-next-symbolic", &tr("Follow Link"));
        let remove = icon_button("user-trash-symbolic", &tr("Remove Link"));
        kind.add_suffix(&open);
        kind.add_suffix(&remove);
        let link = adw::EntryRow::builder()
            .title(tr("Address"))
            .show_apply_button(true)
            .build();
        let choose = icon_button("document-open-symbolic", &tr("Choose Document"));
        link.add_suffix(&choose);
        let second = adw::EntryRow::builder()
            .title(tr("Subject"))
            .show_apply_button(true)
            .build();
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
        group.add(&kind);
        group.add(&link);
        group.add(&second);
        group.add(&inactive);
        let bookmarks = adw::PreferencesGroup::builder()
            .title(tr("Bookmarks"))
            .description(tr(
                "Insert ▸ Bookmark marks the selection. An empty name takes a bookmark away.",
            ))
            .build();
        let by_name = gtk::DropDown::from_strings(&[&tr("By Page"), &tr("By Name")]);
        by_name.set_valign(gtk::Align::Center);
        by_name.set_tooltip_text(Some(&tr("Sort Bookmarks")));
        labelled(&by_name, &tr("Sort Bookmarks"));
        bookmarks.set_header_suffix(Some(&by_name));
        let this = Rc::new(Self {
            group,
            kind,
            link,
            second,
            choose,
            inactive,
            bookmarks,
            by_name,
            rows: RefCell::new(Vec::new()),
            shown: RefCell::new(Default::default()),
            updating: Cell::new(false),
            editor,
        });
        for row in [&this.link, &this.second] {
            let weak = Rc::downgrade(&this);
            row.connect_apply(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.apply();
                }
            });
        }
        let weak = Rc::downgrade(&this);
        this.by_name.connect_selected_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.fill_bookmarks();
            }
        });
        let weak = Rc::downgrade(&this);
        this.kind.connect_selected_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.show_kind();
            }
        });
        let weak = Rc::downgrade(&this);
        this.choose.connect_clicked(move |button| {
            if let Some(this) = weak.upgrade() {
                this.choose_document(button);
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
        let changed = self.shown.borrow().0 != format.link;
        for row in [
            self.kind.upcast_ref::<gtk::Widget>(),
            self.link.upcast_ref(),
        ] {
            row.set_visible(format.link.is_some());
        }
        if let Some(link) = &format.link
            && changed
        {
            let target = Target::parse(link);
            self.kind.set_selected(target.kind());
            let (first, second) = match &target {
                Target::Web(address) => (address.as_str(), ""),
                Target::Email { to, subject } => (to.as_str(), subject.as_str()),
                Target::Document { file, bookmark } => (file.as_str(), bookmark.as_str()),
                Target::Bookmark(name) => (name.as_str(), ""),
            };
            self.link.set_text(first);
            self.second.set_text(second);
        }
        self.updating.set(false);
        self.show_kind();
        self.updating.set(true);
        if format.link.is_none() {
            self.second.set_visible(false);
            self.choose.set_visible(false);
        }
        let names_changed = self.shown.borrow().1 != format.bookmarks;
        *self.shown.borrow_mut() = (format.link.clone(), format.bookmarks.clone());
        if names_changed {
            self.fill_bookmarks();
        }
        self.bookmarks.set_visible(!format.bookmarks.is_empty());
        self.updating.set(false);
    }

    /// Lists the bookmarks, in the order chosen.
    fn fill_bookmarks(self: &Rc<Self>) {
        for row in self.rows.borrow_mut().drain(..) {
            self.bookmarks.remove(&row);
        }
        let mut names = self.shown.borrow().1.clone();
        if self.by_name.selected() == 1 {
            names.sort_by_cached_key(|n| n.to_lowercase());
        }
        for name in &names {
            let row = self.bookmark_row(name);
            self.bookmarks.add(&row);
            self.rows.borrow_mut().push(row);
        }
    }

    /// Names the rows for the kind of link chosen.
    fn show_kind(&self) {
        let kind = self.kind.selected();
        let (first, second) = match kind {
            1 => (tr("To"), Some(tr("Subject"))),
            2 => (tr("File"), Some(tr("Bookmark in the Document"))),
            3 => (tr("Bookmark"), None),
            _ => (tr("Address"), None),
        };
        self.link.set_title(&first);
        self.second.set_visible(second.is_some());
        if let Some(second) = second {
            self.second.set_title(&second);
        }
        self.choose.set_visible(kind == 2);
    }

    /// Links the text at the cursor to what the rows say.
    fn apply(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let first = self.link.text().to_string();
        let second = self.second.text().to_string();
        let target = match self.kind.selected() {
            1 => Target::Email {
                to: first,
                subject: second,
            },
            2 => Target::Document {
                file: first,
                bookmark: second,
            },
            3 => Target::Bookmark(first),
            _ => Target::Web(first),
        };
        let empty = matches!(&target, Target::Web(a) | Target::Bookmark(a) if a.is_empty())
            || matches!(&target, Target::Email { to, .. } if to.is_empty())
            || matches!(&target, Target::Document { file, .. } if file.is_empty());
        editor.set_link(&if empty { String::new() } else { target.link() });
    }

    /// Asks for the document that the link opens.
    fn choose_document(self: &Rc<Self>, button: &gtk::Button) {
        let dialog = gtk::FileDialog::builder()
            .title(tr("Choose Document"))
            .modal(true)
            .build();
        let window = button.root().and_downcast::<gtk::Window>();
        let weak = Rc::downgrade(self);
        dialog.open(
            window.as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| {
                if let (Some(this), Ok(file)) = (weak.upgrade(), result) {
                    this.link.set_text(&file.uri());
                    this.apply();
                }
            },
        );
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
