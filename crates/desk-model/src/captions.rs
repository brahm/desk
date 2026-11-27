//! The title and the caption of an object (W122): text of its own above
//! or below it, which moves with it, as the Style tab of Pages 15.4 turns
//! them on for a picture, a shape, a movie, a chart or a table.

use std::sync::Arc;

use crate::{
    Alignment, Edit, ModelError, NodeId, Object, ParaAttrs, Paragraph, ParagraphStyle, Run, Style,
    StyleFamily, StyleId, op::Op,
};

/// The name of the paragraph style that a new title or caption takes, as
/// in Pages 15.4.
pub const CAPTION_STYLE: &str = "Caption";

/// The title and the caption of an object (W122).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Captions {
    /// The title: above the object, or below it when `title_below`.
    pub title: Caption,
    /// The caption, below the object, after the title when it is below
    /// too.
    pub caption: Caption,
    /// Whether the title is below the object, as the Position menu of the
    /// Style tab puts it at the Bottom.
    pub title_below: bool,
}

/// The text of a title or a caption.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Caption {
    /// Whether it shows. A title or caption turned off keeps its text,
    /// which comes back when it is turned on again.
    pub shown: bool,
    /// Its paragraphs, which hold no objects or notes.
    pub paragraphs: Arc<[Paragraph]>,
}

impl Captions {
    /// The title or the caption.
    pub fn get(&self, title: bool) -> &Caption {
        if title { &self.title } else { &self.caption }
    }

    /// The title or the caption, to change.
    pub fn get_mut(&mut self, title: bool) -> &mut Caption {
        if title {
            &mut self.title
        } else {
            &mut self.caption
        }
    }

    /// Whether neither shows nor holds text, so that the object needs
    /// none.
    pub fn is_empty(&self) -> bool {
        !self.title_below
            && [&self.title, &self.caption]
                .iter()
                .all(|c| !c.shown && c.paragraphs.is_empty())
    }

    /// What shows above the object, then below it, in order.
    pub fn shown(&self) -> (Option<&Caption>, Vec<&Caption>) {
        let title = self.title.shown.then_some(&self.title);
        let caption = self.caption.shown.then_some(&self.caption);
        if self.title_below {
            (None, title.into_iter().chain(caption).collect())
        } else {
            (title, caption.into_iter().collect())
        }
    }
}

impl Object {
    /// The title or the caption of the object when it shows (W122).
    pub fn shown_caption(&self, title: bool) -> Option<&Caption> {
        self.captions
            .as_deref()
            .map(|c| c.get(title))
            .filter(|c| c.shown)
    }
}

/// Whether paragraphs hold an object or a note, which a title or a
/// caption cannot.
fn holds_marks(paragraphs: &[Paragraph]) -> bool {
    paragraphs.iter().any(|p| {
        p.runs().any(|r| {
            matches!(
                r.attrs.field,
                Some(Some(
                    crate::Field::ObjectMark { .. } | crate::Field::NoteMark { .. }
                ))
            )
        })
    })
}

impl Edit {
    /// The paragraph style of titles and captions, made when the document
    /// has none: centered, from the default style.
    pub fn caption_style(&mut self) -> Result<StyleId, ModelError> {
        if let Some(style) = self
            .document()
            .styles()
            .by_name(StyleFamily::Paragraph, CAPTION_STYLE)
        {
            return Ok(style.id());
        }
        let id = self.new_id();
        let mut style = ParagraphStyle::new(id, CAPTION_STYLE);
        style.parent = Some(self.document().styles().default_paragraph());
        style.paragraph.alignment = Some(Alignment::Center);
        self.apply(Op::PutStyle {
            style: Style::from(style),
        })?;
        Ok(id)
    }

