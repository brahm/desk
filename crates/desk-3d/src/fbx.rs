// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! FBX, the format of the 3D models of Microsoft Office, in its binary
//! form and its text one, of version 7 (D-033).
//!
//! A file is a tree of nodes, each with a name, a list of values and the
//! nodes under it. A reader takes from it the models (`Model`) with their
//! places, and the meshes (`Geometry`) and materials (`Material`) that
//! the connections (`Connections`) give each: the polygons as fans of
//! triangles, with the normals, texture coordinates and colors of their
//! corners, the diffuse color and opacity of the materials and the
//! picture of their diffuse color, inside the file or beside it. The
//! first animation stack moves the models by curves of their
//! translation, rotation and scale, from key to key in a line, as the
//! cubic and constant keys of FBX are taken too. A file with Z up is
//! turned to Y up; other axes of `GlobalSettings` are not read. Skins,
//! blend shapes, cameras and lights are left out (B-082).

use std::collections::{HashMap, HashSet};
use std::io::Read as _;

use crate::animation::{Animation, Joint, Local, MeshJoint, Op, OpKind};
use crate::matrix::{IDENTITY, Matrix, invert, mul, normal, point};
use crate::{Error, Material, Mesh, Result, Scene, limits, malformed, texture};

const MAGIC: &[u8] = b"Kaydara FBX Binary  \0";

/// The ticks of a second in the times of keys.
const TICKS: f64 = 46_186_158_000.0;

/// The most numbers of the arrays of a file, in all.
const NUMBERS: usize = 64 * 1024 * 1024;

/// Whether the bytes are an FBX, binary or text.
pub(crate) fn is_fbx(bytes: &[u8]) -> bool {
    if bytes.starts_with(MAGIC) {
        return true;
    }
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = text
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(0);
    text.get(start..).is_some_and(|t| t.starts_with(b"; FBX"))
}

/// A value of a node.
#[derive(Clone, Debug, PartialEq)]
enum Prop {
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
}

impl Prop {
    fn int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Float(f) if f.fract() == 0.0 && f.abs() < 9e15 => Some(*f as i64),
            _ => None,
        }
    }

    fn float(&self) -> Option<f64> {
        match self {
            Self::Int(i) => Some(*i as f64),
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }

    fn floats(&self) -> Option<Vec<f64>> {
        match self {
            Self::Floats(f) => Some(f.clone()),
            Self::Ints(i) => Some(i.iter().map(|v| *v as f64).collect()),
            _ => None,
        }
    }

    fn ints(&self) -> Option<Vec<i64>> {
        match self {
            Self::Ints(i) => Some(i.clone()),
            Self::Floats(f) => f
                .iter()
                .map(|v| (v.fract() == 0.0 && v.abs() < 9e15).then_some(*v as i64))
                .collect(),
            _ => None,
        }
    }
}

/// A node of the tree.
#[derive(Clone, Debug, Default, PartialEq)]
struct Node {
    name: String,
    props: Vec<Prop>,
    children: Vec<Node>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    fn prop(&self, i: usize) -> Option<&Prop> {
        self.props.get(i)
    }

    /// The first value of the child of a name.
    fn value(&self, name: &str) -> Option<&Prop> {
        self.child(name)?.prop(0)
    }

    /// The values of a property of `Properties70`, after its name and
    /// its types.
    fn property(&self, name: &str) -> Option<&[Prop]> {
        let p = self
            .child("Properties70")?
            .all("P")
            .find(|p| p.prop(0).and_then(Prop::text) == Some(name))?;
        p.props.get(4..)
    }

    fn vector(&self, name: &str) -> Option<[f64; 3]> {
        let v = self.property(name)?;
        let n = |i: usize| v.get(i).and_then(Prop::float).filter(|f| f.is_finite());
        Some([n(0)?, n(1)?, n(2)?])
    }

    fn number(&self, name: &str) -> Option<f64> {
        self.property(name)?
            .first()
            .and_then(Prop::float)
            .filter(|f| f.is_finite())
    }
}

/// Counts what is read, against the limits.
struct Budget {
    nodes: usize,
    numbers: usize,
}

impl Budget {
    fn node(&mut self) -> Result<()> {
        self.nodes += 1;
        if self.nodes > limits::PRIMS * 4 {
            return Err(Error::TooLarge);
        }
        Ok(())
    }

    fn numbers(&mut self, n: usize) -> Result<()> {
        self.numbers = self.numbers.saturating_add(n);
        if self.numbers > NUMBERS {
            return Err(Error::TooLarge);
        }
        Ok(())
    }
}

/// The binary form.
struct Binary<'a> {
    bytes: &'a [u8],
    at: usize,
    /// Whether the offsets are of 64 bits, from version 7500.
    wide: bool,
    budget: Budget,
}

