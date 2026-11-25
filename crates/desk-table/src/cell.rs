// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cells: what they hold and how they look.

use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::conditional::Rule;
use crate::format::Format;

/// The text of a cell, as the app keeps it: plain text in Desk Tables and
/// formatted paragraphs in Desk Writer and Desk Slides.
pub trait CellText: Clone + Debug + PartialEq + Eq + Hash {
    /// The text without its formatting, with a line feed between
    /// paragraphs.
    fn plain(&self) -> String;

    /// Text without formatting, with a paragraph for each line.
    fn from_plain(text: &str) -> Self;

    /// The texts joined, with a tab between them when `across` or a line
    /// break (U+2028) otherwise, as merging cells joins them in Pages 15.4
    /// (TAB-23, W131).
    fn join(parts: &[Self], across: bool) -> Self {
        let sep = if across { "\t" } else { "\u{2028}" };
        let plain: Vec<String> = parts.iter().map(Self::plain).collect();
        Self::from_plain(&plain.join(sep))
    }
}

impl CellText for String {
    fn plain(&self) -> String {
        self.clone()
    }

    fn from_plain(text: &str) -> Self {
        text.to_owned()
    }
}

/// A number of a cell, compared by its bits so that the model stays
/// comparable and hashable.
#[derive(Clone, Copy, Debug, Default, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct Num(pub f64);

impl PartialEq for Num {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for Num {}

impl Hash for Num {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

/// What a cell holds, as it was typed (TAB-16, TAB-18).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Content<T> {
    /// Nothing.
    #[default]
    Empty,
    /// Text.
    Text(T),
    /// A number.
    Number(Num),
    /// True or false, as a checkbox shows it.
    Bool(bool),
    /// A date and time, in days from 30 December 1899.
    Date(Num),
    /// A length of time, in days.
    Duration(Num),
    /// A formula, as it is written, with the `=` (TAB-32).
    Formula(String),
}

impl<T> Content<T> {
    /// Whether the cell holds nothing.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// The formula, if the cell holds one.
    pub fn formula(&self) -> Option<&str> {
        match self {
            Self::Formula(f) => Some(f),
            _ => None,
        }
    }
}

/// A color, from 0 to 255 in each channel.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Opacity, 255 for opaque.
    pub a: u8,
}

impl Color {
    /// An opaque color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Black.
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// White.
    pub const WHITE: Self = Self::rgb(255, 255, 255);
}

/// How a cell is filled (TAB-22).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CellFill {
    /// One color.
    Color(Color),
    /// A gradient from one color to another, at an angle in degrees.
    Gradient {
        /// The color it starts with.
        start: Color,
        /// The color it ends with.
        end: Color,
        /// The angle, counterclockwise from left to right.
        angle: i16,
    },
    /// A picture, as the app keeps it, scaled to fill the cell.
    Image(Arc<[u8]>),
}

/// Where the text of a cell sits across it.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum HAlign {
    /// Text to the left, numbers to the right, as iWork aligns by
    /// default.
    #[default]
    Auto,
    /// To the left.
    Left,
    /// In the middle.
    Center,
    /// To the right.
    Right,
    /// Filling the width.
    Justify,
}

/// Where the text of a cell sits down it.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum VAlign {
    /// At the top.
    #[default]
    Top,
    /// In the middle.
    Middle,
    /// At the bottom.
    Bottom,
}

/// The character formatting of a cell that holds no text, and that typing
/// in it starts with: what the format bar sets for the whole cell.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TextStyle {
    /// The font family; none for that of the table.
    pub font: Option<String>,
    /// The size, in thousandths of a point; none for that of the table.
    pub size: Option<u32>,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Struck through.
    pub strike: bool,
    /// The color; none for black.
    pub color: Option<Color>,
}

/// How a cell looks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CellStyle {
    /// Its fill; none for that of its area of the table.
    pub fill: Option<CellFill>,
    /// The formatting of its text.
    pub text: TextStyle,
    /// Where the text sits across.
    pub h_align: HAlign,
    /// Where the text sits down.
    pub v_align: VAlign,
    /// Whether text longer than the cell wraps to new lines; otherwise it
    /// runs into empty cells beside it (TAB-20).
    pub wrap: bool,
}

/// A cell.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Cell<T> {
    /// What it holds.
    pub content: Content<T>,
    /// How its value is written (TAB-28).
    pub format: Format,
    /// How it looks.
    pub style: CellStyle,
    /// The rules of conditional formatting (TAB-21).
    pub rules: Arc<[Rule]>,
    /// The comment on it, by the identifier under which the app keeps it
    /// with its other comments and their replies (TAB-27, W149).
    pub comment: Option<std::num::NonZeroU64>,
    /// The tracked change of what it holds or how it looks, with what it
    /// was before, which rejecting the change puts back (W148, REV-05);
    /// none when it is in no change.
    #[serde(default = "untracked", skip_serializing_if = "Option::is_none")]
    pub tracked: Option<Arc<Tracked<T>>>,
}

/// No tracked change, for a cell written without one.
fn untracked<T>() -> Option<Arc<Tracked<T>>> {
    None
}

/// What a cell held and how it looked before a tracked change, as Pages
/// '09 tracked the content, formulas, format, fill and text of cells
/// (W148, REV-05, D-003), by the identifier under which the app keeps
/// the change with its other changes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Tracked<T> {
    /// The change.
    pub change: std::num::NonZeroU64,
    /// What it held.
    pub content: Content<T>,
    /// How its value was written.
    pub format: Format,
    /// How it looked.
    pub style: CellStyle,
    /// Whether authors other than that of the change changed it after,
    /// which its mark shows in gray (REV-05).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub others: bool,
}

impl<T> Default for Cell<T> {
    fn default() -> Self {
        Self {
            content: Content::Empty,
            format: Format::Automatic,
            style: CellStyle {
                wrap: true,
                ..CellStyle::default()
            },
            rules: Arc::from([]),
            comment: None,
            tracked: None,
        }
    }
}

impl<T> Cell<T> {
    /// The cell moved `cols` columns and `rows` rows: the relative
    /// references of its formula and of its rules move along.
    pub(crate) fn move_references(&mut self, cols: i64, rows: i64) {
        if let Content::Formula(f) = &self.content
            && let Ok(parsed) = desk_formula::Formula::parse(f)
        {
            self.content = Content::Formula(parsed.moved(cols, rows).to_string());
        }
        self.rules = crate::conditional::map_rule_cells(&self.rules, &mut |r| r.moved(cols, rows));
    }

    /// An empty cell that looks like this one, as a row added takes the
    /// look of the row beside it, in no tracked change.
    pub fn cleared(&self) -> Self {
        Self {
            content: Content::Empty,
            format: self.format.clone(),
            style: self.style.clone(),
            rules: self.rules.clone(),
            comment: self.comment,
            tracked: None,
        }
    }
}
