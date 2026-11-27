// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The texts of a document besides its body: its headers and footers,
//! its notes and its text boxes, which are edited on the page as the body
//! is (EST-12, EST-13, OBJ-13).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::document::Document;
use crate::error::ModelError;
use crate::header::MarginPlace;
use crate::id::NodeId;
use crate::object::{Object, ObjectKind, TextBox};
use crate::op::Op;
use crate::text::{Paragraph, TextFlow};
use crate::transaction::Edit;

/// A text of a document other than its body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Story {
    /// A footnote or an endnote.
    Note(NodeId),
    /// A header or a footer of a section, by the paragraph that starts
    /// it, or of the first section with none.
    Margin {
        /// The section.
        section: Option<NodeId>,
        /// Which header or footer.
        place: MarginPlace,
    },
    /// The text of a text box on the pages, by the identifier of the
    /// object (OBJ-13).
    TextBox(NodeId),
}

impl Document {
    /// The paragraphs of a story, which an empty header has none of.
    pub fn story_paragraphs(&self, story: Story) -> Vec<Paragraph> {
        match story {
            Story::Note(id) => self
                .notes()
                .get(id)
                .map(|n| n.paragraphs.to_vec())
                .unwrap_or_default(),
            Story::Margin { section, place } => {
                self.section_headers_footers(section).get(place).to_vec()
            }
            Story::TextBox(id) => {
                let head = self.text_box_head(id);
                self.objects()
                    .get(head)
                    .and_then(Object::text_box)
                    .map(|text| text.paragraphs.to_vec())
                    .unwrap_or_default()
            }
        }
    }

    /// The text box whose text a text box shows: the first of the boxes
    /// linked to it, or itself (OBJ-13). In a loop of links, the head is
    /// the last box met going back before the loop closes.
    pub fn text_box_head(&self, id: NodeId) -> NodeId {
        let before = self.text_box_before();
        let mut head = id;
        let mut seen = HashSet::from([id]);
        while let Some(&b) = before.get(&head) {
            if !seen.insert(b) {
                break;
            }
            head = b;
        }
        head
    }

    /// The box linked to each linked box of the page: the first, from the
    /// back, whose next it is. A box in a group is its own head, as the
    /// chains and the layout of linked boxes go through the boxes of the
    /// page only, until they go through groups too.
    fn text_box_before(&self) -> HashMap<NodeId, NodeId> {
        let top: HashSet<NodeId> = self.objects().iter().map(|o| o.id).collect();
        let mut before = HashMap::new();
        for o in self.objects().iter() {
            if let ObjectKind::TextBox(TextBox {
                next: Some(next), ..
            }) = &o.kind
                && top.contains(next)
            {
                before.entry(*next).or_insert(o.id);
            }
        }
        before
    }

    /// [`Document::text_box_head`] of every object of the page, found
    /// together in time of the number of objects: a file can link
    /// thousands of boxes. Going back
    /// from a box ends at a box with none before it, its head, or at a box
    /// met already, in a loop of links, and the head is then the last box
    /// before it. An object not in the map is its own head.
    pub fn text_box_heads(&self) -> HashMap<NodeId, NodeId> {
        let before = self.text_box_before();
        let mut heads: HashMap<NodeId, NodeId> = HashMap::new();
        for id in self.objects().iter().map(|o| o.id) {
            if heads.contains_key(&id) {
                continue;
            }
            let mut path = vec![id];
            let mut at: HashMap<NodeId, usize> = HashMap::from([(id, 0)]);
            let mut head = None;
            let mut looped = None;
            while let Some(&b) = path.last().and_then(|last| before.get(last)) {
                if let Some(&known) = heads.get(&b) {
                    head = Some(known);
                    break;
                }
                if let Some(&i) = at.get(&b) {
                    looped = Some(i);
                    break;
                }
                at.insert(b, path.len());
                path.push(b);
            }
            let last = path.last().copied().unwrap_or(id);
            match looped {
                // Each box of the loop goes round it to the box before
                // itself; those that lead into it go round to the last.
                Some(start) => {
                    for (i, id) in path.iter().enumerate() {
                        let h = if i > start { path[i - 1] } else { last };
                        heads.insert(*id, h);
                    }
                }
                None => {
                    let h = head.unwrap_or(last);
                    for id in path {
                        heads.insert(id, h);
                    }
                }
            }
        }
        heads
    }

