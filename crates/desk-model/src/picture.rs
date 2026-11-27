// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg_attr(not(test), deny(clippy::indexing_slicing, clippy::unwrap_used))]

//! Pictures: the image bullets of lists (LST-03) and the images of
//! documents (OBJ-01), kept as PNG or, as they came, JPEG, PDF, SVG and
//! HEIC (W108).

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
    /// SVG, drawn as it is so that it stays sharp at any size; its width
    /// and height are in pixels of a CSS inch of 96, as it declares them
    /// (W108).
    Svg,
    /// HEIC, the photographs of iPhones, kept as it came and decoded only
    /// to draw it (W108).
    Heic,
}

impl PictureFormat {
    /// The media type, as `image/png`.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Pdf => "application/pdf",
            Self::Svg => "image/svg+xml",
            Self::Heic => "image/heic",
        }
    }

    /// The extension of a file of the format, as `png`.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Pdf => "pdf",
            Self::Svg => "svg",
            Self::Heic => "heic",
        }
    }

    /// Whether it is drawn as lines and text rather than pixels: PDF and
    /// SVG.
    pub fn is_vector(self) -> bool {
        matches!(self, Self::Pdf | Self::Svg)
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
        while let Some(&[ff, marker, high, low]) = jpeg.get(at..at + 4) {
            if ff != 0xFF {
                return None;
            }
            // Fill bytes, and markers without a length.
            if marker == 0xFF {
                at += 1;
                continue;
            }
            if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
                at += 2;
                continue;
            }
            let length = usize::from(u16::from_be_bytes([high, low]));
            // A start of frame, but for the DHT, JPG and DAC markers.
            if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                let &[_, h1, h2, w1, w2] = jpeg.get(at + 4..at + 9)? else {
                    return None;
                };
                let height = u32::from(u16::from_be_bytes([h1, h2]));
                let width = u32::from(u16::from_be_bytes([w1, w2]));
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

    /// A picture from the bytes of an SVG file, `width` by `height` as an
    /// SVG library measured it; `None` when they do not look like SVG or
    /// the picture is empty (W108).
    pub fn from_svg(svg: impl Into<Arc<[u8]>>, width: f64, height: f64) -> Option<Self> {
        let svg = svg.into();
        if !is_svg(&svg) || !(width >= 1.0 && height >= 1.0) {
            return None;
        }
        let side = |v: f64| v.round().min(f64::from(u32::MAX)) as u32;
        Some(Self {
            png: svg,
            format: PictureFormat::Svg,
            width: side(width),
            height: side(height),
        })
    }

    /// A picture from the bytes of a HEIC file of `width` by `height`
    /// pixels, as a HEIF library read them; `None` when they do not start
    /// as HEIF or it has no pixels (W108).
    pub fn from_heic(heic: impl Into<Arc<[u8]>>, width: u32, height: u32) -> Option<Self> {
        let heic = heic.into();
        if !is_heif(&heic) || width == 0 || height == 0 {
            return None;
        }
        Some(Self {
            png: heic,
            format: PictureFormat::Heic,
            width,
            height,
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

/// Whether bytes look like an SVG file: an XML document, after a byte
/// order mark, white space and comments, whose first element is `svg`.
pub fn is_svg(bytes: &[u8]) -> bool {
    let head = bytes.get(..bytes.len().min(4096)).unwrap_or_default();
    let Ok(text) = std::str::from_utf8(head).or_else(|e| {
        // A head cut inside a character.
        std::str::from_utf8(head.get(..e.valid_up_to()).unwrap_or_default())
    }) else {
        return false;
    };
    let mut rest = text.trim_start_matches('\u{feff}');
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("<?") {
            let Some(end) = after.find("?>") else {
                return false;
            };
            rest = after.get(end + 2..).unwrap_or_default();
        } else if let Some(after) = rest.strip_prefix("<!--") {
            let Some(end) = after.find("-->") else {
                return false;
            };
            rest = after.get(end + 3..).unwrap_or_default();
        } else if let Some(after) = rest.strip_prefix("<!DOCTYPE") {
            // An internal subset, between brackets, holds `>` of its own,
            // as the entities that Illustrator declares.
            let after = match (after.find('['), after.find('>')) {
                (Some(open), Some(close)) if open < close => {
                    let Some(end) = after.get(open..).and_then(|a| a.find(']')) else {
                        return false;
                    };
                    after.get(open + end + 1..).unwrap_or_default()
                }
                _ => after,
            };
            let Some(end) = after.find('>') else {
                return false;
            };
            rest = after.get(end + 1..).unwrap_or_default();
        } else {
            break;
        }
    }
    rest.strip_prefix("<svg")
        .is_some_and(|after| after.starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/'))
        || rest.starts_with("<svg:svg")
}

/// Whether bytes start as a HEIF file of still pictures, such as HEIC: an
/// `ftyp` box whose brands name one (W108).
pub fn is_heif(bytes: &[u8]) -> bool {
    if bytes.get(4..8) != Some(b"ftyp") {
        return false;
    }
    let size = bytes
        .get(..4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, |b: [u8; 4]| u32::from_be_bytes(b) as usize)
        .min(bytes.len())
        .min(256);
    let brands = bytes.get(8..size).unwrap_or_default();
    // Brands of HEVC pictures only: "mif1" alone is that of AVIF too,
    // which Desk does not decode.
    brands.as_chunks::<4>().0.iter().any(|brand| {
        matches!(
            brand,
            b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx"
        )
    })
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
        self.png.get(..ends).hash(state);
        self.png.get(self.png.len() - ends..).hash(state);
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

    #[test]
    fn w108_svg_and_heic_are_known_by_their_first_bytes() {
        assert!(is_svg(b"<svg xmlns='http://www.w3.org/2000/svg'/>"));
        assert!(is_svg(
            b"\xef\xbb\xbf<?xml version='1.0'?>\n<!-- made -->\n<!DOCTYPE svg>\n<svg>"
        ));
        assert!(!is_svg(b"<svgx>"));
        assert!(is_svg(b"<svg/>"));
        // An internal subset holds `>` of its own, as Illustrator writes.
        assert!(is_svg(
            b"<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\" [\n<!ENTITY ns \"a\">\n]>\n<svg version=\"1.1\">"
        ));
        assert!(!is_svg(b"<html><svg>"));
        let svg = Picture::from_svg(b"<svg width='20' height='10'/>".to_vec(), 20.0, 10.0).unwrap();
        assert_eq!(svg.format(), PictureFormat::Svg);
        assert!(svg.format().is_vector());
        assert!(Picture::from_svg(b"<svg/>".to_vec(), 0.0, 10.0).is_none());
        let mut heic = 24u32.to_be_bytes().to_vec();
        heic.extend_from_slice(b"ftypheic\0\0\0\0mif1heic");
        assert!(is_heif(&heic));
        assert!(!is_heif(b"\0\0\0\x18ftypisom\0\0\0\0isommp41"));
        let picture = Picture::from_heic(heic, 64, 48).unwrap();
        assert_eq!(picture.format().extension(), "heic");
        assert!(!picture.format().is_vector());
    }
}
