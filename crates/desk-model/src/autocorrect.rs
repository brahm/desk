// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Corrections made while typing (TXT-06, TXT-07): smart quotes, capitals
//! at the start of sentences, raised ordinal suffixes, fractions such as
//! "1/2" for "½" (PXV-07), and a table of replacements such as "(c)" for
//! "©".
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

    /// The table of a new install, as Pages has it: the symbols on and the
    /// fractions off.
    pub fn defaults() -> Vec<Self> {
        let on = [
            ("(c)", "\u{a9}"),
            ("(p)", "\u{2117}"),
            ("(r)", "\u{ae}"),
            ("TM", "\u{2122}"),
            ("c/o", "\u{2105}"),
            ("...", "\u{2026}"),
            ("teh", "the"),
        ];
        let off = [
            ("1/2", "\u{bd}"),
            ("1/3", "\u{2153}"),
            ("2/3", "\u{2154}"),
            ("1/4", "\u{bc}"),
            ("3/4", "\u{be}"),
            ("1/5", "\u{2155}"),
            ("2/5", "\u{2156}"),
            ("3/5", "\u{2157}"),
            ("4/5", "\u{2158}"),
            ("1/6", "\u{2159}"),
            ("5/6", "\u{215a}"),
            ("1/8", "\u{215b}"),
            ("3/8", "\u{215c}"),
            ("5/8", "\u{215d}"),
            ("7/8", "\u{215e}"),
        ];
        on.into_iter()
            .map(|(from, to)| Self::new(from, to))
            .chain(off.into_iter().map(|(from, to)| Self {
                enabled: false,
                ..Self::new(from, to)
            }))
            .collect()
    }
}

/// Which corrections are made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoCorrect {
    /// Capitals at the start of sentences.
    pub capitalization: bool,
    /// Ordinal suffixes raised, as 1st and 2nd.
    pub ordinals: bool,
    /// Fractions typed with a slash made into the fraction characters of
    /// Unicode, as "Automatically format fractions" of Pages 15.4 does
    /// (PXV-07).
    pub fractions: bool,
    /// The table of replacements, used when `replace` is on.
    pub replacements: Vec<Replacement>,
    /// Whether the table is used.
    pub replace: bool,
}

impl Default for AutoCorrect {
    /// What Pages does on a new install: none of these, though the table
    /// is ready for when it is turned on.
    fn default() -> Self {
        Self {
            capitalization: false,
            ordinals: false,
            fractions: false,
            replacements: Replacement::defaults(),
            replace: false,
        }
    }
}

/// The fractions that Unicode has a character of their own for, the only
/// ones that Pages 15.4 formats: 13/16 and 2/4 stay as typed.
const FRACTIONS: &[(&str, char)] = &[
    ("1/2", '\u{bd}'),
    ("1/3", '\u{2153}'),
    ("2/3", '\u{2154}'),
    ("1/4", '\u{bc}'),
    ("3/4", '\u{be}'),
    ("1/5", '\u{2155}'),
    ("2/5", '\u{2156}'),
    ("3/5", '\u{2157}'),
    ("4/5", '\u{2158}'),
    ("1/6", '\u{2159}'),
    ("5/6", '\u{215a}'),
    ("1/7", '\u{2150}'),
    ("1/8", '\u{215b}'),
    ("3/8", '\u{215c}'),
    ("5/8", '\u{215d}'),
    ("7/8", '\u{215e}'),
    ("1/9", '\u{2151}'),
    ("1/10", '\u{2152}'),
    ("0/3", '\u{2189}'),
];

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

/// A pair of quotes, the one that opens and the one that closes, as the
/// lists of Double Quotes and Single Quotes of Pages 15.4 offer them
/// (PXV-08).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Quotes {
    /// The quote that opens.
    pub open: char,
    /// The quote that closes.
    pub close: char,
}

impl Quotes {
    /// A pair of quotes.
    pub const fn new(open: char, close: char) -> Self {
        Self { open, close }
    }

