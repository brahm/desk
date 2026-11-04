// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pictures from other formats as the model keeps them (LST-03, OBJ-01):
//! PNG, JPEG and PDF as they are, and TIFF, which Pages saves image
//! bullets in, and GIF, converted to PNG.

use std::io::Cursor;

use desk_model::Picture;

/// The largest side of a converted picture, in pixels: plenty for a
/// bullet, and it keeps a large icon from making the document large.
pub const MAX_SIDE: u32 = 256;

/// A picture from the bytes of a PNG or TIFF file, or `None` when they are
/// neither or cannot be read.
pub fn picture(bytes: &[u8]) -> Option<Picture> {
    picture_within(bytes, MAX_SIDE)
}

/// A picture from the bytes of a PNG, JPEG, PDF, TIFF or GIF file, a TIFF
/// or GIF one scaled down to fit `max_side`, or `None` when they are none
/// of those or cannot be read.
pub fn picture_within(bytes: &[u8], max_side: u32) -> Option<Picture> {
    if let Some(picture) = Picture::from_png(bytes.to_vec()) {
        return Some(picture);
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return Picture::from_jpeg(bytes.to_vec());
    }
    if bytes.starts_with(b"%PDF-") {
        return from_pdf(bytes);
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return from_tiff(bytes, max_side.max(1));
    }
    if bytes.starts_with(b"GIF8") {
        return from_gif(bytes, max_side.max(1));
    }
    None
}

/// The first page of a PDF file, measured by Poppler.
fn from_pdf(bytes: &[u8]) -> Option<Picture> {
    let data = glib::Bytes::from(bytes);
    let document = poppler::Document::from_bytes(&data, None).ok()?;
    let (width, height) = document.page(0)?.size();
    Picture::from_pdf(bytes.to_vec(), width, height)
}

/// The first frame of a GIF file as 8-bit RGBA, scaled down to fit
/// `max_side`, in PNG.
fn from_gif(bytes: &[u8], max_side: u32) -> Option<Picture> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(Cursor::new(bytes)).ok()?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    if width == 0 || height == 0 {
        return None;
    }
    // The frame is drawn on a clear screen of the size of the file.
    let frame = decoder.read_next_frame().ok()??;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let (fw, fh) = (u32::from(frame.width), u32::from(frame.height));
    let (left, top) = (u32::from(frame.left), u32::from(frame.top));
    for y in 0..fh.min(height.saturating_sub(top)) {
        for x in 0..fw.min(width.saturating_sub(left)) {
            let from = ((y * fw + x) * 4) as usize;
            let to = (((y + top) * width + x + left) * 4) as usize;
            if let Some(pixel) = frame.buffer.get(from..from + 4) {
                rgba[to..to + 4].copy_from_slice(pixel);
            }
        }
    }
    let (rgba, width, height) = scaled(&rgba, width, height, max_side);
    encode(&rgba, width, height)
}

/// The pixels of a TIFF file as 8-bit RGBA, scaled down to fit
/// `max_side`, in PNG.
fn from_tiff(bytes: &[u8], max_side: u32) -> Option<Picture> {
    use tiff::ColorType;
    use tiff::decoder::{Decoder, DecodingResult};
    let mut decoder = Decoder::new(Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions().ok()?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 64 << 20 {
        return None;
    }
    let color = decoder.colortype().ok()?;
    let DecodingResult::U8(data) = decoder.read_image().ok()? else {
        return None;
    };
    let channels = match color {
        ColorType::Gray(8) => 1,
        ColorType::GrayA(8) => 2,
        ColorType::RGB(8) => 3,
        ColorType::RGBA(8) => 4,
        _ => return None,
    };
    let pixels = (width * height) as usize;
    if data.len() < pixels * channels {
        return None;
    }
    let rgba: Vec<u8> = data
        .chunks_exact(channels)
        .take(pixels)
        .flat_map(|p| match channels {
            1 => [p[0], p[0], p[0], 255],
            2 => [p[0], p[0], p[0], p[1]],
            3 => [p[0], p[1], p[2], 255],
            _ => [p[0], p[1], p[2], p[3]],
        })
        .collect();
    let (rgba, width, height) = scaled(&rgba, width, height, max_side);
    encode(&rgba, width, height)
}

/// RGBA pixels scaled down by a whole factor, averaging each square, so
/// that the larger side fits `max_side`.
fn scaled(rgba: &[u8], width: u32, height: u32, max_side: u32) -> (Vec<u8>, u32, u32) {
    let factor = width.max(height).div_ceil(max_side).max(1);
    if factor == 1 {
        return (rgba.to_vec(), width, height);
    }
    let (w, h) = (width.div_ceil(factor), height.div_ceil(factor));
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            let mut count = 0;
            for sy in y * factor..((y + 1) * factor).min(height) {
                for sx in x * factor..((x + 1) * factor).min(width) {
                    let at = ((sy * width + sx) * 4) as usize;
                    // Colors weighed by their opacity, so that clear
                    // pixels do not darken the edges.
                    let alpha = u32::from(rgba[at + 3]);
                    for (c, s) in sum.iter_mut().take(3).enumerate() {
                        *s += u32::from(rgba[at + c]) * alpha;
                    }
                    sum[3] += alpha;
                    count += 1;
                }
            }
            let alpha = sum[3];
            for s in &sum[..3] {
                out.push(s.checked_div(alpha).unwrap_or(0) as u8);
            }
            out.push((alpha / count.max(1)) as u8);
        }
    }
    (out, w, h)
}

/// Encodes RGBA pixels as PNG.
pub fn encode(rgba: &[u8], width: u32, height: u32) -> Option<Picture> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Picture::from_png(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiff(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut bytes).unwrap();
        let pixels: Vec<u8> = (0..width * height)
            .flat_map(|i| [(i % 256) as u8, 0, 255, 255])
            .collect();
        encoder
            .write_image::<tiff::encoder::colortype::RGBA8>(width, height, &pixels)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn lst_03_tiff_pictures_become_png() {
        let picture = picture(&tiff(4, 2)).unwrap();
        assert_eq!((picture.width(), picture.height()), (4, 2));
        // A large one is scaled down to fit.
        let large = picture_of(&tiff(600, 300));
        assert_eq!((large.width(), large.height()), (200, 100));
        // PNG stays as it is.
        let again = super::picture(large.data()).unwrap();
        assert_eq!(again, large);
        assert!(super::picture(b"not a picture").is_none());
    }

    fn picture_of(bytes: &[u8]) -> Picture {
        picture(bytes).unwrap()
    }
}
