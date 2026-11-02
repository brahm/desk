// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Corrections made while typing (TXT-06, TXT-07): smart quotes, capitals
//! at the start of sentences, two capitals at the start of a word, raised
//! ordinal suffixes, and a table of replacements such as "(c)" for "©".
//!
//! The functions here look at the text before the cursor and say what to
//! change; the editor makes the change as part of the typing.

use std::ops::Range;

/// A replacement of the table, made when the text is typed as a word of
/// its own (TXT-07).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Replacement {
    /// The text typed.
    pub from: String,
    /// The text it becomes.
    pub to: String,
    /// Whether the replacement is made.
    pub enabled: bool,
}

impl Replacement {
    /// A replacement that is made.
    pub fn new(from: &str, to: &str) -> Self {
        Self {
            from: from.to_owned(),
            to: to.to_owned(),
            enabled: true,
        }
    }

    /// The table of a new install, as the symbols that Pages replaces.
    pub fn defaults() -> Vec<Self> {
        [
            ("(c)", "\u{a9}"),
            ("(r)", "\u{ae}"),
            ("(tm)", "\u{2122}"),
            ("c/o", "\u{2105}"),
            ("...", "\u{2026}"),
            ("1/2", "\u{bd}"),
            ("1/3", "\u{2153}"),
            ("2/3", "\u{2154}"),
            ("1/4", "\u{bc}"),
            ("3/4", "\u{be}"),
            ("1/8", "\u{215b}"),
            ("3/8", "\u{215c}"),
            ("5/8", "\u{215d}"),
            ("7/8", "\u{215e}"),
        ]
        .into_iter()
        .map(|(from, to)| Self::new(from, to))
        .collect()
    }
}

/// Which corrections are made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoCorrect {
    /// Capitals at the start of sentences, and one capital where a word
    /// starts with two.
    pub capitalization: bool,
    /// Ordinal suffixes raised, as 1st and 2nd.
    pub ordinals: bool,
    /// The table of replacements, used when `replace` is on.
    pub replacements: Vec<Replacement>,
    /// Whether the table is used.
    pub replace: bool,
}

impl Default for AutoCorrect {
    fn default() -> Self {
        Self {
            capitalization: true,
            ordinals: true,
            replacements: Replacement::defaults(),
            replace: true,
        }
    }
}

/// A change to the text before the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correction {
    /// The range of the text before the cursor to replace.
    pub range: Range<usize>,
    /// The new text.
    pub text: String,
    /// A part of the new text to raise as superscript, relative to it.
    pub superscript: Option<Range<usize>>,
}

/// Whether a character ends a word, so that typing it makes the
/// corrections of the word before it.
pub fn ends_word(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}')
}

/// Whether a quote after a character opens: at the start, after spaces and
/// after opening brackets, dashes and other quotes.
fn opens_after(before: Option<char>) -> bool {
    match before {
        None => true,
        Some(c) => {
            c.is_whitespace()
                || matches!(
                    c,
                    '(' | '[' | '{' | '\u{2014}' | '\u{2013}' | '-' | '/' | '\u{201c}' | '\u{2018}'
                )
        }
    }
}

/// The curly quote that a straight quote becomes after a character, or
/// the character itself when it is not a straight quote (TXT-06).
pub fn smart_quote(before: Option<char>, quote: char) -> char {
    let opens = opens_after(before);
    match (quote, opens) {
        ('"', true) => '\u{201c}',
        ('"', false) => '\u{201d}',
        ('\'', true) => '\u{2018}',
        // Closing quotes and apostrophes, as in "don't", are the same.
        ('\'', false) => '\u{2019}',
        (other, _) => other,
    }
}

/// The suffix of an English ordinal number, as "st" for 21.
fn ordinal_suffix(n: u64) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

