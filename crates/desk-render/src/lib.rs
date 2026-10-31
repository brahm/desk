// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Display list, effects and the Cairo back end.
//!
//! Everything the apps draw goes through one display list, which the Cairo
//! back end paints on screen, for printing and into PDF files. Shadows,
//! reflections and the other object effects are part of the display list.
//!
//! For now the display list holds glyphs and rectangles, enough for the
//! text of a word processing document, and the back end writes it as PDF
//! with the fonts embedded.
//!
//! ```no_run
//! use desk_layout::paginate;
//! use desk_model::Document;
//! use desk_render::{DisplayList, Painter};
//! use desk_text::TextSystem;
//!
//! let document = Document::new_word_processing();
//! let layout = paginate(&document, &mut TextSystem::new());
//! let pages = DisplayList::pages(&layout);
//! Painter::new()?.write_pdf(&pages, document.metadata(), "out.pdf")?;
//! # Ok::<(), desk_render::RenderError>(())
//! ```

mod backend;
mod display;

/// The Cairo bindings, for painting on a context of one's own.
pub use cairo;

pub use backend::{Painter, RenderError};
pub use display::{DisplayList, Item, PlacedGlyph};
