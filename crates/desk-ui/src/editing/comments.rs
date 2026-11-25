// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The comments on the text, as the window adds, edits and goes to them
//! (W149, REV-08): a new comment is on the selection, or on the word at
//! the cursor, as in Pages 15.4, in the body or in a header, a footer, a
//! note, a text box or a shape; a comment emptied is deleted, and a
//! highlight given text becomes a comment (PXV-19).

use std::sync::Arc;

use desk_layout::DocumentLayout;
use desk_model::{Comment, Document, NodeId, ObjectKind, Paragraph, Position, Reply, Story};

use super::stories::story_layout_everywhere;
use super::{Selection, Spot, TextEditor, at_spot};
use crate::text_nav::{self, SpanRect};

/// A comment as the window shows it: the text it is in, none for the
/// body, and the text it is on, as spots of that text, or the object it
/// is on, or the table and the row and column of the cell it is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placed {
    id: NodeId,
    story: Option<Story>,
    start: Spot,
    end: Spot,
    object: Option<NodeId>,
    cell: Option<(u32, u32)>,
    /// Whether it is a tracked change rather than a comment (W148).
    change: bool,
}

/// A comment or a tracked change, as Previous and Next of the bar of
/// comments and changes go through them (REV-02).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewItem {
    /// A comment.
    Comment(NodeId),
    /// A tracked change.
    Change(NodeId),
}

impl Placed {
    fn item(&self) -> ReviewItem {
        if self.change {
            ReviewItem::Change(self.id)
        } else {
            ReviewItem::Comment(self.id)
        }
    }
}

/// Every tracked change of a document, with the text it covers in each
/// text it is in, from the start of its first piece to the end of its
/// last (W148).
fn placed_changes(document: &Document) -> Vec<Placed> {
    let mut out: Vec<Placed> = Vec::new();
    for (story, span) in document.change_places() {
        let spots = match story {
            None => Spot::of(document, span.start).zip(Spot::of(document, span.end)),
            Some(story) => {
                let paragraphs = document.story_paragraphs(story);
                spot_in(&paragraphs, span.start).zip(spot_in(&paragraphs, span.end))
            }
        };
        let Some((start, end)) = spots else { continue };
        match out.iter_mut().find(|p| p.id == span.id && p.story == story) {
            Some(place) => place.end = end,
            None => out.push(Placed {
                id: span.id,
                story,
                start,
                end,
                object: None,
                cell: None,
                change: true,
            }),
        }
    }
    // The objects that changes moved or resized, each where it is now
    // (REV-01).
    out.extend(document.objects().iter().filter_map(|o| {
        let moved = o.moved?;
        document.changes().get(moved.change)?;
        Some(Placed {
            id: moved.change,
            story: None,
            start: Spot::default(),
            end: Spot::default(),
            object: Some(o.id),
            cell: None,
            change: true,
        })
    }));
    out
}

/// What a new comment is on: the text between two spots of the text being
/// edited, an object, or a cell, as Pages 15.4 comments a selected object
/// or cell (W149, TAB-27).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentTarget {
    /// Text.
    Text(Spot, Spot),
    /// An object.
    Object(NodeId),
    /// A cell of a table, by row and column.
    Cell(NodeId, u32, u32),
}

/// Every comment of a document, with the text it is on.
fn placed(document: &Document) -> Vec<Placed> {
    document
        .comment_places()
        .into_iter()
        .filter_map(|place| {
            let (start, end) = match place.story {
                None => (
                    Spot::of(document, place.start)?,
                    Spot::of(document, place.end)?,
                ),
                Some(story) => {
                    let paragraphs = document.story_paragraphs(story);
                    (
                        spot_in(&paragraphs, place.start)?,
                        spot_in(&paragraphs, place.end)?,
                    )
                }
            };
            Some(Placed {
                id: place.id,
                story: place.story,
                start,
                end,
                object: None,
                cell: None,
                change: false,
            })
        })
        .chain(
            document
                .object_comments()
                .into_iter()
                .map(|(id, object)| Placed {
                    id,
                    story: None,
                    start: Spot::default(),
                    end: Spot::default(),
                    object: Some(object),
                    cell: None,
                    change: false,
                }),
        )
        .chain(document.cell_comments().into_iter().map(|c| Placed {
            id: c.id,
            story: None,
            start: Spot::default(),
            end: Spot::default(),
            object: Some(c.table),
            cell: Some((c.row, c.col)),
            change: false,
        }))
        .collect()
}

/// Where a cell of a table is on the pages.
fn cell_rect(layout: &DocumentLayout, table: NodeId, (row, col): (u32, u32)) -> Option<SpanRect> {
    let (page, placed) = super::objects::find(layout, table)?;
    let (x, y, w, h) = placed.table.as_ref()?.cell_rect(row, col)?;
    let (x0, y0) = (f64::from(placed.x), f64::from(placed.y));
    Some(SpanRect {
        page,
        x0: x0 + f64::from(x),
        y0: y0 + f64::from(y),
        x1: x0 + f64::from(x + w),
        y1: y0 + f64::from(y + h),
    })
}

/// What the card of a comment on a cell quotes: the text the cell shows,
/// or the name of the table when it shows none.
fn cell_text(document: &Document, table: NodeId, (row, col): (u32, u32)) -> String {
    let shown = document.objects().get(table).and_then(|o| match &o.kind {
        ObjectKind::Table(t) => {
            let locale = desk_layout::document_locale(document);
            let values = t.values(desk_layout::today());
            Some(t.shown(&values, row, col, &locale).text)
        }
        _ => None,
    });
    match shown {
        Some(text) if !text.trim().is_empty() => text.replace('\n', " ").trim().to_owned(),
        _ => object_name(document, table),
    }
}

/// Where an object is on the pages, in a group too.
pub(super) fn object_rect(layout: &DocumentLayout, id: NodeId) -> Option<SpanRect> {
    fn find(placed: &desk_layout::PlacedObject, id: NodeId) -> Option<&desk_layout::PlacedObject> {
        if placed.object.id == id {
            return Some(placed);
        }
        placed.children.iter().find_map(|c| find(c, id))
    }
    layout.pages.iter().enumerate().find_map(|(page, p)| {
        let o = p.objects.iter().find_map(|o| find(o, id))?;
        Some(SpanRect {
            page,
            x0: f64::from(o.x),
            y0: f64::from(o.y),
            x1: f64::from(o.x + o.width),
            y1: f64::from(o.y + o.height),
        })
    })
}

