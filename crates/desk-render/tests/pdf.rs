// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The display list and the PDF output. The PDFs are read back with the
//! Poppler tools, which CI installs, as the parity report will.

use std::process::Command;
use std::sync::Arc;

use desk_layout::{DocumentLayout, paginate_texts};
use desk_model::{
    CharFormat, LINE_BREAK, Length, LineStyle, Metadata, PageSetup, ParaFormat, Rgba,
};
use desk_render::{DisplayList, Item, Painter, cairo};
use desk_text::{ParagraphText, TextSystem};

fn sans() -> CharFormat {
    CharFormat {
        font_family: Arc::from("Liberation Sans"),
        ..CharFormat::default()
    }
}

fn layout(runs: &[(&str, CharFormat)], setup: PageSetup) -> DocumentLayout {
    let text = ParagraphText::from_runs(
        ParaFormat::default(),
        sans(),
        runs.iter().map(|(t, f)| (*t, f.clone())),
    );
    paginate_texts(setup, &[text], &mut TextSystem::new())
}

/// Runs a Poppler tool on a PDF and returns what it prints.
fn poppler(tool: &str, args: &[&str], pdf: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!("desk-render-{}-{tool}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.pdf");
    std::fs::write(&path, pdf).unwrap();
    let output = Command::new(tool)
        .args(args)
        .arg(&path)
        .args(if tool == "pdftotext" {
            &["-"][..]
        } else {
            &[][..]
        })
        .output()
        .unwrap_or_else(|e| panic!("{tool} is needed for this test (poppler-utils): {e}"));
    assert!(output.status.success(), "{tool} failed");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn glyphs_are_placed_on_the_page_with_their_color() {
    let red = CharFormat {
        color: Rgba::rgb(200, 0, 0),
        ..sans()
    };
    let layout = layout(&[("black ", sans()), ("red", red)], PageSetup::a4());
    let pages = DisplayList::pages(&layout);
    assert_eq!(pages.len(), 1);
    let list = &pages[0];
    assert!((list.width - 595.0).abs() < 0.01);
    let glyphs: Vec<_> = list
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs { color, glyphs, .. } => Some((*color, glyphs)),
            _ => None,
        })
        .collect();
    assert_eq!(glyphs.len(), 2);
    assert_eq!(glyphs[0].0, Rgba::BLACK);
    assert_eq!(glyphs[1].0, Rgba::rgb(200, 0, 0));
    // Inside the one inch margins.
    let first = glyphs[0].1[0];
    assert!((first.x - 72.0).abs() < 0.01);
    assert!(first.y > 72.0 && first.y < 90.0);
}

#[test]
fn fmt_01_fmt_02_lines_become_rectangles() {
    let marked = CharFormat {
        underline: LineStyle::Double,
        strikethrough: LineStyle::Single,
        ..sans()
    };
    let layout = layout(&[("marked", marked)], PageSetup::a4());
    let list = &DisplayList::pages(&layout)[0];
    let rects: Vec<_> = list
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Rect {
                y, width, height, ..
            } => Some((*y, *width, *height)),
            _ => None,
        })
        .collect();
    // Two underlines and one strikethrough, as wide as the word.
    assert_eq!(rects.len(), 3);
    assert!(rects[0].1 > 30.0);
    assert!(rects[1].0 > rects[0].0);
    assert!(rects[2].0 < rects[0].0);
}

