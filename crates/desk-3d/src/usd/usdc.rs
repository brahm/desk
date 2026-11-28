// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The binary layers of USD (`usdc`, the "crate" files of OpenUSD), read
//! into a [`Layer`].
//!
//! A crate file starts with `PXR-USDC`, its version and where its table
//! of sections is. The sections hold the tokens (LZ4-compressed), the
//! strings, the fields (a token and a value each), the sets of fields,
//! the paths (as a tree, compressed) and the specs, each a path with a
//! set of fields. Values sit in the fields as 64-bit "reps": small ones
//! inline, the rest at an offset in the file, arrays of numbers often
//! compressed. Versions from 0.4.0, which every OpenUSD of the last years
//! writes, are read.

use super::{Attribute, Layer, Reference, Value, split_property};
use crate::{Error, Result, limits, lz4, malformed};

/// A place in the bytes of the file, read forward.
#[derive(Clone, Copy)]
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], at: usize) -> Self {
        Self { bytes, at }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(Error::TooLarge)?;
        let out = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| malformed("the layer is cut short"))?;
        self.at = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_| malformed("the layer is cut short"))
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.array()?))
    }

    /// A count, bounded so that what it counts can fit in the file.
    fn count(&mut self, each: usize) -> Result<usize> {
        let n = usize::try_from(self.u64()?).map_err(|_| Error::TooLarge)?;
        if n.saturating_mul(each.max(1)) > self.bytes.len() {
            return Err(malformed("a count larger than the file"));
        }
        Ok(n)
    }
}

/// Decodes the integers that `Usd_IntegerCompression` encodes: a common
/// value, two bits a value telling how its difference from the one
/// before is kept, then the differences that are not the common one.
fn decode_ints(data: &[u8], n: usize, wide: bool) -> Result<Vec<i64>> {
    let mut cursor = Cursor::new(data, 0);
    let common = if wide {
        i64::from_le_bytes(cursor.array()?)
    } else {
        i64::from(i32::from_le_bytes(cursor.array()?))
    };
    let codes = cursor.take(n.div_ceil(4))?;
    let mut out = Vec::with_capacity(n);
    let mut previous: i64 = 0;
    for i in 0..n {
        let byte = codes.get(i / 4).copied().unwrap_or(0);
        let code = (byte >> ((i % 4) * 2)) & 3;
        let delta = match (code, wide) {
            (0, _) => common,
            (1, false) => i64::from(i8::from_le_bytes(cursor.array()?)),
            (2, false) => i64::from(i16::from_le_bytes(cursor.array()?)),
            (3, false) => i64::from(i32::from_le_bytes(cursor.array()?)),
            (1, true) => i64::from(i16::from_le_bytes(cursor.array()?)),
            (2, true) => i64::from(i32::from_le_bytes(cursor.array()?)),
            _ => i64::from_le_bytes(cursor.array()?),
        };
        previous = if wide {
            previous.wrapping_add(delta)
        } else {
            i64::from((previous as i32).wrapping_add(delta as i32))
        };
        out.push(previous);
    }
    Ok(out)
}

/// Reads `n` compressed integers at the cursor: their compressed size,
/// then the LZ4 of their encoding.
fn compressed_ints(cursor: &mut Cursor<'_>, n: usize, wide: bool) -> Result<Vec<i64>> {
    if n > limits::CORNERS * 2 {
        return Err(Error::TooLarge);
    }
    let size = usize::try_from(cursor.u64()?).map_err(|_| Error::TooLarge)?;
    let data = cursor.take(size)?;
    let width = if wide { 8 } else { 4 };
    let most = width + n.div_ceil(4) + n * width;
    let encoded = lz4::decompress(data, most)?;
    decode_ints(&encoded, n, wide)
}

fn compressed_u32s(cursor: &mut Cursor<'_>, n: usize) -> Result<Vec<u32>> {
    Ok(compressed_ints(cursor, n, false)?
        .into_iter()
        .map(|v| v as u32)
        .collect())
}

