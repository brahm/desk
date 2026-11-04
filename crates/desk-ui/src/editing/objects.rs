// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Objects selected, moved, resized and rotated on the pages, as in Pages
//! (OBJ-03, OBJ-04, OBJ-05, OBJ-06, OBJ-07).
//!
//! A click on an object selects it, with its outline and eight handles;
//! Shift or Ctrl and a click add it to the selection or take it out. A
//! drag of an object moves the selected ones, snapping to the edges and
//! the middle of the page and of the other objects, with yellow guides;
//! Shift keeps it straight across or down. A drag of a handle resizes the
//! object, keeping its proportions with Shift; with Ctrl it rotates the
//! object around its center, by steps of 45 degrees with Shift. The drag
//! shows the outline where the objects go, and becomes one action of the
//! history when it ends. The arrows nudge the selected objects by a point,
//! or ten with Shift; Delete removes them and Escape deselects them.

use std::sync::Arc;

use desk_layout::{DocumentLayout, PlacedObject, object_at};
use desk_model::{
    Anchor, Distribution, Edit, Fill, Frame, Layer, Length, ModelError, NodeId, Object,
    ObjectAlignment, ObjectKind, ParaAttrs, Paragraph, Rgba, Run, Shape, Story, Stroke, TextBox,
    Wrap,
};
use gtk::gdk;

use super::TextEditor;
use crate::view::{Guide, ObjectMark};

/// How near the pointer has to be to a handle or a line, in pixels.
const REACH: f64 = 5.0;
/// How near an edge has to be to another for a drag to snap it, in
/// pixels.
const SNAP: f64 = 6.0;
/// The smallest side an object is resized to, in points.
const SMALLEST: f32 = 1.0;

/// A handle of a selected object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    /// The start of a line, at the left of its frame.
    Start,
    /// The end of a line, at the right of its frame.
    End,
}

const BOX_HANDLES: [Handle; 8] = [
    Handle::TopLeft,
    Handle::Top,
    Handle::TopRight,
    Handle::Right,
    Handle::BottomRight,
    Handle::Bottom,
    Handle::BottomLeft,
    Handle::Left,
];

/// Whether an object is a line, and whether it rises.
fn line(object: &Object) -> Option<bool> {
    match object.kind {
        ObjectKind::Shape(Shape::Line { rising }) => Some(rising),
        _ => None,
    }
}

/// The ends of a line, in the coordinates of its frame.
fn line_ends(placed: &PlacedObject, rising: bool) -> ((f32, f32), (f32, f32)) {
    let (w, h) = (placed.width, placed.height);
    if rising {
        ((0.0, h), (w, 0.0))
    } else {
        ((0.0, 0.0), (w, h))
    }
}

/// Where a handle is, in the coordinates of the frame.
fn handle_spot(placed: &PlacedObject, handle: Handle) -> (f32, f32) {
    let (w, h) = (placed.width, placed.height);
    let rising = line(&placed.object).unwrap_or(false);
    let (start, end) = line_ends(placed, rising);
    match handle {
        Handle::TopLeft => (0.0, 0.0),
        Handle::Top => (w / 2.0, 0.0),
        Handle::TopRight => (w, 0.0),
        Handle::Right => (w, h / 2.0),
        Handle::BottomRight => (w, h),
        Handle::Bottom => (w / 2.0, h),
        Handle::BottomLeft => (0.0, h),
        Handle::Left => (0.0, h / 2.0),
        Handle::Start => start,
        Handle::End => end,
    }
}

/// The handles of an object, with where they are on the page.
pub(super) fn handles(placed: &PlacedObject) -> Vec<(Handle, (f32, f32))> {
    let list: &[Handle] = if line(&placed.object).is_some() {
        &[Handle::Start, Handle::End]
    } else {
        &BOX_HANDLES
    };
    list.iter()
        .map(|&h| {
            let (fx, fy) = handle_spot(placed, h);
            (h, placed.to_page(fx, fy))
        })
        .collect()
}

/// The outline and the handles of a selected object.
pub(super) fn mark(page: usize, placed: &PlacedObject) -> ObjectMark {
    let to_page = |(x, y): (f32, f32)| {
        let (x, y) = placed.to_page(x, y);
        (f64::from(x), f64::from(y))
    };
    let (w, h) = (placed.width, placed.height);
    let outline = match line(&placed.object) {
        Some(rising) => {
            let (a, b) = line_ends(placed, rising);
            vec![to_page(a), to_page(b)]
        }
        None => [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
            .into_iter()
            .map(to_page)
            .collect(),
    };
    ObjectMark {
        page,
        outline,
        handles: handles(placed)
            .into_iter()
            .map(|(_, (x, y))| (f64::from(x), f64::from(y)))
            .collect(),
        locked: placed.object.locked,
    }
}

/// An object as a drag leaves it: its frame on the page, as it is placed,
/// and how it is rotated.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Placement {
    pub id: NodeId,
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: i32,
    /// Whether a line rises, which dragging one of its ends can change.
    pub rising: Option<bool>,
}

impl Placement {
    fn of(page: usize, placed: &PlacedObject) -> Self {
        Self {
            id: placed.object.id,
            page,
            x: placed.x,
            y: placed.y,
            width: placed.width,
            height: placed.height,
            rotation: placed.object.rotation,
            rising: line(&placed.object),
        }
    }

    /// The object placed this way, to draw its outline.
    fn placed(&self, original: &PlacedObject) -> PlacedObject {
        let mut placed = original.clone();
        placed.x = self.x;
        placed.y = self.y;
        placed.width = self.width;
        placed.height = self.height;
        placed.object.rotation = self.rotation;
        if let Some(rising) = self.rising {
            placed.object.kind = ObjectKind::Shape(Shape::Line { rising });
        }
        placed
    }
}

