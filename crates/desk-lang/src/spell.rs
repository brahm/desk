// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spelling with Hunspell dictionaries (TXT-10), read by Spellbook.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

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
