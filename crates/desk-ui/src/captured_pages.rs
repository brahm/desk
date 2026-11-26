// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dialog of Insert > Captured Pages (EST-20): the section at the
//! cursor captured under a name, as Format > Advanced > Capture Pages of
//! Pages '09 did, and the pages captured before inserted again or
//! deleted. Kept from Pages '09 by D-003; Pages 15.4 has page templates
//! instead (W095).

use adw::prelude::*;
use gtk::gio;

use crate::editing::TextEditor;
use crate::i18n::tr;

/// Shows the pages captured in the document, to insert or delete, and
/// captures the section at the cursor under a new name.
pub fn present(parent: &impl IsA<gtk::Widget>, editor: &TextEditor) {
    let dialog = adw::AlertDialog::new(
        Some(&tr("Captured Pages")),
        Some(&tr(
            "Capture the section at the cursor to insert its pages again in this document.",
        )),
    );
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    // The pages of a page layout document come with page templates
    // (W095): it captures none.
    let layout = editor.is_page_layout();
    let name = adw::EntryRow::builder()
        .title(tr("_Name"))
        .use_underline(true)
        .sensitive(!layout)
        .build();
    let new = adw::PreferencesGroup::new();
    new.add(&name);
    content.append(&new);
    let names = editor.captured_page_names();
    if !names.is_empty() {
        let captured = adw::PreferencesGroup::builder()
            .title(tr("Captured"))
            .build();
        for page_name in names {
            let row = adw::ActionRow::builder().title(&page_name).build();
            let insert = gtk::Button::builder()
                .label(tr("Insert"))
                .valign(gtk::Align::Center)
                .sensitive(!layout)
                .build();
            let delete = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(tr("Delete"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            row.add_suffix(&insert);
            row.add_suffix(&delete);
            captured.add(&row);
            let (editor_in, dialog_in, named) =
                (editor.clone(), dialog.downgrade(), page_name.clone());
            insert.connect_clicked(move |_| {
                editor_in.insert_captured_pages(&named);
                if let Some(dialog) = dialog_in.upgrade() {
                    dialog.close();
                }
            });
            let (editor_del, group, row_del) =
                (editor.clone(), captured.downgrade(), row.downgrade());
            delete.connect_clicked(move |_| {
                editor_del.remove_captured_pages(&page_name);
                if let (Some(group), Some(row)) = (group.upgrade(), row_del.upgrade()) {
                    group.remove(&row);
                }
            });
        }
        content.append(&captured);
    }
    dialog.set_extra_child(Some(&content));
    dialog.add_response("close", &tr("_Close"));
    dialog.add_response("capture", &tr("C_apture"));
    dialog.set_response_appearance("capture", adw::ResponseAppearance::Suggested);
    dialog.set_response_enabled("capture", false);
    dialog.set_default_response(Some("capture"));
    dialog.set_close_response("close");
    let weak = dialog.downgrade();
    name.connect_changed(move |entry| {
        if let Some(dialog) = weak.upgrade() {
            dialog.set_response_enabled("capture", !entry.text().trim().is_empty());
        }
    });
    let editor = editor.clone();
    dialog.choose(Some(parent), gio::Cancellable::NONE, move |response| {
        if response == "capture" {
            editor.capture_pages(&name.text());
        }
    });
}
