// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transactions, undo and redo.
//!
//! Each user action is one transaction with a name, such as "Typing",
//! made of operations. A transaction is all or nothing: it runs on a
//! snapshot of the document, which replaces the document only when every
//! operation succeeded. Snapshots are cheap, so this costs little.

use crate::document::{Document, Metadata, PageSetup};
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
            let (style, attrs) = paragraph.format_at(position.offset);
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
    /// the attributes, except that only the first half starts a page.
    pub fn split_paragraph(&mut self, at: Position) -> Result<Position, ModelError> {
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
    fn map_runs(
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
        self.map_runs(start, end, |run| run.attrs.merge(attrs))
    }

    /// Removes the character attributes set on the text between two
    /// positions, so that it follows its styles again.
    pub fn clear_character_attrs(
        &mut self,
        start: Position,
        end: Position,
    ) -> Result<(), ModelError> {
        self.map_runs(start, end, |run| run.attrs = CharAttrs::default())
    }

    /// Sets or removes the character style of the text between two
    /// positions.
    pub fn set_character_style(
        &mut self,
        start: Position,
        end: Position,
        style: Option<StyleId>,
    ) -> Result<(), ModelError> {
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
            let paragraph = self.doc.paragraph(id)?;
            if paragraph.style() != style {
                let attrs = paragraph.attrs().clone();
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
        let mut edit = Edit {
            doc: self.clone(),
            ops: Vec::new(),
            inverse: Vec::new(),
        };
        let value = edits(&mut edit)?;
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

/// A document with its undo and redo history.
#[derive(Clone, Debug)]
pub struct Editor {
    document: Document,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
}

impl Editor {
    /// Starts editing a document, with an empty history.
    pub fn new(document: Document) -> Self {
        Self {
            document,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// The document.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// A snapshot of the document, which later edits do not change.
    pub fn snapshot(&self) -> Document {
        self.document.clone()
    }

    /// Ends editing and returns the document.
    pub fn into_document(self) -> Document {
        self.document
    }

    /// Runs a user action as a transaction, which Undo can take back.
    /// An action that changes nothing does not enter the history.
    pub fn edit<T>(
        &mut self,
        name: impl Into<String>,
        edits: impl FnOnce(&mut Edit) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        let (value, transaction) = self.document.transact(name, edits)?;
        if !transaction.is_empty() {
            self.undo.push(transaction);
            self.redo.clear();
        }
        Ok(value)
    }

    /// Undoes the last action. Returns false when there was none.
    pub fn undo(&mut self) -> Result<bool, ModelError> {
        let Some(transaction) = self.undo.pop() else {
            return Ok(false);
        };
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
