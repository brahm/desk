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
    // A folder of its own for each call, as the tests run at once.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("desk-render-{}-{tool}-{call}", std::process::id()));
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
fn obj_08_obj_13_obj_34_obj_39_objects_are_painted_rotated_and_faded() {
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
fn obj_35_obj_37_obj_41_gradients_line_ends_and_shadows_are_painted() {
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
fn obj_38_objects_are_reflected_below_fading_away() {
    use desk_model::{Document, Editor, Fill, Frame, Object, ObjectKind, Shape, Wrap};
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Objects", |edit| {
            edit.set_page_setup(PageSetup {
                width: Length::from_points(200),
                height: Length::from_points(200),
                ..PageSetup::default()
            })?;
            let mut square = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(20.0, 20.0, 100.0, 40.0),
            );
            square.style.fill = Some(Fill::Color(Rgba::rgb(255, 0, 0)));
            square.style.reflection = Some(100);
            square.wrap = Wrap::Through;
            edit.add_object(square)
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
    // Red right below the square, paler further down, and gone past half
    // its height.
    let (r, g, _) = pixel(70, 61);
    assert!(r > 240 && g < 30, "{r} {g}");
    let (_, g, _) = pixel(70, 72);
    assert!((60..230).contains(&g), "{g}");
    assert_eq!(pixel(70, 85), (255, 255, 255));
    assert_eq!(pixel(70, 15), (255, 255, 255));
}

#[test]
fn obj_42_pictures_fill_objects_as_their_mode_says() {
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
                    tint: None,
                });
                square.wrap = Wrap::Through;
                edit.add_object(square)?;
            }
            // Tinted half blue (OBJ-42).
            let mut tinted = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(130.0, 20.0, 50.0, 40.0),
            );
            tinted.style.fill = Some(Fill::Image {
                picture: picture.clone(),
                mode: ImageFill::Stretch,
                tint: Some(Rgba {
                    a: 128,
                    ..Rgba::rgb(0, 0, 255)
                }),
            });
            tinted.wrap = Wrap::Through;
            edit.add_object(tinted)?;
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
    // The tint lies over the picture.
    let (r, g, b) = pixel(155, 40);
    assert!(
        (110..=145).contains(&r) && g < 10 && (110..=145).contains(&b),
        "{r} {g} {b}"
    );
}

#[test]
fn obj_04_obj_06_obj_07_pictures_are_masked_keyed_and_adjusted() {
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
fn obj_05_a_reduced_picture_keeps_what_shows() {
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

#[test]
fn obj_41_advanced_gradients_of_several_colors_and_radial_ones_are_painted() {
    use desk_model::{
        Document, Editor, Fill, Frame, Gradient, GradientStop, Object, ObjectKind, Shape, Wrap,
    };
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Objects", |edit| {
            edit.set_page_setup(PageSetup {
                width: Length::from_points(200),
                height: Length::from_points(200),
                ..PageSetup::default()
            })?;
            // Red, green in the middle and blue, from left to right.
            let mut bar = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(20.0, 20.0, 100.0, 40.0),
            );
            bar.style.fill = Some(Fill::Advanced(Gradient {
                stops: vec![
                    GradientStop::new(0, Rgba::rgb(255, 0, 0)),
                    GradientStop::new(500, Rgba::rgb(0, 255, 0)),
                    GradientStop::new(1000, Rgba::rgb(0, 0, 255)),
                ],
                angle: 0,
                center: None,
            }));
            bar.wrap = Wrap::Through;
            edit.add_object(bar)?;
            // White in the middle of a square, black at its corners.
            let mut round = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(20.0, 100.0, 80.0, 80.0),
            );
            round.style.fill = Some(Fill::Advanced(Gradient {
                stops: vec![
                    GradientStop::new(0, Rgba::WHITE),
                    GradientStop::new(1000, Rgba::BLACK),
                ],
                angle: 0,
                center: Some((500, 500)),
            }));
            round.wrap = Wrap::Through;
            edit.add_object(round)
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
    let (r, g, b) = pixel(22, 40);
    assert!(r > 240 && g < 15 && b < 15, "red at the left: {r} {g} {b}");
    let (r, g, b) = pixel(70, 40);
    assert!(
        r < 15 && g > 240 && b < 15,
        "green in the middle: {r} {g} {b}"
    );
    let (r, g, b) = pixel(118, 40);
    assert!(
        r < 15 && g < 15 && b > 240,
        "blue at the right: {r} {g} {b}"
    );
    let (r, _, _) = pixel(60, 140);
    assert!(r > 245, "white in the middle: {r}");
    let (r, _, _) = pixel(21, 101);
    assert!(r < 15, "black at a corner: {r}");
    let (edge, _, _) = pixel(60, 101);
    assert!((60..200).contains(&edge), "between at the edge: {edge}");
}