/// The name of an object, which the card of a comment on it quotes.
fn object_name(document: &Document, id: NodeId) -> String {
    fn find(o: &desk_model::Object, id: NodeId) -> Option<&desk_model::Object> {
        if o.id == id {
            return Some(o);
        }
        match &o.kind {
            ObjectKind::Group(members) => members.iter().find_map(|m| find(m, id)),
            _ => None,
        }
    }
    let kind = document
        .objects()
        .iter()
        .find_map(|o| find(o, id))
        .map(|o| &o.kind);
    crate::i18n::tr(match kind {
        Some(ObjectKind::TextBox(_)) => "Text Box",
        Some(ObjectKind::Image(_)) => "Image",
        Some(ObjectKind::Media(_)) => "Movie",
        Some(ObjectKind::Table(_)) => "Table",
        Some(ObjectKind::Chart(_)) => "Chart",
        _ => "Shape",
    })
}

/// A position of a story as a spot of its text.
pub(super) fn spot_in(paragraphs: &[Paragraph], at: Position) -> Option<Spot> {
    let index = paragraphs.iter().position(|p| p.id() == at.paragraph)?;
    Some(Spot::new(index, at.offset))
}

/// The document laid out with the lines of each text that has comments,
/// the body and the stories, as the pages show them.
pub(super) struct Texts<'a> {
    main: &'a DocumentLayout,
    stories: Vec<(Story, DocumentLayout)>,
}

impl<'a> Texts<'a> {
    fn new(document: &Document, main: &'a DocumentLayout, places: &[Placed]) -> Self {
        Self::with(document, main, places.iter().filter_map(|p| p.story))
    }

    /// The same, with the lines of these stories.
    pub(super) fn with(
        document: &Document,
        main: &'a DocumentLayout,
        stories_of: impl IntoIterator<Item = Story>,
    ) -> Self {
        let mut stories: Vec<(Story, DocumentLayout)> = Vec::new();
        for story in stories_of {
            if stories.iter().any(|(s, _)| *s == story) {
                continue;
            }
            let copy = document.with_body(document.story_paragraphs(story));
            stories.push((story, story_layout_everywhere(main, story, &copy)));
        }
        Self { main, stories }
    }

    pub(super) fn layout(&self, story: Option<Story>) -> Option<&DocumentLayout> {
        match story {
            None => Some(self.main),
            Some(story) => self
                .stories
                .iter()
                .find(|(s, _)| *s == story)
                .map(|(_, l)| l),
        }
    }

    /// Where the text or the object of a comment is on the pages.
    fn rects(&self, place: &Placed) -> Vec<SpanRect> {
        if let (Some(table), Some(cell)) = (place.object, place.cell) {
            return cell_rect(self.main, table, cell).into_iter().collect();
        }
        if let Some(object) = place.object {
            return object_rect(self.main, object).into_iter().collect();
        }
        self.layout(place.story).map_or_else(Vec::new, |layout| {
            text_nav::selection_rects(
                layout,
                &Selection {
                    anchor: place.start,
                    focus: place.end,
                },
            )
        })
    }

    /// The place of a text in the order of the comments, as Pages 15.4
    /// lists them, by page: on each, those of its header, of the body in
    /// its order, of its text boxes and shapes from the top, of its notes
    /// and of its footer. A text that no page shows comes last.
    fn key(&self, place: &Placed) -> Key {
        let (story, start, end) = (place.story, place.start, place.end);
        // An empty selection, the cursor, is where the cursor stands.
        let first = self.rects(place).into_iter().next().or_else(|| {
            let caret = text_nav::caret(self.layout(story)?, start)?;
            Some(SpanRect {
                page: caret.page,
                x0: caret.x,
                y0: caret.top,
                x1: caret.x,
                y1: caret.bottom,
            })
        });
        let page = first.map_or(usize::MAX, |r| r.page);
        let group = match story {
            Some(Story::Margin { place, .. }) => match place.kind {
                desk_model::MarginKind::Header => 0,
                desk_model::MarginKind::Footer => 4,
            },
            None if place.object.is_some() => 2,
            None => 1,
            Some(Story::TextBox(_)) => 2,
            Some(Story::Note(_)) => 3,
        };
        // Outside the body, by where the text or the object is on the
        // page.
        let outside = story.is_some() || place.object.is_some();
        let (y, x) = match first {
            Some(r) if outside => ((r.y0 * 100.0) as u64, (r.x0 * 100.0) as u64),
            _ => (0, 0),
        };
        (page, group, y, x, start, end)
    }

    /// The place of a comment or change in the order of the bar of
    /// comments and changes: its text, and then comments before changes
    /// on the same text, by identifier.
    fn full_key(&self, place: &Placed) -> FullKey {
        (self.key(place), place.change, place.id)
    }

    /// The comments, in that order.
    fn ordered(&self, mut places: Vec<Placed>) -> Vec<Placed> {
        places.sort_by_cached_key(|p| self.key(p));
        places
    }
}

/// The place of a text in the order of the comments: its page, which of
/// its texts, where it is on the page and where in the text.
type Key = (usize, u8, u64, u64, Spot, Spot);

/// The place of a comment or change in the order of the bar.
type FullKey = (Key, bool, NodeId);

/// The comments and changes of a document in the order of the bar.
pub(super) struct ReviewOrder {
    document: Document,
    places: Vec<Placed>,
}

