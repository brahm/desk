// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Drawing objects: frames with pictures or text boxes, shapes, lines and
//! groups (`draw:` elements), and the properties of their graphic styles.

use crate::ns::{DESK, DRAW, FO, STYLE, SVG, TEXT, XLINK};
use crate::text::Paragraph;
use crate::units::{Color, Length, decimal, parse_percent};
use crate::xml::{Element, Writer};

/// The properties of a graphic style (`style:graphic-properties`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphicProperties {
    /// `draw:fill`: `none`, `solid`, `gradient`, `hatch` or `bitmap`.
    pub fill: Option<String>,
    /// `draw:fill-color`.
    pub fill_color: Option<Color>,
    /// `draw:stroke`: `none`, `solid` or `dash`.
    pub stroke: Option<String>,
    /// `svg:stroke-color`.
    pub stroke_color: Option<Color>,
    /// `svg:stroke-width`.
    pub stroke_width: Option<Length>,
    /// `desk:stroke-style`: how Desk draws a dashed line, `dashed` or
    /// `dotted`, which `draw:stroke-dash` would need a named style for.
    pub stroke_style: Option<String>,
    /// `draw:opacity`, in percent.
    pub opacity: Option<f64>,
    /// `style:wrap`: `none`, `left`, `right`, `parallel`, `dynamic`,
    /// `run-through` or `biggest`.
    pub wrap: Option<String>,
    /// `style:run-through`: `foreground` or `background`.
    pub run_through: Option<String>,
    /// `fo:margin-left`, which Desk writes on every side: the room
    /// between the object and the text around it.
    pub margin: Option<Length>,
    /// `fo:padding`: the room between the frame and its text.
    pub padding: Option<Length>,
    /// `style:protect`, as `position size`.
    pub protect: Option<String>,
    /// `style:mirror`, as `horizontal vertical`.
    pub mirror: Option<String>,
    /// `style:horizontal-pos`, as `from-left`.
    pub horizontal_pos: Option<String>,
    /// `style:horizontal-rel`, as `page` or `paragraph`.
    pub horizontal_rel: Option<String>,
    /// `style:vertical-pos`, as `from-top`.
    pub vertical_pos: Option<String>,
    /// `style:vertical-rel`, as `page` or `paragraph`.
    pub vertical_rel: Option<String>,
    /// `draw:textarea-vertical-align`: `top`, `middle`, `bottom` or
    /// `justify`, where the text of a text box sits.
    pub textarea_vertical_align: Option<String>,
    /// `fo:column-count` of `style:columns`: the columns of a text box.
    pub column_count: Option<u32>,
    /// `fo:column-gap` of `style:columns`.
    pub column_gap: Option<Length>,
}

/// A drawing object.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawObject {
    /// What it is.
    pub kind: DrawKind,
    /// `draw:style-name`: its graphic style.
    pub style: Option<String>,
    /// `draw:name`.
    pub name: Option<String>,
    /// `text:anchor-type`: `page`, `paragraph`, `char` or `as-char`.
    pub anchor: Option<String>,
    /// `text:anchor-page-number`, from 1, of an object anchored to a page.
    pub anchor_page: Option<u32>,
    /// `svg:x`.
    pub x: Option<Length>,
    /// `svg:y`.
    pub y: Option<Length>,
    /// `svg:width`.
    pub width: Option<Length>,
    /// `svg:height`.
    pub height: Option<Length>,
    /// `draw:transform`, as written.
    pub transform: Option<String>,
    /// `draw:z-index`.
    pub z_index: Option<u32>,
}

/// Every number read is finite, so equality is reflexive.
impl Eq for DrawObject {}

impl DrawObject {
    /// An object of a kind with nothing else set.
    pub fn new(kind: DrawKind) -> Self {
        Self {
            kind,
            style: None,
            name: None,
            anchor: None,
            anchor_page: None,
            x: None,
            y: None,
            width: None,
            height: None,
            transform: None,
            z_index: None,
        }
    }

