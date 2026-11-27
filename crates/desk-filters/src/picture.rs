// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pictures from other formats as the model keeps them (LST-03, OBJ-01):
//! PNG, JPEG, PDF, SVG and HEIC as they are, and TIFF, which Pages saves
//! image bullets in, and GIF, converted to PNG.

use std::io::Cursor;

use desk_model::Picture;

/// The largest side of a converted picture, in pixels: plenty for a
/// bullet, and it keeps a large icon from making the document large.
pub const MAX_SIDE: u32 = 256;

/// The largest side of the picture of an object, a fill or a poster, in
/// pixels, from ODT and from Pages alike.
pub const OBJECT_MAX_SIDE: u32 = 4096;

/// The most pixels a TIFF or GIF picture may have, so that a small file
/// that declares a huge size cannot take all the memory: 64 million,
/// 256 MB of RGBA.
const MAX_PIXELS: u64 = 64 << 20;

/// Whether a picture of this size may be converted.
fn fits(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

/// A picture from the bytes of a PNG or TIFF file, or `None` when they are
/// neither or cannot be read.
pub fn picture(bytes: &[u8]) -> Option<Picture> {
    picture_within(bytes, MAX_SIDE)
}

/// Whether the picture in these bytes comes out at a size that depends on
/// the largest side asked for: a TIFF or a GIF file, which is converted.
pub fn scaled_when_read(bytes: &[u8]) -> bool {
    bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") || bytes.starts_with(b"GIF8")
}

/// A picture from the bytes of a PNG, JPEG, PDF, SVG, HEIC, TIFF or GIF
/// file, a TIFF or GIF one scaled down to fit `max_side`, or `None` when
/// they are none of those or cannot be read.
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
    if desk_model::is_heif(bytes) {
        return from_heic(bytes);
    }
    if desk_model::is_svg(bytes) {
        return from_svg(bytes);
    }
    None
}

/// An SVG file, measured by librsvg, which loads nothing from outside it
/// (W108).
fn from_svg(bytes: &[u8]) -> Option<Picture> {
    let (width, height) = svg_size(bytes)?;
    Picture::from_svg(bytes.to_vec(), width, height)
}

/// The size of an SVG file in pixels of 96 to the inch: the one it
/// declares, or that of its view box.
fn svg_size(bytes: &[u8]) -> Option<(f64, f64)> {
    use librsvg_rebind::prelude::*;
    let handle = librsvg_rebind::Handle::from_data(bytes).ok()?;
    let (width, height) = handle.intrinsic_size_in_pixels().or_else(|| {
        let (_, _, view) = handle.intrinsic_dimensions();
        view.map(|v| (v.width(), v.height()))
    })?;
    (width.is_finite() && height.is_finite() && width >= 1.0 && height >= 1.0)
        .then_some((width, height))
}

/// A HEIC or HEIF file, measured by libheif, with its turns (W108).
fn from_heic(bytes: &[u8]) -> Option<Picture> {
    let context = libheif_rs::HeifContext::read_from_bytes(bytes).ok()?;
    let handle = context.primary_image_handle().ok()?;
    let (width, height) = (handle.width(), handle.height());
    fits(width, height).then_some(())?;
    Picture::from_heic(bytes.to_vec(), width, height)
}

/// The largest side of the PNG that stands for an SVG or HEIC picture in
/// a file, for the programs that read neither (W108).
pub const FALLBACK_MAX_SIDE: u32 = 2048;

/// A PNG of an SVG or HEIC picture, at most `FALLBACK_MAX_SIDE` on its
/// larger side, for the programs that read neither; `None` for a picture
/// of another format, or one that cannot be drawn (W108).
pub fn png_fallback(picture: &Picture) -> Option<Picture> {
    match picture.format() {
        desk_model::PictureFormat::Svg => svg_png(picture),
        desk_model::PictureFormat::Heic => heic_png(picture.data()),
        _ => None,
    }
}

