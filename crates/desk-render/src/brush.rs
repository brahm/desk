// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The artistic lines (B-044): a path drawn as a pen or a brush draws it.
//!
//! Pages 15.4 has lines of chalk, a thick brush, a chisel pen, a dry
//! brush and uneven ink. Desk draws its own: the path is walked along its
//! length, and each brush gives how wide the line is at each point, how
//! far each edge wanders and where the ink leaves gaps. What comes out is
//! a shape to fill, which every back end draws as it draws any other.
//!
//! The irregularity is noise of the position along the path, never
//! random: a line looks the same every time it is drawn, on screen, in
//! print and in a PDF.

use desk_model::Brush;

use crate::display::PathOp;

/// Points along a part of a path.
struct Polyline {
    points: Vec<(f32, f32)>,
    closed: bool,
}

/// How many straight pieces a curve becomes, at most.
const CURVE_PIECES: usize = 32;
/// How many points a line takes, at most, along every part of its path.
const MAX_SAMPLES: f32 = 20_000.0;

/// The outline of the line of a brush `width` wide along a path, a shape
/// to fill with the nonzero rule. Empty when there is nothing to draw.
pub fn outline(path: &[PathOp], width: f32, brush: Brush) -> Vec<PathOp> {
    let mut out = Vec::new();
    if !(width.is_finite() && width > 0.0) {
        return out;
    }
    for (part, line) in polylines(path).iter().enumerate() {
        let samples = sample(line, width);
        if samples.len() < 2 {
            continue;
        }
        let seed = brush_seed(brush) ^ (part as u32).wrapping_mul(0x9e37_79b9);
        match brush {
            Brush::DryBrush => dry_brush(&samples, width, seed, &mut out),
            Brush::Chalk => chalk(&samples, width, seed, &mut out),
            _ => {
                // A closed line has no ends to taper.
                let length = |total: f32| if line.closed { f32::INFINITY } else { total };
                let profile =
                    |s: &Sample, total: f32| profile(brush, s, length(total), width, seed);
                let total = samples.last().map_or(0.0, |s| s.at);
                band(&samples, total, line.closed, &profile, &mut out);
            }
        }
    }
    out
}

fn brush_seed(brush: Brush) -> u32 {
    match brush {
        Brush::Chalk => 0x0c4a_1c01,
        Brush::ThickBrush => 0x7b2c_5e02,
        Brush::ChiselPen => 0x3f81_d903,
        Brush::DryBrush => 0x5a6e_0b04,
        Brush::UnevenInk => 0x1d93_a705,
    }
}