/// The types of the values of a crate file that are read, by their
/// numbers in the file.
mod kind {
    pub(super) const BOOL: u8 = 1;
    pub(super) const UCHAR: u8 = 2;
    pub(super) const INT: u8 = 3;
    pub(super) const UINT: u8 = 4;
    pub(super) const INT64: u8 = 5;
    pub(super) const UINT64: u8 = 6;
    pub(super) const HALF: u8 = 7;
    pub(super) const FLOAT: u8 = 8;
    pub(super) const DOUBLE: u8 = 9;
    pub(super) const STRING: u8 = 10;
    pub(super) const TOKEN: u8 = 11;
    pub(super) const ASSET: u8 = 12;
    pub(super) const MATRIX4D: u8 = 15;
    pub(super) const QUATD: u8 = 16;
    pub(super) const QUATF: u8 = 17;
    pub(super) const QUATH: u8 = 18;
    pub(super) const VEC2D: u8 = 19;
    pub(super) const VEC4I: u8 = 30;
    pub(super) const PATH_LIST_OP: u8 = 34;
    pub(super) const REFERENCE_LIST_OP: u8 = 35;
    pub(super) const TOKEN_VECTOR: u8 = 41;
    pub(super) const SPECIFIER: u8 = 42;
    pub(super) const TIME_SAMPLES: u8 = 46;
    pub(super) const PAYLOAD: u8 = 47;
    pub(super) const DOUBLE_VECTOR: u8 = 48;
    pub(super) const PAYLOAD_LIST_OP: u8 = 55;
}

/// What a vector type holds: how many numbers, and of what.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scalar {
    Double,
    Float,
    Half,
    Int,
}

/// The size and numbers of the vector types, from `Vec2d` to `Vec4i`.
fn vector_of(ty: u8) -> Option<(usize, Scalar)> {
    if !(kind::VEC2D..=kind::VEC4I).contains(&ty) {
        return None;
    }
    let i = ty - kind::VEC2D;
    let dim = usize::from(i / 4) + 2;
    let scalar = match i % 4 {
        0 => Scalar::Double,
        1 => Scalar::Float,
        2 => Scalar::Half,
        _ => Scalar::Int,
    };
    Some((dim, scalar))
}

/// A half-precision number.
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1F);
    let fraction = f32::from(bits & 0x3FF);
    match exponent {
        0 => sign * fraction * 2f32.powi(-24),
        31 if fraction == 0.0 => sign * f32::INFINITY,
        31 => f32::NAN,
        e => sign * (1.0 + fraction / 1024.0) * 2f32.powi(e - 15),
    }
}

/// One number of a scalar type at the cursor.
fn scalar(cursor: &mut Cursor<'_>, s: Scalar) -> Result<f64> {
    Ok(match s {
        Scalar::Double => cursor.f64()?,
        Scalar::Float => f64::from(cursor.f32()?),
        Scalar::Half => f64::from(half(u16::from_le_bytes(cursor.array()?))),
        Scalar::Int => f64::from(i32::from_le_bytes(cursor.array()?)),
    })
}

/// The read crate file.
struct Crate<'a> {
    bytes: &'a [u8],
    version: (u8, u8),
    tokens: Vec<String>,
    strings: Vec<u32>,
    fields: Vec<(u32, u64)>,
    fieldsets: Vec<u32>,
    paths: Vec<String>,
}

/// A value rep: where and how a value is kept.
#[derive(Clone, Copy)]
struct Rep(u64);

impl Rep {
    fn is_array(self) -> bool {
        self.0 >> 63 & 1 == 1
    }

    fn is_inlined(self) -> bool {
        self.0 >> 62 & 1 == 1
    }

    fn is_compressed(self) -> bool {
        self.0 >> 61 & 1 == 1
    }

    fn ty(self) -> u8 {
        (self.0 >> 48) as u8
    }

    fn payload(self) -> u64 {
        self.0 & ((1 << 48) - 1)
    }
}