fn svg_png(picture: &Picture) -> Option<Picture> {
    use librsvg_rebind::prelude::*;
    let handle = librsvg_rebind::Handle::from_data(picture.data()).ok()?;
    let (w, h) = (f64::from(picture.width()), f64::from(picture.height()));
    let scale = (f64::from(FALLBACK_MAX_SIDE) / w.max(h)).min(1.0);
    let width = (w * scale).round().max(1.0) as i32;
    let height = (h * scale).round().max(1.0) as i32;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        handle
            .render_document(
                &cr,
                &librsvg_rebind::Rectangle::new(0.0, 0.0, f64::from(width), f64::from(height)),
            )
            .ok()?;
    }
    surface.flush();
    let mut png = Vec::new();
    surface.write_to_png(&mut png).ok()?;
    Picture::from_png(png)
}

fn heic_png(bytes: &[u8]) -> Option<Picture> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
    let context = HeifContext::read_from_bytes(bytes).ok()?;
    let handle = context.primary_image_handle().ok()?;
    if !fits(handle.width(), handle.height()) {
        return None;
    }
    let image = LibHeif::new()
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .ok()?;
    let planes = image.planes();
    let plane = planes.interleaved?;
    let (width, height) = (plane.width, plane.height);
    if !fits(width, height) {
        return None;
    }
    let row = width as usize * 4;
    let mut rgba = Vec::with_capacity(row * height as usize);
    for y in 0..height as usize {
        rgba.extend_from_slice(plane.data.get(y * plane.stride..)?.get(..row)?);
    }
    let (rgba, width, height) = scaled(&rgba, width, height, FALLBACK_MAX_SIDE);
    encode(&rgba, width, height)
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
    if !fits(width, height) {
        return None;
    }
    // The frame is drawn on a clear screen of the size of the file. The
    // offsets are worked out in usize: in u32, a screen of 65535 × 65535
    // pixels overflows.
    let frame = decoder.read_next_frame().ok()??;
    let (width_px, height_px) = (width as usize, height as usize);
    let mut rgba = vec![0u8; width_px * height_px * 4];
    let (fw, fh) = (usize::from(frame.width), usize::from(frame.height));
    let (left, top) = (usize::from(frame.left), usize::from(frame.top));
    for y in 0..fh.min(height_px.saturating_sub(top)) {
        for x in 0..fw.min(width_px.saturating_sub(left)) {
            let from = (y * fw + x) * 4;
            let to = ((y + top) * width_px + x + left) * 4;
            if let (Some(pixel), Some(target)) =
                (frame.buffer.get(from..from + 4), rgba.get_mut(to..to + 4))
            {
                target.copy_from_slice(pixel);
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
    if !fits(width, height) {
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
            // In u64: a square of many pixels, each colour weighed by its
            // opacity, overflows u32.
            let mut sum = [0u64; 4];
            let mut count = 0;
            for sy in y * factor..((y + 1) * factor).min(height) {
                for sx in x * factor..((x + 1) * factor).min(width) {
                    let at = ((sy * width + sx) * 4) as usize;
                    // Colors weighed by their opacity, so that clear
                    // pixels do not darken the edges.
                    let alpha = u64::from(rgba[at + 3]);
                    for (c, s) in sum.iter_mut().take(3).enumerate() {
                        *s += u64::from(rgba[at + c]) * alpha;
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

    /// A GIF with a screen of `width` × `height` and one frame of 1 × 1.
    fn gif(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut bytes, width, height, &[]).unwrap();
            let frame = gif::Frame {
                width: 1,
                height: 1,
                left: width - 1,
                top: height - 1,
                palette: Some(vec![255, 0, 0, 0, 0, 0]),
                buffer: std::borrow::Cow::Borrowed(&[0]),
                ..gif::Frame::default()
            };
            encoder.write_frame(&frame).unwrap();
        }
        bytes
    }

    #[test]
    fn a_gif_that_declares_a_huge_screen_is_refused() {
        // A few dozen bytes that declare 65535 × 65535 pixels: the
        // screen was allocated at once, and its size overflowed u32
        // (review RV, batch 1).
        assert!(picture(&gif(65535, 65535)).is_none());
        // A small one keeps its frame, placed on the screen.
        let small = picture_of(&gif(3, 2));
        assert_eq!((small.width(), small.height()), (3, 2));
    }

    #[test]
    fn a_wide_picture_scaled_by_a_large_factor_is_converted() {
        // A 70000 × 2 TIFF scaled to one pixel sums 140000 pixels, which
        // overflowed u32 when weighed by their opacity.
        let wide = picture_within(&tiff(70000, 2), 1).unwrap();
        assert_eq!((wide.width(), wide.height()), (1, 1));
    }
}
