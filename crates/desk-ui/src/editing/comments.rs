// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The comments on the text of the body, as the window adds, edits and
//! goes to them (W149, REV-08): a new comment is on the selection, or on
//! the word at the cursor, as in Pages 15.4; a comment emptied is
//! deleted, and a highlight given text becomes a comment (PXV-19).

use std::sync::Arc;

use desk_model::{Comment, Document, NodeId, Position, Reply};

use super::{Selection, Spot, TextEditor, at_spot};
use crate::text_nav::{self, SpanRect};

/// The comments of the body, each with the text it is on as spots.
fn comment_spots(document: &Document) -> Vec<(NodeId, Spot, Spot)> {
    document
        .comment_ranges()
        .into_iter()
        .filter_map(|(id, start, end)| {
            Some((id, Spot::of(document, start)?, Spot::of(document, end)?))
        })
        .collect()
}

/// The text that a comment is on, on one line, as its card quotes it.
pub fn commented_text(document: &Document, start: Position, end: Position) -> String {
    let Ok(ranges) = document.paragraph_ranges(start, end) else {
        return String::new();
    };
    let text: Vec<String> = ranges
        .into_iter()
        .filter_map(|(p, range)| {
            let paragraph = document.paragraph(p).ok()?;
            Some(
                paragraph.text()[range]
                    .chars()
                    .filter(|c| !c.is_control() && !('\u{e000}'..='\u{f8ff}').contains(c))
                    .collect(),
            )
        })
        .collect();
    text.join(" ").trim().to_owned()
}

/// The text of a comment or a reply as typed: without the empty lines at
/// its ends.
fn typed(text: &str) -> Arc<str> {
    text.trim_matches(|c: char| c == '\n' || c == '\r' || c == ' ')
        .into()
}

impl TextEditor {
    /// The text that a new comment would be on: the selection, or the
    /// word at the cursor when nothing is selected, as Pages 15.4 does.
    /// None in headers, footers and notes, whose comments come with step 5
    /// of the phase W2, and where there is no text.
    pub fn comment_range(&self) -> Option<(Spot, Spot)> {
        if self.story().is_some() || self.cell_rects().is_some() {
            return None;
        }
        let (start, end) = self.selection().ordered();
        if start != end {
            return Some((start, end));
        }
        let state = self.state.borrow();
        let document = state.editor.document();
        let at = at_spot(document, start).ok()?;
        let (from, to) = document.word_at(at)?;
        Some((Spot::of(document, from)?, Spot::of(document, to)?))
    }

    /// Puts a comment with its text on the text between two spots, by
    /// `author`, now; nothing when the text is empty, as Pages deletes a
    /// comment left empty. Gives the new comment.
    pub fn add_comment(
        &self,
        (start, end): (Spot, Spot),
        author: &str,
        text: &str,
    ) -> Option<NodeId> {
        let text = typed(text);
        if text.is_empty() {
            return None;
        }
        let comment = Comment {
            id: NodeId::new(1).expect("not zero"),
            author: author.into(),
            date: Some(crate::comments_page::now().into()),
            text,
            replies: Arc::new([]),
        };
        let mut added = None;
        self.format_edit("Add Comment", |edit| {
            let from = at_spot(edit.document(), start)?;
            let to = at_spot(edit.document(), end)?;
            added = Some(edit.add_comment(from, to, comment)?);
            Ok(())
        });
        added
    }

    /// Changes the text of a comment. Emptied, a comment is deleted, as
    /// in Pages 15.4, but a highlight stays one; given text, a highlight
    /// becomes a comment (PXV-19).
    pub fn set_comment_text(&self, id: NodeId, text: &str) {
        let text = typed(text);
        let Some(old) = self.document().comments().get(id).cloned() else {
            return;
        };
        if old.text == text {
            return;
        }
        if text.is_empty() {
            self.delete_comment(id);
            return;
        }
        self.format_edit("Edit Comment", move |edit| {
            edit.set_comment(id, Some(Comment { text, ..old }))
        });
    }

