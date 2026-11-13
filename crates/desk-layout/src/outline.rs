// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pages of a document that shows as an outline, as Pages prints and
//! exports it then (TXT-14, DOC-16): every paragraph starts at the left,
//! indented by its level in the outline, in a text frame a little shorter
//! than that of the pages. The options of the outline apply too: the
//! levels that show, the first line of each paragraph alone, and objects
//! in miniature. The handles of the outline are not printed.
//!
//! The line spacing and fonts of the paragraphs stay as they are, but not
//! the space before and after them, nor what keeps lines and paragraphs
//! together.

use desk_model::{Alignment, Anchor, Document, Length, Op, OutlineOptions, Run};
use desk_text::TextSystem;

use crate::pages::{DocumentLayout, paginate};

/// How far each level of the outline is indented, and the indent of the
/// first: what Pages saves as the indents of its outliner.
const OUTLINE_INDENT: i32 = 16;

/// How much lower the text frame of a page of the outline starts than the
/// top margin, rounded up to a whole point, and how much higher it ends
/// than the bottom margin, rounded down to a whole point, as Pages saves
/// the frames of an outline.
const FRAME_TOP: f64 = 4.0;
const FRAME_BOTTOM: f64 = 8.0;

/// The largest side of an object in miniature, in points.
const THUMBNAIL: f64 = 50.0;

/// What ends a paragraph cut to its first line.
const ELLIPSIS: &str = "\u{2026}";

/// Lays out a document on pages as it shows: as an outline when it was
/// saved showing as one, and as [`paginate`] does otherwise.
pub fn paginate_view(document: &Document, system: &mut TextSystem) -> DocumentLayout {
    match document.view() {
        Some(view) if view.outline => paginate_outline(document, view.outline_options, system),
        _ => paginate(document, system),
    }
}

/// Lays out a document on pages as its outline prints, with the options
/// of the outline.
pub fn paginate_outline(
    document: &Document,
    options: OutlineOptions,
    system: &mut TextSystem,
) -> DocumentLayout {
    let outline = outline_document(document, options);
    if !options.first_line {
        return paginate(&outline, system);
    }
    first_lines(outline, system)
}

/// The document as its outline prints: the paragraphs that show, each
/// indented by its level, on pages with the frame of the outline.
pub fn outline_document(document: &Document, options: OutlineOptions) -> Document {
    let mut outline = document.clone();
    let mut page = *outline.page_setup();
    let height = page.height.to_pt();
    let top = page.margin_top.to_pt().ceil() + FRAME_TOP;
    let bottom = (height - page.margin_bottom.to_pt()).floor() - FRAME_BOTTOM;
    page.margin_top = Length::from_pt(top);
    page.margin_bottom = Length::from_pt(height - bottom);
    // The pages of a page layout document have no outline.
    page.layout_pages = None;
    apply(&mut outline, Op::SetPageSetup { page });

    // The level of the topic each paragraph is under, from 0, and the
    // paragraphs that the levels shown leave out.
    let mut depth = 0u8;
    let mut hidden = Vec::new();
    let mut formats = Vec::new();
    for index in 0..outline.body().len() {
        let level = outline.outline_level(index);
        let indent = match level {
            Some(level) => {
                depth = level;
                level - 1
            }
            None => depth,
        };
        let paragraph = outline.body().get(index).expect("a paragraph of the body");
        let shown = match (options.levels, level) {
            (None, _) => true,
            (Some(levels), Some(level)) => level <= levels,
            // Text shows only with every level.
            (Some(_), None) => false,
        };
        if !shown {
            hidden.push(paragraph.id());
            continue;
        }
        let mut attrs = paragraph.attrs().clone();
        let start = Length::from_points(OUTLINE_INDENT * (i32::from(indent) + 1));
        attrs.left_indent = Some(start);
        attrs.first_line_indent = Some(Length::ZERO);
        attrs.right_indent = Some(Length::ZERO);
        attrs.alignment = Some(Alignment::Left);
        // Nor does it keep the space before and after paragraphs.
        attrs.space_before = Some(Length::ZERO);
        attrs.space_after = Some(Length::ZERO);
        // The text flows from page to page without keeping lines or
        // paragraphs together.
        attrs.keep_with_next = Some(false);
        attrs.keep_lines_together = Some(false);
        attrs.widow_control = Some(false);
        formats.push(Op::SetParagraphFormat {
            paragraph: paragraph.id(),
            style: paragraph.style(),
            attrs,
        });
    }
    for op in formats {
        apply(&mut outline, op);
    }
    for paragraph in hidden {
        apply(&mut outline, Op::RemoveParagraph { paragraph });
    }

    // Only the objects in the lines of the text show, in miniature when
    // the outline says.
    let objects: Vec<_> = outline.objects().iter().cloned().collect();
    for mut object in objects {
        if object.anchor != Anchor::InText {
            apply(&mut outline, Op::RemoveObject { object: object.id });
            continue;
        }
        if options.thumbnails {
            let (width, height) = (object.frame.width.to_pt(), object.frame.height.to_pt());
            let scale = THUMBNAIL / width.max(height);
            if scale < 1.0 {
                object.frame.width = Length::from_pt(width * scale);
                object.frame.height = Length::from_pt(height * scale);
                apply(&mut outline, Op::ReplaceObject { object });
            }
        }
    }
    outline
}

