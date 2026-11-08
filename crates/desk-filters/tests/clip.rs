// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Objects copied in one app of Desk and pasted in another (IOP-27 to
//! IOP-29, IOP-31, IOP-33).

use std::sync::Arc;

use desk_filters::clip::{read_objects, write_objects};
use desk_filters::{read_odt, write_odt};
use desk_model::{
    Anchor, CharAttrs, Document, Editor, Frame, Length, MarginPages, Media, Object, ObjectKind,
    ParaAttrs, Paragraph, ParagraphStyle, Picture, Repeat, Run, Shape, TextBox,
};

/// A PNG of one pixel.
fn pixel_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[10, 20, 30, 255]).unwrap();
    }
    bytes
}

/// A page layout document, as a slide would be: on its second page a
/// title in a style of its own, a shape, a picture and a movie, and a
/// shape on its master, behind every page.
fn slide() -> Document {
    let mut document = Document::new_page_layout();
    let title = document.new_id();
    document
        .put_style(ParagraphStyle {
            id: title,
            name: "Slide Title".into(),
            shortcut: None,
            parent: None,
            next: None,
            paragraph: ParaAttrs::default(),
            character: CharAttrs {
                font_size: Some(Length::from_points(40)),
                ..CharAttrs::default()
            },
        })
        .unwrap();
    let mut editor = Editor::new(document);
    editor
        .edit("Slide", |edit| {
            let mut setup = *edit.document().page_setup();
            setup.layout_pages = Some(2);
            edit.set_page_setup(setup)?;
            let text = Paragraph::from_runs(
                edit.new_id(),
                title,
                ParaAttrs::default(),
                vec![Run::new("Results")],
            )?;
            let mut movie = Media::from_bytes({
                let mut mov = vec![0, 0, 0, 20];
                mov.extend(b"ftypqt  \0\0\0\0qt  moov");
                mov
            })
            .unwrap();
            movie.poster = Picture::from_png(pixel_png());
            movie.start = 1_500;
            movie.stop = Some(4_000);
            movie.repeat = Repeat::Loop;
            movie.volume = 50;
            let kinds = [
                ObjectKind::TextBox(TextBox::new(Arc::from([text]), Length::from_points(4))),
                ObjectKind::Shape(Shape::Ellipse),
                ObjectKind::Image(Picture::from_png(pixel_png()).unwrap()),
                ObjectKind::Media(movie),
            ];
            for (i, kind) in kinds.into_iter().enumerate() {
                let mut object = Object::new(
                    edit.new_id(),
                    kind,
                    Frame::from_pt(40.0 + 100.0 * i as f64, 60.0, 90.0, 50.0),
                );
                object.anchor = Anchor::Page(1);
                edit.add_object(object)?;
            }
            let mut back = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(0.0, 0.0, 600.0, 20.0),
            );
            back.anchor = Anchor::Master(None, MarginPages::Default);
            edit.add_object(back)
        })
        .unwrap();
    editor.document().clone()
}

/// The objects that show on a page of a page layout document, as a whole
/// slide copied: the master's behind those of the page.
fn page_objects(document: &Document, page: u32) -> Vec<Object> {
    let objects = document.objects();
    let master = objects
        .iter()
        .filter(|o| matches!(o.anchor, Anchor::Master(..)));
    let own = objects.iter().filter(|o| o.anchor == Anchor::Page(page));
    master.chain(own).cloned().collect()
}

/// Pastes the objects of a clipboard into a document, as an app does.
fn paste(into: Document, bytes: &[u8]) -> Document {
    let clip = read_objects(bytes).unwrap();
    let mut editor = Editor::new(into);
    editor
        .edit("Paste", |edit| {
            let objects = edit.adopt_object_styles(clip.document(), &clip.objects())?;
            edit.add_copies(&objects)
        })
        .unwrap();
    editor.document().clone()
}

#[test]
fn iop_27_iop_29_a_whole_slide_pastes_into_a_word_processing_document() {
    let slide = slide();
    let copied = page_objects(&slide, 1);
    assert_eq!(copied.len(), 5);
    let bytes = write_objects(&slide, &copied).unwrap();
    let document = paste(Document::new_word_processing(), &bytes);
    let objects: Vec<&Object> = document.objects().iter().collect();
    assert_eq!(objects.len(), 5);
    // The master's goes on the page with the others, behind them.
    assert_eq!(objects[0].kind, ObjectKind::Shape(Shape::Rectangle));
    assert!(objects.iter().all(|o| o.anchor == Anchor::Page(0)));
    for (pasted, copied) in objects.iter().zip(&copied) {
        assert_eq!(pasted.frame, copied.frame);
    }
    // The title keeps its style, which comes with it.
    let title = objects[1].text_box().unwrap();
    let style = document
        .styles()
        .paragraph(title.paragraphs[0].style())
        .unwrap();
    assert_eq!(&*style.name, "Slide Title");
    let size = document
        .styles()
        .resolve_character(style.id, None, &CharAttrs::default())
        .font_size;
    assert_eq!(size, Length::from_points(40));
    assert_eq!(title.paragraphs[0].text(), "Results");
    assert!(matches!(objects[3].kind, ObjectKind::Image(_)));
}

