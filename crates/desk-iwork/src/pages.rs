// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! A Pages '09 document: its body text, styles, page and
//! information.
//!
//! The body is the text storage of kind `body` that is not part of a
//! section prototype of the template. Paragraphs are `sf:p` elements,
//! nested in sections and layouts, and spans of text inside them carry a
//! character style. Layout hints that Pages saves to open faster, such as
//! `sf:container-hint`, are skipped.

use crate::error::IworkError;
use crate::ns::{KEY, LS, SF, SFA, SL};
use crate::sf::{Ids, Stylesheet};
use crate::xml::{self, Element, Node};

/// A line break inside a paragraph (`sf:lnbr`), as the model writes it.
pub const LINE_BREAK: char = '\u{2028}';
/// A page break (`sf:pgbr`).
pub const PAGE_BREAK: char = '\u{c}';
/// A column break (`sf:contbr`).
pub const COLUMN_BREAK: char = '\u{b}';

/// The information of the document (Document Inspector).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    /// The title.
    pub title: String,
    /// The authors.
    pub authors: Vec<String>,
    /// The keywords.
    pub keywords: Vec<String>,
    /// The comment.
    pub comment: String,
    /// The copyright notice.
    pub copyright: String,
}

/// The paper and margins, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeometry {
    /// The width of the paper, already turned for landscape.
    pub width: f64,
    /// The height of the paper.
    pub height: f64,
    /// The top margin.
    pub top: f64,
    /// The bottom margin.
    pub bottom: f64,
    /// The left margin.
    pub left: f64,
    /// The right margin.
    pub right: f64,
    /// Whether the pages face each other, as in a book: the left and
    /// right margins are then the inside and outside ones, swapped on left
    /// pages.
    pub facing: bool,
    /// The distance from the top of the paper to the header.
    pub header: f64,
    /// The distance from the bottom of the paper to the footer.
    pub footer: f64,
}

impl Default for PageGeometry {
    /// US Letter with one inch margins, as the blank template.
    fn default() -> Self {
        Self {
            width: 612.0,
            height: 792.0,
            top: 72.0,
            bottom: 72.0,
            left: 72.0,
            right: 72.0,
            facing: false,
            header: 36.0,
            footer: 36.0,
        }
    }
}

/// What a field of the text shows (EST-27).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    /// The number of its page (`sf:page-number`).
    PageNumber {
        /// How the number is written (`sf:format`), as `lower-roman`;
        /// none for 1, 2, 3.
        format: Option<String>,
    },
    /// The number of pages (`sf:page-count`).
    PageCount {
        /// How the number is written, as for the page number.
        format: Option<String>,
    },
    /// A date and time (`sf:date-time`).
    DateTime {
        /// The pattern of the date, in the symbols of Unicode (`sf:format`).
        format: String,
        /// The text that Pages shows.
        shown: String,
        /// Whether it changes to the date of opening ("Automatically
        /// update on open", `sf:auto-update`).
        update: bool,
    },
    /// The name of the file (`sf:filename`).
    FileName {
        /// The text that Pages shows.
        shown: String,
        /// What it shows (`sf:display-format`), when the document says:
        /// 1 adds the folder, 2 the extension.
        display: Option<u8>,
    },
    /// Where the body refers to a footnote or an endnote (`sf:footnote`):
    /// the nth refers to the nth of [`PagesDocument::notes`].
    Note {
        /// The mark of its own that the note shows instead of a number,
        /// when it has one (`sf:mark`).
        mark: Option<String>,
    },
    /// The mark at the start of the text of a note (`sf:footnote-mark`).
    NoteMark,
    /// The number of the page of a paragraph, by the name of its mark
    /// (`sf:toc-pagenum`, in an entry of a table of contents).
    PageOf {
        /// `sf:bookmark`: a name of [`Paragraph::marks`].
        bookmark: String,
    },
}

/// How the notes of a document show, from the Document Inspector
/// (EST-13, EST-15).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoteProperties {
    /// Footnotes (0), endnotes at the end of the document (1) or of each
    /// section (2) (`kSFWPFootnoteKindProperty`).
    pub kind: u8,
    /// Arabic numbers (0), Roman numbers (1) or symbols (2)
    /// (`kSFWPFootnoteFormatProperty`).
    pub format: u8,
    /// Numbered through the document (0), again on each page (1) or in
    /// each section (2) (`kSFWPFootnoteNumberingProperty`).
    pub numbering: u8,
    /// The space between notes, in points, when the document says
    /// (`kSFWPFootnoteGapProperty`).
    pub gap: Option<f64>,
}

