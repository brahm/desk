// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Draws layouts side by side with Cairo, from the glyph outlines that
//! Skrifa reads, as desk-render will.

use cairo::{Context, Format, ImageSurface};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider};

use crate::common::{CIRCLE, COLUMN_WIDTH, FONT_SIZE, Laid};

const SCALE: f64 = 3.0;
const MARGIN: f64 = 24.0;

struct CairoPen<'a> {
    cr: &'a Context,
    x: f64,
    y: f64,
}

impl OutlinePen for CairoPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cr
            .move_to(self.x + f64::from(x), self.y - f64::from(y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cr
            .line_to(self.x + f64::from(x), self.y - f64::from(y));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (x0, y0) = self.cr.current_point().unwrap_or((self.x, self.y));
        let (qx, qy) = (self.x + f64::from(cx), self.y - f64::from(cy));
        let (x3, y3) = (self.x + f64::from(x), self.y - f64::from(y));
        self.cr.curve_to(
            x0 + 2.0 / 3.0 * (qx - x0),
            y0 + 2.0 / 3.0 * (qy - y0),
            x3 + 2.0 / 3.0 * (qx - x3),
            y3 + 2.0 / 3.0 * (qy - y3),
            x3,
            y3,
        );
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.cr.curve_to(
            self.x + f64::from(cx0),
            self.y - f64::from(cy0),
            self.x + f64::from(cx1),
            self.y - f64::from(cy1),
            self.x + f64::from(x),
            self.y - f64::from(y),
        );
    }
    fn close(&mut self) {
        self.cr.close_path();
    }
}

fn draw(cr: &Context, laid: &Laid, title: &str) {
    cr.set_source_rgb(0.85, 0.85, 0.85);
    cr.set_line_width(0.5);
    cr.rectangle(0.0, 0.0, f64::from(COLUMN_WIDTH), 215.0);
    cr.stroke().unwrap();
    cr.set_source_rgb(0.75, 0.82, 0.95);
    cr.arc(
        f64::from(CIRCLE.cx),
        f64::from(CIRCLE.cy),
        f64::from(CIRCLE.r),
        0.0,
        std::f64::consts::TAU,
    );
    cr.fill().unwrap();

    cr.set_source_rgb(0.6, 0.6, 0.6);
    cr.move_to(0.0, -8.0);
    cr.set_font_size(9.0);
    cr.show_text(title).unwrap();

    let font = skrifa::FontRef::from_index(&laid.font.data, laid.font.index).expect("font");
    let outlines = font.outline_glyphs();
    let settings = || DrawSettings::unhinted(Size::new(FONT_SIZE), LocationRef::default());
    cr.set_source_rgb(0.0, 0.0, 0.0);
    for line in &laid.lines {
        for g in &line.glyphs {
            if let Some(glyph) = outlines.get(GlyphId::new(g.id)) {
                let mut pen = CairoPen {
                    cr,
                    x: f64::from(g.x),
                    y: f64::from(g.y),
                };
                let _ = glyph.draw(settings(), &mut pen);
            }
        }
    }
    cr.fill().unwrap();
}

/// Writes a PNG with the layouts side by side.
pub fn write_png(path: &str, panels: &[(&str, &Laid)]) {
    let panel = f64::from(COLUMN_WIDTH) + MARGIN;
    let width = (MARGIN + panel * panels.len() as f64) * SCALE;
    let height = (215.0 + 2.0 * MARGIN) * SCALE;
    let surface = ImageSurface::create(Format::Rgb24, width as i32, height as i32).unwrap();
    let cr = Context::new(&surface).unwrap();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.paint().unwrap();
    cr.scale(SCALE, SCALE);
    for (i, (title, laid)) in panels.iter().enumerate() {
        cr.save().unwrap();
        cr.translate(MARGIN + panel * i as f64, MARGIN);
        draw(&cr, laid, title);
        cr.restore().unwrap();
    }
    let mut file = std::fs::File::create(path).unwrap();
    surface.write_to_png(&mut file).unwrap();
}
