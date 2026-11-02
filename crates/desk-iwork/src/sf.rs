// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The vocabulary that Pages, Numbers and Keynote '09 share, in the `sf`
//! namespace: values, property maps, styles and stylesheets.
//!
//! Objects in the index carry an `sfa:ID`, and a `...-ref` element with
//! an `sfa:IDREF` stands for the object it names, so the same style or
//! value can appear in many places. The reader follows those references
//! through an index of the objects by ID.

use std::collections::HashMap;

use crate::ns::{SF, SFA};
use crate::xml::Element;

/// The objects of an index by their `sfa:ID`.
pub(crate) struct Ids<'a>(HashMap<&'a str, &'a Element>);

impl<'a> Ids<'a> {
    /// Indexes every element with an ID under `root`.
    pub fn new(root: &'a Element) -> Self {
        let mut ids = HashMap::new();
        let mut stack = vec![root];
        while let Some(element) = stack.pop() {
            if let Some(id) = element.attr(SFA, "ID") {
                ids.entry(id).or_insert(element);
            }
            stack.extend(element.elements());
        }
        Self(ids)
    }

    /// The element with an ID.
    pub fn get(&self, id: &str) -> Option<&'a Element> {
        self.0.get(id).copied()
    }

    /// The element itself, or the one it refers to when it is a
    /// reference. A reference to a missing object stays as it is.
    pub fn deref(&self, element: &'a Element) -> &'a Element {
        match element.attr(SFA, "IDREF") {
            Some(id) => self.get(id).unwrap_or(element),
            None => element,
        }
    }
}

/// A color, with components from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// Red.
    pub r: f64,
    /// Green.
    pub g: f64,
    /// Blue.
    pub b: f64,
    /// Opacity.
    pub a: f64,
}

/// How the spacing of the lines of a paragraph is measured.
#[derive(Clone, Debug, PartialEq)]
pub enum LineSpacingMode {
    /// A multiple of single spacing ("Lines").
    Relative,
    /// At least the amount, in points.
    Minimum,
    /// Exactly the amount, in points.
    Absolute,
    /// The amount added between lines ("Between", saved as `keynote`).
    Between,
    /// A mode the reader does not know.
    Other(String),
}

/// A value of a property.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Set to nothing, as for no list style or no fill.
    Null,
    /// A number, a flag (1 or 0) or an enumeration.
    Number(f64),
    /// A string.
    String(String),
    /// A color.
    Color(Color),
    /// The spacing of lines.
    LineSpacing {
        /// The mode.
        mode: LineSpacingMode,
        /// The multiple or the length, depending on the mode.
        amount: f64,
    },
    /// A list style: its name, and whether any level draws a label, a
    /// bullet or a number. The style named None draws none.
    ListStyle {
        /// The name, as the Styles drawer shows it.
        name: Option<String>,
        /// Whether a level has a label.
        labels: bool,
    },
    /// Tab stops, in the order saved.
    Tabs(Vec<Tab>),
    /// A shadow (`sf:shadow`).
    Shadow(Shadow),
    /// A line (`sf:stroke`).
    Stroke(Stroke),
    /// A value the reader does not interpret yet, by the local name of its
    /// element.
    Other(String),
}

/// A tab stop (`sf:tabstop`).
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    /// `sf:pos`, in points from the left edge of the text.
    pub position: f64,
    /// `sf:align`: `left`, `center`, `right` or `decimal`.
    pub align: String,
    /// `sf:leader`, the text that fills the space before the stop.
    pub leader: Option<String>,
}

/// A shadow (`sf:shadow`).
#[derive(Clone, Debug, PartialEq)]
pub struct Shadow {
    /// `sf:angle`, in degrees counterclockwise from the right.
    pub angle: f64,
    /// `sf:offset`, in points.
    pub offset: f64,
    /// `sf:radius`, the blur, in points.
    pub radius: f64,
    /// `sf:opacity`, from 0 to 1.
    pub opacity: f64,
    /// The color.
    pub color: Color,
}

/// How a line is drawn (`sf:pattern`).
#[derive(Clone, Debug, PartialEq)]
pub enum StrokePattern {
    /// `solid`.
    Solid,
    /// `empty`: no line.
    Empty,
    /// `pattern`: dashes and gaps, as multiples of the width.
    Dashes(Vec<f64>),
}

/// A line (`sf:stroke`).
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    /// `sf:width`, in points.
    pub width: f64,
    /// The color.
    pub color: Color,
    /// How it is drawn.
    pub pattern: StrokePattern,
}

