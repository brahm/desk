// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Language and Region of the main menu, as File > Advanced > Language &
//! Region in Pages 15.4 (PXV-09): the language of the document, System by
//! default, and its region, with a preview of a date, a time, a number and
//! an amount written in them.

use adw::prelude::*;
use desk_model::Language;
use desk_model::table::{
    DateStyle, Format, LANGUAGES, Locale, Negative, REGIONS, TimeStyle, Value,
};
use gtk::{gio, glib};

use crate::editing::TextEditor;
use crate::i18n::tr;

/// The name of a language Desk writes dates and numbers in.
fn language_name(code: &str) -> String {
    match code {
        "pt" => tr("Portuguese"),
        _ => tr("English"),
    }
}

/// The name of a region Desk knows.
fn region_name(code: &str) -> String {
    match code {
        "AR" => tr("Argentina"),
        "AU" => tr("Australia"),
        "BR" => tr("Brazil"),
        "CA" => tr("Canada"),
        "DE" => tr("Germany"),
        "ES" => tr("Spain"),
        "FR" => tr("France"),
        "GB" => tr("United Kingdom"),
        "IT" => tr("Italy"),
        "MX" => tr("Mexico"),
        "PT" => tr("Portugal"),
        "ZA" => tr("South Africa"),
        _ => tr("United States"),
    }
}

/// The language and region of the system, from `LANG`, as the document
/// takes them when it follows the system.
fn system() -> String {
    std::env::var("LANG").unwrap_or_default()
}

/// The language and the region of a tag, among those Desk knows: none for
/// the language when the document follows the system, and the region of
/// the language when the tag has none.
fn parts(tag: &str) -> (Option<&'static str>, &'static str) {
    let tag = tag.split(['.', '@']).next().unwrap_or_default();
    let mut pieces = tag.split(['-', '_']);
    let language = pieces.next().unwrap_or_default();
    let known = LANGUAGES
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(language));
    let region = pieces
        .filter_map(|p| REGIONS.iter().find(|r| r.0.eq_ignore_ascii_case(p)))
        .map(|r| r.0)
        .next();
    let fallback = known.map_or("US", |(_, region)| *region);
    (known.map(|(code, _)| *code), region.unwrap_or(fallback))
}

/// The tag of a language and a region; the undetermined language when the
/// document follows the system.
fn tag(language: Option<&str>, region: &str) -> Language {
    match language {
        Some(language) => Language::new(&format!("{language}-{region}")),
        None => Language::undetermined(),
    }
}

/// A date, a time, a number and an amount, at a day counted from 30
/// December 1899, as a locale writes them.
fn preview(locale: &Locale, day: f64) -> [String; 4] {
    let date = Format::DateTime {
        date: DateStyle::Full,
        time: TimeStyle::None,
    };
    let time = Format::DateTime {
        date: DateStyle::None,
        time: TimeStyle::Short,
    };
    let number = Format::Number {
        decimals: Some(2),
        thousands: true,
        negative: Negative::Minus,
    };
    let amount = Format::Currency {
        symbol: locale.currency.clone(),
        decimals: 2,
        thousands: true,
        negative: Negative::Minus,
        accounting: false,
    };
    [
        date.show(&Value::Date(day), locale).text,
        time.show(&Value::Date(day), locale).text,
        number.show(&Value::Number(1234.56), locale).text,
        amount.show(&Value::Number(1234.56), locale).text,
    ]
}

/// The locale of what the rows show.
fn locale_of(language: Option<&str>, region: &str) -> Locale {
    match language {
        Some(language) => Locale::of(language, Some(region)),
        None => Locale::from_tag(&system()),
    }
}

