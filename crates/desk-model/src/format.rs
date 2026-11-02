// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Character and paragraph formatting.
//!
//! Formatting comes in two forms. The attribute sets ([`CharAttrs`] and
//! [`ParaAttrs`]) have every field optional: a style or a run of text sets
//! only what it changes, and the rest is inherited. The resolved formats
//! ([`CharFormat`] and [`ParaFormat`]) have every field set, and are what
//! layout and drawing read.

use std::sync::Arc;

use crate::units::{Length, Rgba};

/// Declares an attribute set with optional fields and its resolved format,
/// with the same fields always set.
macro_rules! formatting {
    (
        $(#[$attrs_meta:meta])*
        attrs $attrs:ident;
        $(#[$format_meta:meta])*
        format $format:ident;
        fields {
            $(
                $(#[$field_meta:meta])*
                $field:ident: $ty:ty = $default:expr,
            )*
        }
    ) => {
        $(#[$attrs_meta])*
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $attrs {
            $(
                $(#[$field_meta])*
                pub $field: Option<$ty>,
            )*
        }

        $(#[$format_meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $format {
            $(
                $(#[$field_meta])*
                pub $field: $ty,
            )*
        }

        impl Default for $format {
            fn default() -> Self {
                Self {
                    $($field: $default,)*
                }
            }
        }

        impl From<$format> for $attrs {
            /// Every attribute set, to the value it has in the format.
            fn from(format: $format) -> Self {
                Self {
                    $($field: Some(format.$field),)*
                }
            }
        }

        impl $attrs {
            /// Whether no attribute is set.
            pub fn is_empty(&self) -> bool {
                true $(&& self.$field.is_none())*
            }

            /// Sets every attribute that `other` sets, keeping the others.
            pub fn merge(&mut self, other: &Self) {
                $(
                    if let Some(value) = &other.$field {
                        self.$field = Some(value.clone());
                    }
                )*
            }

            /// Writes the attributes that are set over a resolved format.
            pub fn apply_to(&self, format: &mut $format) {
                $(
                    if let Some(value) = &self.$field {
                        format.$field = value.clone();
                    }
                )*
            }
        }
    };
}

/// The style of an underline or a strikethrough (FMT-01, FMT-02).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineStyle {
    /// No line.
    #[default]
    None,
    /// One line.
    Single,
    /// Two lines.
    Double,
}

/// The color of a line drawn with the text, such as a strikethrough.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineColor {
    /// The color of the text.
    #[default]
    Text,
    /// A color of its own (FMT-02).
    Custom(Rgba),
}

/// The vertical position of the text (FMT-06).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Baseline {
    /// On the baseline.
    #[default]
    Normal,
    /// Raised, in a smaller font.
    Superscript,
    /// Lowered, in a smaller font.
    Subscript,
}

/// A change of case applied when the text is drawn (FMT-07).
///
/// The text keeps the case it was typed in, as in Pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Capitalization {
    /// The text as typed.
    #[default]
    None,
    /// Every letter in upper case.
    AllCaps,
    /// Lower-case letters drawn as small capitals.
    SmallCaps,
    /// The first letter of each word in upper case.
    Title,
}

/// A language, as a BCP 47 tag such as `pt-BR` (FMT-22).
///
/// It chooses the dictionaries for spelling and hyphenation, and the rules
/// for line breaking.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Language(Arc<str>);

impl Language {
    /// Makes a language from a BCP 47 tag.
    pub fn new(tag: &str) -> Self {
        Self(tag.into())
    }

    /// The undetermined language, `und`, used when nothing sets one.
    pub fn undetermined() -> Self {
        Self::new("und")
    }

    /// The BCP 47 tag.
    pub fn tag(&self) -> &str {
        &self.0
    }
}

formatting! {
    /// Character attributes that a style or a run of text sets.
    attrs CharAttrs;
    /// Character formatting with every attribute resolved.
    format CharFormat;
    fields {
        /// The font family name.
        font_family: Arc<str> = "Helvetica".into(),
        /// The font size (FMT-05).
        font_size: Length = Length::from_points(12),
        /// Bold (FMT-01).
        bold: bool = false,
        /// Italic (FMT-01).
        italic: bool = false,
        /// Underline (FMT-01).
        underline: LineStyle = LineStyle::None,
        /// Strikethrough (FMT-02).
        strikethrough: LineStyle = LineStyle::None,
        /// The color of the strikethrough (FMT-02).
        strikethrough_color: LineColor = LineColor::Text,
        /// The color of the text (FMT-11).
        color: Rgba = Rgba::BLACK,
        /// Superscript or subscript (FMT-06).
        baseline: Baseline = Baseline::Normal,
        /// An exact raise (positive) or lowering (negative) of the
        /// baseline, which adds to the superscript or subscript (FMT-06).
        baseline_shift: Length = Length::ZERO,
        /// A change of case when drawing (FMT-07).
        capitalization: Capitalization = Capitalization::None,
        /// The language of the text (FMT-22).
        language: Language = Language::undetermined(),
    }
}

/// The horizontal alignment of a paragraph (FMT-12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Alignment {
    /// Aligned to the left.
    #[default]
    Left,
    /// Centered.
    Center,
    /// Aligned to the right.
    Right,
    /// Aligned on both sides, except the last line.
    Justify,
    /// Text to the left and numbers to the right, as "Auto Align" does in
    /// table cells.
    Auto,
}

/// The spacing between the lines of a paragraph (FMT-14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineSpacing {
    /// A multiple of the natural line height, in percent: 100 is single
    /// and 200 is double.
    Multiple {
        /// The multiple, in percent.
        percent: u32,
    },
    /// At least this distance from one baseline to the next.
    AtLeast(Length),
    /// Exactly this distance from one baseline to the next.
    Exactly(Length),
    /// This space added between the lines, without making the lines
    /// taller.
    Between(Length),
}