    /// The double quotes to choose from, “abc” first, as most languages
    /// write them: „abc“ in German, „abc” in Polish, ”abc” in Swedish,
    /// «abc» in French and Portuguese of Portugal, »abc« in Danish.
    pub const DOUBLE: [Self; 6] = [
        Self::new('\u{201c}', '\u{201d}'),
        Self::new('\u{201e}', '\u{201c}'),
        Self::new('\u{201e}', '\u{201d}'),
        Self::new('\u{201d}', '\u{201d}'),
        Self::new('\u{ab}', '\u{bb}'),
        Self::new('\u{bb}', '\u{ab}'),
    ];

    /// The single quotes to choose from, ‘abc’ first, in the same order
    /// as the double ones.
    pub const SINGLE: [Self; 6] = [
        Self::new('\u{2018}', '\u{2019}'),
        Self::new('\u{201a}', '\u{2018}'),
        Self::new('\u{201a}', '\u{2019}'),
        Self::new('\u{2019}', '\u{2019}'),
        Self::new('\u{2039}', '\u{203a}'),
        Self::new('\u{203a}', '\u{2039}'),
    ];

    /// The pair as it is shown and kept: the quotes around "abc".
    pub fn sample(self) -> String {
        format!("{}abc{}", self.open, self.close)
    }

    /// The pair that a sample such as "“abc”" names, among `choices`.
    pub fn from_sample(sample: &str, choices: &[Self]) -> Option<Self> {
        choices.iter().copied().find(|q| q.sample() == sample)
    }
}

/// The quotes that smart quotes put in place of straight ones (TXT-06,
/// PXV-08).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QuoteStyle {
    /// The double quotes.
    pub double: Quotes,
    /// The single quotes.
    pub single: Quotes,
}

impl Default for QuoteStyle {
    /// “abc” and ‘abc’, as Pages has them on a new install.
    fn default() -> Self {
        Self {
            double: Quotes::DOUBLE[0],
            single: Quotes::SINGLE[0],
        }
    }
}

/// The apostrophe, which a single quote inside a word always becomes.
const APOSTROPHE: char = '\u{2019}';

/// Whether a character is a quote that smart quotes make or replace.
pub fn is_quote(c: char) -> bool {
    matches!(c, '"' | '\'')
        || Quotes::DOUBLE
            .iter()
            .chain(&Quotes::SINGLE)
            .any(|q| q.open == c || q.close == c)
}

impl QuoteStyle {
    /// Whether a quote after a character opens: at the start, after spaces
    /// and after opening brackets, dashes and other opening quotes.
    fn opens_after(self, before: Option<char>) -> bool {
        match before {
            None => true,
            Some(c) => {
                c.is_whitespace()
                    || matches!(c, '(' | '[' | '{' | '\u{2014}' | '\u{2013}' | '-' | '/')
                    || (c == self.double.open && c != self.double.close)
                    || (c == self.single.open && c != self.single.close)
            }
        }
    }

    /// The curly quote that a straight quote becomes after the text
    /// `before`, or the character itself when it is not a straight quote
    /// (TXT-06). A single quote that does not open is an apostrophe, as in
    /// "don’t", unless a single quote of the style is open before it.
    pub fn smart_quote(self, before: &str, quote: char) -> char {
        let opens = self.opens_after(before.chars().next_back());
        match quote {
            '"' if opens => self.double.open,
            '"' => self.double.close,
            '\'' if opens => self.single.open,
            '\'' if self.single.close == APOSTROPHE || !self.single_open(before) => APOSTROPHE,
            '\'' => self.single.close,
            other => other,
        }
    }

    /// Whether a single quote of the style opened in the text and is not
    /// closed yet.
    fn single_open(self, text: &str) -> bool {
        let mut depth = 0usize;
        for c in text.chars() {
            if c == self.single.open {
                depth += 1;
            } else if c == self.single.close {
                depth = depth.saturating_sub(1);
            }
        }
        depth > 0
    }

    /// The text with its quotes, straight or curly, made again in this
    /// style, given the text before it (PXV-08).
    pub fn convert(self, before: &str, text: &str) -> String {
        let mut context = before.to_owned();
        let start = context.len();
        for c in text.chars() {
            let straight = match c {
                '"' | '\'' => Some(c),
                _ if Quotes::DOUBLE.iter().any(|q| q.open == c || q.close == c) => Some('"'),
                _ if Quotes::SINGLE.iter().any(|q| q.open == c || q.close == c) => Some('\''),
                _ => None,
            };
            let made = straight.map_or(c, |q| self.smart_quote(&context, q));
            context.push(made);
        }
        context.split_off(start)
    }
}