/// An object resized by dragging a handle to a point of the page: the
/// handles on the other side stay where they are, as the object is
/// rotated. With `keep`, a corner keeps the proportions of the object.
pub(super) fn resize(
    placed: &PlacedObject,
    handle: Handle,
    point: (f32, f32),
    keep: bool,
) -> (f32, f32, f32, f32, Option<bool>) {
    let (fx, fy) = placed.to_frame(point.0, point.1);
    let (w, h) = (placed.width, placed.height);
    let (mut l, mut t, mut r, mut b) = (0.0, 0.0, w, h);
    let mut rising = None;
    match handle {
        Handle::Start | Handle::End => {
            let (start, end) = line_ends(placed, line(&placed.object).unwrap_or(false));
            let (a, z) = if handle == Handle::Start {
                ((fx, fy), end)
            } else {
                (start, (fx, fy))
            };
            l = a.0.min(z.0);
            r = a.0.max(z.0);
            t = a.1.min(z.1);
            b = a.1.max(z.1);
            // The end further left is the lower one when it rises.
            let (left, right) = if a.0 <= z.0 { (a, z) } else { (z, a) };
            rising = Some(left.1 > right.1);
        }
        _ => {
            let moves_left = matches!(handle, Handle::TopLeft | Handle::Left | Handle::BottomLeft);
            let moves_right = matches!(
                handle,
                Handle::TopRight | Handle::Right | Handle::BottomRight
            );
            let moves_top = matches!(handle, Handle::TopLeft | Handle::Top | Handle::TopRight);
            let moves_bottom = matches!(
                handle,
                Handle::BottomLeft | Handle::Bottom | Handle::BottomRight
            );
            if moves_left {
                l = fx.min(r - SMALLEST);
            }
            if moves_right {
                r = fx.max(l + SMALLEST);
            }
            if moves_top {
                t = fy.min(b - SMALLEST);
            }
            if moves_bottom {
                b = fy.max(t + SMALLEST);
            }
            let corner = (moves_left || moves_right) && (moves_top || moves_bottom);
            if keep && corner && w > 0.0 && h > 0.0 {
                let scale = ((r - l) / w).max((b - t) / h);
                let (nw, nh) = (w * scale, h * scale);
                if moves_left {
                    l = r - nw;
                } else {
                    r = l + nw;
                }
                if moves_top {
                    t = b - nh;
                } else {
                    b = t + nh;
                }
            }
        }
    }
    let (nw, nh) = (r - l, b - t);
    let (cx, cy) = placed.to_page((l + r) / 2.0, (t + b) / 2.0);
    (cx - nw / 2.0, cy - nh / 2.0, nw, nh, rising)
}

/// The rotation of an object turned by dragging from one point of the
/// page to another around its center, in hundredths of a degree; by
/// steps of 45 degrees with `steps`.
pub(super) fn rotate(placed: &PlacedObject, from: (f32, f32), to: (f32, f32), steps: bool) -> i32 {
    let (cx, cy) = placed.center();
    // Counterclockwise, on a page whose heights go down.
    let angle = |(x, y): (f32, f32)| (cy - y).atan2(x - cx);
    let turn = (angle(to) - angle(from)).to_degrees();
    let mut rotation = placed.object.rotation + (turn * 100.0).round() as i32;
    if steps {
        rotation = (rotation as f32 / 4500.0).round() as i32 * 4500;
    }
    rotation.rem_euclid(36_000)
}

/// The edges and middles that a drag snaps to, across and down, on a
/// page: those of the paper and of its margins, and those of the objects
/// that are not dragged.
pub(super) struct Snaps {
    across: Vec<f32>,
    down: Vec<f32>,
    width: f32,
    height: f32,
}

impl Snaps {
    pub fn new(layout: &DocumentLayout, page: usize, moving: &[NodeId]) -> Self {
        let width = layout.setup.width.to_pt() as f32;
        let height = layout.setup.height.to_pt() as f32;
        let mut across = vec![0.0, width / 2.0, width];
        let mut down = vec![0.0, height / 2.0, height];
        if let Some(page) = layout.pages.get(page) {
            across.extend([page.margins.0, width - page.margins.1]);
            down.extend([page.body.0, page.body.1]);
            for other in page
                .objects
                .iter()
                .filter(|o| !moving.contains(&o.object.id))
            {
                let (x0, y0, x1, y1) = other.bounds();
                across.extend([x0, (x0 + x1) / 2.0, x1]);
                down.extend([y0, (y0 + y1) / 2.0, y1]);
            }
        }
        Self {
            across,
            down,
            width,
            height,
        }
    }

    /// How far to move a box, left, top, right and bottom, across and
    /// down for its edges or middles to meet the nearest within `reach`,
    /// and the guides that show where they meet.
    pub fn snap(
        &self,
        bounds: (f32, f32, f32, f32),
        reach: f32,
    ) -> ((f32, f32), Vec<Guide>, usize) {
        let (x0, y0, x1, y1) = bounds;
        let nearest = |own: [f32; 3], targets: &[f32]| {
            let mut best: Option<(f32, f32)> = None;
            for edge in own {
                for &target in targets {
                    let shift = target - edge;
                    if shift.abs() <= reach && best.is_none_or(|(s, _)| shift.abs() < s.abs()) {
                        best = Some((shift, target));
                    }
                }
            }
            best
        };
        let across = nearest([x0, (x0 + x1) / 2.0, x1], &self.across);
        let down = nearest([y0, (y0 + y1) / 2.0, y1], &self.down);
        let mut guides = Vec::new();
        if let Some((_, x)) = across {
            guides.push(Guide {
                page: 0,
                from: (f64::from(x), 0.0),
                to: (f64::from(x), f64::from(self.height)),
            });
        }
        if let Some((_, y)) = down {
            guides.push(Guide {
                page: 0,
                from: (0.0, f64::from(y)),
                to: (f64::from(self.width), f64::from(y)),
            });
        }
        let count = guides.len();
        (
            (across.map_or(0.0, |a| a.0), down.map_or(0.0, |d| d.0)),
            guides,
            count,
        )
    }
}

