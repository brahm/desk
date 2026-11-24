// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Browse Versions, in the main menu (DOC-11, step 5 of window M+): the
//! versions that `crate::versions` kept of the document, by day, newest
//! first. Each can take the place of the document, which keeps what it
//! holds now as a version first, or open as an untitled copy beside it,
//! to take a part from it, as Browse All Versions of iWork 15 allows.
//!
//! macOS shows the versions as a timeline of windows; GNOME has no such
//! view, so the dialog is a list of Libadwaita rows.

use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use super::DocumentWindow;
use crate::i18n::tr;
use crate::versions::{Kind, Store, Version};

/// What a row says of why a version was kept.
fn kind_label(kind: Kind) -> String {
    match kind {
        Kind::Opened => tr("When the document was opened"),
        Kind::Saved => tr("Kept with Save"),
        Kind::Automatic => tr("Kept automatically"),
    }
}

/// The heading of the versions of a day: Today, Yesterday, or the date.
fn day_heading(time: &glib::DateTime, today: &glib::DateTime) -> String {
    let day = |d: &glib::DateTime| (d.year(), d.day_of_year());
    if day(time) == day(today) {
        return tr("Today");
    }
    if today
        .add_days(-1)
        .is_ok_and(|yesterday| day(time) == day(&yesterday))
    {
        return tr("Yesterday");
    }
    time.format("%x").map(|s| s.to_string()).unwrap_or_default()
}

impl DocumentWindow {
    /// Shows the versions of the document (DOC-11).
    pub(super) fn browse_versions(self: &Rc<Self>) {
        let Some(path) = self.path.borrow().clone() else {
            return;
        };
        // What was typed last is saved first, so that the list is whole.
        self.autosave();
        let name = self.name.borrow().clone();
        let dialog = adw::Dialog::builder()
            .title(tr("Versions of “{name}”").replace("{name}", &name))
            .content_width(460)
            .content_height(520)
            .build();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::new());
        let versions = Store::for_document(&path).list();
        if versions.is_empty() {
            view.set_content(Some(
                &adw::StatusPage::builder()
                    .icon_name("document-open-recent-symbolic")
                    .title(tr("No Versions Yet"))
                    .description(tr(
                        "A version is kept when the document first saves itself after it opens, every hour while it is edited, and with Save.",
                    ))
                    .build(),
            ));
        } else {
            let page = adw::PreferencesPage::new();
            let today = glib::DateTime::now_local().ok();
            let mut group: Option<(String, adw::PreferencesGroup)> = None;
            for version in versions {
                let Ok(time) = glib::DateTime::from_unix_local(version.time) else {
                    continue;
                };
                let heading = today
                    .as_ref()
                    .map(|t| day_heading(&time, t))
                    .unwrap_or_default();
                if group.as_ref().is_none_or(|(h, _)| *h != heading) {
                    let new = adw::PreferencesGroup::builder().title(&heading).build();
                    page.add(&new);
                    group = Some((heading, new));
                }
                let clock = time.format("%X").map(|s| s.to_string()).unwrap_or_default();
                let row = self.version_row(&dialog, &version, &clock, &name);
                if let Some((_, group)) = &group {
                    group.add(&row);
                }
            }
            view.set_content(Some(&page));
        }
        dialog.set_child(Some(&view));
        dialog.present(Some(&self.window()));
    }

    /// The row of a version, with Open a Copy and Restore.
    fn version_row(
        self: &Rc<Self>,
        dialog: &adw::Dialog,
        version: &Version,
        clock: &str,
        name: &str,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(clock)
            .subtitle(kind_label(version.kind))
            .build();
        let copy = gtk::Button::builder()
            .label(tr("Open a _Copy"))
            .use_underline(true)
            .valign(gtk::Align::Center)
            .build();
        let restore = gtk::Button::builder()
            .label(tr("_Restore"))
            .use_underline(true)
            .valign(gtk::Align::Center)
            .build();
        // The buttons are named by their label; their description says
        // which version they act on, for a screen reader.
        let which = tr("The version of {time}").replace("{time}", clock);
        for button in [&copy, &restore] {
            button.update_property(&[gtk::accessible::Property::Description(&which)]);
        }
        {
            let weak = Rc::downgrade(self);
            let dialog = dialog.downgrade();
            let version = version.clone();
            let name = tr("{name} (version of {time})")
                .replace("{name}", name)
                .replace("{time}", clock);
            copy.connect_clicked(move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
                if let Some(this) = weak.upgrade() {
                    this.open_version_copy(&version, name.clone());
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let dialog = dialog.downgrade();
            let version = version.clone();
            restore.connect_clicked(move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
                if let Some(this) = weak.upgrade() {
                    this.restore(&version);
                }
            });
        }
        row.add_suffix(&copy);
        row.add_suffix(&restore);
        row
    }
}
