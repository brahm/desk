// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The 3D models that documents carry (W116, D-033): any bytes, as a
//! USDZ package, a glTF or a GLB, or as one layer of USD, binary or text,
//! must read as a scene or as an error, without a panic, within the
//! limits, and a scene must draw, at rest and as it moves. A glTF packs
//! into a GLB that reads as it does. An OBJ, with the library and the
//! picture of the tests for any file it names, imports as a GLB that
//! reads.

#![no_main]

use desk_3d::{
    Format, Import, View, import, limits, pack_gltf, read, read_layer, render, render_at,
};
use libfuzzer_sys::fuzz_target;

const MTL: &[u8] = include_bytes!("../../crates/desk-3d/tests/data/obj/box materials.mtl");
const PICTURE: &[u8] = include_bytes!("../../crates/desk-3d/tests/data/obj/pictures/wood.png");

fuzz_target!(|data: &[u8]| {
    let scene = match (Format::sniff(data), Import::sniff(data)) {
        (Some(_), _) => read(data),
        (None, Some(_)) => match import(data, &mut |name| {
            Some(if name.ends_with(".mtl") {
                MTL.to_vec()
            } else {
                PICTURE.to_vec()
            })
        }) {
            Ok(glb) => {
                let scene = read(&glb);
                assert!(scene.is_ok(), "{scene:?}");
                scene
            }
            Err(e) => Err(e),
        },
        (None, None) => read_layer(data),
    };
    if matches!(Format::sniff(data), Some(Format::Gltf | Format::Glb))
        && let (Ok(glb), Ok(scene)) = (pack_gltf(data, &mut |_| None), &scene)
    {
        assert_eq!(read(&glb).as_ref(), Ok(scene));
    }
    let Ok(scene) = scene else {
        return;
    };
    assert!(scene.triangle_count() <= limits::TRIANGLES);
    assert!(scene.textures.len() <= limits::TEXTURES);
    let image = render(
        &scene,
        View {
            yaw: 30.0,
            pitch: 20.0,
        },
        32,
        24,
    )
    .unwrap();
    assert_eq!(image.len(), 32 * 24 * 4);
    if let Some(animation) = &scene.animation {
        let view = View::default();
        let image = render_at(&scene, view, animation.duration() / 3.0, 16, 16).unwrap();
        assert_eq!(image.len(), 16 * 16 * 4);
    }
});
