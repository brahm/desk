// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pages of a word processing document.

use std::collections::HashMap;
use std::rc::Rc;

use desk_model::{
    Document, Layout, MarginKind, MarginPlace, PageSetup, ParaBorder, ParaFormat, Paragraph,
    ParagraphSpacing, Rgba, Section, SectionStart, Start,
};
use desk_text::{Column, FieldValues, Line, ParagraphText, TextSystem};

use crate::objects::{Floating, Piece, Place, PlacedObject, WrapSpace, in_the_way};

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

/// The part of a paragraph on a page, for what is drawn behind and around
/// its text: its background and its rules or border (FMT-20, FMT-21).
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedBlock {
    /// The index of the paragraph in the body.
    pub paragraph: usize,
    /// The top of the part, above the space of a rule above.
    pub top: f32,
    /// The top of the first line.
    pub text_top: f32,
    /// The bottom of the last line.
    pub text_bottom: f32,
    /// How much higher than its lines Pages draws the background of the
    /// part, at its top and at its bottom: the part of the ascent of the
    /// first and last lines that their boxes round off (FMT-20).
    pub rise: (f32, f32),
    /// The bottom of the part, below the space of a rule below.
    pub bottom: f32,
    /// The left edge of the text column.
    pub left: f32,
    /// The right edge of the text column.
    pub right: f32,
    /// Whether the paragraph starts in this part, which then has the rule
    /// above.
    pub first: bool,
    /// Whether the paragraph ends in this part, which then has the rule
    /// below.
    pub last: bool,
    /// The color behind the paragraph.
    pub background: Option<Rgba>,
    /// The rules or border.
    pub border: ParaBorder,
}

impl PlacedBlock {
    /// Whether anything is drawn for it.
    pub fn is_visible(&self) -> bool {
        self.background.is_some() || self.border.sides != desk_model::BorderSides::None
    }
}

/// One page.
#[derive(Clone, Debug, Default)]
pub struct Page {
    /// The number that the page shows, by the numbering of its section
    /// (EST-18).
    pub number: u32,
    /// The index of its section in the body, from 0.
    pub section: usize,
    /// Whether it is the first page of its section.
    pub first_of_section: bool,
    /// Whether it is a page left blank so that the next section starts
    /// on the side it asks for. Pages shows no header or footer on it
    /// (est-12-16-section-headers).
    pub blank: bool,
    /// Whether it is a left page, when pages face each other (EST-11).
    /// The first page is a right page.
    pub left: bool,
    /// The left and right margins of the page, which swap on left pages
    /// when pages face each other.
    pub margins: (f32, f32),
    /// The top and bottom of the body on the page, from the top of the
    /// paper: the top and bottom margins, unless the header or the footer
    /// reaches past them and pushes the body away (EST-12).
    pub body: (f32, f32),
    /// The lines on the page, from top to bottom and column by column.
    pub lines: Vec<PlacedLine>,
    /// The parts of paragraphs on the page that have a background or a
    /// border, from top to bottom.
    pub blocks: Vec<PlacedBlock>,
    /// The lines of the header and the footer of the page, from top to
    /// bottom (EST-12). Their paragraphs are indexes in the header or
    /// footer they come from.
    pub margin_lines: Vec<PlacedLine>,
    /// For each line of [`Page::margin_lines`], the header or footer it
    /// belongs to, so that it can be edited on the page.
    pub margin_places: Vec<MarginPlace>,
    /// The notes on the page: its footnotes at its foot, or the endnotes
    /// that end up on it (EST-13). Their paragraphs are the indexes of the
    /// notes, in the order the body refers to them.
    pub note_lines: Vec<PlacedLine>,
    /// For each line of [`Page::note_lines`], the index of its paragraph
    /// in the note.
    pub note_parts: Vec<usize>,
    /// The footnotes that the lines of the page refer to, as the body
    /// paragraph that refers to each and the index of the note.
    pub footnotes: Vec<(usize, usize)>,
    /// The last footnote of the page when only its first lines fit on it,
    /// which happens only when its line starts the page, as in Pages:
    /// the paragraph that refers to it, the note and how many of its lines
    /// the page shows. The rest go on at the foot of the next page.
    pub split_note: Option<(usize, usize, usize)>,
    /// The rest of a footnote that began on a page before: the note, its
    /// first line here and the line after its last here, above the notes
    /// of the page. A rest taller than the page goes on to the next one.
    pub carried_note: Option<(usize, usize, usize)>,
    /// The left end of the short rule above the notes on the page, when
    /// there is one (EST-13).
    pub note_rule: Option<(f32, f32)>,
    /// The floating objects on the page, from back to front (OBJ-01).
    pub objects: Vec<PlacedObject>,
}

/// The length of the rule above notes, as Pages draws it.
pub const NOTE_RULE_LENGTH: f32 = 100.0;
/// The width of the rule above notes.
pub const NOTE_RULE_WIDTH: f32 = 0.5;
/// How far the rule above notes is from the first note below it.
pub(crate) const NOTE_RULE_BELOW: f32 = 8.3;
/// The room above the first note of a page or of a section, with its
/// rule: as much below the text above it as above the note.
pub(crate) const NOTE_SEPARATION: f32 = 16.3;

/// A word processing document laid out on pages.
#[derive(Clone, Debug)]
pub struct DocumentLayout {
    /// The size and margins of every page.
    pub setup: PageSetup,
    /// The pages. There is always at least one.
    pub pages: Vec<Page>,
    /// The sections that have pages, in order: [`Page::section`] is an
    /// index in it.
    pub sections: Vec<Section>,
    /// The section whose headers and footers each section shows, by the
    /// paragraph that starts it, or none for those of the first section
    /// (EST-16).
    pub section_margins: Vec<Option<desk_model::NodeId>>,
    /// What the fields of each paragraph of the body show, when the
    /// document has fields (EST-27); empty when it is laid out without
    /// them.
    pub field_values: Vec<FieldValues>,
    /// The notes, in the order that the body refers to them: the
    /// paragraphs of [`Page::note_lines`] are indexes in it.
    pub notes: Vec<desk_model::NodeId>,
}

impl DocumentLayout {
    /// How many lines the text has, in every page (DOC-20).
    pub fn line_count(&self) -> usize {
        self.pages.iter().map(|p| p.lines.len()).sum()
    }

