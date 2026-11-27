// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Objects on the pages: shapes, lines, text boxes, pictures and groups
//! (OBJ-01, OBJ-08, OBJ-13, OBJ-17).
//!
//! An object is a frame on a page with something drawn in it. A floating
//! object stays where it is put, on its page or beside the paragraph it
//! moves with, and the text of the body goes around it as its wrap says.
//! The objects of a document are kept from back to front, in the order
//! they are drawn.

use std::sync::Arc;

use crate::format::BorderStyle;
use crate::id::NodeId;
use crate::picture::Picture;
use crate::text::Paragraph;
use crate::units::{Length, Rgba};

/// The parts of a ten thousand that the points of a polygon are measured
/// in, across the width and down the height of its frame.
pub const POLYGON_SCALE: i32 = 10_000;

/// An object on the pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    /// The identifier.
    pub id: NodeId,
    /// What the object is.
    pub kind: ObjectKind,
    /// Where the object is, before it is rotated: by its top left corner,
    /// as its anchor says.
    pub frame: Frame,
    /// What the object moves with.
    pub anchor: Anchor,
    /// How far it is rotated around its center, counterclockwise, in
    /// hundredths of a degree, as the Metrics inspector shows it.
    pub rotation: i32,
    /// Whether it is flipped left to right.
    pub flip_horizontal: bool,
    /// Whether it is flipped upside down.
    pub flip_vertical: bool,
    /// How it is filled and outlined.
    pub style: GraphicStyle,
    /// The object style it took, which redefining the style changes it
    /// with while it still looks as the style did (MOD-04); none when it
    /// took none.
    pub object_style: Option<crate::StyleId>,
    /// How the text of the body goes around it (OBJ-17).
    pub wrap: Wrap,
    /// The room left between the object and the text around it.
    pub wrap_margin: Length,
    /// How opaque a pixel of a picture must be, in percent, for the text
    /// that wraps tightly to go around it: the transparency threshold of
    /// the Wrap Inspector (OBJ-17).
    pub wrap_threshold: u8,
    /// The outline that the text goes around when it wraps tightly, in
    /// parts of [`POLYGON_SCALE`] of the frame from its top left corner:
    /// that of a shape, or the edge of the opaque part of a picture
    /// (OBJ-17). None when the text goes around the box of the object.
    pub contour: Option<Arc<[(i32, i32)]>>,
    /// The other parts of the outline, for a picture whose opaque pixels
    /// are in parts apart, in the same units: the text runs between them
    /// (OBJ-17, B-047). Empty without a contour.
    pub contour_parts: ContourParts,
    /// Whether it cannot be moved, resized or deleted until unlocked.
    pub locked: bool,
    /// What the copies of one object share with it, so that Magic Move
    /// of Desk Slides takes them for the same object on two slides; none
    /// for an object never copied (IOP-33).
    pub origin: Option<u64>,
    /// The text inside a shape, as Pages lets one type in any shape but
    /// a line; none until some is typed. Its `next` is always none, as
    /// shapes are not linked.
    pub text: Option<TextBox>,
    /// How a picture is masked and adjusted; the defaults for any other
    /// object (OBJ-04, OBJ-06, OBJ-07).
    pub picture: PictureSettings,
    /// The comments on the object itself, as Pages 15.4 puts one on a
    /// selected object, by identifier (W149).
    pub comments: crate::CommentIds,
    /// Where a floating object was before a tracked change moved or
    /// resized it, which rejecting the change puts it back to (W148,
    /// REV-01); none when it is in no such change.
    pub moved: Option<Moved>,
    /// Where a click on the object goes, as a link of the text does:
    /// Format > Add Link on a shape, a picture or a text box of Pages
    /// 15.4 (W102).
    pub link: Option<Arc<str>>,
    /// That it is a placeholder of a page template, for text or media
    /// (W095).
    pub placeholder: Option<crate::Placeholder>,
    /// The object of a page template that it is a copy of, which Reapply
    /// Page Template puts back (W095).
    pub template_object: Option<NodeId>,
    /// Its title and caption, which show above and below it (W122); none
    /// when it never had one.
    pub captions: Option<Arc<crate::Captions>>,
}

impl Object {
    /// Takes the object out of its tracked changes, its place and its
    /// cells, which it leaves as they are: a copy, or an object grouped
    /// or ungrouped, is in none (W148, REV-05).
    pub fn untrack(&mut self) {
        self.moved = None;
        if let ObjectKind::Table(table) = &mut self.kind
            && !table.tracked_cells().is_empty()
        {
            let table = std::sync::Arc::make_mut(table);
            for ((r, c), _) in table.tracked_cells() {
                table.cell_mut(r, c).tracked = None;
            }
        }
    }

    /// The tracked changes that the object is in: the one that moved it,
    /// and those of the cells of a table (W148, REV-01, REV-05).
    pub fn changes(&self) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self.moved.map(|m| m.change).into_iter().collect();
        if let ObjectKind::Table(table) = &self.kind {
            let mut cells: Vec<NodeId> = table
                .tracked_cells()
                .into_iter()
                .filter_map(|(_, id)| NodeId::new(id.get()))
                .collect();
            cells.sort_unstable();
            cells.dedup();
            cells.retain(|id| !out.contains(id));
            out.extend(cells);
        }
        out
    }
}

/// Where a floating object was before a tracked change moved or resized
/// it (W148, REV-01), as Pages '09 tracked it (D-003).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Moved {
    /// The change, of the kind [`crate::ChangeKind::Placement`].
    pub change: NodeId,
    /// What it moved with.
    pub anchor: Anchor,
    /// Where it was, as its anchor said.
    pub frame: Frame,
}