/// Whether a hyphen typed after the text makes, with the hyphen before
/// it, an em dash, as the smart dashes of Pages 15.4 do with "--".
pub fn smart_dash(before: &str, typed: char) -> bool {
    typed == '-' && before.ends_with('-')
}

/// The curly quote that a straight quote becomes after a character, in
/// the quotes of a new install, or the character itself when it is not a
/// straight quote (TXT-06).
pub fn smart_quote(before: Option<char>, quote: char) -> char {
    let before = before.map(String::from).unwrap_or_default();
    QuoteStyle::default().smart_quote(&before, quote)
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
        if self.fractions
            && let Some(correction) = Self::fraction(before)
        {
            return Some(correction);
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

    /// The fraction character for the fraction that ends the text, when
    /// it stands apart: after a letter or another digit, as in "x1/2" or
    /// "11/2", it stays (PXV-07).
    fn fraction(before: &str) -> Option<Correction> {
        let digits = |s: &str| s.len() - s.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let denominator = digits(before);
        let rest = before[..before.len() - denominator].strip_suffix('/')?;
        let numerator = digits(rest);
        let start = rest.len() - numerator;
        if denominator == 0
            || numerator == 0
            || rest[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '/')
        {
            return None;
        }
        let typed = &before[start..];
        let (_, character) = FRACTIONS.iter().find(|(f, _)| *f == typed)?;
        Some(Correction {
            range: start..before.len(),
            text: character.to_string(),
            superscript: None,
        })
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

    /// A capital for the first letter of a sentence. As in Pages, a word
    /// that starts with two capitals stays as it is, and a dot ends the
    /// sentence even after an abbreviation such as "i.e.".
    fn capitalize(before: &str, word: &str) -> Option<Correction> {
        let first = word.chars().next()?;
        let sentence_start = before
            .trim_end()
            .chars()
            .next_back()
            .is_none_or(|c| matches!(c, '.' | '!' | '?'));
        // Only words of letters: "iPhone", "e.g." and "x2" stay.
        (sentence_start
            && first.is_lowercase()
            && word.chars().all(char::is_alphabetic)
            && word.chars().skip(1).all(char::is_lowercase))
        .then(|| Correction {
            range: 0..first.len_utf8(),
            text: first.to_uppercase().collect(),
            superscript: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every correction on, as after turning them on in the preferences.
    fn all() -> AutoCorrect {
        AutoCorrect {
            capitalization: true,
            ordinals: true,
            fractions: true,
            replacements: Replacement::defaults(),
            replace: true,
        }
    }

    fn apply(before: &str) -> Option<String> {
        let correction = all().correct(before)?;
        let mut text = before.to_owned();
        text.replace_range(correction.range, &correction.text);
        Some(text)
    }

    #[test]
    fn txt_06_quotes_open_and_close() {
        assert_eq!(smart_quote(None, '"'), '“');
        assert_eq!(smart_quote(Some(' '), '"'), '“');
        assert_eq!(smart_quote(Some('a'), '"'), '”');
        assert_eq!(smart_quote(Some('n'), '\''), '’');
        assert_eq!(smart_quote(Some('('), '\''), '‘');
        assert_eq!(smart_quote(Some('a'), 'x'), 'x');
    }

    #[test]
    fn pxv_08_the_quotes_follow_the_style() {
        let german = QuoteStyle {
            double: Quotes::DOUBLE[1],
            single: Quotes::SINGLE[1],
        };
        assert_eq!(german.smart_quote("", '"'), '„');
        assert_eq!(german.smart_quote("„Ja", '"'), '“');
        assert_eq!(german.smart_quote("sagt ", '\''), '‚');
        assert_eq!(german.smart_quote("sagt ‚ja", '\''), '‘');
        // Inside a word, it is an apostrophe.
        assert_eq!(german.smart_quote("Rock", '\''), '’');
        let french = QuoteStyle {
            double: Quotes::DOUBLE[4],
            single: Quotes::SINGLE[4],
        };
        assert_eq!(french.smart_quote("«", '\''), '‹');
        assert_eq!(french.smart_quote("l", '\''), '’');
        assert_eq!(Quotes::DOUBLE[5].sample(), "»abc«");
        assert_eq!(
            Quotes::from_sample("„abc“", &Quotes::DOUBLE),
            Some(Quotes::DOUBLE[1])
        );
        assert!(is_quote('»') && is_quote('"') && !is_quote('a'));
    }

    #[test]
    fn pxv_08_two_hyphens_make_a_dash() {
        assert!(smart_dash("wait-", '-'));
        assert!(!smart_dash("wait", '-'));
        assert!(!smart_dash("wait-", 'a'));
    }

    #[test]
    fn pxv_08_quotes_already_typed_are_converted() {
        let style = QuoteStyle::default();
        assert_eq!(
            style.convert("", "\"Don't,\" she said 'twice'."),
            "“Don’t,” she said ‘twice’."
        );
        // From one style to another, with the text before it.
        let french = QuoteStyle {
            double: Quotes::DOUBLE[4],
            single: Quotes::SINGLE[4],
        };
        assert_eq!(french.convert("Il dit ", "“l’eau”"), "«l’eau»");
        assert_eq!(style.convert("“", "abc\""), "abc”");
    }

    #[test]
    fn sentences_start_with_capitals() {
        assert_eq!(apply("hello").as_deref(), Some("Hello"));
        assert_eq!(apply("It works. then").as_deref(), Some("It works. Then"));
        assert_eq!(apply("It works then"), None);
        // Pages takes the dot of "i.e." as the end of a sentence.
        assert_eq!(apply("as in i.e. not").as_deref(), Some("as in i.e. Not"));
        assert_eq!(apply("iPhone"), None);
        assert_eq!(apply("e.g."), None);
        // Nor does it change two capitals at the start of a word.
        assert_eq!(apply("THis"), None);
        assert_eq!(apply("USA"), None);
    }

    #[test]
    fn ordinals_raise_their_suffix() {
        let correction = all().correct("the 22nd").unwrap();
        assert_eq!(correction.range, 4..8);
        assert_eq!(correction.superscript, Some(2..4));
        assert!(all().correct("the 22th").is_none());
        let correction = all().correct("11th").unwrap();
        assert_eq!(correction.superscript, Some(2..4));
    }

    #[test]
    fn the_table_replaces_whole_words() {
        assert_eq!(apply("Desk (c)").as_deref(), Some("Desk ©"));
        assert_eq!(apply("Wait...").as_deref(), Some("Wait…"));
        assert_eq!(apply("x(c)"), None);
        assert_eq!(apply("Desk TM").as_deref(), Some("Desk ™"));
        assert_eq!(apply("teh").as_deref(), Some("the"));
        // The fractions are in the table but off, as in Pages.
        let mut table = all();
        table.fractions = false;
        assert!(table.correct("1/2").is_none());
        let mut off = all();
        off.replacements[0].enabled = false;
        assert!(off.correct("(c)").is_none());
    }

    #[test]
    fn pxv_07_fractions_become_their_characters() {
        assert_eq!(apply("1/2").as_deref(), Some("½"));
        assert_eq!(apply("add 5/8").as_deref(), Some("add ⅝"));
        assert_eq!(apply("0/3").as_deref(), Some("↉"));
        assert_eq!(apply("1/10").as_deref(), Some("⅒"));
        // Before a dot and inside parentheses, as the dot or the bracket
        // is typed.
        assert_eq!(apply("(1/2").as_deref(), Some("(½"));
        // Without a character of their own, inside a word or after
        // another digit, they stay.
        assert_eq!(apply("13/16"), None);
        assert_eq!(apply("2/4"), None);
        assert_eq!(apply("x1/2"), None);
        assert_eq!(apply("11/2"), None);
        assert_eq!(apply("1/2/3"), None);
        let mut off = all();
        off.fractions = false;
        assert!(off.correct("1/2").is_none());
    }

    #[test]
    fn txt_07_a_new_install_corrects_nothing_as_pages() {
        let fresh = AutoCorrect::default();
        assert!(fresh.correct("hello").is_none());
        assert!(fresh.correct("1st").is_none());
        assert!(fresh.correct("(c)").is_none());
        assert!(fresh.correct("1/2").is_none());
    }
}
