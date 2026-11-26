// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The display list: what a page draws, in page coordinates.

use desk_layout::{DocumentLayout, Page, PlacedBlock, PlacedLine, PlacedObject};
use desk_model::{
    BorderSides, Fill, FrameStyle, Gradient, ImageFill, LineEnd, LineStyle, ObjectKind, Picture,
    PictureFrame, Rgba, Shape, Wrap,
};
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
        /// A gradient or picture that fills the glyphs in place of the
        /// color, which stays for the text of a PDF (PXV-04).
        paint: Option<Paint>,
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
    /// A picture with a color taken out or adjusted, scaled into a
    /// rectangle (OBJ-06, OBJ-07).
    AdjustedImage {
        /// The picture.
        picture: desk_model::Picture,
        /// The color taken out of it.
        alpha: std::sync::Arc<[desk_model::AlphaSeed]>,
        /// The part of it that Instant Alpha keeps, as Pages saved it.
        kept: Option<desk_model::AlphaOutline>,
        /// The changes of its colors.
        adjust: desk_model::Adjustments,
        /// The left edge, top edge, width and height.
        rect: (f32, f32, f32, f32),
    },
    /// Items drawn only inside a rectangle, as a picture inside its mask
    /// (OBJ-04).
    Clipped {
        /// The left edge, top edge, width and height.
        rect: (f32, f32, f32, f32),
        /// The items.
        items: Vec<Item>,
    },
    /// Items drawn only inside a path, as a picture inside a mask of
    /// another shape (OBJ-04).
    ClippedTo {
        /// The path.
        path: Vec<PathOp>,
        /// The items.
        items: Vec<Item>,
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
        /// What fills it; none when it is not filled.
        fill: Option<Paint>,
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
    /// The blurred shadow that items cast, offset from them, without the
    /// items themselves (OBJ-11).
    CastShadow {
        /// The items that cast it.
        items: Vec<Item>,
        /// How far it falls right and down.
        offset: (f32, f32),
        /// The radius of the blur.
        blur: f32,
        /// The color, with the opacity of the shadow.
        color: Rgba,
    },
    /// Items reflected upside down below where they draw, fading from an
    /// opacity at their foot to nothing (OBJ-38).
    Reflection {
        /// The items reflected.
        items: Vec<Item>,
        /// How opaque the reflection is at the foot of the items, from 0
        /// to 1.
        opacity: f32,
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

/// What fills a path.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// One color.
    Color(Rgba),
    /// A color that turns into another along a line, from its start to
    /// its end, and stays the same across it (OBJ-09).
    Linear {
        /// Where the first color is.
        start: (f32, f32),
        /// Where the last color is.
        end: (f32, f32),
        /// The colors, by where they are from 0 at the start to 1 at the
        /// end (OBJ-41).
        stops: Vec<(f32, Rgba)>,
    },
    /// Colors that turn into each other in circles out from a point
    /// (OBJ-41).
    Radial {
        /// The point.
        center: (f32, f32),
        /// How far the last color is from it.
        radius: f32,
        /// The colors, by where they are from 0 at the point to 1 at the
        /// radius.
        stops: Vec<(f32, Rgba)>,
    },
    /// A picture, at a place and size, and over and over from there when
    /// it tiles (OBJ-09).
    Image {
        /// The picture.
        picture: Picture,
        /// Where one copy of it is: left, top, width and height.
        rect: (f32, f32, f32, f32),
        /// Whether it repeats beyond that rectangle.
        tile: bool,
        /// A color laid over it, with its opacity (OBJ-42).
        tint: Option<Rgba>,
    },
}

/// How a gradient fills a frame (OBJ-09, OBJ-41).
fn gradient_paint(gradient: &Gradient, frame: (f32, f32, f32, f32)) -> Paint {
    let (x, y, w, h) = frame;
    let stops = gradient
        .color_stops()
        .into_iter()
        .map(|(at, color)| (at as f32, color))
        .collect();
    if let Some((cx, cy)) = gradient.center {
        // Out from the point, as far as the farthest corner.
        let cx = x + w * f32::from(cx.min(1000)) / 1000.0;
        let cy = y + h * f32::from(cy.min(1000)) / 1000.0;
        let radius = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
            .into_iter()
            .map(|(px, py)| (px - cx).hypot(py - cy))
            .fold(0.0_f32, f32::max);
        return Paint::Radial {
            center: (cx, cy),
            radius: radius.max(0.5),
            stops,
        };
    }
    // Across the frame, through its center, as far as its corners reach
    // along the way it goes.
    let a = (gradient.angle as f32 / 100.0).to_radians();
    let (dx, dy) = (a.cos(), -a.sin());
    let half = (w * dx.abs() + h * dy.abs()) / 2.0;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    Paint::Linear {
        start: (cx - dx * half, cy - dy * half),
        end: (cx + dx * half, cy + dy * half),
        stops,
    }
}

/// What paints a fill over a frame: left, top, width and height.
fn fill_paint(fill: Fill, frame: (f32, f32, f32, f32)) -> Paint {
    match fill {
        Fill::Color(color) => Paint::Color(color),
        Fill::Gradient { from, to, angle } => {
            gradient_paint(&Gradient::linear(from, to, angle), frame)
        }
        Fill::Advanced(gradient) => gradient_paint(&gradient, frame),
        Fill::Image {
            picture,
            mode,
            tint,
        } => match image_paint(picture, mode, frame) {
            Paint::Image {
                picture,
                rect,
                tile,
                ..
            } => Paint::Image {
                picture,
                rect,
                tile,
                tint,
            },
            other => other,
        },
        _ => Paint::Color(Rgba::BLACK),
    }
}

/// The frames that the fills of the runs of a line span: each run with a
/// fill takes the stretch of the line from the first to the last of the
/// runs beside it that share its fill, over the height of the line, so
/// that a gradient goes once across a word whatever its formatting
/// (PXV-04).
fn fill_frames(
    runs: &[desk_text::GlyphRun],
    top: f32,
    bottom: f32,
) -> Vec<Option<(f32, f32, f32, f32)>> {
    let mut frames = vec![None; runs.len()];
    let mut i = 0;
    while i < runs.len() {
        let Some(fill) = &runs[i].fill else {
            i += 1;
            continue;
        };
        let mut j = i + 1;
        while j < runs.len() && runs[j].fill.as_ref() == Some(fill) {
            j += 1;
        }
        let left = runs[i..j].iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
        let right = runs[i..j]
            .iter()
            .map(|r| r.x + r.advance)
            .fold(f32::NEG_INFINITY, f32::max);
        let frame = (left, top, (right - left).max(0.5), (bottom - top).max(0.5));
        for slot in &mut frames[i..j] {
            *slot = Some(frame);
        }
        i = j;
    }
    frames
}

/// A frame of a fill: left, top, width and height.
type FillFrame = (f32, f32, f32, f32);

/// Spreads the frames of fills that go on from one line of a paragraph
/// to the next over all those lines: Pages 15.4 runs one gradient once
/// from the top line to the bottom line of the text that has it, and
/// across the widest of them, whether that text is a word or the lines
/// of a paragraph (PXV-04, W052).
fn spread_fills(lines: &[&PlacedLine], fills: &mut [Vec<Option<FillFrame>>]) {
    // The lines and runs of the fill being followed, and their frame.
    let mut chain: Vec<(usize, usize)> = Vec::new();
    let mut union: Option<FillFrame> = None;
    let close = |chain: &mut Vec<(usize, usize)>,
                 union: &mut Option<FillFrame>,
                 fills: &mut [Vec<Option<FillFrame>>]| {
        if let Some(frame) = union.take()
            && chain
                .iter()
                .map(|&(l, _)| l)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 1
        {
            for &(l, r) in chain.iter() {
                fills[l][r] = Some(frame);
            }
        }
        chain.clear();
    };
    for k in 0..lines.len() {
        let runs = &lines[k].line.runs;
        let first = runs.first().and_then(|r| r.fill.as_ref());
        let goes_on = k > 0
            && !chain.is_empty()
            && lines[k - 1].paragraph == lines[k].paragraph
            && lines[k - 1].origin == lines[k].origin
            && lines[k - 1].line.runs.last().and_then(|r| r.fill.as_ref()) == first
            && first.is_some();
        if goes_on {
            // The runs at the start of the line that share the fill.
            let lead = fills[k][0];
            for (r, frame) in fills[k].iter().enumerate() {
                if *frame != lead {
                    break;
                }
                chain.push((k, r));
            }
            if let (Some(u), Some(f)) = (union, lead) {
                let left = u.0.min(f.0);
                let right = (u.0 + u.2).max(f.0 + f.2);
                let bottom = (u.1 + u.3).max(f.1 + f.3);
                union = Some((left, u.1, right - left, bottom - u.1));
            }
        } else {
            close(&mut chain, &mut union, fills);
        }
        // The runs at the end of the line that share the fill go on.
        let ends_filled = runs.last().is_some_and(|r| r.fill.is_some());
        if !ends_filled {
            close(&mut chain, &mut union, fills);
            continue;
        }
        let tail = fills[k][runs.len() - 1];
        if chain.last().is_none_or(|&(l, _)| l != k) {
            // A new chain from the runs at the end of this line.
            close(&mut chain, &mut union, fills);
            union = tail;
            for (r, frame) in fills[k].iter().enumerate().rev() {
                if *frame != tail {
                    break;
                }
                chain.push((k, r));
            }
        } else if fills[k].iter().any(|f| *f != tail) {
            // The fill stops in this line and another one ends it.
            close(&mut chain, &mut union, fills);
            union = tail;
            for (r, frame) in fills[k].iter().enumerate().rev() {
                if *frame != tail {
                    break;
                }
                chain.push((k, r));
            }
        }
    }
    close(&mut chain, &mut union, fills);
}

/// Where a picture that fills a frame lies, as the mode sizes it, a pixel
/// to a point at its own size (OBJ-09).
fn image_paint(picture: Picture, mode: ImageFill, frame: (f32, f32, f32, f32)) -> Paint {
    let (x, y, w, h) = frame;
    let (pw, ph) = (picture.width() as f32, picture.height() as f32);
    let centered =
        |width: f32, height: f32| (x + (w - width) / 2.0, y + (h - height) / 2.0, width, height);
    let (rect, tile) = match mode {
        ImageFill::Stretch => ((x, y, w, h), false),
        ImageFill::ScaleToFill | ImageFill::ScaleToFit => {
            let (sx, sy) = (w / pw, h / ph);
            let scale = if mode == ImageFill::ScaleToFill {
                sx.max(sy)
            } else {
                sx.min(sy)
            };
            (centered(pw * scale, ph * scale), false)
        }
        ImageFill::OriginalSize => (centered(pw, ph), false),
        ImageFill::Tile => ((x, y, pw, ph), true),
    };
    Paint::Image {
        picture,
        rect,
        tile,
        tint: None,
    }
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
        // The background of the section, behind all else (W098).
        if let Some(fill) = &page.background {
            let (w, h) = (list.width, list.height);
            list.items.push(Item::Path {
                path: vec![
                    PathOp::Move(0.0, 0.0),
                    PathOp::Line(w, 0.0),
                    PathOp::Line(w, h),
                    PathOp::Line(0.0, h),
                    PathOp::Close,
                ],
                fill: Some(fill_paint(fill.clone(), (0.0, 0.0, w, h))),
                stroke: None,
            });
        }
        // The objects of the master of the section, behind them
        // (INT-05).
        for object in page.objects.iter().filter(|o| o.object.anchor.is_master()) {
            list.items.push(object_item(object));
        }
        for block in &page.blocks {
            list.block_background(block);
        }
        // Objects behind the text (OBJ-17).
        for object in page
            .objects
            .iter()
            .filter(|o| o.object.wrap == Wrap::Behind && !o.object.anchor.is_master())
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
            .filter(|o| o.object.wrap != Wrap::Behind && !o.object.anchor.is_master())
        {
            list.items.push(object_item(object));
        }
        list
    }

    /// The items of lines placed on the page: their backgrounds, glyphs,
    /// pictures and lines.
    fn lines<'a>(&mut self, lines: impl Iterator<Item = &'a PlacedLine>) {
        let list = self;
        let lines: Vec<&PlacedLine> = lines.collect();
        let mut fills: Vec<_> = lines
            .iter()
            .map(|p| {
                let dy = p.origin.1;
                fill_frames(&p.line.runs, p.top() - dy, p.bottom() - dy)
            })
            .collect();
        spread_fills(&lines, &mut fills);
        // A gradient applied to the entire page runs across the page,
        // whatever text has it (W052).
        if list.width > 0.0 && list.height > 0.0 {
            for (placed, frames) in lines.iter().zip(&mut fills) {
                let (dx, dy) = placed.origin;
                for (run, frame) in placed.line.runs.iter().zip(frames.iter_mut()) {
                    if run.fill_page && frame.is_some() {
                        *frame = Some((-dx, -dy, list.width, list.height));
                    }
                }
            }
        }
        for (placed, frames) in lines.into_iter().zip(fills) {
            let (dx, dy) = placed.origin;
            // The backgrounds of runs span the height of the line, as
            // Pages 15.4 draws them (FMT-20, B-020).
            let top = placed.top();
            let bottom = placed.bottom();
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
            // The shape behind the letters of a drop cap (PXV-06).
            if let Some(cap) = &placed.line.drop_cap_shape {
                let border = cap.shape.border_width.to_pt() as f32;
                list.items.push(Item::Path {
                    path: shape_path(
                        &Shape::RoundedRectangle {
                            radius: desk_model::Length::from_pt(f64::from(cap.radius)),
                        },
                        cap.x + dx,
                        cap.y + dy,
                        cap.width,
                        cap.height,
                    ),
                    fill: Some(Paint::Color(cap.shape.fill)),
                    stroke: (border > 0.0).then(|| PathStroke {
                        width: border,
                        color: cap.shape.border_color,
                        dashes: cap
                            .shape
                            .border_style
                            .dashes(f64::from(border))
                            .into_iter()
                            .map(|d| d as f32)
                            .collect(),
                    }),
                });
            }
            for (run, frame) in placed.line.runs.iter().zip(frames) {
                if run.glyphs.is_empty() {
                    continue;
                }
                let glyphs: Vec<PlacedGlyph> = run
                    .glyphs
                    .iter()
                    .map(|g| PlacedGlyph {
                        id: g.id,
                        x: g.x + dx,
                        // To a 64th of a point, so that the runs of a line
                        // share one baseline whatever their sums of floats,
                        // and a PDF reads them as one line (OBJ-02).
                        y: ((g.y + dy) * 64.0).round() / 64.0,
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
                    paint: run
                        .fill
                        .clone()
                        .zip(frame)
                        .map(|(fill, (x, y, w, h))| fill_paint(fill, (x + dx, y + dy, w, h))),
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

impl DisplayList {
    /// The display list of an object alone, as large as its frame, as the
    /// outline shows it in miniature (TXT-14).
    pub fn of_object(placed: &PlacedObject) -> Self {
        let mut alone = placed.clone();
        alone.x = 0.0;
        alone.y = 0.0;
        Self {
            width: placed.width.max(1.0),
            height: placed.height.max(1.0),
            items: vec![object_item(&alone)],
        }
    }
}

/// The items that draw an object: its shape, picture or text, with its
/// rotation, flips and opacity (OBJ-01, OBJ-08, OBJ-13).
fn object_item(placed: &PlacedObject) -> Item {
    let object = &placed.object;
    let (x, y, w, h) = (placed.x, placed.y, placed.width, placed.height);
    let mut list = DisplayList::default();
    let style = &object.style;
    let fill = style
        .fill
        .clone()
        .map(|fill| fill_paint(fill, (x, y, w, h)));
    // A picture frame takes the place of the line (OBJ-36).
    let stroke = style
        .stroke
        .filter(|_| style.frame.is_none())
        .map(|stroke| {
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
        ObjectKind::Shape(Shape::Line { rising }) => {
            let (y0, y1) = if *rising { (y + h, y) } else { (y, y + h) };
            if let (Some(stroke), Some(model)) = (stroke, style.stroke) {
                list.items.extend(line_items(
                    (x, y0),
                    (x + w, y1),
                    None,
                    stroke,
                    model.start,
                    model.end,
                ));
            }
        }
        ObjectKind::Shape(Shape::Connection(line)) => {
            let (a, b) = line.ends_in(f64::from(w), f64::from(h));
            let at = |(px, py): (f64, f64)| (x + px as f32, y + py as f32);
            let controls =
                desk_model::bent_controls(a, b, line.bend.to_pt()).map(|(c1, c2)| (at(c1), at(c2)));
            if let (Some(stroke), Some(model)) = (stroke, style.stroke) {
                list.items.extend(line_items(
                    at(a),
                    at(b),
                    controls,
                    stroke,
                    model.start,
                    model.end,
                ));
            }
        }
        ObjectKind::Shape(shape) => {
            let path = Item::Path {
                path: shape_path(shape, x, y, w, h),
                fill,
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
            let settings = &object.picture;
            // Where the whole picture lies; the frame shows a part of it
            // when it masks it.
            let rect = settings.mask.map_or((x, y, w, h), |m| {
                (
                    x + m.x.to_pt() as f32,
                    y + m.y.to_pt() as f32,
                    m.width.to_pt() as f32,
                    m.height.to_pt() as f32,
                )
            });
            let image = if !settings.alpha.is_empty()
                || settings.alpha_kept.is_some()
                || !settings.adjust.is_none()
            {
                Item::AdjustedImage {
                    picture: picture.clone(),
                    alpha: settings.alpha.clone(),
                    kept: settings.alpha_kept.clone(),
                    adjust: settings.adjust,
                    rect,
                }
            } else {
                Item::Image {
                    picture: picture.clone(),
                    x: rect.0,
                    y: rect.1,
                    width: rect.2,
                    height: rect.3,
                }
            };
            // The outline of the mask: a rectangle, or the shape of Mask
            // with Shape (OBJ-04).
            let outline = settings
                .mask_shape
                .as_ref()
                .filter(|_| settings.mask.is_some())
                .map(|shape| shape_path(shape, x, y, w, h));
            list.items.push(match (&outline, settings.mask.is_some()) {
                (Some(path), _) => Item::ClippedTo {
                    path: path.clone(),
                    items: vec![image],
                },
                (None, true) => Item::Clipped {
                    rect: (x, y, w, h),
                    items: vec![image],
                },
                (None, false) => image,
            });
            if stroke.is_some() {
                list.items.push(Item::Path {
                    path: outline.unwrap_or_else(|| shape_path(&Shape::Rectangle, x, y, w, h)),
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
            // As in Pages, the lines that run past the bottom of the box
            // are not drawn, in the last of linked boxes too (OBJ-13).
            list.lines(
                placed
                    .lines
                    .iter()
                    .filter(|line| line.top() + line.line.height <= y + h + 0.5),
            );
        }
        ObjectKind::Group(_) => {
            list.items.extend(placed.children.iter().map(object_item));
        }
        ObjectKind::Table(table) => {
            flips = false;
            if let Some(laid) = &placed.table {
                list.items.extend(table_items(table, laid, x, y));
            }
        }
        ObjectKind::Chart(_) => {
            if fill.is_some() || stroke.is_some() {
                list.items.push(Item::Path {
                    path: shape_path(&Shape::Rectangle, x, y, w, h),
                    fill: fill.clone(),
                    stroke: stroke.clone(),
                });
            }
            if let Some(laid) = &placed.chart {
                list.items.extend(chart_items(laid, x, y));
            }
        }
        ObjectKind::Equation(_) => {
            if fill.is_some() || stroke.is_some() {
                list.items.push(Item::Path {
                    path: shape_path(&Shape::Rectangle, x, y, w, h),
                    fill: fill.clone(),
                    stroke: stroke.clone(),
                });
            }
            if let Some(laid) = &placed.equation {
                list.items.extend(equation_items(laid, x, y, w, h));
            }
        }
        ObjectKind::Media(media) => {
            match &media.poster {
                Some(poster) if !media.is_sound() => list.items.push(Item::Image {
                    picture: poster.clone(),
                    x,
                    y,
                    width: w,
                    height: h,
                }),
                _ => list.items.extend(media_icon(media.is_sound(), x, y, w, h)),
            }
            if stroke.is_some() {
                list.items.push(Item::Path {
                    path: shape_path(&Shape::Rectangle, x, y, w, h),
                    fill: None,
                    stroke,
                });
            }
        }
        _ => {}
    }
    if let Some(frame) = style.frame
        && !matches!(&object.kind, ObjectKind::Shape(shape) if shape.is_line())
    {
        list.items.extend(frame_items(frame, (x, y, w, h)));
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
    let opacity = f32::from(style.opacity.min(100)) / 100.0;
    let object = Item::Group {
        transform,
        opacity: 1.0,
        items: list.items,
    };
    let mut items = Vec::new();
    // The shadow falls on the page the same way however the object is
    // turned, and fades with it.
    if let Some(shadow) = style.shadow {
        let (dx, dy) = shadow.offset_xy();
        let mut color = shadow.color;
        color.a = (u16::from(color.a) * u16::from(shadow.opacity.min(100)) / 100) as u8;
        items.push(Item::CastShadow {
            items: vec![object.clone()],
            offset: (dx as f32, dy as f32),
            blur: shadow.blur.to_pt() as f32,
            color,
        });
    }
    // The reflection, below the object as it stands on the page.
    if let Some(reflection) = style.reflection.filter(|r| *r > 0) {
        items.push(Item::Reflection {
            items: vec![object.clone()],
            opacity: f32::from(reflection.min(100)) / 100.0,
        });
    }
    if items.is_empty() {
        let Item::Group { items, .. } = object else {
            unreachable!("the object is a group")
        };
        return Item::Group {
            transform,
            opacity,
            items,
        };
    }
    items.push(object);
    Item::Group {
        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        opacity,
        items,
    }
}

/// A band between two rectangles, the outer one `out` beyond the frame
/// and the inner one `inside` within it: the outer clockwise and the inner
/// the other way, so that the inner one is a hole.
fn band(frame: (f32, f32, f32, f32), out: f32, inside: f32) -> Vec<PathOp> {
    let (x, y, w, h) = frame;
    let (x0, y0, x1, y1) = (x - out, y - out, x + w + out, y + h + out);
    let (i0, j0) = (x + inside, y + inside);
    let (i1, j1) = ((x + w - inside).max(i0), (y + h - inside).max(j0));
    vec![
        PathOp::Move(x0, y0),
        PathOp::Line(x1, y0),
        PathOp::Line(x1, y1),
        PathOp::Line(x0, y1),
        PathOp::Close,
        PathOp::Move(i0, j0),
        PathOp::Line(i0, j1),
        PathOp::Line(i1, j1),
        PathOp::Line(i1, j0),
        PathOp::Close,
    ]
}

/// The items that draw a picture frame of the library around a frame
/// (OBJ-36), out from its edges.
fn frame_items(frame: PictureFrame, rect: (f32, f32, f32, f32)) -> Vec<Item> {
    let (x, y, w, h) = rect;
    let d = frame.width().to_pt() as f32;
    let fill = |path: Vec<PathOp>, paint: Paint| Item::Path {
        path,
        fill: Some(paint),
        stroke: None,
    };
    let line = |at: f32, width: f32, color: Rgba| Item::Path {
        path: shape_path(
            &Shape::Rectangle,
            x - at,
            y - at,
            w + 2.0 * at,
            h + 2.0 * at,
        ),
        fill: None,
        stroke: Some(PathStroke {
            width,
            color,
            dashes: Vec::new(),
        }),
    };
    // Lit from the top left, across the band.
    let across = |stops: &[(f32, Rgba)]| Paint::Linear {
        start: (x - d, y - d),
        end: (x + w + d, y + h + d),
        stops: stops.to_vec(),
    };
    let mut items = Vec::new();
    match frame.style {
        FrameStyle::Photo | FrameStyle::Tape => {
            // A soft shadow below and right of the print.
            let s = (d / 4.0).max(1.0);
            // Only beyond the print, which it falls from.
            let mut shadow = band((x + s, y + s, w, h), d, 0.0);
            shadow.truncate(5);
            shadow.extend(
                band((x - d, y - d, w + 2.0 * d, h + 2.0 * d), 0.0, 0.0)[5..]
                    .iter()
                    .copied(),
            );
            items.push(fill(
                shadow,
                Paint::Color(Rgba {
                    a: 40,
                    ..Rgba::BLACK
                }),
            ));
            items.push(fill(band(rect, d, 0.0), Paint::Color(Rgba::WHITE)));
            items.push(line(d, 0.5, Rgba::rgb(0xb0, 0xb0, 0xb0)));
            items.push(line(0.0, 0.5, Rgba::rgb(0xd0, 0xd0, 0xd0)));
            if frame.style == FrameStyle::Tape {
                // A strip across each top corner, along its diagonal.
                let tape = Rgba {
                    a: 170,
                    ..Rgba::rgb(0xf3, 0xe9, 0xc6)
                };
                let (half_long, half_wide) = (d * 2.0, d * 0.8);
                let r = std::f32::consts::FRAC_1_SQRT_2;
                for (cx, cy, dir) in [
                    (x - d / 2.0, y - d / 2.0, 1.0),
                    (x + w + d / 2.0, y - d / 2.0, -1.0),
                ] {
                    let (ax, ay) = (dir * r, -r);
                    let (nx, ny) = (-ay, ax);
                    let corner = |a: f32, b: f32| (cx + ax * a + nx * b, cy + ay * a + ny * b);
                    let points = [
                        corner(-half_long, -half_wide),
                        corner(half_long, -half_wide),
                        corner(half_long, half_wide),
                        corner(-half_long, half_wide),
                    ];
                    let mut path = vec![PathOp::Move(points[0].0, points[0].1)];
                    path.extend(points[1..].iter().map(|p| PathOp::Line(p.0, p.1)));
                    path.push(PathOp::Close);
                    items.push(fill(path, Paint::Color(tape)));
                }
            }
        }
        FrameStyle::Gallery => {
            let edge = (d * 0.15).max(1.0);
            items.push(fill(band(rect, d, 0.0), Paint::Color(Rgba::WHITE)));
            items.push(fill(
                band(rect, d, -(d - edge)),
                Paint::Color(Rgba::rgb(0x1a, 0x1a, 0x1a)),
            ));
            items.push(line(0.0, 0.75, Rgba::rgb(0xc8, 0xc8, 0xc8)));
        }
        FrameStyle::Classic => {
            let gold = (d * 0.18).max(1.0);
            items.push(fill(
                band(rect, d, -gold),
                across(&[
                    (0.0, Rgba::rgb(0x7a, 0x4a, 0x24)),
                    (0.45, Rgba::rgb(0x4a, 0x2a, 0x12)),
                    (1.0, Rgba::rgb(0x2e, 0x18, 0x0a)),
                ]),
            ));
            items.push(fill(
                band(rect, gold, 0.0),
                across(&[
                    (0.0, Rgba::rgb(0xf0, 0xd0, 0x7a)),
                    (1.0, Rgba::rgb(0xa8, 0x80, 0x30)),
                ]),
            ));
            items.push(line(d, 0.75, Rgba::rgb(0x1e, 0x0e, 0x04)));
        }
        FrameStyle::Metal => {
            items.push(fill(
                band(rect, d, 0.0),
                across(&[
                    (0.0, Rgba::rgb(0xf4, 0xf4, 0xf4)),
                    (0.35, Rgba::rgb(0xa8, 0xa8, 0xa8)),
                    (0.6, Rgba::rgb(0xe0, 0xe0, 0xe0)),
                    (1.0, Rgba::rgb(0x80, 0x80, 0x80)),
                ]),
            ));
            items.push(line(d, 0.75, Rgba::rgb(0x60, 0x60, 0x60)));
            items.push(line(0.0, 0.75, Rgba::rgb(0x50, 0x50, 0x50)));
        }
    }
    items
}

/// The items that draw a line from one point to another with its ends
/// (OBJ-10): the line stops short of a filled arrowhead, which ends where
/// the line would.
fn line_items(
    from: (f32, f32),
    to: (f32, f32),
    controls: Option<((f32, f32), (f32, f32))>,
    stroke: PathStroke,
    start: LineEnd,
    end: LineEnd,
) -> Vec<Item> {
    // The way out of each end, along the curve when it is bent.
    let unit = |from: (f32, f32), to: (f32, f32)| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length > 0.0 {
            (dx / length, dy / length)
        } else {
            (1.0, 0.0)
        }
    };
    let (start_way, end_way) = match controls {
        Some((c1, c2)) => (unit(from, c1), unit(c2, to)),
        None => (unit(from, to), unit(from, to)),
    };
    // As large as Pages draws them beside a line of a width.
    let size = (stroke.width * 3.0).max(6.0);
    let mut items = Vec::new();
    let mut ends = |at: (f32, f32), dir: (f32, f32), kind: LineEnd| -> (f32, f32) {
        use PathOp::{Close, Curve, Line, Move};
        let (px, py) = (-dir.1, dir.0);
        let back = |d: f32| (at.0 - dir.0 * d, at.1 - dir.1 * d);
        let filled = |path| Item::Path {
            path,
            fill: Some(Paint::Color(stroke.color)),
            stroke: None,
        };
        let plain = PathStroke {
            dashes: Vec::new(),
            ..stroke.clone()
        };
        match kind {
            LineEnd::None => at,
            LineEnd::Arrow => {
                let base = back(size * 1.5);
                items.push(filled(vec![
                    Move(at.0, at.1),
                    Line(base.0 + px * size / 2.0, base.1 + py * size / 2.0),
                    Line(base.0 - px * size / 2.0, base.1 - py * size / 2.0),
                    Close,
                ]));
                back(size * 1.5 - stroke.width / 2.0)
            }
            LineEnd::OpenArrow => {
                let base = back(size * 1.5);
                items.push(Item::Path {
                    path: vec![
                        Move(base.0 + px * size / 2.0, base.1 + py * size / 2.0),
                        Line(at.0, at.1),
                        Line(base.0 - px * size / 2.0, base.1 - py * size / 2.0),
                    ],
                    fill: None,
                    stroke: Some(plain),
                });
                at
            }
            LineEnd::Circle => {
                let r = size / 2.0;
                let k = r * KAPPA;
                let (cx, cy) = at;
                items.push(filled(vec![
                    Move(cx + r, cy),
                    Curve(cx + r, cy + k, cx + k, cy + r, cx, cy + r),
                    Curve(cx - k, cy + r, cx - r, cy + k, cx - r, cy),
                    Curve(cx - r, cy - k, cx - k, cy - r, cx, cy - r),
                    Curve(cx + k, cy - r, cx + r, cy - k, cx + r, cy),
                    Close,
                ]));
                at
            }
            LineEnd::Square => {
                let r = size / 2.0;
                let corner = |a: f32, b: f32| {
                    (
                        at.0 + dir.0 * r * a + px * r * b,
                        at.1 + dir.1 * r * a + py * r * b,
                    )
                };
                let points = [
                    corner(1.0, 1.0),
                    corner(-1.0, 1.0),
                    corner(-1.0, -1.0),
                    corner(1.0, -1.0),
                ];
                items.push(filled(vec![
                    Move(points[0].0, points[0].1),
                    Line(points[1].0, points[1].1),
                    Line(points[2].0, points[2].1),
                    Line(points[3].0, points[3].1),
                    Close,
                ]));
                at
            }
            LineEnd::Bar => {
                items.push(Item::Path {
                    path: vec![
                        Move(at.0 + px * size / 2.0, at.1 + py * size / 2.0),
                        Line(at.0 - px * size / 2.0, at.1 - py * size / 2.0),
                    ],
                    fill: None,
                    stroke: Some(plain),
                });
                at
            }
        }
    };
    let a = ends(from, (-start_way.0, -start_way.1), start);
    let b = ends(to, end_way, end);
    let way = match controls {
        Some((c1, c2)) => PathOp::Curve(c1.0, c1.1, c2.0, c2.1, b.0, b.1),
        None => PathOp::Line(b.0, b.1),
    };
    let mut out = vec![Item::Path {
        path: vec![PathOp::Move(a.0, a.1), way],
        fill: None,
        stroke: Some(stroke),
    }];
    out.extend(items);
    out
}

/// What stands for a movie without a poster frame, or for a sound: a dark
/// frame with a triangle to play it, or a speaker (OBJ-18).
fn media_icon(sound: bool, x: f32, y: f32, w: f32, h: f32) -> Vec<Item> {
    use PathOp::{Close, Line, Move};
    let filled = |path, color| Item::Path {
        path,
        fill: Some(Paint::Color(color)),
        stroke: None,
    };
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let side = w.min(h);
    if sound {
        // A speaker with its cone to the right, in the middle.
        let s = side * 0.8;
        let (l, t) = (cx - s / 2.0, cy - s / 2.0);
        let gray = Rgba::rgb(0x55, 0x55, 0x55);
        return vec![
            filled(
                vec![
                    Move(l + s * 0.1, t + s * 0.35),
                    Line(l + s * 0.35, t + s * 0.35),
                    Line(l + s * 0.65, t + s * 0.1),
                    Line(l + s * 0.65, t + s * 0.9),
                    Line(l + s * 0.35, t + s * 0.65),
                    Line(l + s * 0.1, t + s * 0.65),
                    Close,
                ],
                gray,
            ),
            Item::Path {
                path: vec![
                    Move(l + s * 0.75, t + s * 0.3),
                    PathOp::Curve(
                        l + s * 0.85,
                        t + s * 0.4,
                        l + s * 0.85,
                        t + s * 0.6,
                        l + s * 0.75,
                        t + s * 0.7,
                    ),
                ],
                fill: None,
                stroke: Some(PathStroke {
                    width: (s * 0.06).max(1.0),
                    color: gray,
                    dashes: Vec::new(),
                }),
            },
        ];
    }
    let r = side * 0.18;
    vec![
        filled(
            vec![
                Move(x, y),
                Line(x + w, y),
                Line(x + w, y + h),
                Line(x, y + h),
                Close,
            ],
            Rgba::rgb(0x2b, 0x2b, 0x2b),
        ),
        filled(
            vec![
                Move(cx - r * 0.6, cy - r),
                Line(cx + r, cy),
                Line(cx - r * 0.6, cy + r),
                Close,
            ],
            Rgba::rgb(0xee, 0xee, 0xee),
        ),
    ]
}

/// How far the control points of a Bézier curve that draws a quarter of a
/// circle are from its ends, for a radius of one.
const KAPPA: f32 = 0.552_284_8;

/// The path of a shape in a frame.
pub fn shape_path(shape: &Shape, x: f32, y: f32, w: f32, h: f32) -> Vec<PathOp> {
    use desk_model::PathSegment;
    let at = |px: f64, py: f64| (x + px as f32, y + py as f32);
    shape
        .outline(f64::from(w), f64::from(h))
        .into_iter()
        .map(|segment| match segment {
            PathSegment::Move(px, py) => {
                let (px, py) = at(px, py);
                PathOp::Move(px, py)
            }
            PathSegment::Line(px, py) => {
                let (px, py) = at(px, py);
                PathOp::Line(px, py)
            }
            PathSegment::Curve(ax, ay, bx, by, cx, cy) => {
                let (ax, ay) = at(ax, ay);
                let (bx, by) = at(bx, by);
                let (cx, cy) = at(cx, cy);
                PathOp::Curve(ax, ay, bx, by, cx, cy)
            }
            PathSegment::Close => PathOp::Close,
        })
        .collect()
}

/// The lines of text of a table or a chart, moved from its frame onto the
/// page.
fn moved_lines(lines: &[PlacedLine], dx: f32, dy: f32) -> Vec<PlacedLine> {
    lines
        .iter()
        .map(|l| PlacedLine {
            origin: (l.origin.0 + dx, l.origin.1 + dy),
            ..l.clone()
        })
        .collect()
}

fn table_color(c: desk_model::table::Color) -> Rgba {
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

/// What a table draws: the fills of its cells, their borders and their
/// text, at a place on the page (TAB-04, TAB-14, TAB-20, TAB-25).
fn table_items(
    table: &desk_model::Table,
    laid: &desk_layout::TableLayout,
    x: f32,
    y: f32,
) -> Vec<Item> {
    let mut list = DisplayList::default();
    for cell in &laid.cells {
        if let Some(color) = cell.fill {
            let (cx, cy, cw, ch) = cell.rect;
            list.items.push(Item::Rect {
                x: x + cx,
                y: y + cy,
                width: cw,
                height: ch,
                color,
            });
        }
    }
    // The borders, by segment, but those inside merged cells.
    let inside = |r: u32, c: u32, horizontal: bool| {
        table.merges().iter().any(|m| {
            if horizontal {
                r > m.row && r < m.row_end() && c >= m.col && c < m.col_end()
            } else {
                c > m.col && c < m.col_end() && r >= m.row && r < m.row_end()
            }
        })
    };
    let stroke = |b: desk_model::table::Border, a: (f32, f32), z: (f32, f32)| {
        let width = b.width as f32 / 1000.0;
        Item::Stroke {
            points: vec![a, z],
            closed: false,
            width,
            color: table_color(b.color),
            dashes: match b.style {
                desk_model::table::LineStyle::Solid => Vec::new(),
                desk_model::table::LineStyle::Dashed => {
                    vec![2.0 * width.max(0.5), 2.0 * width.max(0.5)]
                }
                desk_model::table::LineStyle::Dotted => vec![width.max(0.5), width.max(0.5)],
            },
        }
    };
    for r in 0..=table.rows() {
        for c in 0..table.cols() {
            if let Some(b) = table.h_border(r, c)
                && !inside(r, c, true)
                && b.width > 0
            {
                let yy = y + laid.rows[r as usize];
                list.items.push(stroke(
                    b,
                    (x + laid.cols[c as usize], yy),
                    (x + laid.cols[c as usize + 1], yy),
                ));
            }
        }
    }
    for r in 0..table.rows() {
        for c in 0..=table.cols() {
            if let Some(b) = table.v_border(r, c)
                && !inside(r, c, false)
                && b.width > 0
            {
                let xx = x + laid.cols[c as usize];
                list.items.push(stroke(
                    b,
                    (xx, y + laid.rows[r as usize]),
                    (xx, y + laid.rows[r as usize + 1]),
                ));
            }
        }
    }
    for cell in &laid.cells {
        let (cx, cy, cw, ch) = cell.rect;
        let lines = moved_lines(&cell.lines, x, y);
        if cell.overflow {
            list.items.push(Item::Clipped {
                rect: (x + cx, y + cy, cw, ch),
                items: {
                    let mut inner = DisplayList::default();
                    inner.lines(lines.iter());
                    inner.items
                },
            });
            // The + mark of text that does not fit (TAB-20).
            let (mx, my) = (x + cx + cw - 7.0, y + cy + ch - 7.0);
            let mark = Rgba::rgb(0x77, 0x76, 0x7b);
            list.items.push(Item::Rect {
                x: mx,
                y: my,
                width: 6.0,
                height: 6.0,
                color: Rgba::WHITE,
            });
            list.items.push(Item::Stroke {
                points: vec![
                    (mx, my),
                    (mx + 6.0, my),
                    (mx + 6.0, my + 6.0),
                    (mx, my + 6.0),
                ],
                closed: true,
                width: 0.5,
                color: mark,
                dashes: Vec::new(),
            });
            list.items.push(Item::Rect {
                x: mx + 1.5,
                y: my + 2.75,
                width: 3.0,
                height: 0.5,
                color: mark,
            });
            list.items.push(Item::Rect {
                x: mx + 2.75,
                y: my + 1.5,
                width: 0.5,
                height: 3.0,
                color: mark,
            });
        } else {
            list.lines(lines.iter());
        }
        if cell.comment {
            // A comment shows as a small corner of yellow (TAB-27).
            let (tx, ty) = (x + cx + cw, y + cy);
            list.items.push(Item::Path {
                path: vec![
                    PathOp::Move(tx - 6.0, ty),
                    PathOp::Line(tx, ty),
                    PathOp::Line(tx, ty + 6.0),
                    PathOp::Close,
                ],
                fill: Some(Paint::Color(Rgba::rgb(0xf6, 0xd3, 0x2d))),
                stroke: None,
            });
        }
    }
    let name = moved_lines(&laid.name, x, y);
    list.lines(name.iter());
    // Outline Table Title: a thin line around the name (PXV-15).
    if let Some((nx, ny, nw, nh)) = laid.name_outline {
        let (left, top) = (x + nx + 0.5, y + ny + 0.5);
        let (right, bottom) = (x + nx + nw - 0.5, y + ny + nh - 0.5);
        list.items.push(Item::Stroke {
            points: vec![(left, top), (right, top), (right, bottom), (left, bottom)],
            closed: true,
            width: 1.0,
            color: Rgba::rgb(0, 0, 0),
            dashes: Vec::new(),
        });
    }
    list.items
}

fn chart_paint(paint: &desk_model::chart::Paint, bounds: (f32, f32, f32, f32)) -> Paint {
    match paint {
        desk_model::chart::Paint::Color(c) => Paint::Color(table_color(*c)),
        desk_model::chart::Paint::Gradient(a, b) => Paint::Linear {
            start: (bounds.0, bounds.1),
            end: (bounds.0, bounds.3),
            stops: vec![(0.0, table_color(*a)), (1.0, table_color(*b))],
        },
        desk_model::chart::Paint::Texture(t) => Paint::Linear {
            // The grain of a texture, as bands of its two colors.
            start: (bounds.0, bounds.1),
            end: (bounds.2, bounds.3),
            stops: (0..=8)
                .map(|i| {
                    let c = if i % 2 == 0 { t.base() } else { t.grain() };
                    (i as f32 / 8.0, table_color(c))
                })
                .collect(),
        },
    }
}

/// What a chart draws, at a place on the page (GRF-01).
/// What an equation draws, scaled into its frame at a place on the page
/// (W104).
fn equation_items(laid: &desk_layout::EquationLayout, x: f32, y: f32, w: f32, h: f32) -> Vec<Item> {
    let (scale, dx, dy) = laid.fit(w, h);
    let mut items = Vec::new();
    let mut labels = laid.labels.iter();
    for mark in &laid.drawing.marks {
        match mark {
            desk_math::Mark::Text { .. } => {
                let Some(label) = labels.next() else { continue };
                let mut list = DisplayList::default();
                list.lines(moved_lines(&label.lines, label.at.0, label.at.1).iter());
                items.extend(list.items);
            }
            desk_math::Mark::Rule {
                x,
                y,
                width,
                height,
            } => items.push(Item::Path {
                path: shape_path(&Shape::Rectangle, *x, *y, *width, *height),
                fill: Some(Paint::Color(Rgba::BLACK)),
                stroke: None,
            }),
            desk_math::Mark::Stroke { points, width } => items.push(Item::Stroke {
                points: points.clone(),
                closed: false,
                width: *width,
                color: Rgba::BLACK,
                dashes: Vec::new(),
            }),
        }
    }
    vec![Item::Group {
        transform: [scale, 0.0, 0.0, scale, x + dx, y + dy],
        opacity: 1.0,
        items,
    }]
}

fn chart_items(laid: &desk_layout::ChartLayout, x: f32, y: f32) -> Vec<Item> {
    use desk_model::chart::{Mark, PathOp as ChartOp};
    let mut items = Vec::new();
    let mut labels = laid.labels.iter();
    for mark in &laid.drawing.marks {
        match mark {
            Mark::Shape {
                path,
                fill,
                stroke,
                opacity,
                shadow,
            } => {
                let ops: Vec<PathOp> = path
                    .iter()
                    .map(|op| match *op {
                        ChartOp::Move(a, b) => PathOp::Move(x + a, y + b),
                        ChartOp::Line(a, b) => PathOp::Line(x + a, y + b),
                        ChartOp::Curve(a, b, c, d, e, f) => {
                            PathOp::Curve(x + a, y + b, x + c, y + d, x + e, y + f)
                        }
                        ChartOp::Close => PathOp::Close,
                    })
                    .collect();
                let points: Vec<(f32, f32)> = ops
                    .iter()
                    .filter_map(|op| match *op {
                        PathOp::Move(a, b)
                        | PathOp::Line(a, b)
                        | PathOp::Curve(_, _, _, _, a, b) => Some((a, b)),
                        PathOp::Close => None,
                    })
                    .collect();
                let bounds = points
                    .iter()
                    .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, p| {
                        (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
                    });
                let item = Item::Path {
                    path: ops,
                    fill: fill.as_ref().map(|f| chart_paint(f, bounds)),
                    stroke: stroke.as_ref().map(|(w, c, d)| PathStroke {
                        width: *w,
                        color: table_color(*c),
                        dashes: d.clone(),
                    }),
                };
                if *shadow {
                    items.push(Item::CastShadow {
                        items: vec![item.clone()],
                        offset: (2.0, 2.0),
                        blur: 3.0,
                        color: Rgba {
                            a: 96,
                            ..Rgba::BLACK
                        },
                    });
                }
                if *opacity < 1.0 {
                    items.push(Item::Group {
                        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                        opacity: *opacity,
                        items: vec![item],
                    });
                } else {
                    items.push(item);
                }
            }
            Mark::Text { .. } => {
                let Some(label) = labels.next() else { continue };
                let (ax, ay) = (x + label.at.0, y + label.at.1);
                let lines = moved_lines(&label.lines, ax, ay);
                let mut list = DisplayList::default();
                list.lines(lines.iter());
                if label.angle == 0.0 {
                    items.extend(list.items);
                } else {
                    // Turned counterclockwise around its point.
                    let (sin, cos) = (-label.angle.to_radians()).sin_cos();
                    items.push(Item::Group {
                        transform: [
                            cos,
                            sin,
                            -sin,
                            cos,
                            ax - (cos * ax - sin * ay),
                            ay - (sin * ax + cos * ay),
                        ],
                        opacity: 1.0,
                        items: list.items,
                    });
                }
            }
        }
    }
    items
}
