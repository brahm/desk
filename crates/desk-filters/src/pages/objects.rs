// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The floating objects of a Pages document as objects of the model:
//! shapes, text boxes and groups, on pages and on the masters of the
//! sections (OBJ-08, OBJ-13, OBJ-30, INT-05).

use std::collections::HashMap;
use std::sync::Arc;

use desk_iwork::pages::{self as iwork, Drawable, DrawableKind, Place, ShapePath};
use desk_iwork::sf::{StrokePattern, Value};
use desk_model::{
    Anchor, BorderStyle, Fill, Frame, GraphicStyle, Length, LineEnd, MAX_SHAPE_SIDES, NodeId,
    Object, ObjectKind, Op, POLYGON_SCALE, PathPoint, Shape, Stroke, TextBox, VerticalAlignment,
    Wrap,
};

use super::{Importer, rgba};
use crate::FilterError;

/// A part of [`POLYGON_SCALE`], from a part of 1.
fn part(v: f64) -> u16 {
    (v * f64::from(POLYGON_SCALE))
        .round()
        .clamp(0.0, f64::from(POLYGON_SCALE)) as u16
}

/// The shape of the model for an outline of Pages, in an object of
/// natural size `natural`.
/// A line that Pages draws from right to left is drawn from left to right
/// by the model: the second is whether the ends of its line swap.
fn shape(path: &ShapePath, natural: (f64, f64)) -> (Shape, bool) {
    let (w, h) = natural;
    let shape = match path {
        ShapePath::Bezier(path) => {
            let Some(Shape::Path { points, closed }) =
                crate::odt::objects::path_shape(path, Some((0.0, 0.0, w, h)))
            else {
                return (Shape::Rectangle, false);
            };
            let mut points = points.to_vec();
            let mut closed = closed;
            // Pages ends a closed curve where it starts, without closing it.
            if !closed && points.len() > 2 && points[0].at == points[points.len() - 1].at {
                let last = points.pop().expect("more than two points");
                points[0].before = last.before;
                closed = true;
            }
            if let [a, b] = points.as_slice()
                && !closed
                && !a.is_curve()
                && !b.is_curve()
            {
                let rising = (b.at.0 - a.at.0) * (b.at.1 - a.at.1) < 0;
                return (Shape::Line { rising }, a.at.0 > b.at.0);
            }
            plain_shape(&points, closed).unwrap_or(Shape::Path {
                points: points.into(),
                closed,
            })
        }
        ShapePath::Scalar { kind: 0, scalar } => Shape::RoundedRectangle {
            radius: Length::from_pt(scalar.clamp(0.0, w.min(h) / 2.0)),
        },
        ShapePath::Scalar { scalar, .. } => Shape::RegularPolygon {
            sides: scalar.clamp(3.0, f64::from(MAX_SHAPE_SIDES)) as u8,
        },
        ShapePath::Point { kind, point } if kind == "star" => Shape::Star {
            points: point.0.clamp(3.0, f64::from(MAX_SHAPE_SIDES)) as u8,
            inner: part(point.1),
        },
        ShapePath::Point { kind, point } => {
            let double = kind == "double";
            // Each head of a double arrow is as long as the one of an
            // arrow; the model measures it against the whole width.
            Shape::Arrow {
                double,
                head: part(if w > 0.0 { point.0 / w } else { 0.5 }),
                tail: part(point.1),
            }
        }
        ShapePath::Callout { radius } => match Shape::balloon(true) {
            Shape::Balloon { body, tip, .. } => Shape::Balloon {
                radius: Length::from_pt(radius.clamp(0.0, w.min(h) / 2.0)),
                body,
                tip,
            },
            other => other,
        },
        ShapePath::Other(_) => Shape::Rectangle,
    };
    (shape, false)
}

