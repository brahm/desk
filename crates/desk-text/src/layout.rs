// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paragraph layout on top of Parley.
//!
//! Parley shapes the text, picks the fonts, places the glyphs and aligns
//! each line. Desk decides where each line ends, as the proof of concept
//! in `experiments/text-engine` settled: that keeps hyphens, the space
//! around wrapped objects and the rules that must match Pages in our code.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use desk_model::{Alignment, Baseline, CharFormat, LINE_BREAK, LineSpacing, LineStyle, Rgba};
use icu_segmenter::LineSegmenter;
use icu_segmenter::options::LineBreakOptions;
use parley::layout::BreakerState;
use parley::style::{FontFamily, FontStyle, FontWeight, Language, LineHeight, StyleProperty};
use parley::{
    AlignmentOptions, FontContext, FontData, Layout, LayoutContext, PositionedLayoutItem,
    VerticalAlign,
};

use crate::fonts::{FontChoice, PagesFont, choose, pages_font};
use crate::input::{FI, FL, ParagraphText, SMALL_CAPS_SCALE};
use crate::space::LineSpace;
use crate::tabs::{Piece, TabPlace, TabRuler};

/// U+00AD SOFT HYPHEN: a place where a word may break, with a hyphen drawn
/// at the end of the line when it does.
pub const SOFT_HYPHEN: char = '\u{ad}';

/// The size of superscripts and subscripts, relative to the font size, as
/// measured in Pages (FMT-06).
pub const SCRIPT_SCALE: f32 = 2.0 / 3.0;

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
    /// The color of the text (FMT-11).
    pub color: Rgba,
    /// The text that the glyphs draw, for the text of a PDF: ligatures
    /// stand for the letters they join.
    pub text: String,
    /// The clusters of the text, in order: how many bytes of `text` each
    /// draws with how many glyphs. Empty when the glyphs cannot be told
    /// apart.
    pub clusters: Vec<(u32, u32)>,
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
    /// The bottom of the text: the baseline plus the descent, without the
    /// leading or any extra space below. Pages fits a line on a page when
    /// this fits (EST-05).
    pub text_bottom: f32,
    /// The glyphs, in runs.
    pub runs: Vec<GlyphRun>,
    /// Underlines and strikethroughs.
    pub decorations: Vec<Decoration>,
    /// Whether the line ends at a soft hyphen, with a hyphen drawn.
    pub hyphenated: bool,
    /// Where a text cursor stands at each place it can stop on the line:
    /// a model offset in the paragraph and the distance from the left of
    /// the paragraph, ordered by offset. The end of the line is the last.
    pub carets: Vec<(usize, f32)>,
}

impl Line {
    /// Where a cursor before `offset` stands on the line: the nearest
    /// place it can stop at or before the offset.
    pub fn caret_x(&self, offset: usize) -> f32 {
        let at = self.carets.partition_point(|&(o, _)| o <= offset);
        match at {
            0 => self.carets.first().map_or(self.x, |c| c.1),
            i => self.carets[i - 1].1,
        }
    }

    /// The offset of the place on the line nearest to `x`.
    pub fn offset_at(&self, x: f32) -> usize {
        self.carets
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map_or(self.range.start, |c| c.0)
    }
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
    /// The font chosen for each family named so far.
    choices: HashMap<Arc<str>, FontChoice>,
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

/// The box of a run on a line, in whole points as Pages rounds it: how far
/// it reaches over and under the baseline, its leading and its descent.
#[derive(Clone, Copy)]
struct RunBox {
    over: f32,
    under: f32,
    leading: f32,
    descent: f32,
}

impl RunBox {
    fn max(self, other: Self) -> Self {
        Self {
            over: self.over.max(other.over),
            under: self.under.max(other.under),
            leading: self.leading.max(other.leading),
            descent: self.descent.max(other.descent),
        }
    }
}

/// What a line was committed with, kept to describe it afterwards.
struct Placed {
    x: f32,
    width: f32,
    top: f32,
    height: f32,
    /// Where the text of the line was committed, and its box: the
    /// baseline is `text_top + over`. No box for a line without text.
    text_top: f32,
    line_box: Option<RunBox>,
    hyphenated: bool,
    hyphen: Option<(FontData, f32, u32, f32)>,
}

impl TextSystem {
    /// Loads the system fonts through fontconfig.
    pub fn new() -> Self {
        Self {
            fonts: FontContext::new(),
            layouts: LayoutContext::new(),
            choices: HashMap::new(),
        }
    }

