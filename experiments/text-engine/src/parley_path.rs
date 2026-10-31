// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The paragraph laid out with Parley, driving its line breaker one line
//! at a time.

use std::collections::HashSet;

use parley::layout::{BreakerState, YieldData};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};

use icu_segmenter::LineSegmenter;
use icu_segmenter::options::LineBreakOptions;

use crate::common::{
    CIRCLE, FONT_FAMILY, FONT_SIZE, FontBytes, Laid, PlacedGlyph, PlacedLine, SOFT_HYPHEN,
    hyphen_glyph,
};

/// Breaks the lines of the layout around the circle. A line whose index
/// is in `narrow` gets the width of a hyphen less, so that the hyphen fits
/// when the line ends at a soft hyphen. A line whose index is in
/// `hyphenated` is justified to the width less the hyphen, which is drawn
/// after it.
fn break_lines(
    layout: &mut Layout<()>,
    narrow: &HashSet<usize>,
    hyphenated: &HashSet<usize>,
    hyphen: f32,
) {
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(f32::INFINITY);
    let mut y = 0.0_f32;
    // The first guess of the line height; a taller line yields, and the
    // band is tried again with its real height.
    let mut guess = FONT_SIZE * 1.15;
    let mut index = 0;
    'band: while !breaker.is_done() {
        let segments = CIRCLE.segments(y, y + guess);
        if segments.is_empty() {
            y += 1.0;
            continue;
        }
        let saved: BreakerState = breaker.state().clone();
        let saved_index = index;
        let mut tallest = 0.0_f32;
        for (x, width) in segments {
            let state = breaker.state_mut();
            state.set_line_y(f64::from(y));
            state.set_line_x(x);
            let break_width = if narrow.contains(&index) {
                width - hyphen
            } else {
                width
            };
            state.set_line_max_advance(break_width);
            state.set_line_max_height(guess);
            match breaker.break_next() {
                None => break 'band,
                Some(YieldData::LineBreak(line)) => {
                    tallest = tallest.max((line.line_y_end - line.line_y_start) as f32);
                    if hyphenated.contains(&index) {
                        breaker.set_prior_line_width(width - hyphen);
                    }
                    index += 1;
                }
                Some(YieldData::MaxHeightExceeded(data)) => {
                    breaker.revert_to(saved);
                    index = saved_index;
                    guess = data.line_height;
                    continue 'band;
                }
                Some(YieldData::InlineBoxBreak(_)) => unreachable!("no inline boxes"),
            }
            if breaker.is_done() {
                break 'band;
            }
        }
        y += tallest;
    }
    breaker.finish();
}

fn ends_with_soft_hyphen(text: &str, end: usize) -> bool {
    text[..end].ends_with(SOFT_HYPHEN)
}

/// Breaks the lines with our own decision of where each line ends, made
/// from Parley's shaped clusters, and commits each line to Parley by its
/// length. Parley still places the glyphs, measures the lines and aligns
/// them.
fn break_lines_by_length(layout: &mut Layout<()>, text: &str, hyphen: f32) {
    // Shaped advances, from one unbroken line.
    layout.break_all_lines(None);
    let mut clusters: Vec<(usize, f32)> = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(run) = item {
                for cluster in run.run().clusters() {
                    clusters.push((cluster.text_range().start, cluster.advance()));
                }
            }
        }
    }
    clusters.sort_by_key(|c| c.0);
    clusters.dedup_by_key(|c| c.0);
    let breaks: Vec<usize> = LineSegmenter::new_auto(LineBreakOptions::default())
        .segment_str(text)
        .filter(|&b| b > 0)
        .collect();
    let width = |a: usize, b: usize| {
        let first = clusters.partition_point(|c| c.0 < a);
        let last = clusters.partition_point(|c| c.0 < b);
        clusters[first..last].iter().map(|c| c.1).sum::<f32>()
    };
    let natural = |a: usize, b: usize| {
        let end = a + text[a..b].trim_end_matches(' ').len();
        width(a, end)
            + if ends_with_soft_hyphen(text, b) {
                hyphen
            } else {
                0.0
            }
    };

    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(f32::INFINITY);
    let (mut pos, mut next_break) = (0, 0);
    let mut y = 0.0_f32;
    let mut guess = FONT_SIZE * 1.15;
    'band: while pos < text.len() {
        let segments = CIRCLE.segments(y, y + guess);
        if segments.is_empty() {
            y += 1.0;
            continue;
        }
        let saved: BreakerState = breaker.state().clone();
        let (saved_pos, saved_next) = (pos, next_break);
        let mut tallest = 0.0_f32;
        for (x, segment) in segments {
            let mut end = None;
            let mut i = next_break;
            while i < breaks.len() && natural(pos, breaks[i]) <= segment + 0.001 {
                end = Some(i);
                i += 1;
            }
            let chosen = end.unwrap_or(next_break.min(breaks.len() - 1));
            let line_end = breaks[chosen];
            let hyphenated = ends_with_soft_hyphen(text, line_end);
            let state = breaker.state_mut();
            state.set_line_y(f64::from(y));
            state.set_line_x(x);
            // Justify to the width less the hyphen, which is drawn after.
            state.set_line_max_advance(if hyphenated {
                segment - hyphen
            } else {
                segment
            });
            let chars = text[pos..line_end].chars().count() as u32;
            breaker.break_next_with_length(chars);
            let height = (breaker.committed_y() - f64::from(y)) as f32;
            if height > guess + 0.001 {
                breaker.revert_to(saved);
                (pos, next_break) = (saved_pos, saved_next);
                guess = height;
                continue 'band;
            }
            tallest = tallest.max(height);
            pos = line_end;
            next_break = chosen + 1;
            if pos >= text.len() {
                break 'band;
            }
        }
        y += tallest;
    }
    breaker.finish();
}

