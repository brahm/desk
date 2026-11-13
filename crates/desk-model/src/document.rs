// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The document, its container and its metadata.

use unicode_segmentation::UnicodeSegmentation;

use crate::error::ModelError;
use crate::header::HeadersFooters;
use crate::id::{IdAllocator, NodeId};
use crate::note::{Note, NoteSettings, Notes};
use crate::object::{Object, Objects};
use crate::op::Op;
use crate::section::{Columns, Layout, Section, SectionStart};
use crate::style::{ParagraphStyle, Style, StyleFamily, StyleId, Styles};
use crate::text::{Paragraph, Position, TextFlow};
use crate::units::Length;

/// Information about the document (DOC-19).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    /// The title.
    pub title: String,
    /// The author.
    pub author: String,
    /// Comments about the document.
    pub comments: String,
    /// Keywords for searching.
    pub keywords: Vec<String>,
}

/// The zoom of a document window (INT-07, INT-08).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewZoom {
    /// A percentage.
    Percent(u16),
    /// The pages fill the width of the window.
    FitWidth,
    /// A whole page shows.
    FitPage,
}

/// How a document shows in its window, which it keeps when it is saved
/// (INT-08, DOC-16). It is not content: two documents that show
/// differently are equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewSettings {
    /// The zoom.
    pub zoom: ViewZoom,
    /// Whether two pages show side by side.
    pub two_pages: bool,
    /// Whether it shows as an outline (TXT-14).
    pub outline: bool,
    /// What the outline shows, which is also what prints while the
    /// document shows as an outline (TXT-14).
    pub outline_options: OutlineOptions,
}

/// What the outline of a document shows (TXT-14).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutlineOptions {
    /// How many levels of topics show, from 1 to 9, without the text
    /// under them; none for every topic and its text.
    pub levels: Option<u8>,
    /// Whether each paragraph shows only its first line.
    pub first_line: bool,
    /// Whether pictures and other objects show in miniature.
    pub thumbnails: bool,
}

/// Settings of the text of the whole document: whether words are
/// hyphenated and ligatures used at all (TXT-09, FMT-09), and the words
/// that are never hyphenated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSettings {
    /// Whether the paragraphs that allow it are hyphenated. When off, no
    /// word is, whatever its paragraph says.
    pub hyphenation: bool,
    /// Whether text uses ligatures. When off, none is formed, whatever the
    /// text asks for.
    pub ligatures: bool,
    /// The words that are never hyphenated, anywhere in the document
    /// ("Never Hyphenate"), in lower case and in order.
    pub never_hyphenate: std::collections::BTreeSet<String>,
    /// How the space after a paragraph and the space before the next one
    /// make the space between them.
    pub spacing: ParagraphSpacing,
    /// Where the notes go and how they are numbered (EST-13).
    pub notes: NoteSettings,
    /// What tables of contents list (EST-22).
    pub toc: crate::toc::TocSettings,
    /// Whether links do nothing when clicked, so that their text is edited
    /// as any other (EST-25).
    pub links_inactive: bool,
    /// The guides fixed on the pages, which objects snap to (OBJ-28).
    pub guides: Vec<FixedGuide>,
}

/// A guide fixed on every page, dragged out of a ruler, that objects snap
/// to (OBJ-28).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FixedGuide {
    /// Whether it runs down the page, at a distance from its left edge,
    /// rather than across it, at a distance from its top.
    pub vertical: bool,
    /// How far it is from the left or the top edge of the paper.
    pub at: Length,
}

/// How the space after a paragraph and the space before the next make the
/// space between them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParagraphSpacing {
    /// The larger of the two, as in Pages (FMT-15).
    #[default]
    Larger,
    /// The sum of the two, as in LibreOffice and Word.
    Sum,
}

impl Default for TextSettings {
    fn default() -> Self {
        Self {
            hyphenation: true,
            ligatures: true,
            never_hyphenate: std::collections::BTreeSet::new(),
            spacing: ParagraphSpacing::Larger,
            notes: NoteSettings::default(),
            toc: crate::toc::TocSettings::default(),
            links_inactive: false,
            guides: Vec::new(),
        }
    }
}

