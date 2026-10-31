// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The document, its container and its metadata.

use unicode_segmentation::UnicodeSegmentation;

use crate::error::ModelError;
use crate::id::{IdAllocator, NodeId};
use crate::op::Op;
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
}

impl PageSetup {
    /// The margin of the blank templates of Pages: one inch on each side.
    pub const DEFAULT_MARGIN: Length = Length::from_points(72);

    /// A page of the given size with the default margins.
    pub const fn with_size(width: Length, height: Length) -> Self {
        Self {
            width,
            height,
            margin_top: Self::DEFAULT_MARGIN,
            margin_bottom: Self::DEFAULT_MARGIN,
            margin_left: Self::DEFAULT_MARGIN,
            margin_right: Self::DEFAULT_MARGIN,
        }
    }

    /// A4 in portrait, 210 by 297 mm.
    pub const fn a4() -> Self {
        Self::with_size(
            Length::from_millipoints(595_276),
            Length::from_millipoints(841_890),
        )
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
}

/// Where the content of a document lives. Each app has its own container
/// and shares everything inside it: text flows, styles and, later,
/// objects, tables and charts.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Container {
    /// Desk Writer: a paged flow of text. Sections, page layout documents
    /// and page objects come in later phases.
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
    styles: Styles,
    container: Container,
}

impl PartialEq for Document {
    /// Two documents are equal when their content is. The identifiers
    /// that a document has yet to hand out are not content.
    fn eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata
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
            styles: Styles::new(ParagraphStyle::new(body_style, "Body")),
            container: Container::Pages(PagedFlow {
                page: PageSetup::default(),
                body: TextFlow::new(Paragraph::new(first, body_style)),
            }),
        }
    }

    /// Returns a new identifier, never used before in this document.
    pub fn new_id(&mut self) -> NodeId {
        self.ids.allocate()
    }

    /// The metadata.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
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

    fn body_mut(&mut self) -> &mut TextFlow {
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

    /// Counts the whole document.
    pub fn statistics(&self) -> Statistics {
        let mut statistics = Statistics::default();
        for paragraph in self.body().iter() {
            statistics.add_text(&paragraph.text());
        }
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
            Op::SetMetadata { metadata } => {
                let old = std::mem::replace(&mut self.metadata, metadata);
                Ok(Op::SetMetadata { metadata: old })
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
