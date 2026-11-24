// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The outline of the opaque pixels of a picture, as Pages traces it for
//! text to wrap tightly around the picture (OBJ-17).

/// How far, in pixels of the picture, a corner of the outline may be
/// from the straight edge that leaves it out, as in Pages 15.4.
const STRAIGHTENING: f64 = 2.0;

/// The outline that text wraps tightly around for a picture `width` by
/// `height` pixels whose pixels are opaque where `opaque` says so, as
/// Pages 15.4 traces it: the edges of the pixels around the opaque part,
/// from the top left corner of the first opaque pixel of the top row
/// with opaque pixels and clockwise, made straight: from each corner kept,
/// an edge goes to the farthest corner that leaves out none more than two
/// pixels away from it. The corners are in pixels, from the top left
/// corner of the picture.
///
/// None when no pixel is opaque, or when the opaque pixels are in parts
/// apart, which [`pixel_outlines`] traces one by one.
pub fn pixel_outline(
    width: usize,
    height: usize,
    opaque: impl Fn(usize, usize) -> bool,
) -> Option<Vec<(u32, u32)>> {
    let mut parts = pixel_outlines(width, height, opaque);
    (parts.len() == 1).then(|| parts.remove(0))
}

/// The outlines of the parts apart of the opaque pixels of a picture, each
/// traced as [`pixel_outline`] traces one, in the order of their first
/// pixel, row by row: Pages 15.4 runs text between them (OBJ-17, B-047).
/// Pixels that touch by a corner are one part, and a part inside a hole of
/// another is in it.
pub fn pixel_outlines(
    width: usize,
    height: usize,
    opaque: impl Fn(usize, usize) -> bool,
) -> Vec<Vec<(u32, u32)>> {
    let mut covered = vec![false; width * height];
    let mut parts = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if covered[y * width + x] || !opaque(x, y) {
                continue;
            }
            covered[y * width + x] = true;
            let Some((corners, crossings)) = trace(width, height, &opaque, (x, y)) else {
                continue;
            };
            // What the outline holds, by the even-odd rule along each row:
            // the crossings, by row then place, in pairs.
            let mut crossings = crossings;
            crossings.sort_unstable();
            for row in crossings.chunk_by(|a, b| a.0 == b.0) {
                let y = row.first().map_or(0, |c| c.0) as usize;
                for [(_, a), (_, b)] in row.as_chunks::<2>().0 {
                    let (a, b) = ((*a).max(0) as usize, ((*b).max(0) as usize).min(width));
                    covered[y * width + a..y * width + b.max(a)].fill(true);
                }
            }
            // A part of a few pixels could be straightened to a line: its
            // corners are kept as they are.
            let mut kept = straightened(&corners);
            if kept.len() < 3 {
                kept = corners;
            }
            parts.push(
                kept.into_iter()
                    .map(|(x, y)| (x as u32, y as u32))
                    .collect(),
            );
        }
    }
    parts
}

/// The corners of the edges of the pixels around the part of the opaque
/// pixels whose first pixel, row by row, is `first`, clockwise from its
/// top left corner, and where the outline crosses the rows: the row and
/// the place of each crossing, so that a small part costs what it covers,
/// not the height of the picture.
#[allow(clippy::type_complexity)]
fn trace(
    width: usize,
    height: usize,
    opaque: &impl Fn(usize, usize) -> bool,
    first: (usize, usize),
) -> Option<(Vec<(i64, i64)>, Vec<(i64, i64)>)> {
    let pixel = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && (x as usize) < width
            && (y as usize) < height
            && opaque(x as usize, y as usize)
    };
    // Along the edges of the pixels with the opaque ones on the right:
    // turning left when the pixel ahead on the left is opaque, so that
    // pixels that touch by a corner are one part, going on when only the
    // pixel ahead on the right is, and turning right otherwise.
    let start = (first.0 as i64, first.1 as i64);
    let (mut at, mut heading) = (start, (1i64, 0i64));
    let mut corners: Vec<(i64, i64)> = vec![start];
    let mut crossings: Vec<(i64, i64)> = Vec::new();
    let limit = 4 * (width + 1) * (height + 1);
    for _ in 0..limit {
        if heading.1 != 0 {
            let row = at.1.min(at.1 + heading.1);
            crossings.push((row, at.0));
        }
        at = (at.0 + heading.0, at.1 + heading.1);
        let right = (-heading.1, heading.0);
        // The pixel whose square is between two sides from a corner.
        let square = |a: (i64, i64), b: (i64, i64)| {
            (
                at.0.min(at.0 + a.0).min(at.0 + b.0).min(at.0 + a.0 + b.0),
                at.1.min(at.1 + a.1).min(at.1 + b.1).min(at.1 + a.1 + b.1),
            )
        };
        let ahead_left = square(heading, (heading.1, -heading.0));
        let ahead_right = square(heading, right);
        let turned = if pixel(ahead_left.0, ahead_left.1) {
            (heading.1, -heading.0)
        } else if pixel(ahead_right.0, ahead_right.1) {
            heading
        } else {
            right
        };
        if at == start && turned == (1, 0) {
            break;
        }
        if turned != heading {
            corners.push(at);
            heading = turned;
        }
    }
    (at == start).then_some((corners, crossings))
}

