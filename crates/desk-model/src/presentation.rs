// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The slides of a presentation of Desk Slides.
//!
//! A presentation is, for now, a page layout document whose pages are its
//! slides, with what each slide has besides its objects: the presenter
//! notes (KSLD-15) and whether the slide is skipped when the
//! presentation plays (KSLD-06). The slides move, go and come with their
//! pages, so the notes of a slide stay with it. Master slides, themes,
//! transitions and builds come in later phases of Desk Slides.

use crate::document::{Container, Document, PageSetup};
use crate::error::ModelError;
use crate::op::Op;
use crate::transaction::Edit;
use crate::units::Length;

/// What a slide has besides the objects on it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Slide {
    /// The presenter notes, as plain text (KSLD-15).
    pub notes: String,
    /// Whether the presentation skips the slide when it plays (KSLD-06).
    pub skipped: bool,
}

/// The text of a slide as an outline shows it (KINT-08).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Topics {
    /// The title; empty when the slide has no text.
    pub title: String,
    /// The lines under the title.
    pub lines: Vec<String>,
}

/// The size of the text of a new presentation, in points.
pub const BODY_SIZE: i32 = 36;

/// The size of a new slide: 1024 × 768, the default of Keynote '09
/// (KAPR-04), whose pixels are points.
pub const SLIDE_SIZE: (i32, i32) = (1024, 768);

impl Document {
    /// Makes a presentation of one blank slide of the default size.
    pub fn new_presentation() -> Self {
        let mut document = Self::new_page_layout();
        let Container::Pages(flow) = &mut document.container;
        flow.page = PageSetup {
            margin_top: Length::ZERO,
            margin_bottom: Length::ZERO,
            margin_left: Length::ZERO,
            margin_right: Length::ZERO,
            headers: false,
            footers: false,
            layout_pages: Some(1),
            ..PageSetup::with_size(
                Length::from_points(SLIDE_SIZE.0),
                Length::from_points(SLIDE_SIZE.1),
            )
        };
        // Text on a slide is read from across a room: until the themes
        // come, in phase 3, it starts at 36 pt, the size of the body text
        // of the White theme of Keynote.
        let body = document.styles.default_paragraph();
        if let Some(mut style) = document.styles.paragraph(body).cloned() {
            style.character.font_size = Some(Length::from_points(BODY_SIZE));
            document
                .styles
                .insert(crate::style::Style::Paragraph(style));
        }
        document.slides = Some(imbl::vector![Slide::default()]);
        document
    }

    /// Whether the document is a presentation.
    pub fn is_presentation(&self) -> bool {
        self.slides.is_some()
    }

    /// The slides of a presentation, one for each page, in order; none
    /// for another document.
    pub fn slides(&self) -> Option<&imbl::Vector<Slide>> {
        self.slides.as_ref()
    }

    /// A slide of a presentation, by index.
    pub fn slide(&self, index: u32) -> Option<&Slide> {
        self.slides.as_ref()?.get(index as usize)
    }

    /// The topics of each slide, as the Outline of Keynote shows them
    /// (KINT-08): a title, and the lines under it. Until slides have the
    /// placeholders of their master slide, the title is the first line of
    /// the text highest on the slide, and the lines are the others, from
    /// top to bottom.
    pub fn slide_topics(&self) -> Vec<Topics> {
        let count = self.page_setup().layout_pages.unwrap_or(0);
        (0..count)
            .map(|page| {
                let mut texts: Vec<&crate::Object> = self
                    .objects()
                    .iter()
                    .filter(|o| o.anchor == crate::Anchor::Page(page) && o.text_box().is_some())
                    .collect();
                texts.sort_by_key(|o| (o.frame.y, o.frame.x));
                let mut lines = texts
                    .iter()
                    .filter_map(|o| o.text_box())
                    .flat_map(|t| t.paragraphs.iter())
                    .map(|p| p.text().trim().to_owned())
                    .filter(|t| !t.is_empty());
                let title = lines.next().unwrap_or_default();
                Topics {
                    title,
                    lines: lines.collect(),
                }
            })
            .collect()
    }

    /// The slides of a presentation as they would be if every page had
    /// one: missing ones are blank, extra ones go.
    pub(crate) fn fitted_slides(&self) -> Option<imbl::Vector<Slide>> {
        let mut slides = self.slides.clone()?;
        let count = self.page_setup().layout_pages.unwrap_or(1) as usize;
        while slides.len() < count {
            slides.push_back(Slide::default());
        }
        slides.truncate(count);
        Some(slides)
    }
}

impl Edit {
    /// Sets the slides of a presentation. A document that is not a
    /// presentation is left as it is.
    pub(crate) fn set_slides(&mut self, slides: imbl::Vector<Slide>) -> Result<(), ModelError> {
        if !self.document().is_presentation() || self.document().slides() == Some(&slides) {
            return Ok(());
        }
        self.apply(Op::SetSlides {
            slides: Some(slides),
        })
    }

    /// Changes the slides with `f`, starting from one for each page.
    pub(crate) fn change_slides(
        &mut self,
        f: impl FnOnce(&mut imbl::Vector<Slide>),
    ) -> Result<(), ModelError> {
        let Some(mut slides) = self.document().fitted_slides() else {
            return Ok(());
        };
        f(&mut slides);
        self.set_slides(slides)
    }

    /// Sets the presenter notes of a slide (KSLD-15).
    pub fn set_slide_notes(&mut self, index: u32, notes: &str) -> Result<(), ModelError> {
        self.change_slides(|slides| {
            if let Some(slide) = slides.get_mut(index as usize) {
                slide.notes = notes.to_owned();
            }
        })
    }

