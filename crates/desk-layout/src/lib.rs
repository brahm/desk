// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pagination and composition of pages, slides and sheets.
//!
//! Columns, text wrap around objects, page breaks and the placement of
//! everything on a page, a slide or a sheet, including the print view of
//! Desk Tables.
//!
//! For now it lays out the body of a word processing document on pages:
//! margins, the space between paragraphs, page breaks, the rules that
//! keep lines and paragraphs together, and the floating objects that the
//! text goes around.
//!
//! ```no_run
//! use desk_layout::paginate;
//! use desk_model::Document;
//! use desk_text::TextSystem;
//!
//! let document = Document::new_word_processing();
//! let layout = paginate(&document, &mut TextSystem::new());
//! println!("{} pages", layout.pages.len());
//! ```

mod joins;
mod notes;
mod objects;
mod outline;
mod pages;
mod screen;
mod tables;

/// The tracked changes whose text does not show as the document shows:
/// the deletions, in the views of markup without them (W148, REV-04).
pub(crate) fn hidden_changes(
    document: &desk_model::Document,
) -> Option<std::sync::Arc<std::collections::HashSet<desk_model::NodeId>>> {
    if document.markup().shows_deletions() {
        return None;
    }
    let hidden: std::collections::HashSet<desk_model::NodeId> = document
        .changes()
        .iter()
        .filter(|c| c.kind == desk_model::ChangeKind::Deletion)
        .map(|c| c.id)
        .collect();
    (!hidden.is_empty()).then(|| std::sync::Arc::new(hidden))
}

pub use objects::{PlacedObject, flow_text_boxes, object_at, place_object_text, text_box_lines};
pub use outline::{outline_document, paginate_outline, paginate_view};
pub use screen::{SCREEN_MARGIN, paginate_screen, screen_document};
pub use tables::{
    CellLayout, ChartLabel, ChartLayout, TableLayout, document_locale, lay_out_chart,
    lay_out_table, table_size, today,
};

pub use pages::{
    DocumentLayout, NOTE_RULE_LENGTH, NOTE_RULE_WIDTH, Page, PlacedBlock, PlacedLine, field_span,
    margin_lines, margin_lines_split, margin_of_page, paginate, paginate_texts,
    paginate_texts_spaced,
};