impl TextSettings {
    /// Whether a word may be hyphenated, by the list of words never
    /// hyphenated.
    pub fn may_hyphenate(&self, word: &str) -> bool {
        self.never_hyphenate.is_empty() || !self.never_hyphenate.contains(&word.to_lowercase())
    }
}

/// The size and margins of the pages (EST-02, EST-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSetup {
    /// The width of the paper.
    pub width: Length,
    /// The height of the paper.
    pub height: Length,
    /// The top margin.
    pub margin_top: Length,
    /// The bottom margin.
    pub margin_bottom: Length,
    /// The left margin.
    pub margin_left: Length,
    /// The right margin.
    pub margin_right: Length,
    /// Whether pages face each other, as in a book: the left margin is
    /// then the inside one, on the right of left pages (EST-11).
    pub facing_pages: bool,
    /// The first section of the body (EST-16).
    pub section: Section,
    /// The first layout of the body (EST-07, EST-10).
    pub layout: Layout,
    /// Whether pages show their header (EST-01, EST-12).
    pub headers: bool,
    /// Whether pages show their footer (EST-01, EST-12).
    pub footers: bool,
    /// How far the top of the header is from the top of the paper.
    pub header_margin: Length,
    /// How far the bottom of the footer is from the bottom of the paper.
    pub footer_margin: Length,
    /// How many pages a page layout document has (INT-03), or none for a
    /// word processing document. A page layout document has no body text:
    /// its text is in text boxes, and all its objects float on its pages.
    pub layout_pages: Option<u32>,
}

impl PageSetup {
    /// The margin of the blank templates of Pages: one inch on each side.
    pub const DEFAULT_MARGIN: Length = Length::from_points(72);

    /// The default distance of the header and of the footer from the edge
    /// of the paper: half an inch.
    pub const DEFAULT_HEADER_MARGIN: Length = Length::from_points(36);

    /// The distance of the footer from the bottom of the paper in the
    /// Blank template of Pages: 0.6 inch (EST-01).
    pub const BLANK_FOOTER_MARGIN: Length = Length::from_millipoints(43_200);

    /// A page of the given size with the default margins.
    pub const fn with_size(width: Length, height: Length) -> Self {
        Self {
            width,
            height,
            margin_top: Self::DEFAULT_MARGIN,
            margin_bottom: Self::DEFAULT_MARGIN,
            margin_left: Self::DEFAULT_MARGIN,
            margin_right: Self::DEFAULT_MARGIN,
            facing_pages: false,
            section: Section {
                first_number: None,
                first_page_different: false,
                left_right_different: false,
                start: SectionStart::AnyPage,
                own_margins: false,
            },
            layout: Layout {
                columns: Columns::ONE,
                before: Length::ZERO,
                after: Length::ZERO,
                left: Length::ZERO,
                right: Length::ZERO,
                new_page: false,
            },
            headers: true,
            footers: true,
            header_margin: Self::DEFAULT_HEADER_MARGIN,
            footer_margin: Self::DEFAULT_HEADER_MARGIN,
            layout_pages: None,
        }
    }

    /// Whether the pages are those of a page layout document (INT-03).
    pub fn is_page_layout(&self) -> bool {
        self.layout_pages.is_some()
    }

    /// A4 in portrait, 210 by 297 mm rounded to whole points as Pages and
    /// macOS keep it: 595 by 842 pt. Where the last line of a page falls
    /// depends on that rounding (INT-02).
    pub const fn a4() -> Self {
        Self::with_size(Length::from_points(595), Length::from_points(842))
    }

    /// US Letter in portrait, 8.5 by 11 inches.
    pub const fn letter() -> Self {
        Self::with_size(Length::from_points(612), Length::from_points(792))
    }

    /// Whether the page is wider than it is tall.
    pub fn is_landscape(&self) -> bool {
        self.width > self.height
    }

    /// The same page turned to the other orientation. The margins turn
    /// with the paper.
    pub fn rotated(&self) -> Self {
        Self {
            width: self.height,
            height: self.width,
            margin_top: self.margin_left,
            margin_bottom: self.margin_right,
            margin_left: self.margin_bottom,
            margin_right: self.margin_top,
            ..*self
        }
    }