impl Binary<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| malformed("FBX cut short"))?;
        let out = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| malformed("FBX cut short"))?;
        self.at = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32> {
        self.array().map(u32::from_le_bytes)
    }

    fn offset(&mut self) -> Result<u64> {
        if self.wide {
            self.array().map(u64::from_le_bytes)
        } else {
            self.u32().map(u64::from)
        }
    }

    fn node(&mut self, depth: usize) -> Result<Option<Node>> {
        if depth > limits::DEPTH {
            return Err(Error::TooLarge);
        }
        let end = self.offset()?;
        let count = self.offset()?;
        let _length = self.offset()?;
        let name_length = usize::from(self.array::<1>()?[0]);
        if end == 0 && count == 0 && name_length == 0 {
            return Ok(None);
        }
        let end = usize::try_from(end)
            .ok()
            .filter(|e| *e > self.at && *e <= self.bytes.len())
            .ok_or_else(|| malformed("a node of the FBX past its end"))?;
        self.budget.node()?;
        let name = String::from_utf8_lossy(self.take(name_length)?).into_owned();
        let mut node = Node {
            name,
            ..Node::default()
        };
        for _ in 0..count {
            if self.at >= end {
                return Err(malformed("the values of a node of the FBX past its end"));
            }
            node.props.push(self.prop()?);
        }
        while self.at < end {
            match self.node(depth + 1)? {
                Some(child) => node.children.push(child),
                None => break,
            }
        }
        if self.at > end {
            return Err(malformed("a node of the FBX past its end"));
        }
        self.at = end;
        Ok(Some(node))
    }

    fn prop(&mut self) -> Result<Prop> {
        let [kind] = self.array::<1>()?;
        Ok(match kind {
            b'Y' => Prop::Int(i64::from(i16::from_le_bytes(self.array()?))),
            b'C' => Prop::Int(i64::from(self.array::<1>()?[0])),
            b'I' => Prop::Int(i64::from(i32::from_le_bytes(self.array()?))),
            b'L' => Prop::Int(i64::from_le_bytes(self.array()?)),
            b'F' => Prop::Float(f64::from(f32::from_le_bytes(self.array()?))),
            b'D' => Prop::Float(f64::from_le_bytes(self.array()?)),
            b'S' | b'R' => {
                let n = self.u32()? as usize;
                let data = self.take(n)?.to_vec();
                if kind == b'S' {
                    Prop::Text(String::from_utf8_lossy(&data).into_owned())
                } else {
                    Prop::Bytes(data)
                }
            }
            b'f' | b'd' | b'l' | b'i' | b'b' => {
                let count = self.u32()? as usize;
                let encoding = self.u32()?;
                let stored = self.u32()? as usize;
                let size = match kind {
                    b'd' | b'l' => 8,
                    b'f' | b'i' => 4,
                    _ => 1,
                };
                self.budget.numbers(count)?;
                let length = count.checked_mul(size).ok_or(Error::TooLarge)?;
                let raw = self.take(stored)?;
                let data = match encoding {
                    0 => raw.to_vec(),
                    1 => {
                        let mut out = Vec::with_capacity(length.min(1 << 24));
                        flate2::read::ZlibDecoder::new(raw)
                            .take(length as u64)
                            .read_to_end(&mut out)
                            .map_err(|_| malformed("a broken array of the FBX"))?;
                        out
                    }
                    _ => return Err(malformed("an array of the FBX of an unknown coding")),
                };
                if data.len() != length {
                    return Err(malformed("an array of the FBX of the wrong length"));
                }
                match kind {
                    b'd' => Prop::Floats(
                        data.as_chunks::<8>()
                            .0
                            .iter()
                            .map(|b| f64::from_le_bytes(*b))
                            .collect(),
                    ),
                    b'f' => Prop::Floats(
                        data.as_chunks::<4>()
                            .0
                            .iter()
                            .map(|b| f64::from(f32::from_le_bytes(*b)))
                            .collect(),
                    ),
                    b'l' => Prop::Ints(
                        data.as_chunks::<8>()
                            .0
                            .iter()
                            .map(|b| i64::from_le_bytes(*b))
                            .collect(),
                    ),
                    b'i' => Prop::Ints(
                        data.as_chunks::<4>()
                            .0
                            .iter()
                            .map(|b| i64::from(i32::from_le_bytes(*b)))
                            .collect(),
                    ),
                    _ => Prop::Ints(data.iter().map(|b| i64::from(*b)).collect()),
                }
            }
            _ => return Err(malformed("a value of the FBX of an unknown type")),
        })
    }
}

fn read_binary(bytes: &[u8]) -> Result<Node> {
    let version = bytes
        .get(23..27)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| malformed("FBX cut short"))?;
    if version < 7000 {
        return Err(Error::Unknown);
    }
    let mut reader = Binary {
        bytes,
        at: 27,
        wide: version >= 7500,
        budget: Budget {
            nodes: 0,
            numbers: 0,
        },
    };
    let mut root = Node::default();
    while reader.at < bytes.len() {
        match reader.node(0)? {
            Some(node) => root.children.push(node),
            None => break,
        }
    }
    Ok(root)
}

