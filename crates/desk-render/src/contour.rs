// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The outline of the opaque part of a picture, for text to wrap tightly
//! around it (OBJ-17).

use std::sync::Arc;

use desk_model::{Object, ObjectKind, POLYGON_SCALE};

/// Sets the contour that text wraps tightly around an object, as
/// [`object_contours`] traces it: its first part and the parts apart from
/// it. None when there is nothing to go around.
pub fn trace_contour(object: &mut Object) {
    let mut parts = object_contours(object).into_iter();
    object.contour = parts.next();
    object.contour_parts = parts.collect();
}

/// The contour that text wraps tightly around for an object: the outline
/// of a shape, or of the opaque part of a picture after Instant Alpha, in
/// parts of [`POLYGON_SCALE`] of its frame. None for other objects, and
/// for a picture with nothing opaque.
pub fn object_contour(object: &Object) -> Option<Arc<[(i32, i32)]>> {
    object_contours(object).into_iter().next()
}

/// The parts of the contour that text wraps tightly around for an object:
/// one for a shape, and one for each part apart of the opaque pixels of a
/// picture, which Pages 15.4 runs the text between (B-047). Empty when
/// there is nothing to go around.
pub fn object_contours(object: &Object) -> Vec<Arc<[(i32, i32)]>> {
    let ObjectKind::Image(picture) = &object.kind else {
        return object.shape_contour().into_iter().collect();
    };
    let Some(source) = crate::picture_surface(picture) else {
        return Vec::new();
    };
    let settings = &object.picture;
    let mut surface = if settings.alpha.is_empty() && settings.alpha_kept.is_none() {
        source
    } else {
        match crate::adjusted_surface(
            &source,
            &settings.alpha,
            settings.alpha_kept.as_ref(),
            desk_model::Adjustments::default(),
        ) {
            Ok(surface) => surface,
            Err(_) => return Vec::new(),
        }
    };
    let (fw, fh) = (object.frame.width.to_pt(), object.frame.height.to_pt());
    if fw <= 0.0 || fh <= 0.0 {
        return Vec::new();
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
    // The outlines of the opaque pixels as Pages traces them.
    traced_outlines(&mut surface, object.wrap_threshold)
        .into_iter()
        .filter_map(|outline| {
            let mut points: Vec<(i32, i32)> =
                outline.into_iter().map(|(x, y)| place(x, y)).collect();
            points.dedup();
            if points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            // A part that the mask leaves out is a line along its edge.
            let (xs, ys): (Vec<i32>, Vec<i32>) = points.iter().copied().unzip();
            let flat = |v: &[i32]| v.iter().min() == v.iter().max();
            (points.len() >= 3 && !flat(&xs) && !flat(&ys)).then(|| points.into())
        })
        .collect()
}

/// The least alpha of a pixel more opaque than `threshold` percent: 50%
/// counts the pixels from 128 up, as Pages does.
fn least_alpha(threshold: u8) -> u16 {
    (f64::from(threshold.min(100)) * 255.0 / 100.0).floor() as u16 + 1
}

/// The outlines of the parts of the opaque pixels of a picture, as Pages
/// 15.4 traces them, in parts of the picture. Empty when the picture has
/// nothing opaque.
fn traced_outlines(surface: &mut cairo::ImageSurface, threshold: u8) -> Vec<Vec<(f64, f64)>> {
    surface.flush();
    let (w, h) = (surface.width() as usize, surface.height() as usize);
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let stride = surface.stride() as usize;
    let format = surface.format();
    let least = least_alpha(threshold);
    let Ok(data) = surface.data() else {
        return Vec::new();
    };
    desk_model::pixel_outlines(w, h, |x, y| {
        format != cairo::Format::ARgb32 || u16::from(data[y * stride + x * 4 + 3]) >= least
    })
    .into_iter()
    .map(|outline| {
        outline
            .into_iter()
            .map(|(x, y)| (f64::from(x) / w as f64, f64::from(y) / h as f64))
            .collect()
    })
    .collect()
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
            &[(5_000, 0), (10_000, 0), (10_000, 10_000), (5_000, 10_000)]
        );
        // Masked to its right half, the opaque part fills the frame.
        object.picture.mask = Some(Frame::from_pt(-40.0, 0.0, 80.0, 40.0));
        let contour = object_contour(&object).unwrap();
        assert_eq!(
            &*contour,
            &[(0, 0), (10_000, 0), (10_000, 10_000), (0, 10_000)]
        );
        // Nothing opaque in the frame: no contour.
        object.picture.mask = Some(Frame::from_pt(0.0, 0.0, 80.0, 40.0));
        assert_eq!(object_contour(&object), None);
    }

    #[test]
    fn obj_17_the_parts_apart_of_a_picture_each_have_an_outline() {
        // Two opaque columns of 4 by 4 pixels with a gap between them.
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 4, 4).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(1.0, 0.0, 0.0);
            cr.rectangle(0.0, 0.0, 1.0, 4.0);
            cr.rectangle(3.0, 0.0, 1.0, 4.0);
            cr.fill().unwrap();
        }
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        let mut object = Object::new(
            NodeId::new(1).unwrap(),
            ObjectKind::Image(Picture::from_png(png).unwrap()),
            Frame::from_pt(0.0, 0.0, 40.0, 40.0),
        );
        trace_contour(&mut object);
        assert_eq!(
            object.contour.as_deref(),
            Some(&[(0, 0), (2_500, 0), (2_500, 10_000), (0, 10_000)][..])
        );
        assert_eq!(
            object
                .contour_parts
                .iter()
                .map(|p| p.to_vec())
                .collect::<Vec<_>>(),
            [vec![
                (7_500, 0),
                (10_000, 0),
                (10_000, 10_000),
                (7_500, 10_000)
            ]]
        );
    }

    #[test]
    fn obj_17_the_transparency_threshold_sets_what_text_goes_around() {
        // The left half a third opaque, the right half opaque.
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 4, 4).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgba(1.0, 0.0, 0.0, 0.3);
            cr.rectangle(0.0, 0.0, 2.0, 4.0);
            cr.fill().unwrap();
            cr.set_source_rgb(1.0, 0.0, 0.0);
            cr.rectangle(2.0, 0.0, 2.0, 4.0);
            cr.fill().unwrap();
        }
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        let mut object = Object::new(
            NodeId::new(1).unwrap(),
            ObjectKind::Image(Picture::from_png(png).unwrap()),
            Frame::from_pt(0.0, 0.0, 40.0, 40.0),
        );
        // At half, the text goes around the right half only.
        let left = |object: &Object| object_contour(object).unwrap().iter().map(|p| p.0).min();
        assert_eq!(left(&object), Some(5_000));
        // At a fifth, around the whole picture.
        object.wrap_threshold = 20;
        assert_eq!(left(&object), Some(0));
    }
}
