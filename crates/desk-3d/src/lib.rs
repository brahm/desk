// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

#![cfg_attr(not(test), deny(clippy::indexing_slicing, clippy::unwrap_used))]
#![forbid(unsafe_code)]

//! 3D models: readers of their files into a [`Scene`] of triangles, and a
//! renderer that draws a scene, turned as asked, into pixels (W116,
//! D-033).
//!
//! It reads USDZ, the format of the 3D objects of Pages 15.4: the package
//! of Universal Scene Description, with its layers in the text form
//! (`usda`) or the binary one (`usdc`), the pictures of its textures and
//! the samples in time that move its prims; and glTF 2.0, as text (`.gltf`) or binary (`.glb`), with the animation
//! of its nodes. A model of a format that a document does not keep,
//! Wavefront OBJ with its MTL materials or FBX with its animation, is
//! turned into a GLB by [`import`]. A reader takes the meshes of the model with their
//! places, the colors and textures of their materials, and leaves the
//! rest: lights, cameras and skeletons.
//!
//! Files come from anywhere, so the readers treat them as hostile: they
//! use no unsafe code, index nothing without checking it, and cap the
//! triangles, the pictures and the depth of what they read (see
//! [`limits`]). A file they cannot read gives an [`Error`].
//!
//! Like the other format libraries of Desk, this crate is licensed under
//! the MPL-2.0 and depends on no other Desk crate, so that other programs
//! can reuse it.

mod animation;
mod fbx;
mod glb;
mod gltf;
mod lz4;
mod matrix;
mod obj;
mod raster;
mod texture;
mod usd;
mod usdz;

pub use animation::Animation;
pub use gltf::pack as pack_gltf;
pub use raster::{MAX_SIDE, View, render, render_at};

/// The limits of what is read, against hostile or huge files.
pub mod limits {
    /// The most triangles of a scene.
    pub const TRIANGLES: usize = 4_000_000;
    /// The most corners of a scene: the points of its triangles.
    pub const CORNERS: usize = 3 * TRIANGLES;
    /// The most pixels of a texture as it is decoded; a larger one is
    /// left out, and the material keeps its color.
    pub const TEXTURE_PIXELS: u64 = 4096 * 4096;
    /// The longest side of a texture as it is kept, after it is decoded.
    pub const TEXTURE_SIDE: u32 = 2048;
    /// The most textures of a scene.
    pub const TEXTURES: usize = 64;
    /// The most bytes of a file inside a package that is read.
    pub const FILE_BYTES: u64 = 512 * 1024 * 1024;
    /// The most prims of a scene, with those its references bring.
    pub const PRIMS: usize = 1_000_000;
    /// How deep references go, a layer bringing another.
    pub const REFERENCE_DEPTH: usize = 8;
    /// How deep prims and values nest.
    pub const DEPTH: usize = 200;
    /// The most keys of the animation of a scene, in all its tracks.
    pub const ANIMATION_KEYS: usize = 1_000_000;
}

/// Why a model could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The file is not of a format this crate reads.
    Unknown,
    /// The file is of a known format, but broken or cut short.
    Malformed(String),
    /// The file holds more than the [`limits`] allow.
    TooLarge,
    /// The file holds no mesh to draw.
    Empty,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "not a 3D model this version reads"),
            Self::Malformed(why) => write!(f, "a broken 3D model: {why}"),
            Self::TooLarge => write!(f, "a 3D model too large to read"),
            Self::Empty => write!(f, "a 3D model with nothing to show"),
        }
    }
}

impl std::error::Error for Error {}

/// The result of reading.
pub type Result<T> = std::result::Result<T, Error>;

/// A malformed file, with why.
pub(crate) fn malformed(why: impl Into<String>) -> Error {
    Error::Malformed(why.into())
}

/// The formats of 3D models this crate reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    /// USDZ: the package of Universal Scene Description.
    Usdz,
    /// GLB: glTF 2.0 in one binary file.
    Glb,
    /// glTF 2.0 as text, its buffers and pictures inside it as `data:`
    /// URIs; [`pack_gltf`] makes a GLB of one that names files beside it.
    Gltf,
}

impl Format {
    /// All the formats, as file dialogs list them.
    pub const ALL: [Self; 3] = [Self::Usdz, Self::Glb, Self::Gltf];

    /// The format of the bytes of a file, from its start.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if usdz::is_usdz(bytes) {
            Some(Self::Usdz)
        } else if gltf::is_glb(bytes) {
            Some(Self::Glb)
        } else if gltf::is_gltf(bytes) {
            Some(Self::Gltf)
        } else {
            None
        }
    }

    /// The media type of the format.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Usdz => "model/vnd.usdz+zip",
            Self::Glb => "model/gltf-binary",
            Self::Gltf => "model/gltf+json",
        }
    }

    /// The extension of its files.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Usdz => "usdz",
            Self::Glb => "glb",
            Self::Gltf => "gltf",
        }
    }

    /// The format of a media type, as [`Self::media_type`] names it.
    pub fn from_media_type(media_type: &str) -> Option<Self> {
        match media_type.trim() {
            "model/vnd.usdz+zip" | "model/vnd.usd+zip" => Some(Self::Usdz),
            "model/gltf-binary" => Some(Self::Glb),
            "model/gltf+json" => Some(Self::Gltf),
            _ => None,
        }
    }
}

/// The formats of 3D models that are read only to be turned into GLB, as
/// [`import`] does: a document does not keep them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Import {
    /// Wavefront OBJ, with the MTL libraries and pictures beside it.
    Obj,
    /// FBX, binary or text, of version 7, with the pictures inside it or
    /// beside it.
    Fbx,
}

