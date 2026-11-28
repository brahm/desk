// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! glTF 2.0, in its text form (`.gltf`, JSON) and its binary one (`.glb`,
//! the JSON and a buffer in one file).
//!
//! A reader takes the nodes of the scene with their places, the
//! triangles of their meshes (lists, strips and fans), the base colors
//! and textures of their materials, the colors of the corners, and the
//! first animation of the file, which moves nodes by their translation,
//! rotation and scale. It leaves out skins, morph targets, cameras and
//! lights, and the lines and points of meshes.
//!
//! A text glTF names its buffers and pictures by URIs: inside the file
//! as `data:` URIs, or files beside it. [`pack`] gathers them into one
//! GLB, which is what a document keeps.

use std::borrow::Cow;
use std::collections::HashMap;

use serde_json::{Map, Value as Json};

use crate::animation::{Animation, Interpolation, Joint, Local, MeshJoint, Path, Track};
use crate::matrix::{IDENTITY, Matrix, invert, mul, normal, point};
use crate::{Error, Material, Mesh, Result, Scene, Texture, limits, malformed, texture};

/// The magic number of a GLB, and those of its chunks.
const GLB: &[u8; 4] = b"glTF";
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;

/// Whether the bytes are a GLB.
pub(crate) fn is_glb(bytes: &[u8]) -> bool {
    bytes.starts_with(GLB) && bytes.get(4..8) == Some(&2u32.to_le_bytes()[..])
}

/// Whether the bytes are a text glTF: a JSON object with an `asset`.
pub(crate) fn is_gltf(bytes: &[u8]) -> bool {
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = text.iter().position(|b| !b.is_ascii_whitespace());
    start.and_then(|s| text.get(s)) == Some(&b'{') && text.windows(7).any(|w| w == b"\"asset\"")
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The JSON and the buffer of a GLB.
fn split_glb(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>)> {
    let cut = || malformed("the GLB is cut short");
    let length = u32_at(bytes, 8).ok_or_else(cut)? as usize;
    let bytes = bytes.get(..length).ok_or_else(cut)?;
    let mut at = 12;
    let mut json = None;
    let mut bin = None;
    while at + 8 <= bytes.len() {
        let size = u32_at(bytes, at).ok_or_else(cut)? as usize;
        let kind = u32_at(bytes, at + 4).ok_or_else(cut)?;
        let data = bytes
            .get(at + 8..(at + 8).checked_add(size).ok_or_else(cut)?)
            .ok_or_else(cut)?;
        match kind {
            JSON_CHUNK if json.is_none() => json = Some(data),
            BIN_CHUNK if bin.is_none() && json.is_some() => bin = Some(data),
            _ => {}
        }
        at += 8 + size.next_multiple_of(4);
    }
    Ok((json.ok_or_else(|| malformed("the GLB has no JSON"))?, bin))
}

/// The JSON of a glTF or a GLB, and the buffer of a GLB.
fn parse(bytes: &[u8]) -> Result<(Json, Option<&[u8]>)> {
    let (text, bin) = if is_glb(bytes) {
        split_glb(bytes)?
    } else {
        (bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes), None)
    };
    let json: Json =
        serde_json::from_slice(text).map_err(|e| malformed(format!("the JSON of glTF: {e}")))?;
    if !json.is_object() {
        return Err(malformed("the JSON of glTF is not an object"));
    }
    let version = json
        .pointer("/asset/version")
        .and_then(Json::as_str)
        .unwrap_or("");
    if !version.starts_with('2') {
        return Err(Error::Unknown);
    }
    Ok((json, bin))
}

/// The bytes of a `data:` URI in base 64, and its media type.
fn data_uri(uri: &str) -> Option<(Vec<u8>, &str)> {
    let rest = uri.strip_prefix("data:")?;
    let (head, data) = rest.split_once(',')?;
    let media = head.strip_suffix(";base64")?;
    Some((base64(data)?, media))
}

