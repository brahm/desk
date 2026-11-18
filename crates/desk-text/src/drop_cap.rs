// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drop caps (PXV-06): the first characters of a paragraph, made as tall
//! as some of its lines, with those lines wrapped beside them, as Pages
//! 15.4 draws them. The characters stand on the baseline of the last of
//! those lines, and their capitals reach the capitals of the first.

use desk_model::{DropCap, LINE_BREAK, Length, ParaFormat};
use parley::PositionedLayoutItem;

use crate::input::ParagraphText;
use crate::layout::{ParagraphLayout, TextSystem};
use crate::space::{Column, LineSpace};

/// The width that the characters are laid out in, wider than any.
const ROOM: f32 = 10_000.0;

/// The space of the lines beside a drop cap: the column, less the room of
/// the characters down to their baseline.
struct Beside<'a> {
    inner: &'a dyn LineSpace,
    top: f32,
    bottom: f32,
    right: f32,
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
        segments
            .into_iter()
            .filter_map(|(x, w)| {
                let x0 = x.max(self.right);
                (x + w > x0).then_some((x0, x + w - x0))
            })
            .collect()
    }

    fn is_exhausted(&self, top: f32) -> bool {
        self.inner.is_exhausted(top)
    }
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
        // Capitals from the capitals of the first line to the baseline of
        // the last.
        let cap_size = ((lines - 1.0) * pitch + body_caps) / (body_caps / size);
        let mut big = format.clone();
        big.font_size = Length::from_pt(f64::from(cap_size));
        let letters =
            ParagraphText::from_runs(ParaFormat::default(), big.clone(), [(&source[..end], big)]);
        let laid = self.layout(&letters, &Column::new(ROOM), 0.0);
        let line = laid.lines.into_iter().next()?;
        let paragraph = text.paragraph();
        let x = (paragraph.left_indent + paragraph.first_line_indent).to_pt() as f32;
        let baseline = top + first_baseline + (lines - 1.0) * pitch;
        let (dx, dy) = (x - line.x, baseline - line.baseline);
        let right = x + line.advance + cap.distance.to_pt() as f32;
        let beside = Beside {
            inner: space,
            top,
            bottom: baseline,
            right,
        };
        let mut layout = self.layout(&rest, &beside, top);
        let bottom = baseline + (line.text_bottom - line.baseline);
        layout.bottom = layout.bottom.max(bottom);
        let first = layout.lines.first_mut()?;
        first.range.start = 0;
        let mut runs: Vec<_> = line
            .runs
            .into_iter()
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
