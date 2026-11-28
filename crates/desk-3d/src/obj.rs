// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Wavefront OBJ, with the materials of its MTL libraries and the
//! pictures they name (D-033).
//!
//! A reader takes the faces of the file, as fans of triangles, with the
//! normals, texture coordinates and colors of their corners, and makes a
//! mesh of each material that faces use. Of a material it takes the
//! diffuse color (`Kd`), how opaque it is (`d`, or `Tr`), and the picture
//! of its diffuse color (`map_Kd`), whose alpha cuts the mesh out when
//! the material also has a map of alpha (`map_d`). It leaves out lines,
//! points, curves and surfaces.
//!
//! The libraries and the pictures are files beside the OBJ, asked for by
//! their relative paths, which never leave its folder. A library or a
//! picture that is missing leaves the default material, or the color
//! alone: an OBJ is often found without them.

use std::collections::HashMap;

use crate::gltf::safe_path;
use crate::{Material, Mesh, Result, Scene, Texture, limits, malformed, texture};

/// The statements of OBJ, which a line starts with.
const STATEMENTS: &[&str] = &[
    "v",
    "vt",
    "vn",
    "vp",
    "f",
    "fo",
    "l",
    "p",
    "o",
    "g",
    "s",
    "mg",
    "usemtl",
    "mtllib",
    "usemap",
    "maplib",
    "cstype",
    "deg",
    "bmat",
    "step",
    "curv",
    "curv2",
    "surf",
    "parm",
    "trim",
    "hole",
    "scrv",
    "sp",
    "end",
    "con",
    "lod",
    "bevel",
    "c_interp",
    "d_interp",
    "shadow_obj",
    "trace_obj",
    "ctech",
    "stech",
    "call",
    "csh",
];

/// How much of the start of a file is looked at to tell an OBJ.
const SNIFF: usize = 64 * 1024;

/// The lines of the text, those that go on with a backslash joined, and
/// with no comment.
fn lines(text: &str) -> impl Iterator<Item = String> + '_ {
    let mut raw = text.lines();
    std::iter::from_fn(move || {
        let mut line = String::new();
        let mut any = false;
        loop {
            let Some(next) = raw.next() else {
                // A line that goes on past the end of the file ends there.
                return any.then_some(line);
            };
            any = true;
            // A comment goes on to no line after it.
            let comment = line.trim_start().is_empty() && next.trim_start().starts_with('#');
            match next.strip_suffix('\\').filter(|_| !comment) {
                Some(part) => {
                    line.push_str(part);
                    line.push(' ');
                }
                None => {
                    line.push_str(next);
                    break;
                }
            }
        }
        Some(line)
    })
    .filter_map(|l| {
        let trimmed = l.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        // A comment after the data, after a space.
        let end = l
            .char_indices()
            .find(|&(i, c)| {
                c == '#'
                    && l.get(..i)
                        .and_then(|b| b.chars().next_back())
                        .is_some_and(|b| b.is_ascii_whitespace())
            })
            .map_or(l.len(), |(i, _)| i);
        Some(l.get(..end).unwrap_or(&l).to_owned())
    })
}

/// A path of a file beside the OBJ as it is asked for: backslashes as
/// slashes and `.` folders left out, then checked to stay in its folder.
pub(crate) fn clean(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let parts: Vec<&str> = path.split('/').filter(|p| *p != ".").collect();
    safe_path(parts.join("/"))
}

/// The first word of a line, and the rest after it with no space around.
fn word(line: &str) -> (&str, &str) {
    let line = line.trim();
    match line.split_once(|c: char| c.is_ascii_whitespace()) {
        Some((w, rest)) => (w, rest.trim()),
        None => (line, ""),
    }
}

fn text(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))
}

/// Whether the bytes are an OBJ: text whose lines all start with its
/// statements, with a vertex among them.
pub(crate) fn is_obj(bytes: &[u8]) -> bool {
    let window = bytes.get(..SNIFF).unwrap_or(bytes);
    if window.contains(&0) {
        return false;
    }
    let text = text(window);
    let mut all: Vec<String> = lines(&text).collect();
    // The last line of a window that cuts the file may be cut too.
    if window.len() < bytes.len() {
        all.pop();
    }
    let mut vertex = false;
    for line in &all {
        let (w, _) = word(line);
        if !STATEMENTS.contains(&w) {
            return false;
        }
        vertex |= w == "v";
    }
    vertex
}

