// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tracked changes of the body on the pages (W148, REV-04): text
//! added is underlined, and text deleted struck, in the colour of its
//! author, with a bar beside their lines in the margin, as the view of
//! markup shows them: with these marks, without the text deleted, or as
//! if every change were accepted, without marks.

use desk_model::{ChangeKind, Markup};
use gtk::glib;

use super::comments::{Texts, spot_in};
use super::{Selection, Spot, TextEditor};
use crate::text_nav::{self, SpanRect};
use crate::view::ChangeBar;

impl TextEditor {
    /// Where the text of each change of the body is on the pages, with
    /// what the change did and the colour of its author, for the author of
    /// this computer named `local`, as the view of markup shows them: none
    /// in Final, and no text deleted without deletions (W148, W150,
    /// REV-04).
    #[cfg(test)]
    pub(super) fn change_marks(
        &self,
        local: &str,
    ) -> Vec<(SpanRect, ChangeKind, desk_model::AuthorColor)> {
        self.change_view(local).0
    }

    /// The marks of the changes as [`TextEditor::change_marks`] gives
    /// them, and the change bars beside their lines.
    pub(super) fn change_view(
        &self,
        local: &str,
    ) -> (
        Vec<(SpanRect, ChangeKind, desk_model::AuthorColor)>,
        Vec<ChangeBar>,
    ) {
        let markup = self.markup();
        if markup == Markup::Final {
            return (Vec::new(), Vec::new());
        }
        let all = self.placed_change_marks(local);
        let bars = {
            let state = self.state.borrow();
            change_bars(Self::main_document(&state), Self::main_layout(&state))
        };
        let marks = all
            .into_iter()
            .filter(|(_, kind, _)| markup.shows_deletions() || *kind != ChangeKind::Deletion)
            .collect();
        (marks, bars)
    }

