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
/// None when no pixel is opaque, or when some opaque pixels are outside
/// the outline, apart from the others: the traces of the pictures that
/// Pages was compared on are of a single part.
pub fn pixel_outline(
    width: usize,
    height: usize,
    opaque: impl Fn(usize, usize) -> bool,
) -> Option<Vec<(u32, u32)>> {
    let first = (0..height).find_map(|y| (0..width).find(|&x| opaque(x, y)).map(|x| (x, y)))?;
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
    // Where the outline crosses each row, to see that it holds every
    // opaque pixel.
    let mut crossings: Vec<Vec<i64>> = vec![Vec::new(); height];
    let limit = 4 * (width + 1) * (height + 1);
    for _ in 0..limit {
        if heading.1 != 0 {
            let row = at.1.min(at.1 + heading.1);
            crossings[row as usize].push(at.0);
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
    if at != start {
        return None;
    }
    // Every opaque pixel is inside the outline, by the even-odd rule
    // along its row.
    for (y, xs) in crossings.iter_mut().enumerate() {
        xs.sort_unstable();
        let mut inside = xs.as_chunks::<2>().0.iter().map(|&[a, b]| (a, b));
        let mut span = inside.next();
        for x in 0..width as i64 {
            while span.is_some_and(|s| x >= s.1) {
                span = inside.next();
            }
            if opaque(x as usize, y) && !span.is_some_and(|s| x >= s.0) {
                return None;
            }
        }
    }
    // A picture of a few pixels could be straightened to a line: its
    // corners are kept as they are.
    let mut kept = straightened(&corners);
    if kept.len() < 3 {
        kept = corners;
    }
    Some(
        kept.into_iter()
            .map(|(x, y)| (x as u32, y as u32))
            .collect(),
    )
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
    fn obj_17_a_picture_of_a_few_pixels_keeps_its_corners() {
        assert_eq!(
            pixel_outline(4, 4, |x, _| x >= 2),
            Some(vec![(2, 0), (4, 0), (4, 4), (2, 4)])
        );
    }
}