impl LineSpacing {
    /// Single spacing.
    pub const SINGLE: Self = Self::Multiple { percent: 100 };
    /// Double spacing.
    pub const DOUBLE: Self = Self::Multiple { percent: 200 };
}

impl Default for LineSpacing {
    fn default() -> Self {
        Self::SINGLE
    }
}

/// Where the text after a tab lines up with its tab stop (FMT-16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TabAlignment {
    /// The text starts at the stop.
    #[default]
    Left,
    /// The text is centered on the stop.
    Center,
    /// The text ends at the stop.
    Right,
    /// The decimal separator of the text is at the stop, and text without
    /// one ends there.
    Decimal,
}

/// What fills the space that a tab leaves before its text (FMT-16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TabLeader {
    /// Nothing.
    #[default]
    None,
    /// A row of dots.
    Dots,
    /// A row of dashes.
    Dashes,
    /// A line.
    Line,
    /// A row of arrows.
    Arrows,
}

impl TabLeader {
    /// The character repeated to fill the space, if any.
    pub fn character(self) -> Option<char> {
        match self {
            Self::None => None,
            Self::Dots => Some('.'),
            Self::Dashes => Some('-'),
            Self::Line => Some('_'),
            Self::Arrows => Some('\u{2192}'),
        }
    }
}

/// A tab stop (FMT-16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabStop {
    /// The distance from the left edge of the text, as the ruler shows it,
    /// not from the indent.
    pub position: Length,
    /// How the text lines up with the stop.
    pub alignment: TabAlignment,
    /// What fills the space before the text.
    pub leader: TabLeader,
    /// The decimal separator, for a decimal stop.
    pub decimal: char,
}

impl TabStop {
    /// A left stop without a leader at a position.
    pub fn left(position: Length) -> Self {
        Self {
            position,
            alignment: TabAlignment::Left,
            leader: TabLeader::None,
            decimal: '.',
        }
    }
}

/// The tab stops of a paragraph, ordered by position, with at most one at
/// each position (FMT-16).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TabStops(Arc<[TabStop]>);

impl TabStops {
    /// No stops: tabs go to the default stops.
    pub fn none() -> Self {
        Self::default()
    }

    /// The stops, ordered by position. Of two stops at one position, the
    /// last is kept.
    pub fn new(stops: impl IntoIterator<Item = TabStop>) -> Self {
        let mut stops: Vec<TabStop> = stops.into_iter().collect();
        stops.reverse();
        stops.sort_by_key(|s| s.position);
        stops.dedup_by_key(|s| s.position);
        Self(stops.into())
    }

    /// The stops, ordered by position.
    pub fn stops(&self) -> &[TabStop] {
        &self.0
    }