impl Value {
    fn read(ids: &Ids<'_>, element: &Element) -> Self {
        let element = ids.deref(element);
        let number = |name| {
            element
                .attr(SFA, name)
                .and_then(|v| v.trim().parse::<f64>().ok())
        };
        match element.name.local.as_str() {
            "null" => Self::Null,
            "number" | "decimal-number" => number("number").map_or(Self::Null, Self::Number),
            "string" => Self::String(element.attr(SFA, "string").unwrap_or_default().to_owned()),
            "color" => color(element).map_or_else(|| Self::Other("color".into()), Self::Color),
            "linespacing" => Self::LineSpacing {
                mode: match element.attr(SF, "mode").unwrap_or("relative") {
                    "relative" => LineSpacingMode::Relative,
                    "minimum" => LineSpacingMode::Minimum,
                    "absolute" => LineSpacingMode::Absolute,
                    "keynote" => LineSpacingMode::Between,
                    other => LineSpacingMode::Other(other.to_owned()),
                },
                amount: element
                    .attr(SF, "amt")
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(1.0),
            },
            "liststyle" => Self::ListStyle {
                name: element.attr(SF, "name").map(str::to_owned),
                labels: element.descendants().any(|e| {
                    e.name.is(SF, "list-label-typeinfo")
                        && e.attr(SF, "type").is_some_and(|t| t != "none")
                }),
            },
            "tabs" => Self::Tabs(
                element
                    .children_named(SF, "tabstop")
                    .filter_map(|tab| {
                        Some(Tab {
                            position: tab.attr(SF, "pos")?.trim().parse().ok()?,
                            align: tab.attr(SF, "align").unwrap_or("left").to_owned(),
                            leader: tab
                                .attr(SF, "leader")
                                .filter(|l| !l.trim().is_empty())
                                .map(str::to_owned),
                        })
                    })
                    .collect(),
            ),
            "shadow" => {
                let get = |name, default| {
                    element
                        .attr(SF, name)
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                        .unwrap_or(default)
                };
                Self::Shadow(Shadow {
                    angle: get("angle", 315.0),
                    offset: get("offset", 4.0),
                    radius: get("radius", 4.0),
                    opacity: get("opacity", 1.0).clamp(0.0, 1.0),
                    color: element
                        .children_named(SF, "color")
                        .next()
                        .and_then(color)
                        .unwrap_or(Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                })
            }
            "stroke" => {
                let pattern = element.children_named(SF, "pattern").next();
                Self::Stroke(Stroke {
                    width: element
                        .attr(SF, "width")
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .filter(|v| v.is_finite() && *v >= 0.0)
                        .unwrap_or(1.0),
                    color: element
                        .children_named(SF, "color")
                        .next()
                        .and_then(color)
                        .unwrap_or(Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                    pattern: match pattern.and_then(|p| p.attr(SF, "type")) {
                        Some("empty") => StrokePattern::Empty,
                        Some("pattern") => StrokePattern::Dashes(
                            pattern
                                .into_iter()
                                .flat_map(|p| p.descendants())
                                .filter(|e| e.name.is(SF, "element"))
                                .filter_map(|e| e.attr(SF, "val")?.trim().parse().ok())
                                .collect(),
                        ),
                        _ => StrokePattern::Solid,
                    },
                })
            }
            other => Self::Other(other.to_owned()),
        }
    }
}

/// A color element, in any of the color spaces that iWork saves.
fn color(element: &Element) -> Option<Color> {
    let get = |name| {
        element
            .attr(SFA, name)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 1.0))
    };
    let a = get("a").unwrap_or(1.0);
    let kind = element
        .attrs
        .iter()
        .find(|(name, _)| name.local == "type")
        .map_or("", |(_, value)| value.as_str());
    let kind = kind.rsplit(':').next().unwrap_or(kind);
    Some(match kind {
        "calibrated-white-color-type" | "device-white-color-type" => {
            let w = get("w")?;
            Color {
                r: w,
                g: w,
                b: w,
                a,
            }
        }
        "device-cmyk-color-type" | "calibrated-cmyk-color-type" => {
            let k = get("k")?;
            let [c, m, y] = [get("c")?, get("m")?, get("y")?];
            Color {
                r: (1.0 - c) * (1.0 - k),
                g: (1.0 - m) * (1.0 - k),
                b: (1.0 - y) * (1.0 - k),
                a,
            }
        }
        _ => Color {
            r: get("r")?,
            g: get("g")?,
            b: get("b")?,
            a,
        },
    })
}