/// The text that a comment is on, on one line, as its card quotes it.
fn commented_text(document: &Document, start: Position, end: Position) -> String {
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
    /// word at the cursor when nothing is selected, as Pages 15.4 does, in
    /// the body or in the header, footer, note, text box or shape being
    /// edited. None in a cell, whose comments come with the comments on
    /// objects and cells, and where there is no text.
    pub fn comment_range(&self) -> Option<(Spot, Spot)> {
        if self.cell_rects().is_some() {
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

    /// Highlights the text selected, by `author`, now: a comment without
    /// text, which typing in its bubble makes a comment (W059, PXV-19).
    /// Nothing without text selected, as Insert > Highlight of Pages 15.4
    /// is off then. Gives the new highlight.
    pub fn add_highlight(&self, author: &str) -> Option<NodeId> {
        if !self.text_selected_for_comments() {
            return None;
        }
        let (start, end) = self.selection().ordered();
        if start == end {
            return None;
        }
        let highlight = Comment {
            id: NodeId::new(1).expect("not zero"),
            author: author.into(),
            date: Some(crate::comments_page::now().into()),
            text: "".into(),
            replies: Arc::new([]),
        };
        let mut added = None;
        self.format_edit("Highlight", |edit| {
            let from = at_spot(edit.document(), start)?;
            let to = at_spot(edit.document(), end)?;
            added = Some(edit.add_comment(from, to, highlight)?);
            Ok(())
        });
        added
    }

    /// The highlights and comments on the text selected, or on the text
    /// at the cursor: those whose text meets the selection, in the text
    /// being edited.
    pub fn comments_in_selection(&self) -> Vec<NodeId> {
        if !self.text_selected_for_comments() {
            return Vec::new();
        }
        let (start, end) = self.selection().ordered();
        let story = self.story();
        let state = self.state.borrow();
        placed(Self::main_document(&state))
            .into_iter()
            .filter(|p| p.object.is_none() && p.story == story)
            .filter(|p| {
                if p.start == p.end {
                    // A comment at a point, from ODT: the selection holds
                    // it, or the cursor is at it.
                    start <= p.start && p.start <= end
                } else if start == end {
                    // The cursor is in the text, not at its end.
                    p.start <= start && start < p.end
                } else {
                    p.start < end && start < p.end
                }
            })
            .map(|p| p.id)
            .collect()
    }

    /// Edit > Remove Highlights and Comments: deletes those on the text
    /// selected, with their replies, in one step that Undo takes back
    /// (PXV-19). Gives how many.
    pub fn remove_comments_in_selection(&self) -> usize {
        let ids = self.comments_in_selection();
        if ids.is_empty() {
            return 0;
        }
        match self.format_edit_quiet("Remove Highlights and Comments", |edit| {
            ids.iter().try_for_each(|id| edit.remove_comment(*id))
        }) {
            Ok(()) => ids.len(),
            Err(error) => {
                gtk::glib::g_warning!("desk", "cannot remove the comments: {error}");
                0
            }
        }
    }

    /// Whether the selection is of text, not of objects or of cells,
    /// which keep the text selection that was before them.
    fn text_selected_for_comments(&self) -> bool {
        self.cell_rects().is_none()
            && self.table_selection().is_none()
            && !self.has_selected_objects()
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

    /// Deletes a comment, with its replies, and takes it off its text. One
    /// in another text than the one being edited is deleted from the
    /// document, with the cursor back in the body.
    pub fn delete_comment(&self, id: NodeId) {
        let place = self.place_of(id);
        if let Some(editing) = self.story()
            && place.is_some_and(|p| p.story != Some(editing) || p.object.is_some())
        {
            self.leave_story(None);
        }
        self.format_edit("Delete Comment", |edit| edit.remove_comment(id));
    }

    /// Where a comment is in the document.
    fn place_of(&self, id: NodeId) -> Option<Placed> {
        let state = self.state.borrow();
        placed(Self::main_document(&state))
            .into_iter()
            .find(|p| p.id == id)
    }

    /// The comments of the document in the order that the Comments page
    /// lists them and that Next Comment goes through them, by page, with
    /// the text each is on (W149).
    pub fn comments_in_order(&self) -> Vec<(NodeId, String)> {
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        let places = placed(document);
        let texts = Texts::new(document, Self::main_layout(&state), &places);
        // A table with comments on many cells is computed once, within
        // the work of the document.
        let _work = desk_model::table::share_work();
        texts
            .ordered(places)
            .into_iter()
            .filter_map(|p| {
                if let (Some(table), Some(cell)) = (p.object, p.cell) {
                    return Some((p.id, cell_text(document, table, cell)));
                }
                if let Some(object) = p.object {
                    return Some((p.id, object_name(document, object)));
                }
                let place = document
                    .comment_places()
                    .into_iter()
                    .find(|c| c.id == p.id)?;
                let on = match place.story {
                    None => commented_text(document, place.start, place.end),
                    Some(story) => commented_text(
                        &document.with_body(document.story_paragraphs(story)),
                        place.start,
                        place.end,
                    ),
                };
                Some((p.id, on))
            })
            .collect()
    }

    /// Selects the text that a comment is on, and shows it: in the body,
    /// or in the header, footer, note, text box or shape it is in.
    pub fn select_comment(&self, id: NodeId) -> bool {
        match self.place_of(id) {
            Some(place) => self.select_place(&place),
            None => false,
        }
    }

    /// Selects the text or the object of a comment or change, and shows
    /// it.
    fn select_place(&self, place: &Placed) -> bool {
        let place = *place;
        let current = self.story();
        if let Some(object) = place.object {
            if current.is_some() {
                self.leave_story(None);
            }
            self.select_objects(vec![object]);
            if let Some(cell) = place.cell {
                self.select_cell(object, cell);
                return self.selected_cells().is_some_and(|c| c.focus == cell);
            }
            return self.selected_objects() == [object];
        }
        match place.story {
            None if current.is_some() => self.leave_story(None),
            Some(story) if current != Some(story) => {
                let page = {
                    let state = self.state.borrow();
                    let document = Self::main_document(&state);
                    let texts = Texts::new(document, Self::main_layout(&state), &[place]);
                    texts.rects(&place).first().map(|r| r.page)
                };
                match page {
                    Some(page) => self.enter_story(story, page, Some(place.start)),
                    None => self.go_to_story(Some(story)),
                }
                if self.story() != Some(story) {
                    return false;
                }
            }
            _ => {}
        }
        self.move_to(place.start, false, false);
        self.select(Selection {
            anchor: place.start,
            focus: place.end,
        });
        true
    }

    /// Selects the text of the next comment after the selection, or of the
    /// one before it, as Next Comment and Previous Comment do in Pages
    /// 15.4, in the order of the pages. Gives the comment, none at the last
    /// or the first.
    pub fn go_to_comment(&self, forward: bool) -> Option<NodeId> {
        let place = {
            let state = self.state.borrow();
            self.next_place(&state, placed(Self::main_document(&state)), forward)
        }?;
        if !self.select_place(&place) {
            return None;
        }
        self.state.borrow_mut().reviewed = Some(place.item());
        Some(place.id)
    }

    /// Goes to the next comment or tracked change after the selection, or
    /// the one before it, as the arrows of the bar of comments and
    /// changes do (REV-02), and selects it.
    pub fn go_to_review_item(&self, forward: bool) -> Option<ReviewItem> {
        let place = {
            let state = self.state.borrow();
            let document = Self::main_document(&state);
            let mut places = placed(document);
            places.extend(placed_changes(document));
            self.next_place(&state, places, forward)
        }?;
        if !self.select_place(&place) {
            return None;
        }
        self.state.borrow_mut().reviewed = Some(place.item());
        Some(place.item())
    }

    /// The comment or change that the selection is on, with its place in
    /// the order of the comments and changes from 1, and how many there
    /// are, as the label of the bar of comments and changes says them
    /// (REV-02). The one that Previous or Next selected is the one, while
    /// it stays selected.
    pub fn review_position(&self) -> (Option<(usize, ReviewItem)>, usize) {
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        let story = state.story.as_ref().map(|m| m.story);
        let (start, end) = state.selection.ordered();
        let object = match state.objects[..] {
            [object] if story.is_none() => Some(object),
            _ => None,
        };
        let cell = state
            .cells
            .filter(|c| Some(c.id) == object)
            .map(|c| c.focus);
        let mut cache = state.review_order.borrow_mut();
        if cache.as_ref().is_none_or(|c| c.document != *document) {
            let mut places = placed(document);
            places.extend(placed_changes(document));
            let texts = Texts::new(document, Self::main_layout(&state), &places);
            let _work = desk_model::table::share_work();
            let mut keyed: Vec<(FullKey, Placed)> = places
                .into_iter()
                .map(|p| (texts.full_key(&p), p))
                .collect();
            keyed.sort_by_key(|a| a.0);
            *cache = Some(ReviewOrder {
                document: document.clone(),
                places: keyed.into_iter().map(|(_, p)| p).collect(),
            });
        }
        let ordered = &cache.as_ref().expect("just made").places;
        let on = |p: &Placed| match p.object {
            Some(o) => object == Some(o) && p.cell == cell,
            None => object.is_none() && p.story == story && p.start <= start && end <= p.end,
        };
        let remembered = state.reviewed.and_then(|item| {
            ordered
                .iter()
                .position(|p| p.item() == item && on(p) && (p.start, p.end) == (start, end))
        });
        let current = remembered
            .or_else(|| {
                ordered
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| on(p))
                    .max_by_key(|(_, p)| p.start)
                    .map(|(i, _)| i)
            })
            .map(|i| (i + 1, ordered[i].item()));
        (current, ordered.len())
    }

    /// The first place after the selection, or the last before it, in the
    /// order of the pages; of places on the same text, in the order of
    /// their kind and identifier, from the one Previous or Next selected
    /// last.
    fn next_place(
        &self,
        state: &super::State,
        places: Vec<Placed>,
        forward: bool,
    ) -> Option<Placed> {
        let document = Self::main_document(state);
        let (start, end) = state.selection.ordered();
        let story = state.story.as_ref().map(|m| m.story);
        // With the text being edited, where the selection is.
        let stories = places.iter().filter_map(|p| p.story).chain(story);
        let texts = Texts::with(document, Self::main_layout(state), stories);
        let object = match state.objects[..] {
            [object] if story.is_none() => Some(object),
            _ => None,
        };
        let cell = state
            .cells
            .filter(|c| Some(c.id) == object)
            .map(|c| c.focus);
        let selected = Placed {
            id: NodeId::new(1).expect("not zero"),
            story,
            start,
            end,
            object,
            cell,
            change: false,
        };
        // The item selected last, while it is still the selection, or else
        // the first that is the selection.
        let exact = |p: &&Placed| {
            (p.story, p.start, p.end, p.object, p.cell) == (story, start, end, object, cell)
        };
        let here = places
            .iter()
            .filter(exact)
            .find(|p| state.reviewed == Some(p.item()))
            .or_else(|| {
                places
                    .iter()
                    .filter(exact)
                    .min_by_key(|p| texts.full_key(p))
            })
            .map_or_else(
                || {
                    (
                        texts.key(&selected),
                        false,
                        NodeId::new(1).expect("not zero"),
                    )
                },
                |p| texts.full_key(p),
            );
        let keyed = places.into_iter().map(|p| (texts.full_key(&p), p));
        if forward {
            keyed
                .filter(|(k, _)| *k > here)
                .min_by(|a, b| a.0.cmp(&b.0))
        } else {
            keyed
                .filter(|(k, _)| *k < here)
                .max_by(|a, b| a.0.cmp(&b.0))
        }
        .map(|(_, p)| p)
    }

    /// The comment that the cursor or the selection is in, in the text
    /// being edited: the one that starts last of those around it, as it is
    /// on the fewest words.
    pub fn comment_at_cursor(&self) -> Option<NodeId> {
        let state = self.state.borrow();
        let story = state.story.as_ref().map(|m| m.story);
        let (start, end) = state.selection.ordered();
        let places = placed(Self::main_document(&state));
        // A selected object, its comment, or that of the cell selected in
        // it.
        if let [object] = state.objects[..] {
            let cell = state.cells.filter(|c| c.id == object).map(|c| c.focus);
            return places
                .iter()
                .find(|p| p.object == Some(object) && p.cell == cell)
                .map(|p| p.id);
        }
        places
            .into_iter()
            .filter(|p| p.object.is_none())
            .filter(|p| p.story == story && p.start <= start && end <= p.end)
            .max_by_key(|p| p.start)
            .map(|p| p.id)
    }

    /// Where the text under comments is on the pages, which shows it
    /// marked: that of the body and of the headers, footers, notes, text
    /// boxes and shapes, also while one of them is being edited.
    pub(super) fn comment_marks(&self) -> Vec<(NodeId, SpanRect)> {
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        let places = placed(document);
        let texts = Texts::new(document, Self::main_layout(&state), &places);
        places
            .iter()
            .filter(|p| p.object.is_none())
            .flat_map(|p| texts.rects(p).into_iter().map(|r| (p.id, r)))
            .collect()
    }

    /// The colours of the authors of comments, by identifier, for the
    /// author of this computer named `local` (W150): a function that
    /// gives the colour of a comment.
    pub fn comment_colors(
        &self,
        local: &str,
    ) -> impl Fn(NodeId) -> desk_model::AuthorColor + use<> {
        let document = self.document();
        let palette = document.author_palette_of(local, &crate::preferences::get().former_authors);
        let of: std::collections::HashMap<NodeId, desk_model::AuthorColor> = document
            .comments()
            .iter()
            .map(|comment| (comment.id, palette.color(&comment.author)))
            .collect();
        move |id| of.get(&id).copied().unwrap_or_default()
    }

    /// The colour of the author of this computer in the document (W150).
    pub fn local_author_color(&self) -> desk_model::AuthorColor {
        let local = crate::comments_page::author();
        self.document().author_color(&local, &local)
    }

    /// Chooses the colour of the author of this computer for the document,
    /// in its undo, as Author Color of Pages 15.4 (W150).
    pub fn set_local_author_color(&self, color: desk_model::AuthorColor) {
        if self.local_author_color() == color {
            return;
        }
        let local = crate::comments_page::author();
        self.format_edit("Author Color", |edit| edit.set_author_color(&local, color));
    }

    /// The marks of the comments in the left margin of the pages, as Pages
    /// 15.4 puts them: a small square beside the line where the text of
    /// each comment starts, side by side when several start on one line.
    /// Highlights have none (PXV-19).
    pub(super) fn comment_flags(&self) -> Vec<(NodeId, SpanRect)> {
        let state = self.state.borrow();
        flags(Self::main_document(&state), Self::main_layout(&state))
    }

    /// Calls `f` with the comment whose mark in the margin was clicked.
    pub fn connect_comment_clicked(&self, f: impl Fn(NodeId) + 'static) {
        self.state.borrow_mut().on_comment.push(std::rc::Rc::new(f));
    }

    /// A press on the mark of a comment in the margin: selects its text,
    /// and tells those who show its bubble.
    pub(super) fn press_comment_mark(&self, x: f64, y: f64) -> bool {
        let Some((page, px, py)) = self.view.page_at(x, y) else {
            return false;
        };
        // A little room around the square, which is small.
        let slop = 2.0;
        let hit = self.comment_flags().into_iter().find(|(_, f)| {
            f.page == page
                && f.x0 - slop <= px
                && px <= f.x1 + slop
                && f.y0 - slop <= py
                && py <= f.y1 + slop
        });
        let Some((id, _)) = hit else {
            return false;
        };
        self.select_comment(id);
        let callbacks = self.state.borrow().on_comment.clone();
        for callback in callbacks {
            callback(id);
        }
        true
    }

    /// Where the text of a comment starts on the pages: the page and the
    /// first piece of it, for its bubble to point at.
    pub fn comment_anchor(&self, id: NodeId) -> Option<SpanRect> {
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        let place = placed(document).into_iter().find(|p| p.id == id)?;
        let texts = Texts::new(document, Self::main_layout(&state), &[place]);
        texts.rects(&place).into_iter().next()
    }

    /// What a new comment would be on: the cell selected in a table, the
    /// first of the cells selected, or the object selected, or the text
    /// that [`TextEditor::comment_range`] gives.
    pub fn comment_target(&self) -> Option<CommentTarget> {
        if let Some((_, area)) = self.table_selection()
            && let Some(cells) = self.selected_cells()
        {
            return Some(CommentTarget::Cell(cells.id, area.row, area.col));
        }
        if self.story().is_none()
            && let [object] = self.selected_objects()[..]
        {
            return Some(CommentTarget::Object(object));
        }
        self.comment_range().map(|(s, e)| CommentTarget::Text(s, e))
    }

    /// Puts a new comment on its target, by `author`, now; nothing when
    /// its text is empty. Gives the new comment.
    pub fn add_comment_on(
        &self,
        target: CommentTarget,
        author: &str,
        text: &str,
    ) -> Option<NodeId> {
        if let CommentTarget::Text(start, end) = target {
            return self.add_comment((start, end), author, text);
        }
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
            added = Some(match target {
                CommentTarget::Cell(table, row, col) => {
                    edit.add_cell_comment(table, row, col, comment)?
                }
                CommentTarget::Object(object) => edit.add_object_comment(object, comment)?,
                CommentTarget::Text(..) => unreachable!("written above"),
            });
            Ok(())
        });
        added
    }

    /// The comment that a target already has: a cell has one, whose
    /// thread takes what more is said of it, as in Pages 15.4.
    pub fn target_comment(&self, target: CommentTarget) -> Option<NodeId> {
        let CommentTarget::Cell(table, row, col) = target else {
            return None;
        };
        self.document()
            .cell_comments()
            .into_iter()
            .find(|c| c.table == table && (c.row, c.col) == (row, col))
            .map(|c| c.id)
    }

    /// Writes the comment of each selected cell, as the Table pane does:
    /// a cell without one gets one by `author`, now, a cell with one has
    /// its text changed, and the text emptied deletes it (TAB-27).
    pub fn set_cell_comments(&self, author: &str, text: &str) {
        let Some((table, area)) = self.table_selection() else {
            return;
        };
        let Some(id) = self.selected_cells().map(|c| c.id).or_else(|| {
            let [id] = self.selected_objects()[..] else {
                return None;
            };
            Some(id)
        }) else {
            return;
        };
        let text = typed(text);
        let comments = self.document().cell_comments();
        let mut changes: Vec<(u32, u32, Comment)> = Vec::new();
        for (r, c) in area.cells() {
            if table.span(r, c).row != r || table.span(r, c).col != c {
                continue;
            }
            let old = comments
                .iter()
                .find(|p| p.table == id && (p.row, p.col) == (r, c))
                .and_then(|p| self.document().comments().get(p.id).cloned());
            match (old, text.is_empty()) {
                (Some(old), _) if old.text == text => {}
                (Some(old), _) => changes.push((
                    r,
                    c,
                    Comment {
                        text: text.clone(),
                        ..old
                    },
                )),
                (None, true) => {}
                (None, false) => changes.push((
                    r,
                    c,
                    Comment {
                        id: NodeId::new(1).expect("not zero"),
                        author: author.into(),
                        date: Some(crate::comments_page::now().into()),
                        text: text.clone(),
                        replies: Arc::new([]),
                    },
                )),
            }
        }
        if changes.is_empty() {
            return;
        }
        self.format_edit("Comment", |edit| {
            for (r, c, comment) in changes {
                let had = edit
                    .document()
                    .cell_comments()
                    .into_iter()
                    .find(|p| p.table == id && (p.row, p.col) == (r, c))
                    .map(|p| p.id);
                match had {
                    Some(old) if comment.text.is_empty() => edit.remove_comment(old)?,
                    Some(old) => edit.set_comment(old, Some(Comment { id: old, ..comment }))?,
                    None => {
                        edit.add_cell_comment(id, r, c, comment)?;
                    }
                }
            }
            Ok(())
        });
    }

    /// The text of the comment of the first of the cells selected in the
    /// selected table, for the Table pane.
    pub fn cell_comment_text(&self) -> Option<Arc<str>> {
        let (_, area) = self.table_selection()?;
        let [table] = self.selected_objects()[..] else {
            return None;
        };
        let document = self.document();
        let place = document
            .cell_comments()
            .into_iter()
            .find(|p| p.table == table && (p.row, p.col) == (area.row, area.col))?;
        document.comments().get(place.id).map(|c| c.text.clone())
    }

    /// Where the target of a new comment is on the pages, for its bubble
    /// to point at.
    pub fn target_anchor(&self, target: CommentTarget) -> Option<SpanRect> {
        match target {
            CommentTarget::Text(start, end) => self.range_anchor((start, end)),
            CommentTarget::Object(object) => {
                object_rect(Self::main_layout(&self.state.borrow()), object)
            }
            CommentTarget::Cell(table, row, col) => {
                cell_rect(Self::main_layout(&self.state.borrow()), table, (row, col))
            }
        }
    }

    /// Where the text between two spots of the text being edited starts
    /// on the pages, for the bubble of a new comment to point at.
    pub fn range_anchor(&self, (anchor, focus): (Spot, Spot)) -> Option<SpanRect> {
        let state = self.state.borrow();
        text_nav::selection_rects(&state.layout, &Selection { anchor, focus })
            .into_iter()
            .next()
    }
}