impl Crate<'_> {
    fn token(&self, i: u64) -> Result<&str> {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.tokens.get(i))
            .map(String::as_str)
            .ok_or_else(|| malformed("a token past the end"))
    }

    fn string(&self, i: u64) -> Result<&str> {
        let token = usize::try_from(i)
            .ok()
            .and_then(|i| self.strings.get(i))
            .ok_or_else(|| malformed("a string past the end"))?;
        self.token(u64::from(*token))
    }

    fn path(&self, i: u32) -> Result<&str> {
        self.paths
            .get(i as usize)
            .map(String::as_str)
            .ok_or_else(|| malformed("a path past the end"))
    }

    fn at(&self, rep: Rep) -> Result<Cursor<'_>> {
        let at = usize::try_from(rep.payload()).map_err(|_| Error::TooLarge)?;
        if at >= self.bytes.len() {
            return Err(malformed("a value past the end"));
        }
        Ok(Cursor::new(self.bytes, at))
    }

    /// The count of an array at the cursor.
    fn array_count(&self, cursor: &mut Cursor<'_>, each: usize) -> Result<usize> {
        if self.version < (0, 5) {
            cursor.u32()?;
        }
        let n = if self.version < (0, 7) {
            cursor.u32()? as usize
        } else {
            cursor.count(each)?
        };
        if n > limits::CORNERS * 2 || n.saturating_mul(each) > self.bytes.len() {
            return Err(Error::TooLarge);
        }
        Ok(n)
    }

    /// The integers of an array, compressed or not.
    fn int_array(&self, rep: Rep, wide: bool) -> Result<Vec<i64>> {
        let mut cursor = self.at(rep)?;
        let width = if wide { 8 } else { 4 };
        let n = self.array_count(&mut cursor, if rep.is_compressed() { 0 } else { width })?;
        if rep.is_compressed() && n >= 16 {
            return compressed_ints(&mut cursor, n, wide);
        }
        (0..n)
            .map(|_| {
                Ok(if wide {
                    cursor.u64()? as i64
                } else {
                    i64::from(cursor.u32()? as i32)
                })
            })
            .collect()
    }

    /// The numbers of an array of floats, doubles or halves, compressed
    /// or not.
    fn float_array(&self, rep: Rep, s: Scalar) -> Result<Vec<f32>> {
        let mut cursor = self.at(rep)?;
        let size = match s {
            Scalar::Double => 8,
            Scalar::Half => 2,
            _ => 4,
        };
        let n = self.array_count(&mut cursor, if rep.is_compressed() { 0 } else { size })?;
        if rep.is_compressed() && n >= 16 {
            return match cursor.u8()? {
                b'i' => Ok(compressed_ints(&mut cursor, n, false)?
                    .into_iter()
                    .map(|v| v as f32)
                    .collect()),
                b't' => {
                    let lut_size = cursor.u32()? as usize;
                    if lut_size.saturating_mul(size) > self.bytes.len() {
                        return Err(malformed("a table larger than the file"));
                    }
                    let lut = (0..lut_size)
                        .map(|_| scalar(&mut cursor, s).map(|v| v as f32))
                        .collect::<Result<Vec<f32>>>()?;
                    compressed_u32s(&mut cursor, n)?
                        .into_iter()
                        .map(|i| {
                            lut.get(i as usize)
                                .copied()
                                .ok_or_else(|| malformed("an index past its table"))
                        })
                        .collect()
                }
                _ => Err(malformed("an unknown compression of numbers")),
            };
        }
        (0..n)
            .map(|_| scalar(&mut cursor, s).map(|v| v as f32))
            .collect()
    }

    /// The items of a list op, by the bits of its header: the explicit,
    /// added, prepended and appended ones, in that order; the deleted and
    /// ordered ones are read and dropped.
    fn list_op<T>(
        &self,
        rep: Rep,
        mut item: impl FnMut(&Self, &mut Cursor<'_>) -> Result<T>,
    ) -> Result<Vec<T>> {
        let mut cursor = self.at(rep)?;
        let header = cursor.u8()?;
        let mut out = Vec::new();
        // Explicit (2), added (4), prepended (32), appended (64), deleted
        // (8), ordered (16): the order in which OpenUSD writes them.
        for (bit, keep) in [
            (2, true),
            (4, true),
            (32, true),
            (64, true),
            (8, false),
            (16, false),
        ] {
            if header & bit == 0 {
                continue;
            }
            let n = cursor.count(4)?;
            for _ in 0..n {
                let value = item(self, &mut cursor)?;
                if keep {
                    out.push(value);
                }
            }
        }
        Ok(out)
    }

    /// A reference or a payload at the cursor.
    fn reference(&self, cursor: &mut Cursor<'_>, reference: bool) -> Result<Reference> {
        let asset = self.string(u64::from(cursor.u32()?))?.to_owned();
        let prim = self.path(cursor.u32()?)?;
        let prim = (!prim.is_empty()).then(|| prim.to_owned());
        if reference || self.version >= (0, 8) {
            // The layer offset: an offset and a scale.
            cursor.f64()?;
            cursor.f64()?;
        }
        if reference {
            // The custom data, a dictionary, is not read: only an empty
            // one is.
            if cursor.u64()? != 0 {
                return Err(malformed("a reference with custom data"));
            }
        }
        Ok(Reference { asset, prim })
    }

    /// A value, as far as a model needs it; none for what it does not.
    fn value(&self, rep: Rep) -> Result<Option<Value>> {
        let ty = rep.ty();
        let inline = rep.is_inlined();
        let p = rep.payload();
        if rep.is_array() {
            return Ok(Some(match ty {
                kind::INT | kind::UINT => Value::Ints(
                    self.int_array(rep, false)?
                        .into_iter()
                        .map(|v| v as i32)
                        .collect(),
                ),
                kind::INT64 | kind::UINT64 => Value::Ints(
                    self.int_array(rep, true)?
                        .into_iter()
                        .map(|v| i32::try_from(v).unwrap_or(i32::MAX))
                        .collect(),
                ),
                kind::FLOAT => Value::Floats {
                    dim: 1,
                    data: self.float_array(rep, Scalar::Float)?,
                },
                kind::DOUBLE => Value::Floats {
                    dim: 1,
                    data: self.float_array(rep, Scalar::Double)?,
                },
                kind::HALF => Value::Floats {
                    dim: 1,
                    data: self.float_array(rep, Scalar::Half)?,
                },
                kind::TOKEN | kind::ASSET | kind::STRING => {
                    let mut cursor = self.at(rep)?;
                    let n = self.array_count(&mut cursor, 4)?;
                    let texts = (0..n)
                        .map(|_| {
                            let i = u64::from(cursor.u32()?);
                            if ty == kind::STRING {
                                self.string(i)
                            } else {
                                self.token(i)
                            }
                            .map(str::to_owned)
                        })
                        .collect::<Result<Vec<String>>>()?;
                    Value::Texts(texts)
                }
                kind::MATRIX4D => {
                    let mut cursor = self.at(rep)?;
                    let n = self.array_count(&mut cursor, 128)?;
                    let mut out = Vec::with_capacity(n);
                    for _ in 0..n {
                        let mut m = [0.0; 16];
                        for v in &mut m {
                            *v = cursor.f64()?;
                        }
                        out.push(m);
                    }
                    Value::Matrices(out)
                }
                _ => match vector_of(ty) {
                    Some((dim, s)) => {
                        let mut cursor = self.at(rep)?;
                        let size = match s {
                            Scalar::Double => 8,
                            Scalar::Half => 2,
                            _ => 4,
                        };
                        let n = self.array_count(&mut cursor, dim * size)?;
                        let mut data = Vec::with_capacity(n * dim);
                        for _ in 0..n * dim {
                            data.push(scalar(&mut cursor, s)? as f32);
                        }
                        Value::Floats { dim, data }
                    }
                    None => return Ok(None),
                },
            }));
        }
        Ok(match ty {
            kind::BOOL if inline => Some(Value::Bool(p != 0)),
            kind::UCHAR | kind::UINT if inline => Some(Value::Int(p as u32 as i64)),
            kind::INT if inline => Some(Value::Int(i64::from(p as u32 as i32))),
            kind::INT64 | kind::UINT64 if !inline => Some(Value::Int(self.at(rep)?.u64()? as i64)),
            kind::INT64 | kind::UINT64 => Some(Value::Int(p as u32 as i32 as i64)),
            kind::FLOAT if inline => Some(Value::Float(f64::from(f32::from_bits(p as u32)))),
            // A double that is a float exactly is kept as one.
            kind::DOUBLE if inline => Some(Value::Float(f64::from(f32::from_bits(p as u32)))),
            kind::DOUBLE => Some(Value::Float(self.at(rep)?.f64()?)),
            kind::HALF if inline => Some(Value::Float(f64::from(half(p as u16)))),
            kind::TOKEN if inline => Some(Value::Text(self.token(p)?.to_owned())),
            kind::STRING if inline => Some(Value::Text(self.string(p)?.to_owned())),
            kind::ASSET if inline => Some(Value::Asset(self.token(p)?.to_owned())),
            kind::SPECIFIER if inline => Some(Value::Int(p as i64)),
            kind::MATRIX4D => {
                let mut m = [0.0; 16];
                if inline {
                    // The diagonal, as four signed bytes.
                    for (i, b) in (p as u32).to_le_bytes().iter().enumerate() {
                        if let Some(slot) = m.get_mut(i * 5) {
                            *slot = f64::from(*b as i8);
                        }
                    }
                } else {
                    let mut cursor = self.at(rep)?;
                    for v in &mut m {
                        *v = cursor.f64()?;
                    }
                }
                Some(Value::Matrix(Box::new(m)))
            }
            kind::QUATD | kind::QUATF | kind::QUATH => {
                let s = match ty {
                    kind::QUATD => Scalar::Double,
                    kind::QUATF => Scalar::Float,
                    _ => Scalar::Half,
                };
                // Kept as the imaginary part, then the real one.
                let mut cursor = self.at(rep)?;
                let mut q = [0.0; 4];
                for v in &mut q {
                    *v = scalar(&mut cursor, s)?;
                }
                Some(Value::Tuple(vec![q[3], q[0], q[1], q[2]]))
            }
            kind::TOKEN_VECTOR => {
                let mut cursor = self.at(rep)?;
                let n = cursor.count(4)?;
                let texts = (0..n)
                    .map(|_| self.token(u64::from(cursor.u32()?)).map(str::to_owned))
                    .collect::<Result<Vec<String>>>()?;
                Some(Value::Texts(texts))
            }
            kind::DOUBLE_VECTOR => {
                let mut cursor = self.at(rep)?;
                let n = cursor.count(8)?;
                let data = (0..n)
                    .map(|_| cursor.f64().map(|v| v as f32))
                    .collect::<Result<Vec<f32>>>()?;
                Some(Value::Floats { dim: 1, data })
            }
            _ => match vector_of(ty) {
                Some((dim, s)) => {
                    let mut v = Vec::with_capacity(dim);
                    if inline {
                        // Each number as a signed byte.
                        for b in p.to_le_bytes().iter().take(dim) {
                            v.push(f64::from(*b as i8));
                        }
                    } else {
                        let mut cursor = self.at(rep)?;
                        for _ in 0..dim {
                            v.push(scalar(&mut cursor, s)?);
                        }
                    }
                    Some(Value::Tuple(v))
                }
                None => None,
            },
        })
    }

    /// The first sample of the time samples of an attribute: the times
    /// and the values each sit after an offset to them, and the values
    /// are reps.
    fn first_sample(&self, rep: Rep) -> Result<Option<Value>> {
        let mut cursor = self.at(rep)?;
        // An offset, from where it is, to the rep of the times.
        let jump = |cursor: &mut Cursor<'_>| -> Result<()> {
            let start = cursor.at;
            let offset = cursor.u64()? as i64;
            let to = (start as i64)
                .checked_add(offset)
                .and_then(|t| usize::try_from(t).ok())
                .filter(|t| *t < self.bytes.len())
                .ok_or_else(|| malformed("time samples past the end"))?;
            cursor.at = to;
            Ok(())
        };
        jump(&mut cursor)?;
        // The rep of the times, then an offset to the values.
        cursor.u64()?;
        jump(&mut cursor)?;
        let n = cursor.count(8)?;
        if n == 0 {
            return Ok(None);
        }
        let first = Rep(cursor.u64()?);
        if first.ty() == kind::TIME_SAMPLES {
            return Ok(None);
        }
        self.value(first)
    }

    /// The fields of a set, by name, from its index.
    fn fieldset(&self, at: u32) -> Vec<(&str, Rep)> {
        let mut out = Vec::new();
        for index in self.fieldsets.iter().skip(at as usize) {
            if *index == u32::MAX {
                break;
            }
            let Some((token, rep)) = self.fields.get(*index as usize) else {
                break;
            };
            if let Ok(name) = self.token(u64::from(*token)) {
                out.push((name, Rep(*rep)));
            }
        }
        out
    }
}

