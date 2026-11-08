// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Headers, footers and notes edited on the page, as Pages does (EST-12,
//! EST-13, EST-15).
//!
//! A click in the header or footer of a page, or on a note, puts the
//! cursor in that text. While it is there, the editor edits a copy of the
//! document whose body holds the text, with every tool of the body, and
//! writes each change back to the document as the new text of the header,
//! footer or note. The copy has its own history, so Undo goes back one
//! change at a time; in the body, the whole visit is one action. A click
//! in the body, or Esc, takes the cursor back there.

use desk_layout::DocumentLayout;
use desk_model::{
    Alignment, Document, Editor, Field, MarginKind, NodeId, Object, ObjectKind, ParaAttrs,
    Paragraph, Story, StyleFamily,
};

use super::{Selection, Spot, State, TextEditor};

/// A header, footer or note being edited on the page.
pub(super) struct StoryMode {
    /// The text being edited.
    pub story: Story,
    /// The page where it is edited: a header shows on many.
    pub page: usize,
    /// The editor of the document itself, while the editor of the window
    /// edits the copy.
    pub main: Editor,
    /// The document laid out, as the pages show it.
    pub main_layout: DocumentLayout,
    /// How many actions the document had when the visit started.
    pub depth: usize,
    /// The selection of the body to go back to.
    pub body: Selection,
    /// The history of the selections of the body, kept for the return.
    pub undo: Vec<(Selection, Selection, u64)>,
    pub redo: Vec<(Selection, Selection, u64)>,
}

impl StoryMode {
    /// The name of the visit in the history of the body.
    fn action_name(&self) -> &'static str {
        match self.story {
            Story::Note(_) => "Edit Note",
            Story::Margin { place, .. } => match place.kind {
                MarginKind::Header => "Edit Header",
                MarginKind::Footer => "Edit Footer",
            },
            Story::TextBox(_) => "Edit Text Box",
        }
    }
}

/// The header, footer or note at a point of a page of the document as
/// laid out, if any: the lines of a note, or the space above and below
/// the body of a page that shows headers or footers.
pub fn story_at(layout: &DocumentLayout, page: usize, x: f64, y: f64) -> Option<Story> {
    let p = layout.pages.get(page)?;
    if let Some(index) = desk_layout::object_at(&p.objects, x as f32, y as f32, 0.0, true)
        && p.objects[index].object.holds_text()
        // The text of the masters is not typed in on the pages.
        && !p.objects[index].object.anchor.is_master()
    {
        return Some(Story::TextBox(text_box_head(
            layout,
            p.objects[index].object.id,
        )));
    }
    for line in &p.note_lines {
        let (top, bottom) = (f64::from(line.top()), f64::from(line.bottom()));
        if (top - 1.0..=bottom + 1.0).contains(&y)
            && let Some(&id) = layout.notes.get(line.paragraph)
        {
            return Some(Story::Note(id));
        }
    }
    if p.blank {
        return None;
    }
    let kind = if y < f64::from(p.body.0) && layout.setup.headers {
        MarginKind::Header
    } else if y > f64::from(p.body.1) && layout.setup.footers {
        MarginKind::Footer
    } else {
        return None;
    };
    let (place, section) = desk_layout::margin_of_page(layout, page, kind);
    Some(Story::Margin { section, place })
}

/// The document laid out as the editor of a story sees it: the pages of
/// the document with only the lines of the story, whose paragraphs are
/// the paragraphs of the story, as the body of `copy` holds them.
pub fn story_layout(
    main: &DocumentLayout,
    story: Story,
    page: usize,
    copy: &Document,
) -> DocumentLayout {
    story_layout_on(main, story, &[page], copy)
}

/// The document laid out with the lines of a story on every page that
/// shows it, to mark its mistakes while the body is edited.
fn story_layout_everywhere(main: &DocumentLayout, story: Story, copy: &Document) -> DocumentLayout {
    let pages: Vec<usize> = match story {
        Story::Note(_) => Vec::new(),
        Story::TextBox(id) => text_box_page(main, id).into_iter().collect(),
        Story::Margin { section, place } => (0..main.pages.len())
            .filter(|&i| {
                let page = &main.pages[i];
                let shown = match place.kind {
                    MarginKind::Header => main.setup.headers,
                    MarginKind::Footer => main.setup.footers,
                };
                shown
                    && !page.blank
                    && desk_layout::margin_of_page(main, i, place.kind) == (place, section)
            })
            .collect(),
    };
    story_layout_on(main, story, &pages, copy)
}