/// The marks of the comments in the left margin, each beside the line
/// where the text of its comment starts.
fn flags(document: &Document, main: &DocumentLayout) -> Vec<(NodeId, SpanRect)> {
    let places = placed(document);
    let texts = Texts::new(document, main, &places);
    let mut flags: Vec<(NodeId, SpanRect)> = Vec::new();
    for place in &places {
        if document
            .comments()
            .get(place.id)
            .is_none_or(Comment::is_highlight)
        {
            continue;
        }
        // On a cell, over the corner of yellow at its top right that
        // shows it (TAB-27).
        if let (Some(table), Some(cell)) = (place.object, place.cell) {
            if let Some(r) = cell_rect(main, table, cell) {
                flags.push((
                    place.id,
                    SpanRect {
                        page: r.page,
                        x0: r.x1 - FLAG,
                        y0: r.y0,
                        x1: r.x1,
                        y1: r.y0 + FLAG,
                    },
                ));
            }
            continue;
        }
        // On an object, at its top left corner, outside it.
        if let Some(object) = place.object {
            if let Some(r) = object_rect(main, object) {
                let beside = flags
                    .iter()
                    .filter(|(_, f)| f.page == r.page && f.y0 == r.y0)
                    .count();
                let x1 = r.x0 - 3.0 - beside as f64 * (FLAG + 3.0);
                flags.push((
                    place.id,
                    SpanRect {
                        page: r.page,
                        x0: x1 - FLAG,
                        y0: r.y0,
                        x1,
                        y1: r.y0 + FLAG,
                    },
                ));
            }
            continue;
        }
        let Some(layout) = texts.layout(place.story) else {
            continue;
        };
        let start = place.start;
        let found = layout.pages.iter().enumerate().find_map(|(page, p)| {
            p.lines
                .iter()
                .find(|l| {
                    l.paragraph == start.paragraph
                        && l.line.range.start <= start.offset
                        && start.offset <= l.line.range.end
                })
                .map(|l| (page, l))
        });
        let Some((page, line)) = found else { continue };
        let (top, bottom) = (f64::from(line.top()), f64::from(line.bottom()));
        let beside = flags
            .iter()
            .filter(|(_, f)| f.page == page && f.y0 >= top && f.y1 <= bottom)
            .count();
        let x1 = f64::from(line.origin.0) - FLAG_GAP - beside as f64 * (FLAG + 3.0);
        let y0 = (top + bottom - FLAG) / 2.0;
        flags.push((
            place.id,
            SpanRect {
                page,
                x0: x1 - FLAG,
                y0,
                x1,
                y1: y0 + FLAG,
            },
        ));
    }
    flags
}