    /// How many lines hold some of the text from `start` to `end`, each
    /// a paragraph and an offset in it (DOC-20).
    pub fn lines_between(&self, start: (usize, usize), end: (usize, usize)) -> usize {
        self.pages
            .iter()
            .flat_map(|p| &p.lines)
            .filter(|l| {
                let first = (l.paragraph, l.line.range.start);
                let last = (l.paragraph, l.line.range.end);
                first < end && start < last.max((l.paragraph, l.line.range.start + 1))
            })
            .count()
    }

    /// The page that a paragraph starts on.
    pub fn page_of(&self, paragraph: usize) -> Option<usize> {
        self.pages
            .iter()
            .position(|page| page.lines.iter().any(|l| l.paragraph == paragraph))
    }
}

/// Lays out the body of a document on pages, with the headers and
/// footers of the pages and its notes.
///
/// A field in the body shows the number of its page and the number of
/// pages: the body is laid out once to know them and again with them
/// (EST-27). Notes numbered again on each page need the same.
pub fn paginate(document: &Document, system: &mut TextSystem) -> DocumentLayout {
    let settings = document.text_settings();
    let setup = *document.page_setup();
    let sum = settings.spacing == ParagraphSpacing::Sum;
    let plan = crate::notes::NotePlan::new(document);
    let labels = plan.labels(document, None);
    let objects = crate::objects::inline_sizes(document);
    let mut values: Vec<FieldValues> = vec![
        FieldValues {
            notes: Some(labels.clone()),
            objects: objects.clone(),
            ..FieldValues::default()
        };
        document.body().len()
    ];
    let mut layout = plan.paginate(document, &values, &labels, system, setup, sum);
    let with_fields = document.body().iter().any(has_fields);
    if with_fields || plan.numbered_by_page(document) {
        let labels = plan.labels(document, Some(&layout));
        let pages = layout.pages.len() as u32;
        // The number of the page that each paragraph starts on.
        let mut numbers = vec![1; document.body().len()];
        let mut seen = vec![false; document.body().len()];
        for page in &layout.pages {
            for line in &page.lines {
                if let Some(false) = seen.get(line.paragraph).copied() {
                    seen[line.paragraph] = true;
                    numbers[line.paragraph] = page.number;
                }
            }
        }
        let pages_of: std::sync::Arc<HashMap<desk_model::NodeId, u32>> = std::sync::Arc::new(
            document
                .body()
                .iter()
                .zip(&numbers)
                .map(|(p, n)| (p.id(), *n))
                .collect(),
        );
        for (i, value) in values.iter_mut().enumerate() {
            *value = FieldValues {
                page: numbers[i],
                pages,
                notes: Some(labels.clone()),
                pages_of: Some(pages_of.clone()),
                objects: objects.clone(),
            };
        }
        layout = plan.paginate(document, &values, &labels, system, setup, sum);
    }
    if let Some(count) = setup.layout_pages {
        layout_pages(document, &mut layout, count);
    }
    place_margins(document, &mut layout, system);
    crate::objects::place_master_objects(document, &mut layout);
    crate::objects::place_inline_objects(document, &mut layout);
    crate::objects::place_object_text(document, &mut layout, system);
    layout.field_values = values;
    layout.notes = plan.note_ids();
    layout
}

/// The pages of a page layout document: as many as it has, without the
/// text of the body, each with the objects fixed on it, from back to
/// front (INT-03). Objects on pages past the last go on the last, and
/// those that move with a paragraph on the first.
fn layout_pages(document: &Document, layout: &mut DocumentLayout, count: u32) {
    let count = count.max(1) as usize;
    let setup = layout.setup;
    let body = layout.pages.first().map_or(
        (
            setup.margin_top.to_pt().round() as f32,
            (setup.height - setup.margin_bottom).to_pt().round() as f32,
        ),
        |p| p.body,
    );
    let (left, right) = (
        setup.margin_left.to_pt() as f32,
        setup.margin_right.to_pt() as f32,
    );
    let floats = crate::objects::floating(document);
    layout.pages = (0..count)
        .map(|index| {
            let facing_left = setup.facing_pages && index % 2 == 1;
            let objects = floats
                .iter()
                .filter(|f| match f.anchor {
                    Place::Page(page) => page.min(count - 1) == index,
                    Place::Paragraph(_) => index == 0,
                })
                .map(|f| PlacedObject::new(&f.object, None, 0.0))
                .collect();
            Page {
                number: index as u32 + 1,
                section: 0,
                first_of_section: index == 0,
                left: facing_left,
                margins: if facing_left {
                    (right, left)
                } else {
                    (left, right)
                },
                body,
                objects,
                ..Page::default()
            }
        })
        .collect();
    layout.sections.truncate(1);
    layout.section_margins.truncate(1);
}

/// Whether a paragraph has a field.
fn has_fields(paragraph: &Paragraph) -> bool {
    paragraph.runs().any(|run| {
        // The marks of objects show the same on every page.
        run.attrs.field.is_some()
            && !matches!(
                run.attrs.field,
                Some(Some(desk_model::Field::ObjectMark { .. }))
            )
    })
}

/// Lays out the header and the footer of each page (EST-12, EST-19).
///
/// The rules, from the PDFs of Pages:
///
/// - The top of the first line of the header is the header margin below
///   the top of the paper, and the bottom of the text of the last line of
///   the footer the footer margin above its bottom.
/// - They span the page between its left and right margins.
/// - The first page of a section whose first page differs takes the first
///   page header, and the left pages of a section whose left and right
///   pages differ take the left one: every second page, whether pages
///   face each other or not.
pub fn place_margins(document: &Document, layout: &mut DocumentLayout, system: &mut TextSystem) {
    let setup = layout.setup;
    for index in 0..layout.pages.len() {
        let mut lines = Vec::new();
        let mut places = Vec::new();
        if !layout.pages[index].blank {
            for (kind, shown) in [
                (MarginKind::Header, setup.headers),
                (MarginKind::Footer, setup.footers),
            ] {
                if !shown {
                    continue;
                }
                let (place, section) = margin_of_page(layout, index, kind);
                let margins = document.section_headers_footers(section);
                let paragraphs = margins.get(place);
                if paragraphs.iter().all(Paragraph::is_empty) {
                    continue;
                }
                let placed = margin_lines(document, layout, index, kind, paragraphs, system);
                places.extend(placed.iter().map(|_| place));
                lines.extend(placed);
            }
        }
        let page = &mut layout.pages[index];
        page.margin_lines = lines;
        page.margin_places = places;
    }
}

