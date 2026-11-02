// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Hyphenation with Liang's patterns, from the files of LibreOffice
//! (TXT-09).
//!
//! A file starts with its character set, `UTF-8` or `ISO8859-1`, may set
//! how many letters a word keeps before and after a hyphen, and lists
//! patterns such as `.ad4der`: the letters, with a digit between two of
//! them where a break is allowed (odd) or forbidden (even). Each place in
//! a word takes the highest digit of every pattern that covers it. The
//! patterns of compound words (after `NEXTLEVEL`) and the replacement
//! patterns with a `/` are left out.

use std::collections::HashMap;

/// The hyphenation patterns of a language.
#[derive(Clone, Debug, Default)]
pub struct Patterns {
    /// The digits of each pattern, by its letters: one more than the
    /// letters, for the places before, between and after them.
    patterns: HashMap<String, Vec<u8>>,
    /// The letters of the longest pattern.
    longest: usize,
    /// How many letters a word keeps before the first hyphen.
    pub left_min: usize,
    /// How many letters a word keeps after the last hyphen.
    pub right_min: usize,
}

/// The text of a file in its character set.
fn decode(data: &[u8]) -> Option<String> {
    let first = data.split(|b| *b == b'\n').next()?;
    let charset = String::from_utf8_lossy(first).trim().to_ascii_uppercase();
    let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
    match charset.trim_start_matches('\u{feff}') {
        "UTF-8" | "UTF8" => String::from_utf8(data.to_vec()).ok(),
        "ISO8859-1" | "ISO-8859-1" | "LATIN1" => {
            Some(data.iter().map(|&b| char::from(b)).collect())
        }
        _ => None,
    }
}

impl Patterns {
    /// Reads a file of patterns, or gives `None` when its character set is
    /// not one that Desk reads.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let text = decode(data)?;
        let mut out = Self {
            left_min: 2,
            right_min: 2,
            ..Self::default()
        };
        for line in text.lines().skip(1) {
            let line = line.trim();
            let mut words = line.split_whitespace();
            match words.next() {
                None => continue,
                Some("NEXTLEVEL") => break,
                Some("LEFTHYPHENMIN") => {
                    out.left_min = words.next()?.parse().ok()?;
                    continue;
                }
                Some("RIGHTHYPHENMIN") => {
                    out.right_min = words.next()?.parse().ok()?;
                    continue;
                }
                Some(word) if word.starts_with('%') || word.contains('/') => continue,
                Some(word) if word.chars().next().is_some_and(char::is_uppercase) => continue,
                Some(word) => out.add(word),
            }
        }
        out.left_min = out.left_min.max(1);
        out.right_min = out.right_min.max(1);
        Some(out)
    }

    fn add(&mut self, pattern: &str) {
        let mut letters = String::new();
        let mut digits = vec![0u8];
        for c in pattern.chars() {
            match c.to_digit(10) {
                Some(d) => {
                    if let Some(last) = digits.last_mut() {
                        *last = d as u8;
                    }
                }
                None => {
                    letters.push(c);
                    digits.push(0);
                }
            }
        }
        let count = letters.chars().count();
        if count == 0 {
            return;
        }
        self.longest = self.longest.max(count);
        self.patterns.insert(letters, digits);
    }

    /// How many patterns there are.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Whether there are no patterns.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// The byte offsets in a word where it may break with a hyphen.
    pub fn hyphenate(&self, word: &str) -> Vec<usize> {
        let chars: Vec<(usize, char)> = word.char_indices().collect();
        let n = chars.len();
        if n < self.left_min + self.right_min || self.patterns.is_empty() {
            return Vec::new();
        }
        // The word in lower case, one letter for each, between dots.
        let mut padded = vec!['.'];
        padded.extend(
            chars
                .iter()
                .map(|(_, c)| c.to_lowercase().next().unwrap_or(*c)),
        );
        padded.push('.');
        let mut values = vec![0u8; padded.len() + 1];
        let mut key = String::new();
        for start in 0..padded.len() {
            key.clear();
            for &c in padded.iter().skip(start).take(self.longest) {
                key.push(c);
                if let Some(digits) = self.patterns.get(&key) {
                    for (i, d) in digits.iter().enumerate() {
                        let place = &mut values[start + i];
                        *place = (*place).max(*d);
                    }
                }
            }
        }
        // The place before letter k of the word is values[k + 1], since
        // the dot comes first.
        (self.left_min..=n - self.right_min)
            .filter(|&k| values[k + 1] % 2 == 1)
            .map(|k| chars[k].0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The patterns of Liang's thesis that break "hyphenation".
    const PATTERNS: &str = "UTF-8\nLEFTHYPHENMIN 2\nRIGHTHYPHENMIN 3\n\
        hy3ph\nhe2n\nhena4\nhen5at\n1na\nn2at\n1tio\n2io\no2n\n";

    fn breaks(patterns: &Patterns, word: &str) -> String {
        let mut out = String::new();
        let points = patterns.hyphenate(word);
        for (i, c) in word.char_indices() {
            if points.contains(&i) {
                out.push('-');
            }
            out.push(c);
        }
        out
    }

    #[test]
    fn liang_breaks_hyphenation() {
        let patterns = Patterns::parse(PATTERNS.as_bytes()).unwrap();
        assert_eq!(patterns.len(), 9);
        assert_eq!(breaks(&patterns, "hyphenation"), "hy-phen-ation");
        assert_eq!(breaks(&patterns, "Hyphenation"), "Hy-phen-ation");
        // Too short to break.
        assert_eq!(breaks(&patterns, "hyph"), "hyph");
    }

    #[test]
    fn latin_1_files_are_read() {
        let mut data = b"ISO8859-1\n1\xe7a\n".to_vec();
        data.extend_from_slice(b"a1b\n");
        let patterns = Patterns::parse(&data).unwrap();
        assert_eq!(breaks(&patterns, "aaçaab"), "aa-çaab");
        assert!(Patterns::parse(b"KOI8-R\na1b\n").is_none());
    }

    #[test]
    fn the_patterns_of_the_system_break_words() {
        // Only where the system has them, as in CI.
        if let Some(en) = crate::patterns("en-US") {
            assert_eq!(breaks(&en, "hyphenation"), "hy-phen-ation");
        }
        if let Some(pt) = crate::patterns("pt-BR") {
            assert_eq!(breaks(&pt, "computador"), "com-pu-ta-dor");
        }
    }
}