/// Asks for the language and region of the document, and sets them.
pub fn present(parent: &impl IsA<gtk::Widget>, editor: &TextEditor) {
    let dialog = adw::AlertDialog::new(Some(&tr("Language and Region")), None);
    let (language, region) = parts(editor.document_language().tag());
    let (system_language, system_region) = parts(&system());
    let region = if language.is_none() {
        system_region
    } else {
        region
    };

    let mut languages = vec![match system_language {
        Some(code) => tr("System ({language})").replace("{language}", &language_name(code)),
        None => tr("System"),
    }];
    languages.extend(LANGUAGES.iter().map(|(code, _)| language_name(code)));
    let names: Vec<&str> = languages.iter().map(String::as_str).collect();
    let language_row = adw::ComboRow::builder()
        .title(tr("_Language"))
        .use_underline(true)
        .model(&gtk::StringList::new(&names))
        .build();
    let chosen = language
        .and_then(|l| LANGUAGES.iter().position(|(code, _)| *code == l))
        .map_or(0, |i| i + 1);
    language_row.set_selected(chosen as u32);

    let regions: Vec<String> = REGIONS.iter().map(|r| region_name(r.0)).collect();
    let names: Vec<&str> = regions.iter().map(String::as_str).collect();
    let region_row = adw::ComboRow::builder()
        .title(tr("_Region"))
        .use_underline(true)
        .model(&gtk::StringList::new(&names))
        .build();
    let at = REGIONS.iter().position(|r| r.0 == region).unwrap_or(0);
    region_row.set_selected(at as u32);

    let group = adw::PreferencesGroup::new();
    group.add(&language_row);
    group.add(&region_row);
    let samples = adw::PreferencesGroup::builder()
        .title(tr("Preview"))
        .build();
    let rows: Vec<adw::ActionRow> = [tr("Date"), tr("Time"), tr("Number"), tr("Currency")]
        .iter()
        .map(|title| {
            let row = adw::ActionRow::builder()
                .title(title)
                .css_classes(["property"])
                .build();
            samples.add(&row);
            row
        })
        .collect();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .build();
    content.append(&group);
    content.append(&samples);

    // What the rows choose: the language, none for the system, and the
    // region.
    let choice = {
        let language_row = language_row.clone();
        let region_row = region_row.clone();
        move || {
            let language = (language_row.selected() as usize)
                .checked_sub(1)
                .and_then(|i| LANGUAGES.get(i))
                .map(|(code, _)| *code);
            let region = REGIONS
                .get(region_row.selected() as usize)
                .map_or("US", |r| r.0);
            (language, region)
        }
    };
    let day = crate::fields::now().unwrap_or(0.0);
    let update = {
        let choice = choice.clone();
        let region_row = region_row.clone();
        move || {
            let (language, region) = choice();
            // The system gives its own region.
            region_row.set_sensitive(language.is_some());
            let shown = preview(&locale_of(language, region), day);
            for (row, text) in rows.iter().zip(shown) {
                row.set_subtitle(&glib::markup_escape_text(&text));
            }
        }
    };
    update();
    let on_change = update.clone();
    language_row.connect_selected_notify(move |_| on_change());
    region_row.connect_selected_notify(move |_| update());

    dialog.set_extra_child(Some(&content));
    dialog.add_response("cancel", &tr("_Cancel"));
    dialog.add_response("apply", &tr("_Apply"));
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("apply"));
    dialog.set_close_response("cancel");
    let editor = editor.clone();
    dialog.choose(Some(parent), gio::Cancellable::NONE, move |response| {
        if response == "apply" {
            let (language, region) = choice();
            editor.set_document_language(tag(language, region));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pxv_09_the_rows_read_and_write_the_tag() {
        assert_eq!(parts("und"), (None, "US"));
        assert_eq!(parts("pt"), (Some("pt"), "BR"));
        assert_eq!(parts("en-GB"), (Some("en"), "GB"));
        assert_eq!(parts("pt_PT.UTF-8"), (Some("pt"), "PT"));
        assert_eq!(parts("fr-FR"), (None, "FR"));
        assert_eq!(tag(Some("en"), "BR"), Language::new("en-BR"));
        assert_eq!(tag(None, "BR"), Language::undetermined());
    }

    #[test]
    fn pxv_09_the_preview_follows_the_language_and_region() {
        // Wednesday 18 November 2026, at 14:05.
        let day = 46_344.0 + (14.0 * 60.0 + 5.0) / 1440.0;
        assert_eq!(
            preview(&locale_of(Some("pt"), "BR"), day),
            [
                "quarta-feira, 18 de novembro de 2026",
                "14:05",
                "1.234,56",
                "R$\u{a0}1.234,56"
            ]
        );
        assert_eq!(
            preview(&locale_of(Some("en"), "GB"), day),
            [
                "Wednesday, November 18, 2026",
                "14:05",
                "1,234.56",
                "£1,234.56"
            ]
        );
    }
}
