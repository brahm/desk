// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chart styles of the selected chart (W080): giving it a style,
//! Create Chart Style, and Copy and Paste Chart Style, which pass the look
//! of one chart to another, in this document or another one.

use std::cell::RefCell;

use desk_model::ChartStyle;

use super::TextEditor;

thread_local! {
    /// The look that Copy Chart Style took, for Paste Chart Style in any
    /// window.
    static COPIED: RefCell<Option<ChartStyle>> = const { RefCell::new(None) };
}

impl TextEditor {
    /// The chart styles of the document, in order.
    pub fn chart_styles(&self) -> Vec<ChartStyle> {
        self.document().chart_styles().to_vec()
    }

    /// The first chart style that the selected chart looks like, if any.
    pub fn current_chart_style(&self) -> Option<usize> {
        let chart = self.selected_chart()?;
        self.document()
            .chart_styles()
            .iter()
            .position(|s| s.fits(&chart))
    }

    /// Gives the selected chart the look of a chart style.
    pub fn apply_chart_style(&self, index: usize) {
        let Some(style) = self.document().chart_styles().get(index).cloned() else {
            return;
        };
        self.change_chart("Chart Style", |chart| style.apply_to(chart));
    }

    /// Takes the look of the selected chart, for Paste Chart Style.
    pub fn copy_chart_style(&self) {
        if let Some(chart) = self.selected_chart() {
            COPIED.with(|c| *c.borrow_mut() = Some(ChartStyle::of(&chart)));
        }
    }

    /// Whether Copy Chart Style took a look.
    pub fn can_paste_chart_style(&self) -> bool {
        COPIED.with(|c| c.borrow().is_some())
    }

    /// Gives the selected chart the look that Copy Chart Style took.
    pub fn paste_chart_style(&self) {
        let Some(style) = COPIED.with(|c| c.borrow().clone()) else {
            return;
        };
        self.change_chart("Paste Chart Style", |chart| style.apply_to(chart));
    }

    /// Adds the look of the selected chart after the chart styles, as
    /// Create Chart Style does.
    pub fn create_chart_style(&self) {
        if self.selected_chart().is_none() {
            return;
        }
        self.arrange_objects("Create Chart Style", |edit, ids| {
            edit.create_chart_style(ids[0])?;
            Ok(None)
        });
    }
}
