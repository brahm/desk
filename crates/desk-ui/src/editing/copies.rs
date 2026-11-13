// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Copying objects, as Pages does (OBJ-21, OBJ-22): Copy, Cut and Paste
//! of the selected objects, Duplicate, which puts a copy a little lower
//! and to the right, a drag with Alt that leaves a copy where it drops,
//! and selecting the floating objects, all of them or those a drag draws
//! a rectangle over. Other apps of Desk get the objects too, in the format
//! of the app, and paste them with the styles of their text (IOP-27,
//! IOP-28).

use std::cell::RefCell;

use desk_model::{Anchor, Length, NodeId, Object, ObjectKind};
use gtk::gdk;
use gtk::prelude::*;

use super::TextEditor;
use super::objects::{find, on_text};
use crate::view::Marquee;

/// How far Duplicate puts the copy from the object, across and down, in
/// points.
pub(super) const COPY_OFFSET: f64 = 10.0;

/// The objects last copied in Desk, fixed on the page they were on, by
/// its index, while the clipboard still holds what Desk put there.
#[derive(Clone, Debug)]
struct CopiedObjects {
    page: usize,
    objects: Vec<Object>,
    /// The document they are from, whose styles their text names.
    document: desk_model::Document,
}

/// A drag that draws a rectangle on a page and selects the objects it
/// touches (OBJ-21).
#[derive(Clone, Debug)]
pub(super) struct MarqueeDrag {
    page: usize,
    /// Where it started, in points on the page.
    start: (f32, f32),
    /// The objects selected before, which it adds to when Shift or Ctrl
    /// was held.
    before: Vec<NodeId>,
}

thread_local! {
    static COPIED_OBJECTS: RefCell<Option<CopiedObjects>> = const { RefCell::new(None) };
}

/// Forgets the objects copied, as text was copied after them.
pub(super) fn forget_copied_objects() {
    COPIED_OBJECTS.with(|c| *c.borrow_mut() = None);
}

impl TextEditor {
    /// The selected objects as they stand on their page: an object that
    /// moves with a paragraph is fixed where it is, and one of a master
    /// stays on the master. The page is that of the first.
    fn selected_on_page(&self) -> Option<CopiedObjects> {
        let state = self.state.borrow();
        let mut page = None;
        let objects: Vec<Object> = state
            .objects
            .iter()
            .filter_map(|&id| find(&state.layout, id))
            .map(|(on, placed)| {
                page.get_or_insert(on);
                let object = &placed.object;
                match object.anchor {
                    Anchor::Paragraph(_) => {
                        let dx = Length::from_pt(f64::from(placed.x)) - object.frame.x;
                        let dy = Length::from_pt(f64::from(placed.y)) - object.frame.y;
                        let mut fixed = desk_model::translated(object, dx, dy);
                        fixed.anchor = Anchor::Page(on as u32);
                        fixed
                    }
                    _ => object.clone(),
                }
            })
            .collect();
        Some(CopiedObjects {
            page: page?,
            objects,
            document: state.editor.document().clone(),
        })
    }

