// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Comments page of the navigation sidebar of Desk Writer (W149,
//! REV-08), as the Comments & Changes pane of Pages 15.4 shows the
//! comments: a card for each, in the order of the text, with its author,
//! when it was written, the text it is on, its text and its replies. A
//! card is written, edited, answered and deleted in place; a click on it
//! selects the text it is on, and the card of the comment at the cursor is
//! the selected one.
//!
//! The text of a comment is plain, as in Pages, whose comments take no
//! styles. A comment left empty is not added, and one emptied is deleted,
//! as in Pages 15.4; a highlight shows as a card without text, and text
//! typed in it makes it a comment (PXV-19). The name of the author is that
//! of the user until the Author preference comes, with step 7 of the phase
//! W2 (W150).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use desk_model::{Comment, NodeId};
use gtk::glib;

use crate::editing::CommentTarget;
use crate::formatting::{EditorOf, labelled};
use crate::i18n::tr;
#[cfg(test)]
use crate::text_nav::Spot;

/// The name that new comments and replies are written under: the one of
/// the preferences (W150), or else the full name of the user, or the login
/// when there is none.
pub fn author() -> String {
    author_of(&crate::preferences::get())
}

/// The name that new comments are written under with some preferences.
pub fn author_of(preferences: &crate::preferences::Preferences) -> String {
    let chosen = preferences.author.clone();
    if !chosen.is_empty() {
        return chosen;
    }
    let name = glib::real_name().to_string_lossy().trim().to_owned();
    if name.is_empty() || name == "Unknown" {
        glib::user_name().to_string_lossy().into_owned()
    } else {
        name
    }
}

/// The names that comments and changes show under: the current name of
/// the author of this computer for their earlier names, as Pages 15.4
/// shows them (B-066), and the others as they are.
pub struct ShownAuthors {
    current: String,
    former: Vec<String>,
}

impl ShownAuthors {
    /// As the preferences say now.
    pub fn now() -> Self {
        let preferences = crate::preferences::get();
        Self {
            current: author_of(&preferences),
            former: preferences.former_authors,
        }
    }

    /// The name that something written under `name` shows under.
    pub fn name<'a>(&'a self, name: &'a str) -> &'a str {
        if self.former.iter().any(|n| n == name) {
            &self.current
        } else {
            name
        }
    }
}

/// The colours of the authors of a document, with the author of this
/// computer under their name now and the earlier ones (W150, B-066).
pub fn palette(document: &desk_model::Document) -> desk_model::AuthorPalette {
    document.author_palette_of(&author(), &crate::preferences::get().former_authors)
}

/// Now, as comments keep when they were written: a date and time of ISO
/// 8601, local, to the second.
pub fn now() -> String {
    glib::DateTime::now_local()
        .and_then(|now| now.format("%FT%T"))
        .map(|text| text.to_string())
        .unwrap_or_default()
}

/// When a comment was written, as its card shows it beside `now`: the
/// time today or yesterday, the day this year, the date before.
pub fn shown_date(date: &str, now: &glib::DateTime) -> String {
    let local = glib::TimeZone::local();
    let Ok(date) = glib::DateTime::from_iso8601(date, Some(&local)) else {
        return String::new();
    };
    let Ok(date) = date.to_timezone(&now.timezone()) else {
        return String::new();
    };
    let day = |d: &glib::DateTime| d.ymd();
    let format = |pattern: &str| {
        date.format(pattern)
            .map(|t| t.trim().to_owned())
            .unwrap_or_default()
    };
    if day(&date) == day(now) {
        tr("Today at {time}").replace("{time}", &format("%R"))
    } else if now.add_days(-1).is_ok_and(|y| day(&y) == day(&date)) {
        tr("Yesterday at {time}").replace("{time}", &format("%R"))
    } else if date.year() == now.year() {
        format("%e %B")
    } else {
        format("%e %B %Y")
    }
}

/// What a card writes when its text is done.
type Done = Box<dyn Fn(&str)>;