    /// Whether there are no stops.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// These stops with one added, or put in place of the one at its
    /// position.
    pub fn with(&self, stop: TabStop) -> Self {
        Self::new(self.0.iter().copied().chain([stop]))
    }

    /// These stops without the one at a position.
    pub fn without(&self, position: Length) -> Self {
        Self::new(self.0.iter().copied().filter(|s| s.position != position))
    }

    /// The first stop past `x`, a distance from the left edge of the text.
    pub fn after(&self, x: Length) -> Option<&TabStop> {
        self.0.iter().find(|s| s.position > x)
    }
}

/// The distance between the default tab stops of Pages, half an inch.
pub const DEFAULT_TAB_INTERVAL: Length = Length::from_points(36);

formatting! {
    /// Paragraph attributes that a style or a paragraph sets.
    attrs ParaAttrs;
    /// Paragraph formatting with every attribute resolved.
    format ParaFormat;
    fields {
        /// The horizontal alignment (FMT-12).
        alignment: Alignment = Alignment::Left,
        /// The spacing between lines (FMT-14).
        line_spacing: LineSpacing = LineSpacing::SINGLE,
        /// The space above the paragraph. Between two paragraphs, the
        /// larger of the space after and the space before applies
        /// (FMT-15).
        space_before: Length = Length::ZERO,
        /// The space below the paragraph (FMT-15).
        space_after: Length = Length::ZERO,
        /// The indent of the first line, relative to the left indent. A
        /// negative value makes a hanging indent (FMT-18).
        first_line_indent: Length = Length::ZERO,
        /// The left indent (FMT-18).
        left_indent: Length = Length::ZERO,
        /// The right indent (FMT-18).
        right_indent: Length = Length::ZERO,
        /// Start the paragraph on a new page (EST-04, EST-05).
        page_break_before: bool = false,
        /// Keep the paragraph on the same page as the next one (EST-05).
        keep_with_next: bool = false,
        /// Keep the lines of the paragraph on one page (EST-05).
        keep_lines_together: bool = false,
        /// Avoid a single line of the paragraph alone at the top or the
        /// bottom of a page (EST-05).
        widow_control: bool = false,
        /// The tab stops (FMT-16).
        tab_stops: TabStops = TabStops::none(),
        /// The distance between the default tab stops, which tabs go to
        /// past the last stop (FMT-16).
        default_tab_interval: Length = DEFAULT_TAB_INTERVAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_overrides_only_what_is_set() {
        let mut attrs = CharAttrs {
            bold: Some(true),
            italic: Some(true),
            ..CharAttrs::default()
        };
        attrs.merge(&CharAttrs {
            italic: Some(false),
            font_size: Some(Length::from_points(14)),
            ..CharAttrs::default()
        });
        assert_eq!(attrs.bold, Some(true));
        assert_eq!(attrs.italic, Some(false));
        assert_eq!(attrs.font_size, Some(Length::from_points(14)));
        assert!(!attrs.is_empty());
        assert!(CharAttrs::default().is_empty());
    }

    #[test]
    fn fmt_16_tab_stops_are_ordered_and_one_per_position() {
        let at = |pt| Length::from_points(pt);
        let stops = TabStops::new([TabStop::left(at(144)), TabStop::left(at(72))]);
        let right = TabStop {
            alignment: TabAlignment::Right,
            ..TabStop::left(at(144))
        };
        let stops = stops.with(right);
        assert_eq!(stops.stops().len(), 2);
        assert_eq!(stops.stops()[0].position, at(72));
        assert_eq!(stops.stops()[1].alignment, TabAlignment::Right);
        assert_eq!(stops.after(at(72)).map(|s| s.position), Some(at(144)));
        assert_eq!(stops.after(at(144)), None);
        assert_eq!(stops.without(at(72)).stops().len(), 1);
    }

    #[test]
    fn apply_to_fills_a_format() {
        let mut format = ParaFormat::default();
        ParaAttrs {
            alignment: Some(Alignment::Justify),
            line_spacing: Some(LineSpacing::Exactly(Length::from_points(18))),
            ..ParaAttrs::default()
        }
        .apply_to(&mut format);
        assert_eq!(format.alignment, Alignment::Justify);
        assert_eq!(
            format.line_spacing,
            LineSpacing::Exactly(Length::from_points(18))
        );
        assert_eq!(format.space_before, Length::ZERO);
    }
}
