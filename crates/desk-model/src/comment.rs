// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Comments on the text, with their replies, and highlights (W149, W059,
//! REV-08, PXV-19).
//!
//! A comment is kept with the other comments of the document, and the
//! text it is on carries its identifier ([`CharAttrs::comments`]), as Pages
//! 15.4 anchors a comment on a range of text. A comment at the cursor is
//! on the word there. Text carries any number of comments, which may
//! overlap. A highlight is a comment without text or replies: Pages 15.4
//! counts it with the comments, and typing in it makes it one (PXV-19).
//!
//! The text of a comment and of its replies is plain, as in Pages 15.4,
//! whose comments take no styles: paragraphs are kept as lines, `\n`
//! between them.
//!
//! [`CharAttrs::comments`]: crate::CharAttrs::comments

use std::sync::Arc;

use imbl::OrdMap;

use crate::id::NodeId;

/// Where a comment is: the text it is on, the body or a story (a note,
/// a header or footer, a text box or a shape), and the positions in it
/// where the first piece of text it is on starts and where its last ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommentPlace {
    /// The comment.
    pub id: NodeId,
    /// The story it is in, none for the body.
    pub story: Option<crate::Story>,
    /// Where its text starts.
    pub start: crate::Position,
    /// Where its text ends.
    pub end: crate::Position,
}

/// A comment on a cell of a table: the comment, the table and the row and
/// column of the cell (W149, TAB-27).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellComment {
    /// The comment.
    pub id: NodeId,
    /// The table.
    pub table: NodeId,
    /// The row of the cell.
    pub row: u32,
    /// The column of the cell.
    pub col: u32,
}

/// A comment, the first of its thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    /// The identifier, which the text it is on carries.
    pub id: NodeId,
    /// Who wrote it.
    pub author: Arc<str>,
    /// When it was written, as a date and time of ISO 8601
    /// (`2026-11-22T20:33:34`), if it is known.
    pub date: Option<Arc<str>>,
    /// The text, empty for a highlight.
    pub text: Arc<str>,
    /// The replies, in the order they were written.
    pub replies: Arc<[Reply]>,
}

impl Comment {
    /// Whether it is a highlight: a comment without text or replies
    /// (PXV-19).
    pub fn is_highlight(&self) -> bool {
        self.text.is_empty() && self.replies.is_empty()
    }
}

/// A reply in the thread of a comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// Who wrote it.
    pub author: Arc<str>,
    /// When it was written, as in [`Comment::date`].
    pub date: Option<Arc<str>>,
    /// The text.
    pub text: Arc<str>,
}

/// The comments of a document, by identifier.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comments(OrdMap<NodeId, Comment>);

impl Comments {
    /// A comment by identifier.
    pub fn get(&self, id: NodeId) -> Option<&Comment> {
        self.0.get(&id)
    }

    /// Every comment, by identifier.
    pub fn iter(&self) -> impl Iterator<Item = &Comment> {
        self.0.values()
    }

    /// How many comments there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is no comment.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn replace(&mut self, id: NodeId, comment: Option<Comment>) -> Option<Comment> {
        match comment {
            Some(comment) => self.0.insert(id, comment),
            None => self.0.remove(&id),
        }
    }
}

/// The comments that a piece of text is under, by identifier, in order
/// and each once.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CommentIds(Arc<[NodeId]>);

impl CommentIds {
    /// Under no comment.
    pub fn none() -> Self {
        Self::default()
    }

    /// Under these comments.
    pub fn of(ids: impl IntoIterator<Item = NodeId>) -> Self {
        let mut ids: Vec<NodeId> = ids.into_iter().collect();
        ids.sort_unstable();
        ids.dedup();
        Self(ids.into())
    }

    /// The comments, in order.
    pub fn ids(&self) -> &[NodeId] {
        &self.0
    }

    /// Whether the text is under no comment.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether the text is under a comment.
    pub fn contains(&self, id: NodeId) -> bool {
        self.0.binary_search(&id).is_ok()
    }

    /// The same, and under a comment too.
    pub fn with(&self, id: NodeId) -> Self {
        Self::of(self.0.iter().copied().chain([id]))
    }

    /// The same, and not under a comment.
    pub fn without(&self, id: NodeId) -> Self {
        Self::of(self.0.iter().copied().filter(|c| *c != id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> NodeId {
        NodeId::new(n).unwrap()
    }

    #[test]
    fn comment_ids_are_a_set_in_order() {
        let ids = CommentIds::of([id(3), id(1), id(3)]);
        assert_eq!(ids.ids(), &[id(1), id(3)]);
        assert!(ids.contains(id(3)));
        assert_eq!(ids.with(id(2)).ids(), &[id(1), id(2), id(3)]);
        assert_eq!(ids.without(id(1)).ids(), &[id(3)]);
        assert!(CommentIds::none().is_empty());
    }

    #[test]
    fn a_highlight_is_a_comment_without_text() {
        let mut comment = Comment {
            id: id(1),
            author: "Ana".into(),
            date: None,
            text: "".into(),
            replies: Arc::new([]),
        };
        assert!(comment.is_highlight());
        comment.text = "Why?".into();
        assert!(!comment.is_highlight());
    }
}