/// The page of the sidebar.
pub struct CommentsPage {
    root: gtk::Box,
    list: gtk::ListBox,
    empty: adw::StatusPage,
    editor: EditorOf,
    /// The comments of the cards, in order, after the new one.
    rows: RefCell<Vec<NodeId>>,
    /// The text that a new comment being written is on.
    draft: RefCell<Option<CommentTarget>>,
    /// Whether text is being written in a card, which making the page
    /// again would lose, and whether it is due once that ends.
    writing: Cell<bool>,
    due: Cell<bool>,
    shown: Cell<bool>,
    pending: Cell<bool>,
    /// Whether it is the bubble of one comment at its text, rather than
    /// the page of the sidebar, and the comment it shows.
    bubble: bool,
    only: Cell<Option<NodeId>>,
    /// What to call when the bubble has nothing left to show.
    on_empty: RefCell<Option<Rc<dyn Fn()>>>,
}

impl CommentsPage {
    /// The page of the sidebar.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        Self::build(editor, false)
    }

    /// The bubble of a comment, which shows at its text as in Pages 15.4:
    /// the card of the comment, and the buttons that go to the comment
    /// before and after it.
    pub fn bubble(editor: EditorOf) -> Rc<Self> {
        Self::build(editor, true)
    }

    fn build(editor: EditorOf, bubble: bool) -> Rc<Self> {
        let list = gtk::ListBox::builder()
            .selection_mode(if bubble {
                gtk::SelectionMode::None
            } else {
                gtk::SelectionMode::Single
            })
            .css_classes(if bubble {
                ["desk-comment-bubble", "background"]
            } else {
                ["boxed-list-separate", "desk-comments"]
            })
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        list.update_property(&[gtk::accessible::Property::Label(&if bubble {
            tr("Comment")
        } else {
            tr("Comments")
        })]);
        let empty = adw::StatusPage::builder()
            .icon_name("desk-comment-symbolic")
            .title(tr("No Comments"))
            .description(tr("Select text and choose Comment to add one."))
            .css_classes(["compact"])
            .vexpand(true)
            .build();
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        if bubble {
            // As tall as its card, up to a size that a screen of 720
            // pixels shows whole.
            scrolled.set_propagate_natural_height(true);
            scrolled.set_max_content_height(420);
            scrolled.set_width_request(300);
            let previous = icon_button("go-previous-symbolic", &tr("Previous Comment"));
            previous.set_action_name(Some("win.previous-comment"));
            let next = icon_button("go-next-symbolic", &tr("Next Comment"));
            next.set_action_name(Some("win.next-comment"));
            let buttons = gtk::Box::builder()
                .spacing(6)
                .halign(gtk::Align::End)
                .margin_end(6)
                .margin_bottom(6)
                .build();
            buttons.append(&previous);
            buttons.append(&next);
            root.append(&scrolled);
            root.append(&buttons);
        }
        let add = gtk::Button::builder()
            .label(tr("_Add Comment"))
            .use_underline(true)
            .action_name("win.insert-comment")
            .tooltip_text(tr("Comment on the selection, or on the word at the cursor"))
            .build();
        let buttons = gtk::Box::builder()
            .homogeneous(true)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        buttons.append(&add);
        if !bubble {
            root.append(&scrolled);
            root.append(&empty);
            root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            root.append(&buttons);
        }
        let this = Rc::new(Self {
            root,
            list,
            empty,
            editor,
            rows: RefCell::new(Vec::new()),
            draft: RefCell::new(None),
            writing: Cell::new(false),
            due: Cell::new(false),
            shown: Cell::new(false),
            pending: Cell::new(false),
            bubble,
            only: Cell::new(None),
            on_empty: RefCell::new(None),
        });
        let weak = Rc::downgrade(&this);
        this.list.connect_row_activated(move |_, row| {
            let Some(this) = weak.upgrade() else { return };
            let id = usize::try_from(row.index())
                .ok()
                .and_then(|i| this.rows.borrow().get(i).copied());
            if let (Some(editor), Some(id)) = ((this.editor)(), id) {
                editor.select_comment(id);
            }
        });
        this.rebuild();
        this
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Shows or hides it; it is made only while it shows.
    pub fn set_shown(self: &Rc<Self>, shown: bool) {
        self.shown.set(shown);
        if shown {
            self.rebuild();
        } else if self.draft.take().is_some() {
            self.writing.set(false);
        }
    }

    /// Makes it again soon, after the document changed; once the text
    /// being written is done, while it is.
    pub fn refresh(self: &Rc<Self>) {
        if !self.shown.get() {
            return;
        }
        if self.writing.get() {
            self.due.set(true);
            return;
        }
        if self.pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            if let Some(this) = weak.upgrade() {
                this.pending.set(false);
                if !this.writing.get() {
                    this.rebuild();
                }
            }
        });
    }

    /// Shows a comment, in the bubble.
    pub fn show_comment(self: &Rc<Self>, id: NodeId) {
        self.only.set(Some(id));
        self.draft.take();
        self.rebuild();
    }

    /// Calls `f` when the bubble has nothing left to show: its comment was
    /// deleted, or the new one was not written.
    pub fn connect_empty(&self, f: impl Fn() + 'static) {
        *self.on_empty.borrow_mut() = Some(Rc::new(f));
    }

    /// Starts a new comment on the text between two spots or on an
    /// object: its card, at the top, takes the keyboard.
    pub fn start_comment(self: &Rc<Self>, range: CommentTarget) {
        self.only.set(None);
        *self.draft.borrow_mut() = Some(range);
        self.writing.set(false);
        self.rebuild();
    }

    /// Selects the card of a comment, as the cursor goes in its text.
    pub fn show_current(&self, id: Option<NodeId>) {
        let index = id.and_then(|id| self.rows.borrow().iter().position(|r| *r == id));
        let row = index
            .and_then(|i| i32::try_from(i).ok())
            .and_then(|i| self.list.row_at_index(i));
        match row {
            Some(row) => {
                if !row.is_selected() {
                    self.list.select_row(Some(&row));
                }
            }
            None => self.list.unselect_all(),
        }
    }

    fn rebuild(self: &Rc<Self>) {
        self.list.remove_all();
        self.writing.set(false);
        self.due.set(false);
        let Some(editor) = (self.editor)() else {
            return;
        };
        let document = editor.document();
        let now = glib::DateTime::now_local().ok();
        let palette = palette(&document);
        let authors = ShownAuthors::now();
        let mut rows = Vec::new();
        let draft = *self.draft.borrow();
        if let Some(range) = draft {
            self.list.append(&self.draft_card(range));
            rows.push(NodeId::new(u64::MAX).expect("not zero"));
        }
        for (id, on) in editor.comments_in_order() {
            if self.bubble && self.only.get() != Some(id) {
                continue;
            }
            let Some(comment) = document.comments().get(id) else {
                continue;
            };
            self.list
                .append(&self.card(comment, &on, now.as_ref(), &palette, &authors));
            rows.push(id);
        }
        let any = !rows.is_empty();
        *self.rows.borrow_mut() = rows;
        if self.bubble {
            if !any {
                let empty = self.on_empty.borrow().clone();
                if let Some(empty) = empty {
                    empty();
                }
            }
            return;
        }
        self.list.set_visible(any);
        self.empty.set_visible(!any);
        if draft.is_none() {
            self.show_current(editor.comment_at_cursor());
        }
    }

    /// The card of a new comment, with its text being written.
    fn draft_card(self: &Rc<Self>, range: CommentTarget) -> gtk::ListBoxRow {
        let card = card_box();
        let color = (self.editor)().map(|e| e.local_author_color());
        card.append(&heading(&author(), color, None, &[]));
        let weak = Rc::downgrade(self);
        let writer = self.writer(
            &card,
            None,
            "",
            &tr("Comment"),
            Box::new(move |text| {
                let Some(this) = weak.upgrade() else { return };
                this.draft.take();
                if let Some(editor) = (this.editor)() {
                    let added = editor.add_comment_on(range, &author(), text);
                    this.only.set(added);
                    this.rebuild();
                    this.show_current(added);
                    editor.focus();
                }
            }),
        );
        let row = gtk::ListBoxRow::builder()
            .child(&card)
            .activatable(false)
            .build();
        row.update_property(&[gtk::accessible::Property::Label(&tr("New Comment"))]);
        // The keyboard goes to the text once the card shows.
        glib::idle_add_local_once(move || {
            writer.grab_focus();
        });
        row
    }

    /// The card of a comment and its replies.
    fn card(
        self: &Rc<Self>,
        comment: &Comment,
        on: &str,
        now: Option<&glib::DateTime>,
        palette: &desk_model::AuthorPalette,
        authors: &ShownAuthors,
    ) -> gtk::ListBoxRow {
        let id = comment.id;
        let card = card_box();
        let date = |d: &Option<std::sync::Arc<str>>| {
            d.as_deref().zip(now).map(|(d, now)| shown_date(d, now))
        };
        // Under the name the author has now (B-066).
        let color_of = |name: &str| Some(palette.color(name));
        let comment_author = authors.name(&comment.author);
        let text = text_label(&comment.text, comment.is_highlight());
        let edit = icon_button("document-edit-symbolic", &tr("Edit Comment"));
        let delete = icon_button("user-trash-symbolic", &tr("Delete Comment"));
        card.append(&heading(
            comment_author,
            color_of(&comment.author),
            date(&comment.date).as_deref(),
            &[&edit, &delete],
        ));
        if !on.is_empty() {
            let quote = gtk::Label::builder()
                .label(on)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .tooltip_text(on)
                .css_classes(["dim-label", "caption", "desk-comment-quote"])
                .build();
            card.append(&quote);
        }
        card.append(&text);
        let weak = Rc::downgrade(self);
        let old = comment.text.to_string();
        edit.connect_clicked(glib::clone!(
            #[weak]
            card,
            #[weak]
            text,
            move |_| {
                let Some(this) = weak.upgrade() else { return };
                let weak = Rc::downgrade(&this);
                this.writer(
                    &card,
                    Some(&text),
                    &old,
                    &tr("Comment"),
                    Box::new(move |typed| {
                        if let Some(editor) = weak.upgrade().and_then(|t| (t.editor)()) {
                            editor.set_comment_text(id, typed);
                        }
                    }),
                );
            }
        ));
        let weak = Rc::downgrade(self);
        delete.connect_clicked(move |_| {
            if let Some(editor) = weak.upgrade().and_then(|t| (t.editor)()) {
                editor.delete_comment(id);
            }
        });
        for (index, reply) in comment.replies.iter().enumerate() {
            let replies = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .margin_start(12)
                .margin_top(6)
                .css_classes(["desk-comment-reply"])
                .build();
            let edit = icon_button("document-edit-symbolic", &tr("Edit Reply"));
            let delete = icon_button("user-trash-symbolic", &tr("Delete Reply"));
            let reply_author = authors.name(&reply.author);
            replies.append(&heading(
                reply_author,
                color_of(&reply.author),
                date(&reply.date).as_deref(),
                &[&edit, &delete],
            ));
            let text = text_label(&reply.text, false);
            replies.append(&text);
            card.append(&replies);
            let weak = Rc::downgrade(self);
            let old = reply.text.to_string();
            edit.connect_clicked(glib::clone!(
                #[weak]
                replies,
                #[weak]
                text,
                move |_| {
                    let Some(this) = weak.upgrade() else { return };
                    let weak = Rc::downgrade(&this);
                    this.writer(
                        &replies,
                        Some(&text),
                        &old,
                        &tr("Reply"),
                        Box::new(move |typed| {
                            if let Some(editor) = weak.upgrade().and_then(|t| (t.editor)()) {
                                editor.set_reply_text(id, index, typed);
                            }
                        }),
                    );
                }
            ));
            let weak = Rc::downgrade(self);
            delete.connect_clicked(move |_| {
                if let Some(editor) = weak.upgrade().and_then(|t| (t.editor)()) {
                    editor.delete_reply(id, index);
                }
            });
        }
        let reply = gtk::Button::builder()
            .label(tr("_Reply"))
            .use_underline(true)
            .halign(gtk::Align::Start)
            .css_classes(["flat"])
            .build();
        card.append(&reply);
        let weak = Rc::downgrade(self);
        reply.connect_clicked(glib::clone!(
            #[weak]
            card,
            move |button| {
                let Some(this) = weak.upgrade() else { return };
                let weak = Rc::downgrade(&this);
                this.writer(
                    &card,
                    Some(button.upcast_ref()),
                    "",
                    &tr("Reply"),
                    Box::new(move |typed| {
                        if let Some(editor) = weak.upgrade().and_then(|t| (t.editor)()) {
                            editor.add_reply(id, &author(), typed);
                        }
                    }),
                );
            }
        ));
        let row = gtk::ListBoxRow::builder().child(&card).build();
        let name = if comment.is_highlight() {
            tr("Highlight by {author}")
        } else {
            tr("Comment by {author}")
        };
        row.update_property(&[gtk::accessible::Property::Label(
            &name.replace("{author}", comment_author),
        )]);
        row
    }

    /// A box to write text in, in a card in place of `replaces`, or at
    /// its end: Done, or Ctrl+Return, gives the text to `done`; Cancel,
    /// or Escape, takes it away. Gives the text view, to focus.
    fn writer(
        self: &Rc<Self>,
        card: &gtk::Box,
        replaces: Option<&gtk::Widget>,
        text: &str,
        name: &str,
        done: Done,
    ) -> gtk::TextView {
        self.writing.set(true);
        let view = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::WordChar)
            .accepts_tab(false)
            .top_margin(6)
            .bottom_margin(6)
            .left_margin(6)
            .right_margin(6)
            .css_classes(["card", "desk-comment-text"])
            .build();
        view.buffer().set_text(text);
        view.update_property(&[gtk::accessible::Property::Label(name)]);
        let cancel = gtk::Button::with_mnemonic(&tr("_Cancel"));
        let finish = gtk::Button::builder()
            .label(tr("_Done"))
            .use_underline(true)
            .css_classes(["suggested-action"])
            .build();
        let buttons = gtk::Box::builder()
            .spacing(6)
            .halign(gtk::Align::End)
            .build();
        buttons.append(&cancel);
        buttons.append(&finish);
        let writer = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        writer.append(&view);
        writer.append(&buttons);
        match replaces {
            Some(widget) => {
                widget.set_visible(false);
                card.insert_child_after(&writer, Some(widget));
            }
            None => card.append(&writer),
        }
        let done = Rc::new(done);
        let weak = Rc::downgrade(self);
        let finish_text = glib::clone!(
            #[weak]
            view,
            #[strong]
            done,
            move || {
                let buffer = view.buffer();
                let (start, end) = buffer.bounds();
                let text = buffer.text(&start, &end, false).to_string();
                if let Some(this) = weak.upgrade() {
                    this.writing.set(false);
                }
                done(&text);
                if let Some(this) = weak.upgrade() {
                    this.rebuild();
                }
            }
        );
        let weak = Rc::downgrade(self);
        let replaced = replaces.map(|w| w.downgrade());
        let cancel_text = glib::clone!(
            #[weak]
            writer,
            #[weak]
            card,
            move || {
                let Some(this) = weak.upgrade() else { return };
                this.writing.set(false);
                if this.draft.take().is_some() || this.due.get() {
                    this.rebuild();
                    if let Some(editor) = (this.editor)() {
                        editor.focus();
                    }
                    return;
                }
                card.remove(&writer);
                if let Some(widget) = replaced.as_ref().and_then(|w| w.upgrade()) {
                    widget.set_visible(true);
                }
            }
        );
        let on_finish = finish_text.clone();
        finish.connect_clicked(move |_| on_finish());
        let on_cancel = cancel_text.clone();
        cancel.connect_clicked(move |_| on_cancel());
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gtk::gdk::Key::Escape {
                cancel_text();
                glib::Propagation::Stop
            } else if matches!(key, gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter)
                && modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            {
                finish_text();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        view.add_controller(keys);
        view.grab_focus();
        view
    }
}

