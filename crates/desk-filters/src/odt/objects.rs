// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The floating objects of a document to and from ODF drawing objects
//! (OBJ-01, OBJ-08, OBJ-13, OBJ-17).
//!
//! An object fixed on a page is a drawing object anchored to that page,
//! measured from the corner of the paper; one that moves with a paragraph
//! is anchored to it, measured from the left of the text and the top of
//! the paragraph. An object in the text is anchored `as-char` where its
//! mark is, standing on the baseline.

use std::collections::HashMap;
use std::sync::Arc;

use desk_model::{
    Anchor, Document, FIELD_CHARACTER, Field, Fill, Frame, ImageFill, Length, MAX_TEXT_BOX_COLUMNS,
    Media, NodeId, Object, ObjectKind, Op, POLYGON_SCALE, PageSetup, PageTemplate, PageTemplates,
    ParaAttrs, Paragraph, Placeholder, PlaceholderKind, Repeat, Shape, Stroke, TextBox,
    VerticalAlignment, Wrap,
};
use desk_odf as odf;

use super::{
    FilterError, char_attrs, export_spans, length, odf_color, odf_length, odf_paragraph_properties,
    para_attrs, paragraph_properties, rgba, text_properties,
};

use crate::picture::OBJECT_MAX_SIDE as MAX_SIDE;

/// The graphic properties of a style, with those of its parents under
/// them.
fn graphic(source: &odf::TextDocument, name: Option<&str>) -> odf::GraphicProperties {
    let mut chain = Vec::new();
    let mut next = name.map(str::to_owned);
    while let Some(name) = next.take() {
        if chain.len() > 16 {
            break;
        }
        let Some(style) = source
            .automatic_styles
            .iter()
            .chain(&source.styles)
            .find(|s| s.family == odf::Family::Graphic && s.name == name)
        else {
            break;
        };
        if !chain.is_empty() && style.properties.graphic.object_style.is_some() {
            // An object style of Desk is only a name for the object: its
            // own style says the whole look.
            break;
        }
        chain.push(style.properties.graphic.clone());
        next.clone_from(&style.parent);
    }
    let mut out = odf::GraphicProperties::default();
    for g in chain.into_iter().rev() {
        macro_rules! over {
            ($($field:ident),*) => { $( if g.$field.is_some() { out.$field = g.$field; } )* };
        }
        over!(
            fill,
            fill_color,
            stroke,
            stroke_color,
            stroke_width,
            stroke_style,
            fill_gradient_name,
            fill_image_name,
            repeat,
            fill_image_mode,
            clip,
            luminance,
            contrast,
            mask,
            mask_shape,
            image_adjust,
            image_sharpness,
            image_levels,
            instant_alpha,
            alpha_kept,
            marker_start,
            marker_start_width,
            marker_end,
            marker_end_width,
            shadow,
            shadow_offset_x,
            shadow_offset_y,
            shadow_color,
            shadow_opacity,
            shadow_blur,
            reflection,
            picture_frame,
            picture_frame_scale,
            master_pages,
            fill_tint,
            fill_tint_opacity,
            opacity,
            wrap,
            wrap_contour,
            wrap_threshold,
            run_through,
            margin,
            padding,
            protect,
            mirror,
            horizontal_pos,
            horizontal_rel,
            vertical_pos,
            vertical_rel,
            textarea_vertical_align,
            column_count,
            column_gap
        );
    }
    out
}

/// A gradient of ODF as Desk fills with it (OBJ-09, OBJ-41): along a
/// line from one color to the other; the colors of an advanced gradient
/// of Desk; a radial gradient out from its center, as the other round and
/// square ones are taken; or an axial one as a line that turns back.
pub(super) fn model_gradient(gradient: &odf::Gradient) -> Fill {
    use desk_model::{Gradient, GradientStop};
    // ODF turns its gradient counterclockwise from going down; Desk
    // measures the way it goes counterclockwise from going right.
    let angle = (((gradient.angle + 270.0) * 100.0).round() as i64).rem_euclid(36000) as i32;
    let start = gradient.start_color.map_or(desk_model::Rgba::BLACK, rgba);
    let end = gradient.end_color.map_or(desk_model::Rgba::WHITE, rgba);
    let radial = matches!(
        gradient.style.as_str(),
        "radial" | "ellipsoid" | "square" | "rectangular"
    );
    let center = radial.then(|| {
        let (cx, cy) = gradient.center.unwrap_or((50.0, 50.0));
        let part = |v: f64| (v * 10.0).round().clamp(0.0, 1000.0) as u16;
        (part(cx), part(cy))
    });
    let mut stops: Vec<GradientStop> = gradient
        .stops
        .iter()
        .map(|stop| {
            let mut color = stop.color.map_or(desk_model::Rgba::BLACK, rgba);
            if let Some(opacity) = stop.opacity {
                color.a = (opacity * 2.55).round().clamp(0.0, 255.0) as u8;
            }
            GradientStop {
                blend: stop
                    .blend
                    .map_or(500, |b| (b * 1000.0).round().clamp(0.0, 1000.0) as u16),
                ..GradientStop::new((stop.offset * 1000.0).round() as u16, color)
            }
        })
        .collect();
    if stops.len() < 2 {
        stops = match (radial, gradient.style.as_str()) {
            // The end color of ODF is in the middle of these.
            (true, _) => vec![GradientStop::new(0, end), GradientStop::new(1000, start)],
            (false, "axial") => vec![
                GradientStop::new(0, start),
                GradientStop::new(500, end),
                GradientStop::new(1000, start),
            ],
            _ => {
                return Fill::Gradient {
                    from: start,
                    to: end,
                    angle,
                };
            }
        };
    }
    Fill::Advanced(Gradient {
        stops,
        angle,
        center,
    })
}

/// The gradient of ODF that fills as Desk does, without its name.
pub(super) fn odf_gradient(
    from: desk_model::Rgba,
    to: desk_model::Rgba,
    angle: i32,
) -> odf::Gradient {
    odf::Gradient {
        name: String::new(),
        style: "linear".into(),
        start_color: Some(odf_color(from)),
        end_color: Some(odf_color(to)),
        angle: (f64::from(angle) / 100.0 - 270.0).rem_euclid(360.0),
        border: Some(0.0),
        center: None,
        stops: Vec::new(),
    }
}

/// The gradient of ODF for an advanced gradient of Desk, without its
/// name: its first and last colors as ODF has them, for the other apps,
/// and all of them in `desk:gradient-stop` (OBJ-41).
pub(super) fn odf_advanced_gradient(gradient: &desk_model::Gradient) -> odf::Gradient {
    let mut stops = gradient.stops.clone();
    stops.sort_by_key(|s| s.at);
    let first = stops.first().map_or(desk_model::Rgba::BLACK, |s| s.color);
    let last = stops.last().map_or(desk_model::Rgba::WHITE, |s| s.color);
    let mut out = odf_gradient(first, last, gradient.angle);
    if let Some((cx, cy)) = gradient.center {
        out.style = "radial".into();
        // The start color of ODF is outside, and the end color in the
        // middle.
        out.start_color = Some(odf_color(last));
        out.end_color = Some(odf_color(first));
        out.center = Some((f64::from(cx) / 10.0, f64::from(cy) / 10.0));
    }
    out.stops = stops
        .iter()
        .map(|stop| odf::GradientStop {
            offset: f64::from(stop.at) / 1000.0,
            color: Some(odf_color(stop.color)),
            opacity: (stop.color.a != 255).then(|| f64::from(stop.color.a) / 2.55),
            blend: (stop.blend != 500).then(|| f64::from(stop.blend) / 1000.0),
        })
        .collect();
    out
}

/// The end of a line that a marker of ODF draws, by its name, as near as
/// Desk has one (OBJ-10).
fn line_end(marker: Option<&str>) -> desk_model::LineEnd {
    use desk_model::LineEnd;
    // Names in styles write a space as `_20_`.
    let Some(name) = marker
        .map(|n| n.replace("_20_", " ").to_lowercase())
        .filter(|n| !n.is_empty())
    else {
        return LineEnd::None;
    };
    if name.contains("open") || name.contains("line arrow") {
        LineEnd::OpenArrow
    } else if name.contains("circle") {
        LineEnd::Circle
    } else if name.contains("square") || name.contains("diamond") {
        LineEnd::Square
    } else if name.contains("bar") || name.contains("dimension") || name.contains("short") {
        LineEnd::Bar
    } else {
        LineEnd::Arrow
    }
}

/// The marker that Desk writes for an end of a line, with its shape.
fn marker(end: desk_model::LineEnd) -> Option<odf::Marker> {
    use desk_model::LineEnd;
    let (name, view_box, d) = match end {
        LineEnd::None => return None,
        LineEnd::Arrow => ("Arrow", "0 0 20 30", "M10 0l-10 30h20z"),
        LineEnd::OpenArrow => (
            "Open_20_Arrow",
            "0 0 20 20",
            "M10 0l10 18-3 2-7-13-7 13-3-2z",
        ),
        LineEnd::Circle => (
            "Circle",
            "0 0 20 20",
            "M0 10a10 10 0 1 0 20 0a10 10 0 1 0-20 0z",
        ),
        LineEnd::Square => ("Square", "0 0 20 20", "M0 0h20v20h-20z"),
        LineEnd::Bar => ("Bar", "0 0 20 4", "M0 0h20v4h-20z"),
    };
    Some(odf::Marker {
        name: name.into(),
        view_box: view_box.into(),
        d: d.into(),
    })
}

/// The shadow of an object from the properties of its graphic style
/// (OBJ-11): its offset as a distance and a direction, in whole degrees
/// and tenths of a point.
fn model_shadow(g: &odf::GraphicProperties) -> desk_model::ObjectShadow {
    let default = desk_model::ObjectShadow::default();
    let (dx, dy) = match (g.shadow_offset_x, g.shadow_offset_y) {
        (None, None) => default.offset_xy(),
        (x, y) => (
            x.map_or(0.0, |x| length(x).to_pt()),
            y.map_or(0.0, |y| length(y).to_pt()),
        ),
    };
    let offset = ((dx.hypot(dy) * 10.0).round() / 10.0).min(1000.0);
    let angle = if offset > 0.0 {
        ((-dy).atan2(dx).to_degrees().round() as i32).rem_euclid(360) * 100
    } else {
        default.angle
    };
    desk_model::ObjectShadow {
        color: g.shadow_color.map_or(desk_model::Rgba::BLACK, rgba),
        angle,
        offset: Length::from_pt(offset),
        blur: g.shadow_blur.map_or(Length::ZERO, |b| {
            Length::from_pt((length(b).to_pt() * 10.0).round() / 10.0)
        }),
        opacity: g
            .shadow_opacity
            .map_or(100, |o| o.round().clamp(0.0, 100.0) as u8),
    }
}

/// The size of a picture as other programs take it when it says none: a
/// pixel to three quarters of a point, at 96 pixels an inch.
fn natural_size(picture: &desk_model::Picture) -> (f64, f64) {
    (
        f64::from(picture.width()) * 0.75,
        f64::from(picture.height()) * 0.75,
    )
}

/// How a picture is masked and adjusted, as ODF writes it: `fo:clip`,
/// `draw:luminance` and `draw:contrast` for other programs, and the
/// `desk:` attributes that keep it exactly (OBJ-04, OBJ-06, OBJ-07).
fn odf_picture_settings(
    g: &mut odf::GraphicProperties,
    object: &Object,
    picture: &desk_model::Picture,
) {
    let settings = &object.picture;
    if let Some(mask) = settings.mask {
        let (w, h) = (mask.width.to_pt(), mask.height.to_pt());
        if w > 0.0 && h > 0.0 {
            let (nw, nh) = natural_size(picture);
            let (fw, fh) = (object.frame.width.to_pt(), object.frame.height.to_pt());
            let (x, y) = (-mask.x.to_pt(), -mask.y.to_pt());
            let cut = |part: f64, whole: f64, natural: f64| {
                odf_length(Length::from_pt((part / whole * natural).max(0.0)))
            };
            g.clip = Some(format!(
                "rect({}, {}, {}, {})",
                cut(y, h, nh),
                cut(w - x - fw, w, nw),
                cut(h - y - fh, h, nh),
                cut(x, w, nw),
            ));
            g.mask = Some(format!(
                "{} {} {} {}",
                odf_length(mask.x),
                odf_length(mask.y),
                odf_length(mask.width),
                odf_length(mask.height)
            ));
            g.mask_shape = settings.mask_shape.as_ref().and_then(mask_shape_text);
        }
    }
    if !settings.alpha.is_empty() {
        let seeds: Vec<String> = settings
            .alpha
            .iter()
            .map(|s| format!("{} {} {}", s.x, s.y, s.tolerance))
            .collect();
        g.instant_alpha = Some(seeds.join(", "));
    }
    if let Some(kept) = &settings.alpha_kept {
        let polygons: Vec<String> = kept
            .iter()
            .map(|polygon| {
                let points: Vec<String> = polygon.iter().map(|(x, y)| format!("{x} {y}")).collect();
                points.join(" ")
            })
            .collect();
        g.alpha_kept = Some(polygons.join("; "));
    }
    let a = settings.adjust;
    if !a.is_none() {
        if a.brightness != 0 {
            g.luminance = Some(f64::from(a.brightness));
        }
        if a.contrast != 0 {
            g.contrast = Some(f64::from(a.contrast));
        }
        g.image_adjust = Some(format!(
            "{} {} {} {} {} {}",
            a.brightness, a.contrast, a.saturation, a.exposure, a.temperature, a.tint
        ));
        // In attributes of their own, so that older versions of Desk
        // still read the six others (W110).
        if a.sharpness != 0 {
            g.image_sharpness = Some(a.sharpness.to_string());
        }
        if (a.black, a.white) != (0, 255) {
            g.image_levels = Some(format!("{} {}", a.black, a.white));
        }
    }
}

/// The `desk:mask-shape` of the shape of a mask (OBJ-04).
fn mask_shape_text(shape: &Shape) -> Option<String> {
    use desk_model::PathSegment;
    Some(match shape {
        Shape::Rectangle | Shape::Line { .. } | Shape::Connection(_) => return None,
        Shape::Ellipse => "ellipse".into(),
        Shape::RoundedRectangle { radius } => format!("rounded {}", radius.to_pt()),
        Shape::Polygon(points) => {
            let numbers: Vec<String> = points.iter().map(|(x, y)| format!("{x} {y}")).collect();
            format!("points {}", numbers.join(" "))
        }
        Shape::Path { .. } | Shape::Compound(_) => {
            let s = f64::from(POLYGON_SCALE);
            let n = |v: f64| format!("{}", v.round() as i64);
            let path: Vec<String> = shape
                .outline(s, s)
                .into_iter()
                .map(|segment| match segment {
                    PathSegment::Move(x, y) => format!("M {} {}", n(x), n(y)),
                    PathSegment::Line(x, y) => format!("L {} {}", n(x), n(y)),
                    PathSegment::Curve(ax, ay, bx, by, cx, cy) => format!(
                        "C {} {} {} {} {} {}",
                        n(ax),
                        n(ay),
                        n(bx),
                        n(by),
                        n(cx),
                        n(cy)
                    ),
                    PathSegment::Close => "Z".to_owned(),
                })
                .collect();
            format!("outline {}", path.join(" "))
        }
        other => match parametric_shape(other, 1.0, 1.0) {
            odf::DrawKind::CustomShape {
                desk_shape: Some(settings),
                ..
            } if !settings.is_empty() => settings,
            _ => return None,
        },
    })
}

/// The shape of a mask from its `desk:mask-shape` (OBJ-04).
fn mask_shape_of(text: &str) -> Option<Shape> {
    let text = text.trim();
    if text == "ellipse" {
        return Some(Shape::Ellipse);
    }
    if let Some(radius) = text.strip_prefix("rounded ") {
        let radius: f64 = radius.trim().parse().ok()?;
        return radius.is_finite().then(|| Shape::RoundedRectangle {
            radius: Length::from_pt(radius.clamp(0.0, 10_000.0)),
        });
    }
    if let Some(numbers) = text.strip_prefix("points ") {
        let numbers: Vec<i32> = numbers
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        let s = POLYGON_SCALE;
        let points: Vec<(i32, i32)> = numbers
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[x, y]| (x.clamp(-s, 2 * s), y.clamp(-s, 2 * s)))
            .collect();
        return (points.len() >= 3).then(|| Shape::Polygon(points.into()));
    }
    if let Some(path) = text.strip_prefix("outline ") {
        let s = f64::from(POLYGON_SCALE);
        return path_shape(path, Some((0.0, 0.0, s, s)));
    }
    desk_shape_of(text)
}

