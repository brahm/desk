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

/// The picture of an image file for a document: PNG, JPEG, PDF, SVG and
/// HEIC as they are, so that photographs stay small and drawings stay
/// sharp, and any other format that GTK reads as PNG (OBJ-01, W108).
///
/// A file larger than `MAX_PICTURE` is not read, as one dropped on the
/// document was read whole whatever its size (review RV, batch 5).
pub fn document_picture_from_file(file: &gtk::gio::File) -> Option<Picture> {
    let bytes = load_bounded(file, MAX_PICTURE)?;
    let bytes: &[u8] = &bytes;
    if bytes.starts_with(b"%PDF-") {
        let (width, height) = desk_render::pdf_size(bytes)?;
        return Picture::from_pdf(bytes.to_vec(), width, height);
    }
    if desk_model::is_svg(bytes) {
        // One that librsvg cannot measure is taken as GTK reads it.
        return desk_render::svg_size(bytes)
            .and_then(|(width, height)| Picture::from_svg(bytes.to_vec(), width, height))
            .or_else(|| picture_from_file(file));
    }
    if desk_model::is_heif(bytes) {
        // Measured as it is drawn, with its turns; one that cannot be
        // decoded is taken as GTK reads it, if it can.
        let picture = desk_render::heic_size(bytes)
            .and_then(|(width, height)| Picture::from_heic(bytes.to_vec(), width, height))
            .filter(|p| desk_render::picture_surface(p).is_some());
        return picture.or_else(|| picture_from_file(file));
    }
    Picture::from_bytes(bytes.to_vec())
        // A JPEG that the renderer cannot decode is taken as GTK reads it.
        .filter(|p| desk_render::picture_surface(p).is_some())
        .or_else(|| picture_from_file(file))
}

/// The largest movie or sound a document keeps, as large as a file of a
/// package that Desk reads.
const MAX_MEDIA: u64 = 64 << 20;

/// The largest picture file a document takes in.
const MAX_PICTURE: u64 = 256 << 20;

/// The size of a file, in bytes, when its file system knows it.
fn file_size(file: &gtk::gio::File) -> Option<u64> {
    let size = file
        .query_info(
            "standard::size",
            gtk::gio::FileQueryInfoFlags::NONE,
            gtk::gio::Cancellable::NONE,
        )
        .ok()?
        .size();
    u64::try_from(size).ok()
}

/// The bytes of a file of at most `most` bytes. One whose size is not
/// known, as on some remote places, is read up to the bound rather than
/// refused (second review of RV batch 5).
fn load_bounded(file: &gtk::gio::File, most: u64) -> Option<Vec<u8>> {
    if file_size(file).is_some_and(|size| size > most) {
        return None;
    }
    let stream = file.read(gtk::gio::Cancellable::NONE).ok()?;
    let mut out = Vec::new();
    loop {
        let chunk = stream
            .read_bytes(1 << 16, gtk::gio::Cancellable::NONE)
            .ok()?;
        if chunk.is_empty() {
            break;
        }
        if (out.len() + chunk.len()) as u64 > most {
            return None;
        }
        out.extend_from_slice(&chunk);
    }
    Some(out)
}

/// A movie or a sound from a file, if Desk knows its format and it is
/// not too large to keep in a document (OBJ-18).
pub fn media_from_file(file: &gtk::gio::File) -> Option<desk_model::Media> {
    let bytes = load_bounded(file, MAX_MEDIA)?;
    desk_model::Media::from_bytes(bytes)
}

/// The 3D model of a file, if it is one that Desk reads (W116), as
/// large as a movie may be.
pub fn model_from_file(file: &gtk::gio::File) -> Option<desk_model::Model3d> {
    let bytes = load_bounded(file, MAX_MEDIA)?;
    desk_model::Model3d::from_bytes(bytes).filter(|m| m.scene().is_ok())
}

