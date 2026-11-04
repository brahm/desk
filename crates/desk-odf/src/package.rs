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
    let settings = stream(&mut archive, "settings.xml")?;
    let mut document = read_streams(&content, styles.as_deref(), meta.as_deref())?;
    document.tabs_relative_to_indent = settings
        .as_deref()
        .and_then(crate::read::tabs_relative_to_indent);
    document.style_shortcuts = settings
        .as_deref()
        .map(crate::read::style_shortcuts)
        .unwrap_or_default();
    document.text_settings = settings
        .as_deref()
        .map(crate::read::text_settings)
        .unwrap_or_default();
    document.view = settings
        .as_deref()
        .map(crate::read::view_settings)
        .unwrap_or_default();
    // The pictures that the lists link to inside the package.
    let mut hrefs: Vec<String> = document
        .list_styles
        .iter()
        .chain(&document.automatic_list_styles)
        .flat_map(|s| &s.levels)
        .filter_map(|l| match &l.label {
            crate::text::ListLabel::Image { href, .. } => Some(href.clone()),
            _ => None,
        })
        .chain(
            document
                .page_objects
                .iter()
                .chain(document.body.iter().flat_map(|p| p.objects()))
                .flat_map(|o| o.hrefs())
                .map(str::to_owned),
        )
        .chain(document.fill_images.iter().map(|i| i.href.clone()))
        .filter(|href| !href.contains("://") && !href.starts_with('/'))
        .collect();
    hrefs.sort();
    hrefs.dedup();
    for href in hrefs {
        let path = href.strip_prefix("./").unwrap_or(&href);
        let Ok(file) = archive.by_name(path) else {
            continue;
        };
        let mut bytes = Vec::new();
        file.take(MAX_STREAM + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 <= MAX_STREAM {
            document.pictures.push((href, bytes));
        }
    }
    Ok(document)
}

/// Writes a text document as the bytes of an ODT package.
pub fn write_odt(document: &TextDocument) -> Result<Vec<u8>, OdfError> {
    write_odt_with_thumbnail(document, None)
}

/// The path of the preview of a package, which file managers show.
pub const THUMBNAIL: &str = "Thumbnails/thumbnail.png";

/// Writes a text document (ODT) with a preview of its first page, a PNG
/// image at most 256 pixels wide or high, as ODF recommends.
pub fn write_odt_with_thumbnail(
    document: &TextDocument,
    thumbnail: Option<&[u8]>,
) -> Result<Vec<u8>, OdfError> {
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
        (
            "META-INF/manifest.xml",
            &manifest(
                thumbnail.is_some(),
                streams.settings.is_some(),
                &document.pictures,
            ),
        ),
    ] {
        zip.start_file(name, deflated)?;
        zip.write_all(text.as_bytes())?;
    }
    if let Some(settings) = &streams.settings {
        zip.start_file("settings.xml", deflated)?;
        zip.write_all(settings.as_bytes())?;
    }
    for (path, bytes) in &document.pictures {
        // Pictures are compressed already.
        zip.start_file(path.as_str(), stored)?;
        zip.write_all(bytes)?;
    }
    if let Some(png) = thumbnail {
        // PNG is compressed already.
        zip.start_file(THUMBNAIL, stored)?;
        zip.write_all(png)?;
    }
    Ok(zip.finish()?.into_inner())
}

/// The media type of a picture, by the extension of its path.
fn picture_type(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("tif" | "tiff") => "image/tiff",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn manifest(thumbnail: bool, settings: bool, pictures: &[(String, Vec<u8>)]) -> String {
    let mut w = crate::xml::Writer::new();
    w.start("manifest:manifest")
        .attr("xmlns:manifest", crate::ns::MANIFEST)
        .attr("manifest:version", VERSION);
    w.start("manifest:file-entry")
        .attr("manifest:full-path", "/")
        .attr("manifest:version", VERSION)
        .attr("manifest:media-type", MEDIA_TYPE)
        .end();
    let streams = ["content.xml", "styles.xml", "meta.xml", "settings.xml"];
    for path in &streams[..if settings { 4 } else { 3 }] {
        w.start("manifest:file-entry")
            .attr("manifest:full-path", path)
            .attr("manifest:media-type", "text/xml")
            .end();
    }
    if thumbnail {
        w.start("manifest:file-entry")
            .attr("manifest:full-path", THUMBNAIL)
            .attr("manifest:media-type", "image/png")
            .end();
    }
    for (path, _) in pictures {
        w.start("manifest:file-entry")
            .attr("manifest:full-path", path)
            .attr("manifest:media-type", picture_type(path))
            .end();
    }
    w.end();
    w.finish()
}
