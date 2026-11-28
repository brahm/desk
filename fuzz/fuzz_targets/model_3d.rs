// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The 3D models that documents carry (W116, D-033): any bytes, as a
//! USDZ package or as one layer of USD, binary or text, must read as a
//! scene or as an error, without a panic, within the limits, and a scene
//! must draw.

#![no_main]

use desk_3d::{View, limits, read, read_layer, render};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let scene = if data.starts_with(b"PK") {
        read(data)
    } else {
        read_layer(data)
    };
    let Ok(scene) = scene else {
        return;
    };
    assert!(scene.triangle_count() <= limits::TRIANGLES);
    assert!(scene.textures.len() <= limits::TEXTURES);
    let image = render(&scene, View { yaw: 30.0, pitch: 20.0 }, 32, 24).unwrap();
    assert_eq!(image.len(), 32 * 24 * 4);
});
