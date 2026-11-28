// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The glTF reader against files made by the specification of glTF 2.0
//! (W116): see `tests/data/make_gltf.py`.

use desk_3d::{Error, Format, View, pack_gltf, read, render, render_at};

fn data(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 1e-4)
}

fn pixel(image: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let at = (y * width + x) * 4;
    image[at..at + 4].try_into().unwrap()
}

#[test]
fn w116_nodes_materials_and_an_animation_of_a_glb() {
    let bytes = data("animated.glb");
    assert_eq!(Format::sniff(&bytes), Some(Format::Glb));
    let scene = read(&bytes).unwrap();
    assert_eq!(scene.meshes.len(), 3);
    assert_eq!(scene.triangle_count(), 36);
    let (lo, hi) = scene.bounds().unwrap();
    assert!(
        close(lo, [-11.0, -2.0, -2.0]) && close(hi, [12.0, 10.0, 2.0]),
        "{lo:?} {hi:?}"
    );
    let colors: Vec<_> = scene.meshes.iter().map(|m| m.material.color).collect();
    assert_eq!(
        colors,
        [
            [0.8, 0.1, 0.1, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0, 0.5]
        ]
    );
    assert!(scene.meshes.iter().all(|m| m.normals.len() == 24));
    let animation = scene.animation.as_ref().unwrap();
    assert_eq!(animation.duration(), 2.0);
    // At rest and at the start, the red box is at the middle right; at
    // the end it has gone up out of where it rested.
    let view = View::default();
    let (w, h) = (92, 92);
    let start = render_at(&scene, view, 0.0, w, h).unwrap();
    let end = render_at(&scene, view, 2.0, w, h).unwrap();
    assert_eq!(render(&scene, view, w, h).unwrap(), start);
    assert_ne!(start, end);
    let red = |image: &[u8]| {
        (0..h as usize)
            .filter(|y| {
                let p = pixel(image, w as usize, 80, *y);
                p[0] > 100 && p[2] < 30
            })
            .collect::<Vec<_>>()
    };
    let (before, after) = (red(&start), red(&end));
    assert!(!before.is_empty() && !after.is_empty());
    assert!(
        after.iter().max() < before.iter().max(),
        "{before:?} {after:?}"
    );
}

#[test]
fn w116_a_text_gltf_with_a_strip_a_texture_and_colors() {
    let bytes = data("textured.gltf");
    assert_eq!(Format::sniff(&bytes), Some(Format::Gltf));
    let scene = read(&bytes).unwrap();
    assert!(scene.animation.is_none());
    assert_eq!(scene.triangle_count(), 2);
    assert_eq!((scene.textures[0].width, scene.textures[0].height), (2, 2));
    let mesh = &scene.meshes[0];
    assert_eq!(mesh.material.texture, Some(0));
    assert_eq!(mesh.uvs.len(), 4);
    assert!(mesh.colors.iter().all(|c| *c == [1.0, 1.0, 1.0]));
    // Seen from the front, the top left is red, as is the top left of the
    // picture, and the bottom right white.
    let image = render(&scene, View::default(), 64, 64).unwrap();
    let p = pixel(&image, 64, 20, 20);
    assert!(p[0] > 60 && p[1] < 10 && p[2] < 10 && p[3] == 255, "{p:?}");
    let p = pixel(&image, 64, 44, 44);
    assert!(p[0] > 60 && p[1] > 60 && p[2] > 60, "{p:?}");
}

