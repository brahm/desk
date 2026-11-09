// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The floating objects of a Pages document (`sl:drawables`): those of
//! each page, in `sl:page-group`, and those of the masters of each
//! section, in `sl:section-drawables`, in front of the text or behind it.
//!
//! An object is placed by the box around it as it is drawn, turned:
//! `sf:position` is the top left corner of that box and `sf:size` the
//! size of the object before it is turned by `sf:angle`. The objects of a
//! group are placed from the top left corner of the group.

use std::collections::HashMap;

use super::{PagesDocument, Paragraph, read_block};
use crate::ns::{SF, SFA, SL};
use crate::sf::{Ids, Properties};
use crate::xml::Element;

/// Where a floating object is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// On a page, from 0, counting every page of the document.
    Page(u32),
    /// On the master of a section, by the name of the section style, on
    /// all its pages: in front of the text, or behind it.
    Master {
        /// The style of the section.
        section: String,
        /// Whether it is behind the text (`sl:backgroundmaster-group`).
        background: bool,
        /// The local name of the group of the pages it shows on, as
        /// `allpages-group`, `firstpage-group` or `evenpage-group`.
        pages: String,
    },
}

/// The size and place of an object (`sf:geometry`), in points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Geometry {
    /// `sf:naturalSize`: the size its path is drawn at.
    pub natural: (f64, f64),
    /// `sf:size`: its size before it is turned.
    pub size: (f64, f64),
    /// `sf:position`: the top left corner of the box around it as it is
    /// drawn, from the top left corner of the page or of its group.
    pub position: (f64, f64),
    /// `sf:angle`, in degrees counterclockwise.
    pub angle: f64,
    /// `sf:horizontalFlip`.
    pub flip_horizontal: bool,
    /// `sf:verticalFlip`.
    pub flip_vertical: bool,
}

impl Geometry {
    fn read(element: &Element) -> Self {
        let pair = |name, a, b| {
            element
                .child(SF, name)
                .map(|e| (number(e, SFA, a), number(e, SFA, b)))
                .unwrap_or_default()
        };
        let size = pair("size", "w", "h");
        let natural = element
            .child(SF, "naturalSize")
            .map_or(size, |e| (number(e, SFA, "w"), number(e, SFA, "h")));
        Self {
            natural,
            size,
            position: pair("position", "x", "y"),
            angle: number(element, SF, "angle"),
            flip_horizontal: element.attr(SF, "horizontalFlip") == Some("true"),
            flip_vertical: element.attr(SF, "verticalFlip") == Some("true"),
        }
    }

    /// The middle of the object, which it turns around.
    pub fn center(&self) -> (f64, f64) {
        let (w, h) = self.size;
        let angle = self.angle.to_radians();
        let (cos, sin) = (angle.cos().abs(), angle.sin().abs());
        (
            self.position.0 + (w * cos + h * sin) / 2.0,
            self.position.1 + (w * sin + h * cos) / 2.0,
        )
    }
}

/// The outline of a shape (`sf:path`).
#[derive(Clone, Debug, PartialEq)]
pub enum ShapePath {
    /// `sf:bezier-path` or `sf:editable-bezier-path`: the path, in the
    /// units of the natural size, with `M`, `L`, `C` and `Z`.
    Bezier(String),
    /// `sf:scalar-path`: a rounded rectangle (`sf:type` 0, with the radius
    /// of its corners) or a regular polygon (1, with its sides).
    Scalar {
        /// `sf:type`.
        kind: u32,
        /// `sf:scalar`.
        scalar: f64,
    },
    /// `sf:point-path`: an arrow (`right`, with how long its head is and
    /// how thick its tail, as a part of the height), a double arrow
    /// (`double`) or a star (`star`, with its points and how far in its
    /// inner corners are, as a part of the radius).
    Point {
        /// `sf:type`.
        kind: String,
        /// `sf:point`.
        point: (f64, f64),
    },
    /// `sf:callout2-path`: a speech balloon, with the radius of its
    /// corners.
    Callout {
        /// `sf:cornerRadius`.
        radius: f64,
    },
    /// A path the reader does not know, by its element.
    Other(String),
}

