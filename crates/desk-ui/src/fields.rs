// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The formats of fields (EST-27): how numbers of pages are written, the
//! formats of dates that Pages offers, and dates written from their
//! patterns in the symbols of Unicode (LDML).

use desk_model::{Field, NumberFormat};
use gtk::glib;

/// The formats of numbers of pages, in the order of Pages' menu.
pub const NUMBER_FORMATS: [(NumberFormat, &str, &str); 5] = [
    (NumberFormat::Decimal, "decimal", "1, 2, 3"),
    (NumberFormat::LowerRoman, "lower-roman", "i, ii, iii"),
    (NumberFormat::UpperRoman, "upper-roman", "I, II, III"),
    (NumberFormat::LowerAlpha, "lower-alpha", "a, b, c"),
    (NumberFormat::UpperAlpha, "upper-alpha", "A, B, C"),
];

/// The formats of dates and times that a date can take.
pub const DATE_FORMATS: [&str; 10] = [
    "d/M/yy",
    "dd/MM/y",
    "d MMM y",
    "d MMMM y",
    "EEEE, d MMMM y",
    "MMMM y",
    "y-MM-dd",
    "HH:mm",
    "h:mm a",
    "d/M/y HH:mm",
];

/// The pattern of the dates that Insert puts.
pub const DATE_PATTERN: &str = "d MMMM y";

/// The name of a format of numbers, as actions name it.
pub fn number_format_name(format: NumberFormat) -> &'static str {
    NUMBER_FORMATS
        .iter()
        .find(|(f, _, _)| *f == format)
        .map_or("decimal", |(_, name, _)| name)
}

/// The format of numbers of a name.
pub fn number_format(name: &str) -> NumberFormat {
    NUMBER_FORMATS
        .iter()
        .find(|(_, n, _)| *n == name)
        .map_or(NumberFormat::Decimal, |(f, _, _)| *f)
}

/// Now, in days from 30 December 1899, in the time of the system.
pub fn now() -> Option<f64> {
    let now = glib::DateTime::now_local().ok()?;
    let seconds = now.to_unix() as f64 + now.utc_offset().as_seconds() as f64;
    Some(25_569.0 + seconds / 86_400.0)
}

/// Today's date written in a pattern of LDML, in the language and region
/// of a document (PXV-09).
pub fn format_date(pattern: &str, locale: &desk_model::table::Locale) -> Option<String> {
    Some(locale.date(now()?, pattern))
}

/// A field of today's date, as Insert puts it in a document of a locale.
pub fn today(locale: &desk_model::table::Locale) -> Field {
    Field::DateTime {
        pattern: DATE_PATTERN.into(),
        shown: format_date(DATE_PATTERN, locale).unwrap_or_default().into(),
        update: false,
    }
}

/// A document as it opens from a file: its fields of the file name show
/// that file, and its dates that update on open show today (EST-27), in
/// the language and region of the document (PXV-09).
pub fn opened(document: desk_model::Document, file: &str) -> desk_model::Document {
    let locale = desk_layout::document_locale(&document);
    let mut editor = desk_model::Editor::new(document);
    let date = |pattern: &str| format_date(pattern, &locale);
    let refreshed = editor.edit("Open", |edit| edit.refresh_fields(file, Some(&date)));
    if let Err(error) = refreshed {
        glib::g_warning!("desk", "cannot update the fields: {error}");
    }
    editor.into_document()
}

/// A document as it is saved to a file: its fields of the file name show
/// that file.
pub fn saved(document: desk_model::Document, file: &str) -> desk_model::Document {
    let mut editor = desk_model::Editor::new(document);
    // Tables of contents are up to date in what is kept, as Pages updates
    // them when a document closes (EST-22): in the copy saved, so that
    // saving, and autosaving, never changes the undo history.
    // Each in its own edit, so that a table of contents that cannot be
    // updated leaves the fields refreshed (second review of RV batch 5).
    if let Err(error) = editor.edit("Save", |edit| edit.update_tables_of_contents()) {
        glib::g_warning!("desk", "cannot update the tables of contents: {error}");
    }
    if let Err(error) = editor.edit("Save", |edit| edit.refresh_fields(file, None)) {
        glib::g_warning!("desk", "cannot update the fields: {error}");
    }
    editor.into_document()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_27_patterns_of_unicode_write_dates() {
        // Wednesday 18 November 2026, at 14:05.
        let day = 46_344.0 + (14.0 * 60.0 + 5.0) / 1440.0;
        let en = desk_model::table::Locale::en_us();
        let pt = desk_model::table::Locale::pt_br();
        assert_eq!(
            en.date(day, "EEEE, d MMMM y"),
            "Wednesday, 18 November 2026"
        );
        assert_eq!(en.date(day, "dd/MM/yy HH:mm"), "18/11/26 14:05");
        assert_eq!(en.date(day, "h:mm a"), "2:05 PM");
        assert_eq!(pt.date(day, "d 'de' MMMM"), "18 de novembro");
        assert_eq!(en.date(day, "'o''clock' 100%"), "o'clock 100%");
        for pattern in DATE_FORMATS {
            assert!(!pt.date(day, pattern).is_empty());
        }
    }

    #[test]
    fn est_22_a_saved_copy_has_its_table_of_contents_updated() {
        // Review RV, batch 5: saving updated the tables of contents in the
        // editor, so each autosave after an undo added a step to the undo
        // history and cleared the redo.
        use desk_model::{Document, Editor, Position, TocLevel, TocSettings};
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Setup", |edit| {
                let at = edit.insert_text(start, "Chapter one")?;
                let at = edit.split_paragraph(at)?;
                edit.insert_text(at, "Some text")?;
                let heading = edit.new_paragraph_style("Heading", start, start, false)?;
                let entry = edit.new_paragraph_style("TOC Heading", start, start, false)?;
                let first = Position::new(start.paragraph, 0);
                edit.set_paragraph_style(first, first, heading)?;
                let mut settings = edit.document().text_settings().clone();
                settings.toc = TocSettings {
                    levels: vec![TocLevel {
                        style: heading,
                        entry,
                        page_number: false,
                    }],
                    page_links: false,
                };
                edit.set_text_settings(settings)?;
                edit.insert_table_of_contents(0)
            })
            .unwrap();
        editor
            .edit("Rename", |edit| {
                edit.insert_text(Position::new(start.paragraph, 0), "The ")
                    .map(|_| ())
            })
            .unwrap();
        let text = |document: &Document| -> String {
            document
                .body()
                .iter()
                .next()
                .map(|p| p.runs().map(|r| r.text().to_owned()).collect())
                .unwrap_or_default()
        };
        assert!(!text(editor.document()).starts_with("The "));
        let copy = saved(editor.document().clone(), "/tmp/a.odt");
        assert!(
            text(&copy).starts_with("The Chapter one"),
            "{}",
            text(&copy)
        );
        // The editor and its history are as they were.
        assert!(!text(editor.document()).starts_with("The "));
        assert_eq!(editor.undo_name(), Some("Rename"));
    }

    #[test]
    fn est_27_formats_of_numbers_have_names() {
        for (format, name, _) in NUMBER_FORMATS {
            assert_eq!(number_format(number_format_name(format)), format);
            assert_eq!(number_format_name(number_format(name)), name);
        }
    }
}
