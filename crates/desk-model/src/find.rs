// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding and replacing text (TXT-12, TXT-13).
//!
//! A search matches text inside each paragraph, in the order of the flow,
//! as Find & Replace and the Search sidebar of Pages do: a match does not
//! cross the end of a paragraph, and invisible characters such as the
//! paragraph breaks are not searched. Without Match Case, letters match
//! whatever their case; with Whole Words, a match has no letter or digit
//! right before or after it.

use std::ops::Range;

use crate::document::Document;
use crate::error::ModelError;
use crate::id::NodeId;
use crate::op::Op;
use crate::story::Story;
use crate::text::{Position, Run};
use crate::transaction::Edit;

/// What to look for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Query {
    /// The text.
    pub text: String,
    /// Whether upper and lower case must be the same as in the text.
    pub match_case: bool,
    /// Whether the text must be whole words.
    pub whole_words: bool,
}

/// A piece of text that a search found.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Found {
    /// The paragraph.
    pub paragraph: NodeId,
    /// Its index in the flow.
    pub index: usize,
    /// The range in the paragraph, in bytes.
    pub range: Range<usize>,
    /// The header, footer or note it is in, or none for the body; the
    /// index is then that of the paragraph in it.
    pub story: Option<Story>,
}

impl Found {
    /// The position of its start.
    pub fn start(&self) -> Position {
        Position::new(self.paragraph, self.range.start)
    }

    /// The position of its end.
    pub fn end(&self) -> Position {
        Position::new(self.paragraph, self.range.end)
    }
}

/// Text with its letters folded to lower case, and the offset in the text
/// of each byte of the folded text, with the end of the text last.
fn folded(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut offsets = Vec::with_capacity(text.len() + 1);
    for (offset, c) in text.char_indices() {
        for lower in c.to_lowercase() {
            let start = out.len();
            out.push(lower);
            offsets.extend(std::iter::repeat_n(offset, out.len() - start));
        }
    }
    offsets.push(text.len());
    (out, offsets)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Query {
    /// A search for text, in any case and inside words.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            ..Self::default()
        }
    }

    /// The ranges of a text that match, without overlaps.
    pub fn matches_in(&self, text: &str) -> Vec<Range<usize>> {
        if self.text.is_empty() {
            return Vec::new();
        }
        let (haystack, offsets, needle) = if self.match_case {
            (text.to_owned(), Vec::new(), self.text.clone())
        } else {
            let (haystack, offsets) = folded(text);
            (haystack, offsets, folded(&self.text).0)
        };
        // An offset of the searched text in the text.
        let original = |at: usize| {
            if offsets.is_empty() { at } else { offsets[at] }
        };
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(found) = haystack[from..].find(&needle) {
            let start = from + found;
            let end = start + needle.len();
            // A letter that folds to more than one, as İ does, ends where
            // the next one starts.
            let range = original(start)..original(end);
            from = end;
            if range.is_empty()
                || out
                    .last()
                    .is_some_and(|l: &Range<usize>| l.end > range.start)
            {
                continue;
            }
            if self.whole_words {
                let before = text[..range.start].chars().next_back();
                let after = text[range.end..].chars().next();
                if before.is_some_and(is_word_char) || after.is_some_and(is_word_char) {
                    continue;
                }
            }
            out.push(range);
        }
        out
    }
}

impl Document {
    /// Everything that a search finds in the body, in order.
    pub fn find(&self, query: &Query) -> Vec<Found> {
        find_in(self.body().iter(), None, query)
    }

    /// Everything that a search finds in the body, and then in each of
    /// some headers, footers and notes, in order (TXT-12).
    pub fn find_everywhere(&self, query: &Query, stories: &[Story]) -> Vec<Found> {
        let mut out = self.find(query);
        for &story in stories {
            let paragraphs = self.story_paragraphs(story);
            out.extend(find_in(paragraphs.iter(), Some(story), query));
        }
        out
    }
}

fn find_in<'a>(
    paragraphs: impl Iterator<Item = &'a crate::text::Paragraph>,
    story: Option<Story>,
    query: &Query,
) -> Vec<Found> {
    let mut out = Vec::new();
    for (index, paragraph) in paragraphs.enumerate() {
        for range in query.matches_in(&paragraph.text()) {
            out.push(Found {
                paragraph: paragraph.id(),
                index,
                range,
                story,
            });
        }
    }
    out
}

impl Edit {
    /// Replaces a piece of text with other text, which takes the
    /// formatting of the first character replaced, and gives the position
    /// after it.
    pub fn replace_found(&mut self, found: &Found, with: &str) -> Result<Position, ModelError> {
        let paragraph = self.document().paragraph(found.paragraph)?;
        let first = paragraph
            .text()
            .get(found.range.start..)
            .and_then(|t| t.chars().next())
            .map_or(0, char::len_utf8);
        let (style, attrs) = paragraph.format_at(found.range.start + first);
        let with: String = with.chars().filter(|c| !matches!(c, '\n' | '\r')).collect();
        let runs = if with.is_empty() {
            Vec::new()
        } else {
            vec![Run::with_attrs(&with, style, attrs)]
        };
        self.apply(Op::ReplaceText {
            paragraph: found.paragraph,
            range: found.range.clone(),
            runs,
        })?;
        Ok(Position::new(
            found.paragraph,
            found.range.start + with.len(),
        ))
    }

    /// Replaces everything a search finds in the body (Replace All), and
    /// gives how many pieces were replaced.
    pub fn replace_all(&mut self, query: &Query, with: &str) -> Result<usize, ModelError> {
        let found = self.document().find(query);
        // From the end, so that the ranges before stay where they are.
        for piece in found.iter().rev() {
            self.replace_found(piece, with)?;
        }
        Ok(found.len())
    }

    /// Replaces everything a search finds in the body and in some
    /// headers, footers and notes (Replace All), and gives how many
    /// pieces were replaced.
    pub fn replace_everywhere(
        &mut self,
        query: &Query,
        with: &str,
        stories: &[Story],
    ) -> Result<usize, ModelError> {
        let mut count = self.replace_all(query, with)?;
        for &story in stories {
            let paragraphs = self.document().story_paragraphs(story);
            if find_in(paragraphs.iter(), None, query).is_empty() {
                continue;
            }
            let mut copy = self.document().with_body(paragraphs);
            let mut replaced = 0;
            copy.transact("Replace All", |edit| {
                replaced = edit.replace_all(query, with)?;
                Ok(())
            })?;
            count += replaced;
            self.set_story(story, copy.body().iter().cloned().collect())?;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(query: &Query, text: &str) -> Vec<(usize, usize)> {
        query
            .matches_in(text)
            .into_iter()
            .map(|r| (r.start, r.end))
            .collect()
    }

    #[test]
    fn case_folds_unless_asked() {
        let query = Query::new("ação");
        assert_eq!(ranges(&query, "Ação e AÇÃO"), [(0, 6), (9, 15)]);
        let query = Query {
            match_case: true,
            ..query
        };
        assert_eq!(ranges(&query, "Ação e ação"), [(9, 15)]);
    }

    #[test]
    fn whole_words_need_boundaries() {
        let query = Query {
            whole_words: true,
            ..Query::new("cat")
        };
        assert_eq!(ranges(&query, "cat, concat cats (cat)"), [(0, 3), (18, 21)]);
    }

    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(ranges(&Query::new("aa"), "aaaa"), [(0, 2), (2, 4)]);
        assert!(ranges(&Query::new(""), "text").is_empty());
    }
}
