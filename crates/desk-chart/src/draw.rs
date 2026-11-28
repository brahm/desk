// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drawing a chart: the shapes and the text it is made of, in points from
//! the top left corner of its frame, for the display list to draw.

use std::f32::consts::PI;

use desk_table::Color;

use crate::model::{
    Chart, ChartKind, ErrorDirection, LabelPosition, Lighting, NumberStyle, Scale, SeriesFill,
    SeriesKind, SeriesStyle, Symbol, Texture, TrendKind, ValueAxis, darker, lighter,
};
use crate::stats;

/// A step of a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathOp {
    /// Starts a new part at a point.
    Move(f32, f32),
    /// A straight line to a point.
    Line(f32, f32),
    /// A cubic curve, by two control points, to a point.
    Curve(f32, f32, f32, f32, f32, f32),
    /// Back to the start of the part.
    Close,
}

/// How a shape is filled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    /// One color.
    Color(Color),
    /// A gradient from the first color at the top to the second at the
    /// bottom of the shape.
    Gradient(Color, Color),
    /// A texture of the fill collections.
    Texture(Texture),
}

/// Where a text sits on its point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// Starting at the point.
    Start,
    /// Centered on the point.
    Middle,
    /// Ending at the point.
    End,
}

/// Something a chart draws.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// A shape: a bar, a slice, an area, a line or a mark.
    Shape {
        /// The outline.
        path: Vec<PathOp>,
        /// The fill; none for a line.
        fill: Option<Paint>,
        /// The line: width, color and dashes.
        stroke: Option<(f32, Color, Vec<f32>)>,
        /// How opaque, from 0 to 1.
        opacity: f32,
        /// Whether it casts a shadow.
        shadow: bool,
    },
    /// A line of text.
    Text {
        /// Where it sits across.
        x: f32,
        /// Its baseline.
        y: f32,
        /// The text.
        text: String,
        /// The font family.
        font: String,
        /// The size, in points.
        size: f32,
        /// The color.
        color: Color,
        /// Bold.
        bold: bool,
        /// Italic.
        italic: bool,
        /// How it sits on the point.
        align: Align,
        /// The angle, counterclockwise in degrees.
        angle: f32,
    },
}

/// What a click on part of a chart selects: a series, and a category of
/// it for a slice of a pie (GRF-15).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// The series.
    pub series: usize,
    /// The category, for a bar or a point.
    pub category: Option<usize>,
    /// The box it covers.
    pub rect: (f32, f32, f32, f32),
}

/// What a chart draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// The marks, from back to front.
    pub marks: Vec<Mark>,
    /// The parts that a click selects.
    pub hits: Vec<Hit>,
}

/// How wide text is, by its text and size in points.
pub type Measure<'a> = &'a dyn Fn(&str, f32) -> f32;

/// A width of text good enough when no font is at hand.
pub fn rough_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.55
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<PathOp> {
    vec![
        PathOp::Move(x, y),
        PathOp::Line(x + w, y),
        PathOp::Line(x + w, y + h),
        PathOp::Line(x, y + h),
        PathOp::Close,
    ]
}

/// A box with rounded corners, by the radius of each: top left, top
/// right, bottom right and bottom left; a radius is at most half the side.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) -> Vec<PathOp> {
    let most = (w.min(h) / 2.0).max(0.0);
    let [tl, tr, br, bl] = radii.map(|r| r.clamp(0.0, most));
    if tl + tr + br + bl <= 0.0 {
        return rect(x, y, w, h);
    }
    let k = 1.0 - 0.552_284_8;
    let mut path = vec![PathOp::Move(x + tl, y), PathOp::Line(x + w - tr, y)];
    // A corner of no radius stays a corner, with no curve to it.
    let corner = |path: &mut Vec<PathOp>, r: f32, c: [f32; 6]| {
        if r > 0.0 {
            path.push(PathOp::Curve(c[0], c[1], c[2], c[3], c[4], c[5]));
        }
    };
    corner(
        &mut path,
        tr,
        [x + w - tr * k, y, x + w, y + tr * k, x + w, y + tr],
    );
    path.push(PathOp::Line(x + w, y + h - br));
    corner(
        &mut path,
        br,
        [
            x + w,
            y + h - br * k,
            x + w - br * k,
            y + h,
            x + w - br,
            y + h,
        ],
    );
    path.push(PathOp::Line(x + bl, y + h));
    corner(
        &mut path,
        bl,
        [x + bl * k, y + h, x, y + h - bl * k, x, y + h - bl],
    );
    path.push(PathOp::Line(x, y + tl));
    corner(&mut path, tl, [x, y + tl * k, x + tl * k, y, x + tl, y]);
    path.push(PathOp::Close);
    path
}

/// The radii of the corners of a 2D bar, as [`rounded_rect`] takes them
/// (W145): all four corners, or with Outside Corners Only those of the
/// end away from the axis, and in a stack only at its outer end.
fn bar_corners(
    chart: &Chart,
    thickness: f32,
    negative: bool,
    horizontal: bool,
    outermost: bool,
) -> [f32; 4] {
    let r = thickness * f32::from(chart.corner_radius.min(50)) / 100.0;
    if !chart.kind.has_bars() || r <= 0.0 {
        return [0.0; 4];
    }
    if !chart.outside_corners_only {
        return [r; 4];
    }
    if !outermost {
        return [0.0; 4];
    }
    match (horizontal, negative) {
        (false, false) => [r, r, 0.0, 0.0],
        (false, true) => [0.0, 0.0, r, r],
        (true, false) => [0.0, r, r, 0.0],
        (true, true) => [r, 0.0, 0.0, r],
    }
}

fn polygon(points: &[(f32, f32)]) -> Vec<PathOp> {
    let mut path: Vec<PathOp> = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == 0 {
                PathOp::Move(p.0, p.1)
            } else {
                PathOp::Line(p.0, p.1)
            }
        })
        .collect();
    path.push(PathOp::Close);
    path
}

fn polyline(points: &[(f32, f32)], curved: bool) -> Vec<PathOp> {
    let mut path = Vec::new();
    let Some(first) = points.first() else {
        return path;
    };
    path.push(PathOp::Move(first.0, first.1));
    if !curved || points.len() < 3 {
        path.extend(points[1..].iter().map(|p| PathOp::Line(p.0, p.1)));
        return path;
    }
    // Catmull-Rom through the points, as Bézier curves.
    for i in 0..points.len() - 1 {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(points.len() - 1)];
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        path.push(PathOp::Curve(c1.0, c1.1, c2.0, c2.1, p2.0, p2.1));
    }
    path
}

fn paint(fill: &SeriesFill) -> Paint {
    match fill {
        SeriesFill::Color(c) => Paint::Color(*c),
        SeriesFill::Gradient(a, b) => Paint::Gradient(*a, *b),
        SeriesFill::Texture(t) => Paint::Texture(*t),
    }
}

fn shade(fill: &SeriesFill, f: fn(Color) -> Color) -> Paint {
    match fill {
        SeriesFill::Color(c) => Paint::Color(f(*c)),
        SeriesFill::Gradient(a, b) => Paint::Gradient(f(*a), f(*b)),
        SeriesFill::Texture(t) => Paint::Color(f(t.base())),
    }
}

/// A scale of values along an axis.
#[derive(Clone, Copy, Debug)]
struct Range {
    min: f64,
    max: f64,
    step: f64,
    log: bool,
}

impl Range {
    /// Where a value falls along the axis, from 0 at its minimum to 1 at
    /// its maximum; a value far outside it stays at most a thousand lengths
    /// of the axis away, which the plot clips, rather than at coordinates
    /// too large for Cairo.
    fn fraction(&self, v: f64) -> f32 {
        let f = if self.log {
            let lv = v.max(self.min).log10();
            (lv - self.min.log10()) / (self.max.log10() - self.min.log10())
        } else {
            (v - self.min) / (self.max - self.min)
        };
        if f.is_nan() {
            0.0
        } else {
            f.clamp(-1000.0, 1000.0) as f32
        }
    }

    fn ticks(&self) -> Vec<f64> {
        let mut out = Vec::new();
        if self.log {
            let mut v = self.min;
            // A decade each, as many as a double has at most.
            while v <= self.max * 1.000_001 && v.is_finite() && out.len() < 700 {
                out.push(v);
                v *= 10.0;
            }
            return out;
        }
        let n = ((self.max - self.min) / self.step).round() as i64;
        for i in 0..=n.clamp(0, 100) {
            out.push(self.min + self.step * i as f64);
        }
        out
    }
}

/// A step of 1, 2, 2.5 or 5 times a power of 10 that splits a span in
/// about `count` parts.
fn nice_step(span: f64, count: f64) -> f64 {
    if span <= 0.0 || !span.is_finite() {
        return 1.0;
    }
    let raw = span / count;
    let p = 10f64.powf(raw.log10().floor());
    // A span so small or so large that its powers of ten underflow or
    // overflow has no nice step.
    if !(p.is_finite() && p > 0.0) {
        return 1.0;
    }
    let f = raw / p;
    let n = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 2.5 {
        2.5
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    n * p
}

/// The range of a value axis, finite and with its maximum above its
/// minimum, whatever the values and the limits a file gives.
fn value_range(axis: &ValueAxis, lo: f64, hi: f64, zero: bool) -> Range {
    let range = any_value_range(axis, lo, hi, zero);
    let finite = [range.min, range.max, range.step]
        .iter()
        .all(|v| v.is_finite());
    if finite && range.min < range.max && range.step > 0.0 {
        return range;
    }
    fallback_range(range.log, lo, hi)
}

/// A range around the values and as wide as they spread, so that they
/// still fall on the axis: as when they and the minimum are all 1e16,
/// which adding 1 does not change. The limits of the file are left out.
fn fallback_range(log: bool, lo: f64, hi: f64) -> Range {
    let (lo, hi) = if lo.is_finite() && hi.is_finite() && lo <= hi {
        (lo, hi)
    } else {
        (0.0, 0.0)
    };
    if log {
        // Decades from the smallest value a log axis shows to a decade
        // below the largest double.
        let lo = lo.clamp(1e-12, 1e300);
        let min = 10f64.powf(lo.log10().floor());
        let max = 10f64.powf(hi.clamp(lo, 1e306).log10().ceil());
        Range {
            min,
            max: if max <= min { min * 10.0 } else { max },
            step: 10.0,
            log: true,
        }
    } else {
        // Halves, so that neither the spread nor the ends overflow.
        let limit = f64::MAX / 4.0;
        let mid = (lo / 2.0 + hi / 2.0).clamp(-limit, limit);
        let half = (hi / 2.0 - lo / 2.0)
            .max(mid.abs() / 2.0)
            .max(0.5)
            .min(limit);
        Range {
            min: mid - half,
            max: mid + half,
            step: half / 2.5,
            log: false,
        }
    }
}

fn any_value_range(axis: &ValueAxis, lo: f64, hi: f64, zero: bool) -> Range {
    if axis.scale == Scale::Logarithmic {
        // A decade below the largest double, for the axis to end on one.
        let lo = axis.min.map_or(lo, |m| m.0).clamp(1e-12, 1e300);
        let hi = axis.max.map_or(hi, |m| m.0).max(lo * 10.0).min(1e307);
        let min = 10f64.powf(lo.log10().floor());
        let max = 10f64.powf(hi.log10().ceil());
        return Range {
            min,
            max: if max <= min { min * 10.0 } else { max },
            step: 10.0,
            log: true,
        };
    }
    let (mut lo, mut hi) = (lo, hi);
    if zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    if let Some(m) = axis.min {
        lo = m.0;
    }
    if let Some(m) = axis.max {
        hi = m.0;
    }
    if hi <= lo {
        hi = lo + 1.0;
    }
    let steps = f64::from(axis.steps.unwrap_or(0));
    let step = if steps > 0.0 && axis.min.is_some() && axis.max.is_some() {
        (hi - lo) / steps
    } else {
        nice_step(hi - lo, if steps > 0.0 { steps } else { 5.0 })
    };
    let min = if axis.min.is_some() {
        lo
    } else {
        (lo / step).floor() * step
    };
    let max = if axis.max.is_some() {
        hi
    } else {
        (hi / step).ceil() * step
    };
    Range {
        min,
        max: if max <= min { min + step } else { max },
        step,
        log: false,
    }
}

struct Ctx<'a> {
    chart: &'a Chart,
    measure: Measure<'a>,
    size: f32,
    decimal: char,
    group: char,
    out: Drawing,
}

