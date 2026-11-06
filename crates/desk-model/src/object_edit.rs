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
    // A masked picture grows and shrinks with its mask (OBJ-04).
    if let Some(mask) = object.picture.mask {
        let scale = |from: Length, to: Length| {
            if from.millipoints() == 0 {
                1.0
            } else {
                to.to_pt() / from.to_pt()
            }
        };
        let (sx, sy) = (
            scale(old.width, frame.width),
            scale(old.height, frame.height),
        );
        changed.picture.mask = Some(Frame::from_pt(
            mask.x.to_pt() * sx,
            mask.y.to_pt() * sy,
            mask.width.to_pt() * sx,
            mask.height.to_pt() * sy,
        ));
    }
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
            if let Some(mark) = self.document().object_mark(object.id) {
                self.remove_mark(mark)?;
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

    /// Adds copies of objects in front of the others, with identifiers
    /// of their own, and returns those identifiers in the same order
    /// (OBJ-22). A connection line among them stays joined to the copies
    /// of the objects it joined. Objects in the text are not copied, as
    /// each is a character of it.
    pub fn add_copies(&mut self, objects: &[Object]) -> Result<Vec<NodeId>, ModelError> {
        let objects: Vec<&Object> = objects
            .iter()
            .filter(|o| o.anchor != Anchor::InText)
            .collect();
        let mut copies = Vec::with_capacity(objects.len());
        for object in &objects {
            copies.push(self.renumbered_object(object)?);
        }
        let copied = |id: Option<NodeId>| {
            let id = id?;
            objects
                .iter()
                .position(|o| o.id == id)
                .map(|i| copies[i].id)
        };
        let joined: Vec<Option<(Option<NodeId>, Option<NodeId>)>> = objects
            .iter()
            .map(|o| match &o.kind {
                ObjectKind::Shape(crate::Shape::Connection(line)) => {
                    Some((copied(line.start), copied(line.end)))
                }
                _ => None,
            })
            .collect();
        let mut ids = Vec::with_capacity(copies.len());
        for (mut copy, joined) in copies.into_iter().zip(joined) {
            if let (Some((start, end)), ObjectKind::Shape(crate::Shape::Connection(line))) =
                (joined, &mut copy.kind)
            {
                line.start = start;
                line.end = end;
            }
            ids.push(copy.id);
            self.add_object(copy)?;
        }
        Ok(ids)
    }

    /// Adds copies of objects moved by `dx` across and `dy` down, as
    /// Duplicate does, and returns their identifiers, from back to front
    /// (OBJ-22).
    pub fn duplicate_objects(
        &mut self,
        ids: &[NodeId],
        dx: Length,
        dy: Length,
    ) -> Result<Vec<NodeId>, ModelError> {
        for &id in ids {
            self.object(id)?;
        }
        let objects: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| ids.contains(&o.id))
            .map(|o| translated(o, dx, dy))
            .collect();
        self.add_copies(&objects)
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
        // Objects in the text are each a character of it.
        if anchor == Anchor::InText || members.iter().any(|o| o.anchor != anchor) {
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
            if group.anchor == Anchor::InText {
                continue;
            }
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

    /// Adds a page after page `after` of a page layout document, moving
    /// the objects of the pages after it down by one (INT-03). A word
    /// processing document is left as it is.
    pub fn add_layout_page(&mut self, after: u32) -> Result<(), ModelError> {
        let mut setup = *self.document().page_setup();
        let Some(pages) = setup.layout_pages else {
            return Ok(());
        };
        let later: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| matches!(o.anchor, Anchor::Page(p) if p > after))
            .cloned()
            .collect();
        for mut object in later {
            if let Anchor::Page(p) = object.anchor {
                object.anchor = Anchor::Page(p + 1);
            }
            self.replace_object(object)?;
        }
        setup.layout_pages = Some(pages.saturating_add(1));
        self.set_page_setup(setup)
    }

    /// Takes page `page` out of a page layout document with the objects
    /// on it, locked ones too, moving the objects of the pages after it up
    /// by one (INT-03). The last page left, and a word processing
    /// document, are left as they are.
    pub fn delete_layout_page(&mut self, page: u32) -> Result<(), ModelError> {
        let mut setup = *self.document().page_setup();
        let Some(pages) = setup.layout_pages else {
            return Ok(());
        };
        if pages <= 1 || page >= pages {
            return Ok(());
        }
        let on_or_after: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| matches!(o.anchor, Anchor::Page(p) if p >= page))
            .cloned()
            .collect();
        for mut object in on_or_after {
            match object.anchor {
                Anchor::Page(p) if p == page => {
                    if let ObjectKind::TextBox(text) = &object.kind {
                        self.unlink_text_box(object.id, text)?;
                    }
                    self.remove_object(object.id)?;
                }
                Anchor::Page(p) => {
                    object.anchor = Anchor::Page(p - 1);
                    self.replace_object(object)?;
                }
                _ => {}
            }
        }
        setup.layout_pages = Some(pages - 1);
        self.set_page_setup(setup)
    }

    /// Takes pages out of a page layout document, by index, with the
    /// objects on them; at least one page is left (INT-03, TXT-02).
    pub fn delete_layout_pages(&mut self, pages: &[u32]) -> Result<(), ModelError> {
        let mut pages = pages.to_vec();
        pages.sort_unstable();
        pages.dedup();
        for &page in pages.iter().rev() {
            self.delete_layout_page(page)?;
        }
        Ok(())
    }

    /// Moves pages of a page layout document, by index, with their
    /// objects, to before page `before` as the pages stood, or to the end
    /// when it is their count (EST-16). Gives back the index of the first
    /// moved.
    pub fn move_layout_pages(&mut self, pages: &[u32], before: u32) -> Result<u32, ModelError> {
        let Some(count) = self.document().page_setup().layout_pages else {
            return Ok(0);
        };
        let moved: Vec<u32> = (0..count).filter(|p| pages.contains(p)).collect();
        if moved.is_empty() {
            return Ok(0);
        }
        let before = before.min(count);
        let mut order: Vec<u32> = (0..count).filter(|p| !moved.contains(p)).collect();
        let at = order.iter().take_while(|&&p| p < before).count();
        order.splice(at..at, moved.iter().copied());
        // Where each page goes.
        let mut to = vec![0; count as usize];
        for (new, &old) in order.iter().enumerate() {
            to[old as usize] = new as u32;
        }
        let objects: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| matches!(o.anchor, Anchor::Page(p) if to.get(p as usize).is_some_and(|&n| n != p)))
            .cloned()
            .collect();
        for mut object in objects {
            if let Anchor::Page(p) = object.anchor {
                object.anchor = Anchor::Page(to[p as usize]);
            }
            self.replace_object(object)?;
        }
        Ok(at as u32)
    }

    /// Puts copies of the pages of a clip, with their objects, before page
    /// `before` of a page layout document, or at the end when it is their
    /// count. Gives back the index of the first.
    pub fn paste_layout_pages(
        &mut self,
        before: u32,
        clip: &LayoutPagesClip,
    ) -> Result<u32, ModelError> {
        let mut setup = *self.document().page_setup();
        let Some(count) = setup.layout_pages else {
            return Ok(0);
        };
        let before = before.min(count);
        let added = u32::try_from(clip.pages.len()).unwrap_or(u32::MAX);
        if added == 0 {
            return Ok(before);
        }
        let later: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| matches!(o.anchor, Anchor::Page(p) if p >= before))
            .cloned()
            .collect();
        for mut object in later {
            if let Anchor::Page(p) = object.anchor {
                object.anchor = Anchor::Page(p.saturating_add(added));
            }
            self.replace_object(object)?;
        }
        setup.layout_pages = Some(count.saturating_add(added));
        self.set_page_setup(setup)?;
        let objects: Vec<Object> = clip
            .pages
            .iter()
            .zip(before..)
            .flat_map(|(objects, page)| {
                objects.iter().map(move |o| Object {
                    anchor: Anchor::Page(page),
                    ..o.clone()
                })
            })
            .collect();
        self.add_copies(&objects)?;
        Ok(before)
    }

    /// Puts copies of pages of a page layout document after the last of
    /// them. Gives back the index of the first copy.
    pub fn duplicate_layout_pages(&mut self, pages: &[u32]) -> Result<u32, ModelError> {
        let clip = self.document().copy_layout_pages(pages);
        let after = pages.iter().copied().max().map_or(0, |p| p + 1);
        self.paste_layout_pages(after, &clip)
    }
}

