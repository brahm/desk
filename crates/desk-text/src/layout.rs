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

use desk_model::{
    Alignment, Baseline, CharFormat, LINE_BREAK, Ligatures, LineSpacing, LineStyle, Rgba, Shadow,
};
use icu_segmenter::LineSegmenter;
use icu_segmenter::options::LineBreakOptions;
use parley::layout::{BreakReason, BreakerState};
use parley::style::{FontFamily, FontStyle, FontWeight, Language, LineHeight, StyleProperty};
use parley::{
    AlignmentOptions, FontContext, FontData, Layout, LayoutContext, PositionedLayoutItem,
    VerticalAlign,
};

use crate::fonts::{FontChoice, FontMatch, PagesFont, choose, pages_font};
use crate::input::{FF, FFI, FI, FL, ParagraphText, SMALL_CAPS_SCALE};
use crate::space::LineSpace;
use crate::tabs::{Piece, TabPlace, TabRuler};

/// U+00AD SOFT HYPHEN: a place where a word may break, with a hyphen drawn
/// at the end of the line when it does.
pub const SOFT_HYPHEN: char = '\u{ad}';

/// The size of superscripts and subscripts, relative to the font size, as
/// measured in Pages (FMT-06).
pub const SCRIPT_SCALE: f32 = 2.0 / 3.0;

/// The width that a list label is laid out in, wider than any label.
const LABEL_ROOM: f32 = 10_000.0;

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
    /// Drawn as an outline (FMT-04).
    pub outline: bool,
    /// The shadow under the run (FMT-03).
    pub shadow: Option<Shadow>,
    /// The color behind the run, over the height of the line (FMT-20).
    pub background: Option<Rgba>,
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
    /// How much of the ascent of its text Pages rounded off the line box:
    /// it draws the background of a paragraph that much higher (FMT-20).
    pub rise: f32,
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
    /// From the left of the paragraph, where the label of a list item is,
    /// on the first line of the item: what a drag moves to another level
    /// (LST-02).
    pub label: Option<Range<f32>>,
    /// The picture of an image bullet on the first line of an item
    /// (LST-03).
    pub label_image: Option<LabelImage>,
}

/// The picture of an image bullet, where it is drawn: from the left and
/// the top of the paragraph, as glyphs are placed.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelImage {
    /// The picture.
    pub picture: desk_model::Picture,
    /// The left edge.
    pub x: f32,
    /// The top edge.
    pub y: f32,
    /// The width.
    pub width: f32,
    /// The height.
    pub height: f32,
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

/// The size of the space that stands for an object in the text, small
/// enough for the width of the space to be nothing beside the object's.
const OBJECT_FONT_SIZE: f32 = 0.01;

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
    /// How much of the ascent the rounding took off, at most.
    rise: f32,
}