/// The parts of a path as points, with its curves in straight pieces.
fn polylines(path: &[PathOp]) -> Vec<Polyline> {
    let mut lines = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    let finite = |p: (f32, f32)| p.0.is_finite() && p.1.is_finite();
    let mut flush = |current: &mut Vec<(f32, f32)>, closed: bool| {
        if current.len() >= 2 {
            lines.push(Polyline {
                points: std::mem::take(current),
                closed,
            });
        } else {
            current.clear();
        }
    };
    for op in path {
        match *op {
            PathOp::Move(x, y) => {
                flush(&mut current, false);
                if finite((x, y)) {
                    current.push((x, y));
                }
            }
            PathOp::Line(x, y) => {
                if finite((x, y)) {
                    current.push((x, y));
                }
            }
            PathOp::Curve(x1, y1, x2, y2, x, y) => {
                let Some(&start) = current.last() else {
                    continue;
                };
                if ![(x1, y1), (x2, y2), (x, y)].into_iter().all(finite) {
                    continue;
                }
                // As many pieces as the curve is long, roughly.
                let reach = distance(start, (x1, y1))
                    + distance((x1, y1), (x2, y2))
                    + distance((x2, y2), (x, y));
                let pieces = ((reach / 2.0).ceil() as usize).clamp(4, CURVE_PIECES);
                for i in 1..=pieces {
                    let t = i as f32 / pieces as f32;
                    let u = 1.0 - t;
                    let a = u * u * u;
                    let b = 3.0 * u * u * t;
                    let c = 3.0 * u * t * t;
                    let d = t * t * t;
                    current.push((
                        a * start.0 + b * x1 + c * x2 + d * x,
                        a * start.1 + b * y1 + c * y2 + d * y,
                    ));
                }
            }
            PathOp::Close => {
                if let (Some(&first), Some(&last)) = (current.first(), current.last())
                    && distance(first, last) > 1e-3
                {
                    current.push(first);
                }
                flush(&mut current, true);
            }
        }
    }
    flush(&mut current, false);
    lines
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// A point along a path, how far along it is and which way the path goes.
#[derive(Clone, Copy, Debug)]
struct Sample {
    x: f32,
    y: f32,
    /// The length of the path up to the point.
    at: f32,
    /// The direction of the path, a unit vector.
    dx: f32,
    dy: f32,
}

/// Points evenly along a part of a path, a quarter of the width apart,
/// with the corners of the path among them.
fn sample(line: &Polyline, width: f32) -> Vec<Sample> {
    let points = &line.points;
    let total: f32 = points.windows(2).map(|w| distance(w[0], w[1])).sum();
    if !(total.is_finite() && total > 1e-3) {
        return Vec::new();
    }
    let step = (width * 0.25).max(0.25).max(total / MAX_SAMPLES);
    let mut out: Vec<(f32, f32, f32)> = Vec::new();
    let mut at = 0.0;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let length = distance(a, b);
        if length <= 1e-4 {
            continue;
        }
        let pieces = (length / step).ceil().max(1.0) as usize;
        if out.is_empty() {
            out.push((a.0, a.1, at));
        }
        for i in 1..=pieces {
            let t = i as f32 / pieces as f32;
            out.push((
                a.0 + (b.0 - a.0) * t,
                a.1 + (b.1 - a.1) * t,
                at + length * t,
            ));
        }
        at += length;
    }
    let n = out.len();
    (0..n)
        .map(|i| {
            // The direction from the point before to the point after: at a
            // corner, between the two sides.
            let (before, after) = if line.closed {
                // The last point is the first one again.
                let before = if i == 0 { n.saturating_sub(2) } else { i - 1 };
                let after = if i + 1 >= n { 1.min(n - 1) } else { i + 1 };
                (before, after)
            } else {
                (i.saturating_sub(1), (i + 1).min(n - 1))
            };
            let (x0, y0, _) = out[before];
            let (x1, y1, _) = out[after];
            let (mut dx, mut dy) = (x1 - x0, y1 - y0);
            let norm = dx.hypot(dy);
            if norm > 1e-6 {
                dx /= norm;
                dy /= norm;
            } else {
                (dx, dy) = (1.0, 0.0);
            }
            let (x, y, at) = out[i];
            Sample { x, y, at, dx, dy }
        })
        .collect()
}

