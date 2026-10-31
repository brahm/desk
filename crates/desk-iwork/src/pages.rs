// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! A Pages '09 or '08 document: its body text, styles, page and
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
        }
    }
}

/// A run of text with one character style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    /// The character style, by `sf:style`, for text inside a span.
    pub style: Option<String>,
    /// The text, with breaks as [`LINE_BREAK`], [`PAGE_BREAK`],
    /// [`COLUMN_BREAK`] and tabs as `\t`.
    pub text: String,
}

/// A paragraph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paragraph {
    /// The paragraph style, by `sf:style`.
    pub style: Option<String>,
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
    /// The kinds of content that the reader skipped, by element name, in
    /// the order first met, with how many of each: what an import cannot
    /// show yet. Besides the elements of the body, `headers`, `footers`
    /// and `drawables` count the headers, footers and floating objects of
    /// the document, which are outside the body.
    pub skipped: Vec<(String, usize)>,
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
    // Pages saves a header and a footer for each section even when they
    // are empty, so only those with text count.
    for name in ["headers", "footers"] {
        let count = root
            .children_named(SF, name)
            .map(|part| part.elements().filter(|e| has_text(e)).count())
            .sum::<usize>();
        if count > 0 {
            skipped.push((name.to_owned(), count));
        }
    }
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
        let mut reader = BodyReader {
            ids: &ids,
            document: &mut document,
        };
        reader.block(body);
    }
    Ok(document)
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

/// Whether an element holds text other than white space.
fn has_text(element: &Element) -> bool {
    std::iter::once(element)
        .chain(element.descendants())
        .any(|e| {
            e.children
                .iter()
                .any(|n| matches!(n, Node::Text(t) if !t.trim().is_empty()))
        })
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
];

struct BodyReader<'a, 'b> {
    ids: &'a Ids<'a>,
    document: &'b mut PagesDocument,
}

impl BodyReader<'_, '_> {
    fn skip(&mut self, element: &Element) {
        let name = &element.name.local;
        match self.document.skipped.iter_mut().find(|(n, _)| n == name) {
            Some((_, count)) => *count += 1,
            None => self.document.skipped.push((name.clone(), 1)),
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
                "section" | "layout" | "text-body" => self.block(child),
                name if IGNORED.contains(&name) => {}
                _ => self.skip(child),
            }
        }
    }

    fn paragraph(&mut self, element: &Element) {
        let mut paragraph = Paragraph {
            style: element.attr(SF, "style").map(str::to_owned),
            runs: Vec::new(),
        };
        self.inline(element, None, &mut paragraph.runs);
        paragraph.runs.retain(|run| !run.text.is_empty());
        self.document.body.push(paragraph);
    }

    /// Reads the text inside a paragraph or a span into runs.
    fn inline(&mut self, element: &Element, style: Option<&str>, runs: &mut Vec<Run>) {
        for node in &element.children {
            match node {
                Node::Text(text) => push(runs, style, text),
                Node::Element(child) if child.name.ns == SF => {
                    let child = self.ids.deref(child);
                    match child.name.local.as_str() {
                        "span" | "link" => {
                            let inner = child.attr(SF, "style").or(style);
                            self.inline(child, inner, runs);
                        }
                        // Placeholder text of a template, which Pages
                        // shows and prints until it is replaced.
                        "ghost-text" | "ghost-text-ref" => self.inline(child, style, runs),
                        "lnbr" => push(runs, style, &LINE_BREAK.to_string()),
                        "pgbr" => push(runs, style, &PAGE_BREAK.to_string()),
                        "contbr" => push(runs, style, &COLUMN_BREAK.to_string()),
                        "tab" => push(runs, style, "\t"),
                        name if IGNORED.contains(&name) => {}
                        _ => self.skip(child),
                    }
                }
                Node::Element(child) => self.skip(child),
            }
        }
    }
}

/// Adds text to the last run when it has the same style.
fn push(runs: &mut Vec<Run>, style: Option<&str>, text: &str) {
    match runs.last_mut() {
        Some(last) if last.style.as_deref() == style => last.text.push_str(text),
        _ => runs.push(Run {
            style: style.map(str::to_owned),
            text: text.to_owned(),
        }),
    }
}