/// Base 64, with or without padding; none when it is broken.
pub(crate) fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for b in text.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// A URI of a file beside the glTF as a relative path, decoded; none for
/// one that leaves the folder of the glTF, names a whole path or has a
/// scheme.
pub(crate) fn relative_path(uri: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(uri.len());
    let raw = uri.as_bytes();
    let mut i = 0;
    while let Some(&b) = raw.get(i) {
        if b == b'%' {
            let hex = std::str::from_utf8(raw.get(i + 1..i + 3)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            bytes.push(b);
            i += 1;
        }
    }
    safe_path(String::from_utf8(bytes).ok()?)
}

/// A relative path that stays in its folder: none for one that leaves
/// it, names a whole path, has a scheme or names a hidden file.
pub(crate) fn safe_path(path: String) -> Option<String> {
    let scheme = path.split_once(':').is_some_and(|(s, _)| !s.contains('/'));
    if path.is_empty()
        || scheme
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path.split('/').any(|s| s.is_empty() || s.starts_with('.'))
    {
        return None;
    }
    Some(path)
}

fn index(v: &Json, key: &str) -> Option<usize> {
    usize::try_from(v.get(key)?.as_u64()?).ok()
}

fn list<'a>(v: &'a Json, key: &str) -> &'a [Json] {
    v.get(key)
        .and_then(Json::as_array)
        .map_or(&[][..], Vec::as_slice)
}

fn numbers<const N: usize>(v: &Json, key: &str, default: [f64; N]) -> [f64; N] {
    let Some(a) = v.get(key).and_then(Json::as_array) else {
        return default;
    };
    if a.len() != N {
        return default;
    }
    let mut out = default;
    for (o, v) in out.iter_mut().zip(a) {
        match v.as_f64() {
            Some(x) if x.is_finite() => *o = x,
            _ => return default,
        }
    }
    out
}

/// The numbers of an element of an accessor.
fn width(kind: &str) -> Option<usize> {
    Some(match kind {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" | "MAT2" => 4,
        "MAT3" => 9,
        "MAT4" => 16,
        _ => return None,
    })
}

/// The bytes of a component type.
fn component_size(kind: u64) -> Option<usize> {
    Some(match kind {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return None,
    })
}

/// A component as a number, made from 0 to 1 (or -1 to 1) when it is
/// normalized.
fn float(bytes: &[u8], kind: u64, normalized: bool) -> Option<f32> {
    let v = match kind {
        5126 => f32::from_le_bytes(bytes.get(..4)?.try_into().ok()?),
        5120 => {
            let v = f32::from(i8::from_le_bytes([*bytes.first()?]));
            if normalized { (v / 127.0).max(-1.0) } else { v }
        }
        5121 => {
            let v = f32::from(*bytes.first()?);
            if normalized { v / 255.0 } else { v }
        }
        5122 => {
            let v = f32::from(i16::from_le_bytes(bytes.get(..2)?.try_into().ok()?));
            if normalized {
                (v / 32767.0).max(-1.0)
            } else {
                v
            }
        }
        5123 => {
            let v = f32::from(u16::from_le_bytes(bytes.get(..2)?.try_into().ok()?));
            if normalized { v / 65535.0 } else { v }
        }
        5125 => u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as f32,
        _ => return None,
    };
    v.is_finite().then_some(v)
}

/// A component as an index.
fn integer(bytes: &[u8], kind: u64) -> Option<u32> {
    match kind {
        5121 => Some(u32::from(*bytes.first()?)),
        5123 => Some(u32::from(u16::from_le_bytes(
            bytes.get(..2)?.try_into().ok()?,
        ))),
        5125 => Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?)),
        _ => None,
    }
}

/// The most numbers of accessors read from one file, in all.
const ACCESSOR_NUMBERS: usize = 64 * 1024 * 1024;

/// The file being read: its JSON and its buffers.
struct Document<'a> {
    json: &'a Json,
    buffers: Vec<Option<Cow<'a, [u8]>>>,
    /// The numbers of accessors read so far, against
    /// [`ACCESSOR_NUMBERS`].
    read: std::cell::Cell<usize>,
}

