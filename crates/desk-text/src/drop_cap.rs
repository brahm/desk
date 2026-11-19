// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drop caps (PXV-06): the first characters of a paragraph, made as tall
//! as some of its lines, with those lines wrapped beside them, as Pages
//! 15.4 draws them. The characters stand on the baseline of the last of
//! those lines, and their capitals reach the capitals of the first.

use desk_model::{DropCap, LINE_BREAK, Length, ParaFormat};
use parley::PositionedLayoutItem;

use crate::input::ParagraphText;
use crate::layout::{DropCapBox, GlyphRun, ParagraphLayout, TextSystem};
use crate::space::{Column, LineSpace};

/// The width that the characters are laid out in, wider than any.
const ROOM: f32 = 10_000.0;

/// The space of the lines beside a drop cap: the column, less the room of
/// the characters down to their baseline.
struct Beside<'a> {
    inner: &'a dyn LineSpace,
    top: f32,
    bottom: f32,
    /// Where the text may start beside the characters, for a line from a
    /// top and of a height.
    right: Box<dyn Fn(f32, f32) -> f32 + 'a>,
}

impl LineSpace for Beside<'_> {
    fn width(&self) -> f32 {
        self.inner.width()
    }

    fn segments(&self, top: f32, height: f32) -> Vec<(f32, f32)> {
        let segments = self.inner.segments(top, height);
        if top >= self.bottom || top + height <= self.top {
            return segments;
        }
        let right = (self.right)(top, height);
        segments
            .into_iter()
            .filter_map(|(x, w)| {
                let x0 = x.max(right);
                (x + w > x0).then_some((x0, x + w - x0))
            })
            .collect()
    }

    fn is_exhausted(&self, top: f32) -> bool {
        self.inner.is_exhausted(top)
    }
}

/// A straight piece of an outline, from one point to another.
type Segment = ((f32, f32), (f32, f32));

/// The outlines of glyphs, flattened into segments.
#[derive(Default)]
struct Segments {
    /// The segments, in the units of the font at its size, y up.
    lines: Vec<Segment>,
    at: (f32, f32),
    start: (f32, f32),
}

impl Segments {
    fn to(&mut self, point: (f32, f32)) {
        self.lines.push((self.at, point));
        self.at = point;
    }
}

impl skrifa::outline::OutlinePen for Segments {
    fn move_to(&mut self, x: f32, y: f32) {
        self.at = (x, y);
        self.start = (x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.to((x, y));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (x0, y0) = self.at;
        for i in 1..=8 {
            let t = i as f32 / 8.0;
            let u = 1.0 - t;
            self.to((
                u * u * x0 + 2.0 * u * t * cx + t * t * x,
                u * u * y0 + 2.0 * u * t * cy + t * t * y,
            ));
        }
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (x0, y0) = self.at;
        for i in 1..=8 {
            let t = i as f32 / 8.0;
            let u = 1.0 - t;
            self.to((
                u * u * u * x0 + 3.0 * u * u * t * cx0 + 3.0 * u * t * t * cx1 + t * t * t * x,
                u * u * u * y0 + 3.0 * u * u * t * cy0 + 3.0 * u * t * t * cy1 + t * t * t * y,
            ));
        }
    }

    fn close(&mut self) {
        let start = self.start;
        self.to(start);
    }
}

/// The outlines of the glyphs of runs, as segments in the coordinates of
/// the layout.
fn ink(runs: &[GlyphRun]) -> Vec<Segment> {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};
    use skrifa::outline::DrawSettings;
    let mut out = Vec::new();
    for run in runs {
        let Ok(font) = skrifa::FontRef::from_index(run.font.data.as_ref(), run.font.index) else {
            continue;
        };
        let outlines = font.outline_glyphs();
        for glyph in &run.glyphs {
            let Some(outline) = outlines.get(skrifa::GlyphId::new(glyph.id)) else {
                continue;
            };
            let mut pen = Segments::default();
            let settings = DrawSettings::unhinted(Size::new(run.size), LocationRef::default());
            if outline.draw(settings, &mut pen).is_err() {
                continue;
            }
            let place = |(x, y): (f32, f32)| (glyph.x + x, glyph.y - y);
            out.extend(pen.lines.into_iter().map(|(a, b)| (place(a), place(b))));
        }
    }
    out
}

/// The rightmost ink of segments between two heights.
fn right_of(ink: &[Segment], top: f32, bottom: f32) -> Option<f32> {
    ink.iter()
        .filter_map(|&((x0, y0), (x1, y1))| {
            if y0.max(y1) < top || y0.min(y1) > bottom {
                return None;
            }
            // The segment cut to the band: x at each end of what is in it.
            let x_at = |y: f32| x0 + (x1 - x0) * (y - y0) / (y1 - y0);
            let ends = if (y1 - y0).abs() < f32::EPSILON {
                [x0, x1]
            } else {
                [x_at(y0.clamp(top, bottom)), x_at(y1.clamp(top, bottom))]
            };
            Some(ends[0].max(ends[1]))
        })
        .reduce(f32::max)
}

