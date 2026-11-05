// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Connection lines (OBJ-31): Insert Connection Line joins the two
//! selected objects with a line that follows them. Dragging an end of a
//! selected line onto another object joins it there, and anywhere else
//! lets go; dragging its middle bends it.

use desk_layout::PlacedObject;
use desk_model::{
    Connection, Length, NodeId, Object, ObjectKind, PathSegment, Rgba, Shape, Stroke,
};

use super::TextEditor;
use super::objects::{Handle, find, hit};
use crate::view::PathMark;

/// A drag of an end or of the middle of a connection line.
#[derive(Clone, Debug)]
pub(super) struct ConnectionDrag {
    handle: Handle,
    page: usize,
    /// The line as it was placed when the drag started.
    placed: PlacedObject,
    /// Where the pointer is, in points on the page.
    at: Option<(f64, f64)>,
}

/// The line of a placed object, if it is a connection line.
fn connection(placed: &PlacedObject) -> Option<&Connection> {
    match &placed.object.kind {
        ObjectKind::Shape(Shape::Connection(line)) => Some(line),
        _ => None,
    }
}

/// How far the layout puts an object from where its frame says, as the
/// paragraph it moves with.
fn shift(placed: &PlacedObject) -> (f64, f64) {
    let frame = &placed.object.frame;
    (
        f64::from(placed.x) - frame.x.to_pt(),
        f64::from(placed.y) - frame.y.to_pt(),
    )
}

/// The ends of a placed connection line, on the page.
fn ends(placed: &PlacedObject, line: &Connection) -> ((f64, f64), (f64, f64)) {
    let (a, b) = line.ends_in(f64::from(placed.width), f64::from(placed.height));
    let (x, y) = (f64::from(placed.x), f64::from(placed.y));
    ((x + a.0, y + a.1), (x + b.0, y + b.1))
}

/// The ends and bend that a drag gives a connection line, on the page,
/// with the objects it then joins.
type Dragged = ((f64, f64), (f64, f64), f64, Option<NodeId>, Option<NodeId>);

/// What a drag makes of a connection line, with an end dropped on
/// `target`.
fn dragged(drag: &ConnectionDrag, target: Option<NodeId>) -> Option<Dragged> {
    let line = connection(&drag.placed)?;
    let at = drag.at?;
    let (a, b) = ends(&drag.placed, line);
    let bend = line.bend.to_pt();
    Some(match drag.handle {
        Handle::Start => (at, b, bend, target, line.end),
        Handle::End => (a, at, bend, line.start, target),
        _ => (
            a,
            b,
            desk_model::bend_through(a, b, at),
            line.start,
            line.end,
        ),
    })
}

impl TextEditor {
    /// Joins the two selected objects with a connection line (OBJ-31).
    pub fn insert_connection(&self) {
        let ids = self.selected_objects();
        let [from, to] = ids[..] else { return };
        let joined = {
            let state = self.state.borrow();
            let objects = state.editor.document().objects();
            let (Some(from), Some(to)) = (objects.get(from), objects.get(to)) else {
                return;
            };
            desk_model::connect(from, to).map(|joined| (joined, from.anchor))
        };
        let Some(((frame, line), anchor)) = joined else {
            return;
        };
        let selection = self.selection();
        let result = self
            .state
            .borrow_mut()
            .editor
            .edit("Insert Connection Line", |edit| {
                let id = edit.new_id();
                let mut object = Object::new(id, Shape::Connection(line).into(), frame);
                object.anchor = anchor;
                object.style.fill = None;
                object.style.stroke = Some(Stroke::new(Rgba::BLACK, Length::from_points(1)));
                edit.add_object(object)?;
                Ok(id)
            });
        match result {
            Ok(id) => self.select_new_object(selection, id),
            Err(error) => gtk::glib::g_warning!("desk", "cannot connect: {error}"),
        }
    }

    /// Starts dragging a handle of a connection line, when the object is
    /// one. Returns whether it is.
    pub(super) fn start_connection_drag(&self, id: NodeId, handle: Handle, page: usize) -> bool {
        let mut state = self.state.borrow_mut();
        let Some((_, placed)) = find(&state.layout, id) else {
            return false;
        };
        if connection(placed).is_none() {
            return false;
        }
        let placed = placed.clone();
        state.connection_drag = Some(ConnectionDrag {
            handle,
            page,
            placed,
            at: None,
        });
        true
    }

