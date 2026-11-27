// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Mail Merge in the Document tab of the Inspector, as in Pages 15.4
//! (W105, MAL-04, MAL-05, MAL-07): the source to Populate With, the table
//! of a spreadsheet of several, Add Merge Field, the Preview whose arrows
//! step through the records, and Merge, which makes a new document with a
//! copy for each record.
//!
//! A source is a file that the document names. It is read only when the
//! person asks for its data, by choosing it, opening Add Merge Field,
//! turning the Preview on or merging, never when the document opens; so
//! are the contacts of the desktop, which the Preview and Merge also ask
//! for the person's own card, for the sender fields.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_contacts::{Records, SourceError};
use desk_model::{Document, MergeSource};
use gtk::{gio, glib};

use crate::compact;
use crate::editing::{Format, MergePreview, TextEditor};
use crate::formatting::EditorOf;
use crate::i18n::tr;

/// The fields of a contact that the menus offer first, in the order of
/// Pages 15.4, before More Fields: Name is its Full Name.
pub const COMMON_FIELDS: [&str; 9] = [
    "Name",
    "First Name",
    "Last Name",
    "Home Address",
    "Work Address",
    "Mobile Phone",
    "Home Email",
    "Job Title",
    "Company",
];

/// The fields of a contact in a menu: the common ones, then the others
/// under More Fields, as Pages 15.4 has them.
fn contact_items(menu: &gio::Menu, sender: bool) {
    let common: Vec<String> = COMMON_FIELDS.iter().map(|n| (*n).to_owned()).collect();
    field_items(menu, &common, sender);
    let more: Vec<String> = desk_contacts::CONTACT_FIELDS
        .iter()
        .filter(|n| !COMMON_FIELDS.contains(n))
        .map(|n| (*n).to_owned())
        .collect();
    let others = gio::Menu::new();
    field_items(&others, &more, sender);
    menu.append_submenu(Some(&tr("_More Fields")), &others);
}

/// The records of a source: those of a file, read now, or the contacts
/// that the desktop gave last ([`crate::desktop_contacts::with_cards`]).
pub fn read_records(source: &MergeSource) -> Result<Records, SourceError> {
    if let MergeSource::Contacts { group } = source {
        let cards = crate::desktop_contacts::cards_now().ok_or(SourceError::Desktop)?;
        let cards = crate::desktop_contacts::in_group(cards.to_vec(), group.as_deref());
        return Ok(Records::of_cards(&cards));
    }
    desk_contacts::read_source(source, |path| std::fs::File::open(path))
}

/// The datum that a field of a record shows: a recipient field takes
/// that of the record, or nothing when it has none; a sender field that
/// of the person's own card, once the desktop gave it (MAL-01).
fn datum(records: &Records, record: usize, name: &str, sender: bool) -> Option<String> {
    if sender {
        return crate::desktop_contacts::own_card_now().and_then(|c| c.get(name));
    }
    Some(records.datum(record, name).unwrap_or_default().to_owned())
}

/// The merged document of the document of an editor, from its source
/// (MAL-04).
pub fn merged(editor: &TextEditor) -> Result<Document, String> {
    let document = editor.document();
    let Some(source) = document.text_settings().merge_source.clone() else {
        return Err(tr("Choose where the data come from first."));
    };
    let records = read_records(&source).map_err(|e| describe_error(&e))?;
    if records.is_empty() {
        return Err(tr("The source has no records."));
    }
    desk_model::merge_document(&document, records.len(), &|record, name, sender| {
        datum(&records, record, name, sender)
    })
    .map_err(|e| e.to_string())
}

/// Why a source could not be read, for the person.
fn describe_error(error: &SourceError) -> String {
    match error {
        SourceError::Desktop => tr("The contacts of the desktop cannot be read."),
        SourceError::TooLarge => tr("The file is too large."),
        SourceError::Unsupported => tr("The file is not a vCard, ODS or CSV file."),
        SourceError::NoTable(name) => {
            tr("The spreadsheet has no table named “{name}”.").replace("{name}", name)
        }
        SourceError::Io(_) | SourceError::Spreadsheet(_) => tr("The file cannot be read."),
    }
}

/// The name of a source, as Populate With shows it.
fn source_name(source: Option<&MergeSource>) -> String {
    match source {
        None => tr("None"),
        Some(MergeSource::Contacts { group: None }) => tr("All Contacts"),
        Some(MergeSource::Contacts { group: Some(group) }) => group.to_string(),
        Some(MergeSource::VCard { file } | MergeSource::Spreadsheet { file, .. }) => {
            std::path::Path::new(&**file)
                .file_name()
                .map_or_else(|| file.to_string(), |n| n.to_string_lossy().into_owned())
        }
    }
}