/// A section of the table of contents, by name.
fn section<'a>(toc: &[(String, usize, usize)], bytes: &'a [u8], name: &str) -> Result<Cursor<'a>> {
    let (_, start, size) = toc
        .iter()
        .find(|(n, _, _)| n == name)
        .ok_or_else(|| malformed(format!("no {name} section")))?;
    let end = start
        .checked_add(*size)
        .filter(|e| *e <= bytes.len())
        .ok_or_else(|| malformed(format!("the {name} section past the end")))?;
    Ok(Cursor::new(bytes.get(..end).unwrap_or_default(), *start))
}

/// The paths of the file, from the tree of its compressed paths.
fn paths(cursor: &mut Cursor<'_>, tokens: &[String]) -> Result<Vec<String>> {
    let count = cursor.count(0)?;
    let n = cursor.count(0)?;
    if count > limits::PRIMS * 64 || n > count {
        return Err(Error::TooLarge);
    }
    let indexes = compressed_u32s(cursor, n)?;
    let elements: Vec<i64> = compressed_ints(cursor, n, false)?;
    let jumps: Vec<i64> = compressed_ints(cursor, n, false)?;
    let mut paths = vec![String::new(); count];
    // Siblings left for later, with the path of their parent.
    let mut pending: Vec<(usize, Option<String>)> = vec![(0, None)];
    let mut visited = 0usize;
    while let Some((mut i, mut parent)) = pending.pop() {
        loop {
            visited += 1;
            if visited > n {
                return Err(malformed("the tree of paths loops"));
            }
            let this = i;
            i += 1;
            let path = match &parent {
                None => "/".to_owned(),
                Some(p) => {
                    let element = *elements
                        .get(this)
                        .ok_or_else(|| malformed("a path element past the end"))?;
                    let name = tokens
                        .get(element.unsigned_abs() as usize)
                        .ok_or_else(|| malformed("a path element past the tokens"))?;
                    if element < 0 {
                        format!("{p}.{name}")
                    } else if p == "/" {
                        format!("/{name}")
                    } else {
                        format!("{p}/{name}")
                    }
                }
            };
            let slot = indexes
                .get(this)
                .and_then(|at| paths.get_mut(*at as usize))
                .ok_or_else(|| malformed("a path index past the end"))?;
            slot.clone_from(&path);
            let jump = *jumps
                .get(this)
                .ok_or_else(|| malformed("a path jump past the end"))?;
            let has_child = jump > 0 || jump == -1;
            let has_sibling = jump >= 0;
            if has_child {
                if has_sibling {
                    let sibling = usize::try_from(jump)
                        .ok()
                        .and_then(|j| this.checked_add(j))
                        .ok_or_else(|| malformed("a path jump out of range"))?;
                    pending.push((sibling, parent.clone()));
                }
                parent = Some(path);
            } else if !has_sibling {
                break;
            }
            if i >= n {
                break;
            }
        }
    }
    Ok(paths)
}

