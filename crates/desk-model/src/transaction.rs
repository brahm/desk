// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transactions, undo and redo.
//!
//! Each user action is one transaction with a name, such as "Typing",
//! made of operations. A transaction is all or nothing: it runs on a
//! snapshot of the document, which replaces the document only when every
//! operation succeeded. Snapshots are cheap, so this costs little.

use std::sync::Arc;

use crate::document::{Document, Metadata, PageSetup, TextSettings};
use crate::error::ModelError;
use crate::format::{CharAttrs, ParaAttrs};
use crate::id::NodeId;
use crate::op::Op;
use crate::style::StyleId;
use crate::text::{Paragraph, Position, Run, is_paragraph_separator};

/// A finished transaction: its name, its operations and their inverses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    name: String,
    ops: Vec<Op>,
    inverse: Vec<Op>,
}

impl Transaction {
    /// The name shown in the Undo and Redo menu items.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The operations, in the order they ran.
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// Whether the transaction changed nothing.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// A transaction in progress, with the edits that make up user actions.
///
/// Each edit changes the working copy at once, so later edits see the
/// effect of earlier ones.
pub struct Edit {
    doc: Document,
    ops: Vec<Op>,
    inverse: Vec<Op>,
    /// Who makes the changes and when, while they are tracked (W148).
    tracker: Option<Tracker>,
}

/// The author and date of the changes that an edit tracks, with the
/// change of each kind it made, once it made one (W148).
#[derive(Clone, Debug)]
pub struct Tracker {
    author: Arc<str>,
    date: Option<Arc<str>>,
    insertion: Option<NodeId>,
    deletion: Option<NodeId>,
    format: Option<NodeId>,
}

impl Tracker {
    /// Changes made by an author at a date and time of ISO 8601, if it is
    /// known.
    pub fn new(author: &str, date: Option<&str>) -> Self {
        Self {
            author: author.into(),
            date: date.map(Into::into),
            insertion: None,
            deletion: None,
            format: None,
        }
    }
}

/// The date and time now, in UTC, as ISO 8601 writes it
/// (`2026-11-25T09:30:00Z`).
pub fn now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = (seconds / 86_400) as i64;
    let time = seconds % 86_400;
    // The civil date of a count of days since 1970 (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time / 60 % 60,
        time % 60
    )
}

impl Edit {
    /// The document as the transaction has changed it so far.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Returns a new identifier.
    pub fn new_id(&mut self) -> NodeId {
        self.doc.new_id()
    }

    /// Never hands out an identifier that a copy of the document did.
    pub(crate) fn follow_ids(&mut self, copy: &Document) {
        self.doc.follow_ids(copy);
    }

    /// Whether the edit tracks the changes it makes (W148).
    pub fn tracking(&self) -> bool {
        self.tracker.is_some()
    }

    /// The tracked change of a kind that this edit makes, made the first
    /// time it is needed.
    fn tracked(&mut self, kind: crate::ChangeKind) -> Result<NodeId, ModelError> {
        let tracker = self.tracker.as_ref().expect("the edit tracks changes");
        let made = match kind {
            crate::ChangeKind::Insertion => tracker.insertion,
            crate::ChangeKind::Deletion => tracker.deletion,
            crate::ChangeKind::Format => tracker.format,
            // Each object moved is a change of its own, which is
            // accepted or rejected alone.
            crate::ChangeKind::Placement | crate::ChangeKind::Cell => None,
        };
        if let Some(id) = made {
            return Ok(id);
        }
        let (author, date) = (tracker.author.clone(), tracker.date.clone());
        let id = self.add_change(kind, &author, date.as_deref())?;
        let tracker = self.tracker.as_mut().expect("the edit tracks changes");
        match kind {
            crate::ChangeKind::Insertion => tracker.insertion = Some(id),
            crate::ChangeKind::Deletion => tracker.deletion = Some(id),
            crate::ChangeKind::Format => tracker.format = Some(id),
            crate::ChangeKind::Placement | crate::ChangeKind::Cell => {}
        }
        Ok(id)
    }

    /// The change of a kind by the author of the edit that `id` is, if it
    /// is one.
    fn own(&self, id: Option<NodeId>, kind: crate::ChangeKind) -> Option<NodeId> {
        let tracker = self.tracker.as_ref()?;
        let change = self.doc.changes().get(id?)?;
        // A change of no known author is nobody's own.
        (change.kind == kind && !tracker.author.is_empty() && change.author == tracker.author)
            .then_some(change.id)
    }

    /// The insertion that text typed at a position joins: the one the
    /// text before it is in, when the author is the same, or a new one.
    pub(crate) fn insertion_at(&mut self, at: Position) -> Result<NodeId, ModelError> {
        let paragraph = self.doc.paragraph(at.paragraph)?;
        // The change of the run the character before the position is in.
        let mut offset = 0;
        let mut before = None;
        for run in paragraph.runs() {
            if offset >= at.offset {
                break;
            }
            offset += run.len();
            before = run.attrs.change.flatten();
        }
        match self.own(before, crate::ChangeKind::Insertion) {
            Some(id) => Ok(id),
            None => self.tracked(crate::ChangeKind::Insertion),
        }
    }

    /// Runs some edits with their changes made as they are, even while
    /// the edit tracks changes.
    pub fn untracked<T>(
        &mut self,
        edits: impl FnOnce(&mut Self) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        let tracker = self.tracker.take();
        let result = edits(self);
        self.tracker = tracker;
        result
    }

    /// Applies one operation.
    pub fn apply(&mut self, op: Op) -> Result<(), ModelError> {
        let inverse = self.doc.apply(op.clone())?;
        self.ops.push(op);
        self.inverse.push(inverse);
        Ok(())
    }

    /// Inserts text at a position and returns the position after it.
    ///
    /// The text takes the formatting of the character before the
    /// position. Line feeds and other paragraph separators split the
    /// paragraph, as Return does.
    pub fn insert_text(&mut self, at: Position, text: &str) -> Result<Position, ModelError> {
        let text = text.replace("\r\n", "\n");
        let mut position = at;
        for (i, line) in text.split(is_paragraph_separator).enumerate() {
            if i > 0 {
                position = self.split_paragraph(position)?;
            }
            if line.is_empty() {
                continue;
            }
            let paragraph = self.doc.paragraph(position.paragraph)?;
            let (style, mut attrs) = paragraph.format_at(position.offset);
            // Tracked, the text is added in an insertion (W148).
            if self.tracking() {
                attrs.change = Some(Some(self.insertion_at(position)?));
                attrs.formatted = None;
                attrs.former = None;
            } else if let Some(Some(change)) = attrs.change
                && self
                    .doc
                    .changes()
                    .get(change)
                    .is_some_and(|c| c.kind == crate::ChangeKind::Deletion)
            {
                // Text typed next to a deletion is not deleted (REV-04),
                // and is in no change it held.
                attrs.change = None;
                attrs.formatted = None;
                attrs.former = None;
            }
            self.apply(Op::ReplaceText {
                paragraph: position.paragraph,
                range: position.offset..position.offset,
                runs: vec![Run::with_attrs(line, style, attrs)],
            })?;
            position.offset += line.len();
        }
        Ok(position)
    }

