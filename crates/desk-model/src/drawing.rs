//! The drawings of Pages 15.4 (W117, PXV-26): lines drawn freehand with a
//! pen, a pencil or a crayon, in a colour and a width, and the areas they
//! close filled with the fill tool, kept together as one object that can
//! be drawn on again; the Drawing tab animates it, the lines drawn again
//! in their order over a time, once or in a loop. Pages draws them with
//! the Apple Pencil of an iPad; on GNOME the pointer or a stylus does
//! (D-005).

use std::sync::Arc;

use crate::{Edit, Frame, Length, ModelError, NodeId, ObjectKind, POLYGON_SCALE, Rgba};

/// The most lines of a drawing, against hostile files.
pub const MAX_DRAWING_STROKES: usize = 5_000;
/// The most points of a line of a drawing.
pub const MAX_STROKE_POINTS: usize = 20_000;
/// The shortest and the longest time a drawing is animated over, in
/// milliseconds.
pub const ANIMATION_DURATION: std::ops::RangeInclusive<u32> = 100..=60_000;

/// What draws a line of a drawing, as the tools of Pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DrawingTool {
    /// A pen: a smooth, even line with round ends.
    #[default]
    Pen,
    /// A pencil: a thin, grainy line.
    Pencil,
    /// A crayon: a waxy line, with streaks where the paper shows.
    Crayon,
    /// The fill: an area closed by a line, filled with a colour.
    Fill,
}

impl DrawingTool {
    /// Every tool, in the order of the bar of tools.
    pub const ALL: [Self; 4] = [Self::Pen, Self::Pencil, Self::Crayon, Self::Fill];

    /// Its name in files.
    pub fn name(self) -> &'static str {
        match self {
            Self::Pen => "pen",
            Self::Pencil => "pencil",
            Self::Crayon => "crayon",
            Self::Fill => "fill",
        }
    }

    /// The tool of a name in files.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }
}

/// A line of a drawing, or an area filled.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DrawingStroke {
    /// What draws it.
    pub tool: DrawingTool,
    /// Its colour.
    pub color: Rgba,
    /// How wide the line is; nothing for a fill.
    pub width: Length,
    /// The points the line goes through, in parts of [`POLYGON_SCALE`] of
    /// the frame from its top left corner; those around the area of a
    /// fill.
    pub points: Arc<[(i32, i32)]>,
}

/// How a drawing is animated: its lines drawn again in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DrawingAnimation {
    /// How long it takes, in milliseconds.
    pub duration: u32,
    /// Whether it starts again when it ends.
    pub repeat: bool,
}

impl Default for DrawingAnimation {
    fn default() -> Self {
        Self {
            duration: 3_000,
            repeat: false,
        }
    }
}

/// A drawing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Drawing {
    /// The lines and fills, from the first drawn to the last, which is in
    /// front.
    pub strokes: Arc<[DrawingStroke]>,
    /// How it is animated; none when it is not.
    pub animation: Option<DrawingAnimation>,
}

/// The length of a line through points.
fn length(points: &[(f64, f64)]) -> f64 {
    points
        .windows(2)
        .map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1))
        .sum()
}

impl DrawingStroke {
    /// Its points in points of a frame of a size.
    pub fn points_in(&self, width: f64, height: f64) -> Vec<(f64, f64)> {
        let scale = f64::from(POLYGON_SCALE);
        self.points
            .iter()
            .map(|&(x, y)| (f64::from(x) / scale * width, f64::from(y) / scale * height))
            .collect()
    }

