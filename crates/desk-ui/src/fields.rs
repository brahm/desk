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

/// A pattern of LDML in the symbols of `g_date_time_format`. Letters of
/// LDML it does not know stay as they are, and text in quotes is kept.
pub fn strftime(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            // Quoted text, where '' is a quote.
            let mut j = i + 1;
            if chars.get(j) == Some(&'\'') {
                out.push('\'');
                i += 2;
                continue;
            }
            while j < chars.len() {
                if chars[j] == '\'' {
                    if chars.get(j + 1) == Some(&'\'') {
                        out.push('\'');
                        j += 2;
                        continue;
                    }
                    break;
                }
                if chars[j] == '%' {
                    out.push('%');
                }
                out.push(chars[j]);
                j += 1;
            }
            i = j + 1;
            continue;
        }
        if !c.is_ascii_alphabetic() {
            if c == '%' {
                out.push('%');
            }
            out.push(c);
            i += 1;
            continue;
        }
        let mut n = 1;
        while chars.get(i + n) == Some(&c) {
            n += 1;
        }
        let symbol = match (c, n) {
            ('y', 2) => "%y",
            ('y', _) => "%Y",
            ('M', 1) => "%-m",
            ('M', 2) => "%m",
            ('M', 3) => "%b",
            ('M', _) => "%B",
            ('d', 1) => "%-d",
            ('d', _) => "%d",
            ('E', 4..) => "%A",
            ('E', _) => "%a",
            ('H', 1) => "%-H",
            ('H', _) => "%H",
            ('h', 1) => "%-I",
            ('h', _) => "%I",
            ('m', 1) => "%-M",
            ('m', _) => "%M",
            ('s', 1) => "%-S",
            ('s', _) => "%S",
            ('a', _) => "%p",
            _ => {
                out.extend(std::iter::repeat_n(c, n));
                i += n;
                continue;
            }
        };
        out.push_str(symbol);
        i += n;
    }
    out
}

/// Today's date written in a pattern of LDML.
pub fn format_date(pattern: &str) -> Option<String> {
    glib::DateTime::now_local()
        .ok()?
        .format(&strftime(pattern))
        .ok()
        .map(|s| s.to_string())
}

/// A field of today's date, as Insert puts it.
pub fn today() -> Field {
    Field::DateTime {
        pattern: DATE_PATTERN.into(),
        shown: format_date(DATE_PATTERN).unwrap_or_default().into(),
        update: false,
    }
}

/// A document as it opens from a file: its fields of the file name show
/// that file, and its dates that update on open show today (EST-27).
pub fn opened(document: desk_model::Document, file: &str) -> desk_model::Document {
    let mut editor = desk_model::Editor::new(document);
    let refreshed = editor.edit("Open", |edit| edit.refresh_fields(file, Some(&format_date)));
    if let Err(error) = refreshed {
        glib::g_warning!("desk", "cannot update the fields: {error}");
    }
    editor.into_document()
}

/// A document as it is saved to a file: its fields of the file name show
/// that file.
pub fn saved(document: desk_model::Document, file: &str) -> desk_model::Document {
    let mut editor = desk_model::Editor::new(document);
    if let Err(error) = editor.edit("Save", |edit| edit.refresh_fields(file, None)) {
        glib::g_warning!("desk", "cannot update the fields: {error}");
    }
    editor.into_document()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_27_patterns_of_unicode_become_those_of_glib() {
        assert_eq!(strftime("EEEE, d MMMM y"), "%A, %-d %B %Y");
        assert_eq!(strftime("dd/MM/yy HH:mm"), "%d/%m/%y %H:%M");
        assert_eq!(strftime("h:mm a"), "%-I:%M %p");
        assert_eq!(strftime("d 'de' MMMM"), "%-d de %B");
        assert_eq!(strftime("'o''clock' 100%"), "o'clock 100%%");
    }

    #[test]
    fn est_27_formats_of_numbers_have_names() {
        for (format, name, _) in NUMBER_FORMATS {
            assert_eq!(number_format(number_format_name(format)), format);
            assert_eq!(number_format_name(number_format(name)), name);
        }
    }
}
