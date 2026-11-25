// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paragraphs whose break a tracked deletion took, joined in the views of
//! markup without deletions (W148, REV-04, B-071).
//!
//! The paragraph and those its deleted breaks join to it are laid out as
//! one, from the text of all of them, with the formatting of the first,
//! as accepting the deletions would leave them. The lines are then cut
//! where each paragraph starts, so that each keeps lines of its own, side
//! by side on the line they share, as the pieces of a line beside an
//! object are.

use std::collections::HashSet;

use desk_model::{Document, NodeId, Paragraph};
use desk_text::Line;

use crate::pages::{DocumentLayout, PlacedLine};

/// For each paragraph of the body, the paragraph it shows in and where its
/// text starts in the text laid out for that one: none for a paragraph
/// that shows on its own.
pub(crate) fn joins(
    document: &Document,
    hidden: Option<&HashSet<NodeId>>,
) -> Vec<Option<(usize, usize)>> {
    let body = document.body();
    let mut out = vec![None; body.len()];
    // The text of a story edited as a body is not joined yet (B-074).
    let Some(hidden) = hidden.filter(|_| !document.is_story_body()) else {
        return out;
    };
    let mut head = 0;
    let mut len = 0;
    let mut before: Option<&Paragraph> = None;
    for (i, paragraph) in body.iter().enumerate() {
        let joined = before
            .and_then(|p| p.attrs().break_change.flatten())
            .is_some_and(|change| hidden.contains(&change));
        if joined {
            out[i] = Some((head, len));
            len += paragraph.len();
        } else {
            head = i;
            len = paragraph.len();
        }
        before = Some(paragraph);
    }
    out
}

/// The paragraph that a paragraph laid out as the first of joined ones
/// shows: its text and that of those joined to it, with its formatting.
pub(crate) fn joined_paragraph(
    document: &Document,
    joins: &[Option<(usize, usize)>],
    head: usize,
) -> Option<Paragraph> {
    let body = document.body();
    let first = body.get(head)?;
    let rest = (head + 1..body.len()).take_while(|&i| joins[i].is_some_and(|(h, _)| h == head));
    rest.clone().next()?;
    let mut runs: Vec<desk_model::Run> = first.runs().cloned().collect();
    let mut attrs = first.attrs().clone();
    for i in rest {
        let paragraph = body.get(i)?;
        runs.extend(paragraph.runs().cloned());
        attrs.break_change = paragraph.attrs().break_change;
    }
    Paragraph::from_runs(first.id(), first.style(), attrs, runs).ok()
}

/// Cuts the lines of joined paragraphs where each of them starts, giving
/// each its own lines, numbered from its first.
pub(crate) fn split_lines(layout: &mut DocumentLayout, joins: &[Option<(usize, usize)>]) {
    if joins.iter().all(Option::is_none) {
        return;
    }
    // Where each paragraph shown with others starts and ends in their
    // text, by the first of them.
    let mut parts: Vec<Vec<(usize, usize, usize)>> = vec![Vec::new(); joins.len()];
    for (i, join) in joins.iter().enumerate() {
        if let Some((head, start)) = *join {
            if parts[head].is_empty() {
                parts[head].push((head, 0, start));
            }
            if let Some(last) = parts[head].last_mut() {
                last.2 = start;
            }
            parts[head].push((i, start, usize::MAX));
        }
    }
    // The end of the text laid out for each first of joined paragraphs.
    let mut total = vec![0usize; joins.len()];
    for line in layout.pages.iter().flat_map(|p| &p.lines) {
        let end = &mut total[line.paragraph];
        *end = (*end).max(line.line.range.end);
    }
    let mut counts = vec![0usize; joins.len()];
    for page in &mut layout.pages {
        let lines = std::mem::take(&mut page.lines);
        for placed in lines {
            let segments = &parts[placed.paragraph];
            if segments.is_empty() {
                page.lines.push(placed);
                continue;
            }
            let range = placed.line.range.clone();
            let last_line = range.end >= total[placed.paragraph];
            let mut pieces = Vec::new();
            for &(paragraph, start, end) in segments {
                // A paragraph starts on the line its first offset is on,
                // before the end of the line or on the last line, and
                // ends on the line its last offset is on, after the start
                // of the line; an empty one is on the line it starts on.
                let starts_here = range.start <= start && (start < range.end || last_line);
                let on = if start == end {
                    starts_here
                } else {
                    (starts_here || start < range.start) && end > range.start
                };
                let (lo, hi) = (start.max(range.start), end.min(range.end));
                if on && lo <= hi {
                    pieces.push((paragraph, lo, hi, start));
                }
            }
            let count = pieces.len();
            for (k, (paragraph, lo, hi, base)) in pieces.into_iter().enumerate() {
                let line = piece(&placed.line, lo, hi, base, k == 0, k + 1 == count);
                page.lines.push(PlacedLine {
                    paragraph,
                    index: counts[paragraph],
                    line,
                    origin: placed.origin,
                });
                counts[paragraph] += 1;
            }
        }
    }
}

/// The part of a line from offset `lo` to `hi` of the text laid out, as a
/// line of the paragraph whose text starts at `base`: its places, from
/// the place of `lo` to that of `hi`, or to the edges of the line for its
/// first and last parts. The first part draws the glyphs of the whole
/// line, with its label.
fn piece(line: &Line, lo: usize, hi: usize, base: usize, first: bool, last: bool) -> Line {
    let left = if first { line.x } else { line.caret_x(lo) };
    let right = if last {
        line.x + line.width
    } else {
        line.caret_x(hi)
    };
    // Its places, with one at each of its ends, where text that does not
    // show left none.
    let mut carets: Vec<(usize, f32)> = line
        .carets
        .iter()
        .filter(|&&(offset, _)| lo <= offset && offset <= hi)
        .map(|&(offset, x)| (offset - base, x))
        .collect();
    if carets.first().is_none_or(|c| c.0 > lo - base) {
        carets.insert(0, (lo - base, line.caret_x(lo)));
    }
    if carets.last().is_some_and(|c| c.0 < hi - base) {
        carets.push((hi - base, line.caret_x(hi)));
    }
    Line {
        range: lo - base..hi - base,
        x: left,
        width: (right - left).max(0.0),
        advance: if first && last {
            line.advance
        } else {
            (right - left).max(0.0)
        },
        runs: if first { line.runs.clone() } else { Vec::new() },
        decorations: if first {
            line.decorations.clone()
        } else {
            Vec::new()
        },
        hyphenated: last && line.hyphenated,
        carets,
        label: if first { line.label.clone() } else { None },
        label_image: if first {
            line.label_image.clone()
        } else {
            None
        },
        drop_cap_shape: if first { line.drop_cap_shape } else { None },
        ..line.clone()
    }
}