/// The corners of a closed outline kept when each edge goes from a corner
/// kept to the farthest corner that leaves out none more than
/// [`STRAIGHTENING`] away from it.
fn straightened(corners: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let n = corners.len();
    let at = |i: usize| corners[i % n];
    let mut kept = vec![corners[0]];
    let mut i = 0;
    while i < n {
        let mut k = i + 1;
        while k < n && (i + 1..=k).all(|m| distance(at(m), at(i), at(k + 1)) <= STRAIGHTENING) {
            k += 1;
        }
        if k < n {
            kept.push(at(k));
        }
        i = k;
    }
    kept
}

/// How far a point is from the segment from `a` to `b`.
fn distance(p: (i64, i64), a: (i64, i64), b: (i64, i64)) -> f64 {
    let (dx, dy) = ((b.0 - a.0) as f64, (b.1 - a.1) as f64);
    let (px, py) = ((p.0 - a.0) as f64, (p.1 - a.1) as f64);
    let length = dx * dx + dy * dy;
    if length == 0.0 {
        return px.hypot(py);
    }
    let t = (px * dx + py * dy) / length;
    if t <= 0.0 {
        px.hypot(py)
    } else if t >= 1.0 {
        (px - dx).hypot(py - dy)
    } else {
        (dx * py - dy * px).abs() / length.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disc of a radius of 0.475 of the side of a square picture.
    fn disc(side: usize) -> impl Fn(usize, usize) -> bool {
        let (c, r) = (side as f64 / 2.0, 0.475 * side as f64);
        move |x, y| (x as f64 + 0.5 - c).hypot(y as f64 + 0.5 - c) <= r
    }

    #[test]
    fn obj_17_the_outline_of_a_square_is_its_corners() {
        let outline = pixel_outline(10, 10, |x, y| (2..8).contains(&x) && (3..7).contains(&y));
        assert_eq!(outline, Some(vec![(2, 3), (8, 3), (8, 7), (2, 7)]));
    }

    #[test]
    fn obj_17_the_outline_of_a_small_disc_is_straightened_as_in_pages() {
        // The corners that Pages 15.4 wraps the text of
        // wrap-contour-res around for a disc of 40 and of 50 pixels.
        assert_eq!(
            pixel_outline(40, 40, disc(40)).unwrap(),
            [
                (16, 1),
                (31, 4),
                (39, 16),
                (36, 31),
                (24, 39),
                (9, 36),
                (1, 24),
                (4, 9)
            ]
        );
        assert_eq!(
            pixel_outline(50, 50, disc(50)).unwrap(),
            [
                (22, 1),
                (39, 5),
                (48, 17),
                (47, 35),
                (35, 47),
                (15, 47),
                (3, 35),
                (3, 15),
                (15, 3)
            ]
        );
    }

    #[test]
    fn obj_17_no_outline_without_opaque_pixels_or_with_parts_apart() {
        assert_eq!(pixel_outline(4, 4, |_, _| false), None);
        assert_eq!(pixel_outline(9, 3, |x, _| x == 1 || x == 7), None);
        // Pixels that touch by a corner are one part.
        assert!(pixel_outline(2, 2, |x, y| x == y).is_some());
    }

    #[test]
    fn obj_17_parts_apart_are_traced_one_by_one() {
        // Two columns, and a square lower down, in the order of their
        // first pixels.
        let parts = pixel_outlines(9, 6, |x, y| {
            ((x == 1 || x == 7) && y < 3) || ((3..6).contains(&x) && y >= 4)
        });
        assert_eq!(
            parts,
            [
                vec![(1, 0), (2, 0), (2, 3), (1, 3)],
                vec![(7, 0), (8, 0), (8, 3), (7, 3)],
                vec![(3, 4), (6, 4), (6, 6), (3, 6)],
            ]
        );
        // A part in the hole of a ring is in the ring.
        let ring = |x: usize, y: usize| x == 0 || y == 0 || x == 6 || y == 6 || (x == 3 && y == 3);
        assert_eq!(pixel_outlines(7, 7, ring).len(), 1);
        assert!(pixel_outlines(3, 3, |_, _| false).is_empty());
        // Specks all over a picture: each costs the rows it covers, not
        // the height of the picture (review RV, batch 3).
        let specks = pixel_outlines(600, 600, |x, y| x % 2 == 0 && y % 2 == 0);
        assert_eq!(specks.len(), 300 * 300);
    }

    #[test]
    fn obj_17_a_picture_of_a_few_pixels_keeps_its_corners() {
        assert_eq!(
            pixel_outline(4, 4, |x, _| x >= 2),
            Some(vec![(2, 0), (4, 0), (4, 4), (2, 4)])
        );
    }
}
