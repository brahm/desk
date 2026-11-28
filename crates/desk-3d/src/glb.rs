// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! A scene written as a GLB, which is how a document keeps a model read
//! from a format that it does not keep, as OBJ (D-033).
//!
//! Each mesh is a node of its own, with its normals, texture coordinates
//! and colors, and a material of its color and texture. A mesh that
//! does not move has its corners in the space of the scene; one that
//! moves hangs from the nodes of its joint and those above it, each
//! sampled through the animation as a translation, a rotation and a
//! scale, which loses only a shear of a node in its parent. Read again,
//! the GLB draws as the scene does.

use serde_json::{Map, Value as Json, json};

use crate::matrix::{Matrix, at, decompose, normal, point};
use crate::{Error, Result, Scene, limits, texture};

const FLOAT: u32 = 5126;
const UINT: u32 = 5125;

/// The buffer, views and accessors of the GLB being written.
#[derive(Default)]
struct Writer {
    bin: Vec<u8>,
    views: Vec<Json>,
    accessors: Vec<Json>,
}

impl Writer {
    fn view(&mut self, data: &[u8]) -> Result<usize> {
        let at = self.bin.len().next_multiple_of(4);
        if (at + data.len()) as u64 > limits::FILE_BYTES {
            return Err(Error::TooLarge);
        }
        self.bin.resize(at, 0);
        self.bin.extend_from_slice(data);
        self.views
            .push(json!({"buffer": 0, "byteOffset": at, "byteLength": data.len()}));
        Ok(self.views.len() - 1)
    }

    fn floats<const N: usize>(&mut self, values: &[[f32; N]], bounds: bool) -> Result<usize> {
        let data: Vec<u8> = values
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let view = self.view(&data)?;
        let kind = match N {
            1 => "SCALAR",
            2 => "VEC2",
            3 => "VEC3",
            _ => "VEC4",
        };
        let mut accessor = Map::new();
        accessor.insert("bufferView".into(), view.into());
        accessor.insert("componentType".into(), FLOAT.into());
        accessor.insert("count".into(), values.len().into());
        accessor.insert("type".into(), kind.into());
        if bounds {
            let mut lo = [f32::INFINITY; N];
            let mut hi = [f32::NEG_INFINITY; N];
            for v in values {
                lo = crate::zip_with(lo, *v, f32::min);
                hi = crate::zip_with(hi, *v, f32::max);
            }
            accessor.insert("min".into(), json!(lo.as_slice()));
            accessor.insert("max".into(), json!(hi.as_slice()));
        }
        let accessor = Json::Object(accessor);
        self.accessors.push(accessor);
        Ok(self.accessors.len() - 1)
    }

    fn indices(&mut self, triangles: &[[u32; 3]]) -> Result<usize> {
        let data: Vec<u8> = triangles
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let view = self.view(&data)?;
        self.accessors.push(json!({
            "bufferView": view,
            "componentType": UINT,
            "count": triangles.len() * 3,
            "type": "SCALAR",
        }));
        Ok(self.accessors.len() - 1)
    }
}

/// How many times a second the animation of a scene is sampled, and the
/// most samples.
const RATE: f32 = 30.0;
const MOST_SAMPLES: usize = 3600;

