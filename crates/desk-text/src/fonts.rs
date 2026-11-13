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

/// The fonts that Desk brings, each with the advance widths and kerning
/// of a font of the Mac as Pages lays it out, in each of its weights
/// (DOC-05): Desk Neue, which `data/fonts/desk-neue` makes from TeX Gyre
/// Heros for Helvetica Neue, and those that `data/fonts/make-substitutes.py`
/// makes from free fonts for the fonts of the Pages templates.
pub(crate) const BUNDLED: [&[u8]; 51] = [
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-Bold.otf"),
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-BoldItalic.otf"),
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-Italic.otf"),
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-Light.otf"),
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-LightItalic.otf"),
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-Medium.otf"),
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-MediumItalic.otf"),
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-Regular.otf"),
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-UltraLight.otf"),
    include_bytes!("../../../data/fonts/desk-neue/DeskNeue-UltraLightItalic.otf"),
    include_bytes!("../../../data/fonts/desk-humanist/DeskHumanist-Bold.ttf"),
    include_bytes!("../../../data/fonts/desk-humanist/DeskHumanist-BoldItalic.ttf"),
    include_bytes!("../../../data/fonts/desk-humanist/DeskHumanist-Italic.ttf"),
    include_bytes!("../../../data/fonts/desk-humanist/DeskHumanist-Light.ttf"),
    include_bytes!("../../../data/fonts/desk-humanist/DeskHumanist-LightItalic.ttf"),
    include_bytes!("../../../data/fonts/desk-humanist/DeskHumanist-Regular.ttf"),
    include_bytes!("../../../data/fonts/desk-humanist/DeskHumanist-SemiBold.ttf"),
    include_bytes!("../../../data/fonts/desk-humanist/DeskHumanist-SemiBoldItalic.ttf"),
    include_bytes!("../../../data/fonts/desk-humanist/DeskHumanist-UltraBold.ttf"),
    include_bytes!("../../../data/fonts/desk-flare/DeskFlare-Bold.ttf"),
    include_bytes!("../../../data/fonts/desk-flare/DeskFlare-BoldItalic.ttf"),
    include_bytes!("../../../data/fonts/desk-flare/DeskFlare-ExtraBlack.ttf"),
    include_bytes!("../../../data/fonts/desk-flare/DeskFlare-Italic.ttf"),
    include_bytes!("../../../data/fonts/desk-flare/DeskFlare-Regular.ttf"),
    include_bytes!("../../../data/fonts/desk-geometric/DeskGeometric-Bold.ttf"),
    include_bytes!("../../../data/fonts/desk-geometric/DeskGeometric-CondensedExtraBold.ttf"),
    include_bytes!("../../../data/fonts/desk-geometric/DeskGeometric-Medium.ttf"),
    include_bytes!("../../../data/fonts/desk-geometric/DeskGeometric-MediumItalic.ttf"),
    include_bytes!("../../../data/fonts/desk-transitional/DeskTransitional-Bold.ttf"),
    include_bytes!("../../../data/fonts/desk-transitional/DeskTransitional-BoldItalic.ttf"),
    include_bytes!("../../../data/fonts/desk-transitional/DeskTransitional-Italic.ttf"),
    include_bytes!("../../../data/fonts/desk-transitional/DeskTransitional-Regular.ttf"),
    include_bytes!("../../../data/fonts/desk-transitional/DeskTransitional-SemiBold.ttf"),
    include_bytes!("../../../data/fonts/desk-transitional/DeskTransitional-SemiBoldItalic.ttf"),
    include_bytes!("../../../data/fonts/desk-oldstyle/DeskOldstyle-Black.ttf"),
    include_bytes!("../../../data/fonts/desk-oldstyle/DeskOldstyle-BlackItalic.ttf"),
    include_bytes!("../../../data/fonts/desk-oldstyle/DeskOldstyle-Italic.ttf"),
    include_bytes!("../../../data/fonts/desk-oldstyle/DeskOldstyle-Regular.ttf"),
    include_bytes!("../../../data/fonts/desk-didone/DeskDidone-Bold.ttf"),
    include_bytes!("../../../data/fonts/desk-didone/DeskDidone-Italic.ttf"),
    include_bytes!("../../../data/fonts/desk-didone/DeskDidone-Regular.ttf"),
    include_bytes!("../../../data/fonts/desk-engraver/DeskEngraver-Bold.ttf"),
    include_bytes!("../../../data/fonts/desk-engraver/DeskEngraver-BoldItalic.ttf"),
    include_bytes!("../../../data/fonts/desk-engraver/DeskEngraver-Italic.ttf"),
    include_bytes!("../../../data/fonts/desk-engraver/DeskEngraver-Regular.ttf"),
    include_bytes!("../../../data/fonts/desk-typewriter/DeskTypewriter-Bold.ttf"),
    include_bytes!("../../../data/fonts/desk-typewriter/DeskTypewriter-Light.ttf"),
    include_bytes!("../../../data/fonts/desk-typewriter/DeskTypewriter-Regular.ttf"),
    include_bytes!("../../../data/fonts/desk-typewriter/DeskTypewriter-SemiBold.ttf"),
    include_bytes!("../../../data/fonts/desk-marker/DeskMarker-Thin.ttf"),
    include_bytes!("../../../data/fonts/desk-marker/DeskMarker-Wide.ttf"),
];

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