    /// The text boxes linked from one, in order, starting with it.
    pub fn text_box_chain(&self, head: NodeId) -> Vec<NodeId> {
        let boxes: HashMap<NodeId, &TextBox> = self
            .objects()
            .iter()
            .filter_map(|o| match &o.kind {
                ObjectKind::TextBox(t) => Some((o.id, t)),
                _ => None,
            })
            .collect();
        Self::chain_in(&boxes, head)
    }

    fn chain_in(boxes: &HashMap<NodeId, &TextBox>, head: NodeId) -> Vec<NodeId> {
        let mut chain = vec![head];
        let mut seen = HashSet::from([head]);
        let mut at = head;
        while let Some(next) = boxes.get(&at).and_then(|t| t.next) {
            if !boxes.contains_key(&next) || !seen.insert(next) {
                break;
            }
            chain.push(next);
            at = next;
        }
        chain
    }

    /// The threads of text boxes of the document, as Pages numbers them
    /// (PXV-13): the boxes linked with others, and those alone in a
    /// thread, each thread with its boxes in order, by their first boxes
    /// from back to front. A thread is numbered by its place here, from 1.
    pub fn text_box_threads(&self) -> Vec<Vec<NodeId>> {
        let heads = self.text_box_heads();
        let boxes: HashMap<NodeId, &TextBox> = self
            .objects()
            .iter()
            .filter_map(|o| match &o.kind {
                ObjectKind::TextBox(t) => Some((o.id, t)),
                _ => None,
            })
            .collect();
        self.objects()
            .iter()
            .filter(|o| heads.get(&o.id) == Some(&o.id))
            .filter_map(|o| {
                let text = boxes.get(&o.id)?;
                let chain = Self::chain_in(&boxes, o.id);
                (chain.len() > 1 || text.thread).then_some(chain)
            })
            .collect()
    }

    /// The name of a thread of text boxes in its menu: the first words of
    /// its text, or nothing when it has none (PXV-13).
    pub fn text_box_thread_name(&self, id: NodeId) -> String {
        const WORDS: usize = 4;
        let head = self.text_box_head(id);
        let Some(text) = self.objects().get(head).and_then(Object::text_box) else {
            return String::new();
        };
        let mut words = text
            .paragraphs
            .iter()
            .flat_map(|p| {
                p.text()
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .take(WORDS + 1)
            .collect::<Vec<_>>();
        let more = words.len() > WORDS;
        words.truncate(WORDS);
        let mut name = words.join(" ");
        if more {
            name.push('…');
        }
        name
    }

    /// The text boxes of the document, and the shapes with text, from
    /// back to front, as stories.
    pub fn text_box_stories(&self) -> Vec<Story> {
        let heads = self.text_box_heads();
        self.objects()
            .iter()
            .filter(|o| o.text_box().is_some())
            .filter(|o| heads.get(&o.id) == Some(&o.id))
            .map(|o| Story::TextBox(o.id))
            .collect()
    }

    /// Every story of the document with text: the headers and footers of
    /// each section, in the order of [`MarginPlace::ALL`], the notes in
    /// the order of their marks in the body, and the text boxes and the
    /// shapes with text, from back to front.
    pub fn stories(&self) -> Vec<Story> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for (section, margins) in self.all_headers_footers() {
            for place in MarginPlace::ALL {
                if !margins.get(place).is_empty() {
                    out.push(Story::Margin { section, place });
                }
            }
        }
        for paragraph in self.body().iter() {
            for run in paragraph.runs() {
                if let Some(Some(crate::Field::NoteMark { note })) = run.attrs.field {
                    let story = Story::Note(note);
                    if self.notes().get(note).is_some() && seen.insert(story) {
                        out.push(story);
                    }
                }
            }
        }
        out.extend(self.text_box_stories());
        out
    }

    /// The same document with other paragraphs in its body, to edit the
    /// text of a story with the tools of the body. With no paragraphs, it
    /// keeps its body.
    pub fn with_body(&self, paragraphs: Vec<Paragraph>) -> Document {
        let mut document = self.clone();
        let mut paragraphs = paragraphs.into_iter();
        if let Some(first) = paragraphs.next() {
            let mut flow = TextFlow::new(first);
            for paragraph in paragraphs {
                let index = flow.len();
                // Identifiers are unique in a story.
                let _ = flow.insert(index, paragraph);
            }
            *document.body_mut() = flow;
        }
        document
    }
}

impl Edit {
    /// Changes the whole text of a story with the tools of the body,
    /// given the start and the end of the text, as the Text tab of the
    /// Inspector formats all the text of a selected shape or text box.
    /// A shape with no text yet keeps an empty paragraph, centered as its
    /// first text is, with the paragraph formatting, which the first
    /// text typed in it takes; an empty paragraph has no characters for
    /// character formatting.
    pub fn format_whole_story(
        &mut self,
        story: Story,
        change: impl FnOnce(&mut Edit, crate::Position, crate::Position) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        let mut paragraphs = self.document().story_paragraphs(story);
        let first_text = paragraphs.is_empty();
        if first_text {
            let Story::TextBox(id) = story else {
                return Ok(());
            };
            if !self
                .document()
                .objects()
                .get(id)
                .is_some_and(Object::holds_text)
            {
                return Err(ModelError::UnknownObject(id));
            }
            let style = self.document().styles().default_paragraph();
            let attrs = crate::ParaAttrs {
                alignment: Some(crate::Alignment::Center),
                ..crate::ParaAttrs::default()
            };
            let id = self.new_id();
            paragraphs.push(Paragraph::from_runs(id, style, attrs, [])?);
        }
        let mut copy = self.document().with_body(paragraphs);
        copy.transact("Format", |edit| {
            let body = edit.document().body();
            let last = body.len().saturating_sub(1);
            let (Some(first), Some(end)) = (body.get(0), body.get(last)) else {
                return Ok(());
            };
            let from = crate::Position::new(first.id(), 0);
            let to = crate::Position::new(end.id(), end.len());
            change(edit, from, to)
        })?;
        self.adopt_document_settings(&copy)?;
        let paragraphs: Vec<Paragraph> = copy.body().iter().cloned().collect();
        match story {
            Story::TextBox(id) if first_text => {
                let id = self.document().text_box_head(id);
                let mut object = self
                    .document()
                    .objects()
                    .get(id)
                    .cloned()
                    .ok_or(ModelError::UnknownObject(id))?;
                match object.text.as_mut() {
                    Some(text) => text.paragraphs = paragraphs.into(),
                    None => object.text = Some(TextBox::in_shape(paragraphs.into())),
                }
                self.replace_object(object)
            }
            _ => self.set_story(story, paragraphs),
        }
    }

