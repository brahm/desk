//! The image gallery of Pages 15.4 (W111, PXV-23): several pictures in
//! one frame, shown one at a time and stepped through with arrows, each
//! with its description and, when the gallery asks for it, a caption of
//! its own.

use std::sync::Arc;

use crate::{Edit, ModelError, NodeId, Object, ObjectKind, Paragraph, Picture};

/// A picture of a gallery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryImage {
    /// The picture.
    pub picture: Picture,
    /// Its description, for those who cannot see it (Image Description).
    pub description: String,
    /// Its own caption, when the gallery gives each picture one; that of
    /// the picture shown is in the captions of the object instead.
    pub caption: Arc<[Paragraph]>,
}

impl GalleryImage {
    /// A picture with no description or caption yet.
    pub fn new(picture: Picture) -> Self {
        Self {
            picture,
            description: String::new(),
            caption: Arc::from([]),
        }
    }
}

/// An image gallery.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gallery {
    /// The pictures, in the order the arrows step through them. A new
    /// gallery has none, and asks for them.
    pub images: Arc<[GalleryImage]>,
    /// The index of the picture shown.
    pub shown: usize,
    /// Whether each picture has a caption of its own, rather than one
    /// for all of them.
    pub each_caption: bool,
}

impl Gallery {
    /// The picture shown, if there is any.
    pub fn shown_image(&self) -> Option<&GalleryImage> {
        self.images
            .get(self.shown.min(self.images.len().saturating_sub(1)))
    }

    /// The index of the picture shown, within the pictures.
    pub fn shown_index(&self) -> usize {
        self.shown.min(self.images.len().saturating_sub(1))
    }
}

impl Object {
    /// The gallery of the object, if it is one.
    pub fn gallery(&self) -> Option<&Gallery> {
        match &self.kind {
            ObjectKind::Gallery(gallery) => Some(gallery),
            _ => None,
        }
    }
}