    /// The font that lays out a family: the family itself when it is
    /// installed, or the free font that stands for it (DOC-05).
    pub fn font_choice(&mut self, family: &str) -> FontChoice {
        if let Some(choice) = self.choices.get(family) {
            return choice.clone();
        }
        let collection = &mut self.fonts.collection;
        let choice = choose(family, |name| collection.family_id(name).is_some());
        self.choices.insert(family.into(), choice.clone());
        choice
    }

    /// Forgets the fonts chosen so far, so that [`TextSystem::substitutions`]
    /// lists only those of the documents laid out from now on. The system
    /// fonts stay loaded.
    pub fn forget_choices(&mut self) {
        self.choices.clear();
    }

    /// The families laid out so far with another font, by name: what the
    /// Warnings window lists.
    pub fn substitutions(&self) -> Vec<FontChoice> {
        let mut out: Vec<FontChoice> = self
            .choices
            .values()
            .filter(|c| c.is_substitution())
            .cloned()
            .collect();
        out.sort_by(|a, b| a.requested.cmp(&b.requested));
        out
    }

    fn build(&mut self, text: &ParagraphText) -> Layout<u32> {
        let formats = text.formats();
        let families: Vec<Arc<str>> = formats
            .iter()
            .map(|f| self.font_choice(&f.font_family).used)
            .collect();
        let mut builder = self
            .layouts
            // Font metrics rounded to whole points, as Pages lays out lines.
            .ranged_builder(&mut self.fonts, text.text(), 1.0, true);
        for property in properties(&formats[0], &families[0], false) {
            builder.push_default(property);
        }
        builder.push_default(StyleProperty::Brush(0));
        // Every line is laid out at the height of its fonts, and the
        // spacing of the paragraph is applied when the lines are placed.
        builder.push_default(StyleProperty::LineHeight(LineHeight::MetricsRelative(1.0)));
        for range in &text.ranges {
            if range.format == 0 && !range.small_caps {
                continue;
            }
            let index = range.format as usize;
            for property in properties(&formats[index], &families[index], range.small_caps) {
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
        // The box of each run, rounded as the line heights are.
        let mut boxes: Vec<RunBox> = Vec::new();
        let mut clusters: Vec<Cluster> = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                    let run = glyph_run.run();
                    runs.push((run.font().clone(), run.font_size()));
                    let format = &text.formats()[glyph_run.style().brush as usize];
                    boxes.push(run_box(format, run.font_size(), run.font_metrics()));
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
        let ruler = TabRuler::new(&format);
        let has_tabs = source.contains('\t');
        // The width of the text from `a` to `b` on a line from `x` to
        // `limit`, and how much wider its tabs are than as shaped.
        let natural = |a: usize, b: usize, x: f32, limit: f32| -> (f32, f32) {
            let clusters = span(a, trim(a, b));
            let hyphen = hyphen_at(b).map_or(0.0, |h| h.3);
            if !has_tabs {
                return (
                    clusters.iter().map(|c| c.advance).sum::<f32>() + hyphen,
                    0.0,
                );
            }
            let first = |c: &Cluster| source[c.start..].chars().next().unwrap_or(' ');
            let pieces = clusters.iter().map(|c| Piece {
                tab: first(c) == '\t',
                advance: c.advance,
                first: first(c),
            });
            let (tabs, end) = ruler.lay(pieces, x, limit);
            let shaped: f32 = clusters
                .iter()
                .filter(|c| first(c) == '\t')
                .map(|c| c.advance)
                .sum();
            let wider = tabs.iter().map(|t| t.width).sum::<f32>() - shaped;
            (end - x + hyphen, wider)
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
                    if natural(pos, breaks[i], x, x + width).0 > width + 0.001 {
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
                // Parley aligns the line as shaped, with narrow tabs, in
                // the width that the tabs leave; they widen afterwards.
                let tabs_wider = natural(pos, line_end, x, x + width).1;

                let commit = |breaker: &mut parley::layout::BreakLines<'_, u32>, line_y: f32| {
                    let state = breaker.state_mut();
                    state.set_line_y(f64::from(line_y));
                    state.set_line_x(x);
                    // Justify to the width less the hyphen, drawn after it.
                    state.set_line_max_advance(
                        width - hyphen.as_ref().map_or(0.0, |h| h.3) - tabs_wider,
                    );
                    breaker.break_next_with_length(chars);
                    (breaker.committed_y() as f32) - line_y
                };
                let before_line = breaker.state().clone();
                let laid_out = commit(&mut breaker, y);
                // The line is as tall as the boxes of its runs, as in Pages,
                // which also measures some fonts otherwise than they say
                // (DOC-05). A line without text keeps the height that
                // Parley gave it.
                let line_box = span(pos, line_end)
                    .iter()
                    .map(|c| boxes[c.run])
                    .reduce(RunBox::max);
                let height = line_box.map_or(laid_out, |b| b.over + b.under);
                let mut text_top = y;
                // As Pages measured on the reference machine (FMT-14): the
                // text sits at the top of the line and any extra space goes
                // below it, except with exact spacing, where the text sits
                // on the bottom less its descent and leading. A multiple
                // scales the line without its leading.
                let leading = line_box
                    .or_else(|| boxes.first().copied())
                    .map_or(0.0, |b| b.leading);
                let pitch = match format.line_spacing {
                    LineSpacing::Multiple { percent } => {
                        (height - leading) * percent as f32 / 100.0 + leading
                    }
                    LineSpacing::AtLeast(min) => height.max(min.to_pt() as f32),
                    LineSpacing::Exactly(exact) => {
                        let exact = exact.to_pt() as f32;
                        breaker.revert_to(before_line);
                        text_top = y + (exact - height);
                        commit(&mut breaker, text_top);
                        exact
                    }
                    LineSpacing::Between(extra) => height + extra.to_pt() as f32,
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
                    // The line box; exact spacing moves only the text in it.
                    top: y,
                    height: pitch,
                    text_top,
                    line_box,
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
            // Where Pages puts the baseline, and how far that moves what
            // Parley laid out.
            let baseline = place
                .line_box
                .map_or(metrics.baseline, |b| place.text_top + b.over);
            let delta = baseline - metrics.baseline;
            let mut glyph_runs = Vec::new();
            let mut decorations = Vec::new();
            let mut end_x = place.x;
            let mut descent = None::<f32>;
            let mut carets: Vec<(usize, f32)> = Vec::new();
            // Where the tabs of the line go, from where Parley put the
            // text, and how far each moves what comes after it.
            let tabs = if has_tabs {
                line_tabs(&line, source, &ruler, place.x + place.width)
            } else {
                Vec::new()
            };
            let moved = |offset: usize| -> f32 {
                let i = tabs.partition_point(|t| t.offset < offset);
                if i == 0 { 0.0 } else { tabs[i - 1].moved }
            };
            let mut leaders = Vec::new();
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let format_index = glyph_run.style().brush;
                let format = &text.formats()[format_index as usize];
                let shift = script_shift(format, run.font_size(), run.font_metrics()) - delta;
                let spans = cluster_spans(&glyph_run);
                let positioned: Vec<_> = glyph_run.positioned_glyphs().collect();
                let mut glyphs: Vec<Glyph> = Vec::with_capacity(positioned.len());
                let mut next_glyph = 0;
                for span in &spans {
                    let dx = moved(span.range.start);
                    let count = span.glyphs.min(positioned.len() - next_glyph);
                    // A tab draws no glyph of its own, only its leader.
                    if !source[span.range.start..].starts_with('\t') {
                        glyphs.extend(positioned[next_glyph..next_glyph + count].iter().map(|g| {
                            Glyph {
                                id: g.id,
                                x: g.x + dx,
                                y: g.y - shift,
                            }
                        }));
                    } else if let Some(tab) = tabs.iter().find(|t| t.offset == span.range.start) {
                        leaders.push((
                            tab.place,
                            run.font().clone(),
                            run.font_size(),
                            format_index,
                        ));
                    }
                    next_glyph += count;
                }
                let range = run.text_range();
                let x0 = glyph_run.offset() + moved(range.start);
                let x1 = glyph_run.offset() + glyph_run.advance() + moved(range.end);
                end_x = end_x.max(x1);
                // A cursor stops before each cluster, at its first glyph;
                // in right to left text the start of a cluster is on its
                // right.
                for span in &spans {
                    let left = span.x + moved(span.range.start);
                    let right = span.right + moved(span.range.end);
                    if run.is_rtl() {
                        carets.push((text.to_model(span.range.start), span.right));
                        carets.push((text.to_model(span.range.end), span.x));
                    } else {
                        carets.push((text.to_model(span.range.start), left));
                        carets.push((text.to_model(span.range.end), right));
                    }
                }
                let baseline = glyphs.first().map_or(glyph_run.baseline() - shift, |g| g.y);
                let m = run.font_metrics();
                let run_descent = run_box(format, run.font_size(), m).descent;
                descent = Some(descent.map_or(run_descent, |d| d.max(run_descent)));
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
                let (run_text, clusters) = run_text(source, run, glyphs.len());
                glyph_runs.push(GlyphRun {
                    font: run.font().clone(),
                    size: run.font_size(),
                    format: format_index,
                    color: format.color,
                    text: run_text,
                    clusters,
                    glyphs,
                    x: x0,
                    advance: x1 - x0,
                    baseline,
                });
            }
            for (place, font, size, format_index) in leaders {
                let format = &text.formats()[format_index as usize];
                let baseline = glyph_runs
                    .iter()
                    .find(|r| r.format == format_index)
                    .map_or(metrics.baseline + delta, |r| r.baseline);
                if let Some(run) =
                    leader_run(place, &font, size, format_index, format.color, baseline)
                {
                    glyph_runs.push(run);
                }
            }
            let mut advance =
                metrics.advance - metrics.hanging_advance + tabs.last().map_or(0.0, |t| t.moved);
            if let Some((font, size, id, hyphen_advance)) = &place.hyphen {
                glyph_runs.push(GlyphRun {
                    font: font.clone(),
                    size: *size,
                    format: glyph_runs.last().map_or(0, |r| r.format),
                    color: glyph_runs
                        .last()
                        .map_or(text.formats()[0].color, |r| r.color),
                    text: "-".into(),
                    clusters: vec![(1, 1)],
                    glyphs: vec![Glyph {
                        id: *id,
                        x: end_x,
                        y: baseline,
                    }],
                    x: end_x,
                    advance: *hyphen_advance,
                    baseline,
                });
                advance += hyphen_advance;
            }
            // An empty paragraph has a line over a placeholder past its end.
            let range = line.text_range();
            let range = range.start.min(source.len())..range.end.min(source.len());
            let model_range = text.to_model(range.start)..text.to_model(range.end);
            // Each offset once: where two clusters meet, the start of the
            // later one, and a line without text has its start only.
            carets.retain(|c| model_range.contains(&c.0) || c.0 == model_range.end);
            carets.sort_by_key(|c| c.0);
            carets.dedup_by_key(|c| c.0);
            if carets.is_empty() {
                let x = match format.alignment {
                    Alignment::Center => place.x + place.width / 2.0,
                    Alignment::Right => place.x + place.width,
                    _ => place.x,
                };
                carets.push((model_range.start, x));
            }
            lines.push(Line {
                range: text.to_model(range.start)..text.to_model(range.end),
                x: place.x,
                width: place.width,
                advance,
                top: place.top,
                height: place.height,
                baseline,
                text_bottom: descent.map_or(place.top + place.height, |d| baseline + d),
                runs: glyph_runs,
                decorations,
                hyphenated: place.hyphenated,
                carets,
            });
        }
        let bottom = lines.last().map_or(top, |l| l.top + l.height);
        ParagraphLayout { lines, top, bottom }
    }
}

/// A cluster of a glyph run, in visual order: its text, where it starts
/// and ends, and how many glyphs draw it.
struct ClusterSpan {
    range: Range<usize>,
    x: f32,
    right: f32,
    glyphs: usize,
}

fn cluster_spans(glyph_run: &parley::GlyphRun<'_, u32>) -> Vec<ClusterSpan> {
    let xs: Vec<f32> = glyph_run.positioned_glyphs().map(|g| g.x).collect();
    let end = glyph_run.offset() + glyph_run.advance();
    let mut spans: Vec<ClusterSpan> = Vec::new();
    let mut next = 0;
    for cluster in glyph_run.run().visual_clusters() {
        let glyphs = cluster.glyphs().count();
        spans.push(ClusterSpan {
            range: cluster.text_range(),
            x: xs.get(next).copied().unwrap_or(end),
            right: end,
            glyphs,
        });
        next += glyphs;
    }
    for i in 1..spans.len() {
        spans[i - 1].right = spans[i].x;
    }
    spans
}

/// A tab of a line: where it is in the text, where it went, and how far
/// it moves the text after it, the moves of the tabs before it included.
#[derive(Clone, Copy)]
struct LineTab {
    offset: usize,
    place: TabPlace,
    moved: f32,
}

/// Where the tabs of a laid out line go, given the right edge of the line.
/// Lines of right to left text are left as Parley laid them out.
fn line_tabs(
    line: &parley::Line<'_, u32>,
    source: &str,
    ruler: &TabRuler,
    limit: f32,
) -> Vec<LineTab> {
    let mut spans = Vec::new();
    for item in line.items() {
        if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
            if glyph_run.run().is_rtl() {
                return Vec::new();
            }
            spans.extend(cluster_spans(&glyph_run));
        }
    }
    let first = |s: &ClusterSpan| source[s.range.start..].chars().next().unwrap_or(' ');
    if !spans.iter().any(|s| first(s) == '\t') {
        return Vec::new();
    }
    let pieces = spans.iter().map(|s| Piece {
        tab: first(s) == '\t',
        advance: s.right - s.x,
        first: first(s),
    });
    let (places, _) = ruler.lay(pieces, spans[0].x, limit);
    let mut moved = 0.0;
    spans
        .iter()
        .filter(|s| first(s) == '\t')
        .zip(places)
        .map(|(span, place)| {
            // Where the tab starts once the tabs before it have moved.
            let place = TabPlace {
                start: place.start,
                ..place
            };
            moved += place.width - (span.right - span.x);
            LineTab {
                offset: span.range.start,
                place,
                moved,
            }
        })
        .collect()
}

/// The leader of a tab: its character repeated across the space of the
/// tab, ending where the tab does.
fn leader_run(
    place: TabPlace,
    font: &FontData,
    size: f32,
    format: u32,
    color: Rgba,
    baseline: f32,
) -> Option<GlyphRun> {
    let c = place.stop.leader.character()?;
    let (id, advance) = char_glyph(font, size, c)?;
    if advance <= 0.0 || place.width < advance {
        return None;
    }
    let count = ((place.width + 0.01) / advance).floor() as usize;
    let end = place.start + place.width;
    let x = end - count as f32 * advance;
    Some(GlyphRun {
        font: font.clone(),
        size,
        format,
        color,
        text: std::iter::repeat_n(c, count).collect(),
        clusters: vec![(c.len_utf8() as u32, 1); count],
        glyphs: (0..count)
            .map(|i| Glyph {
                id,
                x: x + i as f32 * advance,
                y: baseline,
            })
            .collect(),
        x,
        advance: count as f32 * advance,
        baseline,
    })
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
///
/// Superscripts and subscripts are shifted once the line is laid out, by
/// [`script_shift`], since Parley rounds shifts to whole points.
/// `family` is the font chosen for the family of the format.
fn properties<'a>(
    format: &'a CharFormat,
    family: &'a str,
    small_caps: bool,
) -> Vec<StyleProperty<'a, u32>> {
    let mut size = format.font_size.to_pt() as f32;
    let raise = format.baseline_shift.to_pt() as f32;
    if format.baseline != Baseline::Normal {
        size *= SCRIPT_SCALE;
    }
    if small_caps {
        size *= SMALL_CAPS_SCALE;
    }
    let mut properties = vec![
        StyleProperty::FontFamily(FontFamily::named(family)),
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
    if pages_font(&format.font_family).is_some() {
        // The ligatures are those of Pages, made with the text (DOC-05).
        properties.push(StyleProperty::FontFeatures("\"liga\" 0, \"clig\" 0".into()));
    }
    if format.language.tag() != "und"
        && let Ok(language) = Language::parse(format.language.tag())
    {
        properties.push(StyleProperty::Locale(Some(language)));
    }
    properties
}

/// How far a run is raised for a superscript, or lowered (negative) for a
/// subscript, as Pages measured on the reference machine (FMT-06): a
/// superscript until its top meets the top of the text at full size, a
/// subscript until its bottom meets the bottom. `size` is the size of the
/// run and `metrics` its font's metrics at that size.
fn script_shift(format: &CharFormat, size: f32, metrics: &parley::FontMetrics) -> f32 {
    let full = format.font_size.to_pt() as f32 / size - 1.0;
    let (ascent, descent, _, _) = vertical_metrics(format, size, metrics);
    match format.baseline {
        Baseline::Normal => 0.0,
        Baseline::Superscript => ascent * full,
        Baseline::Subscript => -descent * full,
    }
}

/// The ascent, descent and leading of a run in points, and whether its
/// leading goes above the text: those Pages uses for its family when they
/// were measured (DOC-05), otherwise those of its font. `size` is the size
/// of the run and `metrics` its font's metrics at that size.
fn vertical_metrics(
    format: &CharFormat,
    size: f32,
    metrics: &parley::FontMetrics,
) -> (f32, f32, f32, bool) {
    match pages_font(&format.font_family) {
        Some(PagesFont {
            ascent,
            descent,
            leading,
            leading_above,
            ..
        }) => (ascent * size, descent * size, leading * size, leading_above),
        None => (metrics.ascent, metrics.descent, metrics.leading, false),
    }
}

/// The box of a run, in whole points. The leading is split around the text
/// as Parley splits it, the larger half below, or goes above it; a shift of
/// the baseline moves the box.
fn run_box(format: &CharFormat, size: f32, metrics: &parley::FontMetrics) -> RunBox {
    let (ascent, descent, leading, leading_above) = vertical_metrics(format, size, metrics);
    let (ascent, descent, leading) = (ascent.round(), descent.round(), leading.round());
    let over = if leading_above {
        ascent + leading
    } else {
        ascent + (leading / 2.0).floor()
    };
    let raise = (format.baseline_shift.to_pt() as f32).round();
    RunBox {
        over: over + raise,
        under: ascent + descent + leading - over - raise,
        leading,
        descent,
    }
}

/// The text that a run draws and its clusters, as [`GlyphRun::text`] and
/// [`GlyphRun::clusters`] describe them, given how many glyphs it has,
/// tabs left out.
fn run_text(source: &str, run: &parley::Run<'_, u32>, glyphs: usize) -> (String, Vec<(u32, u32)>) {
    let mut text = String::new();
    let mut clusters = Vec::new();
    let mut count = 0;
    for cluster in run.visual_clusters() {
        // A tab draws no glyph, and its leader is a run of its own.
        if source[cluster.text_range()].starts_with('\t') {
            continue;
        }
        let start = text.len();
        for c in source[cluster.text_range()].chars() {
            match c {
                FI => text.push_str("fi"),
                FL => text.push_str("fl"),
                c => text.push(c),
            }
        }
        let n = cluster.glyphs().count();
        count += n;
        clusters.push(((text.len() - start) as u32, n as u32));
    }
    if count != glyphs {
        clusters.clear();
    }
    (text, clusters)
}

/// The hyphen glyph of a font and its advance at a size.
fn hyphen_glyph(font: &FontData, size: f32) -> Option<(u32, f32)> {
    char_glyph(font, size, '-')
}

/// The glyph of a character in a font and its advance at a size.
fn char_glyph(font: &FontData, size: f32, c: char) -> Option<(u32, f32)> {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};
    let font = skrifa::FontRef::from_index(font.data.as_ref(), font.index).ok()?;
    let id = font.charmap().map(c)?;
    let advance = font
        .glyph_metrics(Size::new(size), LocationRef::default())
        .advance_width(id)?;
    Some((id.to_u32(), advance))
}
