// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dialog of Insert > Auto Page Numbers (EST-27): where the numbers
//! of pages go, how they read, and whether the first page shows one.

use adw::prelude::*;
use desk_model::{AutoPageNumbers, MarginKind, NumberAlignment};
use gtk::{gio, glib};

use crate::editing::TextEditor;
use crate::i18n::tr;

const ALIGNMENTS: [NumberAlignment; 5] = [
    NumberAlignment::Left,
    NumberAlignment::Center,
    NumberAlignment::Right,
    NumberAlignment::Inside,
    NumberAlignment::Outside,
];

/// The lines that the numbers can read as, with `{page}` and `{pages}`
/// for the numbers.
fn texts() -> [String; 4] {
    [
        "{page}".to_owned(),
        tr("Page {page}"),
        tr("{page} of {pages}"),
        tr("Page {page} of {pages}"),
    ]
}

/// How a line reads on the second page of five.
fn sample(text: &str) -> String {
    text.replace("{pages}", "5").replace("{page}", "2")
}

fn combo(title: &str, items: &[String], selected: u32) -> adw::ComboRow {
    let names: Vec<&str> = items.iter().map(String::as_str).collect();
    let row = adw::ComboRow::builder()
        .title(title)
        .use_underline(true)
        .model(&gtk::StringList::new(&names))
        .build();
    row.set_selected(selected);
    row
}

/// Asks for the numbers of pages, and puts them in the document.
pub fn present(parent: &impl IsA<gtk::Widget>, editor: &TextEditor) {
    let dialog = adw::AlertDialog::new(Some(&tr("Auto Page Numbers")), None);
    let group = adw::PreferencesGroup::new();
    let position = combo(&tr("_Position"), &[tr("Header"), tr("Footer")], 1);
    let alignment = combo(
        &tr("_Alignment"),
        &[
            tr("Left"),
            tr("Center"),
            tr("Right"),
            tr("Inside"),
            tr("Outside"),
        ],
        1,
    );
    let texts = texts();
    let format = combo(
        &tr("_Format"),
        &texts.iter().map(|t| sample(t)).collect::<Vec<_>>(),
        0,
    );
    let first = adw::SwitchRow::builder()
        .title(tr("Include Number on _First Page"))
        .use_underline(true)
        .active(true)
        .build();
    let scope = combo(
        &tr("_Apply To"),
        &[tr("Entire Document"), tr("Current Section")],
        0,
    );
    for row in [
        position.upcast_ref::<gtk::Widget>(),
        alignment.upcast_ref(),
        format.upcast_ref(),
        first.upcast_ref(),
        scope.upcast_ref(),
    ] {
        group.add(row);
    }
    dialog.set_extra_child(Some(&group));
    dialog.add_response("cancel", &tr("_Cancel"));
    dialog.add_response("insert", &tr("_Insert"));
    dialog.set_response_appearance("insert", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("insert"));
    dialog.set_close_response("cancel");
    let editor = editor.clone();
    dialog.choose(
        Some(parent),
        gio::Cancellable::NONE,
        glib::clone!(
            #[strong]
            texts,
            move |response| {
                if response != "insert" {
                    return;
                }
                let numbers = AutoPageNumbers {
                    kind: if position.selected() == 0 {
                        MarginKind::Header
                    } else {
                        MarginKind::Footer
                    },
                    alignment: ALIGNMENTS
                        .get(alignment.selected() as usize)
                        .copied()
                        .unwrap_or_default(),
                    first_page: first.is_active(),
                    whole_document: scope.selected() == 0,
                };
                let text = texts.get(format.selected() as usize).unwrap_or(&texts[0]);
                editor.set_auto_page_numbers(numbers, text);
            }
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_27_the_formats_show_how_they_read() {
        let samples: Vec<String> = texts().iter().map(|t| sample(t)).collect();
        assert_eq!(samples, ["2", "Page 2", "2 of 5", "Page 2 of 5"]);
    }
}
