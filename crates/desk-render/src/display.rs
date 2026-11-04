// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The display list: what a page draws, in page coordinates.

use desk_layout::{DocumentLayout, Page, PlacedBlock, PlacedLine, PlacedObject};
use desk_model::{BorderSides, Fill, LineStyle, ObjectKind, POLYGON_SCALE, Rgba, Shape, Wrap};
use desk_text::FontData;

/// One glyph, placed on the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    /// The glyph ID in the font.
    pub id: u32,
    /// The left of the glyph origin, in points from the left of the page.
    pub x: f32,
    /// The baseline, in points from the top of the page.
    pub y: f32,
}

/// Something to draw.
#[derive(Clone, Debug)]
pub enum Item {
    /// Glyphs of one font, size and color.
    Glyphs {
        /// The font.
        font: FontData,
        /// The size, in points.
        size: f32,
        /// The color.
        color: Rgba,
        /// The glyphs.
        glyphs: Vec<PlacedGlyph>,
        /// The text that the glyphs draw, for the text of a PDF.
        text: String,
        /// How many bytes of `text` each cluster draws with how many glyphs,
        /// in order. Empty when they are not known.
        clusters: Vec<(u32, u32)>,
        /// The width of the line of outlined glyphs, which are not filled
        /// (FMT-04).
        outline: Option<f32>,
    },
    /// The blurred shadow of glyphs (FMT-03).
    Shadow {
        /// The font.
        font: FontData,
        /// The size, in points.
        size: f32,
        /// The color, with the opacity of the shadow.
        color: Rgba,
        /// The glyphs that cast the shadow, where they are drawn.
        glyphs: Vec<PlacedGlyph>,
        /// How far the shadow falls right and down from the glyphs.
        offset: (f32, f32),
        /// The radius of the blur.
        blur: f32,
    },
    /// A line along points, as for the rules and borders of paragraphs.
    Stroke {
        /// The points.
        points: Vec<(f32, f32)>,
        /// Whether the line goes back to the first point.
        closed: bool,
        /// The width.
        width: f32,
        /// The color.
        color: Rgba,
        /// The lengths of the dashes and the gaps between them; empty for
        /// a solid line.
        dashes: Vec<f32>,
    },
    /// A picture, as an image bullet (LST-03), scaled into a rectangle.
    Image {
        /// The picture.
        picture: desk_model::Picture,
        /// The left edge.
        x: f32,
        /// The top edge.
        y: f32,
        /// The width.
        width: f32,
        /// The height.
        height: f32,
    },
    /// A path, filled, outlined or both, as for shapes (OBJ-08).
    Path {
        /// The path.
        path: Vec<PathOp>,
        /// The color inside it; none when it is not filled.
        fill: Option<Rgba>,
        /// The line along it; none when it is not outlined.
        stroke: Option<PathStroke>,
    },
    /// Items drawn moved, rotated or flipped together, and faded as one,
    /// as an object is (OBJ-01).
    Group {
        /// The affine transform from the group to the page, as Cairo
        /// takes it: `xx, yx, xy, yy, x0, y0`.
        transform: [f32; 6],
        /// How opaque the group is, from 0 to 1.
        opacity: f32,
        /// The items, from back to front.
        items: Vec<Item>,
    },
    /// A filled rectangle, as for underlines and strikethroughs.
    Rect {
        /// The left edge.
        x: f32,
        /// The top edge.
        y: f32,
        /// The width.
        width: f32,
        /// The height.
        height: f32,
        /// The color.
        color: Rgba,
    },
}

/// One step of a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathOp {
    /// Starts a part of the path at a point.
    Move(f32, f32),
    /// A straight line to a point.
    Line(f32, f32),
    /// A cubic Bézier curve, by its two control points and its end.
    Curve(f32, f32, f32, f32, f32, f32),
    /// Closes the part of the path, back to where it started.
    Close,
}

/// The line along a path.
#[derive(Clone, Debug, PartialEq)]
pub struct PathStroke {
    /// The width.
    pub width: f32,
    /// The color.
    pub color: Rgba,
    /// The lengths of the dashes and the gaps between them; empty for a
    /// solid line.
    pub dashes: Vec<f32>,
}

/// What one page, slide or sheet draws, in points from its top left
/// corner, in drawing order.
#[derive(Clone, Debug, Default)]
pub struct DisplayList {
    /// The width of the page.
    pub width: f32,
    /// The height of the page.
    pub height: f32,
    /// The items, from back to front.
    pub items: Vec<Item>,
}

