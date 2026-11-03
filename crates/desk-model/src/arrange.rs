// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whole sections cut, copied, pasted, moved and deleted, as the page
//! thumbnails of Pages do (EST-17, TXT-02).
//!
//! A section is the paragraphs from one that starts a section to the next
//! one that does. The first section has its settings in the page setup
//! and its headers and footers as those of the document; the others have
//! them on the paragraph that starts them. When a section becomes the
//! first, or stops being it, its settings move between the two.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use crate::document::Document;
use crate::error::ModelError;
use crate::header::{Field, HeadersFooters, MarginPlace};
use crate::id::NodeId;
use crate::note::Note;
use crate::op::Op;
use crate::section::{Layout, Section, Start};
use crate::text::Paragraph;
use crate::transaction::Edit;

/// Sections copied, to paste.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionsClip {
    sections: Vec<ClipSection>,
}

#[derive(Clone, Debug, PartialEq)]
struct ClipSection {
    section: Section,
    layout: Layout,
    /// The paragraphs, the first without its start.
    paragraphs: Vec<Paragraph>,
    /// Its headers and footers, when it has its own.
    margins: Option<HeadersFooters>,
    /// The notes whose marks it holds.
    notes: Vec<Note>,
}

impl SectionsClip {
    /// How many sections it holds.
    pub fn len(&self) -> usize {
        self.sections.len()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
}

impl Document {
    /// The paragraphs of the body that each section holds, by index.
    pub fn section_ranges(&self) -> Vec<Range<usize>> {
        let body = self.body();
        let mut starts: Vec<usize> = vec![0];
        for (index, paragraph) in body.iter().enumerate().skip(1) {
            if let Some(Some(start)) = &paragraph.attrs().start
                && matches!(**start, Start::Section(..))
            {
                starts.push(index);
            }
        }
        let mut out = Vec::with_capacity(starts.len());
        for (i, &start) in starts.iter().enumerate() {
            let end = starts.get(i + 1).copied().unwrap_or(body.len());
            out.push(start..end);
        }
        out
    }

    /// The section, first layout and headers and footers of section `k`.
    fn section_start(
        &self,
        k: usize,
        range: &Range<usize>,
    ) -> (Section, Layout, Option<HeadersFooters>) {
        if k == 0 {
            let setup = self.page_setup();
            let mut section = setup.section;
            // Pasted elsewhere, it keeps the headers of the document.
            section.own_margins = true;
            return (section, setup.layout, Some(self.headers_footers().clone()));
        }
        let Some(first) = self.body().get(range.start) else {
            return (self.page_setup().section, self.page_setup().layout, None);
        };
        match first.attrs().start.as_ref().and_then(|s| s.as_deref()) {
            Some(Start::Section(section, layout)) => {
                let margins = section
                    .own_margins
                    .then(|| self.section_headers_footers(Some(first.id())).clone());
                (*section, *layout, margins)
            }
            _ => (self.page_setup().section, self.page_setup().layout, None),
        }
    }

    /// A copy of some sections, by index, in order.
    pub fn copy_sections(&self, indices: &[usize]) -> SectionsClip {
        let ranges = self.section_ranges();
        let mut indices: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&k| k < ranges.len())
            .collect();
        indices.sort_unstable();
        indices.dedup();
        let mut sections = Vec::new();
        for k in indices {
            let range = &ranges[k];
            let (section, layout, margins) = self.section_start(k, range);
            let mut paragraphs: Vec<Paragraph> = range
                .clone()
                .filter_map(|i| self.body().get(i).cloned())
                .collect();
            if let Some(first) = paragraphs.first_mut() {
                let mut attrs = first.attrs().clone();
                attrs.start = None;
                *first = with_attrs(first, attrs);
            }
            let mut notes = Vec::new();
            for paragraph in &paragraphs {
                for run in paragraph.runs() {
                    if let Some(Some(Field::NoteMark { note })) = &run.attrs.field
                        && let Some(note) = self.notes().get(*note)
                        && !notes.iter().any(|n: &Note| n.id == note.id)
                    {
                        notes.push(note.clone());
                    }
                }
            }
            sections.push(ClipSection {
                section,
                layout,
                paragraphs,
                margins,
                notes,
            });
        }
        SectionsClip { sections }
    }
}

