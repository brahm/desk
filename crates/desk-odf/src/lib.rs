// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! OpenDocument (ODF) package reader and writer.
//!
//! This crate reads and writes ODF packages: the zip container, its
//! manifest and the XML streams inside it. It also holds the schema of the
//! `desk:` extensions, which keep what standard ODF cannot hold. It knows
//! nothing about the Desk document model; the mapping between ODF and the
//! model lives in `desk-filters`.
//!
//! So far it reads and writes text documents (ODT) as a [`TextDocument`]:
//! paragraphs and spans, paragraph and text styles, the page layout and
//! the document information.
//!
//! Files come from anywhere, so the readers treat them as hostile: they
//! use no unsafe code, cap the size of every stream and the depth of the
//! XML, and expand no entities.
//!
//! Like the other format libraries, this crate is licensed under the
//! MPL-2.0 so that other programs can reuse it. For the same reason it must
//! not depend on the other Desk crates, which are GPL-3.0-or-later. The CI
//! checks this rule.
//!
//! ```
//! use desk_odf::{Paragraph, Span, TextDocument, read_odt, write_odt};
//!
//! let mut document = TextDocument::default();
//! document.body.push(Paragraph {
//!     spans: vec![Span { text: "Olá".into(), ..Span::default() }],
//!     ..Paragraph::default()
//! });
//! let bytes = write_odt(&document)?;
//! assert_eq!(read_odt(&bytes)?.body[0].text(), "Olá");
//! # Ok::<(), desk_odf::OdfError>(())
//! ```

#![forbid(unsafe_code)]
// A file read is never trusted, so the library neither indexes without a
// check nor unwraps: either would close the app on a malformed file
// (D-028). Tests may, where a panic is the failure they report.
#![deny(clippy::indexing_slicing, clippy::unwrap_used)]
#![cfg_attr(test, allow(clippy::indexing_slicing, clippy::unwrap_used))]

mod budget;
mod chart;
mod draw;
mod error;
mod formula;
pub mod ns;
mod package;
mod read;
mod sheet;
mod table;
mod text;
mod units;
mod write;
mod xml;

pub use budget::{MAX_CHART_CELLS, MAX_CHART_VALUES, MAX_DOCUMENT_BYTES, MAX_DOCUMENT_CELLS};
pub use chart::{CHART_TYPE, ChartDocument, ChartSeries};
pub use draw::{
    DrawKind, DrawObject, FillImage, Gradient, GradientStop, GraphicProperties, Marker,
    parse_transform,
};
pub use error::OdfError;
pub use formula::{FORMULA_TYPE, MATHML, read_formula};
pub use package::{
    MAX_STREAM, MEDIA_TYPE, THUMBNAIL, read_odt, write_odt, write_odt_with_thumbnail,
};
pub use read::read_streams;
pub use sheet::{MAX_SHEET_ROWS, SPREADSHEET_TYPE, Sheet, read_ods, read_sheets};

/// The date written for a tracked change whose date is not known, as
/// ODF needs one: the epoch, which readers take as unknown.
pub const UNKNOWN_DATE: &str = "1970-01-01T00:00:00";
pub use table::{
    MAX_COLUMNS, MAX_ROWS, Table, TableCell, TableColumn, TableProperties, TableRow, TableTemplate,
};
pub use text::{
    Annotation, Background, Border, BorderLine, Break, ChangeKind, ChangedRegion, Column, Columns,
    DropCap, DropCapBackground, Family, Field, HeaderFooter, HeaderFooterStyle, LINE_BREAK,
    LeaderStyle, Line, LineColor, LineHeight, ListLabel, ListLevelStyle, ListMember, ListStyle,
    MasterPage, Meta, Note, NotesConfiguration, PageFill, PageLayout, PageTemplate, Paragraph,
    ParagraphProperties, Properties, SectionStart, SectionStyle, Span, Style, StyleShortcut,
    TabStop, TabType, TextAlign, TextDocument, TextPosition, TextProperties, TextSettings,
    TextShadow, TextTransform, TocLevel, ViewSettings,
};
pub use units::{Color, Length};
pub use write::{Streams, VERSION, write_streams};
