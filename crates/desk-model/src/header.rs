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

use crate::id::NodeId;
use crate::list::NumberFormat;
use crate::text::Paragraph;

/// The character that holds the place of a field in the text.
pub const FIELD_CHARACTER: char = '\u{fffc}';

/// What a field shows (EST-27).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// The number of the page it is on.
    PageNumber {
        /// How the number is written.
        format: NumberFormat,
    },
    /// The number of pages of the document.
    PageCount {
        /// How the number is written.
        format: NumberFormat,
    },
    /// A date and time, as it was when the field was inserted or last
    /// updated.
    DateTime {
        /// The pattern of the date, in the symbols of Unicode (LDML), such
        /// as `EEEE, d MMMM y`.
        pattern: Arc<str>,
        /// The date as it shows.
        shown: Arc<str>,
        /// Whether it shows the date of the day the document is opened,
        /// as Pages' "Automatically update on open".
        update: bool,
    },
    /// The name of the file of the document.
    FileName {
        /// The name as it shows.
        shown: Arc<str>,
        /// Whether the folder of the file shows before its name.
        path: bool,
        /// Whether the extension of the file shows after its name.
        extension: bool,
    },
    /// The mark of a note (EST-13, EST-14): where the body refers to it,
    /// and at the start of its text. It shows the number of the note, or
    /// the mark of its own.
    NoteMark {
        /// The note.
        note: NodeId,
    },
    /// The number of the page that a paragraph of the body starts on, as
    /// the entries of a table of contents show it (EST-22).
    PageOf {
        /// The paragraph.
        paragraph: NodeId,
    },
    /// The place of an object in the line of the text, as Pages puts an
    /// inline object (OBJ-01): the object, anchored in the text, is drawn
    /// there, as a character as large as its frame.
    ObjectMark {
        /// The object.
        object: NodeId,
    },
}

impl Field {
    /// The number of the page, in digits.
    pub const PAGE_NUMBER: Self = Self::PageNumber {
        format: NumberFormat::Decimal,
    };
    /// The number of pages, in digits.
    pub const PAGE_COUNT: Self = Self::PageCount {
        format: NumberFormat::Decimal,
    };

    /// The text that the field shows on a page with a number, in a
    /// document of so many pages.
    pub fn text(&self, page: u32, pages: u32) -> String {
        match self {
            Self::PageNumber { format } => format.write(page),
            Self::PageCount { format } => format.write(pages),
            Self::DateTime { shown, .. } | Self::FileName { shown, .. } => shown.to_string(),
            // Layout numbers the notes, and knows the pages of
            // paragraphs.
            Self::NoteMark { .. } | Self::PageOf { .. } | Self::ObjectMark { .. } => String::new(),
        }
    }
}

/// Writes today's date in a pattern of LDML, or `None` when it cannot.
pub type DateWriter<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The name of a file as a field of it shows it: its folder before it,
/// and its extension after it, when the field shows them. `file` is the
/// path of the file, or only its name when it has no folder yet.
pub fn file_shown(file: &str, path: bool, extension: bool) -> String {
    let (folder, name) = match file.rfind('/') {
        Some(at) => (&file[..at], &file[at + 1..]),
        None => ("", file),
    };
    let name = match name.rfind('.') {
        Some(at) if !extension && at > 0 => &name[..at],
        _ => name,
    };
    if path && !folder.is_empty() {
        format!("{folder}/{name}")
    } else {
        name.to_owned()
    }
}

impl Field {
    /// The field again for a file and, when it shows the date the
    /// document is opened, for the date of today written by `date` from
    /// its pattern. None when it stays as it is.
    pub fn refreshed(&self, file: &str, date: Option<DateWriter<'_>>) -> Option<Self> {
        let field = match self {
            Self::FileName {
                shown,
                path,
                extension,
            } => {
                let now = file_shown(file, *path, *extension);
                (now != **shown).then(|| Self::FileName {
                    shown: now.into(),
                    path: *path,
                    extension: *extension,
                })?
            }
            Self::DateTime {
                pattern,
                shown,
                update: true,
            } => {
                let now = date.and_then(|date| date(pattern))?;
                (now != **shown).then(|| Self::DateTime {
                    pattern: pattern.clone(),
                    shown: now.into(),
                    update: true,
                })?
            }
            _ => return None,
        };
        Some(field)
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

    /// Its place among [`MarginPlace::ALL`].
    pub fn index(self) -> usize {
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
        assert_eq!(Field::PAGE_NUMBER.text(3, 9), "3");
        assert_eq!(Field::PAGE_COUNT.text(3, 9), "9");
        let roman = Field::PageNumber {
            format: NumberFormat::LowerRoman,
        };
        assert_eq!(roman.text(4, 9), "iv");
        let date = Field::DateTime {
            pattern: "d/M/y".into(),
            shown: "1/11/2026".into(),
            update: false,
        };
        assert_eq!(date.text(1, 1), "1/11/2026");
    }

    #[test]
    fn est_27_file_names_show_their_folder_and_extension() {
        let file = "/home/ana/Cartas/Convite.odt";
        assert_eq!(file_shown(file, false, true), "Convite.odt");
        assert_eq!(file_shown(file, false, false), "Convite");
        assert_eq!(file_shown(file, true, true), file);
        assert_eq!(file_shown(file, true, false), "/home/ana/Cartas/Convite");
        assert_eq!(file_shown("Sem título", true, false), "Sem título");
        assert_eq!(file_shown(".hidden", false, false), ".hidden");
        let date = Field::DateTime {
            pattern: "y".into(),
            shown: "2025".into(),
            update: true,
        };
        let today = |_: &str| Some("2026".to_owned());
        let refreshed = date.refreshed("", Some(&today)).unwrap();
        assert_eq!(refreshed.text(1, 1), "2026");
        let fixed = Field::DateTime {
            pattern: "y".into(),
            shown: "2025".into(),
            update: false,
        };
        assert_eq!(fixed.refreshed("", Some(&today)), None);
    }
}