#[test]
fn the_pdf_has_the_pages_text_fonts_and_metadata() {
    let small = PageSetup {
        width: Length::from_points(300),
        height: Length::from_points(120),
        margin_top: Length::from_points(20),
        margin_bottom: Length::from_points(20),
        margin_left: Length::from_points(20),
        margin_right: Length::from_points(20),
        ..PageSetup::default()
    };
    let text = ["Olá, Desk"; 12].join(&LINE_BREAK.to_string());
    let layout = layout(&[(&text, sans())], small);
    assert!(layout.pages.len() > 1);
    let pages = DisplayList::pages(&layout);
    let metadata = Metadata {
        title: "Relatório".into(),
        author: "Brahm".into(),
        ..Metadata::default()
    };
    let pdf = Painter::new()
        .unwrap()
        .pdf_bytes(&pages, &metadata)
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));

    let info = poppler("pdfinfo", &[], &pdf);
    assert!(info.contains("Title:           Relatório"), "{info}");
    assert!(info.contains("Author:          Brahm"), "{info}");
    assert!(info.contains("Creator:         Desk"), "{info}");
    assert!(
        info.contains(&format!("Pages:           {}", layout.pages.len())),
        "{info}"
    );
    assert!(info.contains("Page size:       300 x 120 pts"), "{info}");

    let text = poppler("pdftotext", &[], &pdf);
    assert_eq!(text.matches("Olá, Desk").count(), 12, "{text}");

    let fonts = poppler("pdffonts", &[], &pdf);
    let line = fonts
        .lines()
        .find(|l| l.contains("LiberationSans"))
        .unwrap_or_else(|| panic!("{fonts}"));
    // Embedded as a subset.
    assert!(line.contains(" yes yes "), "{line}");
}

#[test]
fn painting_puts_the_color_on_the_pixels() {
    let red = CharFormat {
        color: Rgba::rgb(255, 0, 0),
        font_size: Length::from_points(40),
        ..sans()
    };
    let setup = PageSetup {
        width: Length::from_points(200),
        height: Length::from_points(80),
        margin_top: Length::from_points(10),
        margin_bottom: Length::from_points(10),
        margin_left: Length::from_points(10),
        margin_right: Length::from_points(10),
        ..PageSetup::default()
    };
    let layout = layout(&[("HH", red)], setup);
    let list = &DisplayList::pages(&layout)[0];
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 80).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, list).unwrap();
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let red_pixels = (0..80)
        .flat_map(|y| (0..200).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let p = &data[y * stride + x * 4..][..4];
            // BGRA in memory on little-endian machines.
            p[2] > 200 && p[1] < 60 && p[0] < 60
        })
        .count();
    assert!(red_pixels > 300, "{red_pixels}");
}

#[test]
fn doc_09_a_thumbnail_is_a_png_that_fits_its_square() {
    let layout = layout(&[("Preview", sans())], PageSetup::a4());
    let pages = DisplayList::pages(&layout);
    let png = Painter::new()
        .unwrap()
        .thumbnail_png(&pages[0], 256)
        .unwrap();
    let image = cairo::ImageSurface::create_from_png(&mut &png[..]).unwrap();
    // A4 is taller than wide.
    assert_eq!(image.height(), 256);
    assert_eq!(image.width(), 181);
}

