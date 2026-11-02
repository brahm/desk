// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the text cursor stands and how it moves, over a document and its
//! layout, apart from GTK so that it can be tested without a display.
//!
//! A place in the text is a [`Spot`]: the index of a paragraph in the body
//! and a byte offset in its text. The window turns spots into positions of
//! the model when it edits.

use desk_layout::{DocumentLayout, PlacedLine};
use desk_model::{Document, Position};
use unicode_segmentation::UnicodeSegmentation;

/// A place in the body: a paragraph by index and a byte offset in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Spot {
    /// The index of the paragraph in the body.
    pub paragraph: usize,
    /// The byte offset in the text of the paragraph.
    pub offset: usize,
}

impl Spot {
    /// A spot.
    pub const fn new(paragraph: usize, offset: usize) -> Self {
        Self { paragraph, offset }
    }

    /// The position of the model at this spot.
    pub fn position(self, document: &Document) -> Option<Position> {
        let paragraph = document.body().get(self.paragraph)?;
        Some(Position::new(paragraph.id(), self.offset))
    }

    /// The spot of a position of the model.
    pub fn of(document: &Document, position: Position) -> Option<Self> {
        let index = document.body().index_of(position.paragraph)?;
        Some(Self::new(index, position.offset))
    }
}

/// A selection: where it started and where the cursor is. When both are
/// the same, it is a cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// Where the selection started; it stays when Shift extends it.
    pub anchor: Spot,
    /// Where the cursor is.
    pub focus: Spot,
}

impl Selection {
    /// A cursor at a spot.
    pub const fn cursor(spot: Spot) -> Self {
        Self {
            anchor: spot,
            focus: spot,
        }
    }

    /// Whether nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.focus
    }

    /// The first and the last spot.
    pub fn ordered(&self) -> (Spot, Spot) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }
}

/// The cursor drawn on a page, in points from its top left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretRect {
    /// The page.
    pub page: usize,
    /// Where the cursor stands.
    pub x: f64,
    /// The top of its line.
    pub top: f64,
    /// The bottom of its line.
    pub bottom: f64,
}

/// A rectangle of a selection on a page, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpanRect {
    /// The page.
    pub page: usize,
    /// The left edge.
    pub x0: f64,
    /// The top.
    pub y0: f64,
    /// The right edge.
    pub x1: f64,
    /// The bottom.
    pub y1: f64,
}

fn text_of(document: &Document, paragraph: usize) -> String {
    document
        .body()
        .get(paragraph)
        .map(|p| p.text())
        .unwrap_or_default()
}

fn last_spot(document: &Document) -> Spot {
    let last = document.body().len().saturating_sub(1);
    Spot::new(last, text_of(document, last).len())
}

/// Keeps a spot inside the document and on a character boundary.
pub fn clamp(document: &Document, spot: Spot) -> Spot {
    let body = document.body();
    if spot.paragraph >= body.len() {
        return last_spot(document);
    }
    let text = text_of(document, spot.paragraph);
    let mut offset = spot.offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    Spot::new(spot.paragraph, offset)
}

/// The lines of a paragraph, with the page each is on.
fn lines_of(
    layout: &DocumentLayout,
    paragraph: usize,
) -> impl Iterator<Item = (usize, &PlacedLine)> {
    layout.pages.iter().enumerate().flat_map(move |(page, p)| {
        p.lines
            .iter()
            .filter(move |l| l.paragraph == paragraph)
            .map(move |l| (page, l))
    })
}

/// The line a spot is on, and its page. A spot at the end of a line that
/// another line continues belongs to the next line, as the cursor after a
/// space at a break shows at the start of the next line.
fn line_of(layout: &DocumentLayout, spot: Spot) -> Option<(usize, &PlacedLine)> {
    let mut found = None;
    for (page, line) in lines_of(layout, spot.paragraph) {
        if line.line.range.start <= spot.offset {
            found = Some((page, line));
        }
    }
    found
}

/// Where the cursor at a spot is drawn.
pub fn caret(layout: &DocumentLayout, spot: Spot) -> Option<CaretRect> {
    let (page, line) = line_of(layout, spot)?;
    Some(CaretRect {
        page,
        x: f64::from(line.origin.0 + line.line.caret_x(spot.offset)),
        top: f64::from(line.top()),
        bottom: f64::from(line.bottom()),
    })
}

