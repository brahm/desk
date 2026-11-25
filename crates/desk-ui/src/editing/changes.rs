// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tracked changes of the body on the pages (W148, REV-04): text
//! added is underlined, and text deleted struck, in the colour of its
//! author. The views of markup come with step 11 of W2.

use desk_model::ChangeKind;
use gtk::glib;

use super::comments::{Texts, spot_in};
use super::{Selection, Spot, TextEditor};
use crate::text_nav::{self, SpanRect};

impl TextEditor {
    /// Where the text of each change of the body is on the pages, with
    /// what the change did and the colour of its author, for the author of
    /// this computer named `local` (W148, W150).
    pub(super) fn change_marks(
        &self,
        local: &str,
    ) -> Vec<(SpanRect, ChangeKind, desk_model::AuthorColor)> {
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        let places = document.change_places();
        if places.is_empty() {
            return Vec::new();
        }
        // The body and the stories as the pages show them.
        let texts = Texts::with(
            document,
            Self::main_layout(&state),
            places.iter().filter_map(|(story, _)| *story),
        );
        let palette = document.author_palette_of(local, &crate::preferences::get().former_authors);
        let mut out = Vec::new();
        for (story, span) in places {
            let spots = match story {
                None => Spot::of(document, span.start).zip(Spot::of(document, span.end)),
                Some(story) => {
                    let paragraphs = document.story_paragraphs(story);
                    spot_in(&paragraphs, span.start).zip(spot_in(&paragraphs, span.end))
                }
            };
            let (Some(change), Some((anchor, focus)), Some(layout)) =
                (document.changes().get(span.id), spots, texts.layout(story))
            else {
                continue;
            };
            let color = palette.color(&change.author);
            for rect in text_nav::selection_rects(layout, &Selection { anchor, focus }) {
                out.push((rect, change.kind, color));
            }
        }
        out
    }
}

impl TextEditor {
    /// Whether the document tracks its changes (W148, REV-03).
    pub fn tracking(&self) -> desk_model::Tracking {
        let state = self.state.borrow();
        Self::main_document(&state).text_settings().tracking
    }

    /// Turns the tracking of changes on, off or pauses it, as one action
    /// of the history.
    pub fn set_tracking(&self, tracking: desk_model::Tracking) {
        if self.tracking() == tracking {
            return;
        }
        let name = match tracking {
            desk_model::Tracking::On => "Track Changes",
            desk_model::Tracking::Paused => "Pause Tracking",
            desk_model::Tracking::Off => "Turn Off Tracking",
        };
        let mut settings = {
            let state = self.state.borrow();
            Self::main_document(&state).text_settings().clone()
        };
        settings.tracking = tracking;
        self.format_edit(name, |edit| edit.set_text_settings(settings));
    }

    /// Turns the tracking of changes off, after accepting or rejecting
    /// every change pending when `resolve` says which, as one action of
    /// the history (REV-03). Gives whether tracking is off with no change
    /// pending.
    pub fn turn_off_tracking(&self, resolve: Option<bool>) -> bool {
        if self.story().is_some() {
            self.leave_story(None);
        }
        let mut settings = {
            let state = self.state.borrow();
            Self::main_document(&state).text_settings().clone()
        };
        settings.tracking = desk_model::Tracking::Off;
        let done = self.format_edit_quiet("Turn Off Tracking", |edit| {
            if let Some(accept) = resolve {
                edit.resolve_changes(accept, |_| true)?;
            }
            edit.set_text_settings(settings)
        });
        if let Err(error) = &done {
            glib::g_warning!("desk", "cannot turn off tracking: {error}");
        }
        done.is_ok() && self.tracking() == desk_model::Tracking::Off && !self.has_changes()
    }

    /// Whether any text of the document has tracked changes.
    pub fn has_changes(&self) -> bool {
        let state = self.state.borrow();
        !Self::main_document(&state).change_places().is_empty()
    }

    /// Accepts or rejects every tracked change of the document, as one
    /// action of the history (REV-06), and gives how many there were.
    pub fn resolve_all_changes(&self, accept: bool) -> usize {
        // Every text of the document changes: from the body.
        if self.story().is_some() {
            self.leave_story(None);
        }
        let name = if accept {
            "Accept All Changes"
        } else {
            "Reject All Changes"
        };
        let mut count = 0;
        let counted = &mut count;
        self.format_edit(name, |edit| {
            *counted = edit.resolve_changes(accept, |_| true)?;
            Ok(())
        });
        count
    }
}

#[cfg(test)]
mod tests {
    use desk_model::{AuthorColor, ChangeKind, Document, Position};

    use super::*;

