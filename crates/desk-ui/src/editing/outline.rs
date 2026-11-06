// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The changes that the outline makes to the document (TXT-14, IOP-26).

use desk_model::{Found, ModelError};

use super::{Selection, Spot, TextEditor};

/// A paragraph of the body as the outline shows it.
#[derive(Clone, Debug)]
pub struct OutlineRow {
    /// The level of its topic, or none for text.
    pub level: Option<u8>,
    /// Its text.
    pub text: String,
    /// Whether topics or text come under it.
    pub has_children: bool,
    /// The objects in its text or that move with it, each alone, to show
    /// in miniature (TXT-14).
    pub objects: Vec<desk_render::DisplayList>,
}

/// The range that differs between two texts, in bytes of the old one,
/// and what takes its place.
fn difference(old: &str, new: &str) -> (std::ops::Range<usize>, String) {
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((i, _), _)| i);
    let prefix = (0..=prefix)
        .rev()
        .find(|&i| old.is_char_boundary(i) && new.is_char_boundary(i))
        .unwrap_or(0);
    let (old_rest, new_rest) = (&old[prefix..], &new[prefix..]);
    let suffix = old_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    (prefix..old_end, new[prefix..new_end].to_owned())
}

impl TextEditor {
    /// The paragraphs of the body, as the outline shows them.
    pub fn outline_rows(&self) -> Vec<OutlineRow> {
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        let count = document.body().len();
        let objects_of = |paragraph: Option<&desk_model::Paragraph>| {
            let Some(paragraph) = paragraph else {
                return Vec::new();
            };
            let mut ids: Vec<desk_model::NodeId> = paragraph
                .runs()
                .filter_map(|run| match &run.attrs.field {
                    Some(Some(desk_model::Field::ObjectMark { object })) => Some(*object),
                    _ => None,
                })
                .collect();
            ids.extend(
                document
                    .objects()
                    .iter()
                    .filter(|o| o.anchor == desk_model::Anchor::Paragraph(paragraph.id()))
                    .map(|o| o.id),
            );
            ids.iter()
                .filter_map(|&id| super::objects::find(&state.layout, id))
                .map(|(_, placed)| desk_render::DisplayList::of_object(placed))
                .collect()
        };
        (0..count)
            .map(|index| OutlineRow {
                objects: objects_of(document.body().get(index)),
                level: document.outline_level(index),
                text: document
                    .body()
                    .get(index)
                    .map(|p| p.text())
                    .unwrap_or_default(),
                has_children: document.topic_range(index).len() > 1,
            })
            .collect()
    }

    /// The paragraphs of the topic of a paragraph, with its subtopics.
    pub fn topic_range(&self, index: usize) -> std::ops::Range<usize> {
        Self::main_document(&self.state.borrow()).topic_range(index)
    }

    /// Changes the outline as one action, and puts the cursor at a spot.
    fn outline_edit(
        &self,
        name: &str,
        change: impl FnOnce(&mut desk_model::Edit) -> Result<Spot, ModelError>,
    ) {
        self.leave_story(None);
        let before = self.selection();
        let result = self
            .state
            .borrow_mut()
            .editor
            .edit(name, |edit| change(edit).map(Some));
        self.finish_edit(before, result);
    }

    /// Sets the text of a paragraph as typed in the outline. The letters
    /// typed one after another in a paragraph are one action.
    pub fn set_paragraph_text(&self, index: usize, text: &str) {
        self.leave_story(None);
        let (found, with) = {
            let state = self.state.borrow();
            let Some(paragraph) = state.editor.document().body().get(index) else {
                return;
            };
            let (range, with) = difference(&paragraph.text(), text);
            if range.is_empty() && with.is_empty() {
                return;
            }
            let found = Found {
                paragraph: paragraph.id(),
                index,
                range,
                story: None,
            };
            (found, with)
        };
        let before = self.selection();
        let continuing = {
            let state = self.state.borrow();
            state.typing && before.is_empty() && before.focus.paragraph == index
        };
        let result = {
            let mut state = self.state.borrow_mut();
            let write = |edit: &mut desk_model::Edit| {
                let after = edit.replace_found(&found, &with)?;
                Ok(Spot::new(index, after.offset))
            };
            if continuing {
                state.editor.edit_continuing("Typing", write)
            } else {
                state.editor.edit("Typing", write).map(|spot| (spot, false))
            }
        };
        match result {
            Ok((spot, joined)) => {
                let after = Selection::cursor(spot);
                if !joined {
                    let mut state = self.state.borrow_mut();
                    let action = state.next_action;
                    state.next_action += 1;
                    state.undo.push((before, after, action));
                    state.redo.clear();
                }
                self.after_change(after);
                self.state.borrow_mut().typing = true;
            }
            Err(error) => gtk::glib::g_warning!("desk", "cannot edit: {error}"),
        }
    }