/// The numbers of a statement, up to `N`, and how many there were;
/// none when one is not a finite number.
fn numbers<const N: usize>(rest: &str) -> Option<([f32; N], usize)> {
    let mut out = [0.0; N];
    let mut n = 0;
    for (o, w) in out.iter_mut().zip(rest.split_ascii_whitespace()) {
        let v: f32 = w.parse().ok()?;
        if !v.is_finite() {
            return None;
        }
        *o = v;
        n += 1;
    }
    Some((out, n))
}

/// The index of an element by a number of a face, which counts from 1,
/// or back from the last when it is negative.
fn resolve(number: &str, len: usize) -> Option<usize> {
    let n: i64 = number.parse().ok()?;
    let i = if n > 0 {
        usize::try_from(n - 1).ok()?
    } else {
        len.checked_sub(usize::try_from(n.checked_neg()?).ok()?)?
    };
    (i < len).then_some(i)
}

/// A corner of a face: its vertex, texture coordinates and normal.
type Key = (usize, Option<usize>, Option<usize>);

/// The faces of one material, being made into a mesh.
#[derive(Default)]
struct Part {
    material: String,
    corners: HashMap<Key, u32>,
    positions: Vec<[f32; 3]>,
    normals: Vec<Option<[f32; 3]>>,
    uvs: Vec<Option<[f32; 2]>>,
    colors: Vec<Option<[f32; 3]>>,
    triangles: Vec<[u32; 3]>,
}

/// A material of an MTL library.
#[derive(Clone, Debug, PartialEq)]
struct Mtl {
    color: [f32; 3],
    opacity: f32,
    /// The picture of the diffuse color, by its path from the folder of
    /// the OBJ.
    picture: Option<String>,
    /// Whether the material has a map of alpha.
    alpha: bool,
}

impl Default for Mtl {
    fn default() -> Self {
        let [r, g, b, _] = Material::default().color;
        Self {
            color: [r, g, b],
            opacity: 1.0,
            picture: None,
            alpha: false,
        }
    }
}

/// The file of a map statement, after its options, as a path from the
/// folder of its library.
fn map_file(mut rest: &str) -> Option<&str> {
    loop {
        let (w, after) = word(rest);
        let Some(option) = w.strip_prefix('-') else {
            break;
        };
        rest = after;
        // How many words an option takes: some up to three numbers.
        let (least, most) = match option {
            "o" | "s" | "t" => (1, 3),
            "mm" => (2, 2),
            _ => (1, 1),
        };
        for k in 0..most {
            let (w, after) = word(rest);
            if k >= least && w.parse::<f32>().is_err() {
                break;
            }
            rest = after;
        }
    }
    (!rest.is_empty()).then_some(rest)
}

/// The materials of an MTL library, whose folder is `folder`.
fn library(bytes: &[u8], folder: &str, materials: &mut HashMap<String, Mtl>) {
    let text = text(bytes);
    let mut current: Option<(String, Mtl)> = None;
    let mut seen_d = false;
    // `Tr` is the other way round from `d`, and is taken only when a
    // material has no `d`.
    let mut transparency: Option<f32> = None;
    let mut done = |current: &mut Option<(String, Mtl)>, transparency: &mut Option<f32>| {
        if let Some((name, mut m)) = current.take() {
            if let Some(t) = transparency.take() {
                m.opacity = (1.0 - t).clamp(0.0, 1.0);
            }
            materials.entry(name).or_insert(m);
        }
        *transparency = None;
    };
    for line in lines(&text) {
        let (w, rest) = word(&line);
        if w == "newmtl" {
            done(&mut current, &mut transparency);
            current = Some((rest.to_owned(), Mtl::default()));
            seen_d = false;
            continue;
        }
        let Some((_, m)) = current.as_mut() else {
            continue;
        };
        match w {
            "Kd" => match numbers::<3>(rest) {
                Some((c, 3)) => m.color = c.map(|v| v.clamp(0.0, 1.0)),
                Some(([v, ..], 1)) => m.color = [v.clamp(0.0, 1.0); 3],
                _ => {}
            },
            "d" => {
                if let Some(v) = rest
                    .split_ascii_whitespace()
                    .last()
                    .and_then(|v| v.parse::<f32>().ok())
                    .filter(|v| v.is_finite())
                {
                    m.opacity = v.clamp(0.0, 1.0);
                    seen_d = true;
                    transparency = None;
                }
            }
            "Tr" if !seen_d => {
                if let Some(([v], 1)) = numbers::<1>(rest) {
                    transparency = Some(v);
                }
            }
            "map_Kd" => m.picture = map_file(rest).map(|f| format!("{folder}{f}")),
            "map_d" => m.alpha = map_file(rest).is_some(),
            _ => {}
        }
    }
    done(&mut current, &mut transparency);
}