/// The plain shape that a closed path is: a rectangle, a polygon or an
/// ellipse.
fn plain_shape(points: &[PathPoint], closed: bool) -> Option<Shape> {
    let s = POLYGON_SCALE;
    if !closed {
        return None;
    }
    if points.iter().all(PathPoint::is_curve) {
        // Pages draws an ellipse with four curves.
        return (points.len() == 4).then_some(Shape::Ellipse);
    }
    if points.iter().any(PathPoint::is_curve) {
        return None;
    }
    let corners: Vec<(i32, i32)> = points.iter().map(|p| p.at).collect();
    let mut rectangle = [(0, 0), (s, 0), (s, s), (0, s)];
    for _ in 0..4 {
        if corners == rectangle {
            return Some(Shape::Rectangle);
        }
        rectangle.rotate_left(1);
    }
    Some(Shape::Polygon(corners.into()))
}

/// What an end of a line ends in, by the name Pages gives it.
fn line_end(value: Option<&Value>) -> LineEnd {
    let Some(Value::LineEnd(name)) = value else {
        return LineEnd::None;
    };
    let name = name.to_lowercase();
    if name.contains("circle") {
        LineEnd::Circle
    } else if name.contains("square") || name.contains("diamond") {
        LineEnd::Square
    } else if name.contains("arrow") && !name.contains("filled") {
        LineEnd::OpenArrow
    } else if name.contains("arrow") {
        LineEnd::Arrow
    } else if name.contains("bar") || name.contains("line") {
        LineEnd::Bar
    } else {
        LineEnd::None
    }
}

/// The changes of Adjust Image, from the filter properties of Pages,
/// which go from -100 to 100 as the model's do.
fn adjustments(properties: &[(String, f64)]) -> desk_model::Adjustments {
    let get = |name: &str| {
        properties
            .iter()
            .find(|(n, _)| n == name)
            .map_or(0, |(_, v)| v.round().clamp(-100.0, 100.0) as i8)
    };
    desk_model::Adjustments {
        brightness: get("kSFRBrightnessProperty"),
        contrast: get("kSFRContrastProperty"),
        saturation: get("kSFRSaturationProperty"),
        exposure: get("kSFRExposureProperty"),
        temperature: get("kSFRTemperature"),
        tint: get("kSFRTint"),
    }
}

/// The style of the model for the properties of a graphic style.
fn graphic_style(
    drawable: &Drawable,
    picture: impl Fn(&str) -> Option<desk_model::Picture>,
) -> GraphicStyle {
    let properties = &drawable.style;
    let fill = match properties.get("fill") {
        // A picture that fills it (OBJ-42).
        Some(Value::TexturedFill {
            technique,
            path,
            tint,
        }) => picture(path).map(|picture| Fill::Image {
            picture,
            mode: match technique.as_str() {
                "fit" => desk_model::ImageFill::ScaleToFit,
                "stretch" => desk_model::ImageFill::Stretch,
                "natural" => desk_model::ImageFill::OriginalSize,
                "tile" => desk_model::ImageFill::Tile,
                _ => desk_model::ImageFill::ScaleToFill,
            },
            tint: tint.map(rgba),
        }),
        Some(Value::Color(color)) => Some(Fill::Color(rgba(*color))),
        Some(Value::Gradient {
            angle,
            stops,
            inflections,
            center,
        }) => {
            let angle = (angle.to_degrees() * 100.0).round().rem_euclid(36_000.0) as i32;
            let plain = stops.len() == 2
                && center.is_none()
                && stops[0].0 == 0.0
                && stops[1].0 == 1.0
                && inflections.first().is_none_or(|i| (i - 0.5).abs() < 1e-3);
            let part = |v: f64| (v * 1000.0).round().clamp(0.0, 1000.0) as u16;
            match (stops.first(), stops.last()) {
                (Some(from), Some(to)) if plain => Some(Fill::Gradient {
                    from: rgba(from.1),
                    to: rgba(to.1),
                    angle,
                }),
                (Some(only), Some(_)) if stops.len() == 1 => Some(Fill::Color(rgba(only.1))),
                (Some(_), Some(_)) => Some(Fill::Advanced(desk_model::Gradient {
                    stops: stops
                        .iter()
                        .zip(inflections.iter().copied().chain(std::iter::repeat(0.5)))
                        .map(|(&(at, color), blend)| desk_model::GradientStop {
                            blend: part(blend),
                            ..desk_model::GradientStop::new(part(at), rgba(color))
                        })
                        .collect(),
                    angle,
                    center: center.map(|(x, y)| (part(x), part(y))),
                })),
                _ => None,
            }
        }
        _ => None,
    };
    let stroke = match properties.get("stroke") {
        Some(Value::Stroke(stroke))
            if stroke.width > 0.0 && stroke.pattern != StrokePattern::Empty =>
        {
            Some(Stroke {
                style: match stroke.pattern {
                    StrokePattern::Dashes(_) => BorderStyle::Dashed,
                    _ => BorderStyle::Solid,
                },
                start: line_end(properties.get("tailLineEnd")),
                end: line_end(properties.get("headLineEnd")),
                ..Stroke::new(rgba(stroke.color), Length::from_pt(stroke.width))
            })
        }
        _ => None,
    };
    GraphicStyle {
        fill,
        stroke,
        opacity: properties
            .number("opacity")
            .map_or(100, |o| (o * 100.0).round().clamp(0.0, 100.0) as u8),
        shadow: None,
        reflection: None,
        frame: match properties.get("stroke") {
            Some(Value::Frame { name, scale }) => Some(desk_model::PictureFrame {
                style: frame_style(name),
                scale: (scale * 100.0).round().clamp(10.0, 400.0) as u16,
            }),
            _ => None,
        },
    }
}