/// The box of a card.
fn card_box() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_start(10)
        .margin_end(6)
        .margin_top(8)
        .margin_bottom(8)
        .build()
}

/// The line of a card that names who wrote it and when, with its buttons.
/// The heading of a comment or a reply: a dot in the colour of its author
/// (W150), the author and the date, and its buttons.
fn heading(
    author: &str,
    color: Option<desk_model::AuthorColor>,
    date: Option<&str>,
    buttons: &[&gtk::Button],
) -> gtk::Box {
    let line = gtk::Box::builder().spacing(6).build();
    if let Some(color) = color {
        let desk_model::Rgba { r, g, b, .. } = color.ink();
        let dot = gtk::Label::builder()
            .use_markup(true)
            .label(format!(
                "<span foreground=\"#{r:02x}{g:02x}{b:02x}\">●</span>"
            ))
            .valign(gtk::Align::Start)
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        line.append(&dot);
    }
    let author = if author.is_empty() {
        tr("Unknown Author")
    } else {
        author.to_owned()
    };
    let name = gtk::Label::builder()
        .label(&author)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["heading"])
        .build();
    let texts = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .build();
    texts.append(&name);
    if let Some(date) = date.filter(|d| !d.is_empty()) {
        texts.append(
            &gtk::Label::builder()
                .label(date)
                .xalign(0.0)
                .css_classes(["dim-label", "caption"])
                .build(),
        );
    }
    line.append(&texts);
    for button in buttons {
        line.append(*button);
    }
    line
}

