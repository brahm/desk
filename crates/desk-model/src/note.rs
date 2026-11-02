// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Footnotes and endnotes (EST-13, EST-14, EST-15).
//!
//! A note is a few paragraphs of its own, outside the body, which the body
//! refers to with a [`Field::NoteMark`] where its mark goes. Its text
//! starts with the same mark. Layout numbers the notes in the order the
//! body refers to them, unless a note has a mark of its own, and puts
//! them at the foot of their page or at the end of the document or of
//! their section, as the settings of the document say.
//!
//! [`Field::NoteMark`]: crate::Field::NoteMark

use std::sync::Arc;

use imbl::OrdMap;

use crate::id::NodeId;
use crate::text::Paragraph;
use crate::units::Length;

/// A footnote or an endnote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// The identifier, which the marks name.
    pub id: NodeId,
    /// A mark chosen by the user instead of a number (EST-14).
    pub mark: Option<Arc<str>>,
    /// The text.
    pub paragraphs: Arc<[Paragraph]>,
}

/// The notes of a document, by identifier.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes(OrdMap<NodeId, Note>);

impl Notes {
    /// A note by identifier.
    pub fn get(&self, id: NodeId) -> Option<&Note> {
        self.0.get(&id)
    }

    /// Every note, by identifier.
    pub fn iter(&self) -> impl Iterator<Item = &Note> {
        self.0.values()
    }

    /// Whether there is no note.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every paragraph of every note.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.0.values().flat_map(|n| n.paragraphs.iter())
    }

    pub(crate) fn replace(&mut self, id: NodeId, note: Option<Note>) -> Option<Note> {
        match note {
            Some(note) => self.0.insert(id, note),
            None => self.0.remove(&id),
        }
    }
}

/// Where the notes of a document go (EST-13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NoteKind {
    /// At the foot of the page that refers to them.
    #[default]
    Footnotes,
    /// At the end of the document.
    DocumentEndnotes,
    /// At the end of the section that refers to them.
    SectionEndnotes,
}

/// How notes are numbered (EST-13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NoteFormat {
    /// 1, 2, 3.
    #[default]
    Arabic,
    /// i, ii, iii.
    Roman,
    /// *, †, ‡, §, ‖, ¶, then the same twice.
    Symbols,
}

/// Where the numbers of the notes start again (EST-13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NoteNumbering {
    /// Never: they go on through the document.
    #[default]
    Continuous,
    /// On each page.
    Page,
    /// In each section.
    Section,
}

/// The settings of the notes of a document (EST-13, EST-15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NoteSettings {
    /// Where they go.
    pub kind: NoteKind,
    /// How they are numbered.
    pub format: NoteFormat,
    /// Where their numbers start again.
    pub numbering: NoteNumbering,
    /// The space between two notes, and between the text and the notes.
    pub gap: Length,
}

impl Default for NoteSettings {
    fn default() -> Self {
        Self {
            kind: NoteKind::Footnotes,
            format: NoteFormat::Arabic,
            numbering: NoteNumbering::Continuous,
            gap: Length::from_points(10),
        }
    }
}

/// The symbols of the notes, in order.
const SYMBOLS: [char; 6] = ['*', '\u{2020}', '\u{2021}', '\u{a7}', '\u{2016}', '\u{b6}'];

impl NoteFormat {
    /// The mark of the note with a number, from 1.
    pub fn label(self, number: u32) -> String {
        let number = number.max(1);
        match self {
            Self::Arabic => number.to_string(),
            Self::Roman => roman(number),
            Self::Symbols => {
                let index = (number - 1) as usize;
                let symbol = SYMBOLS[index % SYMBOLS.len()];
                std::iter::repeat_n(symbol, index / SYMBOLS.len() + 1).collect()
            }
        }
    }
}

/// A number in lower-case Roman numerals.
fn roman(mut number: u32) -> String {
    const DIGITS: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (value, digits) in DIGITS {
        while number >= value {
            out.push_str(digits);
            number -= value;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_13_notes_are_numbered_in_their_format() {
        assert_eq!(NoteFormat::Arabic.label(12), "12");
        assert_eq!(NoteFormat::Roman.label(14), "xiv");
        assert_eq!(NoteFormat::Symbols.label(1), "*");
        assert_eq!(NoteFormat::Symbols.label(2), "\u{2020}");
        assert_eq!(NoteFormat::Symbols.label(7), "**");
    }
}