/// The properties that a style sets, in the order of its property map.
/// A property saved empty is not set, so it is left out, and one saved
/// with `sf:null` is set to [`Value::Null`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Properties(pub Vec<(String, Value)>);

impl Properties {
    fn read(ids: &Ids<'_>, map: &Element) -> Self {
        let mut properties = Vec::new();
        for property in map.elements() {
            if let Some(value) = property.elements().next() {
                properties.push((property.name.local.clone(), Value::read(ids, value)));
            }
        }
        Self(properties)
    }

    /// The value of a property, when set.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// The value of a property that holds a number.
    pub fn number(&self, name: &str) -> Option<f64> {
        match self.get(name)? {
            Value::Number(n) if n.is_finite() => Some(*n),
            _ => None,
        }
    }

    /// The value of a property that holds a string.
    pub fn string(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}

/// The kind of a style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// A paragraph style, which also holds character properties.
    Paragraph,
    /// A character style.
    Character,
}

/// A paragraph or character style.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// The kind.
    pub family: Family,
    /// The object ID, unique in the index.
    pub id: Option<String>,
    /// The identifier, which the text and child styles refer to. Named
    /// styles have one; anonymous styles, which hold direct formatting,
    /// usually do not.
    pub ident: Option<String>,
    /// The name shown in the Styles drawer, for named styles.
    pub name: Option<String>,
    /// The identifier of the parent style.
    pub parent: Option<String>,
    /// The properties it sets.
    pub properties: Properties,
}

impl Style {
    fn read(ids: &Ids<'_>, element: &Element) -> Option<Self> {
        let family = match element.name.local.as_str() {
            "paragraphstyle" => Family::Paragraph,
            "characterstyle" => Family::Character,
            _ => return None,
        };
        let attr = |name| element.attr(SF, name).map(str::to_owned);
        Some(Self {
            family,
            id: element.attr(SFA, "ID").map(str::to_owned),
            ident: attr("ident"),
            name: attr("name"),
            parent: attr("parent-ident"),
            properties: element
                .child(SF, "property-map")
                .map(|map| Properties::read(ids, map))
                .unwrap_or_default(),
        })
    }
}

/// The styles of a document: the named ones and the anonymous ones that
/// hold direct formatting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stylesheet {
    /// The styles, the stylesheet's own before those of its parents.
    pub styles: Vec<Style>,
}

/// How many parent stylesheets are followed, so that a cycle ends.
const MAX_PARENTS: usize = 16;

impl Stylesheet {
    pub(crate) fn read(ids: &Ids<'_>, element: &Element) -> Self {
        let mut styles = Vec::new();
        let mut sheet = Some(element);
        for _ in 0..MAX_PARENTS {
            let Some(current) = sheet else { break };
            for group in ["styles", "anon-styles"] {
                for group in current.children_named(SF, group) {
                    for style in group.elements() {
                        styles.extend(Style::read(ids, ids.deref(style)));
                    }
                }
            }
            sheet = current
                .child(SF, "parent-ref")
                .and_then(|r| r.attr(SFA, "IDREF").and_then(|id| ids.get(id)));
        }
        Self { styles }
    }

    /// The style that text refers to by `sf:style`: by identifier, or
    /// else by object ID.
    pub fn find(&self, reference: &str) -> Option<&Style> {
        self.styles
            .iter()
            .find(|s| s.ident.as_deref() == Some(reference))
            .or_else(|| {
                self.styles
                    .iter()
                    .find(|s| s.id.as_deref() == Some(reference))
            })
    }

    /// The parent of a style, of the same family.
    pub fn parent(&self, style: &Style) -> Option<&Style> {
        let parent = style.parent.as_deref()?;
        self.styles
            .iter()
            .find(|s| s.family == style.family && s.ident.as_deref() == Some(parent))
    }

    /// A style and its ancestors, nearest first. A cycle ends the chain.
    pub fn ancestry<'a>(&'a self, style: &'a Style) -> Vec<&'a Style> {
        let mut chain = vec![style];
        while let Some(parent) = chain.last().and_then(|s| self.parent(s)) {
            if chain.iter().any(|s| std::ptr::eq(*s, parent)) {
                break;
            }
            chain.push(parent);
        }
        chain
    }
}
