// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The objects on the pages and the text around them (OBJ-01, OBJ-13,
//! OBJ-17).
//!
//! The rules, as Pages '09 places floating objects:
//!
//! - An object fixed on a page is where its frame says, from the top left
//!   corner of the paper. One on a page past the last is drawn on the
//!   last page.
//! - An object that moves with a paragraph goes on the page where the
//!   paragraph starts, its top measured from the top of the paragraph's
//!   first line, and moves with it to the next page or column.
//! - The text goes around the box that holds the object, rotated, with
//!   the room of its wrap margin on every side: on both sides, only on
//!   the side with more room, only on its left or its right, or above and
//!   below it. A piece of a line narrower than
//!   [`desk_text::MIN_SEGMENT_WIDTH`] is left empty. An object that the
//!   text goes through takes no room.

use std::collections::HashMap;
use std::sync::Arc;

use desk_model::{Anchor, Document, Object, ObjectKind, Shape, VerticalAlignment, Wrap};
use desk_text::{Column, LineSpace, ParagraphText, TextSystem};

use crate::pages::{DocumentLayout, PlacedLine};

/// An object placed on a page.
#[derive(Clone, Debug)]
pub struct PlacedObject {
    /// The object.
    pub object: Object,
    /// The paragraph of the body it moves with, by index, if it does.
    pub paragraph: Option<usize>,
    /// The left edge of its frame before it is rotated, in points from
    /// the left of the paper.
    pub x: f32,
    /// The top edge of its frame before it is rotated, in points from the
    /// top of the paper.
    pub y: f32,
    /// The width of its frame.
    pub width: f32,
    /// The height of its frame.
    pub height: f32,
    /// The lines of a text box, placed on the page as if it were not
    /// rotated. Their paragraphs are indexes in the text box.
    pub lines: Vec<PlacedLine>,
    /// The objects of a group, placed.
    pub children: Vec<PlacedObject>,
    /// What the objects above a text box on its page take from its text,
    /// which goes around them as body text does (OBJ-13, OBJ-17).
    pub(crate) wraps: Arc<[Exclusion]>,
}

impl PlacedObject {
    /// An object placed with its frame moved down by `dy`, as its anchor
    /// says, with the objects of a group moved with it.
    pub(crate) fn new(object: &Object, paragraph: Option<usize>, dy: f32) -> Self {
        let frame = object.frame;
        let children = match &object.kind {
            ObjectKind::Group(objects) => objects
                .iter()
                .map(|o| Self::new(o, paragraph, dy))
                .collect(),
            _ => Vec::new(),
        };
        Self {
            object: object.clone(),
            paragraph,
            x: frame.x.to_pt() as f32,
            y: frame.y.to_pt() as f32 + dy,
            width: frame.width.to_pt() as f32,
            height: frame.height.to_pt() as f32,
            lines: Vec::new(),
            children,
            wraps: Arc::from([]),
        }
    }

    /// The center of the frame, which it rotates around.
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// The angle it is rotated by, in radians, counterclockwise.
    pub fn angle(&self) -> f32 {
        (self.object.rotation as f32 / 100.0).to_radians()
    }

