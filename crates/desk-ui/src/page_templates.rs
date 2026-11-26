// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dialog of the page templates of a page layout document (W095,
//! PXV-20), from the Add Page menu: a page added from a template after
//! the current page, that of the selected object or else the page in
//! sight, the template of that page changed, a template deleted, and
//! that page made a template, as Create Page
//! Template From This Page of Pages 15.4 does. A template made again
//! under its own name takes the objects of the page, which is how it is
//! edited.

use adw::prelude::*;
use gtk::gio;

use crate::editing::TextEditor;
use crate::formatting::labelled;
use crate::i18n::tr;

/// Shows the page templates of the document, to add a page from or
/// apply, and makes the current page a template under a name.
pub fn present(parent: &impl IsA<gtk::Widget>, editor: &TextEditor) {
    let dialog = adw::AlertDialog::new(
        Some(&tr("Page Templates")),
        Some(&tr(
            "Add a page from a template after the current page, the page of the selection or in sight, or make that page a template.",
        )),
    );
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let layout = editor.has_page_templates();
    let name = adw::EntryRow::builder()
        .title(tr("_Name"))
        .use_underline(true)
        .sensitive(layout)
        .build();
    let new = adw::PreferencesGroup::builder()
        .description(tr(
            "A template of the same name takes the objects of the current page.",
        ))
        .build();
    new.add(&name);
    content.append(&new);
    let names = editor.page_template_names();
    let current = editor.current_page_template();
    if !names.is_empty() {
        let templates = adw::PreferencesGroup::builder()
            .title(tr("Templates"))
            .build();
        for template in names {
            let row = adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(&template))
                .build();
            if current.as_deref() == Some(&*template) {
                row.set_subtitle(&tr("Template of the current page"));
            }
            let add = gtk::Button::builder()
                .label(tr("Add Page"))
                .valign(gtk::Align::Center)
                .build();
            let apply = gtk::Button::builder()
                .icon_name("object-select-symbolic")
                .tooltip_text(tr("Apply to the Current Page"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            labelled(&apply, &tr("Apply to the Current Page"));
            let delete = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(tr("Delete"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            labelled(&delete, &tr("Delete"));
            row.add_suffix(&add);
            row.add_suffix(&apply);
            row.add_suffix(&delete);
            templates.add(&row);
            for (button, apply) in [(&add, false), (&apply, true)] {
                let (editor, dialog, named) =
                    (editor.clone(), dialog.downgrade(), template.clone());
                button.connect_clicked(move |_| {
                    if apply {
                        editor.apply_page_template(&named);
                    } else {
                        editor.add_page_from_template(&named);
                    }
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                });
            }
            let editor_del = editor.clone();
            let dialog_del = dialog.downgrade();
            delete.connect_clicked(move |_| {
                editor_del.delete_page_template(&template);
                // What the others apply to may have changed with the
                // selection: the dialog closes, as after the others.
                if let Some(dialog) = dialog_del.upgrade() {
                    dialog.close();
                }
            });
        }
        content.append(&templates);
    }
    dialog.set_extra_child(Some(&content));
    dialog.add_response("close", &tr("_Close"));
    dialog.add_response("create", &tr("_Make Template"));
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
    dialog.set_response_enabled("create", false);
    dialog.set_default_response(Some("create"));
    dialog.set_close_response("close");
    let weak = dialog.downgrade();
    name.connect_changed(move |entry| {
        if let Some(dialog) = weak.upgrade() {
            dialog.set_response_enabled("create", !entry.text().trim().is_empty());
        }
    });
    let editor = editor.clone();
    dialog.choose(Some(parent), gio::Cancellable::NONE, move |response| {
        if response == "create" {
            editor.create_page_template(&name.text());
        }
    });
}
