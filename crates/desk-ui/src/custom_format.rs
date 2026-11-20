// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sheet of Create Custom Format and Edit Custom Format (PXV-18,
//! TAB-31): a name, a kind (number, date and time, or text), the pattern
//! and the pieces it is made of, with a preview. Pages 15.4 has the pieces
//! dragged into the pattern; here a click puts one where the cursor is.
//! Editing a format also moves it in the menu or deletes it.

use adw::prelude::*;
use desk_model::table::{CustomFormat, Format, Value};
use gtk::{gio, glib};

use crate::editing::TextEditor;
use crate::i18n::{n_, tr};

/// The kinds of the sheet.
const KINDS: [&str; 3] = [n_("Number"), n_("Date and Time"), n_("Text")];

/// The pattern that a new format of each kind starts with.
const STARTS: [&str; 3] = ["#,##0.00", "dd/MM/yyyy", "@"];

/// The pieces of each kind, by name and what they put in the pattern.
fn pieces(kind: u32, currency: &str) -> Vec<(String, String)> {
    let piece = |name: &str, text: &str| (tr(name), text.to_owned());
    match kind {
        0 => vec![
            piece(n_("Integers"), "#,##0"),
            piece(n_("Decimals"), ".00"),
            piece(n_("Currency"), &format!("\"{currency}\"")),
            piece(n_("Percent"), "%"),
            piece(n_("Space"), " "),
        ],
        1 => vec![
            piece(n_("Day"), "dd"),
            piece(n_("Weekday"), "EEEE"),
            piece(n_("Month"), "MM"),
            piece(n_("Month Name"), "MMMM"),
            piece(n_("Year"), "yyyy"),
            piece(n_("Hours"), "HH"),
            piece(n_("Minutes"), "mm"),
            piece(n_("Space"), " "),
        ],
        _ => vec![piece(n_("Text"), "@"), piece(n_("Space"), " ")],
    }
}

/// The kind of a pattern: text with `@`, a date with letters of dates
/// outside quotes and brackets, and a number otherwise.
fn kind_of(pattern: &str) -> u32 {
    if pattern.contains('@') {
        return 2;
    }
    let (mut quoted, mut bracket) = (false, false);
    for c in pattern.chars() {
        match c {
            '\'' | '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            'y' | 'd' | 'M' | 'H' | 'E' if !quoted && !bracket => return 1,
            _ => {}
        }
    }
    0
}

/// A sample of a kind, which the preview writes.
fn sample(kind: u32) -> Value {
    match kind {
        0 => Value::Number(-1234.5),
        1 => Value::Date(crate::fields::now().unwrap_or(0.0)),
        _ => Value::Text(tr("Text")),
    }
}