    /// Splits a paragraph at an offset, as Return in the outline: the new
    /// paragraph is a topic of the same level.
    pub fn outline_split(&self, index: usize, offset: usize) {
        self.outline_edit("Typing", |edit| {
            let level = edit.document().outline_level(index);
            let id = edit
                .document()
                .body()
                .get(index)
                .map(desk_model::Paragraph::id)
                .ok_or(ModelError::IndexOutOfBounds { index, len: 0 })?;
            edit.split_paragraph(desk_model::Position::new(id, offset))?;
            edit.set_outline_level(index + 1, level)?;
            Ok(Spot::new(index + 1, 0))
        });
    }

    /// Joins a paragraph to the one before, as Backspace at its start.
    pub fn outline_join_previous(&self, index: usize) {
        if index == 0 {
            return;
        }
        self.outline_edit("Typing", |edit| {
            let previous = edit
                .document()
                .body()
                .get(index - 1)
                .cloned()
                .ok_or(ModelError::IndexOutOfBounds { index, len: 0 })?;
            edit.join_with_next(previous.id())?;
            Ok(Spot::new(index - 1, previous.len()))
        });
    }

    /// Makes a paragraph a topic of another level (Promote and Demote),
    /// with its subtopics, or text with none.
    pub fn set_outline_level(&self, index: usize, level: Option<u8>) {
        self.outline_edit("Outline Level", |edit| {
            let old = edit.document().outline_level(index);
            let range = edit.document().topic_range(index);
            edit.set_outline_level(index, level)?;
            // The subtopics move by as many levels.
            if let (Some(old), Some(new)) = (old, level) {
                let by = i16::from(new) - i16::from(old);
                for i in range.start + 1..range.end {
                    if let Some(l) = edit.document().outline_level(i) {
                        let moved =
                            (i16::from(l) + by).clamp(1, i16::from(desk_model::MAX_OUTLINE_LEVEL));
                        edit.set_outline_level(i, u8::try_from(moved).ok())?;
                    }
                }
            }
            Ok(Spot::new(index, 0))
        });
    }

    /// Moves a topic, with its subtopics, before paragraph `before`.
    pub fn move_topic(&self, index: usize, before: usize) {
        let range = self.topic_range(index);
        if (range.start..=range.end).contains(&before) {
            return;
        }
        self.outline_edit("Move Topic", |edit| {
            let moved = edit.move_paragraphs(range, before)?;
            Ok(Spot::new(moved.start, 0))
        });
    }

    /// A topic with its subtopics as outline text, for Keynote (IOP-26).
    pub fn outline_text(&self, index: usize) -> String {
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        document.outline_text(document.topic_range(index))
    }

    /// Inserts outline text as topics before paragraph `at` (IOP-26).
    pub fn paste_outline(&self, at: usize, text: &str) {
        let text = text.to_owned();
        self.outline_edit("Paste", |edit| {
            let range = edit.insert_outline(at, &text)?;
            Ok(Spot::new(range.end.saturating_sub(1), 0))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_14_typing_changes_only_what_differs() {
        assert_eq!(difference("abc", "abXc"), (2..2, "X".to_owned()));
        assert_eq!(difference("abc", "ac"), (1..2, String::new()));
        assert_eq!(difference("aé", "aéb"), (3..3, "b".to_owned()));
        assert_eq!(difference("", "x"), (0..0, "x".to_owned()));
        assert_eq!(difference("aaa", "aa"), (2..3, String::new()));
    }
}