fn story_layout_on(
    main: &DocumentLayout,
    story: Story,
    pages: &[usize],
    copy: &Document,
) -> DocumentLayout {
    let mut layout = main.clone();
    layout.field_values = Vec::new();
    for p in &mut layout.pages {
        p.lines.clear();
        p.blocks.clear();
    }
    match story {
        Story::Note(id) => {
            for (index, p) in main.pages.iter().enumerate() {
                for (i, line) in p.note_lines.iter().enumerate() {
                    if main.notes.get(line.paragraph) == Some(&id) {
                        let mut line = line.clone();
                        line.paragraph = p.note_parts.get(i).copied().unwrap_or(0);
                        layout.pages[index].lines.push(line);
                    }
                }
            }
        }
        Story::Margin { place, .. } => {
            let paragraphs: Vec<Paragraph> = copy.body().iter().cloned().collect();
            for &page in pages.iter().filter(|&&p| p < main.pages.len()) {
                let lines = crate::with_text_system(|system| {
                    desk_layout::margin_lines(copy, main, page, place.kind, &paragraphs, system)
                });
                layout.pages[page].lines = lines;
            }
        }
        Story::TextBox(id) => {
            // Every box linked with it shows its lines, wherever it is.
            let _ = pages;
            let paragraphs: Vec<Paragraph> = copy.body().iter().cloned().collect();
            let chain = copy.text_box_chain(copy.text_box_head(id));
            let boxes: Vec<(usize, &desk_layout::PlacedObject)> = chain
                .iter()
                .filter_map(|b| {
                    main.pages.iter().enumerate().find_map(|(p, page)| {
                        page.objects
                            .iter()
                            .find(|o| o.object.id == *b)
                            .map(|o| (p, o))
                    })
                })
                .collect();
            let refs: Vec<&desk_layout::PlacedObject> = boxes.iter().map(|(_, o)| *o).collect();
            let flowed = crate::with_text_system(|system| {
                desk_layout::flow_text_boxes(copy, &refs, &paragraphs, system)
            });
            for ((page, _), lines) in boxes.iter().zip(flowed) {
                layout.pages[*page].lines.extend(lines);
            }
        }
    }
    layout
}

/// The first of the text boxes linked with one, as the layout has them.
fn text_box_head(layout: &DocumentLayout, id: NodeId) -> NodeId {
    let before = |of: NodeId| {
        layout
            .pages
            .iter()
            .flat_map(|page| &page.objects)
            .find(|o| matches!(&o.object.kind, ObjectKind::TextBox(t) if t.next == Some(of)))
            .map(|o| o.object.id)
    };
    let mut head = id;
    let mut seen = vec![id];
    while let Some(earlier) = before(head) {
        if seen.contains(&earlier) {
            break;
        }
        seen.push(earlier);
        head = earlier;
    }
    head
}

/// The page a text box is on.
fn text_box_page(layout: &DocumentLayout, id: NodeId) -> Option<usize> {
    layout
        .pages
        .iter()
        .position(|p| p.objects.iter().any(|o| o.object.id == id))
}