impl Ctx<'_> {
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        x: f32,
        y: f32,
        text: impl Into<String>,
        size: f32,
        align: Align,
        angle: f32,
        bold: bool,
    ) {
        let t = &self.chart.text;
        self.out.marks.push(Mark::Text {
            x,
            y,
            text: text.into(),
            font: t.font.clone(),
            size,
            color: t.color,
            bold: bold || t.bold,
            italic: t.italic,
            align,
            angle,
        });
    }

    fn shape(
        &mut self,
        path: Vec<PathOp>,
        fill: Option<Paint>,
        stroke: Option<(f32, Color, Vec<f32>)>,
        opacity: f32,
        shadow: bool,
    ) {
        self.out.marks.push(Mark::Shape {
            path,
            fill,
            stroke,
            opacity,
            shadow,
        });
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, color: Color) {
        self.shape(
            vec![PathOp::Move(a.0, a.1), PathOp::Line(b.0, b.1)],
            None,
            Some((width, color, Vec::new())),
            1.0,
            false,
        );
    }

    fn number(&self, style: &NumberStyle, v: f64) -> String {
        style.write(v, self.decimal, self.group)
    }

    fn width(&self, text: &str, size: f32) -> f32 {
        (self.measure)(text, size)
    }
}

/// Draws a chart in a frame of a width and a height in points, with the
/// decimal and thousands separators of the locale.
pub fn draw(
    chart: &Chart,
    width: f32,
    height: f32,
    measure: Measure<'_>,
    decimal: char,
    group: char,
) -> Drawing {
    let size = chart.text.size as f32 / 1000.0;
    let mut cx = Ctx {
        chart,
        measure,
        size,
        decimal,
        group,
        out: Drawing::default(),
    };
    let pad = 6.0;
    let mut top = pad;
    let (left, right, bottom) = (pad, width - pad, height - pad);
    if let Some(title) = &chart.title {
        let s = size * 1.4;
        top += s;
        cx.text(width / 2.0, top, title.clone(), s, Align::Middle, 0.0, true);
        top += s * 0.6;
    }
    if chart.legend && chart.series_count() > 0 {
        top = legend(&mut cx, left, right, top + 2.0);
    }
    let area = (left, top + 4.0, right - left, bottom - top - 4.0);
    if area.2 <= 10.0 || area.3 <= 10.0 {
        return cx.out;
    }
    match chart.kind {
        ChartKind::Pie | ChartKind::Donut => pie(&mut cx, area),
        ChartKind::Scatter | ChartKind::Bubble => scatter(&mut cx, area),
        ChartKind::Radar => radar(&mut cx, area),
        _ => cartesian(&mut cx, area),
    }
    cx.out
}

fn legend(cx: &mut Ctx<'_>, left: f32, right: f32, top: f32) -> f32 {
    let chart = cx.chart;
    let size = cx.size;
    let swatch = size * 0.8;
    let gap = size;
    // A scatter or a bubble chart names each plotted series after its y
    // values, and shows its look; other kinds, each series (W142).
    let names: Vec<(usize, usize, String)> = if chart.kind.has_x_values() {
        chart
            .point_series()
            .iter()
            .enumerate()
            .map(|(p, ps)| (p, ps.y, chart.series_name(ps.y).to_owned()))
            .collect()
    } else {
        (0..chart.series_count())
            .map(|s| (s, s, chart.series_name(s).to_owned()))
            .collect()
    };
    let entries: Vec<(usize, usize, String, f32)> = names
        .into_iter()
        .map(|(s, look, n)| {
            let w = swatch + 4.0 + cx.width(&n, size);
            (s, look, n, w)
        })
        .collect();
    // Lines of entries, centered.
    let mut lines: Vec<Vec<&(usize, usize, String, f32)>> = vec![Vec::new()];
    let mut line_w = 0.0;
    for e in &entries {
        if line_w > 0.0 && line_w + gap + e.3 > right - left {
            lines.push(Vec::new());
            line_w = 0.0;
        }
        line_w += if line_w > 0.0 { gap } else { 0.0 } + e.3;
        if let Some(l) = lines.last_mut() {
            l.push(e);
        }
    }
    let mut y = top;
    for line in lines {
        let total: f32 =
            line.iter().map(|e| e.3).sum::<f32>() + gap * line.len().saturating_sub(1) as f32;
        let mut x = (left + right - total) / 2.0;
        y += size * 1.2;
        for (s, look, name, w) in line {
            let style = chart.style(*look);
            let sy = y - swatch;
            cx.shape(
                rect(x, sy, swatch, swatch),
                Some(paint(&style.fill)),
                None,
                1.0,
                false,
            );
            cx.out.hits.push(Hit {
                series: *s,
                category: None,
                rect: (x, sy, *w, swatch),
            });
            cx.text(
                x + swatch + 4.0,
                y,
                name.clone(),
                size,
                Align::Start,
                0.0,
                false,
            );
            x += w + gap;
        }
    }
    y + size * 0.4
}

/// The values of each series in each category, stacked or as they are;
/// with a percent scale each category adds up to 100. A chart with no
/// categories gets one empty category, as the axes draw one, so that each
/// series has a value for every category the drawing goes through.
fn series_values(chart: &Chart) -> Vec<Vec<Option<f64>>> {
    let ns = chart.series_count();
    let nc = chart.category_count().max(1);
    let mut v: Vec<Vec<Option<f64>>> = (0..ns)
        .map(|s| (0..nc).map(|c| chart.value(s, c)).collect())
        .collect();
    if chart.kind.is_stacked() && chart.value_axis.scale == Scale::Percent {
        for c in 0..nc {
            let total: f64 = (0..ns).filter_map(|s| v[s][c]).map(f64::abs).sum();
            if total > 0.0 {
                for row in v.iter_mut() {
                    if let Some(x) = row[c].as_mut() {
                        *x = *x / total * 100.0;
                    }
                }
            }
        }
    }
    v
}