    /// The width of the area inside the margins.
    pub fn body_width(&self) -> Length {
        self.width - self.margin_left - self.margin_right
    }

    /// The height of the area inside the margins.
    pub fn body_height(&self) -> Length {
        self.height - self.margin_top - self.margin_bottom
    }
}

impl Default for PageSetup {
    fn default() -> Self {
        Self::a4()
    }
}

/// The text of a word processing document, flowing from page to page
/// (INT-02).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagedFlow {
    /// The size and margins of the pages.
    pub page: PageSetup,
    /// The body text.
    pub body: TextFlow,
    /// The headers and footers of the first section (EST-12, EST-19).
    pub margins: HeadersFooters,
    /// The headers and footers of the later sections that have their own,
    /// by the paragraph that starts each (EST-16).
    pub section_margins: imbl::OrdMap<NodeId, HeadersFooters>,
    /// The footnotes or endnotes (EST-13).
    pub notes: Notes,
    /// The objects on the pages, from back to front (OBJ-01).
    pub objects: Objects,
}

/// Where the content of a document lives. Each app has its own container
/// and shares everything inside it: text flows, styles and, later,
/// objects, tables and charts.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Container {
    /// Desk Writer: a paged flow of text, with objects on its pages. Page
    /// layout documents come later in this phase.
    Pages(PagedFlow),
    // Desk Tables adds sheets, and Desk Slides adds slides and master
    // slides, as further variants.
}

/// Counts of the text of a document or a selection (DOC-20). Pages and
/// lines need the layout, so they are counted there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statistics {
    /// Paragraphs, empty ones included.
    pub paragraphs: usize,
    /// Words, by the Unicode word boundaries.
    pub words: usize,
    /// Characters as the user sees them (grapheme clusters).
    pub characters: usize,
    /// Characters that are not white space.
    pub characters_without_spaces: usize,
    /// Sections that the text is in (DOC-20, EST-16).
    pub sections: usize,
    /// Graphics: the objects of the document, floating or in the text, a
    /// group counting once (DOC-20).
    pub graphics: usize,
}

impl Statistics {
    fn add_text(&mut self, text: &str) {
        self.paragraphs += 1;
        self.words += text.unicode_words().count();
        for grapheme in text.graphemes(true) {
            self.characters += 1;
            if !grapheme.chars().all(char::is_whitespace) {
                self.characters_without_spaces += 1;
            }
        }
    }
}

/// A document: metadata, styles and the content in the container of its
/// app.
///
/// Cloning a document is cheap, because the content lives in persistent
/// structures that share their unchanged parts. A clone is a snapshot:
/// layout, saving and versions can work on it in another thread while the
/// editing goes on.
///
/// Every change goes through an [`Op`], so that it can be undone.
#[derive(Clone, Debug)]
pub struct Document {
    ids: IdAllocator,
    metadata: Metadata,
    text_settings: TextSettings,
    view: Option<ViewSettings>,
    styles: Styles,
    container: Container,
}

impl PartialEq for Document {
    /// Two documents are equal when their content is. The identifiers
    /// that a document has yet to hand out are not content.
    fn eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata
            && self.text_settings == other.text_settings
            && self.styles == other.styles
            && self.container == other.container
    }
}

impl Eq for Document {}

impl Document {
    /// Makes an empty word processing document, with a default paragraph
    /// style named "Body", one empty paragraph and A4 pages.
    pub fn new_word_processing() -> Self {
        let mut ids = IdAllocator::default();
        let body_style = ids.allocate();
        let first = ids.allocate();
        Self {
            ids,
            metadata: Metadata::default(),
            // As the Blank template of Pages: words are not hyphenated
            // until the document says so (EST-01).
            text_settings: TextSettings {
                hyphenation: false,
                ..TextSettings::default()
            },
            view: None,
            styles: Styles::new(ParagraphStyle::new(body_style, "Body")),
            container: Container::Pages(PagedFlow {
                // The footer of the Blank template is 0.6 inch from the
                // bottom of the paper, the header half an inch from the top.
                page: PageSetup {
                    footer_margin: PageSetup::BLANK_FOOTER_MARGIN,
                    ..PageSetup::default()
                },
                body: TextFlow::new(Paragraph::new(first, body_style)),
                margins: HeadersFooters::default(),
                section_margins: imbl::OrdMap::new(),
                notes: Notes::default(),
                objects: Objects::default(),
            }),
        }
    }