/// The items of a menu that put a merge field of each name, of a
/// recipient or of the sender.
fn field_items(menu: &gio::Menu, names: &[String], sender: bool) {
    let action = if sender {
        "win.insert-sender-field"
    } else {
        "win.insert-merge-field"
    };
    for name in names {
        let item = gio::MenuItem::new(Some(&name.replace('_', "__")), None);
        item.set_action_and_target_value(Some(action), Some(&name.to_variant()));
        menu.append_item(&item);
    }
}

/// The Mail Merge field menu of Insert ▸ Field (MAL-07): the common
/// fields of a contact, and those of the sender.
pub fn insert_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let recipients = gio::Menu::new();
    contact_items(&recipients, false);
    menu.append_section(None, &recipients);
    let sender = gio::Menu::new();
    contact_items(&sender, true);
    menu.append_submenu(Some(&tr("_Sender Field")), &sender);
    menu
}

/// The Mail Merge group of the Document tab.
pub struct MailMergeGroup {
    group: compact::Group,
    source: compact::ActionRow,
    table: compact::ComboRow,
    fields: gtk::MenuButton,
    preview: compact::SwitchRow,
    record: gtk::Label,
    previous: gtk::Button,
    next: gtk::Button,
    stepper: compact::ActionRow,
    merge: gtk::Button,
    /// The records of the source last read, with the source.
    records: RefCell<Option<(MergeSource, Rc<Records>)>>,
    /// The record that the preview shows.
    at: Cell<usize>,
    /// The source that the group shows.
    shown: RefCell<Option<MergeSource>>,
    updating: Cell<bool>,
    editor: EditorOf,
}

