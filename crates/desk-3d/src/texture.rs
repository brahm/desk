// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The pictures of textures: PNG and JPEG, decoded into RGBA and made
//! smaller when a side is longer than [`limits::TEXTURE_SIDE`], as a model
//! drawn on a page needs no more.

use crate::{Texture, limits};

/// Decodes a picture; none when it is not PNG or JPEG, is broken, or has
/// more pixels than [`limits::TEXTURE_PIXELS`].
pub(crate) fn decode(bytes: &[u8]) -> Option<Texture> {
    decode_shrunk(bytes).map(|(t, _)| t)
}

/// Decodes a picture as [`decode`] does, and tells whether it was made
/// smaller.
pub(crate) fn decode_shrunk(bytes: &[u8]) -> Option<(Texture, bool)> {
    let texture = if bytes.starts_with(b"\x89PNG") {
        png(bytes)?
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        jpeg(bytes)?
    } else {
        return None;
    };
    let side = texture.width.max(texture.height);
    Some((shrink(texture), side > limits::TEXTURE_SIDE))
}

/// A PNG of a texture.
pub(crate) fn encode(texture: &Texture) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, texture.width, texture.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&texture.rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

/// Whether a size is within the limit, before the pixels are allocated.
fn fits(width: u64, height: u64) -> bool {
    width > 0 && height > 0 && width * height <= limits::TEXTURE_PIXELS
}

fn png(bytes: &[u8]) -> Option<Texture> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if !fits(u64::from(width), u64::from(height)) {
        return None;
    }
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        _ => 1,
    };
    let (w, h) = (info.width as usize, info.height as usize);
    let stride = info.line_size;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = pixels.get(y * stride..y * stride + w * channels)?;
        for p in row.chunks_exact(channels) {
            match *p {
                [r, g, b, a] => rgba.extend([r, g, b, a]),
                [r, g, b] => rgba.extend([r, g, b, 255]),
                [v, a] => rgba.extend([v, v, v, a]),
                [v] => rgba.extend([v, v, v, 255]),
                _ => return None,
            }
        }
    }
    Some(Texture {
        width: info.width,
        height: info.height,
        rgba,
    })
}

fn jpeg(bytes: &[u8]) -> Option<Texture> {
    use zune_jpeg::JpegDecoder;
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_max_width(1 << 16)
        .set_max_height(1 << 16);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().ok()?;
    let info = decoder.info()?;
    let (width, height) = (usize::from(info.width), usize::from(info.height));
    if !fits(width as u64, height as u64) {
        return None;
    }
    let pixels = decoder.decode().ok()?;
    let rgb = pixels.get(..width * height * 3)?;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for &[r, g, b] in rgb.as_chunks::<3>().0 {
        rgba.extend([r, g, b, 255]);
    }
    Some(Texture {
        width: u32::from(info.width),
        height: u32::from(info.height),
        rgba,
    })
}

/// The texture made smaller, by averaging, until no side is longer than
/// [`limits::TEXTURE_SIDE`].
fn shrink(mut texture: Texture) -> Texture {
    while texture.width.max(texture.height) > limits::TEXTURE_SIDE {
        let (w, h) = (texture.width as usize, texture.height as usize);
        let (nw, nh) = (w.div_ceil(2), h.div_ceil(2));
        let mut out = Vec::with_capacity(nw * nh * 4);
        for y in 0..nh {
            for x in 0..nw {
                let mut sum = [0u32; 4];
                let mut n = 0;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = (2 * x + dx, 2 * y + dy);
                    if sx >= w || sy >= h {
                        continue;
                    }
                    let at = (sy * w + sx) * 4;
                    if let Some(p) = texture.rgba.get(at..at + 4) {
                        for (s, v) in sum.iter_mut().zip(p) {
                            *s += u32::from(*v);
                        }
                        n += 1;
                    }
                }
                let n = n.max(1);
                out.extend(sum.map(|s| (s / n) as u8));
            }
        }
        texture = Texture {
            width: nw as u32,
            height: nh as u32,
            rgba: out,
        };
    }
    texture
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_large_textures_are_made_smaller() {
        let side = limits::TEXTURE_SIDE * 2 + 2;
        let big = Texture {
            width: side,
            height: 2,
            rgba: vec![200; side as usize * 2 * 4],
        };
        let small = shrink(big);
        assert!(small.width <= limits::TEXTURE_SIDE);
        assert_eq!(small.height, 1);
        assert_eq!(small.rgba.len(), small.width as usize * 4);
        assert!(small.rgba.iter().all(|v| *v == 200));
        assert!(decode(b"not a picture").is_none());
        let picture = encode(&small).unwrap();
        assert_eq!(decode_shrunk(&picture), Some((small, false)));
    }
}