impl Import {
    /// All of them, as file dialogs list them.
    pub const ALL: [Self; 2] = [Self::Obj, Self::Fbx];

    /// The format of the bytes of a file, from its start.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if fbx::is_fbx(bytes) {
            Some(Self::Fbx)
        } else if obj::is_obj(bytes) {
            Some(Self::Obj)
        } else {
            None
        }
    }

    /// The media type of the format.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Obj => "model/obj",
            Self::Fbx => "application/vnd.autodesk.fbx",
        }
    }

    /// The extension of its files.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Obj => "obj",
            Self::Fbx => "fbx",
        }
    }
}

/// The bytes that a document keeps of a model: a USDZ or a GLB as it is,
/// a glTF with the files it names gathered into one GLB (see
/// [`pack_gltf`]), and a format of [`Import`] turned into a GLB. `files`
/// gives the bytes of a file beside the model by its relative path, which
/// never leaves the folder of the model.
pub fn import(bytes: &[u8], files: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Result<Vec<u8>> {
    match (Format::sniff(bytes), Import::sniff(bytes)) {
        (Some(Format::Usdz | Format::Glb), _) => Ok(bytes.to_vec()),
        (Some(Format::Gltf), _) => gltf::pack(bytes, files),
        (None, Some(Import::Obj)) => {
            let read = obj::read(bytes, files)?;
            if read.scene.triangle_count() == 0 {
                return Err(Error::Empty);
            }
            glb::write(&read.scene, &read.pictures)
        }
        (None, Some(Import::Fbx)) => {
            let read = fbx::read(bytes, files)?;
            if read.scene.triangle_count() == 0 {
                return Err(Error::Empty);
            }
            glb::write(&read.scene, &read.pictures)
        }
        (None, None) => Err(Error::Unknown),
    }
}

/// A picture of a texture, decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
    /// Its pixels, by rows from the top, as red, green, blue and alpha,
    /// not premultiplied.
    pub rgba: Vec<u8>,
}

/// How a mesh is colored.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// The color, as red, green, blue and alpha from 0 to 1.
    pub color: [f32; 4],
    /// A texture over the color, by its index in [`Scene::textures`].
    pub texture: Option<usize>,
    /// Whether what is behind shows through, by the alpha of the color
    /// and of the texture; a color whose alpha is under 1 always lets it.
    pub translucent: bool,
    /// For a mesh that does not let what is behind show through, the
    /// alpha under which the texture cuts it out.
    pub cutoff: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            color: [0.7, 0.7, 0.7, 1.0],
            texture: None,
            translucent: false,
            cutoff: 0.5,
        }
    }
}

/// A mesh of triangles, in the space of the scene: Y up, as the models of
/// USD are by default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    /// The corners of its triangles.
    pub positions: Vec<[f32; 3]>,
    /// The normal at each corner, if the file gives them or they could
    /// be made; empty otherwise.
    pub normals: Vec<[f32; 3]>,
    /// The texture coordinates of each corner, if it has a texture;
    /// empty otherwise. V goes up, as in USD.
    pub uvs: Vec<[f32; 2]>,
    /// The color of each corner, over that of the material, if the file
    /// gives one; empty otherwise.
    pub colors: Vec<[f32; 3]>,
    /// Its triangles, by the indices of their corners.
    pub triangles: Vec<[u32; 3]>,
    /// How it is colored.
    pub material: Material,
}

/// A model, as triangles to draw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    /// The meshes.
    pub meshes: Vec<Mesh>,
    /// The textures of their materials.
    pub textures: Vec<Texture>,
    /// How its parts move, if they do; [`render_at`] draws them at a
    /// time.
    pub animation: Option<Animation>,
}

impl Scene {
    /// The least and the greatest corner of the box around the scene,
    /// none when it has no triangle.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        let mut any = false;
        for mesh in &self.meshes {
            for t in &mesh.triangles {
                for &i in t {
                    let Some(p) = mesh.positions.get(i as usize) else {
                        continue;
                    };
                    lo = zip_with(lo, *p, f32::min);
                    hi = zip_with(hi, *p, f32::max);
                    any = true;
                }
            }
        }
        any.then_some((lo, hi))
    }

    /// How many triangles it has.
    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(|m| m.triangles.len()).sum()
    }
}

/// For the fuzz targets: the scene of one layer of USD, binary or text,
/// alone, as if it were the package.
#[doc(hidden)]
pub fn read_layer(bytes: &[u8]) -> Result<Scene> {
    struct One<'a>(&'a [u8]);
    impl usd::Files for One<'_> {
        fn read(&mut self, path: &str) -> Option<Vec<u8>> {
            (path == "layer").then(|| self.0.to_vec())
        }
    }
    usd::build("layer", &mut One(bytes))
}

/// The numbers of two arrays, each pair through `f`.
pub(crate) fn zip_with<T: Copy + Default, const N: usize>(
    a: [T; N],
    b: [T; N],
    f: impl Fn(T, T) -> T,
) -> [T; N] {
    let mut out = [T::default(); N];
    for ((o, x), y) in out.iter_mut().zip(a).zip(b) {
        *o = f(x, y);
    }
    out
}

/// Reads a model from the bytes of its file.
pub fn read(bytes: &[u8]) -> Result<Scene> {
    let scene = match Format::sniff(bytes) {
        Some(Format::Usdz) => usdz::read(bytes)?,
        Some(Format::Glb | Format::Gltf) => gltf::read(bytes)?,
        None => return Err(Error::Unknown),
    };
    if scene.triangle_count() == 0 {
        return Err(Error::Empty);
    }
    Ok(scene)
}
