// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The text of a document as assistive technologies read it: the
//! paragraphs one after the other, each ending with a new line, with
//! offsets counted in characters, as the accessibility interfaces of GTK
//! count them.

use desk_layout::DocumentLayout;
use desk_model::Document;
use unicode_segmentation::UnicodeSegmentation;

use crate::text_nav::Spot;

/// How much text an offset of the text names, as screen readers ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// One character.
    Character,
    /// A word and the spaces after it.
    Word,
    /// A sentence.
    Sentence,
    /// A line as laid out.
    Line,
    /// A paragraph and its new line.
    Paragraph,
}

/// The text of a document, with where its paragraphs and lines start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccessText {
    text: String,
    /// The character offset where each paragraph starts.
    paragraphs: Vec<u32>,
    /// The character ranges of the lines, in order.
    lines: Vec<(u32, u32)>,
}

impl AccessText {
    /// The text of a document as laid out.
    pub fn new(document: &Document, layout: &DocumentLayout) -> Self {
        let mut text = String::new();
        let mut paragraphs = Vec::new();
        let mut chars = 0u32;
        let mut texts = Vec::new();
        for paragraph in document.body().iter() {
            let t = paragraph.text();
            paragraphs.push(chars);
            chars += t.chars().count() as u32 + 1;
            text.push_str(&t);
            text.push('\n');
            texts.push(t);
        }
        let mut lines = Vec::new();
        for page in &layout.pages {
            for line in &page.lines {
                let (Some(start), Some(t)) =
                    (paragraphs.get(line.paragraph), texts.get(line.paragraph))
                else {
                    continue;
                };
                let count = |bytes: usize| t.get(..bytes).map_or(0, |s| s.chars().count()) as u32;
                let range = &line.line.range;
                let mut end = start + count(range.end);
                // The last line of a paragraph takes its new line.
                if range.end >= t.len() {
                    end += 1;
                }
                lines.push((start + count(range.start), end));
            }
        }
        Self {
            text,
            paragraphs,
            lines,
        }
    }

    /// The whole text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The number of characters.
    pub fn len(&self) -> u32 {
        self.text.chars().count() as u32
    }

    /// Whether there is no text, not even a paragraph.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The character offset of a spot.
    pub fn offset_of(&self, document: &Document, spot: Spot) -> u32 {
        let Some(start) = self.paragraphs.get(spot.paragraph) else {
            return self.len();
        };
        let text = document
            .body()
            .get(spot.paragraph)
            .map(|p| p.text())
            .unwrap_or_default();
        start + text.get(..spot.offset).map_or(0, |s| s.chars().count()) as u32
    }

    /// The text between two character offsets.
    pub fn slice(&self, start: u32, end: u32) -> &str {
        let byte = |offset: u32| {
            self.text
                .char_indices()
                .nth(offset as usize)
                .map_or(self.text.len(), |(i, _)| i)
        };
        let (start, end) = (start.min(end), end.max(start));
        &self.text[byte(start)..byte(end)]
    }

    /// The unit of text around a character offset, as character offsets.
    pub fn range_at(&self, offset: u32, unit: Unit) -> (u32, u32) {
        let len = self.len();
        let offset = offset.min(len);
        match unit {
            Unit::Character => (offset, (offset + 1).min(len)),
            Unit::Line => self
                .lines
                .iter()
                .copied()
                .find(|&(start, end)| offset >= start && offset < end)
                .unwrap_or_else(|| self.range_at(offset, Unit::Paragraph)),
            Unit::Paragraph => {
                let index = self.paragraphs.partition_point(|&p| p <= offset).max(1) - 1;
                let start = self.paragraphs.get(index).copied().unwrap_or(0);
                let end = self.paragraphs.get(index + 1).copied().unwrap_or(len);
                (start, end)
            }
            Unit::Sentence => self
                .pieces(self.text.split_sentence_bounds())
                .into_iter()
                .find(|&(_, end)| offset < end)
                .unwrap_or((len, len)),
            Unit::Word => {
                // A word takes the spaces after it, as ATK did; a new line
                // stands alone.
                let mut words: Vec<(u32, u32)> = Vec::new();
                let mut start = 0;
                for piece in self.text.split_word_bounds() {
                    let end = start + piece.chars().count() as u32;
                    let blank = piece.chars().all(char::is_whitespace);
                    let joins = blank && !piece.contains('\n');
                    if start > offset && !joins {
                        break;
                    }
                    match words.last_mut() {
                        Some(last) if joins => last.1 = end,
                        _ => words.push((start, end)),
                    }
                    start = end;
                }
                words
                    .into_iter()
                    .find(|&(_, end)| offset < end)
                    .unwrap_or((len, len))
            }
        }
    }