/// The character that a field takes in the text of its run.
pub const FIELD: char = '\u{fffc}';

/// A run of text with one character style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    /// The character style, by `sf:style`, for text inside a span.
    pub style: Option<String>,
    /// The field that the run is, when it is one: its text is then one
    /// [`FIELD`].
    pub field: Option<Field>,
    /// `href` of the `sf:link` that the run is in (EST-25).
    pub link: Option<String>,
    /// `sf:name` of the `sf:bookmark` that the run is in (EST-26).
    pub bookmark: Option<String>,
    /// The text, with breaks as [`LINE_BREAK`], [`PAGE_BREAK`],
    /// [`COLUMN_BREAK`] and tabs as `\t`. A column break that ends a
    /// paragraph is not kept: the next paragraph starts a new column.
    pub text: String,
}

/// A paragraph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paragraph {
    /// The paragraph style, by `sf:style`.
    pub style: Option<String>,
    /// The level in its list, from 0 (`sf:list-level`).
    pub list_level: u8,
    /// Whether its list starts counting again here (`sf:restart-list`).
    pub restart_list: bool,
    /// Whether it starts a new column, because the paragraph before ended
    /// with a column break (`sf:contbr`).
    pub column_break_before: bool,
    /// The style of the section that starts at this paragraph, by
    /// `sf:style`: the first paragraph of each `sf:section`.
    pub section: Option<String>,
    /// The style of the layout that starts at this paragraph, by
    /// `sf:style`: the first paragraph of each `sf:layout`.
    pub layout: Option<String>,
    /// The names of the hidden marks (`sf:toc-bookmark`) in the paragraph,
    /// by which entries of tables of contents find it.
    pub marks: Vec<String>,
    /// Whether the paragraph is an entry of a table of contents (`sf:toc`).
    /// Neighbors are one table.
    pub toc: bool,
    /// The text, in runs.
    pub runs: Vec<Run>,
}

/// A Pages document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PagesDocument {
    /// The information.
    pub metadata: Metadata,
    /// The paper and margins.
    pub page: PageGeometry,
    /// The styles of the body.
    pub stylesheet: Stylesheet,
    /// The paragraphs of the body.
    pub body: Vec<Paragraph>,
    /// The headers, by the name Pages gives them, as
    /// `SFWPDefaultOddHeaderIdentifier`, with their paragraphs.
    pub headers: Vec<(String, Vec<Paragraph>)>,
    /// The footers, as the headers.
    pub footers: Vec<(String, Vec<Paragraph>)>,
    /// The footnotes or endnotes, in the order the body refers to them,
    /// each with its paragraphs.
    pub notes: Vec<Vec<Paragraph>>,
    /// How the notes show.
    pub note_properties: NoteProperties,
    /// Whether pages show their headers (Document Inspector).
    pub has_headers: bool,
    /// Whether pages show their footers (Document Inspector).
    pub has_footers: bool,
    /// Whether the page numbers of tables of contents are links ("Make
    /// page numbers links", TOC Inspector), true when the document does
    /// not say.
    pub toc_links: bool,
    /// The kinds of content that the reader skipped, by element name, in
    /// the order first met, with how many of each: what an import cannot
    /// show yet. Besides the elements of the body, `headers`, `footers`
    /// and `drawables` count the headers, footers and floating objects of
    /// the document, which are outside the body.
    pub skipped: Vec<(String, usize)>,
    /// Whether Pages hyphenates words at the ends of lines by itself
    /// (Document Inspector), in the paragraphs that allow it.
    pub auto_hyphenation: bool,
    /// Whether the text uses ligatures (Document Inspector, "Use
    /// ligatures"), true when the document does not say.
    pub use_ligatures: bool,
    /// The files of the package that the document uses, by path, with
    /// their bytes: the pictures of image bullets. Only reading a package
    /// fills them, not parsing an index.
    pub files: Vec<(String, Vec<u8>)>,
}

impl PagesDocument {
    /// The paths of the files of the package that the document uses.
    pub fn file_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .stylesheet
            .list_styles
            .iter()
            .flat_map(|s| s.labels.iter().flatten())
            .filter_map(|l| match l {
                crate::sf::ListLabel::Image(path) => path.clone(),
                _ => None,
            })
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// The bytes of a file of the package, as [`PagesDocument::files`]
    /// holds them.
    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, b)| b.as_slice())
    }
}

