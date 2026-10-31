// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The package of a document: a folder, or a flat file zipped from one,
//! with the document in `index.xml`, usually compressed with gzip.

use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use flate2::read::GzDecoder;
use zip::ZipArchive;

use crate::error::IworkError;

/// The name of the index.
pub const INDEX: &str = "index.xml";

/// The name of the compressed index.
pub const INDEX_GZ: &str = "index.xml.gz";

/// The largest index read, uncompressed, so that a small malicious file
/// cannot take all the memory.
pub const MAX_INDEX: u64 = 256 << 20;

const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];

/// Reads at most [`MAX_INDEX`] bytes.
fn limited(reader: impl Read) -> Result<Vec<u8>, IworkError> {
    let mut bytes = Vec::new();
    reader.take(MAX_INDEX + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INDEX {
        return Err(IworkError::TooLarge(INDEX));
    }
    Ok(bytes)
}

/// The text of an index, from its bytes, compressed or not.
fn index_text(bytes: &[u8]) -> Result<String, IworkError> {
    let bytes = if bytes.starts_with(GZIP_MAGIC) {
        limited(GzDecoder::new(bytes))?
    } else {
        bytes.to_vec()
    };
    String::from_utf8(bytes).map_err(|_| IworkError::Malformed(format!("{INDEX} is not UTF-8")))
}

/// The text of the index of a document, from the bytes of a flat file (a
/// zip of the package), of an `index.xml.gz` or of an `index.xml`.
pub fn read_index(bytes: &[u8]) -> Result<String, IworkError> {
    if !bytes.starts_with(ZIP_MAGIC) {
        return index_text(bytes);
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    // The index at the root, or in the one folder that the package was
    // zipped from.
    let names: Vec<String> = archive
        .file_names()
        .filter_map(|name| name.ok().map(|name| name.into_owned()))
        .collect();
    let name = names
        .iter()
        .filter(|name| {
            let file = name.rsplit('/').next().unwrap_or(name);
            (file == INDEX || file == INDEX_GZ) && name.matches('/').count() <= 1
        })
        .min_by_key(|name| (name.matches('/').count(), name.ends_with(".gz")))
        .cloned()
        .ok_or(IworkError::Missing(INDEX))?;
    let file = archive.by_name(&name)?;
    index_text(&limited(file)?)
}

/// The text of the index of a document on disk: a package folder or a
/// flat file.
pub fn read_index_at(path: &Path) -> Result<String, IworkError> {
    if !path.is_dir() {
        return read_index(&limited(fs::File::open(path)?)?);
    }
    for name in [INDEX_GZ, INDEX] {
        let file = path.join(name);
        if file.is_file() {
            return index_text(&limited(fs::File::open(file)?)?);
        }
    }
    Err(IworkError::Missing(INDEX))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use zip::write::SimpleFileOptions;

    use super::*;

    const XML: &str = "<?xml version=\"1.0\"?><sl:document/>";

    fn gzip(text: &str) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(text.as_bytes()).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn reads_plain_and_compressed_indexes() {
        assert_eq!(read_index(XML.as_bytes()).unwrap(), XML);
        assert_eq!(read_index(&gzip(XML)).unwrap(), XML);
    }

    #[test]
    fn reads_the_index_of_a_flat_file() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        zip.start_file("QuickLook/Thumbnail.jpg", options).unwrap();
        zip.start_file("index.xml.gz", options).unwrap();
        zip.write_all(&gzip(XML)).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert_eq!(read_index(&bytes).unwrap(), XML);
    }

    #[test]
    fn refuses_a_zip_without_an_index() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("other.xml", SimpleFileOptions::default())
            .unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(matches!(read_index(&bytes), Err(IworkError::Missing(_))));
    }
}