    /// Changes the text of a story with the tools of the body, in a copy
    /// of the document whose body is that text, and writes it back with
    /// what belongs to the whole document, as its comments (W149), and
    /// the objects in its lines. A
    /// story without paragraphs, as an empty header, has no text to
    /// change.
    pub fn edit_story<T>(
        &mut self,
        story: Story,
        change: impl FnOnce(&mut Edit) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        let paragraphs = self.document().story_paragraphs(story);
        if paragraphs.is_empty() {
            return Err(match story {
                Story::TextBox(id) => ModelError::UnknownObject(id),
                Story::Note(id) => ModelError::UnknownParagraph(id),
                Story::Margin { .. } => ModelError::EmptyRange,
            });
        }
        let before = paragraphs.clone();
        let mut copy = self.document().with_body(paragraphs);
        let (value, _) = copy.transact("Edit", change)?;
        self.adopt_document_settings(&copy)?;
        let paragraphs: Vec<Paragraph> = copy.body().iter().cloned().collect();
        self.set_story(story, paragraphs)?;
        // And the objects in its lines (EST-12, EST-13, W106).
        self.adopt_story_objects(&copy, &before)?;
        Ok(value)
    }

    /// Sets the paragraphs of a story. A note keeps its mark of its own,
    /// and a header or footer whose paragraphs are all empty becomes
    /// empty.
    pub fn set_story(
        &mut self,
        story: Story,
        paragraphs: Vec<Paragraph>,
    ) -> Result<(), ModelError> {
        match story {
            Story::Note(id) => {
                let Some(mut note) = self.document().notes().get(id).cloned() else {
                    return Err(ModelError::UnknownParagraph(id));
                };
                note.paragraphs = paragraphs.into();
                self.set_note(id, Some(note))
            }
            Story::Margin { section, place } => {
                let paragraphs = if paragraphs.iter().all(Paragraph::is_empty) {
                    Vec::new()
                } else {
                    paragraphs
                };
                self.set_section_header_footer(section, place, paragraphs)
            }
            Story::TextBox(id) => {
                let id = self.document().text_box_head(id);
                let mut object = self
                    .document()
                    .objects()
                    .get(id)
                    .cloned()
                    .ok_or(ModelError::UnknownObject(id))?;
                if !object.holds_text() {
                    return Err(ModelError::UnknownObject(id));
                }
                if paragraphs.is_empty() {
                    return Ok(());
                }
                let shape = !matches!(object.kind, ObjectKind::TextBox(_));
                if shape && paragraphs.iter().all(Paragraph::is_empty) {
                    // A shape left with no text has no paragraphs, and
                    // keeps how its text would be set.
                    match object.text.as_mut() {
                        Some(text) if !text.paragraphs.is_empty() => {
                            text.paragraphs = Arc::from([]);
                        }
                        _ => return Ok(()),
                    }
                } else {
                    match object.text_box_mut() {
                        Some(text) => text.paragraphs = paragraphs.into(),
                        // The first text typed in a shape.
                        None => object.text = Some(TextBox::in_shape(paragraphs.into())),
                    }
                }
                self.replace_object(object)
            }
        }
    }

