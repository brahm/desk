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
    /// A floating object was moved or resized: it keeps where it was
    /// ([`crate::Object::moved`]) until the change is accepted or
    /// rejected, as Pages '09 tracked it (D-003).
    Placement,
    /// What a cell of a table holds or how it looks changed: it keeps
    /// what it was ([`crate::table::Cell::tracked`]) until the change is
    /// accepted or rejected, as Pages '09 tracked it (REV-05, D-003).
    Cell,
}

/// The style and the formatting that a paragraph had before a change of
/// its formatting.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ParaFormatting {
    /// The paragraph style.
    pub style: crate::StyleId,
    /// The formatting set on the paragraph, without its structure, as
    /// its breaks and changes.
    pub attrs: crate::ParaAttrs,
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
        // A change of the formatting of the paragraph covers its text,
        // first, as it starts where the paragraph does.
        if let Some(Some(change)) = paragraph.attrs().format_change {
            out.push(ChangeSpan {
                id: change,
                start: Position::new(id, 0),
                end: Position::new(id, paragraph.len()),
                with_break: false,
            });
        }
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

impl crate::Edit {
    /// Accepts or rejects the tracked changes for which `which` is true,
    /// in every text of the document (W148, REV-06): accepted, text added
    /// stays, text deleted goes and text formatted keeps its new
    /// formatting; rejected, text added goes, text deleted stays and text
    /// formatted takes back the formatting it had, when it is known. The
    /// changes resolved are no longer kept. Gives how many there were.
    pub fn resolve_changes(
        &mut self,
        accept: bool,
        which: impl Fn(NodeId) -> bool,
    ) -> Result<usize, crate::ModelError> {
        let resolved: Vec<NodeId> = self
            .document()
            .changes()
            .iter()
            .map(|c| c.id)
            .filter(|id| which(*id))
            .collect();
        if resolved.is_empty() {
            return Ok(0);
        }
        let kinds: std::collections::HashMap<NodeId, ChangeKind> = resolved
            .iter()
            .filter_map(|id| self.document().changes().get(*id).map(|c| (*id, c.kind)))
            .collect();
        let resolve = |edit: &mut crate::Edit| resolve_text(edit, accept, &kinds);
        resolve(self)?;
        for story in self.document().stories() {
            let changed = self
                .document()
                .story_paragraphs(story)
                .iter()
                .any(|p| has_change(p, &kinds));
            if changed {
                self.edit_story(story, resolve)?;
            }
        }
        // The objects moved or resized: accepted, they stay where they
        // are; rejected, they go back where they were, unless the
        // paragraph they moved with is gone (REV-01).
        let moved: Vec<crate::Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| o.moved.is_some_and(|m| kinds.contains_key(&m.change)))
            .cloned()
            .collect();
        for object in moved {
            let before = object.moved.expect("it was moved");
            let there = match before.anchor {
                crate::Anchor::Paragraph(id) => self.document().paragraph(id).is_ok(),
                _ => true,
            };
            let mut placed = if accept || !there {
                object
            } else {
                // A table keeps the size of its cells (REV-05).
                let frame = match object.kind {
                    crate::ObjectKind::Table(_) => crate::Frame {
                        width: object.frame.width,
                        height: object.frame.height,
                        ..before.frame
                    },
                    _ => before.frame,
                };
                let mut placed = crate::reframed(&object, frame);
                placed.anchor = before.anchor;
                placed
            };
            placed.moved = None;
            self.untracked(|edit| edit.replace_object(placed))?;
        }
        // The cells of tables changed: accepted, they keep what they hold
        // and how they look; rejected, they take back what they were
        // (REV-05).
        let tables: Vec<crate::Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| matches!(o.kind, crate::ObjectKind::Table(_)))
            .filter(|o| o.changes().iter().any(|c| kinds.contains_key(c)))
            .cloned()
            .collect();
        for mut object in tables {
            let crate::ObjectKind::Table(table) = &mut object.kind else {
                continue;
            };
            let table = std::sync::Arc::make_mut(table);
            for r in 0..table.rows() {
                for c in 0..table.cols() {
                    let cell = table.cell_mut(r, c);
                    let Some(tracked) = cell.tracked.clone() else {
                        continue;
                    };
                    let resolved =
                        NodeId::new(tracked.change.get()).is_some_and(|id| kinds.contains_key(&id));
                    if !resolved {
                        continue;
                    }
                    if !accept {
                        cell.content = tracked.content.clone();
                        cell.format = tracked.format.clone();
                        cell.style = tracked.style.clone();
                    }
                    cell.tracked = None;
                }
            }
            self.untracked(|edit| edit.replace_object(object))?;
        }
        for id in &resolved {
            self.set_change(*id, None)?;
        }
        Ok(resolved.len())
    }
}

