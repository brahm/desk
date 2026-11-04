// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Cairo back end: painting display lists on a Cairo context, and
//! writing them as PDF.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::rc::Rc;

use cairo::{
    Context, FontFace, FontOptions, HintMetrics, HintStyle, Matrix, PdfMetadata, PdfSurface,
};
use desk_model::{Metadata, Rgba};
use desk_text::FontData;

use crate::display::{DisplayList, Item, Paint, PathOp};

/// Why drawing or writing failed.
#[derive(Debug)]
pub enum RenderError {
    /// Cairo failed.
    Cairo(cairo::Error),
    /// FreeType could not open a font.
    Font(freetype::Error),
    /// The PDF could not be written.
    Io(std::io::Error),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cairo(error) => write!(f, "Cairo failed: {error}"),
            Self::Font(error) => write!(f, "a font could not be opened: {error}"),
            Self::Io(error) => write!(f, "the file could not be written: {error}"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<cairo::Error> for RenderError {
    fn from(error: cairo::Error) -> Self {
        Self::Cairo(error)
    }
}

impl From<freetype::Error> for RenderError {
    fn from(error: freetype::Error) -> Self {
        Self::Font(error)
    }
}

impl From<std::io::Error> for RenderError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Paints display lists with Cairo, keeping the fonts it has opened.
pub struct Painter {
    library: freetype::Library,
    faces: HashMap<(u64, u32), FontFace>,
    /// The pictures decoded so far.
    pictures: Vec<(desk_model::Picture, cairo::ImageSurface)>,
    /// The pictures with their colors changed, by what changed them.
    adjusted: Vec<(AdjustedKey, cairo::ImageSurface)>,
}

impl Painter {
    /// A painter with no fonts open yet.
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            library: freetype::Library::init()?,
            faces: HashMap::new(),
            pictures: Vec::new(),
            adjusted: Vec::new(),
        })
    }

    /// The picture decoded, or `None` when Cairo cannot read it.
    fn picture(&mut self, picture: &desk_model::Picture) -> Option<cairo::ImageSurface> {
        if let Some((_, surface)) = self
            .pictures
            .iter()
            .find(|(p, _)| p.same_data(picture) || p == picture)
        {
            return Some(surface.clone());
        }
        let surface =
            cairo::ImageSurface::create_from_png(&mut std::io::Cursor::new(picture.png())).ok()?;
        // A bound on the memory of long sessions.
        if self.pictures.len() > 64 {
            self.pictures.clear();
        }
        self.pictures.push((picture.clone(), surface.clone()));
        Some(surface)
    }

    /// Sets a picture as the source, over a rectangle of the page, or
    /// repeated from it.
    fn set_picture(
        &mut self,
        cr: &Context,
        picture: &desk_model::Picture,
        rect: (f32, f32, f32, f32),
        tile: bool,
    ) -> Result<(), RenderError> {
        let Some(surface) = self.picture(picture) else {
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
            return Ok(());
        };
        let (x, y, w, h) = rect;
        let pattern = cairo::SurfacePattern::create(&surface);
        let (sw, sh) = (
            f64::from(surface.width().max(1)),
            f64::from(surface.height().max(1)),
        );
        let (w, h) = (f64::from(w).max(1e-3), f64::from(h).max(1e-3));
        // From the page to the pixels of the picture.
        let mut matrix = Matrix::identity();
        matrix.scale(sw / w, sh / h);
        matrix.translate(-f64::from(x), -f64::from(y));
        pattern.set_matrix(matrix);
        pattern.set_filter(cairo::Filter::Good);
        pattern.set_extend(if tile {
            cairo::Extend::Repeat
        } else {
            cairo::Extend::None
        });
        cr.set_source(&pattern)?;
        Ok(())
    }

    /// A picture with a color taken out and its colors adjusted
    /// (OBJ-05, OBJ-06), made once.
    fn adjusted_picture(
        &mut self,
        picture: &desk_model::Picture,
        alpha: &[desk_model::AlphaSeed],
        adjust: desk_model::Adjustments,
    ) -> Option<cairo::ImageSurface> {
        let key = AdjustedKey {
            picture: picture.clone(),
            alpha: alpha.to_vec(),
            adjust,
        };
        if let Some((_, surface)) = self.adjusted.iter().find(|(k, _)| *k == key) {
            return Some(surface.clone());
        }
        let source = self.picture(picture)?;
        let surface = adjusted_surface(&source, alpha, adjust).ok()?;
        if self.adjusted.len() > 16 {
            self.adjusted.clear();
        }
        self.adjusted.push((key, surface.clone()));
        Some(surface)
    }

    fn face(&mut self, font: &FontData) -> Result<FontFace, RenderError> {
        let key = (font.data.id(), font.index);
        if let Some(face) = self.faces.get(&key) {
            return Ok(face.clone());
        }
        // Cairo keeps the FreeType face, and the face its copy of the font.
        let bytes = Rc::new(font.data.data().to_vec());
        let ft = self.library.new_memory_face(bytes, font.index as isize)?;
        let face = FontFace::create_from_ft(&ft)?;
        self.faces.insert(key, face.clone());
        Ok(face)
    }

    fn set_font(&mut self, cr: &Context, font: &FontData, size: f32) -> Result<(), RenderError> {
        cr.set_font_face(&self.face(font)?);
        let size = f64::from(size);
        cr.set_font_matrix(Matrix::new(size, 0.0, 0.0, size, 0.0, 0.0));
        Ok(())
    }

    /// Paints a display list on a context, with the page at the origin
    /// and one unit per point.
    pub fn paint(&mut self, cr: &Context, list: &DisplayList) -> Result<(), RenderError> {
        // Glyphs at their exact positions, as the layout measured them.
        let mut options = FontOptions::new()?;
        options.set_hint_style(HintStyle::None);
        options.set_hint_metrics(HintMetrics::Off);
        cr.set_font_options(&options);
        self.paint_items(cr, &list.items)
    }

    fn paint_items(&mut self, cr: &Context, items: &[Item]) -> Result<(), RenderError> {
        for item in items {
            match item {
                Item::Glyphs {
                    font,
                    size,
                    color,
                    glyphs,
                    text,
                    clusters,
                    outline,
                } => {
                    self.set_font(cr, font, *size)?;
                    let glyphs = cairo_glyphs(glyphs);
                    if let Some(width) = outline {
                        set_color(cr, *color);
                        cr.glyph_path(&glyphs);
                        cr.set_line_width(f64::from(*width));
                        cr.set_line_join(cairo::LineJoin::Miter);
                        cr.stroke()?;
                        // The text itself, all but invisible, so that a
                        // PDF keeps it to select and search, as the text
                        // that Pages strokes.
                        set_color(cr, Rgba { a: 1, ..*color });
                    } else {
                        set_color(cr, *color);
                    }
                    if clusters.is_empty() {
                        cr.show_glyphs(&glyphs)?;
                    } else {
                        // With the text, so that ligatures read as the
                        // letters they join.
                        let clusters: Vec<cairo::TextCluster> = clusters
                            .iter()
                            .map(|&(bytes, glyphs)| {
                                cairo::TextCluster::new(bytes as i32, glyphs as i32)
                            })
                            .collect();
                        cr.show_text_glyphs(
                            text,
                            &glyphs,
                            &clusters,
                            cairo::TextClusterFlags::None,
                        )?;
                    }
                }
                Item::Shadow {
                    font,
                    size,
                    color,
                    glyphs,
                    offset,
                    blur,
                } => {
                    self.set_font(cr, font, *size)?;
                    paint_shadow(cr, &cairo_glyphs(glyphs), *color, *offset, *blur)?;
                }
                Item::Stroke {
                    points,
                    closed,
                    width,
                    color,
                    dashes,
                } => {
                    let Some(((x, y), rest)) = points.split_first() else {
                        continue;
                    };
                    set_color(cr, *color);
                    cr.set_line_width(f64::from(*width));
                    cr.set_line_cap(cairo::LineCap::Butt);
                    cr.set_line_join(cairo::LineJoin::Miter);
                    let dashes: Vec<f64> = dashes.iter().map(|&d| f64::from(d)).collect();
                    cr.set_dash(&dashes, 0.0);
                    cr.move_to(f64::from(*x), f64::from(*y));
                    for (x, y) in rest {
                        cr.line_to(f64::from(*x), f64::from(*y));
                    }
                    if *closed {
                        cr.close_path();
                    }
                    cr.stroke()?;
                    cr.set_dash(&[], 0.0);
                }
                Item::Image {
                    picture,
                    x,
                    y,
                    width,
                    height,
                } => {
                    let Some(surface) = self.picture(picture) else {
                        continue;
                    };
                    cr.save()?;
                    cr.translate(f64::from(*x), f64::from(*y));
                    cr.scale(
                        f64::from(*width) / f64::from(surface.width().max(1)),
                        f64::from(*height) / f64::from(surface.height().max(1)),
                    );
                    cr.set_source_surface(&surface, 0.0, 0.0)?;
                    cr.source().set_filter(cairo::Filter::Good);
                    cr.paint()?;
                    cr.restore()?;
                }
                Item::Rect {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => {
                    set_color(cr, *color);
                    cr.rectangle(
                        f64::from(*x),
                        f64::from(*y),
                        f64::from(*width),
                        f64::from(*height),
                    );
                    cr.fill()?;
                }
                Item::Path { path, fill, stroke } => {
                    cr.new_path();
                    for op in path {
                        match *op {
                            PathOp::Move(x, y) => cr.move_to(f64::from(x), f64::from(y)),
                            PathOp::Line(x, y) => cr.line_to(f64::from(x), f64::from(y)),
                            PathOp::Curve(x1, y1, x2, y2, x, y) => cr.curve_to(
                                f64::from(x1),
                                f64::from(y1),
                                f64::from(x2),
                                f64::from(y2),
                                f64::from(x),
                                f64::from(y),
                            ),
                            PathOp::Close => cr.close_path(),
                        }
                    }
                    if let Some(paint) = fill {
                        if let Paint::Image {
                            picture,
                            rect,
                            tile,
                        } = paint
                        {
                            self.set_picture(cr, picture, *rect, *tile)?;
                        } else {
                            set_paint(cr, paint)?;
                        }
                        if stroke.is_some() {
                            cr.fill_preserve()?;
                        } else {
                            cr.fill()?;
                        }
                    }
                    if let Some(stroke) = stroke {
                        set_color(cr, stroke.color);
                        cr.set_line_width(f64::from(stroke.width));
                        cr.set_line_cap(cairo::LineCap::Butt);
                        cr.set_line_join(cairo::LineJoin::Miter);
                        let dashes: Vec<f64> =
                            stroke.dashes.iter().map(|&d| f64::from(d)).collect();
                        cr.set_dash(&dashes, 0.0);
                        cr.stroke()?;
                        cr.set_dash(&[], 0.0);
                    }
                    cr.new_path();
                }
                Item::AdjustedImage {
                    picture,
                    alpha,
                    adjust,
                    rect,
                } => {
                    let Some(surface) = self.adjusted_picture(picture, alpha, *adjust) else {
                        continue;
                    };
                    paint_surface(cr, &surface, *rect)?;
                }
                Item::Clipped { rect, items } => {
                    cr.save()?;
                    let (x, y, w, h) = *rect;
                    cr.rectangle(f64::from(x), f64::from(y), f64::from(w), f64::from(h));
                    cr.clip();
                    self.paint_items(cr, items)?;
                    cr.restore()?;
                }
                Item::CastShadow {
                    items,
                    offset,
                    blur,
                    color,
                } => self.paint_cast_shadow(cr, items, *offset, *blur, *color)?,
                Item::Group {
                    transform,
                    opacity,
                    items,
                } => {
                    if *opacity <= 0.0 {
                        continue;
                    }
                    let [xx, yx, xy, yy, x0, y0] = transform.map(f64::from);
                    cr.save()?;
                    cr.transform(Matrix::new(xx, yx, xy, yy, x0, y0));
                    if *opacity < 1.0 {
                        cr.push_group();
                        self.paint_items(cr, items)?;
                        cr.pop_group_to_source()?;
                        cr.paint_with_alpha(f64::from(*opacity))?;
                    } else {
                        self.paint_items(cr, items)?;
                    }
                    cr.restore()?;
                }
            }
        }
        Ok(())
    }

    /// Paints the blurred shadow that items cast: their shape, in one
    /// color, blurred and moved by the offset (OBJ-11).
    fn paint_cast_shadow(
        &mut self,
        cr: &Context,
        items: &[Item],
        offset: (f32, f32),
        blur: f32,
        color: Rgba,
    ) -> Result<(), RenderError> {
        if color.a == 0 {
            return Ok(());
        }
        // Where the items draw, in the coordinates of the context.
        let recording = cairo::RecordingSurface::create(cairo::Content::Alpha, None)?;
        {
            let rc = Context::new(&recording)?;
            self.paint_items(&rc, items)?;
        }
        let (ix, iy, iw, ih) = recording.ink_extents();
        if iw <= 0.0 || ih <= 0.0 {
            return Ok(());
        }
        let (sx, sy) = cr.user_to_device_distance(1.0, 1.0)?;
        let scale = sx
            .abs()
            .max(sy.abs())
            .clamp(SHADOW_MIN_SCALE, SHADOW_MAX_SCALE)
            // A large object gets fewer pixels for its shadow.
            .min(SHADOW_MAX_SIDE as f64 / iw.max(ih))
            .max(f64::MIN_POSITIVE);
        let sigma = f64::from(blur.max(0.0)) / 2.0 * scale;
        let radius = (((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) / 2.0).round() as usize;
        // Room for the blur on every side.
        let margin = (3 * radius + 1) as f64 / scale;
        let (x0, y0) = (ix - margin, iy - margin);
        let width = ((iw + 2.0 * margin) * scale).ceil() as i32;
        let height = ((ih + 2.0 * margin) * scale).ceil() as i32;
        if width > 2 * SHADOW_MAX_SIDE || height > 2 * SHADOW_MAX_SIDE {
            return Ok(());
        }
        let mut mask = cairo::ImageSurface::create(cairo::Format::A8, width.max(1), height.max(1))?;
        {
            let mc = Context::new(&mask)?;
            mc.scale(scale, scale);
            mc.translate(-x0, -y0);
            mc.set_source_surface(&recording, 0.0, 0.0)?;
            mc.paint()?;
        }
        mask.flush();
        let stride = mask.stride() as usize;
        if radius > 0 {
            let (w, h) = (width.max(1) as usize, height.max(1) as usize);
            let mut data = mask
                .data()
                .map_err(|_| RenderError::Cairo(cairo::Error::SurfaceFinished))?;
            box_blur(&mut data, w, h, stride, radius);
        }
        mask.mark_dirty();
        cr.save()?;
        set_color(cr, color);
        cr.translate(x0 + f64::from(offset.0), y0 + f64::from(offset.1));
        cr.scale(1.0 / scale, 1.0 / scale);
        cr.mask_surface(&mask, 0.0, 0.0)?;
        cr.restore()?;
        Ok(())
    }

    /// Writes pages as a PDF file, with the document information.
    pub fn write_pdf(
        &mut self,
        pages: &[DisplayList],
        metadata: &Metadata,
        path: impl AsRef<Path>,
    ) -> Result<(), RenderError> {
        let (width, height) = pages.first().map_or((595.0, 842.0), |p| {
            (f64::from(p.width), f64::from(p.height))
        });
        let surface = PdfSurface::new(width, height, path)?;
        self.render_pdf(&surface, pages, metadata)?;
        surface.finish();
        surface.status()?;
        Ok(())
    }

    /// Renders pages as PDF into memory.
    pub fn pdf_bytes(
        &mut self,
        pages: &[DisplayList],
        metadata: &Metadata,
    ) -> Result<Vec<u8>, RenderError> {
        let (width, height) = pages.first().map_or((595.0, 842.0), |p| {
            (f64::from(p.width), f64::from(p.height))
        });
        let surface = PdfSurface::for_stream(width, height, Vec::<u8>::new())?;
        self.render_pdf(&surface, pages, metadata)?;
        let stream = surface.finish_output_stream().map_err(|e| e.error)?;
        Ok(*stream
            .downcast::<Vec<u8>>()
            .expect("the stream is the vector given above"))
    }

    /// A preview of a page, as a PNG image that fits in a square of
    /// `size` pixels, on white (DOC-09).
    pub fn thumbnail_png(&mut self, page: &DisplayList, size: u32) -> Result<Vec<u8>, RenderError> {
        let (width, height) = (
            f64::from(page.width.max(1.0)),
            f64::from(page.height.max(1.0)),
        );
        let scale = f64::from(size) / width.max(height);
        let pixels = |points: f64| ((points * scale).round() as i32).max(1);
        let surface =
            cairo::ImageSurface::create(cairo::Format::Rgb24, pixels(width), pixels(height))?;
        {
            let cr = Context::new(&surface)?;
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.paint()?;
            cr.scale(scale, scale);
            self.paint(&cr, page)?;
        }
        let mut png = Vec::new();
        surface.write_to_png(&mut png).map_err(|e| match e {
            cairo::IoError::Io(error) => RenderError::Io(error),
            cairo::IoError::Cairo(error) => RenderError::Cairo(error),
        })?;
        Ok(png)
    }

    fn render_pdf(
        &mut self,
        surface: &PdfSurface,
        pages: &[DisplayList],
        metadata: &Metadata,
    ) -> Result<(), RenderError> {
        surface.set_metadata(PdfMetadata::Creator, "Desk")?;
        for (key, value) in [
            (PdfMetadata::Title, metadata.title.as_str()),
            (PdfMetadata::Author, metadata.author.as_str()),
            (PdfMetadata::Subject, metadata.comments.as_str()),
            (PdfMetadata::Keywords, &metadata.keywords.join(", ")),
        ] {
            if !value.is_empty() {
                surface.set_metadata(key, value)?;
            }
        }
        let cr = Context::new(surface)?;
        for page in pages {
            surface.set_size(f64::from(page.width), f64::from(page.height))?;
            self.paint(&cr, page)?;
            cr.show_page()?;
        }
        Ok(())
    }
}

fn cairo_glyphs(glyphs: &[crate::display::PlacedGlyph]) -> Vec<cairo::Glyph> {
    glyphs
        .iter()
        .map(|g| cairo::Glyph::new(g.id.into(), f64::from(g.x), f64::from(g.y)))
        .collect()
}

/// The fewest pixels per point a shadow is drawn with, so that it looks
/// smooth in a PDF, whose surface counts in points.
const SHADOW_MIN_SCALE: f64 = 4.0;
/// The most, so that a shadow at a high zoom stays small.
const SHADOW_MAX_SCALE: f64 = 8.0;
/// The most pixels along a side of the shadow of an object, before its
/// blur.
const SHADOW_MAX_SIDE: i32 = 4096;

/// Paints the shadow of glyphs, in the font set on the context: the
/// glyphs drawn into a mask, blurred and moved by `offset`, and painted in
/// `color` (FMT-03). A blur of radius `blur` is about a Gaussian blur with a
/// standard deviation of half the radius, as three box blurs.
fn paint_shadow(
    cr: &Context,
    glyphs: &[cairo::Glyph],
    color: Rgba,
    offset: (f32, f32),
    blur: f32,
) -> Result<(), RenderError> {
    let extents = cr.glyph_extents(glyphs)?;
    if extents.width() <= 0.0 || extents.height() <= 0.0 || color.a == 0 {
        return Ok(());
    }
    let (sx, sy) = cr.user_to_device_distance(1.0, 1.0)?;
    let scale = sx
        .abs()
        .max(sy.abs())
        .clamp(SHADOW_MIN_SCALE, SHADOW_MAX_SCALE);
    let sigma = f64::from(blur.max(0.0)) / 2.0 * scale;
    let radius = (((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) / 2.0).round() as usize;
    // Room for the blur on every side.
    let margin = (3 * radius + 1) as f64 / scale;
    let x0 = extents.x_bearing() - margin;
    let y0 = extents.y_bearing() - margin;
    let width = ((extents.width() + 2.0 * margin) * scale).ceil() as i32;
    let height = ((extents.height() + 2.0 * margin) * scale).ceil() as i32;
    let x0 = x0 + glyphs.first().map_or(0.0, |g| g.x());
    let y0 = y0 + glyphs.first().map_or(0.0, |g| g.y());
    let mut mask = cairo::ImageSurface::create(cairo::Format::A8, width.max(1), height.max(1))?;
    {
        let mc = Context::new(&mask)?;
        mc.scale(scale, scale);
        mc.translate(-x0, -y0);
        mc.set_font_face(&cr.font_face());
        mc.set_font_matrix(cr.font_matrix());
        mc.set_font_options(&cr.font_options()?);
        mc.show_glyphs(glyphs)?;
    }
    mask.flush();
    let stride = mask.stride() as usize;
    if radius > 0 {
        let (w, h) = (width as usize, height as usize);
        let mut data = mask
            .data()
            .map_err(|_| RenderError::Cairo(cairo::Error::SurfaceFinished))?;
        box_blur(&mut data, w, h, stride, radius);
    }
    mask.mark_dirty();
    cr.save()?;
    set_color(cr, color);
    cr.translate(x0 + f64::from(offset.0), y0 + f64::from(offset.1));
    cr.scale(1.0 / scale, 1.0 / scale);
    cr.mask_surface(&mask, 0.0, 0.0)?;
    cr.restore()?;
    Ok(())
}

/// Blurs an 8-bit image in place with three box blurs each way.
fn box_blur(data: &mut [u8], width: usize, height: usize, stride: usize, radius: usize) {
    let mut line = Vec::new();
    for _ in 0..3 {
        for y in 0..height {
            line.clear();
            line.extend((0..width).map(|x| data[y * stride + x]));
            blur_line(&line, radius, |x, v| data[y * stride + x] = v);
        }
        for x in 0..width {
            line.clear();
            line.extend((0..height).map(|y| data[y * stride + x]));
            blur_line(&line, radius, |y, v| data[y * stride + x] = v);
        }
    }
}

/// Averages each value of a line with the `radius` values on each side,
/// zero past the ends, and gives each average to `out`.
fn blur_line(line: &[u8], radius: usize, mut out: impl FnMut(usize, u8)) {
    let window = (2 * radius + 1) as u32;
    let at = |i: isize| -> u32 {
        if i < 0 {
            0
        } else {
            line.get(i as usize).map_or(0, |&v| u32::from(v))
        }
    };
    let r = radius as isize;
    let mut sum: u32 = (-r..=r).map(at).sum();
    for i in 0..line.len() as isize {
        out(i as usize, ((sum + window / 2) / window) as u8);
        sum = sum + at(i + r + 1) - at(i - r);
    }
}

/// What a picture was changed by.
#[derive(PartialEq)]
struct AdjustedKey {
    picture: desk_model::Picture,
    alpha: Vec<desk_model::AlphaSeed>,
    adjust: desk_model::Adjustments,
}

/// Paints a surface scaled into a rectangle.
fn paint_surface(
    cr: &Context,
    surface: &cairo::ImageSurface,
    (x, y, width, height): (f32, f32, f32, f32),
) -> Result<(), RenderError> {
    cr.save()?;
    cr.translate(f64::from(x), f64::from(y));
    cr.scale(
        f64::from(width) / f64::from(surface.width().max(1)),
        f64::from(height) / f64::from(surface.height().max(1)),
    );
    cr.set_source_surface(surface, 0.0, 0.0)?;
    cr.source().set_filter(cairo::Filter::Good);
    cr.paint()?;
    cr.restore()?;
    Ok(())
}

/// A copy of a picture with the regions that Instant Alpha clicked taken
/// out, and its colors adjusted (OBJ-05, OBJ-06).
pub fn adjusted_surface(
    source: &cairo::ImageSurface,
    alpha: &[desk_model::AlphaSeed],
    adjust: desk_model::Adjustments,
) -> Result<cairo::ImageSurface, RenderError> {
    let (width, height) = (source.width(), source.height());
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)?;
    {
        let cr = Context::new(&surface)?;
        cr.set_source_surface(source, 0.0, 0.0)?;
        cr.paint()?;
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let (w, h) = (width as usize, height as usize);
    {
        let mut data = surface
            .data()
            .map_err(|_| RenderError::Cairo(cairo::Error::SurfaceFinished))?;
        // Straight colors and opacity, from premultiplied BGRA as Cairo
        // keeps it in memory on little-endian machines.
        let mut pixels: Vec<([f32; 3], f32)> = Vec::with_capacity(w * h);
        for row in 0..h {
            for p in data[row * stride..][..w * 4].as_chunks::<4>().0 {
                let a = f32::from(p[3]) / 255.0;
                let rgb = if a > 0.0 {
                    [p[2], p[1], p[0]].map(|v| (f32::from(v) / 255.0 / a).min(1.0))
                } else {
                    [0.0; 3]
                };
                pixels.push((rgb, a));
            }
        }
        let keep = instant_alpha(&pixels, w, h, alpha);
        for (i, (rgb, a)) in pixels.into_iter().enumerate() {
            let a = a * keep[i];
            let rgb = if adjust.is_none() {
                rgb
            } else {
                adjust.apply(rgb)
            };
            let byte = |v: f32| (v.clamp(0.0, 1.0) * a * 255.0).round() as u8;
            let pixel = &mut data[(i / w) * stride + (i % w) * 4..][..4];
            pixel[0] = byte(rgb[2]);
            pixel[1] = byte(rgb[1]);
            pixel[2] = byte(rgb[0]);
            pixel[3] = (a * 255.0).round() as u8;
        }
    }
    surface.mark_dirty();
    Ok(surface)
}

/// How much of each pixel Instant Alpha keeps, from 0 to 1: each click
/// takes out the pixels joined to it whose colors stay near the one under
/// it, and fades the pixels just past them.
fn instant_alpha(
    pixels: &[([f32; 3], f32)],
    width: usize,
    height: usize,
    seeds: &[desk_model::AlphaSeed],
) -> Vec<f32> {
    let mut keep = vec![1.0f32; pixels.len()];
    if width == 0 || height == 0 {
        return keep;
    }
    let scale = f32::from(desk_model::AlphaSeed::SCALE);
    let mut seen = vec![false; pixels.len()];
    let mut stack = Vec::new();
    for seed in seeds {
        let x = ((f32::from(seed.x) / scale * width as f32) as usize).min(width - 1);
        let y = ((f32::from(seed.y) / scale * height as f32) as usize).min(height - 1);
        let (color, _) = pixels[y * width + x];
        let tolerance = f32::from(seed.tolerance.min(100)) / 100.0;
        // How far past the tolerance a color still fades.
        let edge = 0.04;
        let distance = |i: usize| {
            let (c, _) = pixels[i];
            ((c[0] - color[0]).powi(2) + (c[1] - color[1]).powi(2) + (c[2] - color[2]).powi(2))
                .sqrt()
                / 3f32.sqrt()
        };
        seen.iter_mut().for_each(|s| *s = false);
        stack.clear();
        stack.push(y * width + x);
        seen[y * width + x] = true;
        while let Some(i) = stack.pop() {
            let d = distance(i);
            if d > tolerance {
                // Just past the region: faded, and not gone through.
                if d < tolerance + edge {
                    keep[i] = keep[i].min((d - tolerance) / edge);
                }
                continue;
            }
            keep[i] = 0.0;
            let (px, py) = (i % width, i / width);
            let mut visit = |j: usize| {
                if !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if px > 0 {
                visit(i - 1);
            }
            if px + 1 < width {
                visit(i + 1);
            }
            if py > 0 {
                visit(i - width);
            }
            if py + 1 < height {
                visit(i + width);
            }
        }
    }
    keep
}

fn set_paint(cr: &Context, paint: &Paint) -> Result<(), RenderError> {
    match paint.clone() {
        Paint::Color(color) => set_color(cr, color),
        // The painter sets a picture, from its surfaces.
        Paint::Image { .. } => {}
        Paint::Linear {
            start,
            end,
            from,
            to,
        } => {
            let gradient = cairo::LinearGradient::new(
                f64::from(start.0),
                f64::from(start.1),
                f64::from(end.0),
                f64::from(end.1),
            );
            for (at, color) in [(0.0, from), (1.0, to)] {
                gradient.add_color_stop_rgba(
                    at,
                    f64::from(color.r) / 255.0,
                    f64::from(color.g) / 255.0,
                    f64::from(color.b) / 255.0,
                    f64::from(color.a) / 255.0,
                );
            }
            cr.set_source(&gradient)?;
        }
    }
    Ok(())
}

fn set_color(cr: &Context, color: Rgba) {
    cr.set_source_rgba(
        f64::from(color.r) / 255.0,
        f64::from(color.g) / 255.0,
        f64::from(color.b) / 255.0,
        f64::from(color.a) / 255.0,
    );
}

/// A picture made smaller for its place, as Reduce File Size does
/// (OBJ-06): only the part that its mask shows, at two pixels a point of
/// its frame, and never larger than it was. `mask` is where the whole
/// picture lies from the top left corner of a frame of `size`, in points.
/// None when it would not be smaller.
pub fn reduced_picture(
    picture: &desk_model::Picture,
    mask: Option<desk_model::Frame>,
    size: (f64, f64),
) -> Option<desk_model::Picture> {
    let source =
        cairo::ImageSurface::create_from_png(&mut std::io::Cursor::new(picture.png())).ok()?;
    let (pw, ph) = (f64::from(source.width()), f64::from(source.height()));
    let (fw, fh) = size;
    if fw <= 0.0 || fh <= 0.0 {
        return None;
    }
    // Where the whole picture lies, in points from the corner of the frame.
    let (mx, my, mw, mh) = mask.map_or((0.0, 0.0, fw, fh), |m| {
        (m.x.to_pt(), m.y.to_pt(), m.width.to_pt(), m.height.to_pt())
    });
    if mw <= 0.0 || mh <= 0.0 {
        return None;
    }
    // The pixels of the picture that the frame shows, and as many as it
    // needs.
    let (shown_w, shown_h) = (fw / mw * pw, fh / mh * ph);
    let width = (fw * 2.0).min(shown_w).ceil().max(1.0) as i32;
    let height = (fh * 2.0).min(shown_h).ceil().max(1.0) as i32;
    if f64::from(width) * f64::from(height) >= pw * ph * 0.95 {
        return None;
    }
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    {
        let cr = Context::new(&surface).ok()?;
        cr.scale(f64::from(width) / fw, f64::from(height) / fh);
        cr.translate(mx, my);
        cr.scale(mw / pw, mh / ph);
        cr.set_source_surface(&source, 0.0, 0.0).ok()?;
        cr.source().set_filter(cairo::Filter::Best);
        cr.paint().ok()?;
    }
    let mut png = Vec::new();
    surface.write_to_png(&mut png).ok()?;
    desk_model::Picture::from_png(png)
}