    /// Where the text of every change is on the pages, with what it did
    /// and the colour of its author.
    fn placed_change_marks(
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

    /// How the tracked changes show (REV-04).
    pub fn markup(&self) -> Markup {
        Self::main_document(&self.state.borrow()).markup()
    }

    /// Shows the tracked changes with their marks, without the text
    /// deleted, or as if they were all accepted (REV-04). It is not an
    /// edit: the history does not change, and the document keeps it as
    /// how it shows.
    pub fn set_markup(&self, markup: Markup) {
        if markup == self.markup() {
            return;
        }
        if self.story().is_some() {
            self.leave_story(None);
        }
        self.state.borrow_mut().editor.set_markup(markup);
        self.lay_out_again();
        // Out of the text that no longer shows.
        self.select(self.selection());
        self.show_comment_marks();
    }
}

/// Adds the tracked changes that a selection of some text holds, once
/// each: its runs, the breaks it goes over and the formatting of the
/// paragraphs it holds some of.
fn changes_in(
    document: &desk_model::Document,
    selection: &Selection,
    out: &mut Vec<desk_model::NodeId>,
) {
    let (start, end) = selection.ordered();
    let mut add = |id: Option<desk_model::NodeId>| {
        if let Some(id) = id
            && document.changes().get(id).is_some()
            && !out.contains(&id)
        {
            out.push(id);
        }
    };
    for index in start.paragraph..=end.paragraph {
        let Some(paragraph) = document.body().get(index) else {
            break;
        };
        let len = paragraph.text().len();
        let from = if index == start.paragraph {
            start.offset
        } else {
            0
        };
        let to = if index == end.paragraph {
            end.offset.min(len)
        } else {
            len
        };
        let attrs = paragraph.attrs();
        // Its formatting, when some of it is selected: an empty paragraph
        // when the selection goes over it.
        if from < to || (len == 0 && index < end.paragraph) {
            add(attrs.format_change.flatten());
        }
        // The break at its end, when the selection goes over it.
        if index < end.paragraph {
            add(attrs.break_change.flatten());
        }
        let mut offset = 0;
        for run in paragraph.runs() {
            let run_end = offset + run.len();
            if offset < to && run_end > from {
                add(run.attrs.change.flatten());
            }
            offset = run_end;
        }
    }
}

/// What the bubble of a tracked change shows (REV-04): who made it and
/// when, what it did and to what text, and where it is on the page.
#[derive(Clone, Debug, PartialEq)]
pub struct ChangeBubble {
    /// The change.
    pub id: desk_model::NodeId,
    /// What it did.
    pub kind: ChangeKind,
    /// Who made it.
    pub author: String,
    /// When, as ISO 8601, if it is known.
    pub date: Option<String>,
    /// The colour of its author.
    pub color: desk_model::AuthorColor,
    /// Its text, its start only when it is long, without the marks of
    /// objects and fields.
    pub text: String,
    /// How many paragraphs it is in, its text, its break or its
    /// formatting.
    pub paragraphs: usize,
    /// Whether it holds an object or a field, which its text leaves out.
    pub object: bool,
    /// Where the selection on it starts on the pages.
    pub rect: SpanRect,
}

/// The characters of the text of a change that its bubble shows.
const BUBBLE_TEXT: usize = 80;

impl TextEditor {
    /// The bubble of the tracked change that the selection is on, while
    /// text is selected or Previous and Next of the bar selected it, for
    /// the author of this computer named `local` (REV-04). None in
    /// Final, where changes do not show.
    pub fn change_bubble(&self, local: &str) -> Option<ChangeBubble> {
        if self.markup() == Markup::Final {
            return None;
        }
        let Some((_, item @ super::ReviewItem::Change(id))) = self.review_position().0 else {
            return None;
        };
        let state = self.state.borrow();
        // A change of no text, as a break, or one the view hides, is
        // selected as a cursor: its bubble shows when the bar went to it.
        if state.selection.is_empty() && state.reviewed != Some(item) {
            return None;
        }
        let rect = match text_nav::selection_rects(&state.layout, &state.selection).first() {
            Some(rect) => *rect,
            None => {
                let caret = text_nav::caret(&state.layout, state.selection.focus)?;
                SpanRect {
                    page: caret.page,
                    x0: caret.x,
                    y0: caret.top,
                    x1: caret.x + 1.0,
                    y1: caret.bottom,
                }
            }
        };
        let document = Self::main_document(&state);
        let change = document.changes().get(id)?;
        let palette = document.author_palette_of(local, &crate::preferences::get().former_authors);
        // The text being edited: the body, or the story visited.
        let paragraphs: Vec<_> = state.editor.document().body().iter().cloned().collect();
        let (text, object) = change_text(&paragraphs, id);
        Some(ChangeBubble {
            id,
            kind: change.kind,
            author: change.author.to_string(),
            date: change.date.as_deref().map(str::to_owned),
            color: palette.color(&change.author),
            text: excerpt(&text),
            object,
            paragraphs: paragraphs.iter().filter(|p| in_change(p, id)).count(),
            rect,
        })
    }
}

/// Whether a change is in a paragraph: its text, its break or its
/// formatting.
fn in_change(paragraph: &desk_model::Paragraph, id: desk_model::NodeId) -> bool {
    let attrs = paragraph.attrs();
    attrs.break_change.flatten() == Some(id)
        || attrs.format_change.flatten() == Some(id)
        || paragraph
            .runs()
            .any(|r| r.attrs.change.flatten() == Some(id))
}

/// The text that a change holds in some paragraphs, a space between
/// its paragraphs, without the marks that stand for objects and fields.
/// Gives too whether it holds any.
fn change_text(paragraphs: &[desk_model::Paragraph], id: desk_model::NodeId) -> (String, bool) {
    let mut out = String::new();
    let mut object = false;
    for paragraph in paragraphs {
        let mut here = String::new();
        for run in paragraph
            .runs()
            .filter(|r| r.attrs.change.flatten() == Some(id))
        {
            if run.attrs.field.clone().flatten().is_some() {
                object = true;
                continue;
            }
            for c in run.text().chars() {
                if c == '\u{fffc}' {
                    object = true;
                } else if c.is_control() {
                    // A tab or a line break.
                    here.push(' ');
                } else {
                    here.push(c);
                }
            }
        }
        if !here.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&here);
        }
    }
    (out, object)
}

