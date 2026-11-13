// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Find & Replace (TXT-12) and the Search sidebar (TXT-13), as in Pages.
//!
//! The Find & Replace window has a simple mode, with the text to find and
//! its replacement, and an advanced one, which adds Match Case and Whole
//! Words. Its buttons replace all, replace, replace and find, and go to
//! the previous and next match. The search of a window is shared: Find
//! Next and Find Previous use it with the window closed, and Use Selection
//! for Find sets it from the selection.
//!
//! The Search sidebar lists every match as it is typed, with its page and
//! the text around it, and selects a match when it is clicked, so that it
//! can be edited in place. It keeps the last ten searches.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Found, Query};
use gtk::glib;

use crate::editing::TextEditor;
use crate::formatting::{EditorOf, labelled};
use crate::i18n::{tr, trn};

/// How many searches the sidebar keeps.
pub const RECENT: usize = 10;

/// How many matches the sidebar lists.
const MAX_RESULTS: usize = 1000;

/// How many characters of context a result shows on each side.
const CONTEXT: usize = 30;

/// The search of a window, which the Find & Replace window, Find Next and
/// the Search sidebar share.
#[derive(Default)]
pub struct Search {
    /// What to find.
    pub query: RefCell<Query>,
    /// What replaces it.
    pub replacement: RefCell<String>,
    /// The last searches, the newest first.
    pub recent: RefCell<Vec<String>>,
}

impl Search {
    /// Keeps a search among the recent ones.
    pub fn remember(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut recent = self.recent.borrow_mut();
        recent.retain(|t| t != text);
        recent.insert(0, text.to_owned());
        recent.truncate(RECENT);
    }
}

/// Where a match or the selection is: the rank of its header, footer or
/// note among those searched (the body first), its paragraph and offset.
pub type Place = (usize, usize, usize);

/// The match after the selection, or before it with `backwards`, going
/// round the document. A match that is the selection is skipped. `rank`
/// gives the place of a header, footer or note in the order searched.
pub fn next_match<'a>(
    found: &'a [Found],
    selection: (Place, Place),
    backwards: bool,
    rank: &dyn Fn(Option<desk_model::Story>) -> usize,
) -> Option<&'a Found> {
    let (start, end) = selection;
    let key = |f: &Found| {
        let r = rank(f.story);
        ((r, f.index, f.range.start), (r, f.index, f.range.end))
    };
    if backwards {
        found
            .iter()
            .rev()
            .find(|f| key(f).1 <= start && key(f) != (start, end))
            .or_else(|| found.last())
    } else {
        found
            .iter()
            .find(|f| key(f).0 >= end && key(f) != (start, end))
            .or_else(|| found.first())
    }
}

/// The selection of an editor, as places.
fn selection_of(editor: &TextEditor) -> (Place, Place) {
    let (start, end) = editor.selection().ordered();
    let rank = editor.story_rank(editor.story());
    (
        (rank, start.paragraph, start.offset),
        (rank, end.paragraph, end.offset),
    )
}

/// Selects the next match of the search, or the previous one, and says
/// whether there was one.
pub fn find_next(editor: &TextEditor, search: &Search, backwards: bool) -> bool {
    let query = search.query.borrow().clone();
    let found = editor.find(&query);
    let rank = |story| editor.story_rank(story);
    match next_match(&found, selection_of(editor), backwards, &rank) {
        Some(next) => {
            editor.select_found(next);
            true
        }
        None => false,
    }
}

/// The match that the selection is, if it is one.
fn selected_match(editor: &TextEditor, query: &Query) -> Option<Found> {
    let selection = selection_of(editor);
    let story = editor.story();
    editor.find(query).into_iter().find(|f| {
        f.story == story
            && ((f.index, f.range.start), (f.index, f.range.end))
                == (
                    (selection.0.1, selection.0.2),
                    (selection.1.1, selection.1.2),
                )
    })
}