    /// Follows the pointer in a drag of a connection line, to a point of
    /// the area. Returns whether one is being dragged.
    pub(super) fn drag_connection(&self, x: f64, y: f64) -> bool {
        let page = match self.state.borrow().connection_drag.as_ref() {
            Some(drag) => drag.page,
            None => return false,
        };
        let Some((on, px, py)) = self.view.page_at(x, y) else {
            return true;
        };
        if on != page {
            return true;
        }
        let shown = {
            let mut state = self.state.borrow_mut();
            let Some(drag) = state.connection_drag.as_mut() else {
                return true;
            };
            drag.at = Some((px, py));
            dragged(drag, None)
        };
        if let Some((a, b, bend, ..)) = shown {
            let way = match desk_model::bent_controls(a, b, bend) {
                Some((c1, c2)) => PathSegment::Curve(c1.0, c1.1, c2.0, c2.1, b.0, b.1),
                None => PathSegment::Line(b.0, b.1),
            };
            self.view.set_path_mark(Some(PathMark {
                page,
                outline: vec![PathSegment::Move(a.0, a.1), way],
                points: vec![(a, false), (b, false)],
                handles: Vec::new(),
            }));
        }
        true
    }

    /// Ends a drag of a connection line: an end dropped on an object
    /// joins it, and the middle bends the line.
    pub(super) fn end_connection_drag(&self) {
        let Some(drag) = self.state.borrow_mut().connection_drag.take() else {
            return;
        };
        self.view.set_path_mark(None);
        let Some((x, y)) = drag.at else {
            return;
        };
        let id = drag.placed.object.id;
        let target = matches!(drag.handle, Handle::Start | Handle::End)
            .then(|| {
                let state = self.state.borrow();
                let reach = (super::objects::REACH / self.view.scale()) as f32;
                hit(
                    &state.layout,
                    drag.page,
                    x as f32,
                    y as f32,
                    reach,
                    state.selectable,
                )
            })
            .flatten()
            .filter(|&other| {
                let state = self.state.borrow();
                let objects = state.editor.document().objects();
                objects
                    .get(other)
                    .zip(objects.get(id))
                    .is_some_and(|(o, line)| {
                        o.id != line.id
                            && o.anchor == line.anchor
                            && o.anchor != desk_model::Anchor::InText
                            && !matches!(o.kind, ObjectKind::Shape(Shape::Connection(_)))
                    })
            });
        let Some((a, b, bend, start, end)) = dragged(&drag, target) else {
            return;
        };
        let (dx, dy) = shift(&drag.placed);
        let (frame, line) = Connection::between(
            (a.0 - dx, a.1 - dy),
            (b.0 - dx, b.1 - dy),
            Length::from_pt(bend),
            start,
            end,
        );
        let mut changed = drag.placed.object.clone();
        changed.frame = frame;
        changed.rotation = 0;
        changed.flip_horizontal = false;
        changed.flip_vertical = false;
        changed.kind = ObjectKind::Shape(Shape::Connection(line));
        let name = if drag.handle == Handle::Bend {
            "Bend Connection Line"
        } else {
            "Move Connection Line"
        };
        self.arrange_objects(name, |edit, _| {
            edit.replace_object(changed)?;
            Ok(None)
        });
        self.show_objects();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::Anchor;

    #[test]
    fn obj_31_dragging_a_connection_line_moves_an_end_or_bends_it() {
        let (frame, line) = Connection::between((0.0, 0.0), (100.0, 0.0), Length::ZERO, None, None);
        let id = NodeId::new(7).unwrap();
        let mut object = Object::new(id, Shape::Connection(line).into(), frame);
        object.anchor = Anchor::Page(0);
        let mut editor = desk_model::Editor::new(desk_model::Document::new_word_processing());
        editor.edit("Add", |e| e.add_object(object)).unwrap();
        let layout = crate::lay_out(editor.document());
        let placed = find(&layout, id).unwrap().1.clone();
        let mut drag = ConnectionDrag {
            handle: Handle::Bend,
            page: 0,
            placed,
            at: Some((50.0, 30.0)),
        };
        let (a, b, bend, ..) = dragged(&drag, None).unwrap();
        assert_eq!((a, b), ((0.0, 0.0), (100.0, 0.0)));
        // Down is to the right of the way right.
        assert!((bend + 30.0).abs() < 1e-9);
        drag.handle = Handle::End;
        let target = NodeId::new(3).unwrap();
        let (_, b, _, start, end) = dragged(&drag, Some(target)).unwrap();
        assert_eq!((b, start, end), ((50.0, 30.0), None, Some(target)));
    }
}
