// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Document model shared by Desk Writer, Desk Tables and Desk Slides.
//!
//! The model holds the content and the styles of a document, the
//! transactions that change it, undo and redo, and snapshots. Text, sheets,
//! slides, master slides and floating objects are parts of one model, so an
//! object copied from one app to another stays the same object.
//!
//! The pieces:
//!
//! - A [`Document`] has [`Metadata`], [`Styles`] and a [`Container`], the
//!   part that each app defines. Desk Writer's container is a
//!   [`PagedFlow`]: a [`PageSetup`] and a body [`TextFlow`].
//! - A [`TextFlow`] is a sequence of [`Paragraph`]s, and a paragraph holds
//!   its text as [`Run`]s with the same formatting. Flows know nothing
//!   about pages, so text boxes, slides and table cells reuse them.
//! - Formatting is set as [`CharAttrs`] and [`ParaAttrs`] on styles and on
//!   the text, and resolved into [`CharFormat`] and [`ParaFormat`] through
//!   the inheritance of the styles.
//! - Every node has a stable [`NodeId`].
//! - Every change is an [`Op`] with an inverse. An [`Edit`] groups the
//!   operations of one user action into a [`Transaction`], and an
//!   [`Editor`] keeps the undo and redo history.
//! - Cloning a [`Document`] is cheap and gives a snapshot, because the
//!   content lives in persistent structures.
//!
//! ```
//! use desk_model::{CharAttrs, Document, Editor};
//!
//! let mut editor = Editor::new(Document::new_word_processing());
//! let start = editor.document().start();
//! let end = editor.edit("Typing", |edit| edit.insert_text(start, "Hello"))?;
//! editor.edit("Bold", |edit| {
//!     edit.set_character_attrs(start, end, &CharAttrs { bold: Some(true), ..Default::default() })
//! })?;
//! assert_eq!(editor.undo_name(), Some("Bold"));
//!
//! editor.undo()?;
//! editor.undo()?;
//! assert_eq!(editor.document(), &Document::new_word_processing());
//! # Ok::<(), desk_model::ModelError>(())
//! ```

mod document;
mod error;
mod format;
mod id;
mod op;
mod style;
mod text;
mod transaction;
mod units;

pub use document::{Container, Document, Metadata, PageSetup, PagedFlow, Statistics};
pub use error::ModelError;
pub use format::{
    Alignment, Baseline, Capitalization, CharAttrs, CharFormat, Language, LineColor, LineSpacing,
    LineStyle, ParaAttrs, ParaFormat,
};
pub use id::NodeId;
pub use op::Op;
pub use style::{CharacterStyle, ParagraphStyle, Style, StyleFamily, StyleId, Styles};
pub use text::{LINE_BREAK, NO_BREAK_SPACE, Paragraph, Position, Run, TextFlow};
pub use transaction::{Edit, Editor, Transaction};
pub use units::{Length, Rgba};
