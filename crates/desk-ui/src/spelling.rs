// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spelling and proofreading of the text of a document (TXT-10, TXT-11).
//!
//! The words of each run are checked with the dictionary of its language.
//! Words that the user taught Desk (Learn Spelling) count as right in
//! every document; they live in `desk/words.txt` in the configuration
//! folder, one on each line. Words ignored (Ignore Spelling) count as
//! right until the app quits.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::PathBuf;

use desk_lang::{Issue, IssueKind};
use desk_model::{Paragraph, Styles};
use gtk::glib;

use crate::i18n::tr;

/// What is wrong with a piece of text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MistakeKind {
    /// A word that the dictionary of its language does not have.
    Spelling {
        /// The language, as a BCP 47 tag.
        language: String,
    },
    /// A mistake that the proofreader found.
    Proofreading(Issue),
}

/// A mistake in a paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mistake {
    /// The byte range in the paragraph.
    pub range: Range<usize>,
    /// What is wrong.
    pub kind: MistakeKind,
}

impl Mistake {
    /// The text that fixes it, the likeliest first.
    pub fn suggestions(&self, text: &str) -> Vec<String> {
        match &self.kind {
            MistakeKind::Spelling { language } => desk_lang::speller(language)
                .map(|s| s.suggest(&text[self.range.clone()]))
                .unwrap_or_default(),
            MistakeKind::Proofreading(issue) => vec![issue.replacement.clone()],
        }
    }

    /// What is wrong, as the Spelling and Grammar window says it.
    pub fn description(&self) -> String {
        match &self.kind {
            MistakeKind::Spelling { .. } => tr("Not in the dictionary"),
            MistakeKind::Proofreading(issue) => issue_description(issue.kind),
        }
    }
}

/// What a kind of mistake of the proofreader is.
pub fn issue_description(kind: IssueKind) -> String {
    match kind {
        IssueKind::RepeatedWord => tr("Repeated word"),
        IssueKind::ExtraSpaces => tr("More than one space"),
        IssueKind::SpaceBeforePunctuation => tr("Space before punctuation"),
        IssueKind::MissingSpace => tr("Missing space after punctuation"),
        IssueKind::Capitalization => tr("A sentence starts with a small letter"),
    }
}

#[derive(Default)]
struct Words {
    learned: Option<BTreeSet<String>>,
    ignored: HashSet<String>,
    /// The mistakes of paragraphs, by a hash of their text, languages and
    /// what is checked, since the last change of the words.
    cache: HashMap<u64, Vec<Mistake>>,
}

thread_local! {
    static WORDS: RefCell<Words> = RefCell::default();
}

fn path() -> PathBuf {
    glib::user_config_dir().join("desk").join("words.txt")
}