    /// The smallest upright box that holds the object as it is rotated:
    /// left, top, right and bottom.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let (cx, cy) = self.center();
        let (sin, cos) = self.angle().sin_cos();
        let half_w = (self.width * cos).abs() / 2.0 + (self.height * sin).abs() / 2.0;
        let half_h = (self.width * sin).abs() / 2.0 + (self.height * cos).abs() / 2.0;
        (cx - half_w, cy - half_h, cx + half_w, cy + half_h)
    }

    /// A point of the page in the frame's own coordinates: from its top
    /// left corner, as if it were not rotated.
    pub fn to_frame(&self, x: f32, y: f32) -> (f32, f32) {
        let (cx, cy) = self.center();
        let (sin, cos) = self.angle().sin_cos();
        let (dx, dy) = (x - cx, y - cy);
        // The page has y going down, so a counterclockwise rotation by a
        // is undone by turning the point clockwise on the screen.
        let fx = dx * cos - dy * sin;
        let fy = dx * sin + dy * cos;
        (fx + self.width / 2.0, fy + self.height / 2.0)
    }

    /// A point of the frame, from its top left corner as if it were not
    /// rotated, on the page.
    pub fn to_page(&self, fx: f32, fy: f32) -> (f32, f32) {
        let (cx, cy) = self.center();
        let (sin, cos) = self.angle().sin_cos();
        let (dx, dy) = (fx - self.width / 2.0, fy - self.height / 2.0);
        (cx + dx * cos + dy * sin, cy - dx * sin + dy * cos)
    }

    /// Whether a point of the page is on the object, or within
    /// `tolerance` of a line (OBJ-21). A shape is hit anywhere in its
    /// frame, as Pages selects it.
    pub fn contains(&self, x: f32, y: f32, tolerance: f32) -> bool {
        let (fx, fy) = self.to_frame(x, y);
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        if let Some((a, b)) = self.object.line_ends_in(w, h) {
            let to = |(x, y): (f64, f64)| (x as f32, y as f32);
            return distance_to_segment((fx, fy), to(a), to(b)) <= tolerance;
        }
        if let ObjectKind::Shape(shape @ desk_model::Shape::Connection(_)) = &self.object.kind {
            // Near the line, along its curve.
            let outline = shape.outline(f64::from(self.width), f64::from(self.height));
            let point = |(x, y): (f64, f64)| (x as f32, y as f32);
            return desk_model::flattened(&outline)
                .into_iter()
                .any(|(a, b)| distance_to_segment((fx, fy), point(a), point(b)) <= tolerance);
        }
        fx >= -tolerance
            && fy >= -tolerance
            && fx <= self.width + tolerance
            && fy <= self.height + tolerance
    }

    /// The room the object takes from the text, if it takes any.
    pub(crate) fn exclusion(&self) -> Option<Exclusion> {
        if matches!(self.object.wrap, Wrap::Through | Wrap::Behind) {
            return None;
        }
        // The margin is from the outside of the stroke, as in Pages
        // (OBJ-17).
        let stroke = self
            .object
            .style
            .stroke
            .as_ref()
            .map_or(0.0, |s| s.width.to_pt() as f32 / 2.0);
        let margin = self.object.wrap_margin.to_pt() as f32 + stroke;
        let (x0, y0, x1, y1) = self.bounds();
        Some(Exclusion {
            x0: x0 - margin,
            y0: y0 - margin,
            x1: x1 + margin,
            y1: y1 + margin,
            wrap: self.object.wrap,
            profile: self.profile(margin),
        })
    }

    /// The room that the contour of the object takes from the text, band
    /// by band down the page, with the margin around it: each band is
    /// from a top to a bottom, and from a left to a right.
    fn profile(&self, margin: f32) -> Option<Profile> {
        let contour = self.object.contour.as_ref().filter(|c| c.len() >= 3)?;
        let scale = desk_model::POLYGON_SCALE as f32;
        let points: Vec<(f32, f32)> = contour
            .iter()
            .map(|&(x, y)| {
                self.to_page(
                    x as f32 / scale * self.width,
                    y as f32 / scale * self.height,
                )
            })
            .collect();
        let top = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let bottom = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        let mut bands = Vec::new();
        let mut b0 = top - margin;
        while b0 < bottom + margin {
            let b1 = b0 + PROFILE_BAND;
            // The contour grown by the margin all around, as Pages grows
            // it: a point of the contour takes the room of a circle of the
            // margin around it, so that the room ends round over the top
            // and bottom of a round picture (OBJ-17).
            let mut span: Option<(f32, f32)> = None;
            let mut take = |x: f32, y: f32| {
                let dy = if y < b0 {
                    b0 - y
                } else if y > b1 {
                    y - b1
                } else {
                    0.0
                };
                if dy > margin {
                    return;
                }
                let reach = (margin * margin - dy * dy).max(0.0).sqrt();
                let (s, t) = (x - reach, x + reach);
                span = Some(span.map_or((s, t), |(a, b)| (a.min(s), b.max(t))));
            };
            let (lo, hi) = (b0 - margin, b1 + margin);
            for (i, &a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                let (ya, yb) = (a.1.min(b.1), a.1.max(b.1));
                if yb < lo || ya > hi {
                    continue;
                }
                let at = |y: f32| {
                    if (b.1 - a.1).abs() < f32::EPSILON {
                        (a.0, y)
                    } else {
                        (a.0 + (b.0 - a.0) * (y - a.1) / (b.1 - a.1), y)
                    }
                };
                if (b.1 - a.1).abs() < f32::EPSILON {
                    take(a.0, a.1);
                    take(b.0, b.1);
                    continue;
                }
                // The edge where it is near the band, at its ends, where
                // it crosses the band and in steps above and below it.
                let (from, to) = (lo.max(ya), hi.min(yb));
                let mut ys = vec![from, to];
                ys.extend([b0, b1].into_iter().filter(|y| (from..=to).contains(y)));
                for k in 1..PROFILE_STEPS {
                    ys.push(from + (to - from) * k as f32 / PROFILE_STEPS as f32);
                }
                for y in ys {
                    let (x, y) = at(y);
                    take(x, y);
                }
            }
            if let Some(span) = span {
                bands.push((b0, b1, span.0, span.1));
            }
            b0 = b1;
        }
        Some(bands.into())
    }
}

/// A floating object of a document, with what it moves with.
#[derive(Clone, Debug)]
pub(crate) struct Floating {
    pub object: Object,
    pub anchor: Place,
}

/// Where a floating object goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// On a page, by index.
    Page(usize),
    /// With a paragraph of the text laid out, by index.
    Paragraph(usize),
}

/// The floating objects of a document. An object whose paragraph is gone
/// stays on the first page.
pub(crate) fn floating(document: &Document) -> Arc<Vec<Floating>> {
    let objects = document.objects();
    if objects.is_empty() {
        return Arc::default();
    }
    let indexes: HashMap<desk_model::NodeId, usize> = document
        .body()
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id(), i))
        .collect();
    let marks = document.object_marks();
    Arc::new(
        objects
            .iter()
            // Objects in the text are placed with their lines, but those
            // that float with their paragraph, and those of the masters on
            // every page of their sections.
            .filter(|object| {
                (object.anchor != Anchor::InText || floats_in_text(object))
                    && !object.anchor.is_master()
            })
            .map(|object| Floating {
                object: object.clone(),
                anchor: match object.anchor {
                    Anchor::Page(page) => Place::Page(page as usize),
                    Anchor::Paragraph(id) => indexes
                        .get(&id)
                        .map_or(Place::Page(0), |&i| Place::Paragraph(i)),
                    Anchor::InText => marks
                        .get(&object.id)
                        .and_then(|at| indexes.get(&at.paragraph))
                        .map_or(Place::Page(0), |&i| Place::Paragraph(i)),
                    Anchor::Master(..) => Place::Page(0),
                },
            })
            .collect(),
    )
}

/// Puts the objects of the master of each section behind everything else
/// on the pages that show the headers and footers of that section, but
/// for the pages left blank (INT-05, EST-16).
pub(crate) fn place_master_objects(document: &Document, layout: &mut crate::DocumentLayout) {
    let masters: Vec<&Object> = document
        .objects()
        .iter()
        .filter(|o| o.anchor.is_master())
        .collect();
    if masters.is_empty() {
        return;
    }
    for index in 0..layout.pages.len() {
        if layout.pages[index].blank {
            continue;
        }
        let section = layout.pages[index].section;
        let key = layout.section_margins.get(section).copied().flatten();
        // The pages of the section it is: the first, a left page or any
        // other, as for its header (EST-19).
        let pages = crate::margin_of_page(layout, index, desk_model::MarginKind::Header)
            .0
            .pages;
        let placed: Vec<PlacedObject> = masters
            .iter()
            .filter(|o| o.anchor == Anchor::Master(key, pages))
            .map(|o| PlacedObject::new(o, None, 0.0))
            .collect();
        layout.pages[index].objects.splice(0..0, placed);
    }
}

/// The bands of a contour down the page, each from a top to a bottom and
/// from a left to a right, in points.
pub(crate) type Profile = Arc<[(f32, f32, f32, f32)]>;

/// How tall the bands of the contour of an object are, in points.
const PROFILE_BAND: f32 = 0.5;

