// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Headers, footers and the fields of text (EST-12, EST-19, EST-27).
//!
//! A document has a header and a footer for most pages, and others for
//! the first page of a section and for left pages, which a section uses
//! when it asks for them. Each is a few paragraphs of their own, outside
//! the body.
//!
//! A field is one character of the text, U+FFFC, whose run says what it
//! shows: the number of the page it is on, the number of pages, a date or
//! the name of the file. Layout draws the value in its place.

use std::sync::Arc;

use crate::text::Paragraph;

/// The character that holds the place of a field in the text.
pub const FIELD_CHARACTER: char = '\u{fffc}';

/// What a field shows (EST-27).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// The number of the page it is on.
    PageNumber,
    /// The number of pages of the document.
    PageCount,
    /// A date and time, as it was when the field was inserted or last
    /// updated.
    DateTime {
        /// The pattern of the date, in the symbols of Unicode (LDML), such
        /// as `EEEE, d MMMM y`.
        pattern: Arc<str>,
        /// The date as it shows.
        shown: Arc<str>,
    },
    /// The name of the file of the document.
    FileName {
        /// The name as it shows.
        shown: Arc<str>,
    },
}

impl Field {
    /// The text that the field shows on a page with a number, in a
    /// document of so many pages.
    pub fn text(&self, page: u32, pages: u32) -> String {
        match self {
            Self::PageNumber => page.to_string(),
            Self::PageCount => pages.to_string(),
            Self::DateTime { shown, .. } | Self::FileName { shown } => shown.to_string(),
        }
    }
}

/// A header or a footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarginKind {
    /// At the top of the page.
    Header,
    /// At the bottom of the page.
    Footer,
}

/// Which pages a header or a footer is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarginPages {
    /// Every page that has no other: all of them, or the right pages of a
    /// section whose left pages differ.
    Default,
    /// The first page of a section whose first page differs.
    First,
    /// The left pages of a section whose left and right pages differ.
    Left,
}

/// One of the headers and footers of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarginPlace {
    /// Header or footer.
    pub kind: MarginKind,
    /// The pages it is for.
    pub pages: MarginPages,
}

impl MarginPlace {
    /// Every header and footer, in order.
    pub const ALL: [Self; 6] = [
        Self::new(MarginKind::Header, MarginPages::Default),
        Self::new(MarginKind::Header, MarginPages::First),
        Self::new(MarginKind::Header, MarginPages::Left),
        Self::new(MarginKind::Footer, MarginPages::Default),
        Self::new(MarginKind::Footer, MarginPages::First),
        Self::new(MarginKind::Footer, MarginPages::Left),
    ];

    /// Makes a place.
    pub const fn new(kind: MarginKind, pages: MarginPages) -> Self {
        Self { kind, pages }
    }

    /// The place of a page: the first of a section, or a left page, in a
    /// section with these flags.
    pub fn of_page(
        kind: MarginKind,
        first_of_section: bool,
        left: bool,
        first_page_different: bool,
        left_right_different: bool,
    ) -> Self {
        let pages = if first_of_section && first_page_different {
            MarginPages::First
        } else if left && left_right_different {
            MarginPages::Left
        } else {
            MarginPages::Default
        };
        Self::new(kind, pages)
    }

    fn index(self) -> usize {
        let kind = match self.kind {
            MarginKind::Header => 0,
            MarginKind::Footer => 3,
        };
        kind + match self.pages {
            MarginPages::Default => 0,
            MarginPages::First => 1,
            MarginPages::Left => 2,
        }
    }
}

/// The text of the headers and footers of a document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeadersFooters {
    texts: [Arc<[Paragraph]>; 6],
}

impl HeadersFooters {
    /// The paragraphs of a header or footer; none when it is empty.
    pub fn get(&self, place: MarginPlace) -> &[Paragraph] {
        &self.texts[place.index()]
    }

    /// Whether every header and footer is empty.
    pub fn is_empty(&self) -> bool {
        self.texts.iter().all(|t| t.iter().all(|p| p.is_empty()))
    }

    /// Every paragraph of every header and footer.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.texts.iter().flat_map(|t| t.iter())
    }

    pub(crate) fn replace(&mut self, place: MarginPlace, text: Vec<Paragraph>) -> Vec<Paragraph> {
        let old = std::mem::replace(&mut self.texts[place.index()], text.into());
        old.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_19_pages_choose_their_header() {
        let place = |first, left, fd, lr| {
            MarginPlace::of_page(MarginKind::Header, first, left, fd, lr).pages
        };
        assert_eq!(place(true, false, true, true), MarginPages::First);
        assert_eq!(place(true, true, false, true), MarginPages::Left);
        assert_eq!(place(false, true, true, true), MarginPages::Left);
        assert_eq!(place(false, true, true, false), MarginPages::Default);
        assert_eq!(place(false, false, true, true), MarginPages::Default);
    }

    #[test]
    fn est_27_fields_show_their_value() {
        assert_eq!(Field::PageNumber.text(3, 9), "3");
        assert_eq!(Field::PageCount.text(3, 9), "9");
        let date = Field::DateTime {
            pattern: "d/M/y".into(),
            shown: "1/11/2026".into(),
        };
        assert_eq!(date.text(1, 1), "1/11/2026");
    }
}