#[test]
fn obj_04_obj_36_pictures_masked_with_a_shape_and_framed_are_painted() {
    use desk_model::{
        Document, Editor, Frame, FrameStyle, Object, ObjectKind, Picture, PictureFrame,
        PictureSettings, Shape, Wrap,
    };
    // Red all over, ten pixels square.
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
            // In an oval: its corners show the page.
            let mut oval = Object::new(
                edit.new_id(),
                ObjectKind::Image(picture.clone()),
                Frame::from_pt(20.0, 20.0, 60.0, 60.0),
            );
            oval.picture = PictureSettings {
                mask: Some(Frame::from_pt(0.0, 0.0, 60.0, 60.0)),
                mask_shape: Some(Shape::Ellipse),
                ..PictureSettings::default()
            };
            oval.wrap = Wrap::Through;
            edit.add_object(oval)?;
            // In a gallery frame: a white mat, then black at its edge.
            let mut framed = Object::new(
                edit.new_id(),
                ObjectKind::Image(picture.clone()),
                Frame::from_pt(120.0, 120.0, 40.0, 40.0),
            );
            framed.style.frame = Some(PictureFrame::new(FrameStyle::Gallery));
            framed.wrap = Wrap::Through;
            edit.add_object(framed)
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
    assert_eq!(pixel(50, 50), (255, 0, 0), "red in the middle of the oval");
    assert_eq!(
        pixel(22, 22),
        (0, 0, 255),
        "the page at a corner of the oval"
    );
    assert_eq!(pixel(140, 140), (255, 0, 0), "the framed picture");
    // The frame reaches 20 points out: white near the picture, black at
    // its outer edge.
    assert_eq!(pixel(140, 110), (255, 255, 255), "the mat");
    assert_eq!(pixel(140, 101), (0x1a, 0x1a, 0x1a), "the edge of the frame");
    assert_eq!(pixel(140, 97), (0, 0, 255), "the page beyond the frame");
}

#[test]
fn iop_07_objects_and_their_effects_look_the_same_on_screen_and_in_the_pdf() {
    // One model of objects and effects, and one painter for the screen,
    // the PDF and the print: a page with a gradient, a shadow, a
    // reflection, an object faded and turned and a line with an arrow is
    // drawn on an image as the screen draws it and written as PDF, which
    // poppler rasterizes; the two images agree.
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
            square.style.shadow = Some(ObjectShadow {
                color: Rgba::BLACK,
                angle: 31500,
                offset: Length::from_pt(10.0 * std::f64::consts::SQRT_2),
                blur: Length::ZERO,
                opacity: 100,
            });
            square.style.reflection = Some(60);
            square.wrap = Wrap::Through;
            edit.add_object(square)?;
            let mut faded = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Ellipse),
                Frame::from_pt(130.0, 100.0, 50.0, 50.0),
            );
            faded.style.fill = Some(Fill::Color(Rgba::rgb(0, 160, 0)));
            faded.style.opacity = 50;
            faded.rotation = 3000;
            faded.wrap = Wrap::Through;
            edit.add_object(faded)?;
            let mut line = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Line { rising: false }),
                Frame::from_pt(20.0, 170.0, 150.0, 0.0),
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

    // As the screen draws it, a pixel a point.
    let mut screen = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 200).unwrap();
    {
        let cr = cairo::Context::new(&screen).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, list).unwrap();
    }
    screen.flush();

    // The PDF, rasterized at 72 pixels an inch.
    let pdf = Painter::new()
        .unwrap()
        .pdf_bytes(std::slice::from_ref(list), &Metadata::default())
        .unwrap();
    let dir = std::env::temp_dir().join(format!("desk-render-{}-iop-07", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("page.pdf"), &pdf).unwrap();
    let status = Command::new("pdftoppm")
        .args(["-r", "72", "-png", "-singlefile"])
        .arg(dir.join("page.pdf"))
        .arg(dir.join("page"))
        .status()
        .unwrap_or_else(|e| panic!("pdftoppm is needed for this test (poppler-utils): {e}"));
    assert!(status.success());
    let png = std::fs::read(dir.join("page.png")).unwrap();
    let mut printed = cairo::ImageSurface::create_from_png(&mut &png[..]).unwrap();
    assert_eq!((printed.width(), printed.height()), (200, 200));

    let rgb = |surface: &mut cairo::ImageSurface| -> Vec<[u8; 3]> {
        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        (0..200)
            .flat_map(|y| (0..200).map(move |x| (x, y)))
            .map(|(x, y)| {
                let p = &data[y * stride + x * 4..][..4];
                [p[2], p[1], p[0]]
            })
            .collect()
    };
    let (screen, printed) = (rgb(&mut screen), rgb(&mut printed));
    // Every effect left its mark on the page.
    let at = |image: &[[u8; 3]], x: usize, y: usize| image[y * 200 + x];
    assert_eq!(at(&screen, 125, 65), [0, 0, 0], "the shadow");
    let reflected = at(&screen, 25, 64);
    assert!(
        reflected[0] > 230 && reflected[1] < 200,
        "the reflection: {reflected:?}"
    );
    let faded = at(&screen, 155, 125);
    assert!(
        (100..=200).contains(&faded[0]) && faded[1] > 150,
        "{faded:?}"
    );
    // Edges fall a pixel apart at most, where the two rasterizers smooth
    // them otherwise: few pixels differ, and none in the middle of a shape.
    let differ = screen
        .iter()
        .zip(&printed)
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 40))
        .count();
    assert!(differ < 200 * 200 / 100, "{differ} pixels differ");
    for (x, y) in [(70, 40), (125, 65), (155, 125), (25, 64)] {
        let (a, b) = (at(&screen, x, y), at(&printed, x, y));
        assert!(
            a.iter().zip(b.iter()).all(|(a, b)| a.abs_diff(*b) <= 12),
            "({x}, {y}): {a:?} on screen, {b:?} in the PDF"
        );
    }
}