impl DisplayList {
    /// The display list of a page of a laid out document.
    pub fn from_page(layout: &DocumentLayout, page: &Page) -> Self {
        let mut list = Self {
            width: layout.setup.width.to_pt() as f32,
            height: layout.setup.height.to_pt() as f32,
            items: Vec::new(),
        };
        for block in &page.blocks {
            list.block_background(block);
        }
        // Objects behind the text (OBJ-17).
        for object in page
            .objects
            .iter()
            .filter(|o| o.object.wrap == Wrap::Behind)
        {
            list.items.push(object_item(object));
        }
        // The short rule above the notes (EST-13).
        if let Some((x, y)) = page.note_rule {
            list.items.push(Item::Stroke {
                points: vec![(x, y), (x + desk_layout::NOTE_RULE_LENGTH, y)],
                closed: false,
                width: desk_layout::NOTE_RULE_WIDTH,
                color: Rgba::BLACK,
                dashes: Vec::new(),
            });
        }
        // The header and footer are drawn as the body is (EST-12).
        list.lines(
            page.lines
                .iter()
                .chain(&page.margin_lines)
                .chain(&page.note_lines),
        );
        for block in &page.blocks {
            list.block_border(block);
        }
        // The other objects, in front of the text.
        for object in page
            .objects
            .iter()
            .filter(|o| o.object.wrap != Wrap::Behind)
        {
            list.items.push(object_item(object));
        }
        list
    }

