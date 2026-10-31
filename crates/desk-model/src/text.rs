// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paragraphs, runs of text and text flows.
//!
//! A paragraph holds its text as runs: pieces of text with the same
//! formatting. Runs are always normalized: none is empty, and two neighbors
//! never have the same formatting. So two paragraphs with the same text
//! and formatting are equal, which is what lets undo be checked by
//! comparing documents.
//!
//! Offsets are in bytes of the UTF-8 text and must fall between
//! characters.

use std::ops::Range;
use std::sync::Arc;

use imbl::Vector;

use crate::error::ModelError;
use crate::format::{CharAttrs, ParaAttrs};
use crate::id::NodeId;
use crate::style::StyleId;

/// A manual line break inside a paragraph, which Shift-Return inserts
/// (EST-06). It is U+2028 LINE SEPARATOR.
pub const LINE_BREAK: char = '\u{2028}';

/// A space that never breaks a line, which Option-Space inserts (TXT-08).
/// It is U+00A0 NO-BREAK SPACE.
pub const NO_BREAK_SPACE: char = '\u{a0}';

/// Whether a character separates paragraphs, and so cannot be in a run.
pub(crate) fn is_paragraph_separator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{85}' | '\u{2029}')
}

/// A place in the text: a paragraph and a byte offset in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    /// The paragraph.
    pub paragraph: NodeId,
    /// The offset in bytes of the paragraph text.
    pub offset: usize,
}

impl Position {
    /// Makes a position.
    pub const fn new(paragraph: NodeId, offset: usize) -> Self {
        Self { paragraph, offset }
    }
}

/// A piece of text with the same formatting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    text: Arc<str>,
    /// The character style.
    pub style: Option<StyleId>,
    /// Attributes set on the text itself, over the styles.
    pub attrs: CharAttrs,
}

impl Run {
    /// Makes a run without a character style or attributes.
    ///
    /// The text cannot hold a paragraph separator, such as a line feed:
    /// those split paragraphs instead. The model checks this when the run
    /// goes into a paragraph.
    pub fn new(text: &str) -> Self {
        Self::with_attrs(text, None, CharAttrs::default())
    }

    /// Makes a run with a character style and attributes.
    pub fn with_attrs(text: &str, style: Option<StyleId>, attrs: CharAttrs) -> Self {
        Self {
            text: text.into(),
            style,
            attrs,
        }
    }

    /// The text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The length of the text in bytes.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Whether two runs have the same formatting.
    pub fn same_format(&self, other: &Self) -> bool {
        self.style == other.style && self.attrs == other.attrs
    }

    /// A run with part of the text and the same formatting.
    pub(crate) fn slice(&self, range: Range<usize>) -> Self {
        Self {
            text: self.text[range].into(),
            style: self.style,
            attrs: self.attrs.clone(),
        }
    }

    fn check(&self) -> Result<(), ModelError> {
        if self.text.chars().any(is_paragraph_separator) {
            Err(ModelError::InvalidText)
        } else {
            Ok(())
        }
    }
}

/// Puts runs in normal form: no empty run, and neighbors with the same
/// formatting joined.
fn normalize(runs: impl IntoIterator<Item = Run>) -> Vector<Run> {
    let mut out: Vector<Run> = Vector::new();
    for run in runs {
        if run.is_empty() {
            continue;
        }
        match out.back_mut() {
            Some(last) if last.same_format(&run) => {
                let mut text = String::with_capacity(last.len() + run.len());
                text.push_str(&last.text);
                text.push_str(&run.text);
                last.text = text.into();
            }
            _ => out.push_back(run),
        }
    }
    out
}

/// A paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paragraph {
    id: NodeId,
    style: StyleId,
    attrs: ParaAttrs,
    runs: Vector<Run>,
    len: usize,
}

impl Paragraph {
    /// Makes an empty paragraph.
    pub fn new(id: NodeId, style: StyleId) -> Self {
        Self {
            id,
            style,
            attrs: ParaAttrs::default(),
            runs: Vector::new(),
            len: 0,
        }
    }

    /// Makes a paragraph from runs, which are normalized.
    pub fn from_runs(
        id: NodeId,
        style: StyleId,
        attrs: ParaAttrs,
        runs: impl IntoIterator<Item = Run>,
    ) -> Result<Self, ModelError> {
        let runs = normalize(runs);
        for run in &runs {
            run.check()?;
        }
        let len = runs.iter().map(Run::len).sum();
        Ok(Self {
            id,
            style,
            attrs,
            runs,
            len,
        })
    }