    /// Skips slides when the presentation plays, or stops skipping them
    /// (KSLD-06).
    pub fn set_slides_skipped(&mut self, indices: &[u32], skipped: bool) -> Result<(), ModelError> {
        self.change_slides(|slides| {
            for &index in indices {
                if let Some(slide) = slides.get_mut(index as usize) {
                    slide.skipped = skipped;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;

    fn three_slides() -> Editor {
        let mut editor = Editor::new(Document::new_presentation());
        editor
            .edit("Add", |edit| {
                edit.add_layout_page(0)?;
                edit.add_layout_page(1)?;
                edit.set_slide_notes(0, "first")?;
                edit.set_slide_notes(1, "second")?;
                edit.set_slide_notes(2, "third")
            })
            .unwrap();
        editor
    }

    fn notes(editor: &Editor) -> Vec<String> {
        editor
            .document()
            .slides()
            .unwrap()
            .iter()
            .map(|s| s.notes.clone())
            .collect()
    }

    #[test]
    fn kapr_04_a_new_presentation_has_one_slide_of_1024_by_768() {
        let document = Document::new_presentation();
        assert!(document.is_presentation());
        assert_eq!(document.page_setup().layout_pages, Some(1));
        assert_eq!(document.page_setup().width, Length::from_points(1024));
        assert_eq!(document.page_setup().height, Length::from_points(768));
        assert_eq!(document.slides().unwrap().len(), 1);
        let body = document.styles().default_paragraph();
        assert_eq!(
            document
                .styles()
                .paragraph(body)
                .unwrap()
                .character
                .font_size,
            Some(Length::from_points(BODY_SIZE))
        );
        assert!(!Document::new_page_layout().is_presentation());
    }

    #[test]
    fn ksld_15_notes_move_with_their_slides() {
        let mut editor = three_slides();
        assert_eq!(notes(&editor), ["first", "second", "third"]);
        editor
            .edit("Move", |edit| edit.move_layout_pages(&[2], 0).map(drop))
            .unwrap();
        assert_eq!(notes(&editor), ["third", "first", "second"]);
        editor
            .edit("Duplicate", |edit| {
                edit.duplicate_layout_pages(&[0]).map(drop)
            })
            .unwrap();
        assert_eq!(notes(&editor), ["third", "third", "first", "second"]);
        editor
            .edit("Delete", |edit| edit.delete_layout_pages(&[0, 2]))
            .unwrap();
        assert_eq!(notes(&editor), ["third", "second"]);
        editor.edit("Add", |edit| edit.add_layout_page(0)).unwrap();
        assert_eq!(notes(&editor), ["third", "", "second"]);
        editor.undo().unwrap();
        editor.undo().unwrap();
        assert_eq!(notes(&editor), ["third", "third", "first", "second"]);
    }

    #[test]
    fn ksld_06_skipped_slides_are_kept_and_undone() {
        let mut editor = three_slides();
        editor
            .edit("Skip", |edit| edit.set_slides_skipped(&[1, 2], true))
            .unwrap();
        let skipped: Vec<bool> = editor
            .document()
            .slides()
            .unwrap()
            .iter()
            .map(|s| s.skipped)
            .collect();
        assert_eq!(skipped, [false, true, true]);
        editor.undo().unwrap();
        assert!(
            editor
                .document()
                .slides()
                .unwrap()
                .iter()
                .all(|s| !s.skipped)
        );
    }

    #[test]
    fn ksld_15_a_pasted_slide_brings_its_notes() {
        let mut editor = three_slides();
        let clip = editor.document().copy_layout_pages(&[1]);
        editor
            .edit("Paste", |edit| edit.paste_layout_pages(3, &clip).map(drop))
            .unwrap();
        assert_eq!(notes(&editor), ["first", "second", "third", "second"]);
    }

    #[test]
    fn kint_08_the_topics_are_the_text_of_each_slide_from_the_top() {
        use crate::{Frame, Object, ObjectKind, ParaAttrs, Paragraph, Run, TextBox};
        let mut editor = three_slides();
        let style = editor.document().styles().default_paragraph();
        editor
            .edit("Add", |edit| {
                for (y, lines) in [
                    (400.0, &["Three cities", "", "Partners"][..]),
                    (40.0, &["Plan"]),
                ] {
                    let paragraphs: Vec<Paragraph> = lines
                        .iter()
                        .map(|text| {
                            let runs = [Run::new(text)].into_iter().filter(|r| !r.is_empty());
                            Paragraph::from_runs(edit.new_id(), style, ParaAttrs::default(), runs)
                                .unwrap()
                        })
                        .collect();
                    let id = edit.new_id();
                    let mut object = Object::new(
                        id,
                        ObjectKind::TextBox(TextBox::new(paragraphs.into(), Length::ZERO)),
                        Frame::from_pt(40.0, y, 600.0, 100.0),
                    );
                    object.anchor = crate::Anchor::Page(1);
                    edit.add_object(object)?;
                }
                Ok(())
            })
            .unwrap();
        let topics = editor.document().slide_topics();
        assert_eq!(topics.len(), 3);
        assert_eq!(topics[0], Topics::default());
        assert_eq!(topics[1].title, "Plan");
        assert_eq!(topics[1].lines, ["Three cities", "Partners"]);
    }

    #[test]
    fn other_documents_have_no_slides() {
        let mut editor = Editor::new(Document::new_page_layout());
        editor
            .edit("Add", |edit| {
                edit.add_layout_page(0)?;
                edit.set_slide_notes(0, "x")
            })
            .unwrap();
        assert!(editor.document().slides().is_none());
    }
}