    /// Makes an empty page layout document, of one page with no body text
    /// (INT-03).
    pub fn new_page_layout() -> Self {
        let mut document = Self::new_word_processing();
        let Container::Pages(flow) = &mut document.container;
        flow.page.layout_pages = Some(1);
        document
    }

    /// Returns a new identifier, never used before in this document.
    pub fn new_id(&mut self) -> NodeId {
        self.ids.allocate()
    }

    /// Never hands out an identifier that a copy of the document did.
    pub(crate) fn follow_ids(&mut self, copy: &Document) {
        self.ids.follow(&copy.ids);
    }

    /// The metadata.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// How the document was shown when it was saved, if it says.
    pub fn view(&self) -> Option<ViewSettings> {
        self.view
    }

    /// Sets how the document shows, to save with it. It is not content,
    /// and has no place in the history.
    pub fn set_view(&mut self, view: Option<ViewSettings>) {
        self.view = view;
    }

    /// The settings of the text of the whole document.
    pub fn text_settings(&self) -> &TextSettings {
        &self.text_settings
    }

    /// The styles.
    pub fn styles(&self) -> &Styles {
        &self.styles
    }

    /// The container.
    pub fn container(&self) -> &Container {
        &self.container
    }

    /// The body text.
    pub fn body(&self) -> &TextFlow {
        match &self.container {
            Container::Pages(flow) => &flow.body,
        }
    }

    pub(crate) fn body_mut(&mut self) -> &mut TextFlow {
        match &mut self.container {
            Container::Pages(flow) => &mut flow.body,
        }
    }

    /// The page setup.
    pub fn page_setup(&self) -> &PageSetup {
        match &self.container {
            Container::Pages(flow) => &flow.page,
        }
    }

    /// The headers and footers of the first section.
    pub fn headers_footers(&self) -> &HeadersFooters {
        match &self.container {
            Container::Pages(flow) => &flow.margins,
        }
    }

    /// The headers and footers of a section, by the paragraph that starts
    /// it, or of the first section with none. A section that has none yet
    /// has empty ones.
    pub fn section_headers_footers(&self, section: Option<NodeId>) -> &HeadersFooters {
        static EMPTY: std::sync::LazyLock<HeadersFooters> =
            std::sync::LazyLock::new(HeadersFooters::default);
        match (&self.container, section) {
            (Container::Pages(flow), None) => &flow.margins,
            (Container::Pages(flow), Some(id)) => flow.section_margins.get(&id).unwrap_or(&EMPTY),
        }
    }

    /// Every section that has headers and footers of its own, the first
    /// one as none, with them.
    pub fn all_headers_footers(&self) -> Vec<(Option<NodeId>, &HeadersFooters)> {
        let Container::Pages(flow) = &self.container;
        std::iter::once((None, &flow.margins))
            .chain(flow.section_margins.iter().map(|(id, m)| (Some(*id), m)))
            .collect()
    }

    /// The section whose headers and footers the paragraph at an index of
    /// the body shows: the paragraph that starts the nearest section at or
    /// before it with its own, or none for those of the first section
    /// (EST-12, EST-16).
    pub fn headers_section(&self, index: usize) -> Option<NodeId> {
        let body = self.body();
        for i in (0..=index.min(body.len().saturating_sub(1))).rev() {
            let paragraph = body.get(i)?;
            if let Some(Some(start)) = &paragraph.attrs().start
                && let crate::Start::Section(section, _) = &**start
                && section.own_margins
            {
                return Some(paragraph.id());
            }
        }
        None
    }

    /// Whether the body holds a mark of a note.
    pub fn has_note_mark(&self, note: NodeId) -> bool {
        self.body().iter().any(|p| {
            p.runs().any(|r| {
                matches!(
                    r.attrs.field,
                    Some(Some(crate::Field::NoteMark { note: n })) if n == note
                )
            })
        })
    }