/// The marks under the mistakes of the headers, footers and notes other
/// than the one being edited, which show as those of the body do
/// (TXT-10, TXT-11).
pub(super) fn story_squiggles(
    document: &Document,
    layout: &DocumentLayout,
    editing: Option<Story>,
    spelling: bool,
    proofreading: bool,
) -> Vec<crate::view::Squiggle> {
    let mut out = Vec::new();
    if !spelling && !proofreading {
        return out;
    }
    for (story, _) in story_pages(layout) {
        if Some(story) == editing {
            continue;
        }
        let paragraphs = document.story_paragraphs(story);
        let mistakes: Vec<(usize, Vec<crate::spelling::Mistake>)> = paragraphs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    i,
                    crate::spelling::mistakes(document.styles(), p, spelling, proofreading),
                )
            })
            .filter(|(_, m)| !m.is_empty())
            .collect();
        if mistakes.is_empty() {
            continue;
        }
        let copy = document.with_body(paragraphs);
        let lines = story_layout_everywhere(layout, story, &copy);
        for (index, list) in mistakes {
            for mistake in list {
                let selection = Selection {
                    anchor: Spot::new(index, mistake.range.start),
                    focus: Spot::new(index, mistake.range.end),
                };
                let proofreading =
                    matches!(mistake.kind, crate::spelling::MistakeKind::Proofreading(_));
                for span in crate::text_nav::selection_rects(&lines, &selection) {
                    out.push(crate::view::Squiggle { span, proofreading });
                }
            }
        }
    }
    out
}

/// The headers, footers and notes that the pages show, each with the
/// first page it is on, in the order of the pages: on each page, its
/// header, its notes and its footer.
pub fn story_pages(layout: &DocumentLayout) -> Vec<(Story, usize)> {
    let mut out: Vec<(Story, usize)> = Vec::new();
    let mut add = |story: Story, page: usize| {
        if !out.iter().any(|(s, _)| *s == story) {
            out.push((story, page));
        }
    };
    for (index, page) in layout.pages.iter().enumerate() {
        let margin = |kind| {
            let (place, section) = desk_layout::margin_of_page(layout, index, kind);
            Story::Margin { section, place }
        };
        if !page.blank && layout.setup.headers {
            add(margin(MarginKind::Header), index);
        }
        for line in &page.note_lines {
            if let Some(&id) = layout.notes.get(line.paragraph) {
                add(Story::Note(id), index);
            }
        }
        if !page.blank && layout.setup.footers {
            add(margin(MarginKind::Footer), index);
        }
        for object in &page.objects {
            if object.object.text_box().is_some() {
                add(
                    Story::TextBox(text_box_head(layout, object.object.id)),
                    index,
                );
            }
        }
    }
    out
}

/// The copy of a document whose body holds the text of a story; an empty
/// header or footer gets an empty paragraph of the style of headers.
fn story_copy(document: &Document, story: Story) -> Document {
    let paragraphs = document.story_paragraphs(story);
    if !paragraphs.is_empty() {
        return document.with_body(paragraphs);
    }
    let mut document = document.clone();
    let styles = document.styles();
    if let Story::TextBox(_) = story {
        // The first text of a shape, centered as in Pages.
        let style = styles.default_paragraph();
        let attrs = ParaAttrs {
            alignment: Some(Alignment::Center),
            ..ParaAttrs::default()
        };
        let id = document.new_id();
        let paragraph = Paragraph::from_runs(id, style, attrs, [])
            .unwrap_or_else(|_| Paragraph::new(id, style));
        return document.with_body(vec![paragraph]);
    }
    let style = ["Header & Footer", "Header", "Footer"]
        .iter()
        .find_map(|name| styles.by_name(StyleFamily::Paragraph, name))
        .map_or(styles.default_paragraph(), |s| s.id());
    let id = document.new_id();
    document.with_body(vec![Paragraph::new(id, style)])
}

impl TextEditor {
    /// The header, footer or note being edited, if any.
    pub fn story(&self) -> Option<Story> {
        self.state.borrow().story.as_ref().map(|m| m.story)
    }

    /// The document laid out, as the pages show it.
    pub(super) fn main_layout(state: &State) -> &DocumentLayout {
        match &state.story {
            Some(mode) => &mode.main_layout,
            None => &state.layout,
        }
    }

    /// Whether a story is still in the document.
    pub(super) fn story_exists(&self, story: Story) -> bool {
        story_copy_ok(Self::main_document(&self.state.borrow()), story)
    }

    /// The place of a header, footer or note in the order that searches
    /// go through them, after the body.
    pub fn story_rank(&self, story: Option<Story>) -> usize {
        let Some(story) = story else {
            return 0;
        };
        let state = self.state.borrow();
        story_pages(Self::main_layout(&state))
            .iter()
            .position(|(s, _)| *s == story)
            .map_or(usize::MAX, |at| at + 1)
    }

