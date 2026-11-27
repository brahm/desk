// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The image gallery of Pages 15.4 on the page (W111, PXV-23): put there
//! from Insert > Image Gallery, filled with pictures dropped on it or
//! chosen in the Inspector, and stepped through with the arrows that show
//! over it while it is selected.

use std::sync::Arc;

use desk_layout::PlacedObject;
use desk_model::{Anchor, Edit, Gallery, ModelError, NodeId, Object, ObjectKind, Picture};
use gtk::gio;

use super::TextEditor;
use super::objects::{REACH, find, hit};

/// How large a new gallery is, in points, as in Pages.
const NEW_GALLERY: (f64, f64) = (320.0, 240.0);

/// The radius of the arrows over a selected gallery, in pixels.
pub(crate) const ARROW_RADIUS: f64 = 12.0;

/// Where the arrows of a gallery are, in points on its page: back, then
/// forward. A gallery with one picture or none has none.
pub(super) fn gallery_arrows(placed: &PlacedObject) -> Vec<(f32, f32)> {
    match placed.object.gallery() {
        Some(gallery) if gallery.images.len() > 1 => {
            let inset = (placed.width * 0.08).clamp(14.0, 28.0);
            let middle = placed.height / 2.0;
            vec![
                placed.to_page(inset, middle),
                placed.to_page(placed.width - inset, middle),
            ]
        }
        _ => Vec::new(),
    }
}

/// The picture that an arrow shows: the one before or after that shown,
/// going around at the ends.
fn stepped(gallery: &Gallery, forward: bool) -> usize {
    let count = gallery.images.len().max(1);
    let shown = gallery.shown_index();
    if forward {
        (shown + 1) % count
    } else {
        (shown + count - 1) % count
    }
}

impl TextEditor {
    /// Puts an empty image gallery in the middle of the page shown, and
    /// selects it (W111).
    pub fn insert_gallery(&self) {
        let (page, frame) = self.placed_frame(
            |room| (NEW_GALLERY.0.min(room.0), NEW_GALLERY.1.min(room.1)),
            None,
        );
        self.leave_story(None);
        let selection = self.selection();
        let result = self
            .state
            .borrow_mut()
            .editor
            .edit("Insert Image Gallery", |edit| {
                let id = edit.new_id();
                let mut object =
                    Object::new(id, ObjectKind::Gallery(Arc::new(Gallery::default())), frame);
                object.anchor = Anchor::Page(page as u32);
                edit.add_object(object)?;
                Ok(id)
            });
        match result {
            Ok(id) => self.select_new_object(selection, id),
            Err(error) => gtk::glib::g_warning!("desk", "cannot insert: {error}"),
        }
    }

    /// The selected gallery, when one alone is selected.
    pub fn selected_gallery(&self) -> Option<(NodeId, Gallery)> {
        let info = self.object_info()?;
        if info.count != 1 {
            return None;
        }
        let gallery = info.object.gallery()?.clone();
        Some((info.object.id, gallery))
    }

    /// Changes the selected gallery as one action of the history.
    pub fn change_gallery(
        &self,
        name: &str,
        change: impl FnOnce(&mut Edit, NodeId) -> Result<(), ModelError>,
    ) {
        let Some((id, _)) = self.selected_gallery() else {
            return;
        };
        self.arrange_objects(name, |edit, _| {
            change(edit, id)?;
            Ok(None)
        });
    }

    /// Adds pictures to the selected gallery, after the one shown.
    pub fn add_gallery_pictures(&self, pictures: Vec<Picture>) {
        if pictures.is_empty() {
            return;
        }
        self.change_gallery("Add Images", |edit, id| {
            edit.add_gallery_images(id, pictures)
        });
    }