impl Edit {
    /// Changes the gallery `id` as `change` does, keeping the caption of
    /// the picture shown in the captions of the object when each picture
    /// has its own.
    fn change_gallery(
        &mut self,
        id: NodeId,
        change: impl FnOnce(&mut Gallery) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        let mut object = self
            .document()
            .objects()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownObject(id))?;
        let ObjectKind::Gallery(gallery) = &object.kind else {
            return Err(ModelError::UnknownObject(id));
        };
        let mut gallery = (**gallery).clone();
        let before = gallery.each_caption;
        // The caption shown goes back to its picture first.
        let shown_caption = object
            .captions
            .as_deref()
            .map(|c| c.caption.paragraphs.clone());
        if before && let Some(paragraphs) = shown_caption {
            let at = gallery.shown_index();
            let mut images = gallery.images.to_vec();
            if let Some(image) = images.get_mut(at) {
                image.caption = paragraphs;
            }
            gallery.images = images.into();
        }
        change(&mut gallery)?;
        gallery.shown = gallery.shown_index();
        // Then the caption of the picture shown now comes out, when each
        // has its own and it has one.
        // A picture with no caption yet shows an empty one, never that of
        // the picture before.
        if gallery.each_caption
            && let Some(image) = gallery.shown_image()
            && (!image.caption.is_empty() || object.captions.is_some())
        {
            let paragraphs = if image.caption.is_empty() {
                let style = self.document().styles().default_paragraph();
                Arc::from([Paragraph::new(self.new_id(), style)])
            } else {
                image.caption.clone()
            };
            let mut captions = object.captions.as_deref().cloned().unwrap_or_default();
            captions.caption.paragraphs = paragraphs;
            object.captions = Some(Arc::new(captions));
        }
        object.kind = ObjectKind::Gallery(Arc::new(gallery));
        self.replace_object(object)
    }

    /// Adds pictures to a gallery after the one shown, and shows the
    /// first of them.
    pub fn add_gallery_images(
        &mut self,
        id: NodeId,
        pictures: Vec<Picture>,
    ) -> Result<(), ModelError> {
        if pictures.is_empty() {
            return Ok(());
        }
        self.change_gallery(id, |gallery| {
            let at = if gallery.images.is_empty() {
                0
            } else {
                gallery.shown_index() + 1
            };
            let mut images = gallery.images.to_vec();
            images.splice(at..at, pictures.into_iter().map(GalleryImage::new));
            gallery.images = images.into();
            gallery.shown = at;
            Ok(())
        })
    }

    /// Takes a picture out of a gallery.
    pub fn remove_gallery_image(&mut self, id: NodeId, index: usize) -> Result<(), ModelError> {
        self.change_gallery(id, |gallery| {
            if index >= gallery.images.len() {
                return Err(ModelError::UnknownObject(id));
            }
            let mut images = gallery.images.to_vec();
            images.remove(index);
            gallery.images = images.into();
            if gallery.shown > index {
                gallery.shown -= 1;
            }
            Ok(())
        })
    }

    /// Moves a picture of a gallery to another place in its order, the
    /// picture shown staying shown.
    pub fn move_gallery_image(
        &mut self,
        id: NodeId,
        from: usize,
        to: usize,
    ) -> Result<(), ModelError> {
        self.change_gallery(id, |gallery| {
            let count = gallery.images.len();
            if from >= count || to >= count {
                return Err(ModelError::UnknownObject(id));
            }
            let mut order: Vec<usize> = (0..count).collect();
            let moved = order.remove(from);
            order.insert(to, moved);
            let images: Vec<GalleryImage> = order
                .iter()
                .filter_map(|&i| gallery.images.get(i).cloned())
                .collect();
            gallery.shown = order
                .iter()
                .position(|&i| i == gallery.shown_index())
                .unwrap_or(0);
            gallery.images = images.into();
            Ok(())
        })
    }

    /// Shows a picture of a gallery, as its arrows do.
    pub fn show_gallery_image(&mut self, id: NodeId, index: usize) -> Result<(), ModelError> {
        self.change_gallery(id, |gallery| {
            if index >= gallery.images.len() {
                return Err(ModelError::UnknownObject(id));
            }
            gallery.shown = index;
            Ok(())
        })
    }

    /// Sets the description of a picture of a gallery.
    pub fn set_gallery_description(
        &mut self,
        id: NodeId,
        index: usize,
        description: &str,
    ) -> Result<(), ModelError> {
        self.change_gallery(id, |gallery| {
            let mut images = gallery.images.to_vec();
            let image = images.get_mut(index).ok_or(ModelError::UnknownObject(id))?;
            image.description = description.to_owned();
            gallery.images = images.into();
            Ok(())
        })
    }

    /// Copies of paragraphs with identifiers of their own.
    pub(crate) fn fresh_paragraphs(
        &mut self,
        paragraphs: &[Paragraph],
    ) -> Result<Arc<[Paragraph]>, ModelError> {
        paragraphs
            .iter()
            .map(|p| {
                Paragraph::from_runs(
                    self.new_id(),
                    p.style(),
                    p.attrs().clone(),
                    p.runs().cloned(),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Arc::from)
    }

    /// Gives each picture of a gallery a caption of its own, or one for
    /// all of them. Each starts with the caption shown; one for all keeps
    /// the caption of the picture shown.
    pub fn set_gallery_each_caption(&mut self, id: NodeId, each: bool) -> Result<(), ModelError> {
        let shown_caption = self
            .document()
            .objects()
            .get(id)
            .and_then(|o| o.captions.as_deref())
            .map(|c| c.caption.paragraphs.clone())
            .unwrap_or_else(|| Arc::from([]));
        // Each picture starts with a copy of its own, with paragraphs of
        // its own.
        let count = self
            .document()
            .objects()
            .get(id)
            .and_then(Object::gallery)
            .map_or(0, |g| g.images.len());
        let mut copies = Vec::with_capacity(count);
        if each {
            for _ in 0..count {
                copies.push(self.fresh_paragraphs(&shown_caption)?);
            }
        }
        self.change_gallery(id, |gallery| {
            if gallery.each_caption == each {
                return Ok(());
            }
            gallery.each_caption = each;
            let mut copies = copies.into_iter();
            let images: Vec<GalleryImage> = gallery
                .images
                .iter()
                .cloned()
                .map(|mut image| {
                    image.caption = copies.next().unwrap_or_else(|| Arc::from([]));
                    image
                })
                .collect();
            gallery.images = images.into();
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, Document, Editor, Frame, ParaAttrs, Run, Story};

    fn picture(side: u32) -> Picture {
        // A PNG header is all the model reads.
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend(side.to_be_bytes());
        png.extend(side.to_be_bytes());
        png.extend([8, 6, 0, 0, 0]);
        Picture::from_png(png).unwrap()
    }

    fn with_gallery() -> (Editor, NodeId) {
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Insert Image Gallery", |edit| {
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::Gallery(Arc::new(Gallery::default())),
                    Frame::from_pt(72.0, 72.0, 320.0, 240.0),
                );
                object.anchor = Anchor::Page(0);
                let id = object.id;
                edit.add_object(object)?;
                Ok(id)
            })
            .unwrap();
        (editor, id)
    }

    fn gallery(editor: &Editor, id: NodeId) -> Gallery {
        editor
            .document()
            .objects()
            .get(id)
            .and_then(Object::gallery)
            .cloned()
            .unwrap()
    }

    #[test]
    fn w111_pictures_are_added_reordered_shown_and_taken_out() {
        let (mut editor, id) = with_gallery();
        assert!(gallery(&editor, id).shown_image().is_none());
        editor
            .edit("Add", |edit| {
                edit.add_gallery_images(id, vec![picture(1), picture(2), picture(3)])
            })
            .unwrap();
        let g = gallery(&editor, id);
        assert_eq!(g.images.len(), 3);
        assert_eq!(g.shown_image().unwrap().picture, picture(1));
        // Added after the one shown, and shown.
        editor
            .edit("Next", |edit| edit.show_gallery_image(id, 1))
            .unwrap();
        editor
            .edit("Add", |edit| edit.add_gallery_images(id, vec![picture(4)]))
            .unwrap();
        let g = gallery(&editor, id);
        let sides: Vec<u32> = g.images.iter().map(|i| i.picture.width()).collect();
        assert_eq!(sides, [1, 2, 4, 3]);
        assert_eq!(g.shown, 2);
        // Moving keeps the one shown shown.
        editor
            .edit("Move", |edit| edit.move_gallery_image(id, 2, 0))
            .unwrap();
        let g = gallery(&editor, id);
        let sides: Vec<u32> = g.images.iter().map(|i| i.picture.width()).collect();
        assert_eq!(sides, [4, 1, 2, 3]);
        assert_eq!(g.shown_image().unwrap().picture.width(), 4);
        editor
            .edit("Describe", |edit| {
                edit.set_gallery_description(id, 0, "A harbor at dawn")
            })
            .unwrap();
        editor
            .edit("Remove", |edit| edit.remove_gallery_image(id, 0))
            .unwrap();
        let g = gallery(&editor, id);
        assert_eq!(g.images.len(), 3);
        assert_eq!(g.shown_image().unwrap().picture.width(), 1);
        assert!(
            editor
                .edit("Bad", |edit| edit.show_gallery_image(id, 9))
                .is_err()
        );
        // Each step is undone on its own.
        editor.undo().unwrap();
        assert_eq!(
            gallery(&editor, id).images[0].description,
            "A harbor at dawn"
        );
    }

    #[test]
    fn w111_each_picture_keeps_its_own_caption() {
        let (mut editor, id) = with_gallery();
        editor
            .edit("Add", |edit| {
                edit.add_gallery_images(id, vec![picture(1), picture(2)])?;
                edit.show_caption(id, false, true)?;
                edit.set_gallery_each_caption(id, true)
            })
            .unwrap();
        let story = Story::Caption {
            object: id,
            title: false,
        };
        let type_caption = |editor: &mut Editor, text: &str| {
            editor
                .edit("Type", |edit| {
                    let style = edit.document().styles().default_paragraph();
                    let p = Paragraph::from_runs(
                        edit.new_id(),
                        style,
                        ParaAttrs::default(),
                        [Run::new(text)],
                    )?;
                    edit.set_story(story, vec![p])
                })
                .unwrap();
        };
        type_caption(&mut editor, "The first");
        editor
            .edit("Next", |edit| edit.show_gallery_image(id, 1))
            .unwrap();
        // The second starts with the caption it was given.
        assert_eq!(
            editor.document().story_paragraphs(story)[0].text(),
            "Caption"
        );
        type_caption(&mut editor, "The second");
        editor
            .edit("Back", |edit| edit.show_gallery_image(id, 0))
            .unwrap();
        assert_eq!(
            editor.document().story_paragraphs(story)[0].text(),
            "The first"
        );
        editor
            .edit("Next", |edit| edit.show_gallery_image(id, 1))
            .unwrap();
        assert_eq!(
            editor.document().story_paragraphs(story)[0].text(),
            "The second"
        );
        // A picture added later starts with an empty caption.
        editor
            .edit("Add", |edit| edit.add_gallery_images(id, vec![picture(3)]))
            .unwrap();
        assert_eq!(editor.document().story_paragraphs(story)[0].text(), "");
        editor
            .edit("Back", |edit| edit.show_gallery_image(id, 1))
            .unwrap();
        assert_eq!(
            editor.document().story_paragraphs(story)[0].text(),
            "The second"
        );
        // One for all keeps the one shown.
        editor
            .edit("All", |edit| edit.set_gallery_each_caption(id, false))
            .unwrap();
        editor
            .edit("Back", |edit| edit.show_gallery_image(id, 0))
            .unwrap();
        assert_eq!(
            editor.document().story_paragraphs(story)[0].text(),
            "The second"
        );
    }
}
