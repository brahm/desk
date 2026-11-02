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
                } => {
                    cr.set_font_face(&self.face(font)?);
                    cr.set_font_matrix(Matrix::new(
                        f64::from(*size),
                        0.0,
                        0.0,
                        f64::from(*size),
                        0.0,
                        0.0,
                    ));
                    set_color(cr, *color);
                    let glyphs: Vec<cairo::Glyph> = glyphs
                        .iter()
                        .map(|g| cairo::Glyph::new(g.id.into(), f64::from(g.x), f64::from(g.y)))
                        .collect();
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

fn set_color(cr: &Context, color: Rgba) {
    cr.set_source_rgba(
        f64::from(color.r) / 255.0,
        f64::from(color.g) / 255.0,
        f64::from(color.b) / 255.0,
        f64::from(color.a) / 255.0,
    );
}