/// The text around a match, with the match in bold, as Pango markup.
pub fn context_markup(text: &str, range: std::ops::Range<usize>) -> String {
    let before: String = {
        let chars: Vec<char> = text[..range.start].chars().collect();
        let skip = chars.len().saturating_sub(CONTEXT);
        let mut out: String = chars[skip..].iter().collect();
        if skip > 0 {
            out.insert(0, '\u{2026}');
        }
        out
    };
    let after: String = {
        let mut chars = text[range.end..].chars();
        let mut out: String = chars.by_ref().take(CONTEXT).collect();
        if chars.next().is_some() {
            out.push('\u{2026}');
        }
        out
    };
    let clean = |t: &str| glib::markup_escape_text(&t.replace(['\t', '\u{2028}'], " ")).to_string();
    format!(
        "{}<b>{}</b>{}",
        clean(&before),
        clean(&text[range]),
        clean(&after)
    )
}

/// The Find & Replace bar of a document window, under the header bar,
/// with Replace on a second row and Match Case and Whole Words in the
/// Advanced popover.
pub struct FindWindow {
    window: gtk::SearchBar,
    find: gtk::Entry,
    replace: gtk::Entry,
    replace_row: gtk::Box,
    replace_toggle: gtk::ToggleButton,
    match_case: gtk::CheckButton,
    whole_words: gtk::CheckButton,
    status: gtk::Label,
    search: Rc<Search>,
    editor: EditorOf,
    updating: Cell<bool>,
}

