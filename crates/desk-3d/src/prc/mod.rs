// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! PRC (ISO 14739-1), the 3D of PDF documents made by CAD programs
//! (D-033).
//!
//! A file holds file structures, each with sections packed with zlib:
//! the globals (colors, materials and styles), the tree (the parts, each
//! with its representation items, and the product occurrences that place
//! them), the tessellations (the triangles of the items) and the exact
//! geometry, which is not read: what is drawn is the tessellation. The
//! product occurrences at the root are those no other one holds or takes
//! as its prototype, as the model file would name them.
//!
//! A tree with what this reader does not know is left out, and the
//! tessellations are drawn where they are, in the color of their faces.
//! The file is Z up, as the 3D of PDF is, and turned to Y up.

mod bits;
mod doubles;

use std::collections::HashMap;
use std::io::Read as _;

use bits::Bits;

use crate::matrix::{IDENTITY, Matrix, invert, mul, normal, point, put};
use crate::{Error, Material, Mesh, Result, Scene, limits, malformed};

const MAGIC: &[u8] = b"PRC";

// The types of the entities that are read.
const TESS_3D: u32 = 172;
const TESS_3D_COMPRESSED: u32 = 173;
const TESS_FACE: u32 = 174;
const TESS_3D_WIRE: u32 = 175;
const TESS_MARKUP: u32 = 176;
const ATTRIBUTE: u32 = 201;
const CARTESIAN: u32 = 202;
const GENERAL: u32 = 207;
const RI_BREP: u32 = 232;
const RI_CURVE: u32 = 233;
const RI_POINT_SET: u32 = 236;
const RI_POLY_BREP: u32 = 237;
const RI_POLY_WIRE: u32 = 238;
const RI_SET: u32 = 239;
const RI_COORDINATE_SYSTEM: u32 = 240;
const FILE_STRUCTURE: u32 = 302;
const GLOBALS: u32 = 303;
const TREE: u32 = 304;
const TESSELLATION: u32 = 305;
const PRODUCT_OCCURRENCE: u32 = 310;
const PART_DEFINITION: u32 = 311;
const STYLE: u32 = 701;
const MATERIAL: u32 = 702;
const PICTURE: u32 = 703;
const TEXTURE_APPLICATION: u32 = 711;
const TEXTURE_DEFINITION: u32 = 712;
const LINE_PATTERN: u32 = 721;

// The kinds of triangles of a face, as bits of its `used_entities_flag`.
const TRIANGLE: u32 = 0x2;
const FAN: u32 = 0x4;
const STRIP: u32 = 0x8;
const TEXTURED: u32 = 0x100;
const POLYFACE: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
const NORMAL_SINGLE: u32 = 0x4000_0000;
const NORMAL_MASK: u32 = 0x3FFF_FFFF;

/// Whether the bytes are those of a PRC file.
pub(crate) fn is_prc(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC) && bytes.len() > 11
}

/// The bytes of a file, read as little-endian words.
struct Words<'a> {
    data: &'a [u8],
    at: usize,
}

impl Words<'_> {
    fn u32(&mut self) -> Result<u32> {
        let w = self
            .data
            .get(self.at..self.at + 4)
            .and_then(|w| <[u8; 4]>::try_from(w).ok())
            .ok_or_else(|| malformed("a PRC header that ends early"))?;
        self.at += 4;
        Ok(u32::from_le_bytes(w))
    }

    fn uuid(&mut self) -> Result<[u32; 4]> {
        Ok([self.u32()?, self.u32()?, self.u32()?, self.u32()?])
    }
}

/// A section, unpacked, within what is left of the bytes that the
/// sections of a file may unpack to: many file structures can name the
/// same section.
fn inflate(bytes: &[u8], from: u32, left: &mut u64) -> Result<Vec<u8>> {
    let packed = bytes
        .get(from as usize..)
        .ok_or_else(|| malformed("a PRC section past the end of its file"))?;
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(packed)
        .take(*left + 1)
        .read_to_end(&mut out)
        .map_err(|_| malformed("a PRC section that does not unpack"))?;
    *left = left.checked_sub(out.len() as u64).ok_or(Error::TooLarge)?;
    Ok(out)
}

/// The graphics of an entity: the style its faces take.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Graphics {
    style: Option<usize>,
}

/// A section being read, with what its entities take from those before
/// them: the name and the graphics of the last.
struct Reader<'a> {
    bits: Bits<'a>,
    graphics: Graphics,
}

/// A style: a color, by its index in the doubles of the colors, or a
/// material; and how transparent.
#[derive(Clone, Copy, Debug)]
struct Style {
    material: bool,
    index: Option<usize>,
    transparency: Option<u8>,
}

/// A material: its diffuse and emissive colors, by their index in the
/// doubles of the colors, and alpha.
#[derive(Clone, Copy, Debug)]
enum Look {
    Generic {
        diffuse: Option<usize>,
        emissive: Option<usize>,
        alpha: f64,
    },
    Texture {
        generic: Option<usize>,
    },
}

#[derive(Default)]
struct Globals {
    colors: Vec<f64>,
    looks: Vec<Look>,
    styles: Vec<Style>,
    /// The coordinate systems that items name as their own.
    systems: Vec<Matrix>,
}

/// What an item of the tree draws.
#[derive(Clone, Debug)]
struct Item {
    tessellation: Option<usize>,
    style: Option<usize>,
    system: Option<usize>,
    children: Vec<Item>,
}

#[derive(Clone, Debug)]
struct Occurrence {
    part: Option<usize>,
    prototype: Option<usize>,
    sons: Vec<usize>,
    location: Matrix,
    style: Option<usize>,
}

#[derive(Default)]
struct Tree {
    parts: Vec<(Vec<Item>, Option<usize>)>,
    occurrences: Vec<Occurrence>,
}

