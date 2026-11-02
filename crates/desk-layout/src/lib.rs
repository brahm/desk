// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pagination and composition of pages, slides and sheets.
//!
//! Columns, text wrap around objects, page breaks and the placement of
//! everything on a page, a slide or a sheet, including the print view of
//! Desk Tables.
//!
//! For now it lays out the body of a word processing document on pages:
//! margins, the space between paragraphs, page breaks and the rules that
//! keep lines and paragraphs together.
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

mod pages;

pub use pages::{DocumentLayout, Page, PlacedBlock, PlacedLine, paginate, paginate_texts};