    /// The notes.
    pub fn notes(&self) -> &Notes {
        match &self.container {
            Container::Pages(flow) => &flow.notes,
        }
    }

    /// The objects on the pages, from back to front.
    pub fn objects(&self) -> &Objects {
        match &self.container {
            Container::Pages(flow) => &flow.objects,
        }
    }

    /// Checks the styles of an object's text and that none of its
    /// identifiers is in use elsewhere than in `replacing`.
    fn check_object(&self, object: &Object, replacing: Option<&Object>) -> Result<(), ModelError> {
        for paragraph in object.paragraphs() {
            self.styles
                .check_family(paragraph.style(), StyleFamily::Paragraph)?;
            self.check_list_style(paragraph.attrs())?;
            self.check_runs_styles(paragraph.runs())?;
        }
        let ids = object.ids();
        let mut seen = std::collections::HashSet::new();
        let kept: Vec<NodeId> = replacing.map(Object::ids).unwrap_or_default();
        for id in ids {
            let elsewhere = !kept.contains(&id)
                && (self.objects().iter().any(|o| o.ids().contains(&id))
                    || self.body().paragraph(id).is_some());
            if !seen.insert(id) || elsewhere {
                return Err(ModelError::DuplicateId(id));
            }
        }
        Ok(())
    }

    /// A paragraph by identifier.
    pub fn paragraph(&self, id: NodeId) -> Result<&Paragraph, ModelError> {
        self.body()
            .paragraph(id)
            .ok_or(ModelError::UnknownParagraph(id))
    }

    /// The position at the start of the document.
    pub fn start(&self) -> Position {
        let first = self.body().get(0).expect("a flow has a paragraph");
        Position::new(first.id(), 0)
    }

    /// The position at the end of the document.
    pub fn end(&self) -> Position {
        let body = self.body();
        let last = body.get(body.len() - 1).expect("a flow has a paragraph");
        Position::new(last.id(), last.len())
    }

    /// Counts the whole document: the text of its body, and that of its
    /// text boxes and shapes, as Pages counts it (DOC-20).
    pub fn statistics(&self) -> Statistics {
        let mut statistics = Statistics::default();
        for paragraph in self.body().iter() {
            statistics.add_text(&paragraph.text());
        }
        let continued: Vec<NodeId> = self
            .objects()
            .iter()
            .filter_map(|o| o.text_box()?.next)
            .collect();
        for object in self.objects().iter() {
            statistics.graphics += 1;
            // The boxes after the first of linked ones hold no text.
            if continued.contains(&object.id) {
                continue;
            }
            for paragraph in object.paragraphs() {
                statistics.add_text(&paragraph.text());
            }
        }
        statistics.sections = self.sections_between(0, self.body().len().saturating_sub(1));
        statistics
    }

    /// Counts the text between two positions, such as a selection.
    pub fn statistics_between(
        &self,
        start: Position,
        end: Position,
    ) -> Result<Statistics, ModelError> {
        let mut statistics = Statistics::default();
        for (id, range) in self.paragraph_ranges(start, end)? {
            let text: String = self
                .paragraph(id)?
                .slice(range)?
                .iter()
                .map(|run| run.text())
                .collect();
            statistics.add_text(&text);
        }
        let body = self.body();
        let index = |id| body.index_of(id).ok_or(ModelError::UnknownParagraph(id));
        statistics.sections = self.sections_between(index(start.paragraph)?, index(end.paragraph)?);
        Ok(statistics)
    }

    /// Splits the text between two positions into a range in each
    /// paragraph, in order.
    pub fn paragraph_ranges(
        &self,
        start: Position,
        end: Position,
    ) -> Result<Vec<(NodeId, std::ops::Range<usize>)>, ModelError> {
        let body = self.body();
        let first = body
            .index_of(start.paragraph)
            .ok_or(ModelError::UnknownParagraph(start.paragraph))?;
        let last = body
            .index_of(end.paragraph)
            .ok_or(ModelError::UnknownParagraph(end.paragraph))?;
        if first > last || (first == last && start.offset > end.offset) {
            return Err(ModelError::BackwardRange);
        }
        let mut ranges = Vec::with_capacity(last - first + 1);
        for index in first..=last {
            let paragraph = body.get(index).expect("index is in the flow");
            let a = if index == first { start.offset } else { 0 };
            let b = if index == last {
                end.offset
            } else {
                paragraph.len()
            };
            if !paragraph.is_boundary(a) || !paragraph.is_boundary(b) {
                return Err(ModelError::InvalidRange {
                    paragraph: paragraph.id(),
                    range: a..b,
                });
            }
            ranges.push((paragraph.id(), a..b));
        }
        Ok(ranges)
    }

