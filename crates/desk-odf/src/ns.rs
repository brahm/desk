// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! XML namespaces used in ODF documents.

/// Namespace URI of the `desk:` extensions to ODF.
///
/// Every document that Desk saves with extensions carries this URI, so it
/// must not change. An incompatible revision of the extensions gets a new
/// URI instead.
pub const DESK: &str = "https://brahm.com.br/ns/desk/1.0";

/// `office:`.
pub const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
/// `style:`.
pub const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
/// `text:`.
pub const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
/// `fo:`.
pub const FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
/// `svg:`.
pub const SVG: &str = "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0";
/// `meta:`.
pub const META: &str = "urn:oasis:names:tc:opendocument:xmlns:meta:1.0";
/// `dc:`.
pub const DC: &str = "http://purl.org/dc/elements/1.1/";
/// `manifest:`.
pub const MANIFEST: &str = "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0";
/// `draw:`.
pub const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