#[test]
fn iop_28_objects_of_a_page_paste_onto_a_slide_and_take_its_styles() {
    // A word processing document, whose text box has a style that the
    // slide has too by name.
    let mut writer = Document::new_word_processing();
    let own = writer.new_id();
    writer
        .put_style(ParagraphStyle {
            id: own,
            name: "Slide Title".into(),
            shortcut: None,
            parent: None,
            next: None,
            paragraph: ParaAttrs::default(),
            character: CharAttrs {
                font_size: Some(Length::from_points(12)),
                ..CharAttrs::default()
            },
        })
        .unwrap();
    let mut editor = Editor::new(writer);
    editor
        .edit("Box", |edit| {
            let text = Paragraph::from_runs(
                edit.new_id(),
                own,
                ParaAttrs::default(),
                vec![Run::new("Note")],
            )?;
            let mut object = Object::new(
                edit.new_id(),
                ObjectKind::TextBox(TextBox::new(Arc::from([text]), Length::from_points(4))),
                Frame::from_pt(72.0, 72.0, 200.0, 80.0),
            );
            object.anchor = Anchor::Page(0);
            edit.add_object(object)
        })
        .unwrap();
    let writer = editor.document().clone();
    let copied: Vec<Object> = writer.objects().iter().cloned().collect();
    let bytes = write_objects(&writer, &copied).unwrap();
    let slide = slide();
    let styles_before = slide.styles().len();
    let pasted = paste(slide, &bytes);
    // The slide's own style of that name, and no new one.
    assert_eq!(pasted.styles().len(), styles_before);
    let text = pasted.objects().iter().last().unwrap().text_box().unwrap();
    let style = pasted
        .styles()
        .paragraph(text.paragraphs[0].style())
        .unwrap();
    assert_eq!(&*style.name, "Slide Title");
    assert_eq!(style.character.font_size, Some(Length::from_points(40)));
}

#[test]
fn iop_31_movies_and_sounds_paste_with_how_they_play() {
    let slide = slide();
    let movie = page_objects(&slide, 1)
        .into_iter()
        .filter(|o| matches!(o.kind, ObjectKind::Media(_)))
        .collect::<Vec<_>>();
    let bytes = write_objects(&slide, &movie).unwrap();
    let pasted = paste(Document::new_word_processing(), &bytes);
    let (ObjectKind::Media(before), ObjectKind::Media(after)) = (
        &movie[0].kind,
        &pasted.objects().iter().next().unwrap().kind,
    ) else {
        panic!("not a movie");
    };
    assert_eq!(after.data(), before.data());
    assert_eq!(
        (after.start, after.stop, after.repeat, after.volume),
        (1_500, Some(4_000), Repeat::Loop, 50)
    );
    assert!(after.poster.is_some());
}

#[test]
fn iop_33_copies_share_their_origin_through_the_clipboard_and_odt() {
    let slide = slide();
    let copied = page_objects(&slide, 1);
    let bytes = write_objects(&slide, &copied).unwrap();
    let clip = read_objects(&bytes).unwrap();
    let origins: Vec<Option<u64>> = clip.objects().iter().map(|o| o.origin).collect();
    assert!(origins.iter().all(Option::is_some), "{origins:?}");
    // Pasted twice, as on two slides: each copy keeps the origin.
    let once = paste(slide.clone(), &bytes);
    let twice = paste(once, &bytes);
    let saved = read_odt(&write_odt(&twice).unwrap()).unwrap();
    for origin in &origins {
        let same = saved
            .objects()
            .iter()
            .filter(|o| o.origin == *origin)
            .count();
        assert_eq!(same, 2, "{origin:?}");
    }
    // Duplicated in the document, an object and its copy share one.
    let mut editor = Editor::new(slide);
    let id = editor.document().objects().iter().nth(1).unwrap().id;
    let copy = editor
        .edit("Duplicate", |edit| {
            edit.duplicate_objects(&[id], Length::ZERO, Length::ZERO)
        })
        .unwrap()[0];
    let objects = editor.document().objects();
    let origin = objects.get(id).unwrap().origin;
    assert!(origin.is_some());
    assert_eq!(objects.get(copy).unwrap().origin, origin);
}