/// The pages of its section that an object of a master shows on, by the
/// group of Pages it is in (EST-19): the first page, the even pages,
/// which are the left ones, or the others.
fn master_pages(group: &str) -> desk_model::MarginPages {
    if group.starts_with("first") {
        desk_model::MarginPages::First
    } else if group.starts_with("even") || group.starts_with("left") {
        desk_model::MarginPages::Left
    } else {
        desk_model::MarginPages::Default
    }
}

/// The frame of the library of Desk most like a frame of Pages, by its
/// name (OBJ-36).
fn frame_style(name: &str) -> desk_model::FrameStyle {
    use desk_model::FrameStyle;
    let name = name.to_lowercase();
    if ["wood", "classic", "baroque", "ornate", "vintage"]
        .iter()
        .any(|n| name.contains(n))
    {
        FrameStyle::Classic
    } else if ["metal", "silver", "chrome", "steel"]
        .iter()
        .any(|n| name.contains(n))
    {
        FrameStyle::Metal
    } else if ["tape", "scrapbook", "pushpin"]
        .iter()
        .any(|n| name.contains(n))
    {
        FrameStyle::Tape
    } else if ["gallery", "museum", "mat", "modern"]
        .iter()
        .any(|n| name.contains(n))
    {
        FrameStyle::Gallery
    } else {
        FrameStyle::Photo
    }
}

/// The contour of a picture, in parts of its frame `shown`, from the
/// `traced` outline of its opaque part in its `natural` size, the picture
/// filling `whole`.
fn traced_outline(
    traced: &str,
    natural: (f64, f64),
    whole: Frame,
    shown: Frame,
) -> Option<Arc<[(i32, i32)]>> {
    let (nw, nh) = natural;
    if nw <= 0.0 || nh <= 0.0 {
        return None;
    }
    let outline = crate::odt::objects::path_shape(traced, Some((0.0, 0.0, nw, nh)))?;
    let outline = Object::new(
        desk_model::NodeId::new(1).expect("not zero"),
        ObjectKind::Shape(outline),
        whole,
    );
    let points = outline.shape_contour()?;
    if whole == shown {
        return Some(points);
    }
    // A mask shows a part of the picture: the contour is placed in it,
    // and kept inside it.
    let (sw, sh) = (shown.width.to_pt(), shown.height.to_pt());
    if sw <= 0.0 || sh <= 0.0 {
        return None;
    }
    let scale = f64::from(POLYGON_SCALE);
    let dx = (whole.x - shown.x).to_pt();
    let dy = (whole.y - shown.y).to_pt();
    let (ww, wh) = (whole.width.to_pt(), whole.height.to_pt());
    let placed: Vec<(i32, i32)> = points
        .iter()
        .map(|&(x, y)| {
            let x = (dx + f64::from(x) / scale * ww) / sw * scale;
            let y = (dy + f64::from(y) / scale * wh) / sh * scale;
            (
                x.round().clamp(0.0, scale) as i32,
                y.round().clamp(0.0, scale) as i32,
            )
        })
        .collect();
    Some(placed.into())
}