    /// Makes what belongs to the whole document as it is in another copy
    /// of it: its styles, page setup, text settings and information, and
    /// the identifiers handed out. A
    /// story edited in a copy of the document changes them with the tools
    /// of the body (see [`Document::with_body`]), and its comments and
    /// the changes tracked in it.
    pub fn adopt_document_settings(&mut self, from: &Document) -> Result<(), ModelError> {
        self.follow_ids(from);
        for style in from.styles().iter() {
            if self.document().styles().get(style.id()) != Some(style) {
                self.apply(Op::PutStyle {
                    style: style.clone(),
                })?;
            }
        }
        let gone: Vec<_> = self
            .document()
            .styles()
            .iter()
            .map(|s| s.id())
            .filter(|id| from.styles().get(*id).is_none())
            .collect();
        for style in gone {
            // A style still in use elsewhere stays.
            let _ = self.apply(Op::RemoveStyle { style });
        }
        self.set_page_setup(*from.page_setup())?;
        self.set_text_settings(from.text_settings().clone())?;
        self.set_metadata(from.metadata().clone())?;
        // The comments, added, changed or deleted in the story (W149).
        for comment in from.comments().iter() {
            self.set_comment(comment.id, Some(comment.clone()))?;
        }
        let gone: Vec<NodeId> = self
            .document()
            .comments()
            .iter()
            .map(|c| c.id)
            .filter(|id| from.comments().get(*id).is_none())
            .collect();
        for id in gone {
            self.set_comment(id, None)?;
        }
        // And the tracked changes made in the story (W148).
        for change in from.changes().iter() {
            if self.document().changes().get(change.id) != Some(change) {
                self.set_change(change.id, Some(change.clone()))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::Query;
    use crate::header::{MarginKind, MarginPages};
    use crate::text::Position;

    #[test]
    fn obj_13_the_text_tab_formats_all_the_text_of_a_shape() {
        use crate::{Alignment, CharAttrs, Frame, ParaAttrs, Run, Shape};
        let mut document = Document::new_word_processing();
        let (with_text, empty) = document
            .transact("Shapes", |edit| {
                let style = edit.document().styles().default_paragraph();
                let paragraphs: Vec<Paragraph> = ["One", "Two"]
                    .iter()
                    .map(|t| {
                        Paragraph::from_runs(
                            edit.new_id(),
                            style,
                            ParaAttrs::default(),
                            [Run::new(t)],
                        )
                    })
                    .collect::<Result<_, _>>()?;
                let a = edit.new_id();
                let mut shape = Object::new(
                    a,
                    Shape::Rectangle.into(),
                    Frame::from_pt(0.0, 0.0, 9.0, 9.0),
                );
                shape.text = Some(TextBox::in_shape(paragraphs.into()));
                edit.add_object(shape)?;
                let b = edit.new_id();
                edit.add_object(Object::new(
                    b,
                    Shape::Ellipse.into(),
                    Frame::from_pt(0.0, 0.0, 9.0, 9.0),
                ))?;
                Ok((a, b))
            })
            .unwrap()
            .0;
        let red = CharAttrs {
            color: Some(crate::Rgba::rgb(255, 0, 0)),
            ..CharAttrs::default()
        };
        let right = ParaAttrs {
            alignment: Some(Alignment::Right),
            ..ParaAttrs::default()
        };
        for id in [with_text, empty] {
            document
                .transact("Format", |edit| {
                    edit.format_whole_story(Story::TextBox(id), |edit, from, to| {
                        edit.set_character_attrs(from, to, &red)?;
                        edit.set_paragraph_attrs(from, to, &right)
                    })
                })
                .unwrap();
        }
        let paragraphs = document.story_paragraphs(Story::TextBox(with_text));
        assert_eq!(paragraphs.len(), 2);
        for paragraph in &paragraphs {
            assert_eq!(paragraph.attrs().alignment, Some(Alignment::Right));
            assert!(paragraph.runs().all(|r| r.attrs.color == red.color));
        }
        // The shape with no text keeps how its first paragraph is set;
        // an empty paragraph has no characters to color.
        let paragraphs = document.story_paragraphs(Story::TextBox(empty));
        assert_eq!(paragraphs.len(), 1);
        assert!(paragraphs[0].is_empty());
        assert_eq!(paragraphs[0].attrs().alignment, Some(Alignment::Right));
    }

    #[test]
    fn txt_12_searches_go_through_the_headers_too() {
        let mut document = Document::new_word_processing();
        let header = Story::Margin {
            section: None,
            place: MarginPlace::new(MarginKind::Header, MarginPages::Default),
        };
        document
            .transact("Setup", |edit| {
                let first = edit.document().body().get(0).unwrap().id();
                edit.insert_text(Position::new(first, 0), "A cat")?;
                let id = edit.new_id();
                let style = edit.document().styles().default_paragraph();
                let paragraph = Paragraph::from_runs(
                    id,
                    style,
                    crate::ParaAttrs::default(),
                    [crate::Run::new("Cat and cat")],
                )?;
                edit.set_story(header, vec![paragraph])
            })
            .unwrap();
        let query = Query {
            text: "cat".into(),
            ..Query::default()
        };
        let found = document.find_everywhere(&query, &[header]);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].story, None);
        assert_eq!(found[1].story, Some(header));
        assert_eq!(found[2].range, 8..11);
        let (count, _) = document
            .transact("Replace All", |edit| {
                edit.replace_everywhere(&query, "dog", &[header])
            })
            .unwrap();
        assert_eq!(count, 3);
        assert_eq!(document.body().get(0).unwrap().text(), "A dog");
        assert_eq!(document.story_paragraphs(header)[0].text(), "dog and dog");
    }

    #[test]
    fn obj_08_obj_12_text_is_typed_in_shapes_but_not_in_lines() {
        use crate::object::{Frame, Shape, VerticalAlignment};
        let mut document = Document::new_word_processing();
        let ((shape, line), _) = document
            .transact("Shapes", |edit| {
                let (shape, line) = (edit.new_id(), edit.new_id());
                let frame = Frame::from_pt(10.0, 10.0, 100.0, 50.0);
                edit.add_object(Object::new(shape, Shape::Ellipse.into(), frame))?;
                let rising = Shape::Line { rising: false };
                edit.add_object(Object::new(line, rising.into(), frame))?;
                Ok((shape, line))
            })
            .unwrap();
        assert!(document.story_paragraphs(Story::TextBox(shape)).is_empty());
        assert_eq!(document.text_box_stories(), []);
        let paragraph = |edit: &mut Edit| {
            let id = edit.new_id();
            let style = edit.document().styles().default_paragraph();
            Paragraph::from_runs(
                id,
                style,
                crate::ParaAttrs::default(),
                [crate::Run::new("Hello")],
            )
        };
        document
            .transact("Type", |edit| {
                let p = paragraph(edit)?;
                edit.set_story(Story::TextBox(shape), vec![p])
            })
            .unwrap();
        let object = document.objects().get(shape).unwrap();
        let text = object.text_box().unwrap();
        assert_eq!(text.vertical, VerticalAlignment::Middle);
        assert_eq!(object.ids().len(), 2);
        assert_eq!(
            document.story_paragraphs(Story::TextBox(shape))[0].text(),
            "Hello"
        );
        assert_eq!(document.text_box_stories(), [Story::TextBox(shape)]);
        document
            .transact("Clear", |edit| {
                let style = edit.document().styles().default_paragraph();
                let empty = Paragraph::new(edit.new_id(), style);
                edit.set_story(Story::TextBox(shape), vec![empty])
            })
            .unwrap();
        let text = document.objects().get(shape).unwrap().text.as_ref();
        assert!(text.unwrap().paragraphs.is_empty());
        let result = document.transact("Type", |edit| {
            let p = paragraph(edit)?;
            edit.set_story(Story::TextBox(line), vec![p])
        });
        assert_eq!(result.err(), Some(ModelError::UnknownObject(line)));
    }
}

#[cfg(test)]
mod chain_tests {
    use super::*;
    use crate::Length;
    use crate::object::{Frame, Object};
    use crate::text::{Paragraph, Run};
    use crate::transaction::Editor;

