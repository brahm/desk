// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paragraph layout on top of Parley.
//!
//! Parley shapes the text, picks the fonts, places the glyphs and aligns
//! each line. Desk decides where each line ends, as the proof of concept
//! in `experiments/text-engine` settled: that keeps hyphens, the space
//! around wrapped objects and the rules that must match Pages in our code.

use std::ops::Range;

use desk_model::{Alignment, Baseline, CharFormat, LINE_BREAK, LineSpacing, LineStyle, Rgba};
use icu_segmenter::LineSegmenter;
use icu_segmenter::options::LineBreakOptions;
use parley::layout::BreakerState;
use parley::style::{FontFamily, FontStyle, FontWeight, Language, LineHeight, StyleProperty};
use parley::{
    AlignmentOptions, FontContext, FontData, Layout, LayoutContext, PositionedLayoutItem,
    VerticalAlign,
};

use crate::input::{ParagraphText, SMALL_CAPS_SCALE};
use crate::space::LineSpace;

/// U+00AD SOFT HYPHEN: a place where a word may break, with a hyphen drawn
/// at the end of the line when it does.
pub const SOFT_HYPHEN: char = '\u{ad}';

/// The size of superscripts and subscripts, relative to the font size.
/// To be checked against Pages in the reference VM.
pub const SCRIPT_SCALE: f32 = 0.66;
/// How far a superscript is raised, relative to the font size.
pub const SUPERSCRIPT_RAISE: f32 = 0.33;
/// How far a subscript is lowered, relative to the font size.
pub const SUBSCRIPT_DROP: f32 = 0.15;

/// The narrowest piece of a line that is worth filling.
pub const MIN_SEGMENT_WIDTH: f32 = 18.0;

/// How far down a line looks for a free piece before it gives up and
/// overflows.
const MAX_SEARCH: f32 = 10_000.0;

/// One glyph, in the coordinates of the layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// The glyph identifier in its font.
    pub id: u32,
    /// The position of its origin, on the baseline.
    pub x: f32,
    /// The position of its baseline.
    pub y: f32,
}

/// A line drawn with the text: an underline or a strikethrough.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decoration {
    /// Where it starts.
    pub x0: f32,
    /// Where it ends.
    pub x1: f32,
    /// The center of the line, or of the first line when it is double.
    pub y: f32,
    /// The thickness.
    pub thickness: f32,
    /// One line or two.
    pub style: LineStyle,
    /// The color.
    pub color: Rgba,
}

/// Glyphs in one font, size and format.
#[derive(Clone, Debug)]
pub struct GlyphRun {
    /// The font.
    pub font: FontData,
    /// The font size, in points.
    pub size: f32,
    /// The index of the character format in [`ParagraphText::formats`].
    pub format: u32,
    /// The glyphs.
    pub glyphs: Vec<Glyph>,
    /// Where the run starts.
    pub x: f32,
    /// The width of the run.
    pub advance: f32,
    /// The baseline of the run, shifted for superscripts and subscripts.
    pub baseline: f32,
}

/// One line of a paragraph, or one piece of a line beside a wrapped object.
#[derive(Clone, Debug)]
pub struct Line {
    /// The text of the line, as model offsets in the paragraph.
    pub range: Range<usize>,
    /// The left edge of the space the line was given.
    pub x: f32,
    /// The width of the space the line was given.
    pub width: f32,
    /// The width that the text of the line takes before justification, a
    /// hyphen included and trailing spaces left out.
    pub advance: f32,
    /// The top of the line.
    pub top: f32,
    /// The distance from this line's top to the next line's top.
    pub height: f32,
    /// The baseline.
    pub baseline: f32,
    /// The glyphs, in runs.
    pub runs: Vec<GlyphRun>,
    /// Underlines and strikethroughs.
    pub decorations: Vec<Decoration>,
    /// Whether the line ends at a soft hyphen, with a hyphen drawn.
    pub hyphenated: bool,
}

/// A laid out paragraph.
#[derive(Clone, Debug)]
pub struct ParagraphLayout {
    /// The lines, top to bottom and left to right.
    pub lines: Vec<Line>,
    /// The top of the paragraph.
    pub top: f32,
    /// The bottom of the last line.
    pub bottom: f32,
}