/// The text of a comment or a reply; a highlight has none, and says so.
fn text_label(text: &str, highlight: bool) -> gtk::Widget {
    let label = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build();
    if highlight {
        label.set_label(&tr("Highlight"));
        label.add_css_class("dim-label");
    } else {
        label.set_label(text);
    }
    label.upcast()
}

/// A flat icon button of a card.
fn icon_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .valign(gtk::Align::Start)
        .tooltip_text(label)
        .css_classes(["flat", "circular"])
        .build();
    labelled(&button, label);
    button
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::TextEditor;
    use desk_model::Document;

    fn one_two_three() -> TextEditor {
        let mut document = Document::new_word_processing();
        document
            .transact("Typing", |edit| {
                let start = edit.document().start();
                edit.insert_text(start, "one two three").map(|_| ())
            })
            .unwrap();
        TextEditor::new(&crate::view::DocumentView::new(), document)
    }

    #[test]
    fn w149_comments_are_written_answered_and_deleted_in_the_window() {
        crate::with_gtk(|| {
            let editor = one_two_three();
            let comment = |editor: &TextEditor| {
                let document = editor.document();
                document.comments().iter().next().cloned()
            };
            // At the cursor, a comment is on the word there.
            editor.go_to_paragraph(0);
            let range = editor.comment_range().unwrap();
            assert_eq!(range, (Spot::new(0, 0), Spot::new(0, 3)));
            // Left empty, it is not added.
            assert!(editor.add_comment(range, "Ana", " \n").is_none());
            assert!(comment(&editor).is_none());
            let id = editor.add_comment(range, "Ana", "Why?\n").unwrap();
            let added = comment(&editor).unwrap();
            assert_eq!((&*added.author, &*added.text), ("Ana", "Why?"));
            assert!(added.date.is_some());
            // The cursor in its text is in the comment, which the page
            // lists.
            assert_eq!(editor.comment_at_cursor(), Some(id));
            let page = CommentsPage::new({
                let editor = editor.clone();
                Rc::new(move || Some(editor.clone()))
            });
            page.set_shown(true);
            assert_eq!(*page.rows.borrow(), [id]);
            assert!(page.list.selected_row().is_some());
            // Answered, edited, and the reply deleted.
            editor.add_reply(id, "Bo", "Because");
            editor.set_reply_text(id, 0, "Because so");
            editor.set_comment_text(id, "Why not?");
            let edited = comment(&editor).unwrap();
            assert_eq!(&*edited.text, "Why not?");
            assert_eq!(&*edited.replies[0].text, "Because so");
            assert_eq!(&*edited.replies[0].author, "Bo");
            editor.delete_reply(id, 0);
            assert!(comment(&editor).unwrap().replies.is_empty());
            // Next Comment selects its text; there is none after it.
            editor.select_all();
            assert_eq!(editor.go_to_comment(false), Some(id));
            assert_eq!(editor.selection().ordered(), range);
            assert_eq!(editor.go_to_comment(true), None);
            // Emptied, it is deleted, and Undo brings it back.
            editor.set_comment_text(id, "");
            assert!(comment(&editor).is_none());
            editor.undo();
            assert_eq!(comment(&editor).unwrap().id, id);
        });
    }

    #[test]
    fn w149_cards_say_when_comments_were_written() {
        let zone = glib::TimeZone::utc();
        let now = glib::DateTime::new(&zone, 2026, 11, 23, 10, 0, 0.0).unwrap();
        // Dates with their zone are shown in the zone of now.
        let date = |d: &str| shown_date(d, &now);
        assert!(date("2026-11-23T09:30:00Z").contains("09:30"));
        assert!(date("2026-11-22T20:33:34Z").contains("20:33"));
        assert_ne!(date("2026-11-22T20:33:34Z"), date("2026-11-23T20:33:34Z"));
        let earlier = date("2026-03-04T08:00:00Z");
        assert!(
            earlier.starts_with('4') && !earlier.contains("2026"),
            "{earlier}"
        );
        assert!(date("2025-03-04T08:00:00Z").ends_with("2025"));
        assert_eq!(shown_date("not a date", &now), "");
    }
}