/// What a drag of objects does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DragKind {
    Move,
    Resize(Handle),
    Rotate,
}

/// A drag of objects in progress.
#[derive(Clone, Debug)]
pub(super) struct ObjectDrag {
    kind: DragKind,
    /// The page it started on, and where, in points.
    page: usize,
    start: (f32, f32),
    /// The objects dragged, as they were placed when it started, with
    /// their pages.
    objects: Vec<(usize, PlacedObject)>,
    /// Where they go.
    preview: Vec<Placement>,
}

/// The objects of a layout with their pages, by identifier.
fn find(layout: &DocumentLayout, id: NodeId) -> Option<(usize, &PlacedObject)> {
    layout.pages.iter().enumerate().find_map(|(index, page)| {
        page.objects
            .iter()
            .find(|o| o.object.id == id)
            .map(|o| (index, o))
    })
}

/// Whether a point of a page is on a line of its text.
fn on_text(layout: &DocumentLayout, page: usize, x: f32, y: f32) -> bool {
    layout.pages.get(page).is_some_and(|page| {
        page.lines.iter().any(|l| {
            let left = l.origin.0 + l.line.x;
            y >= l.top() && y <= l.bottom() && x >= left && x <= left + l.line.width
        })
    })
}

/// The object a press on a page lands on: the frontmost under it, or one
/// behind the text where no text is.
pub(super) fn hit(
    layout: &DocumentLayout,
    page: usize,
    x: f32,
    y: f32,
    reach: f32,
) -> Option<NodeId> {
    let objects = &layout.pages.get(page)?.objects;
    let index = object_at(objects, x, y, reach, false).or_else(|| {
        (!on_text(layout, page, x, y))
            .then(|| object_at(objects, x, y, reach, true))
            .flatten()
    })?;
    Some(objects[index].object.id)
}

/// An object given the place a drag left it in, with its anchor and
/// frame in the terms of the document.
fn placed_object(original: &PlacedObject, from_page: usize, placement: &Placement) -> Object {
    let object = &original.object;
    // The frame of an object that moves with a paragraph is measured from
    // the paragraph, which the layout added to it.
    let shift_x = original.x - object.frame.x.to_pt() as f32;
    let shift_y = original.y - object.frame.y.to_pt() as f32;
    let frame = Frame::from_pt(
        f64::from(placement.x - shift_x),
        f64::from(placement.y - shift_y),
        f64::from(placement.width),
        f64::from(placement.height),
    );
    let mut changed = desk_model::reframed(object, frame);
    if let Anchor::Page(_) = object.anchor
        && placement.page != from_page
    {
        changed.anchor = Anchor::Page(placement.page as u32);
    }
    changed.rotation = placement.rotation;
    if let Some(rising) = placement.rising {
        changed.kind = ObjectKind::Shape(Shape::Line { rising });
    }
    changed
}

/// The shapes that the Insert menu puts on the page (OBJ-08).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewShape {
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Triangle,
    Line,
    TextBox,
}

/// What the Metrics and Wrap inspectors show of the selected objects.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectInfo {
    /// The first selected object, as the document has it.
    pub object: Object,
    /// Its frame as placed on its page, in points.
    pub frame: (f64, f64, f64, f64),
    /// How many objects are selected.
    pub count: usize,
    /// Whether every selected object moves with a page, so that they can
    /// be grouped.
    pub same_anchor: bool,
}

impl TextEditor {
    /// The objects selected, from back to front.
    pub fn selected_objects(&self) -> Vec<NodeId> {
        self.state.borrow().objects.clone()
    }

    /// Whether objects are selected, rather than text.
    pub fn has_selected_objects(&self) -> bool {
        !self.state.borrow().objects.is_empty()
    }

    /// Selects objects, or none to go back to the text.
    pub fn select_objects(&self, ids: Vec<NodeId>) {
        {
            let mut state = self.state.borrow_mut();
            let objects = state.editor.document().objects();
            let mut ids: Vec<NodeId> = objects
                .iter()
                .map(|o| o.id)
                .filter(|id| ids.contains(id))
                .collect();
            ids.dedup();
            state.objects = ids;
        }
        self.show_objects();
        self.selection_changed();
    }

    /// Draws the outline and the handles of the selected objects, or of
    /// where a drag puts them.
    pub(super) fn show_objects(&self) {
        let (marks, guides) = {
            let mut state = self.state.borrow_mut();
            let document = state.editor.document().clone();
            state
                .objects
                .retain(|id| document.objects().get(*id).is_some());
            let mut marks = Vec::new();
            match &state.object_drag {
                Some(drag) if drag.objects.len() == drag.preview.len() => {
                    for ((_, original), placement) in drag.objects.iter().zip(&drag.preview) {
                        marks.push(mark(placement.page, &placement.placed(original)));
                    }
                }
                _ => {
                    for &id in &state.objects {
                        if let Some((page, placed)) = find(&state.layout, id) {
                            marks.push(mark(page, placed));
                        }
                    }
                }
            }
            (marks, state.guides.clone())
        };
        self.view.set_object_marks(marks, guides);
    }

