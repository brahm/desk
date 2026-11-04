// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Objects moved, resized, rotated, flipped, layered, grouped, aligned,
//! distributed and locked, as the Arrange menu of Pages does (OBJ-03,
//! OBJ-04, OBJ-05, OBJ-06, OBJ-07).
//!
//! A locked object is left as it is by every edit here but unlocking it.
//! The objects of a group keep their frames on the page, so a group is
//! moved by moving them with it and resized by scaling them with it.

use std::sync::Arc;

use crate::error::ModelError;
use crate::id::NodeId;
use crate::object::{Anchor, Frame, Object, ObjectKind};
use crate::transaction::Edit;
use crate::units::Length;

/// Where objects are lined up, by their frames (OBJ-05).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectAlignment {
    /// Their left edges, on the leftmost.
    Left,
    /// Their centers, across, on the middle of all of them.
    Center,
    /// Their right edges, on the rightmost.
    Right,
    /// Their top edges, on the topmost.
    Top,
    /// Their middles, down, on the middle of all of them.
    Middle,
    /// Their bottom edges, on the lowest.
    Bottom,
}

/// Which way objects are spread evenly (OBJ-05).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Distribution {
    /// Across, with the same room between each and the next.
    Horizontally,
    /// Down, with the same room between each and the next.
    Vertically,
}

/// Where an object goes among the others, from back to front (OBJ-04).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    /// In front of all the others.
    Front,
    /// In front of the next one.
    Forward,
    /// Behind the one before.
    Backward,
    /// Behind all the others.
    Back,
}

/// An object moved by `dx` and `dy`, with the objects of a group.
pub fn translated(object: &Object, dx: Length, dy: Length) -> Object {
    let mut moved = object.clone();
    moved.frame.x = moved.frame.x + dx;
    moved.frame.y = moved.frame.y + dy;
    if let ObjectKind::Group(objects) = &object.kind {
        moved.kind = ObjectKind::Group(objects.iter().map(|o| translated(o, dx, dy)).collect());
    }
    moved
}

/// An object given a new frame, with the objects of a group scaled from
/// the old one to the new one.
pub fn reframed(object: &Object, frame: Frame) -> Object {
    let mut changed = object.clone();
    let old = object.frame;
    changed.frame = frame;
    if let ObjectKind::Group(objects) = &object.kind {
        let scale = |from: Length, to: Length| {
            if from.millipoints() == 0 {
                1.0
            } else {
                to.to_pt() / from.to_pt()
            }
        };
        let sx = scale(old.width, frame.width);
        let sy = scale(old.height, frame.height);
        let map_x = |x: Length| frame.x + Length::from_pt((x - old.x).to_pt() * sx);
        let map_y = |y: Length| frame.y + Length::from_pt((y - old.y).to_pt() * sy);
        changed.kind = ObjectKind::Group(
            objects
                .iter()
                .map(|o| {
                    let f = o.frame;
                    let x = map_x(f.x);
                    let y = map_y(f.y);
                    let child = Frame {
                        x,
                        y,
                        width: map_x(f.x + f.width) - x,
                        height: map_y(f.y + f.height) - y,
                    };
                    reframed(o, child)
                })
                .collect(),
        );
    }
    changed
}

