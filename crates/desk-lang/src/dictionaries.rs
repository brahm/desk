// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the dictionaries of the system are.

use std::path::PathBuf;

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

/// The subtags of a BCP 47 tag that pick a dictionary.
#[derive(Debug, PartialEq, Eq)]
struct Parts {
    /// The language, in lowercase, as `pt`.
    language: String,
    /// The script, in title case, as `Latn`.
    script: Option<String>,
    /// The region, in uppercase, as `BR`.
    region: Option<String>,
    /// The variant, in lowercase, as `valencia`.
    variant: Option<String>,
}

/// Whether a subtag is all ASCII letters, of a length.
fn letters(subtag: &str, len: usize) -> bool {
    subtag.len() == len && subtag.chars().all(|c| c.is_ascii_alphabetic())
}

/// The language, script and region of a BCP 47 tag, or of the locale of
/// a dictionary file, as `pt_BR`, `sr_Latn_RS` or `fa-IR`.
fn parts(tag: &str) -> Option<Parts> {
    let mut subtags = tag.split(['-', '_']);
    let language = subtags.next()?.to_ascii_lowercase();
    if language.len() < 2 || language == "und" || !language.chars().all(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    let (mut script, mut region, mut variant) = (None, None, None);
    for subtag in subtags {
        if script.is_none() && region.is_none() && letters(subtag, 4) {
            let (first, rest) = subtag.split_at(1);
            script = Some(first.to_ascii_uppercase() + &rest.to_ascii_lowercase());
        } else if region.is_none() && letters(subtag, 2) {
            region = Some(subtag.to_ascii_uppercase());
        } else if variant.is_none() && subtag.len() > 4 {
            variant = Some(subtag.to_ascii_lowercase());
        }
    }
    Some(Parts {
        language,
        script,
        region,
        variant,
    })
}

/// The region whose dictionary a language without one uses, when it is
/// not the language itself, as `de` for `de_DE`: Swedish is `sv_SE` and
/// not `sv_FI`.
fn usual_region(language: &str) -> String {
    match language {
        "be" => "BY",
        "ca" | "eu" | "gl" => "ES",
        "cs" => "CZ",
        "da" => "DK",
        "el" => "GR",
        "en" => "US",
        "fa" => "IR",
        "he" => "IL",
        "hi" | "pa" => "IN",
        "kk" => "KZ",
        "ko" => "KR",
        "nb" | "nn" => "NO",
        "oc" => "FR",
        "sl" => "SI",
        "sr" => "RS",
        "sv" => "SE",
        "uk" => "UA",
        _ => return language.to_ascii_uppercase(),
    }
    .to_owned()
}

/// How well the locale of a dictionary file fits a tag, the lowest best,
/// or `None` when it is of another language. The script comes first, as
/// Serbian in Latin script is `sr_Latn_RS` and in Cyrillic `sr_RS`; then
/// the variant of the tag, as `ca_ES-valencia` for `ca-ES-valencia` and
/// not for `ca-ES`; then the region of the tag, a dictionary for no
/// region, the usual region of the language and any other.
fn fit(wanted: &Parts, locale: &str) -> Option<(u8, bool, u8)> {
    let have = parts(locale)?;
    if have.language != wanted.language {
        return None;
    }
    let script = match (&wanted.script, &have.script) {
        (Some(w), Some(h)) if w == h => 0,
        (_, None) => u8::from(wanted.script.is_some()),
        _ => 2,
    };
    let region = match (&wanted.region, &have.region) {
        (Some(w), Some(h)) if w == h => 0,
        (_, None) => 1,
        (_, Some(h)) if *h == usual_region(&wanted.language) => 2,
        _ => 3,
    };
    Some((script, have.variant != wanted.variant, region))
}

/// The best dictionary of a kind for a language among the files of some
/// folders, by `fit`, then the first folder, then the first by name.
fn best_in(kind: Kind, tag: &str, folders: &[PathBuf]) -> Option<PathBuf> {
    let wanted = parts(tag)?;
    let mut best: Option<((u8, bool, u8), usize, PathBuf)> = None;
    for (index, folder) in folders.iter().enumerate() {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            let Some(score) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| kind.locale_of(n))
                .and_then(|locale| fit(&wanted, locale))
            else {
                continue;
            };
            let candidate = (score, index, path);
            if best.as_ref().is_none_or(|b| candidate < *b) && candidate.2.is_file() {
                best = Some(candidate);
            }
        }
    }
    best.map(|(_, _, path)| path)
}