    /// What the inspectors show of the selected objects.
    pub fn object_info(&self) -> Option<ObjectInfo> {
        let state = self.state.borrow();
        let first = *state.objects.first()?;
        let document = state.editor.document();
        let object = document.objects().get(first)?.clone();
        let frame = find(&state.layout, first).map_or(
            (
                object.frame.x.to_pt(),
                object.frame.y.to_pt(),
                object.frame.width.to_pt(),
                object.frame.height.to_pt(),
            ),
            |(_, p)| {
                (
                    f64::from(p.x),
                    f64::from(p.y),
                    f64::from(p.width),
                    f64::from(p.height),
                )
            },
        );
        let anchors: Vec<Anchor> = state
            .objects
            .iter()
            .filter_map(|id| document.objects().get(*id).map(|o| o.anchor))
            .collect();
        Some(ObjectInfo {
            object,
            frame,
            count: state.objects.len(),
            same_anchor: anchors.windows(2).all(|w| w[0] == w[1]),
        })
    }

    /// Changes the selected objects as one action of the history, and
    /// then selects those that `change` gives back, or keeps the
    /// selection when it gives none.
    pub fn arrange_objects(
        &self,
        name: &str,
        change: impl FnOnce(&mut Edit, &[NodeId]) -> Result<Option<Vec<NodeId>>, ModelError>,
    ) {
        let ids = self.selected_objects();
        if ids.is_empty() {
            return;
        }
        let selection = self.selection();
        let depth = self.state.borrow().editor.undo_depth();
        let result = self
            .state
            .borrow_mut()
            .editor
            .edit(name, |edit| change(edit, &ids));
        self.state.borrow_mut().typing = false;
        match result {
            Ok(selected) => {
                if self.state.borrow().editor.undo_depth() > depth {
                    {
                        let mut state = self.state.borrow_mut();
                        let action = state.next_action;
                        state.next_action += 1;
                        state.undo.push((selection, selection, action));
                        state.redo.clear();
                    }
                    if let Some(selected) = selected {
                        self.state.borrow_mut().objects = selected;
                    }
                    self.after_change(selection);
                }
            }
            Err(error) => gtk::glib::g_warning!("desk", "cannot arrange: {error}"),
        }
    }

    /// Moves the selected objects in front of others or behind them.
    pub fn layer_objects(&self, layer: Layer) {
        let name = match layer {
            Layer::Front => "Bring to Front",
            Layer::Forward => "Bring Forward",
            Layer::Backward => "Send Backward",
            Layer::Back => "Send to Back",
        };
        self.arrange_objects(name, |edit, ids| {
            edit.layer_objects(ids, layer).map(|()| None)
        });
    }

    /// Lines up the selected objects.
    pub fn align_objects(&self, alignment: ObjectAlignment) {
        self.arrange_objects("Align", |edit, ids| {
            edit.align_objects(ids, alignment).map(|()| None)
        });
    }

    /// Spreads the selected objects evenly.
    pub fn distribute_objects(&self, distribution: Distribution) {
        self.arrange_objects("Distribute", |edit, ids| {
            edit.distribute_objects(ids, distribution).map(|()| None)
        });
    }

    /// Groups the selected objects, and selects the group.
    pub fn group_objects(&self) {
        self.arrange_objects("Group", |edit, ids| {
            Ok(edit.group_objects(ids)?.map(|id| vec![id]))
        });
    }

    /// Breaks the selected groups into their objects, and selects them.
    pub fn ungroup_objects(&self) {
        self.arrange_objects("Ungroup", |edit, ids| Ok(Some(edit.ungroup_objects(ids)?)));
    }

    /// Locks the selected objects, or unlocks them.
    pub fn lock_objects(&self, locked: bool) {
        let name = if locked { "Lock" } else { "Unlock" };
        self.arrange_objects(name, |edit, ids| {
            edit.lock_objects(ids, locked).map(|()| None)
        });
    }

    /// Flips the selected objects left to right, or upside down.
    pub fn flip_objects(&self, horizontally: bool) {
        let name = if horizontally {
            "Flip Horizontally"
        } else {
            "Flip Vertically"
        };
        self.arrange_objects(name, |edit, ids| {
            edit.flip_objects(ids, horizontally).map(|()| None)
        });
    }

    /// Removes the selected objects, but the locked ones.
    pub fn delete_objects(&self) {
        self.arrange_objects("Delete", |edit, ids| {
            edit.delete_objects(ids)?;
            let left = ids
                .iter()
                .copied()
                .filter(|id| edit.document().objects().get(*id).is_some())
                .collect();
            Ok(Some(left))
        });
    }

    /// Changes the first selected object as the inspectors ask: its frame
    /// on its page, in points, its rotation, its wrap and its margin.
    pub fn set_object(&self, name: &str, change: impl FnOnce(&mut Object)) {
        let Some(info) = self.object_info() else {
            return;
        };
        let original =
            find(&self.state.borrow().layout, info.object.id).map(|(_, placed)| placed.clone());
        self.arrange_objects(name, |edit, _| {
            let mut object = info.object.clone();
            if object.locked {
                return Ok(None);
            }
            change(&mut object);
            if let Some(placed) = &original
                && object.frame != info.object.frame
            {
                // The inspector shows the frame on the page; an object
                // that moves with a paragraph keeps its anchor.
                let shift_x = placed.x as f64 - info.object.frame.x.to_pt();
                let shift_y = placed.y as f64 - info.object.frame.y.to_pt();
                let mut frame = object.frame;
                frame.x = Length::from_pt(frame.x.to_pt() - shift_x);
                frame.y = Length::from_pt(frame.y.to_pt() - shift_y);
                let reframed = desk_model::reframed(&info.object, frame);
                object.frame = reframed.frame;
                object.kind = reframed.kind;
            }
            edit.replace_object(object).map(|()| None)
        });
    }