/// The header or footer that page `index` shows, and the section whose
/// headers and footers it is, by the paragraph that starts it, or none
/// for the first (EST-12, EST-16, EST-19).
pub fn margin_of_page(
    layout: &DocumentLayout,
    index: usize,
    kind: MarginKind,
) -> (MarginPlace, Option<desk_model::NodeId>) {
    let setup = layout.setup;
    let page = &layout.pages[index];
    let section = layout
        .sections
        .get(page.section)
        .copied()
        .unwrap_or(setup.section);
    let place = MarginPlace::of_page(
        kind,
        page.first_of_section,
        index % 2 == 1,
        section.first_page_different,
        section.left_right_different,
    );
    (
        place,
        layout.section_margins.get(page.section).copied().flatten(),
    )
}

/// The lines of a header or footer with these paragraphs on page `index`,
/// where the page shows it, even when they are empty: to show the cursor
/// in an empty header that is being edited (EST-12).
pub fn margin_lines(
    document: &Document,
    layout: &DocumentLayout,
    index: usize,
    kind: MarginKind,
    paragraphs: &[Paragraph],
    system: &mut TextSystem,
) -> Vec<PlacedLine> {
    let setup = layout.setup;
    let height = setup.height.to_pt() as f32;
    let page = &layout.pages[index];
    let values = FieldValues {
        page: page.number,
        pages: layout.pages.len() as u32,
        notes: None,
        pages_of: None,
        // Objects in the lines of headers and footers (EST-12).
        objects: crate::objects::inline_sizes(document),
    };
    let width = (setup.width.to_pt() as f32 - page.margins.0 - page.margins.1).max(1.0);
    let (mut lines, _) = margin_block(document, paragraphs, width, &values, system);
    for line in &mut lines {
        line.origin.0 = page.margins.0;
    }
    let shift = match kind {
        MarginKind::Header => setup.header_margin.to_pt() as f32,
        MarginKind::Footer => {
            let text_bottom = lines
                .last()
                .map_or(0.0, |l: &PlacedLine| l.origin.1 + l.line.text_bottom);
            height - setup.footer_margin.to_pt() as f32 - text_bottom
        }
    };
    for line in &mut lines {
        line.origin.1 += shift;
    }
    lines
}

/// The room that the headers and footers of a document take on its pages,
/// to push the body away from those that reach past the margins (EST-12).
pub(crate) struct MarginRoom {
    setup: PageSetup,
    /// For each text laid out, the section whose headers and footers it
    /// shows, by the paragraph that starts it.
    pub keys: Vec<Option<desk_model::NodeId>>,
    /// How far each header and footer of each such section reaches: the
    /// bottom of a header, and from the bottom of the text of a footer to
    /// its top, by the index of its place.
    heights: HashMap<(Option<desk_model::NodeId>, usize), f32>,
    /// The floating objects, with the texts they move with.
    pub floats: std::sync::Arc<Vec<Floating>>,
}

impl MarginRoom {
    /// The room of the headers and footers of a document, for its body.
    pub fn new(document: &Document, system: &mut TextSystem) -> Self {
        let setup = *document.page_setup();
        let mut keys = Vec::with_capacity(document.body().len());
        let mut current = None;
        for paragraph in document.body().iter() {
            if let Some(Some(start)) = &paragraph.attrs().start
                && let Start::Section(section, _) = &**start
                && section.own_margins
            {
                current = Some(paragraph.id());
            }
            keys.push(current);
        }
        let mut heights = HashMap::new();
        let width = setup.body_width().to_pt() as f32;
        for (key, margins) in document.all_headers_footers() {
            for place in MarginPlace::ALL {
                let shown = match place.kind {
                    MarginKind::Header => setup.headers,
                    MarginKind::Footer => setup.footers,
                };
                let paragraphs = margins.get(place);
                if !shown || paragraphs.iter().all(Paragraph::is_empty) {
                    continue;
                }
                let values = FieldValues {
                    objects: crate::objects::inline_sizes(document),
                    ..FieldValues::default()
                };
                let (lines, bottom) = margin_block(document, paragraphs, width, &values, system);
                let reach = match place.kind {
                    MarginKind::Header => bottom,
                    MarginKind::Footer => {
                        let text_bottom = lines
                            .last()
                            .map_or(0.0, |l: &PlacedLine| l.origin.1 + l.line.text_bottom);
                        let top = lines.first().map_or(0.0, |l| l.origin.1 + l.line.top);
                        text_bottom - top
                    }
                };
                heights.insert((key, place.index()), reach);
            }
        }
        Self {
            setup,
            keys,
            heights,
            floats: crate::objects::floating(document),
        }
    }

    /// The same room, for texts that show the headers and footers of the
    /// sections of these keys.
    pub fn with_keys(&self, keys: Vec<Option<desk_model::NodeId>>) -> Self {
        Self {
            setup: self.setup,
            keys,
            heights: self.heights.clone(),
            floats: self.floats.clone(),
        }
    }

    /// The top and bottom of the body of a page of a section.
    fn body(
        &self,
        margins: Option<desk_model::NodeId>,
        first_of_section: bool,
        left: bool,
        section: &Section,
    ) -> (f32, f32) {
        let setup = &self.setup;
        let reach = |kind| {
            let place = MarginPlace::of_page(
                kind,
                first_of_section,
                left,
                section.first_page_different,
                section.left_right_different,
            );
            self.heights.get(&(margins, place.index())).copied()
        };
        // Pages puts the frame of the body on whole points.
        let mut top = setup.margin_top.to_pt().round() as f32;
        if let Some(h) = reach(MarginKind::Header) {
            top = top.max(setup.header_margin.to_pt() as f32 + h);
        }
        let mut bottom = setup.margin_bottom.to_pt() as f32;
        if let Some(h) = reach(MarginKind::Footer) {
            bottom = bottom.max(setup.footer_margin.to_pt() as f32 + h);
        }
        (top, (setup.height.to_pt() as f32 - bottom).round())
    }
}