    /// Splits a paragraph at a position, as Return does, and returns the
    /// start of the new paragraph.
    ///
    /// At the end of a paragraph, the new paragraph takes the next style
    /// of the paragraph style. Otherwise both halves keep the style and
    /// the attributes, except that only the first half starts a page or
    /// starts its list again.
    pub fn split_paragraph(&mut self, at: Position) -> Result<Position, ModelError> {
        // Tracked, the new break is an insertion (W148).
        let insertion = if self.tracking() {
            Some(self.insertion_at(at)?)
        } else {
            None
        };
        let next = self.split_untracked(at)?;
        if let Some(insertion) = insertion {
            self.mark_break_change(at.paragraph, Some(insertion))?;
        }
        Ok(next)
    }

    fn split_untracked(&mut self, at: Position) -> Result<Position, ModelError> {
        let body = self.doc.body();
        let index = body
            .index_of(at.paragraph)
            .ok_or(ModelError::UnknownParagraph(at.paragraph))?;
        let paragraph = body.get(index).expect("index is in the flow");
        let len = paragraph.len();
        let tail = paragraph.slice(at.offset..len)?;
        let next_style = if at.offset == len {
            self.doc.next_style(paragraph.style())
        } else {
            paragraph.style()
        };
        let mut attrs = if next_style == paragraph.style() {
            paragraph.attrs().clone()
        } else {
            ParaAttrs::default()
        };
        attrs.page_break_before = None;
        attrs.column_break_before = None;
        // A section or layout starts at the first of the two.
        attrs.start = None;
        // The new item counts on from the one it was split from.
        attrs.list_start = None;
        // The break the paragraph ended at now ends the second, and the
        // new one between them is in no tracked change (W148).
        let ends = paragraph.attrs().break_change;
        attrs.break_change = ends;
        attrs.break_former = paragraph.attrs().break_former;
        if ends.flatten().is_some() {
            let mut own = paragraph.attrs().clone();
            own.break_change = None;
            own.break_former = None;
            let style = paragraph.style();
            self.apply(Op::SetParagraphFormat {
                paragraph: at.paragraph,
                style,
                attrs: own,
            })?;
        }
        let id = self.new_id();
        if !tail.is_empty() {
            self.apply(Op::ReplaceText {
                paragraph: at.paragraph,
                range: at.offset..len,
                runs: Vec::new(),
            })?;
        }
        self.apply(Op::InsertParagraph {
            index: index + 1,
            paragraph: Paragraph::from_runs(id, next_style, attrs, tail)?,
        })?;
        Ok(Position::new(id, 0))
    }

    /// Deletes the text between two positions and returns the position
    /// where it was. When the text spans paragraphs, they join into the
    /// first one, which keeps its style.
    pub fn delete(&mut self, start: Position, end: Position) -> Result<Position, ModelError> {
        if self.tracking() {
            return self.delete_tracked(start, end);
        }
        // The notes whose marks go: those with no mark left go too
        // (EST-13).
        let mut marked: Vec<NodeId> = Vec::new();
        // And the objects in the text whose marks go (OBJ-01).
        let mut objects: Vec<NodeId> = Vec::new();
        // And the comments on the text that goes: those on no text left go
        // too (W149).
        let mut comments: Vec<NodeId> = Vec::new();
        for (id, range) in self.doc.paragraph_ranges(start, end)? {
            for run in self.doc.paragraph(id)?.slice(range)? {
                if let Some(ids) = &run.attrs.comments {
                    for &comment in ids.ids() {
                        if !comments.contains(&comment) {
                            comments.push(comment);
                        }
                    }
                }
                match run.attrs.field {
                    Some(Some(crate::Field::NoteMark { note })) if !marked.contains(&note) => {
                        marked.push(note);
                    }
                    Some(Some(crate::Field::ObjectMark { object })) => objects.push(object),
                    _ => {}
                }
            }
        }
        let at = self.delete_text(start, end)?;
        self.remove_unmarked_objects(&objects)?;
        for note in marked {
            if self.doc.notes().get(note).is_some() && !self.doc.has_note_mark(note) {
                self.set_note(note, None)?;
            }
        }
        for comment in comments {
            if self.doc.comments().get(comment).is_some() && !self.doc.has_comment_anchor(comment) {
                self.set_comment(comment, None)?;
            }
        }
        Ok(at)
    }

    /// Deletes the text between two positions as a tracked change
    /// (W148): text the author added in this way goes, text already
    /// deleted stays as it is, and the rest stays, marked as deleted, with
    /// the breaks between the paragraphs.
    /// Deletes the text between two positions, as the Delete key does,
    /// and gives the position after what stays of it: while the edit
    /// tracks changes, the text deleted stays, so the cursor goes past it.
    pub fn delete_forward(
        &mut self,
        start: Position,
        end: Position,
    ) -> Result<Position, ModelError> {
        if self.tracking() {
            Ok(self.delete_tracked_at(start, end)?.1)
        } else {
            self.delete(start, end)
        }
    }

    fn delete_tracked(&mut self, start: Position, end: Position) -> Result<Position, ModelError> {
        Ok(self.delete_tracked_at(start, end)?.0)
    }

    /// Deletes as a tracked change, and gives where the text deleted
    /// starts and ends after it.
    fn delete_tracked_at(
        &mut self,
        start: Position,
        end: Position,
    ) -> Result<(Position, Position), ModelError> {
        use crate::ChangeKind;
        let ranges = self.doc.paragraph_ranges(start, end)?;
        // The pieces that go, in the order of the text: a range of a
        // paragraph, or its break.
        let mut gone: Vec<(Position, Position)> = Vec::new();
        let mut marked: Vec<(Position, Position)> = Vec::new();
        let mut breaks: Vec<(NodeId, Option<NodeId>)> = Vec::new();
        for (i, (id, range)) in ranges.iter().enumerate() {
            let paragraph = self.doc.paragraph(*id)?;
            let mut offset = 0;
            for run in paragraph.runs() {
                let (from, to) = (offset, offset + run.len());
                offset = to;
                let (from, to) = (from.max(range.start), to.min(range.end));
                if from >= to {
                    continue;
                }
                let piece = (Position::new(*id, from), Position::new(*id, to));
                let change = run.attrs.change.flatten();
                if self.own(change, ChangeKind::Insertion).is_some() {
                    gone.push(piece);
                } else if !change
                    .and_then(|c| self.doc.changes().get(c))
                    .is_some_and(|c| c.kind == ChangeKind::Deletion)
                {
                    marked.push(piece);
                }
            }
            if i + 1 < ranges.len() {
                let ends = paragraph.attrs().break_change.flatten();
                let next = ranges[i + 1].0;
                if self.own(ends, ChangeKind::Insertion).is_some() {
                    gone.push((Position::new(*id, paragraph.len()), Position::new(next, 0)));
                } else if !ends
                    .and_then(|c| self.doc.changes().get(c))
                    .is_some_and(|c| c.kind == ChangeKind::Deletion)
                {
                    breaks.push((*id, ends));
                }
            }
        }
        if !marked.is_empty() || !breaks.is_empty() {
            let deletion = self.tracked(ChangeKind::Deletion)?;
            // Text in another change keeps it, to have it back when the
            // deletion is rejected (B-070).
            for (from, to) in marked {
                self.map_runs(from, to, |run| {
                    let before = run.attrs.change.flatten();
                    run.attrs.change = Some(Some(deletion));
                    run.attrs.former = before.map(Some);
                    if before.is_none() {
                        run.attrs.formatted = None;
                    }
                })?;
            }
            // So does a break in an insertion of another author (B-072).
            for (paragraph, before) in breaks {
                self.mark_break_change(paragraph, Some(deletion))?;
                if let Some(before) = before {
                    self.mark_break_former(paragraph, before)?;
                }
            }
        }
        // What the author added goes, from the last piece, so that the
        // places of those before stay where they are.
        let mut after = end;
        let tracker = self.tracker.take();
        let result = gone.into_iter().rev().try_for_each(|(from, to)| {
            self.delete(from, to)?;
            if to.paragraph == after.paragraph {
                after = if from.paragraph == to.paragraph {
                    Position::new(after.paragraph, after.offset - (to.offset - from.offset))
                } else {
                    // The break went: what follows joins the paragraph.
                    Position::new(from.paragraph, from.offset + after.offset)
                };
            }
            Ok(())
        });
        self.tracker = tracker;
        result?;
        Ok((start, after))
    }