impl Document<'_> {
    fn get(&self, key: &str, i: usize) -> Option<&Json> {
        list(self.json, key).get(i)
    }

    /// The bytes of a buffer view, and its stride.
    fn view(&self, i: usize) -> Option<(&[u8], Option<usize>)> {
        let view = self.get("bufferViews", i)?;
        let buffer = self.buffers.get(index(view, "buffer")?)?.as_deref()?;
        let offset = index(view, "byteOffset").unwrap_or(0);
        let length = index(view, "byteLength")?;
        let bytes = buffer.get(offset..offset.checked_add(length)?)?;
        let stride = index(view, "byteStride").filter(|s| (4..=252).contains(s));
        Some((bytes, stride))
    }

    /// The elements of an accessor, each component read by `read`: the
    /// numbers of an element, and all of them one after another.
    fn accessor<T: Copy + Default>(
        &self,
        i: usize,
        read: impl Fn(&[u8], u64) -> Option<T>,
    ) -> Option<(usize, Vec<T>)> {
        let accessor = self.get("accessors", i)?;
        let count = index(accessor, "count")?;
        let n = width(accessor.get("type")?.as_str()?)?;
        let kind = accessor.get("componentType")?.as_u64()?;
        let size = component_size(kind)?;
        let numbers = count.checked_mul(n)?;
        let total = self.read.get().checked_add(numbers)?;
        if numbers > limits::CORNERS * 4 || total > ACCESSOR_NUMBERS {
            return None;
        }
        self.read.set(total);
        let element = n * size;
        let mut out = vec![T::default(); count * n];
        if let Some(v) = index(accessor, "bufferView") {
            let (bytes, stride) = self.view(v)?;
            let stride = stride.unwrap_or(element);
            let offset = index(accessor, "byteOffset").unwrap_or(0);
            if count > 0 {
                let end = offset
                    .checked_add((count - 1).checked_mul(stride)?)?
                    .checked_add(element)?;
                if end > bytes.len() {
                    return None;
                }
            }
            for (k, slot) in out.chunks_exact_mut(n).enumerate() {
                let at = offset + k * stride;
                for (c, o) in slot.iter_mut().enumerate() {
                    *o = read(bytes.get(at + c * size..)?, kind)?;
                }
            }
        }
        if let Some(sparse) = accessor.get("sparse") {
            let many = index(sparse, "count")?;
            let indices = sparse.get("indices")?;
            let values = sparse.get("values")?;
            let index_kind = indices.get("componentType")?.as_u64()?;
            let index_size = component_size(index_kind)?;
            let (ib, _) = self.view(index(indices, "bufferView")?)?;
            let (vb, _) = self.view(index(values, "bufferView")?)?;
            let io = index(indices, "byteOffset").unwrap_or(0);
            let vo = index(values, "byteOffset").unwrap_or(0);
            for k in 0..many {
                let at = integer(ib.get(io + k * index_size..)?, index_kind)? as usize;
                let slot = out.get_mut(at * n..at * n + n)?;
                for (c, o) in slot.iter_mut().enumerate() {
                    *o = read(vb.get(vo + k * element + c * size..)?, kind)?;
                }
            }
        }
        Some((n, out))
    }

    /// The numbers of an accessor, normalized as it says.
    fn floats(&self, i: usize) -> Option<(usize, Vec<f32>)> {
        let normalized = self
            .get("accessors", i)?
            .get("normalized")
            .and_then(Json::as_bool)
            .unwrap_or(false);
        self.accessor(i, |b, k| float(b, k, normalized))
    }
}

/// The meshes of a scene as they are built.
struct Builder<'a> {
    doc: Document<'a>,
    meshes: Vec<Mesh>,
    /// The node of each mesh, by its index among the joints.
    mesh_joints: Vec<MeshJoint>,
    textures: Vec<Texture>,
    /// The textures made, by the picture and whether the material is
    /// opaque.
    made: HashMap<(usize, bool), Option<usize>>,
    joints: Vec<Joint>,
    /// The joint of each node.
    node_joint: HashMap<usize, usize>,
    triangles: usize,
    corners: usize,
}

