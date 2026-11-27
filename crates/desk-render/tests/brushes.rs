// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The artistic lines (B-044), painted: every brush puts ink along its
//! path, and only near it, around a shape filled inside.

use desk_model::{Brush, Rgba};
use desk_render::{DisplayList, Item, Paint, Painter, PathOp, PathStroke, cairo};

const SIDE: i32 = 120;

fn painted(brush: Option<Brush>) -> cairo::ImageSurface {
    let square = vec![
        PathOp::Move(20.0, 20.0),
        PathOp::Line(100.0, 20.0),
        PathOp::Line(100.0, 100.0),
        PathOp::Line(20.0, 100.0),
        PathOp::Close,
    ];
    let list = DisplayList {
        width: SIDE as f32,
        height: SIDE as f32,
        items: vec![Item::Path {
            path: square,
            fill: Some(Paint::Color(Rgba::rgb(0xff, 0, 0))),
            stroke: Some(PathStroke {
                width: 8.0,
                color: Rgba::BLACK,
                dashes: Vec::new(),
                brush,
            }),
        }],
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

/// The color at a pixel, as (red, green, blue).
fn at(surface: &mut cairo::ImageSurface, x: i32, y: i32) -> (u8, u8, u8) {
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let i = y as usize * stride + x as usize * 4;
    (data[i + 2], data[i + 1], data[i])
}

fn dark(c: (u8, u8, u8)) -> bool {
    c.0 < 96 && c.1 < 96 && c.2 < 96
}

#[test]
fn b_044_every_brush_inks_its_path_and_keeps_the_fill() {
    for brush in Brush::ALL {
        let mut surface = painted(Some(brush));
        if let Some(dir) = std::env::var_os("DESK_BRUSHES_PNG") {
            let path = std::path::Path::new(&dir).join(format!("{brush:?}.png"));
            surface
                .write_to_png(&mut std::fs::File::create(path).unwrap())
                .unwrap();
        }
        let mut ink = 0;
        let mut far = 0;
        for y in 0..SIDE {
            for x in 0..SIDE {
                if !dark(at(&mut surface, x, y)) {
                    continue;
                }
                // How far the pixel is from the outline of the square.
                let d = (x - 20)
                    .abs()
                    .min((x - 100).abs())
                    .min((y - 20).abs())
                    .min((y - 100).abs());
                let inside = (20..=100).contains(&x) && (20..=100).contains(&y);
                if d <= 8 && (inside || (12..=108).contains(&x) && (12..=108).contains(&y)) {
                    ink += 1;
                } else {
                    far += 1;
                }
            }
        }
        assert!(ink > 300, "{brush:?} inks its path: {ink}");
        assert_eq!(far, 0, "{brush:?} inks only near its path");
        // The middle of the square keeps its fill.
        assert_eq!(at(&mut surface, 60, 60), (0xff, 0, 0), "{brush:?}");
    }
}

#[test]
fn b_044_a_brush_draws_otherwise_than_a_solid_line() {
    let ink = |brush| {
        let mut surface = painted(brush);
        let mut count = 0;
        for y in 0..SIDE {
            for x in 0..SIDE {
                count += usize::from(dark(at(&mut surface, x, y)));
            }
        }
        count
    };
    let solid = ink(None);
    for brush in Brush::ALL {
        assert_ne!(ink(Some(brush)), solid, "{brush:?}");
    }
}

/// How many dark pixels a list paints on a white square.
fn ink_of(items: Vec<Item>) -> usize {
    let list = DisplayList {
        width: SIDE as f32,
        height: SIDE as f32,
        items,
    };
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, SIDE, SIDE).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        Painter::new().unwrap().paint(&cr, &list).unwrap();
    }
    let mut count = 0;
    for y in 0..SIDE {
        for x in 0..SIDE {
            count += usize::from(dark(at(&mut surface, x, y)));
        }
    }
    count
}

#[test]
fn b_044_the_nib_of_a_chisel_pen_stays_as_on_the_page_when_flipped() {
    // A line up to the right lies along the nib, and is thin; flipped
    // across, it goes down to the right on the page, across the nib.
    let line = || Item::Path {
        path: vec![PathOp::Move(20.0, 100.0), PathOp::Line(100.0, 20.0)],
        fill: None,
        stroke: Some(PathStroke {
            width: 10.0,
            color: Rgba::BLACK,
            dashes: Vec::new(),
            brush: Some(Brush::ChiselPen),
        }),
    };
    let plain = ink_of(vec![line()]);
    let flipped = ink_of(vec![Item::Group {
        transform: [-1.0, 0.0, 0.0, 1.0, SIDE as f32, 0.0],
        opacity: 1.0,
        items: vec![line()],
    }]);
    assert!(flipped > plain * 3, "{flipped} {plain}");
}