#[test]
fn obj_08_obj_13_objects_are_painted_rotated_and_faded() {
    use desk_model::{
        Document, Editor, Fill, Frame, Object, ObjectKind, Paragraph, Run, Shape, TextBox, Wrap,
    };
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Objects", |edit| {
            edit.set_page_setup(PageSetup {
                width: Length::from_points(200),
                height: Length::from_points(200),
                ..PageSetup::default()
            })?;
            let mut ellipse = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Ellipse),
                Frame::from_pt(20.0, 20.0, 60.0, 60.0),
            );
            ellipse.style.fill = Some(Fill::Color(Rgba::rgb(0, 0, 255)));
            ellipse.wrap = Wrap::Through;
            edit.add_object(ellipse)?;
            let mut square = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(120.0, 20.0, 40.0, 40.0),
            );
            square.style.fill = Some(Fill::Color(Rgba::rgb(255, 0, 0)));
            square.style.opacity = 50;
            square.rotation = 4500;
            square.wrap = Wrap::Through;
            edit.add_object(square)?;
            let style = edit.document().styles().default_paragraph();
            let paragraph = Paragraph::from_runs(
                edit.new_id(),
                style,
                desk_model::ParaAttrs::default(),
                [Run::new("Box")],
            )?;
            let mut text = Object::new(
                edit.new_id(),
                ObjectKind::TextBox(TextBox::new(Arc::from([paragraph]), Length::from_points(4))),
                Frame::from_pt(20.0, 120.0, 100.0, 40.0),
            );
            text.wrap = Wrap::Through;
            edit.add_object(text)
        })
        .unwrap();
    let layout = desk_layout::paginate(editor.document(), &mut TextSystem::new());
    let list = &DisplayList::pages(&layout)[0];
    // The text of the box is drawn inside its group.
    let Some(Item::Group { items, .. }) = list.items.last() else {
        panic!("the text box is the last item");
    };
    assert!(
        items
            .iter()
            .any(|i| matches!(i, Item::Glyphs { text, .. } if text == "Box"))
    );

    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 200).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, list).unwrap();
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    // BGRA in memory on little-endian machines.
    let pixel = |x: usize, y: usize| {
        let p = &data[y * stride + x * 4..][..4];
        (p[2], p[1], p[0])
    };
    assert_eq!(pixel(50, 50), (0, 0, 255));
    // The corner of the frame is outside the ellipse.
    assert_eq!(pixel(22, 22), (255, 255, 255));
    // Half red over white, and turned on its point: the corner of its
    // frame is left white.
    let (r, g, b) = pixel(140, 40);
    assert!(
        r == 255 && (120..=135).contains(&g) && (120..=135).contains(&b),
        "{r} {g} {b}"
    );
    assert_eq!(pixel(122, 22), (255, 255, 255));
    assert_ne!(
        pixel(140, 15),
        (255, 255, 255),
        "the point stands above the frame"
    );
}

#[test]
fn obj_08_the_text_of_a_flipped_shape_is_not_mirrored() {
    use desk_model::{Document, Editor, Frame, Object, Paragraph, Run, Shape, TextBox, Wrap};
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Shape", |edit| {
            let style = edit.document().styles().default_paragraph();
            let paragraph = Paragraph::from_runs(
                edit.new_id(),
                style,
                desk_model::ParaAttrs::default(),
                [Run::new("Hi")],
            )?;
            let mut shape = Object::new(
                edit.new_id(),
                Shape::triangle().into(),
                Frame::from_pt(100.0, 100.0, 100.0, 100.0),
            );
            shape.text = Some(TextBox::in_shape(Arc::from([paragraph])));
            shape.flip_vertical = true;
            shape.wrap = Wrap::Through;
            edit.add_object(shape)
        })
        .unwrap();
    let layout = desk_layout::paginate(editor.document(), &mut TextSystem::new());
    let list = &DisplayList::pages(&layout)[0];
    let Some(Item::Group {
        transform, items, ..
    }) = list.items.last()
    else {
        panic!("the shape is the last item");
    };
    // Not flipped as a whole: only the shape inside is.
    assert_eq!(transform[3], 1.0);
    let Some(Item::Group { transform, .. }) = items.first() else {
        panic!("the shape is drawn first");
    };
    assert_eq!(transform[3], -1.0);
    assert!(
        items
            .iter()
            .any(|i| matches!(i, Item::Glyphs { text, .. } if text == "Hi"))
    );
}