    /// Puts the cursor in a header, footer or note, as on the first page
    /// that shows it, or in the body.
    pub(super) fn go_to_story(&self, story: Option<Story>) {
        if story == self.story() {
            return;
        }
        match story {
            None => self.leave_story(None),
            Some(story) => {
                let page = {
                    let state = self.state.borrow();
                    story_pages(Self::main_layout(&state))
                        .into_iter()
                        .find(|(s, _)| *s == story)
                        .map_or(0, |(_, page)| page)
                };
                self.enter_story(story, page, None);
            }
        }
    }

    /// Enters the header, footer or note at a point of the view, or
    /// leaves the one being edited for the body.
    pub(super) fn press_story(&self, x: f64, y: f64) {
        let Some((page, px, py)) = self.view.page_at(x, y) else {
            return;
        };
        let (target, current) = {
            let state = self.state.borrow();
            let target = story_at(Self::main_layout(&state), page, px, py);
            (target, state.story.as_ref().map(|m| (m.story, m.page)))
        };
        match target {
            Some(story) if current != Some((story, page)) => self.enter_story(story, page, None),
            None if current.is_some() => self.leave_story(None),
            _ => {}
        }
    }

    /// Goes from the mark of a note in the body to the note, or from the
    /// mark at the start of a note to the body. False when there is no
    /// mark at the spot.
    pub(super) fn double_click_mark(&self, spot: Spot) -> bool {
        match self.story() {
            None => {
                let (note, page) = {
                    let state = self.state.borrow();
                    let Some(note) = mark_near(state.editor.document(), spot) else {
                        return false;
                    };
                    let page = crate::text_nav::caret(&state.layout, spot).map_or(0, |c| c.page);
                    (note, page)
                };
                self.enter_story(Story::Note(note), page, None);
                true
            }
            Some(Story::Note(id)) => {
                let at_mark = mark_near(self.state.borrow().editor.document(), spot) == Some(id);
                if !at_mark {
                    return false;
                }
                let back = mark_of(Self::main_document(&self.state.borrow()), id);
                self.leave_story(back.map(Selection::cursor));
                true
            }
            Some(Story::Margin { .. } | Story::TextBox(_)) => false,
        }
    }

    /// The document as it is now, with the text being edited in it.
    pub(super) fn main_document(state: &State) -> &Document {
        match &state.story {
            Some(mode) => mode.main.document(),
            None => state.editor.document(),
        }
    }

    /// Puts the cursor in a header, footer or note, as edited on page
    /// `page`, at a spot of its text, or at its end.
    pub(super) fn enter_story(&self, story: Story, page: usize, spot: Option<Spot>) {
        let current = self.story();
        if current == Some(story) {
            // The same header on another page.
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            if let Some(mode) = state.story.as_mut()
                && mode.page != page
            {
                mode.page = page;
                state.layout =
                    story_layout(&mode.main_layout, story, page, state.editor.document());
            }
        } else {
            if current.is_some() {
                self.leave_story_quietly();
            }
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            if story_copy_ok(state.editor.document(), story) {
                let copy = story_copy(state.editor.document(), story);
                let layout = story_layout(&state.layout, story, page, &copy);
                let main = std::mem::replace(&mut state.editor, Editor::new(copy));
                let main_layout = std::mem::replace(&mut state.layout, layout);
                let depth = main.undo_depth();
                state.story = Some(StoryMode {
                    story,
                    page,
                    main,
                    main_layout,
                    depth,
                    body: state.selection,
                    undo: std::mem::take(&mut state.undo),
                    redo: std::mem::take(&mut state.redo),
                });
                state.typing = false;
                state.others.clear();
                state.spelling_scope = None;
            } else {
                return;
            }
        }
        let spot = spot.unwrap_or_else(|| {
            let state = self.state.borrow();
            let body = state.editor.document().body();
            let last = body.len().saturating_sub(1);
            Spot::new(last, body.get(last).map_or(0, Paragraph::len))
        });
        self.show_pages();
        self.select(Selection::cursor(spot));
        self.update_squiggles();
        self.changed();
    }