/// The text form.
struct Text<'a> {
    bytes: &'a [u8],
    at: usize,
    budget: Budget,
}

impl Text<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Skips spaces, and the ends of lines when `lines`, and comments.
    fn skip(&mut self, lines: bool) {
        while let Some(b) = self.peek() {
            match b {
                b';' => {
                    while self.peek().is_some_and(|b| b != b'\n') {
                        self.at += 1;
                    }
                }
                b'\n' | b'\r' if !lines => return,
                b if b.is_ascii_whitespace() => self.at += 1,
                _ => return,
            }
        }
    }

    fn word(&mut self) -> &str {
        let start = self.at;
        while self.peek().is_some_and(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'|' | b'-' | b'+' | b'.')
        }) {
            self.at += 1;
        }
        std::str::from_utf8(self.bytes.get(start..self.at).unwrap_or_default()).unwrap_or("")
    }

    fn nodes(&mut self, depth: usize) -> Result<Vec<Node>> {
        if depth > limits::DEPTH {
            return Err(Error::TooLarge);
        }
        let mut nodes = Vec::new();
        loop {
            self.skip(true);
            match self.peek() {
                None => return Ok(nodes),
                Some(b'}') => {
                    if depth == 0 {
                        return Err(malformed("an FBX with a } too many"));
                    }
                    self.at += 1;
                    return Ok(nodes);
                }
                _ => {}
            }
            let name = self.word().to_owned();
            self.skip(false);
            if name.is_empty() || self.peek() != Some(b':') {
                return Err(malformed("a node of the FBX with no name"));
            }
            self.at += 1;
            self.budget.node()?;
            let mut node = Node {
                name,
                ..Node::default()
            };
            loop {
                self.skip(false);
                match self.peek() {
                    None | Some(b'\n' | b'\r') => break,
                    Some(b'}') => break,
                    Some(b',') => {
                        self.at += 1;
                        // A list goes on after a comma, on the next line.
                        self.skip(true);
                    }
                    Some(b'{') => {
                        self.at += 1;
                        node.children = self.nodes(depth + 1)?;
                        break;
                    }
                    Some(b'"') => {
                        self.at += 1;
                        let start = self.at;
                        while self.peek().is_some_and(|b| b != b'"') {
                            self.at += 1;
                        }
                        let text = self.bytes.get(start..self.at).unwrap_or_default();
                        node.props
                            .push(Prop::Text(String::from_utf8_lossy(text).into_owned()));
                        if self.peek().is_none() {
                            return Err(malformed("a string of the FBX with no end"));
                        }
                        self.at += 1;
                    }
                    Some(b'*') => {
                        self.at += 1;
                        let _count = self.word();
                        self.skip(true);
                        if self.peek() != Some(b'{') {
                            return Err(malformed("an array of the FBX with no {"));
                        }
                        self.at += 1;
                        self.skip(true);
                        if self.word() != "a" {
                            return Err(malformed("an array of the FBX with no a:"));
                        }
                        self.skip(true);
                        if self.peek() == Some(b':') {
                            self.at += 1;
                        }
                        let mut numbers = Vec::new();
                        loop {
                            self.skip(true);
                            match self.peek() {
                                Some(b',') => self.at += 1,
                                Some(b'}') => {
                                    self.at += 1;
                                    break;
                                }
                                None => return Err(malformed("an array of the FBX with no end")),
                                _ => {
                                    let w = self.word();
                                    let v: f64 =
                                        w.parse().map_err(|_| malformed("a number of the FBX"))?;
                                    numbers.push(v);
                                    self.budget.numbers(1)?;
                                }
                            }
                        }
                        node.props.push(Prop::Floats(numbers));
                    }
                    _ => {
                        let w = self.word().to_owned();
                        if w.is_empty() {
                            return Err(malformed("a value of the FBX that is none"));
                        }
                        node.props.push(if let Ok(i) = w.parse::<i64>() {
                            Prop::Int(i)
                        } else if let Ok(f) = w.parse::<f64>() {
                            Prop::Float(f)
                        } else {
                            Prop::Text(w)
                        });
                    }
                }
            }
            nodes.push(node);
        }
    }
}

fn read_text(bytes: &[u8]) -> Result<Node> {
    let mut reader = Text {
        bytes,
        at: 0,
        budget: Budget {
            nodes: 0,
            numbers: 0,
        },
    };
    let root = Node {
        children: reader.nodes(0)?,
        ..Node::default()
    };
    let version = root
        .child("FBXHeaderExtension")
        .and_then(|h| h.value("FBXVersion"))
        .and_then(Prop::int)
        .unwrap_or(0);
    if version < 7000 {
        return Err(Error::Unknown);
    }
    Ok(root)
}