/// A style that the whole text of an object takes when the object is
/// selected rather than its text (STY-02).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStyling {
    /// A paragraph style, with or without the attributes set over it
    /// (STY-04).
    Paragraph {
        /// The style.
        style: crate::StyleId,
        /// Whether the attributes set over it go.
        clean: bool,
    },
    /// A character style, or none, with or without the attributes set
    /// over it.
    Character {
        /// The style.
        style: Option<crate::StyleId>,
        /// Whether the attributes set over it go.
        clean: bool,
    },
}

impl Edit {
    /// Gives the whole text of an object, a text box or a shape with
    /// text, a paragraph or a character style, as Pages does when the
    /// object is selected (STY-02). Other objects stay as they are.
    pub fn style_object_text(
        &mut self,
        id: NodeId,
        styling: TextStyling,
    ) -> Result<(), ModelError> {
        let Some(mut object) = self.document().objects().get(id).cloned() else {
            return Err(ModelError::UnknownObject(id));
        };
        let Some(text) = object.text_box_mut() else {
            return Ok(());
        };
        let paragraphs = text
            .paragraphs
            .iter()
            .map(|p| {
                let (style, mut attrs) = match styling {
                    TextStyling::Paragraph { style, clean: true } => (style, p.attrs().structure()),
                    TextStyling::Paragraph { style, .. } => (style, p.attrs().clone()),
                    TextStyling::Character { .. } => (p.style(), p.attrs().clone()),
                };
                if let TextStyling::Paragraph { .. } = styling {
                    // A new paragraph style takes the place of a list.
                    attrs.list_style = None;
                }
                let runs = p.runs().cloned().map(|mut run| {
                    match styling {
                        TextStyling::Paragraph { clean: true, .. } => {
                            run.attrs = run.attrs.text_bound()
                        }
                        TextStyling::Character { style, clean } => {
                            run.style = style;
                            if clean {
                                run.attrs = run.attrs.text_bound();
                            }
                        }
                        TextStyling::Paragraph { .. } => {}
                    }
                    run
                });
                crate::text::Paragraph::from_runs(p.id(), style, attrs, runs)
            })
            .collect::<Result<Vec<_>, _>>()?;
        text.paragraphs = paragraphs.into();
        if self.document().objects().get(id) == Some(&object) {
            return Ok(());
        }
        self.replace_object(object)
    }
}

