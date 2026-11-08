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
    Media, NodeId, Object, ObjectKind, Op, POLYGON_SCALE, PageSetup, ParaAttrs, Paragraph, Repeat,
    Shape, Stroke, TextBox, VerticalAlignment, Wrap,
};
use desk_odf as odf;

use super::{
    FilterError, export_spans, length, odf_color, odf_length, odf_paragraph_properties, rgba,
    text_properties,
};

/// The largest side of a picture of an object, in pixels.
const MAX_SIDE: u32 = 4096;

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
            instant_alpha,
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
fn model_gradient(gradient: &odf::Gradient) -> Fill {
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
fn odf_gradient(from: desk_model::Rgba, to: desk_model::Rgba, angle: i32) -> odf::Gradient {
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
fn odf_advanced_gradient(gradient: &desk_model::Gradient) -> odf::Gradient {
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
        Shape::Path { .. } => {
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
    let unit = |v: f64| v.round().clamp(-100.0, 100.0) as i8;
    let numbers: Option<Vec<i8>> = g.image_adjust.as_deref().and_then(|v| {
        v.split_whitespace()
            .map(|n| n.parse::<f64>().ok().filter(|n| n.is_finite()).map(unit))
            .collect()
    });
    match numbers {
        Some(n) if n.len() == 6 => {
            settings.adjust = desk_model::Adjustments {
                brightness: n[0],
                contrast: n[1],
                saturation: n[2],
                exposure: n[3],
                temperature: n[4],
                tint: n[5],
            };
        }
        _ => {
            settings.adjust.brightness = g.luminance.map_or(0, unit);
            settings.adjust.contrast = g.contrast.map_or(0, unit);
        }
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
    let mut points: Vec<desk_model::PathPoint> = Vec::new();
    let mut closed = false;
    for (command, v) in &flat {
        match command {
            'M' | 'L' if command == &'L' || points.is_empty() => {
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
            'Z' => {
                closed = true;
                break;
            }
            // Only the first part of a path that has several.
            'M' => break,
            _ => {}
        }
    }
    // A closed path that ends where it starts ends on its first point.
    if closed && points.len() > 1 && points[0].at == points[points.len() - 1].at {
        let last = points.pop()?;
        points[0].before = last.before;
    }
    (points.len() >= 2).then(|| Shape::Path {
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
        odf::DrawKind::Image { href } => {
            let path = href.strip_prefix("./").unwrap_or(href);
            let Some(picture) = source
                .pictures
                .iter()
                .find(|(p, _)| p.strip_prefix("./").unwrap_or(p) == path)
                .and_then(|(_, bytes)| crate::picture::picture_within(bytes, MAX_SIDE))
            else {
                return Ok(None);
            };
            ObjectKind::Image(picture)
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
        odf::DrawKind::TextBox { paragraphs, .. } => {
            let mut built = Vec::with_capacity(paragraphs.len());
            for p in paragraphs {
                built.push(paragraph(document, p)?);
            }
            if built.is_empty() {
                let style = document.styles().default_paragraph();
                built.push(Paragraph::new(document.new_id(), style));
            }
            ObjectKind::TextBox(TextBox::new(
                Arc::from(built),
                padding.map_or(Length::ZERO, length),
            ))
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
    let style = &mut built.style;
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
        .and_then(|name| source.fill_images.iter().find(|i| i.name == name))
        .and_then(|image| {
            let path = image.href.strip_prefix("./").unwrap_or(&image.href);
            source
                .pictures
                .iter()
                .find(|(p, _)| p.strip_prefix("./").unwrap_or(p) == path)
                .and_then(|(_, bytes)| crate::picture::picture_within(bytes, MAX_SIDE))
        });
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
            style: match (g.stroke.as_deref(), g.stroke_style.as_deref()) {
                (_, Some("dotted")) => desk_model::BorderStyle::Dotted,
                (Some("dash"), _) => desk_model::BorderStyle::Dashed,
                _ => desk_model::BorderStyle::Solid,
            },
            start: line_end(g.marker_start.as_deref()),
            end: line_end(g.marker_end.as_deref()),
        });
    }
    if let ObjectKind::Image(picture) = &built.kind {
        built.picture = model_picture_settings(&g, picture, frame);
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
        style.shadow = Some(model_shadow(&g));
    }
    if let Some(opacity) = g.opacity {
        style.opacity = opacity.round().clamp(0.0, 100.0) as u8;
    }
    built.wrap = match (g.wrap.as_deref(), g.run_through.as_deref()) {
        (Some("none"), _) => Wrap::AboveAndBelow,
        (Some("left"), _) => Wrap::Left,
        (Some("right"), _) => Wrap::Right,
        (Some("biggest"), _) => Wrap::Largest,
        (Some("run-through"), Some("background")) => Wrap::Behind,
        (Some("run-through"), _) => Wrap::Through,
        _ => Wrap::Both,
    };
    if anchor_type == "as-char" {
        built.wrap = Wrap::AboveAndBelow;
    }
    built.wrap_margin = g.margin.map_or(Length::ZERO, length);
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
    }
    built.locked = g.protect.as_deref().is_some_and(|p| p.contains("position"));
    built.origin = object
        .origin
        .as_deref()
        .and_then(|o| u64::from_str_radix(o, 16).ok());
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
                Some(object) => import_one(source, document, object, None, &setup, paragraph, 0)?,
                None => None,
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

/// The `draw:name` of a text box, which the boxes linked to it name.
fn text_box_name(id: NodeId) -> String {
    format!("Text Box {id}")
}

/// Adds the drawing objects of an ODF document to a document whose body
/// has its paragraphs, in the order of their `draw:z-index`.
pub(super) fn import_objects(
    source: &odf::TextDocument,
    document: &mut Document,
    paragraph: &mut dyn FnMut(&mut Document, &odf::Paragraph) -> Result<Paragraph, FilterError>,
) -> Result<(), FilterError> {
    let setup = *document.page_setup();
    let ids: Vec<NodeId> = document.body().iter().map(Paragraph::id).collect();
    let mut found: Vec<(&odf::DrawObject, Option<NodeId>)> =
        source.page_objects.iter().map(|o| (o, None)).collect();
    for (index, p) in source.body.iter().enumerate() {
        found.extend(p.frames.iter().map(|o| (o, ids.get(index).copied())));
    }
    import_inline_objects(source, document, &setup, paragraph)?;
    found.sort_by_key(|(o, _)| o.z_index.unwrap_or(u32::MAX));
    // The frames by name, and the text boxes that go on in another.
    let mut names: HashMap<&str, NodeId> = HashMap::new();
    let mut links: Vec<(NodeId, &str)> = Vec::new();
    // The shapes by id, and the connectors that join them (OBJ-31).
    let mut shapes: HashMap<&str, NodeId> = HashMap::new();
    let mut connectors: Vec<(NodeId, Option<&str>, Option<&str>)> = Vec::new();
    for (object, anchor) in found {
        if let Some(built) = import_one(source, document, object, anchor, &setup, paragraph, 0)? {
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
    Ok(())
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
    fn paragraph(&mut self, paragraph: &Paragraph) -> odf::Paragraph {
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
            self.style("OP", odf::Family::Paragraph, Some(style_name), properties)
        };
        let names = self.names;
        let mut runs = Vec::new();
        for run in paragraph.runs() {
            let attrs = run.attrs.without_field();
            let named = run.style.map(|id| names[&id].clone());
            runs.push(if attrs.is_empty() {
                named
            } else {
                let properties = odf::Properties {
                    text: text_properties(
                        &attrs,
                        &styles.resolve_character(paragraph.style(), run.style, &run.attrs),
                    ),
                    ..Default::default()
                };
                Some(self.style("OT", odf::Family::Text, named, properties))
            });
        }
        let mut names = runs.into_iter();
        let spans = export_spans(paragraph, None, |_| names.next().flatten());
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
        let frame = object.frame;
        let (x, y) = (frame.x.to_pt() - dx, frame.y.to_pt());
        let (w, h) = (frame.width.to_pt(), frame.height.to_pt());
        let mut g = odf::GraphicProperties::default();
        let style = &object.style;
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
                g.stroke = Some(
                    if stroke.style == desk_model::BorderStyle::Solid {
                        "solid"
                    } else {
                        "dash"
                    }
                    .into(),
                );
                g.stroke_color = Some(odf_color(stroke.color));
                g.stroke_width = Some(odf_length(stroke.width));
                g.stroke_style = match stroke.style {
                    desk_model::BorderStyle::Solid => None,
                    desk_model::BorderStyle::Dashed => Some("dashed".into()),
                    desk_model::BorderStyle::Dotted => Some("dotted".into()),
                };
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
        if let Anchor::Master(_, pages) = object.anchor {
            g.master_pages = match pages {
                desk_model::MarginPages::Default => None,
                desk_model::MarginPages::First => Some("first".into()),
                desk_model::MarginPages::Left => Some("left".into()),
            };
        }
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
        let (wrap, through) = match object.wrap {
            Wrap::Both => ("parallel", None),
            Wrap::Largest => ("biggest", None),
            Wrap::Left => ("left", None),
            Wrap::Right => ("right", None),
            Wrap::AboveAndBelow => ("none", None),
            Wrap::Through => ("run-through", Some("foreground")),
            Wrap::Behind => ("run-through", Some("background")),
        };
        g.wrap = Some(wrap.into());
        g.run_through = through.map(Into::into);
        g.margin = Some(odf_length(object.wrap_margin));
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
                | Shape::Path { .. }),
            ) => parametric_shape(shape, w, h),
            ObjectKind::Image(picture) => odf::DrawKind::Image {
                href: self.picture(picture),
            },
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
                    paragraphs: text.paragraphs.iter().map(|p| self.paragraph(p)).collect(),
                    chain_next: text.next.map(text_box_name),
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
            text = inside
                .paragraphs
                .iter()
                .map(|p| self.paragraph(p))
                .collect();
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
        let linked = matches!(&kind, odf::DrawKind::TextBox { .. });
        let mut out = odf::DrawObject::new(kind);
        out.style = Some(style);
        out.text = text;
        out.origin = object.origin.map(|o| format!("{o:016x}"));
        if !matches!(object.kind, ObjectKind::Shape(_)) {
            out.contour = object.contour.as_deref().map(<[_]>::to_vec);
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

/// The spans that hold the places of objects in paragraphs and in the
/// notes in them, by the names of the places.
fn collect_placeholders<'a>(
    paragraphs: impl Iterator<Item = &'a mut odf::Paragraph>,
    out: &mut HashMap<String, &'a mut odf::Span>,
) {
    for paragraph in paragraphs {
        for span in paragraph.spans.iter_mut() {
            let name = match &span.field {
                Some(odf::Field::Object(o)) => o.name.clone(),
                _ => None,
            };
            if let Some(name) = name {
                out.insert(name, span);
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
    .flat_map(|part| part.paragraphs.iter_mut())
}

/// Writes the objects of a document: those fixed on pages before the
/// first paragraph, and those that move with a paragraph in it.
pub(super) fn export_objects(exporter: &mut Exporter<'_>, out: &mut odf::TextDocument) {
    let document = exporter.document;
    let indexes: HashMap<NodeId, usize> = document
        .body()
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id(), i))
        .collect();
    let left = document.page_setup().margin_left.to_pt();
    // Where the marks of the objects in the text were written, in the
    // body and its notes.
    let mut placeholders: HashMap<String, &mut odf::Span> = HashMap::new();
    collect_placeholders(out.body.iter_mut(), &mut placeholders);
    let mut floating = Vec::new();
    for (z, object) in document.objects().iter().enumerate() {
        if object.anchor == Anchor::InText {
            // In the line of the text, where its mark is (OBJ-01).
            if let Some(span) = placeholders.remove(&inline_placeholder(object.id)) {
                let mut drawn = exporter.object(object, "as-char", None, 0.0);
                drawn.x = None;
                drawn.y = None;
                drawn.z_index = Some(z as u32);
                span.field = Some(odf::Field::Object(Box::new(drawn)));
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
        match at.and_then(|i| out.body.get_mut(i)) {
            Some(paragraph) => paragraph.frames.push(drawn),
            None => out.page_objects.push(drawn),
        }
    }
    out.gradients = std::mem::take(&mut exporter.gradients);
    out.markers = std::mem::take(&mut exporter.markers);
    out.fill_images = std::mem::take(&mut exporter.fill_images);
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
    };
    // The objects in the lines of the headers and footers (EST-12), with
    // the styles of the master pages.
    {
        let mut placeholders: HashMap<String, &mut odf::Span> = HashMap::new();
        collect_placeholders(master_paragraphs(&mut out.master), &mut placeholders);
        for (_, page) in &mut out.masters {
            collect_placeholders(master_paragraphs(page), &mut placeholders);
        }
        for (z, object) in document.objects().iter().enumerate() {
            if object.anchor != Anchor::InText {
                continue;
            }
            if let Some(span) = placeholders.remove(&inline_placeholder(object.id)) {
                let mut drawn = exporter.object(object, "as-char", None, 0.0);
                drawn.x = None;
                drawn.y = None;
                drawn.z_index = Some(z as u32);
                span.field = Some(odf::Field::Object(Box::new(drawn)));
            }
        }
        let stray: Vec<String> = placeholders.into_keys().collect();
        drop_placeholders(master_paragraphs(&mut out.master), &stray);
        for (_, page) in &mut out.masters {
            drop_placeholders(master_paragraphs(page), &stray);
        }
    }
    for (z, object) in document.objects().iter().enumerate() {
        let Anchor::Master(section, _) = object.anchor else {
            continue;
        };
        let mut drawn = exporter.object(object, "page", None, 0.0);
        drawn.z_index = Some(z as u32);
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

#[cfg(test)]
mod tests {
    use super::*;

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