/// How a picture shows in its frame: masked, with colors taken out, and
/// adjusted, as Pages does it to an image.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PictureSettings {
    /// Where the whole picture lies, from the top left corner of the
    /// frame, when the frame masks it: only the part inside the frame
    /// shows (OBJ-04). None when the picture fills its frame.
    pub mask: Option<Frame>,
    /// The shape of the mask, which shows only the part of the picture
    /// inside it, scaled to the frame: Mask with Shape (OBJ-04). None for
    /// a rectangle.
    pub mask_shape: Option<Shape>,
    /// Where Instant Alpha was clicked, in order: each click takes out
    /// the color under it and the colors near it around it (OBJ-06).
    pub alpha: Arc<[AlphaSeed]>,
    /// The part of the picture that Instant Alpha keeps, as Pages saves
    /// it: closed polygons in parts of [`POLYGON_SCALE`] of the picture
    /// from its top left corner, inside by the even-odd rule. None when
    /// only `alpha` says what is taken out (OBJ-06).
    pub alpha_kept: Option<AlphaOutline>,
    /// The changes of Adjust Image (OBJ-07).
    pub adjust: Adjustments,
}

/// The parts of the outline that text wraps tightly around, apart from the
/// first, each a closed polygon (B-047).
pub type ContourParts = Arc<[Arc<[(i32, i32)]>]>;

/// Closed polygons whose inside, by the even-odd rule, is the part of a
/// picture that Instant Alpha keeps.
pub type AlphaOutline = Arc<[Arc<[(i32, i32)]>]>;

/// A click of Instant Alpha on a picture: the region of the color under
/// it, as far as the colors stay near it, is taken out (OBJ-06).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AlphaSeed {
    /// Where across the picture, in ten-thousandths of its width.
    pub x: u16,
    /// Where down the picture, in ten-thousandths of its height.
    pub y: u16,
    /// How far a color may be from the one clicked and still go, from 0
    /// to 100, as dragging after the click sets it.
    pub tolerance: u8,
}

impl AlphaSeed {
    /// The most that `x` and `y` count: the far edge of the picture.
    pub const SCALE: u16 = 10_000;
    /// The tolerance of a click before any drag.
    pub const TOLERANCE: u8 = 20;
}

/// The changes of the Adjust Image window, each from -100 to 100 with 0
/// for none, and the levels, the darkest and lightest values kept
/// (OBJ-07).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Adjustments {
    /// Lighter or darker.
    pub brightness: i8,
    /// More or less different between light and dark.
    pub contrast: i8,
    /// More or less colorful.
    pub saturation: i8,
    /// As if exposed longer or shorter.
    pub exposure: i8,
    /// Warmer or cooler.
    pub temperature: i8,
    /// More magenta or more green.
    pub tint: i8,
    /// Sharper edges, or softer below zero (W110).
    pub sharpness: i8,
    /// The value, from 0 to 255, that becomes black: the left handle of
    /// the levels (W110).
    pub black: u8,
    /// The value that becomes white: the right handle of the levels.
    pub white: u8,
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            brightness: 0,
            contrast: 0,
            saturation: 0,
            exposure: 0,
            temperature: 0,
            tint: 0,
            sharpness: 0,
            black: 0,
            white: 255,
        }
    }
}

impl Adjustments {
    /// Whether nothing is changed.
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the colors of each pixel change, apart from the sharpness,
    /// which takes its neighbors.
    pub fn changes_colors(&self) -> bool {
        Self {
            sharpness: 0,
            ..*self
        } != Self::default()
    }

    /// The color of a pixel, adjusted, in straight (not premultiplied)
    /// red, green and blue from 0 to 1. The sharpness is not applied: it
    /// takes the pixels around.
    pub fn apply(&self, [r, g, b]: [f32; 3]) -> [f32; 3] {
        let unit = |v: i8| f32::from(v) / 100.0;
        // The levels first, as the histogram of the window shows the
        // picture as it is.
        // A black of 255 would leave no room for white.
        let black = self.black.min(254);
        let low = f32::from(black) / 255.0;
        let high = f32::from(self.white.max(black + 1)) / 255.0;
        let level = |v: f32| ((v - low) / (high - low)).clamp(0.0, 1.0);
        let (r, g, b) = (level(r), level(g), level(b));
        let exposure = 2f32.powf(unit(self.exposure) * 2.0);
        let (warm, tint) = (unit(self.temperature) * 0.15, unit(self.tint) * 0.15);
        let mut rgb = [
            r * exposure + warm + tint * 0.5,
            g * exposure - tint,
            b * exposure - warm + tint * 0.5,
        ];
        let brightness = unit(self.brightness) * 0.5;
        let contrast = 1.0 + unit(self.contrast);
        for v in &mut rgb {
            *v = (*v - 0.5) * contrast + 0.5 + brightness;
        }
        let gray = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        let saturation = 1.0 + unit(self.saturation);
        rgb.map(|v| (gray + (v - gray) * saturation).clamp(0.0, 1.0))
    }

    /// Enhance (W110): the exposure and the saturation that bring a
    /// picture of this histogram to a middle lightness and a natural
    /// color, the others kept, as the button of the Image tab of Pages
    /// 15.4 sets them.
    pub fn enhanced(&self, histogram: &Histogram) -> Self {
        let mut out = *self;
        let Some(mean) = histogram.mean_luma() else {
            return out;
        };
        // Half of the way to the middle, so that a dark scene stays dark
        // but shows; none for one close to it.
        let target = 0.46;
        out.exposure = if (mean - target).abs() < 0.04 {
            0
        } else {
            let stops = (target / mean.max(0.02)).log2() * 0.5;
            (stops / 2.0 * 100.0).round().clamp(-60.0, 60.0) as i8
        };
        let colorful = histogram.mean_chroma().unwrap_or(0.0);
        out.saturation = if colorful < 0.02 {
            // Gray stays gray.
            0
        } else {
            // Only more: a colorful picture is left as it is.
            ((0.18 - colorful) / 0.18 * 40.0).round().clamp(0.0, 30.0) as i8
        };
        out
    }

