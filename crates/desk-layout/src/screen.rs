// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Screen View, as Pages 15.4 shows a document (PXV-01): the text in one
//! continuous flow as wide as the window, without the edges of pages,
//! their margins, headers and footers. Page, column and section breaks
//! do not start a new page, the columns of a layout become one, and notes
//! go to the end.
//!
//! Which objects show, and how, was settled on Pages 15.4 (step 19e of
//! W1): only the placement decides it, never the wrap.
//!
//! - An object that moves with the text shows, whatever its wrap. In the
//!   line (Inline with Text) it stays where it is, at its own size, where
//!   it can be selected and moved. With any other wrap (Automatic, Around,
//!   Above and Below, None) it leaves its place on the page for a block
//!   of its own after the paragraph it is anchored to, centred in the
//!   column; several objects of one paragraph go one under the other, in
//!   their order.
//! - It keeps its size, made smaller only when it is wider than the
//!   column; but a text box, or a plain rectangle with text in it, takes
//!   the width of the column and the height of its text, as a paragraph
//!   would.
//! - An object fixed on a page does not show, wherever its page is, nor
//!   do the objects of the masters: they are the content that the notice
//!   of Screen View says is hidden.
//!
//! The paragraphs and their text are those of the document, so a place
//! in the text is the same in both; only how they are laid out changes.

use desk_model::{
    Anchor, Document, Layout, Length, NoteKind, Object, ObjectKind, Op, ParaAttrs, Shape,
    TextSettings, VerticalAlignment, Wrap,
};
use desk_text::TextSystem;

use crate::objects::{PlacedObject, text_box_lines};
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

    // Objects that move with the text go in the flow, after their
    // paragraph, as the Mac showed on 2026-11-18; those fixed on a page
    // and those of the masters are hidden, as it showed on 2026-11-21.
    // Objects in the text stay in its lines.
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
            Anchor::Paragraph(id) => index_of(id),
            Anchor::Page(_) | Anchor::Master(..) => None,
        };
        let Some(after) = after else {
            apply(&mut screen, Op::RemoveObject { object: object.id });
            continue;
        };
        // The paragraph after it holds the object above its text; the
        // last paragraph holds those after it.
        let holder = body.get(after + 1).copied().unwrap_or(body[after]);
        let (w, h) = if fills_column(&object) {
            (column, text_height(&screen, &object, column, system))
        } else {
            let (w, h) = (object.frame.width.to_pt(), object.frame.height.to_pt());
            let scale = if w > column && w > 0.0 {
                column / w
            } else {
                1.0
            };
            (w * scale, h * scale)
        };
        object.frame.width = Length::from_pt(w);
        object.frame.height = Length::from_pt(h);
        object.frame.x = Length::from_pt(SCREEN_MARGIN + (column - w) / 2.0);
        let top = stacked.entry(holder).or_insert(0.0);
        object.frame.y = Length::from_pt(*top);
        *top += h + OBJECT_GAP;
        object.anchor = Anchor::Paragraph(holder);
        object.wrap = Wrap::AboveAndBelow;
        apply(&mut screen, Op::ReplaceObject { object });
    }
    screen
}

/// Whether an object takes the width of the column in the flow: a text
/// box, or a plain rectangle with some text in it, which Pages 15.4 shows
/// as a block of text. Other shapes, pictures and empty rectangles keep
/// their size.
fn fills_column(object: &Object) -> bool {
    match &object.kind {
        ObjectKind::TextBox(_) => true,
        ObjectKind::Shape(Shape::Rectangle) => object
            .text_box()
            .is_some_and(|text| text.paragraphs.iter().any(|p| !p.is_empty())),
        _ => false,
    }
}

/// How tall an object that fills the column is: its text laid out
/// `width` points wide from the top, with its inset above and below; its
/// own height when it has no text.
fn text_height(document: &Document, object: &Object, width: f64, system: &mut TextSystem) -> f64 {
    let mut measured = object.clone();
    measured.frame.x = Length::ZERO;
    measured.frame.y = Length::ZERO;
    measured.frame.width = Length::from_pt(width);
    measured.frame.height = Length::from_pt(FLOW_HEIGHT);
    let Some(text) = measured.text_box_mut() else {
        return object.frame.height.to_pt();
    };
    text.vertical = VerticalAlignment::Top;
    let inset = text.inset.to_pt();
    let paragraphs = text.paragraphs.clone();
    let placed = PlacedObject::new(&measured, None, 0.0);
    let lines = text_box_lines(document, &placed, &paragraphs, system);
    if lines.is_empty() {
        return object.frame.height.to_pt();
    }
    let bottom = lines.iter().map(|l| l.bottom()).fold(0.0f32, f32::max);
    f64::from(bottom) + inset
}

fn apply(document: &mut Document, op: Op) {
    // The operations come from the document itself, so they apply.
    let _ = document.apply(op);
}