    fn delete_text(&mut self, start: Position, end: Position) -> Result<Position, ModelError> {
        let ranges = self.doc.paragraph_ranges(start, end)?;
        let (first, first_range) = ranges[0].clone();
        if ranges.len() == 1 {
            if !first_range.is_empty() {
                self.apply(Op::ReplaceText {
                    paragraph: first,
                    range: first_range,
                    runs: Vec::new(),
                })?;
            }
            return Ok(start);
        }
        let (last, last_range) = ranges[ranges.len() - 1].clone();
        let last_paragraph = self.doc.paragraph(last)?;
        let tail = last_paragraph.slice(last_range.end..last_paragraph.len())?;
        // The joined paragraph ends at the break the last one ended at,
        // and with its tracked change (W148).
        let ends = last_paragraph.attrs().break_change;
        let former = last_paragraph.attrs().break_former;
        let first_paragraph = self.doc.paragraph(first)?;
        if first_paragraph.attrs().break_change != ends
            || first_paragraph.attrs().break_former != former
        {
            let mut attrs = first_paragraph.attrs().clone();
            attrs.break_change = ends;
            attrs.break_former = former;
            let style = first_paragraph.style();
            self.apply(Op::SetParagraphFormat {
                paragraph: first,
                style,
                attrs,
            })?;
        }
        self.apply(Op::ReplaceText {
            paragraph: first,
            range: first_range,
            runs: tail,
        })?;
        for (id, _) in &ranges[1..] {
            self.apply(Op::RemoveParagraph { paragraph: *id })?;
        }
        Ok(start)
    }

    /// Joins a paragraph with the next one, as Delete at its end does.
    pub fn join_with_next(&mut self, paragraph: NodeId) -> Result<(), ModelError> {
        let body = self.doc.body();
        let index = body
            .index_of(paragraph)
            .ok_or(ModelError::UnknownParagraph(paragraph))?;
        let (Some(this), Some(next)) = (body.get(index), body.get(index + 1)) else {
            return Ok(());
        };
        let start = Position::new(this.id(), this.len());
        let end = Position::new(next.id(), 0);
        self.delete(start, end).map(drop)
    }

    /// Changes each run between two positions.
    pub(crate) fn map_runs(
        &mut self,
        start: Position,
        end: Position,
        change: impl Fn(&mut Run),
    ) -> Result<(), ModelError> {
        for (id, range) in self.doc.paragraph_ranges(start, end)? {
            if range.is_empty() {
                continue;
            }
            let old = self.doc.paragraph(id)?.slice(range.clone())?;
            let mut runs = old.clone();
            runs.iter_mut().for_each(&change);
            if runs != old {
                self.apply(Op::ReplaceText {
                    paragraph: id,
                    range,
                    runs,
                })?;
            }
        }
        Ok(())
    }

    /// Sets character attributes on the text between two positions,
    /// keeping the attributes that `attrs` does not set.
    pub fn set_character_attrs(
        &mut self,
        start: Position,
        end: Position,
        attrs: &CharAttrs,
    ) -> Result<(), ModelError> {
        self.track_format(start, end, |run| run.attrs.merge(attrs))?;
        self.map_runs(start, end, |run| run.attrs.merge(attrs))
    }

    /// Tracked, puts the text between two positions that `change` would
    /// change in a change of its formatting first (W148).
    fn track_format(
        &mut self,
        start: Position,
        end: Position,
        change: impl Fn(&mut Run),
    ) -> Result<(), ModelError> {
        if !self.tracking() {
            return Ok(());
        }
        let mut changed: Vec<(Position, Position)> = Vec::new();
        for (id, range) in self.doc.paragraph_ranges(start, end)? {
            let mut offset = range.start;
            for run in self.doc.paragraph(id)?.slice(range)? {
                let mut new = run.clone();
                change(&mut new);
                // Text in a change already keeps it.
                if new != run && run.attrs.change.flatten().is_none() {
                    changed.push((
                        Position::new(id, offset),
                        Position::new(id, offset + run.len()),
                    ));
                }
                offset += run.len();
            }
        }
        if changed.is_empty() {
            return Ok(());
        }
        let format = self.tracked(crate::ChangeKind::Format)?;
        for (from, to) in changed {
            self.mark_format_change(from, to, format)?;
        }
        Ok(())
    }

    /// Tracked, puts a paragraph in a change of its formatting before its
    /// style or attributes change (W148).
    fn track_paragraph(&mut self, paragraph: NodeId) -> Result<(), ModelError> {
        if !self.tracking()
            || self
                .doc
                .paragraph(paragraph)?
                .attrs()
                .format_change
                .flatten()
                .is_some()
        {
            return Ok(());
        }
        let format = self.tracked(crate::ChangeKind::Format)?;
        self.mark_paragraph_format_change(paragraph, format)
    }

    /// Links the text between two positions to a place, or takes its
    /// link away (EST-25).
    pub fn set_link(
        &mut self,
        start: Position,
        end: Position,
        link: Option<Arc<str>>,
    ) -> Result<(), ModelError> {
        self.map_runs(start, end, |run| run.attrs.link = link.clone().map(Some))
    }

    /// Makes the text between two positions a bookmark of a name, or not a
    /// bookmark (EST-26).
    pub fn set_bookmark(
        &mut self,
        start: Position,
        end: Position,
        name: Option<Arc<str>>,
    ) -> Result<(), ModelError> {
        self.map_runs(start, end, |run| {
            run.attrs.bookmark = name.clone().map(Some)
        })
    }