/// How a picture is masked and adjusted, from the properties of its style.
fn model_picture_settings(
    g: &odf::GraphicProperties,
    picture: &desk_model::Picture,
    frame: Frame,
) -> desk_model::PictureSettings {
    let mut settings = desk_model::PictureSettings::default();
    let lengths = |value: &str| -> Option<Vec<f64>> {
        value
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(|s| odf::Length::parse(s).map(|l| length(l).to_pt()))
            .collect()
    };
    let mask = g.mask.as_deref().and_then(lengths).filter(|v| v.len() == 4);
    if let Some(m) = mask {
        if m[2] > 0.0 && m[3] > 0.0 && m.iter().all(|v| v.abs() < 1e5) {
            settings.mask = Some(Frame::from_pt(m[0], m[1], m[2], m[3]));
            settings.mask_shape = g.mask_shape.as_deref().and_then(mask_shape_of);
        }
    } else if let Some(clip) = g
        .clip
        .as_deref()
        .and_then(|c| c.trim().strip_prefix("rect(")?.strip_suffix(')'))
        .and_then(lengths)
        .filter(|v| v.len() == 4)
    {
        // What is cut from the picture at its own size, top, right,
        // bottom and left; the frame shows the rest.
        let (nw, nh) = natural_size(picture);
        let (top, right, bottom, left) = (clip[0], clip[1], clip[2], clip[3]);
        let (shown_w, shown_h) = (nw - left - right, nh - top - bottom);
        if shown_w > 0.0 && shown_h > 0.0 && clip.iter().any(|v| *v > 0.0) {
            let (sx, sy) = (
                frame.width.to_pt() / shown_w,
                frame.height.to_pt() / shown_h,
            );
            settings.mask = Some(Frame::from_pt(-left * sx, -top * sy, nw * sx, nh * sy));
        }
    }
    if let Some(value) = g.instant_alpha.as_deref() {
        let scale = desk_model::AlphaSeed::SCALE;
        settings.alpha = value
            .split(',')
            .filter_map(|seed| {
                let numbers: Vec<u16> = seed
                    .split_whitespace()
                    .map(|n| n.parse().ok())
                    .collect::<Option<_>>()?;
                let &[x, y, tolerance] = numbers.as_slice() else {
                    return None;
                };
                Some(desk_model::AlphaSeed {
                    x: x.min(scale),
                    y: y.min(scale),
                    tolerance: tolerance.min(100) as u8,
                })
            })
            // Against hostile files.
            .take(256)
            .collect();
    }
    if let Some(value) = g.alpha_kept.as_deref() {
        settings.alpha_kept = alpha_kept(value);
    }
    let unit = |v: f64| v.round().clamp(-100.0, 100.0) as i8;
    let numbers: Option<Vec<f64>> = g.image_adjust.as_deref().and_then(|v| {
        v.split_whitespace()
            .map(|n| n.parse::<f64>().ok().filter(|n| n.is_finite()))
            .collect()
    });
    match numbers.as_deref() {
        Some(
            &[
                brightness,
                contrast,
                saturation,
                exposure,
                temperature,
                tint,
            ],
        ) => {
            settings.adjust = desk_model::Adjustments {
                brightness: unit(brightness),
                contrast: unit(contrast),
                saturation: unit(saturation),
                exposure: unit(exposure),
                temperature: unit(temperature),
                tint: unit(tint),
                ..desk_model::Adjustments::default()
            };
        }
        _ => {
            settings.adjust.brightness = g.luminance.map_or(0, unit);
            settings.adjust.contrast = g.contrast.map_or(0, unit);
        }
    }
    let number = |v: &str| v.parse::<f64>().ok().filter(|n| n.is_finite());
    if let Some(sharpness) = g.image_sharpness.as_deref().and_then(number) {
        settings.adjust.sharpness = unit(sharpness);
    }
    let levels: Option<Vec<f64>> = g
        .image_levels
        .as_deref()
        .and_then(|v| v.split_whitespace().map(number).collect());
    if let Some(&[black, white]) = levels.as_deref() {
        // A black of 255 leaves no room for white: the most is 254, and
        // white is above it.
        let black = black.round().clamp(0.0, 254.0) as u8;
        let white = white.round().clamp(0.0, 255.0) as u8;
        settings.adjust.black = black;
        settings.adjust.white = white.max(black + 1);
    }
    settings
}

/// A shape of Desk that ODF has no element for, as a custom shape that
/// other programs draw by its path, with its settings for Desk (OBJ-11).
fn parametric_shape(shape: &Shape, w: f64, h: f64) -> odf::DrawKind {
    use desk_model::PathSegment;
    // The path in hundredths of a point, which keeps it exact enough.
    let n = |v: f64| format!("{}", (v * 100.0).round() as i64);
    let path: Vec<String> = shape
        .outline(w, h)
        .into_iter()
        .map(|segment| match segment {
            PathSegment::Move(x, y) => format!("M {} {}", n(x), n(y)),
            PathSegment::Line(x, y) => format!("L {} {}", n(x), n(y)),
            PathSegment::Curve(ax, ay, bx, by, cx, cy) => format!(
                "C {} {} {} {} {} {}",
                n(ax),
                n(ay),
                n(bx),
                n(by),
                n(cx),
                n(cy)
            ),
            PathSegment::Close => "Z".to_owned(),
        })
        .chain(["N".to_owned()])
        .collect();
    let settings = match *shape {
        Shape::Star { points, inner } => format!("star {points} {inner}"),
        Shape::RegularPolygon { sides } => format!("polygon {sides}"),
        Shape::Arrow { double, head, tail } => {
            format!(
                "{} {head} {tail}",
                if double { "double-arrow" } else { "arrow" }
            )
        }
        Shape::Balloon { radius, body, tip } => {
            format!("balloon {} {body} {tip}", radius.to_pt())
        }
        Shape::Path { closed, .. } => {
            format!("path {}", if closed { "closed" } else { "open" })
        }
        // docs/odf-extensions.md: desk:shape (W113).
        Shape::Compound(_) => "path compound".to_owned(),
        _ => String::new(),
    };
    odf::DrawKind::CustomShape {
        shape: "non-primitive".into(),
        view_box: Some((0.0, 0.0, (w * 100.0).round(), (h * 100.0).round())),
        path: Some(path.join(" ")),
        desk_shape: Some(settings),
    }
}

/// A shape drawn with the pen from an `draw:enhanced-path` in the units
/// of its view box, or none when the path is not one of plain numbers in
/// `M`, `L`, `C` and `Z`, or has fewer than two points (OBJ-09).
pub(crate) fn path_shape(path: &str, view_box: Option<(f64, f64, f64, f64)>) -> Option<Shape> {
    let tokens: Vec<&str> = path.split_whitespace().collect();
    let mut flat: Vec<(char, Vec<f64>)> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let command = tokens[i].chars().next()?;
        i += 1;
        let count = match command {
            'M' | 'L' => 2,
            'C' => 6,
            'Z' | 'N' | 'F' | 'S' => 0,
            _ => return None,
        };
        // A command repeats while numbers follow it.
        loop {
            let values: Vec<f64> = tokens
                .get(i..i + count)?
                .iter()
                .map(|t| t.parse().ok())
                .collect::<Option<_>>()?;
            i += count;
            flat.push((command, values));
            let more = tokens.get(i).is_some_and(|t| t.parse::<f64>().is_ok());
            if count == 0 || !more {
                break;
            }
        }
    }
    // The view box, or the box of the points.
    let all = || {
        flat.iter()
            .flat_map(|(_, v)| v.as_chunks::<2>().0.iter().copied())
    };
    let (x0, y0, w, h) = view_box.unwrap_or_else(|| {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for [x, y] in all() {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        (x0, y0, x1 - x0, y1 - y0)
    });
    let scale = f64::from(POLYGON_SCALE);
    let at = |x: f64, y: f64| {
        let part = |v: f64, origin: f64, size: f64| {
            if size > 0.0 {
                ((v - origin) / size * scale).round() as i32
            } else {
                0
            }
        };
        (part(x, x0, w), part(y, y0, h))
    };
    // Each part, with whether it is closed.
    let mut parts: Vec<(Vec<desk_model::PathPoint>, bool)> = vec![(Vec::new(), false)];
    for (command, v) in &flat {
        // A part drawn on after it closed starts again where it started.
        if *command != 'M'
            && *command != 'Z'
            && let Some((points, true)) = parts.last()
        {
            let start = points
                .first()
                .map(|p| desk_model::PathPoint::corner(p.at.0, p.at.1));
            parts.push((start.into_iter().collect(), false));
        }
        if parts.len() > desk_model::MAX_SHAPE_PARTS {
            break;
        }
        let (points, closed) = parts.last_mut()?;
        match command {
            'M' if !points.is_empty() => {
                // Another part starts (W113).
                let (x, y) = at(v[0], v[1]);
                parts.push((vec![desk_model::PathPoint::corner(x, y)], false));
            }
            'M' | 'L' => {
                let (x, y) = at(v[0], v[1]);
                points.push(desk_model::PathPoint::corner(x, y));
            }
            'C' => {
                // A handle on its point is no handle: a corner.
                let handle = |p: (i32, i32), of: (i32, i32)| (p != of).then_some(p);
                let last = points.last_mut()?;
                last.after = handle(at(v[0], v[1]), last.at);
                let to = at(v[4], v[5]);
                points.push(desk_model::PathPoint {
                    at: to,
                    before: handle(at(v[2], v[3]), to),
                    after: None,
                });
            }
            'Z' => *closed = true,
            _ => {}
        }
    }
    for (points, closed) in &mut parts {
        // A closed path that ends where it starts ends on its first point.
        if *closed
            && points.len() > 1
            && points.first().map(|p| p.at) == points.last().map(|p| p.at)
        {
            let last = points.pop()?;
            if let Some(first) = points.first_mut() {
                first.before = last.before;
            }
        }
    }
    parts.retain(|(points, _)| points.len() >= 2);
    // Several closed parts are a shape of several parts; otherwise the
    // first part is a shape drawn with the pen.
    if parts.len() > 1 && parts.iter().all(|(p, closed)| *closed && p.len() >= 3) {
        return Some(Shape::Compound(
            parts
                .into_iter()
                .take(desk_model::MAX_SHAPE_PARTS)
                .map(|(p, _)| Arc::from(p))
                .collect(),
        ));
    }
    let (points, closed) = parts.into_iter().next()?;
    Some(Shape::Path {
        points: points.into(),
        closed,
    })
}

/// The shape that `desk:shape` gives, or none when it is not one.
fn desk_shape_of(settings: &str) -> Option<Shape> {
    let mut words = settings.split_whitespace();
    let kind = words.next()?;
    let numbers: Vec<f64> = words.map(str::parse).collect::<Result<_, _>>().ok()?;
    let count = |v: f64| v.clamp(3.0, f64::from(desk_model::MAX_SHAPE_SIDES)) as u8;
    let part = |v: f64| v.clamp(0.0, f64::from(POLYGON_SCALE)) as u16;
    Some(match (kind, &numbers[..]) {
        ("star", &[points, inner]) => Shape::Star {
            points: count(points),
            inner: part(inner),
        },
        ("polygon", &[sides]) => Shape::RegularPolygon {
            sides: count(sides),
        },
        ("arrow" | "double-arrow", &[head, tail]) => Shape::Arrow {
            double: kind == "double-arrow",
            head: part(head),
            tail: part(tail),
        },
        ("balloon", &[radius, body, tip]) => Shape::Balloon {
            radius: Length::from_pt(radius.clamp(0.0, 10_000.0)),
            body: part(body),
            tip: part(tip),
        },
        _ => return None,
    })
}

/// A predefined shape of other programs, as near as Desk has one.
fn custom_shape(shape: &str) -> Shape {
    let s = POLYGON_SCALE;
    let polygon = |points: &[(i32, i32)]| Shape::Polygon(Arc::from(points));
    match shape {
        "ellipse" | "circle" => Shape::Ellipse,
        "round-rectangle" | "round-square" => Shape::RoundedRectangle {
            radius: Length::from_points(9),
        },
        "isosceles-triangle" => Shape::triangle(),
        "star4" | "star5" | "star6" | "star8" | "star12" | "star24" => Shape::Star {
            points: shape[4..].parse().unwrap_or(5),
            inner: if shape == "star5" { 3_820 } else { 5_000 },
        },
        "octagon" => Shape::RegularPolygon { sides: 8 },
        "right-arrow" => Shape::arrow(false),
        "left-right-arrow" => Shape::arrow(true),
        "round-rectangular-callout" => Shape::balloon(true),
        "rectangular-callout" => Shape::balloon(false),
        "right-triangle" => polygon(&[(0, 0), (s, s), (0, s)]),
        "diamond" => polygon(&[(s / 2, 0), (s, s / 2), (s / 2, s), (0, s / 2)]),
        "pentagon" => polygon(&[(s / 2, 0), (s, 3_820), (8_090, s), (1_910, s), (0, 3_820)]),
        "hexagon" => polygon(&[
            (2_500, 0),
            (7_500, 0),
            (s, s / 2),
            (7_500, s),
            (2_500, s),
            (0, s / 2),
        ]),
        _ => Shape::Rectangle,
    }
}

/// The kind of an object, or none when Desk cannot hold it, as a picture
/// that it cannot read.
fn kind(
    source: &odf::TextDocument,
    document: &mut Document,
    object: &odf::DrawObject,
    paragraph: &mut dyn FnMut(&mut Document, &odf::Paragraph) -> Result<Paragraph, FilterError>,
    padding: Option<odf::Length>,
    setup: &PageSetup,
    depth: usize,
) -> Result<Option<ObjectKind>, FilterError> {
    Ok(Some(match &object.kind {
        odf::DrawKind::Rect { corner_radius } => ObjectKind::Shape(match corner_radius {
            Some(r) if r.0 > 0.0 => Shape::RoundedRectangle { radius: length(*r) },
            _ => Shape::Rectangle,
        }),
        odf::DrawKind::Ellipse => ObjectKind::Shape(Shape::Ellipse),
        odf::DrawKind::Line { x1, y1, x2, y2 } => {
            let (left, right) = if x1.0 <= x2.0 {
                ((x1, y1), (x2, y2))
            } else {
                ((x2, y2), (x1, y1))
            };
            ObjectKind::Shape(Shape::Line {
                rising: left.1.0 > right.1.0,
            })
        }
        odf::DrawKind::Connector { .. } => {
            let Some((_, line)) = connector_line(object) else {
                return Ok(None);
            };
            ObjectKind::Shape(Shape::Connection(line))
        }
        odf::DrawKind::Polygon { points, view_box } => {
            let (x, y, w, h) = view_box.unwrap_or_else(|| {
                let xs = points.iter().map(|p| p.0);
                let ys = points.iter().map(|p| p.1);
                let (x0, x1) = (
                    xs.clone().fold(f64::MAX, f64::min),
                    xs.fold(f64::MIN, f64::max),
                );
                let (y0, y1) = (
                    ys.clone().fold(f64::MAX, f64::min),
                    ys.fold(f64::MIN, f64::max),
                );
                (x0, y0, x1 - x0, y1 - y0)
            });
            let scale = f64::from(POLYGON_SCALE);
            let points: Vec<(i32, i32)> = points
                .iter()
                .map(|&(px, py)| {
                    let at = |v: f64, origin: f64, size: f64| {
                        if size > 0.0 {
                            ((v - origin) / size * scale).round() as i32
                        } else {
                            0
                        }
                    };
                    (at(px, x, w), at(py, y, h))
                })
                .collect();
            if points.len() < 3 {
                return Ok(None);
            }
            ObjectKind::Shape(Shape::Polygon(Arc::from(points)))
        }
        odf::DrawKind::CustomShape {
            shape,
            desk_shape,
            view_box,
            path,
        } => {
            let drawn = || path.as_deref().and_then(|p| path_shape(p, *view_box));
            ObjectKind::Shape(match desk_shape.as_deref() {
                Some(s) if s.starts_with("path") => drawn().unwrap_or(Shape::Rectangle),
                Some(s) => desk_shape_of(s).unwrap_or_else(|| custom_shape(shape)),
                // A shape that other programs drew by its path.
                None if shape == "non-primitive" => drawn().unwrap_or(Shape::Rectangle),
                None => custom_shape(shape),
            })
        }
        odf::DrawKind::Image { href, fallback } => {
            let read = |href: &str| {
                let path = href.strip_prefix("./").unwrap_or(href);
                source
                    .pictures
                    .iter()
                    .find(|(p, _)| p.strip_prefix("./").unwrap_or(p) == path)
                    .and_then(|(_, bytes)| crate::picture::picture_within(bytes, MAX_SIDE))
            };
            // The second picture stands for the first when Desk cannot
            // read that one (W108).
            let Some(picture) = read(href).or_else(|| fallback.as_deref().and_then(read)) else {
                return Ok(None);
            };
            ObjectKind::Image(picture)
        }
        odf::DrawKind::Gallery {
            shown,
            each_caption,
            images,
            ..
        } => {
            // docs/odf-extensions.md: desk:gallery (W111).
            let mut built = Vec::with_capacity(images.len());
            // The picture shown among those Desk reads: the first when it
            // cannot read that one.
            let mut kept_shown = 0;
            for (at, image) in images.iter().enumerate() {
                let path = image.href.strip_prefix("./").unwrap_or(&image.href);
                let Some(picture) = source
                    .pictures
                    .iter()
                    .find(|(p, _)| p.strip_prefix("./").unwrap_or(p) == path)
                    .and_then(|(_, bytes)| crate::picture::picture_within(bytes, MAX_SIDE))
                else {
                    continue;
                };
                let mut caption = Vec::with_capacity(image.caption.len());
                for p in &image.caption {
                    caption.push(paragraph(document, p)?);
                }
                if at == *shown {
                    kept_shown = built.len();
                }
                built.push(desk_model::GalleryImage {
                    picture,
                    description: image.description.clone(),
                    caption: Arc::from(caption),
                });
            }
            let gallery = desk_model::Gallery {
                shown: kept_shown,
                images: Arc::from(built),
                each_caption: *each_caption,
            };
            ObjectKind::Gallery(Arc::new(gallery))
        }
        odf::DrawKind::Media {
            href,
            params,
            poster,
            start,
            stop,
            poster_time,
            repeat,
            duration,
            ..
        } => {
            let file = |href: &str| {
                let path = href.strip_prefix("./").unwrap_or(href).to_owned();
                source
                    .pictures
                    .iter()
                    .find(move |(p, _)| p.strip_prefix("./").unwrap_or(p) == path)
                    .map(|(_, bytes)| bytes)
            };
            let Some(mut media) = file(href).and_then(|bytes| Media::from_bytes(bytes.clone()))
            else {
                return Ok(None);
            };
            media.poster = poster
                .as_deref()
                .and_then(file)
                .and_then(|bytes| crate::picture::picture_within(bytes, MAX_SIDE));
            let param = |name: &str| {
                params
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.as_str())
            };
            let millis = |seconds: &Option<String>| {
                seconds
                    .as_deref()
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .map(|s| (s * 1000.0).round().min(f64::from(u32::MAX)) as u32)
            };
            media.start = millis(start).unwrap_or(0);
            media.stop = millis(stop);
            media.poster_time = millis(poster_time).unwrap_or(0);
            media.duration = millis(duration);
            media.repeat = match (repeat.as_deref(), param("Loop")) {
                (Some("loop"), _) => Repeat::Loop,
                (Some("back-and-forth"), _) => Repeat::BackAndForth,
                (Some(_), _) => Repeat::None,
                (None, Some("true")) => Repeat::Loop,
                _ => Repeat::None,
            };
            // Other programs keep the volume in decibels.
            let decibels = param("VolumeDB").and_then(|v| v.trim().parse::<f64>().ok());
            media.volume = match (param("Mute"), decibels) {
                (Some("true"), _) => 0,
                (_, Some(db)) if db.is_finite() => {
                    (10f64.powf(db / 20.0) * 100.0).round().clamp(0.0, 100.0) as u8
                }
                _ => 100,
            };
            ObjectKind::Media(media)
        }
        odf::DrawKind::TextBox {
            paragraphs, thread, ..
        } => {
            let mut built = Vec::with_capacity(paragraphs.len());
            for p in paragraphs {
                built.push(paragraph(document, p)?);
            }
            // The objects in its lines (W106).
            import_text_objects(
                source,
                document,
                paragraphs,
                &mut built,
                paragraph,
                depth + 1,
            )?;
            if built.is_empty() {
                let style = document.styles().default_paragraph();
                built.push(Paragraph::new(document.new_id(), style));
            }
            ObjectKind::TextBox(TextBox {
                thread: *thread,
                ..TextBox::new(Arc::from(built), padding.map_or(Length::ZERO, length))
            })
        }
        odf::DrawKind::Table(table) => {
            let mut built = super::tables::import_table(source, table);
            super::comments::import_cell_comments(document, &mut built, table)?;
            ObjectKind::Table(Arc::new(built))
        }
        odf::DrawKind::Object { href, desk, latex } => {
            // An equation (W104).
            if let Some((_, mathml)) = source.formulas.iter().find(|(h, _)| h == href) {
                return Ok(Some(ObjectKind::Equation(Arc::new(desk_model::Equation {
                    mathml: mathml.as_str().into(),
                    latex: latex.as_deref().map(Arc::from),
                }))));
            }
            let chart = source
                .charts
                .iter()
                .find(|(h, _)| h == href)
                .map(|(_, c)| c);
            match super::tables::import_chart(desk.as_deref(), chart) {
                Some(chart) => ObjectKind::Chart(Arc::new(chart)),
                None => return Ok(None),
            }
        }
        odf::DrawKind::Group(objects) => {
            let mut children = Vec::new();
            if depth < 16 {
                for child in objects {
                    if let Some(object) =
                        import_one(source, document, child, None, setup, paragraph, depth + 1)?
                    {
                        children.push(object);
                    }
                }
            }
            if children.is_empty() {
                return Ok(None);
            }
            ObjectKind::Group(Arc::from(children))
        }
    }))
}