/// How text goes around an object.
fn wrap(drawable: &Drawable) -> (Wrap, Length) {
    let margin = drawable.wrap.as_ref().map_or(Length::from_points(12), |w| {
        Length::from_pt(w.margin.max(0.0))
    });
    // An object sent to the background is behind the text (OBJ-25).
    if drawable.order < 0 {
        return (Wrap::Behind, margin);
    }
    let Some(wrap) = &drawable.wrap else {
        return (Wrap::Both, margin);
    };
    let kind = if !wrap.enabled {
        Wrap::Through
    } else if wrap.kind == "largest" {
        Wrap::Largest
    } else if wrap.kind == "neither" {
        Wrap::AboveAndBelow
    } else {
        match wrap.direction.as_str() {
            "left" => Wrap::Left,
            "right" => Wrap::Right,
            _ => Wrap::Both,
        }
    };
    (kind, margin)
}

/// Builds the objects of the model, with the ids that the text boxes
/// link to each other by.
struct Builder<'a, 'b> {
    importer: &'b mut Importer<'a>,
    source: &'b iwork::PagesDocument,
    /// The ids of the objects, by the object IDs of Pages.
    ids: HashMap<String, NodeId>,
}

impl Builder<'_, '_> {
    /// Gives text the inset, columns and vertical alignment of the layout
    /// style of the text of an object.
    fn lay_out(&self, text_box: &mut TextBox, drawable: &Drawable, in_shape: bool) {
        let sheet = self.importer.sheet;
        let Some(style) = drawable.text_layout.as_deref().and_then(|l| sheet.find(l)) else {
            return;
        };
        let chain = sheet.ancestry(style);
        let get = |name: &str| chain.iter().find_map(|s| s.properties.get(name));
        // Pages lays the text of a text box out from the edges of its
        // frame: the inset of 4 pt of the default layout style of text
        // boxes is not drawn (FMT-13, OBJ-13). Shapes keep theirs.
        let padding = chain
            .iter()
            .find(|s| s.properties.get("padding").is_some())
            .filter(|s| {
                in_shape || s.ident.as_deref() != Some("graphic-textbox-layout-style-default")
            })
            .and_then(|s| s.properties.get("padding"));
        if let Some(Value::Padding { left, .. }) = padding {
            text_box.inset = Length::from_pt(left.max(0.0));
        }
        if let Some(Value::Number(v)) = get("verticalAlignment") {
            text_box.vertical = match *v as i64 {
                1 => VerticalAlignment::Middle,
                2 => VerticalAlignment::Bottom,
                _ => VerticalAlignment::Top,
            };
        }
        if let Some(Value::Columns { columns, .. }) = get("columns")
            && !columns.is_empty()
        {
            text_box.columns = u8::try_from(columns.len())
                .unwrap_or(u8::MAX)
                .clamp(1, desk_model::MAX_TEXT_BOX_COLUMNS);
            // The gap, a part of the width of the text.
            if let Some(&(_, gap)) = columns.first().filter(|_| columns.len() > 1) {
                let (w, _) = drawable.geometry.size;
                let width = (w - 2.0 * text_box.inset.to_pt()).max(0.0);
                text_box.column_gap = Length::from_pt(gap * width);
            }
        }
    }