    fn check_list_style(&self, attrs: &crate::format::ParaAttrs) -> Result<(), ModelError> {
        match attrs.list_style {
            Some(Some(list)) => self.styles.check_family(list, StyleFamily::List),
            _ => Ok(()),
        }
    }

    fn check_runs_styles<'a>(
        &self,
        runs: impl IntoIterator<Item = &'a crate::text::Run>,
    ) -> Result<(), ModelError> {
        for run in runs {
            if let Some(style) = run.style {
                self.styles.check_family(style, StyleFamily::Character)?;
            }
        }
        Ok(())
    }

    /// Applies one operation and returns its inverse: the operation that
    /// undoes it. On error, the document is unchanged.
    pub fn apply(&mut self, op: Op) -> Result<Op, ModelError> {
        match op {
            Op::ReplaceText {
                paragraph,
                range,
                runs,
            } => {
                self.check_runs_styles(&runs)?;
                let inserted = runs.iter().map(|run| run.len()).sum::<usize>();
                let start = range.start;
                let removed = self
                    .body_mut()
                    .paragraph_mut(paragraph)?
                    .replace(range, runs)?;
                Ok(Op::ReplaceText {
                    paragraph,
                    range: start..start + inserted,
                    runs: removed,
                })
            }
            Op::InsertParagraph { index, paragraph } => {
                self.styles
                    .check_family(paragraph.style(), StyleFamily::Paragraph)?;
                self.check_list_style(paragraph.attrs())?;
                self.check_runs_styles(paragraph.runs())?;
                let id = paragraph.id();
                self.body_mut().insert(index, paragraph)?;
                self.ids.observe(id);
                Ok(Op::RemoveParagraph { paragraph: id })
            }
            Op::RemoveParagraph { paragraph } => {
                let (index, paragraph) = self.body_mut().remove(paragraph)?;
                Ok(Op::InsertParagraph { index, paragraph })
            }
            Op::SetParagraphFormat {
                paragraph,
                style,
                attrs,
            } => {
                self.styles.check_family(style, StyleFamily::Paragraph)?;
                self.check_list_style(&attrs)?;
                let (style, attrs) = self
                    .body_mut()
                    .paragraph_mut(paragraph)?
                    .set_format(style, attrs);
                Ok(Op::SetParagraphFormat {
                    paragraph,
                    style,
                    attrs,
                })
            }
            Op::PutStyle { style } => {
                self.styles.check_put(&style)?;
                let id = style.id();
                self.ids.observe(id);
                Ok(match self.styles.insert(style) {
                    Some(old) => Op::PutStyle { style: old },
                    None => Op::RemoveStyle { style: id },
                })
            }
            Op::RemoveStyle { style } => {
                if style == self.styles.default_paragraph() {
                    return Err(ModelError::DefaultStyle);
                }
                if self.styles.get(style).is_none() {
                    return Err(ModelError::UnknownStyle(style));
                }
                if self.styles.is_referenced(style)
                    || self.body().iter().any(|p| p.uses_style(style))
                    || self
                        .all_headers_footers()
                        .iter()
                        .any(|(_, m)| m.paragraphs().any(|p| p.uses_style(style)))
                    || self.notes().paragraphs().any(|p| p.uses_style(style))
                    || self.objects().paragraphs().any(|p| p.uses_style(style))
                    || self.text_settings().toc.uses(style)
                {
                    return Err(ModelError::StyleInUse(style));
                }
                let old = self.styles.remove(style).expect("the style exists");
                Ok(Op::PutStyle { style: old })
            }
            Op::SetPageSetup { page } => {
                let Container::Pages(flow) = &mut self.container;
                let old = std::mem::replace(&mut flow.page, page);
                Ok(Op::SetPageSetup { page: old })
            }
            Op::SetHeaderFooter {
                section,
                place,
                paragraphs,
            } => {
                for paragraph in &paragraphs {
                    self.styles
                        .check_family(paragraph.style(), StyleFamily::Paragraph)?;
                    self.check_list_style(paragraph.attrs())?;
                    self.check_runs_styles(paragraph.runs())?;
                }
                for paragraph in &paragraphs {
                    self.ids.observe(paragraph.id());
                }
                let Container::Pages(flow) = &mut self.container;
                let old = match section {
                    None => flow.margins.replace(place, paragraphs),
                    Some(id) => {
                        let mut margins =
                            flow.section_margins.get(&id).cloned().unwrap_or_default();
                        let old = margins.replace(place, paragraphs);
                        if margins.is_empty() {
                            flow.section_margins.remove(&id);
                        } else {
                            flow.section_margins.insert(id, margins);
                        }
                        old
                    }
                };
                Ok(Op::SetHeaderFooter {
                    section,
                    place,
                    paragraphs: old,
                })
            }
            Op::SetNote { id, note } => {
                if let Some(note) = &note {
                    if note.id != id {
                        return Err(ModelError::DuplicateId(id));
                    }
                    for paragraph in note.paragraphs.iter() {
                        self.styles
                            .check_family(paragraph.style(), StyleFamily::Paragraph)?;
                        self.check_list_style(paragraph.attrs())?;
                        self.check_runs_styles(paragraph.runs())?;
                    }
                    self.ids.observe(id);
                    for paragraph in note.paragraphs.iter() {
                        self.ids.observe(paragraph.id());
                    }
                }
                let Container::Pages(flow) = &mut self.container;
                let old: Option<Note> = flow.notes.replace(id, note);
                Ok(Op::SetNote { id, note: old })
            }
            Op::InsertObject { index, object } => {
                let len = self.objects().len();
                if index > len {
                    return Err(ModelError::IndexOutOfBounds { index, len });
                }
                self.check_object(&object, None)?;
                for id in object.ids() {
                    self.ids.observe(id);
                }
                let id = object.id;
                let Container::Pages(flow) = &mut self.container;
                flow.objects.insert(index, object);
                Ok(Op::RemoveObject { object: id })
            }
            Op::RemoveObject { object } => {
                let index = self
                    .objects()
                    .index_of(object)
                    .ok_or(ModelError::UnknownObject(object))?;
                let Container::Pages(flow) = &mut self.container;
                let object = flow.objects.remove(index);
                Ok(Op::InsertObject { index, object })
            }
            Op::ReplaceObject { object } => {
                let index = self
                    .objects()
                    .index_of(object.id)
                    .ok_or(ModelError::UnknownObject(object.id))?;
                let old = self.objects().iter().nth(index).cloned();
                self.check_object(&object, old.as_ref())?;
                for id in object.ids() {
                    self.ids.observe(id);
                }
                let Container::Pages(flow) = &mut self.container;
                let old = flow.objects.set(index, object);
                Ok(Op::ReplaceObject { object: old })
            }
            Op::SetMetadata { metadata } => {
                let old = std::mem::replace(&mut self.metadata, metadata);
                Ok(Op::SetMetadata { metadata: old })
            }
            Op::SetTextSettings { settings } => {
                let old = std::mem::replace(&mut self.text_settings, settings);
                Ok(Op::SetTextSettings { settings: old })
            }
        }
    }

    /// The paragraph style of a new paragraph that Return starts at the
    /// end of a paragraph with this style.
    pub(crate) fn next_style(&self, style: StyleId) -> StyleId {
        self.styles
            .paragraph(style)
            .and_then(|s| s.next)
            .unwrap_or(style)
    }

    /// Adds a style or replaces the style with the same identifier,
    /// without undo. For building documents, as readers of files do.
    pub fn put_style(&mut self, style: impl Into<Style>) -> Result<(), ModelError> {
        self.apply(Op::PutStyle {
            style: style.into(),
        })
        .map(drop)
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new_word_processing()
    }
}