/// The line of a connector, as it joins nothing yet, with its frame, or
/// none for another object.
fn connector_line(object: &odf::DrawObject) -> Option<(Frame, desk_model::Connection)> {
    let odf::DrawKind::Connector {
        x1,
        y1,
        x2,
        y2,
        bend,
        ..
    } = &object.kind
    else {
        return None;
    };
    let bend = bend.map_or(Length::ZERO, length);
    Some(desk_model::Connection::between(
        (x1.0, y1.0),
        (x2.0, y2.0),
        bend,
        None,
        None,
    ))
}

/// The frame and rotation of an object, as it says them: its position
/// and size, or the box of a line, and the rotation and translation of
/// its transform, about its corner.
fn placement(object: &odf::DrawObject) -> (f64, f64, f64, f64, i32) {
    if let Some((frame, _)) = connector_line(object) {
        return (
            frame.x.to_pt(),
            frame.y.to_pt(),
            frame.width.to_pt(),
            frame.height.to_pt(),
            0,
        );
    }
    if let odf::DrawKind::Line { x1, y1, x2, y2 } = &object.kind {
        return (
            x1.0.min(x2.0),
            y1.0.min(y2.0),
            (x2.0 - x1.0).abs(),
            (y2.0 - y1.0).abs(),
            0,
        );
    }
    let w = object.width.map_or(0.0, |l| l.0).max(0.0);
    let h = object.height.map_or(0.0, |l| l.0).max(0.0);
    let (mut x, mut y) = (object.x.map_or(0.0, |l| l.0), object.y.map_or(0.0, |l| l.0));
    let mut rotation = 0;
    if let Some(transform) = &object.transform {
        let (angle, tx, ty) = odf::parse_transform(transform);
        // The center of the frame, turned about its corner and moved.
        let (sin, cos) = angle.sin_cos();
        let cx = w / 2.0 * cos + h / 2.0 * sin + tx.0;
        let cy = -w / 2.0 * sin + h / 2.0 * cos + ty.0;
        x = cx - w / 2.0;
        y = cy - h / 2.0;
        rotation = (angle.to_degrees() * 100.0).round() as i32 % 36_000;
    }
    (x, y, w, h, rotation)
}

/// The object styles of a file of Desk, as graphic styles that name the
/// kind of object they are for (MOD-04). They take the place of those of
/// the document; a style of the library keeps its identifier. A file with
/// none leaves the document with its own.
fn import_object_styles(
    source: &odf::TextDocument,
    document: &mut Document,
) -> Result<(), FilterError> {
    let mut styles = desk_model::ObjectStyles::default();
    let mut defaults = Vec::new();
    let mut text_box = None;
    for style in source
        .styles
        .iter()
        .filter(|s| s.family == odf::Family::Graphic)
    {
        let g = &style.properties.graphic;
        let Some(kind) = g
            .object_style
            .as_deref()
            .and_then(desk_model::ObjectStyleKind::from_name)
        else {
            continue;
        };
        let name = style.shown_name();
        if styles.by_name(kind, name).is_some() {
            continue;
        }
        let id = match document.object_styles().by_name(kind, name) {
            Some(own) if styles.get(own.id).is_none() => own.id,
            _ => document.new_id(),
        };
        styles.insert(desk_model::ObjectStyle {
            id,
            name: name.into(),
            kind,
            graphic: model_look(source, g),
        });
        if g.default_object_style == Some(true) {
            defaults.push(id);
            if kind == desk_model::ObjectStyleKind::TextBox && g.padding.is_some() {
                text_box = Some(text_box_default(source, document, style));
            }
        }
    }
    if styles.is_empty() {
        return Ok(());
    }
    for id in defaults {
        styles.set_default(id);
    }
    if let Some(text_box) = text_box {
        styles.set_text_box_default(text_box);
    }
    document.apply(desk_model::Op::SetObjectStyles { styles })?;
    Ok(())
}

/// What new text boxes take besides their look, from the default text box
/// style of a file of Desk (PXV-10).
fn text_box_default(
    source: &odf::TextDocument,
    document: &Document,
    style: &odf::Style,
) -> desk_model::TextBoxDefault {
    let g = &style.properties.graphic;
    let mut default = desk_model::TextBoxDefault::default();
    if let Some(padding) = g.padding {
        default.inset = length(padding);
    }
    default.vertical = match g.textarea_vertical_align.as_deref() {
        Some("middle") => VerticalAlignment::Middle,
        Some("bottom") => VerticalAlignment::Bottom,
        _ => VerticalAlignment::Top,
    };
    default.wrap = model_wrap(g);
    default.paragraph_style = g.text_box_paragraph_style.as_deref().and_then(|name| {
        let shown = source
            .styles
            .iter()
            .find(|s| s.family == odf::Family::Paragraph && s.name == name)
            .map_or(name, odf::Style::shown_name);
        document
            .styles()
            .by_name(desk_model::StyleFamily::Paragraph, shown)
            .map(desk_model::Style::id)
    });
    let text = &style.properties.text;
    default.paragraph = para_attrs(&style.properties.paragraph, text);
    default.character = char_attrs(source, text, 12.0);
    default
}

/// How the body goes around an object, from its graphic properties.
fn model_wrap(g: &odf::GraphicProperties) -> Wrap {
    match (g.wrap.as_deref(), g.run_through.as_deref()) {
        (Some("none"), _) => Wrap::AboveAndBelow,
        (Some("left"), _) => Wrap::Left,
        (Some("right"), _) => Wrap::Right,
        (Some("biggest"), _) => Wrap::Largest,
        (Some("run-through"), Some("background")) => Wrap::Behind,
        (Some("run-through"), _) => Wrap::Through,
        _ => Wrap::Both,
    }
}

/// The wrap of an object as ODF writes it, and whether the object is in
/// front of or behind the text it runs through.
fn odf_wrap(wrap: Wrap) -> (&'static str, Option<&'static str>) {
    match wrap {
        Wrap::Both => ("parallel", None),
        Wrap::Largest => ("biggest", None),
        Wrap::Left => ("left", None),
        Wrap::Right => ("right", None),
        Wrap::AboveAndBelow => ("none", None),
        Wrap::Through => ("run-through", Some("foreground")),
        Wrap::Behind => ("run-through", Some("background")),
    }
}

/// The object style that an object of a file of Desk took: the parent of
/// its graphic style, by its kind and name.
fn object_style_of(
    source: &odf::TextDocument,
    document: &Document,
    name: Option<&str>,
) -> Option<desk_model::StyleId> {
    let find = |name: &str| {
        source
            .automatic_styles
            .iter()
            .chain(&source.styles)
            .find(|s| s.family == odf::Family::Graphic && s.name == name)
    };
    let own = find(name?)?;
    let style = match own.properties.graphic.object_style {
        Some(_) => own,
        None => find(own.parent.as_deref()?)?,
    };
    let kind =
        desk_model::ObjectStyleKind::from_name(style.properties.graphic.object_style.as_deref()?)?;
    document
        .object_styles()
        .by_name(kind, style.shown_name())
        .map(|s| s.id)
}

/// The picture of a `draw:fill-image` of the package, by its name.
pub(super) fn fill_picture(source: &odf::TextDocument, name: &str) -> Option<desk_model::Picture> {
    let image = source.fill_images.iter().find(|i| i.name == name)?;
    let path = image.href.strip_prefix("./").unwrap_or(&image.href);
    source
        .pictures
        .iter()
        .find(|(p, _)| p.strip_prefix("./").unwrap_or(p) == path)
        .and_then(|(_, bytes)| crate::picture::picture_within(bytes, MAX_SIDE))
}

/// The look of an object or an object style from its graphic
/// properties: its fill, line, reflection, picture frame, shadow and
/// opacity.
fn model_look(source: &odf::TextDocument, g: &odf::GraphicProperties) -> desk_model::GraphicStyle {
    let mut look = desk_model::GraphicStyle::default();
    let style = &mut look;
    let filled = match g.fill.as_deref() {
        Some("none") => false,
        Some(_) => true,
        None => g.fill_color.is_some(),
    };
    let gradient = g
        .fill_gradient_name
        .as_deref()
        .filter(|_| g.fill.as_deref() == Some("gradient"))
        .and_then(|name| source.gradients.iter().find(|d| d.name == name));
    let image = g
        .fill_image_name
        .as_deref()
        .filter(|_| g.fill.as_deref() == Some("bitmap"))
        .and_then(|name| fill_picture(source, name));
    if let Some(picture) = image {
        let mode = match (g.fill_image_mode.as_deref(), g.repeat.as_deref()) {
            (Some("scale-to-fill"), _) => ImageFill::ScaleToFill,
            (Some("scale-to-fit"), _) => ImageFill::ScaleToFit,
            (_, Some("no-repeat")) => ImageFill::OriginalSize,
            (_, Some("stretch")) => ImageFill::Stretch,
            // ODF repeats a picture unless it says otherwise.
            _ => ImageFill::Tile,
        };
        let tint = g.fill_tint.map(|color| {
            let mut tint = rgba(color);
            tint.a = (g.fill_tint_opacity.unwrap_or(100.0).clamp(0.0, 100.0) * 2.55).round() as u8;
            tint
        });
        style.fill = Some(Fill::Image {
            picture,
            mode,
            tint,
        });
    } else if let Some(gradient) = gradient {
        style.fill = Some(model_gradient(gradient));
    } else if filled && let Some(color) = g.fill_color {
        style.fill = Some(Fill::Color(rgba(color)));
    }
    let stroked = match g.stroke.as_deref() {
        Some("none") => false,
        Some(_) => true,
        None => g.stroke_color.is_some() || g.stroke_width.is_some(),
    };
    if stroked {
        style.stroke = Some(Stroke {
            color: g.stroke_color.map_or(desk_model::Rgba::BLACK, rgba),
            // A width of zero is the thinnest line, a hair.
            width: g
                .stroke_width
                .map_or(Length::from_pt(1.0), length)
                .max(Length::from_pt(0.5)),
            style: match (
                g.stroke.as_deref(),
                g.stroke_style
                    .as_deref()
                    .and_then(desk_model::BorderStyle::from_name),
            ) {
                // A dotted line is so whatever the dash (OBJ-35).
                (_, Some(desk_model::BorderStyle::Dotted)) => desk_model::BorderStyle::Dotted,
                (Some("dash"), _) => desk_model::BorderStyle::Dashed,
                // An artistic line is solid to other programs: one that
                // made it dashed or took the line away wins (B-044).
                (Some("solid") | None, Some(style @ desk_model::BorderStyle::Artistic(_))) => style,
                _ => desk_model::BorderStyle::Solid,
            },
            start: line_end(g.marker_start.as_deref()),
            end: line_end(g.marker_end.as_deref()),
        });
    }
    style.reflection = g.reflection.map(|r| r.round().clamp(0.0, 100.0) as u8);
    style.frame = g
        .picture_frame
        .as_deref()
        .and_then(desk_model::FrameStyle::from_name)
        .map(|frame_style| desk_model::PictureFrame {
            style: frame_style,
            scale: g
                .picture_frame_scale
                .map_or(100, |s| s.round().clamp(10.0, 400.0) as u16),
        });
    if g.shadow.as_deref() == Some("visible") {
        style.shadow = Some(model_shadow(g));
    }
    if let Some(opacity) = g.opacity {
        style.opacity = opacity.round().clamp(0.0, 100.0) as u8;
    }
    look
}

