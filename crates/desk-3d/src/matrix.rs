// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Matrices of 4×4 numbers that place the parts of a scene, by rows, with
//! points as rows: a point goes through `a` then `b` as `p·a·b`.

/// A 4×4 matrix by rows, with vectors as rows.
pub(crate) type Matrix = [f64; 16];

pub(crate) const IDENTITY: Matrix = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// The number of a matrix at a row and a column.
pub(crate) fn at(m: &Matrix, r: usize, c: usize) -> f64 {
    m.get(r * 4 + c).copied().unwrap_or(0.0)
}

/// Sets the number of a matrix at a row and a column.
pub(crate) fn put(m: &mut Matrix, r: usize, c: usize, v: f64) {
    if let Some(slot) = m.get_mut(r * 4 + c) {
        *slot = v;
    }
}

pub(crate) fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [0.0; 16];
    for r in 0..4 {
        for c in 0..4 {
            put(
                &mut out,
                r,
                c,
                (0..4).map(|k| at(a, r, k) * at(b, k, c)).sum(),
            );
        }
    }
    out
}

pub(crate) fn translate(t: &[f64]) -> Matrix {
    let mut m = IDENTITY;
    m[12] = t.first().copied().unwrap_or(0.0);
    m[13] = t.get(1).copied().unwrap_or(0.0);
    m[14] = t.get(2).copied().unwrap_or(0.0);
    m
}

pub(crate) fn scale(s: &[f64]) -> Matrix {
    let mut m = IDENTITY;
    let all = s.len() == 1;
    for k in 0..3 {
        let v = if all { s.first() } else { s.get(k) };
        put(&mut m, k, k, v.copied().unwrap_or(1.0));
    }
    m
}

/// A turn about the X, Y or Z axis (0, 1 or 2) by degrees.
pub(crate) fn rotate(axis: usize, degrees: f64) -> Matrix {
    let (s, c) = degrees.to_radians().sin_cos();
    let mut m = IDENTITY;
    // The two axes the turn moves, in the order that makes it turn
    // counterclockwise as seen from the third.
    let (a, b) = match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    put(&mut m, a, a, c);
    put(&mut m, a, b, s);
    put(&mut m, b, a, -s);
    put(&mut m, b, b, c);
    m
}

/// The turn of a quaternion: its real part, then its imaginary ones.
pub(crate) fn orient(q: &[f64]) -> Matrix {
    let [w, x, y, z] = [0, 1, 2, 3].map(|i| q.get(i).copied().unwrap_or(0.0));
    let n = (w * w + x * x + y * y + z * z).sqrt();
    if n == 0.0 || !n.is_finite() {
        return IDENTITY;
    }
    let (w, x, y, z) = (w / n, x / n, y / n, z / n);
    // The matrix of columns, transposed for rows.
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        0.0,
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        0.0,
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

/// The inverse of a matrix; none when it has none.
pub(crate) fn invert(m: &Matrix) -> Option<Matrix> {
    // Gauss-Jordan on the matrix beside the identity.
    let mut a = *m;
    let mut out = IDENTITY;
    for col in 0..4 {
        let pivot =
            (col..4).max_by(|i, j| at(&a, *i, col).abs().total_cmp(&at(&a, *j, col).abs()))?;
        if at(&a, pivot, col).abs() < 1e-12 {
            return None;
        }
        for k in 0..4 {
            let (x, y) = (at(&a, col, k), at(&a, pivot, k));
            put(&mut a, col, k, y);
            put(&mut a, pivot, k, x);
            let (x, y) = (at(&out, col, k), at(&out, pivot, k));
            put(&mut out, col, k, y);
            put(&mut out, pivot, k, x);
        }
        let p = at(&a, col, col);
        for k in 0..4 {
            let (x, y) = (at(&a, col, k) / p, at(&out, col, k) / p);
            put(&mut a, col, k, x);
            put(&mut out, col, k, y);
        }
        for r in 0..4 {
            if r == col {
                continue;
            }
            let f = at(&a, r, col);
            for k in 0..4 {
                let x = at(&a, r, k) - f * at(&a, col, k);
                let y = at(&out, r, k) - f * at(&out, col, k);
                put(&mut a, r, k, x);
                put(&mut out, r, k, y);
            }
        }
    }
    Some(out)
}

/// A point through a matrix.
pub(crate) fn point(m: &Matrix, p: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = p.map(f64::from);
    [0, 1, 2].map(|c| (x * at(m, 0, c) + y * at(m, 1, c) + z * at(m, 2, c) + at(m, 3, c)) as f32)
}

/// A normal through a matrix, given the inverse of the matrix: the
/// inverse transposed keeps it square to its surface. Not made of length
/// one.
pub(crate) fn normal(inverse: &Matrix, n: [f32; 3]) -> [f32; 3] {
    let n = n.map(f64::from);
    [0, 1, 2].map(|j| {
        (n[0] * at(inverse, j, 0) + n[1] * at(inverse, j, 1) + n[2] * at(inverse, j, 2)) as f32
    })
}

/// The turn of a quaternion of glTF: its imaginary parts, then its real
/// one.
pub(crate) fn quaternion([x, y, z, w]: [f64; 4]) -> Matrix {
    orient(&[w, x, y, z])
}
