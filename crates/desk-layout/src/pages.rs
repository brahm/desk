// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pages of a word processing document.

use desk_model::{Document, PageSetup, ParaFormat};
use desk_text::{Column, Line, ParagraphText, TextSystem};

/// How much a line may stick out of the bottom of the body and still
/// count as fitting, to absorb rounding.
const TOLERANCE: f32 = 0.01;

/// A line placed on a page.
#[derive(Clone, Debug)]
pub struct PlacedLine {
    /// The index of the paragraph in the body.
    pub paragraph: usize,
    /// The index of the line in the paragraph.
    pub index: usize,
    /// The line. Its coordinates are relative to its paragraph; add
    /// [`PlacedLine::origin`] to place it on the page.
    pub line: Line,
    /// Where the origin of the paragraph falls on the page, from the top
    /// left corner of the paper.
    pub origin: (f32, f32),
}

impl PlacedLine {
    /// The top of the line on the page.
    pub fn top(&self) -> f32 {
        self.origin.1 + self.line.top
    }

    /// The bottom of the line on the page.
    pub fn bottom(&self) -> f32 {
        self.top() + self.line.height
    }

    /// The baseline of the line on the page.
    pub fn baseline(&self) -> f32 {
        self.origin.1 + self.line.baseline
    }
}

/// One page.
#[derive(Clone, Debug, Default)]
pub struct Page {
    /// The lines on the page, from top to bottom.
    pub lines: Vec<PlacedLine>,
}

/// A word processing document laid out on pages.
#[derive(Clone, Debug)]
pub struct DocumentLayout {
    /// The size and margins of every page.
    pub setup: PageSetup,
    /// The pages. There is always at least one.
    pub pages: Vec<Page>,
}

impl DocumentLayout {
    /// The page that a paragraph starts on.
    pub fn page_of(&self, paragraph: usize) -> Option<usize> {
        self.pages
            .iter()
            .position(|page| page.lines.iter().any(|l| l.paragraph == paragraph))
    }
}

/// Lays out the body of a document on pages.
pub fn paginate(document: &Document, system: &mut TextSystem) -> DocumentLayout {
    let texts: Vec<ParagraphText> = document
        .body()
        .iter()
        .map(|paragraph| ParagraphText::new(document.styles(), paragraph))
        .collect();
    paginate_texts(*document.page_setup(), &texts, system)
}

/// A paragraph laid out once, at the top of an endless column.
struct Block {
    format: ParaFormat,
    lines: Vec<Line>,
}

impl Block {
    /// The height of lines `a..b`.
    fn height(&self, a: usize, b: usize) -> f32 {
        if a >= b {
            return 0.0;
        }
        let last = &self.lines[b - 1];
        last.top + last.height - self.lines[a].top
    }

    /// How many lines from `a` on fit in `room`: as in Pages, a line fits
    /// when its text does, without the leading below it.
    fn fitting(&self, a: usize, room: f32) -> usize {
        (a..self.lines.len())
            .take_while(|&b| self.lines[b].text_bottom - self.lines[a].top <= room + TOLERANCE)
            .count()
    }
}

/// Where the next line goes.
#[derive(Clone, Copy)]
struct Cursor {
    /// The page, as an index.
    page: usize,
    /// The distance from the top of the body.
    y: f32,
    /// The space below the last paragraph placed, which collapses with
    /// the space above the next one.
    after: f32,
}

impl Cursor {
    fn at_top(&self, pages: &[Page]) -> bool {
        pages[self.page].lines.is_empty()
    }
}