/// The fonts and scratch space for laying out text. Make one and keep it:
/// loading the system fonts is the slow part.
pub struct TextSystem {
    fonts: FontContext,
    layouts: LayoutContext<u32>,
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// A shaped cluster of the unbroken paragraph: where it starts, how wide
/// it is and which shaped run it came from.
struct Cluster {
    start: usize,
    advance: f32,
    run: usize,
}

/// What a line was committed with, kept to describe it afterwards.
struct Placed {
    x: f32,
    width: f32,
    top: f32,
    height: f32,
    hyphenated: bool,
    hyphen: Option<(FontData, f32, u32, f32)>,
}

impl TextSystem {
    /// Loads the system fonts through fontconfig.
    pub fn new() -> Self {
        Self {
            fonts: FontContext::new(),
            layouts: LayoutContext::new(),
        }
    }

    fn build(&mut self, text: &ParagraphText) -> Layout<u32> {
        let mut builder = self
            .layouts
            .ranged_builder(&mut self.fonts, text.text(), 1.0, false);
        let formats = text.formats();
        for property in properties(&formats[0], false) {
            builder.push_default(property);
        }
        builder.push_default(StyleProperty::Brush(0));
        builder.push_default(StyleProperty::LineHeight(
            match text.paragraph().line_spacing {
                LineSpacing::Multiple { percent } => {
                    LineHeight::MetricsRelative(percent as f32 / 100.0)
                }
                LineSpacing::Exactly(length) => LineHeight::Absolute(length.to_pt() as f32),
                LineSpacing::AtLeast(_) | LineSpacing::Between(_) => {
                    LineHeight::MetricsRelative(1.0)
                }
            },
        ));
        for range in &text.ranges {
            if range.format == 0 && !range.small_caps {
                continue;
            }
            for property in properties(&formats[range.format as usize], range.small_caps) {
                builder.push(property, range.range.clone());
            }
            builder.push(StyleProperty::Brush(range.format), range.range.clone());
        }
        builder.build(text.text())
    }

