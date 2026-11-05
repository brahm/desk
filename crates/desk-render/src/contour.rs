// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The outline of the opaque part of a picture, for text to wrap tightly
//! around it (OBJ-17).

use std::sync::Arc;

use desk_model::{Object, ObjectKind, POLYGON_SCALE};

/// How many bands of the picture the outline follows at most.
const MAX_BANDS: usize = 64;

/// How opaque a pixel is to count, from 0 to 255: half.
const OPAQUE: u8 = 128;

/// The contour that text wraps tightly around for an object: the outline
/// of a shape, or of the opaque part of a picture after Instant Alpha, in
/// parts of [`POLYGON_SCALE`] of its frame. None for other objects, and
/// for a picture with nothing opaque.
pub fn object_contour(object: &Object) -> Option<Arc<[(i32, i32)]>> {
    let ObjectKind::Image(picture) = &object.kind else {
        return object.shape_contour();
    };
    let source = crate::picture_surface(picture)?;
    let settings = &object.picture;
    let surface = if settings.alpha.is_empty() {
        source
    } else {
        crate::adjusted_surface(&source, &settings.alpha, desk_model::Adjustments::default())
            .ok()?
    };
    let spans = opaque_spans(surface)?;
    let (fw, fh) = (object.frame.width.to_pt(), object.frame.height.to_pt());
    if fw <= 0.0 || fh <= 0.0 {
        return None;
    }
    // The picture fills its mask, placed from the frame, or the frame.
    let (mx, my, mw, mh) = match settings.mask {
        Some(mask) => (
            mask.x.to_pt() / fw,
            mask.y.to_pt() / fh,
            mask.width.to_pt() / fw,
            mask.height.to_pt() / fh,
        ),
        None => (0.0, 0.0, 1.0, 1.0),
    };
    let scale = f64::from(POLYGON_SCALE);
    let place = |x: f64, y: f64| {
        let x = (mx + x * mw) * scale;
        let y = (my + y * mh) * scale;
        (
            x.round().clamp(0.0, scale) as i32,
            y.round().clamp(0.0, scale) as i32,
        )
    };
    // Down the right edges of the bands, then up their left edges.
    let mut points: Vec<(i32, i32)> = Vec::with_capacity(spans.len() * 4);
    for &(top, bottom, _, right) in &spans {
        points.push(place(right, top));
        points.push(place(right, bottom));
    }
    for &(top, bottom, left, _) in spans.iter().rev() {
        points.push(place(left, bottom));
        points.push(place(left, top));
    }
    points.dedup();
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    (points.len() >= 3).then(|| points.into())
}

/// The bands of a picture that have opaque pixels, from top to bottom: the
/// top and bottom of each band and the left and right of its opaque
/// pixels, in parts of the picture.
fn opaque_spans(mut surface: cairo::ImageSurface) -> Option<Vec<(f64, f64, f64, f64)>> {
    surface.flush();
    let (w, h) = (surface.width() as usize, surface.height() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let stride = surface.stride() as usize;
    let format = surface.format();
    let data = surface.data().ok()?;
    let bands = h.min(MAX_BANDS);
    let mut spans: Vec<(f64, f64, f64, f64)> = Vec::new();
    for band in 0..bands {
        let (first, last) = (band * h / bands, (band + 1) * h / bands);
        let mut left = usize::MAX;
        let mut right = 0;
        for row in first..last {
            let pixels = &data[row * stride..][..w * 4];
            let opaque = |x: &usize| format != cairo::Format::ARgb32 || pixels[x * 4 + 3] >= OPAQUE;
            if let Some(l) = (0..w).find(opaque) {
                left = left.min(l);
                right = right.max((0..w).rev().find(opaque).unwrap_or(l) + 1);
            }
        }
        if left < right {
            // A band as wide as the one above it lengthens it.
            if let Some(above) = spans.last_mut()
                && above.1 == first as f64 / h as f64
                && (above.2, above.3) == (left as f64 / w as f64, right as f64 / w as f64)
            {
                above.1 = last as f64 / h as f64;
                continue;
            }
            spans.push((
                first as f64 / h as f64,
                last as f64 / h as f64,
                left as f64 / w as f64,
                right as f64 / w as f64,
            ));
        }
    }
    (!spans.is_empty()).then_some(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{Frame, NodeId, Picture};

    /// A picture of 4 by 4 pixels with only its right half opaque.
    fn right_half() -> Picture {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 4, 4).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(1.0, 0.0, 0.0);
            cr.rectangle(2.0, 0.0, 2.0, 4.0);
            cr.fill().unwrap();
        }
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        Picture::from_png(png).unwrap()
    }

    #[test]
    fn obj_17_text_wraps_around_the_opaque_part_of_a_picture() {
        let mut object = Object::new(
            NodeId::new(1).unwrap(),
            ObjectKind::Image(right_half()),
            Frame::from_pt(0.0, 0.0, 40.0, 40.0),
        );
        let contour = object_contour(&object).unwrap();
        assert_eq!(
            &*contour,
            &[(10_000, 0), (10_000, 10_000), (5_000, 10_000), (5_000, 0)]
        );
        // Masked to its right half, the opaque part fills the frame.
        object.picture.mask = Some(Frame::from_pt(-40.0, 0.0, 80.0, 40.0));
        let contour = object_contour(&object).unwrap();
        assert_eq!(
            &*contour,
            &[(10_000, 0), (10_000, 10_000), (0, 10_000), (0, 0)]
        );
        // Nothing opaque in the frame: no contour.
        object.picture.mask = Some(Frame::from_pt(0.0, 0.0, 80.0, 40.0));
        assert_eq!(object_contour(&object), None);
    }
}