/// Lays out prepared paragraphs on pages of one setup.
///
/// The rules, to be checked against the PDFs of Pages in the reference VM:
///
/// - Between two paragraphs, the larger of the space after the first and
///   the space before the second applies (FMT-15). At the top of a page
///   the space before is dropped, and the space after the last paragraph
///   of a page is never carried over.
/// - A paragraph that starts on a new page does so unless it is already at
///   the top of one (EST-04).
/// - Keeping lines together moves a paragraph that does not fit to the
///   next page, unless it is taller than a page (EST-05).
/// - Widow control leaves at least two lines of a paragraph at the bottom
///   of a page and at the top of the next (EST-05).
/// - Keeping a paragraph with the next one moves the paragraph, and any
///   paragraphs kept with it before, to the next page when the next one
///   would start on another page (EST-05). A chain that starts at the top
///   of a page breaks normally, since moving it would not help.
pub fn paginate_texts(
    setup: PageSetup,
    texts: &[ParagraphText],
    system: &mut TextSystem,
) -> DocumentLayout {
    let column = Column::new(setup.body_width().to_pt() as f32);
    let blocks: Vec<Block> = texts
        .iter()
        .map(|text| Block {
            format: text.paragraph().clone(),
            lines: system.layout(text, &column, 0.0).lines,
        })
        .collect();
    let body = setup.body_height().to_pt() as f32;
    let left = setup.margin_left.to_pt() as f32;
    let top = setup.margin_top.to_pt() as f32;

    let mut pages = vec![Page::default()];
    let mut cursor = Cursor {
        page: 0,
        y: 0.0,
        after: 0.0,
    };
    // Paragraphs that start a new page because a chain kept with the next
    // paragraph did not fit.
    let mut forced = vec![false; blocks.len()];
    // Where the current chain of paragraphs kept with the next one started.
    let mut chain: Option<(usize, Cursor, usize, usize)> = None;
    let mut i = 0;
    while i < blocks.len() {
        if i == 0 || !blocks[i - 1].format.keep_with_next {
            chain = Some((i, cursor, pages.len(), pages[cursor.page].lines.len()));
        }
        let block = &blocks[i];
        let format = &block.format;
        let new_page = |pages: &mut Vec<Page>, cursor: &mut Cursor| {
            pages.push(Page::default());
            *cursor = Cursor {
                page: pages.len() - 1,
                y: 0.0,
                after: 0.0,
            };
        };
        if (forced[i] || format.page_break_before) && !cursor.at_top(&pages) {
            new_page(&mut pages, &mut cursor);
        }
        let mut first_page = cursor.page;
        let before = format.space_before.to_pt() as f32;
        let n = block.lines.len();
        let mut a = 0;
        while a < n {
            let at_top = cursor.at_top(&pages);
            let space = if a > 0 || at_top {
                0.0
            } else {
                cursor.after.max(before)
            };
            let room = body - cursor.y - space;
            let rest = n - a;
            let mut k = block.fitting(a, room);
            if k < rest {
                if a == 0 && !at_top && format.keep_lines_together && block.height(0, n) <= body {
                    k = 0;
                } else if format.widow_control {
                    if a == 0 && k == 1 && rest > 1 {
                        // The first line alone at the bottom of a page.
                        k = 0;
                    } else if rest - k == 1 && k > 0 {
                        // The last line alone at the top of the next page:
                        // send one more, unless that leaves the first one
                        // alone.
                        k = if a == 0 && k == 2 { 0 } else { k - 1 };
                    }
                }
                if at_top {
                    // Nothing better is possible: overflow the page.
                    k = k.max(1);
                }
            }
            if k == 0 {
                new_page(&mut pages, &mut cursor);
                continue;
            }
            let k = k.min(rest);
            if a == 0 {
                first_page = cursor.page;
            }
            let dy = top + cursor.y + space - block.lines[a].top;
            for (index, line) in block.lines[a..a + k].iter().enumerate() {
                pages[cursor.page].lines.push(PlacedLine {
                    paragraph: i,
                    index: a + index,
                    line: line.clone(),
                    origin: (left, dy),
                });
            }
            cursor.y += space + block.height(a, a + k);
            a += k;
            if a < n {
                new_page(&mut pages, &mut cursor);
            }
        }
        cursor.after = format.space_after.to_pt() as f32;

        // A paragraph kept with this one ended on another page: start the
        // chain on a new page and lay it out again.
        if i > 0 && blocks[i - 1].format.keep_with_next && !format.page_break_before {
            let previous_page = pages
                .iter()
                .rposition(|p| p.lines.iter().any(|l| l.paragraph == i - 1));
            if previous_page != Some(first_page)
                && let Some((start, saved, page_count, line_count)) = chain
                && line_count > 0
                && !forced[start]
            {
                forced[start] = true;
                pages.truncate(page_count);
                pages[saved.page].lines.truncate(line_count);
                cursor = saved;
                i = start;
                continue;
            }
        }
        i += 1;
    }
    DocumentLayout { setup, pages }
}