    /// Auto Levels of Adjust Image (W110): the darkest and lightest
    /// values of the picture, leaving out the few outermost, become black
    /// and white.
    pub fn auto_levels(&self, histogram: &Histogram) -> Self {
        let mut out = *self;
        if let (Some(black), Some(white)) = (histogram.luma_at(0.005), histogram.luma_at(0.995))
            && white > black
        {
            out.black = black;
            out.white = white;
        }
        out
    }
}

/// How many pixels of a picture have each value, from 0 to 255, of their
/// lightness and of each color, with how far their colors are from gray,
/// for the levels and for Enhance (W110).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Histogram {
    /// By lightness.
    pub luma: [u32; 256],
    /// By red, green and blue.
    pub rgb: [[u32; 256]; 3],
    /// The sum of the chroma of the pixels, from 0 to 255 each.
    pub chroma: u64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            luma: [0; 256],
            rgb: [[0; 256]; 3],
            chroma: 0,
        }
    }
}

impl Histogram {
    /// Counts a pixel of straight red, green and blue from 0 to 255.
    pub fn add(&mut self, [r, g, b]: [u8; 3]) {
        let luma = (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b))
            .round()
            .clamp(0.0, 255.0) as usize;
        if let Some(n) = self.luma.get_mut(luma) {
            *n += 1;
        }
        for (counts, v) in self.rgb.iter_mut().zip([r, g, b]) {
            if let Some(n) = counts.get_mut(usize::from(v)) {
                *n += 1;
            }
        }
        self.chroma += u64::from(r.max(g).max(b) - r.min(g).min(b));
    }

    /// How many pixels were counted.
    pub fn count(&self) -> u64 {
        self.luma.iter().map(|&n| u64::from(n)).sum()
    }

    /// The mean lightness, from 0 to 1.
    pub fn mean_luma(&self) -> Option<f32> {
        let count = self.count();
        (count > 0).then(|| {
            let sum: u64 = self
                .luma
                .iter()
                .enumerate()
                .map(|(v, &n)| v as u64 * u64::from(n))
                .sum();
            sum as f32 / count as f32 / 255.0
        })
    }

    /// The mean chroma, from 0 to 1.
    pub fn mean_chroma(&self) -> Option<f32> {
        let count = self.count();
        (count > 0).then(|| self.chroma as f32 / count as f32 / 255.0)
    }

    /// The lightness below which `share` of the pixels are.
    pub fn luma_at(&self, share: f32) -> Option<u8> {
        let count = self.count();
        if count == 0 {
            return None;
        }
        let wanted = (count as f64 * f64::from(share.clamp(0.0, 1.0)))
            .ceil()
            .max(1.0) as u64;
        let mut seen = 0u64;
        for (v, &n) in self.luma.iter().enumerate() {
            seen += u64::from(n);
            if seen >= wanted {
                return u8::try_from(v).ok();
            }
        }
        Some(255)
    }
}

impl Object {
    /// The start and the end of a line in a frame `width` by `height`,
    /// from its top left corner as if it were not turned, with its flips
    /// applied, as it is drawn (OBJ-29). `None` when it is no line.
    pub fn line_ends_in(&self, width: f64, height: f64) -> Option<((f64, f64), (f64, f64))> {
        let ObjectKind::Shape(Shape::Line { rising }) = self.kind else {
            return None;
        };
        let (mut start, mut end) = if rising {
            ((0.0, height), (width, 0.0))
        } else {
            ((0.0, 0.0), (width, height))
        };
        for p in [&mut start, &mut end] {
            if self.flip_horizontal {
                p.0 = width - p.0;
            }
            if self.flip_vertical {
                p.1 = height - p.1;
            }
        }
        Some((start, end))
    }

    /// Makes the object a line that goes `across` and `down` from its
    /// start to its end, in the axes of its frame: the start stays the
    /// start and keeps its arrow, the way Pages keeps the ends of a line
    /// apart, by flipping it when it goes to the left (OBJ-29).
    pub fn set_line_direction(&mut self, across: f64, down: f64) {
        self.kind = ObjectKind::Shape(Shape::Line { rising: down < 0.0 });
        self.flip_horizontal = across < 0.0;
        self.flip_vertical = false;
    }

    /// The transparency threshold of a new object, as in Pages: half.
    pub const WRAP_THRESHOLD: u8 = 50;

    /// An object with a frame on the first page, no fill and no line,
    /// that the text goes around on both sides.
    pub fn new(id: NodeId, kind: ObjectKind, frame: Frame) -> Self {
        Self {
            id,
            kind,
            frame,
            anchor: Anchor::Page(0),
            rotation: 0,
            flip_horizontal: false,
            flip_vertical: false,
            style: GraphicStyle::default(),
            object_style: None,
            wrap: Wrap::Both,
            wrap_margin: Length::from_points(12),
            wrap_threshold: Self::WRAP_THRESHOLD,
            contour: None,
            contour_parts: Arc::from([]),
            locked: false,
            origin: None,
            text: None,
            picture: PictureSettings::default(),
            comments: crate::CommentIds::none(),
            moved: None,
            link: None,
            placeholder: None,
            template_object: None,
            captions: None,
        }
    }

    /// The outline of a shape as a contour for the text to wrap around
    /// tightly, or none for an object that is not a shape with an inside.
    pub fn shape_contour(&self) -> Option<Arc<[(i32, i32)]>> {
        let ObjectKind::Shape(shape) = &self.kind else {
            return None;
        };
        if shape.is_line() {
            return None;
        }
        let (w, h) = (self.frame.width.to_pt(), self.frame.height.to_pt());
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let scale = f64::from(POLYGON_SCALE);
        let mut points: Vec<(i32, i32)> = Vec::new();
        for (a, _) in crate::flattened(&shape.outline(w, h)) {
            let point = (
                (a.0 / w * scale).round() as i32,
                (a.1 / h * scale).round() as i32,
            );
            if points.last() != Some(&point) {
                points.push(point);
            }
        }
        // Closed by itself.
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        (points.len() >= 3).then(|| points.into())
    }

