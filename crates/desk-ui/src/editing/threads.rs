// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The threads of text boxes, as Pages shows them (PXV-13).
//!
//! A selected text box has a circle above the middle of its top edge.
//! A click on the circle of a box in no thread starts one, numbered 1;
//! while a box of a thread is selected, the boxes in no thread show their
//! circle too, and a click on one adds it at the end of the thread. A box
//! in a thread shows its place in it, in the color of the thread, and a
//! click on the number of the selected box opens a menu with the threads
//! of the document, to move the box to another one, Layout Order, Create
//! New Thread and Remove from Thread.

use desk_layout::DocumentLayout;
use desk_model::{Document, NodeId, ObjectKind, Rgba};
use gtk::prelude::*;

use super::TextEditor;
use super::objects::find;
use crate::i18n::tr;
use crate::view::{THREAD_RADIUS, ThreadMark};

/// What an item of the menu of a thread does to the selected box.
type ThreadAction = fn(&TextEditor, NodeId);

/// How far above the top edge of a box its circle is, in points, so that
/// it leaves the handle of the edge free.
const ABOVE: f32 = 16.0;

/// The colors of the threads, by their number, again from the first after
/// the last.
const COLORS: [Rgba; 8] = [
    Rgba::rgb(0x1c, 0x71, 0xd8),
    Rgba::rgb(0xe0, 0x1b, 0x24),
    Rgba::rgb(0x2e, 0xc2, 0x7e),
    Rgba::rgb(0xe6, 0x61, 0x00),
    Rgba::rgb(0x91, 0x41, 0xac),
    Rgba::rgb(0xc6, 0x46, 0x00),
    Rgba::rgb(0x26, 0xa2, 0x69),
    Rgba::rgb(0x86, 0x5e, 0x3c),
];

/// The color of the thread at a place among those of the document.
pub(super) fn thread_color(index: usize) -> Rgba {
    COLORS[index % COLORS.len()]
}

/// The circles of the threads while `selected` is the one text box
/// selected, with the box each is on.
pub(super) fn thread_marks(
    document: &Document,
    layout: &DocumentLayout,
    selected: NodeId,
) -> Vec<(NodeId, ThreadMark)> {
    if !matches!(
        document.objects().get(selected).map(|o| &o.kind),
        Some(ObjectKind::TextBox(_))
    ) {
        return Vec::new();
    }
    let threads = document.text_box_threads();
    let threaded = threads.iter().any(|t| t.contains(&selected));
    let mut marks = Vec::new();
    for (page, page_layout) in layout.pages.iter().enumerate() {
        for placed in &page_layout.objects {
            let id = placed.object.id;
            if !matches!(placed.object.kind, ObjectKind::TextBox(_)) {
                continue;
            }
            let place = threads
                .iter()
                .enumerate()
                .find_map(|(i, t)| Some((i, t.iter().position(|&b| b == id)?)));
            let (number, color) = match place {
                Some((thread, at)) => (Some(at + 1), thread_color(thread)),
                // A box in no thread shows its circle when it is the one
                // selected, or can join the thread of the selected one.
                None if id == selected || (threaded && !placed.object.locked) => {
                    (None, Rgba::rgb(0x35, 0x84, 0xe4))
                }
                None => continue,
            };
            let (x, y) = placed.to_page(placed.width / 2.0, -ABOVE);
            marks.push((
                id,
                ThreadMark {
                    page,
                    at: (f64::from(x), f64::from(y)),
                    number,
                    color,
                },
            ));
        }
    }
    marks
}