/// How the lines are broken.
#[derive(Clone, Copy)]
pub enum Breaking {
    /// Parley's own breaker, narrowing a line by a hyphen when the hyphen
    /// does not fit.
    Parley,
    /// Our decision, committed to Parley line by line.
    Ours,
}

pub struct Engine {
    fonts: FontContext,
    layouts: LayoutContext<()>,
    breaking: Breaking,
}

impl Engine {
    pub fn new(breaking: Breaking) -> Self {
        Self {
            fonts: FontContext::new(),
            layouts: LayoutContext::new(),
            breaking,
        }
    }

    /// Lays out the text. Returns the layout and how many times the lines
    /// were broken, which grows with each hyphen that did not fit.
    pub fn layout(&mut self, text: &str) -> (Laid, usize) {
        let mut builder = self
            .layouts
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(FONT_FAMILY)));
        builder.push_default(StyleProperty::FontSize(FONT_SIZE));
        let mut layout: Layout<()> = builder.build(text);

        let font = {
            let mut layout_font = None;
            layout.break_all_lines(None);
            if let Some(line) = layout.lines().next() {
                for item in line.items() {
                    if let PositionedLayoutItem::GlyphRun(run) = item {
                        let data = run.run().font();
                        layout_font = Some(FontBytes {
                            data: std::sync::Arc::new(data.data.as_ref().to_vec()),
                            index: data.index,
                        });
                        break;
                    }
                }
            }
            layout_font.expect("the text has glyphs")
        };
        let (hyphen_id, hyphen) = hyphen_glyph(&font);

        // Break, then look for the first line that ends at a soft hyphen
        // but has no room for the hyphen, narrow it and break again.
        let mut narrow = HashSet::new();
        let mut passes = 0;
        let none = HashSet::new();
        if let Breaking::Ours = self.breaking {
            break_lines_by_length(&mut layout, text, hyphen);
            passes = 1;
        }
        while let Breaking::Parley = self.breaking {
            passes += 1;
            break_lines(&mut layout, &narrow, &none, hyphen);
            let offender = layout.lines().enumerate().find(|(i, line)| {
                let m = line.metrics();
                let width = m.inline_max_coord - m.inline_min_coord;
                !narrow.contains(i)
                    && ends_with_soft_hyphen(text, line.text_range().end)
                    && m.advance - m.hanging_advance + hyphen > width + 0.001
            });
            match offender {
                Some((i, _)) => {
                    narrow.insert(i);
                }
                None => break,
            }
        }
        if let Breaking::Parley = self.breaking {
            let hyphenated: HashSet<usize> = layout
                .lines()
                .enumerate()
                .filter(|(_, line)| ends_with_soft_hyphen(text, line.text_range().end))
                .map(|(i, _)| i)
                .collect();
            break_lines(&mut layout, &narrow, &hyphenated, hyphen);
            passes += 1;
        }
        layout.align(Alignment::Justify, AlignmentOptions::default());

        let mut lines = Vec::new();
        for line in layout.lines() {
            let m = line.metrics();
            let range = line.text_range();
            let hyphenated = ends_with_soft_hyphen(text, range.end);
            let mut glyphs = Vec::new();
            let mut end_x = m.inline_min_coord;
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    for g in run.positioned_glyphs() {
                        glyphs.push(PlacedGlyph {
                            id: g.id,
                            x: g.x,
                            y: g.y,
                        });
                    }
                    end_x = end_x.max(run.offset() + run.advance());
                }
            }
            let mut used = m.advance - m.hanging_advance;
            let mut width = m.inline_max_coord - m.inline_min_coord;
            if hyphenated {
                width += hyphen;
                // Parley breaks at the soft hyphen but draws nothing
                // there, so the hyphen is placed here.
                glyphs.push(PlacedGlyph {
                    id: hyphen_id,
                    x: end_x,
                    y: m.baseline,
                });
                used += hyphen;
            }
            lines.push(PlacedLine {
                text: text[range].replace(SOFT_HYPHEN, ""),
                x: m.inline_min_coord,
                width,
                used,
                top: m.block_min_coord,
                bottom: m.block_max_coord,
                glyphs,
                hyphenated,
            });
        }
        (Laid { font, lines }, passes)
    }
}
