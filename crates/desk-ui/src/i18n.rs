// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Translations of the interface. The catalogs in po are built into the
//! apps; the language comes from the environment, as gettext picks it
//! (LANGUAGE, LC_ALL, LC_MESSAGES and LANG).
//!
//! Strings to translate are marked with [`tr`], [`trn`] and [`trc`], which
//! xgettext finds (see ci/update-translations.sh). A value in a string is
//! a name in braces, such as `{name}`, which the translation may move.

use std::sync::OnceLock;

/// How a language picks the plural form of a number.
#[derive(Clone, Copy, Debug)]
#[allow(
    dead_code,
    reason = "only the rules of the languages built in are used"
)]
pub(crate) enum Plural {
    /// One form for 1, another for the rest, as in English.
    NotOne,
    /// One form for 0 and 1, another for the rest, as in Brazilian
    /// Portuguese and French.
    MoreThanOne,
    /// One form for every number.
    None,
}

impl Plural {
    fn form(self, n: u64) -> usize {
        match self {
            Self::NotOne => usize::from(n != 1),
            Self::MoreThanOne => usize::from(n > 1),
            Self::None => 0,
        }
    }
}

/// The translations into a language, sorted by message.
pub(crate) struct Catalog {
    language: &'static str,
    plural: Plural,
    messages: &'static [(&'static str, &'static [&'static str])],
}

static CATALOGS: &[Catalog] = include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

/// The catalog of the first language asked for that has one.
fn choose(languages: &[&str]) -> Option<&'static Catalog> {
    languages
        .iter()
        .find_map(|name| CATALOGS.iter().find(|c| c.language == *name))
}

fn current() -> Option<&'static Catalog> {
    static CURRENT: OnceLock<Option<&'static Catalog>> = OnceLock::new();
    *CURRENT.get_or_init(|| {
        let names = gtk::glib::language_names();
        let names: Vec<&str> = names.iter().map(|n| n.as_str()).collect();
        choose(&names)
    })
}

fn lookup(catalog: Option<&'static Catalog>, key: &str) -> Option<&'static [&'static str]> {
    let messages = catalog?.messages;
    messages
        .binary_search_by(|(k, _)| (*k).cmp(key))
        .ok()
        .map(|i| messages[i].1)
}

/// Marks a string to translate where it cannot be translated yet, such as
/// in a constant; [`tr`] translates it where it is shown.
pub const fn n_(message: &'static str) -> &'static str {
    message
}

/// A string in the language of the interface.
pub fn tr(message: &str) -> String {
    lookup(current(), message)
        .and_then(|t| t.first())
        .map_or_else(|| message.to_owned(), |t| (*t).to_owned())
}

/// A string that names a number, in the form the number takes.
pub fn trn(single: &str, plural: &str, n: u64) -> String {
    let catalog = current();
    lookup(catalog, single)
        .and_then(|t| t.get(catalog.map_or(0, |c| c.plural.form(n))))
        .map_or_else(
            || if n == 1 { single } else { plural }.to_owned(),
            |t| (*t).to_owned(),
        )
}

/// A string that means something else elsewhere, told apart by a context.
pub fn trc(context: &str, message: &str) -> String {
    lookup(current(), &format!("{context}\u{4}{message}"))
        .and_then(|t| t.first())
        .map_or_else(|| message.to_owned(), |t| (*t).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brazilian_portuguese_is_built_in() {
        let catalog = choose(&["pt_BR.UTF-8", "pt_BR", "pt", "C"]).expect("pt_BR");
        assert_eq!(catalog.language, "pt_BR");
        assert_eq!(lookup(Some(catalog), "_Save").unwrap()[0], "_Salvar");
        let words = lookup(Some(catalog), "{n} word").unwrap();
        assert_eq!(words[catalog.plural.form(1)], "{n} palavra");
        assert_eq!(words[catalog.plural.form(0)], "{n} palavra");
        assert_eq!(words[catalog.plural.form(2)], "{n} palavras");
        assert!(choose(&["de_DE", "C"]).is_none());
    }

    #[test]
    fn the_catalogs_are_sorted_for_the_search() {
        for catalog in CATALOGS {
            assert!(catalog.messages.windows(2).all(|w| w[0].0 < w[1].0));
        }
    }
}
