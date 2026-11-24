// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spelling (TXT-10), proofreading (TXT-11) and hyphenation (TXT-09), with
//! the dictionaries of the system.
//!
//! Desk ships no dictionaries. It reads those that the system has, as
//! LibreOffice and GNOME apps do: Hunspell dictionaries for spelling, in
//! `hunspell` or `myspell` folders, and the hyphenation patterns of
//! LibreOffice, in `hyphen` folders, under each data folder of the system
//! (`/usr/share`, and `/app/share` in a Flatpak). `DESK_DICTIONARY_PATH`
//! names more folders to look in first.
//!
//! The language of text chooses the dictionary: `pt-BR` reads `pt_BR.dic`,
//! and falls back to another Portuguese one. Text in the undetermined
//! language is neither checked nor hyphenated.
//!
//! Dictionaries load once, when first needed, and stay for every document.

mod charset;
mod dictionaries;
mod hyphen;
mod proofread;
mod spell;

pub use dictionaries::{Kind, find, installed, search_folders};
pub use hyphen::Patterns;
pub use proofread::{Issue, IssueKind, proofread, style_issues};
pub use spell::{Speller, misspelled_words, words};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

/// The dictionaries in the folders, with the index of each one's folder.
type Listing = Vec<(usize, PathBuf)>;

/// A dictionary of each language, loaded once.
///
/// The languages of a document come from its file: a file may name
/// thousands, each of which used to load the same dictionary again, on
/// the thread of the window (review RV, batch 5). A dictionary is now
/// found by what of the tag picks it, and loaded once by its file.
///
/// The folders are listed once too, so that tags that no dictionary fits
/// cost no reading of the disk (review RV, batch 6, B-063). They are
/// listed again when a tag finds nothing and one of them changed since,
/// as when a dictionary is installed while Desk runs.
struct Loaded<T> {
    /// The folders looked in; those of the system when `None`.
    folders: Option<Vec<PathBuf>>,
    /// When each folder last changed, as it was listed, and what it held.
    listed: Option<(Vec<Option<SystemTime>>, Listing)>,
    by_key: HashMap<String, Option<PathBuf>>,
    by_file: HashMap<PathBuf, Option<Arc<T>>>,
}

impl<T> Default for Loaded<T> {
    fn default() -> Self {
        Self {
            folders: None,
            listed: None,
            by_key: HashMap::new(),
            by_file: HashMap::new(),
        }
    }
}

impl<T> Loaded<T> {
    fn get(
        &mut self,
        kind: Kind,
        language: &str,
        load: impl FnOnce(&Path) -> Option<T>,
    ) -> Option<Arc<T>> {
        let key = dictionaries::key(language)?;
        let file = match self.by_key.get(&key) {
            Some(Some(file)) => file.clone(),
            _ => {
                let files = self.listing(kind);
                let found = dictionaries::best_among(kind, &key, files);
                self.by_key.insert(key, found.clone());
                found?
            }
        };
        self.by_file
            .entry(file)
            .or_insert_with_key(|file| load(file).map(Arc::new))
            .clone()
    }

    /// What the folders hold, listed again only when one of them changed.
    fn listing(&mut self, kind: Kind) -> &[(usize, PathBuf)] {
        let folders = self
            .folders
            .clone()
            .unwrap_or_else(|| dictionaries::search_folders(kind));
        let stamps: Vec<_> = folders
            .iter()
            .map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok())
            .collect();
        if self.listed.as_ref().is_none_or(|(was, _)| *was != stamps) {
            self.listed = Some((stamps, dictionaries::listing(kind, &folders)));
        }
        self.listed.as_ref().map_or(&[], |(_, files)| files)
    }
}

/// The hyphenation patterns of a language, loaded once, or `None` when
/// the system has none.
pub fn patterns(language: &str) -> Option<Arc<Patterns>> {
    static CACHE: OnceLock<Mutex<Loaded<Patterns>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache.get(Kind::Hyphenation, language, |path| {
        Patterns::parse(&std::fs::read(path).ok()?)
    })
}

/// The spelling dictionary of a language, loaded once, or `None` when the
/// system has none.
pub fn speller(language: &str) -> Option<Arc<Speller>> {
    static CACHE: OnceLock<Mutex<Loaded<Speller>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache.get(Kind::Spelling, language, |dic| {
        let aff = std::fs::read(dic.with_extension("aff")).ok()?;
        let dic = std::fs::read(dic).ok()?;
        Speller::from_bytes(&aff, &dic)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dictionary_is_loaded_once_for_the_tags_that_pick_it() {
        // Review RV, batch 5: each tag a file named loaded the dictionary
        // again, as en-q01, en-q02, and so on.
        assert_eq!(dictionaries::key("en-q01").as_deref(), Some("en"));
        assert_eq!(dictionaries::key("pt_br").as_deref(), Some("pt-BR"));
        assert_eq!(dictionaries::key("PT-BR").as_deref(), Some("pt-BR"));
        assert_eq!(dictionaries::key("und"), None);
        let mut loaded: Loaded<u32> = Loaded::default();
        let file = PathBuf::from("/nowhere/en_US.dic");
        loaded.by_key.insert("en".into(), Some(file.clone()));
        loaded.by_key.insert("en-GB".into(), Some(file));
        let mut loads = 0;
        for tag in ["en", "en-q01", "en-q02", "EN", "en-GB", "en_gb"] {
            let got = loaded.get(Kind::Spelling, tag, |_| {
                loads += 1;
                Some(7)
            });
            assert_eq!(got.as_deref(), Some(&7));
        }
        assert_eq!(loads, 1);
    }

    #[test]
    fn the_folders_of_dictionaries_are_listed_once() {
        // Review RV, batch 6 (B-063): each tag that no dictionary fits
        // listed the folders again.
        let folder = std::env::temp_dir().join(format!("desk-lang-once-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("en_US.dic"), "1\nword\n").unwrap();
        let mut loaded: Loaded<u32> = Loaded {
            folders: Some(vec![folder.clone()]),
            ..Loaded::default()
        };
        assert!(loaded.get(Kind::Spelling, "en-GB", |_| Some(1)).is_some());
        let listed = |loaded: &Loaded<u32>| loaded.listed.as_ref().map(|(_, f)| f.as_ptr());
        let first = listed(&loaded);
        for tag in ["fr", "de-AT", "pt-BR", "ja", "xh-ZA", "zu"] {
            assert!(loaded.get(Kind::Spelling, tag, |_| Some(1)).is_none());
        }
        // The folder did not change: the same listing.
        assert_eq!(listed(&loaded), first);
        // Second review: a dictionary installed while Desk runs is found.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(folder.join("fr_FR.dic"), "1\nmot\n").unwrap();
        assert!(loaded.get(Kind::Spelling, "fr", |_| Some(2)).is_some());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