/// A paragraph with other attributes.
fn with_attrs(paragraph: &Paragraph, attrs: crate::ParaAttrs) -> Paragraph {
    Paragraph::from_runs(
        paragraph.id(),
        paragraph.style(),
        attrs,
        paragraph.runs().cloned(),
    )
    .unwrap_or_else(|_| paragraph.clone())
}

/// A paragraph with another identifier, whose marks of notes name other
/// notes.
fn renumbered(paragraph: &Paragraph, id: NodeId, notes: &HashMap<NodeId, NodeId>) -> Paragraph {
    let runs = paragraph.runs().map(|run| {
        let mut run = run.clone();
        if let Some(Some(Field::NoteMark { note })) = &run.attrs.field
            && let Some(new) = notes.get(note)
        {
            run.attrs.field = Some(Some(Field::NoteMark { note: *new }));
        }
        run
    });
    Paragraph::from_runs(id, paragraph.style(), paragraph.attrs().clone(), runs)
        .unwrap_or_else(|_| Paragraph::new(id, paragraph.style()))
}

impl Edit {
    fn set_start(&mut self, index: usize, start: Option<Start>) -> Result<(), ModelError> {
        let paragraph = self
            .document()
            .body()
            .get(index)
            .ok_or(ModelError::IndexOutOfBounds {
                index,
                len: self.document().body().len(),
            })?
            .clone();
        let mut attrs = paragraph.attrs().clone();
        attrs.start = start.map(|s| Some(Arc::new(s)));
        if &attrs != paragraph.attrs() {
            self.apply(Op::SetParagraphFormat {
                paragraph: paragraph.id(),
                style: paragraph.style(),
                attrs,
            })?;
        }
        Ok(())
    }

    fn set_margins(
        &mut self,
        section: Option<NodeId>,
        margins: &HeadersFooters,
    ) -> Result<(), ModelError> {
        for place in MarginPlace::ALL {
            self.set_section_header_footer(section, place, margins.get(place).to_vec())?;
        }
        Ok(())
    }

    /// Makes the paragraph at index 0 start the first section: its
    /// section goes to the page setup.
    fn make_first(&mut self) -> Result<(), ModelError> {
        let Some(first) = self.document().body().get(0).cloned() else {
            return Ok(());
        };
        let Some(Start::Section(section, layout)) = first
            .attrs()
            .start
            .as_ref()
            .and_then(|s| s.as_deref())
            .cloned()
        else {
            return Ok(());
        };
        if section.own_margins {
            let margins = self
                .document()
                .section_headers_footers(Some(first.id()))
                .clone();
            self.set_margins(None, &margins)?;
            self.set_margins(Some(first.id()), &HeadersFooters::default())?;
        }
        let mut setup = *self.document().page_setup();
        setup.section = Section {
            own_margins: false,
            ..section
        };
        setup.layout = layout;
        self.set_page_setup(setup)?;
        self.set_start(0, None)
    }

