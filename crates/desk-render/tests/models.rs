// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! 3D objects (W116), painted: the model in its frame, turned as it says,
//! at the pixels of the device; a file that cannot be read shows as a
//! crossed frame.

use desk_model::Model3d;
use desk_render::{DisplayList, Item, Painter, cairo};

const SIDE: i32 = 100;

fn model(name: &str) -> Model3d {
    let path = format!(
        "{}/../desk-3d/tests/data/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    Model3d::from_bytes(std::fs::read(path).unwrap()).unwrap()
}

fn painted(model: &Model3d, scale: f64) -> cairo::ImageSurface {
    let side = SIDE as f32;
    let list = DisplayList {
        width: side,
        height: side,
        items: vec![Item::Model {
            model: model.clone(),
            rect: (0.0, 0.0, side, side),
        }],
    };
    let pixels = (f64::from(SIDE) * scale) as i32;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, pixels, pixels).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().unwrap();
        cr.scale(scale, scale);
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

#[test]
fn w116_a_model_draws_in_its_frame_turned_as_it_says() {
    let cube = model("cube-usdc.usdz");
    let mut front = painted(&cube, 1.0);
    // The red cube in the middle, the white page at the corners.
    let (r, g, b) = at(&mut front, 50, 50);
    assert!(r > 80 && g < r / 2 && b < r / 2, "{:?}", (r, g, b));
    assert_eq!(at(&mut front, 1, 1), (0xff, 0xff, 0xff));
    // Turned an eighth, the cube is wider: a corner faces the viewer.
    let mut turned = painted(&cube.turned(4500, 0), 1.0);
    let wide = |s: &mut cairo::ImageSurface| {
        (0..SIDE)
            .filter(|&x| at(s, x, 50) != (0xff, 0xff, 0xff))
            .count()
    };
    assert!(wide(&mut turned) > wide(&mut front) + 5);
    // At twice the pixels, it is drawn at twice the pixels.
    let mut sharp = painted(&cube, 2.0);
    assert_eq!(at(&mut sharp, 3, 3), (0xff, 0xff, 0xff));
    let (r, _, _) = at(&mut sharp, 100, 100);
    assert!(r > 80);
}

#[test]
fn w116_a_model_that_cannot_be_read_shows_as_a_crossed_frame() {
    // A package whose first file is named as a layer, but holds nothing.
    let mut zip = b"PK\x03\x04".to_vec();
    zip.extend([0; 22]);
    zip.extend(10u16.to_le_bytes());
    zip.extend(0u16.to_le_bytes());
    zip.extend(b"scene.usdc");
    let broken = Model3d::from_bytes(zip).unwrap();
    let mut surface = painted(&broken, 1.0);
    // Gray along the diagonal, and lightly in between.
    let (r, g, b) = at(&mut surface, 50, 50);
    assert!(r < 0xd0 && r == g && g == b, "{:?}", (r, g, b));
    let (r, ..) = at(&mut surface, 50, 20);
    assert!(r < 0xff && r > 0xd0);
}

#[test]
fn w116_a_model_that_moves_draws_at_the_moment_it_plays() {
    let model = model("animated.glb");
    let rest = painted(&model, 1.0);
    let mut start = painted(&model.at(Some(0)), 1.0);
    let mut end = painted(&model.at(Some(2000)), 1.0);
    // At the start of its animation it is as it rests; at the end the red
    // box has gone up.
    let mut rest = rest;
    let red = |s: &mut cairo::ImageSurface| {
        (0..SIDE)
            .filter(|&y| {
                let (r, g, b) = at(s, 87, y);
                r > 100 && g < 60 && b < 60
            })
            .max()
    };
    assert_eq!(red(&mut rest), red(&mut start));
    assert!(
        red(&mut end) < red(&mut start),
        "{:?} {:?}",
        red(&mut end),
        red(&mut start)
    );
}
