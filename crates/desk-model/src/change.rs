// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tracked changes of the text: insertions, deletions and changes of
//! formatting (W148, REV-01).
//!
//! A change is kept with the other changes of the document, and the text
//! it covers carries its identifier ([`CharAttrs::change`]), as comments
//! are kept. Deleted text stays in the document, marked, until the
//! deletion is accepted or rejected. A paragraph break inserted or
//! deleted is marked on the paragraph that ends at it
//! ([`ParaAttrs::break_change`]). Text whose formatting changed carries
//! the formatting it had before ([`CharAttrs::formatted`]), which
//! rejecting the change puts back.
//!
//! [`CharAttrs::change`]: crate::CharAttrs::change
//! [`CharAttrs::formatted`]: crate::CharAttrs::formatted
//! [`ParaAttrs::break_change`]: crate::ParaAttrs::break_change

use std::sync::Arc;

use imbl::OrdMap;

use crate::id::NodeId;
use crate::text::{Paragraph, Position};

/// What a tracked change did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    /// Text was added.
    Insertion,
    /// Text was deleted; it stays, marked, until the deletion is accepted.
    Deletion,
    /// The formatting of text changed: its character style or the
    /// formatting set on it.
    Format,
}

/// The formatting that text had before a change of its formatting: its
/// character style and the formatting set on it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Formatting {
    /// The character style.
    pub style: Option<crate::StyleId>,
    /// The formatting set on the text, without what belongs to the text,
    /// as fields, comments and changes.
    pub attrs: crate::CharAttrs,
}

/// A tracked change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The identifier, which the text it covers carries.
    pub id: NodeId,
    /// What it did.
    pub kind: ChangeKind,
    /// Who made it.
    pub author: Arc<str>,
    /// When it was made, as a date and time of ISO 8601
    /// (`2026-11-24T20:33:34`), if it is known.
    pub date: Option<Arc<str>>,
}

/// The tracked changes of a document, by identifier.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes(OrdMap<NodeId, Change>);

impl Changes {
    /// A change by identifier.
    pub fn get(&self, id: NodeId) -> Option<&Change> {
        self.0.get(&id)
    }

    /// Every change, by identifier.
    pub fn iter(&self) -> impl Iterator<Item = &Change> {
        self.0.values()
    }

    /// How many changes there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is no change.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn replace(&mut self, id: NodeId, change: Option<Change>) -> Option<Change> {
        match change {
            Some(change) => self.0.insert(id, change),
            None => self.0.remove(&id),
        }
    }
}

/// A piece of text that a change covers, in one paragraph: from `start`
/// to `end`, or, when `start` and `end` are both at the end of the
/// paragraph, its break alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeSpan {
    /// The change.
    pub id: NodeId,
    /// Where the text starts.
    pub start: Position,
    /// Where it ends.
    pub end: Position,
    /// Whether the break at the end of the paragraph is part of it.
    pub with_break: bool,
}