#[test]
fn pxv_04_a_gradient_goes_once_across_the_text_it_fills() {
    use desk_model::{Fill, FontWeight};
    let fill = Fill::Gradient {
        from: Rgba::rgb(200, 0, 0),
        to: Rgba::rgb(0, 0, 200),
        angle: 0,
    };
    let filled = CharFormat {
        fill: Some(fill.clone()),
        ..sans()
    };
    // A bold letter in the middle does not start the gradient again.
    let bold = CharFormat {
        weight: FontWeight::BOLD,
        ..filled.clone()
    };
    let layout = layout(
        &[
            ("plain ", sans()),
            ("Gra", filled.clone()),
            ("d", bold),
            ("ient", filled),
        ],
        PageSetup::a4(),
    );
    let pages = DisplayList::pages(&layout);
    let paints: Vec<_> = pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs { paint, glyphs, .. } => Some((paint.clone(), glyphs[0].x)),
            _ => None,
        })
        .collect();
    assert_eq!(paints.len(), 4);
    assert_eq!(paints[0].0, None);
    let Some(desk_render::Paint::Linear { start, end, stops }) = paints[1].0.clone() else {
        panic!("{:?}", paints[1].0);
    };
    // From the left of the word to its right, left to right.
    assert!((start.0 - paints[1].1).abs() < 0.5, "{start:?}");
    assert!(end.0 > paints[3].1 && (start.1 - end.1).abs() < 0.01);
    assert_eq!(stops.first().map(|s| s.1), Some(Rgba::rgb(200, 0, 0)));
    assert!(paints[2..].iter().all(|(p, _)| p == &paints[1].0));
    // The PDF fills the letters with it, and keeps their text.
    let pdf = Painter::new()
        .unwrap()
        .pdf_bytes(&pages, &Metadata::default());
    assert!(poppler("pdftotext", &[], &pdf.unwrap()).contains("plain Gradient"));
}

