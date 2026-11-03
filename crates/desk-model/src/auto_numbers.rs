// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Auto Page Numbers (EST-27): a header or a footer that holds only the
//! number of the page, put in the place, alignment and words that Pages'
//! dialog offers, in the whole document or in one section.

use std::sync::Arc;

use crate::error::ModelError;
use crate::format::{Alignment, ParaAttrs};
use crate::header::{MarginKind, MarginPages, MarginPlace};
use crate::id::NodeId;
use crate::section::{Section, Start};
use crate::text::{Paragraph, Position};
use crate::transaction::Edit;

/// Where on the line the numbers go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NumberAlignment {
    /// On the left.
    Left,
    /// In the middle.
    #[default]
    Center,
    /// On the right.
    Right,
    /// Towards the spine: on the left of right pages and the right of
    /// left pages.
    Inside,
    /// Away from the spine.
    Outside,
}

impl NumberAlignment {
    /// The alignment of the line on right pages and on left ones.
    pub fn of_pages(self) -> (Alignment, Alignment) {
        match self {
            Self::Left => (Alignment::Left, Alignment::Left),
            Self::Center => (Alignment::Center, Alignment::Center),
            Self::Right => (Alignment::Right, Alignment::Right),
            Self::Inside => (Alignment::Left, Alignment::Right),
            Self::Outside => (Alignment::Right, Alignment::Left),
        }
    }

    /// Whether left pages differ from right ones.
    pub fn mirrored(self) -> bool {
        matches!(self, Self::Inside | Self::Outside)
    }
}

/// The choices of the dialog of Auto Page Numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AutoPageNumbers {
    /// In the header or the footer.
    pub kind: MarginKind,
    /// Where on the line.
    pub alignment: NumberAlignment,
    /// Whether the first page of a section shows its number too.
    pub first_page: bool,
    /// Whether every section gets them, or only the one at the cursor.
    pub whole_document: bool,
}

impl Edit {
    /// Puts the numbers of pages in the header or footer of the section of
    /// paragraph `at` of the body, or of every section. `line` is the
    /// paragraph that the numbers show, with its fields and words; its
    /// alignment is that of the choices.
    pub fn set_auto_page_numbers(
        &mut self,
        at: usize,
        numbers: AutoPageNumbers,
        line: &Paragraph,
    ) -> Result<(), ModelError> {
        let document = self.document();
        let sections: Vec<Option<usize>> = if numbers.whole_document {
            std::iter::once(None)
                .chain(
                    (1..document.body().len())
                        .filter(|&i| {
                            matches!(
                                document.body().get(i).and_then(|p| p.attrs().start.clone()),
                                Some(Some(start)) if matches!(*start, Start::Section(..))
                            )
                        })
                        .map(Some),
                )
                .collect()
        } else {
            vec![document.section_at(at).0]
        };
        let keys: Vec<Option<NodeId>> = if numbers.whole_document {
            let mut keys: Vec<Option<NodeId>> = sections
                .iter()
                .map(|&index| document.headers_section(index.unwrap_or(0)))
                .collect();
            keys.dedup();
            keys
        } else {
            vec![document.headers_section(at)]
        };
        let other = match numbers.kind {
            MarginKind::Header => MarginKind::Footer,
            MarginKind::Footer => MarginKind::Header,
        };
        let mut left = numbers.alignment.mirrored();
        for index in sections {
            let section = self.section_of(index);
            left |= section.left_right_different;
            let mut changed = section;
            changed.first_page_different |= !numbers.first_page;
            changed.left_right_different |= numbers.alignment.mirrored();
            if changed == section {
                continue;
            }
            // Pages that now have a header and footer of their own keep
            // showing what they showed: the other one is copied.
            let key = self.document().headers_section(index.unwrap_or(0));
            for (turned_on, pages) in [
                (
                    changed.first_page_different && !section.first_page_different,
                    MarginPages::First,
                ),
                (
                    changed.left_right_different && !section.left_right_different,
                    MarginPages::Left,
                ),
            ] {
                let place = MarginPlace::new(other, pages);
                let existing = self.document().section_headers_footers(key).get(place);
                if turned_on && existing.is_empty() {
                    let default = self
                        .document()
                        .section_headers_footers(key)
                        .get(MarginPlace::new(other, MarginPages::Default))
                        .to_vec();
                    let copy = self.copy_paragraphs(&default, None)?;
                    self.set_section_header_footer(key, place, copy)?;
                }
            }
            self.set_section_of(index, changed)?;
        }
        let (right_alignment, left_alignment) = numbers.alignment.of_pages();
        for key in keys {
            let right = self.copy_paragraphs(std::slice::from_ref(line), Some(right_alignment))?;
            let place = |pages| MarginPlace::new(numbers.kind, pages);
            self.set_section_header_footer(key, place(MarginPages::Default), right.clone())?;
            if left {
                let paragraphs =
                    self.copy_paragraphs(std::slice::from_ref(line), Some(left_alignment))?;
                self.set_section_header_footer(key, place(MarginPages::Left), paragraphs)?;
            }
            let first = if numbers.first_page {
                self.copy_paragraphs(&right, None)?
            } else {
                Vec::new()
            };
            self.set_section_header_footer(key, place(MarginPages::First), first)?;
        }
        let mut setup = *self.document().page_setup();
        match numbers.kind {
            MarginKind::Header => setup.headers = true,
            MarginKind::Footer => setup.footers = true,
        }
        self.set_page_setup(setup)
    }

    /// Copies of paragraphs, with identifiers of their own and an
    /// alignment, if given.
    fn copy_paragraphs(
        &mut self,
        paragraphs: &[Paragraph],
        alignment: Option<Alignment>,
    ) -> Result<Vec<Paragraph>, ModelError> {
        paragraphs
            .iter()
            .map(|p| {
                let attrs = ParaAttrs {
                    alignment: alignment.or(p.attrs().alignment),
                    ..p.attrs().without_structure()
                };
                Paragraph::from_runs(self.new_id(), p.style(), attrs, p.runs().cloned())
            })
            .collect()
    }

    /// The section started by paragraph `index` of the body, or the first
    /// one.
    fn section_of(&self, index: Option<usize>) -> Section {
        let document = self.document();
        match index {
            None => document.page_setup().section,
            Some(i) => document.section_at(i).1,
        }
    }

    /// Changes the section started by paragraph `index`, or the first one.
    fn set_section_of(&mut self, index: Option<usize>, section: Section) -> Result<(), ModelError> {
        let document = self.document();
        match index {
            None => {
                let mut setup = *document.page_setup();
                setup.section = section;
                self.set_page_setup(setup)
            }
            Some(i) => {
                let out = ModelError::IndexOutOfBounds {
                    index: i,
                    len: document.body().len(),
                };
                let paragraph = document.body().get(i).ok_or(out.clone())?;
                let layout = *paragraph
                    .attrs()
                    .start
                    .clone()
                    .flatten()
                    .ok_or(out)?
                    .layout();
                let at = Position::new(paragraph.id(), 0);
                self.set_paragraph_attrs(
                    at,
                    at,
                    &ParaAttrs {
                        start: Some(Some(Arc::new(Start::Section(section, layout)))),
                        ..ParaAttrs::default()
                    },
                )
            }
        }
    }
}