    /// Whether text can be typed in it: a text box, or a shape but a
    /// line.
    pub fn holds_text(&self) -> bool {
        match &self.kind {
            ObjectKind::TextBox(_) => true,
            ObjectKind::Shape(shape) => !shape.is_line(),
            _ => false,
        }
    }

    /// Its text: that of a text box, or the text typed in a shape.
    pub fn text_box(&self) -> Option<&TextBox> {
        match &self.kind {
            ObjectKind::TextBox(text) => Some(text),
            _ if self.holds_text() => self.text.as_ref(),
            _ => None,
        }
    }

    /// Its text, to change it.
    pub fn text_box_mut(&mut self) -> Option<&mut TextBox> {
        let holds = self.holds_text();
        match &mut self.kind {
            ObjectKind::TextBox(text) => Some(text),
            _ if holds => self.text.as_mut(),
            _ => None,
        }
    }

    /// The paragraphs of its text, if it is a text box or a shape with
    /// text, and those of the objects of a group.
    pub fn paragraphs(&self) -> Box<dyn Iterator<Item = &Paragraph> + '_> {
        match &self.kind {
            ObjectKind::Group(objects) => Box::new(objects.iter().flat_map(Self::paragraphs)),
            _ => match self.text_box() {
                Some(text) => Box::new(text.paragraphs.iter()),
                None => Box::new(std::iter::empty()),
            },
        }
    }

    /// Every identifier inside the object: its own, those of its
    /// paragraphs, and those of the objects of a group.
    pub fn ids(&self) -> Vec<NodeId> {
        let mut ids = vec![self.id];
        match &self.kind {
            ObjectKind::Group(objects) => ids.extend(objects.iter().flat_map(Self::ids)),
            _ => ids.extend(self.paragraphs().map(Paragraph::id)),
        }
        ids
    }
}

/// What an object is.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ObjectKind {
    /// A shape or a line (OBJ-08).
    Shape(Shape),
    /// A text box (OBJ-13).
    TextBox(TextBox),
    /// A picture, scaled to its frame (OBJ-02).
    Image(Picture),
    /// A movie or a sound (OBJ-18).
    Media(crate::Media),
    /// Objects grouped into one, with frames on the same page as the
    /// group's, from back to front.
    Group(Arc<[Object]>),
    /// A table, whose frame gives its place; its size is that of its
    /// columns and rows (TAB-01).
    Table(Arc<crate::Table>),
    /// A chart, drawn in its frame (GRF-01).
    Chart(Arc<desk_chart::Chart>),
    /// An equation, drawn in its frame (W104).
    Equation(Arc<crate::Equation>),
}

impl From<Shape> for ObjectKind {
    fn from(shape: Shape) -> Self {
        Self::Shape(shape)
    }
}

/// The outline of a shape, scaled to its frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Shape {
    /// A rectangle.
    Rectangle,
    /// A rectangle with round corners.
    RoundedRectangle {
        /// The radius of the corners.
        radius: Length,
    },
    /// An ellipse, or a circle in a square frame.
    Ellipse,
    /// A closed polygon, by its points in parts of [`POLYGON_SCALE`] of
    /// the frame from its top left corner.
    Polygon(Arc<[(i32, i32)]>),
    /// A line between two corners of the frame: from the top left to the
    /// bottom right, or from the bottom left to the top right when it
    /// rises.
    Line {
        /// Whether it goes from the bottom left to the top right.
        rising: bool,
    },
    /// A star filling the frame (OBJ-08, OBJ-11).
    Star {
        /// How many points it has, from 3 to [`MAX_SHAPE_SIDES`].
        points: u8,
        /// How far its inner corners are from the middle, in parts of
        /// [`POLYGON_SCALE`] of the outer radius.
        inner: u16,
    },
    /// A polygon with equal sides filling the frame.
    RegularPolygon {
        /// How many sides it has, from 3 to [`MAX_SHAPE_SIDES`].
        sides: u8,
    },
    /// An arrow pointing right, or both ways.
    Arrow {
        /// Whether it has a head at both ends.
        double: bool,
        /// How long each head is, in parts of [`POLYGON_SCALE`] of the
        /// width of the frame.
        head: u16,
        /// How thick the tail is, in parts of [`POLYGON_SCALE`] of the
        /// height of the frame.
        tail: u16,
    },
    /// A speech balloon: a rectangle, with round corners for a quote
    /// bubble or square ones for a callout, and a tail to the bottom of
    /// the frame.
    Balloon {
        /// The radius of the corners.
        radius: Length,
        /// How tall the body is, in parts of [`POLYGON_SCALE`] of the
        /// height of the frame; the tail takes the rest.
        body: u16,
        /// Where the point of the tail is, in parts of [`POLYGON_SCALE`]
        /// of the width of the frame.
        tip: u16,
    },
    /// A shape drawn with the pen, by its points, open or closed
    /// (OBJ-09, OBJ-10).
    Path {
        /// The points, in order.
        points: Arc<[PathPoint]>,
        /// Whether the last point joins the first.
        closed: bool,
    },
    /// A connection line, which joins two objects and follows them
    /// (OBJ-31).
    Connection(crate::Connection),
}

/// A point of a shape drawn with the pen, in parts of [`POLYGON_SCALE`]
/// of the frame from its top left corner, as its handles are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PathPoint {
    /// Where the point is.
    pub at: (i32, i32),
    /// The handle of the curve that comes into the point, or none at a
    /// corner.
    pub before: Option<(i32, i32)>,
    /// The handle of the curve that leaves it.
    pub after: Option<(i32, i32)>,
}

impl PathPoint {
    /// A corner at a point.
    pub fn corner(x: i32, y: i32) -> Self {
        Self {
            at: (x, y),
            before: None,
            after: None,
        }
    }