    /// The items of lines placed on the page: their backgrounds, glyphs,
    /// pictures and lines.
    fn lines<'a>(&mut self, lines: impl Iterator<Item = &'a PlacedLine>) {
        let list = self;
        for placed in lines {
            let (dx, dy) = placed.origin;
            // The backgrounds of runs span the height of the line, with
            // edges on whole points, as Pages draws them (FMT-20).
            let top = placed.top().round();
            let bottom = placed.bottom().round();
            for run in &placed.line.runs {
                if let Some(color) = run.background {
                    list.items.push(Item::Rect {
                        x: run.x + dx,
                        y: top,
                        width: run.advance,
                        height: bottom - top,
                        color,
                    });
                }
            }
            for run in &placed.line.runs {
                if run.glyphs.is_empty() {
                    continue;
                }
                let glyphs: Vec<PlacedGlyph> = run
                    .glyphs
                    .iter()
                    .map(|g| PlacedGlyph {
                        id: g.id,
                        x: g.x + dx,
                        y: g.y + dy,
                    })
                    .collect();
                if let Some(shadow) = run.shadow {
                    let (sx, sy) = shadow.displacement();
                    let mut color = shadow.color;
                    color.a = (f32::from(color.a) * f32::from(shadow.opacity.min(100)) / 100.0)
                        .round() as u8;
                    list.items.push(Item::Shadow {
                        font: run.font.clone(),
                        size: run.size,
                        color,
                        glyphs: glyphs.clone(),
                        offset: (sx as f32, sy as f32),
                        blur: shadow.blur.to_pt() as f32,
                    });
                }
                list.items.push(Item::Glyphs {
                    font: run.font.clone(),
                    size: run.size,
                    color: run.color,
                    text: run.text.clone(),
                    clusters: run.clusters.clone(),
                    glyphs,
                    // As Pages strokes outlined text, at a hundredth of
                    // its size.
                    outline: run.outline.then_some(run.size / 100.0),
                });
            }
            if let Some(image) = &placed.line.label_image {
                list.items.push(Item::Image {
                    picture: image.picture.clone(),
                    x: image.x + dx,
                    y: image.y + dy,
                    width: image.width,
                    height: image.height,
                });
            }
            for decoration in &placed.line.decorations {
                let lines = match decoration.style {
                    LineStyle::None => 0,
                    LineStyle::Single => 1,
                    LineStyle::Double => 2,
                };
                for i in 0..lines {
                    list.items.push(Item::Rect {
                        x: decoration.x0 + dx,
                        y: decoration.y + dy + i as f32 * 2.0 * decoration.thickness,
                        width: decoration.x1 - decoration.x0,
                        height: decoration.thickness,
                        color: decoration.color,
                    });
                }
            }
        }
    }

    /// The background of a paragraph: behind its lines, from margin to
    /// margin, and up to the line of a border around it (FMT-20).
    fn block_background(&mut self, block: &PlacedBlock) {
        let Some(color) = block.background else {
            return;
        };
        let border = &block.border;
        let inset = if border.sides == BorderSides::Box {
            border.offset.to_pt() as f32
        } else {
            0.0
        };
        let top = block.text_top - if block.first { inset } else { 0.0 };
        let bottom = block.text_bottom + if block.last { inset } else { 0.0 };
        self.items.push(Item::Rect {
            x: block.left - inset,
            y: top,
            width: block.right - block.left + 2.0 * inset,
            height: bottom - top,
            color,
        });
    }

    /// The rules and border of a paragraph (FMT-21). As Pages draws them, a
    /// rule spans the text column, its edge nearer the text on a whole
    /// point, and a border stands out of the column by the offset.
    fn block_border(&mut self, block: &PlacedBlock) {
        let border = &block.border;
        if border.sides == BorderSides::None {
            return;
        }
        let width = border.width.to_pt() as f32;
        let offset = border.offset.to_pt() as f32;
        let dashes: Vec<f32> = border
            .style
            .dashes(f64::from(width))
            .into_iter()
            .map(|d| d as f32)
            .collect();
        let above = block.first && border.sides.above();
        let below = block.last && border.sides.below();
        let top = block.top.round() + width / 2.0;
        let bottom = (block.text_bottom + offset).round() + width / 2.0;
        let mut stroke = |points: Vec<(f32, f32)>, closed: bool| {
            self.items.push(Item::Stroke {
                points,
                closed,
                width,
                color: border.color,
                dashes: dashes.clone(),
            });
        };
        if border.sides != BorderSides::Box {
            if above {
                stroke(vec![(block.left, top), (block.right, top)], false);
            }
            if below {
                stroke(vec![(block.left, bottom), (block.right, bottom)], false);
            }
            return;
        }
        let x0 = block.left - offset - width / 2.0;
        let x1 = block.right + offset + width / 2.0;
        let y0 = if above { top } else { block.top };
        let y1 = if below { bottom } else { block.bottom };
        match (above, below) {
            (true, true) => stroke(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true),
            (true, false) => stroke(vec![(x0, y1), (x0, y0), (x1, y0), (x1, y1)], false),
            (false, true) => stroke(vec![(x0, y0), (x0, y1), (x1, y1), (x1, y0)], false),
            (false, false) => {
                stroke(vec![(x0, y0), (x0, y1)], false);
                stroke(vec![(x1, y0), (x1, y1)], false);
            }
        }
    }

    /// The display lists of every page of a laid out document.
    pub fn pages(layout: &DocumentLayout) -> Vec<Self> {
        layout
            .pages
            .iter()
            .map(|page| Self::from_page(layout, page))
            .collect()
    }
}

