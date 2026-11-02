// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The display list: what a page draws, in page coordinates.

use desk_layout::{DocumentLayout, Page, PlacedBlock};
use desk_model::{BorderSides, LineStyle, Rgba};
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
        for placed in &page.lines {
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
        for block in &page.blocks {
            list.block_border(block);
        }
        list
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
