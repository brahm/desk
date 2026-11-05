// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Movies and sounds on the page (OBJ-18, OBJ-19): put there from a
//! file, played with a double click or the QuickTime inspector, and
//! given another poster frame.

use desk_model::{Anchor, Media, NodeId, Object, ObjectKind};
use gtk::prelude::*;
use gtk::{gdk, gio};

use super::TextEditor;
use super::objects::find;

/// How large a sound stands on the page, as a speaker, in points.
const SPEAKER: f64 = 48.0;

/// How large a movie without a poster frame stands on the page.
const MOVIE: (f64, f64) = (320.0, 240.0);

/// The size of a movie or a sound on the page, within `room`.
pub(super) fn media_size(media: &Media, room: (f64, f64)) -> (f64, f64) {
    if media.is_sound() {
        return (SPEAKER, SPEAKER);
    }
    let (w, h) = media
        .poster
        .as_ref()
        .map_or(MOVIE, |p| (f64::from(p.width()), f64::from(p.height())));
    let fit = (room.0 / w).min(room.1 / h).min(1.0);
    (w * fit, h * fit)
}

impl TextEditor {
    /// Puts a movie or a sound on a page, centered on `at` or in the
    /// middle of the page shown, and selects it (OBJ-18).
    pub fn insert_media(&self, media: Media, at: Option<(usize, f64, f64)>) {
        let (page, frame) = self.placed_frame(|room| media_size(&media, room), at);
        self.leave_story(None);
        let selection = self.selection();
        let name = if media.is_sound() {
            "Insert Sound"
        } else {
            "Insert Movie"
        };
        let result = self.state.borrow_mut().editor.edit(name, |edit| {
            let id = edit.new_id();
            let mut object = Object::new(id, ObjectKind::Media(media), frame);
            object.anchor = Anchor::Page(page as u32);
            edit.add_object(object)?;
            Ok(id)
        });
        match result {
            Ok(id) => self.select_new_object(selection, id),
            Err(error) => gtk::glib::g_warning!("desk", "cannot insert: {error}"),
        }
    }

    /// Puts the picture, movie or sound of a file on a page, centered on
    /// `at` or in the middle of the page shown (OBJ-01, OBJ-18). Returns
    /// false when the file is none that Desk reads.
    pub fn insert_file(&self, file: &gio::File, at: Option<(usize, f64, f64)>) -> bool {
        if let Some(media) = crate::bullet_pictures::media_from_file(file) {
            // A movie stands for itself with its first frame.
            let weak = self.downgrade();
            crate::media::probe(&media.clone(), 0, move |poster, duration| {
                let mut media = media;
                media.poster = poster;
                media.duration = duration;
                if let Some(this) = weak.upgrade() {
                    this.insert_media(media, at);
                }
            });
            return true;
        }
        match crate::bullet_pictures::document_picture_from_file(file) {
            Some(picture) => {
                self.insert_picture_at(picture, at);
                true
            }
            None => false,
        }
    }

    /// Takes files dropped on the pages, from the file manager or the
    /// Media Browser, where they are dropped (OBJ-02, INT-14).
    pub(super) fn connect_drop(&self) {
        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let weak = self.downgrade();
        drop.connect_drop(move |_, value, x, y| {
            let (Some(this), Ok(files)) = (weak.upgrade(), value.get::<gdk::FileList>()) else {
                return false;
            };
            let at = this.view.page_at(x, y);
            let mut used = false;
            for (i, file) in files.files().iter().enumerate() {
                // Each a little lower and to the right of the one before.
                let at = at.map(|(page, x, y)| (page, x + 18.0 * i as f64, y + 18.0 * i as f64));
                used |= this.insert_file(file, at);
            }
            used
        });
        self.view.area().add_controller(drop);
    }

    /// Plays a movie or a sound where it is on the page (OBJ-19).
    /// Returns whether the object is one.
    pub(super) fn play_media(&self, id: NodeId) -> bool {
        let (media, size) = {
            let state = self.state.borrow();
            let Some((page, placed)) = find(&state.layout, id) else {
                return false;
            };
            let ObjectKind::Media(media) = &placed.object.kind else {
                return false;
            };
            let (x, y, w, h) = (
                f64::from(placed.x),
                f64::from(placed.y),
                f64::from(placed.width),
                f64::from(placed.height),
            );
            let (Some((ax, ay)), Some((bx, by))) = (
                self.view.to_area(page, x, y),
                self.view.to_area(page, x + w, y + h),
            ) else {
                return true;
            };
            // As large as it shows on the page.
            let size = ((bx - ax).round() as i32, (by - ay).round() as i32);
            (media.clone(), size)
        };
        crate::media::play(self.view.area(), size, &media);
        true
    }

    /// Plays the selected movie or sound, as the QuickTime inspector
    /// does.
    pub fn play_selected_media(&self) {
        if let [id] = self.selected_objects()[..] {
            self.play_media(id);
        }
    }

    /// Takes the poster frame of the selected movie at `at` milliseconds,
    /// once the player has found it.
    pub fn set_poster_time(&self, at: u32) {
        let [id] = self.selected_objects()[..] else {
            return;
        };
        let Some(Object {
            kind: ObjectKind::Media(media),
            ..
        }) = self
            .state
            .borrow()
            .editor
            .document()
            .objects()
            .get(id)
            .cloned()
        else {
            return;
        };
        let weak = self.downgrade();
        crate::media::probe(&media, at, move |poster, _| {
            let Some(this) = weak.upgrade() else { return };
            if this.selected_objects() != [id] {
                return;
            }
            this.set_object("Poster Frame", move |object| {
                if let ObjectKind::Media(media) = &mut object.kind {
                    media.poster_time = at;
                    if poster.is_some() {
                        media.poster = poster;
                    }
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_18_sounds_stand_as_a_speaker_and_movies_as_their_poster() {
        let sound = Media::from_bytes(b"ID3\x04\0\0\0\0\0\0".to_vec()).unwrap();
        assert_eq!(media_size(&sound, (500.0, 500.0)), (SPEAKER, SPEAKER));
        let mut mov = vec![0, 0, 0, 20];
        mov.extend(b"ftypqt  ");
        let movie = Media::from_bytes(mov).unwrap();
        assert_eq!(media_size(&movie, (500.0, 500.0)), MOVIE);
        // Narrower room shrinks it, keeping its proportions.
        assert_eq!(media_size(&movie, (160.0, 500.0)), (160.0, 120.0));
    }
}
