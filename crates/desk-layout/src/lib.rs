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

mod notes;
mod objects;
mod pages;

pub use objects::{PlacedObject, object_at, place_object_text, text_box_lines};

pub use pages::{
    DocumentLayout, NOTE_RULE_LENGTH, NOTE_RULE_WIDTH, Page, PlacedBlock, PlacedLine, margin_lines,
    margin_of_page, paginate, paginate_texts, paginate_texts_spaced,
};