/// How many steps an edge of a contour near a band is taken in.
const PROFILE_STEPS: usize = 8;

/// The room that an object takes from the text, in points on the page:
/// its box, and the bands of its contour when the text wraps tightly
/// around it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Exclusion {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub wrap: Wrap,
    pub profile: Option<Profile>,
}

impl Exclusion {
    /// The left and right of the room it takes from a line from `y0` to
    /// `y1`, or none when it takes none there.
    fn span(&self, y0: f32, y1: f32) -> Option<(f32, f32)> {
        if self.y1 <= y0 || self.y0 >= y1 {
            return None;
        }
        let Some(bands) = &self.profile else {
            return Some((self.x0, self.x1));
        };
        bands
            .iter()
            .filter(|b| b.1 > y0 && b.0 < y1)
            .fold(None, |span, b| {
                Some(span.map_or((b.2, b.3), |(s, t): (f32, f32)| (s.min(b.2), t.max(b.3))))
            })
    }
}

/// The part of a paragraph on one page or column: from a height of the
/// paragraph down, where the paragraph is on the page and what takes room
/// there.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Piece {
    /// The height in the paragraph where the part starts.
    pub from: f32,
    /// The left of the column on the page.
    pub left: f32,
    /// Where the top of the paragraph would be on the page.
    pub origin: f32,
    /// The objects in the way.
    pub exclusions: Vec<Exclusion>,
}

/// A column with objects in it, part by part of a paragraph.
pub(crate) struct WrapSpace<'a> {
    pub width: f32,
    pub pieces: &'a [Piece],
}

impl LineSpace for WrapSpace<'_> {
    fn width(&self) -> f32 {
        self.width
    }

    fn segments(&self, top: f32, height: f32) -> Vec<(f32, f32)> {
        let Some(piece) = self
            .pieces
            .iter()
            .rev()
            .find(|p| p.from <= top + 0.001)
            .or(self.pieces.first())
        else {
            return vec![(0.0, self.width)];
        };
        free_segments(piece, self.width, top, height)
    }
}

/// The pieces of a column of a width that the objects of a part of a
/// paragraph leave free, for a line from `top` that is `height` tall.
fn free_segments(piece: &Piece, width: f32, top: f32, height: f32) -> Vec<(f32, f32)> {
    let (y0, y1) = (piece.origin + top, piece.origin + top + height);
    let mut free = vec![(0.0f32, width)];
    for e in &piece.exclusions {
        let Some((x0, x1)) = e.span(y0, y1) else {
            continue;
        };
        let (a, b) = (x0 - piece.left, x1 - piece.left);
        if b <= 0.0 || a >= width {
            continue;
        }
        let (cut0, cut1) = match e.wrap {
            Wrap::Both => (a, b),
            // The text only on the left of the object.
            Wrap::Left => (a, f32::INFINITY),
            Wrap::Right => (f32::NEG_INFINITY, b),
            Wrap::Largest => {
                if a >= width - b {
                    (a, f32::INFINITY)
                } else {
                    (f32::NEG_INFINITY, b)
                }
            }
            Wrap::AboveAndBelow => (f32::NEG_INFINITY, f32::INFINITY),
            Wrap::Through | Wrap::Behind => continue,
        };
        free = free
            .into_iter()
            .flat_map(|(s, t)| {
                let mut out = Vec::with_capacity(2);
                if cut0 > s {
                    out.push((s, cut0.min(t)));
                }
                if cut1 < t {
                    out.push((cut1.max(s), t));
                }
                out
            })
            .filter(|(s, t)| t > s)
            .collect();
    }
    free.into_iter().map(|(s, t)| (s, t - s)).collect()
}

/// Whether an exclusion is in the way of a column from `left` to `right`
/// below `top` on its page.
pub(crate) fn in_the_way(e: &Exclusion, left: f32, right: f32, top: f32) -> bool {
    e.y1 > top && e.x1 > left && e.x0 < right
}

/// Whether an object in the text hangs from the top of its line instead
/// of standing on its baseline: a text box, as Pages places one (OBJ-14).
fn hangs(object: &Object) -> bool {
    matches!(object.kind, ObjectKind::TextBox(_))
}

/// Whether an object in the text floats with its paragraph instead: a
/// text box that the text wraps around, which Pages centers in the column
/// at the top of its paragraph, and the text of the paragraph goes around
/// it (OBJ-14).
pub(crate) fn floats_in_text(object: &Object) -> bool {
    object.anchor == Anchor::InText
        && hangs(object)
        && !matches!(object.wrap, Wrap::Through | Wrap::Behind)
}

/// The width and height of the objects in the text of a document, by
/// object, for the marks that hold their places (OBJ-01); none when it has
/// none.
pub(crate) fn inline_sizes(
    document: &Document,
) -> Option<Arc<HashMap<desk_model::NodeId, (desk_model::Length, desk_model::Length)>>> {
    let sizes: HashMap<_, _> = document
        .objects()
        .iter()
        .filter(|o| o.anchor == Anchor::InText)
        .map(|o| {
            // A text box in the text hangs from the top of its line, as in
            // Pages, and does not make the line taller; one that floats
            // with its paragraph takes no room in the line (OBJ-14).
            let height = if hangs(o) {
                desk_model::Length::ZERO
            } else {
                o.frame.height
            };
            let width = if floats_in_text(o) {
                desk_model::Length::ZERO
            } else {
                o.frame.width
            };
            (o.id, (width, height))
        })
        .collect();
    (!sizes.is_empty()).then(|| Arc::new(sizes))
}