    /// Lays out a paragraph in a space, starting at `top`.
    pub fn layout(
        &mut self,
        text: &ParagraphText,
        space: &dyn LineSpace,
        top: f32,
    ) -> ParagraphLayout {
        let mut layout = self.build(text);
        let source = text.text();
        let format = text.paragraph().clone();

        // Shaped widths, from the paragraph on one line (or one line per
        // manual line break).
        layout.break_all_lines(None);
        let mut runs: Vec<(FontData, f32)> = Vec::new();
        let mut clusters: Vec<Cluster> = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                    let run = glyph_run.run();
                    runs.push((run.font().clone(), run.font_size()));
                    for cluster in run.clusters() {
                        clusters.push(Cluster {
                            start: cluster.text_range().start,
                            advance: cluster.advance(),
                            run: runs.len() - 1,
                        });
                    }
                }
            }
        }
        clusters.sort_by_key(|c| c.start);
        clusters.dedup_by_key(|c| c.start);
        let breaks: Vec<usize> = LineSegmenter::new_auto(LineBreakOptions::default())
            .segment_str(source)
            .filter(|&b| b > 0)
            .collect();

        let span = |a: usize, b: usize| {
            let first = clusters.partition_point(|c| c.start < a);
            let last = clusters.partition_point(|c| c.start < b);
            &clusters[first..last]
        };
        let trim =
            |a: usize, b: usize| a + source[a..b].trim_end_matches([' ', LINE_BREAK, '\t']).len();
        // The hyphen drawn when a line ends at the soft hyphen before `b`.
        let hyphen_at = |b: usize| -> Option<(FontData, f32, u32, f32)> {
            if !source[..b].ends_with(SOFT_HYPHEN) {
                return None;
            }
            let run = span(0, b).last().map_or(0, |c| c.run);
            let (font, size) = runs.get(run)?.clone();
            let (id, advance) = hyphen_glyph(&font, size)?;
            Some((font, size, id, advance))
        };
        let natural = |a: usize, b: usize| {
            let width: f32 = span(a, trim(a, b)).iter().map(|c| c.advance).sum();
            width + hyphen_at(b).map_or(0.0, |h| h.3)
        };
        let mandatory = |b: usize| source[..b].ends_with(LINE_BREAK);

        let left = format.left_indent.to_pt() as f32;
        let first = format.first_line_indent.to_pt() as f32;
        let right = format.right_indent.to_pt() as f32;
        let column = space.width();

        let mut placed: Vec<Placed> = Vec::new();
        let mut breaker = layout.break_lines();
        breaker.state_mut().set_layout_max_advance(f32::INFINITY);
        let (mut pos, mut next_break) = (0, 0);
        let mut y = top;
        let mut guess = text.formats()[0].font_size.to_pt() as f32 * 1.2;
        let mut first_line = true;
        let mut searched_from = y;
        'band: loop {
            let indent = if first_line { left + first } else { left };
            let mut segments: Vec<(f32, f32)> = space
                .segments(y, guess)
                .into_iter()
                .filter_map(|(x, w)| {
                    let x0 = x.max(indent);
                    let x1 = (x + w).min(column - right);
                    (x1 - x0 >= MIN_SEGMENT_WIDTH).then_some((x0, x1 - x0))
                })
                .collect();
            if segments.is_empty() {
                if space.is_exhausted(y) {
                    break;
                }
                if y - searched_from < MAX_SEARCH {
                    y += 1.0;
                    continue;
                }
                // Nothing fits for a long way down, as when the indents are
                // wider than the column: overflow rather than loop forever.
                segments.push((indent, (column - right - indent).max(MIN_SEGMENT_WIDTH)));
            }
            searched_from = y;
            let saved: BreakerState = breaker.state().clone();
            let (saved_pos, saved_next, saved_placed) = (pos, next_break, placed.len());
            let mut tallest = 0.0_f32;
            for (x, width) in segments {
                // Greedy: the last break that fits, stopping at a manual
                // line break; the first break when nothing fits.
                let mut end = None;
                let mut i = next_break;
                while i < breaks.len() {
                    if natural(pos, breaks[i]) > width + 0.001 {
                        break;
                    }
                    end = Some(i);
                    if mandatory(breaks[i]) {
                        break;
                    }
                    i += 1;
                }
                let (line_end, chosen) =
                    match end.or((next_break < breaks.len()).then_some(next_break)) {
                        Some(i) => (breaks[i], i + 1),
                        None => (source.len(), next_break),
                    };
                let hyphen = hyphen_at(line_end);
                let chars = source[pos..line_end].chars().count() as u32;

                let commit = |breaker: &mut parley::layout::BreakLines<'_, u32>, line_y: f32| {
                    let state = breaker.state_mut();
                    state.set_line_y(f64::from(line_y));
                    state.set_line_x(x);
                    // Justify to the width less the hyphen, drawn after it.
                    state.set_line_max_advance(width - hyphen.as_ref().map_or(0.0, |h| h.3));
                    breaker.break_next_with_length(chars);
                    (breaker.committed_y() as f32) - line_y
                };
                let before_line = breaker.state().clone();
                let mut height = commit(&mut breaker, y);
                let mut line_top = y;
                let pitch = match format.line_spacing {
                    LineSpacing::AtLeast(min) => {
                        let min = min.to_pt() as f32;
                        if height < min {
                            // The extra space goes above the line.
                            breaker.revert_to(before_line);
                            line_top = y + (min - height);
                            height = commit(&mut breaker, line_top);
                            min
                        } else {
                            height
                        }
                    }
                    LineSpacing::Between(extra) => height + extra.to_pt() as f32,
                    _ => height,
                };
                if pitch > guess + 0.001 {
                    // Taller than the band was measured for: measure the
                    // free space again with the real height.
                    breaker.revert_to(saved);
                    (pos, next_break) = (saved_pos, saved_next);
                    placed.truncate(saved_placed);
                    guess = pitch;
                    continue 'band;
                }
                tallest = tallest.max(pitch);
                placed.push(Placed {
                    x,
                    width,
                    top: line_top,
                    height: pitch,
                    hyphenated: hyphen.is_some(),
                    hyphen,
                });
                pos = line_end;
                next_break = chosen;
                first_line = false;
                if pos >= source.len() {
                    break 'band;
                }
            }
            y += tallest;
        }
        breaker.finish();
        layout.align(alignment(format.alignment), AlignmentOptions::default());

        let mut lines = Vec::with_capacity(placed.len());
        for (line, place) in layout.lines().zip(&placed) {
            let metrics = line.metrics();
            let mut glyph_runs = Vec::new();
            let mut decorations = Vec::new();
            let mut end_x = place.x;
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let format_index = glyph_run.style().brush;
                let format = &text.formats()[format_index as usize];
                let glyphs: Vec<Glyph> = glyph_run
                    .positioned_glyphs()
                    .map(|g| Glyph {
                        id: g.id,
                        x: g.x,
                        y: g.y,
                    })
                    .collect();
                let x0 = glyph_run.offset();
                let x1 = x0 + glyph_run.advance();
                end_x = end_x.max(x1);
                let baseline = glyphs.first().map_or(glyph_run.baseline(), |g| g.y);
                let m = run.font_metrics();
                if format.underline != LineStyle::None {
                    decorations.push(Decoration {
                        x0,
                        x1,
                        y: baseline - m.underline_offset,
                        thickness: m.underline_size,
                        style: format.underline,
                        color: format.color,
                    });
                }
                if format.strikethrough != LineStyle::None {
                    decorations.push(Decoration {
                        x0,
                        x1,
                        y: baseline - m.strikethrough_offset,
                        thickness: m.strikethrough_size,
                        style: format.strikethrough,
                        color: match format.strikethrough_color {
                            desk_model::LineColor::Text => format.color,
                            desk_model::LineColor::Custom(color) => color,
                        },
                    });
                }
                glyph_runs.push(GlyphRun {
                    font: run.font().clone(),
                    size: run.font_size(),
                    format: format_index,
                    glyphs,
                    x: x0,
                    advance: glyph_run.advance(),
                    baseline,
                });
            }
            let mut advance = metrics.advance - metrics.hanging_advance;
            if let Some((font, size, id, hyphen_advance)) = &place.hyphen {
                glyph_runs.push(GlyphRun {
                    font: font.clone(),
                    size: *size,
                    format: glyph_runs.last().map_or(0, |r| r.format),
                    glyphs: vec![Glyph {
                        id: *id,
                        x: end_x,
                        y: metrics.baseline,
                    }],
                    x: end_x,
                    advance: *hyphen_advance,
                    baseline: metrics.baseline,
                });
                advance += hyphen_advance;
            }
            // An empty paragraph has a line over a placeholder past its end.
            let range = line.text_range();
            let range = range.start.min(source.len())..range.end.min(source.len());
            lines.push(Line {
                range: text.to_model(range.start)..text.to_model(range.end),
                x: place.x,
                width: place.width,
                advance,
                top: place.top,
                height: place.height,
                baseline: metrics.baseline,
                runs: glyph_runs,
                decorations,
                hyphenated: place.hyphenated,
            });
        }
        let bottom = lines.last().map_or(top, |l| l.top + l.height);
        ParagraphLayout { lines, top, bottom }
    }
}

