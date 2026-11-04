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
                ObjectKind::TextBox(TextBox {
                    paragraphs: Arc::from([paragraph]),
                    inset: Length::from_points(4),
                }),
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