    /// Deletes some sections, by index (TXT-02, EST-17). Deleting every
    /// section leaves one empty paragraph.
    pub fn delete_sections(&mut self, indices: &[usize]) -> Result<(), ModelError> {
        let ranges = self.document().section_ranges();
        let mut indices: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&k| k < ranges.len())
            .collect();
        indices.sort_unstable();
        indices.dedup();
        if indices.is_empty() {
            return Ok(());
        }
        if indices.len() == ranges.len() {
            let style = self.document().body().get(0).map_or_else(
                || self.document().styles().default_paragraph(),
                Paragraph::style,
            );
            let id = self.new_id();
            self.apply(Op::InsertParagraph {
                index: 0,
                paragraph: Paragraph::new(id, style),
            })?;
            let old: Vec<NodeId> = self
                .document()
                .body()
                .iter()
                .skip(1)
                .map(Paragraph::id)
                .collect();
            for paragraph in old {
                self.apply(Op::RemoveParagraph { paragraph })?;
            }
            self.remove_lost_notes()?;
            return Ok(());
        }
        let first_goes = indices[0] == 0;
        for &k in indices.iter().rev() {
            let ids: Vec<NodeId> = ranges[k]
                .clone()
                .filter_map(|i| self.document().body().get(i).map(Paragraph::id))
                .collect();
            if k > 0
                && let Some(first) = ids.first()
            {
                self.set_margins(Some(*first), &HeadersFooters::default())?;
            }
            for paragraph in ids {
                self.apply(Op::RemoveParagraph { paragraph })?;
            }
        }
        if first_goes {
            self.make_first()?;
        }
        self.remove_lost_notes()
    }

    /// Removes the notes whose marks are gone from the body.
    fn remove_lost_notes(&mut self) -> Result<(), ModelError> {
        let lost: Vec<NodeId> = self
            .document()
            .notes()
            .iter()
            .map(|n| n.id)
            .filter(|id| !self.document().has_note_mark(*id))
            .collect();
        for id in lost {
            self.set_note(id, None)?;
        }
        Ok(())
    }

    /// Pastes sections before section `before`, or after the last when it
    /// is their count, and gives the indices of the sections pasted.
    pub fn paste_sections(
        &mut self,
        before: usize,
        clip: &SectionsClip,
    ) -> Result<Range<usize>, ModelError> {
        let ranges = self.document().section_ranges();
        let before = before.min(ranges.len());
        if clip.sections.is_empty() {
            return Ok(before..before);
        }
        let mut index = ranges
            .get(before)
            .map_or(self.document().body().len(), |r| r.start);
        if index == 0 {
            // The first section becomes one like the others.
            let setup = *self.document().page_setup();
            let first = self.document().body().get(0).map(Paragraph::id);
            let margins = self.document().headers_footers().clone();
            self.set_start(
                0,
                Some(Start::Section(
                    Section {
                        own_margins: true,
                        ..setup.section
                    },
                    setup.layout,
                )),
            )?;
            if let Some(first) = first {
                self.set_margins(Some(first), &margins)?;
            }
        }
        for copied in &clip.sections {
            let mut notes: HashMap<NodeId, NodeId> = HashMap::new();
            for note in &copied.notes {
                let id = self.new_id();
                notes.insert(note.id, id);
            }
            let mut first_id = None;
            for (i, paragraph) in copied.paragraphs.iter().enumerate() {
                let id = self.new_id();
                let mut paragraph = renumbered(paragraph, id, &notes);
                if i == 0 {
                    first_id = Some(id);
                    let mut attrs = paragraph.attrs().clone();
                    attrs.start = Some(Some(Arc::new(Start::Section(
                        copied.section,
                        copied.layout,
                    ))));
                    paragraph = with_attrs(&paragraph, attrs);
                }
                self.apply(Op::InsertParagraph { index, paragraph })?;
                index += 1;
            }
            for note in &copied.notes {
                let id = notes[&note.id];
                let paragraphs: Vec<Paragraph> = note
                    .paragraphs
                    .iter()
                    .map(|p| {
                        let new = self.new_id();
                        renumbered(p, new, &notes)
                    })
                    .collect();
                self.set_note(
                    id,
                    Some(Note {
                        id,
                        mark: note.mark.clone(),
                        paragraphs: paragraphs.into(),
                    }),
                )?;
            }
            if let (Some(first), Some(margins)) = (first_id, &copied.margins) {
                self.set_margins(Some(first), margins)?;
            }
        }
        if before == 0 {
            self.make_first()?;
        }
        Ok(before..before + clip.sections.len())
    }

    /// Duplicates some sections, each copy after the last of them, and
    /// gives the indices of the copies.
    pub fn duplicate_sections(&mut self, indices: &[usize]) -> Result<Range<usize>, ModelError> {
        let clip = self.document().copy_sections(indices);
        let after = indices.iter().max().map_or(0, |k| k + 1);
        self.paste_sections(after, &clip)
    }

    /// Moves some sections before section `before` (as counted before the
    /// move), and gives their indices after it.
    pub fn move_sections(
        &mut self,
        indices: &[usize],
        before: usize,
    ) -> Result<Range<usize>, ModelError> {
        let clip = self.document().copy_sections(indices);
        let gone = indices.iter().filter(|&&k| k < before).count();
        let count = self.document().section_ranges().len();
        if clip.len() == count {
            return Ok(0..count);
        }
        self.delete_sections(indices)?;
        self.paste_sections(before - gone, &clip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{Position, Run};

    /// A document of three sections, "A", "B" and "C", the second with a
    /// header of its own.
    fn three() -> Document {
        let mut document = Document::new_word_processing();
        document
            .transact("Sections", |edit| {
                let first = edit.document().body().get(0).unwrap().id();
                edit.insert_text(Position::new(first, 0), "A")?;
                let style = edit.document().styles().default_paragraph();
                let setup = *edit.document().page_setup();
                for (i, text) in ["B", "C"].iter().enumerate() {
                    let id = edit.new_id();
                    let mut attrs = crate::ParaAttrs::default();
                    let section = Section {
                        own_margins: i == 0,
                        ..setup.section
                    };
                    attrs.start = Some(Some(Arc::new(Start::Section(section, setup.layout))));
                    let paragraph = Paragraph::from_runs(id, style, attrs, [Run::new(text)])?;
                    let index = edit.document().body().len();
                    edit.apply(Op::InsertParagraph { index, paragraph })?;
                    if i == 0 {
                        let header = Paragraph::from_runs(
                            edit.new_id(),
                            style,
                            Default::default(),
                            [Run::new("Head B")],
                        )?;
                        edit.set_section_header_footer(
                            Some(id),
                            MarginPlace::ALL[0],
                            vec![header],
                        )?;
                    }
                }
                Ok(())
            })
            .unwrap();
        document
    }

    fn texts(document: &Document) -> Vec<String> {
        document.body().iter().map(Paragraph::text).collect()
    }

    #[test]
    fn est_17_sections_are_ranges_of_paragraphs() {
        let document = three();
        assert_eq!(document.section_ranges(), vec![0..1, 1..2, 2..3]);
    }

    #[test]
    fn txt_02_deleting_the_first_section_makes_the_next_first() {
        let mut document = three();
        document
            .transact("Delete", |edit| edit.delete_sections(&[0]))
            .unwrap();
        assert_eq!(texts(&document), ["B", "C"]);
        assert_eq!(document.section_ranges().len(), 2);
        // Its header is now that of the document.
        assert_eq!(
            document.headers_footers().get(MarginPlace::ALL[0])[0].text(),
            "Head B"
        );
        assert!(document.body().get(0).unwrap().attrs().start.is_none());
        document
            .transact("Delete", |edit| edit.delete_sections(&[0, 1]))
            .unwrap();
        assert_eq!(texts(&document), [""]);
    }

    #[test]
    fn est_17_sections_move_and_duplicate() {
        let mut document = three();
        document
            .transact("Move", |edit| edit.move_sections(&[2], 0))
            .unwrap();
        assert_eq!(texts(&document), ["C", "A", "B"]);
        assert_eq!(document.section_ranges().len(), 3);
        // The first section keeps the headers of the document, now its
        // own.
        let a = document.body().get(1).unwrap();
        assert!(matches!(
            a.attrs().start.as_ref().and_then(|s| s.as_deref()),
            Some(Start::Section(section, _)) if section.own_margins
        ));
        document
            .transact("Duplicate", |edit| edit.duplicate_sections(&[2]))
            .unwrap();
        assert_eq!(texts(&document), ["C", "A", "B", "B"]);
        let copy = document.body().get(3).unwrap().id();
        assert_ne!(copy, document.body().get(2).unwrap().id());
        assert_eq!(
            document
                .section_headers_footers(Some(copy))
                .get(MarginPlace::ALL[0])[0]
                .text(),
            "Head B"
        );
        let clip = document.copy_sections(&[0, 1]);
        document
            .transact("Paste", |edit| edit.paste_sections(4, &clip))
            .unwrap();
        assert_eq!(texts(&document), ["C", "A", "B", "B", "C", "A"]);
    }
}
