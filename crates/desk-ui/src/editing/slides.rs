// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor does to the slides of a presentation of Desk Slides:
//! a new slide, the presenter notes (KSLD-15) and skipping slides
//! (KSLD-06), each as one action of the history.

use super::TextEditor;

impl TextEditor {
    /// Whether the document is a presentation.
    pub fn is_presentation(&self) -> bool {
        Self::main_document(&self.state.borrow()).is_presentation()
    }

    /// The number of slides, or of pages.
    pub fn slide_count(&self) -> usize {
        Self::main_document(&self.state.borrow())
            .page_setup()
            .layout_pages
            .map_or(0, |n| n as usize)
    }

    /// The slide in sight, or of the object selected.
    pub fn current_slide(&self) -> usize {
        self.cursor_page()
            .unwrap_or(0)
            .min(self.slide_count().saturating_sub(1))
    }

    /// Whether nothing is selected: no object, and no text being edited,
    /// so that what is chosen is the slide itself (KINT-13).
    pub fn nothing_selected(&self) -> bool {
        self.is_page_layout() && !self.has_selected_objects() && self.story().is_none()
    }

    /// The presenter notes of a slide.
    pub fn slide_notes(&self, slide: usize) -> String {
        let state = self.state.borrow();
        Self::main_document(&state)
            .slide(u32::try_from(slide).unwrap_or(u32::MAX))
            .map(|s| s.notes.clone())
            .unwrap_or_default()
    }

    /// Whether the presentation skips a slide when it plays.
    pub fn slide_skipped(&self, slide: usize) -> bool {
        let state = self.state.borrow();
        Self::main_document(&state)
            .slide(u32::try_from(slide).unwrap_or(u32::MAX))
            .is_some_and(|s| s.skipped)
    }

    /// Sets the presenter notes of a slide, as one action (KSLD-15).
    pub fn set_slide_notes(&self, slide: usize, notes: &str) {
        if self.slide_notes(slide) == notes {
            return;
        }
        let slide = u32::try_from(slide).unwrap_or(u32::MAX);
        self.keeping_selection("Edit Presenter Notes", |edit| {
            edit.set_slide_notes(slide, notes)
        });
    }

    /// Skips slides when the presentation plays, or stops skipping them
    /// (KSLD-06).
    pub fn set_slides_skipped(&self, slides: &[usize], skipped: bool) {
        let slides: Vec<u32> = slides
            .iter()
            .filter_map(|&s| u32::try_from(s).ok())
            .collect();
        let name = if skipped {
            "Skip Slide"
        } else {
            "Stop Skipping Slide"
        };
        self.keeping_selection(name, |edit| edit.set_slides_skipped(&slides, skipped));
    }

    /// Adds a blank slide after the one in sight and shows it (KSLD-01).
    pub fn new_slide(&self) {
        let slide = self.current_slide() + 1;
        self.add_layout_page();
        // The view scrolls once it has made room for the new slide.
        let view = self.view.clone();
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
            view.scroll_to_page(slide);
        });
    }

    /// Changes the slides as one action, keeping what is selected.
    fn keeping_selection(
        &self,
        name: &str,
        change: impl FnOnce(&mut desk_model::Edit) -> Result<(), desk_model::ModelError>,
    ) {
        let selection = self.selection();
        let depth = self.state.borrow().editor.undo_depth();
        let result = self.state.borrow_mut().editor.edit(name, change);
        self.state.borrow_mut().typing = false;
        match result {
            Ok(()) if self.state.borrow().editor.undo_depth() > depth => {
                {
                    let mut state = self.state.borrow_mut();
                    let action = state.next_action;
                    state.next_action += 1;
                    state.undo.push((selection, selection, action));
                    state.redo.clear();
                }
                self.after_change(selection);
            }
            Ok(()) => {}
            Err(error) => gtk::glib::g_warning!("desk", "cannot change the slides: {error}"),
        }
    }
}