#[test]
fn w116_a_gltf_and_the_files_beside_it_make_one_glb() {
    let gltf = data("external/quad.gltf");
    // Alone, its buffer is missing.
    assert!(read(&gltf).is_err());
    let mut asked = Vec::new();
    let glb = pack_gltf(&gltf, &mut |path| {
        asked.push(path.to_owned());
        std::fs::read(format!(
            "{}/tests/data/external/{path}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .ok()
    })
    .unwrap();
    assert_eq!(asked, ["quad data.bin", "pictures/quad.png"]);
    assert_eq!(Format::sniff(&glb), Some(Format::Glb));
    assert_eq!(read(&glb).unwrap(), read(&data("textured.gltf")).unwrap());
    // A GLB packs to itself, and a file it names that is missing fails.
    assert_eq!(read(&pack_gltf(&glb, &mut |_| None).unwrap()), read(&glb));
    assert!(matches!(
        pack_gltf(&gltf, &mut |_| None),
        Err(Error::Malformed(_))
    ));
}

#[test]
fn w116_what_is_not_a_gltf_is_refused() {
    assert_eq!(Format::sniff(b"{\"a\": 1}"), None);
    assert_eq!(
        read(b"{\"asset\": {\"version\": \"1.0\"}}"),
        Err(Error::Unknown)
    );
    assert_eq!(
        read(b"{\"asset\": {\"version\": \"2.0\"}}"),
        Err(Error::Empty)
    );
    let bytes = data("animated.glb");
    for cut in [10, 30, bytes.len() / 2, bytes.len() - 4] {
        assert!(read(&bytes[..cut]).is_err(), "{cut}");
    }
}

#[test]
fn w116_a_model_that_grows_from_nothing() {
    let scene = read(&data("grow.glb")).unwrap();
    let material = &scene.meshes[0].material;
    assert_eq!(material.color[3], 1.0);
    assert!(!material.translucent);
    assert_eq!(material.cutoff, 0.5);
    let covered = |image: &[u8]| image.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
    let view = View::default();
    // At rest, and at the start, it is at a scale of 0: nothing shows.
    assert_eq!(covered(&render(&scene, view, 40, 40).unwrap()), 0);
    assert_eq!(covered(&render_at(&scene, view, 0.0, 40, 40).unwrap()), 0);
    let half = covered(&render_at(&scene, view, 0.5, 40, 40).unwrap());
    let whole = covered(&render_at(&scene, view, 1.0, 40, 40).unwrap());
    assert!(0 < half && half < whole, "{half} {whole}");
}

#[test]
fn w116_blended_materials_let_what_is_behind_show() {
    let scene = read(&data("animated.glb")).unwrap();
    let translucent: Vec<bool> = scene
        .meshes
        .iter()
        .map(|m| m.material.translucent)
        .collect();
    assert_eq!(translucent, [false, false, true]);
}

#[test]
fn w116_packing_takes_only_the_files_that_are_used() {
    // The quad of the folder beside, with a buffer and pictures that
    // nothing uses, which are not asked for.
    let mut json: serde_json::Value = serde_json::from_slice(&data("external/quad.gltf")).unwrap();
    json["buffers"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"byteLength": 4, "uri": "other.bin"}));
    json["images"].as_array_mut().unwrap().extend([
        serde_json::json!({"uri": "secret.txt"}),
        serde_json::json!({"uri": "notes/x.pdf"}),
    ]);
    let gltf = serde_json::to_vec(&json).unwrap();
    let mut asked = Vec::new();
    let glb = pack_gltf(&gltf, &mut |path| {
        asked.push(path.to_owned());
        std::fs::read(format!(
            "{}/tests/data/external/{path}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .ok()
    })
    .unwrap();
    assert_eq!(asked, ["quad data.bin", "pictures/quad.png"]);
    assert_eq!(read(&glb).unwrap(), read(&data("textured.gltf")).unwrap());
    // A picture that a texture uses must be one.
    json["images"][0] = serde_json::json!({"uri": "quad data.bin"});
    let gltf = serde_json::to_vec(&json).unwrap();
    let packed = pack_gltf(&gltf, &mut |path| {
        std::fs::read(format!(
            "{}/tests/data/external/{path}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .ok()
    });
    assert!(matches!(packed, Err(Error::Malformed(_))), "{packed:?}");
    // A GLB reads no file beside it.
    let mut asked = 0;
    let _ = pack_gltf(&glb, &mut |_| {
        asked += 1;
        None
    });
    assert_eq!(asked, 0);
}
