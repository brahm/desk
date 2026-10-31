// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fonts that stand for missing ones (DOC-05).
//!
//! A document can name a font that the system does not have, as the
//! documents of iWork do with the fonts of the Mac. Desk then lays the text
//! out with a free font from `data/fonts/substitutes.tsv`, preferring one
//! with the same metrics, and keeps a record of each substitution for the
//! Warnings window.

use std::sync::{Arc, OnceLock};

/// How a font was chosen for a family that a document names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontMatch {
    /// The font itself is installed.
    Installed,
    /// A substitute with the same advance widths, so that lines break in
    /// the same places.
    Metric,
    /// A substitute of a similar style.
    Style,
    /// The default font of its kind, as nothing better is installed.
    Fallback,
}

/// The font used for a family that a document names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontChoice {
    /// The family the document names.
    pub requested: Arc<str>,
    /// The family used.
    pub used: Arc<str>,
    /// How it was chosen.
    pub kind: FontMatch,
}

impl FontChoice {
    /// Whether another font stands for the one the document names.
    pub fn is_substitution(&self) -> bool {
        self.kind != FontMatch::Installed
    }
}

/// A row of the table.
struct Substitute {
    requested: &'static str,
    family: &'static str,
    metric: bool,
}

fn table() -> &'static [Substitute] {
    static TABLE: OnceLock<Vec<Substitute>> = OnceLock::new();
    TABLE.get_or_init(|| {
        include_str!("../../../data/fonts/substitutes.tsv")
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .skip(1)
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let (requested, family, kind) = (fields.next()?, fields.next()?, fields.next()?);
                Some(Substitute {
                    requested,
                    family,
                    metric: kind == "metric",
                })
            })
            .collect()
    })
}

/// The substitutes for a family, in order of preference, with whether each
/// has the same metrics. Names compare without regard to case.
pub fn substitutes(family: &str) -> impl Iterator<Item = (&'static str, bool)> {
    let family = family.trim().to_owned();
    table()
        .iter()
        .filter(move |s| s.requested.eq_ignore_ascii_case(&family))
        .map(|s| (s.family, s.metric))
}

/// The default font for a family that nothing stands for, after the kind
/// of font its name suggests.
pub fn fallback(family: &str) -> &'static str {
    let name = family.to_ascii_lowercase();
    if ["mono", "courier", "code", "console", "typewriter"]
        .iter()
        .any(|w| name.contains(w))
    {
        "Liberation Mono"
    } else if name.contains("sans") {
        "Liberation Sans"
    } else if [
        "serif", "roman", "times", "garamond", "book", "antiqua", "caslon", "bodoni",
    ]
    .iter()
    .any(|w| name.contains(w))
    {
        "Liberation Serif"
    } else {
        "Liberation Sans"
    }
}

/// Chooses the font for a family, given whether a family is installed.
pub(crate) fn choose(family: &str, mut installed: impl FnMut(&str) -> bool) -> FontChoice {
    let requested: Arc<str> = family.into();
    let choice = |used: &str, kind| FontChoice {
        requested: requested.clone(),
        used: used.into(),
        kind,
    };
    if installed(family) {
        return choice(family, FontMatch::Installed);
    }
    for (substitute, metric) in substitutes(family) {
        if installed(substitute) {
            let kind = if metric {
                FontMatch::Metric
            } else {
                FontMatch::Style
            };
            return choice(substitute, kind);
        }
    }
    choice(fallback(family), FontMatch::Fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_reads() {
        assert!(table().len() > 40);
        let helvetica: Vec<_> = substitutes("helvetica").collect();
        assert_eq!(helvetica[0], ("Liberation Sans", true));
        assert_eq!(
            substitutes("Optima").next(),
            Some(("TeX Gyre Heros", false))
        );
        assert_eq!(substitutes("Unknown").count(), 0);
    }

    #[test]
    fn doc_05_the_font_itself_comes_first_then_metric_substitutes() {
        let installed = |names: &'static [&'static str]| move |f: &str| names.contains(&f);
        let choice = choose("Times", installed(&["Times", "Liberation Serif"]));
        assert_eq!(choice.kind, FontMatch::Installed);
        assert!(!choice.is_substitution());

        let choice = choose("Times", installed(&["TeX Gyre Termes", "Liberation Serif"]));
        assert_eq!(
            (&*choice.used, choice.kind),
            ("Liberation Serif", FontMatch::Metric)
        );

        let choice = choose("Optima", installed(&["TeX Gyre Heros"]));
        assert_eq!(
            (&*choice.used, choice.kind),
            ("TeX Gyre Heros", FontMatch::Style)
        );

        let choice = choose("Courier Prime", installed(&[]));
        assert_eq!(
            (&*choice.used, choice.kind),
            ("Liberation Mono", FontMatch::Fallback)
        );
        assert_eq!(fallback("Noto Serif Display"), "Liberation Serif");
        assert_eq!(fallback("Chalkboard"), "Liberation Sans");
    }
}