/// Puts the objects in the text on the pages, where the lines put their
/// marks: from the left of the mark, standing on the baseline (OBJ-01).
pub fn place_inline_objects(document: &Document, layout: &mut DocumentLayout) {
    if inline_sizes(document).is_none() {
        return;
    }
    let indexes: HashMap<desk_model::NodeId, usize> = document
        .body()
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id(), i))
        .collect();
    // The marks of each paragraph, by index: their offsets and objects.
    let mut marks: HashMap<usize, Vec<(usize, &Object)>> = HashMap::new();
    for (id, at) in document.object_marks() {
        if let (Some(&paragraph), Some(object)) =
            (indexes.get(&at.paragraph), document.objects().get(id))
            && object.anchor == Anchor::InText
            && !floats_in_text(object)
        {
            marks
                .entry(paragraph)
                .or_default()
                .push((at.offset, object));
        }
    }
    // Those in the lines of headers, footers and notes (EST-12, EST-13),
    // by the identifier of their paragraph.
    let mut story_marks: HashMap<desk_model::NodeId, Vec<(usize, &Object)>> = HashMap::new();
    for (id, (_, at)) in document.story_object_marks() {
        if let Some(object) = document.objects().get(id)
            && object.anchor == Anchor::InText
        {
            story_marks
                .entry(at.paragraph)
                .or_default()
                .push((at.offset, object));
        }
    }
    let place = |line: &PlacedLine, found: &[(usize, &Object)], paragraph: Option<usize>| {
        found
            .iter()
            .filter(|(offset, _)| line.line.range.contains(offset))
            .map(|&(offset, object)| {
                let mut at = PlacedObject::new(object, paragraph, 0.0);
                at.x = line.origin.0 + line.line.caret_x(offset);
                at.y = if hangs(object) {
                    line.top()
                } else {
                    line.baseline() - at.height
                };
                at
            })
            .collect::<Vec<_>>()
    };
    for index in 0..layout.pages.len() {
        let mut placed = Vec::new();
        let page = &layout.pages[index];
        for line in &page.lines {
            if let Some(found) = marks.get(&line.paragraph) {
                placed.extend(place(line, found, Some(line.paragraph)));
            }
        }
        if !story_marks.is_empty() {
            for (line, margin) in page.margin_lines.iter().zip(&page.margin_places) {
                let (_, section) = crate::margin_of_page(layout, index, margin.kind);
                let paragraphs = document.section_headers_footers(section).get(*margin);
                if let Some(found) = paragraphs
                    .get(line.paragraph)
                    .and_then(|p| story_marks.get(&p.id()))
                {
                    placed.extend(place(line, found, None));
                }
            }
            for (line, part) in page.note_lines.iter().zip(&page.note_parts) {
                let found = layout
                    .notes
                    .get(line.paragraph)
                    .and_then(|id| document.notes().get(*id))
                    .and_then(|note| note.paragraphs.get(*part))
                    .and_then(|p| story_marks.get(&p.id()));
                if let Some(found) = found {
                    placed.extend(place(line, found, None));
                }
            }
        }
        layout.pages[index].objects.extend(placed);
    }
}

/// Lays out the text of the text boxes on the pages of a layout (OBJ-13):
/// inside the frame less its inset, from the top, as a column without a
/// bottom.
pub fn place_object_text(
    document: &Document,
    layout: &mut DocumentLayout,
    system: &mut TextSystem,
) {
    // Where each object is placed, to find the boxes linked to a first.
    let mut at: HashMap<desk_model::NodeId, (usize, usize)> = HashMap::new();
    for (p, page) in layout.pages.iter().enumerate() {
        for (i, placed) in page.objects.iter().enumerate() {
            at.insert(placed.object.id, (p, i));
        }
    }
    // Pages takes from the text of a box the room of the objects above it
    // on its page that the text wraps around, other than the boxes linked
    // to it.
    for page in &mut layout.pages {
        let objects = &page.objects;
        let wraps: Vec<Arc<[Exclusion]>> = (0..objects.len())
            .map(|i| {
                if !matches!(objects[i].object.kind, ObjectKind::TextBox(_)) {
                    return Arc::from([]);
                }
                let head = document.text_box_head(objects[i].object.id);
                objects[i + 1..]
                    .iter()
                    .filter(|o| {
                        !matches!(o.object.kind, ObjectKind::TextBox(_))
                            || document.text_box_head(o.object.id) != head
                    })
                    .filter_map(PlacedObject::exclusion)
                    .collect()
            })
            .collect();
        for (placed, wraps) in page.objects.iter_mut().zip(wraps) {
            placed.wraps = wraps;
        }
    }
    for p in 0..layout.pages.len() {
        for i in 0..layout.pages[p].objects.len() {
            let placed = &layout.pages[p].objects[i];
            let id = placed.object.id;
            let ObjectKind::TextBox(text) = &placed.object.kind else {
                lay_out_text(document, &mut layout.pages[p].objects[i], system);
                continue;
            };
            if document.objects().get(id).is_none() || document.text_box_head(id) != id {
                continue;
            }
            let paragraphs = text.paragraphs.clone();
            let chain: Vec<(usize, usize)> = document
                .text_box_chain(id)
                .iter()
                .filter_map(|b| at.get(b).copied())
                .collect();
            let boxes: Vec<PlacedObject> = chain
                .iter()
                .map(|&(p, i)| layout.pages[p].objects[i].clone())
                .collect();
            let refs: Vec<&PlacedObject> = boxes.iter().collect();
            let flowed = flow_text_boxes(document, &refs, &paragraphs, system);
            for ((p, i), lines) in chain.into_iter().zip(flowed) {
                layout.pages[p].objects[i].lines = lines;
            }
        }
    }
}

fn lay_out_text(document: &Document, placed: &mut PlacedObject, system: &mut TextSystem) {
    for child in &mut placed.children {
        lay_out_text(document, child, system);
    }
    let Some(text) = placed.object.text_box() else {
        return;
    };
    let paragraphs = text.paragraphs.clone();
    placed.lines = text_box_lines(document, placed, &paragraphs, system);
}

/// The lines of paragraphs laid out in a text box placed on a page, as if
/// it were not rotated, with the styles of a document: those of the text
/// box, or those being edited in it (OBJ-13).
pub fn text_box_lines(
    document: &Document,
    placed: &PlacedObject,
    paragraphs: &[desk_model::Paragraph],
    system: &mut TextSystem,
) -> Vec<PlacedLine> {
    flow_text_boxes(document, &[placed], paragraphs, system)
        .pop()
        .unwrap_or_default()
}

/// The room of a text box for its text: its columns, each by its left,
/// top and width, the height of each, and where the text sits.
struct BoxRoom {
    columns: Vec<(f32, f32, f32)>,
    height: f32,
    vertical: VerticalAlignment,
}