fn axis_extent(
    chart: &Chart,
    values: &[Vec<Option<f64>>],
    series: impl Fn(usize) -> bool,
) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let nc = chart.category_count();
    if chart.kind.is_stacked() {
        for c in 0..nc {
            let (mut pos, mut neg) = (0.0, 0.0);
            for (s, row) in values.iter().enumerate() {
                if !series(s) {
                    continue;
                }
                if let Some(v) = row[c] {
                    if v >= 0.0 {
                        pos += v;
                    } else {
                        neg += v;
                    }
                }
            }
            lo = lo.min(neg);
            hi = hi.max(pos);
        }
    } else {
        for (s, row) in values.iter().enumerate() {
            if !series(s) {
                continue;
            }
            for v in row.iter().flatten() {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
            // Room for error bars.
            let style = chart.style(s);
            if let Some(e) = &style.errors {
                let vals: Vec<f64> = row.iter().map(|v| v.unwrap_or(0.0)).collect();
                for (i, (p, m)) in stats::error_lengths(&e.amount, &vals).iter().enumerate() {
                    if row[i].is_some() {
                        hi = hi.max(vals[i] + p);
                        lo = lo.min(vals[i] - m);
                    }
                }
            }
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        (0.0, 1.0)
    } else {
        (lo, hi)
    }
}

/// The 3D offset of the depth of the bars: right and up.
fn depth_offset(chart: &Chart, bar: f32) -> (f32, f32) {
    if !chart.three_d {
        return (0.0, 0.0);
    }
    let d = bar * f32::from(chart.scene.depth) / 100.0;
    let yaw = f32::from(chart.scene.yaw).to_radians();
    let pitch = f32::from(chart.scene.pitch.max(5)).to_radians();
    (
        d * yaw.sin().abs().max(0.35) * yaw.signum().max(0.0).max(0.6),
        -d * pitch.sin().max(0.25),
    )
}

/// A box with its depth, as the bars of 3D charts: the front, the top
/// and the side, lit as the scene says.
type Shades = (fn(Color) -> Color, fn(Color) -> Color);

#[allow(clippy::too_many_arguments)]
fn prism(
    cx: &mut Ctx<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    d: (f32, f32),
    style: &SeriesStyle,
    s: usize,
    c: usize,
) {
    let fill = &style.fill;
    let opacity = f32::from(style.opacity) / 100.0;
    let (top_shade, side_shade): Shades = match cx.chart.scene.lighting {
        Lighting::Soft => (lighter, darker),
        Lighting::Strong => (lighter, |c| darker(darker(c))),
        Lighting::Flat => (|c| c, |c| c),
    };
    cx.shape(
        polygon(&[
            (x, y),
            (x + d.0, y + d.1),
            (x + w + d.0, y + d.1),
            (x + w, y),
        ]),
        Some(shade(fill, top_shade)),
        None,
        opacity,
        false,
    );
    cx.shape(
        polygon(&[
            (x + w, y),
            (x + w + d.0, y + d.1),
            (x + w + d.0, y + h + d.1),
            (x + w, y + h),
        ]),
        Some(shade(fill, side_shade)),
        None,
        opacity,
        false,
    );
    cx.shape(
        rect(x, y, w, h),
        Some(paint(fill)),
        stroke_of(style),
        opacity,
        style.shadow,
    );
    // Beveled edges: the front catches the light on its top and left and
    // falls in shadow on its bottom and right (W145).
    if cx.chart.scene.bevel && w > 2.0 && h > 2.0 {
        let b = (w.min(h) * 0.12).clamp(0.5, 3.0);
        let color = style.color();
        cx.shape(
            polygon(&[
                (x, y + h),
                (x, y),
                (x + w, y),
                (x + w - b, y + b),
                (x + b, y + b),
                (x + b, y + h - b),
            ]),
            Some(Paint::Color(lighter(lighter(color)))),
            None,
            opacity * 0.8,
            false,
        );
        cx.shape(
            polygon(&[
                (x + w, y),
                (x + w, y + h),
                (x, y + h),
                (x + b, y + h - b),
                (x + w - b, y + h - b),
                (x + w - b, y + b),
            ]),
            Some(Paint::Color(darker(color))),
            None,
            opacity * 0.8,
            false,
        );
    }
    cx.out.hits.push(Hit {
        series: s,
        category: Some(c),
        rect: (x, y + d.1, w + d.0, h - d.1),
    });
}

fn stroke_of(style: &SeriesStyle) -> Option<(f32, Color, Vec<f32>)> {
    style
        .stroke
        .map(|(w, c)| (w as f32 / 1000.0, c, Vec::new()))
}

fn symbol(cx: &mut Ctx<'_>, kind: Symbol, x: f32, y: f32, size: f32, color: Color) {
    let r = size / 2.0;
    let path = match kind {
        Symbol::None => return,
        Symbol::Circle => circle(x, y, r),
        Symbol::Square => rect(x - r, y - r, size, size),
        Symbol::Triangle => polygon(&[(x, y - r), (x + r, y + r), (x - r, y + r)]),
        Symbol::Diamond => polygon(&[(x, y - r), (x + r, y), (x, y + r), (x - r, y)]),
    };
    cx.shape(
        path,
        Some(Paint::Color(color)),
        Some((0.75, Color::WHITE, Vec::new())),
        1.0,
        false,
    );
}

/// Value axis labels and gridlines; returns the width the labels take.
fn axis_label_width(cx: &Ctx<'_>, axis: &ValueAxis, range: &Range) -> f32 {
    if !axis.labels {
        return 0.0;
    }
    range
        .ticks()
        .iter()
        .map(|v| cx.width(&axis_text(cx, axis, *v), cx.size))
        .fold(0.0, f32::max)
        + 4.0
}

fn axis_text(cx: &Ctx<'_>, axis: &ValueAxis, v: f64) -> String {
    let mut s = cx.number(&axis.number, v);
    if axis.scale == Scale::Percent && axis.number.suffix.is_empty() {
        s.push('%');
    }
    s
}

#[allow(clippy::too_many_lines)]
fn cartesian(cx: &mut Ctx<'_>, area: (f32, f32, f32, f32)) {
    let chart = cx.chart;
    let size = cx.size;
    let ns = chart.series_count();
    let nc = chart.category_count().max(1);
    let horizontal = chart.kind.is_horizontal();
    let two_axes = chart.kind == ChartKind::TwoAxis;
    let mixed = matches!(chart.kind, ChartKind::Mixed | ChartKind::TwoAxis);
    let values = series_values(chart);
    let on_y2 = |s: usize| two_axes && s % 2 == 1;
    let kind_of = |s: usize| -> SeriesKind {
        if mixed {
            chart.style(s).kind
        } else {
            match chart.kind {
                ChartKind::Line => SeriesKind::Line,
                ChartKind::Area | ChartKind::StackedArea => SeriesKind::Area,
                _ => SeriesKind::Column,
            }
        }
    };
    let zero = !matches!(chart.kind, ChartKind::Line) || chart.value_axis.min.is_none();
    let (lo, hi) = axis_extent(chart, &values, |s| !on_y2(s));
    let r1 = value_range(&chart.value_axis, lo, hi, zero);
    let r2 = two_axes.then(|| {
        let (lo, hi) = axis_extent(chart, &values, on_y2);
        value_range(&chart.value_axis2, lo, hi, true)
    });
    // Room for the labels and titles of the axes.
    let (mut x0, mut y0, mut w, mut h) = area;
    let cat_labels: Vec<String> = (0..nc).map(|c| chart.category_name(c).to_owned()).collect();
    let line_h = size * 1.3;
    if let Some(t) = &chart.value_axis.title {
        if horizontal {
            h -= line_h;
            cx.text(
                x0 + w / 2.0,
                y0 + h + line_h - 2.0,
                t.clone(),
                size,
                Align::Middle,
                0.0,
                false,
            );
        } else {
            cx.text(
                x0 + size,
                y0 + h / 2.0,
                t.clone(),
                size,
                Align::Middle,
                90.0,
                false,
            );
            x0 += line_h;
            w -= line_h;
        }
    }
    if let Some(t) = &chart.category_axis.title {
        if horizontal {
            cx.text(
                x0 + size,
                y0 + h / 2.0,
                t.clone(),
                size,
                Align::Middle,
                90.0,
                false,
            );
            x0 += line_h;
            w -= line_h;
        } else {
            h -= line_h;
            cx.text(
                x0 + w / 2.0,
                y0 + h + line_h - 2.0,
                t.clone(),
                size,
                Align::Middle,
                0.0,
                false,
            );
        }
    }
    if let (Some(r2), true) = (&r2, two_axes) {
        let lw = axis_label_width(cx, &chart.value_axis2, r2);
        w -= lw;
        if let Some(t) = &chart.value_axis2.title {
            cx.text(
                x0 + w + lw - 2.0,
                y0 + h / 2.0,
                t.clone(),
                size,
                Align::Middle,
                -90.0,
                false,
            );
            w -= line_h;
        }
    }
    let value_label_w = axis_label_width(cx, &chart.value_axis, &r1);
    let cat_label_w = if chart.category_axis.labels {
        cat_labels
            .iter()
            .map(|t| cx.width(t, size))
            .fold(0.0, f32::max)
            + 6.0
    } else {
        0.0
    };
    if horizontal {
        x0 += cat_label_w;
        w -= cat_label_w;
        if chart.value_axis.labels {
            h -= line_h;
        }
    } else {
        x0 += value_label_w;
        w -= value_label_w;
        if chart.category_axis.labels {
            h -= line_h;
        }
    }
    // Room for the depth of 3D bars.
    let probe_bar = if horizontal { h } else { w } / nc as f32 / 2.0;
    let d = depth_offset(chart, probe_bar.min(40.0));
    w -= d.0;
    y0 -= d.1;
    h += d.1;
    if w <= 4.0 || h <= 4.0 {
        return;
    }
    let grid = Color::rgb(0xd0, 0xd0, 0xd0);
    let axis_color = Color::rgb(0x77, 0x76, 0x7b);
    // Where a value sits along the value axis, from the plot's origin.
    let pos = |r: &Range, v: f64| -> f32 {
        let f = r.fraction(v).clamp(-0.05, 1.05);
        if horizontal {
            x0 + f * w
        } else {
            y0 + h - f * h
        }
    };
    // Gridlines and labels of the value axes.
    for (ri, (r, axis)) in [
        (Some(&r1), &chart.value_axis),
        (r2.as_ref(), &chart.value_axis2),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(r) = r else { continue };
        let ticks = r.ticks();
        if axis.minor_gridlines && ri == 0 && !r.log {
            for t in ticks.windows(2) {
                let m = (t[0] + t[1]) / 2.0;
                let p = pos(r, m);
                let (a, b) = if horizontal {
                    ((p, y0), (p, y0 + h))
                } else {
                    ((x0, p), (x0 + w, p))
                };
                cx.shape(
                    vec![
                        PathOp::Move(a.0 + d.0, a.1 + d.1),
                        PathOp::Line(b.0 + d.0, b.1 + d.1),
                    ],
                    None,
                    Some((0.5, grid, vec![2.0, 2.0])),
                    1.0,
                    false,
                );
            }
        }
        for (i, t) in ticks.iter().enumerate() {
            let p = pos(r, *t);
            if axis.gridlines && ri == 0 {
                let (a, b) = if horizontal {
                    ((p, y0), (p, y0 + h))
                } else {
                    ((x0, p), (x0 + w, p))
                };
                if chart.three_d {
                    cx.line(a, (a.0 + d.0, a.1 + d.1), 0.5, grid);
                }
                cx.line((a.0 + d.0, a.1 + d.1), (b.0 + d.0, b.1 + d.1), 0.5, grid);
            }
            if axis.ticks {
                if horizontal {
                    cx.line((p, y0 + h), (p, y0 + h + 3.0), 0.5, axis_color);
                } else if ri == 0 {
                    cx.line((x0 - 3.0, p), (x0, p), 0.5, axis_color);
                } else {
                    cx.line((x0 + w + d.0, p), (x0 + w + d.0 + 3.0, p), 0.5, axis_color);
                }
            }
            if axis.labels && (i > 0 || axis.min_label) {
                let text = axis_text(cx, axis, *t);
                if horizontal {
                    cx.text(
                        p,
                        y0 + h + size * 1.1,
                        text,
                        size,
                        Align::Middle,
                        0.0,
                        false,
                    );
                } else if ri == 0 {
                    cx.text(
                        x0 - 4.0,
                        p + size * 0.35,
                        text,
                        size,
                        Align::End,
                        0.0,
                        false,
                    );
                } else {
                    cx.text(
                        x0 + w + d.0 + 4.0,
                        p + size * 0.35,
                        text,
                        size,
                        Align::Start,
                        0.0,
                        false,
                    );
                }
            }
        }
        if axis.line {
            if horizontal {
                cx.line((x0, y0 + h), (x0 + w, y0 + h), 0.75, axis_color);
            } else if ri == 0 {
                cx.line((x0, y0), (x0, y0 + h), 0.75, axis_color);
            } else {
                cx.line((x0 + w + d.0, y0), (x0 + w + d.0, y0 + h), 0.75, axis_color);
            }
        }
    }
    // The category axis.
    let cat_len = if horizontal { h } else { w };
    let cat_w = cat_len / nc as f32;
    let cat_center = |c: usize| -> f32 {
        if horizontal {
            y0 + cat_w * (c as f32 + 0.5)
        } else {
            x0 + cat_w * (c as f32 + 0.5)
        }
    };
    let base = pos(
        &r1,
        if r1.log {
            r1.min
        } else {
            0.0f64.clamp(r1.min, r1.max)
        },
    );
    let ca = &chart.category_axis;
    if ca.line {
        if horizontal {
            cx.line((base, y0), (base, y0 + h), 0.75, axis_color);
        } else {
            cx.line((x0, base), (x0 + w, base), 0.75, axis_color);
        }
    }
    #[allow(clippy::needless_range_loop)]
    for c in 0..nc {
        if ca.gridlines && c > 0 {
            let p = if horizontal {
                y0 + cat_w * c as f32
            } else {
                x0 + cat_w * c as f32
            };
            let (a, b) = if horizontal {
                ((x0, p), (x0 + w, p))
            } else {
                ((p, y0), (p, y0 + h))
            };
            cx.line(a, b, 0.5, grid);
        }
        if ca.ticks {
            let p = if horizontal {
                y0 + cat_w * c as f32
            } else {
                x0 + cat_w * c as f32
            };
            if horizontal {
                cx.line((x0 - 3.0, p), (x0, p), 0.5, axis_color);
            } else {
                cx.line((p, y0 + h), (p, y0 + h + 3.0), 0.5, axis_color);
            }
        }
        let every = usize::from(ca.every.max(1));
        let shown = c % every == 0 || (ca.last && c == nc - 1);
        if ca.labels && shown {
            let p = cat_center(c);
            if horizontal {
                cx.text(
                    x0 - 4.0,
                    p + size * 0.35,
                    cat_labels[c].clone(),
                    size,
                    Align::End,
                    0.0,
                    false,
                );
            } else {
                cx.text(
                    p,
                    y0 + h + size * 1.1,
                    cat_labels[c].clone(),
                    size,
                    Align::Middle,
                    0.0,
                    false,
                );
            }
        }
    }
    // The series.
    let column_series: Vec<usize> = (0..ns)
        .filter(|s| kind_of(*s) == SeriesKind::Column)
        .collect();
    let stacked = chart.kind.is_stacked();
    let bars = if stacked {
        1
    } else {
        column_series.len().max(1)
    };
    let gap = f32::from(chart.bar_gap) / 100.0;
    let set_gap = f32::from(chart.set_gap) / 100.0;
    let bar = cat_w / (bars as f32 + gap * (bars as f32 - 1.0) + set_gap);
    let d = depth_offset(chart, bar);
    let mut pos_stack = vec![0.0f64; nc];
    let mut neg_stack = vec![0.0f64; nc];
    // The series at the outer end of each stack, above and below the axis,
    // whose corners Outside Corners Only rounds (W145).
    let outer = |c: usize, negative: bool| -> Option<usize> {
        column_series
            .iter()
            .rev()
            .copied()
            .find(|s| values[*s][c].is_some_and(|v| (v < 0.0) == negative && v != 0.0))
    };
    let mut labels: Vec<(f32, f32, String, Align)> = Vec::new();
    let draw_order: Vec<usize> = (0..ns).collect();
    // Areas first, then columns, then lines, so lines stay visible.
    for pass in [SeriesKind::Area, SeriesKind::Column, SeriesKind::Line] {
        let mut area_base: Vec<f64> = vec![0.0; nc];
        for &s in &draw_order {
            if kind_of(s) != pass {
                continue;
            }
            let style = chart.style(s);
            let r = if on_y2(s) {
                r2.as_ref().unwrap_or(&r1)
            } else {
                &r1
            };
            let opacity = f32::from(style.opacity) / 100.0;
            match pass {
                SeriesKind::Column => {
                    let k = column_series.iter().position(|x| *x == s).unwrap_or(0);
                    for c in 0..nc {
                        let Some(v) = values[s][c] else { continue };
                        let start = cat_center(c) - cat_w / 2.0 + bar * set_gap / 2.0;
                        let along = if stacked {
                            start
                        } else {
                            start + k as f32 * bar * (1.0 + gap)
                        };
                        let (from, to) = if stacked {
                            let from = if v >= 0.0 { pos_stack[c] } else { neg_stack[c] };
                            let to = from + v;
                            if v >= 0.0 {
                                pos_stack[c] = to;
                            } else {
                                neg_stack[c] = to;
                            }
                            (from, to)
                        } else {
                            (
                                if r.log {
                                    r.min
                                } else {
                                    0.0f64.clamp(r.min, r.max)
                                },
                                v,
                            )
                        };
                        let (a, b) = (pos(r, from), pos(r, to));
                        let (x, y, bw, bh) = if horizontal {
                            (a.min(b), along, (b - a).abs(), bar)
                        } else {
                            (along, a.min(b), bar, (b - a).abs())
                        };
                        if chart.three_d {
                            prism(cx, x, y, bw, bh, d, &style, s, c);
                        } else {
                            let outermost = !stacked || outer(c, v < 0.0) == Some(s);
                            let corners = bar_corners(chart, bar, v < 0.0, horizontal, outermost);
                            cx.shape(
                                rounded_rect(x, y, bw, bh, corners),
                                Some(paint(&style.fill)),
                                stroke_of(&style),
                                opacity,
                                style.shadow,
                            );
                            cx.out.hits.push(Hit {
                                series: s,
                                category: Some(c),
                                rect: (x, y, bw, bh),
                            });
                        }
                        error_bar(
                            cx,
                            &style,
                            &values[s],
                            c,
                            r,
                            &pos,
                            if horizontal {
                                y + bh / 2.0
                            } else {
                                x + bw / 2.0
                            },
                            horizontal,
                        );
                        if style.labels.show {
                            let text = cx.number(&style.labels.number, v);
                            let (lx, ly, al) = bar_label(
                                style.labels.position,
                                x,
                                y,
                                bw,
                                bh,
                                v < 0.0,
                                horizontal,
                                size,
                            );
                            labels.push((lx, ly, text, al));
                        }
                    }
                }
                SeriesKind::Line | SeriesKind::Area => {
                    let pts: Vec<(usize, f64)> = (0..nc)
                        .filter_map(|c| values[s][c].map(|v| (c, v)))
                        .collect();
                    if pts.is_empty() {
                        continue;
                    }
                    let to_xy = |c: usize, v: f64| -> (f32, f32) {
                        if horizontal {
                            (pos(r, v), cat_center(c))
                        } else {
                            (cat_center(c), pos(r, v))
                        }
                    };
                    if pass == SeriesKind::Area {
                        let stacked_area = chart.kind == ChartKind::StackedArea;
                        let top: Vec<(f32, f32)> = pts
                            .iter()
                            .map(|(c, v)| {
                                to_xy(*c, if stacked_area { area_base[*c] + v } else { *v })
                            })
                            .collect();
                        let bottom: Vec<(f32, f32)> = pts
                            .iter()
                            .rev()
                            .map(|(c, _)| {
                                to_xy(
                                    *c,
                                    if stacked_area {
                                        area_base[*c]
                                    } else {
                                        0.0f64.clamp(r.min, r.max)
                                    },
                                )
                            })
                            .collect();
                        let mut poly = top.clone();
                        poly.extend(bottom);
                        if chart.three_d {
                            let back: Vec<(f32, f32)> =
                                poly.iter().map(|p| (p.0 + d.0, p.1 + d.1)).collect();
                            cx.shape(
                                polygon(&back),
                                Some(shade(&style.fill, darker)),
                                None,
                                opacity,
                                false,
                            );
                            for win in top.windows(2) {
                                let quad = [
                                    win[0],
                                    win[1],
                                    (win[1].0 + d.0, win[1].1 + d.1),
                                    (win[0].0 + d.0, win[0].1 + d.1),
                                ];
                                cx.shape(
                                    polygon(&quad),
                                    Some(shade(&style.fill, lighter)),
                                    None,
                                    opacity,
                                    false,
                                );
                            }
                        }
                        cx.shape(
                            polygon(&poly),
                            Some(paint(&style.fill)),
                            stroke_of(&style),
                            opacity,
                            style.shadow,
                        );
                        let xs: Vec<f32> = poly.iter().map(|p| p.0).collect();
                        let ys: Vec<f32> = poly.iter().map(|p| p.1).collect();
                        let (minx, maxx) = (
                            xs.iter().copied().fold(f32::MAX, f32::min),
                            xs.iter().copied().fold(f32::MIN, f32::max),
                        );
                        let (miny, maxy) = (
                            ys.iter().copied().fold(f32::MAX, f32::min),
                            ys.iter().copied().fold(f32::MIN, f32::max),
                        );
                        cx.out.hits.push(Hit {
                            series: s,
                            category: None,
                            rect: (minx, miny, maxx - minx, maxy - miny),
                        });
                        if stacked_area {
                            for (c, v) in &pts {
                                area_base[*c] += v;
                            }
                        }
                    } else {
                        let xy: Vec<(f32, f32)> = pts.iter().map(|(c, v)| to_xy(*c, *v)).collect();
                        let lw = style.line_width as f32 / 1000.0;
                        if chart.three_d {
                            for win in xy.windows(2) {
                                let quad = [
                                    win[0],
                                    win[1],
                                    (win[1].0 + d.0, win[1].1 + d.1),
                                    (win[0].0 + d.0, win[0].1 + d.1),
                                ];
                                cx.shape(
                                    polygon(&quad),
                                    Some(shade(&style.fill, lighter)),
                                    None,
                                    opacity,
                                    false,
                                );
                            }
                        }
                        cx.shape(
                            polyline(&xy, style.curved),
                            None,
                            Some((lw, style.color(), Vec::new())),
                            opacity,
                            style.shadow,
                        );
                        for (i, (c, v)) in pts.iter().enumerate() {
                            symbol(
                                cx,
                                style.symbol,
                                xy[i].0,
                                xy[i].1,
                                f32::from(style.symbol_size),
                                style.point_color.unwrap_or(style.color()),
                            );
                            cx.out.hits.push(Hit {
                                series: s,
                                category: Some(*c),
                                rect: (xy[i].0 - 4.0, xy[i].1 - 4.0, 8.0, 8.0),
                            });
                            error_bar(
                                cx,
                                &style,
                                &values[s],
                                *c,
                                r,
                                &pos,
                                if horizontal { xy[i].1 } else { xy[i].0 },
                                horizontal,
                            );
                            if style.labels.show {
                                labels.push((
                                    xy[i].0,
                                    xy[i].1 - 5.0,
                                    cx.number(&style.labels.number, *v),
                                    Align::Middle,
                                ));
                            }
                        }
                    }
                    // Trendlines over categories, numbered from 1.
                    if let Some(trend) = &style.trend
                        && chart.kind.has_trendlines()
                    {
                        let xs: Vec<f64> = pts.iter().map(|(c, _)| *c as f64 + 1.0).collect();
                        let ys: Vec<f64> = pts.iter().map(|(_, v)| *v).collect();
                        trendline(cx, trend, &style, &xs, &ys, &|x: f64, y: f64| {
                            let c = x - 1.0;
                            let p = if horizontal {
                                y0 + cat_w * (c as f32 + 0.5)
                            } else {
                                x0 + cat_w * (c as f32 + 0.5)
                            };
                            if horizontal {
                                (pos(r, y), p)
                            } else {
                                (p, pos(r, y))
                            }
                        });
                    }
                }
            }
            if pass == SeriesKind::Column
                && let Some(trend) = &style.trend
                && chart.kind.has_trendlines()
            {
                let pts: Vec<(usize, f64)> = (0..nc)
                    .filter_map(|c| values[s][c].map(|v| (c, v)))
                    .collect();
                let xs: Vec<f64> = pts.iter().map(|(c, _)| *c as f64 + 1.0).collect();
                let ys: Vec<f64> = pts.iter().map(|(_, v)| *v).collect();
                trendline(cx, trend, &style, &xs, &ys, &|x: f64, y: f64| {
                    let c = x - 1.0;
                    let p = if horizontal {
                        y0 + cat_w * (c as f32 + 0.5)
                    } else {
                        x0 + cat_w * (c as f32 + 0.5)
                    };
                    if horizontal {
                        (pos(r, y), p)
                    } else {
                        (p, pos(r, y))
                    }
                });
            }
        }
    }
    for (x, y, text, align) in labels {
        cx.text(x, y, text, size * 0.9, align, 0.0, false);
    }
}

#[allow(clippy::too_many_arguments)]
fn bar_label(
    position: LabelPosition,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    negative: bool,
    horizontal: bool,
    size: f32,
) -> (f32, f32, Align) {
    if horizontal {
        let cy = y + h / 2.0 + size * 0.35;
        let (start, end) = if negative { (x + w, x) } else { (x, x + w) };
        let dir = if negative { -1.0 } else { 1.0 };
        return match position {
            LabelPosition::Outside => (
                end + 3.0 * dir,
                cy,
                if negative { Align::End } else { Align::Start },
            ),
            LabelPosition::InsideEnd => (
                end - 3.0 * dir,
                cy,
                if negative { Align::Start } else { Align::End },
            ),
            LabelPosition::Middle => (x + w / 2.0, cy, Align::Middle),
            LabelPosition::InsideBase => (
                start + 3.0 * dir,
                cy,
                if negative { Align::End } else { Align::Start },
            ),
        };
    }
    let cx = x + w / 2.0;
    let (top, bottom) = if negative { (y + h, y) } else { (y, y + h) };
    match position {
        LabelPosition::Outside if negative => (cx, top + size, Align::Middle),
        LabelPosition::Outside => (cx, top - 3.0, Align::Middle),
        LabelPosition::InsideEnd if negative => (cx, top - 3.0, Align::Middle),
        LabelPosition::InsideEnd => (cx, top + size, Align::Middle),
        LabelPosition::Middle => (cx, y + h / 2.0 + size * 0.35, Align::Middle),
        LabelPosition::InsideBase if negative => (cx, bottom + size, Align::Middle),
        LabelPosition::InsideBase => (cx, bottom - 3.0, Align::Middle),
    }
}

#[allow(clippy::too_many_arguments)]
fn error_bar(
    cx: &mut Ctx<'_>,
    style: &SeriesStyle,
    row: &[Option<f64>],
    c: usize,
    r: &Range,
    pos: &dyn Fn(&Range, f64) -> f32,
    across: f32,
    horizontal: bool,
) {
    let Some(errors) = &style.errors else { return };
    if !cx.chart.kind.has_error_bars() {
        return;
    }
    let vals: Vec<f64> = row.iter().map(|v| v.unwrap_or(0.0)).collect();
    let lengths = stats::error_lengths(&errors.amount, &vals);
    let Some(v) = row[c] else { return };
    let (up, down) = lengths[c];
    let (hi, lo) = match errors.direction {
        ErrorDirection::Both => (v + up, v - down),
        ErrorDirection::Positive => (v + up, v),
        ErrorDirection::Negative => (v, v - down),
    };
    let (a, b) = (pos(r, lo), pos(r, hi));
    let color = Color::rgb(0x24, 0x1f, 0x31);
    let cap = 3.0;
    if horizontal {
        cx.line((a, across), (b, across), 0.75, color);
        cx.line((a, across - cap), (a, across + cap), 0.75, color);
        cx.line((b, across - cap), (b, across + cap), 0.75, color);
    } else {
        cx.line((across, a), (across, b), 0.75, color);
        cx.line((across - cap, a), (across + cap, a), 0.75, color);
        cx.line((across - cap, b), (across + cap, b), 0.75, color);
    }
}

fn trendline(
    cx: &mut Ctx<'_>,
    trend: &crate::model::Trendline,
    style: &SeriesStyle,
    xs: &[f64],
    ys: &[f64],
    place: &dyn Fn(f64, f64) -> (f32, f32),
) {
    let color = trend.color.unwrap_or_else(|| darker(style.color()));
    let points: Vec<(f32, f32)> = if let TrendKind::MovingAverage(p) = trend.kind {
        stats::moving_average(xs, ys, p)
            .iter()
            .map(|(x, y)| place(*x, *y))
            .collect()
    } else {
        let Some(fit) = stats::fit(trend.kind, xs, ys) else {
            return;
        };
        let (lo, hi) = (
            xs.iter().copied().fold(f64::MAX, f64::min),
            xs.iter().copied().fold(f64::MIN, f64::max),
        );
        let pts: Vec<(f32, f32)> = (0..=40)
            .map(|i| lo + (hi - lo) * f64::from(i) / 40.0)
            .map(|x| place(x, fit.at(x)))
            .filter(|p| p.0.is_finite() && p.1.is_finite())
            .collect();
        let mut caption = Vec::new();
        if trend.show_equation {
            caption.push(fit.equation(cx.decimal));
        }
        if trend.show_r_squared
            && let Some(r2) = fit.r_squared
        {
            caption.push(format!(
                "R² = {}",
                format!("{r2:.4}").replace('.', &cx.decimal.to_string())
            ));
        }
        if !trend.label.is_empty() {
            caption.push(trend.label.clone());
        }
        if let Some(last) = pts.last() {
            let mut y = last.1 - cx.size * 0.5;
            for line in caption {
                cx.text(last.0, y, line, cx.size * 0.85, Align::End, 0.0, false);
                y -= cx.size;
            }
        }
        pts
    };
    cx.shape(
        polyline(&points, false),
        None,
        Some((1.0, color, vec![4.0, 2.0])),
        1.0,
        false,
    );
}

/// A point of a scatter or a bubble chart: x, y and, for a bubble, the
/// size.
type Point = (f64, f64, Option<f64>);

/// The points of each series that a scatter or a bubble chart plots, by
/// [`Chart::point_series`]: a category gives a point when it has all the
/// values the point needs (GRF-18, W141, W142).
fn chart_points(chart: &Chart) -> Vec<Vec<Point>> {
    let nc = chart.category_count();
    chart
        .point_series()
        .iter()
        .map(|ps| {
            (0..nc)
                .filter_map(|c| {
                    let size = match ps.size {
                        Some(s) => Some(chart.value(s, c)?),
                        None => None,
                    };
                    Some((chart.value(ps.x, c)?, chart.value(ps.y, c)?, size))
                })
                .collect()
        })
        .collect()
}

/// The radius of each bubble, in points, by its size: by area or by
/// diameter, the largest at `largest` (W141). A bubble of a negative size
/// gets the radius of its magnitude, and none when negatives are hidden.
fn bubble_radii(chart: &Chart, points: &[Vec<Point>], largest: f32) -> Vec<Vec<Option<f32>>> {
    let top = points
        .iter()
        .flatten()
        .filter_map(|p| p.2)
        .map(f64::abs)
        .filter(|v| v.is_finite())
        .fold(0.0, f64::max);
    points
        .iter()
        .map(|pts| {
            pts.iter()
                .map(|p| {
                    let size = p.2?;
                    if !size.is_finite() || (size < 0.0 && !chart.bubble.negative) {
                        return None;
                    }
                    let part = if top > 0.0 { size.abs() / top } else { 0.0 };
                    let part = if chart.bubble.by_diameter {
                        part
                    } else {
                        part.sqrt()
                    };
                    Some((part as f32 * largest).max(1.0))
                })
                .collect()
        })
        .collect()
}

/// The path of a circle.
fn circle(x: f32, y: f32, r: f32) -> Vec<PathOp> {
    let k = 0.552_284_8 * r;
    vec![
        PathOp::Move(x + r, y),
        PathOp::Curve(x + r, y + k, x + k, y + r, x, y + r),
        PathOp::Curve(x - k, y + r, x - r, y + k, x - r, y),
        PathOp::Curve(x - r, y - k, x - k, y - r, x, y - r),
        PathOp::Curve(x + k, y - r, x + r, y - k, x + r, y),
        PathOp::Close,
    ]
}

/// A scatter chart, or a bubble chart, whose points are circles as big
/// as their sizes (GRF-18, W141).
fn scatter(cx: &mut Ctx<'_>, area: (f32, f32, f32, f32)) {
    let chart = cx.chart;
    let size = cx.size;
    let bubble = chart.kind == ChartKind::Bubble;
    let plotted = chart.point_series();
    let points = chart_points(chart);
    let all: Vec<&Point> = points.iter().flatten().collect();
    let (xlo, xhi) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| {
        (a.0.min(p.0), a.1.max(p.0))
    });
    let (ylo, yhi) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| {
        (a.0.min(p.1), a.1.max(p.1))
    });
    let fin = |a: f64, b: f64| if a.is_finite() { (a, b) } else { (0.0, 1.0) };
    let (xlo, xhi) = fin(xlo, xhi);
    let (ylo, yhi) = fin(ylo, yhi);
    let rx = value_range(&chart.x_axis, xlo, xhi, true);
    let ry = value_range(&chart.value_axis, ylo, yhi, true);
    let (mut x0, y0, mut w, mut h) = area;
    let lw = axis_label_width(cx, &chart.value_axis, &ry);
    x0 += lw;
    w -= lw;
    h -= size * 1.3;
    if let Some(t) = &chart.x_axis.title {
        h -= size * 1.3;
        cx.text(
            x0 + w / 2.0,
            y0 + h + size * 2.5,
            t.clone(),
            size,
            Align::Middle,
            0.0,
            false,
        );
    }
    if let Some(t) = &chart.value_axis.title {
        cx.text(
            area.0 + size,
            y0 + h / 2.0,
            t.clone(),
            size,
            Align::Middle,
            90.0,
            false,
        );
        x0 += size * 1.3;
        w -= size * 1.3;
    }
    if w <= 4.0 || h <= 4.0 {
        return;
    }
    let grid = Color::rgb(0xd0, 0xd0, 0xd0);
    let px = |v: f64| x0 + rx.fraction(v) * w;
    let py = |v: f64| y0 + h - ry.fraction(v) * h;
    for t in ry.ticks() {
        if chart.value_axis.gridlines {
            cx.line((x0, py(t)), (x0 + w, py(t)), 0.5, grid);
        }
        if chart.value_axis.labels {
            let text = axis_text(cx, &chart.value_axis, t);
            cx.text(
                x0 - 4.0,
                py(t) + size * 0.35,
                text,
                size,
                Align::End,
                0.0,
                false,
            );
        }
    }
    for t in rx.ticks() {
        if chart.x_axis.gridlines {
            cx.line((px(t), y0), (px(t), y0 + h), 0.5, grid);
        }
        if chart.x_axis.labels {
            let text = axis_text(cx, &chart.x_axis, t);
            cx.text(
                px(t),
                y0 + h + size * 1.1,
                text,
                size,
                Align::Middle,
                0.0,
                false,
            );
        }
    }
    let axis_color = Color::rgb(0x77, 0x76, 0x7b);
    cx.line((x0, y0 + h), (x0 + w, y0 + h), 0.75, axis_color);
    cx.line((x0, y0), (x0, y0 + h), 0.75, axis_color);
    // The largest bubble is a sixth of the shorter side at a scale of 100.
    let largest = w.min(h) / 6.0 * f32::from(chart.bubble.scale.clamp(10, 300)) / 100.0;
    let radii = bubble_radii(chart, &points, largest);
    for (p, pts) in points.iter().enumerate() {
        // A plotted series takes the look of its series of y values, as
        // the inspector and ODT do.
        let style = chart.style(plotted[p].y);
        let xy: Vec<(f32, f32)> = pts.iter().map(|(x, y, _)| (px(*x), py(*y))).collect();
        if style.connect && xy.len() > 1 && !bubble {
            let mut sorted = xy.clone();
            sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
            cx.shape(
                polyline(&sorted, style.curved),
                None,
                Some((style.line_width as f32 / 1000.0, style.color(), Vec::new())),
                1.0,
                false,
            );
        }
        let symbol_kind = if style.symbol == Symbol::None {
            Symbol::Circle
        } else {
            style.symbol
        };
        // The larger bubbles first, so that the smaller stay in sight.
        let mut order: Vec<usize> = (0..xy.len()).collect();
        if bubble {
            order.sort_by(|a, b| {
                let r = |i: usize| radii[p][i].unwrap_or(0.0);
                r(*b).total_cmp(&r(*a))
            });
        }
        for i in order {
            let (x, y) = xy[i];
            let color = style.point_color.unwrap_or(style.color());
            if bubble {
                let Some(r) = radii[p][i] else { continue };
                // A negative size shows as a ring.
                let negative = pts[i].2.is_some_and(|v| v < 0.0);
                cx.shape(
                    circle(x, y, r),
                    (!negative).then(|| paint(&style.fill)),
                    stroke_of(&style).or(Some((
                        if negative { 1.5 } else { 0.75 },
                        if negative { color } else { Color::WHITE },
                        Vec::new(),
                    ))),
                    f32::from(style.opacity) / 100.0,
                    style.shadow,
                );
                cx.out.hits.push(Hit {
                    series: p,
                    category: Some(i),
                    rect: (x - r, y - r, 2.0 * r, 2.0 * r),
                });
            } else {
                symbol(cx, symbol_kind, x, y, f32::from(style.symbol_size), color);
                cx.out.hits.push(Hit {
                    series: p,
                    category: Some(i),
                    rect: (x - 4.0, y - 4.0, 8.0, 8.0),
                });
            }
            if let Some(errors) = &style.errors
                && chart.kind.has_error_bars()
            {
                let along: Vec<f64> = pts
                    .iter()
                    .map(|q| if errors.along_x { q.0 } else { q.1 })
                    .collect();
                let (up, down) = stats::error_lengths(&errors.amount, &along)[i];
                let (lo, hi) = match errors.direction {
                    ErrorDirection::Both => (along[i] - down, along[i] + up),
                    ErrorDirection::Positive => (along[i], along[i] + up),
                    ErrorDirection::Negative => (along[i] - down, along[i]),
                };
                let color = Color::rgb(0x24, 0x1f, 0x31);
                if errors.along_x {
                    cx.line((px(lo), y), (px(hi), y), 0.75, color);
                } else {
                    cx.line((x, py(lo)), (x, py(hi)), 0.75, color);
                }
            }
            if style.labels.show {
                // A bubble tells its size, in its middle; a point its y.
                let (value, ly) = match pts[i].2 {
                    Some(v) => (v, y + size * 0.35),
                    None => (pts[i].1, y - 5.0),
                };
                let text = cx.number(&style.labels.number, value);
                cx.text(x, ly, text, size * 0.9, Align::Middle, 0.0, false);
            }
        }
        if let Some(trend) = &style.trend
            && chart.kind.has_trendlines()
        {
            let xs: Vec<f64> = pts.iter().map(|q| q.0).collect();
            let ys: Vec<f64> = pts.iter().map(|q| q.1).collect();
            let mut order: Vec<usize> = (0..xs.len()).collect();
            order.sort_by(|a, b| xs[*a].total_cmp(&xs[*b]));
            let xs: Vec<f64> = order.iter().map(|i| xs[*i]).collect();
            let ys: Vec<f64> = order.iter().map(|i| ys[*i]).collect();
            trendline(cx, trend, &style, &xs, &ys, &|x, y| (px(x), py(y)));
        }
    }
}