    /// The identifier.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// The paragraph style.
    pub fn style(&self) -> StyleId {
        self.style
    }

    /// Attributes set on the paragraph itself, over its style.
    pub fn attrs(&self) -> &ParaAttrs {
        &self.attrs
    }

    /// The runs, in normal form.
    pub fn runs(&self) -> impl Iterator<Item = &Run> {
        self.runs.iter()
    }

    /// The length of the text in bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the paragraph has no text.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The text of the paragraph, without formatting.
    pub fn text(&self) -> String {
        let mut text = String::with_capacity(self.len);
        for run in &self.runs {
            text.push_str(run.text());
        }
        text
    }

    /// Whether an offset falls between two characters of the text.
    pub fn is_boundary(&self, offset: usize) -> bool {
        if offset > self.len {
            return false;
        }
        let mut start = 0;
        for run in &self.runs {
            let end = start + run.len();
            if offset < end {
                return run.text.is_char_boundary(offset - start);
            }
            start = end;
        }
        true
    }

    fn check_range(&self, range: &Range<usize>) -> Result<(), ModelError> {
        if range.start <= range.end && self.is_boundary(range.start) && self.is_boundary(range.end)
        {
            Ok(())
        } else {
            Err(ModelError::InvalidRange {
                paragraph: self.id,
                range: range.clone(),
            })
        }
    }

    /// The runs of part of the text, cut at the ends of the range.
    pub fn slice(&self, range: Range<usize>) -> Result<Vec<Run>, ModelError> {
        self.check_range(&range)?;
        let mut out = Vec::new();
        let mut start = 0;
        for run in &self.runs {
            let end = start + run.len();
            let a = start.max(range.start);
            let b = end.min(range.end);
            if a < b {
                out.push(run.slice(a - start..b - start));
            }
            start = end;
        }
        Ok(out)
    }

    /// The formatting that text typed at an offset takes: that of the
    /// character before it, or of the first character at the start.
    pub fn format_at(&self, offset: usize) -> (Option<StyleId>, CharAttrs) {
        let mut start = 0;
        for run in &self.runs {
            let end = start + run.len();
            if offset <= end && (offset > start || start == 0) {
                return (run.style, run.attrs.clone());
            }
            start = end;
        }
        (None, CharAttrs::default())
    }

    /// Replaces a range of the text with runs, and returns the runs that
    /// were there.
    pub(crate) fn replace(
        &mut self,
        range: Range<usize>,
        insert: Vec<Run>,
    ) -> Result<Vec<Run>, ModelError> {
        self.check_range(&range)?;
        for run in &insert {
            run.check()?;
        }
        let mut before = Vec::new();
        let mut removed = Vec::new();
        let mut after = Vec::new();
        let mut start = 0;
        for run in &self.runs {
            let end = start + run.len();
            if start < range.start {
                before.push(run.slice(0..end.min(range.start) - start));
            }
            let (a, b) = (start.max(range.start), end.min(range.end));
            if a < b {
                removed.push(run.slice(a - start..b - start));
            }
            if end > range.end {
                after.push(run.slice(start.max(range.end) - start..end - start));
            }
            start = end;
        }
        self.runs = normalize(before.into_iter().chain(insert).chain(after));
        self.len = self.runs.iter().map(Run::len).sum();
        Ok(removed)
    }

    pub(crate) fn set_format(&mut self, style: StyleId, attrs: ParaAttrs) -> (StyleId, ParaAttrs) {
        let old_style = std::mem::replace(&mut self.style, style);
        let old_attrs = std::mem::replace(&mut self.attrs, attrs);
        (old_style, old_attrs)
    }

    /// Whether the paragraph or one of its runs uses a style.
    pub(crate) fn uses_style(&self, id: StyleId) -> bool {
        self.style == id || self.runs.iter().any(|run| run.style == Some(id))
    }

    /// Checks that the runs are in normal form. Only for tests.
    #[doc(hidden)]
    pub fn is_normalized(&self) -> bool {
        let mut previous: Option<&Run> = None;
        for run in &self.runs {
            if run.is_empty() || previous.is_some_and(|p| p.same_format(run)) {
                return false;
            }
            previous = Some(run);
        }
        self.len == self.runs.iter().map(Run::len).sum::<usize>()
    }
}

/// A sequence of paragraphs: the body of a document and, later, the text
/// of a text box, a shape or a table cell.
///
/// It knows nothing about pages, so the same flow lays out in a page, a
/// slide or a cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFlow {
    paragraphs: Vector<Paragraph>,
}