fn box_room(placed: &PlacedObject) -> BoxRoom {
    let (inset, vertical, columns, gap) = match placed.object.text_box() {
        Some(text) => (
            text.inset.to_pt() as f32,
            text.vertical,
            usize::from(text.columns.max(1)),
            text.column_gap.to_pt() as f32,
        ),
        // The text about to be typed in a shape.
        None if placed.object.holds_text() => (4.0, VerticalAlignment::Middle, 1, 0.0),
        None => (0.0, VerticalAlignment::Top, 1, 0.0),
    };
    // The text of an oval goes in the rectangle inside it.
    let (inset_x, inset_y) = if matches!(placed.object.kind, ObjectKind::Shape(Shape::Ellipse)) {
        let side = (1.0 - std::f32::consts::FRAC_1_SQRT_2) / 2.0;
        (inset + placed.width * side, inset + placed.height * side)
    } else {
        (inset, inset)
    };
    let inner = (placed.width - 2.0 * inset_x).max(1.0);
    let height = (placed.height - 2.0 * inset_y).max(1.0);
    let gaps = gap * (columns - 1) as f32;
    // Columns narrower than a few characters are not columns any more.
    let columns = if (inner - gaps) / columns as f32 >= desk_text::MIN_SEGMENT_WIDTH {
        columns
    } else {
        1
    };
    let gaps = if columns > 1 { gaps } else { 0.0 };
    let width = ((inner - gaps) / columns as f32).max(1.0);
    let left = placed.x + inset_x;
    let top = placed.y + inset_y;
    BoxRoom {
        columns: (0..columns)
            .map(|c| (left + c as f32 * (width + gap), top, width))
            .collect(),
        height,
        vertical,
    }
}

/// The lines of paragraphs laid out through linked text boxes, in order:
/// each line goes in the next column, or the next box, when it does not
/// fit, and the last column of the last box takes what is left (OBJ-13).
pub fn flow_text_boxes(
    document: &Document,
    boxes: &[&PlacedObject],
    paragraphs: &[desk_model::Paragraph],
    system: &mut TextSystem,
) -> Vec<Vec<PlacedLine>> {
    if let [placed] = boxes
        && let Some(space) = ShapeSpace::of(placed)
    {
        return vec![shape_lines(document, placed, &space, paragraphs, system)];
    }
    let rooms: Vec<BoxRoom> = boxes.iter().map(|b| box_room(b)).collect();
    let slots: Vec<(usize, (f32, f32, f32))> = rooms
        .iter()
        .enumerate()
        .flat_map(|(b, room)| room.columns.iter().map(move |c| (b, *c)))
        .collect();
    let mut out: Vec<Vec<PlacedLine>> = vec![Vec::new(); boxes.len()];
    if slots.is_empty() {
        return out;
    }
    let styles = document.styles();
    let settings = document.text_settings();
    let items = styles.list_items(paragraphs.iter());
    // Where the next line goes: its slot, and its top from the top of the
    // column.
    let mut slot = 0;
    let mut y = 0.0f32;
    let mut after = 0.0f32;
    for (i, paragraph) in paragraphs.iter().enumerate() {
        let text = ParagraphText::new_in_document_with(
            styles,
            settings,
            paragraph,
            items[i].as_ref(),
            Default::default(),
        );
        let format = text.paragraph();
        if i > 0 && y > 0.0 {
            y += after.max(format.space_before.to_pt() as f32);
        }
        after = format.space_after.to_pt() as f32;
        // Linked boxes may have columns of other widths; the paragraph is
        // laid out at the width of the column it starts in.
        let width = slots[slot].1.2;
        // Where the paragraph is in each column it goes through, for the
        // objects that its text goes around.
        let piece = |slot: usize, from: f32, origin: f32| {
            let (b, (left, top, _)) = slots[slot];
            Piece {
                from,
                left,
                origin: top + origin,
                exclusions: boxes[b].wraps.to_vec(),
            }
        };
        let mut pieces = vec![piece(slot, f32::NEG_INFINITY, y)];
        let lay_out = |system: &mut TextSystem, pieces: &[Piece]| {
            if pieces.iter().all(|p| p.exclusions.is_empty()) {
                system.layout(&text, &Column::new(width), 0.0).lines
            } else {
                let space = WrapSpace { width, pieces };
                system.layout(&text, &space, 0.0).lines
            }
        };
        let mut laid = lay_out(system, &pieces);
        let first_top = laid.first().map_or(0.0, |l| l.top);
        // The lines of the paragraph keep their spacing from its first.
        let mut start = y - first_top;
        let mut n = 0;
        // The lines of the paragraph in the column it is in now, and
        // whether the paragraph starts there below other text.
        let mut here = 0;
        let mut below_text = y > 0.0;
        while let Some(line) = laid.get(n).cloned() {
            let mut line_top = start + line.top;
            let bottom = line_top + line.height;
            // A line that does not fit goes to the top of the next column,
            // unless it is the first of its column or the last column.
            let height = rooms[slots[slot].0].height;
            if bottom > height && line_top > 0.0 && slot + 1 < slots.len() {
                // With widow control, the first line of a paragraph is not
                // left alone at the bottom of a column, nor its last at the
                // top of the next: the line before goes with it (EST-05).
                let mut back = 0;
                if format.widow_control {
                    if laid.len() - n == 1 && here >= 2 {
                        back = 1;
                    }
                    if here - back == 1 && below_text {
                        back = here;
                    }
                }
                if back > 0 {
                    let b = slots[slot].0;
                    let keep = out[b].len() - back;
                    out[b].truncate(keep);
                    n -= back;
                    let line = &laid[n];
                    slot += 1;
                    start = -line.top;
                    here = 0;
                    below_text = false;
                    if !pieces.iter().all(|p| p.exclusions.is_empty())
                        || !piece(slot, line.top, start).exclusions.is_empty()
                    {
                        let from = line.top;
                        pieces.retain(|p| p.from < from);
                        pieces.push(piece(slot, from, start));
                        laid = lay_out(system, &pieces);
                        let Some(line) = laid.get(n) else { break };
                        start = -line.top;
                    }
                    continue;
                }
                here = 0;
                below_text = false;
                slot += 1;
                start = -line.top;
                line_top = 0.0;
                // The rest of the paragraph goes around what is in the
                // next column.
                let next = piece(slot, line.top, start);
                if !next.exclusions.is_empty() || pieces.iter().any(|p| !p.exclusions.is_empty()) {
                    pieces.retain(|p| p.from < line.top);
                    pieces.push(next);
                    laid = lay_out(system, &pieces);
                    let Some(line) = laid.get(n) else { break };
                    start = -line.top;
                    continue;
                }
            }
            let (b, (x, top, _)) = slots[slot];
            y = line_top + line.height;
            out[b].push(PlacedLine {
                paragraph: i,
                index: n,
                line,
                origin: (x, top + start),
            });
            n += 1;
            here += 1;
        }
    }
    // Text that does not fill a box sits where its alignment says. Pages
    // centers the text from the top of its first line to the baseline of
    // its last, without the descent below it (FMT-19).
    for (b, lines) in out.iter_mut().enumerate() {
        let top = rooms[b].columns.first().map_or(0.0, |c| c.1);
        let lowest = lines
            .iter()
            .map(|l| l.top() + l.line.height - top)
            .fold(0.0f32, f32::max);
        let room = (rooms[b].height - lowest).max(0.0);
        let below = lines
            .iter()
            .max_by(|a, c| a.bottom().total_cmp(&c.bottom()))
            .map_or(0.0, |l| {
                l.top() + l.line.height - (l.origin.1 + l.line.baseline)
            });
        let shift = match rooms[b].vertical {
            VerticalAlignment::Top => 0.0,
            VerticalAlignment::Middle => ((room + below) / 2.0).min(rooms[b].height),
            VerticalAlignment::Bottom => room,
        };
        if shift > 0.0 {
            for line in lines {
                line.origin.1 += shift;
            }
        }
    }
    out
}

