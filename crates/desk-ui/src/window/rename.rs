// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A click on the title of the window renames the document, as decided
//! for interface B on 14/11/2026: the file in its folder, keeping its
//! extension, or the name that Save offers for a document never saved.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;

use super::{DocumentWindow, display_name, remember_recent};
use crate::i18n::tr;

/// The file that a document gets when renamed to `name`, in the same
/// folder and with the same extension; none when the name is empty or
/// would leave the folder.
pub(super) fn renamed(path: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\0']) || name == "." || name == ".." {
        return None;
    }
    let file = match path.extension() {
        Some(extension) => format!("{name}.{}", extension.to_string_lossy()),
        None => name.to_owned(),
    };
    Some(path.with_file_name(file))
}

impl DocumentWindow {
    /// Makes a click on the title open the Rename popover, and adds the
    /// `rename` action that opens it from the keyboard.
    pub(super) fn add_rename(self: &Rc<Self>) {
        let entry = gtk::Entry::builder()
            .activates_default(true)
            .hexpand(true)
            .build();
        crate::formatting::labelled(&entry, &tr("Name"));
        let button = gtk::Button::builder()
            .label(tr("_Rename"))
            .use_underline(true)
            .css_classes(["suggested-action"])
            .build();
        let row = gtk::Box::builder()
            .spacing(6)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        row.append(&entry);
        row.append(&button);
        let popover = gtk::Popover::builder().child(&row).build();
        popover.set_default_widget(Some(&button));
        popover.set_parent(&self.title);
        self.title.set_tooltip_text(Some(&tr("Rename")));

        let open = {
            let weak = Rc::downgrade(self);
            let popover = popover.downgrade();
            let entry = entry.downgrade();
            move || {
                let (Some(this), Some(popover), Some(entry)) =
                    (weak.upgrade(), popover.upgrade(), entry.upgrade())
                else {
                    return;
                };
                if this.editor().is_none() {
                    return;
                }
                let stem = this
                    .path
                    .borrow()
                    .as_deref()
                    .and_then(Path::file_stem)
                    .map_or_else(
                        || this.name.borrow().clone(),
                        |s| s.to_string_lossy().into_owned(),
                    );
                entry.set_text(&stem);
                popover.popup();
                entry.grab_focus();
                entry.select_region(0, -1);
            }
        };
        let open = Rc::new(open);
        let click = gtk::GestureClick::new();
        {
            let open = open.clone();
            click.connect_released(move |gesture, presses, _, _| {
                if presses == 1 {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    open();
                }
            });
        }
        self.title.add_controller(click);
        let action = gio::SimpleAction::new("rename", None);
        action.connect_activate(move |_, _| open());
        self.actions.add_action(&action);

        let rename = {
            let weak = Rc::downgrade(self);
            let popover = popover.downgrade();
            let entry = entry.downgrade();
            move || {
                let (Some(this), Some(popover), Some(entry)) =
                    (weak.upgrade(), popover.upgrade(), entry.upgrade())
                else {
                    return;
                };
                let name = entry.text().trim().to_owned();
                let path = this.path.borrow().clone();
                match path {
                    None if !name.is_empty() => {
                        *this.name.borrow_mut() = name;
                        popover.popdown();
                        this.refresh();
                    }
                    Some(path) => match renamed(&path, &name) {
                        Some(new) if new == path => popover.popdown(),
                        Some(new) if new.exists() => this.alert(
                            &tr("Cannot Rename"),
                            &tr("A file named {name} is already in the folder.")
                                .replace("{name}", &display_name(&new)),
                        ),
                        Some(new) => match std::fs::rename(&path, &new) {
                            Ok(()) => {
                                popover.popdown();
                                this.renamed_to(&new);
                            }
                            Err(error) => {
                                this.alert(&tr("Cannot Rename"), &error.to_string());
                            }
                        },
                        None => this.window.error_bell(),
                    },
                    None => this.window.error_bell(),
                }
            }
        };
        let rename = Rc::new(rename);
        {
            let rename = rename.clone();
            button.connect_clicked(move |_| rename());
        }
        entry.connect_activate(move |_| rename());
    }

    /// Follows the file of the document to its new name, without saving.
    fn renamed_to(&self, path: &Path) {
        *self.path.borrow_mut() = Some(path.to_owned());
        *self.name.borrow_mut() = display_name(path);
        if let Some(editor) = self.editor() {
            editor.set_file_name(&self.file());
            editor.refresh_fields();
        }
        remember_recent(path);
        self.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_renamed_document_keeps_its_folder_and_extension() {
        let path = Path::new("/home/a/Letter.odt");
        assert_eq!(
            renamed(path, " Report "),
            Some(PathBuf::from("/home/a/Report.odt"))
        );
        assert_eq!(renamed(path, ""), None);
        assert_eq!(renamed(path, "a/b"), None);
        assert_eq!(renamed(path, ".."), None);
        assert_eq!(
            renamed(Path::new("/home/a/notes"), "todo"),
            Some(PathBuf::from("/home/a/todo"))
        );
    }
}
