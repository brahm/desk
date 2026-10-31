// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Errors of the reader.

use std::fmt;

/// Why a document could not be read.
#[derive(Debug)]
#[non_exhaustive]
pub enum IworkError {
    /// The zip container of a flat file is damaged.
    Zip(zip::result::ZipError),
    /// The index is not well-formed XML.
    Xml(quick_xml::Error),
    /// The document could not be read, or its index not decompressed.
    Io(std::io::Error),
    /// The document is not of the kind expected, such as a Keynote
    /// presentation given as a Pages document. Holds the root element.
    WrongType(String),
    /// The package has no index.
    Missing(&'static str),
    /// The index is larger than the reader accepts.
    TooLarge(&'static str),
    /// The XML is well formed but does not make sense as an iWork
    /// document.
    Malformed(String),
}

impl fmt::Display for IworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zip(error) => write!(f, "the zip container is damaged: {error}"),
            Self::Xml(error) => write!(f, "the XML is not well formed: {error}"),
            Self::Io(error) => write!(f, "{error}"),
            Self::WrongType(found) => write!(f, "not a Pages document: {found}"),
            Self::Missing(name) => write!(f, "the package has no {name}"),
            Self::TooLarge(name) => write!(f, "{name} is too large"),
            Self::Malformed(why) => write!(f, "the document is malformed: {why}"),
        }
    }
}

impl std::error::Error for IworkError {}

impl From<zip::result::ZipError> for IworkError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Zip(error)
    }
}

impl From<quick_xml::Error> for IworkError {
    fn from(error: quick_xml::Error) -> Self {
        Self::Xml(error)
    }
}

impl From<quick_xml::encoding::EncodingError> for IworkError {
    fn from(error: quick_xml::encoding::EncodingError) -> Self {
        Self::Xml(error.into())
    }
}

impl From<quick_xml::escape::EscapeError> for IworkError {
    fn from(error: quick_xml::escape::EscapeError) -> Self {
        Self::Xml(error.into())
    }
}

impl From<std::io::Error> for IworkError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
