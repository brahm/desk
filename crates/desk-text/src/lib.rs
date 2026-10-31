// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Text shaping and paragraph layout.
//!
//! Shaping, line breaking, bidirectional text, hyphenation and the layout
//! of a single paragraph. Pages, table cells and slides all lay out their
//! text through this crate.
//!
//! It builds on [Parley](https://github.com/linebender/parley), which
//! shapes the text with HarfRust, picks fonts through fontconfig, places
//! the glyphs and aligns the lines. Desk decides where each line ends,
//! which keeps hyphens and the space around wrapped objects under its
//! control. `experiments/text-engine` has the comparison behind this
//! choice.
//!
//! The crate knows nothing about pages: a [`LineSpace`] says where each
//! line may go, and the caller places paragraphs one after another.
//!
//! ```no_run
//! use desk_model::Document;
//! use desk_text::{Column, ParagraphText, TextSystem};
//!
//! let document = Document::new_word_processing();
//! let paragraph = document.body().get(0).unwrap();
//! let text = ParagraphText::new(document.styles(), paragraph);
//! let mut system = TextSystem::new();
//! let layout = system.layout(&text, &Column::new(451.0), 0.0);
//! println!("{} lines", layout.lines.len());
//! ```

mod input;
mod layout;
mod space;

pub use input::{ParagraphText, SMALL_CAPS_SCALE};
pub use layout::{
    Decoration, Glyph, GlyphRun, Line, MIN_SEGMENT_WIDTH, ParagraphLayout, SCRIPT_SCALE,
    SOFT_HYPHEN, TextSystem,
};
pub use parley::FontData;
pub use space::{Column, LineSpace};