/// The items that draw an object: its shape, picture or text, with its
/// rotation, flips and opacity (OBJ-01, OBJ-08, OBJ-13).
fn object_item(placed: &PlacedObject) -> Item {
    let object = &placed.object;
    let (x, y, w, h) = (placed.x, placed.y, placed.width, placed.height);
    let mut list = DisplayList::default();
    let style = &object.style;
    let fill = style.fill.map(|fill| match fill {
        Fill::Color(color) => color,
        _ => Rgba::BLACK,
    });
    let stroke = style.stroke.map(|stroke| {
        let width = stroke.width.to_pt() as f32;
        PathStroke {
            width,
            color: stroke.color,
            dashes: stroke
                .style
                .dashes(f64::from(width))
                .into_iter()
                .map(|d| d as f32)
                .collect(),
        }
    });
    let mut flips = true;
    match &object.kind {
        ObjectKind::Shape(shape) => {
            let line = matches!(shape, Shape::Line { .. });
            let path = Item::Path {
                path: shape_path(shape, x, y, w, h),
                fill: if line { None } else { fill },
                stroke,
            };
            if placed.lines.is_empty() {
                list.items.push(path);
            } else {
                // The shape is flipped around its center, and its text
                // is not (OBJ-08).
                flips = false;
                let (cx, cy) = placed.center();
                let sx = if object.flip_horizontal { -1.0 } else { 1.0 };
                let sy = if object.flip_vertical { -1.0 } else { 1.0 };
                list.items.push(Item::Group {
                    transform: [sx, 0.0, 0.0, sy, cx - sx * cx, cy - sy * cy],
                    opacity: 1.0,
                    items: vec![path],
                });
                list.lines(placed.lines.iter());
            }
        }
        ObjectKind::Image(picture) => {
            list.items.push(Item::Image {
                picture: picture.clone(),
                x,
                y,
                width: w,
                height: h,
            });
            if stroke.is_some() {
                list.items.push(Item::Path {
                    path: shape_path(&Shape::Rectangle, x, y, w, h),
                    fill: None,
                    stroke,
                });
            }
        }
        ObjectKind::TextBox(_) => {
            // Text is never drawn mirrored.
            flips = false;
            if fill.is_some() || stroke.is_some() {
                list.items.push(Item::Path {
                    path: shape_path(&Shape::Rectangle, x, y, w, h),
                    fill,
                    stroke,
                });
            }
            list.lines(placed.lines.iter());
        }
        ObjectKind::Group(_) => {
            list.items.extend(placed.children.iter().map(object_item));
        }
        _ => {}
    }
    // Rotated counterclockwise around the center, and flipped, on a page
    // whose heights go down.
    let (cx, cy) = placed.center();
    let (sin, cos) = (-placed.angle()).sin_cos();
    let sx = if flips && object.flip_horizontal {
        -1.0
    } else {
        1.0
    };
    let sy = if flips && object.flip_vertical {
        -1.0
    } else {
        1.0
    };
    let (xx, yx, xy, yy) = (cos * sx, sin * sx, -sin * sy, cos * sy);
    let transform = [
        xx,
        yx,
        xy,
        yy,
        cx - (xx * cx + xy * cy),
        cy - (yx * cx + yy * cy),
    ];
    Item::Group {
        transform,
        opacity: f32::from(style.opacity.min(100)) / 100.0,
        items: list.items,
    }
}

/// How far the control points of a Bézier curve that draws a quarter of a
/// circle are from its ends, for a radius of one.
const KAPPA: f32 = 0.552_284_8;

/// The path of a shape in a frame.
pub fn shape_path(shape: &Shape, x: f32, y: f32, w: f32, h: f32) -> Vec<PathOp> {
    use PathOp::{Close, Curve, Line, Move};
    match shape {
        Shape::Rectangle => vec![
            Move(x, y),
            Line(x + w, y),
            Line(x + w, y + h),
            Line(x, y + h),
            Close,
        ],
        Shape::RoundedRectangle { radius } => {
            let r = (radius.to_pt() as f32).clamp(0.0, w.min(h) / 2.0);
            let k = r * (1.0 - KAPPA);
            let (x1, y1) = (x + w, y + h);
            vec![
                Move(x + r, y),
                Line(x1 - r, y),
                Curve(x1 - k, y, x1, y + k, x1, y + r),
                Line(x1, y1 - r),
                Curve(x1, y1 - k, x1 - k, y1, x1 - r, y1),
                Line(x + r, y1),
                Curve(x + k, y1, x, y1 - k, x, y1 - r),
                Line(x, y + r),
                Curve(x, y + k, x + k, y, x + r, y),
                Close,
            ]
        }
        Shape::Ellipse => {
            let (rx, ry) = (w / 2.0, h / 2.0);
            let (cx, cy) = (x + rx, y + ry);
            let (kx, ky) = (rx * KAPPA, ry * KAPPA);
            vec![
                Move(cx + rx, cy),
                Curve(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry),
                Curve(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy),
                Curve(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry),
                Curve(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy),
                Close,
            ]
        }
        Shape::Polygon(points) => {
            let scale = POLYGON_SCALE as f32;
            let mut path: Vec<PathOp> = points
                .iter()
                .enumerate()
                .map(|(i, &(px, py))| {
                    let (px, py) = (x + w * px as f32 / scale, y + h * py as f32 / scale);
                    if i == 0 { Move(px, py) } else { Line(px, py) }
                })
                .collect();
            if !path.is_empty() {
                path.push(Close);
            }
            path
        }
        Shape::Line { rising } => {
            if *rising {
                vec![Move(x, y + h), Line(x + w, y)]
            } else {
                vec![Move(x, y), Line(x + w, y + h)]
            }
        }
        _ => shape_path(&Shape::Rectangle, x, y, w, h),
    }
}
