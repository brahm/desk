// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The PRC reader, through `import`, against files that Asymptote writes
//! as the 3D of PDF (D-033): see the `.asy` files of `tests/data/prc`.

use desk_3d::{Error, Format, Import, Scene, import, read};

fn data(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/data/prc/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn scene(name: &str) -> Scene {
    let bytes = data(name);
    assert_eq!(Format::sniff(&bytes), None, "{name}");
    assert_eq!(Import::sniff(&bytes), Some(Import::Prc), "{name}");
    read(&import(&bytes, &mut |_| panic!("a PRC names no file")).unwrap()).unwrap()
}

/// The least and the greatest corner of a mesh.
fn bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in positions {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3 * a.abs().max(b.abs()).max(1.0)
}

#[test]
fn w116_a_prc_of_a_pdf_becomes_a_glb_in_the_color_of_its_style() {
    let scene = scene("tess.prc");
    assert_eq!(scene.triangle_count(), 12);
    assert_eq!(scene.meshes.len(), 1);
    let mesh = &scene.meshes[0];
    assert_eq!(mesh.material.color, [1.0, 0.0, 0.0, 1.0]);
    assert!(!mesh.material.translucent);
    // A cube, as long on each side.
    let (lo, hi) = scene.bounds().unwrap();
    let side = hi[0] - lo[0];
    assert!(side > 0.0);
    assert!(close(hi[1] - lo[1], side) && close(hi[2] - lo[2], side));
}

#[test]
fn w116_the_product_occurrences_of_a_prc_place_their_parts() {
    let scene = scene("shapes.prc");
    assert_eq!(scene.triangle_count(), 36);
    let mut meshes: Vec<_> = scene
        .meshes
        .iter()
        .map(|m| {
            (
                m.material.color,
                m.material.translucent,
                bounds(&m.positions),
            )
        })
        .collect();
    meshes.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])));
    let [blue, green, red] = meshes.as_slice() else {
        panic!("{meshes:?}");
    };
    assert_eq!((red.0, red.1), ([1.0, 0.0, 0.0, 1.0], false));
    assert_eq!((green.0, green.1), ([0.0, 1.0, 0.0, 1.0], false));
    assert_eq!((blue.0, blue.1), ([0.0, 0.0, 1.0, 128.0 / 255.0], true));
    // Blue is three sides along X from red, and green three sides along
    // Y of the file, which is Z up: three sides back.
    let (lo, hi) = red.2;
    let side = hi[0] - lo[0];
    let moved = |m: &([f32; 3], [f32; 3]), by: [f32; 3]| {
        (0..3).all(|k| close(m.0[k] - lo[k], by[k] * side) && close(m.1[k] - hi[k], by[k] * side))
    };
    assert!(moved(&blue.2, [3.0, 0.0, 0.0]), "{blue:?} {red:?}");
    assert!(moved(&green.2, [0.0, 0.0, -3.0]), "{green:?} {red:?}");
}

#[test]
fn w116_a_prc_keeps_the_normal_and_the_color_of_each_corner() {
    let scene = scene("triangles.prc");
    assert_eq!(scene.triangle_count(), 2);
    let mesh = &scene.meshes[0];
    assert_eq!(mesh.normals.len(), mesh.positions.len());
    // The normal of Z up in the file is Y up, turned a little as
    // Asymptote turns the scene to its view.
    for n in &mesh.normals {
        assert!(n[1] > 0.9, "{n:?}");
    }
    let corners: Vec<[f32; 3]> = mesh
        .triangles
        .iter()
        .flatten()
        .map(|&i| mesh.colors[i as usize])
        .collect();
    let (red, green, blue, white) = (
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
    );
    assert_eq!(corners, [red, green, blue, red, blue, white]);
}

#[test]
fn w116_a_prc_of_exact_surfaces_only_has_nothing_to_show() {
    let bytes = data("brep.prc");
    assert_eq!(Import::sniff(&bytes), Some(Import::Prc));
    assert_eq!(import(&bytes, &mut |_| None), Err(Error::Empty));
}

#[test]
fn w116_a_prc_cut_short_or_spoiled_is_an_error_not_a_panic() {
    for name in ["tess.prc", "shapes.prc", "triangles.prc"] {
        let bytes = data(name);
        for end in 0..bytes.len() {
            let _ = import(&bytes[..end], &mut |_| None);
        }
        for at in 0..bytes.len() {
            for flip in [0x01, 0x80, 0xFF] {
                let mut spoiled = bytes.clone();
                spoiled[at] ^= flip;
                if let Ok(glb) = import(&spoiled, &mut |_| None) {
                    read(&glb).unwrap();
                }
            }
        }
    }
}

#[test]
fn w116_a_prc_that_claims_more_than_it_holds_is_refused() {
    let mut bytes = data("tess.prc");
    // The count of file structures, after the two versions and the two
    // identifiers.
    bytes[43..47].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        import(&bytes, &mut |_| None),
        Err(Error::Malformed(_))
    ));
}
