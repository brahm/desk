// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! USDZ: a zip package whose first file is the layer of the scene, with
//! the layers it references and the pictures of its textures after it,
//! stored without compression so that a reader could map them. Compressed
//! entries are read too.

use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::usd::{Files, build};
use crate::{Error, Result, Scene, limits, malformed};

/// The name of the first file of a zip, from its local header.
fn first_name(bytes: &[u8]) -> Option<&[u8]> {
    if !bytes.starts_with(b"PK\x03\x04") {
        return None;
    }
    let length = usize::from(u16::from_le_bytes(bytes.get(26..28)?.try_into().ok()?));
    bytes.get(30..30 + length)
}

/// Whether the name of a file is that of a layer of USD.
fn is_layer(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".usdc", ".usda", ".usd"]
        .iter()
        .any(|e| lower.ends_with(e))
}

/// Whether the bytes are a USDZ package: a zip whose first file is a
/// layer.
pub(crate) fn is_usdz(bytes: &[u8]) -> bool {
    first_name(bytes).is_some_and(|n| is_layer(&String::from_utf8_lossy(n)))
}

/// The files of the package.
struct Package<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
}

impl Files for Package<'_> {
    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        let file = self.archive.by_name(path).ok()?;
        if file.size() > limits::FILE_BYTES {
            return None;
        }
        let mut out = Vec::new();
        file.take(limits::FILE_BYTES).read_to_end(&mut out).ok()?;
        Some(out)
    }
}

/// Reads the scene of a USDZ package.
pub(crate) fn read(bytes: &[u8]) -> Result<Scene> {
    let archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| malformed(format!("the package: {e}")))?;
    let root = first_name(bytes)
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .filter(|n| is_layer(n))
        .ok_or(Error::Unknown)?;
    let mut package = Package { archive };
    build(&root, &mut package)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_packages_are_known_by_their_first_file() {
        let mut zip = b"PK\x03\x04".to_vec();
        zip.extend([0; 22]);
        zip.extend(10u16.to_le_bytes());
        zip.extend(0u16.to_le_bytes());
        zip.extend(b"scene.usdc");
        assert!(is_usdz(&zip));
        let mut other = zip.clone();
        other[30..40].copy_from_slice(b"scene.xmlx");
        assert!(!is_usdz(&other));
        assert!(!is_usdz(b"PK\x03\x04"));
        assert!(!is_usdz(b"#usda 1.0"));
    }
}