impl FindWindow {
    /// The bar, for the editor of a document window.
    pub fn new(editor: EditorOf, search: Rc<Search>) -> Rc<Self> {
        let find = gtk::Entry::builder()
            .placeholder_text(tr("Find"))
            .primary_icon_name("system-search-symbolic")
            .hexpand(true)
            .build();
        labelled(&find, &tr("Find"));
        let replace = gtk::Entry::builder()
            .placeholder_text(tr("Replace"))
            .hexpand(true)
            .build();
        labelled(&replace, &tr("Replace"));
        let match_case = gtk::CheckButton::with_label(&tr("Match Case"));
        let whole_words = gtk::CheckButton::with_label(&tr("Whole Words"));
        let options = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        options.append(&match_case);
        options.append(&whole_words);
        let advanced = gtk::MenuButton::builder()
            .icon_name("emblem-system-symbolic")
            .popover(&gtk::Popover::builder().child(&options).build())
            .tooltip_text(tr("Advanced"))
            .build();
        labelled(&advanced, &tr("Advanced"));

        let button = |label: &str| {
            gtk::Button::builder()
                .label(label)
                .use_underline(true)
                .build()
        };
        let replace_all = button(&tr("Replace _All"));
        let replace_one = button(&tr("_Replace"));
        let replace_find = button(&tr("Replace & _Find"));
        let previous = gtk::Button::builder()
            .icon_name("go-up-symbolic")
            .tooltip_text(tr("Previous"))
            .build();
        labelled(&previous, &tr("Previous"));
        let next = gtk::Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text(tr("Next"))
            .build();
        labelled(&next, &tr("Next"));
        let arrows = gtk::Box::builder().css_classes(["linked"]).build();
        arrows.append(&previous);
        arrows.append(&next);
        let replace_toggle = gtk::ToggleButton::builder()
            .icon_name("edit-find-replace-symbolic")
            .tooltip_text(tr("Replace"))
            .build();
        labelled(&replace_toggle, &tr("Show Replace"));
        let status = gtk::Label::builder()
            .xalign(0.0)
            .width_chars(10)
            .css_classes(["dim-label", "caption"])
            .build();
        status.set_accessible_role(gtk::AccessibleRole::Status);

        let find_row = gtk::Box::builder().spacing(6).build();
        find_row.append(&find);
        find_row.append(&arrows);
        find_row.append(&status);
        find_row.append(&replace_toggle);
        find_row.append(&advanced);
        let replace_row = gtk::Box::builder().spacing(6).visible(false).build();
        replace_row.append(&replace);
        replace_row.append(&replace_one);
        replace_row.append(&replace_find);
        replace_row.append(&replace_all);
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .width_request(360)
            .build();
        content.append(&find_row);
        content.append(&replace_row);
        let clamp = adw::Clamp::builder()
            .maximum_size(720)
            .hexpand(true)
            .child(&content)
            .build();
        let window = gtk::SearchBar::builder()
            .child(&clamp)
            .show_close_button(true)
            .build();
        labelled(&window, &tr("Find & Replace"));
        window.connect_entry(&find);

        let this = Rc::new(Self {
            window,
            find,
            replace,
            replace_row,
            replace_toggle,
            match_case,
            whole_words,
            status,
            search,
            editor,
            updating: Cell::new(false),
        });

        let weak = Rc::downgrade(&this);
        this.replace_toggle.connect_toggled(move |button| {
            let Some(this) = weak.upgrade() else { return };
            this.replace_row.set_visible(button.is_active());
            if button.is_active() {
                this.replace.grab_focus();
            }
        });
        for check in [&this.match_case, &this.whole_words] {
            let weak = Rc::downgrade(&this);
            check.connect_toggled(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.store();
                }
            });
        }
        for entry in [&this.find, &this.replace] {
            let weak = Rc::downgrade(&this);
            entry.connect_changed(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.store();
                }
            });
        }
        let weak = Rc::downgrade(&this);
        this.find.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.go(false);
            }
        });
        let weak = Rc::downgrade(&this);
        this.replace.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.replace_selected();
                this.go(false);
            }
        });
        // Back to the text when the bar closes.
        let weak = Rc::downgrade(&this);
        this.window.connect_search_mode_enabled_notify(move |bar| {
            if let (false, Some(this)) = (bar.is_search_mode(), weak.upgrade())
                && let Some(editor) = (this.editor)()
            {
                editor.focus();
            }
        });
        let connect = |button: &gtk::Button, run: fn(&FindWindow)| {
            let weak = Rc::downgrade(&this);
            button.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    run(&this);
                }
            });
        };
        connect(&next, |this| this.go(false));
        connect(&previous, |this| this.go(true));
        connect(&replace_one, |this| {
            this.replace_selected();
        });
        connect(&replace_find, |this| {
            this.replace_selected();
            this.go(false);
        });
        connect(&replace_all, FindWindow::replace_everything);
        this
    }

    /// The bar, to put under the header bar.
    pub fn widget(&self) -> &gtk::SearchBar {
        &self.window
    }

    /// The query that the fields set.
    fn query(&self) -> Query {
        Query {
            text: self.find.text().to_string(),
            match_case: self.match_case.is_active(),
            whole_words: self.whole_words.is_active(),
        }
    }

    /// Keeps what the fields say in the search of the window.
    fn store(&self) {
        if self.updating.get() {
            return;
        }
        *self.search.query.borrow_mut() = self.query();
        *self.search.replacement.borrow_mut() = self.replace.text().to_string();
        self.status.set_label("");
    }

    /// Shows the bar with the search of the window, and the keyboard in
    /// the field to find.
    pub fn present(&self) {
        self.updating.set(true);
        let query = self.search.query.borrow().clone();
        if self.find.text() != query.text {
            self.find.set_text(&query.text);
        }
        let replacement = self.search.replacement.borrow().clone();
        if self.replace.text() != replacement {
            self.replace.set_text(&replacement);
        }
        self.match_case.set_active(query.match_case);
        self.whole_words.set_active(query.whole_words);
        self.updating.set(false);
        self.window.set_search_mode(true);
        self.find.grab_focus();
    }

    /// Goes to the next match, or the previous one.
    fn go(&self, backwards: bool) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        self.store();
        let text = self.search.query.borrow().text.clone();
        self.search.remember(&text);
        if !find_next(&editor, &self.search, backwards) {
            self.status.set_label(&tr("Not found"));
        }
    }

    /// Replaces the selection when it is a match.
    fn replace_selected(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        self.store();
        let query = self.search.query.borrow().clone();
        if let Some(found) = selected_match(&editor, &query) {
            editor.replace_found(&found, &self.replace.text());
        }
    }

    fn replace_everything(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        self.store();
        let query = self.search.query.borrow().clone();
        let count = editor.replace_all(&query, &self.replace.text());
        self.status.set_label(&if count == 0 {
            tr("Not found")
        } else {
            trn("Replaced {n} match", "Replaced {n} matches", count as u64)
                .replace("{n}", &count.to_string())
        });
    }
}

/// The Search sidebar (TXT-13).
pub struct SearchSidebar {
    root: gtk::Box,
    entry: gtk::SearchEntry,
    match_case: gtk::ToggleButton,
    whole_words: gtk::ToggleButton,
    recent: gtk::MenuButton,
    results: gtk::ListBox,
    count: gtk::Label,
    found: RefCell<Vec<Found>>,
    search: Rc<Search>,
    editor: EditorOf,
    /// Whether the sidebar shows, so that it searches again when the
    /// document changes.
    shown: Cell<bool>,
}

