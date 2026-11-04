// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The notes of a document on its pages (EST-13, EST-14).
//!
//! The rules, from the PDFs of Pages:
//!
//! - Notes are numbered in the order the body refers to them, in the
//!   format of the document, again from 1 on each page or in each section
//!   when the document says so. A note with a mark of its own shows it
//!   and takes no number.
//! - Footnotes go at the foot of the page whose line refers to them, the
//!   last one on the bottom margin, with the space between notes above
//!   each. The lines of the body leave them room.
//! - Endnotes follow the last paragraph of the document, or of their
//!   section, as paragraphs of the body would, with the space between
//!   notes above each.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use desk_model::{
    Document, Field, NodeId, NoteKind, NoteNumbering, NoteSettings, PageSetup, Start,
};
use desk_text::{Column, FieldValues, ParagraphText, TextSystem};

use crate::pages::{
    DocumentLayout, Footnotes, NOTE_RULE_BELOW, NOTE_SEPARATION, paginate_with_footnotes,
};

/// The marks of the notes, by note.
pub(crate) type Labels = Arc<HashMap<NodeId, String>>;

/// The notes that the body refers to, in order.
pub(crate) struct NotePlan {
    /// The paragraph of the body and the offset of each mark, with its
    /// note.
    refs: Vec<(usize, usize, NodeId)>,
    settings: NoteSettings,
}

impl NotePlan {
    /// The notes that the body of a document refers to.
    pub fn new(document: &Document) -> Self {
        let notes = document.notes();
        let mut refs = Vec::new();
        for (index, paragraph) in document.body().iter().enumerate() {
            let mut offset = 0;
            for run in paragraph.runs() {
                if let Some(Some(Field::NoteMark { note })) = &run.attrs.field
                    && notes.get(*note).is_some()
                {
                    for (i, _) in run.text().char_indices() {
                        refs.push((index, offset + i, *note));
                    }
                }
                offset += run.len();
            }
        }
        Self {
            refs,
            settings: document.text_settings().notes,
        }
    }

    /// The notes, in the order that the body refers to them.
    pub fn note_ids(&self) -> Vec<NodeId> {
        self.refs.iter().map(|&(_, _, id)| id).collect()
    }

    /// Whether the numbers of the notes depend on their pages.
    pub fn numbered_by_page(&self, _document: &Document) -> bool {
        self.settings.numbering == NoteNumbering::Page && !self.refs.is_empty()
    }

    /// The marks of the notes, numbered by page from a layout when there
    /// is one.
    pub fn labels(&self, document: &Document, layout: Option<&DocumentLayout>) -> Labels {
        let mut labels = HashMap::new();
        let mut number = 0;
        let mut previous: Option<usize> = None;
        for &(paragraph, offset, id) in &self.refs {
            let group = match self.settings.numbering {
                NoteNumbering::Continuous => 0,
                NoteNumbering::Section => document.section_at(paragraph).0.map_or(0, |i| i + 1),
                NoteNumbering::Page => layout.map_or(0, |l| page_of_offset(l, paragraph, offset)),
            };
            if previous != Some(group) {
                number = 0;
                previous = Some(group);
            }
            let note = document.notes().get(id).expect("the plan has only notes");
            let label = match &note.mark {
                Some(mark) => mark.to_string(),
                None => {
                    number += 1;
                    self.settings.format.label(number)
                }
            };
            labels.insert(id, label);
        }
        Arc::new(labels)
    }

    /// The prepared paragraphs of each note, in the order the body refers
    /// to them.
    fn note_texts(
        &self,
        document: &Document,
        values: &[FieldValues],
        labels: &Labels,
    ) -> Vec<Vec<ParagraphText>> {
        let styles = document.styles();
        let settings = document.text_settings();
        self.refs
            .iter()
            .map(|&(paragraph, _, id)| {
                let note = document.notes().get(id).expect("the plan has only notes");
                let values = FieldValues {
                    notes: Some(labels.clone()),
                    ..values.get(paragraph).cloned().unwrap_or_default()
                };
                let items = styles.list_items(note.paragraphs.iter());
                note.paragraphs
                    .iter()
                    .zip(&items)
                    .map(|(p, item)| {
                        ParagraphText::new_in_document_with(
                            styles,
                            settings,
                            p,
                            item.as_ref(),
                            values.clone(),
                        )
                    })
                    .collect()
            })
            .collect()
    }