    /// Answers a comment, by `author`, now.
    pub fn add_reply(&self, id: NodeId, author: &str, text: &str) {
        let text = typed(text);
        if text.is_empty() {
            return;
        }
        let reply = Reply {
            author: author.into(),
            date: Some(crate::comments_page::now().into()),
            text,
        };
        self.change_replies("Reply", id, move |replies| replies.push(reply));
    }

    /// Changes the text of a reply, by its place in the thread; emptied,
    /// the reply is deleted.
    pub fn set_reply_text(&self, id: NodeId, index: usize, text: &str) {
        let text = typed(text);
        if text.is_empty() {
            self.delete_reply(id, index);
            return;
        }
        self.change_replies("Edit Reply", id, move |replies| {
            if let Some(reply) = replies.get_mut(index)
                && reply.text != text
            {
                reply.text = text;
            }
        });
    }

    /// Deletes a reply, by its place in the thread.
    pub fn delete_reply(&self, id: NodeId, index: usize) {
        self.change_replies("Delete Reply", id, move |replies| {
            if index < replies.len() {
                replies.remove(index);
            }
        });
    }

    fn change_replies(&self, name: &str, id: NodeId, change: impl FnOnce(&mut Vec<Reply>)) {
        let Some(old) = self.document().comments().get(id).cloned() else {
            return;
        };
        let mut replies = old.replies.to_vec();
        change(&mut replies);
        if *replies == *old.replies {
            return;
        }
        self.format_edit(name, move |edit| {
            edit.set_comment(
                id,
                Some(Comment {
                    replies: replies.into(),
                    ..old
                }),
            )
        });
    }

    /// Deletes a comment, with its replies, and takes it off its text.
    pub fn delete_comment(&self, id: NodeId) {
        self.format_edit("Delete Comment", |edit| edit.remove_comment(id));
    }

    /// Selects the text that a comment is on, and shows it.
    pub fn select_comment(&self, id: NodeId) -> bool {
        let found = comment_spots(&self.document())
            .into_iter()
            .find(|(c, _, _)| *c == id);
        let Some((_, start, end)) = found else {
            return false;
        };
        if self.story().is_some() {
            self.leave_story(None);
        }
        self.move_to(start, false, false);
        self.select(Selection {
            anchor: start,
            focus: end,
        });
        true
    }

    /// Selects the text of the next comment after the selection, or of the
    /// one before it, as Next Comment and Previous Comment do in Pages
    /// 15.4. Gives the comment, none at the last or the first.
    pub fn go_to_comment(&self, forward: bool) -> Option<NodeId> {
        if self.story().is_some() {
            return None;
        }
        let (start, end) = self.selection().ordered();
        let spots = comment_spots(&self.document());
        let found = if forward {
            spots
                .into_iter()
                .filter(|(_, s, e)| *s > start || (*s == start && *e > end))
                .min_by_key(|(_, s, e)| (*s, *e))
        } else {
            spots
                .into_iter()
                .filter(|(_, s, e)| *s < start || (*s == start && *e < end))
                .max_by_key(|(_, s, e)| (*s, *e))
        };
        let (id, _, _) = found?;
        self.select_comment(id).then_some(id)
    }

    /// The comment that the cursor or the selection is in: the one that
    /// starts last of those around it, as it is on the fewest words.
    pub fn comment_at_cursor(&self) -> Option<NodeId> {
        if self.story().is_some() {
            return None;
        }
        let (start, end) = self.selection().ordered();
        comment_spots(&self.document())
            .into_iter()
            .filter(|(_, s, e)| *s <= start && end <= *e)
            .max_by_key(|(_, s, _)| *s)
            .map(|(id, _, _)| id)
    }

    /// Where the text under comments is on the pages, which shows it
    /// marked: that of the body, also while a header, a footer or a note
    /// is being edited.
    pub(super) fn comment_marks(&self) -> Vec<SpanRect> {
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        let layout = Self::main_layout(&state);
        comment_spots(document)
            .into_iter()
            .flat_map(|(_, anchor, focus)| {
                text_nav::selection_rects(layout, &Selection { anchor, focus })
            })
            .collect()
    }
}