/// An object of ODF, anchored to a paragraph of the body when it is in
/// one, or none when Desk cannot hold it. `setup` is the page setup that
/// positions measured from the margins need.
fn import_one(
    source: &odf::TextDocument,
    document: &mut Document,
    object: &odf::DrawObject,
    anchor: Option<NodeId>,
    setup: &PageSetup,
    paragraph: &mut dyn FnMut(&mut Document, &odf::Paragraph) -> Result<Paragraph, FilterError>,
    depth: usize,
) -> Result<Option<Object>, FilterError> {
    // The frame that Desk writes around an object with a title or a
    // caption (W122).
    if let Some((inner, parts)) = captioned_parts(object) {
        // Frames in frames count toward the depth of texts in objects.
        if depth >= NESTED_TEXT_DEPTH {
            return Ok(None);
        }
        let Some(mut built) = import_one(
            source,
            document,
            &inner,
            anchor,
            setup,
            paragraph,
            depth + 1,
        )?
        else {
            return Ok(None);
        };
        let odf::DrawKind::TextBox { paragraphs, .. } = &object.kind else {
            return Ok(Some(built));
        };
        let mut captions = desk_model::Captions::default();
        let mut at = 0;
        for (part, count) in parts {
            let Some((title, count)) = part.zip(count) else {
                at += 1;
                continue;
            };
            let mut read = Vec::with_capacity(count);
            for p in &paragraphs[at..at + count] {
                read.push(paragraph(document, p)?);
            }
            at += count;
            let caption = captions.get_mut(title);
            caption.shown = true;
            caption.paragraphs = Arc::from(read);
        }
        // The object comes before the title when the title is below it.
        captions.title_below = title_after_object(object);
        built.captions = Some(Arc::new(captions));
        return Ok(Some(built));
    }
    let g = graphic(source, object.style.as_deref());
    let Some(mut kind) = kind(source, document, object, paragraph, g.padding, setup, depth)? else {
        return Ok(None);
    };
    if let ObjectKind::TextBox(text) = &mut kind {
        text.vertical = match g.textarea_vertical_align.as_deref() {
            Some("middle") => VerticalAlignment::Middle,
            Some("bottom") => VerticalAlignment::Bottom,
            _ => VerticalAlignment::Top,
        };
        if let Some(count) = g.column_count {
            text.columns = count.clamp(1, u32::from(MAX_TEXT_BOX_COLUMNS)) as u8;
        }
        if let Some(gap) = g.column_gap {
            text.column_gap = length(gap);
        }
    }
    let (mut x, mut y, w, h, rotation) = placement(object);
    let anchor_type = object.anchor.as_deref().unwrap_or("paragraph");
    let model_anchor = match (anchor_type, anchor) {
        ("page", _) | (_, None) => Anchor::Page(object.anchor_page.unwrap_or(1).max(1) - 1),
        (_, Some(paragraph)) => Anchor::Paragraph(paragraph),
    };
    let left = setup.margin_left.to_pt();
    let top = setup.margin_top.to_pt();
    let rel = |r: &Option<String>| r.as_deref().unwrap_or_default().to_owned();
    let (h_rel, v_rel) = (rel(&g.horizontal_rel), rel(&g.vertical_rel));
    match model_anchor {
        Anchor::Page(_) => {
            if h_rel.starts_with("page-content") || h_rel.starts_with("paragraph") {
                x += left;
            }
            if v_rel.starts_with("page-content") {
                y += top;
            }
        }
        Anchor::InText | Anchor::Master(..) => {}
        Anchor::Paragraph(_) => {
            // From the edge of the paper, or from the margin.
            if !h_rel.starts_with("page") || h_rel.starts_with("page-content") {
                x += left;
            }
        }
    }
    if anchor_type == "as-char" {
        // In the line, until Desk has objects in the text.
        x = left;
        y = 0.0;
    }
    let mut frame = Frame::from_pt(x, y, w, h);
    // A table of the text has the size of its cells.
    if let ObjectKind::Table(table) = &kind
        && (w <= 0.0 || h <= 0.0)
    {
        let height: u32 = (0..table.rows()).map(|r| table.height(r)).sum();
        frame = Frame::from_pt(
            x,
            y,
            f64::from(table.total_width()) / 1000.0,
            f64::from(height) / 1000.0,
        );
    }
    // A group is as large as what it holds.
    if let ObjectKind::Group(children) = &kind {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for child in children.iter() {
            let f = child.frame;
            x0 = x0.min(f.x.to_pt());
            y0 = y0.min(f.y.to_pt());
            x1 = x1.max((f.x + f.width).to_pt());
            y1 = y1.max((f.y + f.height).to_pt());
        }
        frame = Frame::from_pt(x0, y0, x1 - x0, y1 - y0);
    }
    // A drawing of Pages 15.4, which other programs see as a group of
    // paths (docs/odf-extensions.md: desk:drawing, W117).
    if let (Some(tools), ObjectKind::Group(children)) = (&object.drawing, &kind)
        && let Some((drawn, drawing)) = drawing_of_group(
            children,
            tools,
            object.animation_duration.as_deref(),
            object.animation_repeat,
        )
    {
        frame = drawn;
        kind = ObjectKind::Drawing(Arc::new(drawing));
    }
    let mut built = Object::new(document.new_id(), kind, frame);
    built.anchor = model_anchor;
    // On a master page, the pages of the section it shows on (EST-19);
    // the master is set where the master page is read.
    let master_pages = match g.master_pages.as_deref() {
        Some("first") => Some(desk_model::MarginPages::First),
        Some("left") => Some(desk_model::MarginPages::Left),
        _ => None,
    };
    if let Some(pages) = master_pages {
        built.anchor = Anchor::Master(None, pages);
    }
    built.rotation = rotation;
    built.style = model_look(source, &g);
    built.object_style = object_style_of(source, document, object.style.as_deref()).filter(|id| {
        document.object_styles().get(*id).map(|s| s.kind)
            == desk_model::ObjectStyleKind::of(&built.kind)
    });
    if let ObjectKind::Image(picture) = &built.kind {
        built.picture = model_picture_settings(&g, picture, frame);
    }
    built.wrap = model_wrap(&g);
    if anchor_type == "as-char" {
        built.wrap = Wrap::AboveAndBelow;
    }
    built.wrap_margin = g.margin.map_or(Length::ZERO, length);
    built.wrap_threshold = g.wrap_threshold.map_or(Object::WRAP_THRESHOLD, |t| {
        t.round().clamp(0.0, 100.0) as u8
    });
    if g.wrap_contour == Some(true)
        && !matches!(
            built.wrap,
            Wrap::AboveAndBelow | Wrap::Through | Wrap::Behind
        )
    {
        // The outline it was given, or that of the shape.
        built.contour = object
            .contour
            .as_ref()
            .map(|c| Arc::from(c.as_slice()))
            .or_else(|| built.shape_contour());
        if object.contour.is_some() {
            built.contour_parts = object
                .contour_parts
                .iter()
                .map(|p| Arc::from(p.as_slice()))
                .collect();
        }
    }
    built.locked = g.protect.as_deref().is_some_and(|p| p.contains("position"));
    built.origin = object
        .origin
        .as_deref()
        .and_then(|o| u64::from_str_radix(o, 16).ok());
    // Where a click on it goes (W102).
    built.link = object
        .link
        .as_deref()
        .filter(|l| !l.is_empty())
        .map(Arc::from);
    // A placeholder of a page template (W095).
    built.placeholder = object.placeholder.as_deref().map(|kind| Placeholder {
        kind: if kind == "media" {
            PlaceholderKind::Media
        } else {
            PlaceholderKind::Text
        },
        tag: object.placeholder_tag.as_deref().map(Arc::from),
    });
    // The comments on the object (W149).
    for mut comment in super::comments::object_comments(&object.annotations) {
        let id = document.new_id();
        comment.id = id;
        document.apply(Op::SetComment {
            id,
            comment: Some(comment),
        })?;
        built.comments = built.comments.with(id);
    }
    let mirror = g.mirror.as_deref().unwrap_or_default();
    built.flip_horizontal = mirror.contains("horizontal");
    built.flip_vertical = mirror.contains("vertical");
    if let odf::DrawKind::Line { x1, y1, x2, y2 } = &object.kind {
        // A line goes from its first point to its second, which keeps
        // its start the start, flipped when it goes left (OBJ-29).
        built.set_line_direction(x2.0 - x1.0, y2.0 - y1.0);
    }
    if built.holds_text() && !object.text.is_empty() && built.text_box().is_none() {
        // The text of a shape, in its middle unless it says otherwise.
        let mut paragraphs = Vec::with_capacity(object.text.len());
        for p in &object.text {
            paragraphs.push(paragraph(document, p)?);
        }
        // The objects in its lines (W106).
        import_text_objects(
            source,
            document,
            &object.text,
            &mut paragraphs,
            paragraph,
            depth + 1,
        )?;
        let mut text = TextBox::in_shape(Arc::from(paragraphs));
        if let Some(padding) = g.padding {
            text.inset = length(padding);
        }
        if let Some(count) = g.column_count {
            text.columns = count.clamp(1, u32::from(MAX_TEXT_BOX_COLUMNS)) as u8;
        }
        if let Some(gap) = g.column_gap {
            text.column_gap = length(gap);
        }
        text.vertical = match g.textarea_vertical_align.as_deref() {
            Some("top") => VerticalAlignment::Top,
            Some("bottom") => VerticalAlignment::Bottom,
            _ => VerticalAlignment::Middle,
        };
        built.text = Some(text);
    }
    Ok(Some(built))
}

/// Adds the objects in the text of the body, each with the identifier
/// that its mark names, and takes out the marks of those that Desk cannot
/// hold (OBJ-01).
/// The object in a frame that Desk wrote around it for its title and
/// caption, standing where the frame stands, and the parts of the text
/// of the frame in order: a title (`Some(true)`) or a caption
/// (`Some(false)`) with its number of paragraphs, and the object (`None`).
/// None for any other frame, or one whose parts do not match its text
/// (W122).
#[allow(clippy::type_complexity)]
fn captioned_parts(
    object: &odf::DrawObject,
) -> Option<(odf::DrawObject, Vec<(Option<bool>, Option<usize>)>)> {
    let (held, parts) = captioned_held(object)?;
    let mut inner = held.clone();
    inner.anchor.clone_from(&object.anchor);
    inner.anchor_page = object.anchor_page;
    inner.z_index = object.z_index;
    if inner.transform.is_none() && (inner.x.is_none() || inner.y.is_none()) {
        inner.x = object.x;
        inner.y = object.y;
    }
    Some((inner, parts))
}

/// The object in a frame that Desk wrote around it for its title and
/// caption, as written there, and the parts of the text of the frame
/// (W122).
#[allow(clippy::type_complexity)]
fn captioned_held(
    object: &odf::DrawObject,
) -> Option<(&odf::DrawObject, Vec<(Option<bool>, Option<usize>)>)> {
    let value = object.captions.as_deref()?;
    let odf::DrawKind::TextBox { paragraphs, .. } = &object.kind else {
        return None;
    };
    let mut parts = Vec::new();
    let mut seen = (false, false, false);
    for token in value.split_whitespace() {
        if token == "title-below" {
            continue;
        }
        if token == "object" {
            if seen.2 {
                return None;
            }
            seen.2 = true;
            parts.push((None, None));
            continue;
        }
        let (name, count) = token.split_once('=')?;
        let count: usize = count.parse().ok()?;
        let title = match name {
            "title" if !seen.0 => {
                seen.0 = true;
                true
            }
            "caption" if !seen.1 => {
                seen.1 = true;
                false
            }
            _ => return None,
        };
        parts.push((Some(title), Some(count)));
    }
    // Counts that do not add up to the paragraphs, however large, make
    // the frame a text box.
    let total = parts
        .iter()
        .filter_map(|(_, c)| *c)
        .try_fold(1usize, usize::checked_add)?;
    if !seen.2 || total != paragraphs.len() {
        return None;
    }
    let before: usize = parts
        .iter()
        .take_while(|(p, _)| p.is_some())
        .filter_map(|(_, c)| *c)
        .sum();
    // A title or a caption holds text only: one with an object or a
    // note makes the frame an ordinary text box.
    let marked = paragraphs.iter().enumerate().any(|(i, p)| {
        i != before
            && p.spans
                .iter()
                .any(|s| matches!(s.field, Some(odf::Field::Object(_) | odf::Field::Note(_))))
    });
    if marked {
        return None;
    }
    let held = &paragraphs[before];
    let mut objects = held.spans.iter().filter_map(|s| match &s.field {
        Some(odf::Field::Object(o)) => Some(&**o),
        _ => None,
    });
    let inner = objects.next()?;
    if objects.next().is_some() {
        return None;
    }
    Some((inner, parts))
}

/// The object in a frame that Desk wrote around it for its title and
/// caption, as written there (W122).
fn captioned_inner(object: &odf::DrawObject) -> Option<&odf::DrawObject> {
    captioned_held(object).map(|(inner, _)| inner)
}

/// Whether the title is below the object, by the parts of a captioned
/// frame (W122).
fn title_after_object(object: &odf::DrawObject) -> bool {
    let value = object.captions.as_deref().unwrap_or_default();
    let object_at = value.split_whitespace().position(|t| t == "object");
    let title_at = value
        .split_whitespace()
        .position(|t| t.starts_with("title="));
    value.split_whitespace().any(|t| t == "title-below")
        || matches!((object_at, title_at), (Some(o), Some(t)) if t > o)
}

fn import_inline_objects(
    source: &odf::TextDocument,
    document: &mut Document,
    setup: &PageSetup,
    paragraph: &mut dyn FnMut(&mut Document, &odf::Paragraph) -> Result<Paragraph, FilterError>,
) -> Result<(), FilterError> {
    for (index, p) in source.body.iter().enumerate() {
        let inline: Vec<&odf::DrawObject> = p
            .spans
            .iter()
            .filter_map(|s| match &s.field {
                Some(odf::Field::Object(o)) => Some(&**o),
                _ => None,
            })
            .collect();
        if inline.is_empty() {
            continue;
        }
        let Some(model) = document.body().get(index).cloned() else {
            continue;
        };
        // The marks of the paragraph, in order, as the spans were read.
        let mut marks = Vec::new();
        let mut offset = 0;
        for run in model.runs() {
            if let Some(Some(Field::ObjectMark { object })) = &run.attrs.field {
                marks.push((offset, *object));
            }
            offset += run.len();
        }
        let mut gone = Vec::new();
        for (object, &(offset, id)) in inline.into_iter().zip(&marks) {
            match import_one(source, document, object, None, setup, paragraph, 0)? {
                Some(mut built) => {
                    built.id = id;
                    built.anchor = Anchor::InText;
                    built.frame.x = Length::ZERO;
                    built.frame.y = Length::ZERO;
                    built.rotation = 0;
                    built.wrap = Wrap::Both;
                    let index = document.objects().len();
                    document.apply(Op::InsertObject {
                        index,
                        object: built,
                    })?;
                }
                None => gone.push(offset),
            }
        }
        for offset in gone.into_iter().rev() {
            document.apply(Op::ReplaceText {
                paragraph: model.id(),
                range: offset..offset + FIELD_CHARACTER.len_utf8(),
                runs: Vec::new(),
            })?;
        }
    }
    Ok(())
}

/// Adds the objects in the lines of a header, a footer or a note, each
/// with the identifier that its mark in `model` names, and takes out of
/// `model` the marks of those that Desk cannot hold (EST-12, EST-13).
/// `odf` are the paragraphs that `model` was read from, in order.
pub(super) fn import_story_inline_objects(
    source: &odf::TextDocument,
    document: &mut Document,
    odf: &[odf::Paragraph],
    model: &mut [Paragraph],
    paragraph: &mut dyn FnMut(&mut Document, &odf::Paragraph) -> Result<Paragraph, FilterError>,
) -> Result<(), FilterError> {
    import_text_objects(source, document, odf, model, paragraph, 0)
}

/// How deep objects in the texts of objects are read, each in the text of
/// the one before (W106): those deeper are left out.
const NESTED_TEXT_DEPTH: usize = 16;

/// Adds the objects in the lines of a text as
/// [`import_story_inline_objects`] does, those of a text inside `depth`
/// objects, which leaves out those too deep (W106).
fn import_text_objects(
    source: &odf::TextDocument,
    document: &mut Document,
    odf: &[odf::Paragraph],
    model: &mut [Paragraph],
    paragraph: &mut dyn FnMut(&mut Document, &odf::Paragraph) -> Result<Paragraph, FilterError>,
    depth: usize,
) -> Result<(), FilterError> {
    let setup = *document.page_setup();
    for (p, built) in odf.iter().zip(model.iter_mut()) {
        let inline: Vec<&odf::DrawObject> = p
            .spans
            .iter()
            .filter_map(|s| match &s.field {
                Some(odf::Field::Object(o)) => Some(&**o),
                _ => None,
            })
            .collect();
        let marks = desk_model::marks_in(std::slice::from_ref(built));
        let mut gone = Vec::new();
        for (i, &(id, at)) in marks.iter().enumerate() {
            let object = match inline.get(i) {
                Some(object) if depth < NESTED_TEXT_DEPTH => {
                    import_one(source, document, object, None, &setup, paragraph, depth)?
                }
                _ => None,
            };
            match object {
                Some(mut object) => {
                    object.id = id;
                    object.anchor = Anchor::InText;
                    object.frame.x = Length::ZERO;
                    object.frame.y = Length::ZERO;
                    object.rotation = 0;
                    object.wrap = Wrap::Both;
                    let index = document.objects().len();
                    document.apply(Op::InsertObject { index, object })?;
                }
                None => gone.push(at.offset),
            }
        }
        if gone.is_empty() {
            continue;
        }
        let mut runs = Vec::new();
        let mut from = 0;
        for offset in gone {
            runs.extend(built.slice(from..offset)?);
            from = offset + FIELD_CHARACTER.len_utf8();
        }
        runs.extend(built.slice(from..built.len())?);
        *built = Paragraph::from_runs(built.id(), built.style(), built.attrs().clone(), runs)?;
    }
    Ok(())
}

/// The name that holds the place of an object in the text while the
/// objects are written.
pub(super) fn inline_placeholder(id: NodeId) -> String {
    format!("desk-inline-{id}")
}