    /// Puts a new shape or text box in the middle of the page in sight,
    /// in front of the others, and selects it (OBJ-08, OBJ-13).
    pub fn insert_shape(&self, shape: NewShape) {
        let page = self.view.current_page().unwrap_or(0);
        let (width, height) = {
            let state = self.state.borrow();
            (
                state.layout.setup.width.to_pt(),
                state.layout.setup.height.to_pt(),
            )
        };
        let (w, h) = match shape {
            NewShape::Line => (100.0, 0.0),
            NewShape::TextBox => (144.0, 72.0),
            _ => (100.0, 100.0),
        };
        let frame = Frame::from_pt((width - w) / 2.0, (height - h) / 2.0, w, h);
        let name = match shape {
            NewShape::TextBox => "Insert Text Box",
            _ => "Insert Shape",
        };
        self.leave_story(None);
        let selection = self.selection();
        let result = self.state.borrow_mut().editor.edit(name, |edit| {
            let id = edit.new_id();
            let line = Stroke {
                color: Rgba::BLACK,
                width: Length::from_points(1),
                style: desk_model::BorderStyle::Solid,
            };
            // The blue that the shapes of Pages are filled with.
            let blue = Fill::Color(Rgba::rgb(0x6c, 0x9f, 0xd8));
            let (kind, fill, stroke) = match shape {
                NewShape::Rectangle => (Shape::Rectangle.into(), Some(blue), Some(line)),
                NewShape::RoundedRectangle => (
                    Shape::RoundedRectangle {
                        radius: Length::from_points(10),
                    }
                    .into(),
                    Some(blue),
                    Some(line),
                ),
                NewShape::Ellipse => (Shape::Ellipse.into(), Some(blue), Some(line)),
                NewShape::Triangle => (Shape::triangle().into(), Some(blue), Some(line)),
                NewShape::Line => (Shape::Line { rising: false }.into(), None, Some(line)),
                NewShape::TextBox => {
                    let style = edit.document().styles().default_paragraph();
                    let paragraph = Paragraph::from_runs(
                        edit.new_id(),
                        style,
                        ParaAttrs::default(),
                        [Run::new(&crate::i18n::tr("Text"))],
                    )?;
                    (
                        ObjectKind::TextBox(TextBox::new(
                            Arc::from([paragraph]),
                            Length::from_points(4),
                        )),
                        None,
                        None,
                    )
                }
            };
            let mut object = Object::new(id, kind, frame);
            object.anchor = Anchor::Page(page as u32);
            object.style.fill = fill;
            object.style.stroke = stroke;
            if shape == NewShape::TextBox {
                object.wrap = Wrap::Through;
            }
            edit.add_object(object)?;
            Ok(id)
        });
        match result {
            Ok(id) => {
                {
                    let mut state = self.state.borrow_mut();
                    let action = state.next_action;
                    state.next_action += 1;
                    state.undo.push((selection, selection, action));
                    state.redo.clear();
                    state.objects = vec![id];
                }
                self.after_change(selection);
                if shape == NewShape::TextBox {
                    // A new text box is edited at once, with its text
                    // selected, so that typing replaces it.
                    self.edit_text_box(id, page, None);
                    let end = self.selection().focus;
                    self.select(super::Selection {
                        anchor: super::Spot::new(0, 0),
                        focus: end,
                    });
                }
            }
            Err(error) => gtk::glib::g_warning!("desk", "cannot insert: {error}"),
        }
    }

    /// Adds a text box linked after the last of those linked with the
    /// selected text box, or the one being edited, below it on its page
    /// or on the next page, so that its text goes on there (OBJ-13).
    pub fn add_linked_text_box(&self) {
        let from = match self.story() {
            Some(Story::TextBox(id)) => Some(id),
            _ => match self.selected_objects()[..] {
                [id] => Some(id),
                _ => None,
            },
        };
        let Some(from) = from else {
            return;
        };
        self.leave_story(None);
        let (last, page, height) = {
            let state = self.state.borrow();
            let document = state.editor.document();
            if !matches!(
                document.objects().get(from).map(|o| &o.kind),
                Some(ObjectKind::TextBox(_))
            ) {
                return;
            }
            let chain = document.text_box_chain(document.text_box_head(from));
            let last = *chain.last().unwrap_or(&from);
            let Some((page, _)) = find(&state.layout, last) else {
                return;
            };
            let Some(object) = document.objects().get(last).cloned() else {
                return;
            };
            (object, page, state.layout.setup.height)
        };
        let selection = self.selection();
        let result = self
            .state
            .borrow_mut()
            .editor
            .edit("Add Linked Text Box", |edit| {
                let mut object = last.clone();
                object.id = edit.new_id();
                let ObjectKind::TextBox(text) = &mut object.kind else {
                    return Ok(None);
                };
                let style = text
                    .paragraphs
                    .first()
                    .map(Paragraph::style)
                    .unwrap_or_else(|| edit.document().styles().default_paragraph());
                text.paragraphs = Arc::from([Paragraph::new(edit.new_id(), style)]);
                text.next = None;
                object.locked = false;
                let f = last.frame;
                let below = f.y + f.height + Length::from_points(18);
                if matches!(last.anchor, Anchor::Page(_)) && below + f.height > height {
                    // No room below: the next page, where it was on this one.
                    object.anchor = Anchor::Page(page as u32 + 1);
                } else {
                    object.frame.y = below;
                }
                let id = object.id;
                edit.add_object(object)?;
                edit.link_text_box(last.id, id)?;
                Ok(Some(id))
            });
        match result {
            Ok(Some(id)) => {
                {
                    let mut state = self.state.borrow_mut();
                    let action = state.next_action;
                    state.next_action += 1;
                    state.undo.push((selection, selection, action));
                    state.redo.clear();
                    state.objects = vec![id];
                }
                self.after_change(selection);
            }
            Ok(None) => {}
            Err(error) => gtk::glib::g_warning!("desk", "cannot link: {error}"),
        }
    }