/// Reads a binary layer.
pub(crate) fn read(bytes: &[u8]) -> Result<Layer> {
    if bytes.get(..8) != Some(b"PXR-USDC") {
        return Err(Error::Unknown);
    }
    let mut head = Cursor::new(bytes, 8);
    let version = head.array::<8>()?;
    let version = (version[0], version[1]);
    if version < (0, 4) {
        return Err(malformed("a crate file older than 0.4"));
    }
    let toc_at = usize::try_from(head.u64()?).map_err(|_| Error::TooLarge)?;
    let mut toc_cursor = Cursor::new(bytes, toc_at);
    let sections = toc_cursor.count(32)?;
    let mut toc = Vec::with_capacity(sections);
    for _ in 0..sections {
        let name = toc_cursor.take(16)?;
        let name = name.split(|b| *b == 0).next().unwrap_or_default();
        let start = usize::try_from(toc_cursor.u64()?).map_err(|_| Error::TooLarge)?;
        let size = usize::try_from(toc_cursor.u64()?).map_err(|_| Error::TooLarge)?;
        toc.push((String::from_utf8_lossy(name).into_owned(), start, size));
    }

    // The tokens, as strings ended by zeros.
    let mut c = section(&toc, bytes, "TOKENS")?;
    let count = c.count(1)?;
    let size = usize::try_from(c.u64()?).map_err(|_| Error::TooLarge)?;
    let compressed = usize::try_from(c.u64()?).map_err(|_| Error::TooLarge)?;
    if size > limits::FILE_BYTES as usize {
        return Err(Error::TooLarge);
    }
    let text = lz4::decompress(c.take(compressed)?, size)?;
    let mut tokens: Vec<String> = text
        .split(|b| *b == 0)
        .take(count)
        .map(|t| String::from_utf8_lossy(t).into_owned())
        .collect();
    tokens.resize(count, String::new());

    let mut c = section(&toc, bytes, "STRINGS")?;
    let n = c.count(4)?;
    let strings = (0..n).map(|_| c.u32()).collect::<Result<Vec<u32>>>()?;

    let mut c = section(&toc, bytes, "FIELDS")?;
    let n = c.count(0)?;
    let names = compressed_u32s(&mut c, n)?;
    let size = usize::try_from(c.u64()?).map_err(|_| Error::TooLarge)?;
    let reps = lz4::decompress(c.take(size)?, n * 8)?;
    let fields: Vec<(u32, u64)> = names
        .into_iter()
        .zip(reps.as_chunks::<8>().0)
        .map(|(name, rep)| (name, u64::from_le_bytes(*rep)))
        .collect();

    let mut c = section(&toc, bytes, "FIELDSETS")?;
    let n = c.count(0)?;
    let fieldsets = compressed_u32s(&mut c, n)?;

    let mut c = section(&toc, bytes, "PATHS")?;
    let paths = paths(&mut c, &tokens)?;

    let file = Crate {
        bytes,
        version,
        tokens,
        strings,
        fields,
        fieldsets,
        paths,
    };

    let mut c = section(&toc, bytes, "SPECS")?;
    let n = c.count(0)?;
    if n > limits::PRIMS * 64 {
        return Err(Error::TooLarge);
    }
    let spec_paths = compressed_u32s(&mut c, n)?;
    let spec_sets = compressed_u32s(&mut c, n)?;
    let spec_types = compressed_u32s(&mut c, n)?;

    let mut layer = Layer::new();
    let mut classes = Vec::new();
    let mut prims = 0usize;
    for ((path, set), ty) in spec_paths.iter().zip(&spec_sets).zip(&spec_types) {
        let path = file.path(*path)?;
        // What is inside variants is left out.
        if path.contains('{') {
            continue;
        }
        let fields = file.fieldset(*set);
        match ty {
            // The pseudo-root: the metadata of the layer.
            7 => {
                for (name, rep) in fields {
                    match (name, file.value(rep)?) {
                        ("defaultPrim", Some(Value::Text(t))) => layer.default_prim = Some(t),
                        ("upAxis", Some(Value::Text(t))) => layer.z_up = t == "Z",
                        _ => {}
                    }
                }
            }
            // A prim.
            6 => {
                prims += 1;
                if prims > limits::PRIMS || path.split('/').count() > limits::DEPTH {
                    return Err(Error::TooLarge);
                }
                let prim = layer.root.find_mut(path);
                for (name, rep) in fields {
                    match name {
                        "typeName" => {
                            if let Some(Value::Text(t)) = file.value(rep)? {
                                prim.type_name = t;
                            }
                        }
                        "active" => {
                            prim.active = !matches!(file.value(rep)?, Some(Value::Bool(false)));
                        }
                        "specifier" => {
                            if matches!(file.value(rep)?, Some(Value::Int(2))) {
                                classes.push(path.to_owned());
                            }
                        }
                        "references" if rep.ty() == kind::REFERENCE_LIST_OP => {
                            let refs = file.list_op(rep, |f, c| f.reference(c, true))?;
                            prim.references.extend(refs);
                        }
                        "payload" if rep.ty() == kind::PAYLOAD_LIST_OP => {
                            let refs = file.list_op(rep, |f, c| f.reference(c, false))?;
                            prim.references.extend(refs);
                        }
                        "payload" if rep.ty() == kind::PAYLOAD => {
                            let mut cursor = file.at(rep)?;
                            prim.references.push(file.reference(&mut cursor, false)?);
                        }
                        _ => {}
                    }
                }
            }
            // An attribute or a relationship.
            1 | 8 => {
                let (prim_path, property) = split_property(path);
                if property.is_empty() {
                    continue;
                }
                let mut attribute = Attribute::default();
                let mut targets = Vec::new();
                for (name, rep) in fields {
                    match name {
                        "default" => attribute.value = file.value(rep)?,
                        "timeSamples" if rep.ty() == kind::TIME_SAMPLES => {
                            if attribute.value.is_none() {
                                attribute.value = file.first_sample(rep).unwrap_or(None);
                            }
                        }
                        "interpolation" => {
                            if let Some(Value::Text(t)) = file.value(rep)? {
                                attribute.interpolation = Some(t);
                            }
                        }
                        "connectionPaths" if rep.ty() == kind::PATH_LIST_OP => {
                            attribute.connection = file
                                .list_op(rep, |f, c| f.path(c.u32()?).map(str::to_owned))?
                                .into_iter()
                                .next();
                        }
                        "targetPaths" if rep.ty() == kind::PATH_LIST_OP => {
                            targets =
                                file.list_op(rep, |f, c| f.path(c.u32()?).map(str::to_owned))?;
                        }
                        _ => {}
                    }
                }
                let prim = layer.root.find_mut(prim_path);
                if *ty == 8 {
                    prim.relationships.insert(property.to_owned(), targets);
                } else {
                    prim.attributes.insert(property.to_owned(), attribute);
                }
            }
            _ => {}
        }
    }
    // Classes are left out.
    for path in classes {
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", &path));
        let parent = layer.root.find_mut(parent);
        parent.children.retain(|c| c.name != name);
    }
    Ok(layer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes integers as `Usd_IntegerCompression` does, before LZ4.
    fn encode(values: &[i32]) -> Vec<u8> {
        let mut deltas = Vec::new();
        let mut previous = 0i32;
        for v in values {
            deltas.push(v.wrapping_sub(previous));
            previous = *v;
        }
        let common: i32 = 7;
        let mut out = common.to_le_bytes().to_vec();
        let mut codes = vec![0u8; values.len().div_ceil(4)];
        let mut rest = Vec::new();
        for (i, d) in deltas.iter().enumerate() {
            let code = if *d == common {
                0
            } else if i8::try_from(*d).is_ok() {
                rest.push(*d as i8 as u8);
                1
            } else if i16::try_from(*d).is_ok() {
                rest.extend((*d as i16).to_le_bytes());
                2
            } else {
                rest.extend(d.to_le_bytes());
                3
            };
            codes[i / 4] |= code << ((i % 4) * 2);
        }
        out.extend(codes);
        out.extend(rest);
        out
    }

    #[test]
    fn w116_integers_decode_from_their_differences() {
        let values = [0, 7, 14, 10, 300, -70_000, -70_000, 5];
        let decoded = decode_ints(&encode(&values), values.len(), false).unwrap();
        assert_eq!(decoded, values.map(i64::from));
        assert!(decode_ints(&encode(&values)[..6], values.len(), false).is_err());
    }

    #[test]
    fn w116_half_numbers() {
        assert_eq!(half(0x3C00), 1.0);
        assert_eq!(half(0xC000), -2.0);
        assert_eq!(half(0x3800), 0.5);
        assert_eq!(half(0x7C00), f32::INFINITY);
    }
}