/// Shows the sheet over `parent`: for a new format, or for the format at
/// `index` of the document.
pub fn present(parent: &impl IsA<gtk::Widget>, editor: &TextEditor, index: Option<usize>) {
    let formats = editor.custom_formats();
    let editing = index.and_then(|i| formats.get(i).cloned());
    let heading = if editing.is_some() {
        tr("Edit Custom Format")
    } else {
        tr("Create Custom Format")
    };
    let dialog = adw::AlertDialog::new(Some(&heading), None);
    let name = adw::EntryRow::builder().title(tr("Name")).build();
    let kinds: Vec<String> = KINDS.iter().map(|k| tr(k)).collect();
    let kind_names: Vec<&str> = kinds.iter().map(String::as_str).collect();
    let kind = adw::ComboRow::builder()
        .title(tr("Type"))
        .model(&gtk::StringList::new(&kind_names))
        .build();
    let pattern = adw::EntryRow::builder().title(tr("Format")).build();
    let preview = adw::ActionRow::builder()
        .title(tr("Preview"))
        .css_classes(["property"])
        .build();
    match &editing {
        Some(format) => {
            name.set_text(&format.name);
            kind.set_selected(kind_of(&format.pattern));
            pattern.set_text(&format.pattern);
        }
        None => {
            let n = formats.len() + 1;
            name.set_text(&tr("Custom Format {n}").replace("{n}", &n.to_string()));
            pattern.set_text(STARTS[0]);
        }
    }
    let group = adw::PreferencesGroup::new();
    group.add(&name);
    group.add(&kind);
    group.add(&pattern);
    group.add(&preview);

    // The pieces of the kind chosen, which a click puts in the pattern.
    let holder = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(4)
        .row_spacing(6)
        .column_spacing(6)
        .build();
    let currency = editor.locale().currency.clone();
    let fill_pieces = {
        let holder = holder.clone();
        let pattern = pattern.clone();
        move |kind: u32| {
            holder.remove_all();
            for (label, text) in pieces(kind, &currency) {
                let button = gtk::Button::builder().label(&label).build();
                let pattern = pattern.clone();
                button.connect_clicked(move |_| {
                    let mut at = pattern.position();
                    pattern.insert_text(&text, &mut at);
                    pattern.set_position(at);
                });
                holder.insert(&button, -1);
            }
        }
    };
    fill_pieces(kind.selected());
    let locale = editor.locale();
    let update = {
        let pattern = pattern.clone();
        let preview = preview.clone();
        let kind = kind.clone();
        move || {
            let format = Format::Custom(CustomFormat {
                name: String::new(),
                pattern: pattern.text().as_str().into(),
            });
            let shown = format.show(&sample(kind.selected()), &locale).text;
            preview.set_subtitle(&glib::markup_escape_text(&shown));
        }
    };
    update();
    {
        let update = update.clone();
        pattern.connect_changed(move |_| update());
    }
    {
        let pattern = pattern.clone();
        kind.connect_selected_notify(move |kind| {
            fill_pieces(kind.selected());
            pattern.set_text(STARTS[kind.selected() as usize % STARTS.len()]);
            update();
        });
    }
    let pieces_label = gtk::Label::builder()
        .label(tr("Click a piece to put it in the format"))
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    content.append(&group);
    content.append(&pieces_label);
    content.append(&holder);

    // Editing a format also moves it in the menu or deletes it (TAB-31).
    if let Some(i) = index.filter(|_| editing.is_some()) {
        let row = gtk::Box::builder()
            .spacing(6)
            .halign(gtk::Align::Center)
            .build();
        let action = |label: &str, run: Box<dyn Fn(&TextEditor)>| {
            let button = gtk::Button::builder().label(tr(label)).build();
            let editor = editor.clone();
            let dialog = dialog.clone();
            button.connect_clicked(move |_| {
                run(&editor);
                dialog.close();
            });
            row.append(&button);
            button
        };
        action(
            n_("Move Up"),
            Box::new(move |e| e.move_custom_format(i, false)),
        )
        .set_sensitive(i > 0);
        action(
            n_("Move Down"),
            Box::new(move |e| e.move_custom_format(i, true)),
        )
        .set_sensitive(i + 1 < formats.len());
        action(n_("Delete"), Box::new(move |e| e.delete_custom_format(i)))
            .add_css_class("destructive-action");
        content.append(&row);
    }

    dialog.set_extra_child(Some(&content));
    dialog.add_response("cancel", &tr("_Cancel"));
    dialog.add_response("save", &tr("_OK"));
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("save"));
    dialog.set_close_response("cancel");
    let editor = editor.clone();
    dialog.choose(Some(parent), gio::Cancellable::NONE, move |response| {
        let text = pattern.text();
        if response != "save" || text.trim().is_empty() {
            return;
        }
        let title = name.text().trim().to_owned();
        let format = CustomFormat {
            name: if title.is_empty() {
                tr("Custom Format")
            } else {
                title
            },
            pattern: text.as_str().into(),
        };
        editor.save_custom_format(index.filter(|_| editing.is_some()), format);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pxv_18_the_kind_of_a_pattern() {
        assert_eq!(kind_of("#,##0.00 \"kg\""), 0);
        assert_eq!(kind_of("dd/MM/yyyy"), 1);
        assert_eq!(kind_of("@ \"m\""), 2);
        for (k, start) in STARTS.iter().enumerate() {
            assert_eq!(kind_of(start), k as u32);
        }
    }
}