fn alignment(alignment: Alignment) -> parley::Alignment {
    match alignment {
        Alignment::Left | Alignment::Auto => parley::Alignment::Left,
        Alignment::Center => parley::Alignment::Center,
        Alignment::Right => parley::Alignment::Right,
        Alignment::Justify => parley::Alignment::Justify,
    }
}

/// The Parley style of a character format.
fn properties(format: &CharFormat, small_caps: bool) -> Vec<StyleProperty<'_, u32>> {
    let mut size = format.font_size.to_pt() as f32;
    let mut raise = format.baseline_shift.to_pt() as f32;
    match format.baseline {
        Baseline::Normal => {}
        Baseline::Superscript => {
            raise += size * SUPERSCRIPT_RAISE;
            size *= SCRIPT_SCALE;
        }
        Baseline::Subscript => {
            raise -= size * SUBSCRIPT_DROP;
            size *= SCRIPT_SCALE;
        }
    }
    if small_caps {
        size *= SMALL_CAPS_SCALE;
    }
    let mut properties = vec![
        StyleProperty::FontFamily(FontFamily::named(&format.font_family)),
        StyleProperty::FontSize(size),
        StyleProperty::FontWeight(if format.bold {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        }),
        StyleProperty::FontStyle(if format.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        }),
        StyleProperty::VerticalAlign(VerticalAlign::length(raise)),
    ];
    if format.language.tag() != "und"
        && let Ok(language) = Language::parse(format.language.tag())
    {
        properties.push(StyleProperty::Locale(Some(language)));
    }
    properties
}

/// The hyphen glyph of a font and its advance at a size.
fn hyphen_glyph(font: &FontData, size: f32) -> Option<(u32, f32)> {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};
    let font = skrifa::FontRef::from_index(font.data.as_ref(), font.index).ok()?;
    let id = font.charmap().map('-')?;
    let advance = font
        .glyph_metrics(Size::new(size), LocationRef::default())
        .advance_width(id)?;
    Some((id.to_u32(), advance))
}