#[test]
fn w052_a_gradient_runs_once_over_the_lines_of_its_paragraph() {
    // Pages 15.4, on the owner's Mac on 2026-11-21: a vertical gradient
    // on a paragraph of three lines goes once from the top line to the
    // bottom one, light to dark, and not again on each line.
    use desk_model::Fill;
    let filled = CharFormat {
        fill: Some(Fill::Gradient {
            from: Rgba::rgb(200, 220, 255),
            to: Rgba::rgb(0, 0, 120),
            angle: 27000,
        }),
        ..sans()
    };
    let text = "A gradient on a whole paragraph that runs over several lines \
                spreads once from the top line to the bottom line of the \
                paragraph, and the lines do not each start it again. "
        .repeat(3);
    let layout = layout(&[(text.as_str(), filled)], PageSetup::a4());
    let pages = DisplayList::pages(&layout);
    let paints: Vec<_> = pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs { paint, glyphs, .. } => Some((paint.clone(), glyphs[0].y)),
            _ => None,
        })
        .collect();
    let lines: std::collections::BTreeSet<i64> =
        paints.iter().map(|(_, y)| (*y * 10.0) as i64).collect();
    assert!(lines.len() >= 3, "{}", lines.len());
    // One paint for every line: the same gradient, from above the first
    // line to below the last.
    assert!(paints.iter().all(|(p, _)| p == &paints[0].0));
    let Some(desk_render::Paint::Linear { start, end, .. }) = paints[0].0.clone() else {
        panic!("{:?}", paints[0].0);
    };
    let (first, last) = (paints[0].1, paints[paints.len() - 1].1);
    let (top, bottom) = (start.1.min(end.1), start.1.max(end.1));
    assert!(
        top < first && bottom > last,
        "{start:?} {end:?} {first} {last}"
    );
}

#[test]
fn w052_a_gradient_applied_to_the_entire_page_runs_across_the_page() {
    // "Apply gradient to entire page" of Pages 15.4 stretches the
    // gradient over the whole page, whatever text has it.
    use desk_model::Fill;
    let filled = CharFormat {
        fill: Some(Fill::Gradient {
            from: Rgba::rgb(200, 220, 255),
            to: Rgba::rgb(0, 0, 120),
            angle: 27000,
        }),
        fill_page: true,
        ..sans()
    };
    let layout = layout(&[("One short line.", filled)], PageSetup::a4());
    let pages = DisplayList::pages(&layout);
    let height = pages[0].height;
    let paint = pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Glyphs { paint, .. } => paint.clone(),
            _ => None,
        })
        .unwrap();
    let desk_render::Paint::Linear { start, end, .. } = paint else {
        panic!("{paint:?}");
    };
    // From the top of the page to its bottom.
    let (top, bottom) = (start.1.min(end.1), start.1.max(end.1));
    assert!(
        top.abs() < 1.0 && (bottom - height).abs() < 1.0,
        "{start:?} {end:?}"
    );
}

#[test]
fn w098_the_background_of_a_page_is_drawn_behind_all_else() {
    let mut laid = layout(&[("Text", sans())], PageSetup::default());
    let blue = desk_model::Fill::Color(Rgba::rgb(0, 0, 255));
    laid.pages[0].background = Some(blue);
    let list = DisplayList::from_page(&laid, &laid.pages[0]);
    let Some(Item::Path {
        path,
        fill: Some(_),
        stroke: None,
    }) = list.items.first()
    else {
        panic!("{:?}", list.items.first());
    };
    // The whole paper.
    assert!(path.contains(&desk_render::PathOp::Line(list.width, list.height)));
    assert!(list.items.len() > 1);
}