/// Pages of a page layout document, copied with the objects on them, to
/// paste (EST-16).
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutPagesClip {
    /// The objects of each page, from back to front.
    pages: Vec<Vec<Object>>,
}

impl crate::Document {
    /// Copies pages of a page layout document, by index, in order, with
    /// the objects fixed on them.
    pub fn copy_layout_pages(&self, pages: &[u32]) -> LayoutPagesClip {
        let count = self.page_setup().layout_pages.unwrap_or(0);
        let pages = (0..count)
            .filter(|p| pages.contains(p))
            .map(|page| {
                self.objects()
                    .iter()
                    .filter(|o| o.anchor == Anchor::Page(page))
                    .cloned()
                    .collect()
            })
            .collect();
        LayoutPagesClip { pages }
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
    fn obj_22_copies_get_ids_of_their_own_and_keep_their_lines_joined() {
        let mut editor = Editor::new(Document::new_word_processing());
        let a = add(&mut editor, 10.0, 10.0, 50.0, 20.0);
        let b = add(&mut editor, 100.0, 10.0, 50.0, 20.0);
        let line = editor
            .edit("Arrange", |e| {
                let id = e.new_id();
                e.add_object(Object::new(
                    id,
                    ObjectKind::Shape(Shape::Connection(crate::Connection {
                        start: Some(a),
                        end: Some(b),
                        from: (0, 5_000),
                        to: (10_000, 5_000),
                        bend: Length::ZERO,
                    })),
                    Frame::from_pt(60.0, 20.0, 40.0, 0.0),
                ))?;
                Ok(id)
            })
            .unwrap();
        // Duplicate: copies a little lower and to the right, in front.
        let copies = editor
            .edit("Duplicate", |e| {
                e.duplicate_objects(&[line, a, b], Length::from_pt(9.0), Length::from_pt(9.0))
            })
            .unwrap();
        assert_eq!(copies.len(), 3);
        assert_eq!(order(&editor)[..3], [a, b, line]);
        assert_eq!(order(&editor)[3..], copies[..]);
        assert!(copies.iter().all(|c| ![a, b, line].contains(c)));
        assert_eq!(frame(&editor, copies[0]), (19.0, 19.0, 50.0, 20.0));
        let ObjectKind::Shape(Shape::Connection(joined)) =
            &editor.document().objects().get(copies[2]).unwrap().kind
        else {
            panic!("a line");
        };
        assert_eq!(
            (joined.start, joined.end),
            (Some(copies[0]), Some(copies[1]))
        );
        // A line copied without its objects joins nothing.
        let alone = editor
            .edit("Paste", |e| {
                let line = e.document().objects().get(line).unwrap().clone();
                e.add_copies(&[line])
            })
            .unwrap();
        let ObjectKind::Shape(Shape::Connection(loose)) =
            &editor.document().objects().get(alone[0]).unwrap().kind
        else {
            panic!("a line");
        };
        assert_eq!((loose.start, loose.end), (None, None));
        editor.undo().unwrap();
        editor.undo().unwrap();
        assert_eq!(order(&editor), vec![a, b, line]);
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

    #[test]
    fn int_03_pages_are_added_and_taken_out_with_their_objects() {
        let mut editor = Editor::new(Document::new_page_layout());
        editor.edit("Add Page", |e| e.add_layout_page(0)).unwrap();
        let a = add(&mut editor, 10.0, 10.0, 20.0, 20.0);
        let b = add(&mut editor, 10.0, 10.0, 20.0, 20.0);
        editor
            .edit("Move", |e| {
                let frame = e.document().objects().get(b).unwrap().frame;
                e.place_object(b, Anchor::Page(1), frame)
            })
            .unwrap();
        // A page between them moves the second down.
        editor.edit("Add Page", |e| e.add_layout_page(0)).unwrap();
        let anchor = |editor: &Editor, id| editor.document().objects().get(id).map(|o| o.anchor);
        assert_eq!(editor.document().page_setup().layout_pages, Some(3));
        assert_eq!(anchor(&editor, a), Some(Anchor::Page(0)));
        assert_eq!(anchor(&editor, b), Some(Anchor::Page(2)));
        // Taking out the first takes its object with it.
        editor
            .edit("Delete Page", |e| e.delete_layout_page(0))
            .unwrap();
        assert_eq!(editor.document().page_setup().layout_pages, Some(2));
        assert_eq!(anchor(&editor, a), None);
        assert_eq!(anchor(&editor, b), Some(Anchor::Page(1)));
        editor
            .edit("Delete Page", |e| e.delete_layout_page(0))
            .unwrap();
        // The last page stays.
        editor
            .edit("Delete Page", |e| e.delete_layout_page(0))
            .unwrap();
        assert_eq!(editor.document().page_setup().layout_pages, Some(1));
        assert_eq!(anchor(&editor, b), Some(Anchor::Page(0)));
        // A word processing document has no pages of its own to add.
        let mut editor = Editor::new(Document::new_word_processing());
        editor.edit("Add Page", |e| e.add_layout_page(0)).unwrap();
        assert_eq!(editor.document().page_setup().layout_pages, None);
    }

    #[test]
    fn est_16_pages_of_a_page_layout_document_move_copy_and_go_as_sections() {
        let mut editor = Editor::new(Document::new_page_layout());
        editor
            .edit("Pages", |e| {
                e.add_layout_page(0)?;
                e.add_layout_page(1)
            })
            .unwrap();
        let mut on = Vec::new();
        for page in 0..3 {
            let id = add(&mut editor, 10.0, 10.0, 20.0, 20.0);
            editor
                .edit("Move", |e| {
                    let frame = e.document().objects().get(id).unwrap().frame;
                    e.place_object(id, Anchor::Page(page), frame)
                })
                .unwrap();
            on.push(id);
        }
        let anchor = |editor: &Editor, id| editor.document().objects().get(id).map(|o| o.anchor);
        // The last page moves first.
        let first = editor
            .edit("Move Pages", |e| e.move_layout_pages(&[2], 0))
            .unwrap();
        assert_eq!(first, 0);
        let anchors: Vec<_> = on.iter().map(|&id| anchor(&editor, id)).collect();
        assert_eq!(
            anchors,
            [
                Some(Anchor::Page(1)),
                Some(Anchor::Page(2)),
                Some(Anchor::Page(0))
            ]
        );
        // A copy of the first page goes after it, with a copy of its object.
        let copy = editor
            .edit("Duplicate", |e| e.duplicate_layout_pages(&[0]))
            .unwrap();
        assert_eq!(copy, 1);
        let document = editor.document();
        assert_eq!(document.page_setup().layout_pages, Some(4));
        assert_eq!(
            document
                .objects()
                .iter()
                .filter(|o| o.anchor == Anchor::Page(1))
                .count(),
            1
        );
        assert_eq!(anchor(&editor, on[0]), Some(Anchor::Page(2)));
        // Two pages go at once, and at least one page stays.
        editor
            .edit("Delete Pages", |e| e.delete_layout_pages(&[0, 1]))
            .unwrap();
        assert_eq!(editor.document().page_setup().layout_pages, Some(2));
        assert_eq!(anchor(&editor, on[0]), Some(Anchor::Page(0)));
        editor
            .edit("Delete Pages", |e| e.delete_layout_pages(&[0, 1]))
            .unwrap();
        assert_eq!(editor.document().page_setup().layout_pages, Some(1));
    }

    #[test]
    fn doc_20_statistics_count_the_text_of_objects_and_the_graphics() {
        use crate::object::TextBox;
        use crate::text::{Paragraph, Run};
        let mut editor = Editor::new(Document::new_page_layout());
        let boxes: Vec<NodeId> = ["Três palavras aqui", ""]
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
        editor
            .edit("Link", |e| e.link_text_box(boxes[0], boxes[1]))
            .unwrap();
        add(&mut editor, 100.0, 100.0, 20.0, 20.0);
        let statistics = editor.document().statistics();
        assert_eq!(statistics.words, 3);
        assert_eq!(statistics.graphics, 3);
        // The empty paragraph of the body and the one of the text.
        assert_eq!(statistics.paragraphs, 2);
    }

    #[test]
    fn sty_02_a_selected_text_box_takes_a_style_in_all_its_text() {
        use crate::object::TextBox;
        use crate::style::ParagraphStyle;
        use crate::text::{Paragraph, Run};
        let mut document = Document::new_word_processing();
        let heading = document.new_id();
        document
            .put_style(ParagraphStyle::new(heading, "Heading"))
            .unwrap();
        let mut editor = Editor::new(document);
        let id = editor
            .edit("Add", |e| {
                let style = e.document().styles().default_paragraph();
                let mut paragraphs = Vec::new();
                for text in ["One", "Two"] {
                    let mut run = Run::new(text);
                    run.attrs.italic = Some(true);
                    paragraphs.push(Paragraph::from_runs(
                        e.new_id(),
                        style,
                        crate::ParaAttrs::default(),
                        [run],
                    )?);
                }
                let id = e.new_id();
                e.add_object(Object::new(
                    id,
                    ObjectKind::TextBox(TextBox::new(paragraphs.into(), Length::from_points(4))),
                    Frame::from_pt(0.0, 0.0, 50.0, 50.0),
                ))?;
                Ok(id)
            })
            .unwrap();
        editor
            .edit("Paragraph Style", |e| {
                e.style_object_text(
                    id,
                    TextStyling::Paragraph {
                        style: heading,
                        clean: true,
                    },
                )
            })
            .unwrap();
        let object = editor.document().objects().get(id).unwrap().clone();
        let paragraphs: Vec<_> = object.paragraphs().collect();
        assert!(paragraphs.iter().all(|p| p.style() == heading));
        // The bold set over the style goes with Clean.
        assert!(
            paragraphs
                .iter()
                .flat_map(|p| p.runs())
                .all(|r| r.attrs.italic.is_none())
        );
    }

    #[test]
    fn sty_09_replacing_a_style_reaches_text_boxes() {
        use crate::object::TextBox;
        use crate::style::ParagraphStyle;
        use crate::text::{Paragraph, Run};
        let mut document = Document::new_word_processing();
        let quote = document.new_id();
        document
            .put_style(ParagraphStyle::new(quote, "Quote"))
            .unwrap();
        let mut editor = Editor::new(document);
        let id = editor
            .edit("Add", |e| {
                let paragraph = Paragraph::from_runs(
                    e.new_id(),
                    quote,
                    crate::ParaAttrs::default(),
                    [Run::new("Hi")],
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
            .unwrap();
        let body = editor.document().styles().default_paragraph();
        let stories = editor.document().text_box_stories();
        let count = editor
            .edit("Replace Style", |e| {
                e.replace_style_uses(quote, Some(body), &stories)
            })
            .unwrap();
        assert_eq!(count, 1);
        let object = editor.document().objects().get(id).unwrap();
        assert!(object.paragraphs().all(|p| p.style() == body));
    }
}