impl MailMergeGroup {
    /// The group for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let choose = gtk::MenuButton::builder()
            .label(tr("Choose"))
            .valign(gtk::Align::Center)
            .menu_model(&source_menu(&[]))
            .build();
        // The groups of the contacts, as the desktop gave them last; the
        // menu asks it again as it opens, away from the window, and shows
        // the groups once they come.
        let asked = Rc::new(Cell::new(false));
        choose.set_create_popup_func(move |button| {
            let groups = || {
                crate::desktop_contacts::cards_now()
                    .map(|cards| crate::desktop_contacts::groups(&cards))
                    .unwrap_or_default()
            };
            let before = groups();
            button.set_menu_model(Some(&source_menu(&before)));
            if asked.replace(true) {
                // The menu opened again as the groups came.
                asked.set(false);
                return;
            }
            let button = button.downgrade();
            let asked = asked.clone();
            crate::desktop_contacts::with_cards(move |_| {
                let now = groups();
                match button.upgrade() {
                    Some(button) if now != before && button.is_active() => {
                        button.popdown();
                        button.popup();
                    }
                    _ => asked.set(false),
                }
            });
        });
        let source = compact::ActionRow::builder()
            .title(tr("Populate With"))
            .build();
        source.add_suffix(&choose);
        let table = compact::ComboRow::builder()
            .title(tr("Table"))
            .visible(false)
            .build();
        let fields = gtk::MenuButton::builder()
            .label(tr("Add"))
            .valign(gtk::Align::Center)
            .build();
        let add = compact::ActionRow::builder()
            .title(tr("Add Merge Field"))
            .build();
        add.add_suffix(&fields);
        let preview = compact::SwitchRow::builder().title(tr("Preview")).build();
        let previous = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text(tr("Previous Record"))
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let next = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .tooltip_text(tr("Next Record"))
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        for (button, label) in [
            (&previous, tr("Previous Record")),
            (&next, tr("Next Record")),
        ] {
            button.update_property(&[gtk::accessible::Property::Label(&label)]);
        }
        let record = gtk::Label::builder()
            .css_classes(["numeric", "dim-label"])
            .build();
        let stepper = compact::ActionRow::builder().title(tr("Record")).build();
        stepper.add_suffix(&previous);
        stepper.add_suffix(&record);
        stepper.add_suffix(&next);
        let merge = gtk::Button::builder()
            .label(tr("Merge"))
            .action_name("win.mail-merge")
            .halign(gtk::Align::End)
            .css_classes(["suggested-action"])
            .build();
        let group = compact::Group::builder().title(tr("Mail Merge")).build();
        group.add(&source);
        group.add(&table);
        group.add(&add);
        group.add(&preview);
        group.add(&stepper);
        group.add_widget(&merge);
        let this = Rc::new(Self {
            group,
            source,
            table,
            fields,
            preview,
            record,
            previous,
            next,
            stepper,
            merge,
            records: RefCell::new(None),
            at: Cell::new(0),
            shown: RefCell::new(None),
            updating: Cell::new(false),
            editor,
        });
        this.add_actions();
        // The fields of the source, read when the menu opens.
        let weak = Rc::downgrade(&this);
        this.fields.set_create_popup_func(move |button| {
            if let Some(this) = weak.upgrade() {
                button.set_menu_model(Some(&this.fields_menu()));
            }
        });
        let weak = Rc::downgrade(&this);
        this.preview.connect_active_notify(move |row| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            if this.updating.get() {
                return;
            }
            if !row.is_active() {
                this.show_preview();
                return;
            }
            // The contacts and the person's own card, for the sender
            // fields, come first.
            let weak = Rc::downgrade(&this);
            crate::desktop_contacts::with_cards(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.show_preview();
                }
            });
        });
        for (button, step) in [(&this.previous, -1isize), (&this.next, 1)] {
            let weak = Rc::downgrade(&this);
            button.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.step(step);
                }
            });
        }
        let weak = Rc::downgrade(&this);
        this.table.connect_selected_notify(move |row| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.choose_table(row.selected() as usize);
            }
        });
        this.stepper_visible(false);
        this
    }

    /// The group.
    pub fn widget(&self) -> &compact::Group {
        &self.group
    }

    /// The actions of the menu of Populate With, as `merge.source`.
    fn add_actions(self: &Rc<Self>) {
        let actions = gio::SimpleActionGroup::new();
        let action = gio::SimpleAction::new("source", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        action.connect_activate(move |_, kind| {
            let (Some(this), Some(kind)) = (weak.upgrade(), kind.and_then(|k| k.str())) else {
                return;
            };
            if let Some(group) = kind.strip_prefix("group:") {
                this.set_source(Some(MergeSource::Contacts {
                    group: Some(group.into()),
                }));
                return;
            }
            match kind {
                "contacts" => this.set_source(Some(MergeSource::Contacts { group: None })),
                "none" => this.set_source(None),
                "vcard" | "spreadsheet" => this.choose_file(kind == "vcard"),
                _ => {}
            }
        });
        actions.add_action(&action);
        self.group
            .widget()
            .insert_action_group("merge", Some(&actions));
    }

    /// Chooses a vCard file or a spreadsheet as the source.
    fn choose_file(self: &Rc<Self>, vcard: bool) {
        let filter = gtk::FileFilter::new();
        if vcard {
            filter.set_name(Some(&tr("vCard Files")));
            filter.add_mime_type("text/vcard");
            filter.add_mime_type("text/x-vcard");
            filter.add_suffix("vcf");
        } else {
            filter.set_name(Some(&tr("Spreadsheets")));
            filter.add_mime_type("application/vnd.oasis.opendocument.spreadsheet");
            filter.add_mime_type("text/csv");
            for suffix in ["ods", "fods", "csv", "tsv"] {
                filter.add_suffix(suffix);
            }
        }
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(if vcard {
                tr("Choose a vCard File")
            } else {
                tr("Choose a Spreadsheet")
            })
            .modal(true)
            .filters(&filters)
            .default_filter(&filter)
            .build();
        let parent = self.group.root().and_downcast::<gtk::Window>();
        let weak = Rc::downgrade(self);
        dialog.open(parent.as_ref(), gio::Cancellable::NONE, move |result| {
            let (Some(this), Ok(file)) = (weak.upgrade(), result) else {
                return;
            };
            let Some(path) = file.path() else {
                return;
            };
            let file: std::sync::Arc<str> = path.to_string_lossy().into();
            this.set_source(Some(if vcard {
                MergeSource::VCard { file }
            } else {
                MergeSource::Spreadsheet { file, table: None }
            }));
        });
    }

    /// Makes a source that of the document, and reads it, as the person
    /// chose it.
    fn set_source(self: &Rc<Self>, source: Option<MergeSource>) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        editor.set_merge_preview(None);
        editor.set_merge_source(source.clone());
        let Some(source) = source else {
            return;
        };
        *self.records.borrow_mut() = None;
        let weak = Rc::downgrade(self);
        self.with_contacts(&source.clone(), move || {
            if let Some(this) = weak.upgrade() {
                this.load(&source);
            }
        });
    }

    /// Calls `then` once the contacts of the desktop came, when the source
    /// is the contacts, or at once.
    fn with_contacts(&self, source: &MergeSource, then: impl FnOnce() + 'static) {
        if matches!(source, MergeSource::Contacts { .. }) {
            crate::desktop_contacts::with_cards(move |_| then());
        } else {
            then();
        }
    }

    /// Chooses the table of a spreadsheet of several.
    fn choose_table(self: &Rc<Self>, index: usize) {
        let Some((source, records)) = self.records.borrow().clone() else {
            return;
        };
        let (MergeSource::Spreadsheet { file, .. }, Some(name)) =
            (&source, records.tables.get(index))
        else {
            return;
        };
        self.set_source(Some(MergeSource::Spreadsheet {
            file: file.clone(),
            table: (index > 0).then(|| name.as_str().into()),
        }));
    }

    /// The records of a source, read again when it is another one; says
    /// why it could not be read.
    fn load(&self, source: &MergeSource) -> Option<Rc<Records>> {
        if let Some((read, records)) = &*self.records.borrow()
            && read == source
        {
            return Some(records.clone());
        }
        match read_records(source) {
            Ok(records) => {
                let records = Rc::new(records);
                *self.records.borrow_mut() = Some((source.clone(), records.clone()));
                self.group.set_description(
                    records
                        .truncated
                        .then(|| tr("Only the first records of the source are used."))
                        .as_deref(),
                );
                self.show_tables(source, &records);
                Some(records)
            }
            Err(error) => {
                *self.records.borrow_mut() = None;
                self.group.set_description(Some(&describe_error(&error)));
                None
            }
        }
    }

    /// The records of the source of the document, read now.
    fn current(&self) -> Option<Rc<Records>> {
        let source = self.shown.borrow().clone()?;
        self.load(&source)
    }

    /// Shows the tables of a spreadsheet of several.
    fn show_tables(&self, source: &MergeSource, records: &Records) {
        self.updating.set(true);
        let several = records.tables.len() > 1;
        self.table.set_visible(several);
        if several {
            self.table.set_model(Some(&crate::tab_stops::string_list(
                records.tables.iter().cloned(),
            )));
            let chosen = match source {
                MergeSource::Spreadsheet {
                    table: Some(name), ..
                } => records
                    .tables
                    .iter()
                    .position(|t| **t == **name)
                    .unwrap_or(0),
                _ => 0,
            };
            self.table.set_selected(chosen as u32);
        }
        self.updating.set(false);
    }

    /// The menu of Add Merge Field: the fields of the source, or of a
    /// contact, and those of the sender (MAL-07).
    fn fields_menu(&self) -> gio::Menu {
        let spreadsheet = matches!(&*self.shown.borrow(), Some(MergeSource::Spreadsheet { .. }));
        if !spreadsheet {
            return insert_menu();
        }
        // The columns as the file has them now.
        *self.records.borrow_mut() = None;
        let names = self.current().map(|r| r.fields.clone()).unwrap_or_default();
        let menu = gio::Menu::new();
        let recipients = gio::Menu::new();
        field_items(&recipients, &names, false);
        menu.append_section(None, &recipients);
        let sender = gio::Menu::new();
        field_items(&sender, &names, true);
        menu.append_submenu(Some(&tr("_Sender Field")), &sender);
        menu
    }

    /// Shows the arrows of the preview, while it is on.
    fn stepper_visible(&self, visible: bool) {
        self.stepper.set_visible(visible);
    }

    /// Shows the record of the preview, or the fields as they are.
    fn show_preview(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let on = self.preview.is_active();
        // Turned on, the preview reads the source again, which may have
        // changed since (MAL-04).
        if on && !editor.merge_previewing() {
            *self.records.borrow_mut() = None;
        }
        let records = on.then(|| self.current()).flatten();
        if records.as_ref().is_some_and(|r| r.is_empty()) {
            self.group
                .set_description(Some(&tr("The source has no records.")));
        }
        let Some(records) = records.filter(|r| !r.is_empty()) else {
            if on {
                self.updating.set(true);
                self.preview.set_active(false);
                self.updating.set(false);
            }
            editor.set_merge_preview(None);
            self.stepper_visible(false);
            return;
        };
        let at = self.at.get().min(records.len() - 1);
        self.at.set(at);
        self.record.set_label(
            &tr("{n} of {count}")
                .replace("{n}", &(at + 1).to_string())
                .replace("{count}", &records.len().to_string()),
        );
        self.previous.set_sensitive(at > 0);
        self.next.set_sensitive(at + 1 < records.len());
        self.stepper_visible(true);
        let preview: MergePreview = Rc::new(move |name, sender| datum(&records, at, name, sender));
        editor.set_merge_preview(Some(preview));
    }

    /// Shows the record before or after.
    fn step(&self, step: isize) {
        self.at.set(self.at.get().saturating_add_signed(step));
        self.show_preview();
    }

    /// Shows the source of the document.
    pub fn show(&self, format: &Format) {
        // An edit of the document ended the preview.
        if self.preview.is_active() && !(self.editor)().is_some_and(|e| e.merge_previewing()) {
            self.updating.set(true);
            self.preview.set_active(false);
            self.updating.set(false);
            self.stepper_visible(false);
        }
        let source = format.text_settings.merge_source.clone();
        if *self.shown.borrow() == source {
            return;
        }
        *self.shown.borrow_mut() = source.clone();
        self.source.set_subtitle(&source_name(source.as_ref()));
        self.merge.set_sensitive(source.is_some());
        self.fields.set_sensitive(true);
        // Another source: the preview ends, and its records go.
        let read = self.records.borrow().as_ref().map(|(s, _)| s.clone());
        if read.is_some() && read != source {
            *self.records.borrow_mut() = None;
        }
        if read.as_ref() != source.as_ref() {
            self.table.set_visible(false);
            self.group.set_description(None);
        }
        self.at.set(0);
        if self.preview.is_active() {
            self.updating.set(true);
            self.preview.set_active(false);
            self.updating.set(false);
            if let Some(editor) = (self.editor)() {
                editor.set_merge_preview(None);
            }
            self.stepper_visible(false);
        }
    }
}

