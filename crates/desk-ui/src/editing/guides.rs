// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The layout view, which outlines the body, the columns, the headers,
//! the footers and the text boxes (INT-09, EST-03), and the guides fixed
//! on the pages, dragged out of the rulers and off the page again
//! (OBJ-28).

use desk_layout::DocumentLayout;
use desk_model::{Document, FixedGuide, Length};
use gtk::prelude::*;

use super::TextEditor;
use crate::view::Outline;

/// How near a fixed guide a press must be to take it, in pixels.
const REACH: f64 = 4.0;

/// A fixed guide being dragged along the page, or out of a ruler.
#[derive(Clone, Copy, Debug)]
pub(super) struct GuideDrag {
    /// The guide it moves, by its index, or none for one coming out of a
    /// ruler.
    index: Option<usize>,
    /// Where it is now, or none while it is off the pages, to go when it
    /// is let go.
    guide: Option<FixedGuide>,
}

/// The boxes that the layout view outlines on each page: the body, its
/// columns, the header and the footer, and the text boxes; only the text
/// boxes in a page layout document (INT-09).
pub(super) fn outlines(document: &Document, layout: &DocumentLayout) -> Vec<Outline> {
    let setup = document.page_setup();
    let width = setup.width.to_pt();
    let height = setup.height.to_pt();
    let mut out = Vec::new();
    for (index, page) in layout.pages.iter().enumerate() {
        let mut add = |x: f64, y: f64, w: f64, h: f64| {
            if w > 0.0 && h > 0.0 {
                out.push(Outline {
                    page: index,
                    rect: (x, y, w, h),
                });
            }
        };
        if !setup.is_page_layout() {
            let left = f64::from(page.margins.0);
            let right = width - f64::from(page.margins.1);
            let (top, bottom) = (f64::from(page.body.0), f64::from(page.body.1));
            add(left, top, right - left, bottom - top);
            if setup.headers && !page.blank {
                let from = setup.header_margin.to_pt();
                add(left, from, right - left, top - from);
            }
            if setup.footers && !page.blank {
                let to = height - setup.footer_margin.to_pt();
                add(left, bottom, right - left, to - bottom);
            }
            // The columns of each layout on the page, from its first line
            // to its last, or to the edge of the body at either end.
            let mut runs: Vec<(Option<usize>, desk_model::Layout, f64, f64)> = Vec::new();
            for line in &page.lines {
                let (start, layout) = document.layout_at(line.paragraph);
                let (y0, y1) = (f64::from(line.top()), f64::from(line.bottom()));
                match runs.last_mut() {
                    Some(run) if run.0 == start => {
                        run.2 = run.2.min(y0);
                        run.3 = run.3.max(y1);
                    }
                    _ => runs.push((start, layout, y0, y1)),
                }
            }
            let count = runs.len();
            for (i, (_, layout, y0, y1)) in runs.into_iter().enumerate() {
                if layout.columns.count < 2 {
                    continue;
                }
                let y0 = if i == 0 { top } else { y0 };
                let y1 = if i + 1 == count { bottom } else { y1 };
                let from = left + layout.left.to_pt();
                let across = right - from - layout.right.to_pt();
                for (x, w) in layout.columns.spans(across.max(1.0)) {
                    add(from + x, y0, w, y1 - y0);
                }
            }
        }
        for placed in &page.objects {
            if placed.object.text_box().is_some() {
                add(
                    f64::from(placed.x),
                    f64::from(placed.y),
                    f64::from(placed.width),
                    f64::from(placed.height),
                );
            }
        }
    }
    out
}

impl TextEditor {
    /// Shows or hides the layout view (INT-09).
    pub fn set_show_layout(&self, show: bool) {
        self.state.borrow_mut().show_layout = show;
        self.show_outlines();
    }

    /// Shows the outlines of the layout view, when it is on.
    pub(super) fn show_outlines(&self) {
        let outlines = {
            let state = self.state.borrow();
            if state.show_layout {
                outlines(Self::main_document(&state), Self::main_layout(&state))
            } else {
                Vec::new()
            }
        };
        self.view.set_outlines(outlines);
    }

    /// Shows the guides fixed on the pages, but one being moved, which
    /// shows where it goes.
    pub(super) fn show_guides(&self) {
        let (guides, dragged) = {
            let state = self.state.borrow();
            let mut guides = Self::main_document(&state).text_settings().guides.clone();
            let drag = state.guide_drag;
            if let Some(index) = drag.and_then(|d| d.index)
                && index < guides.len()
            {
                guides.remove(index);
            }
            (guides, drag.and_then(|d| d.guide))
        };
        self.view.set_fixed_guides(guides, dragged);
    }

    /// Follows a guide dragged out of a ruler, to a point of the ruler:
    /// one across the page from the ruler above the pages, or down it
    /// from the one beside them. It shows only while the pointer is over
    /// a page.
    pub fn drag_guide_from_ruler(
        &self,
        ruler: &impl IsA<gtk::Widget>,
        vertical: bool,
        x: f64,
        y: f64,
    ) {
        let guide = self
            .view
            .page_at_widget(ruler, x, y)
            .filter(|(_, _, _, inside)| *inside)
            .and_then(|(page, px, py, _)| {
                let (ax, ay) = self.view.to_area(page, px, py)?;
                self.guide_at(vertical, ax, ay)
            });
        self.state.borrow_mut().guide_drag = Some(GuideDrag { index: None, guide });
        self.show_guides();
    }

