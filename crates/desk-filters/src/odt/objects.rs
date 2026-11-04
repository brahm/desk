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
    NodeId, Object, ObjectKind, Op, POLYGON_SCALE, PageSetup, ParaAttrs, Paragraph, Shape, Stroke,
    TextBox, VerticalAlignment, Wrap,
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
            opacity,
            wrap,
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

/// A gradient of ODF as Desk fills with it: along a line, from one
/// color to the other (OBJ-09). Other kinds of gradients are taken as
/// along a line.
fn model_gradient(gradient: &odf::Gradient) -> Fill {
    // ODF turns its gradient counterclockwise from going down; Desk
    // measures the way it goes counterclockwise from going right.
    let angle = ((gradient.angle + 270.0) * 100.0).round() as i64;
    Fill::Gradient {
        from: gradient.start_color.map_or(desk_model::Rgba::BLACK, rgba),
        to: gradient.end_color.map_or(desk_model::Rgba::WHITE, rgba),
        angle: angle.rem_euclid(36000) as i32,
    }
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
    }
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
        "right-arrow" => polygon(&[
            (0, 2_500),
            (6_500, 2_500),
            (6_500, 0),
            (s, s / 2),
            (6_500, s),
            (6_500, 7_500),
            (0, 7_500),
        ]),
        "left-right-arrow" => polygon(&[
            (0, s / 2),
            (2_500, 0),
            (2_500, 2_500),
            (7_500, 2_500),
            (7_500, 0),
            (s, s / 2),
            (7_500, s),
            (7_500, 7_500),
            (2_500, 7_500),
            (2_500, s),
        ]),
        "star5" => {
            // Five points, and the inner corners between them.
            let points: Vec<(i32, i32)> = (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { 0.5 } else { 0.19 };
                    let a = std::f64::consts::PI * (i as f64 / 5.0 - 0.5);
                    let k = f64::from(s);
                    (
                        ((0.5 + r * a.cos()) * k).round() as i32,
                        ((0.5 + r * a.sin()) * k).round() as i32,
                    )
                })
                .collect();
            Shape::Polygon(Arc::from(points))
        }
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
        odf::DrawKind::CustomShape { shape } => ObjectKind::Shape(custom_shape(shape)),
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

/// The frame and rotation of an object, as it says them: its position
/// and size, or the box of a line, and the rotation and translation of
/// its transform, about its corner.
fn placement(object: &odf::DrawObject) -> (f64, f64, f64, f64, i32) {
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
        Anchor::InText => {}
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
        style.fill = Some(Fill::Image { picture, mode });
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
    built.locked = g.protect.as_deref().is_some_and(|p| p.contains("position"));
    let mirror = g.mirror.as_deref().unwrap_or_default();
    built.flip_horizontal = mirror.contains("horizontal");
    built.flip_vertical = mirror.contains("vertical");
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

/// The name that holds the place of an object in the text while the
/// objects are written.
pub(super) fn inline_placeholder(id: NodeId) -> String {
    format!("desk-inline-{id}")
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
    for (object, anchor) in found {
        if let Some(built) = import_one(source, document, object, anchor, &setup, paragraph, 0)? {
            if let Some(name) = &object.name {
                names.insert(name, built.id);
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

    /// The path of a picture in the package, added when it is not there.
    fn picture(&mut self, picture: &desk_model::Picture) -> String {
        if let Some((href, _)) = self.pictures.iter().find(|(_, b)| b == picture.png()) {
            return href.clone();
        }
        let href = format!("Pictures/object{}.png", self.pictures.len() + 1);
        self.pictures.push((href.clone(), picture.png().to_vec()));
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
            Some(Fill::Image { picture, mode }) => {
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
        if object.locked {
            g.protect = Some("position size".into());
        }
        let mirror: Vec<&str> = [
            (object.flip_horizontal, "horizontal"),
            (object.flip_vertical, "vertical"),
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
            ObjectKind::Shape(Shape::Line { rising }) => {
                let (y1, y2) = if *rising { (y + h, y) } else { (y, y + h) };
                odf::DrawKind::Line {
                    x1: odf::Length(x),
                    y1: odf::Length(y1),
                    x2: odf::Length(x + w),
                    y2: odf::Length(y2),
                }
            }
            ObjectKind::Image(picture) => odf::DrawKind::Image {
                href: self.picture(picture),
            },
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
            ObjectKind::Group(objects) => odf::DrawKind::Group(
                objects
                    .iter()
                    .map(|o| self.object(o, anchor, None, dx))
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
        if linked {
            // Named, for the boxes linked to it to name it.
            out.name = Some(text_box_name(object.id));
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
    // Where the marks of the objects in the text were written.
    let mut placeholders: HashMap<String, &mut odf::Span> = out
        .body
        .iter_mut()
        .flat_map(|p| p.spans.iter_mut())
        .filter_map(|span| match &span.field {
            Some(odf::Field::Object(o)) => o.name.clone().map(|name| (name, span)),
            _ => None,
        })
        .collect();
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
        floating.push((z, object));
    }
    // A mark without its object is left out.
    let stray: Vec<String> = placeholders.into_keys().collect();
    for paragraph in &mut out.body {
        paragraph.spans.retain(|span| match &span.field {
            Some(odf::Field::Object(o)) => o.name.as_ref().is_none_or(|n| !stray.contains(n)),
            _ => true,
        });
    }
    for (z, object) in floating {
        let at = match object.anchor {
            Anchor::Paragraph(id) => indexes.get(&id).copied(),
            Anchor::Page(_) | Anchor::InText => None,
        };
        let mut drawn = match (object.anchor, at) {
            (Anchor::Paragraph(_), Some(_)) => exporter.object(object, "paragraph", None, left),
            (Anchor::Page(page), _) => exporter.object(object, "page", Some(page), 0.0),
            (Anchor::Paragraph(_) | Anchor::InText, None) => {
                exporter.object(object, "page", Some(0), 0.0)
            }
            (Anchor::InText, Some(_)) => continue,
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