/// The spot nearest to a point of a page, in points.
pub fn hit(layout: &DocumentLayout, page: usize, x: f64, y: f64) -> Option<Spot> {
    let lines = &layout.pages.get(page)?.lines;
    let (x, y) = (x as f32, y as f32);
    let line = lines.iter().min_by(|a, b| {
        let distance = |l: &PlacedLine| {
            if y < l.top() {
                l.top() - y
            } else if y >= l.bottom() {
                y - l.bottom() + 0.001
            } else {
                0.0
            }
        };
        distance(a).total_cmp(&distance(b))
    })?;
    Some(Spot::new(
        line.paragraph,
        line.line.offset_at(x - line.origin.0),
    ))
}

/// The rectangles that show a selection, line by line.
pub fn selection_rects(layout: &DocumentLayout, selection: &Selection) -> Vec<SpanRect> {
    let (start, end) = selection.ordered();
    let mut rects = Vec::new();
    if start == end {
        return rects;
    }
    for (page, p) in layout.pages.iter().enumerate() {
        for line in &p.lines {
            let range = &line.line.range;
            let here = Spot::new(line.paragraph, range.start);
            let line_end = Spot::new(line.paragraph, range.end);
            if line_end < start || here > end {
                continue;
            }
            let from = if start > here {
                start.offset
            } else {
                range.start
            };
            let to = if end < line_end {
                end.offset
            } else {
                range.end
            };
            let x0 = line.origin.0 + line.line.caret_x(from);
            let mut x1 = line.origin.0 + line.line.caret_x(to);
            // A selection that goes on past the line shows the end of its
            // paragraph or its break as a little more.
            if end > line_end && line.paragraph < end.paragraph && range.end == line_end.offset {
                x1 += 4.0;
            }
            if x1 > x0 {
                rects.push(SpanRect {
                    page,
                    x0: f64::from(x0),
                    y0: f64::from(line.top()),
                    x1: f64::from(x1),
                    y1: f64::from(line.bottom()),
                });
            }
        }
    }
    rects
}

/// One character back or forward, across paragraphs. A character is what
/// people see as one, such as a letter with its accents.
pub fn move_char(document: &Document, spot: Spot, forward: bool) -> Spot {
    let text = text_of(document, spot.paragraph);
    let boundaries: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    if forward {
        match boundaries.iter().find(|&&b| b > spot.offset) {
            Some(&b) => Spot::new(spot.paragraph, b),
            None if spot.paragraph + 1 < document.body().len() => Spot::new(spot.paragraph + 1, 0),
            None => spot,
        }
    } else {
        match boundaries.iter().rev().find(|&&b| b < spot.offset) {
            Some(&b) => Spot::new(spot.paragraph, b),
            None if spot.paragraph > 0 => Spot::new(
                spot.paragraph - 1,
                text_of(document, spot.paragraph - 1).len(),
            ),
            None => spot,
        }
    }
}

/// The word around a spot: its start and end, or the spot itself between
/// words.
fn words(text: &str) -> Vec<(usize, usize)> {
    text.split_word_bound_indices()
        .filter(|(_, w)| w.chars().any(char::is_alphanumeric))
        .map(|(i, w)| (i, i + w.len()))
        .collect()
}

/// To the start of the previous word or the end of the next one, as
/// Option and an arrow do on the Mac and Control and an arrow on GNOME.
pub fn move_word(document: &Document, spot: Spot, forward: bool) -> Spot {
    let text = text_of(document, spot.paragraph);
    let words = words(&text);
    if forward {
        match words.iter().find(|w| w.1 > spot.offset) {
            Some(w) => Spot::new(spot.paragraph, w.1),
            None if spot.paragraph + 1 < document.body().len() => {
                move_word(document, Spot::new(spot.paragraph + 1, 0), true)
            }
            None => Spot::new(spot.paragraph, text.len()),
        }
    } else {
        match words.iter().rev().find(|w| w.0 < spot.offset) {
            Some(w) => Spot::new(spot.paragraph, w.0),
            None if spot.paragraph > 0 && spot.offset == 0 => {
                let previous = Spot::new(
                    spot.paragraph - 1,
                    text_of(document, spot.paragraph - 1).len(),
                );
                move_word(document, previous, false)
            }
            None => Spot::new(spot.paragraph, 0),
        }
    }
}