    /// Turns the title or the caption of an object on or off, as the
    /// Style tab does (W122). Turned on the first time, it holds one
    /// paragraph of the caption style with its name, as in Pages 15.4.
    pub fn show_caption(&mut self, id: NodeId, title: bool, shown: bool) -> Result<(), ModelError> {
        let mut object = self
            .document()
            .objects()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownObject(id))?;
        let mut captions = object.captions.as_deref().cloned().unwrap_or_default();
        if captions.get(title).shown == shown {
            return Ok(());
        }
        if shown && captions.get(title).paragraphs.is_empty() {
            let style = self.caption_style()?;
            let name = if title { "Title" } else { "Caption" };
            let paragraph =
                Paragraph::from_runs(self.new_id(), style, ParaAttrs::default(), [Run::new(name)])?;
            captions.get_mut(title).paragraphs = Arc::from([paragraph]);
        }
        captions.get_mut(title).shown = shown;
        object.captions = (!captions.is_empty()).then(|| Arc::new(captions));
        self.replace_object(object)
    }

    /// Puts the title of an object below it, or back above it (W122).
    pub fn set_title_below(&mut self, id: NodeId, below: bool) -> Result<(), ModelError> {
        let mut object = self
            .document()
            .objects()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownObject(id))?;
        let mut captions = object.captions.as_deref().cloned().unwrap_or_default();
        if captions.title_below == below {
            return Ok(());
        }
        captions.title_below = below;
        object.captions = (!captions.is_empty()).then(|| Arc::new(captions));
        self.replace_object(object)
    }

    /// Sets the paragraphs of the title or the caption of an object,
    /// which must show; ones that hold an object or a note are refused,
    /// and none at all leave one empty paragraph.
    pub(crate) fn set_caption(
        &mut self,
        id: NodeId,
        title: bool,
        paragraphs: Vec<Paragraph>,
    ) -> Result<(), ModelError> {
        let mut object = self
            .document()
            .objects()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownObject(id))?;
        if object.shown_caption(title).is_none() {
            return Err(ModelError::UnknownObject(id));
        }
        if holds_marks(&paragraphs) {
            return Err(ModelError::InvalidText);
        }
        if paragraphs.is_empty() {
            return Ok(());
        }
        let mut captions = object.captions.as_deref().cloned().unwrap_or_default();
        captions.get_mut(title).paragraphs = paragraphs.into();
        object.captions = Some(Arc::new(captions));
        self.replace_object(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{FIELD_CHARACTER, Field};
    use crate::{Anchor, Document, Editor, Frame, ObjectKind, Position, Shape, Story};

    fn with_shape() -> (Editor, NodeId) {
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Add", |edit| {
                let mut shape = Object::new(
                    edit.new_id(),
                    ObjectKind::Shape(Shape::Rectangle),
                    Frame::from_pt(72.0, 72.0, 144.0, 72.0),
                );
                shape.anchor = Anchor::Page(0);
                let id = shape.id;
                edit.add_object(shape)?;
                Ok(id)
            })
            .unwrap();
        (editor, id)
    }

    #[test]
    fn w122_a_caption_turned_on_holds_its_name_in_the_caption_style() {
        let (mut editor, id) = with_shape();
        editor
            .edit("Caption", |edit| edit.show_caption(id, false, true))
            .unwrap();
        let document = editor.document();
        let object = document.objects().get(id).unwrap();
        let caption = object.shown_caption(false).unwrap();
        assert_eq!(caption.paragraphs[0].text(), "Caption");
        let style = document
            .styles()
            .by_name(StyleFamily::Paragraph, CAPTION_STYLE)
            .unwrap();
        assert_eq!(caption.paragraphs[0].style(), style.id());
        assert!(object.shown_caption(true).is_none());
        // It is edited as a story, and undone as one action.
        let story = Story::Caption {
            object: id,
            title: false,
        };
        assert_eq!(document.story_paragraphs(story)[0].text(), "Caption");
        assert!(document.stories().contains(&story));
        editor
            .edit("Type", |edit| {
                edit.edit_story(story, |edit| {
                    let p = edit.document().body().get(0).unwrap().id();
                    edit.insert_text(Position::new(p, 0), "A ").map(|_| ())
                })
            })
            .unwrap();
        assert_eq!(
            editor.document().story_paragraphs(story)[0].text(),
            "A Caption"
        );
        editor.undo().unwrap();
        assert_eq!(
            editor.document().story_paragraphs(story)[0].text(),
            "Caption"
        );
    }

    #[test]
    fn w122_a_caption_turned_off_keeps_its_text_and_undo_takes_it_away() {
        let (mut editor, id) = with_shape();
        editor
            .edit("Title", |edit| edit.show_caption(id, true, true))
            .unwrap();
        editor
            .edit("Below", |edit| edit.set_title_below(id, true))
            .unwrap();
        editor
            .edit("Off", |edit| edit.show_caption(id, true, false))
            .unwrap();
        let document = editor.document();
        let object = document.objects().get(id).unwrap();
        assert!(object.shown_caption(true).is_none());
        let captions = object.captions.as_deref().unwrap();
        assert_eq!(captions.title.paragraphs[0].text(), "Title");
        assert!(captions.title_below);
        let story = Story::Caption {
            object: id,
            title: true,
        };
        assert!(document.story_paragraphs(story).is_empty());
        assert!(!document.stories().contains(&story));
        for _ in 0..3 {
            editor.undo().unwrap();
        }
        assert!(
            editor
                .document()
                .objects()
                .get(id)
                .unwrap()
                .captions
                .is_none()
        );
    }

    #[test]
    fn w122_a_caption_holds_no_objects_or_notes() {
        let (mut editor, id) = with_shape();
        editor
            .edit("Caption", |edit| edit.show_caption(id, false, true))
            .unwrap();
        let story = Story::Caption {
            object: id,
            title: false,
        };
        let error = editor.edit("Paste", |edit| {
            let style = edit.document().styles().default_paragraph();
            let mark = Run::with_attrs(
                &FIELD_CHARACTER.to_string(),
                None,
                crate::CharAttrs {
                    field: Some(Some(Field::ObjectMark { object: id })),
                    ..crate::CharAttrs::default()
                },
            );
            let p = Paragraph::from_runs(edit.new_id(), style, ParaAttrs::default(), [mark])?;
            edit.set_story(story, vec![p])
        });
        assert!(error.is_err());
        assert_eq!(
            editor.document().story_paragraphs(story)[0].text(),
            "Caption"
        );
    }
}