#[test]
fn obj_09_obj_10_obj_11_gradients_line_ends_and_shadows_are_painted() {
    use desk_model::{
        Document, Editor, Fill, Frame, LineEnd, Object, ObjectKind, ObjectShadow, Shape, Stroke,
        Wrap,
    };
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Objects", |edit| {
            edit.set_page_setup(PageSetup {
                width: Length::from_points(200),
                height: Length::from_points(200),
                ..PageSetup::default()
            })?;
            // Red on the left, blue on the right.
            let mut square = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(20.0, 20.0, 100.0, 40.0),
            );
            square.style.fill = Some(Fill::Gradient {
                from: Rgba::rgb(255, 0, 0),
                to: Rgba::rgb(0, 0, 255),
                angle: 0,
            });
            // Its shadow falls 10 points right and down, unblurred.
            square.style.shadow = Some(ObjectShadow {
                color: Rgba::BLACK,
                angle: 31500,
                offset: Length::from_pt(10.0 * std::f64::consts::SQRT_2),
                blur: Length::ZERO,
                opacity: 100,
            });
            square.wrap = Wrap::Through;
            edit.add_object(square)?;
            let mut line = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Line { rising: false }),
                Frame::from_pt(20.0, 150.0, 150.0, 0.0),
            );
            line.style.stroke = Some(Stroke {
                end: LineEnd::Arrow,
                ..Stroke::new(Rgba::BLACK, Length::from_points(2))
            });
            line.wrap = Wrap::Through;
            edit.add_object(line)
        })
        .unwrap();
    let layout = desk_layout::paginate(editor.document(), &mut TextSystem::new());
    let list = &DisplayList::pages(&layout)[0];
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 200).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, list).unwrap();
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let pixel = |x: usize, y: usize| {
        let p = &data[y * stride + x * 4..][..4];
        (p[2], p[1], p[0])
    };
    let (r, _, b) = pixel(22, 40);
    assert!(r > 240 && b < 15, "red at the left: {r} {b}");
    let (r, _, b) = pixel(118, 40);
    assert!(r < 15 && b > 240, "blue at the right: {r} {b}");
    let (r, _, b) = pixel(70, 40);
    assert!(
        (100..=155).contains(&r) && (100..=155).contains(&b),
        "{r} {b}"
    );
    // The shadow shows below and right of the square, not above it.
    assert_eq!(pixel(125, 65), (0, 0, 0));
    assert_eq!(pixel(15, 15), (255, 255, 255));
    // The arrow is wider than the line near its end.
    assert_eq!(pixel(162, 148), (0, 0, 0));
    assert_eq!(pixel(100, 148), (255, 255, 255));
}

#[test]
fn obj_09_pictures_fill_objects_as_their_mode_says() {
    use desk_model::{
        Document, Editor, Fill, Frame, ImageFill, Object, ObjectKind, Picture, Shape, Wrap,
    };
    // A red picture, ten pixels square.
    let png = {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 10, 10).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(1.0, 0.0, 0.0);
            cr.paint().unwrap();
        }
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        png
    };
    let picture = Picture::from_png(png).unwrap();
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Objects", |edit| {
            edit.set_page_setup(PageSetup {
                width: Length::from_points(200),
                height: Length::from_points(200),
                ..PageSetup::default()
            })?;
            for (y, mode) in [(20.0, ImageFill::ScaleToFit), (120.0, ImageFill::Tile)] {
                let mut square = Object::new(
                    edit.new_id(),
                    ObjectKind::Shape(Shape::Rectangle),
                    Frame::from_pt(20.0, y, 100.0, 40.0),
                );
                square.style.fill = Some(Fill::Image {
                    picture: picture.clone(),
                    mode,
                });
                square.wrap = Wrap::Through;
                edit.add_object(square)?;
            }
            Ok(())
        })
        .unwrap();
    let layout = desk_layout::paginate(editor.document(), &mut TextSystem::new());
    let list = &DisplayList::pages(&layout)[0];
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 200).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, list).unwrap();
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let pixel = |x: usize, y: usize| {
        let p = &data[y * stride + x * 4..][..4];
        (p[2], p[1], p[0])
    };
    // Fitted, it is as tall as the square and in its middle.
    assert_eq!(pixel(70, 40), (255, 0, 0));
    assert_eq!(pixel(25, 40), (255, 255, 255));
    // Tiled, it fills it all.
    assert_eq!(pixel(25, 140), (255, 0, 0));
    assert_eq!(pixel(115, 155), (255, 0, 0));
}