impl Builder<'_> {
    /// The picture of an image, decoded.
    fn image(&self, i: usize) -> Option<Texture> {
        let image = self.doc.get("images", i)?;
        if let Some(v) = index(image, "bufferView") {
            return texture::decode(self.doc.view(v)?.0);
        }
        let (bytes, _) = data_uri(image.get("uri")?.as_str()?)?;
        texture::decode(&bytes)
    }

    /// The texture of a glTF texture, decoded once.
    fn texture(&mut self, i: usize, opaque: bool) -> Option<usize> {
        let source = index(self.doc.get("textures", i)?, "source")?;
        if let Some(made) = self.made.get(&(source, opaque)) {
            return *made;
        }
        let made = if self.textures.len() < limits::TEXTURES {
            self.image(source).map(|mut t| {
                if opaque {
                    for [_, _, _, a] in t.rgba.as_chunks_mut::<4>().0 {
                        *a = 255;
                    }
                }
                self.textures.push(t);
                self.textures.len() - 1
            })
        } else {
            None
        };
        self.made.insert((source, opaque), made);
        made
    }

    /// A material, and whether its texture takes the first coordinates.
    fn material(&mut self, i: Option<usize>) -> Material {
        let Some(m) = i.and_then(|i| self.doc.get("materials", i)).cloned() else {
            return Material::default();
        };
        let opaque = !matches!(
            m.get("alphaMode").and_then(Json::as_str),
            Some("BLEND" | "MASK")
        );
        let pbr = m.get("pbrMetallicRoughness");
        let gloss = m.pointer("/extensions/KHR_materials_pbrSpecularGlossiness");
        let (factor, texture) = match (pbr, gloss) {
            (_, Some(g)) => (
                numbers(g, "diffuseFactor", [1.0; 4]),
                g.get("diffuseTexture"),
            ),
            (Some(p), None) => (
                numbers(p, "baseColorFactor", [1.0; 4]),
                p.get("baseColorTexture"),
            ),
            (None, None) => ([1.0; 4], None),
        };
        let mode = m.get("alphaMode").and_then(Json::as_str);
        let mut color = factor.map(|v| v.clamp(0.0, 1.0) as f32);
        // A mask cuts where the alpha of the color times that of the
        // texture is under its cutoff: the texture's alone is compared
        // with the cutoff over the color's, and the mesh is drawn with the
        // opaque ones. Nothing of an opaque material is cut out.
        let cutoff = match mode {
            Some("MASK") => {
                let c = m
                    .get("alphaCutoff")
                    .and_then(Json::as_f64)
                    .filter(|c| c.is_finite())
                    .map_or(0.5, |c| c.clamp(0.0, 1.0) as f32);
                if color[3] > 0.0 { c / color[3] } else { 2.0 }
            }
            _ => 0.0,
        };
        if mode != Some("BLEND") {
            color[3] = 1.0;
        }
        let texture = texture
            .filter(|t| index(t, "texCoord").unwrap_or(0) == 0)
            .and_then(|t| index(t, "index"))
            .and_then(|t| self.texture(t, opaque));
        Material {
            color,
            texture,
            translucent: mode == Some("BLEND"),
            cutoff,
        }
    }

    /// The meshes of a primitive, placed by the world matrix of its node.
    fn primitive(&mut self, p: &Json, world: &Matrix, joint: usize) -> Result<()> {
        if !matches!(index(p, "mode").unwrap_or(4), 4..=6) {
            return Ok(());
        }
        let Some(attributes) = p.get("attributes") else {
            return Ok(());
        };
        let Some((3, positions)) = index(attributes, "POSITION").and_then(|a| self.doc.floats(a))
        else {
            return Ok(());
        };
        let count = positions.len() / 3;
        let order: Vec<u32> = match index(p, "indices") {
            Some(a) => match self.doc.accessor(a, integer) {
                Some((1, v)) => v,
                _ => return Ok(()),
            },
            None => (0..count as u32).collect(),
        };
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        let mut push = |t: [u32; 3]| {
            if t.iter().all(|i| (*i as usize) < count)
                && t[0] != t[1]
                && t[1] != t[2]
                && t[0] != t[2]
            {
                triangles.push(t);
            }
        };
        match index(p, "mode").unwrap_or(4) {
            4 => order.as_chunks::<3>().0.iter().for_each(|t| push(*t)),
            5 => {
                for (k, &[a, b, c]) in order.array_windows::<3>().enumerate() {
                    push(if k % 2 == 0 { [a, b, c] } else { [b, a, c] });
                }
            }
            _ => {
                if let Some((&first, rest)) = order.split_first() {
                    for &[b, c] in rest.array_windows::<2>() {
                        push([first, b, c]);
                    }
                }
            }
        }
        if triangles.is_empty() {
            return Ok(());
        }
        self.triangles += triangles.len();
        self.corners += count;
        if self.triangles > limits::TRIANGLES || self.corners > limits::CORNERS {
            return Err(Error::TooLarge);
        }
        let material = self.material(index(p, "material"));
        // A node with no inverse, as at a scale of 0, keeps the corners in
        // its own space, for its animation to place.
        let (world, inverse, local) = match invert(world) {
            Some(inverse) => (*world, inverse, false),
            None => (IDENTITY, IDENTITY, true),
        };
        let world = &world;
        let positions: Vec<[f32; 3]> = positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| point(world, *p))
            .collect();
        let normals = match index(attributes, "NORMAL").and_then(|a| self.doc.floats(a)) {
            Some((3, n)) if n.len() == positions.len() * 3 => n
                .as_chunks::<3>()
                .0
                .iter()
                .map(|n| unit(normal(&inverse, *n)))
                .collect(),
            _ => Vec::new(),
        };
        let uvs = match (
            material.texture,
            index(attributes, "TEXCOORD_0").and_then(|a| self.doc.floats(a)),
        ) {
            (Some(_), Some((2, uv))) if uv.len() == positions.len() * 2 => uv
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[u, v]| [*u, 1.0 - v])
                .collect(),
            _ => Vec::new(),
        };
        let mut material = material;
        if uvs.is_empty() {
            material.texture = None;
        }
        let colors = match index(attributes, "COLOR_0").and_then(|a| self.doc.floats(a)) {
            Some((n @ (3 | 4), c)) if c.len() == positions.len() * n => {
                let base = material.color;
                c.chunks_exact(n)
                    .map(|c| {
                        [0, 1, 2].map(|k| {
                            (c.get(k).copied().unwrap_or(1.0) * base.get(k).copied().unwrap_or(1.0))
                                .clamp(0.0, 1.0)
                        })
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        self.meshes.push(Mesh {
            positions,
            normals,
            uvs,
            colors,
            triangles,
            material,
        });
        self.mesh_joints.push(Some((joint, local)));
        Ok(())
    }

    /// A node and those under it, with the world matrix of its parent.
    fn node(
        &mut self,
        i: usize,
        parent: Option<usize>,
        above: &Matrix,
        depth: usize,
    ) -> Result<()> {
        if depth > limits::DEPTH {
            return Err(Error::TooLarge);
        }
        // A node that is reached twice is not a tree: it is left out.
        if self.node_joint.contains_key(&i) || self.joints.len() >= limits::PRIMS {
            return Ok(());
        }
        let Some(node) = self.doc.get("nodes", i).cloned() else {
            return Ok(());
        };
        let rest = if node.get("matrix").is_some() {
            let m = numbers(&node, "matrix", IDENTITY);
            // Column by column, which as rows is the matrix for row
            // vectors.
            Local::Matrix(m)
        } else {
            Local::Trs {
                t: numbers(&node, "translation", [0.0; 3]),
                r: numbers(&node, "rotation", [0.0, 0.0, 0.0, 1.0]),
                s: numbers(&node, "scale", [1.0; 3]),
            }
        };
        let local = rest.matrix(None);
        let world = mul(&local, above);
        let joint = self.joints.len();
        self.joints.push(Joint { parent, rest });
        self.node_joint.insert(i, joint);
        if let Some(mesh) = index(&node, "mesh")
            .and_then(|m| self.doc.get("meshes", m))
            .cloned()
        {
            for p in list(&mesh, "primitives") {
                self.primitive(p, &world, joint)?;
            }
        }
        for child in list(&node, "children") {
            if let Some(c) = child.as_u64().and_then(|c| usize::try_from(c).ok()) {
                self.node(c, Some(joint), &world, depth + 1)?;
            }
        }
        Ok(())
    }

    /// The tracks of the first animation, by the joints they move.
    fn tracks(&self) -> Vec<Track> {
        let Some(animation) = list(self.doc.json, "animations").first() else {
            return Vec::new();
        };
        let samplers = list(animation, "samplers");
        let mut keys = 0usize;
        let mut tracks = Vec::new();
        for channel in list(animation, "channels") {
            let Some(target) = channel.get("target") else {
                continue;
            };
            let path = match target.get("path").and_then(Json::as_str) {
                Some("translation") => Path::Translation,
                Some("rotation") => Path::Rotation,
                Some("scale") => Path::Scale,
                _ => continue,
            };
            let Some(&joint) = index(target, "node").and_then(|n| self.node_joint.get(&n)) else {
                continue;
            };
            if !matches!(
                self.joints.get(joint),
                Some(Joint {
                    rest: Local::Trs { .. },
                    ..
                })
            ) {
                continue;
            }
            let Some(sampler) = index(channel, "sampler").and_then(|s| samplers.get(s)) else {
                continue;
            };
            let interpolation = match sampler.get("interpolation").and_then(Json::as_str) {
                Some("STEP") => Interpolation::Step,
                Some("CUBICSPLINE") => Interpolation::Cubic,
                _ => Interpolation::Linear,
            };
            // The kinds of the accessors first, so that one of the wrong
            // kind is not read.
            let kind = |key: &str| {
                let a = self.doc.get("accessors", index(sampler, key)?)?;
                Some((width(a.get("type")?.as_str()?)?, index(a, "count")?))
            };
            let per = if interpolation == Interpolation::Cubic {
                3
            } else {
                1
            };
            match (kind("input"), kind("output")) {
                (Some((1, keys)), Some((n, values)))
                    if n == path.width() && Some(values) == keys.checked_mul(per) => {}
                _ => continue,
            }
            let Some((1, times)) = index(sampler, "input").and_then(|a| self.doc.floats(a)) else {
                continue;
            };
            let Some((n, values)) = index(sampler, "output").and_then(|a| self.doc.floats(a))
            else {
                continue;
            };
            if n != path.width()
                || times.is_empty()
                || values.len() != times.len() * n * per
                || times.array_windows::<2>().any(|[a, b]| b < a)
                || times.iter().any(|t| !t.is_finite())
            {
                continue;
            }
            keys += times.len();
            if keys > limits::ANIMATION_KEYS {
                break;
            }
            tracks.push(Track {
                joint,
                path,
                interpolation,
                times,
                values,
            });
        }
        tracks
    }
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n > 0.0 && n.is_finite() {
        v.map(|c| c / n)
    } else {
        [0.0; 3]
    }
}

/// The buffers of a document: that of a GLB and those of `data:` URIs.
fn buffers<'a>(json: &Json, bin: Option<&'a [u8]>) -> Vec<Option<Cow<'a, [u8]>>> {
    list(json, "buffers")
        .iter()
        .enumerate()
        .map(|(i, b)| match b.get("uri").and_then(Json::as_str) {
            None if i == 0 => bin.map(Cow::Borrowed),
            Some(uri) => data_uri(uri).map(|(d, _)| Cow::Owned(d)),
            None => None,
        })
        .collect()
}

