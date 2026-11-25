// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Objects moved aside as a table grows (TAB-09), as Pages '09 did and
//! as Numbers 15.4 still does (NTAB-15): when rows or columns make a
//! table larger, the objects beside it that it would now cover move
//! right, and those under it move down, by as much as it grew, keeping
//! the room between them. Objects it already covered, locked ones and
//! those measured from another place stay where they are.

use crate::error::ModelError;
use crate::id::NodeId;
use crate::object::Frame;
use crate::transaction::Edit;

/// The edges of a frame, in points: left, top, right and bottom.
fn edges(frame: &Frame) -> (f64, f64, f64, f64) {
    let (x, y) = (frame.x.to_pt(), frame.y.to_pt());
    (x, y, x + frame.width.to_pt(), y + frame.height.to_pt())
}

/// Whether two frames overlap, more than by touching.
fn overlap(a: &Frame, b: &Frame) -> bool {
    let (al, at, ar, ab) = edges(a);
    let (bl, bt, br, bb) = edges(b);
    al < br && bl < ar && at < bb && bt < ab
}

/// Where an object goes when a table grows from `old` to `new`: aside,
/// when the table would cover it now and did not before.
fn moved_aside(old: &Frame, new: &Frame, other: &Frame) -> Option<Frame> {
    if overlap(old, other) || !overlap(new, other) {
        return None;
    }
    let (_, _, old_right, old_bottom) = edges(old);
    let (_, _, new_right, new_bottom) = edges(new);
    let (left, top, _, _) = edges(other);
    let mut out = *other;
    if left >= old_right && new_right > old_right {
        out.x = crate::Length::from_pt(left + new_right - old_right);
    } else if top >= old_bottom && new_bottom > old_bottom {
        out.y = crate::Length::from_pt(top + new_bottom - old_bottom);
    } else {
        return None;
    }
    Some(out)
}

impl Edit {
    /// Moves aside the objects that the object `id` now covers, since its
    /// frame grew from `old` (TAB-09).
    pub fn move_aside(&mut self, id: NodeId, old: Frame) -> Result<(), ModelError> {
        let Some(grown) = self.document().objects().get(id).cloned() else {
            return Ok(());
        };
        let moved: Vec<_> = self
            .document()
            .objects()
            .iter()
            .filter(|o| o.id != id && !o.locked && o.anchor == grown.anchor)
            .filter_map(|o| {
                let frame = moved_aside(&old, &grown.frame, &o.frame)?;
                let mut o = o.clone();
                o.frame = frame;
                Some(o)
            })
            .collect();
        // Moved aside, they are in no tracked change of the user (W148).
        self.untracked(|edit| {
            for object in moved {
                edit.replace_object(object)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_09_a_growing_table_moves_objects_aside() {
        let old = Frame::from_pt(100.0, 100.0, 200.0, 100.0);
        let wider = Frame::from_pt(100.0, 100.0, 260.0, 100.0);
        let taller = Frame::from_pt(100.0, 100.0, 200.0, 140.0);
        let right = Frame::from_pt(320.0, 120.0, 50.0, 50.0);
        let below = Frame::from_pt(120.0, 220.0, 50.0, 50.0);
        // The object on the right moves right by as much as the table grew,
        // keeping the room between them.
        assert_eq!(
            moved_aside(&old, &wider, &right),
            Some(Frame::from_pt(380.0, 120.0, 50.0, 50.0))
        );
        // The one below moves down.
        assert_eq!(
            moved_aside(&old, &taller, &below),
            Some(Frame::from_pt(120.0, 260.0, 50.0, 50.0))
        );
        // One the table does not reach, or already covered, stays.
        assert_eq!(moved_aside(&old, &wider, &below), None);
        let under = Frame::from_pt(150.0, 150.0, 20.0, 20.0);
        assert_eq!(moved_aside(&old, &wider, &under), None);
    }
}