/// A flag of the publication info of a document, as the settings of the
/// Document Inspector are kept.
fn publication_flag(root: &Element, name: &str) -> Option<bool> {
    root.child(SL, "publication-info")
        .and_then(|info| info.child(SL, name))
        .and_then(|p| p.child(SL, "number"))
        .and_then(|n| n.attr(SFA, "number"))
        .map(|n| n.trim() != "0")
}

/// A number of the publication info of a document.
fn publication_number(root: &Element, name: &str) -> Option<f64> {
    root.child(SL, "publication-info")
        .and_then(|info| info.child(SL, name))
        .and_then(|p| p.child(SL, "number"))
        .and_then(|n| n.attr(SFA, "number"))
        .and_then(|n| n.trim().parse().ok())
}

/// Reads a Pages document from the text of its index.
pub fn parse_pages(index: &str) -> Result<PagesDocument, IworkError> {
    let root = xml::parse(index)?;
    if !root.name.is(SL, "document") {
        let kind = if root.name.ns == LS {
            "a Numbers spreadsheet".to_owned()
        } else if root.name.ns == KEY {
            "a Keynote presentation".to_owned()
        } else {
            format!("the element {}", root.name.local)
        };
        return Err(IworkError::WrongType(kind));
    }
    let ids = Ids::new(&root);
    let mut skipped = Vec::new();
    let count = root
        .children_named(SL, "drawables")
        .map(floating_objects)
        .sum::<usize>();
    if count > 0 {
        skipped.push(("drawables".to_owned(), count));
    }
    let mut document = PagesDocument {
        skipped,
        metadata: root
            .child(SF, "metadata")
            .map(|m| metadata(&ids, m))
            .unwrap_or_default(),
        page: root
            .child(SL, "slprint-info")
            .map(page_geometry)
            .unwrap_or_default(),
        auto_hyphenation: publication_flag(&root, "kSFWPAutoHyphenationProperty").unwrap_or(false),
        use_ligatures: publication_flag(&root, "kSFWPUseLigaturesProperty").unwrap_or(true),
        note_properties: NoteProperties {
            kind: publication_number(&root, "kSFWPFootnoteKindProperty").unwrap_or(0.0) as u8,
            format: publication_number(&root, "kSFWPFootnoteFormatProperty").unwrap_or(0.0) as u8,
            numbering: publication_number(&root, "kSFWPFootnoteNumberingProperty").unwrap_or(0.0)
                as u8,
            gap: publication_number(&root, "kSFWPFootnoteGapProperty"),
        },
        has_headers: publication_flag(&root, "kSFWPHasHeadersProperty").unwrap_or(true),
        has_footers: publication_flag(&root, "kSFWPHasFootersProperty").unwrap_or(true),
        toc_links: publication_flag(&root, "kSFWPTOCLinksEnabledProperty").unwrap_or(true),
        ..PagesDocument::default()
    };
    let Some(storage) = root
        .children_named(SF, "text-storage")
        .find(|s| s.attr(SF, "kind") == Some("body"))
    else {
        return Ok(document);
    };
    let sheet = storage
        .child(SF, "stylesheet-ref")
        .and_then(|r| r.attr(SFA, "IDREF"))
        .and_then(|id| ids.get(id))
        .or_else(|| root.child(SL, "stylesheet"));
    if let Some(sheet) = sheet {
        document.stylesheet = Stylesheet::read(&ids, sheet);
    }
    if let Some(body) = storage.child(SF, "text-body") {
        read_block(&ids, &mut document, body);
    }
    // The notes, in a text storage of their own inside that of the body.
    // Each ends with a note break; what follows the last is an empty
    // paragraph that Pages keeps there.
    let notes = storage
        .child(SF, "footnotes")
        .and_then(|n| n.child(SF, "text-storage"))
        .and_then(|s| s.child(SF, "text-body"));
    if let Some(notes) = notes {
        let body = std::mem::take(&mut document.body);
        let ends = read_block(&ids, &mut document, notes);
        let mut paragraphs = std::mem::replace(&mut document.body, body).into_iter();
        let mut start = 0;
        for end in ends {
            document
                .notes
                .push(paragraphs.by_ref().take(end - start).collect());
            start = end;
        }
    }
    // The headers and footers, each in a text storage of its own, read
    // as the body is.
    for (name, item) in [("headers", "header"), ("footers", "footer")] {
        for part in root.children_named(SF, name) {
            for element in part.children_named(SF, item) {
                let text = element
                    .child(SF, "text-storage")
                    .and_then(|s| s.child(SF, "text-body"));
                let Some(text) = text else {
                    continue;
                };
                let body = std::mem::take(&mut document.body);
                read_block(&ids, &mut document, text);
                let paragraphs = std::mem::replace(&mut document.body, body);
                let name_attr = element.attr(SF, "name").unwrap_or_default().to_owned();
                let list = if item == "header" {
                    &mut document.headers
                } else {
                    &mut document.footers
                };
                list.push((name_attr, paragraphs));
            }
        }
    }
    Ok(document)
}

