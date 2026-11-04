// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The texts of a document besides its body: its headers and footers,
//! its notes and its text boxes, which are edited on the page as the body
//! is (EST-12, EST-13, OBJ-13).

use crate::document::Document;
use crate::error::ModelError;
use crate::header::MarginPlace;
use crate::id::NodeId;
use crate::object::ObjectKind;
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
            Story::TextBox(id) => match self.objects().get(id).map(|o| &o.kind) {
                Some(ObjectKind::TextBox(text)) => text.paragraphs.to_vec(),
                _ => Vec::new(),
            },
        }
    }

    /// The text boxes of the document, from back to front, as stories.
    pub fn text_box_stories(&self) -> Vec<Story> {
        self.objects()
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::TextBox(_)))
            .map(|o| Story::TextBox(o.id))
            .collect()
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
                let mut object = self
                    .document()
                    .objects()
                    .get(id)
                    .cloned()
                    .ok_or(ModelError::UnknownObject(id))?;
                let ObjectKind::TextBox(text) = &mut object.kind else {
                    return Err(ModelError::UnknownObject(id));
                };
                if paragraphs.is_empty() {
                    return Ok(());
                }
                text.paragraphs = paragraphs.into();
                self.replace_object(object)
            }
        }
    }

    /// Makes what belongs to the whole document as it is in another copy
    /// of it: its styles, page setup, text settings and information, and
    /// the identifiers handed out. A
    /// story edited in a copy of the document changes them with the tools
    /// of the body (see [`Document::with_body`]).
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
        self.set_metadata(from.metadata().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::Query;
    use crate::header::{MarginKind, MarginPages};
    use crate::text::Position;

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
}