    /// Takes the cursor back to the body, where it was before, or to a
    /// selection of the body.
    pub fn leave_story(&self, to: Option<Selection>) {
        if let Some(body) = self.leave_story_quietly() {
            self.show_pages();
            self.select(to.unwrap_or(body));
            self.update_squiggles();
            self.changed();
        }
    }

    /// Ends the visit to a story, and gives the selection of the body it
    /// started from.
    fn leave_story_quietly(&self) -> Option<Selection> {
        let mut guard = self.state.borrow_mut();
        let state = &mut *guard;
        let mode = state.story.take()?;
        state.editor = mode.main;
        state.layout = mode.main_layout;
        state.undo = mode.undo;
        state.redo = mode.redo;
        state.typing = false;
        state.others.clear();
        state.spelling_scope = None;
        if state.editor.undo_depth() > mode.depth {
            // The whole visit is one action of the body.
            let action = state.next_action;
            state.next_action += 1;
            state.undo.push((mode.body, mode.body, action));
            state.redo.clear();
        }
        Some(mode.body)
    }

    /// Writes the text of the story as the copy has it back to the
    /// document, and lays out both again. Ends the visit when the story is
    /// gone, as a note whose mark was deleted.
    pub(super) fn sync_story(&self) -> bool {
        let mut guard = self.state.borrow_mut();
        let state = &mut *guard;
        let Some(mode) = state.story.as_mut() else {
            return false;
        };
        let copy = state.editor.document().clone();
        let paragraphs: Vec<Paragraph> = copy.body().iter().cloned().collect();
        let story = mode.story;
        let write = |edit: &mut desk_model::Edit| {
            edit.adopt_document_settings(&copy)?;
            let before = edit.document().story_paragraphs(story);
            edit.set_story(story, paragraphs.clone())?;
            // The objects in its lines (EST-12, EST-13).
            edit.adopt_story_objects(&copy, &before)
        };
        let name = mode.action_name();
        let result = if mode.main.undo_depth() > mode.depth {
            mode.main.edit_continuing(name, write).map(|_| ())
        } else {
            mode.main.edit(name, write)
        };
        if let Err(error) = result {
            gtk::glib::g_warning!("desk", "cannot write the text back: {error}");
        }
        mode.main_layout = crate::lay_out(mode.main.document());
        state.layout = story_layout(&mode.main_layout, story, mode.page, &copy);
        true
    }
}

/// The note whose mark is the character after or before a spot.
fn mark_near(document: &Document, spot: Spot) -> Option<NodeId> {
    let paragraph = document.body().get(spot.paragraph)?;
    let mut offset = 0;
    for run in paragraph.runs() {
        let end = offset + run.len();
        if let Some(Some(Field::NoteMark { note })) = &run.attrs.field
            && (offset..=end).contains(&spot.offset)
            && document.notes().get(*note).is_some()
        {
            return Some(*note);
        }
        offset = end;
    }
    None
}

/// The spot after the mark of a note in the body.
fn mark_of(document: &Document, note: NodeId) -> Option<Spot> {
    for (index, paragraph) in document.body().iter().enumerate() {
        let mut offset = 0;
        for run in paragraph.runs() {
            offset += run.len();
            if run.attrs.field == Some(Some(Field::NoteMark { note })) {
                return Some(Spot::new(index, offset));
            }
        }
    }
    None
}