/// Milliseconds as seconds, with up to three decimals.
fn odf_seconds(ms: u32) -> String {
    let text = format!("{:.3}", f64::from(ms) / 1000.0);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The `xml:id` of an object that a connector joins.
fn shape_id(id: NodeId) -> String {
    format!("desk-object-{id}")
}

/// `desk:moved-from` of an object that a tracked change moved, with
/// the change named `c` and its identifier, which the region written for
/// it replaces (see `changes::export_all`): `page` and the page from 1,
/// or `paragraph` and the index of the paragraph in the body from 0, and
/// the frame, in points, as Desk keeps it (W148, REV-01). None when the
/// paragraph it moved with is not in the body.
fn moved_from(document: &Document, moved: &desk_model::Moved) -> Option<String> {
    let anchor = match moved.anchor {
        Anchor::Page(page) => format!("page {}", page + 1),
        Anchor::Paragraph(id) => {
            let index = document.body().iter().position(|p| p.id() == id)?;
            format!("paragraph {index}")
        }
        Anchor::InText | Anchor::Master(..) => return None,
    };
    let f = moved.frame;
    Some(format!(
        "c{} {anchor} {} {} {} {}",
        moved.change.get(),
        odf::Length(f.x.to_pt()),
        odf::Length(f.y.to_pt()),
        odf::Length(f.width.to_pt()),
        odf::Length(f.height.to_pt()),
    ))
}

/// What `desk:moved-from` says: the name of the region of the change,
/// what the object moved with (a page from 0 or the index of a paragraph
/// of the body), and the frame. None when it cannot be read.
pub(super) fn parse_moved_from(value: &str) -> Option<(&str, Result<u32, usize>, Frame)> {
    let mut words = value.split_whitespace();
    let region = words.next()?;
    let anchor = match (words.next()?, words.next()?.parse::<u64>().ok()?) {
        ("page", page) if page > 0 => Ok(u32::try_from(page - 1).ok()?),
        ("paragraph", index) => Err(usize::try_from(index).ok()?),
        _ => return None,
    };
    let mut next = || odf::Length::parse(words.next()?).map(|l| Length::from_pt(l.0));
    let (x, y, width, height) = (next()?, next()?, next()?, next()?);
    if words.next().is_some() || width < Length::ZERO || height < Length::ZERO {
        return None;
    }
    Some((
        region,
        anchor,
        Frame {
            x,
            y,
            width,
            height,
        },
    ))
}

/// The `draw:name` of a text box, which the boxes linked to it name.
fn text_box_name(id: NodeId) -> String {
    format!("Text Box {id}")
}

/// Adds the drawing objects of an ODF document to a document whose body
/// has its paragraphs, in the order of their `draw:z-index`. Gives the
/// objects that say where they were before a tracked change moved them,
/// with what they say (`desk:moved-from`), for once the changes are read
/// (W148).
pub(super) fn import_objects<'a>(
    source: &'a odf::TextDocument,
    document: &mut Document,
    paragraph: &mut dyn FnMut(&mut Document, &odf::Paragraph) -> Result<Paragraph, FilterError>,
) -> Result<Vec<(NodeId, &'a str)>, FilterError> {
    import_object_styles(source, document)?;
    super::tables::import_table_styles(source, document)?;
    let setup = *document.page_setup();
    let ids: Vec<NodeId> = document.body().iter().map(Paragraph::id).collect();
    let mut found: Vec<(&odf::DrawObject, Option<NodeId>)> =
        source.page_objects.iter().map(|o| (o, None)).collect();
    for (index, p) in source.body.iter().enumerate() {
        found.extend(p.frames.iter().map(|o| (o, ids.get(index).copied())));
    }
    import_inline_objects(source, document, &setup, paragraph)?;
    let template_ids = import_page_templates(source, document, &setup, paragraph)?;
    found.sort_by_key(|(o, _)| o.z_index.unwrap_or(u32::MAX));
    // The frames by name, and the text boxes that go on in another.
    let mut names: HashMap<&str, NodeId> = HashMap::new();
    let mut links: Vec<(NodeId, &str)> = Vec::new();
    // The shapes by id, and the connectors that join them (OBJ-31).
    let mut shapes: HashMap<&str, NodeId> = HashMap::new();
    let mut connectors: Vec<(NodeId, Option<&str>, Option<&str>)> = Vec::new();
    let mut moved = Vec::new();
    for (object, anchor) in found {
        if let Some(mut built) = import_one(source, document, object, anchor, &setup, paragraph, 0)?
        {
            // What names an object with a title or a caption is on it, in
            // the frame around it (W122).
            let object = captioned_inner(object).unwrap_or(object);
            // The object of the page template that it copies (W095).
            built.template_object = object
                .template_object
                .as_deref()
                .and_then(|id| template_ids.get(id).copied());
            if let Some(from) = &object.moved_from {
                moved.push((built.id, from.as_str()));
            }
            if let Some(name) = &object.name {
                names.insert(name, built.id);
            }
            if let Some(id) = &object.id {
                shapes.insert(id, built.id);
            }
            if let odf::DrawKind::Connector {
                start_shape,
                end_shape,
                ..
            } = &object.kind
            {
                connectors.push((built.id, start_shape.as_deref(), end_shape.as_deref()));
            }
            if let odf::DrawKind::TextBox {
                chain_next: Some(next),
                ..
            } = &object.kind
            {
                links.push((built.id, next));
            }
            let index = document.objects().len();
            document.apply(Op::InsertObject {
                index,
                object: built,
            })?;
        }
    }
    for (id, next) in links {
        let Some(&next) = names.get(next) else {
            continue;
        };
        let Some(mut object) = document.objects().get(id).cloned() else {
            continue;
        };
        let next_is_box = matches!(
            document.objects().get(next).map(|o| &o.kind),
            Some(ObjectKind::TextBox(_))
        );
        if let ObjectKind::TextBox(text) = &mut object.kind
            && next_is_box
            && next != id
        {
            text.next = Some(next);
            document.apply(Op::ReplaceObject { object })?;
        }
    }
    for (id, start, end) in connectors {
        let Some(mut object) = document.objects().get(id).cloned() else {
            continue;
        };
        let joined = |name: Option<&str>| {
            let other = *shapes.get(name?)?;
            let target = document.objects().get(other)?;
            (other != id && target.anchor == object.anchor && target.anchor != Anchor::InText)
                .then_some(other)
        };
        let (start, end) = (joined(start), joined(end));
        if let ObjectKind::Shape(Shape::Connection(line)) = &mut object.kind
            && (start.is_some() || end.is_some())
        {
            line.start = start;
            line.end = end;
            document.apply(Op::ReplaceObject { object })?;
        }
    }
    Ok(moved)
}

/// Whether an object or an object in it has comments.
fn has_annotations(object: &odf::DrawObject) -> bool {
    !object.annotations.is_empty()
        || matches!(&object.kind, odf::DrawKind::Group(members) if members.iter().any(has_annotations))
}

/// Takes the comments off an object and the objects in it.
fn drop_annotations(object: &mut odf::DrawObject) {
    object.annotations.clear();
    if let odf::DrawKind::Group(members) = &mut object.kind {
        members.iter_mut().for_each(drop_annotations);
    }
}

/// Reads the page templates of a page layout document and the template
/// of each page (W095, odf-extensions.md), giving back the objects of the
/// templates by their `xml:id`, which the objects copied from them name.
fn import_page_templates<'a>(
    source: &'a odf::TextDocument,
    document: &mut Document,
    setup: &PageSetup,
    paragraph: &mut dyn FnMut(&mut Document, &odf::Paragraph) -> Result<Paragraph, FilterError>,
) -> Result<HashMap<&'a str, NodeId>, FilterError> {
    let mut ids = HashMap::new();
    // The page setup takes the count of pages after the objects.
    let Some(count) = source.text_settings.layout_pages else {
        return Ok(ids);
    };
    let mut templates = PageTemplates::default();
    let mut pages: Vec<Option<Arc<str>>> = vec![None; count as usize];
    for template in &source.page_templates {
        let name: Arc<str> = Arc::from(template.name.as_str());
        let mut found: Vec<&odf::DrawObject> = template.objects.iter().collect();
        found.sort_by_key(|o| o.z_index.unwrap_or(u32::MAX));
        let mut objects = Vec::new();
        // The shapes by id, which its connectors join (OBJ-31).
        let mut shapes: HashMap<&str, NodeId> = HashMap::new();
        for object in found {
            // A template has no comments: those of its objects would be
            // on no object of the document, and are left out.
            let without_comments;
            let read = if has_annotations(object) {
                let mut copy = object.clone();
                drop_annotations(&mut copy);
                without_comments = copy;
                &without_comments
            } else {
                object
            };
            if let Some(mut built) = import_one(source, document, read, None, setup, paragraph, 0)?
            {
                built.anchor = Anchor::Page(0);
                built.template_object = None;
                if let ObjectKind::TextBox(text) = &mut built.kind {
                    text.next = None;
                }
                if let Some(id) = &object.id {
                    ids.insert(id.as_str(), built.id);
                    shapes.insert(id.as_str(), built.id);
                }
                objects.push((object, built));
            }
        }
        // The connectors of the template join its shapes.
        let joined: Vec<(NodeId, Option<NodeId>, Option<NodeId>)> = objects
            .iter()
            .filter_map(|(object, built)| {
                let odf::DrawKind::Connector {
                    start_shape,
                    end_shape,
                    ..
                } = &object.kind
                else {
                    return None;
                };
                let end = |name: &Option<String>| {
                    shapes
                        .get(name.as_deref()?)
                        .copied()
                        .filter(|other| *other != built.id)
                };
                Some((built.id, end(start_shape), end(end_shape)))
            })
            .collect();
        let mut objects: Vec<Object> = objects.into_iter().map(|(_, built)| built).collect();
        for (id, start, end) in joined {
            if let Some(object) = objects.iter_mut().find(|o| o.id == id)
                && let ObjectKind::Shape(Shape::Connection(line)) = &mut object.kind
            {
                line.start = start;
                line.end = end;
            }
        }
        for &page in &template.pages {
            if let Some(slot) = (page as usize)
                .checked_sub(1)
                .and_then(|i| pages.get_mut(i))
            {
                *slot = Some(name.clone());
            }
        }
        templates
            .templates
            .push_back(PageTemplate { name, objects });
    }
    if !templates.templates.is_empty() {
        templates.pages = pages.into_iter().collect();
        document.apply(Op::SetPageTemplates { templates })?;
    }
    Ok(ids)
}

/// What export needs to name the styles of the objects and their text.
pub(super) struct Exporter<'a> {
    pub document: &'a Document,
    pub names: &'a HashMap<desk_model::StyleId, String>,
    pub automatic: &'a mut Vec<odf::Style>,
    pub pictures: &'a mut Vec<(String, Vec<u8>)>,
    /// The gradients that the objects fill with.
    pub gradients: Vec<odf::Gradient>,
    /// The markers at the ends of their lines.
    pub markers: Vec<odf::Marker>,
    /// The pictures that the objects fill with.
    pub fill_images: Vec<odf::FillImage>,
    /// The charts, by the folder of each in the package.
    pub charts: Vec<(String, odf::ChartDocument)>,
    /// The MathML of the equations, by the folder of each (W104).
    pub formulas: Vec<(String, String)>,
    /// The gradients and pictures that fill letters.
    pub text_fills: &'a mut super::TextFills,
    /// The annotations of the comments on their text (W149).
    pub comments: &'a super::CommentMarks,
    /// Whether the objects are those of master pages, whose automatic
    /// styles are in styles.xml: their text styles take names of their
    /// own, apart from those of content.xml, as the tracked changes of
    /// both name them from content.xml (W148).
    pub master: bool,
}

impl super::tables::TableStyles for Exporter<'_> {
    fn table_style(&mut self, family: odf::Family, properties: odf::Properties) -> String {
        let prefix = match family {
            odf::Family::Table => "Ta",
            odf::Family::TableColumn => "Co",
            odf::Family::TableRow => "Ro",
            _ => "Ce",
        };
        self.style(prefix, family, None, properties)
    }
}

