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

use crate::display::{DisplayList, Item};

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
}

impl Painter {
    /// A painter with no fonts open yet.
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            library: freetype::Library::init()?,
            faces: HashMap::new(),
        })
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
        for item in &list.items {
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
            }
        }
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

fn set_color(cr: &Context, color: Rgba) {
    cr.set_source_rgba(
        f64::from(color.r) / 255.0,
        f64::from(color.g) / 255.0,
        f64::from(color.b) / 255.0,
        f64::from(color.a) / 255.0,
    );
}