/// A radar chart: a spoke for each category from the center, the grid
/// at the steps of the value axis as polygons or circles, and each series
/// as a closed line through its values, filled lightly (W141).
#[allow(clippy::too_many_lines)]
fn radar(cx: &mut Ctx<'_>, area: (f32, f32, f32, f32)) {
    let chart = cx.chart;
    let size = cx.size;
    let ns = chart.series_count();
    let nc = chart.category_count();
    if nc == 0 {
        return;
    }
    let (lo, hi) = (0..ns)
        .flat_map(|s| (0..nc).filter_map(move |c| chart.value(s, c)))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |a, v| {
            (a.0.min(v), a.1.max(v))
        });
    let (lo, hi) = if lo.is_finite() { (lo, hi) } else { (0.0, 1.0) };
    let range = value_range(&chart.value_axis, lo, hi, true);
    let (x0, y0, w, h) = area;
    let label_w = (0..nc)
        .map(|c| cx.width(chart.category_name(c), size))
        .fold(0.0, f32::max);
    let r = (w / 2.0 - label_w - 6.0).min(h / 2.0 - size * 1.6);
    if r <= 4.0 {
        return;
    }
    let (ccx, ccy) = (x0 + w / 2.0, y0 + h / 2.0);
    // The first spoke points up, the others go clockwise.
    let angle = |c: usize| -PI / 2.0 + 2.0 * PI * c as f32 / nc as f32;
    let at = |c: usize, f: f32| {
        let a = angle(c);
        (ccx + r * f * a.cos(), ccy + r * f * a.sin())
    };
    let grid = Color::rgb(0xd0, 0xd0, 0xd0);
    let axis_color = Color::rgb(0x77, 0x76, 0x7b);
    let axis = &chart.value_axis;
    for (i, t) in range.ticks().iter().enumerate() {
        let f = range.fraction(*t).clamp(0.0, 1.0);
        if axis.gridlines && f > 0.0 {
            let path = if chart.radar_circular || nc < 3 {
                circle(ccx, ccy, r * f)
            } else {
                polygon(&(0..nc).map(|c| at(c, f)).collect::<Vec<_>>())
            };
            cx.shape(path, None, Some((0.5, grid, Vec::new())), 1.0, false);
        }
        if axis.labels && (i > 0 || axis.min_label) {
            let (lx, ly) = at(0, f);
            let text = axis_text(cx, axis, *t);
            cx.text(
                lx + 3.0,
                ly + size * 0.35,
                text,
                size * 0.9,
                Align::Start,
                0.0,
                false,
            );
        }
    }
    // The spokes and the names of the categories at their ends.
    for c in 0..nc {
        if chart.category_axis.line || chart.category_axis.gridlines {
            cx.line((ccx, ccy), at(c, 1.0), 0.5, axis_color);
        }
        if chart.category_axis.labels {
            let a = angle(c);
            let (lx, ly) = at(c, 1.0);
            let align = if a.cos() > 0.2 {
                Align::Start
            } else if a.cos() < -0.2 {
                Align::End
            } else {
                Align::Middle
            };
            cx.text(
                lx + 4.0 * a.cos(),
                ly + 4.0 * a.sin() + size * (0.35 + 0.5 * a.sin()),
                chart.category_name(c).to_owned(),
                size,
                align,
                0.0,
                false,
            );
        }
    }
    for s in 0..ns {
        let style = chart.style(s);
        let opacity = f32::from(style.opacity) / 100.0;
        // An empty value goes to the center, so that the line stays closed.
        let pts: Vec<(f32, f32)> = (0..nc)
            .map(|c| {
                let f = chart
                    .value(s, c)
                    .map_or(0.0, |v| range.fraction(v).clamp(0.0, 1.0));
                at(c, f)
            })
            .collect();
        cx.shape(
            polygon(&pts),
            Some(paint(&style.fill)),
            None,
            opacity * 0.35,
            false,
        );
        cx.shape(
            polygon(&pts),
            None,
            stroke_of(&style).or(Some((
                style.line_width as f32 / 1000.0,
                style.color(),
                Vec::new(),
            ))),
            opacity,
            style.shadow,
        );
        for (c, (x, y)) in pts.iter().enumerate() {
            let Some(v) = chart.value(s, c) else { continue };
            symbol(
                cx,
                style.symbol,
                *x,
                *y,
                f32::from(style.symbol_size),
                style.point_color.unwrap_or(style.color()),
            );
            cx.out.hits.push(Hit {
                series: s,
                category: Some(c),
                rect: (x - 4.0, y - 4.0, 8.0, 8.0),
            });
            if style.labels.show {
                let text = cx.number(&style.labels.number, v);
                cx.text(*x, *y - 5.0, text, size * 0.9, Align::Middle, 0.0, false);
            }
        }
    }
}