/// Whether a paragraph has any of some changes.
fn has_change(
    paragraph: &Paragraph,
    kinds: &std::collections::HashMap<NodeId, ChangeKind>,
) -> bool {
    let attrs = paragraph.attrs();
    [attrs.break_change, attrs.break_former, attrs.format_change]
        .into_iter()
        .any(|c| c.flatten().is_some_and(|c| kinds.contains_key(&c)))
        || paragraph.runs().any(|r| {
            [r.attrs.change, r.attrs.former]
                .into_iter()
                .any(|c| c.flatten().is_some_and(|c| kinds.contains_key(&c)))
        })
}

/// Resolves some changes in the body of an edit: first what stays, then,
/// from the end, what goes.
fn resolve_text(
    edit: &mut crate::Edit,
    accept: bool,
    kinds: &std::collections::HashMap<NodeId, ChangeKind>,
) -> Result<(), crate::ModelError> {
    // Whether what a change covers goes: text added and rejected, or
    // deleted and accepted.
    let goes = |kind: ChangeKind| match kind {
        ChangeKind::Insertion => !accept,
        ChangeKind::Deletion => accept,
        ChangeKind::Format | ChangeKind::Placement | ChangeKind::Cell => false,
    };
    let kind_of = |id: Option<NodeId>| id.and_then(|id| kinds.get(&id).copied());
    let ids: Vec<NodeId> = edit.document().body().iter().map(Paragraph::id).collect();
    // The changes there are, which the text of a deletion rejected goes
    // back to: one that is gone leaves it in none.
    let changes: std::collections::HashSet<NodeId> =
        edit.document().changes().iter().map(|c| c.id).collect();
    let mut gone: Vec<(Position, Position)> = Vec::new();
    for (i, &id) in ids.iter().enumerate() {
        let paragraph = edit.document().paragraph(id)?.clone();
        // The paragraph: its formatting, and its break when it stays.
        let mut attrs = paragraph.attrs().clone();
        let mut style = paragraph.style();
        if let Some(kind) = kind_of(attrs.format_change.flatten()) {
            debug_assert_eq!(kind, ChangeKind::Format);
            if !accept && let Some(Some(before)) = attrs.formatted.clone() {
                let structure = attrs.structure();
                style = before.style;
                attrs = before.attrs.clone();
                attrs.merge(&structure);
            }
            attrs.format_change = None;
            attrs.formatted = None;
        }
        // The break goes after the text before it. Rejected, a deletion
        // gives it back to the insertion it was in before, as it gives
        // text back (B-072).
        let mut joins = None;
        let change = attrs.break_change.flatten();
        let former = attrs.break_former.flatten();
        let (change_kind, former_kind) = (kind_of(change), kind_of(former));
        let break_goes = match change_kind {
            Some(kind) => {
                goes(kind)
                    || (kind == ChangeKind::Deletion && former_kind == Some(ChangeKind::Insertion))
            }
            None => !accept && former_kind == Some(ChangeKind::Insertion),
        };
        if break_goes && i + 1 < ids.len() {
            joins = Some((
                Position::new(id, paragraph.len()),
                Position::new(ids[i + 1], 0),
            ));
        } else {
            match (change_kind, former_kind) {
                // A deletion rejected (B-072); accepted, the break of the
                // last paragraph, which cannot go, is in no change.
                (Some(ChangeKind::Deletion), None) if !accept => {
                    let former = former.filter(|id| changes.contains(id));
                    attrs.break_change = former.map(Some);
                    attrs.break_former = None;
                }
                (Some(_), _) => {
                    attrs.break_change = None;
                    attrs.break_former = None;
                }
                (None, Some(_)) => attrs.break_former = None,
                (None, None) => {}
            }
        }
        if &attrs != paragraph.attrs() || style != paragraph.style() {
            edit.apply(crate::Op::SetParagraphFormat {
                paragraph: id,
                style,
                attrs,
            })?;
        }
        // Its text.
        let mut offset = 0;
        for run in paragraph.runs() {
            let (from, to) = (offset, offset + run.len());
            offset = to;
            if run_goes(run, accept, &kind_of) {
                gone.push((Position::new(id, from), Position::new(id, to)));
            }
        }
        gone.extend(joins);
        let end = Position::new(id, paragraph.len());
        edit.map_runs(Position::new(id, 0), end, |run| {
            if run_goes(run, accept, &kind_of) {
                return;
            }
            let change = run.attrs.change.flatten();
            let former = run.attrs.former.flatten();
            match (kind_of(change), kind_of(former)) {
                // A deletion rejected gives the text back to the change it
                // was in before, unless that one is resolved too (B-070).
                (Some(ChangeKind::Deletion), None) => {
                    let former = former.filter(|id| changes.contains(id));
                    run.attrs.change = former.map(Some);
                    run.attrs.former = None;
                    if former.is_none() {
                        run.attrs.formatted = None;
                    }
                }
                (Some(ChangeKind::Deletion), Some(_)) => {
                    unformat(run, accept);
                    run.attrs.change = None;
                    run.attrs.former = None;
                }
                (Some(_), _) => {
                    unformat(run, accept);
                    run.attrs.change = None;
                }
                // The change that deleted text was in, resolved while
                // the text stays deleted: it keeps the formatting it has
                // then.
                (None, Some(_)) => {
                    unformat(run, accept);
                    run.attrs.former = None;
                }
                (None, None) => {}
            }
        })?;
    }
    // From the end, so that the places before stay where they are.
    edit.untracked(|edit| {
        for (from, to) in gone.into_iter().rev() {
            edit.delete(from, to)?;
        }
        Ok(())
    })
}