/// The word at a spot, for a double click (TXT-03). Between words it is
/// the white space or punctuation there.
pub fn word_at(document: &Document, spot: Spot) -> Selection {
    let text = text_of(document, spot.paragraph);
    for (i, piece) in text.split_word_bound_indices() {
        let end = i + piece.len();
        if spot.offset < end || end == text.len() {
            return Selection {
                anchor: Spot::new(spot.paragraph, i),
                focus: Spot::new(spot.paragraph, end),
            };
        }
    }
    Selection::cursor(spot)
}

/// The paragraph at a spot, for a triple click (TXT-03).
pub fn paragraph_at(document: &Document, spot: Spot) -> Selection {
    Selection {
        anchor: Spot::new(spot.paragraph, 0),
        focus: Spot::new(spot.paragraph, text_of(document, spot.paragraph).len()),
    }
}

/// The start or the end of the line a spot is on.
pub fn line_edge(layout: &DocumentLayout, spot: Spot, end: bool) -> Spot {
    let Some((_, line)) = line_of(layout, spot) else {
        return spot;
    };
    let range = &line.line.range;
    if !end {
        return Spot::new(spot.paragraph, range.start);
    }
    // Before the space or break that ends a line that goes on.
    let last = line.line.carets.last().map_or(range.end, |c| c.0);
    let continues = lines_of(layout, spot.paragraph)
        .any(|(_, l)| l.line.range.start == range.end && range.end > range.start);
    if continues {
        let before = line
            .line
            .carets
            .iter()
            .rev()
            .nth(1)
            .map_or(range.start, |c| c.0);
        Spot::new(spot.paragraph, before)
    } else {
        Spot::new(spot.paragraph, last)
    }
}

/// The start or the end of the paragraph a spot is in, or of the next or
/// previous one when it is there already.
pub fn paragraph_edge(document: &Document, spot: Spot, forward: bool) -> Spot {
    let len = text_of(document, spot.paragraph).len();
    if forward {
        if spot.offset < len {
            Spot::new(spot.paragraph, len)
        } else if spot.paragraph + 1 < document.body().len() {
            Spot::new(
                spot.paragraph + 1,
                text_of(document, spot.paragraph + 1).len(),
            )
        } else {
            spot
        }
    } else if spot.offset > 0 {
        Spot::new(spot.paragraph, 0)
    } else if spot.paragraph > 0 {
        Spot::new(spot.paragraph - 1, 0)
    } else {
        spot
    }
}

/// The start or the end of the document.
pub fn document_edge(document: &Document, end: bool) -> Spot {
    if end {
        last_spot(document)
    } else {
        Spot::default()
    }
}

/// The spot on the line above or below, nearest to `goal`, the distance
/// from the left of the page that the cursor keeps while it goes up and
/// down. `lines` is how many lines to go: one for an arrow, a screenful
/// for Page Up and Page Down.
pub fn move_line(layout: &DocumentLayout, spot: Spot, goal: f64, down: bool, lines: usize) -> Spot {
    let all: Vec<(usize, &PlacedLine)> = layout
        .pages
        .iter()
        .enumerate()
        .flat_map(|(page, p)| p.lines.iter().map(move |l| (page, l)))
        .collect();
    let Some((page, current)) = line_of(layout, spot) else {
        return spot;
    };
    let Some(index) = all
        .iter()
        .position(|(p, l)| *p == page && std::ptr::eq(*l, current))
    else {
        return spot;
    };
    let target = if down {
        (index + lines).min(all.len() - 1)
    } else {
        index.saturating_sub(lines)
    };
    if target == index {
        // At the first or last line: to the start or the end.
        return if down {
            Spot::new(current.paragraph, current.line.range.end)
        } else {
            Spot::new(current.paragraph, current.line.range.start)
        };
    }
    let (_, line) = all[target];
    let offset = line.line.offset_at(goal as f32 - line.origin.0);
    Spot::new(line.paragraph, offset)
}

/// A character that prints nothing, which Show Invisibles marks (INT-10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invisible {
    /// A space.
    Space,
    /// A space that does not break.
    NoBreakSpace,
    /// A tab.
    Tab,
    /// A line break.
    LineBreak,
    /// The end of a paragraph.
    Paragraph,
}

