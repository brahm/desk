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
//!   [`PagedFlow`]: a [`PageSetup`], a body [`TextFlow`] and the
//!   [`Objects`] on the pages.
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
//! use desk_model::{CharAttrs, Document, Editor, FontWeight};
//!
//! let mut editor = Editor::new(Document::new_word_processing());
//! let start = editor.document().start();
//! let end = editor.edit("Typing", |edit| edit.insert_text(start, "Hello"))?;
//! editor.edit("Bold", |edit| {
//!     edit.set_character_attrs(start, end, &CharAttrs { weight: Some(FontWeight::BOLD), ..Default::default() })
//! })?;
//! assert_eq!(editor.undo_name(), Some("Bold"));
//!
//! editor.undo()?;
//! editor.undo()?;
//! assert_eq!(editor.document(), &Document::new_word_processing());
//! # Ok::<(), desk_model::ModelError>(())
//! ```

mod arrange;
mod auto_numbers;
mod autocorrect;
mod connection;
mod document;
mod error;
mod find;
mod format;
mod header;
mod id;
mod inline;
pub use inline::marks_in;
mod list;
mod media;
mod media_trim;
mod note;
mod object;
mod object_edit;
mod op;
mod outline;
mod picture;
mod section;
mod shape_path;
mod story;
mod style;
mod styling;
mod text;
mod toc;
mod transaction;
mod units;

pub use arrange::SectionsClip;
pub use auto_numbers::{AutoPageNumbers, NumberAlignment};
pub use autocorrect::{AutoCorrect, Correction, Replacement, ends_word, smart_quote};
pub use connection::{Connection, bend_through, bent_controls, bent_middle, connect, flattened};
pub use document::{
    Container, Document, FixedGuide, Metadata, PageSetup, PagedFlow, ParagraphSpacing, Statistics,
    TextSettings, ViewSettings, ViewZoom,
};
pub use error::ModelError;
pub use find::{Found, Query};
pub use format::{
    Alignment, Baseline, BorderSides, BorderStyle, Capitalization, CharAttrs, CharFormat,
    DEFAULT_TAB_INTERVAL, FontFeature, FontFeatures, FontWeight, Language, Ligatures, LineColor,
    LineSpacing, LineStyle, ParaAttrs, ParaBorder, ParaFormat, Shadow, TabAlignment, TabLeader,
    TabStop, TabStops,
};
pub use header::{
    DateWriter, FIELD_CHARACTER, Field, HeadersFooters, MarginKind, MarginPages, MarginPlace,
    file_shown,
};
pub use id::NodeId;
pub use list::{
    LIST_LEVELS, ListItem, ListLabel, ListLevel, ListStyle, Marker, MarkerKind, NumberFormat,
    Numbering, detect_marker,
};
pub use media::{Media, MediaFormat, Repeat};
pub use note::{Note, NoteFormat, NoteKind, NoteNumbering, NoteSettings, Notes};
pub use object::{
    Adjustments, AlphaSeed, Anchor, Fill, Frame, FrameStyle, Gradient, GradientStop, GraphicStyle,
    ImageFill, LineEnd, MAX_SHAPE_SIDES, MAX_TEXT_BOX_COLUMNS, Object, ObjectKind, ObjectShadow,
    Objects, POLYGON_SCALE, PathPoint, PictureFrame, PictureSettings, Shape, Stroke, TextBox,
    VerticalAlignment, Wrap,
};
pub use object_edit::{
    Distribution, Layer, LayoutPagesClip, ObjectAlignment, TextStyling, reframed, translated, union,
};
pub use op::Op;
pub use outline::{MAX_OUTLINE_LEVEL, heading_level};
pub use picture::{Picture, PictureFormat};
pub use section::{Columns, Layout, MAX_COLUMNS, PARTS, Section, SectionStart, Start};
pub use shape_path::PathSegment;
pub use story::Story;
pub use style::{CharacterStyle, ParagraphStyle, Style, StyleFamily, StyleId, Styles};
pub use styling::{CopiedStyle, Fragment, FragmentParagraph, Overrides};
pub use text::{LINE_BREAK, NO_BREAK_SPACE, Paragraph, Position, Run, TextFlow};
pub use toc::{TocEntry, TocLevel, TocSettings, tables_of_contents};
pub use transaction::{Edit, Editor, Transaction};
pub use units::{Length, Rgba};
