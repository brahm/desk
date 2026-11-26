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
mod aside;
mod author;
mod auto_numbers;
mod autocorrect;
mod case_change;
mod change;
mod chart_style;
mod comment;
mod connection;
mod convert_quotes;
mod custom_format;
mod document;
mod document_language;
mod drop_cap_style;
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
mod object_style;
mod op;
mod outline;
mod picture;
mod pixel_outline;
mod presentation;
mod section;
mod shape_path;
mod story;
mod style;
mod styling;
mod table_style;
mod text;
mod toc;
mod transaction;
mod units;
pub mod workbook;

pub use arrange::{CapturedPages, SectionsClip};
pub use author::{AuthorColor, AuthorPalette};
pub use auto_numbers::{AutoPageNumbers, NumberAlignment};
pub use autocorrect::{
    AutoCorrect, Correction, QuoteStyle, Quotes, Replacement, ends_word, is_quote, smart_dash,
    smart_quote,
};
pub use case_change::CaseChange;
pub use change::{Change, ChangeKind, ChangeSpan, Changes, Formatting, ParaFormatting};
pub use chart_style::ChartStyle;
pub use comment::{CellChange, CellComment, Comment, CommentIds, CommentPlace, Comments, Reply};
pub use connection::{Connection, bend_through, bent_controls, bent_middle, connect, flattened};
pub use document::{
    Container, Document, FixedGuide, Markup, Metadata, OutlineOptions, PageSetup, PagedFlow,
    ParagraphSpacing, Statistics, TextSettings, Tracking, ViewSettings, ViewZoom,
};
pub use drop_cap_style::DropCapStyle;
pub use error::ModelError;
pub use find::{Found, Query};
pub use format::{
    Alignment, Baseline, BorderSides, BorderStyle, Capitalization, CharAttrs, CharFormat,
    DEFAULT_TAB_INTERVAL, DropCap, DropCapShape, FontFeature, FontFeatures, FontWeight, Language,
    Ligatures, LineColor, LineSpacing, LineStyle, ParaAttrs, ParaBorder, ParaFormat, Shadow,
    TabAlignment, TabLeader, TabStop, TabStops,
};
pub use header::{
    DateWriter, FIELD_CHARACTER, Field, HeadersFooters, MarginField, MarginKind, MarginPages,
    MarginPlace, file_shown,
};
pub use id::NodeId;
pub use list::{
    LIST_LEVELS, LabelFollow, ListItem, ListLabel, ListLevel, ListStyle, Marker, MarkerKind,
    NumberFormat, Numbering, detect_marker,
};
pub use media::{Media, MediaFormat, Repeat};
pub use note::{Note, NoteFormat, NoteKind, NoteNumbering, NoteSettings, Notes};
pub use object::{
    Adjustments, AlphaOutline, AlphaSeed, Anchor, ContourParts, Fill, Frame, FrameStyle, Gradient,
    GradientStop, GraphicStyle, ImageFill, LineEnd, MAX_SHAPE_SIDES, MAX_TEXT_BOX_COLUMNS, Moved,
    Object, ObjectKind, ObjectShadow, Objects, POLYGON_SCALE, PathPoint, PictureFrame,
    PictureSettings, Shape, Stroke, TextBox, VerticalAlignment, Wrap,
};
pub use object_edit::{
    Distribution, Layer, LayoutPagesClip, ObjectAlignment, TextStyling, new_origin, reframed,
    translated, union,
};
pub use object_style::{
    ObjectStyle, ObjectStyleKind, ObjectStyles, TextBoxDefault, apply_object_style,
};
pub use op::Op;
pub use outline::{MAX_OUTLINE_LEVEL, heading_level};
pub use picture::{Picture, PictureFormat};
pub use pixel_outline::{pixel_outline, pixel_outlines};
pub use presentation::{SLIDE_SIZE, Slide, Topics};
pub use section::{Columns, Layout, MAX_COLUMNS, PARTS, Section, SectionStart, Start};
pub use shape_path::PathSegment;
pub use story::Story;
pub use style::{
    CharacterStyle, ParagraphStyle, Style, StyleFamily, StyleId, Styles, screen_font_size,
};
pub use styling::{CopiedStyle, Fragment, FragmentParagraph, Overrides, StyleParts};
pub use table_style::{TableLook, TableStyleEntry, TableStyles};
pub use text::{LINE_BREAK, NO_BREAK_SPACE, Paragraph, Position, Run, TextFlow};
pub use toc::{TocEntry, TocLevel, TocSettings, tables_of_contents};
pub use transaction::{Edit, Editor, MAX_UNDO, Signer, Tracker, Transaction, now};
pub use units::{Length, Rgba};

/// A table of a document: the table model shared by the apps, with the
/// text of each cell as plain text that takes the formatting of its cell
/// (TAB-01, IOP-01).
pub type Table = desk_table::Table<String>;

pub use desk_chart::{self as chart, Chart};
pub use desk_table as table;