#[test]
fn obj_04_obj_05_obj_06_pictures_are_masked_keyed_and_adjusted() {
    use desk_model::{
        Adjustments, AlphaSeed, Document, Editor, Frame, Object, ObjectKind, Picture,
        PictureSettings, Wrap,
    };
    // Left half white, right half red, twenty pixels square.
    let png = {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 20, 20).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.rectangle(0.0, 0.0, 10.0, 20.0);
            cr.fill().unwrap();
            cr.set_source_rgb(1.0, 0.0, 0.0);
            cr.rectangle(10.0, 0.0, 10.0, 20.0);
            cr.fill().unwrap();
        }
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        png
    };
    let picture = Picture::from_png(png).unwrap();
    // White, red and white again, in three columns of ten pixels.
    let striped = {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 30, 10).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.paint().unwrap();
            cr.set_source_rgb(1.0, 0.0, 0.0);
            cr.rectangle(10.0, 0.0, 10.0, 10.0);
            cr.fill().unwrap();
        }
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        Picture::from_png(png).unwrap()
    };
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Objects", |edit| {
            edit.set_page_setup(PageSetup {
                width: Length::from_points(200),
                height: Length::from_points(200),
                ..PageSetup::default()
            })?;
            // A click on the white on the left takes it out, over a page
            // painted blue below, and not the white past the red.
            let mut keyed = Object::new(
                edit.new_id(),
                ObjectKind::Image(striped.clone()),
                Frame::from_pt(20.0, 20.0, 60.0, 20.0),
            );
            keyed.picture.alpha = std::sync::Arc::from([AlphaSeed {
                x: 1_000,
                y: 5_000,
                tolerance: AlphaSeed::TOLERANCE,
            }]);
            keyed.wrap = Wrap::Through;
            edit.add_object(keyed)?;
            // Masked to its right half, twice as large, and darker.
            let mut masked = Object::new(
                edit.new_id(),
                ObjectKind::Image(picture.clone()),
                Frame::from_pt(100.0, 100.0, 40.0, 80.0),
            );
            masked.picture = PictureSettings {
                mask: Some(Frame::from_pt(-40.0, 0.0, 80.0, 80.0)),
                adjust: Adjustments {
                    brightness: -50,
                    ..Adjustments::default()
                },
                ..PictureSettings::default()
            };
            masked.wrap = Wrap::Through;
            edit.add_object(masked)
        })
        .unwrap();
    let layout = desk_layout::paginate(editor.document(), &mut TextSystem::new());
    let list = &DisplayList::pages(&layout)[0];
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 200).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(0.0, 0.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, list).unwrap();
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let pixel = |x: usize, y: usize| {
        let p = &data[y * stride + x * 4..][..4];
        (p[2], p[1], p[0])
    };
    // The page shows through where the white was taken out.
    assert_eq!(pixel(25, 30), (0, 0, 255));
    assert_eq!(pixel(50, 30), (255, 0, 0));
    assert_eq!(pixel(75, 30), (255, 255, 255));
    // The mask shows only red, darkened, and nothing left of the frame.
    let (r, g, b) = pixel(120, 140);
    assert!((170..=210).contains(&r) && g < 10 && b < 10, "{r} {g} {b}");
    assert_eq!(pixel(90, 140), (0, 0, 255));
}

#[test]
fn obj_06_a_reduced_picture_keeps_what_shows() {
    use desk_model::{Frame, Picture};
    let png = {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 400, 400).unwrap();
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        png
    };
    let picture = Picture::from_png(png).unwrap();
    // Shown at 50 points, a quarter of it masked: 100 pixels square.
    let mask = Frame::from_pt(-50.0, -50.0, 200.0, 200.0);
    let reduced = desk_render::reduced_picture(&picture, Some(mask), (50.0, 50.0)).unwrap();
    assert_eq!((reduced.width(), reduced.height()), (100, 100));
    // At its full size it is not made smaller.
    assert!(desk_render::reduced_picture(&picture, None, (400.0, 400.0)).is_none());
}

