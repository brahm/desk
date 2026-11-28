// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The meshes of a scene, built from its layers.
//!
//! First the layers are composed into one tree of prims: a reference or a
//! payload grafts the prim it names, of the same layer or of another file
//! of the package, under the prim that holds it, whose own opinions are
//! the stronger. The paths inside what is grafted are moved to where it
//! goes, and the asset paths are made paths of the package, so that what
//! follows needs no layer.
//!
//! Then the tree is walked: the transforms of `xformOpOrder` are
//! multiplied down, prims that are inactive, invisible or only guides are
//! left out, and each `Mesh` becomes the meshes of [`Scene`], one per
//! material that its faces or the subsets of its faces are bound to. USD
//! keeps vectors as rows, so a point goes through a matrix as `p · M` and
//! a child's world matrix is its local one times its parent's.

use std::collections::HashMap;
use std::rc::Rc;

use super::{Attribute, Layer, Prim, Value, read_layer, split_property};
use crate::matrix::{IDENTITY, Matrix, at, invert, mul, orient, rotate, scale, translate};
use crate::{Error, Material, Mesh, Result, Scene, limits, texture};

/// The files of a package, by their paths in it.
pub(crate) trait Files {
    /// The bytes of a file, none when there is none of that path.
    fn read(&mut self, path: &str) -> Option<Vec<u8>>;
}

/// The path in the package of an asset that a layer at `base` names:
/// relative to the folder of the layer, or to the package when it starts
/// with a slash.
pub(crate) fn resolve(base: &str, asset: &str) -> String {
    let asset = asset.trim();
    let mut parts: Vec<&str> = Vec::new();
    if !asset.starts_with('/') {
        parts.extend(base.split('/'));
        parts.pop();
    }
    for part in asset.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.retain(|p| !p.is_empty());
    parts.join("/")
}

/// The path of a child of the prim at `path`.
fn join(path: &str, name: &str) -> String {
    if path == "/" {
        format!("/{name}")
    } else {
        format!("{path}/{name}")
    }
}

/// A path that starts with `from`, moved to start with `to`; none when it
/// is not below `from`.
fn rebase(path: &str, from: &str, to: &str) -> Option<String> {
    let rest = path.strip_prefix(from)?;
    (rest.is_empty() || rest.starts_with('/') || rest.starts_with('.'))
        .then(|| format!("{to}{rest}"))
}

/// A path of a relationship or a connection made absolute, from the prim
/// at `at` that holds it.
fn absolute(at: &str, path: &str) -> String {
    if path.starts_with('/') {
        return path.to_owned();
    }
    let mut parts: Vec<&str> = at.split('/').filter(|p| !p.is_empty()).collect();
    let mut rest = path;
    loop {
        if let Some(r) = rest.strip_prefix("../") {
            parts.pop();
            rest = r;
        } else if let Some(r) = rest.strip_prefix("./") {
            rest = r;
        } else {
            break;
        }
    }
    let base = format!("/{}", parts.join("/"));
    if rest == ".." {
        parts.pop();
        return format!("/{}", parts.join("/"));
    }
    if rest.starts_with('.') && !rest.starts_with("..") {
        // A property of the prim itself, as `.outputs:rgb`.
        return format!("{base}{rest}");
    }
    join(&base, rest)
}

/// Composes the layers into one tree of prims.
struct Composer<'a> {
    files: &'a mut dyn Files,
    layers: HashMap<String, Option<Rc<Layer>>>,
    prims: usize,
}