    /// A document of `n` text boxes, each linked to the one `next` gives.
    fn linked(n: usize, next: impl Fn(usize) -> Option<usize>) -> (Document, Vec<NodeId>) {
        let mut editor = Editor::new(Document::new_word_processing());
        let ids = editor
            .edit("Add", |e| {
                let ids: Vec<NodeId> = (0..n).map(|_| e.new_id()).collect();
                for (i, &id) in ids.iter().enumerate() {
                    let style = e.document().styles().default_paragraph();
                    let paragraph = Paragraph::from_runs(
                        e.new_id(),
                        style,
                        crate::ParaAttrs::default(),
                        [Run::new("x")],
                    )?;
                    let mut text = TextBox::new(Arc::from([paragraph]), Length::from_points(4));
                    text.next = next(i).map(|j| ids[j]);
                    e.add_object(Object::new(
                        id,
                        ObjectKind::TextBox(text),
                        Frame::from_pt(0.0, 0.0, 50.0, 50.0),
                    ))?;
                }
                Ok(ids)
            })
            .unwrap();
        (editor.into_document(), ids)
    }

    /// The head as it was found before, one box at a time.
    fn head_one_by_one(document: &Document, id: NodeId) -> NodeId {
        let mut head = id;
        let mut seen = vec![id];
        while let Some(before) = document
            .objects()
            .iter()
            .find(|o| matches!(&o.kind, ObjectKind::TextBox(t) if t.next == Some(head)))
        {
            if seen.contains(&before.id) {
                break;
            }
            seen.push(before.id);
            head = before.id;
        }
        head
    }