    /// Shows the file of the document again in its file name fields and,
    /// with `date`, today's date in the dates that update when the
    /// document opens (EST-27): in the body, the headers and footers and
    /// the notes.
    pub fn refresh_fields(
        &mut self,
        file: &str,
        date: Option<crate::header::DateWriter<'_>>,
    ) -> Result<(), ModelError> {
        let refresh = |paragraph: &Paragraph| -> Option<Paragraph> {
            let mut changed = false;
            let runs: Vec<Run> = paragraph
                .runs()
                .map(|run| {
                    let mut run = run.clone();
                    if let Some(Some(field)) = &run.attrs.field
                        && let Some(new) = field.refreshed(file, date)
                    {
                        run.attrs.field = Some(Some(new));
                        changed = true;
                    }
                    run
                })
                .collect();
            changed.then(|| {
                Paragraph::from_runs(
                    paragraph.id(),
                    paragraph.style(),
                    paragraph.attrs().clone(),
                    runs,
                )
                .ok()
            })?
        };
        let body: Vec<(NodeId, usize)> = self
            .doc
            .body()
            .iter()
            .filter(|p| refresh(p).is_some())
            .map(|p| (p.id(), p.len()))
            .collect();
        for (id, len) in body {
            self.map_runs(Position::new(id, 0), Position::new(id, len), |run| {
                if let Some(Some(field)) = &run.attrs.field
                    && let Some(new) = field.refreshed(file, date)
                {
                    run.attrs.field = Some(Some(new));
                }
            })?;
        }
        let sections: Vec<Option<NodeId>> = self
            .doc
            .all_headers_footers()
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        for section in sections {
            for place in crate::MarginPlace::ALL {
                let old = self
                    .doc
                    .section_headers_footers(section)
                    .get(place)
                    .to_vec();
                if old.iter().any(|p| refresh(p).is_some()) {
                    let new = old
                        .iter()
                        .map(|p| refresh(p).unwrap_or_else(|| p.clone()))
                        .collect();
                    self.set_section_header_footer(section, place, new)?;
                }
            }
        }
        let notes: Vec<crate::Note> = self.doc.notes().iter().cloned().collect();
        for note in notes {
            if note.paragraphs.iter().any(|p| refresh(p).is_some()) {
                let paragraphs: Vec<Paragraph> = note
                    .paragraphs
                    .iter()
                    .map(|p| refresh(p).unwrap_or_else(|| p.clone()))
                    .collect();
                let id = note.id;
                self.set_note(
                    id,
                    Some(crate::Note {
                        paragraphs: paragraphs.into(),
                        ..note
                    }),
                )?;
            }
        }
        Ok(())
    }

    /// Removes the character attributes set on the text between two
    /// positions, so that it follows its styles again.
    pub fn clear_character_attrs(
        &mut self,
        start: Position,
        end: Position,
    ) -> Result<(), ModelError> {
        let clear = |run: &mut Run| run.attrs = run.attrs.text_bound();
        self.track_format(start, end, clear)?;
        self.map_runs(start, end, clear)
    }

    /// Sets or removes the character style of the text between two
    /// positions.
    pub fn set_character_style(
        &mut self,
        start: Position,
        end: Position,
        style: Option<StyleId>,
    ) -> Result<(), ModelError> {
        self.track_format(start, end, |run| run.style = style)?;
        self.map_runs(start, end, |run| run.style = style)
    }

    /// The paragraphs that the text between two positions touches.
    fn paragraphs_between(
        &self,
        start: Position,
        end: Position,
    ) -> Result<Vec<NodeId>, ModelError> {
        Ok(self
            .doc
            .paragraph_ranges(start, end)?
            .into_iter()
            .map(|(id, _)| id)
            .collect())
    }

    /// Sets paragraph attributes on the paragraphs between two positions,
    /// keeping the attributes that `attrs` does not set.
    pub fn set_paragraph_attrs(
        &mut self,
        start: Position,
        end: Position,
        attrs: &ParaAttrs,
    ) -> Result<(), ModelError> {
        for id in self.paragraphs_between(start, end)? {
            let paragraph = self.doc.paragraph(id)?;
            let mut new = paragraph.attrs().clone();
            new.merge(attrs);
            if &new != paragraph.attrs() {
                let style = paragraph.style();
                if self.tracking() {
                    self.track_paragraph(id)?;
                    let marked = self.doc.paragraph(id)?.attrs().structure();
                    new.merge(&marked);
                }
                self.apply(Op::SetParagraphFormat {
                    paragraph: id,
                    style,
                    attrs: new,
                })?;
            }
        }
        Ok(())
    }

    /// Sets the paragraph style of the paragraphs between two positions.
    /// The attributes set on the paragraphs stay.
    pub fn set_paragraph_style(
        &mut self,
        start: Position,
        end: Position,
        style: StyleId,
    ) -> Result<(), ModelError> {
        for id in self.paragraphs_between(start, end)? {
            if self.doc.paragraph(id)?.style() != style {
                self.track_paragraph(id)?;
                let attrs = self.doc.paragraph(id)?.attrs().clone();
                self.apply(Op::SetParagraphFormat {
                    paragraph: id,
                    style,
                    attrs,
                })?;
            }
        }
        Ok(())
    }

    /// Sets the size and margins of the pages.
    pub fn set_page_setup(&mut self, page: PageSetup) -> Result<(), ModelError> {
        if &page != self.doc.page_setup() {
            self.apply(Op::SetPageSetup { page })?;
        }
        Ok(())
    }

    /// Sets the text of a header or a footer of the first section.
    pub fn set_header_footer(
        &mut self,
        place: crate::MarginPlace,
        paragraphs: Vec<crate::Paragraph>,
    ) -> Result<(), ModelError> {
        self.set_section_header_footer(None, place, paragraphs)
    }

    /// Sets the text of a header or a footer of a section, by the
    /// paragraph that starts it, or of the first section with none.
    pub fn set_section_header_footer(
        &mut self,
        section: Option<NodeId>,
        place: crate::MarginPlace,
        paragraphs: Vec<crate::Paragraph>,
    ) -> Result<(), ModelError> {
        if self.doc.section_headers_footers(section).get(place) != paragraphs.as_slice() {
            self.apply(Op::SetHeaderFooter {
                section,
                place,
                paragraphs,
            })?;
        }
        Ok(())
    }

    /// Captures sections by index under a name, to insert them again
    /// (EST-20): pages of that name already captured give way to them,
    /// where they were.
    pub fn capture_pages(&mut self, name: &str, sections: &[usize]) -> Result<(), ModelError> {
        let pages = self.doc.copy_sections(sections);
        if pages.is_empty() {
            return Ok(());
        }
        let mut captures = self.doc.captured_pages().clone();
        let captured = crate::CapturedPages {
            name: name.into(),
            pages,
        };
        match captures.iter().position(|c| &*c.name == name) {
            Some(at) => {
                captures.set(at, captured);
            }
            None => captures.push_back(captured),
        }
        self.apply(Op::SetCapturedPages { captures })
    }