    /// Copies the selected objects for Paste, and gives other apps their
    /// text, or the picture when one picture is selected (OBJ-22).
    pub(super) fn copy_objects(&self) -> bool {
        let Some(copied) = self.selected_on_page() else {
            return false;
        };
        let text: Vec<String> = copied
            .objects
            .iter()
            .filter_map(Object::text_box)
            .map(|text| {
                text.paragraphs
                    .iter()
                    .map(|p| p.text())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        let text = text.join("\n");
        let clipboard = self.view.area().clipboard();
        let picture = match &copied.objects[..] {
            [
                Object {
                    kind: ObjectKind::Image(picture),
                    ..
                },
            ] => gdk::Texture::from_bytes(&gtk::glib::Bytes::from(picture.data())).ok(),
            _ => None,
        };
        let mut content = vec![match picture {
            Some(texture) => gdk::ContentProvider::for_value(&texture.to_value()),
            None => gdk::ContentProvider::for_value(&text.to_value()),
        }];
        // For the other apps of Desk.
        if let Some(format) = crate::objects_format() {
            match (format.write)(&copied.document, &copied.objects) {
                Ok(bytes) => content.push(gdk::ContentProvider::for_bytes(
                    format.media_type,
                    &gtk::glib::Bytes::from_owned(bytes),
                )),
                Err(error) => gtk::glib::g_warning!("desk", "cannot copy the objects: {error}"),
            }
        }
        if let Err(error) = clipboard.set_content(Some(&gdk::ContentProvider::new_union(&content)))
        {
            gtk::glib::g_warning!("desk", "cannot copy: {error}");
        }
        super::forget_copied_text();
        COPIED_OBJECTS.with(|c| *c.borrow_mut() = Some(copied));
        true
    }

    /// Copies the selected objects and deletes them (OBJ-22).
    pub(super) fn cut_objects(&self) -> bool {
        if !self.copy_objects() {
            return false;
        }
        self.delete_objects();
        true
    }

    /// Pastes the objects last copied in Desk, if the clipboard still
    /// holds them: on the page in sight, where they were on theirs, or a
    /// little lower and to the right when copies already stand there. One
    /// that was in the line of the text goes in at the cursor (OBJ-22).
    pub(super) fn paste_objects(&self) -> bool {
        let clipboard = self.view.area().clipboard();
        if !clipboard.is_local() {
            return self.paste_objects_of_other_apps(&clipboard);
        }
        let Some(copied) = COPIED_OBJECTS.with(|c| c.borrow().clone()) else {
            return false;
        };
        self.paste_copied(&copied);
        true
    }

    /// Pastes the objects that another app of Desk put on the clipboard,
    /// when it holds some (IOP-27, IOP-28).
    fn paste_objects_of_other_apps(&self, clipboard: &gdk::Clipboard) -> bool {
        let Some(format) = crate::objects_format() else {
            return false;
        };
        if !clipboard.formats().contain_mime_type(format.media_type) {
            return false;
        }
        let weak = self.downgrade();
        let clipboard = clipboard.clone();
        gtk::glib::spawn_future_local(async move {
            let read = async {
                let (stream, _) = clipboard
                    .read_future(&[format.media_type], gtk::glib::Priority::DEFAULT)
                    .await
                    .map_err(|e| e.to_string())?;
                let out = gtk::gio::MemoryOutputStream::new_resizable();
                out.splice_future(
                    &stream,
                    gtk::gio::OutputStreamSpliceFlags::CLOSE_SOURCE
                        | gtk::gio::OutputStreamSpliceFlags::CLOSE_TARGET,
                    gtk::glib::Priority::DEFAULT,
                )
                .await
                .map_err(|e| e.to_string())?;
                (format.read)(&out.steal_as_bytes())
            };
            let document = match read.await {
                Ok(document) => document,
                Err(error) => {
                    gtk::glib::g_warning!("desk", "cannot paste the objects: {error}");
                    return;
                }
            };
            let Some(this) = weak.upgrade() else { return };
            let page = this.view.current_page().unwrap_or(0);
            let objects = document.objects().iter().cloned().collect();
            this.paste_copied(&CopiedObjects {
                page,
                objects,
                document,
            });
        });
        true
    }

    /// Pastes copied objects: on the page in sight, where they were on
    /// theirs, or a little lower and to the right when copies already
    /// stand there. One that was in the line of the text goes in at the
    /// cursor (OBJ-22).
    fn paste_copied(&self, copied: &CopiedObjects) {
        let cursor = (!self.has_selected_objects() && self.story().is_none())
            .then(|| {
                let state = self.state.borrow();
                super::at_spot(state.editor.document(), state.selection.ordered().0).ok()
            })
            .flatten();
        self.leave_story(None);
        let page = match cursor {
            Some(_) => self.caret_page(),
            None => self.view.current_page(),
        }
        .unwrap_or(copied.page);
        let offset = self.free_offset(&copied.objects, page);
        let objects: Vec<Object> = copied
            .objects
            .iter()
            .map(|object| {
                let mut placed = desk_model::translated(object, offset, offset);
                if let Anchor::Page(_) | Anchor::InText = object.anchor {
                    placed.anchor = Anchor::Page(page as u32);
                }
                placed
            })
            .collect();
        let in_text: Vec<bool> = copied
            .objects
            .iter()
            .map(|o| o.anchor == Anchor::InText)
            .collect();
        let selection = self.selection();
        let depth = self.state.borrow().editor.undo_depth();
        let result = self.state.borrow_mut().editor.edit("Paste", |edit| {
            let objects = edit.adopt_object_styles(&copied.document, &objects)?;
            let ids = edit.add_copies(&objects)?;
            if let Some(at) = cursor {
                // Those from the text go back in it, one after another.
                for (&id, _) in ids.iter().zip(&in_text).filter(|(_, t)| **t).rev() {
                    edit.put_object_in_text(id, at)?;
                }
            }
            Ok(ids)
        });
        match result {
            Ok(ids) if self.state.borrow().editor.undo_depth() > depth => {
                {
                    let mut state = self.state.borrow_mut();
                    let action = state.next_action;
                    state.next_action += 1;
                    state.undo.push((selection, selection, action));
                    state.redo.clear();
                    state.objects = ids;
                }
                self.after_change(selection);
            }
            Ok(_) => {}
            Err(error) => gtk::glib::g_warning!("desk", "cannot paste: {error}"),
        }
    }

    /// The page the cursor is on.
    fn caret_page(&self) -> Option<usize> {
        let state = self.state.borrow();
        crate::text_nav::caret(&state.layout, state.selection.focus).map(|caret| caret.page)
    }

    /// How far to move copies so that they do not stand on objects just
    /// like them on a page: none, or steps of [`COPY_OFFSET`].
    fn free_offset(&self, objects: &[Object], page: usize) -> Length {
        let state = self.state.borrow();
        let Some(on_page) = state.layout.pages.get(page) else {
            return Length::ZERO;
        };
        let taken = |offset: Length| {
            objects.iter().all(|object| {
                on_page.objects.iter().any(|placed| {
                    let frame = placed.object.frame;
                    placed.object.kind == object.kind
                        && frame.x == object.frame.x + offset
                        && frame.y == object.frame.y + offset
                        && frame.width == object.frame.width
                        && frame.height == object.frame.height
                })
            })
        };
        let step = Length::from_pt(COPY_OFFSET);
        let mut offset = Length::ZERO;
        for _ in 0..50 {
            if !taken(offset) {
                break;
            }
            offset = offset + step;
        }
        offset
    }

    /// Puts copies of the selected objects a little lower and to the
    /// right, in front of the others, and selects them (Edit ▸ Duplicate,
    /// OBJ-22). Objects in the line of the text are not duplicated.
    pub fn duplicate_objects(&self) {
        let offset = Length::from_pt(COPY_OFFSET);
        self.duplicate_objects_by("Duplicate", offset, offset);
    }

    /// Puts copies of the selected objects at a distance from them, in
    /// front of the others, and selects them, as one change with a name:
    /// Duplicate, or Nudge for the arrows with Alt (OBJ-22).
    pub(super) fn duplicate_objects_by(&self, name: &str, dx: Length, dy: Length) {
        let ids: Vec<NodeId> = {
            let state = self.state.borrow();
            let document = state.editor.document();
            state
                .objects
                .iter()
                .copied()
                .filter(|&id| {
                    document
                        .objects()
                        .get(id)
                        .is_some_and(|o| o.anchor != Anchor::InText)
                })
                .collect()
        };
        if ids.is_empty() {
            return;
        }
        self.arrange_objects(name, |edit, _| {
            edit.duplicate_objects(&ids, dx, dy).map(Some)
        });
    }

    /// Adds copies of objects, as a drag with Alt leaves them, and
    /// selects them; the objects stay where they were (OBJ-22).
    pub(super) fn add_dragged_copies(&self, copies: Vec<Object>) {
        let copies: Vec<Object> = copies
            .into_iter()
            .filter(|o| o.anchor != Anchor::InText)
            .collect();
        if copies.is_empty() {
            return;
        }
        self.arrange_objects("Duplicate", |edit, _| edit.add_copies(&copies).map(Some));
    }

    /// Selects every floating object that a click can select, as Select
    /// All does when no text is being typed in (OBJ-21).
    pub(super) fn select_all_objects(&self) {
        let ids: Vec<NodeId> = {
            let state = self.state.borrow();
            let selectable = state.selectable;
            state
                .editor
                .document()
                .objects()
                .iter()
                .filter(|o| o.anchor != Anchor::InText && selectable.allows(o))
                .map(|o| o.id)
                .collect()
        };
        self.leave_story(None);
        self.select_objects(ids);
    }

    /// Whether a press on nothing at a point of a page starts a drag that
    /// selects objects: anywhere off text in a page layout document, and
    /// in a word processing one only off the body, the headers, the
    /// footers and the notes (OBJ-21).
    pub(super) fn marquee_starts(&self, page: usize, x: f32, y: f32) -> bool {
        let layout = self.is_page_layout();
        let state = self.state.borrow();
        let Some(on) = state.layout.pages.get(page) else {
            return false;
        };
        if on_text(&state.layout, page, x, y) {
            return false;
        }
        if layout {
            return true;
        }
        let width = state.layout.setup.width.to_pt() as f32;
        let in_body =
            x >= on.margins.0 && x <= width - on.margins.1 && y >= on.body.0 && y <= on.body.1;
        let in_margin_text = on
            .margin_lines
            .iter()
            .chain(&on.note_lines)
            .any(|l| y >= l.top() && y <= l.bottom());
        !in_body && !in_margin_text
    }

    /// Starts a drag that selects objects, from a point of a page. It
    /// takes the press in a page layout document; in a word processing
    /// one the press still puts the cursor in the text, until the drag
    /// moves.
    pub(super) fn start_marquee(&self, page: usize, start: (f32, f32)) -> bool {
        let layout = self.is_page_layout();
        if layout {
            self.leave_story(None);
        }
        let before = self.selected_objects();
        self.state.borrow_mut().marquee = Some(MarqueeDrag {
            page,
            start,
            before,
        });
        layout
    }

    /// Follows the pointer in a drag that selects objects, to a point of
    /// the area, and selects those the rectangle touches. Whether such a
    /// drag is going on.
    pub(super) fn drag_marquee(&self, x: f64, y: f64) -> bool {
        let Some(drag) = self.state.borrow().marquee.clone() else {
            return false;
        };
        let Some((ox, oy)) = self.view.to_area(drag.page, 0.0, 0.0) else {
            return true;
        };
        let scale = self.view.scale();
        let to = (((x - ox) / scale) as f32, ((y - oy) / scale) as f32);
        let (x0, x1) = (drag.start.0.min(to.0), drag.start.0.max(to.0));
        let (y0, y1) = (drag.start.1.min(to.1), drag.start.1.max(to.1));
        let mut ids = drag.before.clone();
        {
            let state = self.state.borrow();
            if let Some(page) = state.layout.pages.get(drag.page) {
                ids.extend(
                    page.objects
                        .iter()
                        .filter(|o| {
                            o.object.anchor != Anchor::InText && state.selectable.allows(&o.object)
                        })
                        .filter(|o| {
                            let (bx0, by0, bx1, by1) = o.bounds();
                            bx0 <= x1 && bx1 >= x0 && by0 <= y1 && by1 >= y0
                        })
                        .map(|o| o.object.id),
                );
            }
        }
        if !ids.is_empty() {
            // The cursor that the press put in a text gives way.
            if self.story().is_some() {
                self.leave_story(None);
            }
            self.select_objects(ids);
        } else if self.has_selected_objects() {
            self.select_objects(Vec::new());
        }
        let point = |(x, y): (f32, f32)| (f64::from(x), f64::from(y));
        self.view.set_marquee(Some(Marquee {
            page: drag.page,
            from: point(drag.start),
            to: point(to),
        }));
        true
    }

    /// Ends a drag that selects objects.
    pub(super) fn end_marquee(&self) {
        if self.state.borrow_mut().marquee.take().is_some() {
            self.view.set_marquee(None);
        }
    }
}
