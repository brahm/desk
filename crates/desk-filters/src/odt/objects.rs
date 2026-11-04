// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The floating objects of a document to and from ODF drawing objects
//! (OBJ-01, OBJ-08, OBJ-13, OBJ-17).
//!
//! An object fixed on a page is a drawing object anchored to that page,
//! measured from the corner of the paper; one that moves with a paragraph
//! is anchored to it, measured from the left of the text and the top of
//! the paragraph. Objects in the text (`as-char`) are read as moving with
//! their paragraph, with the text above and below them, until Desk has
//! inline objects.

use std::collections::HashMap;
use std::sync::Arc;

use desk_model::{
    Anchor, Document, Fill, Frame, Length, NodeId, Object, ObjectKind, Op, POLYGON_SCALE,
    PageSetup, ParaAttrs, Paragraph, Shape, Stroke, TextBox, Wrap,
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
            vertical_rel
        );
    }
    out
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
        odf::DrawKind::TextBox { paragraphs } => {
            let mut built = Vec::with_capacity(paragraphs.len());
            for p in paragraphs {
                built.push(paragraph(document, p)?);
            }
            if built.is_empty() {
                let style = document.styles().default_paragraph();
                built.push(Paragraph::new(document.new_id(), style));
            }
            ObjectKind::TextBox(TextBox {
                paragraphs: Arc::from(built),
                inset: padding.map_or(Length::ZERO, length),
            })
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
    let Some(kind) = kind(source, document, object, paragraph, g.padding, setup, depth)? else {
        return Ok(None);
    };
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
    if filled && let Some(color) = g.fill_color {
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
        });
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
    Ok(Some(built))
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
    found.sort_by_key(|(o, _)| o.z_index.unwrap_or(u32::MAX));
    for (object, anchor) in found {
        if let Some(built) = import_one(source, document, object, anchor, &setup, paragraph, 0)? {
            let index = document.objects().len();
            document.apply(Op::InsertObject {
                index,
                object: built,
            })?;
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
        match style.fill {
            Some(Fill::Color(color)) => {
                g.fill = Some("solid".into());
                g.fill_color = Some(odf_color(color));
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
            }
            None => g.stroke = Some("none".into()),
        }
        if style.opacity < 100 {
            g.opacity = Some(f64::from(style.opacity));
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
        g.horizontal_pos = Some("from-left".into());
        g.horizontal_rel = Some(rel.into());
        g.vertical_pos = Some("from-top".into());
        g.vertical_rel = Some(rel.into());
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
                odf::DrawKind::TextBox {
                    paragraphs: text.paragraphs.iter().map(|p| self.paragraph(p)).collect(),
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
        let style = self.style(
            "gr",
            odf::Family::Graphic,
            None,
            odf::Properties {
                graphic: g,
                ..Default::default()
            },
        );
        let mut out = odf::DrawObject::new(kind);
        out.style = Some(style);
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
    for (z, object) in document.objects().iter().enumerate() {
        let at = match object.anchor {
            Anchor::Paragraph(id) => indexes.get(&id).copied(),
            Anchor::Page(_) => None,
        };
        let mut drawn = match (object.anchor, at) {
            (Anchor::Paragraph(_), Some(_)) => exporter.object(object, "paragraph", None, left),
            (Anchor::Page(page), _) => exporter.object(object, "page", Some(page), 0.0),
            (Anchor::Paragraph(_), None) => exporter.object(object, "page", Some(0), 0.0),
        };
        drawn.z_index = Some(z as u32);
        match at.and_then(|i| out.body.get_mut(i)) {
            Some(paragraph) => paragraph.frames.push(drawn),
            None => out.page_objects.push(drawn),
        }
    }
}
