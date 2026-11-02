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

mod dictionaries;
mod hyphen;
mod proofread;
mod spell;

pub use dictionaries::{Kind, find, search_folders};
pub use hyphen::Patterns;
pub use proofread::{Issue, IssueKind, proofread};
pub use spell::{Speller, misspelled_words, words};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// The hyphenation patterns of a language, loaded once, or `None` when
/// the system has none.
pub fn patterns(language: &str) -> Option<Arc<Patterns>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<Patterns>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(language.to_owned())
        .or_insert_with(|| {
            let path = find(Kind::Hyphenation, language)?;
            let data = std::fs::read(path).ok()?;
            Patterns::parse(&data).map(Arc::new)
        })
        .clone()
}

/// The spelling dictionary of a language, loaded once, or `None` when the
/// system has none.
pub fn speller(language: &str) -> Option<Arc<Speller>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<Speller>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(language.to_owned())
        .or_insert_with(|| {
            let dic = find(Kind::Spelling, language)?;
            let aff = dic.with_extension("aff");
            let dic = std::fs::read_to_string(dic).ok()?;
            let aff = std::fs::read_to_string(aff).ok()?;
            Speller::new(&aff, &dic).map(Arc::new)
        })
        .clone()
}