/// The start of a text, with an ellipsis, when it is long.
fn excerpt(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(BUBBLE_TEXT) {
        Some((at, _)) => format!("{}\u{2026}", text[..at].trim_end()),
        None => text.to_owned(),
    }
}

/// The space between a change bar and the text, in points.
const BAR_GAP: f64 = 9.0;

/// A bar for each line of the body with changes, in the left margin of
/// its page, deleted text that does not show included: lighter when the
/// changes of the line only change its formatting, as in Pages
/// (REV-04).
fn change_bars(
    document: &desk_model::Document,
    layout: &desk_layout::DocumentLayout,
) -> Vec<ChangeBar> {
    let mut bars: Vec<ChangeBar> = Vec::new();
    for (story, span) in document.change_places() {
        let (None, Some(change)) = (story, document.changes().get(span.id)) else {
            continue;
        };
        let (Some(start), Some(end)) =
            (Spot::of(document, span.start), Spot::of(document, span.end))
        else {
            continue;
        };
        let format_only = change.kind == ChangeKind::Format;
        for paragraph in start.paragraph..=end.paragraph {
            for (page, line) in text_nav::lines_of(layout, paragraph) {
                let range = &line.line.range;
                // The lines that hold some of the change.
                if Spot::new(paragraph, range.start) >= end
                    || Spot::new(paragraph, range.end) <= start
                {
                    continue;
                }
                let (y0, y1) = (f64::from(line.top()), f64::from(line.bottom()));
                let same = bars
                    .iter_mut()
                    .find(|b| b.page == page && (b.y0 - y0).abs() < 0.5 && (b.y1 - y1).abs() < 0.5);
                match same {
                    Some(bar) => bar.format_only &= format_only,
                    None => {
                        let margin = layout
                            .pages
                            .get(page)
                            .map_or(0.0, |p| f64::from(p.margins.0));
                        bars.push(ChangeBar {
                            page,
                            x: (margin - BAR_GAP).max(1.0),
                            y0,
                            y1,
                            format_only,
                        });
                    }
                }
            }
        }
    }
    bars
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

    /// The tracked changes that Accept and Reject of the selection take
    /// (REV-06): the change that the bar selected, or that the cursor is
    /// in, or those of the text selected, in every range of a multiple
    /// selection: their text, the breaks they go over and the formatting
    /// of the paragraphs they hold some of.
    pub fn changes_at_selection(&self) -> Vec<desk_model::NodeId> {
        let current = self.review_position().0;
        let state = self.state.borrow();
        if let Some((_, item @ super::ReviewItem::Change(id))) = current
            && (state.selection.is_empty() || state.reviewed == Some(item))
            && state.others.is_empty()
        {
            // The change the bar went to, alone: not what else its text
            // holds.
            return vec![id];
        }
        let document = state.editor.document();
        let mut out = Vec::new();
        for selection in std::iter::once(&state.selection).chain(&state.others) {
            changes_in(document, selection, &mut out);
        }
        out
    }

    /// Accepts or rejects the tracked changes of the selection, as one
    /// action of the history (REV-06), and gives how many there were. The
    /// header, footer, note or text box being edited stays so.
    pub fn resolve_selected_changes(&self, accept: bool) -> usize {
        let ids = self.changes_at_selection();
        if ids.is_empty() {
            return 0;
        }
        // A change may be in more than one text: from the body, and back
        // to the story.
        let visit = self
            .state
            .borrow()
            .story
            .as_ref()
            .map(|m| (m.story, m.page));
        let selection = self.selection();
        if visit.is_some() {
            self.leave_story(None);
        }
        let name = if accept {
            "Accept Change"
        } else {
            "Reject Change"
        };
        let mut count = 0;
        let counted = &mut count;
        self.format_edit(name, |edit| {
            *counted = edit.resolve_changes(accept, |id| ids.contains(&id))?;
            Ok(())
        });
        if let Some((story, page)) = visit
            && self.story_exists(story)
        {
            self.enter_story(story, page, Some(selection.focus));
            if self.story() == Some(story) {
                self.select(selection);
            }
        }
        count
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
    fn rev_04_the_views_of_markup_hide_deletions_and_marks() {
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let p = document.start().paragraph;
            document
                .transact("Changes", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three")?;
                    let added = edit.add_change(ChangeKind::Insertion, "Ana", None)?;
                    let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
                    edit.mark_change(Position::new(p, 0), Position::new(p, 3), added)?;
                    edit.mark_change(Position::new(p, 4), Position::new(p, 8), gone)?;
                    Ok(())
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            let kinds = |editor: &TextEditor| -> Vec<ChangeKind> {
                editor
                    .change_marks("Ana")
                    .iter()
                    .map(|(_, k, _)| *k)
                    .collect()
            };
            // Markup: both, with one bar for their line.
            assert_eq!(editor.markup(), Markup::Markup);
            assert_eq!(
                kinds(&editor),
                [ChangeKind::Insertion, ChangeKind::Deletion]
            );
            let (_, bars) = editor.change_view("Ana");
            assert_eq!(bars.len(), 1);
            assert!(!bars[0].format_only);
            let three = |editor: &TextEditor| {
                let state = editor.state.borrow();
                let selection = Selection {
                    anchor: Spot {
                        paragraph: 0,
                        offset: 8,
                    },
                    focus: Spot {
                        paragraph: 0,
                        offset: 13,
                    },
                };
                text_nav::selection_rects(&state.layout, &selection)[0].x0
            };
            let shown = three(&editor);
            // Without deletions: the deleted word takes no room, and its
            // line keeps its bar.
            editor.set_markup(Markup::WithoutDeletions);
            assert_eq!(editor.markup(), Markup::WithoutDeletions);
            assert_eq!(kinds(&editor), [ChangeKind::Insertion]);
            assert_eq!(editor.change_view("Ana").1.len(), 1);
            assert!(three(&editor) < shown - 5.0);
            // Final: no marks, no bars.
            editor.set_markup(Markup::Final);
            assert_eq!(editor.change_view("Ana"), (Vec::new(), Vec::new()));
            // The view is not an edit, and edits keep it.
            assert_eq!(editor.state.borrow().editor.undo_depth(), 0);
            editor.set_markup(Markup::WithoutDeletions);
            editor.set_tracking(desk_model::Tracking::On);
            editor.undo();
            assert_eq!(editor.markup(), Markup::WithoutDeletions);
            assert_eq!(editor.document().markup(), Markup::WithoutDeletions);
        });
    }

    #[test]
    fn rev_04_the_cursor_goes_past_deleted_text_that_does_not_show() {
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let p = document.start().paragraph;
            document
                .transact("Changes", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three")?;
                    let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
                    edit.mark_change(Position::new(p, 4), Position::new(p, 8), gone)?;
                    Ok(())
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            editor.select(Selection {
                anchor: Spot::new(0, 5),
                focus: Spot::new(0, 6),
            });
            editor.set_markup(Markup::WithoutDeletions);
            // The selection in the text gone is a cursor before it.
            assert_eq!(editor.selection(), Selection::cursor(Spot::new(0, 4)));
            // A line whose only change does not show keeps its bar.
            assert_eq!(editor.change_view("Ana").1.len(), 1);
            let document = editor.document();
            assert_eq!(
                text_nav::move_char(&document, Spot::new(0, 4), true),
                Spot::new(0, 9)
            );
            assert_eq!(
                text_nav::move_char(&document, Spot::new(0, 8), false),
                Spot::new(0, 3)
            );
            // Delete takes the next character that shows.
            editor.delete(true, false);
            assert_eq!(
                editor.document().body().get(0).unwrap().text(),
                "one two hree"
            );
        });
    }

    #[test]
    fn b_071_a_deleted_break_joins_its_paragraphs_without_deletions() {
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            document
                .transact("Changes", |edit| {
                    let start = edit.document().start();
                    let end = edit.insert_text(start, "one ")?;
                    let second = edit.split_paragraph(end)?;
                    let end = edit.insert_text(second, "two")?;
                    let third = edit.split_paragraph(end)?;
                    edit.insert_text(third, "three")?;
                    let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
                    edit.mark_break_change(start.paragraph, Some(gone))?;
                    edit.mark_change(second, Position::new(second.paragraph, 3), gone)?;
                    edit.mark_break_change(second.paragraph, Some(gone))?;
                    Ok(())
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            editor.set_markup(Markup::WithoutDeletions);
            let document = editor.document();
            let state = editor.state.borrow();
            let layout = &state.layout;
            // "three" shows after "one " on its line.
            let one = text_nav::caret(layout, Spot::new(0, 0)).unwrap();
            let three = text_nav::caret(layout, Spot::new(2, 0)).unwrap();
            let end = text_nav::caret(layout, Spot::new(0, 4)).unwrap();
            assert_eq!(three.top, one.top);
            assert_eq!(three.x, end.x);
            // The arrows go past the break that does not show, as past a
            // character, and Home and End to the edges of the line.
            assert_eq!(
                text_nav::move_char(&document, Spot::new(0, 4), true),
                Spot::new(2, 1)
            );
            assert_eq!(
                text_nav::move_char(&document, Spot::new(2, 0), false),
                Spot::new(0, 3)
            );
            assert_eq!(
                text_nav::line_edge(layout, Spot::new(0, 1), true),
                Spot::new(2, 5)
            );
            assert_eq!(
                text_nav::line_edge(layout, Spot::new(2, 3), false),
                Spot::new(0, 0)
            );
            // One mark of the end of a paragraph, after "three".
            let ends = text_nav::invisibles(&document, layout)
                .into_iter()
                .filter(|m| m.kind == text_nav::Invisible::Paragraph)
                .count();
            assert_eq!(ends, 1);
            // The text of a story goes past them too, as it joins where it
            // is laid out (B-074).
            let story = document.with_body(document.body().iter().cloned().collect());
            assert_eq!(
                text_nav::move_char(&story, Spot::new(0, 4), true),
                Spot::new(2, 1)
            );
            drop(state);
            // Delete takes the next character that shows.
            editor.select(Selection::cursor(Spot::new(0, 4)));
            editor.delete(true, false);
            let texts: Vec<String> = editor.document().body().iter().map(|p| p.text()).collect();
            assert_eq!(texts, ["one ", "two", "hree"]);
        });
    }

    /// B-074 review: Show Invisibles marks no end of a paragraph in a
    /// text box where its deleted break does not show.
    #[test]
    fn b_074_a_joined_paragraph_of_a_text_box_shows_no_end_mark() {
        use desk_model::{Frame, Markup, Object, ObjectKind, ParaAttrs, Paragraph, Run, TextBox};
        let mut document = Document::new_word_processing();
        document
            .transact("Box", |edit| {
                let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
                let style = edit.document().styles().default_paragraph();
                let mut texts = Vec::new();
                for (i, text) in ["one ", "two"].into_iter().enumerate() {
                    let mut attrs = ParaAttrs::default();
                    if i == 0 {
                        attrs.break_change = Some(Some(gone));
                    }
                    texts.push(Paragraph::from_runs(
                        edit.new_id(),
                        style,
                        attrs,
                        [Run::new(text)],
                    )?);
                }
                let text = TextBox::new(
                    std::sync::Arc::from(texts),
                    desk_model::Length::from_points(5),
                );
                let id = edit.new_id();
                let mut object = Object::new(
                    id,
                    ObjectKind::TextBox(text),
                    Frame::from_pt(100.0, 100.0, 300.0, 200.0),
                );
                object.wrap = desk_model::Wrap::Through;
                edit.add_object(object)
            })
            .unwrap();
        let ends = |document: &Document| {
            let layout = crate::with_text_system(|system| desk_layout::paginate(document, system));
            text_nav::invisibles(document, &layout)
                .into_iter()
                .filter(|m| m.kind == text_nav::Invisible::Paragraph)
                .count()
        };
        // The end of the body and of each paragraph of the box.
        assert_eq!(ends(&document), 3);
        document.set_markup(Markup::WithoutDeletions);
        assert_eq!(ends(&document), 2);
    }

    #[test]
    fn rev_04_the_change_selected_has_a_bubble() {
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let p = document.start().paragraph;
            let ((added, gone), _) = document
                .transact("Changes", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three")?;
                    let added =
                        edit.add_change(ChangeKind::Insertion, "Bo", Some("2026-11-25T10:00:00Z"))?;
                    let gone = edit.add_change(ChangeKind::Deletion, "Ana", None)?;
                    edit.mark_change(Position::new(p, 0), Position::new(p, 3), added)?;
                    edit.mark_change(Position::new(p, 4), Position::new(p, 8), gone)?;
                    Ok((added, gone))
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            // The cursor alone shows none.
            assert_eq!(editor.change_bubble("Ana"), None);
            assert_eq!(
                editor.go_to_review_item(true),
                Some(super::super::ReviewItem::Change(added))
            );
            let bubble = editor.change_bubble("Ana").unwrap();
            assert_eq!(
                (
                    bubble.id,
                    bubble.kind,
                    bubble.author.as_str(),
                    bubble.text.as_str()
                ),
                (added, ChangeKind::Insertion, "Bo", "one")
            );
            assert_eq!(bubble.date.as_deref(), Some("2026-11-25T10:00:00Z"));
            // Bo takes the first colour Ana leaves.
            assert_eq!(bubble.color, AuthorColor::Green);
            assert_eq!(
                crate::change_bubble::what(&bubble),
                "<b>Added:</b> \u{201c}one\u{201d}"
            );
            editor.go_to_review_item(true);
            let bubble = editor.change_bubble("Ana").unwrap();
            assert_eq!((bubble.id, bubble.text.as_str()), (gone, "two"));
            // A deletion that the view hides is selected as a cursor,
            // and has its bubble when the bar went to it.
            editor.set_markup(Markup::WithoutDeletions);
            editor.go_to_review_item(false);
            assert_eq!(
                editor.go_to_review_item(true),
                Some(super::super::ReviewItem::Change(gone))
            );
            assert!(editor.selection().is_empty());
            let bubble = editor.change_bubble("Ana").unwrap();
            assert_eq!((bubble.id, bubble.text.as_str()), (gone, "two"));
            // Final shows no change, nor its bubble.
            editor.set_markup(Markup::Final);
            assert_eq!(editor.change_bubble("Ana"), None);
            // What a change of no text did.
            let format = ChangeBubble {
                kind: ChangeKind::Format,
                text: String::new(),
                paragraphs: 3,
                ..bubble.clone()
            };
            assert_eq!(
                crate::change_bubble::what(&format),
                "<b>Formatted:</b> 3 Paragraphs"
            );
            let object = ChangeBubble {
                kind: ChangeKind::Insertion,
                text: String::new(),
                object: true,
                ..bubble
            };
            assert_eq!(crate::change_bubble::what(&object), "<b>Added:</b> Object");
        });
    }

    #[test]
    fn rev_04_a_long_change_shows_its_start() {
        let long = "word ".repeat(30);
        let short = excerpt(&long);
        assert!(short.chars().count() <= BUBBLE_TEXT + 1);
        assert!(short.ends_with("word\u{2026}"));
        assert_eq!(excerpt(" two "), "two");
    }

    #[test]
    fn rev_06_the_change_selected_or_those_of_the_selection_are_accepted_or_rejected() {
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let p = document.start().paragraph;
            let ((added, gone), _) = document
                .transact("Changes", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three")?;
                    let added = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
                    let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
                    edit.mark_change(Position::new(p, 0), Position::new(p, 3), added)?;
                    edit.mark_change(Position::new(p, 4), Position::new(p, 8), gone)?;
                    Ok((added, gone))
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            let text = |editor: &TextEditor| editor.document().body().get(0).unwrap().text();
            // The selection over both takes both.
            editor.select(Selection {
                anchor: Spot::new(0, 1),
                focus: Spot::new(0, 6),
            });
            assert_eq!(editor.changes_at_selection(), [added, gone]);
            // The cursor in a change takes it alone.
            editor.select(Selection::cursor(Spot::new(0, 5)));
            assert_eq!(editor.changes_at_selection(), [gone]);
            // Rejected, the deleted text stays, and is no change.
            assert_eq!(editor.resolve_selected_changes(false), 1);
            assert_eq!(text(&editor), "one two three");
            assert!(editor.document().changes().get(gone).is_none());
            // Accepted, the added text stays as text.
            assert_eq!(
                editor.go_to_review_item(false),
                Some(super::super::ReviewItem::Change(added))
            );
            assert_eq!(editor.resolve_selected_changes(true), 1);
            assert_eq!(text(&editor), "one two three");
            assert!(!editor.has_changes());
            assert_eq!(editor.resolve_selected_changes(true), 0);
            // One action of the history each.
            editor.undo();
            assert!(editor.document().changes().get(added).is_some());
            assert!(editor.document().changes().get(gone).is_none());
        });
    }

    #[test]
    fn rev_06_accept_and_reject_take_what_is_selected_and_no_more() {
        use super::super::ReviewItem;
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let ((added, formatted, inside), _) = document
                .transact("Changes", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one\ntwo two")?;
                    let first = edit.document().body().get(0).unwrap().id();
                    let second = edit.document().body().get(1).unwrap().id();
                    let added = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
                    edit.mark_change(Position::new(first, 0), Position::new(first, 3), added)?;
                    let formatted = edit.add_change(ChangeKind::Format, "Ana", None)?;
                    edit.mark_paragraph_format_change(second, formatted)?;
                    let inside = edit.add_change(ChangeKind::Insertion, "Bo", None)?;
                    edit.mark_change(Position::new(second, 0), Position::new(second, 3), inside)?;
                    Ok((added, formatted, inside))
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            // To the start of the next paragraph: not its formatting.
            editor.select(Selection {
                anchor: Spot::new(0, 0),
                focus: Spot::new(1, 0),
            });
            assert_eq!(editor.changes_at_selection(), [added]);
            // Every range of a multiple selection.
            editor.select(Selection {
                anchor: Spot::new(0, 1),
                focus: Spot::new(0, 2),
            });
            editor.state.borrow_mut().others = vec![Selection {
                anchor: Spot::new(1, 5),
                focus: Spot::new(1, 6),
            }];
            assert_eq!(editor.changes_at_selection(), [added, formatted]);
            editor.state.borrow_mut().others.clear();
            // The change the bar went to alone, not the one in its text.
            editor.select(Selection {
                anchor: Spot::new(1, 0),
                focus: Spot::new(1, 7),
            });
            assert_eq!(editor.changes_at_selection(), [formatted, inside]);
            editor.state.borrow_mut().reviewed = Some(ReviewItem::Change(formatted));
            assert_eq!(editor.changes_at_selection(), [formatted]);
            assert_eq!(editor.resolve_selected_changes(false), 1);
            assert!(editor.document().changes().get(inside).is_some());
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