/// Reads the paragraphs of a text body into the body of a document, and
/// returns how many paragraphs there are after each note break.
fn read_block(ids: &Ids<'_>, document: &mut PagesDocument, body: &Element) -> Vec<usize> {
    let start = document.body.len();
    let mut reader = BodyReader {
        ids,
        document,
        section: None,
        layout: None,
        column_break: false,
        in_toc: false,
        link: None,
        bookmark: None,
        marks: Vec::new(),
        note_break: false,
        note_ends: Vec::new(),
    };
    reader.block(body);
    reader.note_ends.into_iter().map(|e| e - start).collect()
}

/// The strings of a metadata field, which holds a string or an array.
fn strings(ids: &Ids<'_>, field: &Element) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack: Vec<&Element> = field.elements().collect();
    stack.reverse();
    while let Some(element) = stack.pop() {
        let element = ids.deref(element);
        if element.name.is(SF, "string") {
            if let Some(s) = element.attr(SFA, "string").filter(|s| !s.is_empty()) {
                out.push(s.to_owned());
            }
        } else {
            let mut children: Vec<&Element> = element.elements().collect();
            children.reverse();
            stack.extend(children);
        }
    }
    out
}

fn metadata(ids: &Ids<'_>, element: &Element) -> Metadata {
    let field = |name| {
        element
            .child(SF, name)
            .map(|f| strings(ids, f))
            .unwrap_or_default()
    };
    Metadata {
        title: field("title").join(" "),
        authors: field("authors"),
        keywords: field("keywords"),
        comment: field("comment").join("\n"),
        copyright: field("copyright").join(" "),
    }
}

fn page_geometry(info: &Element) -> PageGeometry {
    let mut page = PageGeometry::default();
    let number = |element: &Element, ns, name| {
        element
            .attr(ns, name)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
    };
    if let Some(width) = number(info, SL, "page-width") {
        page.width = width;
    }
    if let Some(height) = number(info, SL, "page-height") {
        page.height = height;
    }
    if let Some(margins) = info.child(SF, "page-margins") {
        for (field, name) in [
            (&mut page.top, "top"),
            (&mut page.bottom, "bottom"),
            (&mut page.left, "left"),
            (&mut page.right, "right"),
        ] {
            if let Some(value) = number(margins, SF, name) {
                *field = value;
            }
        }
        for (field, name) in [(&mut page.header, "header"), (&mut page.footer, "footer")] {
            if let Some(value) = number(margins, SF, name) {
                *field = value;
            }
        }
        page.facing = margins.attr(SF, "facing-pages") == Some("true");
    }
    page
}

/// Elements under `sl:drawables` that only group the floating objects: of
/// the pages, and of the masters of each section.
const DRAWABLE_GROUPS: &[&str] = &[
    "masters-group",
    "section-drawables",
    "page-group",
    "foregroundmaster-group",
    "backgroundmaster-group",
];

/// How many floating objects the drawables of a document hold.
fn floating_objects(drawables: &Element) -> usize {
    let mut count = 0;
    let mut stack = vec![drawables];
    while let Some(group) = stack.pop() {
        for child in group.elements() {
            if DRAWABLE_GROUPS.contains(&child.name.local.as_str()) {
                stack.push(child);
            } else if group.name.local != "section-drawables" {
                // The section drawables also hold their style.
                count += 1;
            }
        }
    }
    count
}