impl Exporter<'_> {
    /// The name of an automatic style with these properties, made when
    /// there is none yet, with a prefix of its own.
    fn style(
        &mut self,
        prefix: &str,
        family: odf::Family,
        parent: Option<String>,
        properties: odf::Properties,
    ) -> String {
        if let Some(existing) = self.automatic.iter().find(|s| {
            s.family == family
                && s.parent == parent
                && s.properties == properties
                && s.name.starts_with(prefix)
        }) {
            return existing.name.clone();
        }
        let count = self
            .automatic
            .iter()
            .filter(|s| s.name.starts_with(prefix))
            .count();
        let name = format!("{prefix}{}", count + 1);
        self.automatic.push(odf::Style {
            name: name.clone(),
            family,
            parent,
            properties,
            ..odf::Style::default()
        });
        name
    }

    /// The name of a gradient like this one, added when there is none
    /// yet.
    fn gradient(&mut self, gradient: odf::Gradient) -> String {
        if let Some(existing) = self.gradients.iter().find(|g| {
            odf::Gradient {
                name: String::new(),
                ..(*g).clone()
            } == gradient
        }) {
            return existing.name.clone();
        }
        let name = format!("Gradient_{}", self.gradients.len() + 1);
        self.gradients.push(odf::Gradient {
            name: name.clone(),
            ..gradient
        });
        name
    }

    /// The name of the fill image of a picture of the package, added when
    /// there is none yet.
    fn fill_image(&mut self, href: String) -> String {
        if let Some(existing) = self.fill_images.iter().find(|i| i.href == href) {
            return existing.name.clone();
        }
        let name = format!("Image_{}", self.fill_images.len() + 1);
        self.fill_images.push(odf::FillImage {
            name: name.clone(),
            href,
        });
        name
    }

    /// The name of a marker, added when it is not there yet.
    fn marker(&mut self, marker: odf::Marker) -> String {
        if !self.markers.iter().any(|m| m.name == marker.name) {
            self.markers.push(marker.clone());
        }
        marker.name
    }

    /// A paragraph of a text box, with automatic styles for what is set
    /// on it and its text.
    fn paragraph(&mut self, paragraph: &Paragraph, track: &mut super::Track) -> odf::Paragraph {
        let styles = self.document.styles();
        let style_name = self.names[&paragraph.style()].clone();
        let own = ParaAttrs {
            list_level: None,
            list_start: None,
            list_override: None,
            list_style: None,
            ..paragraph.attrs().without_structure()
        };
        let style = if own.is_empty() {
            style_name
        } else {
            let properties = odf::Properties {
                paragraph: odf_paragraph_properties(
                    &own,
                    &styles.resolve_paragraph(paragraph.style(), &own),
                    false,
                ),
                ..Default::default()
            };
            let prefix = if self.master { "MOP" } else { "OP" };
            self.style(prefix, odf::Family::Paragraph, Some(style_name), properties)
        };
        let names = self.names;
        let comments = self.comments;
        let mut spans = export_spans(paragraph, None, comments, Some(track), |run| {
            let attrs = run.attrs.without_field();
            let named = run.style.and_then(|id| names.get(&id).cloned());
            if attrs.is_empty() {
                named
            } else {
                let properties = odf::Properties {
                    text: text_properties(
                        &attrs,
                        &styles.resolve_character(paragraph.style(), run.style, &run.attrs),
                        self.text_fills,
                    ),
                    ..Default::default()
                };
                let prefix = if self.master { "MOT" } else { "OT" };
                Some(self.style(prefix, odf::Family::Text, named, properties))
            }
        });
        super::paragraph_change(
            paragraph,
            track.came_deleted(self.document),
            &mut spans,
            |before| {
                let parent = names
                    .get(&before.style)
                    .cloned()
                    .unwrap_or_else(|| names[&styles.default_paragraph()].clone());
                let own = desk_model::ParaAttrs {
                    list_level: None,
                    list_start: None,
                    list_override: None,
                    list_style: None,
                    ..before.attrs.without_structure()
                };
                if own.is_empty() {
                    return parent;
                }
                let properties = odf::Properties {
                    paragraph: odf_paragraph_properties(
                        &own,
                        &styles.resolve_paragraph(before.style, &own),
                        false,
                    ),
                    ..Default::default()
                };
                let prefix = if self.master { "MOP" } else { "OP" };
                self.style(prefix, odf::Family::Paragraph, Some(parent), properties)
            },
        );
        odf::Paragraph {
            style: Some(style),
            spans,
            ..odf::Paragraph::default()
        }
    }

    /// The path of a movie or a sound in the package, added when it is
    /// not there.
    fn media(&mut self, media: &Media) -> String {
        if let Some((href, _)) = self.pictures.iter().find(|(_, b)| b == media.data()) {
            return href.clone();
        }
        let href = format!(
            "Media/object{}.{}",
            self.pictures.len() + 1,
            media.format().extension()
        );
        self.pictures.push((href.clone(), media.data().to_vec()));
        href
    }

    /// The path of a picture in the package, added when it is not there.
    fn picture(&mut self, picture: &desk_model::Picture) -> String {
        if let Some((href, _)) = self.pictures.iter().find(|(_, b)| b == picture.data()) {
            return href.clone();
        }
        let href = format!(
            "Pictures/object{}.{}",
            self.pictures.len() + 1,
            picture.format().extension()
        );
        self.pictures.push((href.clone(), picture.data().to_vec()));
        href
    }

    /// The graphic properties of a look: its fill, line, opacity,
    /// reflection, picture frame and shadow, as an object or an object
    /// style has it.
    pub fn look(&mut self, style: &desk_model::GraphicStyle) -> odf::GraphicProperties {
        let mut g = odf::GraphicProperties::default();
        match &style.fill {
            Some(Fill::Image {
                picture,
                mode,
                tint,
            }) => {
                if let Some(tint) = *tint {
                    g.fill_tint = Some(odf_color(tint));
                    g.fill_tint_opacity = Some((f64::from(tint.a) / 2.55 * 10.0).round() / 10.0);
                }
                g.fill = Some("bitmap".into());
                let href = self.picture(picture);
                g.fill_image_name = Some(self.fill_image(href));
                let (repeat, extension) = match mode {
                    ImageFill::ScaleToFill => ("stretch", Some("scale-to-fill")),
                    ImageFill::ScaleToFit => ("stretch", Some("scale-to-fit")),
                    ImageFill::Stretch => ("stretch", None),
                    ImageFill::OriginalSize => ("no-repeat", None),
                    ImageFill::Tile => ("repeat", None),
                };
                g.repeat = Some(repeat.into());
                g.fill_image_mode = extension.map(Into::into);
            }
            &Some(Fill::Color(color)) => {
                g.fill = Some("solid".into());
                g.fill_color = Some(odf_color(color));
            }
            &Some(Fill::Gradient { from, to, angle }) => {
                g.fill = Some("gradient".into());
                g.fill_color = Some(odf_color(from));
                g.fill_gradient_name = Some(self.gradient(odf_gradient(from, to, angle)));
            }
            Some(Fill::Advanced(gradient)) => {
                g.fill = Some("gradient".into());
                if let Some(first) = gradient.stops.iter().min_by_key(|s| s.at) {
                    g.fill_color = Some(odf_color(first.color));
                }
                g.fill_gradient_name = Some(self.gradient(odf_advanced_gradient(gradient)));
            }
            _ => g.fill = Some("none".into()),
        }
        match style.stroke {
            Some(stroke) => {
                // Other programs draw an artistic line solid (B-044).
                g.stroke = Some(
                    if matches!(
                        stroke.style,
                        desk_model::BorderStyle::Solid | desk_model::BorderStyle::Artistic(_)
                    ) {
                        "solid"
                    } else {
                        "dash"
                    }
                    .into(),
                );
                g.stroke_color = Some(odf_color(stroke.color));
                g.stroke_width = Some(odf_length(stroke.width));
                g.stroke_style = (stroke.style != desk_model::BorderStyle::Solid)
                    .then(|| stroke.style.name().to_owned());
                // As wide as the ends that Desk draws.
                let size = odf_length(Length::from_pt((stroke.width.to_pt() * 3.0).max(6.0)));
                if let Some(m) = marker(stroke.start) {
                    g.marker_start = Some(self.marker(m));
                    g.marker_start_width = Some(size);
                }
                if let Some(m) = marker(stroke.end) {
                    g.marker_end = Some(self.marker(m));
                    g.marker_end_width = Some(size);
                }
            }
            None => g.stroke = Some("none".into()),
        }
        if style.opacity < 100 {
            g.opacity = Some(f64::from(style.opacity));
        }
        g.reflection = style.reflection.map(f64::from);
        if let Some(frame) = style.frame {
            g.picture_frame = Some(frame.style.name().into());
            g.picture_frame_scale = (frame.scale != 100).then_some(f64::from(frame.scale));
        }
        if let Some(shadow) = style.shadow {
            let (dx, dy) = shadow.offset_xy();
            g.shadow = Some("visible".into());
            g.shadow_offset_x = Some(odf_length(Length::from_pt(dx)));
            g.shadow_offset_y = Some(odf_length(Length::from_pt(dy)));
            g.shadow_color = Some(odf_color(shadow.color));
            g.shadow_opacity = Some(f64::from(shadow.opacity));
            g.shadow_blur = Some(odf_length(shadow.blur));
        }
        g
    }

    /// An object as an ODF drawing object, its frame moved by `dx`, as
    /// the margin of the text that a paragraph's objects are measured
    /// from.
    pub fn object(
        &mut self,
        object: &Object,
        anchor: &str,
        page: Option<u32>,
        dx: f64,
    ) -> odf::DrawObject {
        // A drawing is written as a group of paths, which other programs
        // show, and the tool of each path (W117).
        if let ObjectKind::Drawing(drawing) = &object.kind {
            let group = group_of_drawing(object, drawing);
            let mut out = self.object(&group, anchor, page, dx);
            out.drawing = Some(
                drawing
                    .strokes
                    .iter()
                    .map(|s| s.tool.name())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            if let Some(animation) = drawing.animation {
                out.animation_duration =
                    Some(format!("{}", f64::from(animation.duration) / 1000.0));
                out.animation_repeat = animation.repeat;
            }
            return out;
        }
        let frame = object.frame;
        let (x, y) = (frame.x.to_pt() - dx, frame.y.to_pt());
        let (w, h) = (frame.width.to_pt(), frame.height.to_pt());
        let mut g = self.look(&object.style);
        if let Anchor::Master(_, pages) = object.anchor {
            g.master_pages = match pages {
                desk_model::MarginPages::Default => None,
                desk_model::MarginPages::First => Some("first".into()),
                desk_model::MarginPages::Left => Some("left".into()),
            };
        }
        let (wrap, through) = odf_wrap(object.wrap);
        g.wrap = Some(wrap.into());
        g.run_through = through.map(Into::into);
        g.margin = Some(odf_length(object.wrap_margin));
        if object.wrap_threshold != Object::WRAP_THRESHOLD {
            g.wrap_threshold = Some(f64::from(object.wrap_threshold));
        }
        if object.contour.is_some() {
            g.wrap_contour = Some(true);
        }
        if object.locked {
            g.protect = Some("position size".into());
        }
        if let ObjectKind::Image(picture) = &object.kind {
            odf_picture_settings(&mut g, object, picture);
        }
        // A line is flipped in the order of its points.
        let flips = !matches!(object.kind, ObjectKind::Shape(Shape::Line { .. }));
        let mirror: Vec<&str> = [
            (flips && object.flip_horizontal, "horizontal"),
            (flips && object.flip_vertical, "vertical"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        if !mirror.is_empty() {
            g.mirror = Some(mirror.join(" "));
        }
        let rel = if anchor == "page" {
            "page"
        } else {
            "paragraph"
        };
        if anchor == "as-char" {
            // Standing on the baseline, as a character.
            g.vertical_pos = Some("top".into());
            g.vertical_rel = Some("baseline".into());
        } else {
            g.horizontal_pos = Some("from-left".into());
            g.horizontal_rel = Some(rel.into());
            g.vertical_pos = Some("from-top".into());
            g.vertical_rel = Some(rel.into());
        }
        let kind = match &object.kind {
            ObjectKind::Shape(Shape::Rectangle) => odf::DrawKind::Rect {
                corner_radius: None,
            },
            ObjectKind::Shape(Shape::RoundedRectangle { radius }) => odf::DrawKind::Rect {
                corner_radius: Some(odf_length(*radius)),
            },
            ObjectKind::Shape(Shape::Ellipse) => odf::DrawKind::Ellipse,
            ObjectKind::Shape(Shape::Polygon(points)) => odf::DrawKind::Polygon {
                points: points
                    .iter()
                    .map(|&(px, py)| (f64::from(px), f64::from(py)))
                    .collect(),
                view_box: Some((0.0, 0.0, f64::from(POLYGON_SCALE), f64::from(POLYGON_SCALE))),
            },
            ObjectKind::Shape(Shape::Line { .. }) => {
                // From the start to the end, flipped as it is drawn.
                let ((sx, sy), (ex, ey)) =
                    object.line_ends_in(w, h).unwrap_or(((0.0, 0.0), (w, h)));
                odf::DrawKind::Line {
                    x1: odf::Length(x + sx),
                    y1: odf::Length(y + sy),
                    x2: odf::Length(x + ex),
                    y2: odf::Length(y + ey),
                }
            }
            ObjectKind::Shape(Shape::Connection(line)) => {
                let (a, b) = line.ends_in(w, h);
                let joined = |end: Option<NodeId>| {
                    end.filter(|id| self.document.objects().get(*id).is_some())
                        .map(shape_id)
                };
                let bent = line.bend != Length::ZERO;
                // The way it goes, in hundredths of a point on the page,
                // as ODF wants a path beside its ends.
                let unit = |v: f64| (v * 100.0).round();
                let (start, end) = ((x + a.0, y + a.1), (x + b.0, y + b.1));
                let way = match desk_model::bent_controls(start, end, line.bend.to_pt()) {
                    Some((c1, c2)) => format!(
                        "C {} {} {} {} {} {}",
                        unit(c1.0),
                        unit(c1.1),
                        unit(c2.0),
                        unit(c2.1),
                        unit(end.0),
                        unit(end.1)
                    ),
                    None => format!("L {} {}", unit(end.0), unit(end.1)),
                };
                odf::DrawKind::Connector {
                    view_box: Some((unit(x), unit(y), unit(w).max(1.0), unit(h).max(1.0))),
                    path: Some(format!("M {} {} {way}", unit(start.0), unit(start.1))),
                    x1: odf::Length(x + a.0),
                    y1: odf::Length(y + a.1),
                    x2: odf::Length(x + b.0),
                    y2: odf::Length(y + b.1),
                    start_shape: joined(line.start),
                    end_shape: joined(line.end),
                    line_type: Some(if bent { "curve" } else { "line" }.into()),
                    bend: bent.then(|| odf_length(line.bend)),
                }
            }
            ObjectKind::Shape(
                shape @ (Shape::Star { .. }
                | Shape::RegularPolygon { .. }
                | Shape::Arrow { .. }
                | Shape::Balloon { .. }
                | Shape::Path { .. }
                | Shape::Compound(_)),
            ) => parametric_shape(shape, w, h),
            ObjectKind::Image(picture) => odf::DrawKind::Image {
                href: self.picture(picture),
                // A PNG for the programs that read no SVG or HEIC (W108).
                fallback: crate::picture::png_fallback(picture).map(|png| self.picture(&png)),
            },
            ObjectKind::Gallery(gallery) => {
                let mut track = super::Track::new(self.document);
                let shown = gallery.shown_index();
                // The caption of the picture shown is that of the object
                // when each has its own.
                let shown_caption = object
                    .captions
                    .as_deref()
                    .filter(|_| gallery.each_caption)
                    .map(|c| c.caption.paragraphs.clone());
                let images = gallery
                    .images
                    .iter()
                    .enumerate()
                    .map(|(i, image)| {
                        let caption = match &shown_caption {
                            Some(caption) if i == shown => caption,
                            _ => &image.caption,
                        };
                        odf::GalleryImage {
                            href: self.picture(&image.picture),
                            description: image.description.clone(),
                            caption: if gallery.each_caption {
                                caption
                                    .iter()
                                    .map(|p| self.paragraph(p, &mut track))
                                    .collect()
                            } else {
                                Vec::new()
                            },
                        }
                    })
                    .collect();
                odf::DrawKind::Gallery {
                    // What other programs show, in a format they read.
                    poster: gallery.shown_image().map(|image| {
                        let png = crate::picture::png_fallback(&image.picture);
                        self.picture(png.as_ref().unwrap_or(&image.picture))
                    }),
                    shown,
                    each_caption: gallery.each_caption,
                    images,
                }
            }
            ObjectKind::Media(media) => {
                let seconds = odf_seconds;
                let mut params = vec![(
                    "Loop".to_owned(),
                    (media.repeat != Repeat::None).to_string(),
                )];
                params.push(("Mute".to_owned(), (media.volume == 0).to_string()));
                if media.volume > 0 {
                    let db = 20.0 * (f64::from(media.volume.min(100)) / 100.0).log10();
                    params.push(("VolumeDB".to_owned(), format!("{}", db.round() as i32)));
                }
                odf::DrawKind::Media {
                    href: self.media(media),
                    // As other programs know a movie or a sound to play.
                    mime_type: Some("application/vnd.sun.star.media".into()),
                    params,
                    poster: media.poster.as_ref().map(|p| self.picture(p)),
                    start: (media.start > 0).then(|| seconds(media.start)),
                    stop: media.stop.map(seconds),
                    poster_time: (media.poster_time > 0).then(|| seconds(media.poster_time)),
                    repeat: match media.repeat {
                        Repeat::None => None,
                        Repeat::Loop => Some("loop".into()),
                        Repeat::BackAndForth => Some("back-and-forth".into()),
                    },
                    duration: media.duration.map(seconds),
                }
            }
            ObjectKind::TextBox(text) => {
                g.padding = Some(odf_length(text.inset));
                g.textarea_vertical_align = match text.vertical {
                    VerticalAlignment::Top => None,
                    VerticalAlignment::Middle => Some("middle".into()),
                    VerticalAlignment::Bottom => Some("bottom".into()),
                };
                if text.columns > 1 {
                    g.column_count = Some(u32::from(text.columns));
                    g.column_gap = Some(odf_length(text.column_gap));
                }
                odf::DrawKind::TextBox {
                    paragraphs: {
                        let mut track = super::Track::new(self.document);
                        text.paragraphs
                            .iter()
                            .map(|p| self.paragraph(p, &mut track))
                            .collect()
                    },
                    chain_next: text.next.map(text_box_name),
                    thread: text.thread,
                }
            }
            ObjectKind::Table(table) => {
                let locale = super::tables::locale(self.document);
                let document = self.document;
                odf::DrawKind::Table(super::tables::export_table(table, &locale, self, document))
            }
            ObjectKind::Chart(chart) => {
                // Named by the object, apart from those of master pages.
                let href = format!("./Object {}", object.id);
                self.charts
                    .push((href.clone(), super::tables::export_chart(chart, w, h)));
                odf::DrawKind::Object {
                    href,
                    desk: serde_json::to_string(&**chart).ok(),
                    latex: None,
                }
            }
            // MathML in an object of its own, as LibreOffice Math writes
            // it, named by the object (W104).
            ObjectKind::Equation(equation) => {
                let href = format!("./Equation {}", object.id);
                let mathml = odf::read_formula(&equation.mathml)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| format!("<math xmlns=\"{}\"/>", odf::MATHML));
                self.formulas.push((href.clone(), mathml));
                odf::DrawKind::Object {
                    href,
                    desk: None,
                    latex: equation.latex.as_deref().map(str::to_owned),
                }
            }
            // A draw:g is not turned: its objects are, about its center.
            ObjectKind::Group(objects) => odf::DrawKind::Group(
                objects
                    .iter()
                    .map(|o| self.object(&turned_with_group(o, object), anchor, None, dx))
                    .collect(),
            ),
            _ => odf::DrawKind::Rect {
                corner_radius: None,
            },
        };
        let mut text = Vec::new();
        if let (ObjectKind::Shape(_), Some(inside)) = (&object.kind, object.text_box())
            && !inside.paragraphs.is_empty()
        {
            g.padding = Some(odf_length(inside.inset));
            g.textarea_vertical_align = Some(
                match inside.vertical {
                    VerticalAlignment::Top => "top",
                    VerticalAlignment::Middle => "middle",
                    VerticalAlignment::Bottom => "bottom",
                }
                .into(),
            );
            if inside.columns > 1 {
                g.column_count = Some(u32::from(inside.columns));
                g.column_gap = Some(odf_length(inside.column_gap));
            }
            let mut track = super::Track::new(self.document);
            text = inside
                .paragraphs
                .iter()
                .map(|p| self.paragraph(p, &mut track))
                .collect();
        }
        // The object style it took is the parent of its own, which says
        // all of its look, so that nothing of the parent shows through
        // where the object differs from it.
        let parent = object
            .object_style
            .and_then(|id| self.names.get(&id))
            .cloned();
        if parent.is_some() {
            g.shadow.get_or_insert_with(|| "hidden".into());
            g.opacity.get_or_insert(100.0);
        }
        let style = self.style(
            "gr",
            odf::Family::Graphic,
            parent,
            odf::Properties {
                graphic: g,
                ..Default::default()
            },
        );
        let linked = matches!(&kind, odf::DrawKind::TextBox { .. });
        let mut out = odf::DrawObject::new(kind);
        out.style = Some(style);
        out.text = text;
        out.origin = object.origin.map(|o| format!("{o:016x}"));
        out.link = object.link.as_deref().map(str::to_owned);
        // A placeholder of a page template, and the object of the
        // template that it copies (W095).
        if let Some(placeholder) = &object.placeholder {
            out.placeholder = Some(
                match placeholder.kind {
                    PlaceholderKind::Text => "text",
                    PlaceholderKind::Media => "media",
                }
                .to_owned(),
            );
            out.placeholder_tag = placeholder.tag.as_deref().map(str::to_owned);
        }
        out.template_object = object.template_object.map(shape_id);
        // Where it was before a tracked change moved it (W148).
        out.moved_from = object.moved.and_then(|m| moved_from(self.document, &m));
        // The comments on the object (W149).
        out.annotations = super::comments::object_annotations(self.document, object);
        if !matches!(object.kind, ObjectKind::Shape(_)) {
            out.contour = object.contour.as_deref().map(<[_]>::to_vec);
            out.contour_parts = object.contour_parts.iter().map(|p| p.to_vec()).collect();
        }
        if linked {
            // Named, for the boxes linked to it to name it.
            out.name = Some(text_box_name(object.id));
        }
        if self.document.objects().iter().any(|line| {
            matches!(&line.kind, ObjectKind::Shape(Shape::Connection(c))
                if c.start == Some(object.id) || c.end == Some(object.id))
        }) {
            // With an id, for the connectors that join it to name it.
            out.id = Some(shape_id(object.id));
        }
        out.anchor = page.map(|_| "page").or(Some(anchor)).map(str::to_owned);
        out.anchor_page = page.map(|p| p + 1);
        out.width = Some(odf::Length(w));
        out.height = Some(odf::Length(h));
        if object.rotation == 0 {
            out.x = Some(odf::Length(x));
            out.y = Some(odf::Length(y));
        } else {
            // Turned about the corner and moved so that the center stays.
            let angle = (f64::from(object.rotation) / 100.0).to_radians();
            let (sin, cos) = angle.sin_cos();
            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
            let tx = cx - (w / 2.0 * cos + h / 2.0 * sin);
            let ty = cy - (-w / 2.0 * sin + h / 2.0 * cos);
            out.transform = Some(format!(
                "rotate ({angle}) translate ({} {})",
                odf::Length(tx),
                odf::Length(ty)
            ));
        }
        out
    }

    /// An object with a title or a caption that shows, written as
    /// LibreOffice writes a caption: in a frame of text around it, with
    /// the paragraphs of its title before the object, as a character, and
    /// those of its caption after (W122). The frame stands where the
    /// object does, as it moves, and `desk:captions` says which
    /// paragraphs are which; the object keeps its size and rotation.
    pub(super) fn captioned(
        &mut self,
        object: &Object,
        mut inner: odf::DrawObject,
    ) -> odf::DrawObject {
        let Some(captions) = object.captions.as_deref() else {
            return inner;
        };
        let (above, below) = captions.shown();
        if above.is_none() && below.is_empty() {
            return inner;
        }
        let mut track = super::Track::new(self.document);
        let mut paragraphs: Vec<odf::Paragraph> = Vec::new();
        let mut parts: Vec<String> = Vec::new();
        if let Some(title) = above {
            self.caption_part(title, true, &mut track, &mut paragraphs, &mut parts);
        }
        let anchor = inner.anchor.take();
        let anchor_page = inner.anchor_page.take();
        let z_index = inner.z_index.take();
        // The object keeps its place in its own attributes, which a
        // program that reads it as a character leaves aside; the frame
        // starts higher by about the title, which other programs show
        // above the object.
        let rotated = inner.transform.is_some();
        let frame = |l: desk_model::Length| rotated.then(|| odf::Length(l.to_pt()));
        let x = inner.x.or_else(|| frame(object.frame.x));
        let title_lines = above.map_or(0, |t| t.paragraphs.len()) as f64;
        let y = inner.y.or_else(|| frame(object.frame.y)).map(|y| {
            let above = if title_lines > 0.0 {
                title_lines * 14.0 + 4.0
            } else {
                0.0
            };
            odf::Length(y.0 - above)
        });
        let width = inner.width;
        inner.anchor = Some("as-char".into());
        let style = paragraphs.first().and_then(|p| p.style.clone());
        paragraphs.push(odf::Paragraph {
            style,
            spans: vec![odf::Span {
                field: Some(odf::Field::Object(Box::new(inner))),
                ..odf::Span::default()
            }],
            ..odf::Paragraph::default()
        });
        parts.push("object".into());
        for caption in below {
            let title = std::ptr::eq(caption, &captions.title);
            self.caption_part(caption, title, &mut track, &mut paragraphs, &mut parts);
        }
        // A title below that is turned off keeps its place.
        if captions.title_below && !captions.title.shown {
            parts.push("title-below".into());
        }
        // As high as the object and a line of each paragraph, for the
        // programs that keep the frame at its height.
        let lines = paragraphs.len().saturating_sub(1) as f64;
        let height = object.frame.height.to_pt() + lines * 14.0 + 4.0;
        let mut g = odf::GraphicProperties {
            fill: Some("none".into()),
            stroke: Some("none".into()),
            padding: Some(odf::Length(0.0)),
            ..odf::GraphicProperties::default()
        };
        let (wrap, through) = odf_wrap(object.wrap);
        g.wrap = Some(wrap.into());
        g.run_through = through.map(Into::into);
        g.margin = Some(odf_length(object.wrap_margin));
        if anchor.as_deref() == Some("as-char") {
            g.vertical_pos = Some("top".into());
            g.vertical_rel = Some("baseline".into());
        } else {
            let rel = if anchor.as_deref() == Some("page") {
                "page"
            } else {
                "paragraph"
            };
            g.horizontal_pos = Some("from-left".into());
            g.horizontal_rel = Some(rel.into());
            g.vertical_pos = Some("from-top".into());
            g.vertical_rel = Some(rel.into());
        }
        let style = self.style(
            "gr",
            odf::Family::Graphic,
            None,
            odf::Properties {
                graphic: g,
                ..Default::default()
            },
        );
        let mut outer = odf::DrawObject::new(odf::DrawKind::TextBox {
            paragraphs,
            chain_next: None,
            thread: false,
        });
        outer.style = Some(style);
        outer.anchor = anchor;
        outer.anchor_page = anchor_page;
        outer.z_index = z_index;
        outer.x = x;
        outer.y = y;
        outer.width = width;
        outer.height = Some(odf::Length(height));
        outer.captions = Some(parts.join(" "));
        outer
    }

    /// Writes the paragraphs of a title or a caption, and its part of
    /// `desk:captions`.
    fn caption_part(
        &mut self,
        caption: &desk_model::Caption,
        title: bool,
        track: &mut super::Track,
        paragraphs: &mut Vec<odf::Paragraph>,
        parts: &mut Vec<String>,
    ) {
        let name = if title { "title" } else { "caption" };
        parts.push(format!("{name}={}", caption.paragraphs.len()));
        for p in caption.paragraphs.iter() {
            paragraphs.push(self.paragraph(p, track));
        }
    }
}

/// An object of a group as it shows when the group is turned: turned as
/// much more, with its center turned about the center of the group.
fn turned_with_group(child: &Object, group: &Object) -> Object {
    let mut out = child.clone();
    if group.rotation == 0 {
        return out;
    }
    let g = group.frame;
    let (gx, gy) = (
        g.x.to_pt() + g.width.to_pt() / 2.0,
        g.y.to_pt() + g.height.to_pt() / 2.0,
    );
    let f = child.frame;
    let (w, h) = (f.width.to_pt(), f.height.to_pt());
    let (dx, dy) = (f.x.to_pt() + w / 2.0 - gx, f.y.to_pt() + h / 2.0 - gy);
    let (sin, cos) = (f64::from(group.rotation) / 100.0).to_radians().sin_cos();
    // As the page shows a turned object: counterclockwise, y down.
    let (cx, cy) = (gx + dx * cos + dy * sin, gy - dx * sin + dy * cos);
    out.frame = Frame::from_pt(cx - w / 2.0, cy - h / 2.0, w, h);
    out.rotation = (child.rotation + group.rotation).rem_euclid(36_000);
    out
}

/// A drawing as a group of paths on the page, as other programs show it
/// (W117): an open path for each line, along its points, in its colour and
/// width, and a closed one for each fill, filled with its colour, flipped
/// into their points; the group turns them as the drawing is. The paths have
/// identifiers of their own, which name no object of the document.
fn group_of_drawing(object: &Object, drawing: &desk_model::Drawing) -> Object {
    let f = object.frame;
    let (w, h) = (f.width.to_pt(), f.height.to_pt());
    let (fx, fy) = (f.x.to_pt(), f.y.to_pt());
    let scale = f64::from(POLYGON_SCALE);
    let members: Vec<Object> = drawing
        .strokes
        .iter()
        .enumerate()
        .filter_map(|(i, stroke)| {
            let points: Vec<(f64, f64)> = stroke
                .points_in(w, h)
                .into_iter()
                .map(|(x, y)| {
                    (
                        if object.flip_horizontal { w - x } else { x },
                        if object.flip_vertical { h - y } else { y },
                    )
                })
                .collect();
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for &(x, y) in &points {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
            if points.is_empty() {
                return None;
            }
            let (pw, ph) = ((x1 - x0).max(0.01), (y1 - y0).max(0.01));
            let part =
                |v: f64, origin: f64, size: f64| ((v - origin) / size * scale).round() as i32;
            let mut path: Vec<desk_model::PathPoint> = points
                .iter()
                .map(|&(x, y)| desk_model::PathPoint::corner(part(x, x0, pw), part(y, y0, ph)))
                .collect();
            // A dot is a path of two points in one place.
            if path.len() == 1 {
                path.push(path[0]);
            }
            let fill = stroke.tool == desk_model::DrawingTool::Fill;
            let id = NodeId::new(u64::MAX - i as u64)?;
            let mut member = Object::new(
                id,
                ObjectKind::Shape(Shape::Path {
                    points: path.into(),
                    closed: fill,
                }),
                Frame::from_pt(fx + x0, fy + y0, pw, ph),
            );
            member.anchor = object.anchor;
            member.rotation = 0;
            if fill {
                member.style.fill = Some(Fill::Color(stroke.color));
                member.style.stroke = None;
            } else {
                member.style.fill = None;
                member.style.stroke = Some(Stroke::new(stroke.color, stroke.width));
            }
            // The group turns its paths as it is written.
            Some(member)
        })
        .collect();
    let mut group = object.clone();
    group.kind = ObjectKind::Group(members.into());
    group.flip_horizontal = false;
    group.flip_vertical = false;
    group
}

/// The drawing that a group of paths written by Desk is (W117), from the
/// tools of its paths, with its frame on the page: none when the paths
/// are not those of a drawing, as when another program changed them.
fn drawing_of_group(
    children: &[Object],
    tools: &str,
    duration: Option<&str>,
    repeat: bool,
) -> Option<(Frame, desk_model::Drawing)> {
    let tools: Vec<desk_model::DrawingTool> = tools
        .split_whitespace()
        .map(desk_model::DrawingTool::from_name)
        .collect::<Option<_>>()?;
    if tools.len() != children.len() || tools.len() > desk_model::MAX_DRAWING_STROKES {
        return None;
    }
    let mut strokes = Vec::with_capacity(tools.len());
    for (child, tool) in children.iter().zip(tools) {
        let ObjectKind::Shape(Shape::Path { points, .. }) = &child.kind else {
            return None;
        };
        if points.len() > desk_model::MAX_STROKE_POINTS {
            return None;
        }
        let f = child.frame;
        let (w, h) = (f.width.to_pt(), f.height.to_pt());
        let (cx, cy) = (f.x.to_pt() + w / 2.0, f.y.to_pt() + h / 2.0);
        let (sin, cos) = (f64::from(child.rotation) / 100.0).to_radians().sin_cos();
        let scale = f64::from(POLYGON_SCALE);
        // Each point on the page, turned as the path is.
        let on_page = points
            .iter()
            .map(|p| {
                let (mut dx, mut dy) = (
                    f64::from(p.at.0) / scale * w - w / 2.0,
                    f64::from(p.at.1) / scale * h - h / 2.0,
                );
                if child.flip_horizontal {
                    dx = -dx;
                }
                if child.flip_vertical {
                    dy = -dy;
                }
                (cx + dx * cos + dy * sin, cy - dx * sin + dy * cos)
            })
            .collect();
        let (color, width) = match tool {
            desk_model::DrawingTool::Fill => match child.style.fill {
                Some(Fill::Color(color)) => (color, Length::ZERO),
                _ => return None,
            },
            _ => {
                let stroke = child.style.stroke?;
                (stroke.color, stroke.width)
            }
        };
        strokes.push(desk_model::NewStroke {
            tool,
            color,
            width,
            points: on_page,
        });
    }
    let ((x, y, w, h), strokes) = desk_model::drawing_frame(&strokes)?;
    let animation = duration
        .and_then(|d| d.trim().parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d > 0.0)
        .map(|seconds| desk_model::DrawingAnimation {
            duration: ((seconds * 1000.0).round().min(f64::from(u32::MAX)) as u32).clamp(
                *desk_model::ANIMATION_DURATION.start(),
                *desk_model::ANIMATION_DURATION.end(),
            ),
            repeat,
        });
    Some((
        Frame::from_pt(x, y, w, h),
        desk_model::Drawing {
            strokes: strokes.into(),
            animation,
        },
    ))
}

/// The spans that hold the places of objects in paragraphs and in the
/// notes in them, by the names of the places: more than one where a
/// master page copies the headers of another (W098).
fn collect_placeholders<'a>(
    paragraphs: impl Iterator<Item = &'a mut odf::Paragraph>,
    out: &mut HashMap<String, Vec<&'a mut odf::Span>>,
) {
    for paragraph in paragraphs {
        for span in paragraph.spans.iter_mut() {
            let name = match &span.field {
                Some(odf::Field::Object(o)) => o.name.clone(),
                _ => None,
            };
            if let Some(name) = name {
                out.entry(name).or_default().push(span);
                continue;
            }
            if let Some(odf::Field::Note(note)) = &mut span.field {
                collect_placeholders(note.body.iter_mut(), out);
            }
        }
    }
}

/// Takes out the places of objects that were not written, in paragraphs
/// and in the notes in them.
fn drop_placeholders<'a>(
    paragraphs: impl Iterator<Item = &'a mut odf::Paragraph>,
    stray: &[String],
) {
    for paragraph in paragraphs {
        paragraph.spans.retain(|span| match &span.field {
            Some(odf::Field::Object(o)) => o.name.as_ref().is_none_or(|n| !stray.contains(n)),
            _ => true,
        });
        for span in &mut paragraph.spans {
            if let Some(odf::Field::Note(note)) = &mut span.field {
                drop_placeholders(note.body.iter_mut(), stray);
            }
        }
    }
}

