// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The USDZ reader against packages that OpenUSD wrote (W116): see
//! `tests/data/make.py`.

use desk_3d::{Error, Format, Scene, View, read, render};

fn scene(name: &str) -> Scene {
    let path = format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(Format::sniff(&bytes), Some(Format::Usdz), "{name}");
    read(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 1e-4)
}

#[test]
fn w116_a_cube_moved_turned_and_scaled() {
    for name in ["cube-usdc.usdz", "cube-usda.usdz"] {
        let scene = scene(name);
        assert_eq!(scene.triangle_count(), 12, "{name}");
        let (lo, hi) = scene.bounds().unwrap();
        assert!(
            close(lo, [8.0, -2.0, -2.0]) && close(hi, [12.0, 2.0, 2.0]),
            "{name}: {lo:?} {hi:?}"
        );
        let mesh = &scene.meshes[0];
        // The material, not the display color, colors it.
        assert_eq!(mesh.material.color, [0.8, 0.1, 0.1, 1.0], "{name}");
        assert!(mesh.colors.is_empty());
        assert_eq!(mesh.normals.len(), mesh.positions.len());
    }
}

#[test]
fn w116_subsets_of_faces_and_a_reference_to_another_layer() {
    for name in ["two-materials-usdc.usdz", "two-materials-usda.usdz"] {
        let scene = scene(name);
        assert_eq!(scene.triangle_count(), 24, "{name}");
        let colors: Vec<_> = scene
            .meshes
            .iter()
            .map(|m| (m.material.color, m.triangles.len()))
            .collect();
        assert!(
            colors.contains(&([0.0, 0.0, 1.0, 1.0], 2)),
            "{name}: {colors:?}"
        );
        assert!(
            colors.contains(&([1.0, 1.0, 0.0, 1.0], 10)),
            "{name}: {colors:?}"
        );
        // The top face, blue, is at y = 1.
        let blue = scene
            .meshes
            .iter()
            .find(|m| m.material.color[2] == 1.0)
            .unwrap();
        assert!(blue.positions.iter().all(|p| p[1] == 1.0), "{name}");
        // The cube of the other layer, moved 5 up.
        let (lo, hi) = scene.bounds().unwrap();
        assert!(
            close(lo, [-1.0, -1.0, -1.0]) && close(hi, [1.0, 6.0, 1.0]),
            "{name}: {lo:?} {hi:?}"
        );
    }
}

#[test]
fn w116_a_texture_on_a_z_up_quad() {
    let scene = scene("textured.usdz");
    assert_eq!(scene.textures.len(), 1);
    assert_eq!((scene.textures[0].width, scene.textures[0].height), (2, 2));
    let mesh = &scene.meshes[0];
    assert_eq!(mesh.material.texture, Some(0));
    assert_eq!(mesh.uvs.len(), mesh.positions.len());
    // Z up: the quad in X and Y stands in X and -Z.
    let (lo, hi) = scene.bounds().unwrap();
    assert!(
        close(lo, [0.0, 0.0, -1.0]) && close(hi, [1.0, 0.0, 0.0]),
        "{lo:?} {hi:?}"
    );
    // Seen from above, the top left is red, as is the top left of the
    // picture.
    let image = render(
        &scene,
        View {
            yaw: 0.0,
            pitch: 90.0,
        },
        64,
        64,
    )
    .unwrap();
    let at = (20 * 64 + 20) * 4;
    let p = &image[at..at + 4];
    assert!(p[0] > 60 && p[1] < 10 && p[2] < 10 && p[3] == 255, "{p:?}");
}

#[test]
fn w116_what_is_not_a_model_is_refused() {
    assert_eq!(read(b"not a model"), Err(Error::Unknown));
    let bytes = std::fs::read(format!(
        "{}/tests/data/cube-usdc.usdz",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    for cut in [100, bytes.len() / 2, bytes.len() - 30] {
        assert!(read(&bytes[..cut]).is_err());
    }
}

#[test]
fn w116_compressed_arrays_quaternions_and_samples_in_time() {
    let scene = scene("many-usdc.usdz");
    assert_eq!(scene.triangle_count(), 800 + 256);
    let grid = scene
        .meshes
        .iter()
        .find(|m| m.triangles.len() == 800)
        .unwrap();
    // A quarter turn about Z, then the first sample of the move, 100 in Z.
    let lo = grid
        .positions
        .iter()
        .fold([f32::INFINITY; 3], |a, p| [0, 1, 2].map(|k| a[k].min(p[k])));
    let hi = grid.positions.iter().fold([f32::NEG_INFINITY; 3], |a, p| {
        [0, 1, 2].map(|k| a[k].max(p[k]))
    });
    assert!(
        close(lo, [-20.0, 0.0, 100.0]) && close(hi, [0.0, 20.0, 100.0]),
        "{lo:?} {hi:?}"
    );
    assert!(grid.normals.iter().all(|n| close(*n, [0.0, 0.0, 1.0])));
    // The move plays over its ten time codes, at 24 a second, and the
    // grid ends 100 further in Z.
    let animation = scene.animation.as_ref().unwrap();
    assert!((animation.duration() - 10.0 / 24.0).abs() < 1e-6);
    let view = desk_3d::View {
        yaw: 45.0,
        pitch: 0.0,
    };
    let start = desk_3d::render_at(&scene, view, 0.0, 48, 48).unwrap();
    let end = desk_3d::render_at(&scene, view, animation.duration(), 48, 48).unwrap();
    assert_eq!(desk_3d::render(&scene, view, 48, 48).unwrap(), start);
    assert!(start != end && start.iter().any(|v| *v > 0));
    let sphere = scene
        .meshes
        .iter()
        .find(|m| m.triangles.len() == 256)
        .unwrap();
    assert!(
        sphere
            .positions
            .iter()
            .all(|p| ((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 1.0).abs() < 1e-5)
    );
    assert!(sphere.positions.iter().any(|p| close(*p, [0.0, -1.0, 0.0])));
}