/// The boxes of a thread in the order of the pages: by page, then from
/// top to bottom and from left to right (Layout Order).
pub(super) fn layout_order(layout: &DocumentLayout, chain: &[NodeId]) -> Vec<NodeId> {
    let mut order = chain.to_vec();
    let place = |id: &NodeId| {
        find(layout, *id).map_or((usize::MAX, f32::MAX, f32::MAX), |(page, placed)| {
            (page, placed.y, placed.x)
        })
    };
    order.sort_by(|a, b| {
        let (a, b) = (place(a), place(b));
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    order
}

impl TextEditor {
    /// The one text box selected, if only one is.
    fn selected_text_box(&self) -> Option<NodeId> {
        let state = self.state.borrow();
        if state.object_drag.is_some() {
            return None;
        }
        match state.objects[..] {
            [id] => Some(id),
            _ => None,
        }
    }

    /// Shows the circles of the threads around the selected text box.
    pub(super) fn show_threads(&self) {
        let marks = match self.selected_text_box() {
            Some(selected) => {
                let state = self.state.borrow();
                thread_marks(state.editor.document(), &state.layout, selected)
                    .into_iter()
                    .map(|(_, mark)| mark)
                    .collect()
            }
            None => Vec::new(),
        };
        self.view.set_thread_marks(marks);
    }

    /// Answers a press on the circle of a thread: it starts a thread with
    /// the selected box, adds another box to its thread, or opens the menu
    /// of the selected box. Whether the press was on one.
    pub(super) fn press_thread(&self, x: f64, y: f64) -> bool {
        let Some(selected) = self.selected_text_box() else {
            return false;
        };
        let hit = {
            let state = self.state.borrow();
            thread_marks(state.editor.document(), &state.layout, selected)
                .into_iter()
                .rev()
                .find(|(_, mark)| {
                    self.view
                        .to_area(mark.page, mark.at.0, mark.at.1)
                        .is_some_and(|(mx, my)| (mx - x).hypot(my - y) <= THREAD_RADIUS)
                })
        };
        let Some((id, mark)) = hit else {
            return false;
        };
        match (id == selected, mark.number) {
            (true, None) => self.arrange_objects("Start Thread", |edit, _| {
                edit.start_text_box_thread(selected)?;
                Ok(None)
            }),
            (true, Some(_)) => self.thread_menu(selected, &mark),
            (false, None) => self.arrange_objects("Add to Thread", |edit, _| {
                edit.move_to_text_box_thread(id, selected)?;
                Ok(None)
            }),
            // The number of another box: a click selects that box.
            (false, Some(_)) => return false,
        }
        true
    }

    /// Puts the boxes of the thread of the selected text box in the order
    /// of the pages (Layout Order).
    pub fn order_thread(&self) {
        let Some(selected) = self.selected_text_box() else {
            return;
        };
        let order = {
            let state = self.state.borrow();
            let document = state.editor.document();
            let chain = document.text_box_chain(document.text_box_head(selected));
            layout_order(&state.layout, &chain)
        };
        self.arrange_objects("Layout Order", |edit, _| {
            edit.order_text_box_thread(&order)?;
            Ok(None)
        });
    }

    /// Opens the menu of the thread of the selected text box, below its
    /// number.
    fn thread_menu(&self, selected: NodeId, mark: &ThreadMark) {
        let Some((ax, ay)) = self.view.to_area(mark.page, mark.at.0, mark.at.1) else {
            return;
        };
        let (threads, names) = {
            let state = self.state.borrow();
            let document = state.editor.document();
            let threads = document.text_box_threads();
            let names: Vec<String> = threads
                .iter()
                .map(|t| document.text_box_thread_name(t[0]))
                .collect();
            (threads, names)
        };
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let popover = gtk::Popover::builder()
            .child(&list)
            .position(gtk::PositionType::Bottom)
            .autohide(true)
            .build();
        popover.add_css_class("menu");
        popover.set_parent(self.view.area());
        let item = |label: &str, active: bool| {
            let button = gtk::Button::builder().has_frame(false).build();
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.set_opacity(if active { 1.0 } else { 0.0 });
            row.append(&check);
            let text = gtk::Label::new(Some(label));
            text.set_xalign(0.0);
            row.append(&text);
            button.set_child(Some(&row));
            list.append(&button);
            button
        };
        for (i, (thread, name)) in threads.iter().zip(&names).enumerate() {
            let label = if name.is_empty() {
                tr("Thread {number}").replace("{number}", &(i + 1).to_string())
            } else {
                format!("{}  {name}", i + 1)
            };
            let button = item(&label, thread.contains(&selected));
            let head = thread[0];
            let current = thread.contains(&selected);
            let weak = self.downgrade();
            let menu = popover.clone();
            button.connect_clicked(move |_| {
                menu.popdown();
                let Some(this) = weak.upgrade() else { return };
                if !current {
                    this.arrange_objects("Add to Thread", |edit, _| {
                        edit.move_to_text_box_thread(selected, head)?;
                        Ok(None)
                    });
                }
            });
        }
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let actions: [(String, ThreadAction); 3] = [
            (tr("Layout Order"), |this, _| this.order_thread()),
            (tr("Create New Thread"), |this, id| {
                this.arrange_objects("Create New Thread", |edit, _| {
                    edit.new_text_box_thread(id)?;
                    Ok(None)
                });
            }),
            (tr("Remove from Thread"), |this, id| {
                this.arrange_objects("Remove from Thread", |edit, _| {
                    edit.remove_from_text_box_thread(id)?;
                    Ok(None)
                });
            }),
        ];
        for (label, action) in actions {
            let button = item(&label, false);
            let weak = self.downgrade();
            let menu = popover.clone();
            button.connect_clicked(move |_| {
                menu.popdown();
                if let Some(this) = weak.upgrade() {
                    action(&this, selected);
                }
            });
        }
        popover.connect_closed(|popover| {
            let popover = popover.clone();
            gtk::glib::idle_add_local_once(move || popover.unparent());
        });
        let r = THREAD_RADIUS as i32;
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
            ax as i32 - r,
            ay as i32 - r,
            2 * r,
            2 * r,
        )));
        popover.popup();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use desk_model::{Editor, Frame, Length, Object, Paragraph, TextBox};

    use super::*;

    #[test]
    fn pxv_13_text_boxes_show_their_threads() {
        let mut editor = Editor::new(Document::new_page_layout());
        let mut add = |y: f64| {
            editor
                .edit("Add", |edit| {
                    let style = edit.document().styles().default_paragraph();
                    let empty = Paragraph::new(edit.new_id(), style);
                    let id = edit.new_id();
                    edit.add_object(Object::new(
                        id,
                        ObjectKind::TextBox(TextBox::new(
                            Arc::from([empty]),
                            Length::from_points(4),
                        )),
                        Frame::from_pt(100.0, y, 150.0, 60.0),
                    ))?;
                    Ok(id)
                })
                .unwrap()
        };
        let (a, b, c) = (add(400.0), add(300.0), add(200.0));
        let numbers = |editor: &Editor| -> Vec<(NodeId, Option<usize>)> {
            let layout = crate::lay_out(editor.document());
            thread_marks(editor.document(), &layout, a)
                .into_iter()
                .map(|(id, mark)| (id, mark.number))
                .collect()
        };
        // A box in no thread shows its empty circle, above its top edge.
        assert_eq!(numbers(&editor), vec![(a, None)]);
        let layout = crate::lay_out(editor.document());
        let (_, mark) = thread_marks(editor.document(), &layout, a)[0];
        assert_eq!(mark.at, (175.0, 400.0 - f64::from(ABOVE)));
        // In a thread, it shows its place, and the boxes in no thread
        // their circle, to be added.
        editor
            .edit("Start Thread", |e| e.start_text_box_thread(a))
            .unwrap();
        editor
            .edit("Add to Thread", |e| e.move_to_text_box_thread(c, a))
            .unwrap();
        let mut shown = numbers(&editor);
        shown.sort();
        let mut expected = vec![(a, Some(1)), (b, None), (c, Some(2))];
        expected.sort();
        assert_eq!(shown, expected);
        let layout = crate::lay_out(editor.document());
        let colors: Vec<Rgba> = thread_marks(editor.document(), &layout, a)
            .into_iter()
            .filter(|(_, m)| m.number.is_some())
            .map(|(_, m)| m.color)
            .collect();
        assert_eq!(colors, vec![thread_color(0); 2]);
        // Layout Order puts the box higher on the page first.
        assert_eq!(layout_order(&layout, &[a, c]), vec![c, a]);
        // Nothing shows for an object that is not a text box.
        assert!(thread_marks(editor.document(), &layout, NodeId::new(9999).unwrap()).is_empty());
    }
}