    /// Whether a point of a frame of a size is inside the area the line
    /// closes, from its last point back to its first.
    pub fn encloses(&self, (x, y): (f64, f64), width: f64, height: f64) -> bool {
        let points = self.points_in(width, height);
        if points.len() < 3 {
            return false;
        }
        let mut inside = false;
        let mut j = points.len() - 1;
        for (i, &(xi, yi)) in points.iter().enumerate() {
            let (xj, yj) = points[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// Whether the line closes an area by itself: it ends near where it
    /// starts, within twice its width or four points.
    pub fn closes(&self, width: f64, height: f64) -> bool {
        let points = self.points_in(width, height);
        match (points.first(), points.last()) {
            (Some(a), Some(b)) if points.len() >= 3 => {
                (a.0 - b.0).hypot(a.1 - b.1) <= (2.0 * self.width.to_pt()).max(4.0)
            }
            _ => false,
        }
    }

    /// Whether a point of a frame of a size is on the line, within a
    /// distance more than half its width.
    pub fn touches(&self, at: (f64, f64), reach: f64, width: f64, height: f64) -> bool {
        let points = self.points_in(width, height);
        let reach = reach + self.width.to_pt() / 2.0;
        match points.as_slice() {
            [] => false,
            [only] => (only.0 - at.0).hypot(only.1 - at.1) <= reach,
            _ => points
                .windows(2)
                .any(|p| distance_to_segment(at, p[0], p[1]) <= reach),
        }
    }
}

/// The distance from a point to a segment.
fn distance_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let length = vx * vx + vy * vy;
    let t = if length > 0.0 {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.0 - (a.0 + t * vx)).hypot(p.1 - (a.1 + t * vy))
}

/// How many cells the longer side of a drawing is cut into, to find the
/// area that several lines close around a point.
const FILL_CELLS: f64 = 500.0;

/// The area that the lines of a drawing close together around a point of
/// its frame of a size, as an outline in points of the frame, with the
/// index of the first line around it, which the fill goes behind: none
/// when the point is on a line or the area is open to the edge. The frame
/// is cut into cells, those under a line are inked, and the cells joined
/// to that of the point by their sides are the area.
fn enclosed_area(
    drawing: &Drawing,
    at: (f64, f64),
    width: f64,
    height: f64,
) -> Option<(Vec<(f64, f64)>, usize)> {
    let cell = (width.max(height) / FILL_CELLS).max(1e-3);
    // A cell more around the frame, through which an open area leaks.
    let columns = (width / cell).ceil() as usize + 2;
    let rows = (height / cell).ceil() as usize + 2;
    let center = |i: usize| (i as f64 - 0.5) * cell;
    let mut ink = vec![u32::MAX; columns * rows];
    for (index, stroke) in drawing.strokes.iter().enumerate() {
        if stroke.tool == DrawingTool::Fill {
            continue;
        }
        let points = stroke.points_in(width, height);
        // A little thinner than the line, so that the fill goes under it,
        // and never so thin that the area leaks between its cells.
        let reach = (stroke.width.to_pt() / 2.0 - cell).max(0.75 * cell);
        let pieces: Vec<((f64, f64), (f64, f64))> = match points.as_slice() {
            [] => Vec::new(),
            [only] => vec![(*only, *only)],
            _ => points.windows(2).map(|p| (p[0], p[1])).collect(),
        };
        for (a, b) in pieces {
            let range = |lo: f64, hi: f64, count: usize| {
                let first = ((lo - reach) / cell + 0.5).floor().max(0.0) as usize;
                let last = (((hi + reach) / cell + 1.5).ceil().max(0.0) as usize).min(count - 1);
                first..=last
            };
            for row in range(a.1.min(b.1), a.1.max(b.1), rows) {
                for column in range(a.0.min(b.0), a.0.max(b.0), columns) {
                    if distance_to_segment((center(column), center(row)), a, b) <= reach {
                        let owner = &mut ink[row * columns + column];
                        *owner = (*owner).min(index as u32);
                    }
                }
            }
        }
    }
    let start = ((at.0 / cell + 1.0).floor(), (at.1 / cell + 1.0).floor());
    if start.0 < 1.0 || start.1 < 1.0 {
        return None;
    }
    let start = (start.0 as usize, start.1 as usize);
    if start.0 >= columns - 1 || start.1 >= rows - 1 || ink[start.1 * columns + start.0] != u32::MAX
    {
        return None;
    }
    let mut area = vec![false; columns * rows];
    area[start.1 * columns + start.0] = true;
    let mut next = vec![start];
    let mut behind = u32::MAX;
    while let Some((column, row)) = next.pop() {
        // The area reaches the edge: the lines leave it open.
        if column == 0 || row == 0 || column == columns - 1 || row == rows - 1 {
            return None;
        }
        for (c, r) in [
            (column - 1, row),
            (column + 1, row),
            (column, row - 1),
            (column, row + 1),
        ] {
            let i = r * columns + c;
            if ink[i] != u32::MAX {
                behind = behind.min(ink[i]);
            } else if !area[i] {
                area[i] = true;
                next.push((c, r));
            }
        }
    }
    let outline = crate::pixel_outline(columns, rows, |x, y| area[y * columns + x])?;
    let points = outline
        .into_iter()
        .map(|(x, y)| {
            (
                ((f64::from(x) - 1.0) * cell).clamp(0.0, width),
                ((f64::from(y) - 1.0) * cell).clamp(0.0, height),
            )
        })
        .collect();
    Some((points, behind as usize))
}

impl Drawing {
    /// The drawing as far as its animation has drawn it, from 0 for none
    /// of it to 1 for all: the lines in their order, each as long as its
    /// share of the time, by its length in a frame of a size. A fill shows
    /// once the lines before it are drawn.
    pub fn drawn_to(&self, progress: f64, width: f64, height: f64) -> Self {
        let progress = progress.clamp(0.0, 1.0);
        if progress >= 1.0 {
            return self.clone();
        }
        let lengths: Vec<f64> = self
            .strokes
            .iter()
            .map(|s| match s.tool {
                DrawingTool::Fill => 0.0,
                _ => length(&s.points_in(width, height)).max(1.0),
            })
            .collect();
        let total: f64 = lengths.iter().sum();
        let mut left = progress * total;
        let mut strokes = Vec::new();
        for (stroke, &long) in self.strokes.iter().zip(&lengths) {
            if long == 0.0 {
                // A fill shows once the lines before it are whole.
                if progress > 0.0 {
                    strokes.push(stroke.clone());
                }
                continue;
            }
            if left >= long {
                strokes.push(stroke.clone());
                left -= long;
                continue;
            }
            if left <= 0.0 {
                break;
            }
            // The part of the line drawn so far.
            let points = stroke.points_in(width, height);
            let mut kept = vec![stroke.points[0]];
            let mut went = 0.0;
            for (i, pair) in points.windows(2).enumerate() {
                let step = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
                if went + step <= left {
                    went += step;
                    kept.push(stroke.points[i + 1]);
                    continue;
                }
                let t = if step > 0.0 {
                    (left - went) / step
                } else {
                    0.0
                };
                let (a, b) = (stroke.points[i], stroke.points[i + 1]);
                kept.push((
                    a.0 + (f64::from(b.0 - a.0) * t).round() as i32,
                    a.1 + (f64::from(b.1 - a.1) * t).round() as i32,
                ));
                break;
            }
            strokes.push(DrawingStroke {
                points: kept.into(),
                ..stroke.clone()
            });
            break;
        }
        Self {
            strokes: strokes.into(),
            animation: self.animation,
        }
    }
}

impl crate::Object {
    /// The drawing of the object, if it is one.
    pub fn drawing(&self) -> Option<&Drawing> {
        match &self.kind {
            ObjectKind::Drawing(drawing) => Some(drawing),
            _ => None,
        }
    }
}

/// A line to add to a drawing, in points of the frame of the drawing from
/// its top left corner as it is before it turns: they may be out of it.
#[derive(Clone, Debug, PartialEq)]
pub struct NewStroke {
    /// What draws it.
    pub tool: DrawingTool,
    /// Its colour.
    pub color: Rgba,
    /// How wide it is.
    pub width: Length,
    /// Its points.
    pub points: Vec<(f64, f64)>,
}

/// A frame in points, `x, y, width, height`, with the lines of a drawing
/// in parts of it.
pub type FramedStrokes = ((f64, f64, f64, f64), Vec<DrawingStroke>);

/// The frame of a drawing that holds lines through points, a line as wide
/// as it is, with each line in parts of it: a frame of a point at least.
pub fn drawing_frame(strokes: &[NewStroke]) -> Option<FramedStrokes> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for stroke in strokes {
        let half = match stroke.tool {
            DrawingTool::Fill => 0.0,
            _ => stroke.width.to_pt() / 2.0,
        };
        for &(x, y) in &stroke.points {
            (x0, y0) = (x0.min(x - half), y0.min(y - half));
            (x1, y1) = (x1.max(x + half), y1.max(y + half));
        }
    }
    if x0 > x1 || !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return None;
    }
    let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    let scale = f64::from(POLYGON_SCALE);
    let built = strokes
        .iter()
        .map(|s| DrawingStroke {
            tool: s.tool,
            color: s.color,
            width: s.width,
            points: s
                .points
                .iter()
                .map(|&(x, y)| {
                    (
                        ((x - x0) / w * scale).round() as i32,
                        ((y - y0) / h * scale).round() as i32,
                    )
                })
                .collect(),
        })
        .collect();
    Some(((x0, y0, w, h), built))
}