impl TextFlow {
    /// Makes a flow with one paragraph. A flow always has at least one.
    pub fn new(first: Paragraph) -> Self {
        let mut paragraphs = Vector::new();
        paragraphs.push_back(first);
        Self { paragraphs }
    }

    /// The number of paragraphs.
    pub fn len(&self) -> usize {
        self.paragraphs.len()
    }

    /// Whether the flow has no paragraphs, which never happens.
    pub fn is_empty(&self) -> bool {
        self.paragraphs.is_empty()
    }

    /// The paragraph at an index.
    pub fn get(&self, index: usize) -> Option<&Paragraph> {
        self.paragraphs.get(index)
    }

    /// The paragraphs, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Paragraph> {
        self.paragraphs.iter()
    }

    /// The index of a paragraph.
    pub fn index_of(&self, id: NodeId) -> Option<usize> {
        // A linear search is enough for now; an index from identifiers to
        // positions can come with long documents.
        self.paragraphs.iter().position(|p| p.id == id)
    }

    /// A paragraph by identifier.
    pub fn paragraph(&self, id: NodeId) -> Option<&Paragraph> {
        self.index_of(id).and_then(|index| self.get(index))
    }

    pub(crate) fn paragraph_mut(&mut self, id: NodeId) -> Result<&mut Paragraph, ModelError> {
        let index = self.index_of(id).ok_or(ModelError::UnknownParagraph(id))?;
        Ok(&mut self.paragraphs[index])
    }

    pub(crate) fn insert(&mut self, index: usize, paragraph: Paragraph) -> Result<(), ModelError> {
        if index > self.len() {
            return Err(ModelError::IndexOutOfBounds {
                index,
                len: self.len(),
            });
        }
        if self.index_of(paragraph.id).is_some() {
            return Err(ModelError::DuplicateId(paragraph.id));
        }
        self.paragraphs.insert(index, paragraph);
        Ok(())
    }

    pub(crate) fn remove(&mut self, id: NodeId) -> Result<(usize, Paragraph), ModelError> {
        let index = self.index_of(id).ok_or(ModelError::UnknownParagraph(id))?;
        if self.len() == 1 {
            return Err(ModelError::LastParagraph);
        }
        Ok((index, self.paragraphs.remove(index)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> NodeId {
        NodeId::new(n).unwrap()
    }

    fn bold() -> CharAttrs {
        CharAttrs {
            bold: Some(true),
            ..CharAttrs::default()
        }
    }

    #[test]
    fn runs_are_normalized() {
        let paragraph = Paragraph::from_runs(
            id(1),
            id(2),
            ParaAttrs::default(),
            [Run::new("ab"), Run::new(""), Run::new("cd")],
        )
        .unwrap();
        assert_eq!(paragraph.runs().count(), 1);
        assert_eq!(paragraph.text(), "abcd");
        assert!(paragraph.is_normalized());
    }

    #[test]
    fn replace_returns_what_it_removed() {
        let mut paragraph = Paragraph::from_runs(
            id(1),
            id(2),
            ParaAttrs::default(),
            [Run::new("hello "), Run::with_attrs("world", None, bold())],
        )
        .unwrap();
        let removed = paragraph.replace(4..8, vec![Run::new("--")]).unwrap();
        assert_eq!(paragraph.text(), "hell--rld");
        assert_eq!(
            removed,
            vec![Run::new("o "), Run::with_attrs("wo", None, bold())]
        );
        assert!(paragraph.is_normalized());
    }

    #[test]
    fn ranges_must_fall_between_characters() {
        let mut paragraph =
            Paragraph::from_runs(id(1), id(2), ParaAttrs::default(), [Run::new("é")]).unwrap();
        assert!(paragraph.replace(1..2, vec![]).is_err());
        assert!(paragraph.replace(0..3, vec![]).is_err());
        assert!(paragraph.replace(0..2, vec![]).is_ok());
    }

    #[test]
    fn runs_cannot_hold_paragraph_separators() {
        let mut paragraph = Paragraph::new(id(1), id(2));
        assert_eq!(
            paragraph.replace(0..0, vec![Run::new("a\nb")]),
            Err(ModelError::InvalidText)
        );
    }

    #[test]
    fn typed_text_takes_the_format_before_it() {
        let paragraph = Paragraph::from_runs(
            id(1),
            id(2),
            ParaAttrs::default(),
            [Run::with_attrs("ab", None, bold()), Run::new("cd")],
        )
        .unwrap();
        assert_eq!(paragraph.format_at(0).1, bold());
        assert_eq!(paragraph.format_at(2).1, bold());
        assert_eq!(paragraph.format_at(3).1, CharAttrs::default());
    }
}
