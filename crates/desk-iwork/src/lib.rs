// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Reader for Apple iWork '09 documents.
//!
//! This crate reads the XML inside Pages, Numbers and Keynote documents
//! saved by iWork '09 into a typed tree that follows the files
//! closely. The mapping from that tree to the Desk document model lives in
//! `desk-filters`.
//!
//! So far it reads the body text of Pages documents, with its paragraph
//! and character styles, the paper and margins and the document
//! information (DOC-22). The [`sf`] vocabulary is shared by the three
//! apps, so Numbers and Keynote will reuse it.
//!
//! Like the other format libraries, this crate is licensed under the
//! MPL-2.0 so that other programs can reuse it. For the same reason it must
//! not depend on the other Desk crates, which are GPL-3.0-or-later. The CI
//! checks this rule.

#![forbid(unsafe_code)]

mod error;
pub mod ns;
mod package;
pub mod pages;
pub mod sf;
mod xml;

use std::path::Path;

pub use error::IworkError;
pub use package::{INDEX, INDEX_GZ, MAX_INDEX, read_index, read_index_at};
pub use pages::{Outliner, PagesDocument, parse_pages};

/// Reads a Pages document from the bytes of a flat file or of its index,
/// with the files of the package that it uses.
pub fn read_pages(bytes: &[u8]) -> Result<PagesDocument, IworkError> {
    let mut document = parse_pages(&read_index(bytes)?)?;
    for name in document.file_paths() {
        if let Some(file) = package::read_file(bytes, &name) {
            document.files.push((name, file));
        }
    }
    Ok(document)
}

/// Reads a Pages document on disk, a package folder or a flat file, with
/// the files of the package that it uses.
pub fn read_pages_at(path: &Path) -> Result<PagesDocument, IworkError> {
    if !path.is_dir() {
        return read_pages(&package::limited(std::fs::File::open(path)?)?);
    }
    let mut document = parse_pages(&read_index_at(path)?)?;
    for name in document.file_paths() {
        if let Some(file) = package::read_file_at(path, &name) {
            document.files.push((name, file));
        }
    }
    Ok(document)
}