fn arc_points(cx0: f32, cy0: f32, rx: f32, ry: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
    let steps = ((a1 - a0).abs() / (PI / 36.0)).ceil().max(2.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / steps as f32;
            (cx0 + rx * a.cos(), cy0 + ry * a.sin())
        })
        .collect()
}

/// A pie, or a donut, whose slices have a hole of the inner radius in
/// the middle (GRF-15, W141, W145).
fn pie(cx: &mut Ctx<'_>, area: (f32, f32, f32, f32)) {
    let chart = cx.chart;
    let three_d = chart.three_d && chart.kind.has_3d();
    let hole = if chart.kind == ChartKind::Donut {
        f32::from(chart.inner_radius.min(95)) / 100.0
    } else {
        0.0
    };
    let size = cx.size;
    let ns = chart.series_count();
    // A pie draws the first value of each series (GRF-02).
    let values: Vec<f64> = (0..ns)
        .map(|s| chart.value(s, 0).unwrap_or(0.0).max(0.0))
        .collect();
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }
    let (x0, y0, w, h) = area;
    let max_explode = (0..ns).map(|s| chart.style(s).explode).max().unwrap_or(0);
    let tilt = if three_d {
        f32::from(chart.scene.pitch.clamp(5, 45))
            .to_radians()
            .sin()
            .max(0.3)
    } else {
        1.0
    };
    let thickness = if three_d {
        h * 0.12 * f32::from(chart.scene.depth.min(200)) / 100.0
    } else {
        0.0
    };
    let label_room = size * 1.5;
    let r = ((w / 2.0 - label_room).min((h - thickness) / 2.0 / tilt - label_room)
        / (1.0 + f32::from(max_explode) / 100.0))
        .max(4.0);
    let (ccx, ccy) = (x0 + w / 2.0, y0 + (h - thickness) / 2.0);
    let rotation = f32::from(chart.pie.rotation).to_radians();
    let mut start = -PI / 2.0 + rotation;
    let mut slices = Vec::new();
    for (s, v) in values.iter().enumerate() {
        let sweep = (*v / total) as f32 * 2.0 * PI;
        slices.push((s, start, start + sweep, *v));
        start += sweep;
    }
    let center = |s: usize, a0: f32, a1: f32| {
        let mid = (a0 + a1) / 2.0;
        let e = r * f32::from(chart.style(s).explode) / 100.0;
        (ccx + e * mid.cos(), ccy + e * mid.sin() * tilt)
    };
    if chart.pie.group_shadow && (0..ns).any(|s| chart.style(s).shadow) {
        let mut pts = arc_points(ccx + 3.0, ccy + 4.0, r, r * tilt, 0.0, 2.0 * PI);
        if hole > 0.0 {
            pts.extend(arc_points(
                ccx + 3.0,
                ccy + 4.0,
                r * hole,
                r * hole * tilt,
                2.0 * PI,
                0.0,
            ));
        }
        cx.shape(
            polygon(&pts),
            Some(Paint::Color(Color {
                a: 60,
                ..Color::BLACK
            })),
            None,
            1.0,
            false,
        );
    }
    if three_d {
        // The sides of the slices that face the viewer, from back to front.
        for &(s, a0, a1, _) in &slices {
            let style = chart.style(s);
            let (sx, sy) = center(s, a0, a1);
            let lo = a0.max(0.0f32.min(a1));
            let front: Vec<(f32, f32)> = arc_points(sx, sy, r, r * tilt, a0, a1)
                .into_iter()
                .filter(|p| p.1 >= sy - 0.01)
                .collect();
            let _ = lo;
            if front.len() >= 2 {
                let mut side = front.clone();
                side.extend(front.iter().rev().map(|p| (p.0, p.1 + thickness)));
                cx.shape(
                    polygon(&side),
                    Some(shade(&style.fill, darker)),
                    None,
                    f32::from(style.opacity) / 100.0,
                    false,
                );
            }
        }
    }
    for &(s, a0, a1, v) in &slices {
        let style = chart.style(s);
        let (sx, sy) = center(s, a0, a1);
        let mut pts = if hole > 0.0 {
            arc_points(sx, sy, r * hole, r * hole * tilt, a1, a0)
        } else {
            vec![(sx, sy)]
        };
        pts.extend(arc_points(sx, sy, r, r * tilt, a0, a1));
        let shadow = style.shadow && !chart.pie.group_shadow;
        cx.shape(
            polygon(&pts),
            Some(paint(&style.fill)),
            stroke_of(&style).or(Some((0.75, Color::WHITE, Vec::new()))),
            f32::from(style.opacity) / 100.0,
            shadow,
        );
        let (minx, maxx) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
        let (miny, maxy) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
        // A beveled 3D pie catches the light on the rim of its top (W145).
        if three_d && chart.scene.bevel {
            let rim = arc_points(sx, sy, r - 1.0, (r - 1.0) * tilt, a0, a1);
            cx.shape(
                polyline(&rim, false),
                None,
                Some((2.0, lighter(lighter(style.color())), Vec::new())),
                0.8,
                false,
            );
        }
        cx.out.hits.push(Hit {
            series: s,
            category: Some(0),
            rect: (minx, miny, maxx - minx, maxy - miny),
        });
        // Labels at their distance from the center (GRF-15).
        let mid = (a0 + a1) / 2.0;
        let dist = r * f32::from(chart.pie.label_distance) / 100.0;
        let (lx, ly) = (sx + dist * mid.cos(), sy + dist * mid.sin() * tilt);
        let mut lines = Vec::new();
        if chart.pie.series_names {
            lines.push(chart.series_name(s).to_owned());
        }
        if style.labels.show {
            if chart.pie.percentages {
                let pct = NumberStyle {
                    suffix: "%".into(),
                    decimals: style.labels.number.decimals.or(Some(0)),
                    ..style.labels.number.clone()
                };
                lines.push(cx.number(&pct, v / total * 100.0));
            } else {
                lines.push(cx.number(&style.labels.number, v));
            }
        }
        let n = lines.len() as f32;
        for (i, line) in lines.into_iter().enumerate() {
            let y = ly + size * 0.35 + (i as f32 - (n - 1.0) / 2.0) * size * 1.1;
            cx.text(lx, y, line, size, Align::Middle, 0.0, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChartText, ErrorAmount, ErrorBars, Trendline};
    use desk_table::Num;

    fn texts(d: &Drawing) -> Vec<String> {
        d.marks
            .iter()
            .filter_map(|m| match m {
                Mark::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn shapes(d: &Drawing) -> usize {
        d.marks
            .iter()
            .filter(|m| matches!(m, Mark::Shape { fill: Some(_), .. }))
            .count()
    }

    #[test]
    fn grf_01_every_kind_draws() {
        for kind in ChartKind::ALL {
            for three_d in [false, true] {
                let mut chart = Chart::new(kind);
                chart.three_d = three_d && kind.has_3d();
                let d = draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
                assert!(shapes(&d) > 0, "{kind:?} {three_d}");
                assert!(!d.hits.is_empty(), "{kind:?}");
            }
        }
    }

    #[test]
    fn grf_07_grf_10_title_legend_and_axis_labels() {
        let mut chart = Chart::new(ChartKind::Column);
        chart.title = Some("Vendas".into());
        chart.value_axis.title = Some("Milhares".into());
        let t = texts(&draw(&chart, 300.0, 200.0, &rough_width, ',', '.'));
        assert!(t.contains(&"Vendas".to_string()));
        assert!(t.contains(&"Região 1".to_string()));
        assert!(t.contains(&"2008".to_string()));
        assert!(t.contains(&"100".to_string()));
        assert!(t.contains(&"Milhares".to_string()));
        chart.legend = false;
        chart.category_axis.every = 2;
        let t = texts(&draw(&chart, 300.0, 200.0, &rough_width, ',', '.'));
        assert!(!t.contains(&"Região 1".to_string()));
        assert!(!t.contains(&"2008".to_string()));
        assert!(t.contains(&"2009".to_string()));
    }

    #[test]
    fn grf_08_the_text_of_the_chart_formats_every_title_and_label() {
        let mut chart = Chart::new(ChartKind::Column);
        chart.title = Some("Vendas".into());
        chart.value_axis.title = Some("Milhares".into());
        chart.text = ChartText {
            font: "Cantarell".into(),
            size: 14_000,
            color: Color::rgb(0x1c, 0x71, 0xd8),
            bold: false,
            italic: true,
        };
        let d = draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        let mut count = 0;
        for mark in &d.marks {
            if let Mark::Text {
                font,
                color,
                italic,
                ..
            } = mark
            {
                count += 1;
                assert_eq!(font, "Cantarell");
                assert_eq!(*color, Color::rgb(0x1c, 0x71, 0xd8));
                assert!(*italic);
            }
        }
        // The title, the axis title, the legend and the axis labels.
        assert!(count > 5, "{count} texts");
        // The size of the text sets the size of the labels; the title is
        // drawn larger than them.
        let sizes: Vec<f32> = d
            .marks
            .iter()
            .filter_map(|m| match m {
                Mark::Text { size, .. } => Some(*size),
                _ => None,
            })
            .collect();
        assert!(sizes.contains(&14.0), "{sizes:?}");
    }

    #[test]
    fn grf_10_axis_ranges_and_scales() {
        let auto = value_range(&ValueAxis::default(), 17.0, 96.0, true);
        assert_eq!((auto.min, auto.max, auto.step), (0.0, 100.0, 20.0));
        let fixed = ValueAxis {
            min: Some(Num(10.0)),
            max: Some(Num(50.0)),
            steps: Some(4),
            ..ValueAxis::default()
        };
        let r = value_range(&fixed, 0.0, 0.0, true);
        assert_eq!(r.ticks(), [10.0, 20.0, 30.0, 40.0, 50.0]);
        let log = ValueAxis {
            scale: Scale::Logarithmic,
            ..ValueAxis::default()
        };
        assert_eq!(
            value_range(&log, 3.0, 2000.0, true).ticks(),
            [1.0, 10.0, 100.0, 1000.0, 10000.0]
        );
        let mut chart = Chart::new(ChartKind::StackedColumn);
        chart.value_axis.scale = Scale::Percent;
        let v = series_values(&chart);
        assert!((v[0][0].unwrap() + v[1][0].unwrap() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn values_at_the_ends_of_doubles_draw_on_finite_axes() {
        // Review RV, batch 4: a logarithmic axis to 1.7e308 counted its
        // decades without end, and a value of 1e-323 made a step of 0 and
        // an axis of NaN, which clamp refused.
        for (kind, scale, value) in [
            (ChartKind::Column, Scale::Logarithmic, 1.7e308),
            (ChartKind::StackedColumn, Scale::Logarithmic, 1e308),
            (ChartKind::Column, Scale::Linear, 1e-323),
            (ChartKind::Line, Scale::Linear, f64::MAX),
            (ChartKind::StackedColumn, Scale::Linear, f64::MAX),
        ] {
            let mut chart = Chart::new(kind);
            chart.value_axis.scale = scale;
            for s in 0..chart.series_count() {
                for c in 0..chart.category_count() {
                    let (r, k) = if chart.series_in_rows { (s, c) } else { (c, s) };
                    chart.data.set(r, k, Some(value));
                }
            }
            draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        }
        let r = value_range(&ValueAxis::default(), 0.0, 1e-323, true);
        assert!(r.min.is_finite() && r.max > r.min && r.step > 0.0);
        let log = ValueAxis {
            scale: Scale::Logarithmic,
            max: Some(Num(f64::MAX)),
            ..ValueAxis::default()
        };
        assert!(value_range(&log, 1.0, f64::MAX, true).ticks().len() <= 700);
        // Review of RV batch 4: with a minimum and all the values at 1e16,
        // the axis went from 0 to 1, and the points to coordinates far
        // off the chart.
        let fixed = ValueAxis {
            min: Some(Num(1e16)),
            ..ValueAxis::default()
        };
        let r = value_range(&fixed, 1e16, 1e16, true);
        assert!(
            r.min <= 1e16 && 1e16 <= r.max && r.min < r.max,
            "{} {}",
            r.min,
            r.max
        );
        assert!((0.0..=1.0).contains(&r.fraction(1e16)));
        assert_eq!(r.fraction(f64::MAX), 1000.0);
        // Second review: the range fallen back on was as wide as the middle
        // of the values, not as they spread, and its ends could overflow.
        for (log, lo, hi) in [
            (false, 1e16, 1e18),
            (false, -1e18, 1e16),
            (false, -f64::MAX, f64::MAX),
            (false, f64::MAX, f64::MAX),
            (false, 5.0, 5.0),
            (true, 1e16, 1e18),
            (true, 1e-5, 1e300),
            (true, 3.0, 3.0),
        ] {
            let r = fallback_range(log, lo, hi);
            assert!(
                r.min.is_finite() && r.max.is_finite() && r.min < r.max && r.step > 0.0,
                "{lo} {hi}: {} {}",
                r.min,
                r.max
            );
            if hi.abs() < 1e300 && lo.abs() < 1e300 {
                assert!(r.min <= lo && hi <= r.max, "{lo} {hi}: {} {}", r.min, r.max);
            }
        }
    }

    #[test]
    fn a_chart_with_series_and_no_categories_draws_empty() {
        // Review RV, batch 4: the categories all deleted in the Data
        // Editor, or none in a file, made drawing read past the values.
        for kind in [
            ChartKind::Column,
            ChartKind::Bar,
            ChartKind::Line,
            ChartKind::Area,
            ChartKind::Scatter,
            ChartKind::Mixed,
            ChartKind::TwoAxis,
            ChartKind::Pie,
        ] {
            let mut chart = Chart::new(kind);
            chart.series_in_rows = true;
            while !chart.data.cols.is_empty() {
                chart.data.delete_col(0);
            }
            assert_eq!(chart.category_count(), 0);
            assert!(chart.series_count() > 0);
            draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
            chart.series_in_rows = false;
            draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        }
    }

    #[test]
    fn grf_12_grf_15_labels_of_values_and_slices() {
        let mut chart = Chart::new(ChartKind::Pie);
        chart.series[0].labels.show = true;
        chart.series[0].labels.number.decimals = Some(0);
        chart.pie.series_names = true;
        let t = texts(&draw(&chart, 300.0, 200.0, &rough_width, ',', '.'));
        // 17 of 72 is 24%.
        assert!(t.contains(&"24%".to_string()), "{t:?}");
        let mut col = Chart::new(ChartKind::Column);
        col.series[1].labels.show = true;
        let t = texts(&draw(&col, 300.0, 200.0, &rough_width, ',', '.'));
        assert!(t.contains(&"70".to_string()));
    }

    #[test]
    fn grf_13_grf_14_errors_and_trends_draw() {
        let mut chart = Chart::new(ChartKind::Line);
        chart.series[0].errors = Some(ErrorBars {
            direction: ErrorDirection::Both,
            amount: ErrorAmount::Percent(Num(10.0)),
            along_x: false,
        });
        chart.series[0].trend = Some(Trendline {
            kind: TrendKind::Linear,
            show_equation: true,
            show_r_squared: true,
            label: String::new(),
            color: None,
        });
        let d = draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        let t = texts(&d);
        assert!(t.iter().any(|s| s.starts_with("y = ")), "{t:?}");
        assert!(t.iter().any(|s| s.starts_with("R² = 0,")), "{t:?}");
    }

    #[test]
    fn grf_18_scatter_pairs_and_odd_series() {
        let mut chart = Chart::new(ChartKind::Scatter);
        chart.data.insert_row(2, "Sobra".into());
        let d = draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        // One pair of four points; the third series has no pair.
        assert_eq!(d.hits.iter().filter(|h| h.category.is_some()).count(), 4);
    }

    /// The points of the path of a filled shape.
    fn points(path: &[PathOp]) -> Vec<(f32, f32)> {
        path.iter()
            .filter_map(|op| match *op {
                PathOp::Move(x, y) | PathOp::Line(x, y) | PathOp::Curve(_, _, _, _, x, y) => {
                    Some((x, y))
                }
                PathOp::Close => None,
            })
            .collect()
    }

    /// The paths of the shapes filled with a color, in order.
    fn filled_with(d: &Drawing, color: Color) -> Vec<Vec<PathOp>> {
        d.marks
            .iter()
            .filter_map(|m| match m {
                Mark::Shape {
                    path,
                    fill: Some(Paint::Color(c)),
                    ..
                } if *c == color => Some(path.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn w141_w145_a_donut_has_a_hole_of_its_inner_radius() {
        let mut chart = Chart::new(ChartKind::Donut);
        chart.legend = false;
        let first = chart.style(0).color();
        // The distances from the center of the points of the first slice,
        // the center being that of the box of all the slices.
        let reach = |chart: &Chart| -> (f32, f32) {
            let d = draw(chart, 300.0, 300.0, &rough_width, ',', '.');
            let (x0, y0, x1, y1) =
                d.hits
                    .iter()
                    .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |a, h| {
                        let (x, y, w, h) = h.rect;
                        (a.0.min(x), a.1.min(y), a.2.max(x + w), a.3.max(y + h))
                    });
            let center = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let distances: Vec<f32> = points(&filled_with(&d, first)[0])
                .iter()
                .map(|p| ((p.0 - center.0).powi(2) + (p.1 - center.1).powi(2)).sqrt())
                .collect();
            (
                distances.iter().copied().fold(f32::MAX, f32::min),
                distances.iter().copied().fold(0.0, f32::max),
            )
        };
        let near = |chart: &Chart| reach(chart).0;
        let (ring, outer) = reach(&chart);
        // Half the radius by default, and as much as the inspector says.
        assert!((ring / outer - 0.5).abs() < 0.05, "{ring} {outer}");
        chart.inner_radius = 80;
        assert!((near(&chart) / outer - 0.8).abs() < 0.05);
        // A pie has its center in each slice.
        chart.kind = ChartKind::Pie;
        assert!(near(&chart) < 1.0);
    }

    #[test]
    fn w141_a_radar_has_a_spoke_for_each_category() {
        let mut chart = Chart::new(ChartKind::Radar);
        chart.legend = false;
        let d = draw(&chart, 300.0, 300.0, &rough_width, ',', '.');
        // A point for each value of each series, and the names around.
        assert_eq!(d.hits.iter().filter(|h| h.category.is_some()).count(), 8);
        let t = texts(&d);
        for name in ["2007", "2008", "2009", "2010"] {
            assert!(t.contains(&name.to_string()), "{name}");
        }
        // The first series goes through four points, filled lightly.
        let area = filled_with(&d, chart.style(0).color());
        assert_eq!(points(&area[0]).len(), 4);
        let curves = |d: &Drawing| {
            d.marks
                .iter()
                .filter(|m| {
                    matches!(m, Mark::Shape { path, fill: None, .. }
                        if path.iter().any(|op| matches!(op, PathOp::Curve(..))))
                })
                .count()
        };
        assert_eq!(curves(&d), 0);
        chart.radar_circular = true;
        let round = draw(&chart, 300.0, 300.0, &rough_width, ',', '.');
        assert!(curves(&round) > 0);
    }

    #[test]
    fn w141_bubbles_are_as_big_as_their_sizes() {
        let mut chart = Chart::new(ChartKind::Bubble);
        // Sizes 30, 12, 45 and 20; by area the radius goes as the root.
        let radius = |chart: &Chart, c: usize| {
            let d = draw(chart, 400.0, 300.0, &rough_width, ',', '.');
            hit(&d, 0, c).2 / 2.0
        };
        let (small, large) = (radius(&chart, 1), radius(&chart, 2));
        assert!(((large / small) - (45.0f32 / 12.0).sqrt()).abs() < 0.05);
        chart.bubble.by_diameter = true;
        let (small, large) = (radius(&chart, 1), radius(&chart, 2));
        assert!(((large / small) - 45.0 / 12.0).abs() < 0.05);
        chart.bubble.scale = 50;
        assert!((radius(&chart, 2) - large / 2.0).abs() < 0.05);
        // A negative bubble shows only when asked to, as a ring.
        chart.data.set(2, 0, Some(-30.0));
        let d = draw(&chart, 400.0, 300.0, &rough_width, ',', '.');
        assert!(!d.hits.iter().any(|h| h.category == Some(0)));
        chart.bubble.negative = true;
        let d = draw(&chart, 400.0, 300.0, &rough_width, ',', '.');
        assert!(d.hits.iter().any(|h| h.category == Some(0)));
        // The legend names the series of the y values.
        assert!(texts(&d).contains(&"Região 2".to_string()));
    }

    #[test]
    fn w142_series_share_the_x_values_of_the_first() {
        let mut chart = Chart::new(ChartKind::Scatter);
        chart.data.insert_row(2, "Região 3".into());
        for c in 0..4 {
            chart.data.set(2, c, Some(10.0 * c as f64));
        }
        chart.share_x = true;
        let d = draw(&chart, 400.0, 300.0, &rough_width, ',', '.');
        // Two series of four points, each point of a category on the
        // same x.
        assert_eq!(d.hits.iter().filter(|h| h.category.is_some()).count(), 8);
        for c in 0..4 {
            let (a, b) = (hit(&d, 0, c), hit(&d, 1, c));
            assert!((a.0 - b.0).abs() < 0.01);
        }
        let t = texts(&d);
        assert!(t.contains(&"Região 2".to_string()));
        assert!(t.contains(&"Região 3".to_string()));
    }

    #[test]
    fn w145_bars_get_rounded_ends() {
        let curved = |path: &[PathOp]| path.iter().any(|op| matches!(op, PathOp::Curve(..)));
        let mut chart = Chart::new(ChartKind::StackedColumn);
        chart.legend = false;
        let (first, second) = (chart.style(0).color(), chart.style(1).color());
        let d = draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        assert!(!curved(&filled_with(&d, first)[0]));
        chart.corner_radius = 30;
        let d = draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        assert!(filled_with(&d, first).iter().all(|p| curved(p)));
        assert!(filled_with(&d, second).iter().all(|p| curved(p)));
        // Outside Corners Only rounds the top of the stack alone.
        chart.outside_corners_only = true;
        let d = draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        assert!(filled_with(&d, first).iter().all(|p| !curved(p)));
        let top = &filled_with(&d, second)[0];
        assert!(curved(top));
        // Its bottom corners stay square: the lowest points are corners.
        let pts = points(top);
        let bottom = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        assert_eq!(
            pts.iter().filter(|p| (p.1 - bottom).abs() < 0.01).count(),
            2
        );
    }

    #[test]
    fn w145_3d_bars_and_pies_are_beveled() {
        for kind in [ChartKind::Column, ChartKind::Pie] {
            let mut chart = Chart::new(kind);
            chart.three_d = true;
            let plain = draw(&chart, 300.0, 200.0, &rough_width, ',', '.')
                .marks
                .len();
            chart.scene.bevel = true;
            let beveled = draw(&chart, 300.0, 200.0, &rough_width, ',', '.')
                .marks
                .len();
            assert!(beveled > plain, "{kind:?}");
        }
    }

    #[test]
    fn grf_04_changing_the_kind_resets_series_marks() {
        let mut chart = Chart::new(ChartKind::Column);
        chart.series[0].labels.show = true;
        chart.value_axis.gridlines = false;
        chart.series[0].trend = Some(Trendline {
            kind: TrendKind::Linear,
            show_equation: false,
            show_r_squared: false,
            label: String::new(),
            color: None,
        });
        chart.set_kind(ChartKind::StackedColumn);
        assert!(!chart.series[0].labels.show);
        assert!(!chart.value_axis.gridlines);
        assert!(chart.series[0].trend.is_none());
    }

    fn hit(d: &Drawing, series: usize, category: usize) -> (f32, f32, f32, f32) {
        d.hits
            .iter()
            .find(|h| h.series == series && h.category == Some(category))
            .map(|h| h.rect)
            .expect("a hit for the bar")
    }

    #[test]
    fn grf_02_series_in_rows_or_in_columns() {
        let mut chart = Chart::new(ChartKind::Column);
        assert_eq!((chart.series_count(), chart.category_count()), (2, 4));
        chart.series_in_rows = false;
        assert_eq!((chart.series_count(), chart.category_count()), (4, 2));
        let d = draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        let mut series: Vec<usize> = d.hits.iter().map(|h| h.series).collect();
        series.dedup();
        series.sort_unstable();
        series.dedup();
        assert_eq!(series, [0, 1, 2, 3]);
        assert!(texts(&d).contains(&"2010".to_string()));
    }

    #[test]
    fn grf_05_edited_data_redraws() {
        let mut chart = Chart::new(ChartKind::Column);
        let before = hit(&draw(&chart, 300.0, 200.0, &rough_width, ',', '.'), 0, 0);
        chart.data.set(0, 0, Some(90.0));
        let after = hit(&draw(&chart, 300.0, 200.0, &rough_width, ',', '.'), 0, 0);
        assert!(after.3 > before.3 * 4.0, "{before:?} {after:?}");
    }

    #[test]
    fn grf_09_a_larger_frame_draws_larger() {
        let chart = Chart::new(ChartKind::Column);
        let small = hit(&draw(&chart, 300.0, 200.0, &rough_width, ',', '.'), 1, 3);
        let large = hit(&draw(&chart, 600.0, 400.0, &rough_width, ',', '.'), 1, 3);
        assert!(large.2 > small.2 * 1.8 && large.3 > small.3 * 1.8);
    }

    #[test]
    fn grf_11_series_fill() {
        let mut chart = Chart::new(ChartKind::Column);
        let red = Color::rgb(200, 0, 0);
        chart.series[0].fill = SeriesFill::Color(red);
        let d = draw(&chart, 300.0, 200.0, &rough_width, ',', '.');
        assert!(d.marks.iter().any(|m| matches!(
            m,
            Mark::Shape { fill: Some(Paint::Color(c)), .. } if *c == red
        )));
    }

    #[test]
    fn grf_16_columns_stand_and_bars_lie() {
        let column = hit(
            &draw(
                &Chart::new(ChartKind::Column),
                300.0,
                200.0,
                &rough_width,
                ',',
                '.',
            ),
            0,
            3,
        );
        assert!(column.3 > column.2, "{column:?}");
        let bar = hit(
            &draw(
                &Chart::new(ChartKind::Bar),
                300.0,
                200.0,
                &rough_width,
                ',',
                '.',
            ),
            0,
            3,
        );
        assert!(bar.2 > bar.3, "{bar:?}");
    }

    fn lines_and_fills(d: &Drawing) -> (usize, usize) {
        let lines = d
            .marks
            .iter()
            .filter(|m| {
                matches!(
                    m,
                    Mark::Shape {
                        fill: None,
                        stroke: Some(_),
                        ..
                    }
                )
            })
            .count();
        (lines, shapes(d))
    }

    #[test]
    fn grf_17_lines_stroke_and_areas_fill() {
        let line = lines_and_fills(&draw(
            &Chart::new(ChartKind::Line),
            300.0,
            200.0,
            &rough_width,
            ',',
            '.',
        ));
        assert!(line.0 >= 2, "{line:?}");
        let area = draw(
            &Chart::new(ChartKind::Area),
            300.0,
            200.0,
            &rough_width,
            ',',
            '.',
        );
        assert!(shapes(&area) >= 2);
    }

    #[test]
    fn grf_19_mixed_draws_columns_and_a_line() {
        let mut chart = Chart::new(ChartKind::Mixed);
        chart.series[0].kind = SeriesKind::Column;
        chart.series[1].kind = SeriesKind::Line;
        let (lines, fills) = lines_and_fills(&draw(&chart, 300.0, 200.0, &rough_width, ',', '.'));
        assert!(lines >= 1 && fills >= 4, "{lines} {fills}");
    }
}