    /// Answers a press on the pages: on a handle of a selected object, it
    /// starts resizing or rotating it; on an object, it selects it and
    /// starts moving it. Elsewhere the objects are deselected, and the
    /// press is left to the text.
    pub(super) fn press_object(
        &self,
        presses: i32,
        x: f64,
        y: f64,
        modifiers: gdk::ModifierType,
    ) -> bool {
        let Some((page, px, py)) = self.view.page_at(x, y) else {
            return false;
        };
        let (px, py) = (px as f32, py as f32);
        let scale = self.view.scale();
        let reach = (REACH / scale) as f32;
        let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
        let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        let (grabbed, hit_id) = {
            let state = self.state.borrow();
            let mut grabbed = None;
            if state.story.is_none() {
                for &id in &state.objects {
                    if let Some((p, placed)) = find(&state.layout, id)
                        && p == page
                        && !placed.object.locked
                    {
                        let near = handles(placed).into_iter().find(|(_, (hx, hy))| {
                            (hx - px).abs() <= reach && (hy - py).abs() <= reach
                        });
                        if let Some((handle, _)) = near {
                            grabbed = Some((id, handle));
                            break;
                        }
                    }
                }
            }
            (grabbed, hit(&state.layout, page, px, py, reach))
        };
        if let Some((id, handle)) = grabbed {
            let kind = if control && line(&self.object_by_id(id)).is_none() {
                DragKind::Rotate
            } else {
                DragKind::Resize(handle)
            };
            self.start_object_drag(kind, page, (px, py), vec![id]);
            return true;
        }
        let Some(id) = hit_id else {
            if self.has_selected_objects() {
                self.state.borrow_mut().objects.clear();
                self.show_objects();
                self.selection_changed();
            }
            return false;
        };
        // A click in the text box being edited is a click in its text.
        let head = Self::main_document(&self.state.borrow()).text_box_head(id);
        if self.story() == Some(Story::TextBox(head)) {
            return false;
        }
        if presses > 1 {
            // A double click on a text box edits its text (OBJ-13).
            let text_box = matches!(self.object_by_id(id).kind, ObjectKind::TextBox(_));
            if text_box {
                self.edit_text_box(id, page, Some((x, y)));
            }
            return true;
        }
        self.leave_story(None);
        let mut selected = self.selected_objects();
        if shift || control {
            if let Some(i) = selected.iter().position(|s| *s == id) {
                selected.remove(i);
            } else {
                selected.push(id);
            }
        } else if !selected.contains(&id) {
            selected = vec![id];
        }
        self.select_objects(selected.clone());
        if selected.contains(&id) {
            self.start_object_drag(DragKind::Move, page, (px, py), self.selected_objects());
        }
        true
    }

    /// Puts the cursor in the text of a text box: where a point of the
    /// view is, or at its end (OBJ-13).
    pub(super) fn edit_text_box(&self, id: NodeId, page: usize, at: Option<(f64, f64)>) {
        {
            let mut state = self.state.borrow_mut();
            state.objects.clear();
            state.object_drag = None;
            state.guides.clear();
        }
        self.show_objects();
        let id = Self::main_document(&self.state.borrow()).text_box_head(id);
        self.enter_story(Story::TextBox(id), page, None);
        if self.story() != Some(Story::TextBox(id)) {
            return;
        }
        if let Some((x, y)) = at
            && let Some(spot) = self.spot_at(x, y)
        {
            self.select(super::Selection::cursor(spot));
        }
    }

    fn object_by_id(&self, id: NodeId) -> Object {
        self.state
            .borrow()
            .editor
            .document()
            .objects()
            .get(id)
            .cloned()
            .expect("a selected object is in the document")
    }

    fn start_object_drag(&self, kind: DragKind, page: usize, start: (f32, f32), ids: Vec<NodeId>) {
        let mut state = self.state.borrow_mut();
        let objects: Vec<(usize, PlacedObject)> = ids
            .iter()
            .filter_map(|id| find(&state.layout, *id).map(|(p, o)| (p, o.clone())))
            .filter(|(_, o)| !o.object.locked)
            .collect();
        let preview = objects.iter().map(|(p, o)| Placement::of(*p, o)).collect();
        state.object_drag = Some(ObjectDrag {
            kind,
            page,
            start,
            objects,
            preview,
        });
    }

    /// Whether a drag of objects is going on.
    pub(super) fn dragging_objects(&self) -> bool {
        self.state.borrow().object_drag.is_some()
    }

