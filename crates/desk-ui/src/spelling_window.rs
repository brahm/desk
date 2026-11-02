// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Spelling and Grammar window (TXT-10, TXT-11): the mistake at the
//! selection, what is wrong with it and how to fix it, with the actions of
//! the Spelling window of the Mac: Change, Find Next, Ignore, Learn and
//! Forget.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::formatting::{EditorOf, labelled};
use crate::i18n::tr;
use crate::spelling::{self, Mistake, MistakeKind};

/// The Spelling and Grammar window of a document window.
pub struct SpellingWindow {
    window: adw::Window,
    description: gtk::Label,
    language: gtk::Label,
    entry: gtk::Entry,
    guesses: gtk::ListBox,
    grammar: gtk::CheckButton,
    forget: gtk::Button,
    /// The mistake shown, and the word it is.
    shown: RefCell<Option<(Mistake, String)>>,
    /// The guesses listed.
    suggestions: RefCell<Vec<String>>,
    editor: EditorOf,
}

impl SpellingWindow {
    /// The window, over a document window.
    pub fn new(parent: &impl IsA<gtk::Window>, editor: EditorOf) -> Rc<Self> {
        let description = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["heading"])
            .build();
        description.set_accessible_role(gtk::AccessibleRole::Status);
        let language = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();
        let entry = gtk::Entry::builder().hexpand(true).build();
        labelled(&entry, &tr("Correction"));
        let guesses = gtk::ListBox::builder()
            .css_classes(["boxed-list"])
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        labelled(&guesses, &tr("Guesses"));
        let scroll = gtk::ScrolledWindow::builder()
            .child(&guesses)
            .min_content_height(160)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let grammar = gtk::CheckButton::with_label(&tr("Check Grammar"));
        grammar.set_active(crate::preferences::get().proofread);

        let button = |label: &str| {
            gtk::Button::builder()
                .label(label)
                .use_underline(true)
                .build()
        };
        let change = button(&tr("_Change"));
        change.add_css_class("suggested-action");
        let next = button(&tr("_Find Next"));
        let ignore = button(&tr("_Ignore"));
        let learn = button(&tr("_Learn"));
        let forget = button(&tr("F_orget"));
        let actions = gtk::Box::builder().spacing(6).homogeneous(true).build();
        for b in [&change, &next, &ignore, &learn, &forget] {
            actions.append(b);
        }

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        content.append(&description);
        content.append(&language);
        content.append(&entry);
        content.append(&scroll);
        content.append(&grammar);
        content.append(&actions);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::new());
        view.set_content(Some(&content));
        let window = adw::Window::builder()
            .title(tr("Spelling and Grammar"))
            .transient_for(parent)
            .hide_on_close(true)
            .default_width(440)
            .content(&view)
            .build();

        let this = Rc::new(Self {
            window,
            description,
            language,
            entry,
            guesses,
            grammar,
            forget,
            shown: RefCell::new(None),
            suggestions: RefCell::new(Vec::new()),
            editor,
        });
        let weak = Rc::downgrade(&this);
        this.guesses.connect_row_selected(move |_, row| {
            let (Some(this), Some(row)) = (weak.upgrade(), row) else {
                return;
            };
            let suggestion = this.suggestions.borrow().get(row.index() as usize).cloned();
            if let Some(suggestion) = suggestion {
                this.entry.set_text(&suggestion);
            }
        });
        let weak = Rc::downgrade(&this);
        this.guesses.connect_row_activated(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.change();
            }
        });
        let weak = Rc::downgrade(&this);
        this.entry.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.change();
            }
        });
        let connect = |button: &gtk::Button, run: fn(&SpellingWindow)| {
            let weak = Rc::downgrade(&this);
            button.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    run(&this);
                }
            });
        };
        connect(&change, SpellingWindow::change);
        connect(&next, SpellingWindow::find_next);
        connect(&ignore, |this| {
            if let Some((mistake, word)) = this.shown.borrow().clone()
                && matches!(mistake.kind, MistakeKind::Spelling { .. })
            {
                spelling::ignore(&word);
            }
            this.refresh_marks();
            this.find_next();
        });
        connect(&learn, |this| {
            if let Some((_, word)) = this.shown.borrow().clone() {
                spelling::learn(&word);
            }
            this.refresh_marks();
            this.find_next();
        });
        connect(&this.forget, |this| {
            let word = this.entry.text().to_string();
            spelling::forget(&word);
            this.forget.set_sensitive(false);
            this.refresh_marks();
        });
        this
    }

    fn refresh_marks(&self) {
        if let Some(editor) = (self.editor)() {
            editor.update_squiggles();
        }
    }

    /// Shows the window with the mistake at the selection, or the next.
    pub fn present(&self) {
        self.window.present();
        let grammar = self.grammar.is_active();
        let Some(editor) = (self.editor)() else {
            return;
        };
        match editor.selected_mistake(true, grammar) {
            Some((mistake, text)) => self.show(Some((mistake, text))),
            None => self.find_next(),
        }
    }

    fn find_next(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let found = editor.next_mistake(true, self.grammar.is_active());
        self.show(found);
    }

    fn change(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        if self.shown.borrow().is_some() {
            editor.correct(&self.entry.text());
        }
        self.find_next();
    }

    /// Shows a mistake, or that there are none.
    fn show(&self, found: Option<(Mistake, String)>) {
        while let Some(row) = self.guesses.row_at_index(0) {
            self.guesses.remove(&row);
        }
        let Some((mistake, text)) = found else {
            *self.shown.borrow_mut() = None;
            self.description.set_label(&tr("No mistakes found"));
            self.language.set_label("");
            self.entry.set_text("");
            self.forget.set_sensitive(false);
            return;
        };
        let word = text[mistake.range.clone()].to_owned();
        self.description.set_label(&mistake.description());
        self.language.set_label(&match &mistake.kind {
            MistakeKind::Spelling { language } => {
                tr("Language: {language}").replace("{language}", language)
            }
            MistakeKind::Proofreading(_) => String::new(),
        });
        let suggestions = mistake.suggestions(&text);
        self.entry
            .set_text(suggestions.first().map_or(word.as_str(), String::as_str));
        for suggestion in &suggestions {
            let label = gtk::Label::builder()
                .label(if suggestion.is_empty() {
                    tr("(Delete)")
                } else {
                    suggestion.clone()
                })
                .xalign(0.0)
                .margin_top(6)
                .margin_bottom(6)
                .margin_start(6)
                .build();
            self.guesses.append(&label);
        }
        self.forget.set_sensitive(spelling::is_learned(&word));
        *self.suggestions.borrow_mut() = suggestions;
        *self.shown.borrow_mut() = Some((mistake, word));
    }
}

/// Makes the window when it is first shown.
#[derive(Default)]
pub struct SpellingSlot(RefCell<Option<Rc<SpellingWindow>>>);

impl SpellingSlot {
    /// Shows the window, making it the first time.
    pub fn present(&self, parent: &impl IsA<gtk::Window>, editor: EditorOf) {
        let window = self
            .0
            .borrow_mut()
            .get_or_insert_with(|| SpellingWindow::new(parent, editor))
            .clone();
        window.present();
    }
}
