// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Screen View, as Pages 15.4 shows a document (PXV-01): the text in one
//! continuous flow as wide as the window, without the edges of pages,
//! their margins, headers and footers. Page, column and section breaks
//! do not start a new page, the columns of a layout become one, and notes
//! go to the end. Whatever its wrap, an object that moves with the text
//! shows in a block of its own after the paragraph it is anchored to, in
//! the middle of the column: a text box, or a rectangle with text, across
//! the column and as high as its text, and the others at their size.
//! Objects in the text stay in their lines, where they can be selected
//! and moved. Objects fixed on a page and those of the masters do not
//! show. So Pages 15.4 did on the owner's Mac on 2026-11-21.
//!
//! The paragraphs and their text are those of the document, so a place
//! in the text is the same in both; only how they are laid out changes.

use desk_model::{Anchor, Document, Layout, Length, NoteKind, Op, ParaAttrs, TextSettings, Wrap};
use desk_text::TextSystem;

use crate::pages::{DocumentLayout, paginate};

/// The room between the edges of the flow and its text, in points.
pub const SCREEN_MARGIN: f64 = 24.0;

/// The height of the one page that holds the flow while it is laid out:
/// more than any document needs, and small enough that points keep their
/// fractions in an `f32`.
const FLOW_HEIGHT: f64 = 2_000_000.0;

/// The room between two objects in the flow, in points.
const OBJECT_GAP: f64 = 6.0;

/// The longest line of Screen View, in points at a Text Size of 100%:
/// Pages 15.4 keeps its column this wide at most, centred in a wider
/// window, and draws a table in the text this wide whatever its own width
/// (the Mac, 2026-11-21, step 19f).
pub const SCREEN_LINE: f64 = 664.0;

/// The narrowest flow, in points.
const NARROWEST: f64 = 144.0;

/// Lays out a document in Screen View, `width` points wide: one page as
/// tall as its text, whose `setup` gives that size.
///
/// `zoom` is the Text Size, 1 for 100%: pictures and shapes in the text
/// keep their size on the screen whatever it is, as in Pages 15.4.
pub fn paginate_screen(
    document: &Document,
    width: f64,
    zoom: f64,
    system: &mut TextSystem,
) -> DocumentLayout {
    // The layouts that fit the boxes to their text share one budget.
    let _work = desk_model::table::share_work();
    let mut fits = Fits::default();
    let mut layout = paginate(&screen_document_with(document, width, zoom, &fits), system);
    // The boxes of text across the column take the height of their text,
    // and the objects of the last paragraph go below its text: both are
    // measured on a first layout of the flow.
    let last = document.body().len().checked_sub(1);
    for page in &layout.pages {
        for placed in &page.objects {
            let id = placed.object.id;
            if !document.objects().get(id).is_some_and(spans_column) || placed.lines.is_empty() {
                continue;
            }
            let top = placed
                .lines
                .iter()
                .map(|l| l.top())
                .fold(f32::INFINITY, f32::min);
            let bottom = placed
                .lines
                .iter()
                .map(|l| l.bottom())
                .fold(0.0f32, f32::max);
            let inset = placed.object.text_box().map_or(0.0, |t| t.inset.to_pt());
            fits.heights
                .insert(id, f64::from(bottom - top) + 2.0 * inset);
        }
        // How far the text of the last paragraph reaches below where its
        // objects are measured from: the top of the objects above it, or
        // of its first line.
        let lines = page.lines.iter().filter(|l| Some(l.paragraph) == last);
        let (top, bottom) = lines.fold((f32::INFINITY, 0.0f32), |(t, b), l| {
            (t.min(l.top()), b.max(l.bottom()))
        });
        let before = last
            .and_then(|i| i.checked_sub(1))
            .and_then(|i| document.body().get(i))
            .map(|p| p.id());
        let origin = page
            .objects
            .iter()
            .filter(|o| {
                before.is_some()
                    && document
                        .objects()
                        .get(o.object.id)
                        .is_some_and(|d| Some(d.anchor) == before.map(Anchor::Paragraph))
            })
            .map(|o| o.y)
            .fold(top, f32::min);
        if bottom > origin {
            fits.last = Some(f64::from(bottom - origin));
        }
    }
    if !fits.heights.is_empty() || fits.last.is_some() {
        layout = paginate(&screen_document_with(document, width, zoom, &fits), system);
    }
    // The flow ends below the last thing in it.
    let bottom = layout
        .pages
        .iter()
        .flat_map(|page| {
            page.lines
                .iter()
                .map(|l| l.bottom())
                .chain(page.note_lines.iter().map(|l| l.bottom()))
                .chain(page.objects.iter().map(|o| o.y + o.height))
        })
        .fold(0.0f32, f32::max);
    layout.setup.height = Length::from_pt(f64::from(bottom) + SCREEN_MARGIN);
    layout
}