    /// The picture of a file of the package, when it can be read.
    fn picture(&self, path: &str) -> Option<desk_model::Picture> {
        self.importer
            .pictures
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, picture)| picture.clone())
    }

    /// The object for a drawable, placed from `origin`, or none for one
    /// that the import leaves out.
    fn object(
        &mut self,
        drawable: &Drawable,
        anchor: Anchor,
        origin: (f64, f64),
    ) -> Result<Option<Object>, FilterError> {
        let geometry = &drawable.geometry;
        let (w, h) = geometry.size;
        let (cx, cy) = geometry.center();
        let mut frame = Frame::from_pt(
            origin.0 + cx - w / 2.0,
            origin.1 + cy - h / 2.0,
            w.max(0.0),
            h.max(0.0),
        );
        let mut picture_settings = desk_model::PictureSettings::default();
        let id = match drawable.id.as_ref().and_then(|i| self.ids.get(i)) {
            Some(&id) => id,
            None => self.importer.document.new_id(),
        };
        let mut text = Vec::new();
        let mut swap_ends = false;
        let mut traced_contour = None;
        let kind = match &drawable.kind {
            DrawableKind::Shape {
                path,
                text_box,
                text: paragraphs,
                next,
            } => {
                for paragraph in paragraphs {
                    text.extend(self.importer.paragraph(paragraph, None)?);
                }
                if *text_box {
                    if text.is_empty() {
                        text.push(desk_model::Paragraph::new(
                            self.importer.document.new_id(),
                            self.importer.body,
                        ));
                    }
                    let mut text_box = TextBox::new(std::mem::take(&mut text).into(), Length::ZERO);
                    text_box.next = next.as_ref().and_then(|n| self.ids.get(n)).copied();
                    self.lay_out(&mut text_box, drawable, false);
                    ObjectKind::TextBox(text_box)
                } else {
                    let (shape, reversed) = shape(path, geometry.natural);
                    swap_ends = reversed;
                    ObjectKind::Shape(shape)
                }
            }
            DrawableKind::Group(children) => {
                // The objects of a group are placed from its top left
                // corner, and kept on the page.
                let origin = (
                    origin.0 + geometry.position.0,
                    origin.1 + geometry.position.1,
                );
                let mut objects = Vec::new();
                for child in children {
                    if let Some(object) = self.object(child, anchor, origin)? {
                        objects.push(object);
                    }
                }
                if objects.is_empty() {
                    return Ok(None);
                }
                ObjectKind::Group(objects.into())
            }
            DrawableKind::Image {
                path,
                mask,
                mask_path,
                adjust,
                traced,
            } => {
                let Some(picture) = self.picture(path) else {
                    return Ok(None);
                };
                let whole = frame;
                // Masked, the object is the part that shows, and the
                // picture lies around it.
                if let Some(mask) = mask {
                    let (mw, mh) = mask.size;
                    let (mx, my) = mask.center();
                    let shown = Frame::from_pt(
                        origin.0 + mx - mw / 2.0,
                        origin.1 + my - mh / 2.0,
                        mw.max(0.0),
                        mh.max(0.0),
                    );
                    picture_settings.mask = Some(Frame {
                        x: frame.x - shown.x,
                        y: frame.y - shown.y,
                        ..frame
                    });
                    frame = shown;
                    // A mask of another shape than a rectangle (OBJ-04).
                    if let Some(path) = mask_path {
                        let (outline, _) = shape(path, mask.natural);
                        if !matches!(outline, Shape::Rectangle | Shape::Line { .. }) {
                            picture_settings.mask_shape = Some(outline);
                        }
                    }
                }
                picture_settings.adjust = adjustments(adjust);
                traced_contour = traced
                    .as_deref()
                    .and_then(|t| traced_outline(t, geometry.natural, whole, frame));
                ObjectKind::Image(picture)
            }
            DrawableKind::Media {
                path,
                poster,
                looping,
                volume,
            } => {
                let Some(mut media) = self
                    .source
                    .file(path)
                    .and_then(|bytes| desk_model::Media::from_bytes(bytes.to_vec()))
                else {
                    return Ok(None);
                };
                media.poster = poster.as_deref().and_then(|p| self.picture(p));
                media.repeat = match looping.as_str() {
                    "loop" => desk_model::Repeat::Loop,
                    "palindrome" => desk_model::Repeat::BackAndForth,
                    _ => desk_model::Repeat::None,
                };
                media.volume = (volume * 100.0).round().clamp(0.0, 100.0) as u8;
                ObjectKind::Media(media)
            }
            DrawableKind::Other(_) => return Ok(None),
        };
        let (wrap, wrap_margin) = wrap(drawable);
        let mut object = Object::new(id, kind, frame);
        object.anchor = anchor;
        object.rotation = (geometry.angle * 100.0).round().rem_euclid(36_000.0) as i32;
        object.flip_horizontal = geometry.flip_horizontal;
        object.flip_vertical = geometry.flip_vertical;
        object.style = graphic_style(drawable, |path| self.picture(path));
        if swap_ends && let Some(stroke) = &mut object.style.stroke {
            std::mem::swap(&mut stroke.start, &mut stroke.end);
        }
        object.wrap = wrap;
        object.wrap_margin = wrap_margin;
        if drawable.wrap.as_ref().is_some_and(|w| w.tight)
            && matches!(wrap, Wrap::Both | Wrap::Left | Wrap::Right | Wrap::Largest)
        {
            // Around the outline of a shape, or the opaque part of a
            // picture (OBJ-17).
            object.contour = match &object.kind {
                ObjectKind::Image(_) => traced_contour,
                _ => object.shape_contour(),
            };
        }
        object.locked = drawable.locked;
        object.picture = picture_settings;
        object.style.reflection = drawable
            .reflection
            .map(|r| (r * 100.0).round().clamp(0.0, 100.0) as u8);
        object.style.shadow = drawable.shadow.map(|shadow| desk_model::ObjectShadow {
            color: rgba(shadow.color),
            // Pages turns it clockwise, the model counterclockwise.
            angle: (-shadow.angle * 100.0).round().rem_euclid(36_000.0) as i32,
            offset: Length::from_pt(shadow.distance.max(0.0)),
            blur: Length::from_pt(shadow.radius.max(0.0)),
            opacity: (shadow.opacity * 100.0).round().clamp(0.0, 100.0) as u8,
        });
        if object.holds_text() && !text.iter().all(desk_model::Paragraph::is_empty) {
            // Pages centers the text of a shape that does not say how it
            // is aligned.
            let centered = text
                .into_iter()
                .map(|p| {
                    if p.attrs().alignment.is_some() {
                        return Ok(p);
                    }
                    let attrs = desk_model::ParaAttrs {
                        alignment: Some(desk_model::Alignment::Center),
                        ..p.attrs().clone()
                    };
                    desk_model::Paragraph::from_runs(p.id(), p.style(), attrs, p.runs().cloned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut text_box = TextBox::in_shape(Arc::from(centered));
            self.lay_out(&mut text_box, drawable, true);
            object.text = Some(text_box);
        }
        Ok(Some(object))
    }
}

/// Puts the floating objects of a document in it: those of the pages on
/// their pages, and those of the masters on the masters of their
/// sections, by the paragraphs that start the sections with headers of
/// their own. The first section is that of `first`. Then puts the objects
/// in the lines of the body where their marks are. Returns how many
/// floating objects and objects in the text it left out.
pub(super) fn import_objects(
    importer: &mut Importer<'_>,
    source: &iwork::PagesDocument,
    first: Option<&str>,
    sections: &HashMap<&str, NodeId>,
) -> Result<(usize, usize), FilterError> {
    let mut drawables: Vec<&Drawable> = source.drawables.iter().collect();
    // From back to front: the masters behind the pages, the objects behind
    // the text first, then by their order.
    drawables.sort_by_key(|d| match d.place {
        Place::Master { background, .. } => (0, i32::from(!background), d.order),
        Place::Page(page) => (1, i32::try_from(page).unwrap_or(i32::MAX), d.order),
    });
    let mut builder = Builder {
        importer,
        source,
        ids: HashMap::new(),
    };
    for drawable in &drawables {
        if let Some(id) = &drawable.id {
            let node = builder.importer.document.new_id();
            builder.ids.insert(id.clone(), node);
        }
    }
    let mut left_out = 0;
    for drawable in drawables {
        let anchor = match &drawable.place {
            Place::Page(page) => Anchor::Page(*page),
            Place::Master { section, pages, .. } if Some(section.as_str()) == first => {
                Anchor::Master(None, master_pages(pages))
            }
            Place::Master { section, pages, .. } => match sections.get(section.as_str()) {
                Some(&id) => Anchor::Master(Some(id), master_pages(pages)),
                None => {
                    left_out += 1;
                    continue;
                }
            },
        };
        let Some(mut object) = builder.object(drawable, anchor, (0.0, 0.0))? else {
            // The reader counts the objects it does not read; these are a
            // picture or a movie whose file cannot be read.
            if matches!(
                drawable.kind,
                DrawableKind::Image { .. } | DrawableKind::Media { .. }
            ) {
                left_out += 1;
            }
            continue;
        };
        if matches!(
            drawable.place,
            Place::Master {
                background: true,
                ..
            }
        ) {
            object.wrap = Wrap::Behind;
        }
        let index = builder.importer.document.objects().len();
        builder
            .importer
            .document
            .apply(Op::InsertObject { index, object })?;
    }
    // The objects in the lines of the body, each where its mark is.
    let inline = std::mem::take(&mut builder.importer.inline);
    let mut inline_left_out = 0;
    for (id, attachment) in inline {
        let built = source
            .attachments
            .iter()
            .find(|(a, _)| *a == attachment)
            .map(|(_, d)| builder.object(d, Anchor::InText, (0.0, 0.0)))
            .transpose()?
            .flatten();
        let Some(mut object) = built else {
            inline_left_out += 1;
            remove_mark(&mut builder.importer.document, id)?;
            continue;
        };
        object.id = id;
        object.frame.x = Length::ZERO;
        object.frame.y = Length::ZERO;
        object.rotation = 0;
        object.wrap = Wrap::Both;
        let index = builder.importer.document.objects().len();
        builder
            .importer
            .document
            .apply(Op::InsertObject { index, object })?;
    }
    Ok((left_out, inline_left_out))
}

/// Takes out of the body the mark of an object that is left out.
fn remove_mark(document: &mut desk_model::Document, id: NodeId) -> Result<(), FilterError> {
    let found = document.body().iter().find_map(|paragraph| {
        let mut offset = 0;
        for run in paragraph.runs() {
            if let Some(Some(desk_model::Field::ObjectMark { object })) = &run.attrs.field
                && *object == id
            {
                return Some((paragraph.id(), offset..offset + run.len()));
            }
            offset += run.len();
        }
        None
    });
    if let Some((paragraph, range)) = found {
        document.apply(Op::ReplaceText {
            paragraph,
            range,
            runs: Vec::new(),
        })?;
        return Ok(());
    }
    // A mark in a header or a footer.
    let Some((desk_model::Story::Margin { section, place }, at)) =
        document.story_object_marks().get(&id).copied()
    else {
        return Ok(());
    };
    let Some(paragraphs) = document
        .all_headers_footers()
        .into_iter()
        .find(|(s, _)| *s == section)
        .map(|(_, margins)| margins.get(place).to_vec())
    else {
        return Ok(());
    };
    let mark = |run: &desk_model::Run| matches!(&run.attrs.field, Some(Some(desk_model::Field::ObjectMark { object })) if *object == id);
    let paragraphs = paragraphs
        .into_iter()
        .map(|p| {
            if p.id() != at.paragraph {
                return Ok(p);
            }
            let runs: Vec<desk_model::Run> = p.runs().filter(|r| !mark(r)).cloned().collect();
            desk_model::Paragraph::from_runs(p.id(), p.style(), p.attrs().clone(), runs)
        })
        .collect::<Result<Vec<_>, _>>()?;
    document.apply(Op::SetHeaderFooter {
        section,
        place,
        paragraphs,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_17_the_traced_outline_of_a_picture_is_placed_in_its_mask() {
        let whole = Frame::from_pt(0.0, 0.0, 100.0, 50.0);
        // A triangle over the picture, in its natural size of 200 by 100.
        let traced = "M 0 0 L 200 0 L 100 100 Z";
        let outline = traced_outline(traced, (200.0, 100.0), whole, whole).unwrap();
        assert_eq!(&*outline, &[(0, 0), (10_000, 0), (5_000, 10_000)]);
        // Its right half shows: the outline is kept inside it.
        let shown = Frame::from_pt(50.0, 0.0, 50.0, 50.0);
        let outline = traced_outline(traced, (200.0, 100.0), whole, shown).unwrap();
        assert_eq!(&*outline, &[(0, 0), (10_000, 0), (0, 10_000)]);
        assert!(traced_outline("M 0 0 L 1 1", (0.0, 1.0), whole, whole).is_none());
    }
}