/// Elements that hold nothing to show, or that Pages saves only to lay
/// out faster.
const IGNORED: &[&str] = &[
    "container-hint",
    "page-start",
    "insertion-point",
    "selection-start",
    "selection-end",
    "stylesheet-ref",
    "crbr",
    "br",
    // The hidden marks of the headings that a table of contents lists.
    "toc-bookmark",
    // The alias of the file that a link to a document opens, which only
    // the Finder reads: the link has its path too.
    "file-alias",
    // The ends of a layout and of a section, which the elements around
    // the paragraphs record.
    "layoutbr",
    "sectbr",
];

/// Elements around text that the reader keeps, by recording the element
/// and reading what it holds: tracked changes, whose text Pages shows with
/// its markup, and bookmarks.
const WRAPPERS: &[&str] = &["change-ref", "changed-ref", "changes-ref"];

struct BodyReader<'a, 'b> {
    ids: &'a Ids<'a>,
    document: &'b mut PagesDocument,
    /// The section and layout that start at the next paragraph.
    section: Option<String>,
    layout: Option<String>,
    /// Whether the next paragraph starts a new column.
    column_break: bool,
    /// Whether the paragraphs read are entries of a table of contents.
    in_toc: bool,
    /// The link and the bookmark that the text read now is in, and the
    /// marks of the paragraph read.
    link: Option<String>,
    bookmark: Option<String>,
    marks: Vec<String>,
    /// Whether the paragraph being read ends a note.
    note_break: bool,
    /// The number of paragraphs read when each note ended.
    note_ends: Vec<usize>,
}

impl BodyReader<'_, '_> {
    fn skip(&mut self, element: &Element) {
        self.skip_named(&element.name.local.clone());
    }

    fn skip_named(&mut self, name: &str) {
        match self.document.skipped.iter_mut().find(|(n, _)| n == name) {
            Some((_, count)) => *count += 1,
            None => self.document.skipped.push((name.to_owned(), 1)),
        }
    }

    /// Reads the paragraphs inside sections, layouts and other blocks.
    fn block(&mut self, element: &Element) {
        for child in element.elements() {
            if child.name.ns != SF {
                continue;
            }
            match child.name.local.as_str() {
                "p" => self.paragraph(child),
                "section" => {
                    self.section = child.attr(SF, "style").map(str::to_owned);
                    self.block(child);
                }
                "layout" => {
                    self.layout = child.attr(SF, "style").map(str::to_owned);
                    self.block(child);
                }
                "text-body" => self.block(child),
                // A table of contents: its entries, each a paragraph.
                "toc" => {
                    let outer = std::mem::replace(&mut self.in_toc, true);
                    for content in child.elements() {
                        for entry in content.elements() {
                            self.block(entry);
                        }
                    }
                    self.in_toc = outer;
                }
                name if IGNORED.contains(&name) => {}
                _ => self.skip(child),
            }
        }
    }

    fn paragraph(&mut self, element: &Element) {
        let mut paragraph = Paragraph {
            style: element.attr(SF, "style").map(str::to_owned),
            list_level: element
                .attr(SF, "list-level")
                .and_then(|v| v.trim().parse::<u8>().ok())
                .unwrap_or(0)
                .min(8),
            restart_list: element.attr(SF, "restart-list") == Some("true"),
            column_break_before: std::mem::take(&mut self.column_break),
            section: self.section.take(),
            layout: self.layout.take(),
            marks: Vec::new(),
            toc: self.in_toc,
            runs: Vec::new(),
        };
        self.inline(element, None, &mut paragraph.runs);
        paragraph.marks = std::mem::take(&mut self.marks);
        // A column break ends its paragraph, as a return does.
        if let Some(last) = paragraph.runs.last_mut()
            && last.text.ends_with(COLUMN_BREAK)
        {
            last.text.pop();
            self.column_break = true;
        }
        paragraph.runs.retain(|run| !run.text.is_empty());
        self.document.body.push(paragraph);
        if std::mem::take(&mut self.note_break) {
            self.note_ends.push(self.document.body.len());
        }
    }