/// A face of a tessellation: its triangles by the corners of
/// `corners`, and the style it names.
struct Face {
    style: Option<usize>,
    /// Its first corner in the tessellation, that of its first color.
    first: usize,
    triangles: Vec<[u32; 3]>,
    colors: Vec<[f32; 3]>,
}

/// A tessellation of triangles: its corners, each as its point and its
/// normal, and its faces.
struct Tess {
    points: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    faces: Vec<Face>,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            bits: Bits::new(data),
            graphics: Graphics { style: None },
        }
    }

    fn expect(&mut self, kind: u32) -> Result<()> {
        if self.bits.uint()? != kind {
            return Err(malformed(format!(
                "a PRC entity that is not of type {kind}"
            )));
        }
        Ok(())
    }

    fn skip_count<T>(&mut self, mut each: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let n = self.bits.count()?;
        (0..n).map(|_| each(self)).collect()
    }

    fn attribute_entry(&mut self) -> Result<()> {
        if self.bits.bit()? {
            self.bits.uint()?;
        } else {
            self.bits.string()?;
        }
        Ok(())
    }

    fn attributes(&mut self) -> Result<()> {
        self.skip_count(|r| {
            r.expect(ATTRIBUTE)?;
            r.attribute_entry()?;
            r.skip_count(|r| {
                r.attribute_entry()?;
                match r.bits.uint()? {
                    1 => drop(r.bits.int()?),
                    2 => drop(r.bits.double()?),
                    3 => drop(r.bits.uint()?),
                    4 => drop(r.bits.string()?),
                    _ => {}
                }
                Ok(())
            })?;
            Ok(())
        })?;
        Ok(())
    }

    /// The base of an entity: its attributes, its name, and its
    /// identifiers when it can be named by others.
    fn base(&mut self, referable: bool) -> Result<()> {
        self.attributes()?;
        if !self.bits.bit()? {
            self.bits.string()?;
        }
        if referable {
            for _ in 0..3 {
                self.bits.uint()?;
            }
        }
        Ok(())
    }

    fn graphics(&mut self) -> Result<Graphics> {
        if !self.bits.bit()? {
            self.bits.uint()?; // the layer
            self.graphics = Graphics {
                style: self.bits.index()?,
            };
            self.bits.char()?;
            self.bits.char()?;
        }
        Ok(self.graphics)
    }

    fn user_data(&mut self) -> Result<()> {
        let n = self.bits.uint()? as usize;
        if n > self.bits.left() {
            return Err(malformed("user data of the PRC past its end"));
        }
        for _ in 0..n {
            self.bits.bit()?;
        }
        Ok(())
    }

    fn vector(&mut self) -> Result<[f64; 3]> {
        Ok([
            self.bits.double()?,
            self.bits.double()?,
            self.bits.double()?,
        ])
    }

    /// A transformation of three dimensions, of either kind, as a matrix
    /// that takes points from the row on its left.
    fn transformation(&mut self) -> Result<Matrix> {
        match self.bits.uint()? {
            GENERAL => {
                let mut m = IDENTITY;
                for v in m.iter_mut() {
                    *v = self.bits.double()?;
                }
                Ok(m)
            }
            CARTESIAN => self.cartesian(),
            _ => Err(malformed("a PRC transformation of no known type")),
        }
    }

    fn cartesian(&mut self) -> Result<Matrix> {
        let behaviour = self.bits.char()?;
        let origin = if behaviour & 0x01 != 0 {
            self.vector()?
        } else {
            [0.0; 3]
        };
        let (mut x, mut y, mut z) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
        if behaviour & 0x20 != 0 {
            x = self.vector()?;
            y = self.vector()?;
            z = self.vector()?;
        } else if behaviour & 0x02 != 0 {
            x = self.vector()?;
            y = self.vector()?;
            z = cross(x, y);
        }
        // A mirror turns Z around, unless the three axes are given.
        if behaviour & 0x04 != 0 && behaviour & 0x20 == 0 {
            z = z.map(|v| -v);
        }
        let mut s = [1.0; 3];
        if behaviour & 0x10 != 0 {
            s = self.vector()?;
        } else if behaviour & 0x08 != 0 {
            s = [self.bits.double()?; 3];
        }
        if behaviour & 0x40 != 0 {
            for _ in 0..4 {
                self.bits.double()?;
            }
        }
        let mut m = IDENTITY;
        for (row, (axis, scale)) in [x, y, z].iter().zip(s).enumerate() {
            for (column, v) in axis.iter().enumerate() {
                put(&mut m, row, column, v * scale);
            }
        }
        for (column, v) in origin.iter().enumerate() {
            put(&mut m, 3, column, *v);
        }
        Ok(m)
    }

    // The globals.

    fn globals(&mut self) -> Result<Globals> {
        if self.bits.uint()? != 0 {
            return Err(malformed("a PRC schema, which this reader does not know"));
        }
        self.expect(GLOBALS)?;
        self.base(false)?;
        self.skip_count(|r| {
            for _ in 0..4 {
                r.bits.uint()?;
            }
            Ok(())
        })?;
        self.bits.double()?;
        self.bits.double()?;
        self.bits.string()?;
        self.skip_count(|r| {
            r.bits.string()?;
            r.bits.uint()?;
            r.skip_count(|r| {
                r.bits.uint()?;
                r.bits.char()
            })?;
            Ok(())
        })?;
        let mut globals = Globals::default();
        for _ in 0..self.bits.count()? {
            globals.colors.extend(self.vector()?);
        }
        self.skip_count(|r| {
            r.expect(PICTURE)?;
            r.base(false)?;
            r.bits.int()?;
            for _ in 0..3 {
                r.bits.uint()?;
            }
            Ok(())
        })?;
        self.skip_count(Self::texture_definition)?;
        globals.looks = self.skip_count(Self::look)?;
        self.skip_count(|r| {
            r.expect(LINE_PATTERN)?;
            r.base(true)?;
            r.skip_count(|r| r.bits.double())?;
            r.bits.double()?;
            r.bits.bit()
        })?;
        globals.styles = self.skip_count(Self::style)?;
        if self.bits.count()? != 0 {
            return Err(malformed(
                "PRC fill patterns, which this reader does not know",
            ));
        }
        let systems = self.skip_count(|r| r.item(0))?;
        globals.systems = systems
            .into_iter()
            .map(|(_, system)| system.unwrap_or(IDENTITY))
            .collect();
        self.user_data()?;
        Ok(globals)
    }

    fn texture_definition(&mut self) -> Result<()> {
        self.expect(TEXTURE_DEFINITION)?;
        self.base(true)?;
        self.bits.uint()?; // the picture
        let dimension = self.bits.char()?;
        if self.bits.int()? == 3 {
            return Err(malformed("a PRC texture of a mapping operator"));
        }
        self.bits.uint()?;
        self.skip_count(|r| r.bits.double())?;
        self.skip_count(|r| r.bits.char())?;
        if self.bits.int()? == 3 {
            for _ in 0..4 {
                self.bits.double()?;
            }
        }
        for _ in 0..2 {
            if self.bits.int()? != 0 {
                self.bits.int()?;
            }
        }
        if self.bits.char()? & 0x02 != 0 {
            self.bits.int()?;
            self.bits.double()?;
        }
        for _ in 0..dimension.clamp(1, 3) {
            self.bits.int()?;
        }
        if self.bits.bit()? {
            return Err(malformed("a PRC texture transformation"));
        }
        Ok(())
    }

    fn look(&mut self) -> Result<Look> {
        match self.bits.uint()? {
            MATERIAL => {
                self.base(true)?;
                self.bits.uint()?;
                let diffuse = self.bits.index()?;
                let emissive = self.bits.index()?;
                self.bits.uint()?;
                self.bits.double()?;
                self.bits.double()?;
                let alpha = self.bits.double()?;
                self.bits.double()?;
                self.bits.double()?;
                Ok(Look::Generic {
                    diffuse,
                    emissive,
                    alpha,
                })
            }
            TEXTURE_APPLICATION => {
                self.base(true)?;
                let generic = self.bits.index()?;
                for _ in 0..3 {
                    self.bits.uint()?;
                }
                Ok(Look::Texture { generic })
            }
            _ => Err(malformed("a PRC material of no known type")),
        }
    }

    fn style(&mut self) -> Result<Style> {
        self.expect(STYLE)?;
        self.base(true)?;
        self.bits.double()?;
        self.bits.bit()?;
        self.bits.uint()?;
        let material = self.bits.bit()?;
        let index = self.bits.index()?;
        let transparency = if self.bits.bit()? {
            Some(self.bits.char()?)
        } else {
            None
        };
        for _ in 0..3 {
            if self.bits.bit()? {
                self.bits.char()?;
            }
        }
        Ok(Style {
            material,
            index,
            transparency,
        })
    }

    // The tree.

    /// A representation item, and the transformation of a coordinate
    /// system.
    fn item(&mut self, depth: usize) -> Result<(Item, Option<Matrix>)> {
        if depth > limits::DEPTH {
            return Err(Error::TooLarge);
        }
        let kind = self.bits.uint()?;
        self.base(true)?;
        let graphics = self.graphics()?;
        let mut item = Item {
            system: self.bits.index()?,
            tessellation: self.bits.index()?,
            style: graphics.style,
            children: Vec::new(),
        };
        let mut system = None;
        match kind {
            RI_BREP => {
                if self.bits.bit()? {
                    self.bits.uint()?;
                    self.bits.uint()?;
                }
                self.bits.bit()?;
            }
            RI_CURVE => {
                if self.bits.bit()? {
                    self.bits.uint()?;
                    self.bits.uint()?;
                }
            }
            RI_POINT_SET => {
                self.skip_count(Self::vector)?;
            }
            RI_POLY_BREP => {
                self.bits.bit()?;
            }
            RI_POLY_WIRE => {}
            RI_SET => {
                item.children = self
                    .skip_count(|r| r.item(depth + 1))?
                    .into_iter()
                    .map(|(child, _)| child)
                    .collect();
            }
            RI_COORDINATE_SYSTEM => {
                system = Some(self.transformation()?);
            }
            _ => {
                return Err(malformed(format!(
                    "a PRC representation item of type {kind}, which this reader does not know"
                )));
            }
        }
        self.user_data()?;
        Ok((item, system))
    }

    /// The markups and views of a part or a product occurrence, which
    /// are only read when there are none.
    fn markups_and_views(&mut self) -> Result<()> {
        for _ in 0..4 {
            if self.bits.uint()? != 0 {
                return Err(malformed("PRC markups, which this reader does not know"));
            }
        }
        if self.bits.uint()? != 0 {
            return Err(malformed("PRC views, which this reader does not know"));
        }
        Ok(())
    }

    fn part(&mut self) -> Result<(Vec<Item>, Option<usize>)> {
        self.expect(PART_DEFINITION)?;
        self.base(true)?;
        let style = self.graphics()?.style;
        for _ in 0..2 {
            self.vector()?;
        }
        let items = self
            .skip_count(|r| r.item(0))?
            .into_iter()
            .map(|(item, _)| item)
            .collect();
        self.markups_and_views()?;
        self.user_data()?;
        Ok((items, style))
    }

    fn occurrence(&mut self) -> Result<Occurrence> {
        self.expect(PRODUCT_OCCURRENCE)?;
        self.base(true)?;
        let style = self.graphics()?.style;
        let part = self.bits.index()?;
        let prototype = self.bits.index()?;
        if prototype.is_some() && !self.bits.bit()? {
            return Err(malformed("a PRC prototype in another file structure"));
        }
        if self.bits.index()?.is_some() && !self.bits.bit()? {
            return Err(malformed("PRC external data in another file structure"));
        }
        let sons = self.skip_count(|r| Ok(r.bits.uint()? as usize))?;
        self.bits.char()?;
        self.bits.bit()?;
        self.bits.double()?;
        self.bits.char()?;
        self.bits.int()?;
        let location = if self.bits.bit()? {
            self.transformation()?
        } else {
            IDENTITY
        };
        if self.bits.uint()? != 0 {
            return Err(malformed("PRC references, which this reader does not know"));
        }
        self.markups_and_views()?;
        if self.bits.bit()? || self.bits.uint()? != 0 {
            return Err(malformed("PRC filters, which this reader does not know"));
        }
        if self.bits.uint()? != 0 {
            return Err(malformed(
                "PRC scene parameters, which this reader does not know",
            ));
        }
        self.user_data()?;
        Ok(Occurrence {
            part,
            prototype,
            sons,
            location,
            style,
        })
    }

    fn tree(&mut self) -> Result<Tree> {
        self.expect(TREE)?;
        self.base(false)?;
        let parts = self.skip_count(Self::part)?;
        let occurrences = self.skip_count(Self::occurrence)?;
        self.expect(FILE_STRUCTURE)?;
        Ok(Tree { parts, occurrences })
    }

    // The tessellations.

    fn tessellations(&mut self) -> Result<Vec<Option<Tess>>> {
        self.expect(TESSELLATION)?;
        self.base(false)?;
        let n = self.bits.count()?;
        let mut out = Vec::new();
        for _ in 0..n {
            // What cannot be read ends the section, and keeps what came
            // before.
            let tess = match self.bits.uint()? {
                TESS_3D => self.tess(),
                TESS_3D_WIRE => self.wire().map(|()| None),
                TESS_MARKUP => self.markup().map(|()| None),
                TESS_3D_COMPRESSED => break,
                _ => break,
            };
            match tess {
                Ok(t) => out.push(t),
                Err(Error::TooLarge) => return Err(Error::TooLarge),
                Err(_) => break,
            }
        }
        Ok(out)
    }

    fn doubles(&mut self) -> Result<Vec<f64>> {
        let n = self.bits.count()?;
        (0..n).map(|_| self.bits.double()).collect()
    }

    fn uints(&mut self) -> Result<Vec<u32>> {
        let n = self.bits.count()?;
        (0..n).map(|_| self.bits.uint()).collect()
    }

    fn wire(&mut self) -> Result<()> {
        self.bits.bit()?;
        self.doubles()?;
        self.uints()?;
        if self.bits.bit()? {
            return Err(malformed("colors of a PRC wire"));
        }
        Ok(())
    }

    fn markup(&mut self) -> Result<()> {
        self.bits.bit()?;
        self.doubles()?;
        self.uints()?;
        self.skip_count(|r| r.bits.string())?;
        self.bits.string()?;
        self.bits.char()?;
        Ok(())
    }

    fn tess(&mut self) -> Result<Option<Tess>> {
        self.bits.bit()?;
        let coordinates = self.doubles()?;
        self.bits.bit()?;
        self.bits.bit()?;
        if self.bits.bit()? {
            self.bits.char()?;
            self.bits.double()?;
        }
        let normals = self.doubles()?;
        let has_normals = !normals.is_empty();
        self.uints()?; // the wires
        let indices = self.uints()?;
        let n = self.bits.count()?;
        let mut tess = Tess {
            points: Vec::new(),
            normals: Vec::new(),
            faces: Vec::new(),
        };
        let triple = |values: &[f64], at: u32| -> Option<[f32; 3]> {
            let at = at as usize;
            let v = values.get(at..at.checked_add(3)?)?;
            let v = [v.first()?, v.get(1)?, v.get(2)?].map(|v| *v as f32);
            v.iter().all(|c| c.is_finite()).then_some(v)
        };
        let mut corners = 0usize;
        for _ in 0..n {
            self.expect(TESS_FACE)?;
            let lines = self.skip_count(|r| r.bits.index())?;
            self.bits.uint()?;
            self.uints()?;
            let flags = self.bits.uint()?;
            let start = self.bits.uint()? as usize;
            let sizes = self.uints()?;
            let uv_count = self.bits.uint()? as usize;
            // The corners of the face, each its normal and its point.
            let mut face_corners: Vec<(Option<u32>, u32)> = Vec::new();
            let mut triangles: Vec<[usize; 3]> = Vec::new();
            let mut at = start;
            let mut next = || -> Option<u32> {
                let v = indices.get(at).copied();
                at += 1;
                v
            };
            let mut sizes = sizes.into_iter();
            let mut usable = flags & POLYFACE == 0;
            'kinds: for kind in [
                TRIANGLE,
                FAN,
                STRIP,
                TRIANGLE << 4,
                FAN << 4,
                STRIP << 4,
                TRIANGLE << 8,
                FAN << 8,
                STRIP << 8,
                TRIANGLE << 12,
                FAN << 12,
                STRIP << 12,
            ] {
                if flags & kind == 0 || !usable {
                    continue;
                }
                // The variants of one normal are the bits 4 to 7 and 12
                // to 15.
                let shift = kind.trailing_zeros();
                let one_normal = (4..8).contains(&shift) || (12..16).contains(&shift);
                let uvs = if kind >= TEXTURED { uv_count } else { 0 };
                let base = kind >> (shift / 4 * 4);
                // A corner, its normal first when the tessellation has
                // normals and the corner is not of one that has one.
                let mut corner = |normal: Option<u32>, next: &mut dyn FnMut() -> Option<u32>| {
                    let normal = match normal {
                        Some(n) => Some(n),
                        None if has_normals => Some(next()?),
                        None => None,
                    };
                    for _ in 0..uvs {
                        next()?;
                    }
                    let point = next()?;
                    face_corners.push((normal, point));
                    Some(face_corners.len() - 1)
                };
                match base {
                    TRIANGLE => {
                        let Some(count) = sizes.next() else {
                            usable = false;
                            continue;
                        };
                        for _ in 0..count {
                            let normal = if one_normal && has_normals {
                                match next() {
                                    Some(n) => Some(n),
                                    None => {
                                        usable = false;
                                        continue 'kinds;
                                    }
                                }
                            } else {
                                None
                            };
                            let mut t = [0usize; 3];
                            for c in &mut t {
                                match corner(normal, &mut next) {
                                    Some(i) => *c = i,
                                    None => {
                                        usable = false;
                                        continue 'kinds;
                                    }
                                }
                            }
                            triangles.push(t);
                        }
                    }
                    _ => {
                        let Some(fans) = sizes.next() else {
                            usable = false;
                            continue;
                        };
                        for _ in 0..fans {
                            let Some(size) = sizes.next() else {
                                usable = false;
                                continue 'kinds;
                            };
                            let count = (size & NORMAL_MASK) as usize;
                            let normal = if one_normal && has_normals && size & NORMAL_SINGLE != 0 {
                                match next() {
                                    Some(n) => Some(n),
                                    None => {
                                        usable = false;
                                        continue 'kinds;
                                    }
                                }
                            } else {
                                None
                            };
                            let mut fan = Vec::new();
                            for _ in 0..count {
                                match corner(normal, &mut next) {
                                    Some(i) => fan.push(i),
                                    None => {
                                        usable = false;
                                        continue 'kinds;
                                    }
                                }
                            }
                            if base == FAN {
                                if let Some((&first, rest)) = fan.split_first() {
                                    for [b, c] in rest.array_windows::<2>() {
                                        triangles.push([first, *b, *c]);
                                    }
                                }
                            } else {
                                for (k, [a, b, c]) in fan.array_windows::<3>().enumerate() {
                                    triangles.push(if k % 2 == 0 {
                                        [*a, *b, *c]
                                    } else {
                                        [*b, *a, *c]
                                    });
                                }
                            }
                        }
                    }
                }
            }
            // What is decoded counts, kept or not.
            corners += face_corners.len();
            if corners > limits::CORNERS {
                return Err(Error::TooLarge);
            }
            // The colors of its corners, as many as it has: those of a
            // face that is not decoded cannot be counted, and end the
            // tessellation.
            let mut colors = Vec::new();
            if self.bits.bit()? {
                if !usable {
                    return Err(malformed("colors of a PRC face that is not read"));
                }
                let rgba = self.bits.bit()?;
                if self.bits.bit()? {
                    return Err(malformed("optimised colors of a PRC face"));
                }
                let width = if rgba { 4 } else { 3 };
                let mut last = [0u8; 4];
                for i in 0..face_corners.len() {
                    if i == 0 || !self.bits.bit()? {
                        for c in last.iter_mut().take(width) {
                            *c = self.bits.char()?;
                        }
                    }
                    colors.push(
                        [0, 1, 2].map(|k| f32::from(last.get(k).copied().unwrap_or(0)) / 255.0),
                    );
                }
            }
            if !lines.is_empty() {
                self.bits.uint()?;
            }
            // A face with no triangle draws nothing.
            if !usable || triangles.is_empty() {
                continue;
            }
            let base = tess.points.len();
            let mut kept = Vec::with_capacity(face_corners.len());
            for (normal, point) in &face_corners {
                let Some(p) = triple(&coordinates, *point) else {
                    usable = false;
                    break;
                };
                kept.push(p);
                if let Some(n) = normal.and_then(|n| triple(&normals, n)) {
                    tess.normals.push(n);
                }
            }
            if !usable {
                tess.normals.truncate(tess.points.len());
                continue;
            }
            tess.points.extend(kept);
            if tess.normals.len() != tess.points.len() {
                tess.normals.clear();
            }
            tess.faces.push(Face {
                style: lines.first().copied().flatten(),
                first: base,
                triangles: triangles
                    .into_iter()
                    .map(|t| t.map(|c| (base + c) as u32))
                    .collect(),
                colors: if colors.len() == face_corners.len() {
                    colors
                } else {
                    Vec::new()
                },
            });
        }
        self.doubles()?; // the texture coordinates
        Ok(Some(tess))
    }
}

