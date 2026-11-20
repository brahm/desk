// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Screen View (PXV-01): the text in one flow as wide as the window,
//! without the edges of pages, their margins, headers and footers. The
//! objects show in the flow, where they cannot be moved; the flow follows
//! the width of the window and its zoom, which sets the size of the text.

use desk_model::Anchor;

use super::TextEditor;

impl TextEditor {
    /// Shows the text in one flow as wide as the view, or as pages.
    pub fn set_screen_view(&self, on: bool) {
        if on == self.screen_view() {
            self.view.set_screen(on);
            return;
        }
        if self.story().is_some() {
            self.leave_story(None);
        }
        if on && self.has_selected_objects() {
            self.select_objects(Vec::new());
        }
        self.view.set_screen(on);
        self.state.borrow_mut().screen = on.then(|| self.view.screen_width());
        self.lay_out_again();
    }

    /// Whether Screen View is on.
    pub fn screen_view(&self) -> bool {
        self.state.borrow().screen.is_some()
    }

    /// Whether Screen View hides something of the document: its headers
    /// and footers, or the objects of its masters.
    pub fn screen_hides_content(&self) -> bool {
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        let pages = crate::lay_out(document);
        pages.pages.iter().any(|p| !p.margin_lines.is_empty())
            || document
                .objects()
                .iter()
                .any(|o| matches!(o.anchor, Anchor::Master(..)))
    }

    /// Lays out the flow again when the view changes width or zoom.
    pub(super) fn follow_screen_width(&self) {
        let weak = self.downgrade();
        self.view.connect_width_changed(move || {
            let Some(this) = weak.upgrade() else { return };
            let Some(width) = this.state.borrow().screen else {
                return;
            };
            let now = this.view.screen_width();
            if (now - width).abs() >= 0.5 {
                this.state.borrow_mut().screen = Some(now);
                this.lay_out_again();
            }
        });
    }

    /// Lays out the document again as the view shows it, keeping the
    /// selection.
    fn lay_out_again(&self) {
        {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            state.layout = state.lay_out(state.editor.document());
        }
        self.show_pages();
        self.show_objects();
        self.update_squiggles();
        self.show_selection();
    }
}