/// The matrices of the place of a model in its parent, as ops from the
/// outside in: its translation, its rotation about its pivot with the
/// rotations before and after it, and its scale about its pivot.
fn model_ops(model: &Node) -> Vec<Op> {
    let v = |name: &str, default: [f64; 3]| model.vector(name).unwrap_or(default).to_vec();
    let order = match model.number("RotationOrder").map(|o| o as i64) {
        Some(1) => vec![0, 2, 1],
        Some(2) => vec![1, 2, 0],
        Some(3) => vec![1, 0, 2],
        Some(4) => vec![2, 0, 1],
        Some(5) => vec![2, 1, 0],
        _ => vec![0, 1, 2],
    };
    let op = |kind: OpKind, inverse: bool, rest: Vec<f64>| Op {
        kind,
        inverse,
        rest,
        times: Vec::new(),
        values: Vec::new(),
    };
    let xyz = || OpKind::RotateAxes(vec![0, 1, 2]);
    let zero = [0.0; 3];
    vec![
        op(OpKind::Translate, false, v("Lcl Translation", zero)),
        op(OpKind::Translate, false, v("RotationOffset", zero)),
        op(OpKind::Translate, false, v("RotationPivot", zero)),
        op(xyz(), false, v("PreRotation", zero)),
        op(OpKind::RotateAxes(order), false, v("Lcl Rotation", zero)),
        op(xyz(), true, v("PostRotation", zero)),
        op(OpKind::Translate, true, v("RotationPivot", zero)),
        op(OpKind::Translate, false, v("ScalingOffset", zero)),
        op(OpKind::Translate, false, v("ScalingPivot", zero)),
        op(OpKind::Scale, false, v("Lcl Scaling", [1.0; 3])),
        op(OpKind::Translate, true, v("ScalingPivot", zero)),
    ]
}

/// Which of the ops of [`model_ops`] a property of the curves moves.
fn animated_op(property: &str) -> Option<usize> {
    match property {
        "Lcl Translation" => Some(0),
        "Lcl Rotation" => Some(4),
        "Lcl Scaling" => Some(9),
        _ => None,
    }
}

/// The matrix of ops at rest.
fn ops_matrix(ops: &[Op]) -> Matrix {
    let mut m = IDENTITY;
    for op in ops {
        if let Some(o) = op.kind.matrix(&op.rest) {
            let o = if op.inverse {
                invert(&o).unwrap_or(IDENTITY)
            } else {
                o
            };
            m = mul(&o, &m);
        }
    }
    m
}

/// The place of the geometry of a model in the model.
fn geometric(model: &Node) -> Matrix {
    let zero = [0.0; 3];
    let t = model.vector("GeometricTranslation").unwrap_or(zero);
    let r = model.vector("GeometricRotation").unwrap_or(zero);
    let s = model.vector("GeometricScaling").unwrap_or([1.0; 3]);
    let ops = [
        (OpKind::Translate, t.to_vec()),
        (OpKind::RotateAxes(vec![0, 1, 2]), r.to_vec()),
        (OpKind::Scale, s.to_vec()),
    ];
    let mut m = IDENTITY;
    for (kind, value) in ops {
        if let Some(o) = kind.matrix(&value) {
            m = mul(&o, &m);
        }
    }
    m
}

/// How a layer of a geometry goes to the corners of its polygons.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mapping {
    PolygonVertex,
    Vertex,
    Polygon,
    All,
}

/// A layer of values of a geometry: normals, coordinates or colors.
struct Layer {
    values: Vec<f64>,
    width: usize,
    indices: Option<Vec<i64>>,
    mapping: Mapping,
}

impl Layer {
    fn read(
        geometry: &Node,
        element: &str,
        values: &str,
        indices: &str,
        width: usize,
    ) -> Option<Self> {
        let e = geometry.child(element)?;
        let mapping = match e.value("MappingInformationType").and_then(Prop::text)? {
            "ByPolygonVertex" => Mapping::PolygonVertex,
            "ByVertex" | "ByVertice" | "ByControlPoint" => Mapping::Vertex,
            "ByPolygon" => Mapping::Polygon,
            "AllSame" => Mapping::All,
            _ => return None,
        };
        let indexed =
            e.value("ReferenceInformationType").and_then(Prop::text) == Some("IndexToDirect");
        Some(Self {
            values: e.value(values)?.floats()?,
            width,
            indices: if indexed {
                Some(e.value(indices)?.ints()?)
            } else {
                None
            },
            mapping,
        })
    }

    /// The value at a corner: polygon-vertex `k` of polygon `p`, of
    /// control point `v`.
    fn at(&self, k: usize, p: usize, v: usize) -> Option<&[f64]> {
        let i = match self.mapping {
            Mapping::PolygonVertex => k,
            Mapping::Vertex => v,
            Mapping::Polygon => p,
            Mapping::All => 0,
        };
        let i = match &self.indices {
            Some(indices) => usize::try_from(*indices.get(i)?).ok()?,
            None => i,
        };
        let from = i.checked_mul(self.width)?;
        self.values.get(from..from.checked_add(self.width)?)
    }
}

