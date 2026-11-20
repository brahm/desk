// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Screen View, as Pages 15.4 shows a document (PXV-01): the text in one
//! continuous flow as wide as the window, without the edges of pages,
//! their margins, headers and footers. Page, column and section breaks
//! do not start a new page, the columns of a layout become one, and notes
//! go to the end. A floating object shows in the flow, across the column,
//! after the paragraph it is anchored to; an object fixed on a page goes
//! after the last paragraph that starts on that page. Objects of the
//! masters do not show.
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

/// The narrowest flow, in points.
const NARROWEST: f64 = 144.0;

/// Lays out a document in Screen View, `width` points wide: one page as
/// tall as its text, whose `setup` gives that size.
pub fn paginate_screen(document: &Document, width: f64, system: &mut TextSystem) -> DocumentLayout {
    let screen = screen_document(document, width, system);
    let mut layout = paginate(&screen, system);
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

/// The document as Screen View lays it out: see the module.
pub fn screen_document(document: &Document, width: f64, system: &mut TextSystem) -> Document {
    let mut screen = document.clone();
    let mut page = *screen.page_setup();
    // Where the objects fixed on pages go: after the last paragraph that
    // starts on their page, as the pages lay it out.
    let fixed_on_pages = screen
        .objects()
        .iter()
        .any(|o| matches!(o.anchor, Anchor::Page(_)));
    let last_on_page: Vec<Option<usize>> = if fixed_on_pages && page.layout_pages.is_none() {
        let pages = paginate(document, system);
        pages
            .pages
            .iter()
            .map(|p| p.lines.iter().map(|l| l.paragraph).max())
            .collect()
    } else {
        Vec::new()
    };

    page.width = Length::from_pt(width.max(NARROWEST));
    page.height = Length::from_pt(FLOW_HEIGHT);
    let margin = Length::from_pt(SCREEN_MARGIN);
    page.margin_top = margin;
    page.margin_bottom = margin;
    page.margin_left = margin;
    page.margin_right = margin;
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

    // Floating objects go in the flow, after their paragraph; those of
    // the masters do not show.
    let body: Vec<_> = screen.body().iter().map(|p| p.id()).collect();
    let index_of = |id| body.iter().position(|p| *p == id);
    let column = width.max(NARROWEST) - 2.0 * SCREEN_MARGIN;
    let objects: Vec<_> = screen.objects().iter().cloned().collect();
    // How far down the objects already above the text of each paragraph
    // reach, so that the next goes under them.
    let mut stacked: std::collections::HashMap<desk_model::NodeId, f64> = Default::default();
    for mut object in objects {
        let after = match object.anchor {
            Anchor::InText => continue,
            Anchor::Master(..) => {
                apply(&mut screen, Op::RemoveObject { object: object.id });
                continue;
            }
            Anchor::Paragraph(id) => index_of(id),
            Anchor::Page(n) => last_on_page
                .get(n as usize)
                .or(last_on_page.last())
                .copied()
                .flatten(),
        };
        let Some(after) = after.or(if body.is_empty() { None } else { Some(0) }) else {
            continue;
        };
        // The paragraph after it holds the object above its text; the
        // last paragraph holds those after it.
        let holder = body.get(after + 1).copied().unwrap_or(body[after]);
        let (w, h) = (object.frame.width.to_pt(), object.frame.height.to_pt());
        let scale = if w > column && w > 0.0 {
            column / w
        } else {
            1.0
        };
        object.frame.width = Length::from_pt(w * scale);
        object.frame.height = Length::from_pt(h * scale);
        object.frame.x = Length::from_pt(SCREEN_MARGIN);
        let top = stacked.entry(holder).or_insert(0.0);
        object.frame.y = Length::from_pt(*top);
        *top += h * scale + OBJECT_GAP;
        object.anchor = Anchor::Paragraph(holder);
        object.wrap = Wrap::AboveAndBelow;
        apply(&mut screen, Op::ReplaceObject { object });
    }
    screen
}

fn apply(document: &mut Document, op: Op) {
    // The operations come from the document itself, so they apply.
    let _ = document.apply(op);
}