impl SearchSidebar {
    /// The sidebar of a document window.
    pub fn new(editor: EditorOf, search: Rc<Search>) -> Rc<Self> {
        let entry = gtk::SearchEntry::builder()
            .placeholder_text(tr("Search"))
            .hexpand(true)
            .build();
        labelled(&entry, &tr("Search"));
        let recent = gtk::MenuButton::builder()
            .icon_name("document-open-recent-symbolic")
            .tooltip_text(tr("Recent Searches"))
            .build();
        labelled(&recent, &tr("Recent Searches"));
        let top = gtk::Box::builder().spacing(6).build();
        top.append(&entry);
        top.append(&recent);
        let toggle = |label: &str| {
            gtk::ToggleButton::builder()
                .label(label)
                .css_classes(["flat"])
                .build()
        };
        let match_case = toggle(&tr("Match Case"));
        let whole_words = toggle(&tr("Whole Words"));
        let options = gtk::Box::builder().spacing(6).build();
        options.append(&match_case);
        options.append(&whole_words);
        let count = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();
        count.set_accessible_role(gtk::AccessibleRole::Status);
        let results = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        labelled(&results, &tr("Search Results"));
        let scroll = gtk::ScrolledWindow::builder()
            .child(&results)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_start(12)
            .margin_end(12)
            .width_request(260)
            .build();
        root.append(&top);
        root.append(&options);
        root.append(&count);
        root.append(&scroll);

        let this = Rc::new(Self {
            root,
            entry,
            match_case,
            whole_words,
            recent,
            results,
            count,
            found: RefCell::new(Vec::new()),
            search,
            editor,
            shown: Cell::new(false),
        });
        let weak = Rc::downgrade(&this);
        this.entry.connect_search_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.run();
            }
        });
        let weak = Rc::downgrade(&this);
        this.entry.connect_activate(move |entry| {
            if let Some(this) = weak.upgrade() {
                this.search.remember(&entry.text());
                this.show_recent();
                if let Some(row) = this.results.row_at_index(0) {
                    this.results.select_row(Some(&row));
                }
            }
        });
        for toggle in [&this.match_case, &this.whole_words] {
            let weak = Rc::downgrade(&this);
            toggle.connect_toggled(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.run();
                }
            });
        }
        let weak = Rc::downgrade(&this);
        this.results.connect_row_selected(move |_, row| {
            let (Some(this), Some(row)) = (weak.upgrade(), row) else {
                return;
            };
            let found = this.found.borrow().get(row.index() as usize).cloned();
            if let (Some(found), Some(editor)) = (found, (this.editor)()) {
                this.search.remember(&this.entry.text());
                editor.select_found(&found);
            }
        });
        this.show_recent();
        this
    }

    /// The widget to put at the side of the window.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Shows or hides the sidebar, with the keyboard in its field.
    pub fn set_shown(&self, shown: bool) {
        self.shown.set(shown);
        if shown {
            self.show_recent();
            self.run();
            self.entry.grab_focus();
        }
    }

    /// Searches with a text, as a recent search does.
    pub fn search_for(&self, text: &str) {
        self.entry.set_text(text);
        self.entry.set_position(-1);
    }

    /// The menu of recent searches.
    fn show_recent(&self) {
        let menu = gtk::gio::Menu::new();
        for text in self.search.recent.borrow().iter() {
            let item = gtk::gio::MenuItem::new(Some(text), None);
            item.set_action_and_target_value(Some("win.recent-search"), Some(&text.to_variant()));
            menu.append_item(&item);
        }
        self.recent.set_sensitive(menu.n_items() > 0);
        self.recent.set_menu_model(Some(&menu));
    }

    /// Searches again, as the document changed.
    pub fn document_changed(&self) {
        if self.shown.get() {
            self.run();
        }
    }

    /// Lists the matches.
    fn run(&self) {
        while let Some(row) = self.results.row_at_index(0) {
            self.results.remove(&row);
        }
        let query = Query {
            text: self.entry.text().to_string(),
            match_case: self.match_case.is_active(),
            whole_words: self.whole_words.is_active(),
        };
        let Some(editor) = (self.editor)() else {
            return;
        };
        let found = if query.text.is_empty() {
            Vec::new()
        } else {
            editor.find(&query)
        };
        self.count.set_label(&if query.text.is_empty() {
            String::new()
        } else {
            trn("{n} match", "{n} matches", found.len() as u64)
                .replace("{n}", &found.len().to_string())
        });
        let document = editor.document();
        for piece in found.iter().take(MAX_RESULTS) {
            let text = match piece.story {
                None => document.body().get(piece.index).map(|p| p.text()),
                Some(story) => document
                    .story_paragraphs(story)
                    .get(piece.index)
                    .map(|p| p.text()),
            }
            .unwrap_or_default();
            let label = gtk::Label::builder()
                .label(context_markup(&text, piece.range.clone()))
                .use_markup(true)
                .wrap(true)
                .xalign(0.0)
                .build();
            let page = editor.page_of(piece).map_or(String::new(), |page| {
                tr("Page {n}").replace("{n}", &(page + 1).to_string())
            });
            let page = gtk::Label::builder()
                .label(page)
                .xalign(0.0)
                .css_classes(["dim-label", "caption"])
                .build();
            let column = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(2)
                .margin_top(6)
                .margin_bottom(6)
                .build();
            column.append(&page);
            column.append(&label);
            self.results.append(&column);
        }
        *self.found.borrow_mut() = found;
        // Find Next goes on from this search.
        if !query.text.is_empty() {
            *self.search.query.borrow_mut() = query;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::NodeId;

    fn found(index: usize, start: usize, end: usize) -> Found {
        Found {
            paragraph: NodeId::new(1).expect("not zero"),
            index,
            range: start..end,
            story: None,
        }
    }

    #[test]
    fn txt_12_next_and_previous_go_round() {
        let note = desk_model::Story::Note(NodeId::new(9).expect("not zero"));
        let all = [
            found(0, 2, 4),
            found(0, 8, 10),
            found(3, 0, 2),
            Found {
                story: Some(note),
                ..found(0, 0, 2)
            },
        ];
        let rank = |story: Option<desk_model::Story>| usize::from(story.is_some());
        let at = |f: &Found| {
            let r = rank(f.story);
            ((r, f.index, f.range.start), (r, f.index, f.range.end))
        };
        let next = |from, backwards| next_match(&all, from, backwards, &rank);
        // From the cursor at the start.
        assert_eq!(next(((0, 0, 0), (0, 0, 0)), false), Some(&all[0]));
        // From a match, the next one.
        assert_eq!(next(at(&all[0]), false), Some(&all[1]));
        // From the last, round to the first.
        assert_eq!(next(at(&all[3]), false), Some(&all[0]));
        assert_eq!(next(at(&all[1]), true), Some(&all[0]));
        assert_eq!(next(at(&all[0]), true), Some(&all[3]));
        assert_eq!(next_match(&[], ((0, 0, 0), (0, 0, 0)), false, &rank), None);
        // A match in a note comes after those in the body, even at the
        // first paragraph of the note.
        assert_eq!(next(at(&all[2]), false), Some(&all[3]));
        assert_eq!(next(((1, 0, 0), (1, 0, 0)), true), Some(&all[2]));
    }

    #[test]
    fn txt_13_results_show_their_context() {
        assert_eq!(context_markup("a <cat> b", 3..6), "a &lt;<b>cat</b>&gt; b");
        let long = "x".repeat(40) + "cat" + &"y".repeat(40);
        let markup = context_markup(&long, 40..43);
        assert!(markup.starts_with('\u{2026}'));
        assert!(markup.ends_with('\u{2026}'));
        assert!(markup.contains("<b>cat</b>"));
    }

    #[test]
    fn txt_13_recent_searches_keep_ten() {
        let search = Search::default();
        for n in 0..12 {
            search.remember(&n.to_string());
        }
        search.remember("5");
        let recent = search.recent.borrow();
        assert_eq!(recent.len(), RECENT);
        assert_eq!(recent[0], "5");
        assert_eq!(recent.iter().filter(|t| *t == "5").count(), 1);
    }
}