    /// Whether the curves through the point are smooth, with handles.
    pub fn is_curve(&self) -> bool {
        self.before.is_some() || self.after.is_some()
    }
}

/// The most points of a star and sides of a polygon.
pub const MAX_SHAPE_SIDES: u8 = 32;

impl Shape {
    /// The star of Pages: five points.
    pub fn star() -> Self {
        Self::Star {
            points: 5,
            inner: 3_820,
        }
    }

    /// The polygon of Pages: five sides.
    pub fn regular_polygon() -> Self {
        Self::RegularPolygon { sides: 5 }
    }

    /// The arrow of Pages, to the right, or both ways.
    pub fn arrow(double: bool) -> Self {
        Self::Arrow {
            double,
            head: if double { 3_000 } else { 4_000 },
            tail: 5_000,
        }
    }

    /// The quote bubble of Pages, or with square corners its callout.
    pub fn balloon(rounded: bool) -> Self {
        Self::Balloon {
            radius: Length::from_points(if rounded { 12 } else { 0 }),
            body: 7_500,
            tip: 2_000,
        }
    }

    /// The outline of the shape in a frame of `width` by `height` points,
    /// from its top left corner.
    pub fn outline(&self, width: f64, height: f64) -> Vec<crate::PathSegment> {
        crate::shape_path::outline(self, width, height)
    }

    /// Whether it is a line, straight or a connection line, which has
    /// ends and no inside.
    pub fn is_line(&self) -> bool {
        matches!(self, Self::Line { .. } | Self::Connection(_))
    }

    /// A triangle with its point at the top middle.
    pub fn triangle() -> Self {
        Self::Polygon(Arc::from(
            [
                (POLYGON_SCALE / 2, 0),
                (POLYGON_SCALE, POLYGON_SCALE),
                (0, POLYGON_SCALE),
            ]
            .as_slice(),
        ))
    }
}

/// A text box: paragraphs laid out inside a frame (OBJ-13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBox {
    /// The text.
    pub paragraphs: Arc<[Paragraph]>,
    /// The room between the frame and the text, on every side.
    pub inset: Length,
    /// Where the text sits in the frame, from top to bottom, when it
    /// does not fill it.
    pub vertical: VerticalAlignment,
    /// How many columns the text flows through, from 1.
    pub columns: u8,
    /// The room between two columns.
    pub column_gap: Length,
    /// The text box that the text goes on in when it does not fit, as
    /// linked text boxes of Pages (OBJ-13). The text of linked boxes is
    /// all in the first of them; the paragraphs of the others are left
    /// empty.
    pub next: Option<NodeId>,
    /// Whether the box is in a thread of Pages even when it is the only
    /// box of it, which shows its number on the box (PXV-13). Boxes linked
    /// with others are always in a thread; this is kept on the first.
    pub thread: bool,
}

impl TextBox {
    /// The text of a shape: in the middle of it, with the inset of
    /// Pages.
    pub fn in_shape(paragraphs: Arc<[Paragraph]>) -> Self {
        Self {
            vertical: VerticalAlignment::Middle,
            ..Self::new(paragraphs, Length::from_points(4))
        }
    }

    /// A text box with paragraphs, an inset, its text at the top and one
    /// column.
    pub fn new(paragraphs: Arc<[Paragraph]>, inset: Length) -> Self {
        Self {
            paragraphs,
            inset,
            vertical: VerticalAlignment::Top,
            columns: 1,
            column_gap: Length::from_points(18),
            next: None,
            thread: false,
        }
    }
}

/// Where the text of a text box sits, from top to bottom (OBJ-13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VerticalAlignment {
    /// At the top.
    #[default]
    Top,
    /// In the middle.
    Middle,
    /// At the bottom.
    Bottom,
}

/// The most columns a text box has, as Pages allows.
pub const MAX_TEXT_BOX_COLUMNS: u8 = 10;

/// A rectangle on the page, from its top left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Frame {
    /// The left edge.
    pub x: Length,
    /// The top edge.
    pub y: Length,
    /// The width.
    pub width: Length,
    /// The height.
    pub height: Length,
}

impl Frame {
    /// A frame from its position and size, in points.
    pub fn from_pt(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x: Length::from_pt(x),
            y: Length::from_pt(y),
            width: Length::from_pt(width),
            height: Length::from_pt(height),
        }
    }
}

/// What a floating object moves with (OBJ-01).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Anchor {
    /// Fixed on a page, by its index from 0: the frame is measured from
    /// the top left corner of the paper, as Pages keeps floating objects.
    Page(u32),
    /// Moving with a paragraph of the body, as other programs anchor
    /// objects: the frame is measured from the left of the paper and the
    /// top of the first line of the paragraph, on the page it starts on.
    Paragraph(NodeId),
    /// In the line of the text of the body, as a character, where its
    /// mark is (see [`crate::Field::ObjectMark`]); the frame gives only its
    /// size, and the text does not wrap around it (OBJ-01).
    InText,
    /// On the master of a section, by the paragraph that starts the
    /// section, or none for the first: behind the text of the pages of
    /// that section that show the headers and footers of the same pages,
    /// where the frame puts it from the top left corner of the paper
    /// (INT-05, EST-21). A section whose first page or left pages differ
    /// has masters of their own for them (EST-19). The text does not wrap
    /// around it, and it is selected only when master objects are made
    /// selectable.
    Master(Option<NodeId>, crate::MarginPages),
}

impl Anchor {
    /// Whether the object is on the master of a section (INT-05).
    pub fn is_master(self) -> bool {
        matches!(self, Self::Master(..))
    }
}

/// How the text of the body goes around an object (OBJ-17).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Wrap {
    /// On both sides.
    #[default]
    Both,
    /// Only on the side with more room.
    Largest,
    /// Only on its left.
    Left,
    /// Only on its right.
    Right,
    /// Above and below it, and not beside it.
    AboveAndBelow,
    /// Through it, as if it were not there, with the object in front of
    /// the text.
    Through,
    /// Through it, with the object behind the text.
    Behind,
}