/// What a first layout of the flow measured: the heights of the text of
/// the boxes across the column, and the height of the last paragraph.
#[derive(Default)]
struct Fits {
    heights: std::collections::HashMap<desk_model::NodeId, f64>,
    last: Option<f64>,
}

/// Whether an object goes across the column in Screen View, as high as
/// its text: a text box, or a rectangle with text, as the Mac showed on
/// 2026-11-21. Other objects keep their size.
fn spans_column(object: &desk_model::Object) -> bool {
    let has_text = |object: &desk_model::Object| {
        object
            .text_box()
            .is_some_and(|t| t.paragraphs.iter().any(|p| !p.is_empty()))
    };
    match &object.kind {
        desk_model::ObjectKind::TextBox(_) => true,
        desk_model::ObjectKind::Shape(desk_model::Shape::Rectangle) => has_text(object),
        _ => false,
    }
}

/// The document as Screen View lays it out: see the module.
pub fn screen_document(document: &Document, width: f64, zoom: f64) -> Document {
    screen_document_with(document, width, zoom, &Fits::default())
}

fn screen_document_with(document: &Document, width: f64, zoom: f64, fits: &Fits) -> Document {
    let mut screen = document.clone();
    // Text at the sizes Screen View shows (the Mac, step 19g).
    screen.set_screen_styles(Some(zoom));
    let mut page = *screen.page_setup();

    let width = width.max(NARROWEST);
    // The column, no longer than the longest line, in the middle.
    let side = SCREEN_MARGIN.max((width - SCREEN_LINE) / 2.0);
    let column = width - 2.0 * side;
    page.width = Length::from_pt(width);
    page.height = Length::from_pt(FLOW_HEIGHT);
    let margin = Length::from_pt(SCREEN_MARGIN);
    page.margin_top = margin;
    page.margin_bottom = margin;
    page.margin_left = Length::from_pt(side);
    page.margin_right = Length::from_pt(side);
    page.facing_pages = false;
    page.headers = false;
    page.footers = false;
    page.layout_pages = None;
    page.layout = Layout {
        columns: desk_model::Columns::ONE,
        before: Length::ZERO,
        after: Length::ZERO,
        left: Length::ZERO,
        right: Length::ZERO,
        new_page: false,
        balanced: false,
    };
    apply(&mut screen, Op::SetPageSetup { page });

    // Notes go to the end, since the flow has no foot.
    let settings = screen.text_settings();
    if settings.notes.kind == NoteKind::Footnotes
        || settings.notes.kind == NoteKind::SectionEndnotes
    {
        let mut notes = settings.notes;
        notes.kind = NoteKind::DocumentEndnotes;
        let settings = TextSettings {
            notes,
            ..settings.clone()
        };
        apply(&mut screen, Op::SetTextSettings { settings });
    }

    // Nothing breaks the flow.
    let formats: Vec<Op> = screen
        .body()
        .iter()
        .filter_map(|paragraph| {
            let mut attrs = paragraph.attrs().clone();
            attrs.merge(&ParaAttrs {
                start: Some(None),
                page_break_before: Some(false),
                column_break_before: Some(false),
                ..ParaAttrs::default()
            });
            (&attrs != paragraph.attrs()).then(|| Op::SetParagraphFormat {
                paragraph: paragraph.id(),
                style: paragraph.style(),
                attrs,
            })
        })
        .collect();
    for op in formats {
        apply(&mut screen, op);
    }

    // Objects that move with the text go in the flow, each in a block
    // of its own after the paragraph it is anchored to, in the middle of
    // the column; those fixed on a page and those of the masters are
    // hidden. Objects in the text stay in its lines. So the Mac showed on
    // 2026-11-21, whatever the wrap of the object.
    let body: Vec<_> = screen.body().iter().map(|p| p.id()).collect();
    let index_of = |id| body.iter().position(|p| *p == id);
    // Those of the last paragraph go below its text, so after those
    // that go above it.
    let last = body.last().copied();
    let mut objects: Vec<_> = screen.objects().iter().cloned().collect();
    objects.sort_by_key(|o| last.is_some_and(|p| o.anchor == Anchor::Paragraph(p)));
    // How far down the objects already in the flow of each paragraph
    // reach, so that the next goes under them.
    let mut stacked: std::collections::HashMap<(desk_model::NodeId, bool), f64> =
        Default::default();
    for mut object in objects {
        let after = match object.anchor {
            Anchor::InText => {
                in_text(&mut object, zoom);
                apply(&mut screen, Op::ReplaceObject { object });
                continue;
            }
            Anchor::Paragraph(id) => index_of(id),
            Anchor::Page(_) | Anchor::Master(..) => None,
        };
        let Some(after) = after else {
            apply(&mut screen, Op::RemoveObject { object: object.id });
            continue;
        };
        // The paragraph after it holds the object above its text; the
        // last paragraph holds those after it, below its own text.
        let (holder, below) = match body.get(after + 1) {
            Some(next) => (*next, false),
            None => (body[after], true),
        };
        // Below its text, once measured; until then, out of the way of
        // the text, so that the text is measured as it will lie.
        let start = if below {
            // Past the margin that the text keeps from it.
            fits.last.map_or(0.0, |h| h + OBJECT_GAP + 1.0)
        } else {
            0.0
        };
        // The others keep their size on the screen, whatever the Text
        // Size (the Mac, step 19g).
        let zoom = if zoom > 0.0 { zoom } else { 1.0 };
        let (w, h) = (
            object.frame.width.to_pt() / zoom,
            object.frame.height.to_pt() / zoom,
        );
        let (w, h) = if spans_column(&object) {
            (column, fits.heights.get(&object.id).copied().unwrap_or(h))
        } else if w > column && w > 0.0 {
            (column, h * column / w)
        } else {
            (w, h)
        };
        object.frame.width = Length::from_pt(w);
        object.frame.height = Length::from_pt(h);
        object.frame.x = Length::from_pt(side + (column - w) / 2.0);
        let top = stacked.entry((holder, below)).or_insert(start);
        object.frame.y = Length::from_pt(*top);
        *top += h + OBJECT_GAP;
        object.anchor = Anchor::Paragraph(holder);
        object.wrap_margin = Length::from_pt(OBJECT_GAP);
        object.wrap = if below && fits.last.is_none() {
            Wrap::Through
        } else {
            Wrap::AboveAndBelow
        };
        apply(&mut screen, Op::ReplaceObject { object });
    }
    screen
}