    /// The pictures of the package that it and the objects of a group
    /// link to.
    pub fn hrefs(&self) -> Vec<&str> {
        match &self.kind {
            DrawKind::Image { href } => vec![href.as_str()],
            DrawKind::Group(objects) => objects.iter().flat_map(Self::hrefs).collect(),
            DrawKind::TextBox { paragraphs } => paragraphs
                .iter()
                .flat_map(|p| &p.frames)
                .flat_map(Self::hrefs)
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// What a drawing object is.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawKind {
    /// `draw:rect`.
    Rect {
        /// `draw:corner-radius`.
        corner_radius: Option<Length>,
    },
    /// `draw:ellipse` or `draw:circle`.
    Ellipse,
    /// `draw:line`, from one end to the other.
    Line {
        /// `svg:x1`.
        x1: Length,
        /// `svg:y1`.
        y1: Length,
        /// `svg:x2`.
        x2: Length,
        /// `svg:y2`.
        y2: Length,
    },
    /// `draw:polygon`: its points (`svg:points`) in the units of its
    /// `svg:viewBox`, as left, top, width and height.
    Polygon {
        /// The points.
        points: Vec<(f64, f64)>,
        /// The view box.
        view_box: Option<(f64, f64, f64, f64)>,
    },
    /// `draw:custom-shape`, by the `draw:type` of its geometry, as
    /// `rectangle` or `isosceles-triangle`.
    CustomShape {
        /// The type.
        shape: String,
    },
    /// `draw:frame` with a `draw:image`.
    Image {
        /// `xlink:href`: the path of the picture in the package.
        href: String,
    },
    /// `draw:frame` with a `draw:text-box`.
    TextBox {
        /// The text.
        paragraphs: Vec<Paragraph>,
    },
    /// `draw:g`.
    Group(Vec<DrawObject>),
}

pub(crate) fn graphic_properties(e: &Element) -> GraphicProperties {
    let text = |ns, name| e.attr(ns, name).map(str::to_owned);
    let length = |ns, name| e.attr(ns, name).and_then(Length::parse);
    GraphicProperties {
        fill: text(DRAW, "fill"),
        fill_color: e.attr(DRAW, "fill-color").and_then(Color::parse),
        stroke: text(DRAW, "stroke"),
        stroke_color: e.attr(SVG, "stroke-color").and_then(Color::parse),
        stroke_width: length(SVG, "stroke-width"),
        stroke_style: text(DESK, "stroke-style"),
        opacity: e.attr(DRAW, "opacity").and_then(parse_percent),
        wrap: text(STYLE, "wrap"),
        run_through: text(STYLE, "run-through"),
        margin: length(FO, "margin-left").or_else(|| length(FO, "margin")),
        padding: length(FO, "padding").or_else(|| length(FO, "padding-left")),
        protect: text(STYLE, "protect"),
        mirror: text(STYLE, "mirror"),
        horizontal_pos: text(STYLE, "horizontal-pos"),
        horizontal_rel: text(STYLE, "horizontal-rel"),
        vertical_pos: text(STYLE, "vertical-pos"),
        vertical_rel: text(STYLE, "vertical-rel"),
        textarea_vertical_align: text(DRAW, "textarea-vertical-align"),
        column_count: e
            .child(STYLE, "columns")
            .and_then(|c| c.attr(FO, "column-count"))
            .and_then(|n| n.parse().ok()),
        column_gap: e
            .child(STYLE, "columns")
            .and_then(|c| c.attr(FO, "column-gap"))
            .and_then(Length::parse),
    }
}

pub(crate) fn write_graphic_properties(w: &mut Writer, g: &GraphicProperties) {
    if g == &GraphicProperties::default() {
        return;
    }
    w.start("style:graphic-properties");
    w.attr_opt("draw:fill", g.fill.as_deref());
    if let Some(color) = g.fill_color {
        w.attr("draw:fill-color", &color.to_string());
    }
    w.attr_opt("draw:stroke", g.stroke.as_deref());
    if let Some(color) = g.stroke_color {
        w.attr("svg:stroke-color", &color.to_string());
    }
    if let Some(width) = g.stroke_width {
        w.attr("svg:stroke-width", &width.to_string());
    }
    w.attr_opt("desk:stroke-style", g.stroke_style.as_deref());
    if let Some(opacity) = g.opacity {
        w.attr("draw:opacity", &format!("{}%", decimal(opacity)));
    }
    w.attr_opt("style:wrap", g.wrap.as_deref());
    w.attr_opt("style:run-through", g.run_through.as_deref());
    if let Some(margin) = g.margin {
        let margin = margin.to_string();
        for side in [
            "fo:margin-left",
            "fo:margin-right",
            "fo:margin-top",
            "fo:margin-bottom",
        ] {
            w.attr(side, &margin);
        }
    }
    if let Some(padding) = g.padding {
        w.attr("fo:padding", &padding.to_string());
    }
    w.attr_opt("style:protect", g.protect.as_deref());
    w.attr_opt("style:mirror", g.mirror.as_deref());
    w.attr_opt("style:horizontal-pos", g.horizontal_pos.as_deref());
    w.attr_opt("style:horizontal-rel", g.horizontal_rel.as_deref());
    w.attr_opt("style:vertical-pos", g.vertical_pos.as_deref());
    w.attr_opt("style:vertical-rel", g.vertical_rel.as_deref());
    w.attr_opt(
        "draw:textarea-vertical-align",
        g.textarea_vertical_align.as_deref(),
    );
    if let Some(count) = g.column_count.filter(|&n| n > 1) {
        w.start("style:columns");
        w.attr("fo:column-count", &count.to_string());
        if let Some(gap) = g.column_gap {
            w.attr("fo:column-gap", &gap.to_string());
        }
        w.end();
    }
    w.end();
}

/// The numbers of a list such as `0 0 100 50` or `1,2 3,4`.
fn numbers(value: &str) -> Vec<f64> {
    value
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .filter(|n: &f64| n.is_finite())
        .collect()
}

/// How many objects a group may hold, deep, against hostile files.
const MAX_DEPTH: usize = 16;

/// A drawing object, if the element is one Desk reads. `paragraph` reads
/// the paragraphs of a text box.
pub(crate) fn draw_object(
    e: &Element,
    paragraph: &dyn Fn(&Element) -> Paragraph,
) -> Option<DrawObject> {
    draw_object_at(e, paragraph, 0)
}

fn draw_object_at(
    e: &Element,
    paragraph: &dyn Fn(&Element) -> Paragraph,
    depth: usize,
) -> Option<DrawObject> {
    if e.name.ns != DRAW || depth > MAX_DEPTH {
        return None;
    }
    let kind = match e.name.local.as_str() {
        "rect" => DrawKind::Rect {
            corner_radius: e.attr(DRAW, "corner-radius").and_then(Length::parse),
        },
        "ellipse" | "circle" => DrawKind::Ellipse,
        "line" => {
            let at = |name| {
                e.attr(SVG, name)
                    .and_then(Length::parse)
                    .unwrap_or_default()
            };
            DrawKind::Line {
                x1: at("x1"),
                y1: at("y1"),
                x2: at("x2"),
                y2: at("y2"),
            }
        }
        "polygon" => {
            let flat = numbers(e.attr(DRAW, "points").unwrap_or_default());
            let view_box = match numbers(e.attr(SVG, "viewBox").unwrap_or_default())[..] {
                [x, y, w, h] => Some((x, y, w, h)),
                _ => None,
            };
            DrawKind::Polygon {
                points: flat
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| (p[0], p[1]))
                    .collect(),
                view_box,
            }
        }
        "custom-shape" => DrawKind::CustomShape {
            shape: e
                .child(DRAW, "enhanced-geometry")
                .and_then(|g| g.attr(DRAW, "type"))
                .unwrap_or("rectangle")
                .to_owned(),
        },
        "frame" => {
            if let Some(image) = e.child(DRAW, "image") {
                DrawKind::Image {
                    href: image.attr(XLINK, "href")?.to_owned(),
                }
            } else {
                let text = e.child(DRAW, "text-box")?;
                DrawKind::TextBox {
                    paragraphs: text
                        .elements()
                        .filter(|p| p.name.is(TEXT, "p") || p.name.is(TEXT, "h"))
                        .map(paragraph)
                        .collect(),
                }
            }
        }
        "g" => DrawKind::Group(
            e.elements()
                .filter_map(|c| draw_object_at(c, paragraph, depth + 1))
                .collect(),
        ),
        _ => return None,
    };
    let length = |ns, name| e.attr(ns, name).and_then(Length::parse);
    // A text box takes its height from its text when it has only the
    // least height.
    let height = length(SVG, "height").or_else(|| length(FO, "min-height"));
    Some(DrawObject {
        kind,
        style: e.attr(DRAW, "style-name").map(str::to_owned),
        name: e.attr(DRAW, "name").map(str::to_owned),
        anchor: e.attr(TEXT, "anchor-type").map(str::to_owned),
        anchor_page: e
            .attr(TEXT, "anchor-page-number")
            .and_then(|n| n.trim().parse().ok()),
        x: length(SVG, "x"),
        y: length(SVG, "y"),
        width: length(SVG, "width").or_else(|| length(FO, "min-width")),
        height,
        transform: e.attr(DRAW, "transform").map(str::to_owned),
        z_index: e.attr(DRAW, "z-index").and_then(|n| n.trim().parse().ok()),
    })
}

/// Writes a drawing object. `paragraph` writes the paragraphs of a text
/// box.
pub(crate) fn write_draw_object(
    w: &mut Writer,
    object: &DrawObject,
    paragraph: &dyn Fn(&mut Writer, &Paragraph),
) {
    let name = match &object.kind {
        DrawKind::Rect { .. } => "draw:rect",
        DrawKind::Ellipse => "draw:ellipse",
        DrawKind::Line { .. } => "draw:line",
        DrawKind::Polygon { .. } => "draw:polygon",
        DrawKind::CustomShape { .. } => "draw:custom-shape",
        DrawKind::Image { .. } | DrawKind::TextBox { .. } => "draw:frame",
        DrawKind::Group(_) => "draw:g",
    };
    w.start(name);
    w.attr_opt("draw:style-name", object.style.as_deref());
    w.attr_opt("draw:name", object.name.as_deref());
    w.attr_opt("text:anchor-type", object.anchor.as_deref());
    if let Some(page) = object.anchor_page {
        w.attr("text:anchor-page-number", &page.to_string());
    }
    if let Some(z) = object.z_index {
        w.attr("draw:z-index", &z.to_string());
    }
    if !matches!(object.kind, DrawKind::Line { .. } | DrawKind::Group(_)) {
        for (attr, value) in [
            ("svg:x", object.x),
            ("svg:y", object.y),
            ("svg:width", object.width),
            ("svg:height", object.height),
        ] {
            if let Some(value) = value {
                w.attr(attr, &value.to_string());
            }
        }
        w.attr_opt("draw:transform", object.transform.as_deref());
    }
    match &object.kind {
        DrawKind::Rect { corner_radius } => {
            if let Some(radius) = corner_radius {
                w.attr("draw:corner-radius", &radius.to_string());
            }
        }
        DrawKind::Ellipse | DrawKind::Group(_) => {}
        DrawKind::Line { x1, y1, x2, y2 } => {
            w.attr("svg:x1", &x1.to_string())
                .attr("svg:y1", &y1.to_string())
                .attr("svg:x2", &x2.to_string())
                .attr("svg:y2", &y2.to_string());
        }
        DrawKind::Polygon { points, view_box } => {
            if let Some((x, y, width, height)) = view_box {
                let numbers = [x, y, width, height].map(|n| decimal(*n));
                w.attr("svg:viewBox", &numbers.join(" "));
            }
            let points: Vec<String> = points
                .iter()
                .map(|(x, y)| format!("{},{}", decimal(*x), decimal(*y)))
                .collect();
            w.attr("draw:points", &points.join(" "));
        }
        DrawKind::CustomShape { shape } => {
            w.start("draw:enhanced-geometry")
                .attr("draw:type", shape)
                .end();
        }
        DrawKind::Image { href } => {
            w.start("draw:image")
                .attr("xlink:href", href)
                .attr("xlink:type", "simple")
                .attr("xlink:show", "embed")
                .attr("xlink:actuate", "onLoad")
                .end();
        }
        DrawKind::TextBox { paragraphs } => {
            w.start("draw:text-box");
            for p in paragraphs {
                paragraph(w, p);
            }
            w.end();
        }
    }
    if let DrawKind::Group(objects) = &object.kind {
        for child in objects {
            write_draw_object(w, child, paragraph);
        }
    }
    w.end();
}

/// The rotation, counterclockwise in radians, and the translation that a
/// `draw:transform` sets: `rotate (a) translate (x y)`, as other programs
/// write it.
pub fn parse_transform(value: &str) -> (f64, Length, Length) {
    let mut angle = 0.0;
    let (mut x, mut y) = (Length::default(), Length::default());
    for part in value.split(')') {
        let Some((name, args)) = part.split_once('(') else {
            continue;
        };
        let args: Vec<&str> = args
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .collect();
        match name.trim() {
            "rotate" => {
                if let Some(a) = args.first().and_then(|a| a.parse::<f64>().ok()) {
                    angle += a;
                }
            }
            "translate" => {
                let at = |i: usize| {
                    args.get(i)
                        .and_then(|v| Length::parse(v).or_else(|| v.parse().ok().map(Length)))
                        .unwrap_or_default()
                };
                x = Length(x.0 + at(0).0);
                y = Length(y.0 + at(1).0);
            }
            _ => {}
        }
    }
    if !angle.is_finite() {
        angle = 0.0;
    }
    (angle, x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_rotate_then_translate() {
        let (angle, x, y) = parse_transform("rotate (1.5707963) translate (1in 2cm)");
        assert!((angle - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(x, Length(72.0));
        assert!((y.0 - 56.693).abs() < 0.01);
        assert_eq!(parse_transform("skewX(1)"), (0.0, Length(0.0), Length(0.0)));
    }
}