/// The materials of a geometry's polygons.
fn polygon_materials(geometry: &Node) -> (Mapping, Vec<i64>) {
    let Some(e) = geometry.child("LayerElementMaterial") else {
        return (Mapping::All, vec![0]);
    };
    let ints = e
        .value("Materials")
        .and_then(Prop::ints)
        .unwrap_or_default();
    match e.value("MappingInformationType").and_then(Prop::text) {
        Some("ByPolygon") => (Mapping::Polygon, ints),
        _ => (Mapping::All, ints.into_iter().take(1).collect()),
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

/// The scene being made.
struct Builder<'a> {
    objects: HashMap<i64, &'a Node>,
    /// The objects connected to each, in order: what hangs from it.
    under: HashMap<i64, Vec<(i64, Option<String>)>>,
    /// What each object hangs from, with the property it is connected to.
    over: HashMap<i64, Vec<(i64, Option<String>)>>,
    /// The first animation stack of the file.
    stack: Option<i64>,
    files: &'a mut dyn FnMut(&str) -> Option<Vec<u8>>,
    scene: Scene,
    pictures: Vec<Option<Vec<u8>>>,
    looks: HashMap<i64, Material>,
    textures: HashMap<i64, Option<usize>>,
    joints: Vec<Joint>,
    joint_of: HashMap<i64, usize>,
    mesh_joints: Vec<MeshJoint>,
    triangles: usize,
    corners: usize,
}

impl Builder<'_> {
    fn under(&self, id: i64, kind: &str) -> Vec<(i64, Option<String>)> {
        self.under
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|(c, _)| self.objects.get(c).is_some_and(|n| n.name == kind))
            .cloned()
            .collect()
    }

    /// The picture of a texture: inside the file, or beside it.
    fn texture(&mut self, id: i64) -> Option<usize> {
        if let Some(t) = self.textures.get(&id) {
            return *t;
        }
        let made = self.read_texture(id);
        self.textures.insert(id, made);
        made
    }

    fn read_texture(&mut self, id: i64) -> Option<usize> {
        if self.scene.textures.len() >= limits::TEXTURES {
            return None;
        }
        let node = *self.objects.get(&id)?;
        let mut bytes = None;
        for (video, _) in self.under(id, "Video") {
            // Bytes in a binary file; base 64 in a text one, in as many
            // strings as it takes.
            let content = self.objects.get(&video).and_then(|v| v.child("Content"));
            let data = match content.map(|c| c.props.as_slice()) {
                Some([Prop::Bytes(b), ..]) => Some(b.clone()),
                Some(props @ [Prop::Text(_), ..]) => {
                    let text: String = props.iter().filter_map(Prop::text).collect();
                    crate::gltf::base64(&text)
                }
                _ => None,
            };
            if let Some(b) = data.filter(|b| !b.is_empty()) {
                bytes = Some(b);
                break;
            }
        }
        if bytes.is_none() {
            for key in ["RelativeFilename", "FileName"] {
                let Some(name) = node.value(key).and_then(Prop::text) else {
                    continue;
                };
                let path = name.replace('\\', "/");
                let last = path.rsplit('/').next().unwrap_or(&path).to_owned();
                bytes = crate::obj::clean(&path)
                    .and_then(|p| (self.files)(&p))
                    .or_else(|| crate::obj::clean(&last).and_then(|p| (self.files)(&p)));
                if bytes.is_some() {
                    break;
                }
            }
        }
        let bytes = bytes?;
        let (t, shrunk) = texture::decode_shrunk(&bytes)?;
        self.scene.textures.push(t);
        self.pictures.push((!shrunk).then_some(bytes));
        Some(self.scene.textures.len() - 1)
    }

    fn material(&mut self, id: i64) -> Material {
        if let Some(m) = self.looks.get(&id) {
            return m.clone();
        }
        let Some(node) = self.objects.get(&id).copied() else {
            return Material::default();
        };
        let mut material = Material::default();
        if let Some(c) = node
            .vector("DiffuseColor")
            .or_else(|| node.vector("Diffuse"))
        {
            let f = node.number("DiffuseFactor").unwrap_or(1.0);
            for (m, c) in material.color.iter_mut().zip(c) {
                *m = (c * f).clamp(0.0, 1.0) as f32;
            }
        }
        let opacity = match (node.number("Opacity"), node.number("TransparencyFactor")) {
            (Some(o), _) => o,
            (None, Some(t)) => {
                let color = node
                    .vector("TransparentColor")
                    .map_or(1.0, |c| (c[0] + c[1] + c[2]) / 3.0);
                1.0 - t * color
            }
            _ => 1.0,
        };
        material.color[3] = opacity.clamp(0.0, 1.0) as f32;
        material.translucent = material.color[3] < 1.0;
        let textures: Vec<i64> = self
            .under(id, "Texture")
            .into_iter()
            .filter(|(_, p)| {
                p.as_deref()
                    .is_none_or(|p| p.contains("DiffuseColor") || p.contains("baseColor"))
            })
            .map(|(t, _)| t)
            .collect();
        for t in textures {
            if let Some(t) = self.texture(t) {
                // The alpha of a picture of the diffuse color is not how
                // opaque the material is.
                if let Some(texture) = self.scene.textures.get_mut(t) {
                    let mut opaque = true;
                    for [_, _, _, a] in texture.rgba.as_chunks_mut::<4>().0 {
                        opaque &= *a == 255;
                        *a = 255;
                    }
                    if !opaque && let Some(p) = self.pictures.get_mut(t) {
                        *p = None;
                    }
                }
                material.texture = Some(t);
                material.color[..3].copy_from_slice(&[1.0; 3]);
                break;
            }
        }
        self.looks.insert(id, material.clone());
        material
    }

    /// The meshes of a geometry of a model, by the materials of its
    /// polygons.
    fn geometry(
        &mut self,
        geometry: &Node,
        world: &Matrix,
        materials: &[i64],
        joint: usize,
    ) -> Result<()> {
        let Some(points) = geometry.value("Vertices").and_then(Prop::floats) else {
            return Ok(());
        };
        let Some(order) = geometry.value("PolygonVertexIndex").and_then(Prop::ints) else {
            return Ok(());
        };
        let count = points.len() / 3;
        let inverse = invert(world);
        let normals = Layer::read(geometry, "LayerElementNormal", "Normals", "NormalsIndex", 3);
        let uvs = Layer::read(geometry, "LayerElementUV", "UV", "UVIndex", 2);
        let colors = Layer::read(geometry, "LayerElementColor", "Colors", "ColorIndex", 4);
        let (by, which) = polygon_materials(geometry);
        // The polygons, as their first polygon-vertex and their count.
        let mut polygons = Vec::new();
        let mut start = 0;
        for (k, v) in order.iter().enumerate() {
            if *v < 0 {
                polygons.push((start, k + 1 - start));
                start = k + 1;
            }
        }
        let vertex = |k: usize| -> Option<usize> {
            let v = *order.get(k)?;
            let v = usize::try_from(if v < 0 { !v } else { v }).ok()?;
            (v < count).then_some(v)
        };
        let mut meshes: HashMap<usize, Mesh> = HashMap::new();
        let mut slots: Vec<usize> = Vec::new();
        for (p, &(s, n)) in polygons.iter().enumerate() {
            let slot = match by {
                Mapping::Polygon => which.get(p).copied().unwrap_or(0),
                _ => which.first().copied().unwrap_or(0),
            };
            // A slot with no material shares one mesh of the default.
            let slot = usize::try_from(slot)
                .ok()
                .filter(|s| *s < materials.len())
                .unwrap_or(materials.len());
            let corners: Option<Vec<usize>> = (s..s + n).map(vertex).collect();
            let Some(corners) = corners else { continue };
            if corners.len() < 3 {
                continue;
            }
            // Its corners placed; a polygon with one that is not finite
            // there is left out.
            let placed: Vec<[f32; 3]> = corners
                .iter()
                .map(|&v| {
                    let at = |i: usize| points.get(v * 3 + i).copied().unwrap_or(0.0) as f32;
                    point(world, [at(0), at(1), at(2)])
                })
                .collect();
            if placed.iter().flatten().any(|c| !c.is_finite()) {
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(entry) = meshes.entry(slot) {
                let material = match materials.get(slot) {
                    Some(&id) => self.material(id),
                    None => Material::default(),
                };
                slots.push(slot);
                entry.insert(Mesh {
                    material,
                    ..Mesh::default()
                });
            }
            let Some(mesh) = meshes.get_mut(&slot) else {
                continue;
            };
            let base = mesh.positions.len() as u32;
            self.corners += n;
            self.triangles += n - 2;
            if self.corners > limits::CORNERS || self.triangles > limits::TRIANGLES {
                return Err(Error::TooLarge);
            }
            for ((j, &v), position) in corners.iter().enumerate().zip(placed) {
                let k = s + j;
                mesh.positions.push(position);
                if let Some(layer) = &normals
                    && let (Some(nv), Some(inv)) = (layer.at(k, p, v), inverse.as_ref())
                {
                    let get = |i: usize| nv.get(i).copied().unwrap_or(0.0) as f32;
                    let n = unit(normal(inv, [get(0), get(1), get(2)]));
                    // One normal not finite leaves the mesh with none.
                    if n.iter().all(|c| c.is_finite()) {
                        mesh.normals.push(n);
                    }
                }
                if let Some(layer) = &uvs {
                    let uv = layer.at(k, p, v).unwrap_or(&[0.0, 0.0]);
                    let get = |i: usize| {
                        Some(uv.get(i).copied().unwrap_or(0.0) as f32)
                            .filter(|c| c.is_finite())
                            .unwrap_or(0.0)
                    };
                    mesh.uvs.push([get(0), get(1)]);
                }
                if let Some(layer) = &colors {
                    let c = layer.at(k, p, v).unwrap_or(&[1.0, 1.0, 1.0]);
                    mesh.colors.push([0, 1, 2].map(|i| {
                        let c = c.get(i).copied().unwrap_or(1.0) as f32;
                        if c.is_finite() {
                            c.clamp(0.0, 1.0)
                        } else {
                            1.0
                        }
                    }));
                }
            }
            for j in 1..n as u32 - 1 {
                mesh.triangles.push([base, base + j, base + j + 1]);
            }
        }
        for slot in slots {
            let Some(mut mesh) = meshes.remove(&slot) else {
                continue;
            };
            let count = mesh.positions.len();
            if mesh.normals.len() != count {
                mesh.normals.clear();
            }
            if mesh.colors.len() != count {
                mesh.colors.clear();
            }
            if mesh.material.texture.is_none() || mesh.uvs.len() != count {
                mesh.uvs.clear();
                mesh.material.texture = None;
            }
            if !mesh.triangles.is_empty() {
                self.scene.meshes.push(mesh);
                self.mesh_joints.push(Some((joint, false)));
            }
        }
        Ok(())
    }

    /// A model and those under it.
    fn model(
        &mut self,
        id: i64,
        parent: usize,
        above: &Matrix,
        depth: usize,
        seen: &mut HashSet<i64>,
    ) -> Result<()> {
        if depth > limits::DEPTH || self.joints.len() >= limits::PRIMS {
            return Err(Error::TooLarge);
        }
        if !seen.insert(id) {
            return Ok(());
        }
        let Some(model) = self.objects.get(&id).copied() else {
            return Ok(());
        };
        let ops = model_ops(model);
        let local = ops_matrix(&ops);
        let world = mul(&local, above);
        let joint = self.joints.len();
        self.joints.push(Joint {
            parent: Some(parent),
            rest: Local::Ops(ops),
        });
        self.joint_of.insert(id, joint);
        let materials: Vec<i64> = self
            .under(id, "Material")
            .into_iter()
            .map(|(m, _)| m)
            .collect();
        let place = mul(&geometric(model), &world);
        for (g, _) in self.under(id, "Geometry") {
            if let Some(geometry) = self.objects.get(&g).copied() {
                self.geometry(geometry, &place, &materials, joint)?;
            }
        }
        for (child, _) in self.under(id, "Model") {
            self.model(child, joint, &world, depth + 1, seen)?;
        }
        Ok(())
    }

    /// The curves of the first animation stack, onto the ops of the
    /// joints of the models they move.
    fn animate(&mut self) {
        let Some((stack, node)) = self
            .stack
            .and_then(|id| Some((id, *self.objects.get(&id)?)))
        else {
            return;
        };
        let start = node
            .property("LocalStart")
            .and_then(|v| v.first())
            .and_then(Prop::int)
            .unwrap_or(0) as f64;
        let mut keys = 0usize;
        // The models, the property each curve node moves, and its curves.
        let mut moves: Vec<(i64, usize, i64)> = Vec::new();
        for (layer, _) in self.under(stack, "AnimationLayer") {
            for (curve_node, _) in self.under(layer, "AnimationCurveNode") {
                for (parent, property) in self.parents(curve_node) {
                    if let Some(op) = property.as_deref().and_then(animated_op)
                        && self.joint_of.contains_key(&parent)
                    {
                        moves.push((parent, op, curve_node));
                    }
                }
            }
        }
        for (model, op, curve_node) in moves {
            let mut curves: [Option<(Vec<f64>, Vec<f64>)>; 3] = [None, None, None];
            for (curve, property) in self.under(curve_node, "AnimationCurve") {
                let axis = match property.as_deref() {
                    Some("d|X") => 0,
                    Some("d|Y") => 1,
                    Some("d|Z") => 2,
                    _ => continue,
                };
                let Some(c) = self.objects.get(&curve) else {
                    continue;
                };
                let (Some(times), Some(values)) = (
                    c.value("KeyTime").and_then(Prop::ints),
                    c.value("KeyValueFloat").and_then(Prop::floats),
                ) else {
                    continue;
                };
                if times.len() != values.len()
                    || times.is_empty()
                    || times.array_windows::<2>().any(|[a, b]| b < a)
                    || values.iter().any(|v| !v.is_finite())
                {
                    continue;
                }
                keys += times.len();
                if keys > limits::ANIMATION_KEYS {
                    return;
                }
                let seconds = times.iter().map(|t| (*t as f64 - start) / TICKS).collect();
                if let Some(slot) = curves.get_mut(axis) {
                    *slot = Some((seconds, values));
                }
            }
            if curves.iter().all(Option::is_none) {
                continue;
            }
            let Some(&joint) = self.joint_of.get(&model) else {
                continue;
            };
            let Some(Joint {
                rest: Local::Ops(ops),
                ..
            }) = self.joints.get_mut(joint)
            else {
                continue;
            };
            let Some(op) = ops.get_mut(op) else { continue };
            let mut times: Vec<f64> = curves
                .iter()
                .flatten()
                .flat_map(|(t, _)| t.iter().copied())
                .collect();
            times.sort_by(f64::total_cmp);
            times.dedup();
            let value_at = |axis: usize, time: f64| -> Option<f64> {
                let (t, v) = curves.get(axis)?.as_ref()?;
                let next = t.partition_point(|k| *k <= time);
                let (Some(k), Some(b)) = (next.checked_sub(1), v.get(next)) else {
                    return if next == 0 {
                        v.first().copied()
                    } else {
                        v.last().copied()
                    };
                };
                let (a, t0, t1) = (*v.get(k)?, *t.get(k)?, *t.get(next)?);
                let s = if t1 > t0 {
                    (time - t0) / (t1 - t0)
                } else {
                    0.0
                };
                Some(a + (b - a) * s)
            };
            op.times = times.iter().map(|t| *t as f32).collect();
            op.values = times
                .iter()
                .map(|&t| {
                    (0..3)
                        .map(|axis| {
                            value_at(axis, t).unwrap_or(op.rest.get(axis).copied().unwrap_or(0.0))
                        })
                        .collect()
                })
                .collect();
        }
    }

    /// What an object hangs from, with the property it is connected to.
    fn parents(&self, id: i64) -> Vec<(i64, Option<String>)> {
        self.over.get(&id).cloned().unwrap_or_default()
    }
}