/// The objects in the texts of the objects written (W106), each written
/// as a character once, when a text first marks it.
struct Nested {
    /// The object in the text that each place names, and where it is
    /// among the objects, from the back.
    names: HashMap<String, (NodeId, usize)>,
    /// Those written already, by the names of their places.
    drawn: HashMap<String, odf::DrawObject>,
    /// The names of the objects whose texts are being written, each in
    /// the text of the one before.
    within: Vec<String>,
}

impl Nested {
    fn new(document: &Document) -> Self {
        let names = document
            .objects()
            .iter()
            .enumerate()
            .filter(|(_, o)| o.anchor == Anchor::InText)
            .map(|(z, o)| (inline_placeholder(o.id), (o.id, z)))
            .collect();
        Self {
            names,
            drawn: HashMap::new(),
            within: Vec::new(),
        }
    }

    /// The object in the text that a place names, written as a character
    /// with the objects in its own text; none for one not in the
    /// document, for one whose text is being written, and deeper than
    /// [`NESTED_TEXT_DEPTH`].
    fn object(&mut self, exporter: &mut Exporter<'_>, name: &str) -> Option<odf::DrawObject> {
        if self.within.iter().any(|w| w == name) || self.within.len() >= NESTED_TEXT_DEPTH {
            return None;
        }
        if let Some(drawn) = self.drawn.get(name) {
            return Some(drawn.clone());
        }
        let &(id, z) = self.names.get(name)?;
        let document = exporter.document;
        let object = document.objects().get(id)?;
        let mut drawn = exporter.object(object, "as-char", None, 0.0);
        drawn.x = None;
        drawn.y = None;
        drawn.z_index = Some(z as u32);
        self.within.push(name.to_owned());
        self.fill(exporter, &mut drawn);
        self.within.pop();
        let drawn = exporter.captioned(object, drawn);
        self.drawn.insert(name.to_owned(), drawn.clone());
        Some(drawn)
    }

    /// Writes in the places of the marks in the text of a drawn object,
    /// and in those of the objects of a group, the objects they hold, and
    /// takes out the places of the others.
    fn fill(&mut self, exporter: &mut Exporter<'_>, drawn: &mut odf::DrawObject) {
        self.fill_paragraphs(exporter, &mut drawn.text);
        match &mut drawn.kind {
            odf::DrawKind::TextBox { paragraphs, .. } => self.fill_paragraphs(exporter, paragraphs),
            odf::DrawKind::Group(members) => {
                for member in members {
                    self.fill(exporter, member);
                }
            }
            _ => {}
        }
    }

    fn fill_paragraphs(&mut self, exporter: &mut Exporter<'_>, paragraphs: &mut [odf::Paragraph]) {
        for paragraph in paragraphs {
            paragraph.spans.retain_mut(|span| {
                let Some(odf::Field::Object(placeholder)) = &span.field else {
                    return true;
                };
                let Some(name) = placeholder.name.clone() else {
                    return true;
                };
                match self.object(exporter, &name) {
                    Some(drawn) => {
                        span.field = Some(odf::Field::Object(Box::new(drawn)));
                        true
                    }
                    None => false,
                }
            });
        }
    }
}

/// The paragraphs of the headers and footers of a master page.
fn master_paragraphs(page: &mut odf::MasterPage) -> impl Iterator<Item = &mut odf::Paragraph> {
    [
        &mut page.header,
        &mut page.header_first,
        &mut page.header_left,
        &mut page.footer,
        &mut page.footer_first,
        &mut page.footer_left,
    ]
    .into_iter()
    .flatten()
    // Its three fields (W096).
    .flat_map(|part| {
        part.paragraphs
            .iter_mut()
            .chain(part.centre.iter_mut())
            .chain(part.right.iter_mut())
    })
}