/// Reads the scene of a glTF or a GLB whose buffers are inside it.
pub(crate) fn read(bytes: &[u8]) -> Result<Scene> {
    let (json, bin) = parse(bytes)?;
    let doc = Document {
        buffers: buffers(&json, bin),
        json: &json,
        read: std::cell::Cell::new(0),
    };
    let mut builder = Builder {
        doc,
        meshes: Vec::new(),
        mesh_joints: Vec::new(),
        textures: Vec::new(),
        made: HashMap::new(),
        joints: Vec::new(),
        node_joint: HashMap::new(),
        triangles: 0,
        corners: 0,
    };
    let scenes = list(&json, "scenes");
    let roots: Vec<usize> = match index(&json, "scene")
        .and_then(|s| scenes.get(s))
        .or_else(|| scenes.first())
    {
        Some(scene) => list(scene, "nodes")
            .iter()
            .filter_map(|n| usize::try_from(n.as_u64()?).ok())
            .collect(),
        // No scene: each node that is no other's child.
        None => {
            let nodes = list(&json, "nodes");
            let children: std::collections::HashSet<u64> = nodes
                .iter()
                .flat_map(|n| list(n, "children"))
                .filter_map(Json::as_u64)
                .collect();
            (0..nodes.len())
                .filter(|i| !children.contains(&(*i as u64)))
                .collect()
        }
    };
    for root in roots {
        builder.node(root, None, &IDENTITY, 0)?;
    }
    let tracks = builder.tracks();
    let animation = Animation::new(
        std::mem::take(&mut builder.joints),
        tracks,
        &builder.mesh_joints,
        &builder.meshes,
    );
    // With nothing to move them, the meshes of nodes at a scale of 0 are
    // nowhere to be seen.
    if animation.is_none() {
        let keep: Vec<bool> = builder
            .mesh_joints
            .iter()
            .map(|j| !matches!(j, Some((_, true))))
            .collect();
        let mut keep = keep.into_iter();
        builder.meshes.retain(|_| keep.next().unwrap_or(true));
    }
    Ok(Scene {
        meshes: builder.meshes,
        textures: builder.textures,
        animation,
    })
}