/// The menu of Populate With: every contact or those of a group, a
/// vCard file or a spreadsheet.
fn source_menu(groups: &[String]) -> gio::Menu {
    let item = |label: &str, kind: &str| {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("merge.source"), Some(&kind.to_variant()));
        item
    };
    let menu = gio::Menu::new();
    let contacts = gio::Menu::new();
    contacts.append_item(&item(&tr("_All Contacts"), "contacts"));
    for group in groups {
        contacts.append_item(&item(&group.replace('_', "__"), &format!("group:{group}")));
    }
    menu.append_section(Some(&tr("Contacts")), &contacts);
    let files = gio::Menu::new();
    files.append_item(&item(&tr("_vCard File…"), "vcard"));
    files.append_item(&item(&tr("_Spreadsheet…"), "spreadsheet"));
    menu.append_section(None, &files);
    let remove = gio::Menu::new();
    let item = gio::MenuItem::new(Some(&tr("_Remove Source")), None);
    item.set_action_and_target_value(Some("merge.source"), Some(&"none".to_variant()));
    remove.append_item(&item);
    menu.append_section(None, &remove);
    menu
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mal_07_the_menu_of_fields_is_that_of_pages() {
        let menu = insert_menu();
        let recipients = menu.item_link(0, gio::MENU_LINK_SECTION).unwrap();
        // The common fields, then More Fields with every other field.
        assert_eq!(recipients.n_items(), COMMON_FIELDS.len() as i32 + 1);
        let more = recipients
            .item_link(COMMON_FIELDS.len() as i32, gio::MENU_LINK_SUBMENU)
            .unwrap();
        assert_eq!(
            more.n_items() as usize + COMMON_FIELDS.len(),
            desk_contacts::CONTACT_FIELDS.len()
        );
        assert!(
            COMMON_FIELDS
                .iter()
                .all(|f| desk_contacts::CONTACT_FIELDS.contains(f))
        );
    }

    #[test]
    fn mal_04_a_source_is_named_by_its_file() {
        assert_eq!(
            source_name(Some(&MergeSource::VCard {
                file: "/home/ana/Clients.vcf".into()
            })),
            "Clients.vcf"
        );
        assert_eq!(
            source_name(Some(&MergeSource::Contacts {
                group: Some("Clients".into())
            })),
            "Clients"
        );
    }

    #[test]
    fn mal_04_a_recipient_field_without_a_datum_shows_nothing() {
        let records = Records::of_table(&[
            vec!["First Name".into(), "City".into()],
            vec!["Ana".into(), String::new()],
        ]);
        assert_eq!(
            datum(&records, 0, "First Name", false).as_deref(),
            Some("Ana")
        );
        assert_eq!(datum(&records, 0, "City", false).as_deref(), Some(""));
        assert_eq!(datum(&records, 0, "First Name", true), None);
    }
}