impl Composer<'_> {
    /// The layer of a path, read once; none when the package has no such
    /// file.
    fn layer(&mut self, path: &str) -> Result<Option<Rc<Layer>>> {
        if let Some(layer) = self.layers.get(path) {
            return Ok(layer.clone());
        }
        if self.layers.len() >= limits::PRIMS / 1000 {
            return Err(Error::TooLarge);
        }
        let layer = match self.files.read(path) {
            Some(bytes) => Some(Rc::new(read_layer(&bytes)?)),
            None => None,
        };
        self.layers.insert(path.to_owned(), layer.clone());
        Ok(layer)
    }

    /// The prim at `path` of the layer at `file`, with what its
    /// references bring, `depth` references down.
    fn compose(
        &mut self,
        prim: &Prim,
        path: &str,
        file: &str,
        layer: &Rc<Layer>,
        depth: usize,
    ) -> Result<Prim> {
        self.prims += 1;
        if self.prims > limits::PRIMS || path.matches('/').count() > limits::DEPTH {
            return Err(Error::TooLarge);
        }
        let mut out = Prim::new(prim.name.clone());
        out.type_name.clone_from(&prim.type_name);
        out.active = prim.active;
        for (name, attribute) in &prim.attributes {
            let mut attribute = attribute.clone();
            if let Some(Value::Asset(asset)) = &attribute.value {
                attribute.value = Some(Value::Asset(resolve(file, asset)));
            }
            if let Some(connection) = &attribute.connection {
                attribute.connection = Some(absolute(path, connection));
            }
            out.attributes.insert(name.clone(), attribute);
        }
        for (name, targets) in &prim.relationships {
            let targets = targets.iter().map(|t| absolute(path, t)).collect();
            out.relationships.insert(name.clone(), targets);
        }
        for child in &prim.children {
            let child = self.compose(child, &join(path, &child.name), file, layer, depth)?;
            out.children.push(child);
        }
        for reference in &prim.references {
            // Past the depth, as a layer that brings itself would go on
            // forever, the reference is left out, as OpenUSD leaves out a
            // cycle.
            if depth >= limits::REFERENCE_DEPTH {
                continue;
            }
            let (from_file, from_layer) = if reference.asset.is_empty() {
                (file.to_owned(), Rc::clone(layer))
            } else {
                let at = resolve(file, &reference.asset);
                match self.layer(&at)? {
                    Some(l) => (at, l),
                    None => continue,
                }
            };
            let target = reference
                .prim
                .clone()
                .or_else(|| from_layer.default_prim.clone())
                .or_else(|| from_layer.root.children.first().map(|c| c.name.clone()));
            let Some(target) = target else {
                continue;
            };
            let target = absolute("/", &target);
            let Some(source) = from_layer.root.find(&target) else {
                continue;
            };
            let mut grafted = self.compose(source, &target, &from_file, &from_layer, depth + 1)?;
            move_paths(&mut grafted, &target, path);
            merge(&mut out, grafted);
        }
        Ok(out)
    }
}

/// Moves the paths of the relationships and connections of a tree from
/// below `from` to below `to`.
fn move_paths(prim: &mut Prim, from: &str, to: &str) {
    for attribute in prim.attributes.values_mut() {
        if let Some(moved) = attribute
            .connection
            .as_deref()
            .and_then(|c| rebase(c, from, to))
        {
            attribute.connection = Some(moved);
        }
    }
    for targets in prim.relationships.values_mut() {
        for t in targets.iter_mut() {
            if let Some(moved) = rebase(t, from, to) {
                *t = moved;
            }
        }
    }
    for child in &mut prim.children {
        move_paths(child, from, to);
    }
}

/// Adds to a prim what a weaker one has that it has not.
fn merge(strong: &mut Prim, weak: Prim) {
    if strong.type_name.is_empty() {
        strong.type_name = weak.type_name;
    }
    strong.active &= weak.active;
    for (name, attribute) in weak.attributes {
        strong.attributes.entry(name).or_insert(attribute);
    }
    for (name, targets) in weak.relationships {
        strong.relationships.entry(name).or_insert(targets);
    }
    // By name, so that many children merge in one pass.
    let mut mine: std::collections::HashMap<String, usize> = strong
        .children
        .iter()
        .enumerate()
        .map(|(i, c)| (c.name.clone(), i))
        .collect();
    for child in weak.children {
        match mine
            .get(&child.name)
            .and_then(|&i| strong.children.get_mut(i))
        {
            Some(own) => merge(own, child),
            None => {
                mine.insert(child.name.clone(), strong.children.len());
                strong.children.push(child);
            }
        }
    }
}