/// How an object is filled and outlined, as the Graphic inspector of
/// Pages sets it (OBJ-09, OBJ-10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphicStyle {
    /// What fills its inside; nothing when none.
    pub fill: Option<Fill>,
    /// The line around it, or the line itself; none when it has none.
    pub stroke: Option<Stroke>,
    /// How opaque the whole object is, in percent.
    pub opacity: u8,
    /// The shadow it casts on the page; none when it casts none.
    pub shadow: Option<ObjectShadow>,
    /// How opaque its reflection below it is where it starts, in percent;
    /// none when it is not reflected (OBJ-38).
    pub reflection: Option<u8>,
    /// The picture frame around it, which takes the place of the line
    /// (OBJ-36); none when it has none.
    pub frame: Option<PictureFrame>,
}

/// A picture frame of the library of Desk, drawn around an object as
/// the frames of Pages are (OBJ-36).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PictureFrame {
    /// Which frame of the library.
    pub style: FrameStyle,
    /// How thick it is, in percent of its own thickness.
    pub scale: u16,
}

impl PictureFrame {
    /// A frame of a style at its own thickness.
    pub fn new(style: FrameStyle) -> Self {
        Self { style, scale: 100 }
    }

    /// How far it reaches out of the object, at its scale.
    pub fn width(&self) -> Length {
        Length::from_pt(self.style.width().to_pt() * f64::from(self.scale) / 100.0)
    }
}

/// The picture frames of the library of Desk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FrameStyle {
    /// A white border with a gray edge and a soft shadow, as a print.
    #[default]
    Photo,
    /// A wide white mat inside a thin black frame, as in a gallery.
    Gallery,
    /// A frame of dark wood with a gold edge.
    Classic,
    /// Brushed metal.
    Metal,
    /// A white border with tape across its top corners.
    Tape,
}

impl FrameStyle {
    /// Every style, in the order of the library.
    pub const ALL: [Self; 5] = [
        Self::Photo,
        Self::Gallery,
        Self::Classic,
        Self::Metal,
        Self::Tape,
    ];

    /// Its name in files.
    pub fn name(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Gallery => "gallery",
            Self::Classic => "classic",
            Self::Metal => "metal",
            Self::Tape => "tape",
        }
    }

    /// The style of a name in files.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// How far it reaches out of the object at its own thickness.
    pub fn width(self) -> Length {
        Length::from_points(match self {
            Self::Photo | Self::Tape => 8,
            Self::Gallery => 20,
            Self::Classic => 14,
            Self::Metal => 9,
        })
    }
}

impl Default for GraphicStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            opacity: 100,
            shadow: None,
            reflection: None,
            frame: None,
        }
    }
}

impl GraphicStyle {
    /// How opaque a reflection is where it starts when the Graphic
    /// inspector turns it on, in percent, as in Pages.
    pub const REFLECTION: u8 = 50;
}

/// What fills an object.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Fill {
    /// One color.
    Color(Rgba),
    /// A color that turns into another along a straight line across the
    /// object (OBJ-09).
    Gradient {
        /// The color where it starts.
        from: Rgba,
        /// The color where it ends.
        to: Rgba,
        /// Which way it goes, in hundredths of a degree counterclockwise
        /// from left to right, as Pages measures it: 9000 goes from the
        /// bottom up.
        angle: i32,
    },
    /// Colors that turn into each other one after another, along a line
    /// or out from a point: the advanced gradient of Pages (OBJ-41).
    Advanced(Gradient),
    /// A picture, sized to the object as the mode says (OBJ-09).
    Image {
        /// The picture.
        picture: Picture,
        /// How it is sized to the object.
        mode: ImageFill,
        /// A color laid over it, with its opacity in its alpha: the
        /// tinted image fill of Pages (OBJ-42).
        tint: Option<Rgba>,
    },
}

/// The advanced gradient of Pages: several colors, with where each
/// one blends into the next, along a line or out from a point (OBJ-41).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Gradient {
    /// The colors, in order from where it starts; two at least.
    pub stops: Vec<GradientStop>,
    /// Which way a gradient along a line goes, as in [`Fill::Gradient`].
    pub angle: i32,
    /// The point a radial gradient goes out from, in thousandths of the
    /// width and the height of the object from its top left corner; none
    /// along a line. It reaches the farthest corner.
    pub center: Option<(u16, u16)>,
}

/// A color of an advanced gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GradientStop {
    /// Where it is, in thousandths of the way.
    pub at: u16,
    /// The color.
    pub color: Rgba,
    /// Where it is half blended into the next color, in thousandths of
    /// the way to it: 500 is halfway.
    pub blend: u16,
}

impl GradientStop {
    /// A color at a place, which blends halfway into the next.
    pub fn new(at: u16, color: Rgba) -> Self {
        Self {
            at: at.min(1000),
            color,
            blend: 500,
        }
    }
}

impl Gradient {
    /// A gradient along a line of the colors of a two color one.
    pub fn linear(from: Rgba, to: Rgba, angle: i32) -> Self {
        Self {
            stops: vec![GradientStop::new(0, from), GradientStop::new(1000, to)],
            angle,
            center: None,
        }
    }

    /// The colors to draw, by where they are from 0 to 1, with one more
    /// halfway between two where the blend is not in the middle.
    pub fn color_stops(&self) -> Vec<(f64, Rgba)> {
        let mut stops = self.stops.clone();
        stops.sort_by_key(|s| s.at);
        let mut out = Vec::new();
        for (i, stop) in stops.iter().enumerate() {
            out.push((f64::from(stop.at.min(1000)) / 1000.0, stop.color));
            if let Some(next) = stops.get(i + 1)
                && stop.blend != 500
                && next.at > stop.at
            {
                let at = f64::from(stop.at)
                    + f64::from(next.at - stop.at) * f64::from(stop.blend.clamp(1, 999)) / 1000.0;
                out.push((at.min(1000.0) / 1000.0, stop.color.mix(next.color)));
            }
        }
        out
    }
}

