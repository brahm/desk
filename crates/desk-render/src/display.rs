// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The display list: what a page draws, in page coordinates.

use desk_layout::{DocumentLayout, Page};
use desk_model::{LineStyle, Rgba};
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
        for placed in &page.lines {
            let (dx, dy) = placed.origin;
            for run in &placed.line.runs {
                if run.glyphs.is_empty() {
                    continue;
                }
                list.items.push(Item::Glyphs {
                    font: run.font.clone(),
                    size: run.size,
                    color: run.color,
                    text: run.text.clone(),
                    clusters: run.clusters.clone(),
                    glyphs: run
                        .glyphs
                        .iter()
                        .map(|g| PlacedGlyph {
                            id: g.id,
                            x: g.x + dx,
                            y: g.y + dy,
                        })
                        .collect(),
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
        list
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