/// An object in the text as Screen View shows it: a table as wide as the
/// longest line, its columns in proportion, and a picture or a shape at
/// its size on the screen, whatever the Text Size (the Mac, step 19f).
fn in_text(object: &mut desk_model::Object, zoom: f64) {
    match &object.kind {
        desk_model::ObjectKind::Table(table) => {
            let mut wide = (**table).clone();
            let total: f64 = (0..wide.cols()).map(|c| f64::from(wide.width(c))).sum();
            if total > 0.0 {
                let scale = SCREEN_LINE * 1000.0 / total;
                for c in 0..wide.cols() {
                    let w = (f64::from(wide.width(c)) * scale).round() as u32;
                    wide.set_width(c, w);
                }
                object.kind = desk_model::ObjectKind::Table(std::sync::Arc::new(wide));
                object.frame.width = Length::from_pt(SCREEN_LINE);
            }
        }
        desk_model::ObjectKind::Image(_) | desk_model::ObjectKind::Shape(_) if zoom > 0.0 => {
            object.frame.width = Length::from_pt(object.frame.width.to_pt() / zoom);
            object.frame.height = Length::from_pt(object.frame.height.to_pt() / zoom);
        }
        _ => {}
    }
}

fn apply(document: &mut Document, op: Op) {
    // The operations come from the document itself, so they apply.
    let _ = document.apply(op);
}
