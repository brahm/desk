// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Text of comma separated values (RFC 4180), as spreadsheets export a
//! table (MAL-05): fields in quotes may hold the separator, quotes
//! doubled and line breaks.

/// The most rows read.
pub const MAX_ROWS: usize = 100_000;

/// The most fields of a row.
pub const MAX_FIELDS: usize = 1024;

/// The separator of a text: of the comma, the semicolon and the tab, the
/// one its first line has most of outside quotes; the comma when it has
/// none. A quote opens only at the start of a field, as [`parse`] reads
/// it.
pub fn separator(text: &str) -> char {
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0)];
    let mut quoted = false;
    let mut start = true;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                } else {
                    quoted = false;
                }
            }
            continue;
        }
        match c {
            '"' if start => quoted = true,
            '\n' | '\r' => break,
            c => {
                if let Some(count) = counts.iter_mut().find(|(s, _)| *s == c) {
                    count.1 += 1;
                    start = true;
                    continue;
                }
            }
        }
        start = false;
    }
    counts
        .iter()
        .filter(|(_, n)| *n > 0)
        .max_by_key(|(_, n)| *n)
        .map_or(',', |(s, _)| *s)
}

/// The rows of a text, split at its separator; a line that is empty is
/// no row.
pub fn parse(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let separator = separator(text);
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let finish_row = |row: &mut Vec<String>, field: &mut String, rows: &mut Vec<Vec<String>>| {
        if row.len() < MAX_FIELDS {
            row.push(std::mem::take(field));
        }
        field.clear();
        let row = std::mem::take(row);
        if !(row.len() == 1 && row.first().is_some_and(String::is_empty)) {
            rows.push(row);
        }
    };
    while let Some(c) = chars.next() {
        if rows.len() >= MAX_ROWS {
            break;
        }
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                '\r' if chars.peek() == Some(&'\n') => {}
                other => field.push(other),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                finish_row(&mut row, &mut field, &mut rows);
            }
            '\n' => finish_row(&mut row, &mut field, &mut rows),
            c if c == separator => {
                if row.len() < MAX_FIELDS {
                    row.push(std::mem::take(&mut field));
                } else {
                    field.clear();
                }
            }
            other => field.push(other),
        }
    }
    if (!field.is_empty() || !row.is_empty()) && rows.len() < MAX_ROWS {
        finish_row(&mut row, &mut field, &mut rows);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mal_05_rows_of_comma_separated_values() {
        let text = "\u{feff}First Name,Last Name,Address\r\nAna,\"Souza, Jr.\",\"Rua A, 1\nSão Paulo\"\r\n\r\n\"Bo \"\"B\"\"\",Lee,\n";
        assert_eq!(
            parse(text),
            [
                vec!["First Name", "Last Name", "Address"],
                vec!["Ana", "Souza, Jr.", "Rua A, 1\nSão Paulo"],
                vec!["Bo \"B\"", "Lee", ""],
            ]
        );
    }

    #[test]
    fn mal_05_the_separator_is_the_one_of_the_first_line() {
        assert_eq!(separator("a;b;\"c,d\"\n1,2,3"), ';');
        assert_eq!(separator("a\tb\tc"), '\t');
        assert_eq!(separator("one"), ',');
        assert_eq!(
            parse("Nome;Cidade\nAna;Lisboa"),
            [["Nome", "Cidade"], ["Ana", "Lisboa"]]
        );
        assert!(parse("").is_empty());
        // A quote inside a field opens nothing.
        assert_eq!(separator("Nome;Altura 5\";Cidade;Estado\na,b,c"), ';');
    }
}