    /// Adds the pictures of files dropped on a gallery to it, and selects
    /// it. Returns false when they were not dropped on a gallery, or none
    /// of them is a picture.
    pub(super) fn drop_on_gallery(&self, files: &[gio::File], x: f64, y: f64) -> bool {
        let Some((page, px, py)) = self.view.page_at(x, y) else {
            return false;
        };
        let reach = (REACH / self.view.scale()) as f32;
        let id = {
            let state = self.state.borrow();
            hit(
                &state.layout,
                page,
                px as f32,
                py as f32,
                reach,
                state.selectable,
            )
            .filter(|id| {
                find(&state.layout, *id)
                    .is_some_and(|(_, p)| p.object.gallery().is_some() && !p.object.locked)
            })
        };
        let Some(id) = id else {
            return false;
        };
        let pictures: Vec<Picture> = files
            .iter()
            .filter_map(crate::bullet_pictures::document_picture_from_file)
            .collect();
        if pictures.is_empty() {
            return false;
        }
        self.leave_story(None);
        self.select_objects(vec![id]);
        self.add_gallery_pictures(pictures);
        true
    }

    /// Answers a press on an arrow of the selected gallery by showing the
    /// picture before or after; returns whether the press was on one.
    pub(super) fn press_gallery_arrow(&self, page: usize, x: f32, y: f32) -> bool {
        let reach = (ARROW_RADIUS / self.view.scale()) as f32;
        let step = {
            let state = self.state.borrow();
            let [id] = state.objects[..] else {
                return false;
            };
            let Some((on, placed)) = find(&state.layout, id) else {
                return false;
            };
            let Some(gallery) = placed.object.gallery() else {
                return false;
            };
            if on != page || placed.object.locked {
                return false;
            }
            gallery_arrows(placed)
                .iter()
                .position(|(ax, ay)| (ax - x).hypot(ay - y) <= reach)
                .map(|arrow| stepped(gallery, arrow == 1))
        };
        let Some(index) = step else {
            return false;
        };
        self.change_gallery("Show Image", |edit, id| edit.show_gallery_image(id, index));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w111_the_arrows_go_around_at_the_ends() {
        let png = |side: u32| {
            let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
            png.extend(side.to_be_bytes());
            png.extend(side.to_be_bytes());
            png.extend([8, 6, 0, 0, 0]);
            Picture::from_png(png).unwrap()
        };
        let gallery = Gallery {
            images: (1..=3)
                .map(|side| desk_model::GalleryImage::new(png(side)))
                .collect(),
            shown: 0,
            each_caption: false,
        };
        assert_eq!(stepped(&gallery, true), 1);
        assert_eq!(stepped(&gallery, false), 2);
        let last = Gallery {
            shown: 2,
            ..gallery
        };
        assert_eq!(stepped(&last, true), 0);
    }

    #[test]
    fn w111_a_gallery_is_inserted_filled_and_stepped_through_with_its_arrows() {
        crate::with_gtk(|| {
            let document = desk_model::Document::new_word_processing();
            let editor = TextEditor::new(&crate::view::DocumentView::new(), document);
            editor.insert_gallery();
            let (id, gallery) = editor.selected_gallery().unwrap();
            assert!(gallery.images.is_empty());
            let png = |side: u32| {
                let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
                png.extend(side.to_be_bytes());
                png.extend(side.to_be_bytes());
                png.extend([8, 6, 0, 0, 0]);
                Picture::from_png(png).unwrap()
            };
            editor.add_gallery_pictures(vec![png(1), png(2)]);
            let arrows = {
                let state = editor.state.borrow();
                let (page, placed) = find(&state.layout, id).unwrap();
                (page, gallery_arrows(placed))
            };
            let (page, arrows) = arrows;
            let [back, forward] = arrows[..] else {
                panic!("{arrows:?}");
            };
            let shown = |editor: &TextEditor| editor.selected_gallery().unwrap().1.shown;
            assert_eq!(shown(&editor), 0);
            assert!(editor.press_gallery_arrow(page, forward.0, forward.1));
            assert_eq!(shown(&editor), 1);
            assert!(editor.press_gallery_arrow(page, back.0, back.1));
            assert_eq!(shown(&editor), 0);
            // Away from the arrows, the press is the object's.
            assert!(!editor.press_gallery_arrow(page, back.0 + 60.0, back.1));
            editor.undo();
            assert_eq!(shown(&editor), 1);
        });
    }
}