    /// Takes out the pages captured under a name (EST-20).
    pub fn remove_captured_pages(&mut self, name: &str) -> Result<(), ModelError> {
        let mut captures = self.doc.captured_pages().clone();
        let before = captures.len();
        captures.retain(|c| &*c.name != name);
        if captures.len() != before {
            self.apply(Op::SetCapturedPages { captures })?;
        }
        Ok(())
    }

    /// Inserts the pages captured under a name before section `before`,
    /// or after the last when it is their count, and gives the indices of
    /// their sections (EST-20).
    pub fn insert_captured_pages(
        &mut self,
        name: &str,
        before: usize,
    ) -> Result<std::ops::Range<usize>, ModelError> {
        let Some(captured) = self
            .doc
            .captured_pages()
            .iter()
            .find(|c| &*c.name == name)
            .cloned()
        else {
            return Ok(before..before);
        };
        self.paste_sections(before, &captured.pages)
    }

    /// Sets or removes the background of the pages of a section, by the
    /// paragraph that starts it, or of the first section with none
    /// (W098).
    pub fn set_section_background(
        &mut self,
        section: Option<NodeId>,
        fill: Option<crate::Fill>,
    ) -> Result<(), ModelError> {
        if self.doc.section_background(section) != fill.as_ref() {
            self.apply(Op::SetSectionBackground { section, fill })?;
        }
        Ok(())
    }

    /// Adds, replaces or removes a note.
    pub fn set_note(
        &mut self,
        id: crate::NodeId,
        note: Option<crate::Note>,
    ) -> Result<(), ModelError> {
        if self.doc.notes().get(id) != note.as_ref() {
            self.apply(Op::SetNote { id, note })?;
        }
        Ok(())
    }

    /// Adds, replaces or removes a comment, leaving the text it is on as
    /// it is (W149).
    pub fn set_comment(
        &mut self,
        id: crate::NodeId,
        comment: Option<crate::Comment>,
    ) -> Result<(), ModelError> {
        if self.doc.comments().get(id) != comment.as_ref() {
            self.apply(Op::SetComment { id, comment })?;
        }
        Ok(())
    }

    /// Adds, replaces or removes a tracked change, leaving the text it
    /// covers as it is (W148).
    pub fn set_change(
        &mut self,
        id: NodeId,
        change: Option<crate::Change>,
    ) -> Result<(), ModelError> {
        if self.doc.changes().get(id) != change.as_ref() {
            self.apply(Op::SetChange { id, change })?;
        }
        Ok(())
    }

    /// Adds a new tracked change and returns its identifier, which the
    /// text it covers is then given (W148).
    pub fn add_change(
        &mut self,
        kind: crate::ChangeKind,
        author: &str,
        date: Option<&str>,
    ) -> Result<NodeId, ModelError> {
        let id = self.new_id();
        self.set_change(
            id,
            Some(crate::Change {
                id,
                kind,
                author: author.into(),
                date: date.map(Into::into),
            }),
        )?;
        Ok(id)
    }

    /// Puts the text between two positions in a tracked change (W148).
    pub fn mark_change(
        &mut self,
        start: Position,
        end: Position,
        change: NodeId,
    ) -> Result<(), ModelError> {
        self.map_runs(start, end, |run| {
            run.attrs.change = Some(Some(change));
            // Added or deleted, it has no change of formatting.
            run.attrs.formatted = None;
            run.attrs.former = None;
        })
    }

    /// Puts text that a tracked deletion holds in the change it was in
    /// before the deletion took it, an insertion or a change of its
    /// formatting with the formatting before, as a file gives it (W148,
    /// B-070).
    pub fn mark_former(
        &mut self,
        start: Position,
        end: Position,
        former: NodeId,
        formatted: Option<Arc<crate::Formatting>>,
    ) -> Result<(), ModelError> {
        let format = self
            .doc
            .changes()
            .get(former)
            .is_some_and(|c| c.kind == crate::ChangeKind::Format);
        self.map_runs(start, end, |run| {
            run.attrs.former = Some(Some(former));
            if format {
                run.attrs.formatted = Some(formatted.clone());
            }
        })
    }

    /// Puts the text between two positions in a tracked change of its
    /// formatting, keeping in each run the formatting it has now, which
    /// rejecting the change puts back (W148). Text already in a change,
    /// added, deleted or formatted before, keeps that change: added text
    /// has no formatting of its own to go back to, and formatted text
    /// keeps the formatting it had first.
    pub fn mark_format_change(
        &mut self,
        start: Position,
        end: Position,
        change: NodeId,
    ) -> Result<(), ModelError> {
        self.map_runs(start, end, |run| {
            if run.attrs.change.flatten().is_some() {
                return;
            }
            let before = crate::Formatting {
                style: run.style,
                attrs: run.attrs.without_field(),
            };
            run.attrs.change = Some(Some(change));
            run.attrs.formatted = Some(Some(Arc::new(before)));
        })
    }

    /// Puts the text between two positions in a tracked change of its
    /// formatting whose formatting before is `before` for all of it, or
    /// unknown, as a file gives it (W148). Text already in a change keeps
    /// it, as with [`Edit::mark_format_change`].
    pub fn mark_formatted(
        &mut self,
        start: Position,
        end: Position,
        change: NodeId,
        before: Option<Arc<crate::Formatting>>,
    ) -> Result<(), ModelError> {
        self.map_runs(start, end, |run| {
            if run.attrs.change.flatten().is_some() {
                return;
            }
            run.attrs.change = Some(Some(change));
            run.attrs.formatted = Some(before.clone());
        })
    }

    /// Puts a paragraph in a tracked change of its formatting, keeping
    /// the style and the formatting it has now, which rejecting the
    /// change puts back (W148). A paragraph already in a change of its
    /// formatting keeps it, with the formatting it had first.
    pub fn mark_paragraph_format_change(
        &mut self,
        paragraph: NodeId,
        change: NodeId,
    ) -> Result<(), ModelError> {
        let p = self.doc.paragraph(paragraph)?;
        if p.attrs().format_change.flatten().is_some() {
            return Ok(());
        }
        let before = crate::ParaFormatting {
            style: p.style(),
            attrs: p.attrs().without_structure(),
        };
        self.mark_paragraph_formatted(paragraph, change, Some(Arc::new(before)))
    }

    /// Puts a paragraph in a tracked change of its formatting whose
    /// formatting before is `before`, or unknown, as a file gives it
    /// (W148).
    pub fn mark_paragraph_formatted(
        &mut self,
        paragraph: NodeId,
        change: NodeId,
        before: Option<Arc<crate::ParaFormatting>>,
    ) -> Result<(), ModelError> {
        let p = self.doc.paragraph(paragraph)?;
        let mut attrs = p.attrs().clone();
        attrs.format_change = Some(Some(change));
        attrs.formatted = Some(before);
        let style = p.style();
        self.apply(Op::SetParagraphFormat {
            paragraph,
            style,
            attrs,
        })
    }