/// The folder of a path, with its last slash; empty when it has none.
fn folder(path: &str) -> &str {
    path.rfind('/').and_then(|i| path.get(..=i)).unwrap_or("")
}

/// A model read from an OBJ, with the bytes of the pictures of its
/// textures as they were in their files, where they were not made
/// smaller.
pub(crate) struct Read {
    pub(crate) scene: Scene,
    pub(crate) pictures: Vec<Option<Vec<u8>>>,
}

/// Reads an OBJ; `files` gives the bytes of a file beside it by its
/// relative path.
pub(crate) fn read(bytes: &[u8], files: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Result<Read> {
    let text = text(bytes);
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut vertex_colors: Vec<Option<[f32; 3]>> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut libraries: Vec<String> = Vec::new();
    let mut parts: Vec<Part> = Vec::new();
    let mut by_material: HashMap<String, usize> = HashMap::new();
    let mut material = String::new();
    let (mut corners, mut triangles) = (0usize, 0usize);
    let mut face: Vec<u32> = Vec::new();
    for line in lines(&text) {
        let (w, rest) = word(&line);
        match w {
            "v" => {
                let (v, n) = numbers::<6>(rest).ok_or_else(|| malformed("a vertex of the OBJ"))?;
                if n < 3 {
                    return Err(malformed("a vertex of the OBJ"));
                }
                if positions.len() >= limits::CORNERS {
                    return Err(crate::Error::TooLarge);
                }
                let [x, y, z, r, g, b] = v;
                positions.push([x, y, z]);
                vertex_colors.push((n == 6).then(|| [r, g, b].map(|c| c.clamp(0.0, 1.0))));
            }
            "vt" => {
                let ([u, v], _) =
                    numbers::<2>(rest).ok_or_else(|| malformed("a texture vertex of the OBJ"))?;
                if uvs.len() >= limits::CORNERS {
                    return Err(crate::Error::TooLarge);
                }
                uvs.push([u, v]);
            }
            "vn" => {
                let (n, _) = numbers::<3>(rest).ok_or_else(|| malformed("a normal of the OBJ"))?;
                if normals.len() >= limits::CORNERS {
                    return Err(crate::Error::TooLarge);
                }
                normals.push(unit(n));
            }
            "usemtl" => rest.clone_into(&mut material),
            "mtllib" => {
                if libraries.len() < limits::TEXTURES {
                    libraries.push(rest.to_owned());
                }
            }
            "f" | "fo" => {
                let p = match by_material.get(&material) {
                    Some(&p) => p,
                    None => {
                        by_material.insert(material.clone(), parts.len());
                        parts.push(Part {
                            material: material.clone(),
                            ..Part::default()
                        });
                        parts.len() - 1
                    }
                };
                let Some(part) = parts.get_mut(p) else {
                    continue;
                };
                face.clear();
                let mut whole = true;
                for corner in rest.split_ascii_whitespace() {
                    let mut numbers = corner.split('/');
                    let v = numbers.next().and_then(|n| resolve(n, positions.len()));
                    let mut other = |len: usize| match numbers.next() {
                        None | Some("") => Some(None),
                        Some(n) => resolve(n, len).map(Some),
                    };
                    let (Some(v), Some(t), Some(n)) = (v, other(uvs.len()), other(normals.len()))
                    else {
                        whole = false;
                        break;
                    };
                    let key = (v, t, n);
                    let index = match part.corners.get(&key) {
                        Some(&i) => i,
                        None => {
                            corners += 1;
                            if corners > limits::CORNERS {
                                return Err(crate::Error::TooLarge);
                            }
                            let i = part.positions.len() as u32;
                            part.corners.insert(key, i);
                            part.positions
                                .push(positions.get(v).copied().unwrap_or_default());
                            part.colors.push(vertex_colors.get(v).copied().flatten());
                            part.uvs.push(t.and_then(|t| uvs.get(t).copied()));
                            part.normals.push(n.and_then(|n| normals.get(n).copied()));
                            i
                        }
                    };
                    face.push(index);
                }
                // A face with a corner that is not there is left out.
                if !whole {
                    continue;
                }
                if let Some((&first, rest)) = face.split_first() {
                    for &[b, c] in rest.array_windows::<2>() {
                        if first != b && b != c && first != c {
                            triangles += 1;
                            if triangles > limits::TRIANGLES {
                                return Err(crate::Error::TooLarge);
                            }
                            part.triangles.push([first, b, c]);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // The libraries, each from the folder of the OBJ.
    let mut fetch = |path: &str| -> Option<Vec<u8>> { files(&clean(path)?) };
    let mut materials: HashMap<String, Mtl> = HashMap::new();
    for name in &libraries {
        let name = name.replace('\\', "/");
        if let Some(bytes) = fetch(&name) {
            library(&bytes, folder(&name), &mut materials);
            continue;
        }
        // Names with no space in them, several on one line.
        for one in name.split_ascii_whitespace().filter(|n| *n != name) {
            if let Some(bytes) = fetch(one) {
                library(&bytes, folder(one), &mut materials);
            }
        }
    }
    let mut textures: Vec<Texture> = Vec::new();
    let mut pictures: Vec<Option<Vec<u8>>> = Vec::new();
    let mut made: HashMap<(String, bool), Option<usize>> = HashMap::new();
    let mut picture = |path: &str, alpha: bool| -> Option<usize> {
        let path = clean(path).unwrap_or_else(|| path.replace('\\', "/"));
        if let Some(m) = made.get(&(path.clone(), alpha)) {
            return *m;
        }
        // A whole path, as exporters write, is looked for by its name in
        // the folder of the OBJ.
        let name = path.rsplit('/').next().unwrap_or(&path);
        let m = if textures.len() < limits::TEXTURES {
            fetch(&path)
                .or_else(|| fetch(name))
                .and_then(|bytes| {
                    let (mut t, shrunk) = texture::decode_shrunk(&bytes)?;
                    // The file is kept as it was only when its alpha is
                    // what the material shows.
                    let mut keep = !shrunk;
                    if !alpha {
                        for [_, _, _, a] in t.rgba.as_chunks_mut::<4>().0 {
                            keep &= *a == 255;
                            *a = 255;
                        }
                    }
                    Some((t, keep.then_some(bytes)))
                })
                .map(|(t, bytes)| {
                    textures.push(t);
                    pictures.push(bytes);
                    textures.len() - 1
                })
        } else {
            None
        };
        made.insert((path, alpha), m);
        m
    };
    let mut meshes = Vec::new();
    for part in parts {
        if part.triangles.is_empty() {
            continue;
        }
        let mtl = materials.get(&part.material).cloned().unwrap_or_default();
        let [r, g, b] = mtl.color;
        let has_uvs = part.uvs.iter().any(Option::is_some);
        let texture = mtl
            .picture
            .as_deref()
            .filter(|_| has_uvs)
            .and_then(|p| picture(p, mtl.alpha));
        let all = |v: Vec<Option<_>>| -> Vec<_> {
            if v.iter().all(Option::is_some) {
                v.into_iter().flatten().collect()
            } else {
                Vec::new()
            }
        };
        meshes.push(Mesh {
            uvs: if texture.is_some() {
                part.uvs
                    .into_iter()
                    .map(Option::unwrap_or_default)
                    .collect()
            } else {
                Vec::new()
            },
            normals: all(part.normals),
            colors: all(part.colors),
            positions: part.positions,
            triangles: part.triangles,
            material: Material {
                color: [r, g, b, mtl.opacity],
                texture,
                translucent: mtl.opacity < 1.0,
                ..Material::default()
            },
        });
    }
    Ok(Read {
        scene: Scene {
            meshes,
            textures,
            animation: None,
        },
        pictures,
    })
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n > 0.0 && n.is_finite() {
        v.map(|c| c / n)
    } else {
        [0.0; 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_an_obj_is_told_by_its_statements() {
        assert!(is_obj(
            b"# a box\nmtllib box.mtl\no Box\nv 0 0 0\nv 1 0 0\n"
        ));
        assert!(is_obj(b"v 0 0 0 \\\n\nf 1 1 1\r\n"));
        assert!(!is_obj(b"# only a comment\n"));
        assert!(!is_obj(b"v 0 0 0\nhello there\n"));
        assert!(!is_obj(b"{\"asset\": {}}"));
        assert!(!is_obj(b"v 0 0 0\0"));
    }

    #[test]
    fn w116_comments_continued_lines_and_paths_as_exporters_write_them() {
        let all: Vec<String> =
            lines("# from C:\\work\\\nmtllib a.mtl\nv 1 2 3 # top\nf 1 2 \\").collect();
        assert_eq!(all, ["mtllib a.mtl", "v 1 2 3 ", "f 1 2  "]);
        assert_eq!(
            clean("./pictures/./a.png").as_deref(),
            Some("pictures/a.png")
        );
        assert_eq!(clean("mats\\b.mtl").as_deref(), Some("mats/b.mtl"));
        for bad in ["../a.png", ".\\..\\a.png", ".hidden.png", "/a.png"] {
            assert_eq!(clean(bad), None, "{bad}");
        }
        // A library in a folder named with backslashes finds the pictures
        // beside it, each read once however many materials use it; a
        // picture whose alpha the material does not show is written
        // again, opaque.
        let leaf = texture::encode(&Texture {
            width: 1,
            height: 1,
            rgba: vec![0, 200, 0, 0],
        })
        .unwrap();
        let mtl = b"newmtl a\nd 0.5\nmap_Kd ./leaf.png\nnewmtl b\nmap_Kd sub\\..\\leaf.png\nmap_Kd leaf.png\n";
        let obj = b"mtllib ./mats\\leaf.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nusemtl a\nf 1/1 2/1 3/1\nusemtl b\nf 1/1 3/1 2/1\n";
        let mut asked = Vec::new();
        let read = read(obj, &mut |path| {
            asked.push(path.to_owned());
            match path {
                "mats/leaf.mtl" => Some(mtl.to_vec()),
                "mats/leaf.png" => Some(leaf.clone()),
                _ => None,
            }
        })
        .unwrap();
        assert_eq!(asked, ["mats/leaf.mtl", "mats/leaf.png"]);
        assert_eq!(read.scene.textures.len(), 1);
        assert_eq!(read.scene.textures[0].rgba, [0, 200, 0, 255]);
        assert_eq!(read.pictures, [None]);
        let textures: Vec<_> = read
            .scene
            .meshes
            .iter()
            .map(|m| m.material.texture)
            .collect();
        assert_eq!(textures, [Some(0), Some(0)]);
    }

    #[test]
    fn w116_the_numbers_of_faces_count_from_one_or_back() {
        assert_eq!(resolve("1", 3), Some(0));
        assert_eq!(resolve("3", 3), Some(2));
        assert_eq!(resolve("-1", 3), Some(2));
        assert_eq!(resolve("-3", 3), Some(0));
        for n in ["0", "4", "-4", "x", "", "-9223372036854775808"] {
            assert_eq!(resolve(n, 3), None, "{n}");
        }
    }

    #[test]
    fn w116_the_file_of_a_map_comes_after_its_options() {
        assert_eq!(map_file("wood grain.png"), Some("wood grain.png"));
        assert_eq!(
            map_file("-s 2 2 1 -o 0.5 -bm 0.3 -clamp on t.jpg"),
            Some("t.jpg")
        );
        assert_eq!(map_file("-mm 0 1 -s 2 x.png"), Some("x.png"));
        assert_eq!(map_file("-blendu"), None);
        let mut materials = HashMap::new();
        library(
            b"newmtl a\nKd 1 0 0\nTr 0.25\nmap_Kd -s 1 1 1 pics\\a.png\nnewmtl b\nd 0.5\nTr 0.9\nKd 0.2\n",
            "sub/",
            &mut materials,
        );
        let a = &materials["a"];
        assert_eq!(
            (a.color, a.opacity, a.picture.as_deref()),
            ([1.0, 0.0, 0.0], 0.75, Some("sub/pics\\a.png"))
        );
        let b = &materials["b"];
        assert_eq!((b.color, b.opacity), ([0.2; 3], 0.5));
    }
}
