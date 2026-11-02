// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The proofreader (TXT-11): mistakes of writing that are not spelling,
//! in any language. It finds repeated words, extra spaces, spaces before
//! punctuation, missing spaces after it, and sentences that start with a
//! small letter, each with the change that fixes it. In English it also
//! finds wordy phrases, jargon and gendered terms ([`style_issues`]).

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// The kinds of mistake.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IssueKind {
    /// A word written twice in a row, as "the the".
    RepeatedWord,
    /// More than one space between words.
    ExtraSpaces,
    /// A space before a comma, a period or another mark.
    SpaceBeforePunctuation,
    /// No space after a comma or the end of a sentence.
    MissingSpace,
    /// A sentence that starts with a small letter.
    Capitalization,
    /// A phrase that says in many words what fewer say, as "in order to".
    Wordy,
    /// A word of jargon, as "utilize".
    Jargon,
    /// A term that names a man or a woman where it means anyone, as
    /// "chairman".
    Gendered,
}

/// A mistake of a text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// What is wrong.
    pub kind: IssueKind,
    /// The byte range of the text that is wrong.
    pub range: Range<usize>,
    /// The text that replaces the range to fix it.
    pub replacement: String,
}

fn is_mark(c: char) -> bool {
    matches!(c, ',' | '.' | ';' | ':' | '!' | '?')
}

/// The mistakes of a paragraph, in order.
pub fn proofread(text: &str) -> Vec<Issue> {
    let mut out = Vec::new();
    repeated_words(text, &mut out);
    spaces(text, &mut out);
    capitals(text, &mut out);
    out.sort_by_key(|i| (i.range.start, i.range.end));
    out.dedup_by(|b, a| a.range.end > b.range.start);
    out
}

fn repeated_words(text: &str, out: &mut Vec<Issue>) {
    let words: Vec<(usize, &str)> = text.unicode_word_indices().collect();
    for pair in words.windows(2) {
        let ((a, first), (b, second)) = (pair[0], pair[1]);
        let between = &text[a + first.len()..b];
        if first.chars().any(char::is_alphabetic)
            && first.to_lowercase() == second.to_lowercase()
            && !between.is_empty()
            && between.chars().all(|c| c == ' ')
        {
            out.push(Issue {
                kind: IssueKind::RepeatedWord,
                range: a..b + second.len(),
                replacement: first.to_owned(),
            });
        }
    }
}

fn spaces(text: &str, out: &mut Vec<Issue>) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        if c == ' ' {
            let mut j = i;
            while j < chars.len() && chars[j].1 == ' ' {
                j += 1;
            }
            let end = chars.get(j).map_or(text.len(), |(o, _)| *o);
            let after = chars.get(j).map(|(_, c)| *c);
            // Not the spaces that indent a line, nor those at its end.
            if i > 0 && after.is_some_and(is_mark) {
                out.push(Issue {
                    kind: IssueKind::SpaceBeforePunctuation,
                    range: at..end,
                    replacement: String::new(),
                });
            } else if i > 0 && j - i > 1 && after.is_some() {
                out.push(Issue {
                    kind: IssueKind::ExtraSpaces,
                    range: at..end,
                    replacement: " ".to_owned(),
                });
            }
            i = j;
            continue;
        }
        // A mark between a letter and a letter: a comma or a semicolon
        // anywhere, the end of a sentence before a capital, as in
        // "end.Next", and not "e.g." or "example.com".
        if is_mark(c)
            && let (Some(&(_, before)), Some(&(next_at, next))) = (
                i.checked_sub(1).and_then(|p| chars.get(p)),
                chars.get(i + 1),
            )
        {
            let missing = before.is_alphabetic()
                && match c {
                    ',' | ';' => next.is_alphabetic(),
                    '.' | '!' | '?' => {
                        next.is_uppercase()
                            && before.is_lowercase()
                            && word_before(text, at).chars().count() > 1
                    }
                    _ => false,
                };
            if missing {
                out.push(Issue {
                    kind: IssueKind::MissingSpace,
                    range: at..next_at,
                    replacement: format!("{c} "),
                });
            }
        }
        i += 1;
    }
}

/// The letters right before an offset.
fn word_before(text: &str, at: usize) -> &str {
    let start = text[..at]
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_alphabetic())
        .map_or(0, |(i, c)| i + c.len_utf8());
    &text[start..at]
}

fn capitals(text: &str, out: &mut Vec<Issue>) {
    let mut sentence_start = true;
    let mut last_word = String::new();
    for (at, word) in text.split_word_bound_indices() {
        let first = word.chars().next().unwrap_or(' ');
        if first.is_alphabetic() {
            if sentence_start && first.is_lowercase() && word.chars().all(char::is_alphabetic) {
                out.push(Issue {
                    kind: IssueKind::Capitalization,
                    range: at..at + first.len_utf8(),
                    replacement: first.to_uppercase().collect(),
                });
            }
            sentence_start = false;
            last_word = word.to_owned();
        } else if matches!(first, '.' | '!' | '?') {
            // Not after one letter, as in "J. Smith", nor after "e.g.".
            sentence_start =
                first != '.' || (last_word.chars().count() > 1 && !last_word.contains('.'));
        } else if !first.is_whitespace() && !matches!(first, '"' | '\u{201c}' | '(' | '\'') {
            sentence_start = false;
        }
    }
}