impl TextSystem {
    /// The height of the capitals of the font of a format, in points.
    fn cap_height(&mut self, text: &ParagraphText) -> f32 {
        let mut layout = self.build(text);
        layout.break_all_lines(None);
        layout
            .lines()
            .flat_map(|line| line.items().collect::<Vec<_>>())
            .find_map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => {
                    let run = run.run();
                    let metrics = run.font_metrics();
                    Some(metrics.cap_height.unwrap_or(metrics.ascent * 0.7))
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Lays out a paragraph that starts with a drop cap, or gives `None`
    /// when it has nothing after the characters for them to drop beside.
    pub(crate) fn layout_drop_cap(
        &mut self,
        text: &ParagraphText,
        cap: DropCap,
        space: &dyn LineSpace,
        top: f32,
    ) -> Option<ParagraphLayout> {
        let source = text.text();
        // The characters, up to a tab or a line break.
        let end = source
            .char_indices()
            .take(usize::from(cap.characters.max(1)))
            .take_while(|&(_, c)| c != '\t' && c != LINE_BREAK)
            .last()
            .map(|(i, c)| i + c.len_utf8())?;
        if end >= source.len() || cap.lines < 2 {
            return None;
        }
        let rest = text.without_start(end);
        let format = &text.formats()[text.format_at(0) as usize];
        // The lines as they would be with no drop cap: where the first
        // baseline is, and how far apart the lines are.
        let plain = self.layout(&rest, &Column::new(space.width()), top);
        let first = plain.lines.first()?;
        let pitch = first.height;
        let first_baseline = first.baseline - top;
        let lines = f32::from(cap.lines);
        // The lines the letters rise above the text, which moves down by
        // as many (PXV-06).
        let raised = f32::from(cap.raised.min(cap.lines - 1));
        let text_top = top + raised * pitch;
        let body = ParagraphText::from_runs(
            ParaFormat::default(),
            format.clone(),
            [(&source[..end], format.clone())],
        );
        let body_caps = self.cap_height(&body);
        let size = format.font_size.to_pt() as f32;
        if body_caps <= 0.0 || size <= 0.0 {
            return None;
        }
        // Capitals as tall as the lines, from capitals to baseline: the
        // height of the shape behind them, when they have one, with the
        // letters smaller in it.
        let height = (lines - 1.0) * pitch + body_caps;
        let scale = cap
            .background
            .map_or(1.0, |shape| f32::from(shape.scale.clamp(10, 100)) / 100.0);
        let cap_size = height / (body_caps / size) * scale;
        let mut big = format.clone();
        big.font_size = Length::from_pt(f64::from(cap_size));
        let letters =
            ParagraphText::from_runs(ParaFormat::default(), big.clone(), [(&source[..end], big)]);
        let laid = self.layout(&letters, &Column::new(ROOM), 0.0);
        let line = laid.lines.into_iter().next()?;
        let paragraph = text.paragraph();
        let indent = (paragraph.left_indent + paragraph.first_line_indent).to_pt() as f32;
        let distance = cap.distance.to_pt() as f32;
        let baseline = text_top + first_baseline + (lines - raised - 1.0) * pitch;
        // The shape is as wide as it is tall, or as the letters with as
        // much room on each side as above and below them.
        let margin = height * (1.0 - scale) / 2.0;
        let width = match cap.background {
            Some(_) => height.max(line.advance + 2.0 * margin),
            None => line.advance,
        };
        // Outdented, the letters stand out into the margin by a part of
        // their width: all of it at 100 %.
        let left = indent - width * f32::from(cap.outdent.min(200)) / 100.0;
        let shape = cap.background.map(|shape| DropCapBox {
            x: left,
            y: baseline - height,
            width,
            height,
            radius: f32::from(shape.radius.min(100)) / 100.0 * width.min(height) / 2.0,
            shape,
        });
        let x = left + (width - line.advance) / 2.0;
        let (dx, dy) = (x - line.x, baseline - margin - line.baseline);
        let runs: Vec<GlyphRun> = line
            .runs
            .iter()
            .cloned()
            .map(|mut run| {
                run.x += dx;
                run.baseline += dy;
                for glyph in &mut run.glyphs {
                    glyph.x += dx;
                    glyph.y += dy;
                }
                run
            })
            .collect();
        // The text beside the letters starts after them: after their
        // rectangle or their shape, or after their ink at the height of
        // each line, as far from it as the letters end past their ink;
        // never before the indent, where outdented letters leave the text.
        let rectangle = (left + width + distance).max(indent);
        let right: Box<dyn Fn(f32, f32) -> f32> = if cap.follow_shape && shape.is_none() {
            let ink = ink(&runs);
            let bearing = right_of(&ink, f32::MIN, f32::MAX)
                .map_or(0.0, |widest| (x + line.advance - widest).max(0.0));
            Box::new(move |top, height| {
                right_of(&ink, top, top + height).map_or(indent, |r| {
                    (r + bearing + distance).clamp(indent, rectangle)
                })
            })
        } else {
            Box::new(move |_, _| rectangle)
        };
        let beside = Beside {
            inner: space,
            top: text_top,
            bottom: baseline,
            right,
        };
        let mut layout = self.layout(&rest, &beside, text_top);
        layout.top = top;
        let bottom = baseline - margin + (line.text_bottom - line.baseline);
        layout.bottom = layout.bottom.max(bottom).max(baseline);
        let first = layout.lines.first_mut()?;
        first.drop_cap_shape = shape;
        first.range.start = 0;
        let mut runs = runs;
        runs.append(&mut first.runs);
        first.runs = runs;
        first
            .decorations
            .extend(line.decorations.into_iter().map(|mut d| {
                d.x0 += dx;
                d.x1 += dx;
                d.y += dy;
                d
            }));
        // A cursor stops before each of the characters, where they are
        // drawn; the one after them stays at the start of the line.
        let mut carets: Vec<(usize, f32)> = line
            .carets
            .into_iter()
            .filter(|&(offset, _)| offset < end)
            .map(|(offset, at)| (text.to_model(offset), at + dx))
            .collect();
        carets.append(&mut first.carets);
        first.carets = carets;
        Some(layout)
    }
}
