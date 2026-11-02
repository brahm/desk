// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sections and layouts of a word processing document (EST-07 to EST-10,
//! EST-16, EST-18).
//!
//! The body is a sequence of sections, each a sequence of layouts, as in
//! Pages. A section starts on a new page and numbers its pages; a layout
//! sets the columns of the text and its margins, and may start on the
//! same page as the layout before it. The first section and layout of the
//! body are in the page setup; each later one starts at a paragraph,
//! which holds it.

use crate::units::Length;

/// The most columns a layout has, as in Pages.
pub const MAX_COLUMNS: usize = 10;

/// A millionth: columns are measured in parts of the width of their
/// layout, as Pages keeps them, so that they follow the margins.
pub const PARTS: u32 = 1_000_000;

/// The columns of a layout (EST-07).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Columns {
    /// How many, from 1 to [`MAX_COLUMNS`].
    pub count: u8,
    /// Whether the columns are as wide as each other, with the gap of the
    /// first between each two.
    pub equal: bool,
    /// The width of each column, in millionths of the width of the layout.
    pub widths: [u32; MAX_COLUMNS],
    /// The gap after each column, in millionths of the width of the
    /// layout.
    pub gaps: [u32; MAX_COLUMNS],
}

impl Columns {
    /// The gap between columns of a new layout in Pages: a twentieth of
    /// the width.
    pub const DEFAULT_GAP: u32 = PARTS / 20;

    /// One column, as wide as the layout.
    pub const ONE: Self = Self {
        count: 1,
        equal: true,
        widths: [PARTS, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        gaps: [Self::DEFAULT_GAP; MAX_COLUMNS],
    };

    /// `count` columns as wide as each other, with a gap of `gap`
    /// millionths between each two.
    pub fn equal(count: u8, gap: u32) -> Self {
        let count = count.clamp(1, MAX_COLUMNS as u8);
        let n = u32::from(count);
        let gap = gap.min(PARTS / n.max(2));
        let width = (PARTS - gap * (n - 1)) / n;
        let mut columns = Self {
            count,
            equal: true,
            widths: [0; MAX_COLUMNS],
            gaps: [gap; MAX_COLUMNS],
        };
        for w in &mut columns.widths[..usize::from(count)] {
            *w = width;
        }
        columns
    }

    /// The left edge and width of each column in a layout `width` wide,
    /// from the left of the layout.
    pub fn spans(&self, width: f64) -> Vec<(f64, f64)> {
        let count = usize::from(self.count.clamp(1, MAX_COLUMNS as u8));
        let part = |p: u32| width * f64::from(p) / f64::from(PARTS);
        if self.equal {
            let gap = part(self.gaps[0]);
            let w = (width - gap * (count - 1) as f64) / count as f64;
            return (0..count).map(|i| (i as f64 * (w + gap), w)).collect();
        }
        let mut x = 0.0;
        (0..count)
            .map(|i| {
                let span = (x, part(self.widths[i]));
                x += part(self.widths[i]) + part(self.gaps[i]);
                span
            })
            .collect()
    }
}

impl Default for Columns {
    fn default() -> Self {
        Self::ONE
    }
}

/// A layout: columns and the margins of the text inside the page margins
/// (EST-07, EST-09, EST-10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Layout {
    /// The columns.
    pub columns: Columns,
    /// The space before the layout, from the layout above or the top
    /// margin.
    pub before: Length,
    /// The space after the layout, to the layout below or the bottom
    /// margin.
    pub after: Length,
    /// How far in from the left margin the layout starts.
    pub left: Length,
    /// How far in from the right margin the layout ends.
    pub right: Length,
    /// Whether the layout starts on a new page rather than below the
    /// layout before it.
    pub new_page: bool,
}

/// The page that a section starts on (EST-16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SectionStart {
    /// The next page.
    #[default]
    AnyPage,
    /// The next right page, with a blank page before it when needed.
    RightPage,
    /// The next left page.
    LeftPage,
}

/// A section: pages numbered on their own, with their own headers and
/// footers (EST-16, EST-18, EST-19).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Section {
    /// The number of the first page, or `None` to count on from the
    /// section before (EST-18).
    pub first_number: Option<u32>,
    /// Whether the first page has headers and footers of its own (EST-19).
    pub first_page_different: bool,
    /// Whether left and right pages have headers and footers of their own
    /// (EST-19).
    pub left_right_different: bool,
    /// The page the section starts on.
    pub start: SectionStart,
}

/// Where a new section or layout starts: at the paragraph that holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Start {
    /// A new section, with the first layout of the section.
    Section(Section, Layout),
    /// A new layout, in the same section.
    Layout(Layout),
}

impl Start {
    /// The layout that starts here.
    pub fn layout(&self) -> &Layout {
        match self {
            Self::Section(_, layout) | Self::Layout(layout) => layout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_07_columns_share_the_width() {
        let columns = Columns::equal(2, Columns::DEFAULT_GAP);
        let spans = columns.spans(400.0);
        assert_eq!(spans.len(), 2);
        assert!((spans[0].1 - 190.0).abs() < 1e-6);
        assert!((spans[1].0 - 210.0).abs() < 1e-6);
        let one = Columns::ONE.spans(300.0);
        assert_eq!(one, [(0.0, 300.0)]);
        let mut uneven = Columns::equal(2, 0);
        uneven.equal = false;
        uneven.widths[0] = PARTS / 4;
        uneven.gaps[0] = PARTS / 10;
        uneven.widths[1] = PARTS * 13 / 20;
        let spans = uneven.spans(100.0);
        assert!((spans[1].0 - 35.0).abs() < 1e-6 && (spans[1].1 - 65.0).abs() < 1e-6);
    }
}