/// The rules of style of a language: the kind, the phrase and its
/// replacement.
fn rules(language: &str) -> &'static [(IssueKind, &'static str, &'static str)] {
    use std::sync::OnceLock;
    static ENGLISH: OnceLock<Vec<(IssueKind, &str, &str)>> = OnceLock::new();
    let primary = language.split(['-', '_']).next().unwrap_or("");
    if !primary.eq_ignore_ascii_case("en") {
        return &[];
    }
    ENGLISH.get_or_init(|| {
        let mut rules: Vec<_> = include_str!("en_style.tsv")
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let kind = match fields.next()? {
                    "wordy" => IssueKind::Wordy,
                    "jargon" => IssueKind::Jargon,
                    "gendered" => IssueKind::Gendered,
                    _ => return None,
                };
                Some((kind, fields.next()?, fields.next()?))
            })
            .collect();
        // The longest phrase first, so that it wins where two start at
        // the same word.
        rules.sort_by_key(|(_, phrase, _)| std::cmp::Reverse(phrase.len()));
        rules
    })
}

/// The mistakes of style of a text in a language, in order: wordy
/// phrases, jargon and gendered terms. Only English has these rules; a
/// text in another language has none.
pub fn style_issues(text: &str, language: &str) -> Vec<Issue> {
    let rules = rules(language);
    let mut out: Vec<Issue> = Vec::new();
    if rules.is_empty() {
        return out;
    }
    let bound = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '\'');
    for (at, _) in text.unicode_word_indices() {
        if out.last().is_some_and(|last| last.range.end > at) {
            continue;
        }
        if !bound(text[..at].chars().next_back()) {
            continue;
        }
        let rest = &text[at..];
        let found = rules.iter().find(|(_, phrase, _)| {
            rest.get(..phrase.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(phrase))
                && bound(rest[phrase.len()..].chars().next())
        });
        if let Some((kind, phrase, replacement)) = found {
            let capital = rest.starts_with(|c: char| c.is_uppercase());
            let replacement = if capital {
                let mut chars = replacement.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_uppercase().chain(chars).collect()
                })
            } else {
                (*replacement).to_owned()
            };
            out.push(Issue {
                kind: *kind,
                range: at..at + phrase.len(),
                replacement,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(text: &str) -> String {
        let mut out = text.to_owned();
        for issue in proofread(text).iter().rev() {
            out.replace_range(issue.range.clone(), &issue.replacement);
        }
        out
    }

    fn kinds(text: &str) -> Vec<IssueKind> {
        proofread(text).iter().map(|i| i.kind).collect()
    }

    #[test]
    fn repeated_words_and_spaces() {
        assert_eq!(kinds("It was the the end"), [IssueKind::RepeatedWord]);
        assert_eq!(fixed("It was the the end"), "It was the end");
        assert_eq!(fixed("One  two"), "One two");
        assert_eq!(fixed("Yes , no ."), "Yes, no.");
        assert_eq!(fixed("Yes,no"), "Yes, no");
        assert_eq!(fixed("The end.Next one"), "The end. Next one");
    }

    #[test]
    fn sentences_start_with_capitals() {
        assert_eq!(fixed("one. two! three"), "One. Two! Three");
        assert_eq!(fixed("Mr J. smith"), "Mr J. smith");
        assert_eq!(fixed("as in e.g. this"), "As in e.g. this");
    }

    fn styled(text: &str) -> String {
        let mut out = text.to_owned();
        for issue in style_issues(text, "en-US").iter().rev() {
            out.replace_range(issue.range.clone(), &issue.replacement);
        }
        out
    }

    #[test]
    fn txt_11_english_style_rules() {
        assert_eq!(
            styled("In order to utilize the room, ask the chairman."),
            "To use the room, ask the chair."
        );
        assert_eq!(
            style_issues("We met prior to lunch", "en-GB")[0].kind,
            IssueKind::Wordy
        );
        assert_eq!(
            style_issues("A man-made lake", "en")[0].kind,
            IssueKind::Gendered
        );
        assert_eq!(
            style_issues("Facilitate it", "en")[0].kind,
            IssueKind::Jargon
        );
        // The longest phrase wins, and only whole words match.
        assert_eq!(styled("in spite of the fact that"), "although");
        assert_eq!(
            styled("chairmanship and reutilize"),
            "chairmanship and reutilize"
        );
        // No rules in other languages.
        assert!(style_issues("in order to", "pt-BR").is_empty());
    }

    #[test]
    fn right_text_has_no_issues() {
        for text in [
            "The end. Next one, then example.com and 3.5 or 1,000.",
            "\tIndented line ",
            "Uma frase. Outra frase!",
            "",
        ] {
            assert!(proofread(text).is_empty(), "{text}: {:?}", proofread(text));
        }
    }
}
