// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tracked changes of the body on the pages (W148, REV-04): text
//! added is underlined, and text deleted struck, in the colour of its
//! author. The views of markup come with step 11 of W2.

use desk_model::ChangeKind;

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
        let palette = document.author_palette(local);
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
}
