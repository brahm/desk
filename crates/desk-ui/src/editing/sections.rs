// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whole sections and pages, as the page thumbnails and Edit ▸ Delete
//! Page change them (EST-17, TXT-02).

use std::cell::RefCell;

use desk_model::{ModelError, SectionsClip};

use super::{Selection, Spot, TextEditor};

thread_local! {
    /// The sections last cut or copied in the thumbnails.
    static CLIP: RefCell<Option<SectionsClip>> = const { RefCell::new(None) };
}

/// Whether sections were cut or copied, to paste.
pub fn has_sections_clip() -> bool {
    CLIP.with(|c| c.borrow().is_some())
}

impl TextEditor {
    /// Whether the pages face each other, a left and a right page (EST-11).
    pub fn facing_pages(&self) -> bool {
        Self::main_document(&self.state.borrow())
            .page_setup()
            .facing_pages
    }

    /// The section of each page, by its index among the sections.
    pub fn page_sections(&self) -> Vec<usize> {
        let state = self.state.borrow();
        let ranges = Self::main_document(&state).section_ranges();
        let layout = Self::main_layout(&state);
        let mut out = Vec::with_capacity(layout.pages.len());
        let mut last = 0;
        for page in &layout.pages {
            if let Some(line) = page.lines.first() {
                last = ranges
                    .iter()
                    .position(|r| r.contains(&line.paragraph))
                    .unwrap_or(last);
            }
            out.push(last);
        }
        out
    }

    /// The page the cursor is on, in the body.
    pub fn cursor_page(&self) -> Option<usize> {
        let state = self.state.borrow();
        let spot = match &state.story {
            Some(mode) => mode.body.focus,
            None => state.selection.focus,
        };
        crate::text_nav::caret(Self::main_layout(&state), spot).map(|c| c.page)
    }

    /// Changes whole sections as one action, and puts the cursor at the
    /// start of section `then` as the change leaves them.
    fn section_edit(
        &self,
        name: &str,
        change: impl FnOnce(&mut desk_model::Edit) -> Result<usize, ModelError>,
    ) {
        self.leave_story(None);
        let before = self.selection();
        let result = self.state.borrow_mut().editor.edit(name, |edit| {
            let then = change(edit)?;
            let ranges = edit.document().section_ranges();
            let paragraph = ranges.get(then).or(ranges.last()).map_or(0, |r| r.start);
            Ok(Some(Spot::new(paragraph, 0)))
        });
        self.finish_edit(before, result);
    }

    /// Copies sections, by index, to paste them.
    pub fn copy_sections(&self, indices: &[usize]) {
        let clip = Self::main_document(&self.state.borrow()).copy_sections(indices);
        CLIP.with(|c| *c.borrow_mut() = Some(clip));
    }

    /// Cuts sections.
    pub fn cut_sections(&self, indices: &[usize]) {
        self.copy_sections(indices);
        self.delete_sections(indices);
    }

    /// Deletes sections.
    pub fn delete_sections(&self, indices: &[usize]) {
        let first = indices.iter().copied().min().unwrap_or(0);
        let name = if indices.len() == 1 {
            "Delete Section"
        } else {
            "Delete Sections"
        };
        self.section_edit(name, |edit| {
            edit.delete_sections(indices)?;
            Ok(first)
        });
    }

    /// Pastes the sections cut or copied before section `before`, or at
    /// the end when it is their count.
    pub fn paste_sections(&self, before: usize) {
        let Some(clip) = CLIP.with(|c| c.borrow().clone()) else {
            return;
        };
        self.section_edit("Paste Sections", |edit| {
            Ok(edit.paste_sections(before, &clip)?.start)
        });
    }

    /// Duplicates sections, after the last of them.
    pub fn duplicate_sections(&self, indices: &[usize]) {
        self.section_edit("Duplicate", |edit| {
            Ok(edit.duplicate_sections(indices)?.start)
        });
    }

    /// Moves sections before section `before`, or copies them there.
    pub fn move_sections(&self, indices: &[usize], before: usize, copy: bool) {
        if copy {
            let clip = Self::main_document(&self.state.borrow()).copy_sections(indices);
            self.section_edit("Copy Sections", |edit| {
                Ok(edit.paste_sections(before, &clip)?.start)
            });
        } else {
            self.section_edit("Move Sections", |edit| {
                Ok(edit.move_sections(indices, before)?.start)
            });
        }
    }

    /// The text of the body on a page: from the start of its first line
    /// to the end of its last.
    pub fn page_text_range(&self, page: usize) -> Option<(Spot, Spot)> {
        let state = self.state.borrow();
        let layout = Self::main_layout(&state);
        let lines = &layout.pages.get(page)?.lines;
        let (first, last) = (lines.first()?, lines.last()?);
        Some((
            Spot::new(first.paragraph, first.line.range.start),
            Spot::new(last.paragraph, last.line.range.end),
        ))
    }

    /// Deletes what a page holds (Edit ▸ Delete Page, TXT-02): its
    /// section, when the page is all of it, or else its text.
    pub fn delete_page(&self, page: usize) {
        self.leave_story(None);
        let sections = self.page_sections();
        let Some(&section) = sections.get(page) else {
            return;
        };
        if sections.iter().filter(|&&s| s == section).count() == 1 {
            self.delete_sections(&[section]);
            return;
        }
        let Some((start, mut end)) = self.page_text_range(page) else {
            return;
        };
        // The break at the end of the last paragraph goes too, unless the
        // next page starts in the same paragraph.
        let next = self.page_text_range(page + 1);
        if next.is_none_or(|(n, _)| n.paragraph > end.paragraph) {
            let count = self.state.borrow().editor.document().body().len();
            if end.paragraph + 1 < count {
                end = Spot::new(end.paragraph + 1, 0);
            }
        }
        self.select(Selection {
            anchor: start,
            focus: end,
        });
        self.replace_selection("Delete Page", "");
    }
}