    /// Ends a drag of a guide, out of a ruler or along the page: the guide
    /// stays where it is let go, or goes when it is off the pages.
    pub fn end_guide_drag(&self) {
        let Some(drag) = self.state.borrow_mut().guide_drag.take() else {
            return;
        };
        let guides = self.main_guides();
        let mut changed = guides.clone();
        if let Some(index) = drag.index
            && index < changed.len()
        {
            changed.remove(index);
        }
        if let Some(guide) = drag.guide {
            changed.push(guide);
        }
        changed.sort();
        changed.dedup();
        if changed == guides {
            self.show_guides();
            return;
        }
        if self.story().is_some() {
            self.leave_story(None);
        }
        self.change_text_settings("Guides", |settings| settings.guides = changed);
        self.show_guides();
    }

    fn main_guides(&self) -> Vec<FixedGuide> {
        Self::main_document(&self.state.borrow())
            .text_settings()
            .guides
            .clone()
    }

    /// Takes a fixed guide that a press on a page is on, to move it.
    pub(super) fn press_guide(&self, page: usize, x: f32, y: f32) -> bool {
        let reach = REACH / self.view.scale();
        let found = self.main_guides().iter().position(|g| {
            let along = if g.vertical { x } else { y };
            g.is_on(page) && (f64::from(along) - g.at.to_pt()).abs() <= reach
        });
        let Some(index) = found else {
            return false;
        };
        let guide = self.main_guides()[index];
        self.state.borrow_mut().guide_drag = Some(GuideDrag {
            index: Some(index),
            guide: Some(guide),
        });
        self.show_guides();
        true
    }

    /// Follows the pointer in a drag of a fixed guide along the page, to a
    /// point of the area. Whether such a drag is going on.
    pub(super) fn drag_guide(&self, x: f64, y: f64) -> bool {
        let Some(drag) = self.state.borrow().guide_drag else {
            return false;
        };
        let Some(vertical) = drag.guide.map(|g| g.vertical).or_else(|| {
            drag.index
                .and_then(|i| self.main_guides().get(i).map(|g| g.vertical))
        }) else {
            return true;
        };
        let guide = self.guide_at(vertical, x, y);
        self.state.borrow_mut().guide_drag = Some(GuideDrag { guide, ..drag });
        self.show_guides();
        true
    }

    /// The guide at a point of the area, or none off the pages.
    fn guide_at(&self, vertical: bool, x: f64, y: f64) -> Option<FixedGuide> {
        let (page, px, py) = self.view.page_at(x, y)?;
        let (top_x, top_y) = self.view.to_area(page, 0.0, 0.0)?;
        let scale = self.view.scale();
        let setup = *Self::main_document(&self.state.borrow()).page_setup();
        let (width, height) = (setup.width.to_pt(), setup.height.to_pt());
        let on =
            x >= top_x && y >= top_y && x <= top_x + width * scale && y <= top_y + height * scale;
        on.then(|| FixedGuide {
            page: Some(page as u32),
            vertical,
            at: Length::from_pt(if vertical { px } else { py }),
        })
    }
}

#[cfg(test)]
mod tests {
    use desk_model::{Columns, Document, Editor};

    use super::outlines;

    #[test]
    fn int_09_est_03_the_layout_view_outlines_the_body_the_margins_and_the_columns() {
        let mut editor = Editor::new(Document::new_word_processing());
        editor
            .edit("Columns", |edit| {
                let mut setup = *edit.document().page_setup();
                setup.layout.columns = Columns::equal(2, Columns::DEFAULT_GAP);
                edit.set_page_setup(setup)
            })
            .unwrap();
        let layout = crate::lay_out(editor.document());
        let setup = *editor.document().page_setup();
        let boxes: Vec<_> = outlines(editor.document(), &layout)
            .into_iter()
            .map(|o| o.rect)
            .collect();
        let (width, margin) = (setup.width.to_pt(), setup.margin_left.to_pt());
        // The body, inside the margins of an inch.
        let body = boxes[0];
        assert_eq!((body.0, body.1), (margin, setup.margin_top.to_pt()));
        assert!((body.2 - (width - 2.0 * margin)).abs() < 0.01);
        // The header and the footer, and the two columns of the body.
        assert_eq!(boxes.len(), 5);
        let columns = &boxes[3..];
        assert_eq!(columns[0].0, margin);
        assert!((columns[1].0 + columns[1].2 - (width - margin)).abs() < 0.01);
        assert!(columns[0].0 + columns[0].2 < columns[1].0);
    }

    #[test]
    fn obj_28_objects_snap_to_the_fixed_guides() {
        use desk_model::{FixedGuide, Length};
        let editor = Editor::new(Document::new_word_processing());
        let layout = crate::lay_out(editor.document());
        let guide = FixedGuide {
            page: Some(0),
            vertical: true,
            at: Length::from_pt(201.0),
        };
        let snaps = super::super::objects::Snaps::new(&layout, 0, &[]).with_fixed(&[guide]);
        let ((dx, _), _, _) = snaps.snap((199.0, 300.0, 230.0, 320.0), 5.0);
        assert!((dx - 2.0).abs() < 0.01);
        // A guide belongs to its page, as in Pages: the objects of another
        // page do not snap to it.
        let other = FixedGuide {
            page: Some(1),
            ..guide
        };
        let snaps = super::super::objects::Snaps::new(&layout, 0, &[]).with_fixed(&[other]);
        let ((dx, _), _, _) = snaps.snap((199.0, 300.0, 230.0, 320.0), 5.0);
        assert_eq!(dx, 0.0);
    }
}
