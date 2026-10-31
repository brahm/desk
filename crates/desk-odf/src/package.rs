// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The zip package of a text document.

use std::io::{Cursor, Read, Write};

use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::OdfError;
use crate::read::read_streams;
use crate::text::TextDocument;
use crate::write::{VERSION, write_streams};

/// The media type of a text document.
pub const MEDIA_TYPE: &str = "application/vnd.oasis.opendocument.text";

/// The largest XML stream read, uncompressed, so that a small hostile
/// file cannot take all the memory.
pub const MAX_STREAM: u64 = 64 << 20;

fn stream(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &'static str,
) -> Result<Option<String>, OdfError> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_STREAM + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STREAM {
        return Err(OdfError::TooLarge(name));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| OdfError::Malformed(format!("{name} is not UTF-8")))
}

/// Reads a text document (ODT) from the bytes of its package.
pub fn read_odt(bytes: &[u8]) -> Result<TextDocument, OdfError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    if let Some(media_type) = stream(&mut archive, "mimetype")? {
        let media_type = media_type.trim();
        // A template reads as a document.
        if media_type != MEDIA_TYPE
            && media_type != "application/vnd.oasis.opendocument.text-template"
        {
            return Err(OdfError::WrongType(media_type.to_owned()));
        }
    }
    let content = stream(&mut archive, "content.xml")?.ok_or(OdfError::Missing("content.xml"))?;
    let styles = stream(&mut archive, "styles.xml")?;
    let meta = stream(&mut archive, "meta.xml")?;
    read_streams(&content, styles.as_deref(), meta.as_deref())
}

/// Writes a text document as the bytes of an ODT package.
pub fn write_odt(document: &TextDocument) -> Result<Vec<u8>, OdfError> {
    let streams = write_streams(document);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    // The media type comes first and uncompressed, so that it can be read
    // at a fixed offset.
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("mimetype", stored)?;
    zip.write_all(MEDIA_TYPE.as_bytes())?;
    for (name, text) in [
        ("content.xml", &streams.content),
        ("styles.xml", &streams.styles),
        ("meta.xml", &streams.meta),
        ("META-INF/manifest.xml", &manifest()),
    ] {
        zip.start_file(name, deflated)?;
        zip.write_all(text.as_bytes())?;
    }
    Ok(zip.finish()?.into_inner())
}

fn manifest() -> String {
    let mut w = crate::xml::Writer::new();
    w.start("manifest:manifest")
        .attr("xmlns:manifest", crate::ns::MANIFEST)
        .attr("manifest:version", VERSION);
    w.start("manifest:file-entry")
        .attr("manifest:full-path", "/")
        .attr("manifest:version", VERSION)
        .attr("manifest:media-type", MEDIA_TYPE)
        .end();
    for path in ["content.xml", "styles.xml", "meta.xml"] {
        w.start("manifest:file-entry")
            .attr("manifest:full-path", path)
            .attr("manifest:media-type", "text/xml")
            .end();
    }
    w.end();
    w.finish()
}