/// The narrowest room of a line in a shape, half a point above the line:
/// Pages moves the text down until a line has at least this much
/// (OBJ-12).
const SHAPE_MIN_WIDTH: f32 = 21.25;

/// The room for the text inside a shape, as Pages lays it out (OBJ-12):
/// each line takes what the outline of the shape leaves free across its
/// height, its inset and half its stroke in from the outline, and the
/// text starts at the top and stops where no line fits any more. Points
/// are from the top left corner of the frame, and the column starts at
/// the inset.
struct ShapeSpace {
    edges: Vec<((f32, f32), (f32, f32))>,
    inset: f32,
    width: f32,
    height: f32,
    vertical: VerticalAlignment,
}

impl ShapeSpace {
    /// The room of a shape with text, but a text box.
    fn of(placed: &PlacedObject) -> Option<Self> {
        let ObjectKind::Shape(shape) = &placed.object.kind else {
            return None;
        };
        let text = placed.object.text_box()?;
        let (w, h) = (f64::from(placed.width), f64::from(placed.height));
        let point = |(x, y): (f64, f64)| (x as f32, y as f32);
        let edges: Vec<_> = desk_model::flattened(&shape.outline(w, h))
            .into_iter()
            .map(|(a, b)| (point(a), point(b)))
            .collect();
        if edges.is_empty() {
            return None;
        }
        let stroke = placed
            .object
            .style
            .stroke
            .as_ref()
            .map_or(0.0, |s| s.width.to_pt() as f32 / 2.0);
        let inset = text.inset.to_pt() as f32 + stroke;
        Some(Self {
            edges,
            inset,
            width: (placed.width - 2.0 * inset).max(1.0),
            height: placed.height,
            vertical: text.vertical,
        })
    }

    /// The pieces of a line across the shape at height `y` that are at
    /// least the inset from its outline, from left to right.
    fn across(&self, y: f32) -> Vec<(f32, f32)> {
        let mut crossings: Vec<f32> = self
            .edges
            .iter()
            .filter(|((_, y0), (_, y1))| (*y0 > y) != (*y1 > y))
            .map(|&((x0, y0), (x1, y1))| x0 + (y - y0) * (x1 - x0) / (y1 - y0))
            .collect();
        crossings.sort_by(f32::total_cmp);
        let mut free: Vec<(f32, f32)> = crossings
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[u, v]| (u, v))
            .collect();
        let d = self.inset;
        for &(a, b) in &self.edges {
            // What is within the inset of the edge, on this line: the
            // edge widened into a capsule, which is convex.
            let Some((cut0, cut1)) = capsule_span(a, b, d, y) else {
                continue;
            };
            free = free
                .into_iter()
                .flat_map(|(s, t)| {
                    let mut out = Vec::with_capacity(2);
                    if cut0 > s {
                        out.push((s, cut0.min(t)));
                    }
                    if cut1 < t {
                        out.push((cut1.max(s), t));
                    }
                    out
                })
                .filter(|(s, t)| t > s)
                .collect();
        }
        free
    }
}

/// Where a horizontal line at `y` meets the points within `d` of the
/// segment from `a` to `b`, if it does.
fn capsule_span(a: (f32, f32), b: (f32, f32), d: f32, y: f32) -> Option<(f32, f32)> {
    let mut span: Option<(f32, f32)> = None;
    let mut add = |x0: f32, x1: f32| {
        span = Some(span.map_or((x0, x1), |(s, t)| (s.min(x0), t.max(x1))));
    };
    for (cx, cy) in [a, b] {
        let dy = y - cy;
        if dy.abs() <= d {
            let dx = (d * d - dy * dy).sqrt();
            add(cx - dx, cx + dx);
        }
    }
    // The band along the segment: a parallelogram, its sides the
    // segment moved by `d` either way.
    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
    let length = (ex * ex + ey * ey).sqrt();
    if length > 0.0 {
        let (nx, ny) = (-ey / length * d, ex / length * d);
        let corners = [
            (a.0 + nx, a.1 + ny),
            (b.0 + nx, b.1 + ny),
            (b.0 - nx, b.1 - ny),
            (a.0 - nx, a.1 - ny),
        ];
        let mut xs = Vec::new();
        for i in 0..4 {
            let (p, q) = (corners[i], corners[(i + 1) % 4]);
            if (p.1 > y) != (q.1 > y) {
                xs.push(p.0 + (y - p.1) * (q.0 - p.0) / (q.1 - p.1));
            } else if p.1 == y {
                xs.push(p.0);
            }
        }
        if let (Some(lo), Some(hi)) = (
            xs.iter().copied().reduce(f32::min),
            xs.iter().copied().reduce(f32::max),
        ) {
            add(lo, hi);
        }
    }
    span
}