    /// Puts the break at the end of a paragraph in a tracked change, or
    /// takes it out of any with none (W148). The break is then in no
    /// change it was in before.
    pub fn mark_break_change(
        &mut self,
        paragraph: NodeId,
        change: Option<NodeId>,
    ) -> Result<(), ModelError> {
        let p = self.doc.paragraph(paragraph)?;
        let mut attrs = p.attrs().clone();
        attrs.break_change = change.map(Some);
        attrs.break_former = None;
        let style = p.style();
        self.apply(Op::SetParagraphFormat {
            paragraph,
            style,
            attrs,
        })
    }

    /// Puts the break at the end of a paragraph, which a tracked deletion
    /// holds, in the insertion it was part of before the deletion took
    /// it, as a file gives it (W148, B-072).
    pub fn mark_break_former(
        &mut self,
        paragraph: NodeId,
        former: NodeId,
    ) -> Result<(), ModelError> {
        let p = self.doc.paragraph(paragraph)?;
        let mut attrs = p.attrs().clone();
        attrs.break_former = Some(Some(former));
        let style = p.style();
        self.apply(Op::SetParagraphFormat {
            paragraph,
            style,
            attrs,
        })
    }

    /// Puts a new comment on the text between two positions, which must
    /// not be empty, and returns its identifier (W149). The identifier of
    /// `comment` is replaced with a new one.
    pub fn add_comment(
        &mut self,
        start: Position,
        end: Position,
        mut comment: crate::Comment,
    ) -> Result<NodeId, ModelError> {
        if start == end {
            return Err(ModelError::EmptyRange);
        }
        let id = self.new_id();
        comment.id = id;
        self.set_comment(id, Some(comment))?;
        self.map_runs(start, end, |run| {
            let ids = run.attrs.comments.clone().unwrap_or_default();
            run.attrs.comments = Some(ids.with(id));
        })?;
        Ok(id)
    }

    /// Puts a new comment on an object, as Pages 15.4 comments a selected
    /// object, and returns its identifier (W149). The identifier of
    /// `comment` is replaced with a new one. An object in a group is found
    /// in it.
    pub fn add_object_comment(
        &mut self,
        object: NodeId,
        mut comment: crate::Comment,
    ) -> Result<NodeId, ModelError> {
        let id = self.new_id();
        comment.id = id;
        self.set_comment(id, Some(comment))?;
        self.change_object_comments(object, |ids| ids.with(id))?;
        Ok(id)
    }

    /// Puts a new comment on a cell of a table, as Pages 15.4 comments a
    /// selected cell, and returns its identifier (W149, TAB-27). The
    /// identifier of `comment` is replaced with a new one. A cell has one
    /// comment: a second is refused with the first, whose thread takes the
    /// replies.
    pub fn add_cell_comment(
        &mut self,
        table: NodeId,
        row: u32,
        col: u32,
        mut comment: crate::Comment,
    ) -> Result<NodeId, ModelError> {
        if let Some(c) = self
            .doc
            .cell_comments()
            .into_iter()
            .find(|c| c.table == table && (c.row, c.col) == (row, col))
        {
            return Err(ModelError::CommentedCell(c.id));
        }
        let id = self.new_id();
        comment.id = id;
        self.set_comment(id, Some(comment))?;
        self.change_cell_comment(table, row, col, Some(id))?;
        Ok(id)
    }

    /// Sets the comment of a cell of a table, at the top or in a group.
    fn change_cell_comment(
        &mut self,
        table: NodeId,
        row: u32,
        col: u32,
        comment: Option<NodeId>,
    ) -> Result<(), ModelError> {
        self.change_object(table, |object| {
            let crate::ObjectKind::Table(t) = &mut object.kind else {
                return Err(ModelError::NotATable(table));
            };
            if row >= t.rows() || col >= t.cols() {
                return Err(ModelError::IndexOutOfBounds {
                    index: (if row >= t.rows() { row } else { col }) as usize,
                    len: (if row >= t.rows() { t.rows() } else { t.cols() }) as usize,
                });
            }
            std::sync::Arc::make_mut(t).cell_mut(row, col).comment =
                comment.and_then(|id| std::num::NonZeroU64::new(id.get()));
            Ok(())
        })
    }

    /// Changes the comments of an object, at the top or in a group.
    fn change_object_comments(
        &mut self,
        object: NodeId,
        change: impl Fn(&crate::CommentIds) -> crate::CommentIds,
    ) -> Result<(), ModelError> {
        self.change_object(object, |o| {
            o.comments = change(&o.comments);
            Ok(())
        })
    }

    /// Changes an object, at the top or in a group.
    fn change_object(
        &mut self,
        object: NodeId,
        change: impl FnOnce(&mut crate::Object) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        fn find(o: &mut crate::Object, id: NodeId) -> Option<&mut crate::Object> {
            if o.id == id {
                return Some(o);
            }
            match &mut o.kind {
                crate::ObjectKind::Group(members) => std::sync::Arc::make_mut(members)
                    .iter_mut()
                    .find_map(|m| find(m, id)),
                _ => None,
            }
        }
        let top = self
            .doc
            .objects()
            .iter()
            .find(|o| o.ids().contains(&object))
            .cloned()
            .ok_or(ModelError::UnknownObject(object))?;
        let mut changed = top;
        let target = find(&mut changed, object).ok_or(ModelError::UnknownObject(object))?;
        change(target)?;
        self.replace_object(changed)
    }

    /// Removes a comment and takes it off the text, the object or the
    /// cell it is on, in the body or in a story (W149).
    pub fn remove_comment(&mut self, id: NodeId) -> Result<(), ModelError> {
        if let Some(c) = self.doc.cell_comments().into_iter().find(|c| c.id == id) {
            self.change_cell_comment(c.table, c.row, c.col, None)?;
            return self.set_comment(id, None);
        }
        if let Some((_, object)) = self
            .doc
            .object_comments()
            .into_iter()
            .find(|(c, _)| *c == id)
        {
            self.change_object_comments(object, |ids| ids.without(id))?;
            return self.set_comment(id, None);
        }
        let story = self
            .doc
            .comment_places()
            .into_iter()
            .find(|p| p.id == id)
            .and_then(|p| p.story);
        if let Some(story) = story {
            return self.edit_story(story, |edit| edit.remove_comment(id));
        }
        if let Some((_, start, end)) = self
            .doc
            .comment_ranges()
            .into_iter()
            .find(|(c, _, _)| *c == id)
        {
            self.map_runs(start, end, |run| {
                if let Some(ids) = &run.attrs.comments {
                    let left = ids.without(id);
                    run.attrs.comments = (!left.is_empty()).then_some(left);
                }
            })?;
        }
        self.set_comment(id, None)
    }

    /// Puts an object at a place from the back, among the objects of the
    /// document (OBJ-01).
    pub fn insert_object(&mut self, index: usize, object: crate::Object) -> Result<(), ModelError> {
        self.apply(Op::InsertObject { index, object })
    }

    /// Puts an object in front of the others.
    pub fn add_object(&mut self, object: crate::Object) -> Result<(), ModelError> {
        let index = self.doc.objects().len();
        self.insert_object(index, object)
    }