/// The filter of the 3D models that Desk reads (W116).
pub fn model_filter() -> gtk::FileFilter {
    let models = gtk::FileFilter::new();
    models.set_name(Some(&crate::i18n::tr("3D Objects")));
    let format = desk_model::ModelFormat::Usdz;
    models.add_mime_type(format.media_type());
    models.add_suffix(format.extension());
    models
}

/// The filters of the dialogs that choose a picture for a document: the
/// images and PDF, with movies, sounds and 3D objects when `media`.
pub fn document_picture_filters(with_media: bool) -> gtk::gio::ListStore {
    let images = gtk::FileFilter::new();
    images.set_name(Some(&crate::i18n::tr("Images")));
    images.add_mime_type("image/*");
    images.add_mime_type("application/pdf");
    images.add_suffix("pdf");
    let media = gtk::FileFilter::new();
    media.set_name(Some(&crate::i18n::tr("Movies and Sounds")));
    for kind in ["video/*", "audio/*"] {
        media.add_mime_type(kind);
    }
    let all = gtk::FileFilter::new();
    all.set_name(Some(&crate::i18n::tr(
        "Images, Movies, Sounds and 3D Objects",
    )));
    all.add_mime_type("image/*");
    for kind in ["application/pdf", "video/*", "audio/*"] {
        all.add_mime_type(kind);
    }
    let format = desk_model::ModelFormat::Usdz;
    all.add_mime_type(format.media_type());
    all.add_suffix(format.extension());
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    if with_media {
        filters.append(&all);
    }
    filters.append(&images);
    if with_media {
        filters.append(&media);
        filters.append(&model_filter());
    }
    filters
}

/// The image of a picture for GTK, for menus and the clipboard: PNG and
/// JPEG as they are, and the others, which GTK may not read, as their
/// pixels drawn in PNG (W108).
pub fn texture(picture: &Picture) -> Option<gtk::gdk::Texture> {
    use desk_model::PictureFormat;
    if matches!(picture.format(), PictureFormat::Png | PictureFormat::Jpeg) {
        return gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from(picture.data())).ok();
    }
    let surface = desk_render::picture_surface(picture)?;
    let mut png = Vec::new();
    surface.write_to_png(&mut png).ok()?;
    gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(png)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_read_up_to_a_bound() {
        // Second review of RV batch 5: a file whose size was not known was
        // refused; it is read up to the bound instead.
        let path = std::env::temp_dir().join(format!("desk-bounded-{}", std::process::id()));
        std::fs::write(&path, vec![7u8; 100_000]).unwrap();
        let file = gtk::gio::File::for_path(&path);
        assert_eq!(load_bounded(&file, 100_000).map(|b| b.len()), Some(100_000));
        assert_eq!(load_bounded(&file, 99_999), None);
        std::fs::remove_file(&path).unwrap();
        // A pipe has no size: what it gives is still bounded.
        let file = gtk::gio::File::for_path("/proc/self/status");
        assert!(file_size(&file).is_none_or(|size| size == 0));
        assert!(load_bounded(&file, 1 << 20).is_some_and(|b| !b.is_empty()));
        assert_eq!(load_bounded(&file, 10), None);
    }

    #[test]
    fn w108_svg_and_heic_pictures_are_given_to_gtk_as_their_pixels() {
        // GTK may read neither: the clipboard and the menus take their
        // pixels.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"/>"#;
        let svg = Picture::from_svg(svg.to_vec(), 40.0, 30.0).unwrap();
        let pixels = texture(&svg).unwrap();
        assert_eq!((pixels.width(), pixels.height()), (80, 60));
        let heic = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/fixtures/images/gradient-64x48.heic"
        ))
        .unwrap();
        let heic = Picture::from_heic(heic, 64, 48).unwrap();
        let pixels = texture(&heic).unwrap();
        assert_eq!((pixels.width(), pixels.height()), (64, 48));
    }

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
