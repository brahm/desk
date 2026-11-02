// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pictures: the image bullets of lists (LST-03), kept as PNG.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// A picture in PNG, with its size in pixels. Copies share the data.
#[derive(Clone)]
pub struct Picture {
    png: Arc<[u8]>,
    width: u32,
    height: u32,
}

impl Picture {
    /// A picture from the bytes of a PNG file, or `None` when they are not
    /// one or it has no pixels.
    pub fn from_png(png: impl Into<Arc<[u8]>>) -> Option<Self> {
        let png = png.into();
        // The signature, then the IHDR chunk with the width and height.
        if !png.starts_with(PNG_SIGNATURE) || png.get(12..16)? != b"IHDR" {
            return None;
        }
        let number = |at: usize| -> Option<u32> {
            Some(u32::from_be_bytes(png.get(at..at + 4)?.try_into().ok()?))
        };
        let (width, height) = (number(16)?, number(20)?);
        if width == 0 || height == 0 {
            return None;
        }
        Some(Self { png, width, height })
    }

    /// The bytes of the PNG file.
    pub fn png(&self) -> &[u8] {
        &self.png
    }

    /// The width, in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The height, in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The width over the height.
    pub fn aspect(&self) -> f64 {
        f64::from(self.width) / f64::from(self.height)
    }

    /// Whether two pictures share their data, which tells copies apart
    /// from equal pictures cheaply.
    pub fn same_data(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.png, &other.png)
    }
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Picture({}x{}, {} bytes)",
            self.width,
            self.height,
            self.png.len()
        )
    }
}

impl PartialEq for Picture {
    fn eq(&self, other: &Self) -> bool {
        self.same_data(other) || self.png == other.png
    }
}

impl Eq for Picture {}

impl Hash for Picture {
    /// Hashes the size and the ends of the data, so that a large picture
    /// hashes fast; equal pictures still hash the same.
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.width, self.height, self.png.len()).hash(state);
        let ends = 64.min(self.png.len());
        self.png[..ends].hash(state);
        self.png[self.png.len() - ends..].hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn pictures_know_their_size() {
        let picture = Picture::from_png(png(32, 16)).unwrap();
        assert_eq!((picture.width(), picture.height()), (32, 16));
        assert_eq!(picture.aspect(), 2.0);
        assert_eq!(picture, Picture::from_png(png(32, 16)).unwrap());
        assert!(Picture::from_png(png(0, 16)).is_none());
        assert!(Picture::from_png(b"GIF89a".to_vec()).is_none());
    }
}