    #[test]
    fn w148_changes_are_marked_in_the_colours_of_their_authors() {
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            document
                .transact("Changes", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three")?;
                    let p = start.paragraph;
                    let added = edit.add_change(ChangeKind::Insertion, "Ana", None)?;
                    let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
                    edit.mark_change(Position::new(p, 0), Position::new(p, 3), added)?;
                    edit.mark_change(Position::new(p, 8), Position::new(p, 13), gone)?;
                    Ok(())
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            let marks = editor.change_marks("Ana");
            let kinds: Vec<_> = marks.iter().map(|(_, k, c)| (*k, *c)).collect();
            assert_eq!(
                kinds,
                [
                    (ChangeKind::Insertion, AuthorColor::Yellow),
                    (ChangeKind::Deletion, AuthorColor::Green),
                ]
            );
            // On the text: the deleted word is to the right of the added.
            assert!(marks[1].0.x0 > marks[0].0.x1);
        });
    }

    #[test]
    fn rev_02_03_the_bar_goes_through_comments_and_changes_and_turns_tracking() {
        use super::super::ReviewItem;
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let p = document.start().paragraph;
            let ((comment, added, gone), _) = document
                .transact("Changes", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three")?;
                    let comment = edit.add_comment(
                        Position::new(p, 0),
                        Position::new(p, 3),
                        desk_model::Comment {
                            id: desk_model::NodeId::new(1).unwrap(),
                            author: "Ana".into(),
                            date: None,
                            text: "Why?".into(),
                            replies: std::sync::Arc::new([]),
                        },
                    )?;
                    let added = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
                    let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
                    edit.mark_change(Position::new(p, 4), Position::new(p, 7), added)?;
                    edit.mark_change(Position::new(p, 7), Position::new(p, 13), gone)?;
                    Ok((comment, added, gone))
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            // The cursor at the start is in the comment, the first of
            // three.
            assert_eq!(
                editor.review_position(),
                (Some((1, ReviewItem::Comment(comment))), 3)
            );
            // Next from the cursor selects the comment it is in, then
            // the change after it.
            assert_eq!(
                editor.go_to_review_item(true),
                Some(ReviewItem::Comment(comment))
            );
            assert_eq!(
                editor.go_to_review_item(true),
                Some(ReviewItem::Change(added))
            );
            assert_eq!(
                editor.review_position(),
                (Some((2, ReviewItem::Change(added))), 3)
            );
            assert_eq!(
                editor.go_to_review_item(true),
                Some(ReviewItem::Change(gone))
            );
            assert_eq!(editor.go_to_review_item(true), None);
            assert_eq!(
                editor.go_to_review_item(false),
                Some(ReviewItem::Change(added))
            );
            // On, Paused, and off with the changes accepted.
            assert_eq!(editor.tracking(), desk_model::Tracking::Off);
            editor.set_tracking(desk_model::Tracking::On);
            assert_eq!(editor.tracking(), desk_model::Tracking::On);
            editor.set_tracking(desk_model::Tracking::Paused);
            assert_eq!(editor.tracking(), desk_model::Tracking::Paused);
            assert!(editor.has_changes());
            assert_eq!(editor.resolve_all_changes(true), 2);
            assert!(!editor.has_changes());
            assert_eq!(editor.document().body().get(0).unwrap().text(), "one two");
            editor.set_tracking(desk_model::Tracking::Off);
            assert_eq!(editor.tracking(), desk_model::Tracking::Off);
        });
    }

    #[test]
    fn rev_02_a_comment_and_a_change_on_the_same_text_are_both_reached() {
        use super::super::ReviewItem;
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let p = document.start().paragraph;
            let ((comment, added), _) = document
                .transact("Changes", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two")?;
                    let comment = edit.add_comment(
                        Position::new(p, 4),
                        Position::new(p, 7),
                        desk_model::Comment {
                            id: desk_model::NodeId::new(1).unwrap(),
                            author: "Ana".into(),
                            date: None,
                            text: "Why?".into(),
                            replies: std::sync::Arc::new([]),
                        },
                    )?;
                    let added = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
                    edit.mark_change(Position::new(p, 4), Position::new(p, 7), added)?;
                    Ok((comment, added))
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            assert_eq!(
                editor.go_to_review_item(true),
                Some(ReviewItem::Comment(comment))
            );
            assert_eq!(
                editor.review_position(),
                (Some((1, ReviewItem::Comment(comment))), 2)
            );
            assert_eq!(
                editor.go_to_review_item(true),
                Some(ReviewItem::Change(added))
            );
            assert_eq!(
                editor.review_position(),
                (Some((2, ReviewItem::Change(added))), 2)
            );
            assert_eq!(editor.go_to_review_item(true), None);
            assert_eq!(
                editor.go_to_review_item(false),
                Some(ReviewItem::Comment(comment))
            );
            // Turned off, the change is accepted with it, in one action.
            editor.set_tracking(desk_model::Tracking::On);
            assert!(editor.turn_off_tracking(Some(true)));
            assert_eq!(editor.review_position().1, 1);
        });
    }
}
