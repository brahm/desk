// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The OBJ reader, through `import`, against files made by the
//! specification of Wavefront OBJ and MTL (D-033): see
//! `tests/data/make_obj.py`.

use desk_3d::{Error, Format, Import, View, import, read, render};

fn data(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn beside(path: &str) -> Option<Vec<u8>> {
    std::fs::read(format!(
        "{}/tests/data/obj/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .ok()
}

#[test]
fn w116_an_obj_with_its_materials_becomes_a_glb() {
    let obj = data("obj/box.obj");
    assert_eq!(Format::sniff(&obj), None);
    assert_eq!(Import::sniff(&obj), Some(Import::Obj));
    // A document does not keep an OBJ.
    assert_eq!(read(&obj), Err(Error::Unknown));
    let mut asked = Vec::new();
    let glb = import(&obj, &mut |path| {
        asked.push(path.to_owned());
        beside(path)
    })
    .unwrap();
    assert_eq!(asked, ["box materials.mtl", "pictures/wood.png"]);
    assert_eq!(Format::sniff(&glb), Some(Format::Glb));
    let scene = read(&glb).unwrap();
    assert_eq!(scene.triangle_count(), 12);
    assert_eq!(scene.bounds(), Some(([-1.0; 3], [1.0; 3])));
    let materials: Vec<_> = scene
        .meshes
        .iter()
        .map(|m| (m.material.color, m.material.texture, m.material.translucent))
        .collect();
    assert_eq!(
        materials,
        [
            ([0.8, 0.1, 0.1, 1.0], None, false),
            ([1.0, 1.0, 1.0, 1.0], Some(0), false),
            ([0.0, 0.0, 1.0, 0.5], None, true),
        ]
    );
    assert_eq!((scene.textures[0].width, scene.textures[0].height), (2, 2));
    // Four sides of four corners each, a corner by its normal; the top
    // and the front, with the coordinates of the texture.
    let corners: Vec<_> = scene
        .meshes
        .iter()
        .map(|m| (m.positions.len(), m.normals.len(), m.uvs.len()))
        .collect();
    assert_eq!(corners, [(16, 16, 0), (4, 4, 4), (4, 4, 0)]);
    assert_eq!(scene.meshes[1].normals[0], [0.0, 1.0, 0.0]);
    // Seen from above, the top shows its picture.
    let image = render(
        &scene,
        View {
            pitch: 90.0,
            ..View::default()
        },
        64,
        64,
    )
    .unwrap();
    let mut seen = std::collections::HashSet::new();
    for p in image.as_chunks::<4>().0 {
        if p[3] == 255 {
            seen.insert([p[0] > 100, p[1] > 100, p[2] > 100]);
        }
    }
    for color in [
        [true, false, false],
        [false, true, false],
        [false, false, true],
    ] {
        assert!(seen.contains(&color), "{color:?} {seen:?}");
    }
}

#[test]
fn w116_an_obj_without_its_files_and_with_colors() {
    // With no library, each material that faces use is the default.
    let glb = import(&data("obj/box.obj"), &mut |_| None).unwrap();
    let scene = read(&glb).unwrap();
    assert_eq!(scene.meshes.len(), 3);
    assert!(scene.textures.is_empty());
    assert!(
        scene
            .meshes
            .iter()
            .all(|m| m.material.color == desk_3d::Material::default().color)
    );
    let glb = import(&data("obj/colors.obj"), &mut |_| None).unwrap();
    let scene = read(&glb).unwrap();
    assert_eq!(
        scene.meshes[0].colors,
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    );
    assert!(scene.meshes[0].normals.is_empty());
}

#[test]
fn w116_what_import_keeps_and_refuses() {
    // A GLB and a USDZ are kept as they are.
    for name in ["animated.glb", "cube-usdc.usdz"] {
        let bytes = data(name);
        assert!(import(&bytes, &mut |_| None).unwrap() == bytes, "{name}");
    }
    assert_eq!(import(b"hello", &mut |_| None), Err(Error::Unknown));
    assert_eq!(
        import(b"v 0 0 0\nv 1 0 0\n", &mut |_| None),
        Err(Error::Empty)
    );
    assert!(matches!(
        import(b"v 0 x 0\nf 1 1 1\n", &mut |_| None),
        Err(Error::Malformed(_))
    ));
    // A face with a corner that is not there is left out.
    assert_eq!(
        import(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 4\n", &mut |_| None),
        Err(Error::Empty)
    );
    // The files it names never leave its folder.
    let obj = b"mtllib ../up.mtl\nmtllib /etc/passwd\nmtllib .hidden.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
    let mut asked = Vec::new();
    import(obj, &mut |path| {
        asked.push(path.to_owned());
        None
    })
    .unwrap();
    assert!(asked.is_empty(), "{asked:?}");
}