/// The matrix of one transform op of a prim, by its name in
/// `xformOpOrder`.
fn op(prim: &Prim, name: &str) -> Option<Matrix> {
    let (inverse, name) = match name.strip_prefix("!invert!") {
        Some(n) => (true, n),
        None => (false, name),
    };
    let value = prim.value(name)?;
    let kind = name.strip_prefix("xformOp:")?;
    let kind = kind.split(':').next().unwrap_or(kind);
    let m = match kind {
        "transform" => match value {
            Value::Matrix(m) => **m,
            Value::Matrices(ms) => *ms.first()?,
            _ => return None,
        },
        "translate" => translate(&value.tuple()?),
        "scale" => scale(&value.tuple()?),
        "orient" => orient(&value.tuple()?),
        "rotateX" | "rotateY" | "rotateZ" => {
            let axis = usize::from(kind.as_bytes().get(6).copied()? - b'X');
            rotate(
                axis,
                value.number().or_else(|| value.tuple()?.first().copied())?,
            )
        }
        _ => {
            // rotateXYZ and its orders: the first axis turns first.
            let axes = kind.strip_prefix("rotate")?.as_bytes();
            if axes.len() != 3 || !axes.iter().all(|a| (b'X'..=b'Z').contains(a)) {
                return None;
            }
            let angles = value.tuple()?;
            let mut m = IDENTITY;
            for a in axes {
                let axis = usize::from(a - b'X');
                m = mul(&m, &rotate(axis, angles.get(axis).copied().unwrap_or(0.0)));
            }
            m
        }
    };
    if inverse { invert(&m) } else { Some(m) }
}

/// The local matrix of a prim, and whether it resets the transforms of
/// its parents.
fn local(prim: &Prim) -> (Matrix, bool) {
    let Some(Value::Texts(order)) = prim.value("xformOpOrder") else {
        return (IDENTITY, false);
    };
    let mut m = IDENTITY;
    let mut reset = false;
    for name in order {
        if name == "!resetXformStack!" {
            m = IDENTITY;
            reset = true;
            continue;
        }
        if let Some(o) = op(prim, name) {
            // The last op turns the point first, so it goes leftmost.
            m = mul(&o, &m);
        }
    }
    if m.iter().all(|v| v.is_finite()) {
        (m, reset)
    } else {
        (IDENTITY, reset)
    }
}

/// How a primvar's values go to the faces.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Interpolation {
    Constant,
    Uniform,
    Vertex,
    FaceVarying,
}

/// A primvar of numbers, checked to cover what it goes to.
struct Primvar<'a> {
    data: &'a [f32],
    dim: usize,
    interpolation: Interpolation,
    indices: Option<&'a [i32]>,
}

impl Primvar<'_> {
    /// The value at a corner: of face `face`, face-vertex `corner`, point
    /// `point`.
    fn at(&self, face: usize, corner: usize, point: usize) -> Option<&[f32]> {
        let i = match self.interpolation {
            Interpolation::Constant => 0,
            Interpolation::Uniform => face,
            Interpolation::Vertex => point,
            Interpolation::FaceVarying => corner,
        };
        let i = match self.indices {
            Some(indices) => usize::try_from(*indices.get(i)?).ok()?,
            None => i,
        };
        self.data.get(i * self.dim..i * self.dim + self.dim)
    }
}

/// The counts a mesh has, to check its primvars against.
struct Counts {
    faces: usize,
    points: usize,
    corners: usize,
}

/// A primvar of a mesh by its name, if it has `dim` numbers a value and
/// enough values; `vertex` is the interpolation when none is given and
/// the size does not tell.
fn primvar<'a>(
    prim: &'a Prim,
    name: &str,
    dim: usize,
    counts: &Counts,
    vertex: bool,
) -> Option<Primvar<'a>> {
    let attribute: &Attribute = prim.attributes.get(name)?;
    let Some(Value::Floats { dim: d, data }) = &attribute.value else {
        return None;
    };
    if *d != dim || data.is_empty() {
        return None;
    }
    let indices = match prim.value(&format!("{name}:indices")) {
        Some(Value::Ints(i)) if !i.is_empty() => Some(i.as_ref()),
        _ => None,
    };
    let values = data.len() / dim;
    let used = indices.map_or(values, <[i32]>::len);
    let interpolation = match attribute.interpolation.as_deref() {
        Some("uniform") => Interpolation::Uniform,
        Some("vertex" | "varying") => Interpolation::Vertex,
        Some("faceVarying") => Interpolation::FaceVarying,
        Some(_) => Interpolation::Constant,
        None if used == counts.corners && used != counts.points => Interpolation::FaceVarying,
        None if used == counts.points || vertex => Interpolation::Vertex,
        None => Interpolation::Constant,
    };
    let needed = match interpolation {
        Interpolation::Constant => 1,
        Interpolation::Uniform => counts.faces,
        Interpolation::Vertex => counts.points,
        Interpolation::FaceVarying => counts.corners,
    };
    if used < needed {
        return None;
    }
    if let Some(indices) = indices
        && indices
            .iter()
            .any(|i| usize::try_from(*i).map_or(true, |i| i >= values))
    {
        return None;
    }
    Some(Primvar {
        data,
        dim,
        interpolation,
        indices,
    })
}

