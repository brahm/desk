// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The FBX reader, through `import`, against files made by the layout of
//! FBX 7 (D-033): see `tests/data/make_fbx.py`.

use desk_3d::{Error, Format, Import, View, import, read, render, render_at};

fn data(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/data/fbx/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 1e-4)
}

#[test]
fn w116_an_fbx_binary_or_text_becomes_a_glb_that_moves() {
    for name in ["box-7400.fbx", "box-7500.fbx", "box.fbx"] {
        let bytes = data(name);
        assert_eq!(Format::sniff(&bytes), None, "{name}");
        assert_eq!(Import::sniff(&bytes), Some(Import::Fbx), "{name}");
        let mut asked = Vec::new();
        let glb = import(&bytes, &mut |path| {
            asked.push(path.to_owned());
            Some(data(path))
        })
        .unwrap();
        // The binary files hold their picture; the text one names it.
        let text = name == "box.fbx";
        assert_eq!(
            asked,
            if text { vec!["missing.png"] } else { vec![] },
            "{name}"
        );
        let scene = read(&glb).unwrap();
        assert_eq!(scene.triangle_count(), 24, "{name}");
        assert_eq!(scene.textures.len(), 1, "{name}");
        let materials: Vec<_> = scene
            .meshes
            .iter()
            .map(|m| (m.material.color, m.material.texture))
            .collect();
        assert_eq!(
            materials,
            [
                ([0.8, 0.1, 0.1, 1.0], None),
                ([1.0, 1.0, 1.0, 1.0], Some(0)),
                ([0.8, 0.1, 0.1, 1.0], None),
                ([1.0, 1.0, 1.0, 1.0], Some(0)),
            ],
            "{name}"
        );
        // The child, 4 above its parent, turned a quarter; Z up in the
        // text, so that 4 above is 4 back.
        let (lo, hi) = scene.bounds().unwrap();
        if text {
            assert!(
                close(lo, [-1.0, -1.0, -5.0]) && close(hi, [1.0, 1.0, 1.0]),
                "{lo:?} {hi:?}"
            );
        } else {
            assert!(
                close(lo, [-1.0, -1.0, -1.0]) && close(hi, [1.0, 5.0, 1.0]),
                "{lo:?} {hi:?}"
            );
        }
        let animation = scene.animation.as_ref().unwrap();
        assert!((animation.duration() - 1.0).abs() < 1e-6, "{name}");
        let view = View::default();
        assert_eq!(
            render(&scene, view, 40, 40),
            render_at(&scene, view, 0.0, 40, 40)
        );
        assert_ne!(
            render_at(&scene, view, 0.0, 40, 40),
            render_at(&scene, view, 1.0, 40, 40)
        );
    }
}

#[test]
fn w116_what_is_not_an_fbx_is_refused() {
    let bytes = data("box-7500.fbx");
    for cut in [30, 100, bytes.len() / 2, bytes.len() - 30] {
        assert!(import(&bytes[..cut], &mut |_| None).is_err(), "{cut}");
    }
    let text = data("box.fbx");
    let half = &text[..text.len() / 2];
    assert!(import(half, &mut |_| None).is_err());
    // An FBX of an older version is not one this reads.
    let mut old = bytes.clone();
    old[23..27].copy_from_slice(&6100u32.to_le_bytes());
    assert_eq!(import(&old, &mut |_| None), Err(Error::Unknown));
    assert_eq!(
        import(
            b"; FBX 6.1.0 project file\nFBXHeaderExtension: {\n FBXVersion: 6100\n}\n",
            &mut |_| None
        ),
        Err(Error::Unknown)
    );
}
