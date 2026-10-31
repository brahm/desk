// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Errors of the readers and writers.

use std::fmt;

/// Why a package could not be read or written.
#[derive(Debug)]
#[non_exhaustive]
pub enum OdfError {
    /// The zip container is damaged or not a zip file.
    Zip(zip::result::ZipError),
    /// A stream is not well-formed XML.
    Xml(quick_xml::Error),
    /// A stream could not be read or written.
    Io(std::io::Error),
    /// The package is not of the kind expected, such as a spreadsheet
    /// given as a text document. Holds the media type found.
    WrongType(String),
    /// A required stream is missing from the package.
    Missing(&'static str),
    /// A stream is larger than the readers accept.
    TooLarge(&'static str),
    /// The XML is well formed but does not make sense as ODF.
    Malformed(String),
}

impl fmt::Display for OdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zip(error) => write!(f, "the zip container is damaged: {error}"),
            Self::Xml(error) => write!(f, "the XML is not well formed: {error}"),
            Self::Io(error) => write!(f, "{error}"),
            Self::WrongType(found) => write!(f, "not an OpenDocument text: {found}"),
            Self::Missing(name) => write!(f, "the package has no {name}"),
            Self::TooLarge(name) => write!(f, "{name} is too large"),
            Self::Malformed(why) => write!(f, "the document is malformed: {why}"),
        }
    }
}

impl std::error::Error for OdfError {}

impl From<zip::result::ZipError> for OdfError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Zip(error)
    }
}

impl From<quick_xml::Error> for OdfError {
    fn from(error: quick_xml::Error) -> Self {
        Self::Xml(error)
    }
}

impl From<quick_xml::encoding::EncodingError> for OdfError {
    fn from(error: quick_xml::encoding::EncodingError) -> Self {
        Self::Xml(error.into())
    }
}

impl From<quick_xml::escape::EscapeError> for OdfError {
    fn from(error: quick_xml::escape::EscapeError) -> Self {
        Self::Xml(error.into())
    }
}

impl From<std::io::Error> for OdfError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