impl ShapePath {
    fn read(path: &Element) -> Self {
        let Some(element) = path.elements().next() else {
            return Self::Other(String::new());
        };
        match element.name.local.as_str() {
            "bezier-path" | "editable-bezier-path" => element
                .child(SF, "bezier")
                .and_then(|b| b.attr(SFA, "path"))
                .map_or_else(|| Self::Other("bezier".into()), |p| Self::Bezier(p.into())),
            "scalar-path" => Self::Scalar {
                kind: number(element, SF, "type") as u32,
                scalar: number(element, SF, "scalar"),
            },
            "point-path" => Self::Point {
                kind: element.attr(SF, "type").unwrap_or_default().to_owned(),
                point: element
                    .child(SF, "point")
                    .map(|p| (number(p, SFA, "x"), number(p, SFA, "y")))
                    .unwrap_or_default(),
            },
            "callout2-path" => Self::Callout {
                radius: number(element, SF, "cornerRadius"),
            },
            other => Self::Other(other.to_owned()),
        }
    }
}

/// What a floating object is.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawableKind {
    /// A shape (`sf:drawable-shape`), or a text box: a shape that can be
    /// linked (`sf:linkable`).
    Shape {
        /// Its outline.
        path: ShapePath,
        /// Whether it is a text box.
        text_box: bool,
        /// The text inside it. A text box linked from others holds none:
        /// the first of the boxes holds the text of them all.
        text: Vec<Paragraph>,
        /// The object ID of the next box linked from this text box.
        next: Option<String>,
    },
    /// A group of objects (`sf:group`), placed from its top left corner.
    Group(Vec<Drawable>),
    /// A picture (`sf:media` with `sf:image-media`), by the path of its
    /// file in the package.
    Image {
        /// `sf:data sf:path` of the picture as it was put in.
        path: String,
        /// `sf:crop-geometry`: the part of the picture that shows, when a
        /// mask hides the rest, placed as the picture is.
        mask: Option<Geometry>,
        /// `sf:masking-shape-path-source`: the outline of the mask, in
        /// the units of its natural size, when Mask with Shape made it.
        mask_path: Option<ShapePath>,
        /// The changes of Adjust Image (`sf:filter-properties`), by name,
        /// as `kSFRSaturationProperty`.
        adjust: Vec<(String, f64)>,
        /// `sf:traced-path`: the outline of the opaque part of the
        /// picture, in its natural size, that text wraps tightly around.
        traced: Option<String>,
        /// `sf:alpha-mask-path`: the part of the picture that Instant
        /// Alpha keeps, in its natural size, inside by the even-odd rule.
        alpha_mask: Option<String>,
    },
    /// A movie or a sound (`sf:media` with `sf:movie-media`).
    Media {
        /// The path of its file in the package: the first of
        /// `sf:other-datas`, which holds the movie itself, or else
        /// `sf:main-movie`.
        path: String,
        /// The path of the picture that stands for a movie
        /// (`sf:poster-image`).
        poster: Option<String>,
        /// `sf:poster`: where the poster frame was taken, in the units of
        /// the time scale of the movie.
        poster_time: Option<u64>,
        /// `sf:looping`: `no`, `loop`, or `palindrome` for back and forth.
        looping: String,
        /// `sf:volume`, from 0 to 1.
        volume: f64,
    },
    /// An object the reader does not read yet, by its element: a picture,
    /// a movie, a chart or a table.
    Other(String),
}

/// A floating object.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawable {
    /// The object ID, by which text boxes are linked.
    pub id: Option<String>,
    /// Where it is.
    pub place: Place,
    /// `sl:order`: objects with a higher one are in front; those below 0
    /// are behind the text of the page.
    pub order: i32,
    /// Its size and place.
    pub geometry: Geometry,
    /// What it is.
    pub kind: DrawableKind,
    /// The properties of its graphic style and of the styles that style
    /// comes from, its own first: `fill`, `stroke`, `opacity` and
    /// `externalTextWrap`.
    pub style: Properties,
    /// How text goes around it (`sf:external-text-wrap`), from its style.
    pub wrap: Option<TextWrap>,
    /// `sf:locked`.
    pub locked: bool,
    /// The shadow of its style (a `SFIUDropShadow` filter).
    pub shadow: Option<ObjectShadow>,
    /// How opaque the reflection of its style is, from 0 to 1, when its
    /// style reflects it (a `SFIUReflection` filter).
    pub reflection: Option<f64>,
    /// The layout style of its text (`sf:text sf:layoutstyle`), by its
    /// identifier or object ID: its inset, columns and vertical
    /// alignment.
    pub text_layout: Option<String>,
}

