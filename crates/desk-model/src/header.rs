// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Headers, footers and the fields of text (EST-12, EST-19, EST-27).
//!
//! A document has a header and a footer for most pages, and others for
//! the first page of a section and for left pages, which a section uses
//! when it asks for them. Each is a few paragraphs of their own, outside
//! the body.
//!
//! Each header and footer has three fields, left, centre and right, as in
//! Pages 15.4 (W096). The left one alone, with the others empty, is the
//! free header of Pages '09, as wide as the text (D-003); once the centre
//! or the right one has text, each takes a third of the width.
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
    /// A merge field (W105, MAL-01): a datum of the recipient of a mail
    /// merge, or of its sender, by the name of the field, as `First Name`
    /// for a contact or a column of a spreadsheet. Without data it shows
    /// its name, as its label.
    Merge {
        /// The name of the field.
        name: Arc<str>,
        /// Whether it is a sender field, which stays a field after a merge
        /// and is filled from the card of the sender (MAL-01, MAL-02).
        sender: bool,
        /// Whether an address shows on one line, its lines joined by
        /// commas (MAL-08).
        single_line: bool,
        /// The datum it shows, once a contact or a record filled it; none
        /// shows its name.
        shown: Option<Arc<str>>,
        /// The source of data and the table that another program named,
        /// kept for it; none for the source of the document.
        origin: Option<Arc<(String, String)>>,
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
            Self::Merge {
                name,
                single_line,
                shown,
                ..
            } => match shown {
                Some(shown) if *single_line => one_line(shown),
                Some(shown) => shown.to_string(),
                None => name.to_string(),
            },
            // Layout numbers the notes, and knows the pages of
            // paragraphs.
            Self::NoteMark { .. } | Self::PageOf { .. } | Self::ObjectMark { .. } => String::new(),
        }
    }
}

/// The lines of an address on one line, joined by commas (MAL-08).
pub fn one_line(text: &str) -> String {
    text.split(['\n', crate::LINE_BREAK])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

impl Field {
    /// A merge field of a name, for the recipient, with no datum yet.
    pub fn merge(name: &str) -> Self {
        Self::Merge {
            name: name.into(),
            sender: false,
            single_line: false,
            shown: None,
            origin: None,
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

/// One of the three fields of a header or footer (W096).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarginField {
    /// At the left; alone, the free header of Pages '09, as wide as the
    /// text.
    Left,
    /// In the middle.
    Centre,
    /// At the right.
    Right,
}

impl MarginField {
    /// The three, from left to right.
    pub const ALL: [Self; 3] = [Self::Left, Self::Centre, Self::Right];

    /// Its place among [`MarginField::ALL`].
    pub const fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Centre => 1,
            Self::Right => 2,
        }
    }
}

/// One of the headers and footers of a document, and one of its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarginPlace {
    /// Header or footer.
    pub kind: MarginKind,
    /// The pages it is for.
    pub pages: MarginPages,
    /// Its field.
    pub field: MarginField,
}

impl MarginPlace {
    /// The left field of every header and footer, in order: each header
    /// and footer once.
    pub const MARGINS: [Self; 6] = [
        Self::new(MarginKind::Header, MarginPages::Default),
        Self::new(MarginKind::Header, MarginPages::First),
        Self::new(MarginKind::Header, MarginPages::Left),
        Self::new(MarginKind::Footer, MarginPages::Default),
        Self::new(MarginKind::Footer, MarginPages::First),
        Self::new(MarginKind::Footer, MarginPages::Left),
    ];

    /// Every field of every header and footer, in order: the fields of
    /// each, from left to right, in the order of [`MarginPlace::MARGINS`].
    pub const ALL: [Self; 18] = {
        let mut all = [Self::MARGINS[0]; 18];
        let mut i = 0;
        while i < 18 {
            let margin = Self::MARGINS[i / 3];
            all[i] = margin.with_field(MarginField::ALL[i % 3]);
            i += 1;
        }
        all
    };

    /// Makes the place of the left field of a header or footer.
    pub const fn new(kind: MarginKind, pages: MarginPages) -> Self {
        Self {
            kind,
            pages,
            field: MarginField::Left,
        }
    }

    /// The same header or footer, at another field.
    pub const fn with_field(self, field: MarginField) -> Self {
        Self { field, ..self }
    }

    /// Its three fields, from left to right.
    pub const fn fields(self) -> [Self; 3] {
        [
            self.with_field(MarginField::Left),
            self.with_field(MarginField::Centre),
            self.with_field(MarginField::Right),
        ]
    }

    /// The place of a page: the first of a section, or a left page, in a
    /// section with these flags; its left field.
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
    pub const fn index(self) -> usize {
        self.margin_index() * 3 + self.field.index()
    }

    /// The place of its header or footer among [`MarginPlace::MARGINS`].
    pub const fn margin_index(self) -> usize {
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
    texts: [Arc<[Paragraph]>; 18],
}

impl HeadersFooters {
    /// The paragraphs of a field of a header or footer; none when it is
    /// empty.
    pub fn get(&self, place: MarginPlace) -> &[Paragraph] {
        &self.texts[place.index()]
    }

    /// Whether a field of a header or footer has no text.
    pub fn is_field_empty(&self, place: MarginPlace) -> bool {
        self.get(place).iter().all(Paragraph::is_empty)
    }

    /// Whether the header or footer of a place shows its fields in thirds
    /// of the width: when its centre or right field has text (W096).
    /// Otherwise its left field is a free header, as wide as the text.
    pub fn is_split(&self, place: MarginPlace) -> bool {
        !self.is_field_empty(place.with_field(MarginField::Centre))
            || !self.is_field_empty(place.with_field(MarginField::Right))
    }

    /// Whether every header and footer is empty.
    pub fn is_empty(&self) -> bool {
        self.texts.iter().all(|t| t.iter().all(|p| p.is_empty()))
    }

    /// The same headers and footers with each paragraph as `f` makes it.
    pub(crate) fn map_paragraphs(&self, f: &dyn Fn(&Paragraph) -> Paragraph) -> Self {
        Self {
            texts: std::array::from_fn(|i| self.texts[i].iter().map(f).collect()),
        }
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
    fn w096_each_header_and_footer_has_three_fields() {
        assert_eq!(MarginPlace::ALL.len(), 18);
        for (i, place) in MarginPlace::ALL.iter().enumerate() {
            assert_eq!(place.index(), i);
            assert_eq!(
                MarginPlace::MARGINS[place.margin_index()].fields()[i % 3],
                *place
            );
        }
        let header = MarginPlace::new(MarginKind::Header, MarginPages::Default);
        assert_eq!(header.field, MarginField::Left);
        let mut margins = HeadersFooters::default();
        let paragraph = |text: &str| {
            let id = |n| NodeId::new(n).unwrap();
            Paragraph::from_runs(
                id(1),
                id(2),
                crate::format::ParaAttrs::default(),
                [crate::text::Run::new(text)],
            )
            .unwrap()
        };
        margins.replace(header, vec![paragraph("Title")]);
        assert!(!margins.is_split(header));
        let right = header.with_field(MarginField::Right);
        margins.replace(right, vec![paragraph("1")]);
        assert!(margins.is_split(header));
        assert_eq!(margins.get(right)[0].text(), "1");
        assert_eq!(margins.get(header)[0].text(), "Title");
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
