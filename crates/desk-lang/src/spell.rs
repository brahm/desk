// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spelling with Hunspell dictionaries (TXT-10), read by Spellbook.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::charset;

/// A spelling dictionary.
pub struct Speller {
    dictionary: spellbook::Dictionary,
}

impl std::fmt::Debug for Speller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Speller").finish_non_exhaustive()
    }
}

impl Speller {
    /// A dictionary from the text of its `.aff` and `.dic` files, or
    /// `None` when they cannot be read.
    pub fn new(aff: &str, dic: &str) -> Option<Self> {
        let strip = |t: &str| t.strip_prefix('\u{feff}').unwrap_or(t).to_owned();
        spellbook::Dictionary::new(&strip(aff), &strip(dic))
            .ok()
            .map(|dictionary| Self { dictionary })
    }

    /// A dictionary from the bytes of its `.aff` and `.dic` files, in the
    /// encoding that the `SET` line of the `.aff` file names: many
    /// dictionaries of the system are in ISO 8859, as Greek, Polish,
    /// Slovenian and Norwegian are in Debian and Ubuntu.
    pub fn from_bytes(aff: &[u8], dic: &[u8]) -> Option<Self> {
        let encoding = declared_encoding(aff);
        Self::new(
            &charset::decode(aff, encoding),
            &charset::decode(dic, encoding),
        )
    }

    /// Whether a word is spelled right.
    pub fn check(&self, word: &str) -> bool {
        self.dictionary.check(word)
    }

    /// The words that the dictionary suggests for a misspelled one, the
    /// likeliest first.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.dictionary.suggest(word, &mut out);
        out
    }
}

/// The encoding that the `SET` line of an `.aff` file names, UTF-8 when
/// there is none or it is unknown.
fn declared_encoding(aff: &[u8]) -> &'static encoding_rs::Encoding {
    aff.split(|&b| b == b'\n')
        .find_map(|line| line.strip_prefix(b"SET"))
        .and_then(|rest| {
            let rest = String::from_utf8_lossy(rest);
            charset::named(rest.split_whitespace().next()?)
        })
        .unwrap_or(encoding_rs::UTF_8)
}

/// The words of a text that a spelling checker looks at, by their byte
/// ranges: words with letters and no digits, with the apostrophes inside
/// them, as in "don't".
pub fn words(text: &str) -> Vec<Range<usize>> {
    text.unicode_word_indices()
        .filter(|(_, w)| {
            w.chars().any(char::is_alphabetic) && !w.chars().any(|c| c.is_ascii_digit())
        })
        .map(|(start, w)| start..start + w.len())
        .collect()
}

/// The ranges of the words of a text that `correct` says are misspelled.
pub fn misspelled_words(text: &str, mut correct: impl FnMut(&str) -> bool) -> Vec<Range<usize>> {
    words(text)
        .into_iter()
        .filter(|range| !correct(&text[range.clone()]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFF: &str = "SET UTF-8\nSFX S Y 1\nSFX S 0 s .\n";
    const DIC: &str = "3\ncat/S\ndog\ndon't\n";

    #[test]
    fn words_skip_numbers() {
        let text = "Cats, 2nd dog's x2 \u{e9}t\u{e9}";
        let found: Vec<&str> = words(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(found, ["Cats", "dog's", "\u{e9}t\u{e9}"]);
    }

    #[test]
    fn a_dictionary_checks_and_suggests() {
        let speller = Speller::new(AFF, DIC).unwrap();
        assert!(speller.check("cats"));
        assert!(speller.check("Dog"));
        assert!(!speller.check("dgo"));
        assert!(speller.suggest("dgo").contains(&"dog".to_owned()));
        let text = "The cats dont bark";
        let wrong: Vec<&str> = misspelled_words(text, |w| speller.check(w))
            .into_iter()
            .map(|r| &text[r])
            .collect();
        assert_eq!(wrong, ["The", "dont", "bark"]);
    }

    #[test]
    fn dictionaries_in_iso_8859_are_decoded() {
        // "łódź" and "żółw" in ISO 8859-2.
        let aff = b"SET ISO8859-2\nSFX S Y 1\nSFX S 0 s .\n";
        let dic = b"2\n\xb3\xf3d\xbc\n\xbf\xf3\xb3w\n";
        let speller = Speller::from_bytes(aff, dic).unwrap();
        assert!(speller.check("\u{142}\u{f3}d\u{17a}"));
        assert!(speller.check("\u{17c}\u{f3}\u{142}w"));
        assert!(!speller.check("lodz"));
    }

    #[test]
    fn stray_bytes_in_a_utf8_dictionary_are_replaced() {
        // A comment in Latin-1 in a dictionary that says it is UTF-8.
        let aff = b"# by L\xe1szl\xf3\nSET UTF-8\nSFX S Y 1\nSFX S 0 s .\n";
        let dic = "1\nma\u{e7}\u{e3}/S\n".as_bytes();
        let speller = Speller::from_bytes(aff, dic).unwrap();
        assert!(speller.check("ma\u{e7}\u{e3}s"));
    }

    #[test]
    fn the_dictionaries_of_the_system_check_words() {
        if let Some(pt) = crate::speller("pt-BR") {
            assert!(pt.check("computador"));
            assert!(pt.check("ação"));
            assert!(!pt.check("computadro"));
        }
        if let Some(en) = crate::speller("en-US") {
            assert!(en.check("hyphenation"));
            assert!(en.suggest("teh").contains(&"the".to_owned()));
        }
    }
}
