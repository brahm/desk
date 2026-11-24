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
        if let Some(at) = self
            .pictures
            .iter()
            .position(|(p, _)| p.same_data(picture) || p == picture)
        {
            return Some(used(&mut self.pictures, at));
        }
        let surface = picture_surface(picture)?;
        // A bound on the memory of long sessions: on the pictures, and on
        // their pixels, which a few large ones fill.
        evict(&mut self.pictures, 64, surface_bytes(&surface));
        self.pictures.push((picture.clone(), surface.clone()));
        Some(surface)
    }

    /// Sets what fills glyphs as the source: their gradient or picture,
    /// or their color (PXV-04).
    fn set_glyph_paint(
        &mut self,
        cr: &Context,
        paint: Option<&Paint>,
        color: Rgba,
    ) -> Result<(), RenderError> {
        match paint {
            Some(Paint::Image {
                picture,
                rect,
                tile,
                ..
            }) => self.set_picture(cr, picture, *rect, *tile),
            Some(paint) => set_paint(cr, paint),
            None => {
                set_color(cr, color);
                Ok(())
            }
        }
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
    /// (OBJ-06, OBJ-07), made once.
    fn adjusted_picture(
        &mut self,
        picture: &desk_model::Picture,
        alpha: &[desk_model::AlphaSeed],
        kept: Option<&desk_model::AlphaOutline>,
        adjust: desk_model::Adjustments,
    ) -> Option<cairo::ImageSurface> {
        let key = AdjustedKey {
            picture: picture.clone(),
            alpha: alpha.to_vec(),
            kept: kept.cloned(),
            adjust,
        };
        if let Some(at) = self.adjusted.iter().position(|(k, _)| *k == key) {
            return Some(used(&mut self.adjusted, at));
        }
        let source = self.picture(picture)?;
        let surface = adjusted_surface(&source, alpha, kept, adjust).ok()?;
        evict(&mut self.adjusted, 16, surface_bytes(&surface));
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
                    paint,
                } => {
                    self.set_font(cr, font, *size)?;
                    let glyphs = cairo_glyphs(glyphs);
                    if let Some(width) = outline {
                        self.set_glyph_paint(cr, paint.as_ref(), *color)?;
                        cr.glyph_path(&glyphs);
                        cr.set_line_width(f64::from(*width));
                        cr.set_line_join(cairo::LineJoin::Miter);
                        cr.stroke()?;
                        // The text itself, all but invisible, so that a
                        // PDF keeps it to select and search, as the text
                        // that Pages strokes.
                        set_color(cr, Rgba { a: 1, ..*color });
                    } else {
                        self.set_glyph_paint(cr, paint.as_ref(), *color)?;
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
                    // The tint of a picture goes over it, inside the
                    // letters (OBJ-42, PXV-04).
                    if outline.is_none()
                        && let Some(Paint::Image {
                            tint: Some(tint), ..
                        }) = paint
                        && tint.a > 0
                    {
                        cr.new_path();
                        cr.glyph_path(&glyphs);
                        set_color(cr, *tint);
                        cr.fill()?;
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
                    if !drawn_width(*width) {
                        continue;
                    }
                    set_color(cr, *color);
                    cr.set_line_width(f64::from(*width));
                    cr.set_line_cap(cairo::LineCap::Butt);
                    cr.set_line_join(cairo::LineJoin::Miter);
                    cr.set_dash(&dash_pattern(dashes), 0.0);
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
                    if !drawn_rect(*width, *height) {
                        continue;
                    }
                    if picture.format() == desk_model::PictureFormat::Pdf {
                        paint_pdf(cr, picture, (*x, *y, *width, *height))?;
                        continue;
                    }
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
                    trace(cr, path);
                    if let Some(paint) = fill {
                        if let Paint::Image {
                            picture,
                            rect,
                            tile,
                            tint,
                        } = paint
                        {
                            self.set_picture(cr, picture, *rect, *tile)?;
                            // The tint goes over the picture, inside the
                            // path (OBJ-42).
                            if let Some(tint) = tint.filter(|t| t.a > 0) {
                                cr.fill_preserve()?;
                                set_color(cr, tint);
                            }
                        } else {
                            set_paint(cr, paint)?;
                        }
                        if stroke.is_some() {
                            cr.fill_preserve()?;
                        } else {
                            cr.fill()?;
                        }
                    }
                    if let Some(stroke) = stroke.as_ref().filter(|s| drawn_width(s.width)) {
                        set_color(cr, stroke.color);
                        cr.set_line_width(f64::from(stroke.width));
                        cr.set_line_cap(cairo::LineCap::Butt);
                        cr.set_line_join(cairo::LineJoin::Miter);
                        cr.set_dash(&dash_pattern(&stroke.dashes), 0.0);
                        cr.stroke()?;
                        cr.set_dash(&[], 0.0);
                    }
                    cr.new_path();
                }
                Item::AdjustedImage {
                    picture,
                    alpha,
                    kept,
                    adjust,
                    rect,
                } => {
                    let Some(surface) =
                        self.adjusted_picture(picture, alpha, kept.as_ref(), *adjust)
                    else {
                        continue;
                    };
                    paint_surface(cr, &surface, *rect)?;
                }
                Item::ClippedTo { path, items } => {
                    cr.save()?;
                    cr.new_path();
                    trace(cr, path);
                    cr.clip();
                    self.paint_items(cr, items)?;
                    cr.restore()?;
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
                Item::Reflection { items, opacity } => {
                    self.paint_reflection(cr, items, *opacity)?;
                }
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

    /// Paints items upside down below where they draw, mirrored at their
    /// foot, fading from an opacity there to nothing over half their
    /// height, as the reflection of Pages (OBJ-38).
    fn paint_reflection(
        &mut self,
        cr: &Context,
        items: &[Item],
        opacity: f32,
    ) -> Result<(), RenderError> {
        if opacity <= 0.0 {
            return Ok(());
        }
        let recording = cairo::RecordingSurface::create(cairo::Content::ColorAlpha, None)?;
        {
            let rc = Context::new(&recording)?;
            self.paint_items(&rc, items)?;
        }
        let (_, iy, iw, ih) = recording.ink_extents();
        if iw <= 0.0 || ih <= 0.0 {
            return Ok(());
        }
        let foot = iy + ih;
        cr.save()?;
        cr.push_group();
        cr.transform(Matrix::new(1.0, 0.0, 0.0, -1.0, 0.0, 2.0 * foot));
        cr.set_source_surface(&recording, 0.0, 0.0)?;
        cr.paint()?;
        cr.pop_group_to_source()?;
        let fade = cairo::LinearGradient::new(0.0, foot, 0.0, foot + ih / 2.0);
        fade.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, f64::from(opacity.min(1.0)));
        fade.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
        cr.mask(&fade)?;
        cr.restore()?;
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
        .clamp(SHADOW_MIN_SCALE, SHADOW_MAX_SCALE)
        // Large text gets fewer pixels for its shadow, as an object does.
        .min(f64::from(SHADOW_MAX_SIDE) / extents.width().max(extents.height()))
        .max(f64::MIN_POSITIVE);
    let sigma = f64::from(blur.max(0.0)) / 2.0 * scale;
    let radius = (((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) / 2.0).round() as usize;
    // Room for the blur on every side.
    let margin = (3 * radius + 1) as f64 / scale;
    let x0 = extents.x_bearing() - margin;
    let y0 = extents.y_bearing() - margin;
    let width = ((extents.width() + 2.0 * margin) * scale).ceil();
    let height = ((extents.height() + 2.0 * margin) * scale).ceil();
    // A blur wider than the text leaves no shadow to see: it is not drawn,
    // and the rest of the page is.
    if !(width <= f64::from(2 * SHADOW_MAX_SIDE) && height <= f64::from(2 * SHADOW_MAX_SIDE)) {
        return Ok(());
    }
    let (width, height) = (width as i32, height as i32);
    let x0 = x0 + glyphs.first().map_or(0.0, |g| g.x());
    let y0 = y0 + glyphs.first().map_or(0.0, |g| g.y());
    let Ok(mut mask) = cairo::ImageSurface::create(cairo::Format::A8, width.max(1), height.max(1))
    else {
        return Ok(());
    };
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
    kept: Option<desk_model::AlphaOutline>,
    adjust: desk_model::Adjustments,
}

/// Whether a line of a width draws anything: a width of 0, negative or
/// not finite does not, and Cairo would take its dashes for an error.
fn drawn_width(width: f32) -> bool {
    width.is_finite() && width > 0.0
}

/// Whether a rectangle of a size draws anything: scaling into a side of 0
/// would leave Cairo in an error that stops the rest of the page.
fn drawn_rect(width: f32, height: f32) -> bool {
    drawn_width(width) && drawn_width(height)
}

/// The dashes of a line as Cairo takes them: none, a solid line, for a
/// pattern Cairo refuses, of a negative or infinite dash or all of 0,
/// which would put the context in an error for the rest of the page.
fn dash_pattern(dashes: &[f32]) -> Vec<f64> {
    let valid =
        dashes.iter().all(|d| d.is_finite() && *d >= 0.0) && dashes.iter().any(|d| *d > 0.0);
    if valid {
        dashes.iter().map(|&d| f64::from(d)).collect()
    } else {
        Vec::new()
    }
}

/// Paints a surface scaled into a rectangle.
fn paint_surface(
    cr: &Context,
    surface: &cairo::ImageSurface,
    (x, y, width, height): (f32, f32, f32, f32),
) -> Result<(), RenderError> {
    if !drawn_rect(width, height) {
        return Ok(());
    }
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
/// out, and its colors adjusted (OBJ-06, OBJ-07).
pub fn adjusted_surface(
    source: &cairo::ImageSurface,
    alpha: &[desk_model::AlphaSeed],
    kept: Option<&desk_model::AlphaOutline>,
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
    match kept {
        Some(kept) => keep_inside(&surface, kept),
        None => Ok(surface),
    }
}

/// The part of a picture inside the polygons that Instant Alpha keeps, by
/// the even-odd rule, as Pages saves it (OBJ-06).
fn keep_inside(
    surface: &cairo::ImageSurface,
    kept: &desk_model::AlphaOutline,
) -> Result<cairo::ImageSurface, RenderError> {
    let (width, height) = (surface.width(), surface.height());
    let out = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)?;
    {
        let cr = Context::new(&out)?;
        let scale = f64::from(desk_model::POLYGON_SCALE);
        let (sx, sy) = (f64::from(width) / scale, f64::from(height) / scale);
        for polygon in kept.iter() {
            for (i, &(x, y)) in polygon.iter().enumerate() {
                let (x, y) = (f64::from(x) * sx, f64::from(y) * sy);
                if i == 0 {
                    cr.move_to(x, y);
                } else {
                    cr.line_to(x, y);
                }
            }
            cr.close_path();
        }
        cr.set_fill_rule(cairo::FillRule::EvenOdd);
        cr.clip();
        cr.set_source_surface(surface, 0.0, 0.0)?;
        cr.paint()?;
    }
    Ok(out)
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
        Paint::Linear { start, end, stops } => {
            let gradient = cairo::LinearGradient::new(
                f64::from(start.0),
                f64::from(start.1),
                f64::from(end.0),
                f64::from(end.1),
            );
            add_stops(&gradient, &stops);
            cr.set_source(&gradient)?;
        }
        Paint::Radial {
            center,
            radius,
            stops,
        } => {
            let (cx, cy) = (f64::from(center.0), f64::from(center.1));
            let gradient = cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, f64::from(radius));
            add_stops(&gradient, &stops);
            cr.set_source(&gradient)?;
        }
    }
    Ok(())
}

/// Adds a path to the current one of a context.
fn trace(cr: &Context, path: &[PathOp]) {
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
}

fn add_stops(gradient: &cairo::Gradient, stops: &[(f32, Rgba)]) {
    for &(at, color) in stops {
        gradient.add_color_stop_rgba(
            f64::from(at.clamp(0.0, 1.0)),
            f64::from(color.r) / 255.0,
            f64::from(color.g) / 255.0,
            f64::from(color.b) / 255.0,
            f64::from(color.a) / 255.0,
        );
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

/// A picture made smaller for its place, as Reduce File Size does
/// (OBJ-05): only the part that its mask shows, at two pixels a point of
/// its frame, and never larger than it was. `mask` is where the whole
/// picture lies from the top left corner of a frame of `size`, in points.
/// None when it would not be smaller.
pub fn reduced_picture(
    picture: &desk_model::Picture,
    mask: Option<desk_model::Frame>,
    size: (f64, f64),
) -> Option<desk_model::Picture> {
    // A PDF stays as it is, which is as sharp and about as small.
    if picture.format() == desk_model::PictureFormat::Pdf {
        return None;
    }
    let source = picture_surface(picture)?;
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
    // A photograph in JPEG can be smaller than the fewer pixels in PNG.
    if png.len() >= picture.data().len() {
        return None;
    }
    desk_model::Picture::from_png(png)
}

/// The largest side of a PDF drawn as pixels, in pixels.
const MAX_DECODED_SIDE: f64 = 8192.0;

/// The most pixels of a picture decoded to draw: those of a square of
/// 8192, in any shape, with no side over the 32767 that Cairo takes.
const MAX_DECODED_PIXELS: u64 = 8192 * 8192;
const MAX_SURFACE_SIDE: u32 = 32767;

/// Whether a picture is small enough to decode, by the size its header
/// declares.
fn decodable(picture: &desk_model::Picture) -> bool {
    let (w, h) = (picture.width(), picture.height());
    w.max(h) <= MAX_SURFACE_SIDE && u64::from(w) * u64::from(h) <= MAX_DECODED_PIXELS
}

/// The most bytes of pixels that the decoded pictures, and those adjusted,
/// keep between paints.
const MAX_CACHED_BYTES: usize = 512 << 20;

/// Leaves out the surfaces kept that were used least lately, until there
/// are at most `count` and room for `adding` more bytes within
/// [`MAX_CACHED_BYTES`]: the pictures of the page painted stay, and are not
/// decoded again at each paint.
fn evict<K>(kept: &mut Vec<(K, cairo::ImageSurface)>, count: usize, adding: usize) {
    let mut held: usize = kept.iter().map(|(_, s)| surface_bytes(s)).sum();
    let mut drop = 0;
    while drop < kept.len() && (kept.len() - drop > count || held + adding > MAX_CACHED_BYTES) {
        held -= kept.get(drop).map_or(0, |(_, s)| surface_bytes(s));
        drop += 1;
    }
    kept.drain(..drop);
}

/// The surface kept at `at`, moved last as the newest used, so that the
/// pictures painted again are the last to be left out.
fn used<K>(kept: &mut Vec<(K, cairo::ImageSurface)>, at: usize) -> cairo::ImageSurface {
    let entry = kept.remove(at);
    let surface = entry.1.clone();
    kept.push(entry);
    surface
}

/// The bytes of the pixels of a surface.
fn surface_bytes(surface: &cairo::ImageSurface) -> usize {
    usize::try_from(surface.stride()).unwrap_or(0) * usize::try_from(surface.height()).unwrap_or(0)
}

/// The pixels of a picture, to draw: PNG and JPEG decoded, and the first
/// page of a PDF drawn at two pixels a point.
pub fn picture_surface(picture: &desk_model::Picture) -> Option<cairo::ImageSurface> {
    use desk_model::PictureFormat;
    match picture.format() {
        // A PNG or JPEG of a few bytes may declare billions of pixels,
        // which would be allocated and filled: its size, from its header,
        // is checked first. A tall screenshot keeps its pixels.
        PictureFormat::Png => decodable(picture)
            .then(|| {
                cairo::ImageSurface::create_from_png(&mut std::io::Cursor::new(picture.data())).ok()
            })
            .flatten(),
        PictureFormat::Jpeg if !decodable(picture) => None,
        PictureFormat::Jpeg => jpeg_surface(picture.shared_data()),
        PictureFormat::Pdf => {
            let (w, h) = (f64::from(picture.width()), f64::from(picture.height()));
            let scale = 2.0f64.min(MAX_DECODED_SIDE / w.max(h)).max(1e-3);
            let width = (w * scale).ceil().max(1.0) as i32;
            let height = (h * scale).ceil().max(1.0) as i32;
            let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
            {
                let cr = Context::new(&surface).ok()?;
                cr.scale(scale, scale);
                pdf_page(picture.data())?.render(&cr);
            }
            surface.flush();
            Some(surface)
        }
    }
}

/// The first page of a PDF.
fn pdf_page(data: &[u8]) -> Option<poppler::Page> {
    let bytes = cairo::glib::Bytes::from(data);
    let document = poppler::Document::from_bytes(&bytes, None).ok()?;
    document.page(0)
}

/// The size of the first page of a PDF, in points.
pub fn pdf_size(data: &[u8]) -> Option<(f64, f64)> {
    let (width, height) = pdf_page(data)?.size();
    (width.is_finite() && height.is_finite() && width >= 1.0 && height >= 1.0)
        .then_some((width, height))
}

/// Draws the first page of a PDF into a rectangle, as lines and text, so
/// that it stays sharp on screen and in a PDF.
fn paint_pdf(
    cr: &Context,
    picture: &desk_model::Picture,
    (x, y, width, height): (f32, f32, f32, f32),
) -> Result<(), RenderError> {
    let Some(page) = pdf_page(picture.data()) else {
        return Ok(());
    };
    let (pw, ph) = page.size();
    if !(pw > 0.0 && ph > 0.0 && drawn_rect(width, height)) {
        return Ok(());
    }
    cr.save()?;
    cr.rectangle(
        f64::from(x),
        f64::from(y),
        f64::from(width),
        f64::from(height),
    );
    cr.clip();
    cr.translate(f64::from(x), f64::from(y));
    cr.scale(f64::from(width) / pw, f64::from(height) / ph);
    page.render(cr);
    cr.restore()?;
    Ok(())
}

/// The pixels of a JPEG file, with the file kept on the surface so that a
/// PDF embeds it as it is.
fn jpeg_surface(data: std::sync::Arc<[u8]>) -> Option<cairo::ImageSurface> {
    use zune_jpeg::JpegDecoder;
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_max_width(MAX_SURFACE_SIDE as usize)
        .set_max_height(MAX_SURFACE_SIDE as usize);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(&data[..]), options);
    // The size the decoder reads, which may not be the one the model read
    // for the picture: bounded before the pixels are allocated.
    decoder.decode_headers().ok()?;
    let info = decoder.info()?;
    let (width, height) = (usize::from(info.width), usize::from(info.height));
    if (width as u64) * (height as u64) > MAX_DECODED_PIXELS {
        return None;
    }
    let pixels = decoder.decode().ok()?;
    if width == 0 || height == 0 || pixels.len() < width * height * 3 {
        return None;
    }
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::Rgb24, width as i32, height as i32).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut out = surface.data().ok()?;
        for row in 0..height {
            for (x, rgb) in pixels[row * width * 3..][..width * 3]
                .as_chunks::<3>()
                .0
                .iter()
                .enumerate()
            {
                // BGRX in memory on little-endian machines.
                let at = row * stride + x * 4;
                out[at] = rgb[2];
                out[at + 1] = rgb[1];
                out[at + 2] = rgb[0];
                out[at + 3] = 255;
            }
        }
    }
    surface.mark_dirty();
    // Cairo writes the JPEG itself into a PDF, rather than its pixels.
    let _ = surface.set_mime_data("image/jpeg", data);
    Some(surface)
}

#[cfg(test)]
mod picture_tests {
    use super::*;

    /// The CRC of a PNG chunk.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in bytes {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// A JPEG of 8 by 8 pixels, made for the test.
    const SMALL_JPEG: &str = concat!(
        "ffd8ffe000104a46494600010100000100010000ffdb004300100b0c0e0c0a10",
        "0e0d0e1211101318281a181616183123251d283a333d3c3933383740485c4e40",
        "4457453738506d51575f626768673e4d71797064785c656763ffdb0043011112",
        "121815182f1a1a2f634238426363636363636363636363636363636363636363",
        "636363636363636363636363636363636363636363636363636363636363ffc0",
        "0011080008000803012200021101031101ffc400150001010000000000000000",
        "0000000000000002ffc40014100100000000000000000000000000000000ffc4",
        "0014010100000000000000000000000000000005ffc400141101000000000000",
        "00000000000000000000ffda000c03010002110311003f009005997fffd9",
    );

    #[test]
    fn the_pictures_used_least_lately_are_left_out_first() {
        // Second review of RV batch 4: a picture painted again was left out
        // as the oldest, and decoded again at the next paint.
        let surface = || cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).unwrap();
        let mut kept: Vec<(u8, cairo::ImageSurface)> = (0..3).map(|k| (k, surface())).collect();
        used(&mut kept, 0);
        evict(&mut kept, 2, 4);
        let keys: Vec<u8> = kept.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, [2, 0]);
    }

    #[test]
    fn a_jpeg_is_bounded_by_the_size_its_decoder_reads() {
        // Second review of RV batch 4: the size checked was the one the
        // model read for the picture, and the decoder took up to 32767 a
        // side from the frame it reads.
        let mut bytes: Vec<u8> = (0..SMALL_JPEG.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&SMALL_JPEG[i..i + 2], 16).unwrap())
            .collect();
        assert!(jpeg_surface(bytes.clone().into()).is_some());
        let frame = bytes.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        // 30000 by 30000.
        bytes.splice(frame + 5..frame + 9, [0x75, 0x30, 0x75, 0x30]);
        assert!(jpeg_surface(bytes.into()).is_none());
    }

    #[test]
    fn the_shadow_of_huge_text_or_a_huge_blur_is_bounded() {
        // Review RV, batch 4: text at 8000 points made a mask of a
        // gigabyte, and a blur of 5000 points one too large for Cairo,
        // whose error left the rest of the page undrawn.
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 100, 100).unwrap();
        let cr = Context::new(&surface).unwrap();
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        let glyphs = [cairo::Glyph::new(36, 10.0, 50.0)];
        let black = Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        cr.set_font_size(12.0);
        assert!(paint_shadow(&cr, &glyphs, black, (1.0, 1.0), 5000.0).is_ok());
        cr.set_font_size(8000.0);
        let started = std::time::Instant::now();
        assert!(paint_shadow(&cr, &glyphs, black, (1.0, 1.0), 1.0).is_ok());
        assert!(started.elapsed().as_secs() < 5);
        assert_eq!(cr.status(), Ok(()));
    }

    #[test]
    fn lines_and_pictures_of_no_size_leave_cairo_able_to_draw() {
        // Review RV, batch 4: the dashes of a line 0 wide, all of 0, and a
        // picture 0 wide put the context in an error for the rest of the
        // page, and made PDF export fail.
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 10, 10).unwrap();
        let cr = Context::new(&surface).unwrap();
        cr.set_dash(&dash_pattern(&[0.0, 0.0]), 0.0);
        cr.set_dash(&dash_pattern(&[3.0, -1.0]), 0.0);
        cr.set_dash(&dash_pattern(&[f32::NAN]), 0.0);
        assert_eq!(cr.status(), Ok(()));
        assert_eq!(dash_pattern(&[3.0, 1.0]), [3.0, 1.0]);
        assert!(!drawn_width(0.0) && !drawn_width(f32::NAN) && drawn_width(0.1));
        let picture = cairo::ImageSurface::create(cairo::Format::ARgb32, 2, 2).unwrap();
        paint_surface(&cr, &picture, (0.0, 0.0, 0.0, 5.0)).unwrap();
        assert_eq!(cr.status(), Ok(()));
    }

    #[test]
    fn a_png_that_declares_a_huge_size_is_not_decoded() {
        // Review RV, batch 4: a PNG of a few megabytes may declare 30000
        // pixels a side, which Cairo allocated, gigabytes, and filled.
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 2, 2).unwrap();
        let mut png = Vec::new();
        surface.write_to_png(&mut png).unwrap();
        let small = desk_model::Picture::from_png(png.clone()).unwrap();
        assert!(picture_surface(&small).is_some());
        png[16..20].copy_from_slice(&30_000u32.to_be_bytes());
        png[20..24].copy_from_slice(&30_000u32.to_be_bytes());
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        let huge = desk_model::Picture::from_png(png.clone()).unwrap();
        assert_eq!(huge.width(), 30_000);
        assert!(picture_surface(&huge).is_none());
        // A tall screenshot is not refused for its height.
        let tall = cairo::ImageSurface::create(cairo::Format::ARgb32, 1200, 16_000).unwrap();
        let mut png = Vec::new();
        tall.write_to_png(&mut png).unwrap();
        let tall = desk_model::Picture::from_png(png).unwrap();
        assert!(decodable(&tall));
        assert!(picture_surface(&tall).is_some());
    }
}
