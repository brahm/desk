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

use crate::compact;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use desk_model::{StyleId, TocLevel, TocScope, TocSettings};

use crate::editing::Format;
use crate::formatting::{EditorOf, labelled};
use crate::i18n::{n_, tr};
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

/// The styles that links suggest as bookmarks, and the paragraph styles
/// of the document, as shown.
type Suggested = (Vec<StyleId>, Vec<(StyleId, String)>);

/// The Links and Bookmarks group of the Inspector.
pub struct LinksGroup {
    group: compact::Group,
    /// What the link at the cursor points to: a web page, an e-mail
    /// message, a document or a bookmark (EST-25).
    kind: compact::ComboRow,
    /// Its address, whom the message is to, the file or the bookmark.
    link: compact::EntryRow,
    /// The subject of the message, or the bookmark of the document.
    second: compact::EntryRow,
    choose: gtk::Button,
    /// The bookmarks and the suggested ones that a link to a bookmark can
    /// go to (W101).
    pick: gtk::MenuButton,
    inactive: compact::SwitchRow,
    /// The Bookmarks tab of the Document pane (W101).
    bookmarks: compact::Group,
    /// The paragraph styles that links suggest as bookmarks: Manage
    /// Bookmark Suggestions of Pages 15.4 (W101).
    suggestions: compact::Group,
    suggested: compact::ExpanderRow,
    suggestion_rows: RefCell<Vec<(StyleId, compact::SwitchRow)>>,
    suggestions_shown: RefCell<Suggested>,
    /// Whether the bookmarks are listed by name, rather than in the order
    /// of their pages (EST-26).
    by_name: gtk::DropDown,
    rows: RefCell<Vec<compact::EntryRow>>,
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
            tr("Phone Number"),
            tr("Document"),
            tr("Bookmark"),
        ];
        let kinds: Vec<&str> = kinds.iter().map(String::as_str).collect();
        let kind = compact::ComboRow::builder()
            .title(tr("Link To"))
            .model(&gtk::StringList::new(&kinds))
            .build();
        let open = icon_button("go-next-symbolic", &tr("Follow Link"));
        let remove = icon_button("user-trash-symbolic", &tr("Remove Link"));
        kind.add_suffix(&open);
        kind.add_suffix(&remove);
        let link = compact::EntryRow::builder()
            .title(tr("Address"))
            .show_apply_button(true)
            .build();
        let choose = icon_button("document-open-symbolic", &tr("Choose Document"));
        link.add_suffix(&choose);
        let pick = gtk::MenuButton::builder()
            .icon_name("pan-down-symbolic")
            .valign(gtk::Align::Center)
            .tooltip_text(tr("Choose Bookmark"))
            .css_classes(["flat"])
            .build();
        labelled(&pick, &tr("Choose Bookmark"));
        link.add_suffix(&pick);
        let second = compact::EntryRow::builder()
            .title(tr("Subject"))
            .show_apply_button(true)
            .build();
        let inactive = compact::SwitchRow::builder()
            .title(tr("Make Links Inactive"))
            .subtitle(tr(
                "A click puts the cursor in a link instead of following it",
            ))
            .build();
        let group = compact::Group::builder()
            .title(tr("Hyperlinks"))
            .description(tr(
                "Insert ▸ Hyperlink links the selection. A link to a bookmark starts with #.",
            ))
            .build();
        group.add(&kind);
        group.add(&link);
        group.add(&second);
        group.add(&inactive);
        let bookmarks = compact::Group::builder()
            .title(tr("Bookmarks"))
            .description(tr(
                "Add Bookmark marks the selection, or the word at the cursor. An empty name takes a bookmark away.",
            ))
            .build();
        let by_name = gtk::DropDown::from_strings(&[&tr("By Page"), &tr("By Name")]);
        by_name.set_valign(gtk::Align::Center);
        by_name.set_tooltip_text(Some(&tr("Sort Bookmarks")));
        labelled(&by_name, &tr("Sort Bookmarks"));
        let add = gtk::Button::builder()
            .label(tr("Add Bookmark"))
            .action_name("win.insert-bookmark")
            .valign(gtk::Align::Center)
            .build();
        let header = gtk::Box::builder().spacing(6).build();
        header.append(&by_name);
        header.append(&add);
        bookmarks.set_header_suffix(Some(&header));
        let suggested = compact::ExpanderRow::builder()
            .title(tr("Styles to Suggest"))
            .build();
        let suggestions = compact::Group::builder()
            .title(tr("Bookmark Suggestions"))
            .description(tr(
                "A link to a bookmark also suggests the paragraphs of these styles.",
            ))
            .build();
        suggestions.add(&suggested);
        let this = Rc::new(Self {
            group,
            kind,
            link,
            second,
            choose,
            pick,
            inactive,
            bookmarks,
            suggestions,
            suggested,
            suggestion_rows: RefCell::new(Vec::new()),
            suggestions_shown: RefCell::new(Default::default()),
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
        this.pick.set_create_popup_func(move |button| {
            if let Some(this) = weak.upgrade() {
                button.set_popover(Some(&this.bookmark_menu()));
            }
        });
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

    /// The groups: the link, the bookmarks and the styles they suggest.
    pub fn groups(&self) -> [&compact::Group; 3] {
        [&self.group, &self.bookmarks, &self.suggestions]
    }

    /// The list that a link to a bookmark chooses from: the bookmarks,
    /// then the paragraphs suggested (W101).
    fn bookmark_menu(self: &Rc<Self>) -> gtk::Popover {
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        let popover = gtk::Popover::builder()
            .child(
                &gtk::ScrolledWindow::builder()
                    .hscrollbar_policy(gtk::PolicyType::Never)
                    .propagate_natural_height(true)
                    .max_content_height(360)
                    .child(&list)
                    .build(),
            )
            .build();
        let (names, suggested) = (self.editor)()
            .map(|e| e.bookmark_choices())
            .unwrap_or_default();
        let item = |label: &str| {
            gtk::Button::builder()
                .child(
                    &gtk::Label::builder()
                        .label(label)
                        .xalign(0.0)
                        .ellipsize(gtk::pango::EllipsizeMode::End)
                        .max_width_chars(32)
                        .build(),
                )
                .css_classes(["flat"])
                .build()
        };
        let heading = |text: String| {
            gtk::Label::builder()
                .label(text)
                .xalign(0.0)
                .margin_start(6)
                .margin_top(6)
                .css_classes(["heading"])
                .build()
        };
        if !names.is_empty() {
            list.append(&heading(tr("Bookmarks")));
        }
        for name in names {
            let button = item(&name);
            let weak = Rc::downgrade(self);
            let popover = popover.downgrade();
            button.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.link.set_text(&name);
                    this.apply();
                }
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            });
            list.append(&button);
        }
        if !suggested.is_empty() {
            list.append(&heading(tr("Suggested")));
        }
        for (index, text) in suggested {
            let button = item(&text);
            let editor = self.editor.clone();
            let popover = popover.downgrade();
            button.connect_clicked(move |_| {
                if let Some(editor) = editor() {
                    editor.link_to_paragraph(index);
                }
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            });
            list.append(&button);
        }
        if list.first_child().is_none() {
            list.append(
                &gtk::Label::builder()
                    .label(tr("No Bookmarks"))
                    .margin_top(6)
                    .margin_bottom(6)
                    .css_classes(["dim-label"])
                    .build(),
            );
        }
        popover
    }

    /// Shows the styles that links suggest as bookmarks.
    fn show_suggestions(self: &Rc<Self>, format: &Format) {
        let styles = (self.editor)()
            .map(|e| e.paragraph_styles())
            .unwrap_or_default();
        let chosen = format.bookmark_suggestions.clone();
        if *self.suggestions_shown.borrow() == (chosen.clone(), styles.clone()) {
            return;
        }
        if self.suggestions_shown.borrow().1 != styles {
            for (_, row) in self.suggestion_rows.borrow_mut().drain(..) {
                self.suggested.remove(&row);
            }
            for (id, name) in &styles {
                // A style's name is text, not markup.
                let row = compact::SwitchRow::builder()
                    .title(gtk::glib::markup_escape_text(name))
                    .build();
                let weak = Rc::downgrade(self);
                row.connect_active_notify(move |_| {
                    if let Some(this) = weak.upgrade()
                        && !this.updating.get()
                    {
                        this.suggestions_changed();
                    }
                });
                self.suggested.add_row(&row);
                self.suggestion_rows.borrow_mut().push((*id, row));
            }
        }
        for (id, row) in self.suggestion_rows.borrow().iter() {
            let on = chosen.contains(id);
            if row.is_active() != on {
                row.set_active(on);
            }
        }
        self.suggested.set_subtitle(&if chosen.is_empty() {
            tr("None")
        } else {
            tr("{n} styles").replace("{n}", &chosen.len().to_string())
        });
        *self.suggestions_shown.borrow_mut() = (chosen, styles);
    }

    fn suggestions_changed(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let chosen: Vec<StyleId> = self
            .suggestion_rows
            .borrow()
            .iter()
            .filter(|(_, row)| row.is_active())
            .map(|(id, _)| *id)
            .collect();
        if chosen != self.suggestions_shown.borrow().0 {
            editor.set_bookmark_suggestions(chosen);
        }
    }

    /// Shows the link at the cursor, and the bookmarks of the document.
    pub fn show(self: &Rc<Self>, format: &Format) {
        self.updating.set(true);
        let inactive = format.text_settings.links_inactive;
        if self.inactive.is_active() != inactive {
            self.inactive.set_active(inactive);
        }
        let changed = self.shown.borrow().0 != format.link;
        for row in [self.kind.upcast_ref(), self.link.upcast_ref()] {
            row.set_visible(format.link.is_some());
        }
        if let Some(link) = &format.link
            && changed
        {
            let target = Target::parse(link);
            if !matches!(target, Target::Slide(_)) {
                self.kind.set_selected(target.kind());
            }
            let (first, second) = match &target {
                Target::Web(address) => (address.as_str(), ""),
                Target::Email { to, subject } => (to.as_str(), subject.as_str()),
                Target::Phone(number) => (number.as_str(), ""),
                Target::Document { file, bookmark } => (file.as_str(), bookmark.as_str()),
                Target::Bookmark(name) => (name.as_str(), ""),
                Target::Slide(_) => ("", ""),
            };
            // A link to a slide, which only Desk Slides follows, shows what
            // it does and stays as it is (IOP-32).
            let slide = match &target {
                Target::Slide(slide) => Some(slide.describe()),
                _ => None,
            };
            self.kind.set_sensitive(slide.is_none());
            self.link.set_editable(slide.is_none());
            self.link.set_text(slide.as_deref().unwrap_or(first));
            self.second.set_text(second);
        }
        self.updating.set(false);
        self.show_kind();
        self.updating.set(true);
        if format.link.is_none() {
            self.second.set_visible(false);
            self.choose.set_visible(false);
            self.pick.set_visible(false);
        }
        let names_changed = self.shown.borrow().1 != format.bookmarks;
        *self.shown.borrow_mut() = (format.link.clone(), format.bookmarks.clone());
        if names_changed {
            self.fill_bookmarks();
        }
        self.show_suggestions(format);
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
        if !self.kind.is_sensitive() {
            self.link.set_title(&tr("Link of a Presentation"));
            self.second.set_visible(false);
            self.choose.set_visible(false);
            self.pick.set_visible(false);
            return;
        }
        let (first, second) = match kind {
            1 => (tr("To"), Some(tr("Subject"))),
            2 => (tr("Phone Number"), None),
            3 => (tr("File"), Some(tr("Bookmark in the Document"))),
            4 => (tr("Bookmark"), None),
            _ => (tr("Address"), None),
        };
        self.link.set_title(&first);
        self.second.set_visible(second.is_some());
        if let Some(second) = second {
            self.second.set_title(&second);
        }
        self.choose.set_visible(kind == 3);
        self.pick.set_visible(kind == 4);
    }

    /// Links the text at the cursor to what the rows say.
    fn apply(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        if !self.kind.is_sensitive() {
            return;
        }
        let first = self.link.text().to_string();
        let second = self.second.text().to_string();
        let target = match self.kind.selected() {
            1 => Target::Email {
                to: first,
                subject: second,
            },
            2 => Target::Phone(first),
            3 => Target::Document {
                file: first,
                bookmark: second,
            },
            4 => Target::Bookmark(first),
            _ => Target::Web(first),
        };
        let empty = matches!(&target, Target::Web(a) | Target::Bookmark(a) if a.is_empty())
            || matches!(&target, Target::Phone(n) if !n.chars().any(|c| c.is_ascii_digit()))
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

    fn bookmark_row(self: &Rc<Self>, name: &Arc<str>) -> compact::EntryRow {
        let row = compact::EntryRow::builder()
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

/// The ranges of a table of contents, as Insert > Table of Contents and
/// the Range of the Format pane name them in Pages 15.4 (W100).
pub const TOC_SCOPES: [(TocScope, &str); 3] = [
    (TocScope::Document, n_("Document")),
    (TocScope::Section, n_("Section")),
    (TocScope::NextTable, n_("To Next Occurrence")),
];

/// The Table of Contents group of the Inspector.
pub struct TocGroup {
    group: compact::Group,
    /// Which paragraphs the table at the cursor lists (W100).
    range: compact::ComboRow,
    page_links: compact::SwitchRow,
    styles: compact::ExpanderRow,
    /// For each paragraph style, whether it is listed, and whether its
    /// entries show their page number: Pages' "#" column.
    rows: RefCell<Vec<(StyleId, compact::SwitchRow, gtk::ToggleButton)>>,
    shown: RefCell<(TocSettings, Vec<(StyleId, String)>)>,
    updating: Cell<bool>,
    editor: EditorOf,
}

impl TocGroup {
    /// The group for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let page_links = compact::SwitchRow::builder()
            .title(tr("Make Page Numbers Links"))
            .active(true)
            .build();
        let styles = compact::ExpanderRow::builder()
            .title(tr("Styles to Include"))
            .build();
        let range = compact::ComboRow::builder()
            .title(tr("Range"))
            .subtitle(tr("The paragraphs that the table at the cursor lists"))
            .model(&crate::tab_stops::string_list(
                TOC_SCOPES.iter().map(|(_, name)| tr(name)),
            ))
            .visible(false)
            .build();
        let update = gtk::Button::builder()
            .label(tr("Update Now"))
            .valign(gtk::Align::Center)
            .build();
        let group = compact::Group::builder()
            .title(tr("Table of Contents"))
            .description(tr(
                "Insert ▸ Table of Contents adds one before the paragraph of the cursor.",
            ))
            .header_suffix(&update)
            .build();
        group.add(&range);
        group.add(&styles);
        group.add(&page_links);
        let this = Rc::new(Self {
            group,
            range,
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
        this.range.connect_selected_notify(move |row| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
                && let Some((scope, _)) = TOC_SCOPES.get(row.selected() as usize)
                && let Some(editor) = (this.editor)()
            {
                editor.set_toc_scope(*scope);
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
    pub fn widget(&self) -> &compact::Group {
        &self.group
    }

    /// Shows what tables of contents list.
    pub fn show(self: &Rc<Self>, format: &Format) {
        let styles = (self.editor)()
            .map(|e| e.paragraph_styles())
            .unwrap_or_default();
        let toc = format.text_settings.toc.clone();
        self.updating.set(true);
        self.range.set_visible(format.toc_scope.is_some());
        if let Some(scope) = format.toc_scope
            && let Some(at) = TOC_SCOPES.iter().position(|(s, _)| *s == scope)
            && self.range.selected() != at as u32
        {
            self.range.set_selected(at as u32);
        }
        if self.shown.borrow().1 != styles {
            for (_, row, _) in self.rows.borrow_mut().drain(..) {
                self.styles.remove(&row);
            }
            for (id, name) in &styles {
                // A style's name is text, not markup: "Header & Footer".
                let row = compact::SwitchRow::builder()
                    .title(gtk::glib::markup_escape_text(name))
                    .build();
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