    /// The character ranges of pieces of the text.
    fn pieces<'a>(&self, pieces: impl Iterator<Item = &'a str>) -> Vec<(u32, u32)> {
        let mut start = 0;
        pieces
            .map(|piece| {
                let end = start + piece.chars().count() as u32;
                let range = (start, end);
                start = end;
                range
            })
            .collect()
    }
}

/// What changed between two texts: where the change starts, how many
/// characters it removed, and how many it inserted.
pub fn change(old: &str, new: &str) -> (u32, u32, u32) {
    let old: Vec<char> = old.chars().collect();
    let new: Vec<char> = new.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    (
        prefix as u32,
        (old.len() - prefix - suffix) as u32,
        (new.len() - prefix - suffix) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{Editor, Position};
    use desk_text::TextSystem;

    fn text(paragraphs: &[&str]) -> (Document, AccessText) {
        let mut editor = Editor::new(Document::new_word_processing());
        editor
            .edit("Type", |edit| {
                let first = edit.document().body().get(0).unwrap().id();
                edit.insert_text(Position::new(first, 0), &paragraphs.join("\n"))
            })
            .unwrap();
        let document = editor.into_document();
        let layout = desk_layout::paginate(&document, &mut TextSystem::new());
        let access = AccessText::new(&document, &layout);
        (document, access)
    }

    #[test]
    fn the_text_is_the_paragraphs_with_new_lines() {
        let (document, access) = text(&["Café crème", "Second"]);
        assert_eq!(access.text(), "Café crème\nSecond\n");
        assert_eq!(access.len(), 18);
        // Offsets count characters, not bytes: "é" is one.
        assert_eq!(access.offset_of(&document, Spot::new(0, 5)), 4);
        assert_eq!(access.offset_of(&document, Spot::new(1, 2)), 13);
        assert_eq!(access.slice(5, 10), "crème");
    }

    #[test]
    fn units_around_an_offset() {
        let (_, access) = text(&["One two. Three", "Last"]);
        assert_eq!(access.range_at(1, Unit::Character), (1, 2));
        // A word takes the space after it.
        assert_eq!(access.range_at(1, Unit::Word), (0, 4));
        assert_eq!(access.range_at(5, Unit::Word), (4, 7));
        assert_eq!(access.range_at(2, Unit::Sentence), (0, 9));
        assert_eq!(access.range_at(3, Unit::Paragraph), (0, 15));
        assert_eq!(access.range_at(16, Unit::Paragraph), (15, 20));
        // One line each, with its new line.
        assert_eq!(access.range_at(3, Unit::Line), (0, 15));
        assert_eq!(access.range_at(17, Unit::Line), (15, 20));
    }

    #[test]
    fn a_change_is_what_differs_between_the_texts() {
        assert_eq!(change("abc\n", "abXc\n"), (2, 0, 1));
        assert_eq!(change("abc\n", "ac\n"), (1, 1, 0));
        assert_eq!(change("ação\n", "acção\n"), (1, 0, 1));
        assert_eq!(change("same", "same"), (4, 0, 0));
    }
}