#[test]
fn w108_svg_and_heic_pictures_are_drawn() {
    use desk_model::Picture;
    // Forty pixels by thirty, filled in blue, as lines.
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30">
        <rect width="40" height="30" fill="blue"/></svg>"#;
    let (w, h) = desk_render::svg_size(svg).unwrap();
    assert_eq!((w, h), (40.0, 30.0));
    let svg = Picture::from_svg(svg.to_vec(), w, h).unwrap();
    let heic = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/fixtures/images/gradient-64x48.heic"
    ))
    .unwrap();
    assert_eq!(desk_render::heic_size(&heic), Some((64, 48)));
    let heic = Picture::from_heic(heic, 64, 48).unwrap();
    let surface = desk_render::picture_surface(&heic).unwrap();
    assert_eq!((surface.width(), surface.height()), (64, 48));
    let list = DisplayList {
        width: 200.0,
        height: 100.0,
        items: vec![
            Item::Image {
                picture: svg,
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 40.0,
            },
            Item::Image {
                picture: heic,
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
    let pixel = |x: usize, y: usize| {
        let p = &data[y * stride + x * 4..][..3];
        (p[2], p[1], p[0])
    };
    // The drawing is stretched to its frame.
    assert_eq!(pixel(5, 5), (0, 0, 255));
    assert_eq!(pixel(75, 35), (0, 0, 255));
    assert_ne!(pixel(140, 70), (255, 255, 255));
    assert_eq!(pixel(150, 20), (255, 255, 255));

    // In a PDF, the SVG stays lines and the photograph is its pixels.
    let out = Painter::new()
        .unwrap()
        .pdf_bytes(&[list], &Metadata::default())
        .unwrap();
    let images = poppler("pdfimages", &["-list"], &out);
    assert_eq!(images.lines().count(), 3, "{images}");
}

#[test]
fn w110_sharpness_and_the_histogram_of_a_picture() {
    use desk_model::{Adjustments, Picture};
    // Black on the left half, white on the right.
    let source = cairo::ImageSurface::create(cairo::Format::ARgb32, 8, 4).unwrap();
    {
        let cr = cairo::Context::new(&source).unwrap();
        cr.set_source_rgb(0.0, 0.0, 0.0);
        cr.paint().unwrap();
        cr.set_source_rgb(0.6, 0.6, 0.6);
        cr.rectangle(4.0, 0.0, 4.0, 4.0);
        cr.fill().unwrap();
    }
    let mut png = Vec::new();
    source.write_to_png(&mut png).unwrap();
    let picture = Picture::from_png(png).unwrap();
    let histogram = desk_render::histogram(&picture, Adjustments::default()).unwrap();
    assert_eq!(histogram.count(), 32);
    assert_eq!(histogram.luma[0], 16);
    assert_eq!(histogram.luma[153], 16);
    // With the levels, as Enhance sees the picture: the gray becomes white.
    let leveled = desk_render::histogram(
        &picture,
        Adjustments {
            white: 153,
            ..Adjustments::default()
        },
    )
    .unwrap();
    assert_eq!(leveled.luma[255], 16);
    let gray = |surface: &mut cairo::ImageSurface, x: usize| {
        let stride = surface.stride() as usize;
        surface.data().unwrap()[stride + x * 4]
    };
    let mut plain =
        desk_render::adjusted_surface(&source, &[], None, Adjustments::default()).unwrap();
    let mut sharp = desk_render::adjusted_surface(
        &source,
        &[],
        None,
        Adjustments {
            sharpness: 100,
            ..Adjustments::default()
        },
    )
    .unwrap();
    let mut soft = desk_render::adjusted_surface(
        &source,
        &[],
        None,
        Adjustments {
            sharpness: -100,
            ..Adjustments::default()
        },
    )
    .unwrap();
    // The light side of the edge gets lighter when sharper, and darker
    // when softer; far from the edge nothing changes.
    let edge = gray(&mut plain, 4);
    assert!(gray(&mut sharp, 4) > edge);
    assert!(gray(&mut soft, 4) < edge);
    assert_eq!(gray(&mut sharp, 7), gray(&mut plain, 7));
}

#[test]
fn w111_a_gallery_shows_the_picture_shown_fitted_in_its_frame() {
    use desk_model::{
        Document, Editor, Frame, Gallery, GalleryImage, Object, ObjectKind, Picture, Wrap,
    };
    let solid = |(r, g, b): (f64, f64, f64), w: i32, h: i32| {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(r, g, b);
            cr.paint().unwrap();
        }
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        Picture::from_png(png).unwrap()
    };
    // Two pictures, twice as wide as tall; the second is shown.
    let gallery = Gallery {
        images: Arc::from([
            GalleryImage::new(solid((1.0, 0.0, 0.0), 40, 20)),
            GalleryImage::new(solid((0.0, 1.0, 0.0), 40, 20)),
        ]),
        shown: 1,
        each_caption: false,
    };
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Galleries", |edit| {
            edit.set_page_setup(PageSetup {
                width: Length::from_points(200),
                height: Length::from_points(100),
                ..PageSetup::default()
            })?;
            for (gallery, x) in [(gallery, 0.0), (Gallery::default(), 100.0)] {
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::Gallery(Arc::new(gallery)),
                    Frame::from_pt(x, 0.0, 80.0, 80.0),
                );
                object.wrap = Wrap::Through;
                edit.add_object(object)?;
            }
            Ok(())
        })
        .unwrap();
    let layout = desk_layout::paginate(editor.document(), &mut TextSystem::new());
    let list = &DisplayList::pages(&layout)[0];
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 100).unwrap();
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
    // The picture shown, whole and centered: 80 wide, 40 tall.
    assert_eq!(pixel(40, 40), (0, 255, 0));
    assert_eq!(pixel(4, 24), (0, 255, 0));
    assert_eq!(pixel(40, 10), (255, 255, 255));
    assert_eq!(pixel(40, 70), (255, 255, 255));
    // A gallery with no pictures yet is a light gray frame.
    assert_eq!(pixel(140, 40), (0xe8, 0xe8, 0xe8));
}