/// The smallest frame that holds every one of `frames`.
pub fn union(frames: impl IntoIterator<Item = Frame>) -> Option<Frame> {
    let mut frames = frames.into_iter();
    let first = frames.next()?;
    let (mut x0, mut y0) = (first.x, first.y);
    let (mut x1, mut y1) = (first.x + first.width, first.y + first.height);
    for f in frames {
        x0 = x0.min(f.x);
        y0 = y0.min(f.y);
        x1 = x1.max(f.x + f.width);
        y1 = y1.max(f.y + f.height);
    }
    Some(Frame {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

impl Edit {
    fn object(&self, id: NodeId) -> Result<&Object, ModelError> {
        self.document()
            .objects()
            .get(id)
            .ok_or(ModelError::UnknownObject(id))
    }

    /// The objects among `ids` that are not locked, from back to front,
    /// each once.
    fn unlocked(&self, ids: &[NodeId]) -> Result<Vec<Object>, ModelError> {
        for &id in ids {
            self.object(id)?;
        }
        Ok(self
            .document()
            .objects()
            .iter()
            .filter(|o| ids.contains(&o.id) && !o.locked)
            .cloned()
            .collect())
    }

    /// Moves objects by `dx` across and `dy` down, leaving the locked
    /// ones where they are (OBJ-03).
    pub fn move_objects(
        &mut self,
        ids: &[NodeId],
        dx: Length,
        dy: Length,
    ) -> Result<(), ModelError> {
        if dx == Length::ZERO && dy == Length::ZERO {
            return Ok(());
        }
        for object in self.unlocked(ids)? {
            self.replace_object(translated(&object, dx, dy))?;
        }
        Ok(())
    }

    /// Puts an object at a frame and an anchor, as when it is dragged to
    /// another page, or resized; the objects of a group are scaled with
    /// it. A locked object is left as it is.
    pub fn place_object(
        &mut self,
        id: NodeId,
        anchor: Anchor,
        frame: Frame,
    ) -> Result<(), ModelError> {
        let object = self.object(id)?;
        if object.locked {
            return Ok(());
        }
        let mut placed = reframed(object, frame);
        placed.anchor = anchor;
        self.replace_object(placed)
    }

    /// Sets how far objects are rotated, in hundredths of a degree
    /// counterclockwise, from 0 up to a whole turn (OBJ-03).
    pub fn rotate_objects(&mut self, ids: &[NodeId], rotation: i32) -> Result<(), ModelError> {
        let rotation = rotation.rem_euclid(36_000);
        for mut object in self.unlocked(ids)? {
            object.rotation = rotation;
            self.replace_object(object)?;
        }
        Ok(())
    }

    /// Flips objects left to right, or upside down (OBJ-03).
    pub fn flip_objects(&mut self, ids: &[NodeId], horizontally: bool) -> Result<(), ModelError> {
        for mut object in self.unlocked(ids)? {
            if horizontally {
                object.flip_horizontal = !object.flip_horizontal;
            } else {
                object.flip_vertical = !object.flip_vertical;
            }
            self.replace_object(object)?;
        }
        Ok(())
    }

    /// Locks objects, or unlocks them (OBJ-07).
    pub fn lock_objects(&mut self, ids: &[NodeId], locked: bool) -> Result<(), ModelError> {
        for &id in ids {
            let mut object = self.object(id)?.clone();
            if object.locked != locked {
                object.locked = locked;
                self.replace_object(object)?;
            }
        }
        Ok(())
    }

    /// Removes objects, leaving the locked ones. A linked text box
    /// leaves the boxes before and after it linked to each other, and the
    /// first of linked boxes leaves its text to the next (OBJ-13).
    pub fn delete_objects(&mut self, ids: &[NodeId]) -> Result<(), ModelError> {
        for object in self.unlocked(ids)? {
            if let ObjectKind::TextBox(text) = &object.kind {
                self.unlink_text_box(object.id, text)?;
            }
            self.remove_object(object.id)?;
        }
        Ok(())
    }

    /// Takes a text box out of the boxes it is linked with.
    fn unlink_text_box(
        &mut self,
        id: NodeId,
        text: &crate::object::TextBox,
    ) -> Result<(), ModelError> {
        let before = self
            .document()
            .objects()
            .iter()
            .find_map(|o| match &o.kind {
                ObjectKind::TextBox(t) if t.next == Some(id) => Some(o.clone()),
                _ => None,
            });
        match before {
            Some(mut before) => {
                if let ObjectKind::TextBox(t) = &mut before.kind {
                    t.next = text.next;
                }
                self.replace_object(before)
            }
            None => {
                let Some(next) = text.next else {
                    return Ok(());
                };
                let Some(mut after) = self.document().objects().get(next).cloned() else {
                    return Ok(());
                };
                if let ObjectKind::TextBox(t) = &mut after.kind {
                    // The paragraphs change boxes, so the box about to
                    // go gives them up first.
                    let mut emptied = self.object(id)?.clone();
                    if let ObjectKind::TextBox(e) = &mut emptied.kind {
                        let style = text
                            .paragraphs
                            .first()
                            .map(crate::text::Paragraph::style)
                            .unwrap_or_else(|| self.document().styles().default_paragraph());
                        let empty = crate::text::Paragraph::new(self.new_id(), style);
                        e.paragraphs = std::sync::Arc::from([empty]);
                        e.next = None;
                    }
                    self.replace_object(emptied)?;
                    t.paragraphs = text.paragraphs.clone();
                }
                self.replace_object(after)
            }
        }
    }

    /// Links a text box after the last of the boxes linked from another,
    /// so that the text that does not fit goes on in it (OBJ-13). Its own
    /// text is added at the end of theirs.
    pub fn link_text_box(&mut self, from: NodeId, to: NodeId) -> Result<(), ModelError> {
        let head = self.document().text_box_head(from);
        let chain = self.document().text_box_chain(head);
        if chain.contains(&to) || self.document().text_box_head(to) != to {
            return Ok(());
        }
        let last = *chain.last().unwrap_or(&head);
        let mut moved = self.object(to)?.clone();
        let ObjectKind::TextBox(added) = &mut moved.kind else {
            return Ok(());
        };
        let text: Vec<crate::text::Paragraph> = added
            .paragraphs
            .iter()
            .filter(|p| !p.is_empty())
            .cloned()
            .collect();
        let style = added
            .paragraphs
            .first()
            .map(crate::text::Paragraph::style)
            .unwrap_or_else(|| self.document().styles().default_paragraph());
        added.paragraphs =
            std::sync::Arc::from([crate::text::Paragraph::new(self.new_id(), style)]);
        self.replace_object(moved)?;
        if !text.is_empty() {
            let mut first = self.object(head)?.clone();
            if let ObjectKind::TextBox(t) = &mut first.kind {
                let mut all = t.paragraphs.to_vec();
                all.extend(text);
                t.paragraphs = all.into();
            }
            self.replace_object(first)?;
        }
        let mut end = self.object(last)?.clone();
        if let ObjectKind::TextBox(t) = &mut end.kind {
            t.next = Some(to);
        }
        self.replace_object(end)
    }

    /// Moves objects in front of others or behind them, keeping their
    /// order among themselves (OBJ-04).
    pub fn layer_objects(&mut self, ids: &[NodeId], layer: Layer) -> Result<(), ModelError> {
        let moving = self.unlocked(ids)?;
        if moving.is_empty() {
            return Ok(());
        }
        let objects = self.document().objects();
        let mut order: Vec<NodeId> = objects.iter().map(|o| o.id).collect();
        let chosen = |id: &NodeId| moving.iter().any(|o| o.id == *id);
        match layer {
            Layer::Front | Layer::Back => {
                let (picked, rest): (Vec<NodeId>, Vec<NodeId>) =
                    order.iter().partition(|id| chosen(id));
                order = if layer == Layer::Front {
                    [rest, picked].concat()
                } else {
                    [picked, rest].concat()
                };
            }
            Layer::Forward => {
                for i in (0..order.len().saturating_sub(1)).rev() {
                    if chosen(&order[i]) && !chosen(&order[i + 1]) {
                        order.swap(i, i + 1);
                    }
                }
            }
            Layer::Backward => {
                for i in 1..order.len() {
                    if chosen(&order[i]) && !chosen(&order[i - 1]) {
                        order.swap(i, i - 1);
                    }
                }
            }
        }
        for (index, id) in order.into_iter().enumerate() {
            if self.document().objects().index_of(id) != Some(index) {
                let object = self.object(id)?.clone();
                self.remove_object(id)?;
                self.insert_object(index, object)?;
            }
        }
        Ok(())
    }

    /// Groups objects into one, where the frontmost of them was, and
    /// returns its identifier (OBJ-06). They must move with the same
    /// thing, and not be locked.
    pub fn group_objects(&mut self, ids: &[NodeId]) -> Result<Option<NodeId>, ModelError> {
        let members = self.unlocked(ids)?;
        if members.len() < 2 {
            return Ok(None);
        }
        let anchor = members[0].anchor;
        if members.iter().any(|o| o.anchor != anchor) {
            return Ok(None);
        }
        let objects = self.document().objects();
        let index = members
            .iter()
            .filter_map(|o| objects.index_of(o.id))
            .max()
            .unwrap_or(0)
            + 1
            - members.len();
        let frame = union(members.iter().map(|o| o.frame)).unwrap_or_default();
        for object in &members {
            self.remove_object(object.id)?;
        }
        let id = self.new_id();
        let mut group = Object::new(id, ObjectKind::Group(Arc::from(members)), frame);
        group.anchor = anchor;
        self.insert_object(index, group)?;
        Ok(Some(id))
    }

    /// Breaks groups into their objects, in their place, and returns the
    /// identifiers of those objects (OBJ-06).
    pub fn ungroup_objects(&mut self, ids: &[NodeId]) -> Result<Vec<NodeId>, ModelError> {
        let mut freed = Vec::new();
        for group in self.unlocked(ids)? {
            let ObjectKind::Group(members) = &group.kind else {
                continue;
            };
            let index = self.document().objects().index_of(group.id).unwrap_or(0);
            self.remove_object(group.id)?;
            for (i, member) in members.iter().enumerate() {
                let mut member = member.clone();
                member.anchor = group.anchor;
                freed.push(member.id);
                self.insert_object(index + i, member)?;
            }
        }
        Ok(freed)
    }

    /// Lines up objects by their frames (OBJ-05).
    pub fn align_objects(
        &mut self,
        ids: &[NodeId],
        alignment: ObjectAlignment,
    ) -> Result<(), ModelError> {
        let objects = self.unlocked(ids)?;
        let Some(all) = union(objects.iter().map(|o| o.frame)) else {
            return Ok(());
        };
        let half = |l: Length| Length::from_millipoints(l.millipoints() / 2);
        for object in objects {
            let f = object.frame;
            let (dx, dy) = match alignment {
                ObjectAlignment::Left => (all.x - f.x, Length::ZERO),
                ObjectAlignment::Center => {
                    (all.x + half(all.width) - f.x - half(f.width), Length::ZERO)
                }
                ObjectAlignment::Right => (all.x + all.width - f.x - f.width, Length::ZERO),
                ObjectAlignment::Top => (Length::ZERO, all.y - f.y),
                ObjectAlignment::Middle => (
                    Length::ZERO,
                    all.y + half(all.height) - f.y - half(f.height),
                ),
                ObjectAlignment::Bottom => (Length::ZERO, all.y + all.height - f.y - f.height),
            };
            if dx != Length::ZERO || dy != Length::ZERO {
                self.replace_object(translated(&object, dx, dy))?;
            }
        }
        Ok(())
    }

    /// Spreads objects so that the room between each and the next is the
    /// same, leaving the first and the last where they are (OBJ-05).
    pub fn distribute_objects(
        &mut self,
        ids: &[NodeId],
        distribution: Distribution,
    ) -> Result<(), ModelError> {
        let mut objects = self.unlocked(ids)?;
        if objects.len() < 3 {
            return Ok(());
        }
        let across = distribution == Distribution::Horizontally;
        let start = |f: &Frame| if across { f.x } else { f.y };
        let size = |f: &Frame| if across { f.width } else { f.height };
        objects.sort_by_key(|o| start(&o.frame));
        let first = &objects[0].frame;
        let last = &objects[objects.len() - 1].frame;
        let span = start(last) + size(last) - start(first);
        let taken: i64 = objects
            .iter()
            .map(|o| size(&o.frame).millipoints() as i64)
            .sum();
        let gaps = (objects.len() - 1) as i64;
        let room = (span.millipoints() as i64 - taken) / gaps;
        let mut at = start(first).millipoints() as i64;
        for object in objects {
            let target = Length::from_millipoints(at as i32);
            let shift = target - start(&object.frame);
            at += size(&object.frame).millipoints() as i64 + room;
            if shift == Length::ZERO {
                continue;
            }
            let (dx, dy) = if across {
                (shift, Length::ZERO)
            } else {
                (Length::ZERO, shift)
            };
            self.replace_object(translated(&object, dx, dy))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::Shape;
    use crate::{Document, Editor};

    fn add(editor: &mut Editor, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        editor
            .edit("Arrange", |e| {
                let id = e.new_id();
                e.add_object(Object::new(
                    id,
                    ObjectKind::Shape(Shape::Rectangle),
                    Frame::from_pt(x, y, w, h),
                ))?;
                Ok(id)
            })
            .unwrap()
    }

    fn frame(editor: &Editor, id: NodeId) -> (f64, f64, f64, f64) {
        let f = editor.document().objects().get(id).unwrap().frame;
        (f.x.to_pt(), f.y.to_pt(), f.width.to_pt(), f.height.to_pt())
    }

    fn order(editor: &Editor) -> Vec<NodeId> {
        editor.document().objects().iter().map(|o| o.id).collect()
    }

    #[test]
    fn obj_03_objects_move_rotate_and_flip_but_not_when_locked() {
        let mut editor = Editor::new(Document::new_word_processing());
        let a = add(&mut editor, 10.0, 10.0, 50.0, 20.0);
        let b = add(&mut editor, 100.0, 10.0, 50.0, 20.0);
        editor
            .edit("Arrange", |e| e.lock_objects(&[b], true))
            .unwrap();
        editor
            .edit("Arrange", |e| {
                e.move_objects(&[a, b], Length::from_pt(5.0), Length::from_pt(-2.0))
            })
            .unwrap();
        assert_eq!(frame(&editor, a), (15.0, 8.0, 50.0, 20.0));
        assert_eq!(frame(&editor, b), (100.0, 10.0, 50.0, 20.0));
        editor
            .edit("Arrange", |e| e.rotate_objects(&[a], -9000))
            .unwrap();
        editor
            .edit("Arrange", |e| e.flip_objects(&[a], true))
            .unwrap();
        let object = editor.document().objects().get(a).unwrap();
        assert_eq!(object.rotation, 27_000);
        assert!(object.flip_horizontal && !object.flip_vertical);
        editor
            .edit("Arrange", |e| e.delete_objects(&[a, b]))
            .unwrap();
        assert_eq!(order(&editor), vec![b]);
        editor.undo().unwrap();
        assert_eq!(order(&editor), vec![a, b]);
    }

    #[test]
    fn obj_04_objects_go_forward_backward_front_and_back() {
        let mut editor = Editor::new(Document::new_word_processing());
        let ids: Vec<NodeId> = (0..4)
            .map(|i| add(&mut editor, i as f64, 0.0, 1.0, 1.0))
            .collect();
        let [a, b, c, d] = ids[..] else {
            unreachable!()
        };
        editor
            .edit("Arrange", |e| e.layer_objects(&[a], Layer::Forward))
            .unwrap();
        assert_eq!(order(&editor), vec![b, a, c, d]);
        editor
            .edit("Arrange", |e| e.layer_objects(&[b, c], Layer::Front))
            .unwrap();
        assert_eq!(order(&editor), vec![a, d, b, c]);
        editor
            .edit("Arrange", |e| e.layer_objects(&[c], Layer::Back))
            .unwrap();
        assert_eq!(order(&editor), vec![c, a, d, b]);
        editor
            .edit("Arrange", |e| e.layer_objects(&[b], Layer::Backward))
            .unwrap();
        assert_eq!(order(&editor), vec![c, a, b, d]);
        editor.undo().unwrap();
        assert_eq!(order(&editor), vec![c, a, d, b]);
    }

    #[test]
    fn obj_06_objects_are_grouped_resized_and_ungrouped() {
        let mut editor = Editor::new(Document::new_word_processing());
        let back = add(&mut editor, 0.0, 0.0, 5.0, 5.0);
        let a = add(&mut editor, 10.0, 10.0, 10.0, 10.0);
        let b = add(&mut editor, 30.0, 20.0, 10.0, 20.0);
        let group = editor
            .edit("Arrange", |e| e.group_objects(&[a, b]))
            .unwrap()
            .unwrap();
        assert_eq!(order(&editor), vec![back, group]);
        assert_eq!(frame(&editor, group), (10.0, 10.0, 30.0, 30.0));
        editor
            .edit("Arrange", |e| {
                e.place_object(group, Anchor::Page(1), Frame::from_pt(0.0, 0.0, 60.0, 30.0))
            })
            .unwrap();
        let ids = editor
            .edit("Arrange", |e| e.ungroup_objects(&[group]))
            .unwrap();
        assert_eq!(ids, vec![a, b]);
        assert_eq!(order(&editor), vec![back, a, b]);
        assert_eq!(frame(&editor, a), (0.0, 0.0, 20.0, 10.0));
        assert_eq!(frame(&editor, b), (40.0, 10.0, 20.0, 20.0));
        assert_eq!(
            editor.document().objects().get(b).unwrap().anchor,
            Anchor::Page(1)
        );
    }

    #[test]
    fn obj_05_objects_are_aligned_and_distributed() {
        let mut editor = Editor::new(Document::new_word_processing());
        let a = add(&mut editor, 10.0, 0.0, 10.0, 10.0);
        let b = add(&mut editor, 50.0, 30.0, 20.0, 10.0);
        let c = add(&mut editor, 30.0, 60.0, 10.0, 30.0);
        editor
            .edit("Arrange", |e| {
                e.align_objects(&[a, b, c], ObjectAlignment::Right)
            })
            .unwrap();
        assert_eq!(frame(&editor, a).0, 60.0);
        assert_eq!(frame(&editor, b).0, 50.0);
        assert_eq!(frame(&editor, c).0, 60.0);
        editor
            .edit("Arrange", |e| {
                e.align_objects(&[a, b], ObjectAlignment::Middle)
            })
            .unwrap();
        assert_eq!(frame(&editor, a).1, 15.0);
        assert_eq!(frame(&editor, b).1, 15.0);
        let d = add(&mut editor, 0.0, 200.0, 10.0, 10.0);
        // a is at 15 to 25, c 30 high and d at 200 to 210: the room left
        // between them is shared evenly.
        editor
            .edit("Arrange", |e| {
                e.distribute_objects(&[a, c, d], Distribution::Vertically)
            })
            .unwrap();
        assert_eq!(frame(&editor, a).1, 15.0);
        assert_eq!(frame(&editor, c).1, 97.5);
        assert_eq!(frame(&editor, d).1, 200.0);
    }

    #[test]
    fn obj_13_text_boxes_are_linked_and_unlinked() {
        use crate::object::TextBox;
        use crate::story::Story;
        use crate::text::{Paragraph, Run};
        let mut editor = Editor::new(Document::new_word_processing());
        let boxes: Vec<NodeId> = ["One", "Two", ""]
            .iter()
            .map(|text| {
                editor
                    .edit("Add", |e| {
                        let style = e.document().styles().default_paragraph();
                        let paragraph = Paragraph::from_runs(
                            e.new_id(),
                            style,
                            crate::ParaAttrs::default(),
                            [Run::new(text)],
                        )?;
                        let id = e.new_id();
                        e.add_object(Object::new(
                            id,
                            ObjectKind::TextBox(TextBox::new(
                                Arc::from([paragraph]),
                                Length::from_points(4),
                            )),
                            Frame::from_pt(0.0, 0.0, 50.0, 50.0),
                        ))?;
                        Ok(id)
                    })
                    .unwrap()
            })
            .collect();
        let [a, b, c] = boxes[..] else { unreachable!() };
        editor.edit("Link", |e| e.link_text_box(a, b)).unwrap();
        editor.edit("Link", |e| e.link_text_box(b, c)).unwrap();
        let document = editor.document();
        assert_eq!(document.text_box_chain(a), vec![a, b, c]);
        assert_eq!(document.text_box_head(c), a);
        // The text of b went to the end of that of a.
        let text: Vec<String> = document
            .story_paragraphs(Story::TextBox(c))
            .iter()
            .map(|p| p.text().to_string())
            .collect();
        assert_eq!(text, vec!["One", "Two"]);
        assert_eq!(document.text_box_stories(), vec![Story::TextBox(a)]);
        // Linking again, or in a circle, changes nothing.
        editor.edit("Link", |e| e.link_text_box(c, a)).unwrap();
        assert_eq!(editor.document().text_box_chain(a), vec![a, b, c]);
        // Without the box in the middle, the others stay linked; without
        // the first, its text goes on in the next.
        editor.edit("Delete", |e| e.delete_objects(&[b])).unwrap();
        assert_eq!(editor.document().text_box_chain(a), vec![a, c]);
        editor.edit("Delete", |e| e.delete_objects(&[a])).unwrap();
        let document = editor.document();
        assert_eq!(document.text_box_chain(c), vec![c]);
        assert_eq!(document.story_paragraphs(Story::TextBox(c)).len(), 2);
    }
}