    /// Lays out the body with its notes.
    pub fn paginate(
        &self,
        document: &Document,
        values: &[FieldValues],
        labels: &Labels,
        system: &mut TextSystem,
        setup: PageSetup,
        sum: bool,
    ) -> DocumentLayout {
        let styles = document.styles();
        let settings = document.text_settings();
        let items = styles.list_items(document.body().iter());
        let body: Vec<ParagraphText> = document
            .body()
            .iter()
            .enumerate()
            .map(|(i, paragraph)| {
                ParagraphText::new_in_document_with(
                    styles,
                    settings,
                    paragraph,
                    items[i].as_ref(),
                    values.get(i).cloned().unwrap_or_default(),
                )
            })
            .collect();
        let room = crate::pages::MarginRoom::new(document, system);
        if self.refs.is_empty() {
            return paginate_with_footnotes(setup, &body, system, sum, None, Some(&room));
        }
        let notes = self.note_texts(document, values, labels);
        let gap = self.settings.gap.to_pt() as f32;
        match self.settings.kind {
            NoteKind::Footnotes => {
                let width = setup.body_width().to_pt() as f32;
                let mut refs = vec![Vec::new(); body.len()];
                for (index, &(paragraph, offset, _)) in self.refs.iter().enumerate() {
                    refs[paragraph].push((offset, index));
                }
                let mut parts = Vec::with_capacity(notes.len());
                let laid = notes
                    .iter()
                    .map(|texts| {
                        let mut lines = Vec::new();
                        let mut part = Vec::new();
                        let mut y = 0.0f32;
                        for (i, text) in texts.iter().enumerate() {
                            let format = text.paragraph();
                            if i > 0 {
                                y += format.space_before.to_pt() as f32;
                            }
                            let laid = system.layout(text, &Column::new(width), 0.0).lines;
                            let Some(first) = laid.first() else {
                                continue;
                            };
                            let dy = y - first.top;
                            for line in &laid {
                                y = dy + line.top + line.height;
                                lines.push((line.clone(), dy));
                                part.push(i);
                            }
                            y += format.space_after.to_pt() as f32
                                * f32::from(u8::from(i + 1 < texts.len()));
                        }
                        parts.push(part);
                        (lines, y)
                    })
                    .collect();
                let footnotes = Rc::new(Footnotes {
                    refs,
                    notes: laid,
                    parts,
                    gap,
                });
                paginate_with_footnotes(setup, &body, system, sum, Some(footnotes), Some(&room))
            }
            NoteKind::DocumentEndnotes | NoteKind::SectionEndnotes => {
                self.paginate_endnotes(document, body, notes, gap, system, setup, sum, &room)
            }
        }
    }