impl Edit {
    /// Changes the drawing `id` as `change` does: given its lines in
    /// points of its frame, it gives them back changed, and the frame
    /// grows or shrinks to hold them, staying where it was on the page,
    /// turned as it was.
    fn change_drawing_strokes(
        &mut self,
        id: NodeId,
        change: impl FnOnce(Vec<NewStroke>) -> Result<Vec<NewStroke>, ModelError>,
    ) -> Result<(), ModelError> {
        let mut object = self
            .document()
            .objects()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownObject(id))?;
        let ObjectKind::Drawing(drawing) = &object.kind else {
            return Err(ModelError::UnknownObject(id));
        };
        let f = object.frame;
        let (w, h) = (f.width.to_pt(), f.height.to_pt());
        let strokes = drawing
            .strokes
            .iter()
            .map(|s| NewStroke {
                tool: s.tool,
                color: s.color,
                width: s.width,
                points: s.points_in(w, h),
            })
            .collect();
        let strokes = change(strokes)?;
        if strokes.len() > MAX_DRAWING_STROKES
            || strokes.iter().any(|s| s.points.len() > MAX_STROKE_POINTS)
        {
            return Err(ModelError::InvalidText);
        }
        let mut drawing = (**drawing).clone();
        match drawing_frame(&strokes) {
            Some(((bx, by, bw, bh), built)) => {
                // The middle moves by the middle of the lines, turned and
                // flipped as the object is.
                let (mut dx, mut dy) = (bx + bw / 2.0 - w / 2.0, by + bh / 2.0 - h / 2.0);
                if object.flip_horizontal {
                    dx = -dx;
                }
                if object.flip_vertical {
                    dy = -dy;
                }
                let a = (f64::from(object.rotation) / 100.0).to_radians();
                let (dx, dy) = (dx * a.cos() + dy * a.sin(), -dx * a.sin() + dy * a.cos());
                let (cx, cy) = (f.x.to_pt() + w / 2.0 + dx, f.y.to_pt() + h / 2.0 + dy);
                object.frame = Frame::from_pt(cx - bw / 2.0, cy - bh / 2.0, bw, bh);
                drawing.strokes = built.into();
            }
            // A drawing with no lines left keeps its frame.
            None => drawing.strokes = Arc::from([]),
        }
        object.kind = ObjectKind::Drawing(Arc::new(drawing));
        self.replace_object(object)
    }

    /// Adds a line to the drawing `id`, in front of the others.
    pub fn add_drawing_stroke(&mut self, id: NodeId, stroke: NewStroke) -> Result<(), ModelError> {
        if stroke.points.is_empty() {
            return Ok(());
        }
        self.change_drawing_strokes(id, |mut strokes| {
            strokes.push(stroke);
            Ok(strokes)
        })
    }

    /// Takes lines out of the drawing `id`, by their index.
    pub fn remove_drawing_strokes(
        &mut self,
        id: NodeId,
        indices: &[usize],
    ) -> Result<(), ModelError> {
        if indices.is_empty() {
            return Ok(());
        }
        self.change_drawing_strokes(id, |strokes| {
            Ok(strokes
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !indices.contains(i))
                .map(|(_, s)| s)
                .collect())
        })
    }

    /// Fills the area around a point of the frame of the drawing `id` with
    /// a colour, as the fill of Pages does: the area that the frontmost
    /// line closes around it alone, or else the area that several lines
    /// close together. The fill goes behind the lines around it, and an
    /// area filled already takes the new colour. Returns whether lines
    /// close an area there.
    pub fn fill_drawing_at(
        &mut self,
        id: NodeId,
        at: (f64, f64),
        color: Rgba,
    ) -> Result<bool, ModelError> {
        let object = self
            .document()
            .objects()
            .get(id)
            .ok_or(ModelError::UnknownObject(id))?;
        let Some(drawing) = object.drawing() else {
            return Err(ModelError::UnknownObject(id));
        };
        let (w, h) = (object.frame.width.to_pt(), object.frame.height.to_pt());
        let scale = f64::from(POLYGON_SCALE);
        let (area, behind): (Vec<(f64, f64)>, usize) =
            match drawing.strokes.iter().rposition(|s| {
                s.tool != DrawingTool::Fill && s.closes(w, h) && s.encloses(at, w, h)
            }) {
                Some(index) => (drawing.strokes[index].points_in(w, h), index),
                None => match enclosed_area(drawing, at, w, h) {
                    Some(found) => found,
                    None => return Ok(false),
                },
            };
        let parts: Vec<(i32, i32)> = area
            .iter()
            .map(|&(x, y)| {
                (
                    (x / w.max(f64::MIN_POSITIVE) * scale).round() as i32,
                    (y / h.max(f64::MIN_POSITIVE) * scale).round() as i32,
                )
            })
            .collect();
        // An area filled already takes the new colour.
        if let Some(filled) = drawing
            .strokes
            .iter()
            .position(|s| s.tool == DrawingTool::Fill && *s.points == *parts)
        {
            if drawing.strokes[filled].color == color {
                return Ok(true);
            }
            let mut drawing = drawing.clone();
            let mut strokes = drawing.strokes.to_vec();
            strokes[filled].color = color;
            drawing.strokes = strokes.into();
            let mut object = object.clone();
            object.kind = ObjectKind::Drawing(Arc::new(drawing));
            self.replace_object(object)?;
            return Ok(true);
        }
        self.change_drawing_strokes(id, |mut strokes| {
            let fill = NewStroke {
                tool: DrawingTool::Fill,
                color,
                width: Length::ZERO,
                points: area,
            };
            strokes.insert(behind.min(strokes.len()), fill);
            Ok(strokes)
        })?;
        Ok(true)
    }

    /// Animates the drawing `id` as `animation` says, or stops animating
    /// it.
    pub fn set_drawing_animation(
        &mut self,
        id: NodeId,
        animation: Option<DrawingAnimation>,
    ) -> Result<(), ModelError> {
        let mut object = self
            .document()
            .objects()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownObject(id))?;
        let ObjectKind::Drawing(drawing) = &object.kind else {
            return Err(ModelError::UnknownObject(id));
        };
        let animation = animation.map(|a| DrawingAnimation {
            duration: a
                .duration
                .clamp(*ANIMATION_DURATION.start(), *ANIMATION_DURATION.end()),
            ..a
        });
        if drawing.animation == animation {
            return Ok(());
        }
        let mut drawing = (**drawing).clone();
        drawing.animation = animation;
        object.kind = ObjectKind::Drawing(Arc::new(drawing));
        self.replace_object(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, Document, Editor, Object};

    fn line(points: &[(f64, f64)]) -> NewStroke {
        NewStroke {
            tool: DrawingTool::Pen,
            color: Rgba::BLACK,
            width: Length::from_points(2),
            points: points.to_vec(),
        }
    }

    /// A new drawing of one line, as the first line drawn makes it.
    fn with_drawing(first: NewStroke) -> (Editor, NodeId) {
        let mut editor = Editor::new(Document::new_word_processing());
        let id = editor
            .edit("Draw", |edit| {
                let ((x, y, w, h), strokes) = drawing_frame(&[first]).unwrap();
                let drawing = Drawing {
                    strokes: strokes.into(),
                    animation: None,
                };
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::Drawing(Arc::new(drawing)),
                    Frame::from_pt(x, y, w, h),
                );
                object.anchor = Anchor::Page(0);
                let id = object.id;
                edit.add_object(object)?;
                Ok(id)
            })
            .unwrap();
        (editor, id)
    }

    fn object(editor: &Editor, id: NodeId) -> Object {
        editor.document().objects().get(id).cloned().unwrap()
    }

    fn near(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05
    }

    #[test]
    fn w117_a_drawing_grows_to_hold_its_lines_where_they_were_drawn() {
        // A line from (100, 100) to (200, 150) on the page: the frame is
        // its box and half its width around it.
        let (mut editor, id) = with_drawing(line(&[(100.0, 100.0), (200.0, 150.0)]));
        let first = object(&editor, id);
        assert_eq!(first.frame, Frame::from_pt(99.0, 99.0, 102.0, 52.0));
        // A second line, given in points of the frame, goes left of it:
        // the frame grows, and the first line stays where it was.
        editor
            .edit("Draw", |edit| {
                edit.add_drawing_stroke(id, line(&[(-49.0, 1.0), (1.0, 1.0)]))
            })
            .unwrap();
        let grown = object(&editor, id);
        let f = grown.frame;
        let frame = (f.x.to_pt(), f.y.to_pt(), f.width.to_pt(), f.height.to_pt());
        assert!(
            near((frame.0, frame.1), (49.0, 99.0)) && near((frame.2, frame.3), (152.0, 52.0)),
            "{frame:?}"
        );
        let drawing = grown.drawing().unwrap();
        assert_eq!(drawing.strokes.len(), 2);
        let (w, h) = (152.0, 52.0);
        let at = |i: usize| {
            let p = drawing.strokes[i].points_in(w, h);
            p.iter()
                .map(|&(x, y)| (x + 49.0, y + 99.0))
                .collect::<Vec<_>>()
        };
        assert!(near(at(0)[0], (100.0, 100.0)) && near(at(0)[1], (200.0, 150.0)));
        assert!(near(at(1)[0], (50.0, 100.0)));
        // Undo takes the line back, with the frame.
        editor.undo().unwrap();
        assert_eq!(object(&editor, id), first);
    }

    #[test]
    fn w117_a_turned_drawing_keeps_its_lines_in_place() {
        let (mut editor, id) = with_drawing(line(&[(0.0, 0.0), (100.0, 0.0)]));
        editor
            .edit("Rotate", |edit| {
                let mut o = edit.document().objects().get(id).cloned().unwrap();
                o.rotation = 18_000;
                edit.replace_object(o)
            })
            .unwrap();
        let before = object(&editor, id);
        // Turned half round, a line under the frame in its own points is
        // above it on the page.
        editor
            .edit("Draw", |edit| {
                edit.add_drawing_stroke(id, line(&[(1.0, 30.0), (101.0, 30.0)]))
            })
            .unwrap();
        let after = object(&editor, id);
        // The line drawn first is a point from the top of the frame, and
        // the new one 29 points under it.
        assert!((after.frame.height.to_pt() - 31.0).abs() < 1e-6);
        // The frame grew by 29 points, upwards on the page.
        let top = |o: &Object| o.frame.y.to_pt();
        let bottom = |o: &Object| o.frame.y.to_pt() + o.frame.height.to_pt();
        assert!((bottom(&after) - bottom(&before)).abs() < 1e-6);
        assert!((top(&before) - top(&after) - 29.0).abs() < 1e-6);
    }

    #[test]
    fn w117_lines_are_erased_and_areas_filled() {
        let square = [
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (0.0, 1.0),
        ];
        let (mut editor, id) = with_drawing(line(&square));
        // Outside the square nothing is filled.
        let filled = editor
            .edit("Fill", |edit| {
                edit.fill_drawing_at(id, (200.0, 200.0), Rgba::rgb(255, 0, 0))
            })
            .unwrap();
        assert!(!filled);
        let red = Rgba::rgb(255, 0, 0);
        let filled = editor
            .edit("Fill", |edit| edit.fill_drawing_at(id, (50.0, 50.0), red))
            .unwrap();
        assert!(filled);
        let drawing = object(&editor, id).drawing().cloned().unwrap();
        assert_eq!(drawing.strokes.len(), 2);
        // The fill goes behind its line.
        assert_eq!(drawing.strokes[0].tool, DrawingTool::Fill);
        assert_eq!(drawing.strokes[0].color, red);
        assert_eq!(drawing.strokes[0].points, drawing.strokes[1].points);
        // Filling again takes the new colour, with no second fill.
        let blue = Rgba::rgb(0, 0, 255);
        editor
            .edit("Fill", |edit| edit.fill_drawing_at(id, (50.0, 50.0), blue))
            .unwrap();
        let drawing = object(&editor, id).drawing().cloned().unwrap();
        assert_eq!(drawing.strokes.len(), 2);
        assert_eq!(drawing.strokes[0].color, blue);
        // The line is near the edge, not in the middle.
        let (w, h) = {
            let o = object(&editor, id);
            (o.frame.width.to_pt(), o.frame.height.to_pt())
        };
        assert!(drawing.strokes[1].touches((1.0, 50.0), 2.0, w, h));
        assert!(!drawing.strokes[1].touches((50.0, 50.0), 2.0, w, h));
        editor
            .edit("Erase", |edit| edit.remove_drawing_strokes(id, &[1]))
            .unwrap();
        let drawing = object(&editor, id).drawing().cloned().unwrap();
        assert_eq!(drawing.strokes.len(), 1);
        assert_eq!(drawing.strokes[0].tool, DrawingTool::Fill);
    }

    #[test]
    fn w117_an_area_closed_by_several_lines_is_filled() {
        // Two lines, each open, that close a square together; a third
        // line is in front, away from it.
        let (mut editor, id) = with_drawing(line(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]));
        editor
            .edit("Draw", |edit| {
                edit.add_drawing_stroke(id, line(&[(101.0, 101.0), (1.0, 101.0), (1.0, 1.0)]))?;
                edit.add_drawing_stroke(id, line(&[(1.0, 150.0), (101.0, 150.0)]))
            })
            .unwrap();
        let red = Rgba::rgb(255, 0, 0);
        let filled = editor
            .edit("Fill", |edit| edit.fill_drawing_at(id, (50.0, 50.0), red))
            .unwrap();
        assert!(filled);
        let o = object(&editor, id);
        let (w, h) = (o.frame.width.to_pt(), o.frame.height.to_pt());
        let drawing = o.drawing().cloned().unwrap();
        assert_eq!(drawing.strokes.len(), 4);
        // Behind both lines around it.
        assert_eq!(drawing.strokes[0].tool, DrawingTool::Fill);
        let fill = &drawing.strokes[0];
        assert!(fill.encloses((50.0, 50.0), w, h));
        assert!(fill.encloses((5.0, 95.0), w, h));
        assert!(!fill.encloses((50.0, 130.0), w, h));
        // Filled again, it takes the new colour.
        let blue = Rgba::rgb(0, 0, 255);
        editor
            .edit("Fill", |edit| edit.fill_drawing_at(id, (60.0, 40.0), blue))
            .unwrap();
        let drawing = object(&editor, id).drawing().cloned().unwrap();
        assert_eq!(drawing.strokes.len(), 4);
        assert_eq!(drawing.strokes[0].color, blue);
        // Below the square, the area is open.
        let filled = editor
            .edit("Fill", |edit| edit.fill_drawing_at(id, (50.0, 130.0), red))
            .unwrap();
        assert!(!filled);
    }

    #[test]
    fn w117_the_animation_draws_the_lines_in_their_order() {
        let (mut editor, id) = with_drawing(line(&[(0.0, 0.0), (100.0, 0.0)]));
        editor
            .edit("Draw", |edit| {
                edit.add_drawing_stroke(id, line(&[(1.0, 51.0), (101.0, 51.0)]))
            })
            .unwrap();
        let o = object(&editor, id);
        let (w, h) = (o.frame.width.to_pt(), o.frame.height.to_pt());
        let drawing = o.drawing().unwrap();
        assert!(drawing.drawn_to(0.0, w, h).strokes.is_empty());
        let half = drawing.drawn_to(0.5, w, h);
        assert_eq!(half.strokes.len(), 1);
        assert_eq!(half.strokes[0], drawing.strokes[0]);
        let quarter = drawing.drawn_to(0.25, w, h);
        let points = quarter.strokes[0].points_in(w, h);
        assert!((points[1].0 - points[0].0 - 50.0).abs() < 0.1, "{points:?}");
        assert_eq!(drawing.drawn_to(1.0, w, h), *drawing);
        // The time is kept within its bounds.
        editor
            .edit("Animate Drawing", |edit| {
                edit.set_drawing_animation(
                    id,
                    Some(DrawingAnimation {
                        duration: 10,
                        repeat: true,
                    }),
                )
            })
            .unwrap();
        let animation = object(&editor, id).drawing().unwrap().animation.unwrap();
        assert_eq!(animation.duration, 100);
        assert!(animation.repeat);
    }

    #[test]
    fn w117_a_fill_shows_when_the_lines_before_it_are_drawn() {
        let (mut editor, id) = with_drawing(line(&[
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 1.0),
        ]));
        editor
            .edit("Fill", |edit| {
                edit.fill_drawing_at(id, (90.0, 50.0), Rgba::BLACK)
            })
            .unwrap();
        let o = object(&editor, id);
        let (w, h) = (o.frame.width.to_pt(), o.frame.height.to_pt());
        let drawing = o.drawing().unwrap();
        // The fill is first: it shows as soon as anything is drawn.
        assert_eq!(drawing.drawn_to(0.01, w, h).strokes.len(), 2);
        assert!(drawing.drawn_to(0.0, w, h).strokes.is_empty());
    }
}