/// Where an input of a shader takes its value from.
enum Source<'a> {
    /// A value.
    Value(&'a Value),
    /// An output of another shader.
    Output(&'a Prim),
}

/// The source of an input or an output of a prim, following its
/// connections through node graphs and the inputs of materials.
fn source<'a>(root: &'a Prim, prim: &'a Prim, name: &str) -> Option<Source<'a>> {
    let mut attribute = prim.attributes.get(name)?;
    for _ in 0..16 {
        let Some(connection) = &attribute.connection else {
            return attribute.value.as_ref().map(Source::Value);
        };
        let (path, property) = split_property(connection);
        let target = root.find(path)?;
        if target.type_name == "Shader" {
            return Some(Source::Output(target));
        }
        attribute = target.attributes.get(property)?;
    }
    None
}

/// The `info:id` of a shader.
fn shader_id(prim: &Prim) -> &str {
    prim.value("info:id").and_then(Value::text).unwrap_or("")
}

/// A material, with the name of the primvar its texture reads.
#[derive(Clone)]
struct Look {
    material: Material,
    uv: Option<String>,
}

/// Walks the composed tree into a scene.
struct Builder<'a> {
    root: &'a Prim,
    files: &'a mut dyn Files,
    z_up: bool,
    scene: Scene,
    looks: HashMap<String, Option<Look>>,
    textures: HashMap<String, Option<usize>>,
    triangles: usize,
}