/// The lines of a header or a footer from the top of its first line, and
/// the bottom of its last line.
fn margin_block(
    document: &Document,
    paragraphs: &[Paragraph],
    width: f32,
    values: &FieldValues,
    system: &mut TextSystem,
) -> (Vec<PlacedLine>, f32) {
    let styles = document.styles();
    let settings = document.text_settings();
    let items = styles.list_items(paragraphs.iter());
    let mut lines = Vec::new();
    let mut y = 0.0f32;
    let mut after = 0.0f32;
    for (i, paragraph) in paragraphs.iter().enumerate() {
        let text = ParagraphText::new_in_document_with(
            styles,
            settings,
            paragraph,
            items[i].as_ref(),
            values.clone(),
        );
        let format = text.paragraph().clone();
        let laid = system.layout(&text, &Column::new(width), 0.0).lines;
        let Some(first) = laid.first() else {
            continue;
        };
        if i > 0 {
            y += after.max(format.space_before.to_pt() as f32);
        }
        let dy = y - first.top;
        let mut bottom = y;
        for (n, line) in laid.iter().enumerate() {
            bottom = dy + line.top + line.height;
            lines.push(PlacedLine {
                paragraph: i,
                index: n,
                line: line.clone(),
                origin: (0.0, dy),
            });
        }
        y = bottom;
        after = format.space_after.to_pt() as f32;
    }
    (lines, y)
}

/// A paragraph laid out at the top of an endless column of one width.
struct Block {
    format: ParaFormat,
    lines: Vec<Line>,
}

impl Block {
    /// The space that the rule below takes, if there is one.
    fn below(&self) -> f32 {
        let border = &self.format.border;
        if border.sides.below() {
            border.space().to_pt() as f32
        } else {
            0.0
        }
    }

    /// The height of lines `a..b`.
    fn height(&self, a: usize, b: usize) -> f32 {
        if a >= b {
            return 0.0;
        }
        self.height_from(a, b, self.lines[a].top)
    }

    /// The height of lines `a..b` from `base`, where line `a` would start
    /// without an object above it.
    fn height_from(&self, a: usize, b: usize, base: f32) -> f32 {
        if a >= b {
            return 0.0;
        }
        let last = &self.lines[b - 1];
        last.top + last.height - base
    }

    /// How many lines from `a` on fit in `room` below `base`: as in Pages,
    /// a line fits when its text does, without the leading below it.
    fn fitting(&self, a: usize, base: f32, room: f32) -> usize {
        (a..self.lines.len())
            .take_while(|&b| self.lines[b].text_bottom - base <= room + TOLERANCE)
            .count()
    }
}

/// The part of a page that a layout takes: its columns, from a top to
/// the bottom of the body.
#[derive(Clone, Copy)]
struct Region {
    layout: Layout,
    /// Where its columns start, from the top of the body.
    top: f32,
}

/// Where the next line goes.
#[derive(Clone, Copy)]
struct Cursor {
    /// The page, as an index.
    page: usize,
    /// The layout on the page.
    region: Region,
    /// The column of the layout.
    column: usize,
    /// The distance from the top of the body.
    y: f32,
    /// The bottom of the lowest column of the layout on the page so far.
    lowest: f32,
    /// The space below the last paragraph placed, which collapses with
    /// the space above the next one.
    after: f32,
    /// Whether nothing was placed in the column yet.
    empty: bool,
}

impl Cursor {
    /// A cursor at the top of the columns of a region.
    fn at(page: usize, region: Region) -> Self {
        Self {
            page,
            region,
            column: 0,
            y: region.top,
            lowest: region.top,
            after: 0.0,
            empty: true,
        }
    }
}

/// Lays out prepared paragraphs on pages of one setup.
///
/// The rules, to be checked against the PDFs of Pages in the reference VM:
///
/// - Between two paragraphs, the larger of the space after the first and
///   the space before the second applies (FMT-15). At the top of a column
///   the space before is dropped, and the space after the last paragraph
///   of a column is never carried over.
/// - A paragraph that starts on a new page does so unless it is already at
///   the top of one (EST-04); one that starts in a new column goes to the
///   next column, or the next page from the last (EST-08).
/// - Keeping lines together moves a paragraph that does not fit to the
///   next column, unless it is taller than a column (EST-05).
/// - Widow control leaves at least two lines of a paragraph at the bottom
///   of a column and at the top of the next (EST-05).
/// - A rule above a paragraph takes its offset and width between the
///   space above and the text, and a rule below as much under the text,
///   as in Pages (FMT-21). A border takes both.
/// - Keeping a paragraph with the next one moves the paragraph, and any
///   paragraphs kept with it before, to the next column when the next one
///   would start in another (EST-05). A chain that starts at the top of a
///   column breaks normally, since moving it would not help.
/// - Text fills the columns of a layout one after the other, without
///   balancing them. A layout starts below the lowest column of the one
///   before it on the page, after the space after that one and the space
///   before its own, or on a new page when it says so (EST-09, EST-10).
/// - A section starts on a new page, the next right or left one when it
///   says so, with a blank page before it when needed, and numbers its
///   pages from its first number or on from the section before (EST-16,
///   EST-18).
pub fn paginate_texts(
    setup: PageSetup,
    texts: &[ParagraphText],
    system: &mut TextSystem,
) -> DocumentLayout {
    paginate_texts_spaced(setup, texts, system, false)
}

/// Lays out prepared paragraphs on pages, as [`paginate_texts`] does, with
/// the space between two paragraphs the sum of the space after the first
/// and before the second when `sum` is true, as in documents of other
/// programs.
pub fn paginate_texts_spaced(
    setup: PageSetup,
    texts: &[ParagraphText],
    system: &mut TextSystem,
    sum: bool,
) -> DocumentLayout {
    paginate_with_footnotes(setup, texts, system, sum, None, None)
}

/// How many times a document with notes in later columns is laid out, at
/// most, for their room to settle.
const MAX_NOTE_PASSES: usize = 4;

/// A footnote met in a column of a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct NoteAhead {
    page: usize,
    column: usize,
    note: usize,
}