/// The media type of a picture, by its first bytes.
fn picture_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

/// Gathers a glTF or a GLB with the files it names into one GLB: its
/// buffers into one, and its pictures into views of it. `files` gives the
/// bytes of a file beside the glTF by its relative path, which never
/// leaves the folder of the glTF: a URI with `..`, a whole path, a
/// scheme or a hidden file is refused. Only the buffers that views use
/// and the pictures that textures use are asked for, and none for a GLB.
pub fn pack(bytes: &[u8], files: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Result<Vec<u8>> {
    let (mut json, bin) = parse(bytes)?;
    // A GLB stands alone: no file beside it is read for it.
    let alone = is_glb(bytes);
    let mut outside = |uri: &str| -> Option<Vec<u8>> {
        if alone {
            return None;
        }
        files(&relative_path(uri)?)
    };
    // Only the buffers that views use, and the pictures that textures
    // use, are read: a file names no other file beside it to be taken.
    let used_buffers: std::collections::HashSet<usize> = list(&json, "bufferViews")
        .iter()
        .filter_map(|v| index(v, "buffer"))
        .collect();
    let used_images: std::collections::HashSet<usize> = list(&json, "textures")
        .iter()
        .filter_map(|t| index(t, "source"))
        .collect();
    let mut out: Vec<u8> = Vec::new();
    let grow = |out: &mut Vec<u8>, data: &[u8]| -> Result<usize> {
        let at = out.len().next_multiple_of(4);
        if (at + data.len()) as u64 > limits::FILE_BYTES {
            return Err(Error::TooLarge);
        }
        out.resize(at, 0);
        out.extend_from_slice(data);
        Ok(at)
    };
    let mut offsets = Vec::new();
    let mut sizes = Vec::new();
    for (i, b) in list(&json, "buffers").iter().enumerate() {
        let data = match b.get("uri").and_then(Json::as_str) {
            _ if !used_buffers.contains(&i) => Some(Vec::new()),
            None if i == 0 => bin.map(<[u8]>::to_vec),
            Some(uri) => match data_uri(uri) {
                Some((data, _)) => Some(data),
                None => outside(uri),
            },
            None => None,
        };
        let data = data.ok_or_else(|| malformed(format!("buffer {i} of the glTF is missing")))?;
        offsets.push(grow(&mut out, &data)?);
        sizes.push(data.len());
    }
    // Each view into the one buffer; one that is not inside its buffer is
    // left pointing at none, as it was.
    let object = json
        .as_object_mut()
        .ok_or_else(|| malformed("the JSON of glTF is not an object"))?;
    if let Some(Json::Array(views)) = object.get_mut("bufferViews") {
        for view in views.iter_mut() {
            let Some(buffer) = index(view, "buffer") else {
                continue;
            };
            let offset = index(view, "byteOffset").unwrap_or(0);
            let length = index(view, "byteLength");
            let inside = match (sizes.get(buffer), length) {
                (Some(size), Some(length)) => {
                    offset.checked_add(length).is_some_and(|end| end <= *size)
                }
                _ => false,
            };
            let Some(view) = view.as_object_mut() else {
                continue;
            };
            match offsets.get(buffer) {
                Some(base) if inside => {
                    view.insert("buffer".into(), 0.into());
                    view.insert("byteOffset".into(), (base + offset).into());
                }
                _ => {
                    view.insert("buffer".into(), 1.into());
                }
            }
        }
    }
    // The pictures named by URIs, as new views of the buffer.
    let mut views = match object.get("bufferViews") {
        Some(Json::Array(v)) => v.len(),
        _ => 0,
    };
    let known = views as u64;
    // A reference to a view past the last one stays one to no view, not
    // to a picture added after them.
    let dangling = |v: &mut Json| {
        if let Some(r) = v.get_mut("bufferView")
            && r.as_u64().is_some_and(|i| i >= known)
        {
            *r = u64::MAX.into();
        }
    };
    if let Some(Json::Array(accessors)) = object.get_mut("accessors") {
        for accessor in accessors.iter_mut() {
            dangling(accessor);
            if let Some(sparse) = accessor.get_mut("sparse") {
                for part in ["indices", "values"] {
                    if let Some(p) = sparse.get_mut(part) {
                        dangling(p);
                    }
                }
            }
        }
    }
    let mut added = Vec::new();
    if let Some(Json::Array(images)) = object.get_mut("images") {
        for (n, image) in images.iter_mut().enumerate() {
            let Some(image) = image.as_object_mut() else {
                continue;
            };
            let Some(uri) = image.remove("uri") else {
                continue;
            };
            // A view wins over a URI, as the reader takes it, when it is an
            // index; a view that is none is dropped, and the URI taken.
            match image.get_mut("bufferView") {
                Some(r) if r.as_u64().is_some() => {
                    if r.as_u64().is_some_and(|i| i >= known) {
                        *r = u64::MAX.into();
                    }
                    continue;
                }
                Some(_) => {
                    image.remove("bufferView");
                }
                None => {}
            }
            let Some(uri) = uri.as_str() else {
                continue;
            };
            let data = match data_uri(uri) {
                Some((data, _)) => data,
                // A picture beside the file that no texture uses is not
                // read; one that a texture uses must be there, and be a
                // picture.
                None if !used_images.contains(&n) => continue,
                None => {
                    let data = outside(uri)
                        .ok_or_else(|| malformed(format!("picture {n} of the glTF is missing")))?;
                    if picture_type(&data) == "application/octet-stream" {
                        return Err(malformed(format!(
                            "picture {n} of the glTF is not a picture"
                        )));
                    }
                    data
                }
            };
            let at = grow(&mut out, &data)?;
            image.insert("mimeType".into(), picture_type(&data).into());
            image.insert("bufferView".into(), views.into());
            views += 1;
            let mut view = Map::new();
            view.insert("buffer".into(), 0.into());
            view.insert("byteOffset".into(), at.into());
            view.insert("byteLength".into(), data.len().into());
            added.push(Json::Object(view));
        }
    }
    if !added.is_empty() {
        match object.get_mut("bufferViews") {
            Some(Json::Array(v)) => v.extend(added),
            _ => {
                object.insert("bufferViews".into(), Json::Array(added));
            }
        }
    }
    out.resize(out.len().next_multiple_of(4), 0);
    let mut buffer = Map::new();
    buffer.insert("byteLength".into(), out.len().into());
    object.insert(
        "buffers".into(),
        Json::Array(if out.is_empty() {
            Vec::new()
        } else {
            vec![Json::Object(buffer)]
        }),
    );
    glb(&json, &out)
}

/// A GLB of its JSON and its buffer, which is left out when it is empty.
pub(crate) fn glb(json: &Json, bin: &[u8]) -> Result<Vec<u8>> {
    let mut text = serde_json::to_vec(json).map_err(|e| malformed(e.to_string()))?;
    text.resize(text.len().next_multiple_of(4), b' ');
    let padded = bin.len().next_multiple_of(4);
    let length = 12 + 8 + text.len() + if bin.is_empty() { 0 } else { 8 + padded };
    let length = u32::try_from(length).map_err(|_| Error::TooLarge)?;
    let mut glb = Vec::with_capacity(length as usize);
    glb.extend_from_slice(GLB);
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&length.to_le_bytes());
    let chunk = |glb: &mut Vec<u8>, kind: u32, data: &[u8], size: usize| -> Result<()> {
        let size32 = u32::try_from(size).map_err(|_| Error::TooLarge)?;
        glb.extend_from_slice(&size32.to_le_bytes());
        glb.extend_from_slice(&kind.to_le_bytes());
        glb.extend_from_slice(data);
        glb.resize(glb.len() + size - data.len(), 0);
        Ok(())
    };
    chunk(&mut glb, JSON_CHUNK, &text, text.len())?;
    if !bin.is_empty() {
        chunk(&mut glb, BIN_CHUNK, bin, padded)?;
    }
    Ok(glb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_files_beside_a_gltf_stay_beside_it() {
        assert_eq!(relative_path("a%20b.bin").as_deref(), Some("a b.bin"));
        assert_eq!(
            relative_path("pictures/x.png").as_deref(),
            Some("pictures/x.png")
        );
        for uri in [
            "../x.bin",
            "a/../../x.bin",
            "/etc/passwd",
            "file:///etc/passwd",
            "https://example.com/x.bin",
            "C:/x.bin",
            "a\\x.bin",
            "%2e%2e/x.bin",
            "./x.bin",
            ".ssh/id_ed25519",
            "a/.hidden.bin",
            "a//b.bin",
            "",
        ] {
            assert_eq!(relative_path(uri), None, "{uri}");
        }
    }

    #[test]
    fn w116_base_64_and_data_uris() {
        assert_eq!(base64("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64("aGVsbG8").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64("a*"), None);
        let (data, media) = data_uri("data:image/png;base64,AAEC").unwrap();
        assert_eq!((data.as_slice(), media), (&[0u8, 1, 2][..], "image/png"));
        assert!(data_uri("data:text/plain,hello").is_none());
    }

    #[test]
    fn w116_the_parts_of_an_animation_move_with_their_nodes() {
        let bytes = include_bytes!("../tests/data/animated.glb");
        let scene = read(bytes).unwrap();
        let animation = scene.animation.as_ref().unwrap();
        let at = |time: f32, mesh: usize, p: [f32; 3]| {
            let poses = animation.poses(Some(time));
            match poses.get(mesh).copied().flatten() {
                Some(m) => point(&m, p),
                None => p,
            }
        };
        // The red box goes up 5 in two seconds, and half of it in one.
        assert_eq!(at(1.0, 0, [12.0, 2.0, 2.0]), [12.0, 4.5, 2.0]);
        assert_eq!(at(2.0, 0, [12.0, 2.0, 2.0]), [12.0, 7.0, 2.0]);
        // The blue box goes with it, and has turned a quarter about Y at
        // one second: its corner at +X goes to -Z.
        let p = at(1.5, 1, [12.0, 8.0, 0.0]);
        assert!(
            (p[0] - 10.0).abs() < 1e-4 && (p[1] - 11.75).abs() < 1e-4,
            "{p:?}"
        );
        assert!((p[2] + 2.0).abs() < 1e-4, "{p:?}");
        // The green box does not move.
        assert!(animation.poses(Some(1.0))[2].is_none());
        let (lo, hi) = animation.bounds;
        assert_eq!((lo[1], hi[1]), (-2.0, 15.0));
    }
}
