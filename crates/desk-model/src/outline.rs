// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The document as an outline (TXT-14): its headings are the topics, by
//! the level of their styles, "Heading" or "Heading 1" to "Heading 9", and
//! the other paragraphs are text under the topic before them. A topic
//! moves with its subtopics and its text.
//!
//! An outline goes to and from other programs as plain text with a tab
//! for each level below the first, as the outline of Keynote copies it
//! (IOP-26).

use std::ops::Range;

use crate::document::Document;
use crate::error::ModelError;
use crate::op::Op;
use crate::style::{ParagraphStyle, Style, StyleFamily, StyleId};
use crate::text::{Paragraph, Run};
use crate::transaction::Edit;

/// The deepest level of a topic.
pub const MAX_OUTLINE_LEVEL: u8 = 9;

/// The level of the topics of a style by its name: 1 to 9 for the
/// headings, none for text.
pub fn heading_level(name: &str) -> Option<u8> {
    let rest = name.strip_prefix("Heading")?;
    if rest.is_empty() {
        return Some(1);
    }
    let level: u8 = rest.strip_prefix(' ')?.trim().parse().ok()?;
    (1..=MAX_OUTLINE_LEVEL).contains(&level).then_some(level)
}

impl Document {
    /// The level of the topic that a paragraph of the body is, or none
    /// when it is text.
    pub fn outline_level(&self, index: usize) -> Option<u8> {
        let paragraph = self.body().get(index)?;
        let style = self.styles().get(paragraph.style())?;
        heading_level(style.name())
    }

    /// The paragraphs of a topic: it, its subtopics and the text under
    /// them. A paragraph of text is a topic of its own.
    pub fn topic_range(&self, index: usize) -> Range<usize> {
        let len = self.body().len();
        let Some(level) = self.outline_level(index) else {
            return index..(index + 1).min(len);
        };
        let mut end = index + 1;
        while end < len {
            if self.outline_level(end).is_some_and(|l| l <= level) {
                break;
            }
            end += 1;
        }
        index..end
    }

    /// Some paragraphs as outline text: a line for each, with a tab for
    /// each level below the first; text goes one level under its topic.
    pub fn outline_text(&self, range: Range<usize>) -> String {
        let mut out = String::new();
        let mut current = 0u8;
        for index in range {
            let Some(paragraph) = self.body().get(index) else {
                break;
            };
            let depth = match self.outline_level(index) {
                Some(level) => {
                    current = level;
                    level - 1
                }
                None => current,
            };
            for _ in 0..depth {
                out.push('\t');
            }
            out.push_str(&paragraph.text().replace(['\t', '\u{2028}'], " "));
            out.push('\n');
        }
        out
    }

    /// The paragraph style of the topics of a level, if the document has
    /// it.
    fn heading_style(&self, level: u8) -> Option<StyleId> {
        self.styles()
            .iter()
            .filter(|s| s.family() == StyleFamily::Paragraph)
            .find(|s| heading_level(s.name()) == Some(level))
            .map(Style::id)
    }
}

impl Edit {
    /// The paragraph style of the topics of a level, made from the level
    /// above when the document has none.
    pub fn heading_style(&mut self, level: u8) -> Result<StyleId, ModelError> {
        let level = level.clamp(1, MAX_OUTLINE_LEVEL);
        if let Some(id) = self.document().heading_style(level) {
            return Ok(id);
        }
        let above = if level > 1 {
            Some(self.heading_style(level - 1)?)
        } else {
            None
        };
        let id = self.new_id();
        let name = format!("Heading {level}");
        let mut style = match above.and_then(|a| self.document().styles().get(a).cloned()) {
            Some(Style::Paragraph(style)) => ParagraphStyle {
                id,
                name: name.as_str().into(),
                shortcut: None,
                ..style
            },
            _ => {
                let mut style = ParagraphStyle::new(id, &name);
                style.parent = Some(self.document().styles().default_paragraph());
                style.character.weight = Some(crate::FontWeight::BOLD);
                style
            }
        };
        style.next = Some(self.document().styles().default_paragraph());
        self.apply(Op::PutStyle {
            style: Style::Paragraph(style),
        })?;
        Ok(id)
    }

    /// Makes a paragraph a topic of a level, or text with none.
    pub fn set_outline_level(&mut self, index: usize, level: Option<u8>) -> Result<(), ModelError> {
        let style = match level {
            Some(level) => self.heading_style(level)?,
            None => self.document().styles().default_paragraph(),
        };
        let paragraph = self
            .document()
            .body()
            .get(index)
            .ok_or(ModelError::IndexOutOfBounds {
                index,
                len: self.document().body().len(),
            })?
            .clone();
        if paragraph.style() != style {
            self.apply(Op::SetParagraphFormat {
                paragraph: paragraph.id(),
                style,
                attrs: paragraph.attrs().clone(),
            })?;
        }
        Ok(())
    }