impl RunBox {
    fn max(self, other: Self) -> Self {
        Self {
            over: self.over.max(other.over),
            under: self.under.max(other.under),
            leading: self.leading.max(other.leading),
            descent: self.descent.max(other.descent),
            rise: self.rise.max(other.rise),
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

    /// Adds the fonts of a font file to those that lay out text, as if
    /// they were installed, such as the test fonts of the corpus. Gives
    /// back how many fonts the data held.
    pub fn add_fonts(&mut self, data: Vec<u8>) -> usize {
        let blob = parley::fontique::Blob::new(std::sync::Arc::new(data));
        let added = self.fonts.collection.register_fonts(blob, None);
        // A family chosen before may now be installed.
        self.choices.clear();
        added.iter().map(|(_, fonts)| fonts.len()).sum()
    }

    /// The font that lays out a family: the family itself when it is
    /// installed, or the free font that stands for it (DOC-05).
    pub fn font_choice(&mut self, family: &str) -> FontChoice {
        if let Some(choice) = self.choices.get(family) {
            return choice.clone();
        }
        let collection = &mut self.fonts.collection;
        // The family from a PostScript name may be spaced otherwise than
        // the installed one: DeskSansSpace60 for "DeskSans Space60".
        let squeezed = |name: &str| -> String {
            name.chars()
                .filter(|c| !c.is_whitespace())
                .flat_map(char::to_lowercase)
                .collect()
        };
        let installed = if collection.family_id(family).is_none() {
            let want = squeezed(family);
            collection
                .family_names()
                .find(|name| squeezed(name) == want)
                .map(str::to_owned)
        } else {
            None
        };
        let choice = match installed {
            Some(name) => FontChoice {
                requested: family.into(),
                used: name.into(),
                kind: FontMatch::Installed,
            },
            None => choose(family, |name| collection.family_id(name).is_some()),
        };
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
        // An object in the text is a tiny space, spaced out to its
        // width (OBJ-01).
        for range in &text.ranges {
            if let Some((width, _)) = text.object_box(range.format) {
                builder.push(
                    StyleProperty::FontSize(OBJECT_FONT_SIZE),
                    range.range.clone(),
                );
                builder.push(StyleProperty::LetterSpacing(width), range.range.clone());
            }
        }
        // A place where a word may break draws nothing, so tracking adds
        // no space there.
        for (at, _) in text.text().match_indices(SOFT_HYPHEN) {
            builder.push(
                StyleProperty::LetterSpacing(0.0),
                at..at + SOFT_HYPHEN.len_utf8(),
            );
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
        let mut format = text.paragraph().clone();
        // The label of a list item, laid out on its own: the text of the
        // item starts after it, on every line (LST-02).
        let label = text.label().map(|label| {
            let label_format = text.formats()[label.format as usize].clone();
            let label_text = ParagraphText::from_runs(
                desk_model::ParaFormat::default(),
                label_format.clone(),
                [(label.text.as_str(), label_format)],
            );
            let laid = self.layout(&label_text, &crate::space::Column::new(LABEL_ROOM), 0.0);
            let width = match &label.image {
                Some((picture, height)) => *height * picture.aspect() as f32,
                None => laid.lines.first().map_or(0.0, |l| l.advance),
            };
            let x = format.left_indent.to_pt() as f32 + label.indent;
            format.left_indent =
                desk_model::Length::from_pt(f64::from(x + label.text_indent.max(width)));
            format.first_line_indent = desk_model::Length::ZERO;
            (laid, x, label.format, label.raise, label.image.clone())
        });
        let mut layout = self.build(text);
        let source = text.text();

        // Shaped widths, from the paragraph on one line (or one line per
        // manual line break).
        layout.break_all_lines(None);
        let mut runs: Vec<(FontData, f32)> = Vec::new();
        // The tracking of each run, in points after each of its letters.
        let mut tracking: Vec<f32> = Vec::new();
        // The box of each run, rounded as the line heights are.
        let mut boxes: Vec<RunBox> = Vec::new();
        // Whether each run is an object in the text, and the box of the
        // text of each run: for an object, of the text it stands in.
        let mut objects: Vec<bool> = Vec::new();
        let mut text_boxes: Vec<RunBox> = Vec::new();
        let mut clusters: Vec<Cluster> = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                    let run = glyph_run.run();
                    runs.push((run.font().clone(), run.font_size()));
                    let brush = glyph_run.style().brush;
                    let format = &text.formats()[brush as usize];
                    let object = text.object_box(brush).is_some();
                    tracking.push(if object {
                        0.0
                    } else {
                        format.tracking as f32 / 1000.0 * run.font_size()
                    });
                    objects.push(object);
                    text_boxes.push(if object {
                        // Shaped tiny: measured at the size of its text.
                        let size = format.font_size.to_pt() as f32;
                        let k = size / run.font_size().max(f32::MIN_POSITIVE);
                        let metrics = run.font_metrics();
                        let scaled = parley::FontMetrics {
                            ascent: metrics.ascent * k,
                            descent: metrics.descent * k,
                            leading: metrics.leading * k,
                            ..*metrics
                        };
                        run_box(format, size, &scaled)
                    } else {
                        run_box(format, run.font_size(), run.font_metrics())
                    });
                    boxes.push(match text.object_box(brush) {
                        // An object stands on the baseline.
                        Some((_, height)) => RunBox {
                            over: height,
                            under: 0.0,
                            leading: 0.0,
                            descent: 0.0,
                            rise: 0.0,
                        },
                        None => run_box(format, run.font_size(), run.font_metrics()),
                    });
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
        let mut breaks: Vec<usize> = LineSegmenter::new_auto(LineBreakOptions::default())
            .segment_str(source)
            .filter(|&b| b > 0)
            .collect();
        // As in Pages, a line may break before an object in the text, even
        // with no space between it and a word (OBJ-18): an object too wide
        // for what is left of the line starts the next.
        breaks.extend(
            clusters
                .iter()
                .filter(|c| objects[c.run] && c.start > 0)
                .map(|c| c.start),
        );
        breaks.sort_unstable();
        breaks.dedup();

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
        // Whether the band was measured again with a shorter line.
        let mut shortened = false;
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
                // An object stands on the baseline of the text it is in, so
                // the line keeps the part of that text below the baseline,
                // whatever the spacing (DOC-05).
                let height = line_box.map_or(laid_out, |b| {
                    let under = span(pos, line_end)
                        .iter()
                        .filter(|c| objects[c.run])
                        .map(|c| text_boxes[c.run].under)
                        .fold(b.under, f32::max);
                    b.over + under
                });
                let mut text_top = y;
                // As Pages measured on the reference machine (FMT-14): the
                // text sits at the top of the line and any extra space goes
                // below it, except with exact spacing, where the text sits
                // on the bottom less its descent and leading. A multiple
                // scales the line without its leading, to a whole point.
                let leading = line_box
                    .or_else(|| boxes.first().copied())
                    .map_or(0.0, |b| b.leading);
                let pitch = match format.line_spacing {
                    // An object in the text is not scaled: the multiple
                    // scales the text of the line, and the object only
                    // makes it taller above its baseline, as Pages lays
                    // out a picture as tall as the page (DOC-05).
                    LineSpacing::Multiple { percent } => {
                        let scale = |h: f32, leading: f32| {
                            ((h - leading) * percent as f32 / 100.0 + leading).round()
                        };
                        let cs = span(pos, line_end);
                        let object_over = cs
                            .iter()
                            .filter(|c| objects[c.run])
                            .map(|c| boxes[c.run].over)
                            .reduce(f32::max);
                        let text_box = cs.iter().map(|c| text_boxes[c.run]).reduce(RunBox::max);
                        match (object_over, text_box) {
                            (Some(over), Some(b)) if over > b.over => {
                                scale(b.over + b.under, b.leading) + over - b.over
                            }
                            _ => scale(height, leading),
                        }
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
                if pitch + 0.001 < guess
                    && !shortened
                    && space.segments(y, pitch) != space.segments(y, guess)
                {
                    // Shorter than the band was measured for, next to an
                    // object: the line takes only its own height from it,
                    // as in Pages (OBJ-17).
                    breaker.revert_to(saved);
                    (pos, next_break) = (saved_pos, saved_next);
                    placed.truncate(saved_placed);
                    guess = pitch;
                    shortened = true;
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
            shortened = false;
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
            // How far justification moves each cluster from where Parley
            // put it, as Pages spreads the spaces and the letters. In a
            // line with tabs, only the text after a last left tab is
            // justified, from its stop.
            let last_tab = tabs.last();
            let spread = if format.alignment == Alignment::Justify
                && last_tab.is_none_or(|t| t.place.stop.alignment == desk_model::TabAlignment::Left)
                && !matches!(
                    line.break_reason(),
                    BreakReason::None | BreakReason::Explicit
                ) {
                let range = line.text_range();
                let from = last_tab.map_or(range.start, |t| t.offset + '\t'.len_utf8());
                let clusters = span(from, trim(range.start, range.end));
                // Pages tracks no letter after the last of the line.
                let last = clusters
                    .last()
                    .filter(|c| !source[c.start..].starts_with([' ', '\u{a0}']))
                    .and_then(|c| tracking.get(c.run))
                    .copied()
                    .unwrap_or(0.0);
                let hyphen = place.hyphen.as_ref().map_or(0.0, |h| h.3);
                let (given, extra) = match last_tab {
                    None => {
                        let given =
                            place.width - hyphen - (metrics.advance - metrics.hanging_advance);
                        (given, given + last)
                    }
                    Some(tab) => {
                        // Where Parley put the text after the tab, and how
                        // much it widened its spaces.
                        let spans: Vec<ClusterSpan> = line
                            .items()
                            .filter_map(|item| match item {
                                PositionedLayoutItem::GlyphRun(g) => Some(cluster_spans(&g)),
                                _ => None,
                            })
                            .flatten()
                            .collect();
                        let x = |c: &Cluster| {
                            spans.iter().find(|s| s.range.start == c.start).map(|s| s.x)
                        };
                        match (clusters.first(), clusters.last()) {
                            (Some(first), Some(end)) if clusters.len() > 1 => {
                                match (x(first), x(end)) {
                                    (Some(start), Some(stop)) => {
                                        let widths: f32 = clusters.iter().map(|c| c.advance).sum();
                                        let given = stop - start - (widths - end.advance);
                                        let extra = place.x + place.width
                                            - hyphen
                                            - (start + tab.moved)
                                            - widths
                                            + last;
                                        (given, extra)
                                    }
                                    _ => (0.0, 0.0),
                                }
                            }
                            _ => (0.0, 0.0),
                        }
                    }
                };
                letter_spread(clusters, source, given, extra, |c| {
                    runs.get(c.run).map_or(0.0, |r| r.1)
                })
            } else {
                Vec::new()
            };
            let moved = |offset: usize| -> f32 {
                let i = tabs.partition_point(|t| t.offset < offset);
                let tab = if i == 0 { 0.0 } else { tabs[i - 1].moved };
                let j = spread.partition_point(|s| s.0 <= offset);
                tab + if j == 0 { 0.0 } else { spread[j - 1].1 }
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
                let range = match (
                    spans.iter().map(|s| s.range.start).min(),
                    spans.iter().map(|s| s.range.end).max(),
                ) {
                    (Some(start), Some(end)) => start..end,
                    _ => run.text_range(),
                };
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
                        color: match format.underline_color {
                            desk_model::LineColor::Text => format.color,
                            desk_model::LineColor::Custom(color) => color,
                        },
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
                let (run_text, clusters) =
                    run_text(source, run, glyph_run.style_index(), glyphs.len());
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
                    outline: format.outline,
                    shadow: format.shadow,
                    background: format.background,
                });
            }
            for (place, font, size, format_index) in leaders {
                let format = &text.formats()[format_index as usize];
                let baseline = glyph_runs
                    .iter()
                    .find(|r| r.format == format_index)
                    .map_or(metrics.baseline + delta, |r| r.baseline);
                if let Some(run) = leader_run(place, &font, size, format_index, format, baseline) {
                    glyph_runs.push(run);
                }
            }
            let mut advance =
                metrics.advance - metrics.hanging_advance + tabs.last().map_or(0.0, |t| t.moved);
            if let Some((font, size, id, hyphen_advance)) = &place.hyphen {
                let last = glyph_runs.last();
                let (outline, shadow, background) =
                    last.map_or((false, None, None), |r| (r.outline, r.shadow, r.background));
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
                    outline,
                    shadow,
                    background,
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
                rise: place.line_box.map_or(0.0, |b| b.rise),
                runs: glyph_runs,
                decorations,
                hyphenated: place.hyphenated,
                carets,
                label: None,
                label_image: None,
            });
        }
        if let (Some((laid, x, format, raise, image)), Some(first)) = (label, lines.first_mut()) {
            let baseline = first.baseline - raise;
            if let Some((picture, height)) = image {
                // As Pages draws it: a third of the picture below the
                // baseline of the line, which the offset of the label
                // does not move (LST-03).
                let width = height * picture.aspect() as f32;
                first.label = Some(x..x + width);
                first.label_image = Some(LabelImage {
                    picture,
                    x,
                    y: first.baseline - height * 2.0 / 3.0,
                    width,
                    height,
                });
                let bottom = lines.last().map_or(top, |l| l.top + l.height);
                return ParagraphLayout { lines, top, bottom };
            }
            let width = laid.lines.first().map_or(0.0, |l| l.advance);
            first.label = Some(x..x + width);
            let runs = laid.lines.into_iter().take(1).flat_map(|line| {
                let shift = baseline - line.baseline;
                line.runs.into_iter().map(move |mut run| {
                    for glyph in &mut run.glyphs {
                        glyph.x += x;
                        glyph.y += shift;
                    }
                    run.x += x;
                    run.baseline += shift;
                    run.format = format;
                    run
                })
            });
            first.runs.splice(0..0, runs);
        }
        let bottom = lines.last().map_or(top, |l| l.top + l.height);
        ParagraphLayout { lines, top, bottom }
    }
}

/// The widest that each space of a justified line grows before the
/// letters are spread too, in ems of the space (FMT-12).
const WIDEST_SPACE_GROWTH: f32 = 0.5;

/// The most that the gap between two letters of a justified line grows,
/// in ems of the letter; past it the rest goes to the spaces again (FMT-12).
const WIDEST_LETTER_GROWTH: f32 = 0.2892;

/// How Pages justifies a line, as measured on the reference machine
/// (FMT-12). Each space grows in proportion to its own size, up to half
/// of it. Past that, the letters of the words spread too: before each
/// letter that follows another the gap grows by `l` times the size of the
/// letter, and each space by half its size and half `l` times the size of
/// what follows it, `l` being alike in the whole line and at most 0.2892;
/// past that the spaces take the rest, again in proportion to their size.
/// `size` gives the size of the text of a cluster. Gives, for each cluster
/// from which the text moves, the offset where it starts and how far it
/// and what comes after it move from where Parley put them, with the
/// `given` room alike in the spaces, when Pages has `extra` room to spread;
/// nothing when Parley put them right.
fn letter_spread(
    clusters: &[Cluster],
    source: &str,
    given: f32,
    extra: f32,
    size: impl Fn(&Cluster) -> f32,
) -> Vec<(usize, f32)> {
    let is_space = |c: &Cluster| source[c.start..].starts_with([' ', '\u{a0}']);
    let words = clusters.iter().filter(|c| is_space(c)).count() as f32;
    if words == 0.0 || extra <= 0.0 {
        return Vec::new();
    }
    // The sizes of the spaces, of what follows them and of the letters
    // that follow letters.
    let spaces: f32 = clusters.iter().filter(|c| is_space(c)).map(&size).sum();
    let after_spaces: f32 = clusters
        .windows(2)
        .filter(|p| is_space(&p[0]) && !is_space(&p[1]))
        .map(|p| size(&p[1]))
        .sum();
    let letters: f32 = clusters
        .windows(2)
        .filter(|p| !is_space(&p[0]) && !is_space(&p[1]))
        .map(|p| size(&p[1]))
        .sum();
    if spaces <= 0.0 {
        return Vec::new();
    }
    // How much each point of a space grows, and each point of a letter.
    let (mut grow, mut between) = (extra / spaces, 0.0);
    if grow > WIDEST_SPACE_GROWTH && letters > 0.0 {
        between = (extra - WIDEST_SPACE_GROWTH * spaces) / (letters + after_spaces / 2.0);
        grow = WIDEST_SPACE_GROWTH;
        if between > WIDEST_LETTER_GROWTH {
            between = WIDEST_LETTER_GROWTH;
            grow = (extra - between * (letters + after_spaces / 2.0)) / spaces;
        }
    }
    let each = given / words;
    let mut moved = 0.0;
    let mut spread = Vec::new();
    let mut previous: Option<&Cluster> = None;
    for cluster in clusters {
        let Some(before) = previous.replace(cluster) else {
            continue;
        };
        moved += match (is_space(before), is_space(cluster)) {
            (false, false) => between * size(cluster),
            (true, false) => grow * size(before) + between * size(cluster) / 2.0 - each,
            // A space after a space, at the end of what is spread.
            (true, true) => grow * size(before) - each,
            (false, true) => continue,
        };
        spread.push((cluster.start, moved));
    }
    if spread.iter().all(|s| s.1.abs() < 1e-3) {
        return Vec::new();
    }
    spread
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
    // A run of one font can hold glyph runs of several formats.
    let style = glyph_run.style_index();
    for cluster in glyph_run.run().visual_clusters() {
        if cluster.style_index() != style {
            continue;
        }
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
    index: u32,
    format: &CharFormat,
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
        format: index,
        color: format.color,
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
        outline: format.outline,
        shadow: format.shadow,
        // The background of the tab, which spans the whole of it, is drawn
        // with the tab.
        background: None,
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
        StyleProperty::FontWeight(FontWeight::new(f32::from(format.weight.value()))),
        StyleProperty::FontStyle(if format.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        }),
        StyleProperty::VerticalAlign(VerticalAlign::length(raise)),
    ];
    let pages = pages_font(&format.font_family, format.weight.value());
    // A family laid out as in Pages spaces its letters so that its lines
    // are as wide as there, whatever font stands for it (DOC-05).
    let spacing = format.tracking as f32 / 1000.0 + pages.map_or(0.0, |f| f.spacing);
    if spacing != 0.0 {
        properties.push(StyleProperty::LetterSpacing(spacing * size));
    }
    properties.push(StyleProperty::FontFeatures(parley::FontFeatures::Source(
        font_features(format, pages.is_some()).into(),
    )));
    if format.language.tag() != "und"
        && let Ok(language) = Language::parse(format.language.tag())
    {
        properties.push(StyleProperty::Locale(Some(language)));
    }
    properties
}

/// The features of the font for a format, in the syntax of CSS
/// `font-feature-settings` (FMT-08, FMT-09). In the fonts of Pages, the
/// default ligatures are made with the text (DOC-05), and the font makes
/// none of its own.
fn font_features(format: &CharFormat, pages: bool) -> String {
    let mut features: Vec<String> = Vec::new();
    if pages || format.ligatures == Ligatures::None {
        features.push("\"liga\" 0".into());
        features.push("\"clig\" 0".into());
    }
    if format.ligatures == Ligatures::All {
        features.push("\"dlig\" 1".into());
    }
    for feature in format.font_features.features() {
        features.push(format!("\"{}\" {}", feature.tag(), feature.value));
    }
    features.join(", ")
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

/// Where the leading of a line goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeadingPlace {
    /// Over the text, as Pages puts it with Apple's own fonts.
    Above,
    /// Under the text, as Pages puts it with fonts that have a line gap.
    Below,
    /// Split around the text, as Parley splits it, the larger half below.
    Around,
}

/// The ascent, descent and leading of a run in points, and where its
/// leading goes: those Pages uses for its family and weight when they were
/// measured (DOC-05), otherwise those of its font. `size` is the size of
/// the run and `metrics` its font's metrics at that size.
fn vertical_metrics(
    format: &CharFormat,
    size: f32,
    metrics: &parley::FontMetrics,
) -> (f32, f32, f32, LeadingPlace) {
    match pages_font(&format.font_family, format.weight.value()) {
        Some(PagesFont {
            ascent,
            descent,
            leading,
            leading_above,
            ..
        }) => (
            ascent * size,
            descent * size,
            leading * size,
            if leading_above {
                LeadingPlace::Above
            } else {
                LeadingPlace::Below
            },
        ),
        None => (
            metrics.ascent,
            metrics.descent,
            metrics.leading,
            LeadingPlace::Around,
        ),
    }
}

/// The box of a run, in whole points: Pages rounds the ascent, the descent
/// and the leading each to a whole point. A shift of the baseline moves
/// the box.
fn run_box(format: &CharFormat, size: f32, metrics: &parley::FontMetrics) -> RunBox {
    let (exact, descent, leading, place) = vertical_metrics(format, size, metrics);
    let (ascent, descent, leading) = (exact.round(), descent.round(), leading.round());
    let over = match place {
        LeadingPlace::Above => ascent + leading,
        LeadingPlace::Below => ascent,
        LeadingPlace::Around => ascent + (leading / 2.0).floor(),
    };
    let raise = (format.baseline_shift.to_pt() as f32).round();
    RunBox {
        over: over + raise,
        under: ascent + descent + leading - over - raise,
        leading,
        descent,
        rise: (exact - ascent).max(0.0),
    }
}

/// The text that a run draws and its clusters, as [`GlyphRun::text`] and
/// [`GlyphRun::clusters`] describe them, given how many glyphs it has,
/// tabs left out.
fn run_text(
    source: &str,
    run: &parley::Run<'_, u32>,
    style: u16,
    glyphs: usize,
) -> (String, Vec<(u32, u32)>) {
    let mut text = String::new();
    let mut clusters: Vec<(u32, u32)> = Vec::new();
    let mut count = 0;
    // The text of clusters without glyphs before the first with any.
    let mut pending = 0;
    for cluster in run.visual_clusters() {
        if cluster.style_index() != style {
            continue;
        }
        // A tab draws no glyph, and its leader is a run of its own.
        if source[cluster.text_range()].starts_with('\t') {
            continue;
        }
        let start = text.len();
        for c in source[cluster.text_range()].chars() {
            match c {
                FI => text.push_str("fi"),
                FL => text.push_str("fl"),
                FF => text.push_str("ff"),
                FFI => text.push_str("ffi"),
                // A place where the word may break is not text to copy.
                SOFT_HYPHEN => {}
                c => text.push(c),
            }
        }
        let n = cluster.glyphs().count();
        count += n;
        let bytes = (text.len() - start) as u32;
        if n == 0 && cluster.is_ligature_continuation() {
            // A letter that a ligature of the font joins to the one before
            // it draws no glyph of its own: it goes with that ligature, so
            // that the ligature reads as all its letters.
            match clusters.last_mut() {
                Some(last) => last.0 += bytes,
                None => pending += bytes,
            }
        } else if n > 0 {
            clusters.push((bytes + std::mem::take(&mut pending), n as u32));
        } else if bytes > 0 {
            clusters.push((bytes, 0));
        }
    }
    if pending > 0
        && let Some(last) = clusters.last_mut()
    {
        last.0 += pending;
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
