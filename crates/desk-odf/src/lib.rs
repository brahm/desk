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
//!     spans: vec![Span { style: None, text: "Olá".into() }],
//!     ..Paragraph::default()
//! });
//! let bytes = write_odt(&document)?;
//! assert_eq!(read_odt(&bytes)?.body[0].text(), "Olá");
//! # Ok::<(), desk_odf::OdfError>(())
//! ```

#![forbid(unsafe_code)]

mod error;
pub mod ns;
mod package;
mod read;
mod text;
mod units;
mod write;
mod xml;

pub use error::OdfError;
pub use package::{
    MAX_STREAM, MEDIA_TYPE, THUMBNAIL, read_odt, write_odt, write_odt_with_thumbnail,
};
pub use read::read_streams;
pub use text::{
    Break, Family, LINE_BREAK, LeaderStyle, Line, LineColor, LineHeight, Meta, PageLayout,
    Paragraph, ParagraphProperties, Properties, Span, Style, TabStop, TabType, TextAlign,
    TextDocument, TextPosition, TextProperties, TextTransform,
};
pub use units::{Color, Length};
pub use write::{Streams, VERSION, write_streams};
