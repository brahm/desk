// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pictures: the image bullets of lists (LST-03) and the images of
//! documents (OBJ-01), kept as PNG or, as they came, JPEG.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The format of the bytes of a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PictureFormat {
    /// PNG.
    Png,
    /// JPEG, kept as it came so that photographs stay small.
    Jpeg,
    /// PDF, its first page, drawn as it is so that it stays sharp at any
    /// size; its width and height are in points.
    Pdf,
}

impl PictureFormat {
    /// The media type, as `image/png`.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Pdf => "application/pdf",
        }
    }

    /// The extension of a file of the format, as `png`.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Pdf => "pdf",
        }
    }
}

/// A picture in PNG or JPEG, with its size in pixels. Copies share the
/// data.
#[derive(Clone)]
pub struct Picture {
    png: Arc<[u8]>,
    format: PictureFormat,
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
        Some(Self {
            png,
            format: PictureFormat::Png,
            width,
            height,
        })
    }

    /// A picture from the bytes of a JPEG file, or `None` when they are
    /// not one or it has no pixels. Its size comes from the first frame
    /// header; the pixels are only decoded to draw it.
    pub fn from_jpeg(jpeg: impl Into<Arc<[u8]>>) -> Option<Self> {
        let jpeg = jpeg.into();
        if !jpeg.starts_with(&[0xFF, 0xD8]) {
            return None;
        }
        let mut at = 2;
        while at + 4 <= jpeg.len() {
            if jpeg[at] != 0xFF {
                return None;
            }
            let marker = jpeg[at + 1];
            // Fill bytes, and markers without a length.
            if marker == 0xFF {
                at += 1;
                continue;
            }
            if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
                at += 2;
                continue;
            }
            let length = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
            // A start of frame, but for the DHT, JPG and DAC markers.
            if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                let header = jpeg.get(at + 4..at + 9)?;
                let height = u32::from(u16::from_be_bytes([header[1], header[2]]));
                let width = u32::from(u16::from_be_bytes([header[3], header[4]]));
                if width == 0 || height == 0 {
                    return None;
                }
                return Some(Self {
                    png: jpeg,
                    format: PictureFormat::Jpeg,
                    width,
                    height,
                });
            }
            if marker == 0xDA || length < 2 {
                return None;
            }
            at += 2 + length;
        }
        None
    }

    /// A picture from the bytes of a PDF file, whose first page is `width`
    /// by `height` points, as a PDF library measured it; `None` when they
    /// do not start as PDF or the page is empty.
    pub fn from_pdf(pdf: impl Into<Arc<[u8]>>, width: f64, height: f64) -> Option<Self> {
        let pdf = pdf.into();
        if !pdf.starts_with(b"%PDF-") || !(width >= 1.0 && height >= 1.0) {
            return None;
        }
        let side = |v: f64| v.round().min(f64::from(u32::MAX)) as u32;
        Some(Self {
            png: pdf,
            format: PictureFormat::Pdf,
            width: side(width),
            height: side(height),
        })
    }

    /// A picture from the bytes of a PNG or JPEG file.
    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Option<Self> {
        let bytes = bytes.into();
        if bytes.starts_with(PNG_SIGNATURE) {
            Self::from_png(bytes)
        } else {
            Self::from_jpeg(bytes)
        }
    }

    /// The bytes of the file, in its format.
    pub fn data(&self) -> &[u8] {
        &self.png
    }

    /// The bytes of the file, shared rather than copied.
    pub fn shared_data(&self) -> Arc<[u8]> {
        Arc::clone(&self.png)
    }

    /// The format of the bytes.
    pub fn format(&self) -> PictureFormat {
        self.format
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
            "Picture({:?} {}x{}, {} bytes)",
            self.format,
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

    #[test]
    fn obj_01_the_size_of_a_jpeg_comes_from_its_frame_header() {
        // SOI, an APP0 of four bytes, then SOF0 of 17 by 9 pixels.
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0, 0];
        jpeg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 8, 0, 9, 0, 17, 3]);
        let picture = Picture::from_bytes(jpeg).unwrap();
        assert_eq!(picture.format(), PictureFormat::Jpeg);
        assert_eq!((picture.width(), picture.height()), (17, 9));
        assert!(Picture::from_jpeg(vec![0xFF, 0xD8, 0xFF, 0xDA, 0, 2]).is_none());
    }
}