impl LineSpace for ShapeSpace {
    fn width(&self) -> f32 {
        self.width
    }

    fn segments(&self, top: f32, height: f32) -> Vec<(f32, f32)> {
        if top + height > self.height - self.inset + 0.01 || top < self.inset - 0.01 {
            return Vec::new();
        }
        // Pages measures whether a line has room half a point above it.
        if !self
            .across((top - 0.5).max(self.inset + 0.01))
            .iter()
            .any(|(s, t)| t - s >= SHAPE_MIN_WIDTH)
        {
            return Vec::new();
        }
        // What is free all across the height of the line.
        let steps = (height * 2.0).ceil().max(1.0) as usize;
        let mut free = self.across(top);
        for k in 1..=steps {
            let y = (top + height * k as f32 / steps as f32).min(self.height);
            let row = self.across(y);
            free = free
                .into_iter()
                .flat_map(|(s, t)| {
                    row.iter()
                        .map(move |&(u, v)| (s.max(u), t.min(v)))
                        .filter(|&(u, v)| u < v)
                })
                .collect();
        }
        free.into_iter()
            .map(|(s, t)| (s - self.inset, t - s))
            .collect()
    }

    fn is_exhausted(&self, top: f32) -> bool {
        top > self.height - self.inset
    }
}

/// The lines of the text of a shape, from the top of its room down, as
/// many as fit (OBJ-12). Text aligned to the middle or the bottom that
/// fits is laid out again lower, by half or all of the room left below
/// it; text that does not fit then starts a point lower than its first
/// line, as Pages lays it out.
fn shape_lines(
    document: &Document,
    placed: &PlacedObject,
    space: &ShapeSpace,
    paragraphs: &[desk_model::Paragraph],
    system: &mut TextSystem,
) -> Vec<PlacedLine> {
    let (lines, end, fits) = shape_lines_from(
        document,
        placed,
        space,
        paragraphs,
        system,
        space.inset.ceil(),
    );
    let start = lines.first().map_or(space.inset.ceil(), |l| l.line.top);
    let room = (space.height - space.inset - end).max(0.0);
    let lower = match (space.vertical, fits) {
        (VerticalAlignment::Top, _) => 0.0,
        (_, false) => 1.0,
        (VerticalAlignment::Middle, true) => (room / 2.0).floor(),
        (VerticalAlignment::Bottom, true) => room.floor(),
    };
    if lower > 0.0 {
        return shape_lines_from(document, placed, space, paragraphs, system, start + lower).0;
    }
    lines
}

/// The lines of the text of a shape from `y` down, where the last ends,
/// and whether all the text fits.
fn shape_lines_from(
    document: &Document,
    placed: &PlacedObject,
    space: &ShapeSpace,
    paragraphs: &[desk_model::Paragraph],
    system: &mut TextSystem,
    mut y: f32,
) -> (Vec<PlacedLine>, f32, bool) {
    let styles = document.styles();
    let settings = document.text_settings();
    let items = styles.list_items(paragraphs.iter());
    let origin = (placed.x + space.inset, placed.y);
    let mut out = Vec::new();
    let mut after = 0.0f32;
    for (i, paragraph) in paragraphs.iter().enumerate() {
        let text = ParagraphText::new_in_document_with(
            styles,
            settings,
            paragraph,
            items[i].as_ref(),
            Default::default(),
        );
        let format = text.paragraph();
        if i > 0 {
            y += after.max(format.space_before.to_pt() as f32);
        }
        after = format.space_after.to_pt() as f32;
        let laid = system.layout(&text, space, y).lines;
        let Some(last) = laid.last() else {
            return (out, y, false);
        };
        y = last.top + last.height;
        let cut = last.range.end < text.to_model(text.text().len());
        for (n, line) in laid.into_iter().enumerate() {
            out.push(PlacedLine {
                paragraph: i,
                index: n,
                line,
                origin,
            });
        }
        if cut || (i + 1 < paragraphs.len() && space.is_exhausted(y)) {
            return (out, y, false);
        }
    }
    (out, y, true)
}

/// How far a point is from a segment.
fn distance_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length).clamp(0.0, 1.0)
    };
    let (x, y) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - x).powi(2) + (p.1 - y).powi(2)).sqrt()
}