    /// Moves some paragraphs before paragraph `before`, as counted before
    /// the move, and gives where they are after it.
    pub fn move_paragraphs(
        &mut self,
        range: Range<usize>,
        before: usize,
    ) -> Result<Range<usize>, ModelError> {
        if range.is_empty() || (range.start..=range.end).contains(&before) {
            return Ok(range);
        }
        let paragraphs: Vec<Paragraph> = range
            .clone()
            .filter_map(|i| self.document().body().get(i).cloned())
            .collect();
        let mut at = before;
        for paragraph in &paragraphs {
            self.apply(Op::RemoveParagraph {
                paragraph: paragraph.id(),
            })?;
        }
        if before > range.end {
            at -= paragraphs.len();
        }
        for (i, paragraph) in paragraphs.into_iter().enumerate() {
            self.apply(Op::InsertParagraph {
                index: at + i,
                paragraph,
            })?;
        }
        Ok(at..at + range.len())
    }

    /// Inserts outline text before paragraph `at` as topics, by the tabs
    /// before each line, and gives the paragraphs inserted.
    pub fn insert_outline(&mut self, at: usize, text: &str) -> Result<Range<usize>, ModelError> {
        let at = at.min(self.document().body().len());
        let mut index = at;
        for line in text.lines() {
            let depth = line.chars().take_while(|c| *c == '\t').count();
            let content = line.trim_start_matches('\t').trim_end_matches('\r');
            let level = u8::try_from(depth + 1)
                .unwrap_or(MAX_OUTLINE_LEVEL)
                .min(MAX_OUTLINE_LEVEL);
            let style = self.heading_style(level)?;
            let id = self.new_id();
            let runs = (!content.is_empty()).then(|| Run::new(content));
            let paragraph = Paragraph::from_runs(id, style, crate::ParaAttrs::default(), runs)?;
            self.apply(Op::InsertParagraph { index, paragraph })?;
            index += 1;
        }
        Ok(at..index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Position;

    fn outline() -> Document {
        let mut document = Document::new_word_processing();
        document
            .transact("Outline", |edit| {
                edit.insert_outline(0, "Plan\n\tWhy\n\tHow\n\t\tSteps\nEnd")?;
                // The empty paragraph of the new document is left at the
                // end; it becomes text under the last topic.
                let last = edit.document().body().len() - 1;
                let id = edit.document().body().get(last).unwrap().id();
                edit.insert_text(Position::new(id, 0), "Notes")?;
                Ok(())
            })
            .unwrap();
        document
    }

    #[test]
    fn txt_14_headings_are_the_topics() {
        assert_eq!(heading_level("Heading"), Some(1));
        assert_eq!(heading_level("Heading 9"), Some(9));
        assert_eq!(heading_level("Heading 10"), None);
        assert_eq!(heading_level("Headings"), None);
        assert_eq!(heading_level("Body"), None);
        let document = outline();
        let levels: Vec<_> = (0..6).map(|i| document.outline_level(i)).collect();
        assert_eq!(levels, [Some(1), Some(2), Some(2), Some(3), Some(1), None]);
        assert_eq!(document.topic_range(2), 2..4);
        assert_eq!(document.topic_range(0), 0..4);
        assert_eq!(document.topic_range(4), 4..6);
    }

    #[test]
    fn iop_26_the_outline_goes_as_tabbed_text() {
        let document = outline();
        assert_eq!(
            document.outline_text(0..6),
            "Plan\n\tWhy\n\tHow\n\t\tSteps\nEnd\n\tNotes\n"
        );
    }

    #[test]
    fn txt_14_a_topic_moves_with_its_subtopics() {
        let mut document = outline();
        document
            .transact("Move", |edit| {
                let range = edit.document().topic_range(2);
                assert_eq!(edit.move_paragraphs(range, 1)?, 1..3);
                edit.set_outline_level(4, Some(3))
            })
            .unwrap();
        assert_eq!(
            document.outline_text(0..6),
            "Plan\n\tHow\n\t\tSteps\n\tWhy\n\t\tEnd\n\t\t\tNotes\n"
        );
        document
            .transact("Move", |edit| edit.move_paragraphs(0..1, 6).map(|_| ()))
            .unwrap();
        assert_eq!(document.body().get(5).unwrap().text(), "Plan");
    }
}
