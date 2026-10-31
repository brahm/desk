// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The same paragraph with a minimal engine of our own: HarfRust for
//! shaping, ICU4X for the break opportunities and a greedy breaker that
//! takes a width per line.
//!
//! It covers only what this case needs: one font, one style, left to
//! right, no reshaping across a break, no bidi, no fallback fonts, no
//! inline objects and no cursor geometry. Each of those is work that
//! Parley already has.

use harfrust::{Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use icu_segmenter::LineSegmenter;
use icu_segmenter::options::LineBreakOptions;
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};

use crate::common::{
    CIRCLE, FONT_SIZE, FontBytes, Laid, PlacedGlyph, PlacedLine, SOFT_HYPHEN, hyphen_glyph,
};

/// A shaped cluster: where it starts in the text, and its glyphs with
/// their offsets and advances, in points.
struct Cluster {
    start: usize,
    glyphs: Vec<(u32, f32, f32)>,
    advance: f32,
}

pub struct Engine {
    font: FontBytes,
    data: ShaperData,
    line_height: f32,
    ascent: f32,
}

impl Engine {
    pub fn new(font: FontBytes) -> Self {
        let face = FontRef::from_index(&font.data, font.index).expect("font");
        let data = ShaperData::new(&face);
        let metrics = skrifa::FontRef::from_index(&font.data, font.index)
            .expect("font")
            .metrics(Size::new(FONT_SIZE), LocationRef::default());
        // The same rule as Parley's default: ascent, descent and leading.
        let line_height = metrics.ascent - metrics.descent + metrics.leading;
        Self {
            ascent: metrics.ascent + metrics.leading / 2.0,
            font,
            data,
            line_height,
        }
    }

    fn shape(&self, text: &str) -> Vec<Cluster> {
        let face = FontRef::from_index(&self.font.data, self.font.index).expect("font");
        let upem = skrifa::FontRef::from_index(&self.font.data, self.font.index)
            .expect("font")
            .metrics(Size::unscaled(), LocationRef::default())
            .units_per_em;
        let scale = FONT_SIZE / f32::from(upem);
        let shaper = self.data.shaper(&face).build();
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(Direction::LeftToRight);
        buffer.guess_segment_properties();
        let output = shaper.shape(buffer, ShapeOptions::new());

        let mut clusters: Vec<Cluster> = Vec::new();
        for (info, pos) in output.glyph_infos().iter().zip(output.glyph_positions()) {
            let start = info.cluster as usize;
            let glyph = (
                info.glyph_id,
                pos.x_offset as f32 * scale,
                pos.y_offset as f32 * scale,
            );
            let advance = pos.x_advance as f32 * scale;
            match clusters.last_mut() {
                Some(last) if last.start == start => {
                    last.glyphs.push(glyph);
                    last.advance += advance;
                }
                _ => clusters.push(Cluster {
                    start,
                    glyphs: vec![glyph],
                    advance,
                }),
            }
        }
        clusters
    }

    pub fn layout(&self, text: &str) -> Laid {
        let clusters = self.shape(text);
        let (hyphen_id, hyphen) = hyphen_glyph(&self.font);
        let breaks: Vec<usize> = LineSegmenter::new_auto(LineBreakOptions::default())
            .segment_str(text)
            .filter(|&b| b > 0)
            .collect();

        // The clusters between two byte offsets.
        let span = |a: usize, b: usize| {
            let first = clusters.partition_point(|c| c.start < a);
            let last = clusters.partition_point(|c| c.start < b);
            &clusters[first..last]
        };
        // The end of a line without its trailing spaces.
        let trim = |a: usize, b: usize| a + text[a..b].trim_end_matches(' ').len();
        let width = |a: usize, b: usize| span(a, b).iter().map(|c| c.advance).sum::<f32>();
        let natural = |a: usize, b: usize| {
            let hyphen = if text[..b].ends_with(SOFT_HYPHEN) {
                hyphen
            } else {
                0.0
            };
            width(a, trim(a, b)) + hyphen
        };

        let mut lines = Vec::new();
        let mut pos = 0;
        let mut next_break = 0;
        let mut y = 0.0_f32;
        'band: while pos < text.len() {
            let segments = CIRCLE.segments(y, y + self.line_height);
            if segments.is_empty() {
                y += 1.0;
                continue;
            }
            for (x, segment) in segments {
                // Greedy: the last break opportunity that fits, or the first
                // one when nothing fits.
                let mut end = None;
                let mut i = next_break;
                while i < breaks.len() && natural(pos, breaks[i]) <= segment + 0.001 {
                    end = Some(i);
                    i += 1;
                }
                let chosen = end.unwrap_or(next_break.min(breaks.len() - 1));
                let line_end = breaks[chosen];
                next_break = chosen + 1;

                let last = line_end == text.len();
                let content_end = trim(pos, line_end);
                let hyphenated = text[..line_end].ends_with(SOFT_HYPHEN);
                let used = natural(pos, line_end);
                let spaces = text[pos..content_end].matches(' ').count();
                let stretch = if last || spaces == 0 {
                    0.0
                } else {
                    (segment - used) / spaces as f32
                };

                let baseline = y + self.ascent;
                let mut pen = x;
                let mut glyphs = Vec::new();
                for cluster in span(pos, content_end) {
                    for &(id, dx, dy) in &cluster.glyphs {
                        glyphs.push(PlacedGlyph {
                            id,
                            x: pen + dx,
                            y: baseline - dy,
                        });
                    }
                    pen += cluster.advance;
                    if text[cluster.start..].starts_with(' ') {
                        pen += stretch;
                    }
                }
                if hyphenated {
                    glyphs.push(PlacedGlyph {
                        id: hyphen_id,
                        x: pen,
                        y: baseline,
                    });
                }
                lines.push(PlacedLine {
                    text: text[pos..line_end].replace(SOFT_HYPHEN, ""),
                    x,
                    width: segment,
                    used,
                    top: y,
                    bottom: y + self.line_height,
                    glyphs,
                    hyphenated,
                });
                pos = line_end;
                if last {
                    break 'band;
                }
            }
            y += self.line_height;
        }
        Laid {
            font: self.font.clone(),
            lines,
        }
    }
}