/// How Pages lays out a font of the Mac, as measured on the reference
/// machine (`data/fonts/originals.tsv`). Desk lays the family out this way
/// whatever font stands for it, so that its lines are as tall and as wide
/// as in Pages (DOC-05).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PagesFont {
    /// The height of the text above the baseline, in ems.
    pub ascent: f32,
    /// The depth of the text below the baseline, in ems.
    pub descent: f32,
    /// The space added to each line, in ems.
    pub leading: f32,
    /// Whether the leading goes above the text, as with Apple's own fonts,
    /// rather than below it.
    pub leading_above: bool,
    /// Whether a line whose leading rounds to nothing gets a fifth of its
    /// height above the text in its place, as in some faces of Didot and
    /// Futura.
    pub fifth: bool,
    /// Whether fi and fl are joined into ligatures.
    pub ligatures: bool,
    /// Whether ff and ffi are joined too.
    pub ff_ligatures: bool,
    /// Whether ffl is joined too, as in Hoefler Text. No other letters
    /// are.
    pub ffl_ligatures: bool,
    /// The space added between letters, in ems, so that a line of the font
    /// that stands for the family is as wide as in Pages.
    pub spacing: f32,
}

/// How Pages lays out a face of a family, when it was measured: the face
/// of that weight and slant when it lays out otherwise, or else the
/// family.
pub fn pages_font(family: &str, weight: u16, italic: bool) -> Option<PagesFont> {
    static TABLE: OnceLock<Vec<(&'static str, PagesFont)>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        include_str!("../../../data/fonts/originals.tsv")
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .skip(1)
            .filter_map(|line| {
                let fields: Vec<&str> = line.split('\t').collect();
                let (family, ascent, descent, leading, place, ligatures, spacing) = match fields[..]
                {
                    [family, ascent, descent, leading, place, ligatures] => {
                        (family, ascent, descent, leading, place, ligatures, "0")
                    }
                    [family, ascent, descent, leading, place, ligatures, spacing] => {
                        (family, ascent, descent, leading, place, ligatures, spacing)
                    }
                    _ => return None,
                };
                Some((
                    family,
                    PagesFont {
                        ascent: ascent.parse().ok()?,
                        descent: descent.parse().ok()?,
                        leading: leading.parse().ok()?,
                        leading_above: place == "above",
                        fifth: place == "fifth",
                        ligatures: ligatures.starts_with("fi fl"),
                        ff_ligatures: ligatures.starts_with("fi fl ff ffi"),
                        ffl_ligatures: ligatures == "fi fl ff ffi ffl",
                        spacing: spacing.parse().ok()?,
                    },
                ))
            })
            .collect()
    });
    let family = family.trim();
    let find = |wanted: &str| {
        table
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
            .map(|&(_, font)| font)
    };
    let weights: &[&str] = match weight {
        0..=200 => &["UltraLight"],
        201..=350 => &["Light"],
        351..=450 => &[],
        451..=550 => &["Medium"],
        751..=850 => &["ExtraBold", "Bold"],
        _ => &["Bold"],
    };
    let faces = || {
        weights
            .iter()
            .map(|face| format!("{family} {face}"))
            .chain([family.to_owned()])
    };
    italic
        .then(|| faces().find_map(|face| find(&format!("{face} Italic"))))
        .flatten()
        .or_else(|| faces().find_map(|face| find(&face)))
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
        assert_eq!(substitutes("Optima").next(), Some(("Desk Flare", true)));
        assert_eq!(
            substitutes("Optima").nth(1),
            Some(("TeX Gyre Heros", false))
        );
        assert_eq!(substitutes("Unknown").count(), 0);
    }

    #[test]
    fn the_fonts_of_pages_read() {
        let helvetica = pages_font("helvetica", 400, false).unwrap();
        assert_eq!(helvetica.ascent, 0.77002);
        assert!(helvetica.leading_above && helvetica.ligatures && !helvetica.ff_ligatures);
        assert_eq!(pages_font("Helvetica", 700, true), Some(helvetica));
        let arial = pages_font("Arial", 400, false).unwrap();
        assert!(!arial.leading_above && !arial.ligatures);
        assert_eq!(pages_font("Optima", 400, false), None);

        // Each face of Helvetica Neue has its own height, measured on the
        // reference machine, and its leading below the text.
        let neue = |weight| pages_font("Helvetica Neue", weight, false).unwrap();
        assert_eq!((neue(400).ascent, neue(400).leading), (0.952, 0.028));
        assert!(!neue(400).leading_above && neue(400).ff_ligatures);
        assert_eq!(neue(300).ascent, 0.967);
        assert!(neue(300).ligatures && !neue(300).ff_ligatures);
        assert_eq!(neue(100).ascent, 0.931);
        assert_eq!((neue(500).ascent, neue(500).descent), (0.975, 0.217));
        assert_eq!(neue(700), neue(900));
        // Desk Neue has the widths of each face, so none needs spacing.
        assert_eq!((neue(400).spacing, neue(300).spacing), (0.0, 0.0));
        assert_eq!(helvetica.spacing, 0.0);
        assert!(neue(700).ff_ligatures);

        // The faces of the fonts of the templates that lay out otherwise
        // than the font that stands for them says.
        let hoefler = pages_font("Hoefler Text", 900, true).unwrap();
        assert!(hoefler.leading_above && hoefler.ffl_ligatures && hoefler.ff_ligatures);
        assert!(pages_font("Didot", 400, false).unwrap().fifth);
        assert!(pages_font("Didot", 700, false).unwrap().fifth);
        assert!(!pages_font("Didot", 400, true).unwrap().fifth);
        assert!(pages_font("Futura", 700, false).unwrap().fifth);
        assert!(!pages_font("Futura", 800, false).unwrap().fifth);
        assert_eq!(pages_font("Futura", 500, false), None);
        assert!(!neue(400).fifth && !neue(400).ffl_ligatures);
    }

    #[test]
    fn doc_05_desk_neue_stands_for_helvetica_neue_in_each_weight() {
        let mut system = crate::TextSystem::new();
        let choice = system.font_choice("Helvetica Neue");
        assert_eq!(
            (&*choice.used, choice.kind),
            ("Desk Neue", FontMatch::Metric)
        );
    }

    #[test]
    fn doc_05_fonts_of_desk_stand_for_the_fonts_of_the_pages_templates() {
        let mut system = crate::TextSystem::new();
        for (family, stands) in [
            ("Gill Sans", "Desk Humanist"),
            ("Optima", "Desk Flare"),
            ("Futura", "Desk Geometric"),
            ("Baskerville", "Desk Transitional"),
            ("Hoefler Text", "Desk Oldstyle"),
            ("Didot", "Desk Didone"),
            ("Cochin", "Desk Engraver"),
            ("American Typewriter", "Desk Typewriter"),
            ("Marker Felt", "Desk Marker"),
        ] {
            let choice = system.font_choice(family);
            assert_eq!(
                (&*choice.used, choice.kind),
                (stands, FontMatch::Metric),
                "{family}"
            );
        }
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