/// The frontmost object on a page under a point, by its index among the
/// objects of the page, if any is (OBJ-21). Objects behind the text, and
/// those of the master of the section, are found only when `behind` is
/// set, as Pages selects them only when nothing else is under the
/// pointer.
pub fn object_at(
    objects: &[PlacedObject],
    x: f32,
    y: f32,
    tolerance: f32,
    behind: bool,
) -> Option<usize> {
    objects
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, o)| behind || (o.object.wrap != Wrap::Behind && !o.object.anchor.is_master()))
        .find(|(_, o)| o.contains(x, y, tolerance))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(wrap: Wrap) -> Piece {
        Piece {
            from: f32::NEG_INFINITY,
            left: 100.0,
            origin: 50.0,
            exclusions: vec![Exclusion {
                x0: 200.0,
                y0: 100.0,
                x1: 300.0,
                y1: 200.0,
                wrap,
                profile: None,
            }],
        }
    }

    #[test]
    fn obj_17_text_wraps_tightly_around_a_contour() {
        use desk_model::{Frame, NodeId, Object, ObjectKind, Shape};
        let mut object = Object::new(
            NodeId::new(1).unwrap(),
            ObjectKind::Shape(Shape::triangle()),
            Frame::from_pt(200.0, 100.0, 100.0, 100.0),
        );
        object.wrap_margin = desk_model::Length::ZERO;
        object.contour = object.shape_contour();
        let placed = PlacedObject::new(&object, None, 0.0);
        let e = placed.exclusion().unwrap();
        // Near the point of the triangle, little room is taken; near its
        // base, all its width.
        let (a, b) = e.span(100.0, 102.0).unwrap();
        assert!(a > 245.0 && b < 255.0, "{a} {b}");
        let (a, b) = e.span(196.0, 198.0).unwrap();
        assert!(a < 203.0 && b > 297.0, "{a} {b}");
        assert_eq!(e.span(201.0, 210.0), None);
        // Without a contour, the box.
        object.contour = None;
        let e = PlacedObject::new(&object, None, 0.0).exclusion().unwrap();
        assert_eq!(e.span(100.0, 102.0), Some((200.0, 300.0)));
    }

    #[test]
    fn obj_17_the_margin_of_a_contour_is_round_and_from_the_outside_of_its_stroke() {
        use desk_model::{Frame, Length, NodeId, Object, ObjectKind, Shape, Stroke};
        let mut object = Object::new(
            NodeId::new(1).unwrap(),
            ObjectKind::Shape(Shape::Rectangle),
            Frame::from_pt(200.0, 100.0, 100.0, 100.0),
        );
        object.wrap_margin = Length::from_points(12);
        object.style.stroke = Some(Stroke::new(desk_model::Rgba::BLACK, Length::from_points(1)));
        object.contour = object.shape_contour();
        let e = PlacedObject::new(&object, None, 0.0).exclusion().unwrap();
        // Beside the rectangle, its margin and half its stroke.
        let (a, b) = e.span(150.0, 151.0).unwrap();
        assert!(
            (a - 187.5).abs() < 0.01 && (b - 312.5).abs() < 0.01,
            "{a} {b}"
        );
        // Ten points below it, the corners of the margin are round, as
        // Pages draws them: 7.5 points at their side.
        let (a, b) = e.span(210.0, 211.0).unwrap();
        assert!(
            (a - 192.5).abs() < 0.3 && (b - 307.5).abs() < 0.3,
            "{a} {b}"
        );
        assert_eq!(e.span(213.0, 220.0), None);
    }

    #[test]
    fn obj_17_wraps_leave_the_sides_they_say() {
        let width = 400.0;
        // Above the object, the whole column.
        assert_eq!(
            free_segments(&piece(Wrap::Both), width, 0.0, 20.0),
            vec![(0.0, 400.0)]
        );
        // Beside it, both sides, one side or none.
        assert_eq!(
            free_segments(&piece(Wrap::Both), width, 60.0, 20.0),
            vec![(0.0, 100.0), (200.0, 200.0)]
        );
        assert_eq!(
            free_segments(&piece(Wrap::Left), width, 60.0, 20.0),
            vec![(0.0, 100.0)]
        );
        assert_eq!(
            free_segments(&piece(Wrap::Right), width, 60.0, 20.0),
            vec![(200.0, 200.0)]
        );
        assert_eq!(
            free_segments(&piece(Wrap::Largest), width, 60.0, 20.0),
            vec![(200.0, 200.0)]
        );
        assert!(free_segments(&piece(Wrap::AboveAndBelow), width, 60.0, 20.0).is_empty());
        assert_eq!(
            free_segments(&piece(Wrap::Through), width, 60.0, 20.0),
            vec![(0.0, 400.0)]
        );
    }

    #[test]
    fn obj_17_rotated_objects_take_their_bounding_box() {
        let mut object = Object::new(
            desk_model::NodeId::new(1).unwrap(),
            ObjectKind::Shape(desk_model::Shape::Rectangle),
            desk_model::Frame::from_pt(100.0, 100.0, 100.0, 50.0),
        );
        object.rotation = 9000;
        let placed = PlacedObject::new(&object, None, 0.0);
        let (x0, y0, x1, y1) = placed.bounds();
        assert!((x0 - 125.0).abs() < 0.01 && (x1 - 175.0).abs() < 0.01);
        assert!((y0 - 75.0).abs() < 0.01 && (y1 - 175.0).abs() < 0.01);
    }

    #[test]
    fn obj_21_objects_are_hit_where_they_are_drawn() {
        use desk_model::{Frame, NodeId, Shape};
        let shape = |id: u64, shape: Shape, rotation: i32| {
            let mut object = Object::new(
                NodeId::new(id).unwrap(),
                ObjectKind::Shape(shape),
                Frame::from_pt(100.0, 100.0, 100.0, 20.0),
            );
            object.rotation = rotation;
            PlacedObject::new(&object, None, 0.0)
        };
        let flat = shape(1, Shape::Rectangle, 0);
        assert!(flat.contains(190.0, 115.0, 0.0));
        assert!(!flat.contains(150.0, 150.0, 0.0));
        // Turned a quarter, it stands from 60 to 160 down the page.
        let standing = shape(2, Shape::Rectangle, 9000);
        assert!(standing.contains(150.0, 150.0, 0.0));
        assert!(!standing.contains(190.0, 115.0, 0.0));
        let (fx, fy) = standing.to_frame(150.0, 65.0);
        let (px, py) = standing.to_page(fx, fy);
        assert!((px - 150.0).abs() < 1e-3 && (py - 65.0).abs() < 1e-3);
        // Counterclockwise: the right end of the frame goes to the top.
        assert!(fx > 90.0 && (fy - 10.0).abs() < 1e-3);
        // A line is hit near it, not anywhere in its frame.
        let line = shape(3, Shape::Line { rising: false }, 0);
        assert!(line.contains(150.0, 111.0, 2.0));
        assert!(!line.contains(110.0, 118.0, 2.0));
        let objects = [flat, line];
        assert_eq!(object_at(&objects, 150.0, 110.0, 2.0, false), Some(1));
        assert_eq!(object_at(&objects, 110.0, 118.0, 2.0, false), Some(0));
        assert_eq!(object_at(&objects, 10.0, 10.0, 2.0, false), None);
        // A bent connection line is hit along its curve (OBJ-31).
        let (frame, bent) = desk_model::Connection::between(
            (100.0, 120.0),
            (200.0, 120.0),
            desk_model::Length::from_points(20),
            None,
            None,
        );
        let bent = PlacedObject::new(
            &Object::new(
                NodeId::new(4).unwrap(),
                Shape::Connection(bent).into(),
                frame,
            ),
            None,
            0.0,
        );
        assert!(bent.contains(150.0, 100.0, 2.0));
        assert!(!bent.contains(150.0, 119.0, 2.0));
    }
}