/// Whether a run goes when some changes are resolved: text added and
/// rejected, or deleted and accepted, or deleted and rejected back into
/// an insertion rejected too, or deleted still and in an insertion
/// rejected (B-070).
fn run_goes(
    run: &crate::Run,
    accept: bool,
    kind_of: &impl Fn(Option<NodeId>) -> Option<ChangeKind>,
) -> bool {
    let change = kind_of(run.attrs.change.flatten());
    let former = kind_of(run.attrs.former.flatten());
    match change {
        Some(ChangeKind::Insertion) => !accept,
        Some(ChangeKind::Deletion) => accept || former == Some(ChangeKind::Insertion),
        Some(ChangeKind::Format | ChangeKind::Placement | ChangeKind::Cell) => false,
        None => !accept && former == Some(ChangeKind::Insertion),
    }
}

/// The formatting a run had before a change of its formatting, put back
/// when the change is rejected; the change of formatting is gone either
/// way.
fn unformat(run: &mut crate::Run, accept: bool) {
    if !accept && let Some(Some(before)) = run.attrs.formatted.clone() {
        let bound = run.attrs.text_bound();
        run.style = before.style;
        run.attrs = before.attrs.clone();
        run.attrs.merge(&bound);
    }
    run.attrs.formatted = None;
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

    #[test]
    fn w148_a_paragraph_keeps_the_formatting_it_had() {
        let mut document = Document::new_word_processing();
        let first = document.body().iter().next().unwrap().id();
        let style = document.body().iter().next().unwrap().style();
        let (change, _) = document
            .transact("Format", |edit| {
                let start = edit.document().start();
                edit.insert_text(start, "one two")?;
                let change = edit.add_change(ChangeKind::Format, "Ana", None)?;
                edit.mark_paragraph_format_change(first, change)?;
                // Marked again, it keeps the formatting it had first.
                let again = edit.add_change(ChangeKind::Format, "Bo", None)?;
                edit.mark_paragraph_format_change(first, again)?;
                Ok(change)
            })
            .unwrap();
        // Split, both halves are in the change; its text is covered.
        document
            .transact("Typing", |edit| {
                edit.split_paragraph(Position::new(first, 3))
            })
            .unwrap();
        let changes: Vec<_> = document
            .body()
            .iter()
            .map(|p| p.attrs().format_change.flatten())
            .collect();
        assert_eq!(changes, [Some(change), Some(change)]);
        let before = document
            .body()
            .iter()
            .next()
            .unwrap()
            .attrs()
            .formatted
            .clone();
        assert_eq!(before.flatten().unwrap().style, style);
        assert!(document.change_spans().iter().any(|s| s.id == change));
        // A copy is in no change.
        let fragment = document
            .copy_fragment(Position::new(first, 0), Position::new(first, 3))
            .unwrap();
        assert!(fragment.paragraphs[0].attrs.format_change.is_none());
    }
}