impl Builder<'_> {
    fn texture(&mut self, path: &str) -> Option<usize> {
        if let Some(t) = self.textures.get(path) {
            return *t;
        }
        let t = if self.scene.textures.len() < limits::TEXTURES {
            self.files
                .read(path)
                .and_then(|bytes| texture::decode(&bytes))
                .map(|texture| {
                    self.scene.textures.push(texture);
                    self.scene.textures.len() - 1
                })
        } else {
            None
        };
        self.textures.insert(path.to_owned(), t);
        t
    }

    /// The look of the material at a path; none when there is no
    /// `UsdPreviewSurface` there.
    fn look(&mut self, path: &str) -> Option<Look> {
        if let Some(look) = self.looks.get(path) {
            return look.clone();
        }
        let look = self.read_look(path);
        self.looks.insert(path.to_owned(), look.clone());
        look
    }

    fn read_look(&mut self, path: &str) -> Option<Look> {
        let root = self.root;
        let material = root.find(path)?;
        let surface = material
            .attributes
            .keys()
            .find(|k| *k == "outputs:surface")
            .or_else(|| {
                material
                    .attributes
                    .keys()
                    .find(|k| k.starts_with("outputs:") && k.ends_with(":surface"))
            })?;
        let Source::Output(shader) = source(root, material, surface)? else {
            return None;
        };
        if shader_id(shader) != "UsdPreviewSurface" {
            return None;
        }
        // The defaults of UsdPreviewSurface.
        let mut color = [0.18, 0.18, 0.18, 1.0];
        let mut texture = None;
        let mut uv = None;
        match source(root, shader, "inputs:diffuseColor") {
            Some(Source::Value(v)) => {
                if let Some(t) = v.tuple() {
                    for (c, v) in color.iter_mut().zip(t.iter().take(3)) {
                        *c = (*v as f32).clamp(0.0, 1.0);
                    }
                }
            }
            Some(Source::Output(tex)) if shader_id(tex) == "UsdUVTexture" => {
                let file = match source(root, tex, "inputs:file") {
                    Some(Source::Value(Value::Asset(f) | Value::Text(f))) => Some(f.clone()),
                    _ => None,
                };
                texture = file.and_then(|f| self.texture(&f));
                if texture.is_some() {
                    color = [1.0; 4];
                    if let Some(Source::Value(v)) = source(root, tex, "inputs:scale")
                        && let Some(t) = v.tuple()
                    {
                        for (c, v) in color.iter_mut().zip(t.iter().take(3)) {
                            *c = (*v as f32).clamp(0.0, 1.0);
                        }
                    }
                    uv = Some(match source(root, tex, "inputs:st") {
                        Some(Source::Output(reader)) => {
                            match source(root, reader, "inputs:varname") {
                                Some(Source::Value(v)) => v.text().unwrap_or("st").to_owned(),
                                _ => "st".to_owned(),
                            }
                        }
                        _ => "st".to_owned(),
                    });
                } else {
                    color = Material::default().color;
                }
            }
            _ => {}
        }
        if let Some(Source::Value(v)) = source(root, shader, "inputs:opacity")
            && let Some(o) = v.number()
        {
            color[3] = (o as f32).clamp(0.0, 1.0);
        }
        Some(Look {
            material: Material { color, texture },
            uv,
        })
    }

    fn walk<'t>(
        &mut self,
        prim: &'t Prim,
        path: &str,
        parent: &Matrix,
        binding: Option<&'t str>,
    ) -> Result<()> {
        if !prim.active
            || prim.value("visibility").and_then(Value::text) == Some("invisible")
            || matches!(
                prim.value("purpose").and_then(Value::text),
                Some("guide" | "proxy")
            )
        {
            return Ok(());
        }
        if matches!(
            prim.type_name.as_str(),
            "Material" | "Shader" | "NodeGraph" | "GeomSubset"
        ) {
            return Ok(());
        }
        let (m, reset) = local(prim);
        let world = if reset { m } else { mul(&m, parent) };
        let binding = bound(prim).or(binding);
        if prim.type_name == "Mesh" {
            self.mesh(prim, &world, binding)?;
        }
        for child in &prim.children {
            self.walk(child, &join(path, &child.name), &world, binding)?;
        }
        Ok(())
    }

    /// A point of the mesh in the space of the scene.
    fn place(&self, world: &Matrix, p: &[f32]) -> [f32; 3] {
        let [x, y, z] = [0, 1, 2].map(|i| f64::from(p.get(i).copied().unwrap_or(0.0)));
        let v = [0, 1, 2].map(|c| {
            x * at(world, 0, c) + y * at(world, 1, c) + z * at(world, 2, c) + at(world, 3, c)
        });
        self.up([v[0] as f32, v[1] as f32, v[2] as f32])
    }

    /// A vector of a Z-up stage turned to Y up.
    fn up(&self, [x, y, z]: [f32; 3]) -> [f32; 3] {
        if self.z_up { [x, z, -y] } else { [x, y, z] }
    }

    fn mesh(&mut self, prim: &Prim, world: &Matrix, binding: Option<&str>) -> Result<()> {
        let Some(Value::Floats {
            dim: 3,
            data: points,
        }) = prim.value("points")
        else {
            return Ok(());
        };
        let (Some(Value::Ints(counts)), Some(Value::Ints(indices))) = (
            prim.value("faceVertexCounts"),
            prim.value("faceVertexIndices"),
        ) else {
            return Ok(());
        };
        let point_count = points.len() / 3;
        let holes: std::collections::HashSet<i32> = match prim.value("holeIndices") {
            Some(Value::Ints(h)) => h.iter().copied().collect(),
            _ => Default::default(),
        };
        let left = prim.value("orientation").and_then(Value::text) == Some("leftHanded");
        let flat = prim.value("subdivisionScheme").and_then(Value::text) == Some("none");

        // The faces, as the face-vertex where each starts and its count.
        let mut faces = Vec::with_capacity(counts.len());
        let mut start = 0usize;
        for &c in counts.iter() {
            let Ok(c) = usize::try_from(c) else { break };
            let Some(end) = start.checked_add(c).filter(|e| *e <= indices.len()) else {
                break;
            };
            faces.push((start, c));
            start = end;
        }
        let shape = Counts {
            faces: counts.len(),
            points: point_count,
            corners: indices.len(),
        };

        // The material of each face: the mesh's, or that of a subset.
        let mut groups: Vec<Option<&str>> = vec![binding];
        let mut group_of = vec![0usize; faces.len()];
        for subset in &prim.children {
            if subset.type_name != "GeomSubset"
                || !matches!(
                    subset.value("elementType").and_then(Value::text),
                    None | Some("face")
                )
            {
                continue;
            }
            let (Some(target), Some(Value::Ints(members))) =
                (bound(subset), subset.value("indices"))
            else {
                continue;
            };
            groups.push(Some(target));
            for m in members.iter() {
                if let Some(slot) = usize::try_from(*m).ok().and_then(|m| group_of.get_mut(m)) {
                    *slot = groups.len() - 1;
                }
            }
        }

        let point = |at: usize| -> Option<usize> {
            let p = usize::try_from(*indices.get(at)?).ok()?;
            (p < point_count).then_some(p)
        };
        let corner_point = |p: usize| points.get(p * 3..p * 3 + 3);

        // The places of the points, and their smooth normals.
        let placed: Vec<[f32; 3]> = (0..point_count)
            .map(|p| self.place(world, corner_point(p).unwrap_or(&[0.0; 3])))
            .collect();
        let normals_given = primvar(prim, "primvars:normals", 3, &shape, true)
            .or_else(|| primvar(prim, "normals", 3, &shape, true));
        let normal_matrix = invert(world);
        let mut smooth = Vec::new();
        if normals_given.is_none() && !flat {
            smooth = vec![[0.0f32; 3]; point_count];
            for (f, &(s, c)) in faces.iter().enumerate() {
                if holes.contains(&(f as i32)) {
                    continue;
                }
                for k in 1..c.saturating_sub(1) {
                    let corners = [point(s), point(s + k), point(s + k + 1)];
                    let [Some(pa), Some(pb), Some(pd)] =
                        corners.map(|p| p.and_then(|p| placed.get(p).copied()))
                    else {
                        continue;
                    };
                    let n = cross(sub(pb, pa), sub(pd, pa));
                    for p in corners.into_iter().flatten() {
                        if let Some(sum) = smooth.get_mut(p) {
                            *sum = crate::zip_with(*sum, n, |s, n| s + n);
                        }
                    }
                }
            }
        }

        for (g, target) in groups.iter().enumerate() {
            let look = target.and_then(|t| self.look(t));
            let mut mesh = Mesh::default();
            let uvs = look
                .as_ref()
                .filter(|l| l.material.texture.is_some())
                .and_then(|l| {
                    let name = l.uv.as_deref().unwrap_or("st");
                    primvar(prim, &format!("primvars:{name}"), 2, &shape, false)
                        .or_else(|| primvar(prim, "primvars:st", 2, &shape, false))
                });
            let mut colors = None;
            match &look {
                Some(l) => mesh.material = l.material.clone(),
                None => {
                    // With no material, the display color colors the mesh.
                    if let Some(c) = primvar(prim, "primvars:displayColor", 3, &shape, false) {
                        if c.interpolation == Interpolation::Constant {
                            if let Some(v) = c.at(0, 0, 0) {
                                for (m, v) in mesh.material.color.iter_mut().zip(v) {
                                    *m = v.clamp(0.0, 1.0);
                                }
                            }
                        } else {
                            colors = Some(c);
                        }
                    }
                    if let Some(Value::Floats { data, .. }) = prim.value("primvars:displayOpacity")
                        && let Some(o) = data.first()
                    {
                        mesh.material.color[3] = o.clamp(0.0, 1.0);
                    }
                }
            }
            for (f, &(s, c)) in faces.iter().enumerate() {
                if group_of.get(f) != Some(&g) || c < 3 || holes.contains(&(f as i32)) {
                    continue;
                }
                let corners: Option<Vec<usize>> = (0..c).map(|k| point(s + k)).collect();
                let Some(corners) = corners else { continue };
                for k in 1..c - 1 {
                    self.triangles += 1;
                    if self.triangles > limits::TRIANGLES {
                        return Err(Error::TooLarge);
                    }
                    let (Some(&first), Some(&this), Some(&next)) =
                        (corners.first(), corners.get(k), corners.get(k + 1))
                    else {
                        continue;
                    };
                    let mut tri = [(s, first), (s + k, this), (s + k + 1, next)];
                    if left {
                        tri.swap(1, 2);
                    }
                    let face_normal = {
                        let [a, b, d] =
                            tri.map(|(_, p)| placed.get(p).copied().unwrap_or_default());
                        normalize(cross(sub(b, a), sub(d, a)))
                    };
                    let base = mesh.positions.len() as u32;
                    for (corner, p) in tri {
                        mesh.positions
                            .push(placed.get(p).copied().unwrap_or_default());
                        let given = normals_given.as_ref().zip(normal_matrix.as_ref()).and_then(
                            |(n, inv)| n.at(f, corner, p).map(|n| self.up(turn_normal(inv, n))),
                        );
                        let n = match given {
                            Some(n) => n,
                            None if !smooth.is_empty() => {
                                let n = normalize(smooth.get(p).copied().unwrap_or_default());
                                if n == [0.0; 3] { face_normal } else { n }
                            }
                            None => face_normal,
                        };
                        mesh.normals.push(n);
                        if let Some(uv) = &uvs {
                            let v = uv.at(f, corner, p).unwrap_or(&[0.0, 0.0]);
                            mesh.uvs.push([
                                v.first().copied().unwrap_or(0.0),
                                v.get(1).copied().unwrap_or(0.0),
                            ]);
                        }
                        if let Some(col) = &colors {
                            let v = col.at(f, corner, p).unwrap_or(&[0.7, 0.7, 0.7]);
                            mesh.colors.push(
                                [0, 1, 2].map(|i| v.get(i).copied().unwrap_or(0.7).clamp(0.0, 1.0)),
                            );
                        }
                    }
                    mesh.triangles.push([base, base + 1, base + 2]);
                }
            }
            if !mesh.triangles.is_empty() {
                self.scene.meshes.push(mesh);
            }
        }
        Ok(())
    }
}