/// The pieces of text that changes cover in some paragraphs, in their
/// order: one for each run of text of one change in a paragraph, with
/// the break of the paragraph when the change covers it too.
pub(crate) fn change_spans_in<'a>(
    paragraphs: impl Iterator<Item = &'a Paragraph>,
) -> Vec<ChangeSpan> {
    let mut out: Vec<ChangeSpan> = Vec::new();
    for paragraph in paragraphs {
        let id = paragraph.id();
        let mut offset = 0;
        let first = out.len();
        for run in paragraph.runs() {
            let end = offset + run.len();
            if let Some(Some(change)) = run.attrs.change {
                match out[first..].last_mut() {
                    Some(last) if last.id == change && last.end.offset == offset => {
                        last.end.offset = end;
                    }
                    _ => out.push(ChangeSpan {
                        id: change,
                        start: Position::new(id, offset),
                        end: Position::new(id, end),
                        with_break: false,
                    }),
                }
            }
            offset = end;
        }
        if let Some(Some(change)) = paragraph.attrs().break_change {
            match out[first..].last_mut() {
                Some(last) if last.id == change && last.end.offset == offset => {
                    last.with_break = true;
                }
                _ => out.push(ChangeSpan {
                    id: change,
                    start: Position::new(id, offset),
                    end: Position::new(id, offset),
                    with_break: true,
                }),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn w148_text_and_breaks_carry_their_changes() {
        let mut document = Document::new_word_processing();
        let first = document.body().iter().next().unwrap().id();
        let ((inserted, deleted), _) = document
            .transact("Changes", |edit| {
                let start = edit.document().start();
                let end = edit.insert_text(start, "one two three")?;
                let next = edit.split_paragraph(end)?;
                edit.insert_text(next, "four")?;
                let inserted =
                    edit.add_change(ChangeKind::Insertion, "Ana", Some("2026-11-24T10:00:00"))?;
                let deleted = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
                edit.mark_change(Position::new(first, 4), Position::new(first, 7), inserted)?;
                edit.mark_change(Position::new(first, 8), Position::new(first, 13), deleted)?;
                edit.mark_break_change(first, Some(deleted))?;
                Ok((inserted, deleted))
            })
            .unwrap();
        assert_eq!(document.changes().len(), 2);
        assert_eq!(
            document.changes().get(inserted).unwrap().author.as_ref(),
            "Ana"
        );
        assert_eq!(
            document.change_spans(),
            [
                ChangeSpan {
                    id: inserted,
                    start: Position::new(first, 4),
                    end: Position::new(first, 7),
                    with_break: false,
                },
                ChangeSpan {
                    id: deleted,
                    start: Position::new(first, 8),
                    end: Position::new(first, 13),
                    with_break: true,
                },
            ]
        );
        // Typing in a change does not join it: changes are made by the
        // tracking, not by the formatting of the text before.
        let (_, attrs) = document.body().iter().next().unwrap().format_at(6);
        assert_eq!(attrs.change, None);
    }

    #[test]
    fn w148_a_break_keeps_its_change_when_paragraphs_split_and_join() {
        let mut document = Document::new_word_processing();
        let first = document.body().iter().next().unwrap().id();
        let (gone, _) = document
            .transact("Changes", |edit| {
                let start = edit.document().start();
                edit.insert_text(start, "one two")?;
                let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
                edit.mark_break_change(first, Some(gone))?;
                Ok(gone)
            })
            .unwrap();
        // Return in the middle: the new break is in no change, and the
        // one that ended the paragraph ends the second half.
        let (second, _) = document
            .transact("Typing", |edit| {
                edit.split_paragraph(Position::new(first, 3))
            })
            .unwrap();
        let breaks = |d: &Document| -> Vec<Option<crate::NodeId>> {
            d.body()
                .iter()
                .map(|p| p.attrs().break_change.flatten())
                .collect()
        };
        assert_eq!(breaks(&document), [None, Some(gone)]);
        // Joined again, the paragraph ends at that break.
        document
            .transact("Delete", |edit| edit.join_with_next(first))
            .unwrap();
        assert_eq!(breaks(&document), [Some(gone)]);
        let _ = second;
        // A copy of text in a change is in none.
        document
            .transact("Change", |edit| {
                edit.mark_change(Position::new(first, 0), Position::new(first, 3), gone)
            })
            .unwrap();
        let fragment = document
            .copy_fragment(Position::new(first, 0), Position::new(first, 3))
            .unwrap();
        assert!(
            fragment
                .paragraphs
                .iter()
                .all(|p| p.attrs.break_change.is_none()
                    && p.runs.iter().all(|(_, _, a)| a.change.is_none()))
        );
    }

    #[test]
    fn w148_formatted_text_keeps_the_formatting_it_had() {
        use crate::CharAttrs;
        let mut document = Document::new_word_processing();
        let first = document.body().iter().next().unwrap().id();
        let italic = CharAttrs {
            italic: Some(true),
            ..CharAttrs::default()
        };
        let (change, _) = document
            .transact("Format", |edit| {
                let start = edit.document().start();
                edit.insert_text(start, "old new")?;
                let added = edit.add_change(ChangeKind::Insertion, "Ana", None)?;
                edit.mark_change(Position::new(first, 4), Position::new(first, 7), added)?;
                let change = edit.add_change(ChangeKind::Format, "Bo", None)?;
                edit.mark_format_change(Position::new(first, 0), Position::new(first, 7), change)?;
                edit.set_character_attrs(
                    Position::new(first, 0),
                    Position::new(first, 7),
                    &italic,
                )?;
                // Formatted again, it keeps what it had first.
                let again = edit.add_change(ChangeKind::Format, "Cy", None)?;
                edit.mark_format_change(Position::new(first, 0), Position::new(first, 3), again)?;
                Ok(change)
            })
            .unwrap();
        let paragraph = document.body().iter().next().unwrap().clone();
        let runs: Vec<_> = paragraph.runs().collect();
        // The old text is in the change, with no formatting before; the
        // text added stays in its insertion.
        assert_eq!(runs[0].text(), "old ");
        assert_eq!(runs[0].attrs.change, Some(Some(change)));
        let before = runs[0].attrs.formatted.clone().flatten().unwrap();
        assert_eq!(before.attrs, CharAttrs::default());
        assert_ne!(runs[1].attrs.change, Some(Some(change)));
        assert_eq!(runs[1].attrs.formatted, None);
        // A copy is in no change.
        let fragment = document
            .copy_fragment(Position::new(first, 0), Position::new(first, 3))
            .unwrap();
        assert!(
            fragment.paragraphs[0]
                .runs
                .iter()
                .all(|(_, _, a)| a.change.is_none() && a.formatted.is_none())
        );
    }
}