    /// Follows the pointer in a drag of objects, to a point of the area.
    pub(super) fn drag_objects(&self, x: f64, y: f64, modifiers: gdk::ModifierType) {
        let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
        let scale = self.view.scale();
        let Some(drag) = self.state.borrow().object_drag.clone() else {
            return;
        };
        // The point on the page the drag started on, and the page under
        // the pointer, where a moved object fixed on a page goes.
        let Some((ox, oy)) = self.view.to_area(drag.page, 0.0, 0.0) else {
            return;
        };
        let point = (((x - ox) / scale) as f32, ((y - oy) / scale) as f32);
        let under = self.view.page_at(x, y);
        let (sx, sy) = drag.start;
        let mut guides = Vec::new();
        let preview: Vec<Placement> = match drag.kind {
            DragKind::Move => {
                let (mut dx, mut dy) = (point.0 - sx, point.1 - sy);
                if shift {
                    if dx.abs() > dy.abs() {
                        dy = 0.0;
                    } else {
                        dx = 0.0;
                    }
                }
                let fixed = drag
                    .objects
                    .iter()
                    .all(|(_, o)| matches!(o.object.anchor, Anchor::Page(_)));
                let target = match under {
                    Some((page, px, py)) if fixed && page != drag.page => {
                        Some((page, px as f32 - sx, py as f32 - sy))
                    }
                    _ => None,
                };
                let page = target.map_or(drag.page, |t| t.0);
                if let Some((_, tx, ty)) = target {
                    (dx, dy) = (tx, ty);
                }
                // Snap the box of the objects on the page it started on.
                let state = self.state.borrow();
                let ids: Vec<NodeId> = drag.objects.iter().map(|(_, o)| o.object.id).collect();
                let starting: Vec<&PlacedObject> = drag
                    .objects
                    .iter()
                    .filter(|(p, _)| *p == drag.page)
                    .map(|(_, o)| o)
                    .collect();
                if let Some(bounds) = starting
                    .iter()
                    .map(|o| o.bounds())
                    .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
                {
                    let snaps = Snaps::new(&state.layout, page, &ids);
                    let moved = (bounds.0 + dx, bounds.1 + dy, bounds.2 + dx, bounds.3 + dy);
                    let ((ax, ay), found, _) = snaps.snap(moved, (SNAP / scale) as f32);
                    if !shift || dy == 0.0 {
                        dx += ax;
                    }
                    if !shift || dx == 0.0 {
                        dy += ay;
                    }
                    guides = found.into_iter().map(|g| Guide { page, ..g }).collect();
                }
                drag.objects
                    .iter()
                    .map(|(p, o)| {
                        let mut placement = Placement::of(*p, o);
                        placement.x += dx;
                        placement.y += dy;
                        if *p == drag.page {
                            placement.page = page;
                        }
                        placement
                    })
                    .collect()
            }
            DragKind::Resize(handle) => drag
                .objects
                .iter()
                .map(|(p, o)| {
                    let (x, y, width, height, rising) = resize(o, handle, point, shift);
                    Placement {
                        x,
                        y,
                        width,
                        height,
                        rising: rising.or(line(&o.object)),
                        ..Placement::of(*p, o)
                    }
                })
                .collect(),
            DragKind::Rotate => drag
                .objects
                .iter()
                .map(|(p, o)| Placement {
                    rotation: rotate(o, drag.start, point, shift),
                    ..Placement::of(*p, o)
                })
                .collect(),
        };
        {
            let mut state = self.state.borrow_mut();
            if let Some(drag) = state.object_drag.as_mut() {
                drag.preview = preview;
            }
            state.guides = guides;
        }
        self.show_objects();
    }

    /// Ends a drag of objects, as one action of the history.
    pub(super) fn end_object_drag(&self) {
        let Some(drag) = self.state.borrow_mut().object_drag.take() else {
            return;
        };
        self.state.borrow_mut().guides.clear();
        let changed: Vec<Object> = drag
            .objects
            .iter()
            .zip(&drag.preview)
            .filter(|((p, o), placement)| **placement != Placement::of(*p, o))
            .map(|((p, o), placement)| placed_object(o, *p, placement))
            .collect();
        if changed.is_empty() {
            self.show_objects();
            return;
        }
        let name = match drag.kind {
            DragKind::Move => "Move",
            DragKind::Resize(_) => "Resize",
            DragKind::Rotate => "Rotate",
        };
        self.arrange_objects(name, |edit, _| {
            for object in changed {
                edit.replace_object(object)?;
            }
            Ok(None)
        });
        self.show_objects();
    }

