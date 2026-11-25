// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bubble of a tracked change (W148, REV-04), as Pages 15.4 shows
//! it at the change that Previous or Next of the bar of comments and
//! changes selects: a dot in the colour of its author, the author and
//! when, what it did to which text, and the arrows to the change before
//! and after. It stays while the change is selected, and the keyboard
//! stays with the document. Accept and Reject come with step 12 of W2.

use std::cell::Cell;

use gtk::glib;
use gtk::prelude::*;

use crate::editing::ChangeBubble;
use crate::i18n::tr;
use crate::text_nav::SpanRect;
use crate::view::DocumentView;

/// The popover of the bubble, and the change it shows.
pub struct ChangeBubbleView {
    popover: gtk::Popover,
    content: gtk::Box,
    shown: Cell<Option<(desk_model::NodeId, SpanRect)>>,
}

impl ChangeBubbleView {
    /// A bubble over the pages of `view`, hidden.
    pub fn new(view: &DocumentView) -> Self {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .margin_start(10)
            .margin_end(6)
            .margin_top(8)
            .margin_bottom(8)
            .width_request(240)
            .build();
        let popover = gtk::Popover::builder()
            .child(&content)
            .position(gtk::PositionType::Bottom)
            .autohide(false)
            .can_focus(false)
            .css_classes(["desk-comment-popover"])
            .build();
        popover.set_parent(view.area());
        Self {
            popover,
            content,
            shown: Cell::new(None),
        }
    }

    /// The change the bubble shows, if it shows.
    pub fn shown(&self) -> Option<desk_model::NodeId> {
        self.shown.get().map(|(id, _)| id)
    }

    /// Shows the bubble of a change at its text.
    pub fn show(&self, bubble: &ChangeBubble, view: &DocumentView) {
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
        let authors = crate::comments_page::ShownAuthors::now();
        let date = glib::DateTime::now_local().ok().and_then(|now| {
            bubble
                .date
                .as_deref()
                .map(|d| crate::comments_page::shown_date(d, &now))
        });
        let previous = arrow(
            "go-previous-symbolic",
            "win.previous-review-item",
            &tr("Previous Comment or Change"),
        );
        let next = arrow(
            "go-next-symbolic",
            "win.next-review-item",
            &tr("Next Comment or Change"),
        );
        self.content.append(&crate::comments_page::heading(
            authors.name(&bubble.author),
            Some(bubble.color),
            date.as_deref(),
            &[&previous, &next],
        ));
        let label = gtk::Label::builder()
            .use_markup(true)
            .label(what(bubble))
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .max_width_chars(36)
            .build();
        self.content.append(&label);
        self.shown.set(Some((bubble.id, bubble.rect)));
        self.place(view);
        self.popover.popup();
    }

    /// Hides the bubble.
    pub fn hide(&self) {
        if self.shown.take().is_some() {
            self.popover.popdown();
        }
    }

    /// Points the bubble at its change again, after the pages moved.
    pub fn place(&self, view: &DocumentView) {
        let Some((_, at)) = self.shown.get() else {
            return;
        };
        let (Some((x0, y0)), Some((x1, y1))) = (
            view.to_area(at.page, at.x0, at.y0),
            view.to_area(at.page, at.x1, at.y1),
        ) else {
            return;
        };
        let rect = gtk::gdk::Rectangle::new(
            x0 as i32,
            y0 as i32,
            ((x1 - x0) as i32).max(1),
            ((y1 - y0) as i32).max(1),
        );
        self.popover.set_pointing_to(Some(&rect));
    }
}

/// What a change did to which text, as the bubble says it, in Pango
/// markup.
pub fn what(bubble: &ChangeBubble) -> String {
    let label = match bubble.kind {
        desk_model::ChangeKind::Insertion => tr("Added:"),
        desk_model::ChangeKind::Deletion => tr("Deleted:"),
        desk_model::ChangeKind::Format => tr("Formatted:"),
    };
    let text = if bubble.text.is_empty() {
        // The formatting of a paragraph, which holds no text of its own.
        tr("Paragraph")
    } else {
        tr("“{text}”").replace("{text}", &bubble.text)
    };
    format!(
        "<b>{}</b> {}",
        glib::markup_escape_text(&label),
        glib::markup_escape_text(&text)
    )
}

/// A flat arrow of the bubble.
fn arrow(icon: &str, action: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .action_name(action)
        .valign(gtk::Align::Start)
        .tooltip_text(label)
        .can_focus(false)
        .css_classes(["flat", "circular"])
        .build();
    crate::formatting::labelled(&button, label);
    button
}