/// The shadow of an object (`SFIUDropShadow`), with what Pages takes when
/// the filter does not say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectShadow {
    /// `inputColor`.
    pub color: crate::sf::Color,
    /// `inputAngle`, in degrees clockwise from the right.
    pub angle: f64,
    /// `inputDistance`, in points.
    pub distance: f64,
    /// `inputRadius`: how blurred it is, in points.
    pub radius: f64,
    /// `inputOpacity`, from 0 to 1.
    pub opacity: f64,
}

impl Default for ObjectShadow {
    fn default() -> Self {
        Self {
            color: crate::sf::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            angle: 45.0,
            distance: 6.0,
            radius: 10.0,
            opacity: 0.75,
        }
    }
}

/// How text goes around an object (`sf:external-text-wrap`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextWrap {
    /// `sf:floating-wrap-enabled`: whether text goes around it at all.
    pub enabled: bool,
    /// `sf:direction`: `both`, `left`, `right` or `neither`.
    pub direction: String,
    /// `sf:floating-wrap-type`: `directional`, `largest` for the side with
    /// more room, or `neither` for above and below.
    pub kind: String,
    /// `sf:margin`, in points.
    pub margin: f64,
    /// Whether `sf:wrap-style` is `tight`: around the outline of the
    /// object rather than its box.
    pub tight: bool,
    /// `sf:alpha-threshold`, from 0 to 1: how opaque a pixel of a picture
    /// must be for the text that wraps tightly to go around it.
    pub alpha_threshold: Option<f64>,
}

/// How many of an object and the objects of its group the reader does
/// not read.
fn left_out(drawable: &Drawable) -> usize {
    match &drawable.kind {
        DrawableKind::Other(_) => 1,
        DrawableKind::Group(objects) => objects.iter().map(left_out).sum(),
        _ => 0,
    }
}

/// The path of the file of an `sf:data` element under an element.
fn data_path(ids: &Ids<'_>, element: &Element) -> Option<String> {
    let element = ids.deref(element);
    let data = if element.name.is(SF, "data") {
        element
    } else {
        element.child(SF, "data")?
    };
    data.attr(SF, "path").map(str::to_owned)
}