/// What a file structure holds that is drawn.
struct Structure {
    globals: Globals,
    tree: Option<Tree>,
    tessellations: Vec<Option<Tess>>,
}

impl Globals {
    /// The material of a style.
    fn material(&self, style: Option<usize>) -> Material {
        let Some(style) = style.and_then(|i| self.styles.get(i)) else {
            return Material::default();
        };
        let rgb = |at: Option<usize>| -> Option<[f64; 3]> {
            let at = at?;
            let c = self.colors.get(at..at.checked_add(3)?)?;
            Some([*c.first()?, *c.get(1)?, *c.get(2)?])
        };
        // The color of a material is its diffuse color with the light it
        // gives itself, which is all that a model drawn with no light
        // has.
        let (color, mut alpha) = if style.material {
            let mut look = style.index.and_then(|i| self.looks.get(i));
            if let Some(Look::Texture { generic }) = look {
                look = generic.and_then(|i| self.looks.get(i));
            }
            match look {
                Some(Look::Generic {
                    diffuse,
                    emissive,
                    alpha,
                }) => {
                    let color = match (rgb(*diffuse), rgb(*emissive)) {
                        (Some(d), Some(e)) => Some([d[0] + e[0], d[1] + e[1], d[2] + e[2]]),
                        (d, e) => d.or(e),
                    };
                    (color, *alpha)
                }
                _ => (None, 1.0),
            }
        } else {
            (rgb(style.index), 1.0)
        };
        if let Some(t) = style.transparency {
            alpha = f64::from(t) / 255.0;
        }
        let Some(rgb) = color else {
            return Material::default();
        };
        let alpha = alpha.clamp(0.0, 1.0) as f32;
        let mut material = Material {
            color: [0.0, 0.0, 0.0, alpha],
            translucent: alpha < 1.0,
            ..Material::default()
        };
        for (c, v) in material.color.iter_mut().zip(rgb) {
            *c = v.clamp(0.0, 1.0) as f32;
        }
        material
    }
}

