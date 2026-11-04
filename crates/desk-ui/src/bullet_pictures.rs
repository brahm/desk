// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The image bullets that come with Desk (LST-03): pictures drawn here,
//! so that they need no files, and pictures from files of the user.

use std::f64::consts::PI;

use desk_model::Picture;
use gtk::cairo;
use gtk::prelude::*;

use crate::i18n::tr;

/// The side of a picture that comes with Desk, in pixels.
const SIDE: i32 = 64;

/// How a picture that comes with Desk is drawn.
#[derive(Clone, Copy)]
enum Shape {
    /// A shiny ball of a color.
    Ball(f64, f64, f64),
    /// A check mark.
    Check,
    /// A rounded square of a color.
    Square(f64, f64, f64),
    /// An arrow to the right.
    Arrow,
}

/// How many pictures come with Desk.
const SHAPES: usize = 8;

/// The pictures that come with Desk, with their names.
fn shapes() -> [(String, Shape); SHAPES] {
    [
        (tr("Blue Ball"), Shape::Ball(0.2, 0.45, 0.9)),
        (tr("Red Ball"), Shape::Ball(0.85, 0.2, 0.2)),
        (tr("Green Ball"), Shape::Ball(0.25, 0.65, 0.3)),
        (tr("Orange Ball"), Shape::Ball(0.95, 0.55, 0.15)),
        (tr("Gray Ball"), Shape::Ball(0.5, 0.5, 0.52)),
        (tr("Check Mark"), Shape::Check),
        (tr("Blue Square"), Shape::Square(0.2, 0.45, 0.9)),
        (tr("Arrow"), Shape::Arrow),
    ]
}

fn draw(cr: &cairo::Context, shape: Shape) -> Result<(), cairo::Error> {
    let side = f64::from(SIDE);
    match shape {
        Shape::Ball(r, g, b) => {
            let gradient = cairo::RadialGradient::new(
                side * 0.38,
                side * 0.32,
                side * 0.04,
                side * 0.5,
                side * 0.5,
                side * 0.48,
            );
            gradient.add_color_stop_rgb(0.0, 1.0, 1.0, 1.0);
            gradient.add_color_stop_rgb(0.25, r, g, b);
            gradient.add_color_stop_rgb(1.0, r * 0.45, g * 0.45, b * 0.45);
            cr.arc(side / 2.0, side / 2.0, side * 0.46, 0.0, 2.0 * PI);
            cr.set_source(&gradient)?;
            cr.fill()?;
        }
        Shape::Check => {
            cr.set_source_rgb(0.15, 0.6, 0.25);
            cr.set_line_width(side * 0.14);
            cr.set_line_cap(cairo::LineCap::Round);
            cr.set_line_join(cairo::LineJoin::Round);
            cr.move_to(side * 0.15, side * 0.55);
            cr.line_to(side * 0.4, side * 0.8);
            cr.line_to(side * 0.85, side * 0.2);
            cr.stroke()?;
        }
        Shape::Square(r, g, b) => {
            let (x, y, w, radius) = (side * 0.1, side * 0.1, side * 0.8, side * 0.14);
            cr.new_sub_path();
            cr.arc(x + w - radius, y + radius, radius, -PI / 2.0, 0.0);
            cr.arc(x + w - radius, y + w - radius, radius, 0.0, PI / 2.0);
            cr.arc(x + radius, y + w - radius, radius, PI / 2.0, PI);
            cr.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
            cr.close_path();
            let gradient = cairo::LinearGradient::new(0.0, y, 0.0, y + w);
            gradient.add_color_stop_rgb(0.0, r * 0.6 + 0.4, g * 0.6 + 0.4, b * 0.6 + 0.4);
            gradient.add_color_stop_rgb(1.0, r * 0.7, g * 0.7, b * 0.7);
            cr.set_source(&gradient)?;
            cr.fill()?;
        }
        Shape::Arrow => {
            cr.set_source_rgb(0.25, 0.25, 0.28);
            cr.move_to(side * 0.1, side * 0.38);
            cr.line_to(side * 0.52, side * 0.38);
            cr.line_to(side * 0.52, side * 0.15);
            cr.line_to(side * 0.92, side * 0.5);
            cr.line_to(side * 0.52, side * 0.85);
            cr.line_to(side * 0.52, side * 0.62);
            cr.line_to(side * 0.1, side * 0.62);
            cr.close_path();
            cr.fill()?;
        }
    }
    Ok(())
}

fn picture_of(shape: Shape) -> Option<Picture> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, SIDE, SIDE).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        draw(&cr, shape).ok()?;
    }
    let mut png = Vec::new();
    surface.write_to_png(&mut png).ok()?;
    Picture::from_png(png)
}

thread_local! {
    static PICTURES: Vec<(String, Picture)> = shapes()
        .into_iter()
        .filter_map(|(name, shape)| Some((name, picture_of(shape)?)))
        .collect();
}

/// The pictures that come with Desk, with their names, in the order of
/// their menu.
pub fn pictures() -> Vec<(String, Picture)> {
    PICTURES.with(Clone::clone)
}

/// The picture that a new image bullet takes.
pub fn default_picture() -> Option<Picture> {
    PICTURES.with(|p| p.first().map(|(_, picture)| picture.clone()))
}

/// The picture of an image file, in any format that GTK reads, as PNG.
pub fn picture_from_file(file: &gtk::gio::File) -> Option<Picture> {
    let texture = gtk::gdk::Texture::from_file(file).ok()?;
    Picture::from_png(texture.save_to_png_bytes().to_vec())
}

/// The picture of an image file for a document: PNG, JPEG and PDF as
/// they are, so that photographs stay small and PDF stays sharp, and any
/// other format that GTK reads as PNG (OBJ-01).
pub fn document_picture_from_file(file: &gtk::gio::File) -> Option<Picture> {
    let (bytes, _) = file.load_contents(gtk::gio::Cancellable::NONE).ok()?;
    let bytes: &[u8] = &bytes;
    if bytes.starts_with(b"%PDF-") {
        let (width, height) = desk_render::pdf_size(bytes)?;
        return Picture::from_pdf(bytes.to_vec(), width, height);
    }
    Picture::from_bytes(bytes.to_vec())
        // A JPEG that the renderer cannot decode is taken as GTK reads it.
        .filter(|p| desk_render::picture_surface(p).is_some())
        .or_else(|| picture_from_file(file))
}

/// The filter of the dialogs that choose a picture for a document: the
/// formats that GTK reads, and PDF.
pub fn document_picture_filters() -> gtk::gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&crate::i18n::tr("Images")));
    filter.add_pixbuf_formats();
    filter.add_mime_type("application/pdf");
    filter.add_suffix("pdf");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

/// A small image of a picture, for menus.
pub fn texture(picture: &Picture) -> Option<gtk::gdk::Texture> {
    gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from(picture.data())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lst_03_desk_has_pictures_for_image_bullets() {
        let pictures = pictures();
        assert_eq!(pictures.len(), SHAPES);
        for (_, picture) in &pictures {
            assert_eq!((picture.width(), picture.height()), (64, 64));
        }
        assert_ne!(pictures[0].1, pictures[1].1);
    }
}