/// Lays out prepared paragraphs on pages with their footnotes, which take
/// room at the foot of the pages that refer to them.
pub(crate) fn paginate_with_footnotes(
    setup: PageSetup,
    texts: &[ParagraphText],
    system: &mut TextSystem,
    sum: bool,
    footnotes: Option<Rc<Footnotes>>,
    room: Option<&MarginRoom>,
) -> DocumentLayout {
    // A note met in a later column of a page takes its room from the
    // columns before it too, as in Pages: the page is laid out again with
    // that room kept from its first column, until the notes stay on the
    // pages and in the columns they were met in (EST-07, EST-13).
    let mut ahead: Vec<NoteAhead> = Vec::new();
    let mut blocks = HashMap::new();
    let mut passes = 0;
    let mut layout = loop {
        let mut paginator = Paginator {
            setup,
            texts,
            system: &mut *system,
            sum,
            footnotes: footnotes.clone(),
            blocks: std::mem::take(&mut blocks),
            floats: room.map(|r| r.floats.clone()).unwrap_or_default(),
            pages: Vec::new(),
            sections: Vec::new(),
            section_margins: Vec::new(),
            room,
            ahead: ahead.clone(),
            met: Vec::new(),
        };
        let layout = paginator.run();
        let mut met: Vec<NoteAhead> = paginator.met.into_iter().filter(|m| m.column > 0).collect();
        met.sort_unstable();
        met.dedup();
        passes += 1;
        if met == ahead || passes == MAX_NOTE_PASSES {
            break layout;
        }
        blocks = paginator.blocks;
        ahead = met;
    };
    if let Some(footnotes) = footnotes {
        footnotes.place(&mut layout);
    }
    layout
}

/// The footnotes of a document, laid out at the width of the body.
pub(crate) struct Footnotes {
    /// For each paragraph of the body, the notes it refers to: the offset
    /// of each mark and the index of its note.
    pub refs: Vec<Vec<(usize, usize)>>,
    /// The lines of each note, with the offset of each from the top of
    /// the note, and its height.
    pub notes: Vec<(Vec<(Line, f32)>, f32)>,
    /// For each line of each note, the index of its paragraph in the note.
    pub parts: Vec<Vec<usize>>,
    /// The space between two notes, and above the first.
    pub gap: f32,
}

impl Footnotes {
    /// The room that the notes a line refers to take, below notes already
    /// on its page or as the first.
    fn cost(&self, paragraph: usize, line: &Line, first: bool) -> f32 {
        let mut first = first;
        self.refs
            .get(paragraph)
            .into_iter()
            .flatten()
            .filter(|(offset, _)| line.range.contains(offset))
            .map(|&(_, note)| {
                let above = if std::mem::take(&mut first) {
                    NOTE_SEPARATION
                } else {
                    self.gap
                };
                self.notes[note].1 + above
            })
            .sum()
    }

    /// How far the top of line `line` of a note is from the top of the
    /// note; its height for the line after the last.
    fn offset(&self, note: usize, line: usize) -> f32 {
        let (lines, height) = &self.notes[note];
        lines.get(line).map_or(*height, |(line, dy)| dy + line.top)
    }

    /// The line after the last of a note, from line `from`, that fits in
    /// `room` with the rule above it: the end of the note when it fits,
    /// and one line at least.
    fn carried_end(&self, note: usize, from: usize, room: f32) -> usize {
        let count = self.notes[note].0.len();
        let top = self.offset(note, from);
        (from + 1..=count)
            .take_while(|&m| self.offset(note, m) - top + NOTE_SEPARATION <= room + TOLERANCE)
            .last()
            .unwrap_or(from + 1)
    }

    /// The lines of each note of a page, in order: the rest of the one
    /// carried from the page before, then its own.
    fn parts(&self, page: &Page) -> Vec<(usize, std::ops::Range<usize>)> {
        let carried = page.carried_note.map(|(note, from, to)| (note, from..to));
        carried
            .into_iter()
            .chain(page.footnotes.iter().map(|&(paragraph, note)| {
                let end = match page.split_note {
                    Some((p, n, lines)) if p == paragraph && n == note => lines,
                    _ => self.notes[note].0.len(),
                };
                (note, 0..end)
            }))
            .collect()
    }

    /// The room that the notes of a page take, with the rule above them.
    fn reserved(&self, page: &Page) -> f32 {
        self.reserved_with(page, &[])
    }

    /// The room that the notes of a page take, and the whole of `later`
    /// notes below them, with the rule above them.
    fn reserved_with(&self, page: &Page, later: &[usize]) -> f32 {
        let mut parts = self.parts(page);
        parts.extend(
            later
                .iter()
                .map(|&note| (note, 0..self.notes[note].0.len())),
        );
        if parts.is_empty() {
            return 0.0;
        }
        parts
            .iter()
            .map(|(note, lines)| {
                self.offset(*note, lines.end) - self.offset(*note, lines.start) + self.gap
            })
            .sum::<f32>()
            - self.gap
            + NOTE_SEPARATION
    }

    /// When the notes of a line do not all fit in `room`, with the line,
    /// whether they would with only the first lines of the last of them:
    /// that note and how many of its lines fit. At least one line of it
    /// must fit, and one line must go on to the next page.
    fn split(
        &self,
        paragraph: usize,
        line: &Line,
        first: bool,
        room: f32,
    ) -> Option<(usize, usize)> {
        let notes: Vec<usize> = self
            .refs
            .get(paragraph)?
            .iter()
            .filter(|(offset, _)| line.range.contains(offset))
            .map(|&(_, note)| note)
            .collect();
        let (&last, before) = notes.split_last()?;
        let mut used: f32 = before
            .iter()
            .enumerate()
            .map(|(k, &note)| {
                let above = if first && k == 0 {
                    NOTE_SEPARATION
                } else {
                    self.gap
                };
                self.notes[note].1 + above
            })
            .sum();
        used += if first && before.is_empty() {
            NOTE_SEPARATION
        } else {
            self.gap
        };
        let count = self.notes[last].0.len();
        let fit = (1..count)
            .take_while(|&m| used + self.offset(last, m) <= room + TOLERANCE)
            .last()?;
        Some((last, fit))
    }

    /// Puts the notes of each page at its foot, the last one on the bottom
    /// margin, under a short rule.
    fn place(&self, layout: &mut DocumentLayout) {
        for page in &mut layout.pages {
            let parts = self.parts(page);
            if parts.is_empty() {
                continue;
            }
            let bottom = page.body.1;
            let mut y = bottom - self.reserved(page) + NOTE_SEPARATION;
            page.note_rule = Some((page.margins.0, y - NOTE_RULE_BELOW));
            for (note, range) in parts {
                let lines = &self.notes[note].0;
                let top = self.offset(note, range.start);
                for index in range.clone() {
                    let (line, dy) = &lines[index];
                    page.note_lines.push(PlacedLine {
                        paragraph: note,
                        index,
                        line: line.clone(),
                        origin: (page.margins.0, y + dy - top),
                    });
                    page.note_parts
                        .push(self.parts[note].get(index).copied().unwrap_or(0));
                }
                y += self.offset(note, range.end) - top + self.gap;
            }
        }
    }
}