    /// Answers a key while objects are selected: the arrows nudge them,
    /// Delete removes them and Escape deselects them.
    pub(super) fn object_key(&self, key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
        if !self.has_selected_objects() {
            return false;
        }
        let step = if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
            10.0
        } else {
            1.0
        };
        let nudge = |dx: f64, dy: f64| {
            self.arrange_objects("Move", |edit, ids| {
                edit.move_objects(ids, Length::from_pt(dx), Length::from_pt(dy))
                    .map(|()| None)
            });
        };
        match key {
            gdk::Key::Left | gdk::Key::KP_Left => nudge(-step, 0.0),
            gdk::Key::Right | gdk::Key::KP_Right => nudge(step, 0.0),
            gdk::Key::Up | gdk::Key::KP_Up => nudge(0.0, -step),
            gdk::Key::Down | gdk::Key::KP_Down => nudge(0.0, step),
            gdk::Key::Delete | gdk::Key::KP_Delete | gdk::Key::BackSpace => self.delete_objects(),
            gdk::Key::Escape => self.select_objects(Vec::new()),
            // Return edits the text of a selected text box (OBJ-13).
            gdk::Key::Return | gdk::Key::KP_Enter => {
                let ids = self.selected_objects();
                let found = match ids[..] {
                    [id] => find(&self.state.borrow().layout, id).map(|(page, placed)| {
                        (
                            id,
                            page,
                            matches!(placed.object.kind, ObjectKind::TextBox(_)),
                        )
                    }),
                    _ => None,
                };
                if let Some((id, page, true)) = found {
                    self.edit_text_box(id, page, None);
                }
            }
            // Other keys do nothing while objects are selected, rather
            // than typing in the text that is not in sight.
            _ => return !modifiers.contains(gdk::ModifierType::CONTROL_MASK),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(rotation: i32, shape: Shape) -> PlacedObject {
        let mut object = Object::new(
            NodeId::new(1).unwrap(),
            ObjectKind::Shape(shape),
            Frame::from_pt(100.0, 100.0, 100.0, 50.0),
        );
        object.rotation = rotation;
        let mut editor = desk_model::Editor::new(desk_model::Document::new_word_processing());
        editor.edit("Add", |e| e.add_object(object)).unwrap();
        let layout = crate::lay_out(editor.document());
        layout.pages[0].objects[0].clone()
    }

    fn near(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
        (a.0 - b.0).abs() < 0.01
            && (a.1 - b.1).abs() < 0.01
            && (a.2 - b.2).abs() < 0.01
            && (a.3 - b.3).abs() < 0.01
    }

    #[test]
    fn obj_03_handles_resize_from_the_other_side() {
        let flat = placed(0, Shape::Rectangle);
        let (x, y, w, h, _) = resize(&flat, Handle::BottomRight, (250.0, 170.0), false);
        assert!(near((x, y, w, h), (100.0, 100.0, 150.0, 70.0)));
        let (x, y, w, h, _) = resize(&flat, Handle::Left, (80.0, 0.0), false);
        assert!(near((x, y, w, h), (80.0, 100.0, 120.0, 50.0)));
        // Shift keeps the proportions, from the corner across.
        let (x, y, w, h, _) = resize(&flat, Handle::TopLeft, (0.0, 90.0), true);
        assert!(near((x, y, w, h), (0.0, 50.0, 200.0, 100.0)));
        // Turned a quarter, the right handle is at the top: dragging it up
        // makes the object wider, with its bottom where it was.
        let standing = placed(9000, Shape::Rectangle);
        let (hx, hy) = handles(&standing)[3].1;
        assert!((hx - 150.0).abs() < 0.01 && (hy - 75.0).abs() < 0.01);
        let (x, y, w, h, _) = resize(&standing, Handle::Right, (150.0, 55.0), false);
        assert!(near((x, y, w, h), (90.0, 90.0, 120.0, 50.0)));
    }

    #[test]
    fn obj_03_the_ends_of_a_line_move_and_it_turns() {
        let line = placed(0, Shape::Line { rising: false });
        let (x, y, w, h, rising) = resize(&line, Handle::End, (50.0, 20.0), false);
        assert!(near((x, y, w, h), (50.0, 20.0, 50.0, 80.0)));
        assert_eq!(rising, Some(false));
        let (x, y, w, h, rising) = resize(&line, Handle::End, (50.0, 180.0), false);
        assert!(near((x, y, w, h), (50.0, 100.0, 50.0, 80.0)));
        assert_eq!(rising, Some(true));
        let square = placed(0, Shape::Rectangle);
        let (cx, cy) = square.center();
        // From the right of the center up to above it: a quarter turn
        // counterclockwise, and by steps of 45 degrees with Shift.
        assert_eq!(
            rotate(&square, (cx + 50.0, cy), (cx, cy - 50.0), false),
            9000
        );
        assert_eq!(
            rotate(&square, (cx + 50.0, cy), (cx + 50.0, cy - 30.0), true),
            4500
        );
        assert_eq!(
            rotate(&square, (cx + 50.0, cy), (cx, cy + 50.0), false),
            27_000
        );
    }

    #[test]
    fn obj_05_drags_snap_to_the_page_and_other_objects() {
        let mut editor = desk_model::Editor::new(desk_model::Document::new_word_processing());
        let other = editor
            .edit("Add", |e| {
                let id = e.new_id();
                e.add_object(Object::new(
                    id,
                    Shape::Rectangle.into(),
                    Frame::from_pt(300.0, 400.0, 100.0, 50.0),
                ))?;
                Ok(id)
            })
            .unwrap();
        let layout = crate::lay_out(editor.document());
        let snaps = Snaps::new(&layout, 0, &[]);
        let width = layout.setup.width.to_pt() as f32;
        // The middle of a box near the middle of the page snaps to it.
        let ((dx, dy), guides, _) =
            snaps.snap((width / 2.0 - 52.0, 10.0, width / 2.0 + 48.0, 20.0), 5.0);
        assert!((dx - 2.0).abs() < 0.01 && dy == 0.0);
        assert_eq!(guides.len(), 1);
        // Its top snaps to the bottom of the other object, unless that is
        // the one moved.
        let ((_, dy), _, _) = snaps.snap((10.0, 448.0, 20.0, 460.0), 5.0);
        assert!((dy - 2.0).abs() < 0.01);
        let alone = Snaps::new(&layout, 0, &[other]);
        let ((_, dy), _, _) = alone.snap((10.0, 448.0, 20.0, 460.0), 5.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn obj_03_presses_find_objects_but_behind_ones_only_off_the_text() {
        let mut editor = desk_model::Editor::new(desk_model::Document::new_word_processing());
        let start = editor.document().start();
        let (front, back) = editor
            .edit("Add", |e| {
                e.insert_text(start, "Some text at the top of the page.")?;
                let front = e.new_id();
                e.add_object(Object::new(
                    front,
                    Shape::Ellipse.into(),
                    Frame::from_pt(300.0, 400.0, 100.0, 50.0),
                ))?;
                let back = e.new_id();
                let mut behind = Object::new(
                    back,
                    Shape::Rectangle.into(),
                    Frame::from_pt(0.0, 0.0, 600.0, 800.0),
                );
                behind.wrap = Wrap::Behind;
                e.insert_object(0, behind)?;
                Ok((front, back))
            })
            .unwrap();
        let layout = crate::lay_out(editor.document());
        let line = &layout.pages[0].lines[0];
        let (tx, ty) = (line.origin.0 + 5.0, (line.top() + line.bottom()) / 2.0);
        assert_eq!(hit(&layout, 0, 350.0, 420.0, 2.0), Some(front));
        assert_eq!(hit(&layout, 0, 10.0, 700.0, 2.0), Some(back));
        assert_eq!(hit(&layout, 0, tx, ty, 2.0), None);
    }
}
