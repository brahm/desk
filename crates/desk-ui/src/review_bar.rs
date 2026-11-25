// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bar of comments and changes of Desk Writer (W148, REV-02), under
//! the header bar as window M+ puts the bars of iWork (D-008), as Pages
//! 15.4 shows it under its toolbar: the Comments & Changes pane, Previous
//! and Next, which go through the comments and the tracked changes in the
//! order of the pages, the place of the one selected among them, Comment,
//! Highlight and Track Changes. While changes are tracked, Track Changes
//! gives way to the switch between Paused and On and the menu of the
//! actions: Accept and Reject of the change selected (REV-06), Accept All
//! Changes, Reject All Changes, the views of markup (REV-04), Author Color
//! and Turn Off Tracking. It shows while the document has comments or
//! changes, or tracks them.

use adw::prelude::*;
use gtk::gio;

use crate::editing::{ReviewItem, TextEditor};
use crate::i18n::{tr, trn};

/// The bar, with what changes as the document does.
pub struct ReviewBar {
    revealer: gtk::Revealer,
    label: gtk::Label,
    track: gtk::Button,
    tracking: gtk::Box,
    switch: gtk::Switch,
    state: gtk::Label,
}

impl ReviewBar {
    pub fn new() -> Self {
        let bar = gtk::Box::builder()
            .spacing(6)
            .margin_start(6)
            .margin_end(6)
            .margin_top(3)
            .margin_bottom(3)
            .build();
        bar.add_css_class("toolbar");
        let pane = gtk::ToggleButton::builder()
            .icon_name("desk-comment-symbolic")
            .action_name("win.comments")
            .tooltip_text(tr("Comments & Changes"))
            .build();
        crate::formatting::labelled(&pane, &tr("Comments & Changes"));
        bar.append(&pane);
        let previous = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .action_name("win.previous-review-item")
            .tooltip_text(tr("Previous Comment or Change"))
            .build();
        crate::formatting::labelled(&previous, &tr("Previous Comment or Change"));
        let next = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .action_name("win.next-review-item")
            .tooltip_text(tr("Next Comment or Change"))
            .build();
        crate::formatting::labelled(&next, &tr("Next Comment or Change"));
        let label = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        label.add_css_class("dim-label");
        label.add_css_class("numeric");
        let arrows = gtk::Box::builder().build();
        arrows.add_css_class("linked");
        arrows.append(&previous);
        arrows.append(&next);
        bar.append(&arrows);
        bar.append(&label);
        let comment = gtk::Button::builder()
            .label(tr("Co_mment"))
            .use_underline(true)
            .action_name("win.insert-comment")
            .build();
        let highlight = gtk::Button::builder()
            .label(tr("_Highlight"))
            .use_underline(true)
            .action_name("win.insert-highlight")
            .build();
        bar.append(&comment);
        bar.append(&highlight);
        let track = gtk::Button::builder()
            .label(tr("_Track Changes"))
            .use_underline(true)
            .action_name("win.track-changes")
            .build();
        bar.append(&track);
        // While changes are tracked: Paused or On, and the actions.
        let state = gtk::Label::new(None);
        let switch = gtk::Switch::builder()
            .action_name("win.tracking-on")
            .valign(gtk::Align::Center)
            .tooltip_text(tr("Record Changes"))
            .build();
        crate::formatting::labelled(&switch, &tr("Record Changes"));
        let actions = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .menu_model(&actions_menu())
            .tooltip_text(tr("Tracking Actions"))
            .build();
        crate::formatting::labelled(&actions, &tr("Tracking Actions"));
        let tracking = gtk::Box::builder().spacing(6).visible(false).build();
        tracking.append(&state);
        tracking.append(&switch);
        tracking.append(&actions);
        bar.append(&tracking);
        let revealer = gtk::Revealer::builder()
            .child(&bar)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .build();
        Self {
            revealer,
            label,
            track,
            tracking,
            switch,
            state,
        }
    }

    pub fn widget(&self) -> &gtk::Revealer {
        &self.revealer
    }

    /// Shows the bar for a document, or hides it without one.
    pub fn show(&self, editor: Option<&TextEditor>) {
        let Some(editor) = editor else {
            self.revealer.set_reveal_child(false);
            return;
        };
        let tracking = editor.tracking();
        let (current, total) = editor.review_position();
        let tracked = tracking != desk_model::Tracking::Off;
        self.revealer.set_reveal_child(tracked || total > 0);
        self.label.set_label(&position_label(current, total));
        self.track.set_visible(!tracked);
        self.tracking.set_visible(tracked);
        let on = tracking == desk_model::Tracking::On;
        self.state
            .set_label(&if on { tr("On") } else { tr("Paused") });
        if self.switch.is_active() != on {
            self.switch.set_active(on);
        }
    }
}

/// What the label of the bar says: which comment or change is selected,
/// of how many, or how many there are.
pub fn position_label(current: Option<(usize, ReviewItem)>, total: usize) -> String {
    let text = match current {
        Some((_, ReviewItem::Comment(_))) => tr("Comment {i} of {n}"),
        Some((_, ReviewItem::Change(_))) => tr("Change {i} of {n}"),
        None if total == 0 => tr("No Comments or Changes"),
        None => trn(
            "{n} Comment or Change",
            "{n} Comments & Changes",
            total as u64,
        ),
    };
    text.replace("{i}", &current.map_or(0, |(i, _)| i).to_string())
        .replace("{n}", &total.to_string())
}

/// The menu of the actions of tracking.
fn actions_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let one = gio::Menu::new();
    one.append(Some(&tr("Acc_ept Change")), Some("win.accept-change"));
    one.append(Some(&tr("Re_ject Change")), Some("win.reject-change"));
    menu.append_section(None, &one);
    let all = gio::Menu::new();
    all.append(
        Some(&tr("_Accept All Changes")),
        Some("win.accept-all-changes"),
    );
    all.append(
        Some(&tr("_Reject All Changes")),
        Some("win.reject-all-changes"),
    );
    menu.append_section(None, &all);
    // Save a Copy as Final of the action menu of Pages '09 (REV-07).
    let copy = gio::Menu::new();
    copy.append(Some(&tr("_Save a Copy as Final…")), Some("win.save-final"));
    menu.append_section(None, &copy);
    let colors = gio::Menu::new();
    for color in desk_model::AuthorColor::ALL {
        colors.append(
            Some(&crate::window::author_color_name(color)),
            Some(&format!("win.author-color('{}')", color.key())),
        );
    }
    menu.append_section(None, &crate::window::markup_menu());
    let author = gio::Menu::new();
    author.append_submenu(Some(&tr("Author _Color")), &colors);
    menu.append_section(None, &author);
    let off = gio::Menu::new();
    off.append(Some(&tr("Turn _Off Tracking")), Some("win.track-changes"));
    menu.append_section(None, &off);
    menu
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rev_02_the_label_says_which_comment_or_change_of_how_many() {
        let id = desk_model::NodeId::new(7).unwrap();
        assert_eq!(
            position_label(Some((5, ReviewItem::Comment(id))), 9),
            "Comment 5 of 9"
        );
        assert_eq!(
            position_label(Some((2, ReviewItem::Change(id))), 9),
            "Change 2 of 9"
        );
        assert_eq!(position_label(None, 9), "9 Comments & Changes");
        assert_eq!(position_label(None, 1), "1 Comment or Change");
        assert_eq!(position_label(None, 0), "No Comments or Changes");
    }
}