    /// Removes an object, with the tracked changes that only it was in
    /// (W148).
    pub fn remove_object(&mut self, id: crate::NodeId) -> Result<(), ModelError> {
        let changes = self
            .doc
            .objects()
            .get(id)
            .map(crate::Object::changes)
            .unwrap_or_default();
        self.apply(Op::RemoveObject { object: id })?;
        self.drop_unused(&changes)
    }

    /// Replaces an object with another with the same identifier. While
    /// the edit tracks changes, a floating object moved or resized keeps
    /// where it was until the change is accepted or rejected (REV-01),
    /// and so does a cell of a table whose content, format or look
    /// changes keep what it was (REV-05), each in a change of its own;
    /// one already in such a change stays in it, and one put back as it
    /// was is in none (W148).
    pub fn replace_object(&mut self, mut object: crate::Object) -> Result<(), ModelError> {
        let floating = |anchor: crate::Anchor| {
            matches!(anchor, crate::Anchor::Page(_) | crate::Anchor::Paragraph(_))
        };
        let old = self.doc.objects().get(object.id).cloned();
        // The size of a table follows its rows, columns and cells, which
        // are not moved: only where it is (REV-05).
        let table = matches!(object.kind, crate::ObjectKind::Table(_));
        let (width, height) = (object.frame.width, object.frame.height);
        let place = |frame: crate::Frame| {
            if table {
                crate::Frame {
                    width,
                    height,
                    ..frame
                }
            } else {
                frame
            }
        };
        if self.tracking()
            && let Some(old) = &old
        {
            if object.moved == old.moved && floating(old.anchor) && floating(object.anchor) {
                match old.moved {
                    Some(m) if m.anchor == object.anchor && place(m.frame) == object.frame => {
                        object.moved = None;
                    }
                    Some(_) => {}
                    None if (old.anchor, place(old.frame)) != (object.anchor, object.frame) => {
                        let change = self.tracked(crate::ChangeKind::Placement)?;
                        object.moved = Some(crate::Moved {
                            change,
                            anchor: old.anchor,
                            frame: old.frame,
                        });
                    }
                    None => {}
                }
            }
            if let (crate::ObjectKind::Table(before), crate::ObjectKind::Table(after)) =
                (&old.kind, &mut object.kind)
            {
                self.track_cells(before, after)?;
            }
        }
        if old.as_ref() != Some(&object) {
            let kept: std::collections::HashSet<crate::NodeId> =
                object.changes().into_iter().collect();
            let gone: Vec<crate::NodeId> = old
                .as_ref()
                .map(crate::Object::changes)
                .unwrap_or_default()
                .into_iter()
                .filter(|c| !kept.contains(c))
                .collect();
            self.apply(Op::ReplaceObject { object })?;
            self.drop_unused(&gone)?;
        }
        Ok(())
    }

    /// Tracks the cells of a table that an edit changes: what each
    /// held, how its value was written and how it looked, unless the
    /// edit already says what it was. Only an edit that keeps the rows
    /// and columns changes cells (REV-05).
    fn track_cells(
        &mut self,
        before: &crate::Table,
        after: &mut std::sync::Arc<crate::Table>,
    ) -> Result<(), ModelError> {
        if (before.rows(), before.cols()) != (after.rows(), after.cols()) {
            return Ok(());
        }
        for r in 0..before.rows() {
            for c in 0..before.cols() {
                let (old, new) = (before.cell(r, c), after.cell(r, c));
                let same = |content: &crate::table::Content<String>,
                            format: &crate::table::Format,
                            style: &crate::table::CellStyle| {
                    (content, format, style) == (&new.content, &new.format, &new.style)
                };
                if new.tracked != old.tracked || same(&old.content, &old.format, &old.style) {
                    continue;
                }
                // A cell whose change is gone is in none.
                let live = old.tracked.as_ref().filter(|t| {
                    crate::NodeId::new(t.change.get())
                        .is_some_and(|id| self.doc.changes().get(id).is_some())
                });
                let tracked = match live {
                    Some(t) if same(&t.content, &t.format, &t.style) => None,
                    Some(t) => {
                        // Another author changing it again joins the
                        // change, which shows they are several.
                        let tracker = self.tracker.as_ref().expect("the edit tracks changes");
                        let id = crate::NodeId::new(t.change.get()).expect("not zero");
                        let theirs = self
                            .doc
                            .changes()
                            .get(id)
                            .is_some_and(|c| c.author != tracker.author);
                        if t.others || !theirs {
                            continue;
                        }
                        Some(std::sync::Arc::new(crate::table::Tracked {
                            others: true,
                            ..(**t).clone()
                        }))
                    }
                    None => {
                        let change = self.tracked(crate::ChangeKind::Cell)?;
                        Some(std::sync::Arc::new(crate::table::Tracked {
                            change: std::num::NonZeroU64::new(change.get())
                                .expect("identifiers are not zero"),
                            content: old.content.clone(),
                            format: old.format.clone(),
                            style: old.style.clone(),
                            others: false,
                        }))
                    }
                };
                std::sync::Arc::make_mut(after).cell_mut(r, c).tracked = tracked;
            }
        }
        Ok(())
    }

    /// Takes away the changes of objects or cells that nothing is in any
    /// longer (W148).
    pub(crate) fn drop_unused(&mut self, changes: &[crate::NodeId]) -> Result<(), ModelError> {
        let of_objects: Vec<crate::NodeId> = changes
            .iter()
            .copied()
            .filter(|&id| {
                self.doc.changes().get(id).is_some_and(|c| {
                    matches!(
                        c.kind,
                        crate::ChangeKind::Placement | crate::ChangeKind::Cell
                    )
                })
            })
            .collect();
        if of_objects.is_empty() {
            return Ok(());
        }
        let used: std::collections::HashSet<crate::NodeId> = self
            .doc
            .objects()
            .iter()
            .flat_map(crate::Object::changes)
            .collect();
        for change in of_objects {
            if !used.contains(&change) {
                self.set_change(change, None)?;
            }
        }
        Ok(())
    }

    /// Sets the settings of the text of the whole document.
    pub fn set_text_settings(&mut self, settings: TextSettings) -> Result<(), ModelError> {
        if &settings != self.doc.text_settings() {
            self.apply(Op::SetTextSettings { settings })?;
        }
        Ok(())
    }

    /// Sets the information about the document.
    pub fn set_metadata(&mut self, metadata: Metadata) -> Result<(), ModelError> {
        if &metadata != self.doc.metadata() {
            self.apply(Op::SetMetadata { metadata })?;
        }
        Ok(())
    }
}

impl Document {
    /// Runs a transaction. If the closure returns an error, the document
    /// stays as it was.
    pub fn transact<T>(
        &mut self,
        name: impl Into<String>,
        edits: impl FnOnce(&mut Edit) -> Result<T, ModelError>,
    ) -> Result<(T, Transaction), ModelError> {
        self.transact_tracked(name, None, edits)
    }