/// The side of the mark of a comment in the margin, in points.
const FLAG: f64 = 7.0;
/// How far it is from the text, in points.
const FLAG_GAP: f64 = 9.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w149_a_comment_has_a_mark_in_the_margin_and_a_highlight_none() {
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            document
                .transact("Typing", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three").map(|_| ())
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            let one = editor
                .add_comment((Spot::new(0, 0), Spot::new(0, 3)), "Ana", "a")
                .unwrap();
            let two = editor
                .add_comment((Spot::new(0, 4), Spot::new(0, 7)), "Bo", "b")
                .unwrap();
            let flags = editor.comment_flags();
            assert_eq!(
                flags.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                [one, two]
            );
            // In the margin, left of the text, the second beside the first.
            let anchor = editor.comment_anchor(one).unwrap();
            let (first, second) = (flags[0].1, flags[1].1);
            assert!(first.x1 < anchor.x0 && second.x1 < first.x0);
            assert!(first.y0 >= anchor.y0 && first.y1 <= anchor.y1);
            assert!(
                editor
                    .range_anchor((Spot::new(0, 8), Spot::new(0, 13)))
                    .is_some_and(|r| r.x0 > anchor.x1)
            );
            // A highlight has no mark.
            editor.set_comment_text(two, "");
            let _ = editor.format_edit_quiet("Highlight", |edit| {
                let id = edit.document().body().get(0).unwrap().id();
                edit.add_comment(
                    Position::new(id, 8),
                    Position::new(id, 13),
                    Comment {
                        id,
                        author: "Ana".into(),
                        date: None,
                        text: "".into(),
                        replies: Arc::new([]),
                    },
                )
                .map(|_| ())
            });
            assert_eq!(editor.document().comments().len(), 2);
            assert_eq!(editor.comment_flags().len(), 1);
        });
    }

    #[test]
    fn w149_a_comment_is_written_in_a_header_and_found_from_the_body() {
        use desk_model::{MarginKind, MarginPages, MarginPlace, ParaAttrs, Run};
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let mut setup = *document.page_setup();
            setup.headers = true;
            let place = MarginPlace::new(MarginKind::Header, MarginPages::Default);
            document
                .transact("Setup", |edit| {
                    edit.set_page_setup(setup)?;
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three")?;
                    let style = edit.document().styles().default_paragraph();
                    let header = Paragraph::from_runs(
                        edit.new_id(),
                        style,
                        ParaAttrs::default(),
                        [Run::new("Header words")],
                    )?;
                    edit.set_header_footer(place, vec![header])
                })
                .unwrap();
            let story = Story::Margin {
                section: None,
                place,
            };
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            let body = editor
                .add_comment((Spot::new(0, 4), Spot::new(0, 7)), "Ana", "In the body")
                .unwrap();
            // In the header, on the word at the cursor.
            editor.enter_story(story, 0, Some(Spot::new(0, 1)));
            let range = editor.comment_range().unwrap();
            assert_eq!(range, (Spot::new(0, 0), Spot::new(0, 6)));
            let header = editor.add_comment(range, "Bo", "In the header").unwrap();
            assert_eq!(editor.comment_at_cursor(), Some(header));
            editor.leave_story(None);
            assert_eq!(editor.story(), None);
            let document = editor.document();
            assert_eq!(
                document.comments().get(header).unwrap().text.as_ref(),
                "In the header"
            );
            // Listed by page, the header first, with the text each is on.
            assert_eq!(
                editor.comments_in_order(),
                [(header, "Header".to_owned()), (body, "two".to_owned())]
            );
            // Marked where the header is, above the body, with its mark in
            // the margin.
            let anchor = editor.comment_anchor(header).unwrap();
            let top = f64::from(crate::lay_out(&document).pages[0].body.0);
            assert!(anchor.y1 <= top);
            assert_eq!(editor.comment_marks().len(), 2);
            assert_eq!(editor.comment_flags().len(), 2);
            // Previous Comment from the body goes into the header, and Next
            // Comment back to the body.
            editor.select(Selection {
                anchor: Spot::new(0, 4),
                focus: Spot::new(0, 7),
            });
            assert_eq!(editor.go_to_comment(false), Some(header));
            assert_eq!(editor.story(), Some(story));
            assert_eq!(editor.selection().ordered(), range);
            assert_eq!(editor.go_to_comment(true), Some(body));
            assert_eq!(editor.story(), None);
            // From the cursor at the start of the body, too.
            editor.select(Selection::cursor(Spot::new(0, 0)));
            assert_eq!(editor.go_to_comment(true), Some(body));
            // Deleted from the body, it goes from the header.
            editor.delete_comment(header);
            let document = editor.document();
            assert!(document.comments().get(header).is_none());
            assert!(
                document.headers_footers().get(place)[0]
                    .runs()
                    .all(|r| r.attrs.comments.is_none())
            );
            assert_eq!(editor.comments_in_order().len(), 1);
        });
    }

    #[test]
    fn w059_a_highlight_is_on_the_selection_and_goes_with_the_comments_on_it() {
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            document
                .transact("Typing", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three four").map(|_| ())
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            // Nothing selected: no highlight, as Insert > Highlight is off.
            editor.select(Selection::cursor(Spot::new(0, 1)));
            assert_eq!(editor.add_highlight("Ana"), None);
            editor.select(Selection {
                anchor: Spot::new(0, 4),
                focus: Spot::new(0, 7),
            });
            let highlight = editor.add_highlight("Ana").unwrap();
            let made = editor
                .document()
                .comments()
                .get(highlight)
                .cloned()
                .unwrap();
            assert!(made.is_highlight());
            assert_eq!(&*made.author, "Ana");
            assert_eq!(editor.undo_name().as_deref(), Some("Highlight"));
            // It has no mark in the margin, and is listed with the
            // comments; given text, it is a comment.
            assert!(editor.comment_flags().is_empty());
            assert_eq!(editor.comments_in_order().len(), 1);
            let comment = editor
                .add_comment((Spot::new(0, 14), Spot::new(0, 18)), "Bo", "four")
                .unwrap();
            // Remove Highlights and Comments takes those that the
            // selection meets, at once.
            editor.select(Selection {
                anchor: Spot::new(0, 0),
                focus: Spot::new(0, 5),
            });
            assert_eq!(editor.comments_in_selection(), [highlight]);
            editor.select(Selection::cursor(Spot::new(0, 16)));
            assert_eq!(editor.comments_in_selection(), [comment]);
            // A cursor at the end of a highlight is not in it.
            editor.select(Selection::cursor(Spot::new(0, 7)));
            assert!(editor.comments_in_selection().is_empty());
            editor.select(Selection {
                anchor: Spot::new(0, 18),
                focus: Spot::new(0, 0),
            });
            assert_eq!(editor.remove_comments_in_selection(), 2);
            assert!(editor.document().comments().is_empty());
            editor.select(Selection::cursor(Spot::new(0, 9)));
            assert_eq!(editor.remove_comments_in_selection(), 0);
            editor.undo();
            assert_eq!(editor.document().comments().len(), 2);
        });
    }

    #[test]
    fn w059_a_selected_object_takes_no_highlight_and_keeps_the_comments_of_the_text() {
        use desk_model::{Frame, Object, Shape};
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let shape = document
                .transact("Shape", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three")?;
                    let object = Object::new(
                        edit.new_id(),
                        ObjectKind::Shape(Shape::Rectangle),
                        Frame::from_pt(200.0, 300.0, 100.0, 50.0),
                    );
                    let id = object.id;
                    edit.add_object(object)?;
                    Ok(id)
                })
                .unwrap()
                .0;
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            editor
                .add_comment((Spot::new(0, 4), Spot::new(0, 7)), "Ana", "two")
                .unwrap();
            editor.select(Selection {
                anchor: Spot::new(0, 4),
                focus: Spot::new(0, 7),
            });
            // The shape selected over the text selected: neither acts on
            // the text, which does not show as selected.
            editor.select_objects(vec![shape]);
            assert_eq!(editor.add_highlight("Ana"), None);
            assert_eq!(editor.remove_comments_in_selection(), 0);
            assert_eq!(editor.document().comments().len(), 1);
        });
    }

    #[test]
    fn w150_comments_take_the_colours_of_their_authors() {
        use desk_model::AuthorColor;
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            document
                .transact("Typing", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three").map(|_| ())
                })
                .unwrap();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            let local = crate::comments_page::author();
            let mine = editor
                .add_comment((Spot::new(0, 0), Spot::new(0, 3)), &local, "a")
                .unwrap();
            let theirs = editor
                .add_comment((Spot::new(0, 4), Spot::new(0, 7)), "Someone Else", "b")
                .unwrap();
            let color = editor.comment_colors(&local);
            assert_eq!(color(mine), AuthorColor::Yellow);
            assert_eq!(color(theirs), AuthorColor::Green);
            // Choosing a colour is a step of the document, which Undo takes
            // back.
            editor.set_local_author_color(AuthorColor::Teal);
            assert_eq!(editor.local_author_color(), AuthorColor::Teal);
            assert_eq!(editor.undo_name().as_deref(), Some("Author Color"));
            let color = editor.comment_colors(&local);
            assert_eq!(color(mine), AuthorColor::Teal);
            assert_eq!(color(theirs), AuthorColor::Yellow);
            assert_eq!(editor.comment_marks().len(), 2);
            editor.undo();
            assert_eq!(editor.local_author_color(), AuthorColor::Yellow);
        });
    }

    #[test]
    fn w149_a_selected_object_takes_a_comment() {
        use desk_model::{Frame, Object, Shape};
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let shape = document
                .transact("Shape", |edit| {
                    let start = edit.document().start();
                    edit.insert_text(start, "one two three")?;
                    let object = Object::new(
                        edit.new_id(),
                        ObjectKind::Shape(Shape::Rectangle),
                        Frame::from_pt(200.0, 300.0, 100.0, 50.0),
                    );
                    let id = object.id;
                    edit.add_object(object)?;
                    Ok(id)
                })
                .unwrap()
                .0;
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            editor.select_objects(vec![shape]);
            let target = editor.comment_target().unwrap();
            assert_eq!(target, CommentTarget::Object(shape));
            let at = editor.target_anchor(target).unwrap();
            assert_eq!((at.x0, at.y0), (200.0, 300.0));
            let id = editor
                .add_comment_on(target, "Ana", "On the shape")
                .unwrap();
            assert_eq!(editor.document().object_comments(), [(id, shape)]);
            assert_eq!(editor.comment_at_cursor(), Some(id));
            assert_eq!(editor.comments_in_order(), [(id, crate::i18n::tr("Shape"))]);
            // Its mark is at the top left corner of the object, not on text.
            assert!(editor.comment_marks().is_empty());
            let flags = editor.comment_flags();
            assert_eq!(flags.len(), 1);
            assert!(flags[0].1.x1 <= 200.0 && flags[0].1.y0 == 300.0);
            assert_eq!(editor.comment_anchor(id), Some(at));
            // Going to it selects the object.
            editor.select_objects(Vec::new());
            assert!(editor.select_comment(id));
            assert_eq!(editor.selected_objects(), [shape]);
            editor.delete_comment(id);
            assert!(editor.document().object_comments().is_empty());
            assert!(editor.comments_in_order().is_empty());
        });
    }

    #[test]
    fn w149_a_selected_cell_takes_a_comment_and_has_one() {
        use desk_model::{Anchor, Frame, Object};
        crate::with_gtk(|| {
            let mut document = Document::new_word_processing();
            let table = document
                .transact("Table", |edit| {
                    let mut table = desk_model::Table::new(3, 3);
                    table.set_content(1, 1, desk_model::table::Content::Text("Norte".into()));
                    let mut object = Object::new(
                        edit.new_id(),
                        ObjectKind::Table(Arc::new(table)),
                        Frame::from_pt(100.0, 200.0, 240.0, 60.0),
                    );
                    object.anchor = Anchor::Page(0);
                    let id = object.id;
                    edit.add_object(object)?;
                    Ok(id)
                })
                .unwrap()
                .0;
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            editor.select_objects(vec![table]);
            editor.select_cell(table, (1, 1));
            let target = editor.comment_target().unwrap();
            assert_eq!(target, CommentTarget::Cell(table, 1, 1));
            assert_eq!(editor.target_comment(target), None);
            let at = editor.target_anchor(target).unwrap();
            assert!(at.x0 > 100.0 && at.y0 > 200.0);
            let id = editor.add_comment_on(target, "Ana", "Conferir").unwrap();
            let on = desk_model::CellComment {
                id,
                table,
                row: 1,
                col: 1,
            };
            assert_eq!(editor.document().cell_comments(), [on]);
            // A cell has one comment, which Comment goes to.
            assert_eq!(editor.target_comment(target), Some(id));
            assert_eq!(editor.comment_at_cursor(), Some(id));
            assert_eq!(editor.comments_in_order(), [(id, "Norte".to_owned())]);
            // Its mark is over the corner of the cell, not on text.
            assert!(editor.comment_marks().is_empty());
            let flags = editor.comment_flags();
            assert_eq!(flags.len(), 1);
            assert_eq!((flags[0].1.x1, flags[0].1.y0), (at.x1, at.y0));
            assert_eq!(editor.comment_anchor(id), Some(at));
            // Going to it selects the cell.
            editor.select_objects(Vec::new());
            assert!(editor.select_comment(id));
            assert_eq!(editor.selected_cells().map(|c| c.focus), Some((1, 1)));
            // The Table pane writes it, and emptied, it is deleted.
            assert_eq!(editor.cell_comment_text().as_deref(), Some("Conferir"));
            editor.set_cell_comments("Ana", "Conferido");
            assert_eq!(editor.cell_comment_text().as_deref(), Some("Conferido"));
            assert_eq!(editor.document().cell_comments(), [on]);
            editor.set_cell_comments("Ana", "");
            assert!(editor.document().cell_comments().is_empty());
            assert!(editor.document().comments().is_empty());
            // Written in the pane on a cell without one, it is a new one.
            editor.set_cell_comments("Rui", "Novo");
            let found = editor.document().cell_comments();
            assert_eq!(found.len(), 1);
            let comment = editor
                .document()
                .comments()
                .get(found[0].id)
                .cloned()
                .unwrap();
            assert_eq!((&*comment.author, &*comment.text), ("Rui", "Novo"));
        });
    }
}