/// A picture or a movie (`sf:media`).
fn media(ids: &Ids<'_>, element: &Element) -> DrawableKind {
    let Some(content) = element
        .child(SF, "content")
        .and_then(|c| c.elements().next())
    else {
        return DrawableKind::Other("media".into());
    };
    let path = match content.name.local.as_str() {
        "image-media" => content
            .child(SF, "filtered-image")
            .and_then(|f| {
                f.child(SF, "unfiltered")
                    .or_else(|| f.child(SF, "unfiltered-ref"))
            })
            .and_then(|u| data_path(ids, u))
            .map(|path| DrawableKind::Image {
                path,
                mask: element.child(SF, "crop-geometry").map(Geometry::read),
                mask_path: element
                    .child(SF, "masking-shape-path-source")
                    .map(ShapePath::read),
                adjust: content
                    .child(SF, "filtered-image")
                    .and_then(|f| f.child(SF, "filter-properties"))
                    .map(|p| {
                        p.elements()
                            .filter_map(|e| {
                                let n = e.child(SF, "number")?;
                                let v = n.attr(SFA, "number")?.trim().parse::<f64>().ok()?;
                                v.is_finite().then(|| (e.name.local.clone(), v))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                alpha_mask: content
                    .child(SF, "alpha-mask-path")
                    .and_then(|t| t.attr(SFA, "path"))
                    .map(str::to_owned),
                traced: content
                    .child(SF, "traced-path")
                    .and_then(|t| t.attr(SFA, "path"))
                    .map(str::to_owned),
            }),
        "movie-media" => {
            let movie = content.child(SF, "self-contained-movie");
            let path = movie
                .and_then(|m| m.child(SF, "other-datas"))
                .and_then(|o| o.elements().find_map(|d| data_path(ids, d)))
                .or_else(|| {
                    movie
                        .and_then(|m| m.child(SF, "main-movie"))
                        .and_then(|m| m.attr(SF, "path"))
                        .map(str::to_owned)
                });
            path.map(|path| DrawableKind::Media {
                path,
                poster: content
                    .child(SF, "poster-image")
                    .and_then(|p| data_path(ids, p)),
                poster_time: content
                    .attr(SF, "poster")
                    .and_then(|t| t.trim().parse::<u64>().ok()),
                looping: content.attr(SF, "looping").unwrap_or("no").to_owned(),
                volume: content
                    .attr(SF, "volume")
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .unwrap_or(1.0),
            })
        }
        _ => None,
    };
    path.unwrap_or_else(|| DrawableKind::Other("media".into()))
}

/// A number of an attribute, 0 when it is missing or not a number.
fn number(element: &Element, ns: &str, name: &str) -> f64 {
    element
        .attr(ns, name)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

/// How many styles up a graphic style is followed, so that a cycle ends.
const MAX_PARENTS: usize = 16;

struct Reader<'a, 'b> {
    ids: &'a Ids<'a>,
    document: &'b mut PagesDocument,
    /// The graphic styles with an identifier.
    graphic_styles: HashMap<&'a str, &'a Element>,
    /// The elements the reader does not read, by name, with how many.
    skipped: Vec<(String, usize)>,
}

impl<'a> Reader<'a, '_> {
    /// The properties of a graphic style and of its parents.
    fn style(&self, element: &'a Element) -> Properties {
        let mut out = Properties::default();
        let mut style = element
            .child(SF, "style")
            .and_then(|s| s.elements().next())
            .map(|s| self.ids.deref(s));
        for _ in 0..MAX_PARENTS {
            let Some(current) = style else { break };
            if let Some(map) = current.child(SF, "property-map") {
                for (name, value) in Properties::read(self.ids, map).0 {
                    if out.get(&name).is_none() {
                        out.0.push((name, value));
                    }
                }
            }
            style = current
                .attr(SF, "parent-ident")
                .and_then(|p| self.graphic_styles.get(p).copied());
        }
        out
    }

    /// The shadow and the reflection of a graphic style, from the first
    /// of it and its parents that has filters.
    fn filters(&self, element: &'a Element) -> (Option<ObjectShadow>, Option<f64>) {
        let mut style = element
            .child(SF, "style")
            .and_then(|s| s.elements().next())
            .map(|s| self.ids.deref(s));
        for _ in 0..MAX_PARENTS {
            let Some(current) = style else { break };
            let filters = current
                .child(SF, "property-map")
                .and_then(|m| m.child(SF, "filters"));
            if let Some(filters) = filters {
                let mut shadow = None;
                let mut reflection = None;
                for info in filters
                    .descendants()
                    .filter(|e| e.name.is(SF, "core-image-filter-info"))
                {
                    let class = info
                        .elements()
                        .map(|e| self.ids.deref(e))
                        .find(|e| e.name.is(SF, "core-image-filter-descriptor"))
                        .and_then(|d| d.attr(SF, "filterClassName"));
                    let overrides = info.child(SF, "overrides");
                    let get = |name: &str| {
                        overrides?
                            .child(SF, name)?
                            .child(SF, "number")?
                            .attr(SFA, "number")?
                            .trim()
                            .parse::<f64>()
                            .ok()
                            .filter(|v| v.is_finite())
                    };
                    match class {
                        Some("SFIUDropShadow") => {
                            let base = ObjectShadow::default();
                            shadow = Some(ObjectShadow {
                                color: overrides
                                    .and_then(|o| o.child(SF, "inputColor"))
                                    .and_then(|c| c.child(SF, "color"))
                                    .and_then(crate::sf::color)
                                    .unwrap_or(base.color),
                                angle: get("inputAngle").unwrap_or(base.angle),
                                distance: get("inputDistance").unwrap_or(base.distance),
                                radius: get("inputRadius").unwrap_or(base.radius),
                                opacity: get("inputOpacity").unwrap_or(base.opacity),
                            });
                        }
                        Some("SFIUReflection") => {
                            reflection = Some(get("inputOpacity").unwrap_or(0.5).clamp(0.0, 1.0));
                        }
                        _ => {}
                    }
                }
                return (shadow, reflection);
            }
            style = current
                .attr(SF, "parent-ident")
                .and_then(|p| self.graphic_styles.get(p).copied());
        }
        (None, None)
    }

    /// The text wrap of a graphic style and of its parents.
    fn wrap(&self, element: &'a Element) -> Option<TextWrap> {
        let mut style = element
            .child(SF, "style")
            .and_then(|s| s.elements().next())
            .map(|s| self.ids.deref(s));
        for _ in 0..MAX_PARENTS {
            let current = style?;
            let wrap = current
                .child(SF, "property-map")
                .and_then(|m| m.child(SF, "externalTextWrap"))
                .and_then(|w| w.child(SF, "external-text-wrap"));
            if let Some(wrap) = wrap {
                return Some(TextWrap {
                    // Pages leaves it out when it is off.
                    enabled: wrap.attr(SF, "floating-wrap-enabled") == Some("true"),
                    direction: wrap.attr(SF, "direction").unwrap_or("both").to_owned(),
                    kind: wrap
                        .attr(SF, "floating-wrap-type")
                        .unwrap_or("directional")
                        .to_owned(),
                    margin: number(wrap, SF, "margin"),
                    tight: wrap.attr(SF, "wrap-style") == Some("tight"),
                    alpha_threshold: wrap
                        .attr(SF, "alpha-threshold")
                        .and_then(|t| t.trim().parse::<f64>().ok())
                        .filter(|t| t.is_finite()),
                });
            }
            style = current
                .attr(SF, "parent-ident")
                .and_then(|p| self.graphic_styles.get(p).copied());
        }
        None
    }

    /// The paragraphs of the text of a shape, and whether the last one
    /// goes on in the next linked box.
    fn text(&mut self, shape: &Element) -> (Vec<Paragraph>, bool) {
        let body = shape
            .child(SF, "text")
            .and_then(|t| t.child(SF, "text-storage"))
            .and_then(|s| s.child(SF, "text-body"));
        let Some(body) = body else {
            return (Vec::new(), false);
        };
        let open = body
            .children_named(SF, "p")
            .last()
            .is_some_and(|p| !p.descendants().any(|e| e.name.is(SF, "br")));
        let outer = std::mem::take(&mut self.document.body);
        // What the text of objects loses is not counted with the body.
        let skipped = self.document.skipped.clone();
        read_block(self.ids, self.document, body);
        let paragraphs = std::mem::replace(&mut self.document.body, outer);
        self.document.skipped = skipped;
        (paragraphs, open)
    }

    /// Reads the objects of a group of the drawables, of pages or of
    /// masters.
    fn group(&mut self, group: &'a Element, place: &Place, out: &mut Vec<Drawable>) {
        for child in group.elements() {
            match child.name.local.as_str() {
                "masters-group" => self.group(child, place, out),
                "section-drawables" => {
                    let section = child.attr(SL, "name").unwrap_or_default().to_owned();
                    for (name, background) in [
                        ("foregroundmaster-group", false),
                        ("backgroundmaster-group", true),
                    ] {
                        for master in child.children_named(SL, name) {
                            // All pages, the first, odd and even pages.
                            for pages in master.elements() {
                                let place = Place::Master {
                                    section: section.clone(),
                                    background,
                                    pages: pages.name.local.clone(),
                                };
                                self.group(pages, &place, out);
                            }
                        }
                    }
                }
                "page-group" => {
                    let page = number(child, SL, "page").max(1.0) as u32 - 1;
                    self.group(child, &Place::Page(page), out);
                }
                "textbox-chain" | "style" => {}
                _ => {
                    if let Some(drawable) = self.drawable(child, place) {
                        // Counted as the floating objects an import leaves
                        // out.
                        let left_out = left_out(&drawable);
                        if left_out > 0 {
                            match self.skipped.iter_mut().find(|(n, _)| n == "drawables") {
                                Some((_, count)) => *count += left_out,
                                None => self.skipped.push(("drawables".to_owned(), left_out)),
                            }
                        }
                        out.push(drawable);
                    }
                }
            }
        }
    }

    /// Reads an object, or one it refers to.
    fn drawable(&mut self, element: &'a Element, place: &Place) -> Option<Drawable> {
        let element = self.ids.deref(element);
        let kind = match element.name.local.as_str() {
            "drawable-shape" => {
                let (text, _) = self.text(element);
                DrawableKind::Shape {
                    path: element
                        .child(SF, "path")
                        .map_or_else(|| ShapePath::Other(String::new()), ShapePath::read),
                    text_box: element.attr(SF, "linkable") == Some("true"),
                    text,
                    next: None,
                }
            }
            "group" => {
                let mut objects = Vec::new();
                for child in element.elements() {
                    if matches!(
                        child.name.local.as_str(),
                        "geometry" | "wrap" | "style" | "external-text-wrap"
                    ) {
                        continue;
                    }
                    if let Some(object) = self.drawable(child, place) {
                        objects.push(object);
                    }
                }
                DrawableKind::Group(objects)
            }
            "media" => media(self.ids, element),
            other => DrawableKind::Other(other.to_owned()),
        };
        let (shadow, reflection) = self.filters(element);
        Some(Drawable {
            id: element.attr(SFA, "ID").map(str::to_owned),
            place: place.clone(),
            order: element
                .attr(SL, "order")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            geometry: element
                .child(SF, "geometry")
                .map(Geometry::read)
                .unwrap_or_default(),
            kind,
            style: self.style(element),
            wrap: self.wrap(element),
            locked: element.attr(SF, "locked") == Some("true"),
            shadow,
            reflection,
            text_layout: element
                .child(SF, "text")
                .and_then(|t| t.attr(SF, "layoutstyle"))
                .map(str::to_owned),
        })
    }
}

/// Reads the floating objects of a document, and links its text boxes.
/// Returns the elements it could not read, by name, with how many.
pub(super) fn read(
    ids: &Ids<'_>,
    root: &Element,
    document: &mut PagesDocument,
) -> Vec<(String, usize)> {
    let graphic_styles = root
        .descendants()
        .filter(|e| e.name.is(SF, "graphic-style"))
        .filter_map(|e| Some((e.attr(SF, "ident")?, e)))
        .collect();
    let mut reader = Reader {
        ids,
        document,
        graphic_styles,
        skipped: Vec::new(),
    };
    let mut drawables = Vec::new();
    let mut chains: Vec<Vec<String>> = Vec::new();
    for group in root.children_named(SL, "drawables") {
        reader.group(group, &Place::Page(0), &mut drawables);
        for chain in group.children_named(SL, "textbox-chain") {
            let mut boxes: Vec<(f64, String)> = chain
                .elements()
                .filter_map(|r| {
                    let id = r.attr(SFA, "IDREF")?.to_owned();
                    Some((number(r, SF, "order"), id))
                })
                .collect();
            boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
            chains.push(boxes.into_iter().map(|(_, id)| id).collect());
        }
    }
    // The text of linked boxes goes in the first, each part after the
    // text before it, joined to its last paragraph when that goes on.
    for chain in chains {
        let mut text: Vec<Paragraph> = Vec::new();
        let mut open = false;
        for (i, id) in chain.iter().enumerate() {
            let Some(element) = ids.get(id) else {
                continue;
            };
            let (part, part_open) = reader.text(element);
            let mut part = part.into_iter();
            if open && let (Some(last), Some(first)) = (text.last_mut(), part.next()) {
                last.runs.extend(first.runs);
            }
            text.extend(part);
            open = part_open;
            if let Some(Drawable {
                kind:
                    DrawableKind::Shape {
                        text: own, next, ..
                    },
                ..
            }) = drawables.iter_mut().find(|d| d.id.as_deref() == Some(id))
            {
                own.clear();
                *next = chain.get(i + 1).cloned();
            }
        }
        if let Some(Drawable {
            kind: DrawableKind::Shape { text: own, .. },
            ..
        }) = chain
            .first()
            .and_then(|head| drawables.iter_mut().find(|d| d.id.as_ref() == Some(head)))
        {
            *own = text;
        }
    }
    reader.document.drawables = drawables;
    // The objects in the line of the text, which the text refers to.
    let attachments: Vec<&Element> = root
        .descendants()
        .filter(|e| e.name.is(SF, "attachment"))
        .filter(|e| e.attr(SF, "kind") == Some("drawable-attachment"))
        .collect();
    for attachment in attachments {
        let Some(id) = attachment.attr(SFA, "ID") else {
            continue;
        };
        let object = attachment
            .elements()
            .find(|e| !matches!(e.name.local.as_str(), "position" | "original-size"));
        let Some(drawable) = object.and_then(|o| reader.drawable(o, &Place::Page(0))) else {
            continue;
        };
        if left_out(&drawable) == 0 {
            reader.document.attachments.push((id.to_owned(), drawable));
        }
    }
    reader.skipped
}