/// Whether a story can be edited: a note that exists, or any header or
/// footer.
fn story_copy_ok(document: &Document, story: Story) -> bool {
    match story {
        Story::Note(id) => document.notes().get(id).is_some(),
        Story::Margin { .. } => true,
        Story::TextBox(id) => document.objects().get(id).is_some_and(Object::holds_text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{MarginPages, MarginPlace, Position};

    #[test]
    fn est_12_a_click_above_the_body_is_in_the_header() {
        let mut document = Document::new_word_processing();
        let mut setup = *document.page_setup();
        setup.headers = true;
        setup.footers = true;
        document
            .transact("Setup", |edit| {
                edit.set_page_setup(setup)?;
                let first = edit.document().body().get(0).unwrap().id();
                edit.insert_text(Position::new(first, 0), "Body")?;
                Ok(())
            })
            .unwrap();
        let layout = crate::lay_out(&document);
        let page = &layout.pages[0];
        let header = MarginPlace::new(MarginKind::Header, MarginPages::Default);
        assert_eq!(
            story_at(&layout, 0, 100.0, f64::from(page.body.0) - 5.0),
            Some(Story::Margin {
                section: None,
                place: header
            })
        );
        assert!(matches!(
            story_at(&layout, 0, 100.0, f64::from(page.body.1) + 5.0),
            Some(Story::Margin { place, .. }) if place.kind == MarginKind::Footer
        ));
        assert_eq!(
            story_at(&layout, 0, 100.0, f64::from(page.body.0) + 5.0),
            None
        );
        // The copy of an empty header has one empty paragraph, laid out
        // where the header goes.
        let story = Story::Margin {
            section: None,
            place: header,
        };
        let copy = story_copy(&document, story);
        assert_eq!(copy.body().len(), 1);
        let edited = story_layout(&layout, story, 0, &copy);
        assert_eq!(edited.pages[0].lines.len(), 1);
        let line = &edited.pages[0].lines[0];
        assert!(line.top() < page.body.0);
        assert_eq!(line.paragraph, 0);
    }

    #[test]
    fn obj_13_a_text_box_is_edited_in_a_copy_and_written_back() {
        use desk_model::{Frame, Object, Run, TextBox};
        use std::sync::Arc;
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Insert Text Box", |edit| {
                let style = edit.document().styles().default_paragraph();
                let paragraph = Paragraph::from_runs(
                    edit.new_id(),
                    style,
                    desk_model::ParaAttrs::default(),
                    [Run::new("Box")],
                )?;
                let id = edit.new_id();
                edit.add_object(Object::new(
                    id,
                    ObjectKind::TextBox(TextBox::new(
                        Arc::from([paragraph]),
                        desk_model::Length::from_points(4),
                    )),
                    Frame::from_pt(300.0, 400.0, 150.0, 60.0),
                ))?;
                Ok(id)
            })
            .unwrap();
        let document = editor.document().clone();
        let story = Story::TextBox(id);
        let layout = crate::lay_out(&document);
        assert_eq!(story_at(&layout, 0, 320.0, 420.0), Some(story));
        assert_eq!(story_at(&layout, 0, 100.0, 420.0), None);
        assert!(story_pages(&layout).contains(&(story, 0)));
        // The copy holds the text of the box in its body, laid out in
        // the box, inside its inset.
        let copy = story_copy(&document, story);
        assert_eq!(copy.body().get(0).unwrap().text(), "Box");
        let edited = story_layout(&layout, story, 0, &copy);
        let line = &edited.pages[0].lines[0];
        assert_eq!(edited.pages[0].lines.len(), 1);
        assert_eq!(line.origin, (304.0, 404.0));
        // Text typed in the copy goes back to the box.
        let mut sub = Editor::new(copy);
        let end = {
            let p = sub.document().body().get(0).unwrap();
            desk_model::Position::new(p.id(), p.len())
        };
        sub.edit("Typing", |edit| edit.insert_text(end, "ed").map(|_| ()))
            .unwrap();
        let paragraphs: Vec<Paragraph> = sub.document().body().iter().cloned().collect();
        editor
            .edit("Edit Text Box", |edit| edit.set_story(story, paragraphs))
            .unwrap();
        assert_eq!(editor.document().story_paragraphs(story)[0].text(), "Boxed");
        // Find goes through the text boxes too.
        let query = desk_model::Query::new("boxed");
        let found = editor
            .document()
            .find_everywhere(&query, &editor.document().text_box_stories());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].story, Some(story));

        // A box linked after it is edited as the same text, whose lines
        // go on in it.
        let linked = editor
            .edit("Add Linked Text Box", |edit| {
                let style = edit.document().styles().default_paragraph();
                let empty = Paragraph::new(edit.new_id(), style);
                let linked = edit.new_id();
                edit.add_object(Object::new(
                    linked,
                    ObjectKind::TextBox(TextBox::new(
                        Arc::from([empty]),
                        desk_model::Length::from_points(4),
                    )),
                    Frame::from_pt(300.0, 500.0, 150.0, 60.0),
                ))?;
                edit.link_text_box(id, linked)?;
                // Enough text for both boxes.
                let mut paragraphs = edit.document().story_paragraphs(story);
                let style = paragraphs[0].style();
                for _ in 0..4 {
                    paragraphs.push(Paragraph::from_runs(
                        edit.new_id(),
                        style,
                        desk_model::ParaAttrs::default(),
                        [Run::new("More text")],
                    )?);
                }
                edit.set_story(story, paragraphs)?;
                Ok(linked)
            })
            .unwrap();
        let document = editor.document().clone();
        let layout = crate::lay_out(&document);
        assert_eq!(story_at(&layout, 0, 320.0, 520.0), Some(story));
        assert!(!story_pages(&layout).contains(&(Story::TextBox(linked), 0)));
        let copy = story_copy(&document, story);
        let edited = story_layout(&layout, story, 0, &copy);
        let lines = &edited.pages[0].lines;
        assert!(lines.iter().any(|l| l.top() < 460.0));
        assert!(lines.iter().any(|l| l.top() >= 504.0));
    }

    #[test]
    fn obj_08_obj_12_text_is_typed_in_the_middle_of_a_shape() {
        use desk_model::{Frame, Object, Shape};
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Insert Shape", |edit| {
                let id = edit.new_id();
                let mut shape = Object::new(
                    id,
                    Shape::Rectangle.into(),
                    Frame::from_pt(300.0, 400.0, 150.0, 60.0),
                );
                shape.wrap = desk_model::Wrap::Through;
                edit.add_object(shape)?;
                Ok(id)
            })
            .unwrap();
        let document = editor.document().clone();
        let story = Story::TextBox(id);
        let layout = crate::lay_out(&document);
        // A shape with no text is not searched, but can be typed in.
        assert_eq!(story_at(&layout, 0, 320.0, 420.0), Some(story));
        assert!(!story_pages(&layout).contains(&(story, 0)));
        assert!(story_copy_ok(&document, story));
        let copy = story_copy(&document, story);
        let first = copy.body().get(0).unwrap();
        assert_eq!(first.attrs().alignment, Some(Alignment::Center));
        let at = desk_model::Position::new(first.id(), 0);
        let mut sub = Editor::new(copy);
        sub.edit("Typing", |edit| edit.insert_text(at, "Hi").map(|_| ()))
            .unwrap();
        let paragraphs: Vec<Paragraph> = sub.document().body().iter().cloned().collect();
        let edited = story_layout(&layout, story, 0, sub.document());
        let line = &edited.pages[0].lines[0];
        // In the middle of the shape, from top to bottom.
        assert!(((line.top() + line.bottom()) / 2.0 - 430.0).abs() < 0.01);
        editor
            .edit("Edit Text", |edit| edit.set_story(story, paragraphs))
            .unwrap();
        let layout = crate::lay_out(editor.document());
        assert!(story_pages(&layout).contains(&(story, 0)));
        assert_eq!(layout.pages[0].objects[0].lines.len(), 1);
    }

    #[test]
    fn est_13_a_note_is_edited_in_a_copy_and_written_back_as_one_action() {
        use desk_model::{CharAttrs, Note, Run};
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        let mark = desk_model::FIELD_CHARACTER.to_string();
        editor
            .edit("Insert Note", |edit| {
                let after = edit.insert_text(start, &format!("Body{mark}"))?;
                let note = edit.new_id();
                let attrs = CharAttrs {
                    field: Some(Some(Field::NoteMark { note })),
                    ..CharAttrs::default()
                };
                let at = desk_model::Position::new(after.paragraph, 4);
                edit.set_character_attrs(at, after, &attrs)?;
                let id = edit.new_id();
                let style = edit.document().styles().default_paragraph();
                let paragraph = Paragraph::from_runs(
                    id,
                    style,
                    desk_model::ParaAttrs::default(),
                    [Run::with_attrs(&mark, None, attrs), Run::new(" ")],
                )?;
                edit.set_note(
                    note,
                    Some(Note {
                        id: note,
                        mark: None,
                        paragraphs: vec![paragraph].into(),
                    }),
                )
            })
            .unwrap();
        let document = editor.document().clone();
        let note = document.notes().iter().next().unwrap().id;
        // The mark in the body leads to the note, and back; offsets
        // count bytes, and the mark has three.
        assert_eq!(mark_near(&document, Spot::new(0, 5)), Some(note));
        assert_eq!(mark_near(&document, Spot::new(0, 1)), None);
        assert_eq!(mark_of(&document, note), Some(Spot::new(0, 7)));
        let story = Story::Note(note);
        let layout = crate::lay_out(&document);
        let note_line = layout.pages[0].note_lines[0].clone();
        assert_eq!(
            story_at(&layout, 0, 100.0, f64::from(note_line.top()) + 1.0),
            Some(story)
        );
        let copy = story_copy(&document, story);
        let edited = story_layout(&layout, story, 0, &copy);
        assert_eq!(edited.pages[0].lines.len(), 1);
        assert_eq!(edited.pages[0].lines[0].paragraph, 0);
        assert_eq!(mark_near(&copy, Spot::new(0, 0)), Some(note));
        // Two changes in the copy, written back each time, are one action
        // of the document.
        let mut sub = Editor::new(copy);
        let depth = editor.undo_depth();
        for text in ["Ver", " p. 3"] {
            let end = {
                let p = sub.document().body().get(0).unwrap();
                desk_model::Position::new(p.id(), p.len())
            };
            sub.edit("Typing", |edit| edit.insert_text(end, text))
                .unwrap();
            let copy = sub.document().clone();
            let paragraphs: Vec<Paragraph> = copy.body().iter().cloned().collect();
            let write = |edit: &mut desk_model::Edit| {
                edit.adopt_document_settings(&copy)?;
                edit.set_story(story, paragraphs.clone())
            };
            if editor.undo_depth() > depth {
                editor.edit_continuing("Edit Note", write).unwrap();
            } else {
                editor.edit("Edit Note", write).unwrap();
            }
        }
        assert_eq!(editor.undo_depth(), depth + 1);
        let text = editor.document().notes().get(note).unwrap().paragraphs[0]
            .text()
            .to_string();
        assert_eq!(text, format!("{mark} Ver p. 3"));
        assert_eq!(
            editor.document().body().get(0).unwrap().text(),
            format!("Body{mark}")
        );
        editor.undo().unwrap();
        let text = editor.document().notes().get(note).unwrap().paragraphs[0]
            .text()
            .to_string();
        assert_eq!(text, format!("{mark} "));
    }

    #[test]
    fn txt_10_mistakes_in_a_header_are_marked_while_the_body_is_edited() {
        if desk_lang::speller("en-US").is_none() {
            return;
        }
        let mut document = Document::new_word_processing();
        let header = Story::Margin {
            section: None,
            place: MarginPlace::new(MarginKind::Header, MarginPages::Default),
        };
        document
            .transact("Header", |edit| {
                let style = edit.document().styles().default_paragraph();
                let english = desk_model::CharAttrs {
                    language: Some(desk_model::Language::new("en-US")),
                    ..desk_model::CharAttrs::default()
                };
                let id = edit.new_id();
                let paragraph = Paragraph::from_runs(
                    id,
                    style,
                    desk_model::ParaAttrs::default(),
                    [desk_model::Run::with_attrs("We acheive", None, english)],
                )?;
                edit.set_story(header, vec![paragraph])
            })
            .unwrap();
        let layout = crate::lay_out(&document);
        let marks = story_squiggles(&document, &layout, None, true, false);
        assert!(!marks.is_empty());
        assert!(
            marks
                .iter()
                .all(|m| m.span.y1 < f64::from(layout.pages[0].body.0) + 1.0)
        );
        // Not while the header itself is edited, whose marks come from
        // its copy.
        assert!(story_squiggles(&document, &layout, Some(header), true, false).is_empty());
    }
}
