// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bubble of a tracked change (W148, REV-04), as Pages 15.4 shows
//! it at the change that Previous or Next of the bar of comments and
//! changes selects: a dot in the colour of its author, the author and
//! when, what it did to which text, and the arrows to the change before
//! and after, and Accept and Reject. It stays while the change is
//! selected, and the keyboard stays with the document.

use std::cell::Cell;

use gtk::glib;
use gtk::prelude::*;

use crate::editing::ChangeBubble;
use crate::i18n::{tr, trn};
use crate::text_nav::SpanRect;
use crate::view::DocumentView;

/// The popover of the bubble, and the change it shows.
pub struct ChangeBubbleView {
    popover: gtk::Popover,
    content: gtk::Box,
    shown: Cell<Option<(desk_model::NodeId, SpanRect)>>,
    /// What it shows, so that the same bubble is not built again.
    current: std::cell::RefCell<Option<ChangeBubble>>,
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
            current: std::cell::RefCell::new(None),
        }
    }

    /// The change the bubble shows, if it shows.
    pub fn shown(&self) -> Option<desk_model::NodeId> {
        self.shown.get().map(|(id, _)| id)
    }

    /// Shows the bubble of a change at its text, built again only when
    /// what it says changed.
    pub fn show(&self, bubble: &ChangeBubble, view: &DocumentView) {
        self.shown.set(Some((bubble.id, bubble.rect)));
        if self.current.borrow().as_ref() == Some(bubble) {
            self.place(view);
            return;
        }
        *self.current.borrow_mut() = Some(bubble.clone());
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
        // Accept and Reject of this change (REV-06).
        let accept = gtk::Button::builder()
            .label(tr("_Accept"))
            .use_underline(true)
            .action_name("win.accept-change")
            .can_focus(false)
            .build();
        accept.add_css_class("suggested-action");
        let reject = gtk::Button::builder()
            .label(tr("_Reject"))
            .use_underline(true)
            .action_name("win.reject-change")
            .can_focus(false)
            .build();
        let buttons = gtk::Box::builder()
            .spacing(6)
            .halign(gtk::Align::End)
            .margin_top(4)
            .build();
        buttons.append(&reject);
        buttons.append(&accept);
        let label = gtk::Label::builder()
            .use_markup(true)
            .label(what(bubble))
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .max_width_chars(36)
            .build();
        self.content.append(&label);
        self.content.append(&buttons);
        self.place(view);
    }

    /// Hides the bubble.
    pub fn hide(&self) {
        self.current.take();
        if self.shown.take().is_some() {
            self.popover.popdown();
        }
    }

    /// Takes the bubble off the pages, when the window closes.
    pub fn unparent(&self) {
        self.hide();
        self.popover.unparent();
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
            // Its change is not on the pages shown.
            self.popover.popdown();
            return;
        };
        let rect = gtk::gdk::Rectangle::new(
            x0 as i32,
            y0 as i32,
            ((x1 - x0) as i32).max(1),
            ((y1 - y0) as i32).max(1),
        );
        self.popover.set_pointing_to(Some(&rect));
        if !self.popover.is_visible() {
            self.popover.popup();
        }
    }
}

/// What a change did to which text, as the bubble says it, in Pango
/// markup.
pub fn what(bubble: &ChangeBubble) -> String {
    let label = match bubble.kind {
        desk_model::ChangeKind::Insertion => tr("Added:"),
        desk_model::ChangeKind::Deletion => tr("Deleted:"),
        desk_model::ChangeKind::Format => tr("Formatted:"),
        desk_model::ChangeKind::Placement => tr("Moved:"),
        desk_model::ChangeKind::Cell => tr("Changed:"),
    };
    let text = if !bubble.text.is_empty() {
        tr("“{text}”").replace("{text}", &bubble.text)
    } else if bubble.kind == desk_model::ChangeKind::Format {
        // The formatting of paragraphs, which hold no text of their own.
        trn(
            "{n} Paragraph",
            "{n} Paragraphs",
            bubble.paragraphs.max(1) as u64,
        )
        .replace("{n}", &bubble.paragraphs.max(1).to_string())
    } else if bubble.kind == desk_model::ChangeKind::Cell {
        tr("Cell")
    } else if bubble.object {
        tr("Object")
    } else {
        tr("Paragraph Break")
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