/// How a picture fills an object, as the Graphic inspector of Pages
/// offers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageFill {
    /// As large as the object, on the side that leaves none of it empty,
    /// and cut on the other.
    #[default]
    ScaleToFill,
    /// As large as the object, on the side that shows all of the picture.
    ScaleToFit,
    /// To the size of the object, out of its proportions.
    Stretch,
    /// At its own size, in the middle of the object.
    OriginalSize,
    /// At its own size, over and over from the top left corner.
    Tile,
}

/// The line around an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stroke {
    /// The color.
    pub color: Rgba,
    /// The width.
    pub width: Length,
    /// How it is drawn.
    pub style: BorderStyle,
    /// What the start of a line ends in (OBJ-10).
    pub start: LineEnd,
    /// What the end of a line ends in.
    pub end: LineEnd,
}

impl Stroke {
    /// A solid line of a color and width, with plain ends.
    pub fn new(color: Rgba, width: Length) -> Self {
        Self {
            color,
            width,
            style: BorderStyle::Solid,
            start: LineEnd::None,
            end: LineEnd::None,
        }
    }
}

/// What an end of a line ends in, as the endpoints of Pages (OBJ-10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineEnd {
    /// Nothing: the line stops there.
    #[default]
    None,
    /// A filled arrowhead.
    Arrow,
    /// An arrowhead of two strokes.
    OpenArrow,
    /// A filled circle.
    Circle,
    /// A filled square.
    Square,
    /// A short bar across the line.
    Bar,
}

/// The shadow an object casts, as the Graphic inspector of Pages sets it
/// (OBJ-11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectShadow {
    /// The color.
    pub color: Rgba,
    /// Which way it falls from the object, in hundredths of a degree
    /// counterclockwise from the right: 31500 is down and to the right.
    pub angle: i32,
    /// How far it falls.
    pub offset: Length,
    /// How blurred it is.
    pub blur: Length,
    /// How opaque it is, in percent.
    pub opacity: u8,
}

impl Default for ObjectShadow {
    /// The shadow of Pages: black, down and to the right.
    fn default() -> Self {
        Self {
            color: Rgba::BLACK,
            angle: 31_500,
            offset: Length::from_points(5),
            blur: Length::from_points(4),
            opacity: 75,
        }
    }
}

impl ObjectShadow {
    /// How far the shadow falls right and down, in points.
    pub fn offset_xy(&self) -> (f64, f64) {
        let angle = (f64::from(self.angle) / 100.0).to_radians();
        let offset = self.offset.to_pt();
        (offset * angle.cos(), -offset * angle.sin())
    }
}

/// The objects of a document, from back to front.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Objects(imbl::Vector<Object>, imbl::HashSet<NodeId>);

impl Objects {
    /// How many objects there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is no object.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The objects, from back to front.
    pub fn iter(&self) -> impl Iterator<Item = &Object> {
        self.0.iter()
    }

    /// An object by identifier.
    pub fn get(&self, id: NodeId) -> Option<&Object> {
        self.0.iter().find(|o| o.id == id)
    }

    /// The place of an object from the back, by identifier.
    pub fn index_of(&self, id: NodeId) -> Option<usize> {
        self.0.iter().position(|o| o.id == id)
    }