/// Z up turned to Y up.
const STAGE: Matrix = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 0.0, -1.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// The scene being made, and how much of it there is.
#[derive(Default)]
struct Builder {
    scene: Scene,
    corners: usize,
    /// The occurrences and items visited, which a tree that holds the
    /// same ones many times makes many.
    visits: usize,
}

impl Builder {
    fn visit(&mut self, depth: usize) -> Result<()> {
        self.visits += 1;
        if self.visits > limits::PRIMS || depth > limits::DEPTH {
            return Err(Error::TooLarge);
        }
        Ok(())
    }

    /// A tessellation, placed by a matrix and in a style its faces do not
    /// name themselves: one mesh for each material.
    fn draw(
        &mut self,
        globals: &Globals,
        tess: &Tess,
        m: &Matrix,
        style: Option<usize>,
    ) -> Result<()> {
        let m = mul(m, &STAGE);
        let inverse = invert(&m);
        let mut meshes: HashMap<([u32; 4], bool), usize> = HashMap::new();
        'faces: for face in &tess.faces {
            // The corners of the face, each once, placed; a face with a
            // corner that is not finite there is left out.
            let mut taken: HashMap<u32, u32> = HashMap::new();
            let (mut positions, mut normals, mut colors) = (Vec::new(), Vec::new(), Vec::new());
            let mut triangles = Vec::with_capacity(face.triangles.len());
            for t in &face.triangles {
                let mut out = [0u32; 3];
                for (o, &c) in out.iter_mut().zip(t) {
                    if let Some(&i) = taken.get(&c) {
                        *o = i;
                        continue;
                    }
                    let Some(&p) = tess.points.get(c as usize) else {
                        return Err(malformed("a PRC triangle past its corners"));
                    };
                    self.corners += 1;
                    if self.corners > limits::CORNERS {
                        return Err(Error::TooLarge);
                    }
                    let p = point(&m, p);
                    if !p.iter().all(|v| v.is_finite()) {
                        continue 'faces;
                    }
                    let i = u32::try_from(positions.len()).map_err(|_| Error::TooLarge)?;
                    positions.push(p);
                    if let (Some(n), Some(inverse)) =
                        (tess.normals.get(c as usize), inverse.as_ref())
                    {
                        let n = unit(normal(inverse, *n));
                        if n.iter().all(|v| v.is_finite()) {
                            normals.push(n);
                        }
                    }
                    if let Some(color) = face.colors.get((c as usize).wrapping_sub(face.first)) {
                        colors.push(*color);
                    }
                    taken.insert(c, i);
                    *o = i;
                }
                triangles.push(out);
            }
            let material = globals.material(face.style.or(style));
            let colored = !face.colors.is_empty();
            let key = (material.color.map(f32::to_bits), colored);
            let at = *meshes.entry(key).or_insert_with(|| {
                self.scene.meshes.push(Mesh {
                    material,
                    ..Mesh::default()
                });
                self.scene.meshes.len() - 1
            });
            let Some(mesh) = self.scene.meshes.get_mut(at) else {
                continue;
            };
            let base = u32::try_from(mesh.positions.len())
                .ok()
                .filter(|b| b.checked_add(positions.len() as u32).is_some())
                .ok_or(Error::TooLarge)?;
            // The normals and the colors of a mesh are those of all its
            // corners, or none.
            if mesh.normals.len() == mesh.positions.len() && normals.len() == positions.len() {
                mesh.normals.extend(normals);
            } else {
                mesh.normals.clear();
            }
            if mesh.colors.len() == mesh.positions.len() && colors.len() == positions.len() {
                mesh.colors.extend(colors);
            } else {
                mesh.colors.clear();
            }
            mesh.positions.extend(positions);
            mesh.triangles
                .extend(triangles.into_iter().map(|t| t.map(|c| base + c)));
        }
        Ok(())
    }

    fn occurrence(
        &mut self,
        s: &Structure,
        tree: &Tree,
        index: usize,
        parent: &Matrix,
        style: Option<usize>,
        depth: usize,
    ) -> Result<()> {
        self.visit(depth)?;
        let Some(o) = tree.occurrences.get(index) else {
            return Ok(());
        };
        let world = mul(&o.location, parent);
        let style = o.style.or(style);
        // A prototype gives the part and the sons this one does not.
        let (mut part, mut sons) = (o.part, &o.sons);
        let mut prototype = o.prototype;
        let mut hops = 0;
        while let Some(p) = prototype.and_then(|p| tree.occurrences.get(p)) {
            hops += 1;
            if hops > limits::DEPTH {
                return Err(Error::TooLarge);
            }
            part = part.or(p.part);
            if sons.is_empty() {
                sons = &p.sons;
            }
            prototype = p.prototype;
        }
        if let Some((items, part_style)) = part.and_then(|p| tree.parts.get(p)) {
            for item in items {
                self.item(s, item, &world, part_style.or(style), depth + 1)?;
            }
        }
        for &son in sons {
            self.occurrence(s, tree, son, &world, style, depth + 1)?;
        }
        Ok(())
    }

    fn item(
        &mut self,
        s: &Structure,
        item: &Item,
        parent: &Matrix,
        style: Option<usize>,
        depth: usize,
    ) -> Result<()> {
        self.visit(depth)?;
        let m = match item.system.and_then(|i| s.globals.systems.get(i)) {
            Some(system) => mul(system, parent),
            None => *parent,
        };
        let style = item.style.or(style);
        if let Some(tess) = item
            .tessellation
            .and_then(|t| s.tessellations.get(t))
            .and_then(Option::as_ref)
        {
            self.draw(&s.globals, tess, &m, style)?;
        }
        for child in &item.children {
            self.item(s, child, &m, style, depth + 1)?;
        }
        Ok(())
    }

    fn structure(&mut self, s: &Structure) -> Result<()> {
        if let Some(tree) = &s.tree
            && !tree.occurrences.is_empty()
        {
            let mut held = vec![false; tree.occurrences.len()];
            for o in &tree.occurrences {
                for &i in o.sons.iter().chain(&o.prototype) {
                    if let Some(h) = held.get_mut(i) {
                        *h = true;
                    }
                }
            }
            for (i, _) in held.iter().enumerate().filter(|(_, h)| !**h) {
                self.occurrence(s, tree, i, &IDENTITY, None, 0)?;
            }
            return Ok(());
        }
        for tess in s.tessellations.iter().flatten() {
            self.draw(&s.globals, tess, &IDENTITY, None)?;
        }
        Ok(())
    }
}