/// The dictionary of a kind for a language, as a BCP 47 tag, if the
/// system has one.
pub fn find(kind: Kind, tag: &str) -> Option<PathBuf> {
    best_in(kind, tag, &search_folders(kind))
}

/// The languages that the system has dictionaries of a kind for, as BCP
/// 47 tags such as `pt-BR`, sorted and each once (TXT-10).
pub fn installed(kind: Kind) -> Vec<String> {
    let mut tags: Vec<String> = search_folders(kind)
        .iter()
        .filter_map(|folder| std::fs::read_dir(folder).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let locale = kind.locale_of(&name)?;
            parts(locale)?;
            Some(locale.replace('_', "-"))
        })
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(language: &str, script: Option<&str>, region: Option<&str>) -> Option<Parts> {
        Some(Parts {
            language: language.into(),
            script: script.map(Into::into),
            region: region.map(Into::into),
            variant: None,
        })
    }

    #[test]
    fn tags_name_language_script_and_region() {
        assert_eq!(parts("pt-BR"), p("pt", None, Some("BR")));
        assert_eq!(parts("en-Latn-US"), p("en", Some("Latn"), Some("US")));
        assert_eq!(parts("sr_Latn_RS"), p("sr", Some("Latn"), Some("RS")));
        assert_eq!(
            parts("ca_ES-valencia").and_then(|p| p.variant).as_deref(),
            Some("valencia")
        );
        assert_eq!(parts("de"), p("de", None, None));
        assert_eq!(parts("und"), None);
        assert_eq!(parts(""), None);
    }

    #[test]
    fn the_script_and_region_win_then_the_usual_region() {
        let folder = std::env::temp_dir().join(format!("desk-lang-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for name in [
            "pt_PT.dic",
            "pt_BR.dic",
            "hyph_pt_BR.dic",
            "en_GB.dic",
            "fa-IR.dic",
            "sv_FI.dic",
            "sv_SE.dic",
            "sr_Latn_RS.dic",
            "sr_ME.dic",
            "sr_RS.dic",
            "hyph_sr-Latn_RS.dic",
            "hyph_sr_RS.dic",
            "ca.dic",
            "ca_ES-valencia.dic",
        ] {
            std::fs::write(folder.join(name), "").unwrap();
        }
        let folders = [folder.clone()];
        let find = |kind, tag| {
            best_in(kind, tag, &folders).and_then(|p| Some(p.file_name()?.to_str()?.to_owned()))
        };
        let spelling = |tag| find(Kind::Spelling, tag);
        assert_eq!(spelling("pt-BR").as_deref(), Some("pt_BR.dic"));
        // Angola writes as Portugal does.
        assert_eq!(spelling("pt-AO").as_deref(), Some("pt_PT.dic"));
        assert_eq!(spelling("en-US").as_deref(), Some("en_GB.dic"));
        // The Persian dictionary of LibreOffice has a hyphen in its name.
        assert_eq!(spelling("fa-IR").as_deref(), Some("fa-IR.dic"));
        assert_eq!(spelling("fa").as_deref(), Some("fa-IR.dic"));
        assert_eq!(spelling("sv").as_deref(), Some("sv_SE.dic"));
        assert_eq!(spelling("sv-FI").as_deref(), Some("sv_FI.dic"));
        assert_eq!(spelling("sr").as_deref(), Some("sr_RS.dic"));
        assert_eq!(spelling("sr-ME").as_deref(), Some("sr_ME.dic"));
        assert_eq!(spelling("sr-Latn").as_deref(), Some("sr_Latn_RS.dic"));
        assert_eq!(spelling("sr-Latn-RS").as_deref(), Some("sr_Latn_RS.dic"));
        assert_eq!(spelling("ca").as_deref(), Some("ca.dic"));
        assert_eq!(spelling("ca-ES").as_deref(), Some("ca.dic"));
        assert_eq!(
            spelling("ca-ES-valencia").as_deref(),
            Some("ca_ES-valencia.dic")
        );
        assert_eq!(
            find(Kind::Hyphenation, "sr-Latn").as_deref(),
            Some("hyph_sr-Latn_RS.dic")
        );
        assert_eq!(
            find(Kind::Hyphenation, "sr").as_deref(),
            Some("hyph_sr_RS.dic")
        );
        assert_eq!(
            find(Kind::Hyphenation, "pt").as_deref(),
            Some("hyph_pt_BR.dic")
        );
        assert_eq!(find(Kind::Hyphenation, "en"), None);
        assert_eq!(spelling("und"), None);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