    /// Runs a transaction whose changes to the text are tracked when
    /// `tracker` says who makes them (W148).
    pub fn transact_tracked<T>(
        &mut self,
        name: impl Into<String>,
        tracker: Option<Tracker>,
        edits: impl FnOnce(&mut Edit) -> Result<T, ModelError>,
    ) -> Result<(T, Transaction), ModelError> {
        let mut edit = Edit {
            doc: self.clone(),
            ops: Vec::new(),
            inverse: Vec::new(),
            tracker,
        };
        let value = edits(&mut edit)?;
        edit.follow_connections()?;
        *self = edit.doc;
        let transaction = Transaction {
            name: name.into(),
            ops: edit.ops,
            inverse: edit.inverse,
        };
        Ok((value, transaction))
    }

    /// Undoes a transaction, which must be the last one applied.
    pub fn revert(&mut self, transaction: &Transaction) -> Result<(), ModelError> {
        let mut doc = self.clone();
        for op in transaction.inverse.iter().rev() {
            doc.apply(op.clone())?;
        }
        *self = doc;
        Ok(())
    }

    /// Applies a transaction again after it was undone.
    pub fn replay(&mut self, transaction: &Transaction) -> Result<(), ModelError> {
        let mut doc = self.clone();
        for op in &transaction.ops {
            doc.apply(op.clone())?;
        }
        *self = doc;
        Ok(())
    }
}

/// The most actions that Undo can take back.
pub const MAX_UNDO: usize = 1000;

/// A document with its undo and redo history.
#[derive(Clone, Debug)]
pub struct Editor {
    document: Document,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    /// The actions that entered the history, which goes on counting when
    /// the oldest leave it.
    recorded: u64,
    /// Whether the next action may join the last one: false after an
    /// Undo or a Redo.
    joinable: bool,
    /// Who makes the changes that tracking keeps, and when (W148).
    signer: Signer,
}

/// Gives the author of a change and its date, when it is made.
#[derive(Clone)]
pub struct Signer(Arc<dyn Fn() -> (String, String) + Send + Sync>);

impl Signer {
    /// Changes signed by what `sign` gives: the author and the date.
    pub fn new(sign: impl Fn() -> (String, String) + Send + Sync + 'static) -> Self {
        Self(Arc::new(sign))
    }
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Signer").field(&(self.0)().0).finish()
    }
}

impl Editor {
    /// Sets the name of the author of the changes the user makes, made
    /// at the time [`now`] gives.
    pub fn set_author(&mut self, author: &str) {
        let author = author.to_owned();
        self.signer = Signer::new(move || (author.clone(), now()));
    }

    /// Sets what gives the author of the changes the user makes, and
    /// their date, when they are made.
    pub fn set_signer(&mut self, signer: Signer) {
        self.signer = signer;
    }

    /// What gives the author of the changes and their date.
    pub fn signer(&self) -> &Signer {
        &self.signer
    }

    /// The author of the changes and the time, while the document tracks
    /// its changes.
    fn tracker(&self) -> Option<Tracker> {
        (self.document.text_settings().tracking == crate::Tracking::On).then(|| {
            let (author, date) = (self.signer.0)();
            Tracker::new(&author, Some(&date))
        })
    }

    /// Starts editing a document, with an empty history.
    pub fn new(document: Document) -> Self {
        Self {
            document,
            undo: Vec::new(),
            redo: Vec::new(),
            recorded: 0,
            joinable: false,
            signer: Signer::new(|| (String::new(), now())),
        }
    }

    /// The document.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Sets how the tracked changes of the document show, outside the
    /// history (W148, REV-04).
    pub fn set_markup(&mut self, markup: crate::Markup) {
        self.document.set_markup(markup);
    }

    /// A snapshot of the document, which later edits do not change.
    pub fn snapshot(&self) -> Document {
        self.document.clone()
    }

    /// Ends editing and returns the document.
    pub fn into_document(self) -> Document {
        self.document
    }

    /// The number of actions that Undo can take back.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// The number of actions that have entered the history since editing
    /// began. An edit entered it when this grows; the depth of the history
    /// stops growing at [`MAX_UNDO`], and does not tell.
    pub fn recorded(&self) -> u64 {
        self.recorded
    }

    /// Runs a user action as a transaction, which Undo can take back.
    /// An action that changes nothing does not enter the history.
    pub fn edit<T>(
        &mut self,
        name: impl Into<String>,
        edits: impl FnOnce(&mut Edit) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        let tracker = self.tracker();
        let (value, transaction) = self.document.transact_tracked(name, tracker, edits)?;
        if !transaction.is_empty() {
            self.push_undo(transaction);
            self.redo.clear();
        }
        Ok(value)
    }

    /// Runs a user action that continues the last one when that has the
    /// same name and nothing was undone since, as each letter typed in a
    /// row joins one Typing action. Undo then takes back all of it.
    /// Returns the value of the edits and whether they joined the last
    /// action.
    pub fn edit_continuing<T>(
        &mut self,
        name: impl Into<String>,
        edits: impl FnOnce(&mut Edit) -> Result<T, ModelError>,
    ) -> Result<(T, bool), ModelError> {
        let tracker = self.tracker();
        let (value, transaction) = self.document.transact_tracked(name, tracker, edits)?;
        if transaction.is_empty() {
            return Ok((value, false));
        }
        // After an Undo or a Redo, the action is a new one, not the end of
        // the one before.
        let joinable = self.joinable;
        self.redo.clear();
        match self.undo.last_mut() {
            Some(last) if joinable && last.name == transaction.name => {
                last.ops.extend(transaction.ops);
                last.inverse.extend(transaction.inverse);
                Ok((value, true))
            }
            _ => {
                self.push_undo(transaction);
                Ok((value, false))
            }
        }
    }

    /// Adds an action to the history, which keeps the last
    /// [`MAX_UNDO`]: each keeps what it changed, a whole table for an edit
    /// of its cells, and a long session would hold them all.
    fn push_undo(&mut self, transaction: Transaction) {
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(transaction);
        self.recorded += 1;
        self.joinable = true;
    }

    /// Undoes the last action. Returns false when there was none.
    pub fn undo(&mut self) -> Result<bool, ModelError> {
        let Some(transaction) = self.undo.pop() else {
            return Ok(false);
        };
        // Even when it fails, what comes next is a new action.
        self.joinable = false;
        if let Err(error) = self.document.revert(&transaction) {
            self.undo.push(transaction);
            return Err(error);
        }
        self.redo.push(transaction);
        Ok(true)
    }

    /// Redoes the last undone action. Returns false when there was none.
    pub fn redo(&mut self) -> Result<bool, ModelError> {
        let Some(transaction) = self.redo.pop() else {
            return Ok(false);
        };
        self.joinable = false;
        if let Err(error) = self.document.replay(&transaction) {
            self.redo.push(transaction);
            return Err(error);
        }
        self.undo.push(transaction);
        Ok(true)
    }

    /// The name of the action that Undo would take back.
    pub fn undo_name(&self) -> Option<&str> {
        self.undo.last().map(Transaction::name)
    }

    /// The name of the action that Redo would bring back.
    pub fn redo_name(&self) -> Option<&str> {
        self.redo.last().map(Transaction::name)
    }
}