struct Paginator<'a> {
    setup: PageSetup,
    texts: &'a [ParagraphText],
    system: &'a mut TextSystem,
    sum: bool,
    footnotes: Option<Rc<Footnotes>>,
    /// The paragraphs laid out so far, by index and column width.
    blocks: HashMap<(usize, u32), Rc<Block>>,
    /// The floating objects.
    floats: std::sync::Arc<Vec<Floating>>,
    pages: Vec<Page>,
    /// The sections that have pages, in order.
    sections: Vec<Section>,
    /// The headers and footers that each section shows.
    section_margins: Vec<Option<desk_model::NodeId>>,
    /// The room the headers and footers take, when known.
    room: Option<&'a MarginRoom>,
    /// The footnotes that a layout before met in later columns, whose
    /// room the columns before them keep.
    ahead: Vec<NoteAhead>,
    /// The footnotes met so far, by page and column.
    met: Vec<NoteAhead>,
}

impl Paginator<'_> {
    fn top(&self) -> f32 {
        self.setup.margin_top.to_pt().round() as f32
    }

    /// The left and right margins of a page, by its index: they swap on
    /// left pages when pages face each other.
    fn margins(&self, page: usize) -> (f32, f32) {
        let (left, right) = (
            self.setup.margin_left.to_pt() as f32,
            self.setup.margin_right.to_pt() as f32,
        );
        if self.setup.facing_pages && page % 2 == 1 {
            (right, left)
        } else {
            (left, right)
        }
    }

    /// The left edge and width of a column of a region on a page.
    fn column(&self, page: usize, region: &Region, column: usize) -> (f32, f32) {
        let (left, right) = self.margins(page);
        let layout = &region.layout;
        let inner_left = layout.left.to_pt() as f32;
        let width = self.setup.width.to_pt() as f32
            - left
            - right
            - inner_left
            - layout.right.to_pt() as f32;
        let spans = layout.columns.spans(f64::from(width.max(1.0)));
        let (cx, cw) = spans[column.min(spans.len() - 1)];
        (left + inner_left + cx as f32, cw as f32)
    }

    /// The paragraph laid out in a column of a width.
    fn block(&mut self, index: usize, width: f32) -> Rc<Block> {
        let key = (index, width.to_bits());
        if !self.blocks.contains_key(&key) {
            let text = &self.texts[index];
            let lines = self.system.layout(text, &Column::new(width), 0.0).lines;
            self.blocks.insert(
                key,
                Rc::new(Block {
                    format: text.paragraph().clone(),
                    lines,
                }),
            );
        }
        self.blocks[&key].clone()
    }

    /// The paragraph laid out in a column of a width, part by part around
    /// the objects on the pages it is on.
    fn wrapped_block(&mut self, index: usize, width: f32, pieces: &[Piece]) -> Rc<Block> {
        let text = &self.texts[index];
        let space = WrapSpace { width, pieces };
        let lines = self.system.layout(text, &space, 0.0).lines;
        Rc::new(Block {
            format: text.paragraph().clone(),
            lines,
        })
    }

    /// The objects that move with paragraph `paragraph`, placed with the
    /// top of the paragraph at `top` on the page.
    fn anchored(&self, paragraph: usize, top: f32) -> Vec<PlacedObject> {
        self.floats
            .iter()
            .filter(|f| f.anchor == Place::Paragraph(paragraph))
            .map(|f| PlacedObject::new(&f.object, Some(paragraph), top))
            .collect()
    }

    /// Adds a page of the last section, numbered after the page before or
    /// from the first number of the section, and returns its index.
    fn add_page(&mut self, first_of_section: bool) -> usize {
        let index = self.pages.len();
        let next = self.pages.last().map_or(1, |p| p.number + 1);
        let number = if first_of_section {
            self.sections
                .last()
                .and_then(|s| s.first_number)
                .unwrap_or(next)
        } else {
            next
        };
        let section = self.sections.last().copied().unwrap_or(self.setup.section);
        let margins = self.section_margins.last().copied().flatten();
        let body = match self.room {
            Some(room) => room.body(margins, first_of_section, index % 2 == 1, &section),
            None => (
                self.top(),
                (self.setup.height - self.setup.margin_bottom).to_pt() as f32,
            ),
        };
        // The rest of a note split on the page before, or of one that
        // filled it and goes on.
        let carried_from = self.pages.last().and_then(|p| {
            p.split_note
                .map(|(_, note, lines)| (note, lines))
                .or_else(|| p.carried_note.map(|(note, _, to)| (note, to)))
        });
        let carried_note = match (carried_from, &self.footnotes) {
            (Some((note, from)), Some(notes)) if from < notes.notes[note].0.len() => {
                let to = notes.carried_end(note, from, body.1 - body.0);
                Some((note, from, to))
            }
            _ => None,
        };
        let objects = self
            .floats
            .iter()
            .filter(|f| f.anchor == Place::Page(index))
            .map(|f| PlacedObject::new(&f.object, None, 0.0))
            .collect();
        self.pages.push(Page {
            number,
            section: self.sections.len().saturating_sub(1),
            first_of_section,
            left: self.setup.facing_pages && index % 2 == 1,
            margins: self.margins(index),
            body,
            carried_note,
            objects,
            ..Page::default()
        });
        index
    }

    /// A cursor at the top of a new page, in a layout that starts there
    /// when `starts`, after the space before it, or goes on from the page
    /// before.
    fn new_page(&mut self, layout: Layout, first_of_section: bool, starts: bool) -> Cursor {
        let page = self.add_page(first_of_section);
        let top = if starts {
            layout.before.to_pt() as f32
        } else {
            0.0
        };
        Cursor::at(page, Region { layout, top })
    }

    /// The next column of the layout, or the first of a new page.
    fn next_column(&mut self, cursor: Cursor) -> Cursor {
        if cursor.column + 1 < usize::from(cursor.region.layout.columns.count.max(1)) {
            return Cursor {
                column: cursor.column + 1,
                y: cursor.region.top,
                after: 0.0,
                empty: true,
                ..cursor
            };
        }
        self.new_page(cursor.region.layout, false, false)
    }

    /// The cursor where a layout that starts after a cursor begins: below
    /// the lowest column of the layout before, or on a new page.
    fn start_layout(&mut self, cursor: Cursor, layout: Layout) -> Cursor {
        let empty = self.pages[cursor.page].lines.is_empty();
        if layout.new_page && !empty {
            return self.new_page(layout, false, true);
        }
        let top = if empty {
            0.0
        } else {
            cursor.lowest + cursor.region.layout.after.to_pt() as f32
        };
        Cursor::at(
            cursor.page,
            Region {
                layout,
                top: top + layout.before.to_pt() as f32,
            },
        )
    }

    /// The cursor at the start of a section, on the page it starts on.
    fn start_section(&mut self, paragraph: usize, section: Section, layout: Layout) -> Cursor {
        // A page left empty takes the section instead; when it was the
        // first of its section, that section has nothing else.
        if self.pages.last().is_some_and(|p| p.lines.is_empty())
            && let Some(page) = self.pages.pop()
            && page.first_of_section
        {
            self.sections.pop();
            self.section_margins.pop();
        }
        let left = match section.start {
            SectionStart::AnyPage => None,
            SectionStart::RightPage => Some(false),
            SectionStart::LeftPage => Some(true),
        };
        // A blank page of the section before, so that the section starts
        // on the side it asks for. Without facing pages, the odd pages
        // count as right pages too.
        if let Some(left) = left
            && !self.pages.is_empty()
            && (self.pages.len() % 2 == 1) != left
        {
            let blank = self.add_page(false);
            self.pages[blank].blank = true;
        }
        self.sections.push(section);
        let margins = self
            .room
            .and_then(|r| r.keys.get(paragraph).copied().flatten());
        self.section_margins.push(margins);
        self.new_page(layout, true, true)
    }

    fn run(&mut self) -> DocumentLayout {
        let n = self.texts.len();
        self.sections.push(self.setup.section);
        self.section_margins.push(None);
        let mut cursor = self.new_page(self.setup.layout, true, true);
        // Paragraphs that start a new column because a chain kept with the
        // next paragraph did not fit.
        let mut forced = vec![false; n];
        // The page and column where each paragraph ends.
        let mut ends = vec![(0, 0); n];
        // Where the current chain of paragraphs kept with the next one started.
        let mut chain: Option<(usize, Cursor, usize, usize)> = None;
        let mut i = 0;
        while i < n {
            let format = self.texts[i].paragraph().clone();
            // A section or a layout that starts here, once: a chain laid
            // out again starts after it.
            if !forced[i] {
                match format.start.as_deref() {
                    Some(Start::Section(section, layout)) => {
                        cursor = self.start_section(i, *section, *layout);
                    }
                    Some(Start::Layout(layout)) => {
                        cursor = self.start_layout(cursor, *layout);
                    }
                    None => {}
                }
            }
            if i == 0 || !self.texts[i - 1].paragraph().keep_with_next || format.start.is_some() {
                chain = Some((
                    i,
                    cursor,
                    self.pages.len(),
                    self.pages[cursor.page].lines.len(),
                ));
            }
            if format.page_break_before && !self.pages[cursor.page].lines.is_empty() {
                cursor = self.new_page(cursor.region.layout, false, false);
            } else if (forced[i] || format.column_break_before) && !cursor.empty {
                cursor = self.next_column(cursor);
            }
            let (start, end) = self.place(i, &mut cursor);
            ends[i] = end;

            // A paragraph kept with this one ended in another column: start
            // the chain in the next column and lay it out again.
            if i > 0
                && self.texts[i - 1].paragraph().keep_with_next
                && !format.page_break_before
                && format.start.is_none()
                && ends[i - 1] != start
                && let Some((first, saved, page_count, line_count)) = chain
                && line_count > 0
                && !forced[first]
            {
                forced[first] = true;
                self.pages.truncate(page_count);
                let page = &mut self.pages[saved.page];
                page.lines.truncate(line_count);
                page.blocks.retain(|b| b.paragraph < first);
                page.objects
                    .retain(|o| o.paragraph.is_none_or(|p| p < first));
                page.footnotes.retain(|&(p, _)| p < first);
                if page.split_note.is_some_and(|(p, _, _)| p >= first) {
                    page.split_note = None;
                }
                cursor = saved;
                i = first;
                continue;
            }
            i += 1;
        }
        // Objects on pages past the last go on the last.
        let count = self.pages.len();
        let past: Vec<PlacedObject> = self
            .floats
            .iter()
            .filter(|f| matches!(f.anchor, Place::Page(p) if p >= count))
            .map(|f| PlacedObject::new(&f.object, None, 0.0))
            .collect();
        if let Some(last) = self.pages.last_mut() {
            last.objects.extend(past);
        }
        // From back to front, as the document keeps them.
        if !self.floats.is_empty() {
            let order: HashMap<desk_model::NodeId, usize> = self
                .floats
                .iter()
                .enumerate()
                .map(|(i, f)| (f.object.id, i))
                .collect();
            for page in &mut self.pages {
                page.objects
                    .sort_by_key(|o| order.get(&o.object.id).copied());
            }
        }
        DocumentLayout {
            setup: self.setup,
            pages: std::mem::take(&mut self.pages),
            sections: std::mem::take(&mut self.sections),
            section_margins: std::mem::take(&mut self.section_margins),
            field_values: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Places paragraph `i` from a cursor, and returns the page and column
    /// where it starts and where it ends.
    fn place(&mut self, i: usize, cursor: &mut Cursor) -> ((usize, usize), (usize, usize)) {
        let sum = self.sum;
        let footnotes = self.footnotes.clone();
        let mut start = (cursor.page, cursor.column);
        let mut a = 0;
        // The parts of the paragraph so far, around the objects of their
        // pages, and the lines as last laid out.
        let mut pieces: Vec<Piece> = Vec::new();
        let mut laid: Option<Rc<Block>> = None;
        loop {
            // The body of the page, which a tall header or footer makes
            // smaller.
            let (top, bottom) = self.pages[cursor.page].body;
            let body = bottom - top;
            let (x, width) = self.column(cursor.page, &cursor.region, cursor.column);
            let column_height = body - cursor.region.top;
            let reserved = footnotes.as_ref().map_or(0.0, |n| {
                let page = &self.pages[cursor.page];
                // The notes a later column of the page will meet.
                let later: Vec<usize> = self
                    .ahead
                    .iter()
                    .filter(|h| h.page == cursor.page && h.column > cursor.column)
                    .map(|h| h.note)
                    .filter(|note| !page.footnotes.iter().any(|&(_, n)| n == *note))
                    .collect();
                n.reserved_with(page, &later)
            });
            let split_before = self.pages[cursor.page].split_note.is_some();
            let carries = self.pages[cursor.page].carried_note.is_some();
            let at_top = cursor.empty;
            let format = self.texts[i].paragraph();
            let before = format.space_before.to_pt() as f32;
            let collapsed = if a > 0 || at_top {
                0.0
            } else if sum {
                cursor.after + before
            } else {
                cursor.after.max(before)
            };
            let above = if a == 0 && format.border.sides.above() {
                format.border.space().to_pt() as f32
            } else {
                0.0
            };
            let space = collapsed + above;
            // Where line `a` would start, and where the top of the
            // paragraph is on the page for it to start there.
            if laid.as_ref().is_some_and(|b| a >= b.lines.len()) {
                break;
            }
            let base = match (&laid, a) {
                (Some(block), a) if a > 0 => block.lines[a].top,
                _ => 0.0,
            };
            let origin = top + cursor.y + space - base;
            // The objects that move with the paragraph go where it starts.
            let anchored = if a == 0 {
                self.anchored(i, origin)
            } else {
                Vec::new()
            };
            let exclusions: Vec<_> = self.pages[cursor.page]
                .objects
                .iter()
                .chain(&anchored)
                .filter_map(PlacedObject::exclusion)
                .filter(|e| in_the_way(e, x, x + width, top + cursor.y))
                .collect();
            pieces.retain(|p| p.from < base);
            pieces.push(Piece {
                from: if a == 0 { f32::NEG_INFINITY } else { base },
                left: x,
                origin,
                exclusions,
            });
            let block = if pieces.iter().all(|p| p.exclusions.is_empty()) {
                self.block(i, width)
            } else {
                self.wrapped_block(i, width, &pieces)
            };
            laid = Some(block.clone());
            let n = block.lines.len();
            if a >= n {
                break;
            }
            let format = &block.format;
            let room = body - cursor.y - space;
            let rest = n - a;
            let mut split: Option<(usize, usize, usize)> = None;
            let mut k = match &footnotes {
                None => block.fitting(a, base, room),
                Some(notes) => {
                    // Each line takes the room of its notes too, or of
                    // the first lines of its last note, whose rest goes
                    // on the next page.
                    let room = room - reserved;
                    let mut taken = 0.0;
                    let mut stop = false;
                    (a..n)
                        .take_while(|&b| {
                            if stop {
                                return false;
                            }
                            let line = &block.lines[b];
                            let first = reserved == 0.0 && taken == 0.0;
                            let cost = notes.cost(i, line, first);
                            let height = line.text_bottom - base;
                            if split_before && cost > 0.0 {
                                return false;
                            }
                            let fits = height <= room - taken - cost + TOLERANCE;
                            if fits {
                                taken += cost;
                                return true;
                            }
                            if height > room - taken + TOLERANCE {
                                return false;
                            }
                            // As in Pages, the line goes to the next page
                            // with its notes whole, unless it already
                            // starts an empty page, where the last note
                            // cannot fit whole anywhere
                            // (est-13-footnote-continues).
                            if !(at_top && b == a && reserved == 0.0) {
                                return false;
                            }
                            match notes.split(i, line, first, room - taken - height) {
                                Some((note, lines)) => {
                                    split = Some((b, note, lines));
                                    stop = true;
                                    true
                                }
                                None => false,
                            }
                        })
                        .count()
                }
            };
            if k < rest {
                if a == 0
                    && !at_top
                    && format.keep_lines_together
                    && block.height(0, n) <= column_height
                {
                    k = 0;
                } else if format.widow_control {
                    if a == 0 && k == 1 && rest > 1 {
                        // The first line alone at the bottom of a column.
                        k = 0;
                    } else if rest - k == 1 && k > 0 {
                        // The last line alone at the top of the next
                        // column: send one more, unless that leaves the
                        // first one alone.
                        k = if a == 0 && k == 2 { 0 } else { k - 1 };
                    }
                }
                // Nothing better is possible: overflow the column, unless
                // the rest of a note fills the page, which then goes on.
                if at_top && !carries {
                    k = k.max(1);
                }
            }
            if k == 0 {
                if a == 0 {
                    pieces.clear();
                    laid = None;
                }
                *cursor = self.next_column(*cursor);
                continue;
            }
            let k = k.min(rest);
            if a == 0 {
                start = (cursor.page, cursor.column);
            }
            let dy = origin;
            let lines: Vec<PlacedLine> = block.lines[a..a + k]
                .iter()
                .enumerate()
                .map(|(index, line)| PlacedLine {
                    paragraph: i,
                    index: a + index,
                    line: line.clone(),
                    origin: (x, dy),
                })
                .collect();
            let last = a + k == n;
            let below = if last { block.below() } else { 0.0 };
            let height = block.height_from(a, a + k, base);
            let placed = PlacedBlock {
                paragraph: i,
                top: top + cursor.y + collapsed,
                text_top: top + cursor.y + space,
                text_bottom: top + cursor.y + space + height,
                rise: (block.lines[a].rise, block.lines[a + k - 1].rise),
                bottom: top + cursor.y + space + height + below,
                left: x,
                right: x + width,
                first: a == 0,
                last,
                background: format.background,
                border: format.border,
            };
            let space_after = format.space_after.to_pt() as f32;
            let page = &mut self.pages[cursor.page];
            if let Some((line, note, lines)) = split
                && line + 1 == a + k
            {
                page.split_note = Some((i, note, lines));
            }
            if let Some(notes) = &footnotes {
                for line in &lines {
                    for &(offset, note) in notes.refs.get(i).into_iter().flatten() {
                        if line.line.range.contains(&offset) {
                            page.footnotes.push((i, note));
                            self.met.push(NoteAhead {
                                page: cursor.page,
                                column: cursor.column,
                                note,
                            });
                        }
                    }
                }
            }
            page.lines.extend(lines);
            page.objects.extend(anchored);
            if placed.is_visible() {
                page.blocks.push(placed);
            }
            cursor.y += space + height + below;
            cursor.lowest = cursor.lowest.max(cursor.y);
            cursor.empty = false;
            a += k;
            if a < n {
                *cursor = self.next_column(*cursor);
            } else {
                cursor.after = space_after;
            }
        }
        (start, (cursor.page, cursor.column))
    }
}