    /// Every paragraph of every text box.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.0.iter().flat_map(Object::paragraphs)
    }

    /// Whether an object, or a paragraph or an object inside one, has an
    /// identifier, without going through them: a file adds objects one
    /// by one, and each identifier is checked to be new.
    pub fn holds_id(&self, id: NodeId) -> bool {
        self.1.contains(&id)
    }

    pub(crate) fn insert(&mut self, index: usize, object: Object) {
        self.1.extend(object.ids());
        self.0.insert(index, object);
    }

    pub(crate) fn remove(&mut self, index: usize) -> Object {
        let object = self.0.remove(index);
        for id in object.ids() {
            self.1.remove(&id);
        }
        object
    }

    pub(crate) fn set(&mut self, index: usize, object: Object) -> Object {
        let ids = object.ids();
        let old = self.0.set(index, object);
        for id in old.ids() {
            self.1.remove(&id);
        }
        self.1.extend(ids);
        old
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn histogram_of(pixels: &[[u8; 3]]) -> Histogram {
        let mut histogram = Histogram::default();
        for &p in pixels {
            histogram.add(p);
        }
        histogram
    }

    #[test]
    fn w110_the_levels_stretch_the_values_between_them() {
        let a = Adjustments {
            black: 51,
            white: 204,
            ..Adjustments::default()
        };
        assert!(!a.is_none());
        assert!(a.changes_colors());
        let [r, g, b] = a.apply([0.2, 0.5, 0.8]);
        assert!(r.abs() < 0.01 && (g - 0.5).abs() < 0.01 && (b - 1.0).abs() < 0.01);
        // Handles that cross keep a step between them.
        let crossed = Adjustments {
            black: 200,
            white: 100,
            ..Adjustments::default()
        };
        assert!(crossed.apply([0.9, 0.5, 0.0]).iter().all(|v| v.is_finite()));
        let all_black = Adjustments {
            black: 255,
            white: 255,
            ..Adjustments::default()
        };
        assert_eq!(all_black.apply([1.0, 0.5, 0.0]), [1.0, 0.0, 0.0]);
        // The sharpness alone changes no pixel by itself.
        let sharp = Adjustments {
            sharpness: 50,
            ..Adjustments::default()
        };
        assert!(!sharp.is_none() && !sharp.changes_colors());
    }

    #[test]
    fn w110_enhance_brightens_a_dark_picture_and_colors_a_dull_one() {
        let dark = histogram_of(&[[30, 25, 20], [50, 40, 35], [20, 20, 25]]);
        let enhanced = Adjustments::default().enhanced(&dark);
        assert!(enhanced.exposure > 20, "{enhanced:?}");
        let bright = histogram_of(&[[240, 235, 230], [250, 245, 240]]);
        assert!(Adjustments::default().enhanced(&bright).exposure < 0);
        // A picture of middle lightness and color keeps them.
        let fine = histogram_of(&[[150, 100, 60], [80, 110, 160], [130, 120, 90]]);
        let kept = Adjustments::default().enhanced(&fine);
        assert!(
            kept.exposure.abs() <= 10 && kept.saturation.abs() <= 15,
            "{kept:?}"
        );
        // Gray stays gray; the other adjustments are kept.
        let gray = histogram_of(&[[60, 60, 60], [90, 90, 90]]);
        let start = Adjustments {
            tint: 12,
            ..Adjustments::default()
        };
        let enhanced = start.enhanced(&gray);
        assert_eq!((enhanced.saturation, enhanced.tint), (0, 12));
        // Nothing counted, nothing changed.
        assert_eq!(start.enhanced(&Histogram::default()), start);
    }

    #[test]
    fn w110_auto_levels_take_the_darkest_and_lightest_values() {
        let mut pixels = vec![[100, 100, 100]; 400];
        pixels.extend([[40, 40, 40]; 100]);
        pixels.extend([[180, 180, 180]; 100]);
        let a = Adjustments::default().auto_levels(&histogram_of(&pixels));
        assert_eq!((a.black, a.white), (40, 180));
        // A picture of one value keeps the levels.
        let flat = Adjustments::default().auto_levels(&histogram_of(&[[90, 90, 90]]));
        assert_eq!((flat.black, flat.white), (0, 255));
    }
    use crate::{Document, Editor, ModelError, ParaAttrs, Run};

    fn shape(id: NodeId) -> Object {
        Object::new(
            id,
            ObjectKind::Shape(Shape::Rectangle),
            Frame::from_pt(10.0, 20.0, 30.0, 40.0),
        )
    }

    #[test]
    fn obj_41_a_blend_point_off_the_middle_adds_the_color_halfway_there() {
        let gradient = Gradient {
            stops: vec![
                GradientStop {
                    blend: 250,
                    ..GradientStop::new(0, Rgba::BLACK)
                },
                GradientStop::new(1000, Rgba::WHITE),
            ],
            angle: 0,
            center: None,
        };
        assert_eq!(
            gradient.color_stops(),
            vec![
                (0.0, Rgba::BLACK),
                (0.25, Rgba::rgb(128, 128, 128)),
                (1.0, Rgba::WHITE)
            ]
        );
        assert_eq!(
            Gradient::linear(Rgba::BLACK, Rgba::WHITE, 0).color_stops(),
            vec![(0.0, Rgba::BLACK), (1.0, Rgba::WHITE)]
        );
    }

    #[test]
    fn obj_01_objects_are_added_moved_and_removed_with_undo() {
        let mut editor = Editor::new(Document::new_word_processing());
        let empty = editor.document().clone();
        let (a, b) = editor
            .edit("Shapes", |edit| {
                let (a, b) = (edit.new_id(), edit.new_id());
                edit.add_object(shape(a))?;
                edit.insert_object(0, shape(b))?;
                Ok((a, b))
            })
            .unwrap();
        let order: Vec<NodeId> = editor.document().objects().iter().map(|o| o.id).collect();
        assert_eq!(order, [b, a]);
        editor
            .edit("Move", |edit| {
                let mut moved = shape(a);
                moved.frame.x = Length::from_points(100);
                moved.rotation = 4500;
                edit.replace_object(moved)
            })
            .unwrap();
        let moved = editor.document().objects().get(a).unwrap();
        assert_eq!(
            (moved.frame.x, moved.rotation),
            (Length::from_points(100), 4500)
        );
        editor.edit("Delete", |edit| edit.remove_object(b)).unwrap();
        assert_eq!(editor.document().objects().len(), 1);
        editor.undo().unwrap();
        editor.undo().unwrap();
        assert_eq!(editor.document().objects().get(a), Some(&shape(a)));
        editor.undo().unwrap();
        assert_eq!(editor.document(), &empty);
        assert_eq!(
            editor.edit("Unknown", |edit| edit.remove_object(a)),
            Err(ModelError::UnknownObject(a))
        );
    }

    #[test]
    fn obj_13_text_boxes_keep_their_ids_and_styles_in_use() {
        let mut editor = Editor::new(Document::new_word_processing());
        let body = editor.document().styles().default_paragraph();
        let first = editor.document().start().paragraph;
        let result = editor.edit("Box", |edit| {
            let id = edit.new_id();
            let paragraph =
                Paragraph::from_runs(first, body, ParaAttrs::default(), [Run::new("x")])?;
            let text = TextBox::new(Arc::from([paragraph]), Length::from_points(4));
            edit.add_object(Object::new(id, ObjectKind::TextBox(text), Frame::default()))
        });
        // The paragraph of the body already has that identifier.
        assert_eq!(result, Err(ModelError::DuplicateId(first)));
        let object = editor
            .edit("Box", |edit| {
                let (id, p) = (edit.new_id(), edit.new_id());
                let paragraph =
                    Paragraph::from_runs(p, body, ParaAttrs::default(), [Run::new("x")])?;
                let text = TextBox::new(Arc::from([paragraph]), Length::from_points(4));
                let object = Object::new(id, ObjectKind::TextBox(text), Frame::default());
                edit.add_object(object.clone())?;
                Ok(object)
            })
            .unwrap();
        assert_eq!(object.ids().len(), 2);
        assert_eq!(editor.document().objects().paragraphs().count(), 1);
    }
}
