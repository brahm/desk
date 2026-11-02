// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the dictionaries of the system are.

use std::path::{Path, PathBuf};

/// The kinds of dictionary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A Hunspell dictionary, as `pt_BR.dic` with its `pt_BR.aff`.
    Spelling,
    /// Hyphenation patterns, as `hyph_pt_BR.dic`.
    Hyphenation,
}

impl Kind {
    /// The folders of a data folder that hold the dictionaries.
    fn subfolders(self) -> &'static [&'static str] {
        match self {
            Self::Spelling => &["hunspell", "myspell", "myspell/dicts"],
            Self::Hyphenation => &["hyphen", "myspell", "myspell/dicts"],
        }
    }

    /// The file of a dictionary for a locale such as `pt_BR`.
    fn file_name(self, locale: &str) -> String {
        match self {
            Self::Spelling => format!("{locale}.dic"),
            Self::Hyphenation => format!("hyph_{locale}.dic"),
        }
    }

    /// The locale of a dictionary file, as `pt_BR` for `hyph_pt_BR.dic`.
    fn locale_of(self, file: &str) -> Option<&str> {
        let stem = file.strip_suffix(".dic")?;
        match self {
            Self::Spelling => (!stem.starts_with("hyph_")).then_some(stem),
            Self::Hyphenation => stem.strip_prefix("hyph_"),
        }
    }
}

/// The folders searched for a kind of dictionary, in order:
/// `DESK_DICTIONARY_PATH`, then the data folders of the system.
pub fn search_folders(kind: Kind) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::env::var_os("DESK_DICTIONARY_PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    let data = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    let mut roots: Vec<PathBuf> = std::env::split_paths(&data).collect();
    for root in ["/app/share", "/usr/share"] {
        let root = PathBuf::from(root);
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    for root in roots {
        for sub in kind.subfolders() {
            out.push(root.join(sub));
        }
    }
    out
}

/// The language and region of a BCP 47 tag, as ("pt", Some("BR")) for
/// `pt-BR` or `pt-Latn-BR`.
fn parts(tag: &str) -> Option<(String, Option<String>)> {
    let mut subtags = tag.split(['-', '_']);
    let language = subtags.next()?.to_ascii_lowercase();
    if language.len() < 2 || language == "und" || !language.chars().all(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    let region = subtags
        .find(|s| s.len() == 2 && s.chars().all(|c| c.is_ascii_alphabetic()))
        .map(str::to_ascii_uppercase);
    Some((language, region))
}

/// The best dictionary of a kind for a language among the files of some
/// folders: the same language and region, then the same language.
fn best_in(kind: Kind, tag: &str, folders: &[PathBuf]) -> Option<PathBuf> {
    let (language, region) = parts(tag)?;
    let exact = region.as_ref().map(|r| format!("{language}_{r}"));
    let mut fallback: Option<PathBuf> = None;
    for folder in folders {
        if let Some(exact) = &exact {
            let path = folder.join(kind.file_name(exact));
            if path.is_file() {
                return Some(path);
            }
        }
        let path = folder.join(kind.file_name(&language));
        if fallback.is_none() && path.is_file() {
            fallback = Some(path);
        }
        if fallback.is_none() {
            fallback = same_language(kind, &language, folder);
        }
    }
    fallback
}

/// A dictionary of a folder in a language, for any region, the first by
/// name.
fn same_language(kind: Kind, language: &str, folder: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| kind.locale_of(n))
                .and_then(|locale| locale.split('_').next())
                .is_some_and(|l| l.eq_ignore_ascii_case(language))
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

/// The dictionary of a kind for a language, as a BCP 47 tag, if the
/// system has one.
pub fn find(kind: Kind, tag: &str) -> Option<PathBuf> {
    best_in(kind, tag, &search_folders(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_name_language_and_region() {
        assert_eq!(parts("pt-BR"), Some(("pt".into(), Some("BR".into()))));
        assert_eq!(parts("en-Latn-US"), Some(("en".into(), Some("US".into()))));
        assert_eq!(parts("de"), Some(("de".into(), None)));
        assert_eq!(parts("und"), None);
        assert_eq!(parts(""), None);
    }

    #[test]
    fn the_region_wins_then_the_language() {
        let folder = std::env::temp_dir().join(format!("desk-lang-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["pt_PT.dic", "pt_BR.dic", "hyph_pt_BR.dic", "en_GB.dic"] {
            std::fs::write(folder.join(name), "").unwrap();
        }
        let folders = [folder.clone()];
        let name = |p: Option<PathBuf>| p.and_then(|p| Some(p.file_name()?.to_str()?.to_owned()));
        assert_eq!(
            name(best_in(Kind::Spelling, "pt-BR", &folders)).as_deref(),
            Some("pt_BR.dic")
        );
        assert_eq!(
            name(best_in(Kind::Spelling, "pt-AO", &folders)).as_deref(),
            Some("pt_BR.dic")
        );
        assert_eq!(
            name(best_in(Kind::Spelling, "en-US", &folders)).as_deref(),
            Some("en_GB.dic")
        );
        assert_eq!(
            name(best_in(Kind::Hyphenation, "pt", &folders)).as_deref(),
            Some("hyph_pt_BR.dic")
        );
        assert_eq!(best_in(Kind::Hyphenation, "en", &folders), None);
        assert_eq!(best_in(Kind::Spelling, "und", &folders), None);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