    #[test]
    fn heads_are_those_found_one_by_one_in_loops_too() {
        // A line, a loop, and a tail into a loop.
        let next = |i: usize| match i {
            0..=2 => Some(i + 1),
            4..=6 => Some(if i == 6 { 4 } else { i + 1 }),
            7 => Some(8),
            8 => Some(9),
            9..=10 => Some(if i == 10 { 9 } else { 10 }),
            _ => None,
        };
        let (document, ids) = linked(12, next);
        let heads = document.text_box_heads();
        for &id in &ids {
            assert_eq!(heads[&id], head_one_by_one(&document, id), "{id:?}");
        }
    }

    #[test]
    fn a_box_in_a_group_is_its_own_head() {
        // Review of RV batch 3: a box in a group that a box outside it
        // links to shows its own text, as the chains and the layout go
        // through the boxes of the page only; its head is itself, so that
        // editing it does not change the text of the other.
        let mut editor = Editor::new(Document::new_word_processing());
        let ids = editor
            .edit("Add", |e| {
                let ids = [e.new_id(), e.new_id(), e.new_id()];
                let mut boxes = Vec::new();
                for i in 0..2 {
                    let style = e.document().styles().default_paragraph();
                    let paragraph = Paragraph::from_runs(
                        e.new_id(),
                        style,
                        crate::ParaAttrs::default(),
                        [Run::new("x")],
                    )?;
                    let mut text = TextBox::new(Arc::from([paragraph]), Length::from_points(4));
                    text.next = (i == 0).then_some(ids[1]);
                    boxes.push(Object::new(
                        ids[i],
                        ObjectKind::TextBox(text),
                        Frame::from_pt(0.0, 0.0, 50.0, 50.0),
                    ));
                }
                let inner = boxes.pop().into_iter().collect::<Vec<_>>();
                e.add_object(boxes.remove(0))?;
                e.add_object(Object::new(
                    ids[2],
                    ObjectKind::Group(Arc::from(inner)),
                    Frame::from_pt(0.0, 0.0, 50.0, 50.0),
                ))?;
                Ok(ids)
            })
            .unwrap();
        let document = editor.into_document();
        assert!(document.objects().iter().all(|o| o.id != ids[1]));
        assert_eq!(document.text_box_head(ids[1]), ids[1]);
        assert_eq!(document.text_box_head(ids[0]), ids[0]);
    }

    #[test]
    fn thousands_of_linked_boxes_are_found_in_linear_time() {
        // Review RV, batch 3: a chain of boxes took the cube of its length.
        let n = 20_000;
        let (document, ids) = linked(n, |i| (i + 1 < n).then_some(i + 1));
        assert_eq!(document.text_box_stories(), vec![Story::TextBox(ids[0])]);
        assert_eq!(document.text_box_threads()[0].len(), n);
        assert_eq!(document.text_box_head(ids[n - 1]), ids[0]);
    }
}