    /// Lays out the body with its endnotes after the paragraphs of the
    /// document or of each section.
    #[allow(clippy::too_many_arguments)]
    fn paginate_endnotes(
        &self,
        document: &Document,
        body: Vec<ParagraphText>,
        notes: Vec<Vec<ParagraphText>>,
        gap: f32,
        system: &mut TextSystem,
        setup: PageSetup,
        sum: bool,
        room: &crate::pages::MarginRoom,
    ) -> DocumentLayout {
        let n = body.len();
        // Where the notes of each section go: before the paragraph that
        // starts the next one, or at the end.
        let ends: Vec<usize> = match self.settings.kind {
            NoteKind::SectionEndnotes => document
                .body()
                .iter()
                .enumerate()
                .filter(|(i, p)| {
                    *i > 0
                        && matches!(
                            p.attrs().start.clone().flatten().as_deref(),
                            Some(Start::Section(..))
                        )
                })
                .map(|(i, _)| i)
                .chain([n])
                .collect(),
            _ => vec![n],
        };
        // Each text of the flow, as a paragraph of the body or of a note.
        let mut flow: Vec<ParagraphText> = Vec::with_capacity(n + notes.len());
        // Endnotes of the document follow its text with no more room than
        // between notes; those of sections take a rule above them.
        let rule = self.settings.kind == NoteKind::SectionEndnotes;
        let mut sources: Vec<Result<usize, (usize, usize, bool)>> = Vec::new();
        let mut notes: Vec<Option<Vec<ParagraphText>>> = notes.into_iter().map(Some).collect();
        let mut body = body.into_iter().map(Some).collect::<Vec<_>>();
        let mut next = 0;
        for end in ends {
            while next < end {
                flow.push(body[next].take().expect("each paragraph once"));
                sources.push(Ok(next));
                next += 1;
            }
            let mut first = true;
            for (index, &(paragraph, _, _)) in self.refs.iter().enumerate() {
                if paragraph < end
                    && let Some(texts) = notes[index].take()
                {
                    for (i, mut text) in texts.into_iter().enumerate() {
                        let format = text.paragraph_mut();
                        format.page_break_before = false;
                        format.start = None;
                        let starts = i == 0 && std::mem::take(&mut first);
                        if i == 0 {
                            let above = match (starts, rule) {
                                (true, true) => NOTE_SEPARATION,
                                (true, false) => 0.0,
                                (false, _) => gap,
                            };
                            format.space_before = desk_model::Length::from_pt(f64::from(above));
                        }
                        flow.push(text);
                        sources.push(Err((index, i, starts && rule)));
                    }
                }
            }
        }
        // The notes show the headers and footers of the paragraph before.
        let mut key = None;
        let keys = sources
            .iter()
            .map(|source| {
                if let Ok(paragraph) = source {
                    key = room.keys.get(*paragraph).copied().flatten();
                }
                key
            })
            .collect();
        let mut room = room.with_keys(keys);
        // The objects move with the paragraphs of the body where they are
        // in the flow.
        let flow_index: HashMap<usize, usize> = sources
            .iter()
            .enumerate()
            .filter_map(|(j, source)| source.as_ref().ok().map(|&p| (p, j)))
            .collect();
        room.floats = Arc::new(
            room.floats
                .iter()
                .map(|f| crate::objects::Floating {
                    anchor: match f.anchor {
                        crate::objects::Place::Paragraph(p) => flow_index
                            .get(&p)
                            .map_or(f.anchor, |&j| crate::objects::Place::Paragraph(j)),
                        page => page,
                    },
                    ..f.clone()
                })
                .collect(),
        );
        let mut layout = paginate_with_footnotes(setup, &flow, system, sum, None, Some(&room));
        for page in &mut layout.pages {
            let lines = std::mem::take(&mut page.lines);
            for mut line in lines {
                match sources[line.paragraph] {
                    Ok(paragraph) => {
                        line.paragraph = paragraph;
                        page.lines.push(line);
                    }
                    Err((note, part, ruled)) => {
                        if ruled && line.index == 0 {
                            page.note_rule = Some((page.margins.0, line.top() - NOTE_RULE_BELOW));
                        }
                        line.paragraph = note;
                        page.note_lines.push(line);
                        page.note_parts.push(part);
                    }
                }
            }
            for object in &mut page.objects {
                if let Some(Ok(paragraph)) = object.paragraph.map(|p| sources[p]) {
                    object.paragraph = Some(paragraph);
                }
            }
            page.blocks
                .retain_mut(|block| match sources[block.paragraph] {
                    Ok(paragraph) => {
                        block.paragraph = paragraph;
                        true
                    }
                    Err(_) => false,
                });
        }
        layout
    }
}

/// The index of the page whose line holds an offset of a paragraph.
fn page_of_offset(layout: &DocumentLayout, paragraph: usize, offset: usize) -> usize {
    layout
        .pages
        .iter()
        .position(|page| {
            page.lines.iter().any(|l| {
                l.paragraph == paragraph
                    && (l.line.range.contains(&offset) || l.line.range.end == offset)
            })
        })
        .unwrap_or(0)
}