/// A model read from an FBX, with the bytes of the pictures of its
/// textures as they were, where they were not changed.
pub(crate) struct Read {
    pub(crate) scene: Scene,
    pub(crate) pictures: Vec<Option<Vec<u8>>>,
}

/// Reads an FBX; `files` gives the bytes of a file beside it by its
/// relative path.
pub(crate) fn read(bytes: &[u8], files: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Result<Read> {
    let root = if bytes.starts_with(MAGIC) {
        read_binary(bytes)?
    } else {
        read_text(bytes)?
    };
    let mut objects = HashMap::new();
    let mut stack = None;
    if let Some(o) = root.child("Objects") {
        for node in &o.children {
            if let Some(id) = node.prop(0).and_then(Prop::int) {
                objects.insert(id, node);
                if node.name == "AnimationStack" && stack.is_none() {
                    stack = Some(id);
                }
            }
        }
    }
    let mut under: HashMap<i64, Vec<(i64, Option<String>)>> = HashMap::new();
    let mut over: HashMap<i64, Vec<(i64, Option<String>)>> = HashMap::new();
    if let Some(c) = root.child("Connections") {
        for link in c.all("C") {
            let (Some(child), Some(parent)) = (
                link.prop(1).and_then(Prop::int),
                link.prop(2).and_then(Prop::int),
            ) else {
                continue;
            };
            let property = link.prop(3).and_then(Prop::text).map(str::to_owned);
            over.entry(child)
                .or_default()
                .push((parent, property.clone()));
            under.entry(parent).or_default().push((child, property));
        }
    }
    // The stage, turned up to Y up when its axis up is Z.
    let settings = root.child("GlobalSettings");
    let up = settings.and_then(|s| s.number("UpAxis")).unwrap_or(1.0) as i64;
    let sign = settings
        .and_then(|s| s.number("UpAxisSign"))
        .unwrap_or(1.0)
        .signum();
    let stage: Matrix = if up == 2 {
        [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 0.0, -sign, 0.0, //
            0.0, sign, 0.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ]
    } else {
        IDENTITY
    };
    let mut builder = Builder {
        objects,
        under,
        over,
        stack,
        files,
        scene: Scene::default(),
        pictures: Vec::new(),
        looks: HashMap::new(),
        textures: HashMap::new(),
        joints: vec![Joint {
            parent: None,
            rest: Local::Matrix(stage),
        }],
        joint_of: HashMap::new(),
        mesh_joints: Vec::new(),
        triangles: 0,
        corners: 0,
    };
    let mut seen = HashSet::new();
    for (model, _) in builder.under(0, "Model") {
        builder.model(model, 0, &stage, 0, &mut seen)?;
    }
    builder.animate();
    let mut scene = builder.scene;
    scene.animation = Animation::new(
        builder.joints,
        Vec::new(),
        &builder.mesh_joints,
        &scene.meshes,
    );
    Ok(Read {
        scene,
        pictures: builder.pictures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_an_index_past_any_array_is_none() {
        let layer = Layer {
            values: vec![0.0; 4],
            width: 2,
            indices: Some(vec![i64::MAX, 1]),
            mapping: Mapping::PolygonVertex,
        };
        assert_eq!(layer.at(0, 0, 0), None);
        assert_eq!(layer.at(1, 0, 0), Some(&[0.0, 0.0][..]));
    }
}