/// Writes the objects of a document: those fixed on pages before the
/// first paragraph, and those that move with a paragraph in it.
pub(super) fn export_objects(exporter: &mut Exporter<'_>, out: &mut odf::TextDocument) {
    let document = exporter.document;
    super::tables::export_table_styles(document, out);
    // The object styles, among the named styles, with the kind of object
    // each is for and the default of each kind (MOD-04, MOD-07).
    let object_styles = document.object_styles();
    for style in object_styles.iter() {
        let name = exporter.names[&style.id].clone();
        let mut graphic = exporter.look(&style.graphic);
        graphic.object_style = Some(style.kind.name().to_owned());
        graphic.default_object_style = object_styles.is_default(style.id).then_some(true);
        let mut properties = odf::Properties::default();
        // What new text boxes take besides their look, on the default text
        // box style, when the document set it (PXV-10).
        let text_box = object_styles.text_box_default();
        if style.kind == desk_model::ObjectStyleKind::TextBox
            && object_styles.is_default(style.id)
            && *text_box != desk_model::TextBoxDefault::default()
        {
            graphic.padding = Some(odf_length(text_box.inset));
            graphic.textarea_vertical_align = match text_box.vertical {
                VerticalAlignment::Top => Some("top".into()),
                VerticalAlignment::Middle => Some("middle".into()),
                VerticalAlignment::Bottom => Some("bottom".into()),
            };
            let (wrap, through) = odf_wrap(text_box.wrap);
            graphic.wrap = Some(wrap.into());
            graphic.run_through = through.map(Into::into);
            graphic.text_box_paragraph_style = text_box
                .paragraph_style
                .and_then(|id| exporter.names.get(&id).cloned());
            properties.paragraph = paragraph_properties(&text_box.paragraph);
            properties.text = text_properties(
                &text_box.character,
                &desk_model::CharFormat::default(),
                exporter.text_fills,
            );
        }
        properties.graphic = graphic;
        out.styles.push(odf::Style {
            display_name: (name != *style.name).then(|| style.name.to_string()),
            name,
            family: odf::Family::Graphic,
            properties,
            ..odf::Style::default()
        });
    }
    let indexes: HashMap<NodeId, usize> = document
        .body()
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id(), i))
        .collect();
    let left = document.page_setup().margin_left.to_pt();
    // Where the marks of the objects in the text were written, in the
    // body and its notes.
    let mut placeholders: HashMap<String, Vec<&mut odf::Span>> = HashMap::new();
    collect_placeholders(out.body.iter_mut(), &mut placeholders);
    let mut nested = Nested::new(document);
    let mut floating = Vec::new();
    for (z, object) in document.objects().iter().enumerate() {
        if object.anchor == Anchor::InText {
            // In the line of the text, where its mark is (OBJ-01).
            if let Some(spans) = placeholders.remove(&inline_placeholder(object.id))
                && let Some(drawn) = nested.object(exporter, &inline_placeholder(object.id))
            {
                for span in spans {
                    span.field = Some(odf::Field::Object(Box::new(drawn.clone())));
                }
            }
            continue;
        }
        if object.anchor.is_master() {
            // In the master page of its section, with the headers.
            continue;
        }
        floating.push((z, object));
    }
    // A mark without its object is left out.
    let stray: Vec<String> = placeholders.into_keys().collect();
    drop_placeholders(out.body.iter_mut(), &stray);
    for (z, object) in floating {
        let at = match object.anchor {
            Anchor::Paragraph(id) => indexes.get(&id).copied(),
            Anchor::Page(_) | Anchor::InText | Anchor::Master(..) => None,
        };
        let mut drawn = match (object.anchor, at) {
            (Anchor::Paragraph(_), Some(_)) => exporter.object(object, "paragraph", None, left),
            (Anchor::Page(page), _) => exporter.object(object, "page", Some(page), 0.0),
            (Anchor::Paragraph(_) | Anchor::InText, None) => {
                exporter.object(object, "page", Some(0), 0.0)
            }
            (Anchor::InText | Anchor::Master(..), _) => continue,
        };
        drawn.z_index = Some(z as u32);
        nested.fill(exporter, &mut drawn);
        let drawn = exporter.captioned(object, drawn);
        match at.and_then(|i| out.body.get_mut(i)) {
            Some(paragraph) => paragraph.frames.push(drawn),
            None => out.page_objects.push(drawn),
        }
    }
    export_page_templates(exporter, out);
    out.gradients = std::mem::take(&mut exporter.gradients);
    out.markers = std::mem::take(&mut exporter.markers);
    out.fill_images = std::mem::take(&mut exporter.fill_images);
    out.charts.append(&mut exporter.charts);
    out.formulas.append(&mut exporter.formulas);
}

/// The page templates of a page layout document, with the pages from 1
/// that come from each (W095, odf-extensions.md). The objects of a
/// template are on page 1, each with the `xml:id` that the objects
/// copied from it name.
fn export_page_templates(exporter: &mut Exporter<'_>, out: &mut odf::TextDocument) {
    let document = exporter.document;
    if document.page_setup().layout_pages.is_none() {
        return;
    }
    let templates = document.page_templates();
    for template in templates.templates.iter() {
        let pages = templates
            .pages
            .iter()
            .enumerate()
            .filter(|(_, n)| n.as_deref() == Some(&*template.name))
            .map(|(i, _)| i as u32 + 1)
            .collect();
        let objects = template
            .objects
            .iter()
            .enumerate()
            .map(|(z, object)| {
                let mut drawn = exporter.object(object, "page", Some(0), 0.0);
                drawn.z_index = Some(z as u32);
                drawn.id = Some(shape_id(object.id));
                // A connector joins shapes of the template, which are not
                // among the objects of the document.
                if let (
                    ObjectKind::Shape(Shape::Connection(line)),
                    odf::DrawKind::Connector {
                        start_shape,
                        end_shape,
                        ..
                    },
                ) = (&object.kind, &mut drawn.kind)
                {
                    let joined = |end: Option<NodeId>| {
                        end.filter(|id| template.objects.iter().any(|o| o.id == *id))
                            .map(shape_id)
                    };
                    *start_shape = joined(line.start);
                    *end_shape = joined(line.end);
                }
                drawn
            })
            .collect();
        out.page_templates.push(odf::PageTemplate {
            name: template.name.to_string(),
            pages,
            objects,
        });
    }
}

/// Writes the objects of the masters of the sections in their master
/// pages, with their styles among the automatic styles of `styles.xml`
/// (INT-05). `masters` names the master page of each section that has
/// one; the objects of another go on the first.
pub(super) fn export_master_objects(
    document: &Document,
    names: &HashMap<desk_model::StyleId, String>,
    masters: &HashMap<NodeId, String>,
    pictures: &mut Vec<(String, Vec<u8>)>,
    text_fills: &mut super::TextFills,
    comments: &super::CommentMarks,
    out: &mut odf::TextDocument,
) {
    let mut automatic = std::mem::take(&mut out.master_styles);
    let mut exporter = Exporter {
        document,
        names,
        automatic: &mut automatic,
        pictures,
        gradients: std::mem::take(&mut out.gradients),
        markers: std::mem::take(&mut out.markers),
        fill_images: std::mem::take(&mut out.fill_images),
        charts: Vec::new(),
        formulas: Vec::new(),
        text_fills,
        comments,
        master: true,
    };
    // The objects in the lines of the headers and footers (EST-12), with
    // the styles of the master pages.
    let mut nested = Nested::new(document);
    {
        let mut placeholders: HashMap<String, Vec<&mut odf::Span>> = HashMap::new();
        collect_placeholders(master_paragraphs(&mut out.master), &mut placeholders);
        for (_, page) in &mut out.masters {
            collect_placeholders(master_paragraphs(page), &mut placeholders);
        }
        for object in document.objects().iter() {
            if object.anchor != Anchor::InText {
                continue;
            }
            if let Some(spans) = placeholders.remove(&inline_placeholder(object.id))
                && let Some(drawn) = nested.object(&mut exporter, &inline_placeholder(object.id))
            {
                for span in spans {
                    span.field = Some(odf::Field::Object(Box::new(drawn.clone())));
                }
            }
        }
        let stray: Vec<String> = placeholders.into_keys().collect();
        drop_placeholders(master_paragraphs(&mut out.master), &stray);
        for (_, page) in &mut out.masters {
            drop_placeholders(master_paragraphs(page), &stray);
        }
    }
    // The master pages of sections with only a background of their own
    // copy the master objects of the section whose headers they show
    // (W098).
    let copies = super::background_masters(document);
    for (z, object) in document.objects().iter().enumerate() {
        let Anchor::Master(section, _) = object.anchor else {
            continue;
        };
        let mut drawn = exporter.object(object, "page", None, 0.0);
        drawn.z_index = Some(z as u32);
        nested.fill(&mut exporter, &mut drawn);
        for (copy, _) in copies.iter().filter(|(_, of)| **of == section) {
            if let Some(name) = masters.get(copy)
                && let Some((_, page)) = out.masters.iter_mut().find(|(n, _)| n == name)
            {
                page.objects.push(drawn.clone());
            }
        }
        let name = section.and_then(|id| masters.get(&id));
        let page = match name {
            Some(name) => out
                .masters
                .iter_mut()
                .find(|(n, _)| n == name)
                .map(|(_, page)| page),
            None => None,
        };
        page.unwrap_or(&mut out.master).objects.push(drawn);
    }
    out.gradients = std::mem::take(&mut exporter.gradients);
    out.markers = std::mem::take(&mut exporter.markers);
    out.fill_images = std::mem::take(&mut exporter.fill_images);
    out.charts.append(&mut exporter.charts);
    out.formulas.append(&mut exporter.formulas);
    out.master_styles = automatic;
}

/// The drawing objects of a master page as objects of the master of
/// `section` (INT-05), read with the styles of `styles.xml`; and those
/// that move with the paragraphs of its headers and footers, which Desk
/// puts on the master too, below the top of the header or the footer by
/// `offset`, as `framed` gives them.
pub(super) fn import_master_objects(
    source: &odf::TextDocument,
    master: &odf::MasterPage,
    section: Option<NodeId>,
    framed: &[(Vec<odf::DrawObject>, f64)],
    document: &mut Document,
    paragraph: &mut dyn FnMut(&mut Document, &odf::Paragraph) -> Result<Paragraph, FilterError>,
) -> Result<(), FilterError> {
    let mut parts: Vec<(odf::TextDocument, f64)> = Vec::new();
    let on_master =
        |page_objects: Vec<odf::DrawObject>, frames: Vec<odf::DrawObject>| odf::TextDocument {
            default_style: source.default_style.clone(),
            styles: source.styles.clone(),
            automatic_styles: source.master_styles.clone(),
            page_layout: source.page_layout.clone(),
            pictures: source.pictures.clone(),
            body: if frames.is_empty() {
                Vec::new()
            } else {
                vec![odf::Paragraph {
                    frames,
                    ..odf::Paragraph::default()
                }]
            },
            page_objects,
            gradients: source.gradients.clone(),
            markers: source.markers.clone(),
            fill_images: source.fill_images.clone(),
            // The charts and equations that the objects show.
            charts: source.charts.clone(),
            formulas: source.formulas.clone(),
            ..odf::TextDocument::default()
        };
    if !master.objects.is_empty() {
        let objects = master
            .objects
            .iter()
            .map(|o| odf::DrawObject {
                anchor: Some("page".into()),
                anchor_page: None,
                ..o.clone()
            })
            .collect();
        parts.push((on_master(objects, Vec::new()), 0.0));
    }
    for (frames, offset) in framed {
        let frames: Vec<odf::DrawObject> = frames
            .iter()
            // Those in the line of the text are not read yet.
            .filter(|o| o.anchor.as_deref() != Some("as-char"))
            .map(|o| odf::DrawObject {
                anchor: Some("paragraph".into()),
                ..o.clone()
            })
            .collect();
        if !frames.is_empty() {
            parts.push((on_master(Vec::new(), frames), *offset));
        }
    }
    for (part, offset) in parts {
        let before = document.objects().len();
        import_objects(&part, document, paragraph)?;
        let added: Vec<Object> = document.objects().iter().skip(before).cloned().collect();
        for mut object in added {
            let pages = match object.anchor {
                Anchor::Master(_, pages) => pages,
                _ => desk_model::MarginPages::Default,
            };
            object.anchor = Anchor::Master(section, pages);
            object.frame.y = object.frame.y + Length::from_pt(offset);
            document.apply(Op::ReplaceObject { object })?;
        }
    }
    Ok(())
}

/// The polygons of `desk:alpha-kept`: each of `x y` pairs, separated by
/// semicolons; none when there is no polygon of three points or more.
fn alpha_kept(value: &str) -> Option<desk_model::AlphaOutline> {
    let scale = POLYGON_SCALE;
    let polygons: Vec<Arc<[(i32, i32)]>> = value
        .split(';')
        // Against hostile files.
        .take(1024)
        .filter_map(|polygon| {
            let numbers: Vec<i32> = polygon
                .split_whitespace()
                .map(|n| n.parse().ok())
                .collect::<Option<_>>()?;
            let points: Vec<(i32, i32)> = numbers
                .as_chunks::<2>()
                .0
                .iter()
                .take(65_536)
                .map(|&[x, y]| (x.clamp(-scale, 2 * scale), y.clamp(-scale, 2 * scale)))
                .collect();
            (points.len() >= 3).then(|| points.into())
        })
        .collect();
    (!polygons.is_empty()).then(|| polygons.into())
}

#[cfg(test)]
mod tests {

    #[test]
    fn w113_the_parts_of_a_path_are_read_each_from_its_start() {
        let ring = path_shape(
            "M 0 0 L 100 0 L 100 100 L 0 100 Z M 25 25 L 25 75 L 75 75 L 75 25 Z",
            Some((0.0, 0.0, 100.0, 100.0)),
        );
        assert!(matches!(ring, Some(Shape::Compound(parts)) if parts.len() == 2));
        // Drawn on after it closes, a part starts again where it started.
        let Some(Shape::Compound(parts)) = path_shape(
            "M 0 0 L 100 0 L 100 100 Z L 50 0 L 0 50 Z",
            Some((0.0, 0.0, 100.0, 100.0)),
        ) else {
            panic!("two parts");
        };
        assert_eq!(parts[1][0].at, (0, 0));
        assert_eq!(parts[1].len(), 3);
    }

    use super::*;

    #[test]
    fn w110_the_sharpness_and_the_levels_are_read_within_their_bounds() {
        let picture = crate::picture::picture_within(&png(8, 8), MAX_SIDE).unwrap();
        let frame = Frame::from_pt(0.0, 0.0, 6.0, 6.0);
        let read = |sharpness: &str, levels: &str| {
            let g = odf::GraphicProperties {
                image_adjust: Some("0 0 -40 0 0 0".into()),
                image_sharpness: Some(sharpness.into()),
                image_levels: Some(levels.into()),
                ..odf::GraphicProperties::default()
            };
            model_picture_settings(&g, &picture, frame).adjust
        };
        let a = read("30", "12 240");
        assert_eq!(
            (a.saturation, a.sharpness, a.black, a.white),
            (-40, 30, 12, 240)
        );
        // A black of 255 keeps room for white; white stays above black.
        let a = read("500", "255 255");
        assert_eq!((a.sharpness, a.black, a.white), (100, 254, 255));
        let a = read("x", "200 100");
        assert_eq!((a.sharpness, a.black, a.white), (0, 200, 201));
        // Written back the same way.
        let mut g = odf::GraphicProperties::default();
        let mut object = Object::new(
            desk_model::NodeId::new(1).unwrap(),
            ObjectKind::Image(picture.clone()),
            frame,
        );
        object.picture.adjust = desk_model::Adjustments {
            sharpness: -20,
            black: 5,
            white: 250,
            ..desk_model::Adjustments::default()
        };
        odf_picture_settings(&mut g, &object, &picture);
        assert_eq!(g.image_adjust.as_deref(), Some("0 0 0 0 0 0"));
        assert_eq!(g.image_sharpness.as_deref(), Some("-20"));
        assert_eq!(g.image_levels.as_deref(), Some("5 250"));
    }

    #[test]
    fn obj_04_obj_07_masks_and_adjustments_of_other_programs_are_read() {
        // Eight pixels square: six points at 96 pixels an inch.
        let picture = crate::picture::picture_within(&png(8, 8), MAX_SIDE).unwrap();
        let g = odf::GraphicProperties {
            clip: Some("rect(1.5pt, 0pt, 1.5pt, 3pt)".into()),
            luminance: Some(20.0),
            contrast: Some(-10.0),
            ..odf::GraphicProperties::default()
        };
        // The frame shows three points by three of it, at twice its size.
        let settings = model_picture_settings(&g, &picture, Frame::from_pt(0.0, 0.0, 6.0, 6.0));
        assert_eq!(settings.mask, Some(Frame::from_pt(-6.0, -3.0, 12.0, 12.0)));
        assert_eq!(settings.adjust.brightness, 20);
        assert_eq!(settings.adjust.contrast, -10);
        assert!(settings.alpha.is_empty());
    }

    /// A PNG of a size, in one color.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgb);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![200; (width * height * 3) as usize])
            .unwrap();
        drop(writer);
        bytes
    }
}