/// The material a prim binds, by its path.
fn bound(prim: &Prim) -> Option<&str> {
    [
        "material:binding",
        "material:binding:preview",
        "material:binding:full",
    ]
    .iter()
    .find_map(|name| prim.relationships.get(*name)?.first())
    .map(String::as_str)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n > 0.0 && n.is_finite() {
        v.map(|c| c / n)
    } else {
        [0.0; 3]
    }
}

/// A normal through the inverse of the world matrix, transposed: what
/// keeps it square to its surface.
fn turn_normal(inverse: &Matrix, n: &[f32]) -> [f32; 3] {
    let n = [0, 1, 2].map(|i| f64::from(n.get(i).copied().unwrap_or(0.0)));
    let v = [0, 1, 2].map(|j| {
        n.iter()
            .enumerate()
            .map(|(i, n)| n * at(inverse, j, i))
            .sum::<f64>() as f32
    });
    normalize(v)
}

/// Builds the scene of the layer at `root` of a package.
pub(crate) fn build(root: &str, files: &mut dyn Files) -> Result<Scene> {
    let mut composer = Composer {
        files,
        layers: HashMap::new(),
        prims: 0,
    };
    let Some(layer) = composer.layer(root)? else {
        return Err(crate::malformed("no root layer"));
    };
    let tree = composer.compose(&layer.root, "/", root, &layer, 0)?;
    let mut builder = Builder {
        root: &tree,
        files: composer.files,
        z_up: layer.z_up,
        scene: Scene::default(),
        looks: HashMap::new(),
        textures: HashMap::new(),
        triangles: 0,
    };
    builder.walk(&tree, "/", &IDENTITY, None)?;
    Ok(builder.scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Memory(Vec<(&'static str, Vec<u8>)>);

    impl Files for Memory {
        fn read(&mut self, path: &str) -> Option<Vec<u8>> {
            self.0
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, b)| b.clone())
        }
    }

    #[test]
    fn w116_asset_and_prim_paths_resolve() {
        assert_eq!(resolve("scene.usdc", "./part.usdc"), "part.usdc");
        assert_eq!(resolve("a/b/scene.usda", "../tex/w.png"), "a/tex/w.png");
        assert_eq!(resolve("a/scene.usda", "/0/w.png"), "0/w.png");
        assert_eq!(resolve("scene.usda", "../../x"), "x");
        assert_eq!(absolute("/Root/Cube", "../Looks/Red"), "/Root/Looks/Red");
        assert_eq!(
            absolute("/Root/M", ".outputs:surface"),
            "/Root/M.outputs:surface"
        );
        assert_eq!(
            rebase("/Part/Box", "/Part", "/Root/Placed").as_deref(),
            Some("/Root/Placed/Box")
        );
        assert_eq!(rebase("/Partly", "/Part", "/X"), None);
    }

    #[test]
    fn w116_transforms_go_in_their_order() {
        let mut prim = Prim::new("X");
        prim.attributes.insert(
            "xformOpOrder".into(),
            Attribute {
                value: Some(Value::Texts(vec![
                    "xformOp:translate".into(),
                    "xformOp:rotateZ".into(),
                    "!invert!xformOp:scale".into(),
                ])),
                ..Attribute::default()
            },
        );
        let set = |prim: &mut Prim, name: &str, value: Value| {
            prim.attributes.insert(
                name.into(),
                Attribute {
                    value: Some(value),
                    ..Attribute::default()
                },
            );
        };
        set(
            &mut prim,
            "xformOp:translate",
            Value::Tuple(vec![1.0, 2.0, 3.0]),
        );
        set(&mut prim, "xformOp:rotateZ", Value::Float(90.0));
        set(
            &mut prim,
            "xformOp:scale",
            Value::Tuple(vec![0.5, 0.5, 0.5]),
        );
        let (m, reset) = local(&prim);
        assert!(!reset);
        // (1, 0, 0) scaled by 2, turned to (0, 2, 0), then moved.
        let p = [0, 1, 2].map(|c| m[c] + m[12 + c]);
        assert!(
            (p[0] - 1.0).abs() < 1e-9 && (p[1] - 4.0).abs() < 1e-9 && (p[2] - 3.0).abs() < 1e-9
        );
        let q = orient(&[0.5f64.sqrt(), 0.0, 0.0, 0.5f64.sqrt()]);
        let r = rotate(2, 90.0);
        assert!(q.iter().zip(&r).all(|(a, b)| (a - b).abs() < 1e-9));
        let inv = invert(&m).unwrap();
        let id = mul(&m, &inv);
        assert!(id.iter().zip(&IDENTITY).all(|(a, b)| (a - b).abs() < 1e-9));
    }

    #[test]
    fn w116_a_text_scene_with_colors_subsets_and_a_reference() {
        let scene = br#"#usda 1.0
(
    defaultPrim = "Root"
    upAxis = "Z"
)
def Xform "Root"
{
    def Mesh "Tri"
    {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0)]
        int[] faceVertexCounts = [3, 3]
        int[] faceVertexIndices = [0, 1, 2, 1, 3, 2]
        color3f[] primvars:displayColor = [(1, 0, 0), (0, 1, 0), (0, 0, 1), (1, 1, 1)] (
            interpolation = "vertex"
        )
        uniform token subdivisionScheme = "none"
    }
    def Mesh "Hidden"
    {
        token visibility = "invisible"
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
    }
    def Xform "Placed" (
        prepend references = @./part.usda@
    )
    {
        double3 xformOp:translate = (0, 0, 5)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
}
"#;
        let part = br#"#usda 1.0
(
    defaultPrim = "Part"
)
def Xform "Part"
{
    def Mesh "Box"
    {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        rel material:binding = </Part/Looks/Gold>
    }
    def Scope "Looks"
    {
        def Material "Gold"
        {
            token outputs:surface.connect = </Part/Looks/Gold/Surface.outputs:surface>
            def Shader "Surface"
            {
                uniform token info:id = "UsdPreviewSurface"
                color3f inputs:diffuseColor = (1, 0.8, 0)
                float inputs:opacity = 0.5
                token outputs:surface
            }
        }
    }
}
"#;
        let mut files = Memory(vec![
            ("scene.usda", scene.to_vec()),
            ("part.usda", part.to_vec()),
        ]);
        let scene = build("scene.usda", &mut files).unwrap();
        assert_eq!(scene.meshes.len(), 2);
        let tri = &scene.meshes[0];
        assert_eq!(tri.triangles.len(), 2);
        // Z up: (0, 1, 0) goes to (0, 0, -1).
        assert_eq!(tri.positions[2], [0.0, 0.0, -1.0]);
        assert_eq!(tri.colors[0], [1.0, 0.0, 0.0]);
        assert_eq!(tri.colors[2], [0.0, 0.0, 1.0]);
        assert_eq!(tri.normals[0], [0.0, 1.0, 0.0]);
        let gold = &scene.meshes[1];
        assert_eq!(gold.material.color, [1.0, 0.8, 0.0, 0.5]);
        // Moved 5 up in Z, which is Y in the scene.
        assert_eq!(gold.positions[0], [0.0, 5.0, 0.0]);
    }

    #[test]
    fn w116_a_layer_that_brings_itself_ends() {
        let scene = br#"#usda 1.0
def Xform "Root" (
    references = @./scene.usda@
)
{
    def Mesh "Tri"
    {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
    }
}
"#;
        let mut files = Memory(vec![("scene.usda", scene.to_vec())]);
        let scene = build("scene.usda", &mut files).unwrap();
        assert_eq!(scene.triangle_count(), 1);
    }
}