fn with_words<T>(f: impl FnOnce(&mut Words) -> T) -> T {
    WORDS.with(|words| {
        let mut words = words.borrow_mut();
        if words.learned.is_none() {
            let learned = std::fs::read_to_string(path())
                .map(|text| {
                    text.lines()
                        .map(str::trim)
                        .filter(|w| !w.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            words.learned = Some(learned);
        }
        f(&mut words)
    })
}

fn save(learned: &BTreeSet<String>) {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut text = learned.iter().cloned().collect::<Vec<_>>().join("\n");
    text.push('\n');
    if let Err(error) = std::fs::write(&path, text) {
        glib::g_warning!("desk", "Cannot save the learned words: {error}");
    }
}

/// Whether the user taught Desk a word.
pub fn is_learned(word: &str) -> bool {
    with_words(|w| w.learned.as_ref().is_some_and(|l| l.contains(word)))
}

/// Teaches Desk a word, for every document (Learn Spelling).
pub fn learn(word: &str) {
    with_words(|w| {
        let learned = w.learned.get_or_insert_with(BTreeSet::new);
        if learned.insert(word.to_owned()) {
            save(learned);
        }
        w.cache.clear();
    });
}

/// Makes Desk forget a word it learned (Forget Spelling).
pub fn forget(word: &str) {
    with_words(|w| {
        let learned = w.learned.get_or_insert_with(BTreeSet::new);
        if learned.remove(word) {
            save(learned);
        }
        w.cache.clear();
    });
}

/// Takes a word as right until the app quits (Ignore Spelling).
pub fn ignore(word: &str) {
    with_words(|w| {
        w.ignored.insert(word.to_owned());
        w.cache.clear();
    });
}

/// The language that text in no language is checked in: the language of
/// the interface, as "pt-BR" for pt_BR.UTF-8, or "en-US".
pub fn default_language() -> String {
    thread_local! {
        static LANGUAGE: String = glib::language_names()
            .iter()
            .map(|l| l.split(['.', '@']).next().unwrap_or_default().replace('_', "-"))
            .find(|l| {
                let lang = l.split('-').next().unwrap_or_default();
                lang.len() >= 2 && lang.chars().all(|c| c.is_ascii_lowercase())
            })
            .unwrap_or_else(|| "en-US".to_owned());
    }
    LANGUAGE.with(Clone::clone)
}

/// The language of each run of a paragraph, by its byte range; text in no
/// language takes the language of the interface.
fn languages(styles: &Styles, paragraph: &Paragraph) -> Vec<(Range<usize>, String)> {
    let mut start = 0;
    paragraph
        .runs()
        .map(|run| {
            let format = styles.resolve_character(paragraph.style(), run.style, &run.attrs);
            let range = start..start + run.text().len();
            start = range.end;
            let tag = format.language.tag();
            let language = if tag == "und" {
                default_language()
            } else {
                tag.to_owned()
            };
            (range, language)
        })
        .collect()
}

/// The mistakes of a paragraph, in order: misspelled words when
/// `spelling` is on and the language has a dictionary, and the mistakes
/// of the proofreader when `proofreading` is on.
pub fn mistakes(
    styles: &Styles,
    paragraph: &Paragraph,
    spelling: bool,
    proofreading: bool,
) -> Vec<Mistake> {
    if !spelling && !proofreading {
        return Vec::new();
    }
    let text = paragraph.text();
    let languages = languages(styles, paragraph);
    let key = {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (&text, &languages, spelling, proofreading).hash(&mut hasher);
        hasher.finish()
    };
    if let Some(found) = with_words(|w| w.cache.get(&key).cloned()) {
        return found;
    }
    let mut out = Vec::new();
    if spelling {
        for range in desk_lang::words(&text) {
            let Some((_, language)) = languages.iter().find(|(r, _)| r.contains(&range.start))
            else {
                continue;
            };
            let word = &text[range.clone()];
            let known = with_words(|w| {
                w.ignored.contains(word) || w.learned.as_ref().is_some_and(|l| l.contains(word))
            });
            if known {
                continue;
            }
            if let Some(speller) = desk_lang::speller(language)
                && !speller.check(word)
            {
                out.push(Mistake {
                    range,
                    kind: MistakeKind::Spelling {
                        language: language.clone(),
                    },
                });
            }
        }
    }
    if proofreading {
        for issue in desk_lang::proofread(&text) {
            if out
                .iter()
                .any(|m| m.range.start < issue.range.end && issue.range.start < m.range.end)
            {
                continue;
            }
            out.push(Mistake {
                range: issue.range.clone(),
                kind: MistakeKind::Proofreading(issue),
            });
        }
        out.sort_by_key(|m| m.range.start);
    }
    with_words(|w| {
        // A bound on the memory of long sessions.
        if w.cache.len() > 10_000 {
            w.cache.clear();
        }
        w.cache.insert(key, out.clone());
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{CharAttrs, Document, Editor, Language};

    #[test]
    fn txt_11_the_proofreader_marks_mistakes_with_their_fixes() {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Typing", |edit| edit.insert_text(start, "It is is done ."))
            .unwrap();
        let document = editor.document();
        let paragraph = document.body().get(0).unwrap();
        let found = mistakes(document.styles(), paragraph, false, true);
        let kinds: Vec<_> = found
            .iter()
            .map(|m| match &m.kind {
                MistakeKind::Proofreading(issue) => issue.kind,
                MistakeKind::Spelling { .. } => panic!("no spelling"),
            })
            .collect();
        assert_eq!(
            kinds,
            [IssueKind::RepeatedWord, IssueKind::SpaceBeforePunctuation]
        );
        assert_eq!(found[0].suggestions(&paragraph.text()), ["is"]);
        assert!(mistakes(document.styles(), paragraph, false, false).is_empty());
    }

    #[test]
    fn txt_10_words_are_checked_in_the_language_of_their_run() {
        if desk_lang::speller("pt-BR").is_none() {
            return;
        }
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        let end = editor
            .edit("Typing", |edit| {
                edit.insert_text(start, "Uma casa vermelah")
            })
            .unwrap();
        editor
            .edit("Language", |edit| {
                edit.set_character_attrs(
                    start,
                    end,
                    &CharAttrs {
                        language: Some(Language::new("pt-BR")),
                        ..CharAttrs::default()
                    },
                )
            })
            .unwrap();
        let document = editor.document();
        let paragraph = document.body().get(0).unwrap();
        let found = mistakes(document.styles(), paragraph, true, false);
        assert_eq!(found.len(), 1);
        assert_eq!(&paragraph.text()[found[0].range.clone()], "vermelah");
        assert!(
            found[0]
                .suggestions(&paragraph.text())
                .contains(&"vermelha".to_owned())
        );
        ignore("vermelah");
        assert!(mistakes(document.styles(), paragraph, true, false).is_empty());
    }

    #[test]
    fn txt_10_undetermined_text_takes_the_language_of_the_interface() {
        let language = default_language();
        assert!(!language.contains(['_', '.']), "{language}");
        if desk_lang::speller(&language).is_none() {
            return;
        }
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Typing", |edit| edit.insert_text(start, "zzqxw"))
            .unwrap();
        let document = editor.document();
        let paragraph = document.body().get(0).unwrap();
        let found = mistakes(document.styles(), paragraph, true, false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, MistakeKind::Spelling { language });
    }
}