#[test]
fn obj_01_jpeg_and_pdf_pictures_are_drawn() {
    use desk_model::Picture;
    // Sixteen pixels by eight, red on the left and blue on the right.
    let jpeg = Picture::from_jpeg(include_bytes!("two-colors.jpg").to_vec()).unwrap();
    assert_eq!((jpeg.width(), jpeg.height()), (16, 8));
    // A page of forty points by twenty, filled in green, as lines.
    let pdf = {
        let surface = cairo::PdfSurface::for_stream(40.0, 20.0, Vec::<u8>::new()).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(0.0, 0.6, 0.0);
            cr.paint().unwrap();
        }
        *surface
            .finish_output_stream()
            .unwrap()
            .downcast::<Vec<u8>>()
            .unwrap()
    };
    let (w, h) = desk_render::pdf_size(&pdf).unwrap();
    assert_eq!((w.round(), h.round()), (40.0, 20.0));
    let pdf = Picture::from_pdf(pdf, w, h).unwrap();
    let list = DisplayList {
        width: 200.0,
        height: 100.0,
        items: vec![
            Item::Image {
                picture: jpeg,
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 40.0,
            },
            Item::Image {
                picture: pdf,
                x: 100.0,
                y: 50.0,
                width: 80.0,
                height: 40.0,
            },
        ],
    };
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 100).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, &list).unwrap();
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    // BGRA in memory on little-endian machines.
    let pixel = |x: usize, y: usize| {
        let p = &data[y * stride + x * 4..][..3];
        (p[2], p[1], p[0])
    };
    let (r, g, b) = pixel(20, 20);
    assert!(r > 180 && g < 80 && b < 80, "{:?}", (r, g, b));
    let (r, g, b) = pixel(60, 20);
    assert!(r < 80 && g < 80 && b > 180, "{:?}", (r, g, b));
    let (r, g, b) = pixel(140, 70);
    assert!(
        r < 30 && (140..=165).contains(&g) && b < 30,
        "{:?}",
        (r, g, b)
    );
    // Outside the pictures, the page stays white.
    assert_eq!(pixel(150, 20), (255, 255, 255));

    // In a PDF, the JPEG keeps its bytes and the PDF page stays lines.
    let out = Painter::new()
        .unwrap()
        .pdf_bytes(&[list], &Metadata::default())
        .unwrap();
    let images = poppler("pdfimages", &["-list"], &out);
    assert!(images.lines().any(|l| l.contains("jpeg")), "{images}");
    assert_eq!(images.lines().count(), 3, "{images}");
}

#[test]
fn obj_18_movies_show_their_poster_and_sounds_a_speaker() {
    use desk_model::{Document, Editor, Frame, Media, Object, ObjectKind, Picture, Wrap};
    let poster = {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 4, 3).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(0.0, 1.0, 0.0);
            cr.paint().unwrap();
        }
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        Picture::from_png(png).unwrap()
    };
    let mut mov = vec![0, 0, 0, 20];
    mov.extend(b"ftypqt  ");
    let mut movie = Media::from_bytes(mov.clone()).unwrap();
    movie.poster = Some(poster);
    let blank = Media::from_bytes(mov).unwrap();
    let sound = Media::from_bytes(b"ID3\x04\0\0\0\0\0\0".to_vec()).unwrap();
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Media", |edit| {
            edit.set_page_setup(PageSetup {
                width: Length::from_points(200),
                height: Length::from_points(200),
                ..PageSetup::default()
            })?;
            for (media, x, y) in [(movie, 0.0, 0.0), (blank, 100.0, 0.0), (sound, 0.0, 100.0)] {
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::Media(media),
                    Frame::from_pt(x, y, 80.0, 80.0),
                );
                object.wrap = Wrap::Through;
                edit.add_object(object)?;
            }
            Ok(())
        })
        .unwrap();
    let layout = desk_layout::paginate(editor.document(), &mut TextSystem::new());
    let list = &DisplayList::pages(&layout)[0];
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 200).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, list).unwrap();
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let pixel = |x: usize, y: usize| {
        let p = &data[y * stride + x * 4..][..4];
        (p[2], p[1], p[0])
    };
    // The poster, a dark frame with a light triangle, and a gray speaker.
    assert_eq!(pixel(40, 40), (0, 255, 0));
    assert_eq!(pixel(105, 5), (0x2b, 0x2b, 0x2b));
    assert_eq!(pixel(140, 40), (0xee, 0xee, 0xee));
    assert_eq!(pixel(30, 140), (0x55, 0x55, 0x55));
    assert_eq!(pixel(5, 105), (255, 255, 255));
}