impl AutoCorrect {
    /// The correction of the word that ends the text before the cursor,
    /// when a character that ends words is typed after it.
    pub fn correct(&self, before: &str) -> Option<Correction> {
        // The word: what follows the last space.
        let start = before
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(i, c)| i + c.len_utf8());
        let word = &before[start..];
        if word.is_empty() {
            return None;
        }
        if self.replace
            && let Some(found) = self
                .replacements
                .iter()
                .filter(|r| r.enabled && !r.from.is_empty() && Self::ends_with(word, &r.from))
                .max_by_key(|r| r.from.len())
        {
            return Some(Correction {
                range: before.len() - found.from.len()..before.len(),
                text: found.to.clone(),
                superscript: None,
            });
        }
        if self.ordinals
            && let Some(correction) = Self::ordinal(word)
        {
            return Some(Correction {
                range: start..before.len(),
                ..correction
            });
        }
        if self.capitalization {
            return Self::capitalize(&before[..start], word).map(|c| Correction {
                range: start + c.range.start..start + c.range.end,
                ..c
            });
        }
        None
    }

    /// Whether a word ends with the text of a replacement: the whole word,
    /// or its end for text without letters or digits, as "...".
    fn ends_with(word: &str, from: &str) -> bool {
        word == from || (word.ends_with(from) && !from.chars().any(char::is_alphanumeric))
    }

    /// 1st, 2nd, 3rd and 4th with the suffix raised.
    fn ordinal(word: &str) -> Option<Correction> {
        let digits = word.find(|c: char| !c.is_ascii_digit())?;
        let (number, suffix) = word.split_at(digits);
        let n: u64 = number.parse().ok()?;
        (suffix.eq_ignore_ascii_case(ordinal_suffix(n)) && suffix == suffix.to_lowercase()).then(
            || Correction {
                range: 0..word.len(),
                text: word.to_owned(),
                superscript: Some(digits..word.len()),
            },
        )
    }

    /// A capital for the first letter of a sentence, and one capital
    /// where a word starts with two, as "THe".
    fn capitalize(before: &str, word: &str) -> Option<Correction> {
        let letters: Vec<char> = word.chars().take(3).collect();
        if letters.len() == 3
            && letters[0].is_uppercase()
            && letters[1].is_uppercase()
            && letters[2].is_lowercase()
            && word.chars().skip(2).all(|c| !c.is_uppercase())
        {
            let first = letters[0].len_utf8();
            let second = letters[1].len_utf8();
            return Some(Correction {
                range: first..first + second,
                text: letters[1].to_lowercase().collect(),
                superscript: None,
            });
        }
        let first = word.chars().next()?;
        let sentence_start = before
            .trim_end()
            .chars()
            .next_back()
            .is_none_or(|c| matches!(c, '.' | '!' | '?'));
        // Only words of letters: "iPhone", "e.g." and "x2" stay.
        if sentence_start
            && first.is_lowercase()
            && word.chars().all(char::is_alphabetic)
            && word.chars().skip(1).all(char::is_lowercase)
            // Not after an abbreviation such as "e.g.", whose last word
            // has a dot inside.
            && !before
                .trim_end()
                .rsplit(char::is_whitespace)
                .next()
                .is_some_and(|w| w.trim_end_matches('.').contains('.'))
        {
            return Some(Correction {
                range: 0..first.len_utf8(),
                text: first.to_uppercase().collect(),
                superscript: None,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(before: &str) -> Option<String> {
        let correction = AutoCorrect::default().correct(before)?;
        let mut text = before.to_owned();
        text.replace_range(correction.range, &correction.text);
        Some(text)
    }

    #[test]
    fn quotes_open_and_close() {
        assert_eq!(smart_quote(None, '"'), '“');
        assert_eq!(smart_quote(Some(' '), '"'), '“');
        assert_eq!(smart_quote(Some('a'), '"'), '”');
        assert_eq!(smart_quote(Some('n'), '\''), '’');
        assert_eq!(smart_quote(Some('('), '\''), '‘');
        assert_eq!(smart_quote(Some('a'), 'x'), 'x');
    }

    #[test]
    fn sentences_start_with_capitals() {
        assert_eq!(apply("hello").as_deref(), Some("Hello"));
        assert_eq!(apply("It works. then").as_deref(), Some("It works. Then"));
        assert_eq!(apply("It works then"), None);
        assert_eq!(apply("as in e.g. this"), None);
        assert_eq!(apply("iPhone"), None);
        assert_eq!(apply("THe").as_deref(), Some("The"));
        assert_eq!(apply("USA"), None);
    }

    #[test]
    fn ordinals_raise_their_suffix() {
        let correction = AutoCorrect::default().correct("the 22nd").unwrap();
        assert_eq!(correction.range, 4..8);
        assert_eq!(correction.superscript, Some(2..4));
        assert!(AutoCorrect::default().correct("the 22th").is_none());
        let correction = AutoCorrect::default().correct("11th").unwrap();
        assert_eq!(correction.superscript, Some(2..4));
    }

    #[test]
    fn the_table_replaces_whole_words() {
        assert_eq!(apply("Desk (c)").as_deref(), Some("Desk ©"));
        assert_eq!(apply("Wait...").as_deref(), Some("Wait…"));
        assert_eq!(apply("x(c)"), None);
        let mut off = AutoCorrect::default();
        off.replacements[0].enabled = false;
        assert!(off.correct("(c)").is_none());
    }
}