/// A place as a translation, a rotation and a scale; one with no scale
/// on an axis keeps its translation, and nothing else.
fn trs(m: Option<&Matrix>) -> ([f64; 3], [f64; 4], [f64; 3]) {
    match m {
        Some(m) => decompose(m).unwrap_or((
            [0, 1, 2].map(|c| at(m, 3, c)),
            [0.0, 0.0, 0.0, 1.0],
            [0.0; 3],
        )),
        None => ([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
    }
}

/// Puts a node under another.
fn adopt(nodes: &mut [Map<String, Json>], parent: usize, child: usize) {
    let Some(p) = nodes.get_mut(parent) else {
        return;
    };
    match p.get_mut("children") {
        Some(Json::Array(children)) => children.push(child.into()),
        _ => {
            p.insert("children".into(), json!([child]));
        }
    }
}

/// Writes the meshes of a scene, and how they move, as a GLB. `pictures` are
/// the files of its textures, by their indices, where they are kept as
/// they were; the others are written as PNG.
pub(crate) fn write(scene: &Scene, pictures: &[Option<Vec<u8>>]) -> Result<Vec<u8>> {
    let mut w = Writer::default();
    let mut images = Vec::new();
    for (i, t) in scene.textures.iter().enumerate() {
        let data = match pictures.get(i) {
            Some(Some(bytes)) => bytes.clone(),
            _ => texture::encode(t).ok_or_else(|| crate::malformed("a texture to write"))?,
        };
        let kind = if data.starts_with(&[0xFF, 0xD8]) {
            "image/jpeg"
        } else {
            "image/png"
        };
        let view = w.view(&data)?;
        images.push(json!({"bufferView": view, "mimeType": kind}));
    }
    let textures: Vec<Json> = (0..images.len()).map(|i| json!({"source": i})).collect();
    let mut nodes: Vec<Map<String, Json>> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    let mut channels = Vec::new();
    let mut samplers = Vec::new();
    // The nodes of the joints that meshes hang from, and of those above
    // them, each moved by the samples of its place in its parent; a mesh
    // that moves hangs from the node of its joint, its corners in the
    // space of that node, so that a parent with a scale that differs by
    // axis bends what turns under it as it does in the scene.
    let mut joint_nodes: Vec<Option<usize>> = Vec::new();
    let mut rest_worlds = Vec::new();
    if let Some(a) = &scene.animation {
        rest_worlds = a.worlds(None);
        let mut needed = vec![false; a.joints.len()];
        for i in 0..scene.meshes.len() {
            let mut joint = a.binding(i).map(|(j, _)| j);
            while let Some(j) = joint {
                match needed.get_mut(j) {
                    Some(n) if !*n => *n = true,
                    _ => break,
                }
                joint = a.joints.get(j).and_then(|j| j.parent);
            }
        }
        let steps = ((a.duration() * RATE).ceil() as usize).clamp(1, MOST_SAMPLES);
        let times: Vec<[f32; 1]> = (0..=steps)
            .map(|k| [a.duration() * k as f32 / steps as f32])
            .collect();
        let samples: Vec<Vec<Matrix>> = times.iter().map(|[t]| a.locals(Some(*t))).collect();
        let rest = a.locals(None);
        let mut time_accessor = None;
        joint_nodes = vec![None; a.joints.len()];
        for (j, joint) in a.joints.iter().enumerate() {
            if !needed.get(j).copied().unwrap_or(false) {
                continue;
            }
            let mut node = Map::new();
            let (t, r, s) = trs(rest.get(j));
            node.insert("translation".into(), json!(t.as_slice()));
            node.insert("rotation".into(), json!(r.as_slice()));
            node.insert("scale".into(), json!(s.as_slice()));
            let index = nodes.len();
            nodes.push(node);
            match joint
                .parent
                .and_then(|p| joint_nodes.get(p).copied().flatten())
            {
                Some(p) => adopt(&mut nodes, p, index),
                None => roots.push(index),
            }
            if let Some(n) = joint_nodes.get_mut(j) {
                *n = Some(index);
            }
            let first = samples.first().and_then(|s| s.get(j));
            if samples.iter().all(|s| s.get(j) == first) {
                continue;
            }
            let input = match time_accessor {
                Some(a) => a,
                None => {
                    let a = w.floats(&times, true)?;
                    time_accessor = Some(a);
                    a
                }
            };
            let mut ts = Vec::new();
            let mut rs: Vec<[f32; 4]> = Vec::new();
            let mut ss = Vec::new();
            for sample in &samples {
                let (t, r, s) = trs(sample.get(j));
                let mut r = r.map(|v| v as f32);
                // The short way from the turn before.
                if let Some(last) = rs.last()
                    && last.iter().zip(&r).map(|(a, b)| a * b).sum::<f32>() < 0.0
                {
                    r = r.map(|v| -v);
                }
                ts.push(t.map(|v| v as f32));
                rs.push(r);
                ss.push(s.map(|v| v as f32));
            }
            for (path, output) in [
                ("translation", w.floats(&ts, false)?),
                ("rotation", w.floats(&rs, false)?),
                ("scale", w.floats(&ss, false)?),
            ] {
                samplers.push(json!({"input": input, "output": output}));
                channels.push(json!({
                    "sampler": samplers.len() - 1,
                    "target": {"node": index, "path": path},
                }));
            }
        }
    }
    let mut meshes = Vec::new();
    let mut materials = Vec::new();
    for (index, mesh) in scene.meshes.iter().enumerate() {
        let count = mesh.positions.len();
        if mesh.triangles.is_empty() || count == 0 {
            continue;
        }
        // The node the mesh hangs from, and its corners in its space.
        let binding = scene.animation.as_ref().and_then(|a| a.binding(index));
        let parent = binding.and_then(|(j, _)| joint_nodes.get(j).copied().flatten());
        let into = binding
            .filter(|_| parent.is_some())
            .and_then(|(j, inverse)| Some((inverse?, rest_worlds.get(j)?)));
        let (positions, normals) = match into {
            Some((inverse, world)) => (
                mesh.positions.iter().map(|p| point(inverse, *p)).collect(),
                mesh.normals.iter().map(|n| normal(world, *n)).collect(),
            ),
            None => (mesh.positions.clone(), mesh.normals.clone()),
        };
        let mut attributes = serde_json::Map::new();
        attributes.insert("POSITION".into(), w.floats(&positions, true)?.into());
        if normals.len() == count {
            attributes.insert("NORMAL".into(), w.floats(&normals, false)?.into());
        }
        let m = &mesh.material;
        let texture = m
            .texture
            .filter(|t| *t < images.len() && mesh.uvs.len() == count);
        if texture.is_some() {
            // V goes down in glTF.
            let uvs: Vec<[f32; 2]> = mesh.uvs.iter().map(|[u, v]| [*u, 1.0 - v]).collect();
            attributes.insert("TEXCOORD_0".into(), w.floats(&uvs, false)?.into());
        }
        // The colors of the corners stand for that of the material, which
        // glTF multiplies them by.
        let mut color = m.color;
        if mesh.colors.len() == count {
            attributes.insert("COLOR_0".into(), w.floats(&mesh.colors, false)?.into());
            color[..3].copy_from_slice(&[1.0; 3]);
        }
        let indices = w.indices(&mesh.triangles)?;
        let opaque_texture = texture
            .and_then(|t| scene.textures.get(t))
            .is_none_or(|t| t.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        let mut material = Map::new();
        if m.translucent || m.color[3] < 0.999 {
            material.insert("alphaMode".into(), "BLEND".into());
        } else {
            color[3] = 1.0;
            if !opaque_texture {
                material.insert("alphaMode".into(), "MASK".into());
                material.insert("alphaCutoff".into(), m.cutoff.into());
            }
        }
        let mut pbr = Map::new();
        pbr.insert("baseColorFactor".into(), json!(color.as_slice()));
        pbr.insert("metallicFactor".into(), 0.0.into());
        pbr.insert("roughnessFactor".into(), 1.0.into());
        if let Some(t) = texture {
            pbr.insert("baseColorTexture".into(), json!({"index": t}));
        }
        material.insert("pbrMetallicRoughness".into(), Json::Object(pbr));
        let material = Json::Object(material);
        materials.push(material);
        meshes.push(json!({"primitives": [{
            "attributes": attributes,
            "indices": indices,
            "material": materials.len() - 1,
        }]}));
        let mut node = Map::new();
        node.insert("mesh".into(), (meshes.len() - 1).into());
        let at = nodes.len();
        nodes.push(node);
        match parent {
            Some(p) => adopt(&mut nodes, p, at),
            None => roots.push(at),
        }
    }
    if meshes.is_empty() {
        return Err(Error::Empty);
    }
    w.bin.resize(w.bin.len().next_multiple_of(4), 0);
    let mut document = json!({
        "asset": {"version": "2.0", "generator": "Desk"},
        "scene": 0,
        "scenes": [{"nodes": roots}],
        "nodes": nodes.into_iter().map(Json::Object).collect::<Vec<_>>(),
        "meshes": meshes,
        "materials": materials,
        "buffers": [{"byteLength": w.bin.len()}],
        "bufferViews": w.views,
        "accessors": w.accessors,
    });
    if !channels.is_empty()
        && let Some(d) = document.as_object_mut()
    {
        d.insert(
            "animations".into(),
            json!([{"channels": channels, "samplers": samplers}]),
        );
    }
    if !images.is_empty()
        && let Some(d) = document.as_object_mut()
    {
        d.insert("images".into(), images.into());
        d.insert("textures".into(), textures.into());
    }
    crate::gltf::glb(&document, &w.bin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{View, render};

    #[test]
    fn w116_a_scene_written_as_a_glb_draws_as_it_did() {
        let folder = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/obj");
        for name in ["box.obj", "colors.obj"] {
            let bytes = std::fs::read(format!("{folder}/{name}")).unwrap();
            let read =
                crate::obj::read(&bytes, &mut |p| std::fs::read(format!("{folder}/{p}")).ok())
                    .unwrap();
            let again = crate::gltf::read(&write(&read.scene, &read.pictures).unwrap()).unwrap();
            assert_eq!(again.textures, read.scene.textures, "{name}");
            for view in [
                View::default(),
                View {
                    yaw: 30.0,
                    pitch: 60.0,
                },
            ] {
                assert_eq!(
                    render(&again, view, 48, 48),
                    render(&read.scene, view, 48, 48),
                    "{name}"
                );
            }
        }
        // A scene that moves moves as it did, at the times it is sampled.
        let scene = crate::gltf::read(include_bytes!("../tests/data/animated.glb")).unwrap();
        let again = crate::gltf::read(&write(&scene, &[]).unwrap()).unwrap();
        let animation = again.animation.as_ref().unwrap();
        assert!((animation.duration() - 2.0).abs() < 1e-6);
        for time in [0.0, 0.5, 1.0, 1.5, 2.0] {
            let view = View::default();
            let a = crate::render_at(&scene, view, time, 48, 48).unwrap();
            let b = crate::render_at(&again, view, time, 48, 48).unwrap();
            let apart = a
                .as_chunks::<4>()
                .0
                .iter()
                .zip(b.as_chunks::<4>().0)
                .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 8))
                .count();
            assert!(apart < 48 * 48 / 100, "{time}: {apart}");
        }
        // What turns under a parent of a scale that differs by axis is
        // bent by it, in the GLB as in the scene.
        let usda = br#"#usda 1.0
(
    timeCodesPerSecond = 1
    startTimeCode = 0
)
def Xform "Tall"
{
    float3 xformOp:scale = (1, 3, 1)
    uniform token[] xformOpOrder = ["xformOp:scale"]
    def Xform "Turn"
    {
        float xformOp:rotateZ.timeSamples = { 0: 0, 1: 45, 2: 90 }
        uniform token[] xformOpOrder = ["xformOp:rotateZ"]
        def Mesh "Bar"
        {
            point3f[] points = [(-1, -0.1, 0), (1, -0.1, 0), (1, 0.1, 0), (-1, 0.1, 0)]
            int[] faceVertexCounts = [4]
            int[] faceVertexIndices = [0, 1, 2, 3]
        }
    }
}
"#;
        let scene = crate::read_layer(usda).unwrap();
        let again = crate::gltf::read(&write(&scene, &[]).unwrap()).unwrap();
        for time in [0.0, 0.5, 1.0, 1.5, 2.0] {
            let view = View::default();
            let a = crate::render_at(&scene, view, time, 48, 48).unwrap();
            let b = crate::render_at(&again, view, time, 48, 48).unwrap();
            let apart = a
                .as_chunks::<4>()
                .0
                .iter()
                .zip(b.as_chunks::<4>().0)
                .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 8))
                .count();
            assert!(apart < 48 * 48 / 100, "bent at {time}: {apart}");
        }
        // A texture with no file is written as a PNG, and one that cuts
        // out keeps its alpha.
        let mut scene = crate::Scene {
            meshes: vec![crate::Mesh {
                positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                uvs: vec![[0.0; 2], [1.0, 0.0], [0.0, 1.0]],
                triangles: vec![[0, 1, 2]],
                material: crate::Material {
                    texture: Some(0),
                    cutoff: 0.25,
                    ..crate::Material::default()
                },
                ..crate::Mesh::default()
            }],
            textures: vec![crate::Texture {
                width: 1,
                height: 2,
                rgba: vec![10, 20, 30, 0, 40, 50, 60, 255],
            }],
            animation: None,
        };
        let again = crate::gltf::read(&write(&scene, &[]).unwrap()).unwrap();
        assert_eq!(again, scene);
        scene.meshes.clear();
        assert_eq!(write(&scene, &[]), Err(Error::Empty));
    }
}