/// A number from 0 to 1 for an integer and a seed.
fn hash(i: i64, seed: u32) -> f32 {
    let mut h = (i as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(u64::from(seed).wrapping_mul(0xbf58_476d_1ce4_e5b9));
    h ^= h >> 31;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Smooth noise from 0 to 1 along a line, changing about once a `scale`.
fn noise(at: f32, scale: f32, seed: u32) -> f32 {
    let t = at / scale;
    let i = t.floor();
    let f = t - i;
    let f = f * f * (3.0 - 2.0 * f);
    let i = i as i64;
    hash(i, seed) * (1.0 - f) + hash(i + 1, seed) * f
}

/// How far each edge of the line is from the path at a point, to its
/// left and to its right, 0 for none where the ink leaves a gap. The
/// length is infinite for a closed line.
fn profile(brush: Brush, s: &Sample, length: f32, width: f32, seed: u32) -> (f32, f32) {
    let half = width / 2.0;
    let at = s.at;
    match brush {
        Brush::ThickBrush => {
            let swell = 0.7 + 0.6 * noise(at, (width * 6.0).max(6.0), seed);
            let taper = taper(at, length, width * 4.0);
            let w = half * swell * taper;
            (w, w)
        }
        Brush::ChiselPen => {
            // A nib at 45 degrees, from the bottom left to the top right:
            // thin along it, thick across it.
            let across = (s.dx + s.dy).abs() * std::f32::consts::FRAC_1_SQRT_2;
            let w = half * (0.2 + 1.2 * across);
            (w, w)
        }
        Brush::UnevenInk => {
            let flow = 0.8 + 0.35 * noise(at, (width * 5.0).max(5.0), seed);
            // Here and there the ink pools.
            let pool = noise(at, (width * 2.0).max(2.0), seed ^ 0x77);
            let pool = if pool > 0.8 {
                1.0 + (pool - 0.8) * 2.0
            } else {
                1.0
            };
            let left = 0.9 + 0.2 * noise(at, (width * 1.5).max(1.5), seed ^ 0x11);
            let right = 0.9 + 0.2 * noise(at, (width * 1.5).max(1.5), seed ^ 0x22);
            (half * flow * pool * left, half * flow * pool * right)
        }
        Brush::Chalk | Brush::DryBrush => (half, half),
    }
}

/// How much of the width is left near the ends of a line, which a brush
/// starts and leaves thin.
fn taper(at: f32, length: f32, over: f32) -> f32 {
    if !length.is_finite() {
        return 1.0;
    }
    let over = over.min(length / 3.0).max(1e-3);
    let near = at.min(length - at).max(0.0);
    (near / over).min(1.0).sqrt().max(0.15)
}

/// The points of one edge of a line.
type Edge = Vec<(f32, f32)>;

/// The shapes of a line between two edges that a profile gives, one shape
/// for every stretch without a gap.
fn band(
    samples: &[Sample],
    length: f32,
    closed: bool,
    profile: &dyn Fn(&Sample, f32) -> (f32, f32),
    out: &mut Vec<PathOp>,
) {
    let mut left: Vec<(f32, f32)> = Vec::new();
    let mut right: Vec<(f32, f32)> = Vec::new();
    let mut gaps = false;
    let mut stretches: Vec<(Edge, Edge)> = Vec::new();
    for s in samples {
        let (l, r) = profile(s, length);
        if l <= 0.0 && r <= 0.0 {
            gaps = true;
            if left.len() >= 2 {
                stretches.push((std::mem::take(&mut left), std::mem::take(&mut right)));
            }
            left.clear();
            right.clear();
            continue;
        }
        // The normal to the left of the direction.
        let (nx, ny) = (s.dy, -s.dx);
        left.push((s.x + nx * l, s.y + ny * l));
        right.push((s.x - nx * r, s.y - ny * r));
    }
    if left.len() >= 2 {
        stretches.push((left, right));
    }
    if closed && !gaps && stretches.len() == 1 {
        // A ring: the outer edge one way and the inner one the other, so
        // that the nonzero rule leaves the inside empty.
        let (left, right) = &stretches[0];
        polygon(left.iter().copied(), out);
        polygon(right.iter().rev().copied(), out);
        return;
    }
    for (left, right) in &stretches {
        polygon(left.iter().chain(right.iter().rev()).copied(), out);
    }
}

fn polygon(points: impl Iterator<Item = (f32, f32)>, out: &mut Vec<PathOp>) {
    let start = out.len();
    for (x, y) in points {
        out.push(if out.len() == start {
            PathOp::Move(x, y)
        } else {
            PathOp::Line(x, y)
        });
    }
    if out.len() - start < 3 {
        out.truncate(start);
    } else {
        out.push(PathOp::Close);
    }
}

/// Chalk: grains side by side along the path, scattered across its
/// width, with the paper showing between them.
fn chalk(samples: &[Sample], width: f32, seed: u32, out: &mut Vec<PathOp>) {
    let half = width / 2.0;
    // Grains a little under half the width, close enough to overlap.
    let spacing = (width * 0.3).max(0.3);
    let mut next = 0.0;
    let mut index: i64 = 0;
    for s in samples {
        if s.at < next {
            continue;
        }
        next = s.at + spacing;
        for k in 0..3i64 {
            index += 1;
            // A grain here and there is missing.
            if hash(index, seed ^ 0x0f) < 0.12 {
                continue;
            }
            let across = (hash(index, seed) * 2.0 - 1.0) * half * 0.75;
            let along = (hash(index, seed ^ 0x33) - 0.5) * spacing;
            let size = width * (0.18 + 0.14 * hash(index, seed ^ 0x55)) + 0.1 * k as f32;
            let (nx, ny) = (s.dy, -s.dx);
            let cx = s.x + s.dx * along + nx * across;
            let cy = s.y + s.dy * along + ny * across;
            // A grain is a square turned by an angle of its own.
            let turn = hash(index, seed ^ 0x99) * std::f32::consts::FRAC_PI_2;
            let (c, si) = (turn.cos() * size, turn.sin() * size);
            polygon(
                [(c, si), (-si, c), (-c, -si), (si, -c)]
                    .into_iter()
                    .map(|(x, y)| (cx + x, cy + y)),
                out,
            );
        }
    }
}

/// A dry brush: thin streaks side by side across the width, each with
/// gaps of its own where the bristle ran dry.
fn dry_brush(samples: &[Sample], width: f32, seed: u32, out: &mut Vec<PathOp>) {
    let bristles = ((width / 2.0).round() as usize).clamp(3, 7);
    let pitch = width / bristles as f32;
    let length = samples.last().map_or(0.0, |s| s.at);
    for b in 0..bristles {
        let seed = seed ^ (b as u32 + 1).wrapping_mul(0x85eb_ca6b);
        // The middle of the bristle across the line.
        let offset = -width / 2.0 + pitch * (b as f32 + 0.5);
        let thickness = pitch * (0.8 + 0.3 * hash(b as i64, seed));
        let streak = |s: &Sample, _: f32| -> (f32, f32) {
            // The outer bristles run dry more often.
            let edge = (offset.abs() / (width / 2.0)).min(1.0);
            let dry = noise(s.at, (width * 2.0).max(2.0), seed) < 0.08 + 0.25 * edge;
            if dry {
                (0.0, 0.0)
            } else {
                let w = thickness * (0.75 + 0.25 * noise(s.at, width.max(1.0), seed ^ 0x3c));
                // Edges to the left of the path, from the offset of the
                // bristle.
                (offset + w / 2.0, -(offset - w / 2.0))
            }
        };
        band_signed(samples, length, &streak, out);
    }
}

/// A band whose edges may both lie on one side of the path: the left edge
/// at `l` to the left, the right one at `r` to the right, either negative.
fn band_signed(
    samples: &[Sample],
    length: f32,
    profile: &dyn Fn(&Sample, f32) -> (f32, f32),
    out: &mut Vec<PathOp>,
) {
    let mut left: Vec<(f32, f32)> = Vec::new();
    let mut right: Vec<(f32, f32)> = Vec::new();
    let mut flush = |left: &mut Vec<(f32, f32)>, right: &mut Vec<(f32, f32)>| {
        if left.len() >= 2 {
            polygon(left.iter().chain(right.iter().rev()).copied(), out);
        }
        left.clear();
        right.clear();
    };
    for s in samples {
        let (l, r) = profile(s, length);
        if l == 0.0 && r == 0.0 {
            flush(&mut left, &mut right);
            continue;
        }
        let (nx, ny) = (s.dy, -s.dx);
        left.push((s.x + nx * l, s.y + ny * l));
        right.push((s.x - nx * r, s.y - ny * r));
    }
    flush(&mut left, &mut right);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<PathOp> {
        vec![
            PathOp::Move(10.0, 10.0),
            PathOp::Line(110.0, 10.0),
            PathOp::Line(110.0, 110.0),
            PathOp::Line(10.0, 110.0),
            PathOp::Close,
        ]
    }

    fn bounds(path: &[PathOp]) -> (f32, f32, f32, f32) {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for op in path {
            if let PathOp::Move(x, y) | PathOp::Line(x, y) = *op {
                b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            }
        }
        b
    }

    #[test]
    fn b_044_every_brush_draws_near_the_path_and_the_same_every_time() {
        for brush in Brush::ALL {
            let once = outline(&square(), 6.0, brush);
            assert!(!once.is_empty(), "{brush:?} draws");
            assert_eq!(once, outline(&square(), 6.0, brush), "{brush:?} is steady");
            assert!(
                once.iter().all(|op| match *op {
                    PathOp::Move(x, y) | PathOp::Line(x, y) => x.is_finite() && y.is_finite(),
                    _ => true,
                }),
                "{brush:?} has finite points"
            );
            let (x0, y0, x1, y1) = bounds(&once);
            // The pools of ink reach at most a little past the width.
            assert!(x0 > 10.0 - 6.0 && y0 > 10.0 - 6.0, "{brush:?} {x0} {y0}");
            assert!(x1 < 110.0 + 6.0 && y1 < 110.0 + 6.0, "{brush:?} {x1} {y1}");
            assert!(
                x0 < 10.0 + 1.0 && x1 > 110.0 - 1.0,
                "{brush:?} reaches the corners"
            );
        }
    }

    #[test]
    fn b_044_the_brushes_differ() {
        let lines: Vec<Vec<PathOp>> = Brush::ALL
            .iter()
            .map(|b| outline(&square(), 6.0, *b))
            .collect();
        for (i, a) in lines.iter().enumerate() {
            for b in &lines[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn b_044_chalk_and_a_dry_brush_leave_gaps() {
        let parts = |brush| {
            outline(&square(), 6.0, brush)
                .iter()
                .filter(|op| matches!(op, PathOp::Move(..)))
                .count()
        };
        assert!(parts(Brush::Chalk) > 4);
        assert!(parts(Brush::DryBrush) > 10);
        // A closed line of a thick brush is one ring, its two edges.
        assert_eq!(parts(Brush::ThickBrush), 2);
    }

    #[test]
    fn b_044_a_chisel_pen_is_thin_along_its_nib() {
        let width_of = |path: Vec<PathOp>| {
            let (_, y0, _, y1) = bounds(&path);
            let (x0, _, x1, _) = bounds(&path);
            (x1 - x0, y1 - y0)
        };
        // A line at 45 degrees, down to the right, goes across the nib;
        // one up to the right goes along it.
        let down = vec![PathOp::Move(0.0, 0.0), PathOp::Line(100.0, 100.0)];
        let up = vec![PathOp::Move(0.0, 100.0), PathOp::Line(100.0, 0.0)];
        let (w_down, _) = width_of(outline(&down, 10.0, Brush::ChiselPen));
        let (w_up, _) = width_of(outline(&up, 10.0, Brush::ChiselPen));
        assert!(w_down > w_up + 5.0, "{w_down} {w_up}");
    }

    #[test]
    fn b_044_nothing_to_draw() {
        for brush in Brush::ALL {
            assert!(outline(&square(), 0.0, brush).is_empty());
            assert!(outline(&square(), f32::NAN, brush).is_empty());
            assert!(outline(&[PathOp::Move(1.0, 1.0)], 4.0, brush).is_empty());
            assert!(outline(&[PathOp::Close], 4.0, brush).is_empty());
            let wild = [PathOp::Move(0.0, 0.0), PathOp::Line(f32::INFINITY, 0.0)];
            assert!(outline(&wild, 4.0, brush).is_empty());
        }
        // A huge path keeps to a bounded number of points.
        let huge = [PathOp::Move(0.0, 0.0), PathOp::Line(1.0e7, 0.0)];
        assert!(outline(&huge, 1.0, Brush::UnevenInk).len() < 50_000);
    }

    #[test]
    fn b_044_curves_are_followed() {
        let curve = [
            PathOp::Move(0.0, 0.0),
            PathOp::Curve(0.0, 50.0, 50.0, 100.0, 100.0, 100.0),
        ];
        let (x0, y0, x1, y1) = bounds(&outline(&curve, 4.0, Brush::UnevenInk));
        assert!(x0 < 3.0 && y0 < 3.0 && x1 > 97.0 && y1 > 97.0);
    }
}
