// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bookmarks that a link suggests (W101).
//!
//! A link to a bookmark offers the bookmarks of the document and, as
//! Pages 15.4 does, the paragraphs of the styles chosen in Manage Bookmark
//! Suggestions: choosing one marks the paragraph as a bookmark. A new
//! document suggests its paragraphs of Heading 2 and Heading 3, as the
//! Blank template of Pages does.

use crate::style::{StyleFamily, StyleId};
use crate::text::Paragraph;

/// The styles that a document suggests until it chooses its own.
pub const DEFAULT_BOOKMARK_SUGGESTIONS: [&str; 2] = ["Heading 2", "Heading 3"];

impl crate::Document {
    /// The paragraph styles whose paragraphs a link to a bookmark
    /// suggests: those chosen, or those of a new document by name.
    pub fn bookmark_suggestion_styles(&self) -> Vec<StyleId> {
        let styles = self.styles();
        match &self.text_settings().bookmark_suggestions {
            Some(chosen) => chosen
                .iter()
                .copied()
                .filter(|&id| styles.get(id).is_some())
                .collect(),
            None => DEFAULT_BOOKMARK_SUGGESTIONS
                .iter()
                .filter_map(|name| styles.by_name(StyleFamily::Paragraph, name))
                .map(|style| style.id())
                .collect(),
        }
    }

    /// The paragraphs of the body that a link to a bookmark suggests, by
    /// index, with their text: those of the styles suggested, outside
    /// tables of contents, that have text.
    pub fn suggested_bookmarks(&self) -> Vec<(usize, String)> {
        let styles = self.bookmark_suggestion_styles();
        if styles.is_empty() {
            return Vec::new();
        }
        self.body()
            .iter()
            .enumerate()
            .filter(|(_, p)| styles.contains(&p.style()) && p.attrs().toc_entry.flatten().is_none())
            .filter_map(|(i, p): (usize, &Paragraph)| {
                let text = crate::toc::entry_text(p);
                (!text.is_empty()).then_some((i, text))
            })
            .collect()
    }
}