impl Invisible {
    /// The mark drawn for it, as in Pages.
    pub fn mark(self) -> &'static str {
        match self {
            Self::Space => "·",
            Self::NoBreakSpace => "°",
            Self::Tab => "→",
            Self::LineBreak => "↵",
            Self::Paragraph => "¶",
        }
    }
}

/// Where an invisible character falls on a page, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvisibleMark {
    /// The index of the page.
    pub page: usize,
    /// The character.
    pub kind: Invisible,
    /// The left edge of the character.
    pub x0: f64,
    /// The right edge of the character; the left edge for the end of a
    /// paragraph, which takes no room.
    pub x1: f64,
    /// The baseline of its line.
    pub baseline: f64,
    /// The height of its line above the baseline.
    pub ascent: f64,
}

/// The invisible characters of a document as laid out.
pub fn invisibles(document: &Document, layout: &DocumentLayout) -> Vec<InvisibleMark> {
    let mut marks = Vec::new();
    for (page, p) in layout.pages.iter().enumerate() {
        for line in &p.lines {
            let Some(paragraph) = document.body().get(line.paragraph) else {
                continue;
            };
            let text = paragraph.text();
            let range = line.line.range.clone();
            let x = |offset: usize| f64::from(line.origin.0 + line.line.caret_x(offset));
            let baseline = f64::from(line.baseline());
            let ascent = baseline - f64::from(line.top());
            let Some(slice) = text.get(range.clone()) else {
                continue;
            };
            for (at, c) in slice.char_indices() {
                let kind = match c {
                    ' ' => Invisible::Space,
                    desk_model::NO_BREAK_SPACE => Invisible::NoBreakSpace,
                    '\t' => Invisible::Tab,
                    desk_model::LINE_BREAK => Invisible::LineBreak,
                    _ => continue,
                };
                let offset = range.start + at;
                marks.push(InvisibleMark {
                    page,
                    kind,
                    x0: x(offset),
                    x1: x(offset + c.len_utf8()),
                    baseline,
                    ascent,
                });
            }
            if range.end == text.len() && !slice.ends_with(desk_model::LINE_BREAK) {
                let end = x(text.len());
                marks.push(InvisibleMark {
                    page,
                    kind: Invisible::Paragraph,
                    x0: end,
                    x1: end,
                    baseline,
                    ascent,
                });
            }
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{Editor, Length, PageSetup};
    use desk_text::TextSystem;

    const TEXT: &str = "Desk lays out text the way Pages does, one line at a time, \
                        and decides itself where every line of the paragraph ends.";

    fn document(paragraphs: &[&str]) -> Document {
        let mut editor = Editor::new(Document::new_word_processing());
        editor
            .edit("Type", |edit| {
                let first = edit.document().body().get(0).unwrap().id();
                let text = paragraphs.join("\n");
                edit.insert_text(Position::new(first, 0), &text)?;
                // Narrow pages, so that a paragraph takes several lines.
                edit.set_page_setup(PageSetup::with_size(
                    Length::from_points(300),
                    Length::from_points(400),
                ))
            })
            .unwrap();
        editor.into_document()
    }

    fn laid_out(document: &Document) -> DocumentLayout {
        let mut system = TextSystem::new();
        desk_layout::paginate(document, &mut system)
    }

    #[test]
    fn txt_03_the_cursor_moves_by_character_and_word() {
        let doc = document(&["Café crème", "Second"]);
        // "é" is two bytes in one character.
        assert_eq!(move_char(&doc, Spot::new(0, 3), true), Spot::new(0, 5));
        assert_eq!(move_char(&doc, Spot::new(0, 5), false), Spot::new(0, 3));
        // Across paragraphs.
        let end = Spot::new(0, "Café crème".len());
        assert_eq!(move_char(&doc, end, true), Spot::new(1, 0));
        assert_eq!(move_char(&doc, Spot::new(1, 0), false), end);
        assert_eq!(move_char(&doc, Spot::default(), false), Spot::default());
        // Words: to the end of this one, then the next.
        assert_eq!(move_word(&doc, Spot::new(0, 1), true), Spot::new(0, 5));
        assert_eq!(move_word(&doc, Spot::new(0, 5), true), end);
        assert_eq!(move_word(&doc, end, true), Spot::new(1, 6));
        assert_eq!(move_word(&doc, Spot::new(1, 0), false), Spot::new(0, 6));
        assert_eq!(move_word(&doc, Spot::new(0, 8), false), Spot::new(0, 6));
    }

    #[test]
    fn txt_03_double_and_triple_click_select_a_word_and_a_paragraph() {
        let doc = document(&["Select a word here"]);
        let word = word_at(&doc, Spot::new(0, 10));
        assert_eq!(word.ordered(), (Spot::new(0, 9), Spot::new(0, 13)));
        let paragraph = paragraph_at(&doc, Spot::new(0, 3));
        assert_eq!(paragraph.ordered(), (Spot::new(0, 0), Spot::new(0, 18)));
    }

    #[test]
    fn txt_03_clicks_land_where_the_cursor_is_drawn() {
        let doc = document(&[TEXT, "Short"]);
        let layout = laid_out(&doc);
        for paragraph in 0..2 {
            let text = text_of(&doc, paragraph);
            for (offset, _) in text.char_indices() {
                let spot = Spot::new(paragraph, offset);
                let rect = caret(&layout, spot).unwrap();
                let back = hit(
                    &layout,
                    rect.page,
                    rect.x + 0.1,
                    (rect.top + rect.bottom) / 2.0,
                );
                assert_eq!(back, Some(spot), "{spot:?} at {rect:?}");
            }
        }
        // Above the text, a click goes to the first line; below, to the last.
        assert_eq!(hit(&layout, 0, 0.0, 0.0), Some(Spot::new(0, 0)));
        let last = hit(&layout, 0, 1000.0, 399.0).unwrap();
        assert_eq!(last, Spot::new(1, 5));
    }

    #[test]
    fn txt_03_lines_up_and_down_keep_the_column() {
        let doc = document(&[TEXT, "Short"]);
        let layout = laid_out(&doc);
        let start = Spot::new(0, 10);
        let rect = caret(&layout, start).unwrap();
        let down = move_line(&layout, start, rect.x, true, 1);
        let down_rect = caret(&layout, down).unwrap();
        assert!(down_rect.top > rect.top);
        assert!((down_rect.x - rect.x).abs() < 8.0, "{down_rect:?} {rect:?}");
        assert_eq!(move_line(&layout, down, rect.x, false, 1), start);
        // From the first line up: to the start; from the last down: the end.
        assert_eq!(move_line(&layout, start, rect.x, false, 1), Spot::new(0, 0));
        let end = Spot::new(1, 2);
        assert_eq!(move_line(&layout, end, 0.0, true, 1), Spot::new(1, 5));
        // Home and End of a line.
        let home = line_edge(&layout, down, false);
        assert_eq!(caret(&layout, home).unwrap().top, down_rect.top);
        let end_of_line = line_edge(&layout, down, true);
        assert_eq!(caret(&layout, end_of_line).unwrap().top, down_rect.top);
        assert!(end_of_line.offset > down.offset);
    }

    #[test]
    fn txt_03_a_selection_shows_on_every_line_it_covers() {
        let doc = document(&[TEXT, "Short"]);
        let layout = laid_out(&doc);
        let selection = Selection {
            anchor: Spot::new(1, 3),
            focus: Spot::new(0, 5),
        };
        let rects = selection_rects(&layout, &selection);
        let lines = layout.pages[0].lines.len();
        assert_eq!(rects.len(), lines);
        assert!(rects.iter().all(|r| r.x1 > r.x0));
        assert!(selection_rects(&layout, &Selection::cursor(Spot::new(0, 2))).is_empty());
    }

    #[test]
    fn int_10_invisibles_mark_spaces_tabs_breaks_and_paragraph_ends() {
        let doc = document(&["a b\tc", ""]);
        let layout = laid_out(&doc);
        let kinds: Vec<_> = invisibles(&doc, &layout).iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds,
            [
                Invisible::Space,
                Invisible::Tab,
                Invisible::Paragraph,
                Invisible::Paragraph
            ]
        );
        let marks = invisibles(&doc, &layout);
        // The space lies between "a" and "b", and takes room.
        assert!(marks[0].x1 > marks[0].x0);
        assert!(marks[0].ascent > 0.0);
        // The empty paragraph ends at its start, on the next line.
        assert!(marks[3].baseline > marks[2].baseline);
    }
}
