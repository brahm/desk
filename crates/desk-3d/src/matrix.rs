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

/// A matrix as a translation, a rotation (as a quaternion of glTF: x, y,
/// z, then w) and a scale, which make it again as glTF makes a node's;
/// a shear is lost. None when it has a scale of 0.
pub(crate) fn decompose(m: &Matrix) -> Option<([f64; 3], [f64; 4], [f64; 3])> {
    let t = [at(m, 3, 0), at(m, 3, 1), at(m, 3, 2)];
    let row = |r: usize| [at(m, r, 0), at(m, r, 1), at(m, r, 2)];
    let length = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let mut s = [length(row(0)), length(row(1)), length(row(2))];
    if s.iter().any(|v| !(*v > 1e-12 && v.is_finite())) {
        return None;
    }
    let [a, b, c] = [row(0), row(1), row(2)];
    let det = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0]);
    if det < 0.0 {
        s[0] = -s[0];
    }
    // The rows of the turn alone; as columns, the matrix of the turn.
    let mut r = IDENTITY;
    for (i, scale) in s.iter().enumerate() {
        for (j, v) in row(i).iter().enumerate() {
            put(&mut r, i, j, v / scale);
        }
    }
    let col = |i: usize, j: usize| at(&r, j, i);
    let trace = col(0, 0) + col(1, 1) + col(2, 2);
    let q = if trace > 0.0 {
        let k = 0.5 / (trace + 1.0).sqrt();
        [
            (col(2, 1) - col(1, 2)) * k,
            (col(0, 2) - col(2, 0)) * k,
            (col(1, 0) - col(0, 1)) * k,
            0.25 / k,
        ]
    } else if col(0, 0) > col(1, 1) && col(0, 0) > col(2, 2) {
        let k = 2.0 * (1.0 + col(0, 0) - col(1, 1) - col(2, 2)).sqrt();
        [
            0.25 * k,
            (col(0, 1) + col(1, 0)) / k,
            (col(0, 2) + col(2, 0)) / k,
            (col(2, 1) - col(1, 2)) / k,
        ]
    } else if col(1, 1) > col(2, 2) {
        let k = 2.0 * (1.0 + col(1, 1) - col(0, 0) - col(2, 2)).sqrt();
        [
            (col(0, 1) + col(1, 0)) / k,
            0.25 * k,
            (col(1, 2) + col(2, 1)) / k,
            (col(0, 2) - col(2, 0)) / k,
        ]
    } else {
        let k = 2.0 * (1.0 + col(2, 2) - col(0, 0) - col(1, 1)).sqrt();
        [
            (col(0, 2) + col(2, 0)) / k,
            (col(1, 2) + col(2, 1)) / k,
            0.25 * k,
            (col(1, 0) - col(0, 1)) / k,
        ]
    };
    q.iter().all(|v| v.is_finite()).then_some((t, q, s))
}

/// The turn of a quaternion of glTF: its imaginary parts, then its real
/// one.
pub(crate) fn quaternion([x, y, z, w]: [f64; 4]) -> Matrix {
    orient(&[w, x, y, z])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_a_matrix_comes_apart_and_back_together() {
        let m = mul(
            &mul(&scale(&[2.0, 3.0, 0.5]), &rotate(1, 30.0)),
            &mul(&rotate(0, -70.0), &translate(&[1.0, -2.0, 4.0])),
        );
        let (t, r, s) = decompose(&m).unwrap();
        let again = mul(&mul(&scale(&s), &quaternion(r)), &translate(&t));
        assert!(
            m.iter().zip(&again).all(|(a, b)| (a - b).abs() < 1e-9),
            "{m:?} {again:?}"
        );
        assert!(decompose(&scale(&[0.0, 1.0, 1.0])).is_none());
    }
}