    /// Reads the text inside a paragraph or a span into runs.
    fn inline(&mut self, element: &Element, style: Option<&str>, runs: &mut Vec<Run>) {
        for node in &element.children {
            match node {
                Node::Text(text) => self.push(runs, style, text),
                Node::Element(child)
                    if child.name.ns == SF && WRAPPERS.contains(&child.name.local.as_str()) =>
                {
                    // A reference to a change holds the changed text, so it
                    // is read, not replaced by what it refers to.
                    self.skip(child);
                    self.inline(child, style, runs);
                }
                Node::Element(child) if child.name.ns == SF => {
                    let child = self.ids.deref(child);
                    match child.name.local.as_str() {
                        "span" => {
                            let inner = child.attr(SF, "style").or(style);
                            self.inline(child, inner, runs);
                        }
                        "link" => {
                            let outer = self.link.take();
                            self.link = child
                                .attr("", "href")
                                .or(child.attr(SF, "href"))
                                .map(str::to_owned);
                            let inner = child.attr(SF, "style").or(style);
                            self.inline(child, inner, runs);
                            self.link = outer;
                        }
                        "bookmark" => {
                            let outer = self.bookmark.take();
                            self.bookmark = child.attr(SF, "name").map(str::to_owned);
                            self.inline(child, style, runs);
                            self.bookmark = outer;
                        }
                        "toc-bookmark" => {
                            if let Some(name) = child.attr(SF, "name") {
                                self.marks.push(name.to_owned());
                            }
                        }
                        "toc-pagenum" => {
                            let field = Field::PageOf {
                                bookmark: child.attr(SF, "bookmark").unwrap_or_default().to_owned(),
                            };
                            self.push_field(runs, style, field);
                        }
                        // Placeholder text of a template, which Pages
                        // shows and prints until it is replaced.
                        "ghost-text" | "ghost-text-ref" => self.inline(child, style, runs),
                        "lnbr" => self.push(runs, style, &LINE_BREAK.to_string()),
                        "pgbr" => self.push(runs, style, &PAGE_BREAK.to_string()),
                        "contbr" => self.push(runs, style, &COLUMN_BREAK.to_string()),
                        "tab" => self.push(runs, style, "\t"),
                        "page-number" => {
                            let format = child.attr(SF, "format").map(str::to_owned);
                            self.push_field(runs, style, Field::PageNumber { format });
                        }
                        "page-count" => {
                            let format = child.attr(SF, "format").map(str::to_owned);
                            self.push_field(runs, style, Field::PageCount { format });
                        }
                        "date-time" => {
                            let field = Field::DateTime {
                                format: child.attr(SF, "format").unwrap_or_default().to_owned(),
                                shown: child.text(),
                                update: child.attr(SF, "auto-update") == Some("true"),
                            };
                            self.push_field(runs, style, field);
                        }
                        "filename" => {
                            let field = Field::FileName {
                                shown: child.text(),
                                display: child
                                    .attr(SF, "display-format")
                                    .and_then(|d| d.trim().parse().ok()),
                            };
                            self.push_field(runs, style, field);
                        }
                        "footnote" => {
                            let field = Field::Note {
                                mark: child
                                    .attr(SF, "autonumber")
                                    .is_none()
                                    .then(|| child.attr(SF, "mark"))
                                    .flatten()
                                    .map(str::to_owned),
                            };
                            self.push_field(runs, style, field);
                        }
                        "footnote-mark" => self.push_field(runs, style, Field::NoteMark),
                        "footnotebr" => self.note_break = true,
                        name if IGNORED.contains(&name) => {}
                        _ => self.skip(child),
                    }
                }
                Node::Element(child) => self.skip(child),
            }
        }
    }
}

impl BodyReader<'_, '_> {
    /// Adds text to the last run when it has the same style, link and
    /// bookmark and is not a field.
    fn push(&self, runs: &mut Vec<Run>, style: Option<&str>, text: &str) {
        match runs.last_mut() {
            Some(last)
                if last.style.as_deref() == style
                    && last.field.is_none()
                    && last.link == self.link
                    && last.bookmark == self.bookmark =>
            {
                last.text.push_str(text);
            }
            _ => runs.push(Run {
                style: style.map(str::to_owned),
                field: None,
                link: self.link.clone(),
                bookmark: self.bookmark.clone(),
                text: text.to_owned(),
            }),
        }
    }

    /// Adds a field, as a run of its own.
    fn push_field(&self, runs: &mut Vec<Run>, style: Option<&str>, field: Field) {
        runs.push(Run {
            style: style.map(str::to_owned),
            field: Some(field),
            link: self.link.clone(),
            bookmark: self.bookmark.clone(),
            text: FIELD.to_string(),
        });
    }
}