/// Reads a PRC file, as a PDF holds it.
pub(crate) fn read(bytes: &[u8]) -> Result<Scene> {
    if !is_prc(bytes) {
        return Err(Error::Unknown);
    }
    let mut words = Words {
        data: bytes,
        at: MAGIC.len(),
    };
    words.u32()?; // the version that reads it
    words.u32()?; // the version that wrote it
    words.uuid()?;
    words.uuid()?;
    let n = words.u32()? as usize;
    // Each file structure takes at least six words of the header.
    if n > bytes.len() / 24 {
        return Err(malformed("more PRC file structures than the file holds"));
    }
    let mut offsets = Vec::with_capacity(n);
    for _ in 0..n {
        words.uuid()?;
        words.u32()?;
        let k = words.u32()? as usize;
        if k > bytes.len() / 4 {
            return Err(malformed("more PRC sections than the file holds"));
        }
        offsets.push((0..k).map(|_| words.u32()).collect::<Result<Vec<u32>>>()?);
    }
    let mut builder = Builder::default();
    let mut read_any = false;
    let mut left = limits::FILE_BYTES;
    // A section of a file structure: none when it has none or it does
    // not read, and an error when it is too large.
    fn section<T>(
        bytes: &[u8],
        offsets: &[u32],
        i: usize,
        left: &mut u64,
        parse: impl FnOnce(&mut Reader<'_>) -> Result<T>,
    ) -> Result<Option<T>> {
        let Some(&from) = offsets.get(i) else {
            return Ok(None);
        };
        match inflate(bytes, from, left).and_then(|d| parse(&mut Reader::new(&d))) {
            Ok(t) => Ok(Some(t)),
            Err(Error::TooLarge) => Err(Error::TooLarge),
            Err(_) => Ok(None),
        }
    }
    for offsets in offsets {
        // The sections after the header: the globals, the tree and the
        // tessellations.
        let Some(tessellations) = section(bytes, &offsets, 3, &mut left, |r| r.tessellations())?
        else {
            continue;
        };
        read_any = true;
        let globals = section(bytes, &offsets, 1, &mut left, |r| r.globals())?.unwrap_or_default();
        let tree = section(bytes, &offsets, 2, &mut left, |r| r.tree())?;
        builder.structure(&Structure {
            globals,
            tree,
            tessellations,
        })?;
    }
    if !read_any {
        return Err(malformed("a PRC file with no tessellation that reads"));
    }
    Ok(builder.scene)
}

/// A vector made of length one, or as it is when it has none.
fn unit(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length > 0.0 {
        v.map(|c| c / length)
    } else {
        v
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bits written as a section of PRC writes them.
    #[derive(Default)]
    struct Writer {
        bits: Vec<bool>,
    }

    impl Writer {
        fn bit(&mut self, b: bool) -> &mut Self {
            self.bits.push(b);
            self
        }

        fn char(&mut self, c: u8) -> &mut Self {
            for k in (0..8).rev() {
                self.bit(c >> k & 1 == 1);
            }
            self
        }

        fn uint(&mut self, mut v: u32) -> &mut Self {
            while v != 0 {
                self.bit(true).char(v as u8);
                v >>= 8;
            }
            self.bit(false)
        }

        fn uints(&mut self, values: &[u32]) -> &mut Self {
            self.uint(values.len() as u32);
            for &v in values {
                self.uint(v);
            }
            self
        }

        /// A double of 0 or 1, which have codes of their own.
        fn double(&mut self, v: f64) -> &mut Self {
            if v == 0.0 {
                return self.bit(false).bit(true);
            }
            let &(_, length, code, _) = doubles::CODES
                .iter()
                .find(|c| c.0 && f64::from_bits(c.3) == v)
                .unwrap();
            for k in (0..length).rev() {
                self.bit(code >> k & 1 == 1);
            }
            self.bit(false)
        }

        fn bytes(&self) -> Vec<u8> {
            self.bits
                .chunks(8)
                .map(|c| {
                    c.iter()
                        .enumerate()
                        .fold(0u8, |b, (i, bit)| b | (u8::from(*bit) << (7 - i)))
                })
                .collect()
        }
    }

    #[test]
    fn w116_the_fans_and_the_strips_of_a_face_are_triangles() {
        // The four corners of a square, then a fifth.
        let points = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let mut w = Writer::default();
        w.bit(false).uint(15);
        for p in points {
            for v in p {
                w.double(v);
            }
        }
        // No faces nor loops, normals not to recalculate, no normals nor
        // wires.
        w.bit(false).bit(false).bit(false).uint(0).uint(0);
        // A triangle, a fan of the square and a strip of four corners, by
        // the offsets of their points.
        w.uints(&[0, 3, 12, 0, 3, 6, 9, 0, 3, 6, 9]);
        w.uint(1).uint(TESS_FACE);
        w.uint(0).uint(0).uints(&[]);
        w.uint(TRIANGLE | FAN | STRIP)
            .uint(0)
            .uints(&[1, 1, 4, 1, 4]);
        w.uint(0).bit(false);
        w.uint(0);
        let bytes = w.bytes();
        let tess = Reader::new(&bytes).tess().unwrap().unwrap();
        assert_eq!(tess.points.len(), 11);
        let face = &tess.faces[0];
        let corners: Vec<[[f32; 3]; 3]> = face
            .triangles
            .iter()
            .map(|t| t.map(|c| tess.points[c as usize]))
            .collect();
        let p = points.map(|p| p.map(|v| v as f32));
        assert_eq!(
            corners,
            [
                [p[0], p[1], p[4]],
                [p[0], p[1], p[2]],
                [p[0], p[2], p[3]],
                [p[0], p[1], p[2]],
                [p[2], p[1], p[3]],
            ]
        );
    }

    #[test]
    fn w116_a_face_whose_corners_are_past_the_points_is_left_out() {
        let mut w = Writer::default();
        w.bit(false).uint(3);
        for _ in 0..3 {
            w.double(1.0);
        }
        w.bit(false).bit(false).bit(false).uint(0).uint(0);
        w.uints(&[0, 3, u32::MAX]);
        w.uint(1).uint(TESS_FACE);
        w.uint(0).uint(0).uints(&[]);
        w.uint(TRIANGLE).uint(0).uints(&[1]);
        w.uint(0).bit(false);
        w.uint(0);
        let bytes = w.bytes();
        let tess = Reader::new(&bytes).tess().unwrap().unwrap();
        assert!(tess.faces.is_empty());
        assert!(tess.points.is_empty());
    }

    #[test]
    fn w116_a_mirror_with_no_rotation_turns_z_around() {
        let mut w = Writer::default();
        w.char(0x05).double(1.0).double(0.0).double(0.0);
        let bytes = w.bytes();
        let m = Reader::new(&bytes).cartesian().unwrap();
        assert_eq!(crate::matrix::at(&m, 2, 2), -1.0);
        assert_eq!(crate::matrix::at(&m, 3, 0), 1.0);
    }

    /// A tessellation of the three corners of a triangle and one face.
    fn face(w: &mut Writer, flags: u32, sizes: &[u32], colors: bool) {
        w.bit(false).uint(9);
        for v in [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            w.double(v);
        }
        w.bit(false).bit(false).bit(false).uint(0).uint(0);
        w.uints(&[0, 3, 6]);
        w.uint(1).uint(TESS_FACE);
        w.uint(0).uint(0).uints(&[]);
        w.uint(flags).uint(0).uints(sizes);
        w.uint(0).bit(colors);
        if colors {
            w.bit(false).bit(false).char(255).char(0).char(0);
            w.bit(true).bit(true);
        }
        w.uint(0);
    }

    #[test]
    fn w116_a_face_with_no_triangle_is_left_out_and_its_colors_counted() {
        let mut w = Writer::default();
        face(&mut w, 0, &[], false);
        let bytes = w.bytes();
        assert!(
            Reader::new(&bytes)
                .tess()
                .unwrap()
                .unwrap()
                .faces
                .is_empty()
        );
        // The colors of a triangle, one for each corner.
        let mut w = Writer::default();
        face(&mut w, TRIANGLE, &[1], true);
        let bytes = w.bytes();
        let tess = Reader::new(&bytes).tess().unwrap().unwrap();
        assert_eq!(tess.faces[0].colors, [[1.0, 0.0, 0.0]; 3]);
        // A polyface is not read, so its colors cannot be counted.
        let mut w = Writer::default();
        face(&mut w, 0x1, &[1], true);
        let bytes = w.bytes();
        assert!(Reader::new(&bytes).tess().is_err());
    }

    #[test]
    fn w116_a_face_with_a_corner_not_finite_is_not_drawn() {
        let tess = Tess {
            points: vec![
                [0.0; 3],
                [f32::INFINITY, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
            ],
            normals: Vec::new(),
            faces: vec![
                Face {
                    style: None,
                    first: 0,
                    triangles: vec![[0, 1, 2]],
                    colors: Vec::new(),
                },
                Face {
                    style: None,
                    first: 0,
                    triangles: vec![[0, 3, 2]],
                    colors: Vec::new(),
                },
            ],
        };
        let mut builder = Builder::default();
        builder
            .draw(&Globals::default(), &tess, &IDENTITY, None)
            .unwrap();
        assert_eq!(builder.scene.triangle_count(), 1);
        assert_eq!(builder.scene.meshes[0].positions.len(), 3);
    }

    #[test]
    fn w116_the_sections_of_a_file_share_what_they_may_unpack() {
        use std::io::Write as _;
        let mut packed =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        packed.write_all(&[0; 1000]).unwrap();
        let packed = packed.finish().unwrap();
        let mut left = 1500;
        assert_eq!(inflate(&packed, 0, &mut left).unwrap().len(), 1000);
        assert_eq!(left, 500);
        assert_eq!(inflate(&packed, 0, &mut left), Err(Error::TooLarge));
    }
}