/// Lays out the outline with only the first line of each paragraph: the
/// text of a longer paragraph is cut at the end of its first line, with
/// an ellipsis that takes the place of what does not fit beside it.
fn first_lines(mut outline: Document, system: &mut TextSystem) -> DocumentLayout {
    // The paragraphs still too long, with how much of their text to keep.
    let mut keep: Vec<Option<usize>> = vec![None; outline.body().len()];
    let mut layout = paginate(&outline, system);
    // Each round cuts one more character from the paragraphs whose text
    // and ellipsis still take two lines; a few rounds are enough.
    for _ in 0..32 {
        let mut lines = vec![0usize; outline.body().len()];
        let mut first_end = vec![0usize; outline.body().len()];
        for page in &layout.pages {
            for line in &page.lines {
                if let Some(count) = lines.get_mut(line.paragraph) {
                    if *count == 0 {
                        first_end[line.paragraph] = line.line.range.end;
                    }
                    *count += 1;
                }
            }
        }
        let mut changed = false;
        for (index, count) in lines.iter().enumerate() {
            if *count <= 1 {
                continue;
            }
            let paragraph = outline.body().get(index).expect("a paragraph of the body");
            let text = paragraph.text();
            let end = match keep[index] {
                // First the text of the first line and the word after
                // it: Pages fills the line with letters up to the
                // ellipsis, even within a word.
                None => text[first_end[index]..]
                    .find(char::is_whitespace)
                    .map_or(text.len(), |i| first_end[index] + i),
                // Then a character less each time.
                Some(kept) => text[..kept]
                    .char_indices()
                    .next_back()
                    .map_or(0, |(i, _)| i),
            };
            keep[index] = Some(end);
            cut(&mut outline, index, end);
            changed = true;
        }
        if !changed {
            break;
        }
        layout = paginate(&outline, system);
    }
    layout
}

/// Cuts the text of a paragraph after a number of bytes and ends it with
/// an ellipsis in the format of the text before it. The ellipsis that a
/// cut before added goes too.
fn cut(outline: &mut Document, index: usize, end: usize) {
    let paragraph = outline.body().get(index).expect("a paragraph of the body");
    let id = paragraph.id();
    let len = paragraph.len();
    let last = paragraph
        .runs()
        .scan(0usize, |at, run| {
            let start = *at;
            *at += run.len();
            Some((start, run))
        })
        .take_while(|(start, _)| *start < end.max(1))
        .last()
        .map(|(_, run)| Run::with_attrs(ELLIPSIS, run.style, run.attrs.clone()));
    let runs = last.into_iter().collect();
    apply(
        outline,
        Op::ReplaceText {
            paragraph: id,
            range: end..len,
            runs,
        },
    );
}

fn apply(document: &mut Document, op: Op) {
    // The operations come from the document itself, so they apply.
    let _ = document.apply(op);
}
