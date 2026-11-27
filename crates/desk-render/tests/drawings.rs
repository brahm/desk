// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The drawings of Pages 15.4 (W117), painted: each tool puts its colour
//! along its line, and the fill inside the area it fills.

use std::sync::Arc;

use desk_model::{Drawing, DrawingStroke, DrawingTool, Length, Rgba};
use desk_render::{DisplayList, Painter, cairo, drawing_items};

const SIDE: i32 = 120;

fn stroke(tool: DrawingTool, color: Rgba, points: &[(i32, i32)]) -> DrawingStroke {
    DrawingStroke {
        tool,
        color,
        width: Length::from_points(6),
        points: Arc::from(points),
    }
}

fn painted(drawing: &Drawing) -> cairo::ImageSurface {
    let side = SIDE as f32;
    let list = DisplayList {
        width: side,
        height: side,
        items: drawing_items(drawing, (0.0, 0.0, side, side)),
    };
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, SIDE, SIDE).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, &list).unwrap();
    }
    surface
}

/// The colour at a pixel, as (red, green, blue).
fn at(surface: &mut cairo::ImageSurface, x: i32, y: i32) -> (u8, u8, u8) {
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let i = y as usize * stride + x as usize * 4;
    (data[i + 2], data[i + 1], data[i])
}

/// How many pixels of a row are not white.
fn inked(surface: &mut cairo::ImageSurface, y: i32) -> usize {
    (0..SIDE)
        .filter(|&x| at(surface, x, y) != (0xff, 0xff, 0xff))
        .count()
}

#[test]
fn w117_each_tool_draws_along_its_line() {
    // A line across the middle, a tenth of the frame in from each side.
    let across = [(1_000, 5_000), (5_000, 5_000), (9_000, 5_000)];
    for tool in [DrawingTool::Pen, DrawingTool::Pencil, DrawingTool::Crayon] {
        let drawing = Drawing {
            strokes: Arc::from([stroke(tool, Rgba::rgb(0, 0, 0xff), &across)]),
            animation: None,
        };
        let mut surface = painted(&drawing);
        assert!(inked(&mut surface, 60) > 60, "{tool:?} draws its line");
        assert_eq!(inked(&mut surface, 20), 0, "{tool:?} draws only near it");
        assert_eq!(inked(&mut surface, 100), 0, "{tool:?} draws only near it");
    }
    // The pen goes round its ends, past the last point by half its width.
    let drawing = Drawing {
        strokes: Arc::from([stroke(DrawingTool::Pen, Rgba::BLACK, &across)]),
        animation: None,
    };
    let mut surface = painted(&drawing);
    assert_eq!(at(&mut surface, 109, 60), (0, 0, 0));
    assert_eq!(at(&mut surface, 116, 60), (0xff, 0xff, 0xff));
}

#[test]
fn w117_a_fill_fills_its_area_behind_its_line() {
    let square = [
        (2_000, 2_000),
        (8_000, 2_000),
        (8_000, 8_000),
        (2_000, 8_000),
    ];
    let drawing = Drawing {
        strokes: Arc::from([
            stroke(DrawingTool::Fill, Rgba::rgb(0xff, 0, 0), &square),
            stroke(DrawingTool::Pen, Rgba::BLACK, &square),
        ]),
        animation: None,
    };
    let mut surface = painted(&drawing);
    assert_eq!(at(&mut surface, 60, 60), (0xff, 0, 0));
    assert_eq!(at(&mut surface, 10, 10), (0xff, 0xff, 0xff));
}
