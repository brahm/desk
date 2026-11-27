// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shape library of Pages 15.4 (W113): shapes by category, found by
//! their names and words, and My Shapes, which the user saves from the
//! page. The shapes are drawn for Desk: none comes from Pages (D-011).
//!
//! A shape of the library is written as a path of absolute commands,
//! `M x y`, `L x y`, `C x1 y1 x2 y2 x y` and `Z`, in any units: its box
//! gives the proportions of the shape it makes. My Shapes are kept in the
//! same way, one a line, in `my-shapes.txt` of the data folder of Desk.

use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use desk_model::{MAX_SHAPE_PARTS, POLYGON_SCALE, PathPoint, PathSegment, Shape};

use crate::editing::NewShape;
use crate::i18n::{n_, tr};

/// The categories of the library, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    Basic,
    Geometry,
    Arrows,
    Symbols,
    Nature,
    Objects,
    Mine,
}

impl Category {
    pub(crate) const ALL: [Self; 7] = [
        Self::Basic,
        Self::Geometry,
        Self::Arrows,
        Self::Symbols,
        Self::Nature,
        Self::Objects,
        Self::Mine,
    ];

    /// The name of the category, translated.
    pub(crate) fn name(self) -> String {
        tr(match self {
            Self::Basic => n_("Basic"),
            Self::Geometry => n_("Geometry"),
            Self::Arrows => n_("Arrows"),
            Self::Symbols => n_("Symbols"),
            Self::Nature => n_("Nature"),
            Self::Objects => n_("Objects"),
            Self::Mine => n_("My Shapes"),
        })
    }
}

/// How a shape of the library is made.
#[derive(Clone, Copy)]
enum Outline {
    /// A shape of Insert > Shape, made as that makes it.
    New(NewShape),
    /// A shape with handles of its own, in a frame of a size.
    Parametric(fn() -> Shape, (f64, f64)),
    /// A path.
    Path(&'static str),
    /// A path worked out, for shapes of many equal parts.
    Built(fn() -> String),
}

/// A shape of the library.
pub(crate) struct Entry {
    /// What names it in the action that inserts it.
    pub id: &'static str,
    pub category: Category,
    /// Its name, to translate.
    name: &'static str,
    /// Other words that find it, to translate.
    words: &'static str,
    outline: Outline,
}

/// What a shape of the library or of My Shapes puts on the page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Made {
    /// A shape of Insert > Shape.
    New(NewShape),
    /// A shape of the library, and the size of its frame in points.
    Shape(Shape, (f64, f64)),
    /// A shape of My Shapes, and the size it was saved at.
    Saved(Shape, (f64, f64)),
}

impl Entry {
    /// Its name, translated, without the mnemonic of a menu.
    pub(crate) fn name(&self) -> String {
        tr(self.name).replace('_', "")
    }

    /// Whether it is found with `query`, by its name or its words.
    pub(crate) fn matches(&self, query: &str) -> bool {
        matches(&format!("{} {}", self.name(), tr(self.words)), query)
    }

    /// What it puts on the page, with the longer side of its frame 100
    /// points long.
    pub(crate) fn made(&self) -> Option<Made> {
        let (shape, (w, h)) = match self.outline {
            Outline::New(shape) => return Some(Made::New(shape)),
            Outline::Parametric(shape, size) => (shape(), size),
            Outline::Path(path) => shape_of_path(path)?,
            Outline::Built(path) => shape_of_path(&path())?,
        };
        Some(Made::Shape(shape, fitted(w, h, 100.0)))
    }
}

/// A size of the proportions of `w` by `h`, with its longer side `side`.
fn fitted(w: f64, h: f64, side: f64) -> (f64, f64) {
    let longer = w.max(h).max(f64::EPSILON);
    (w / longer * side, h / longer * side)
}

/// The shapes of the library, by category.
pub(crate) fn entries() -> &'static [Entry] {
    &ENTRIES
}

/// The shape of the library that `id` names.
pub(crate) fn entry(id: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.id == id)
}

const fn new(id: &'static str, name: &'static str, words: &'static str, shape: NewShape) -> Entry {
    Entry {
        id,
        category: Category::Basic,
        name,
        words,
        outline: Outline::New(shape),
    }
}

const fn path(
    id: &'static str,
    category: Category,
    name: &'static str,
    words: &'static str,
    path: &'static str,
) -> Entry {
    Entry {
        id,
        category,
        name,
        words,
        outline: Outline::Path(path),
    }
}

const fn built(
    id: &'static str,
    category: Category,
    name: &'static str,
    words: &'static str,
    path: fn() -> String,
) -> Entry {
    Entry {
        id,
        category,
        name,
        words,
        outline: Outline::Built(path),
    }
}

const fn parametric(
    id: &'static str,
    name: &'static str,
    words: &'static str,
    shape: fn() -> Shape,
) -> Entry {
    Entry {
        id,
        category: Category::Geometry,
        name,
        words,
        outline: Outline::Parametric(shape, (100.0, 100.0)),
    }
}

use Category::{Arrows, Geometry, Nature, Objects, Symbols};

static ENTRIES: [Entry; 57] = [
    // The shapes of Insert > Shape, as Basic is in Pages.
    new(
        "rectangle",
        n_("_Rectangle"),
        n_("square box"),
        NewShape::Rectangle,
    ),
    new(
        "rounded",
        n_("Rounded Re_ctangle"),
        n_("square box button"),
        NewShape::RoundedRectangle,
    ),
    new(
        "ellipse",
        n_("_Oval"),
        n_("circle ellipse round"),
        NewShape::Ellipse,
    ),
    new(
        "triangle",
        n_("Tr_iangle"),
        n_("pyramid"),
        NewShape::Triangle,
    ),
    new(
        "right-triangle",
        n_("Ri_ght Triangle"),
        n_("triangle"),
        NewShape::RightTriangle,
    ),
    new(
        "diamond",
        n_("_Diamond"),
        n_("rhombus lozenge"),
        NewShape::Diamond,
    ),
    new("arrow", n_("_Arrow"), n_("right"), NewShape::Arrow),
    new(
        "double-arrow",
        n_("Dou_ble Arrow"),
        n_("left right"),
        NewShape::DoubleArrow,
    ),
    new("star", n_("_Star"), n_("favorite"), NewShape::Star),
    new("polygon", n_("_Polygon"), n_("pentagon"), NewShape::Polygon),
    new(
        "quote-bubble",
        n_("_Quote Bubble"),
        n_("speech talk comic balloon"),
        NewShape::QuoteBubble,
    ),
    new(
        "callout",
        n_("Call_out"),
        n_("speech talk balloon"),
        NewShape::Callout,
    ),
    new("line", n_("_Line"), n_("stroke rule"), NewShape::Line),
    // Geometry.
    parametric("hexagon", n_("Hexagon"), n_("polygon six"), || {
        Shape::RegularPolygon { sides: 6 }
    }),
    parametric("octagon", n_("Octagon"), n_("polygon eight stop"), || {
        Shape::RegularPolygon { sides: 8 }
    }),
    parametric(
        "star-four",
        n_("Four-Pointed Star"),
        n_("sparkle twinkle"),
        || Shape::Star {
            points: 4,
            inner: 3_000,
        },
    ),
    parametric("star-six", n_("Six-Pointed Star"), n_("hexagram"), || {
        Shape::Star {
            points: 6,
            inner: 5_000,
        }
    }),
    parametric(
        "burst",
        n_("Burst"),
        n_("starburst seal badge explosion"),
        || Shape::Star {
            points: 16,
            inner: 7_800,
        },
    ),
    path(
        "trapezoid",
        Geometry,
        n_("Trapezoid"),
        n_("quadrilateral"),
        "M 20 0 L 80 0 L 100 100 L 0 100 Z",
    ),
    path(
        "parallelogram",
        Geometry,
        n_("Parallelogram"),
        n_("quadrilateral slanted"),
        "M 25 0 L 100 0 L 75 100 L 0 100 Z",
    ),
    path(
        "cross",
        Geometry,
        n_("Cross"),
        n_("plus add"),
        "M 35 0 L 65 0 L 65 35 L 100 35 L 100 65 L 65 65 L 65 100 L 35 100 L 35 65 \
         L 0 65 L 0 35 L 35 35 Z",
    ),
    path(
        "semicircle",
        Geometry,
        n_("Semicircle"),
        n_("half circle arch dome"),
        "M 0 50 C 0 22.4 22.4 0 50 0 C 77.6 0 100 22.4 100 50 Z",
    ),
    path(
        "quarter-circle",
        Geometry,
        n_("Quarter Circle"),
        n_("pie wedge sector"),
        "M 0 100 L 0 0 C 55.2 0 100 44.8 100 100 Z",
    ),
    built(
        "ring",
        Geometry,
        n_("Ring"),
        n_("donut circle hole"),
        || {
            format!(
                "{} {}",
                circle(50.0, 50.0, 50.0, false),
                circle(50.0, 50.0, 28.0, true)
            )
        },
    ),
    path(
        "frame",
        Geometry,
        n_("Frame"),
        n_("border square hole"),
        "M 0 0 L 100 0 L 100 100 L 0 100 Z M 15 15 L 15 85 L 85 85 L 85 15 Z",
    ),
    path(
        "chevron",
        Geometry,
        n_("Chevron"),
        n_("arrow next step"),
        "M 0 0 L 70 0 L 100 50 L 70 100 L 0 100 L 30 50 Z",
    ),
    // Arrows.
    path(
        "arrow-up",
        Arrows,
        n_("Up Arrow"),
        n_("north top"),
        "M 50 0 L 100 45 L 70 45 L 70 100 L 30 100 L 30 45 L 0 45 Z",
    ),
    path(
        "arrow-down",
        Arrows,
        n_("Down Arrow"),
        n_("south bottom"),
        "M 30 0 L 70 0 L 70 55 L 100 55 L 50 100 L 0 55 L 30 55 Z",
    ),
    path(
        "arrow-left",
        Arrows,
        n_("Left Arrow"),
        n_("west back previous"),
        "M 0 50 L 45 0 L 45 30 L 100 30 L 100 70 L 45 70 L 45 100 Z",
    ),
    path(
        "arrow-four",
        Arrows,
        n_("Four-Way Arrow"),
        n_("move cross directions"),
        "M 50 0 L 65 15 L 57 15 L 57 43 L 85 43 L 85 35 L 100 50 L 85 65 L 85 57 \
         L 57 57 L 57 85 L 65 85 L 50 100 L 35 85 L 43 85 L 43 57 L 15 57 L 15 65 \
         L 0 50 L 15 35 L 15 43 L 43 43 L 43 15 L 35 15 Z",
    ),
    path(
        "arrow-notched",
        Arrows,
        n_("Notched Arrow"),
        n_("right forward"),
        "M 0 25 L 60 25 L 60 0 L 100 50 L 60 100 L 60 75 L 0 75 L 15 50 Z",
    ),
    path(
        "arrow-pentagon",
        Arrows,
        n_("Pointed Box"),
        n_("pentagon arrow label step"),
        "M 0 0 L 75 0 L 100 50 L 75 100 L 0 100 Z",
    ),
    path(
        "arrow-u-turn",
        Arrows,
        n_("U-Turn Arrow"),
        n_("back return curved"),
        "M 0 100 L 0 40 C 0 18 18 0 40 0 C 62 0 80 18 80 40 L 80 55 L 100 55 \
         L 70 90 L 40 55 L 60 55 L 60 40 C 60 29 51 20 40 20 C 29 20 20 29 20 40 \
         L 20 100 Z",
    ),
    // Symbols.
    path(
        "heart",
        Symbols,
        n_("Heart"),
        n_("love like valentine"),
        "M 50 25 C 50 10 38 0 25 0 C 11 0 0 11 0 27 C 0 55 30 75 50 100 \
         C 70 75 100 55 100 27 C 100 11 89 0 75 0 C 62 0 50 10 50 25 Z",
    ),
    path(
        "check",
        Symbols,
        n_("Check Mark"),
        n_("tick yes done right correct"),
        "M 0 55 L 15 40 L 38 62 L 85 10 L 100 25 L 38 92 Z",
    ),
    path(
        "x-mark",
        Symbols,
        n_("X Mark"),
        n_("cross no wrong close cancel"),
        "M 15 0 L 50 35 L 85 0 L 100 15 L 65 50 L 100 85 L 85 100 L 50 65 L 15 100 \
         L 0 85 L 35 50 L 0 15 Z",
    ),
    path(
        "lightning",
        Symbols,
        n_("Lightning Bolt"),
        n_("flash thunder power energy"),
        "M 60 0 L 10 58 L 45 58 L 30 100 L 90 38 L 55 38 L 75 0 Z",
    ),
    path(
        "warning",
        Symbols,
        n_("Warning"),
        n_("caution alert exclamation triangle"),
        "M 50 0 L 100 90 L 0 90 Z M 45 30 L 45 62 L 55 62 L 55 30 Z \
         M 45 70 L 45 80 L 55 80 L 55 70 Z",
    ),
    path(
        "music",
        Symbols,
        n_("Music Notes"),
        n_("song sound melody"),
        "M 35 0 L 100 0 L 100 70 C 100 81 91 88 80 88 C 69 88 62 81 62 72 \
         C 62 63 70 56 80 56 C 83 56 86 57 88 58 L 88 20 L 47 20 L 47 82 \
         C 47 93 38 100 27 100 C 16 100 9 93 9 84 C 9 75 17 68 27 68 \
         C 30 68 33 69 35 70 Z",
    ),
    path(
        "speech",
        Symbols,
        n_("Speech Bubble"),
        n_("talk chat comment balloon"),
        "M 50 0 C 78 0 100 17 100 38 C 100 59 78 76 50 76 C 45 76 40 75 35 74 \
         L 12 92 L 18 68 C 7 61 0 50 0 38 C 0 17 22 0 50 0 Z",
    ),
    // Nature.
    built("sun", Nature, n_("Sun"), n_("sunny weather day light"), sun),
    path(
        "moon",
        Nature,
        n_("Moon"),
        n_("crescent night"),
        "M 60 0 C 27 0 0 22 0 50 C 0 78 27 100 60 100 C 40 88 30 70 30 50 \
         C 30 30 40 12 60 0 Z",
    ),
    path(
        "cloud",
        Nature,
        n_("Cloud"),
        n_("weather sky"),
        "M 25 90 C 11 90 0 80 0 67 C 0 55 9 46 20 45 C 20 28 33 15 50 15 \
         C 63 15 74 23 78 35 C 91 36 100 47 100 61 C 100 77 88 90 72 90 Z",
    ),
    path(
        "drop",
        Nature,
        n_("Drop"),
        n_("water rain teardrop liquid"),
        "M 50 0 C 50 0 90 45 90 65 C 90 87 72 100 50 100 C 28 100 10 87 10 65 \
         C 10 45 50 0 50 0 Z",
    ),
    path(
        "leaf",
        Nature,
        n_("Leaf"),
        n_("plant green"),
        "M 0 100 C 0 40 40 0 100 0 C 100 60 60 100 0 100 Z",
    ),
    path(
        "tree",
        Nature,
        n_("Tree"),
        n_("pine fir forest christmas"),
        "M 50 0 L 80 35 L 65 35 L 90 65 L 58 65 L 58 100 L 42 100 L 42 65 L 10 65 \
         L 35 35 L 20 35 Z",
    ),
    built(
        "flower",
        Nature,
        n_("Flower"),
        n_("blossom petals plant"),
        flower,
    ),
    path(
        "mountain",
        Nature,
        n_("Mountains"),
        n_("hills peak landscape"),
        "M 0 100 L 35 30 L 50 55 L 68 20 L 100 100 Z",
    ),
    // Objects.
    path(
        "house",
        Objects,
        n_("House"),
        n_("home building"),
        "M 50 0 L 100 45 L 88 45 L 88 100 L 12 100 L 12 45 L 0 45 Z",
    ),
    path(
        "document",
        Objects,
        n_("Document"),
        n_("page paper file sheet"),
        "M 0 0 L 55 0 L 80 25 L 80 100 L 0 100 Z",
    ),
    path(
        "folder",
        Objects,
        n_("Folder"),
        n_("directory files"),
        "M 0 5 L 38 5 L 48 15 L 100 15 L 100 85 L 0 85 Z",
    ),
    built("tag", Objects, n_("Tag"), n_("label price"), || {
        format!(
            "M 30 0 L 100 0 L 100 60 L 30 60 L 0 30 Z {}",
            circle(24.0, 30.0, 7.0, true)
        )
    }),
    path(
        "light-bulb",
        Objects,
        n_("Light Bulb"),
        n_("idea lamp"),
        "M 50 0 C 78 0 95 20 95 42 C 95 58 85 68 72 78 L 72 88 L 28 88 L 28 78 \
         C 15 68 5 58 5 42 C 5 20 22 0 50 0 Z M 32 92 L 68 92 L 68 100 L 32 100 Z",
    ),
    path(
        "shield",
        Objects,
        n_("Shield"),
        n_("security protection badge"),
        "M 50 0 L 100 15 L 100 45 C 100 75 78 92 50 100 C 22 92 0 75 0 45 L 0 15 Z",
    ),
    path(
        "bookmark",
        Objects,
        n_("Bookmark"),
        n_("ribbon save"),
        "M 0 0 L 60 0 L 60 100 L 30 75 L 0 100 Z",
    ),
    built("gear", Objects, n_("Gear"), n_("settings cog wheel"), gear),
    path(
        "padlock",
        Objects,
        n_("Padlock"),
        n_("lock security closed"),
        "M 10 45 L 90 45 L 90 100 L 10 100 Z M 22 45 L 22 30 C 22 13 35 0 50 0 \
         C 65 0 78 13 78 30 L 78 45 L 66 45 L 66 30 C 66 20 59 12 50 12 \
         C 41 12 34 20 34 30 L 34 45 Z",
    ),
];

/// How far the control points of a Bézier curve that draws a quarter of a
/// circle are from its ends, for a radius of one.
const KAPPA: f64 = 0.552_284_749_8;

/// A circle, clockwise on the page, or the other way around for a hole.
fn circle(cx: f64, cy: f64, r: f64, hole: bool) -> String {
    let k = r * KAPPA;
    if hole {
        format!(
            "M {x1} {cy} C {x1} {a} {b} {y0} {cx} {y0} C {c} {y0} {x0} {a} {x0} {cy} \
             C {x0} {d} {c} {y1} {cx} {y1} C {b} {y1} {x1} {d} {x1} {cy} Z",
            x0 = cx - r,
            x1 = cx + r,
            y0 = cy - r,
            y1 = cy + r,
            a = cy - k,
            b = cx + k,
            c = cx - k,
            d = cy + k,
        )
    } else {
        format!(
            "M {x1} {cy} C {x1} {d} {b} {y1} {cx} {y1} C {c} {y1} {x0} {d} {x0} {cy} \
             C {x0} {a} {c} {y0} {cx} {y0} C {b} {y0} {x1} {a} {x1} {cy} Z",
            x0 = cx - r,
            x1 = cx + r,
            y0 = cy - r,
            y1 = cy + r,
            a = cy - k,
            b = cx + k,
            c = cx - k,
            d = cy + k,
        )
    }
}

/// The point at `radius` from the middle of a box of 100, at `degrees`
/// clockwise from the right.
fn polar(radius: f64, degrees: f64) -> (f64, f64) {
    let a = degrees * PI / 180.0;
    (50.0 + radius * a.cos(), 50.0 + radius * a.sin())
}

/// A closed part through points.
fn polygon(points: impl IntoIterator<Item = (f64, f64)>) -> String {
    let mut path = String::new();
    for (i, (x, y)) in points.into_iter().enumerate() {
        path.push_str(if i == 0 { "M " } else { " L " });
        path.push_str(&format!("{} {}", number(x), number(y)));
    }
    path.push_str(" Z");
    path
}

/// A disc and eight rays around it.
fn sun() -> String {
    let mut path = circle(50.0, 50.0, 22.0, false);
    for i in 0..8 {
        let a = f64::from(i) * 45.0;
        path.push(' ');
        path.push_str(&polygon([
            polar(30.0, a - 12.0),
            polar(50.0, a),
            polar(30.0, a + 12.0),
        ]));
    }
    path
}

/// Five petals around a heart.
fn flower() -> String {
    let mut path = circle(50.0, 50.0, 16.0, false);
    for i in 0..5 {
        let (x, y) = polar(28.0, -90.0 + f64::from(i) * 72.0);
        path.push(' ');
        path.push_str(&circle(x, y, 21.0, false));
    }
    path
}

/// A wheel of ten teeth, with a hole in the middle.
fn gear() -> String {
    let teeth = (0..10).flat_map(|i| {
        let c = f64::from(i) * 36.0;
        [
            polar(38.0, c - 11.0),
            polar(50.0, c - 6.0),
            polar(50.0, c + 6.0),
            polar(38.0, c + 11.0),
        ]
    });
    format!("{} {}", polygon(teeth), circle(50.0, 50.0, 14.0, true))
}

/// A number of a path, with at most two decimals.
fn number(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let text = format!("{rounded:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// Folds text to find it: in small letters, without accents.
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            other => other,
        })
        .collect()
}

/// Whether each word of `query` begins a word of `text`, whatever their
/// case and accents.
pub(crate) fn matches(text: &str, query: &str) -> bool {
    let text = fold(text);
    let words: Vec<&str> = text.split(|c: char| !c.is_alphanumeric()).collect();
    fold(query)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|q| !q.is_empty())
        .all(|q| words.iter().any(|w| w.starts_with(q)))
}

/// The most points of a path that the library reads.
const MAX_POINTS: usize = 100_000;

/// A point of a path while it is read.
#[derive(Clone, Copy)]
struct Point {
    at: (f64, f64),
    before: Option<(f64, f64)>,
    after: Option<(f64, f64)>,
}

/// A part of a path while it is read.
struct Part {
    points: Vec<Point>,
    closed: bool,
}

/// Reads the parts of a path, or none when it is not one the library
/// writes.
fn parts_of_path(data: &str) -> Option<Vec<Part>> {
    let mut tokens = data.split_ascii_whitespace();
    let mut parts: Vec<Part> = Vec::new();
    let mut count = 0usize;
    let next = |tokens: &mut std::str::SplitAsciiWhitespace| -> Option<(f64, f64)> {
        let mut value = || {
            let v: f64 = tokens.next()?.parse().ok()?;
            (v.is_finite() && v.abs() <= 1.0e6).then_some(v)
        };
        Some((value()?, value()?))
    };
    while let Some(command) = tokens.next() {
        match command {
            "M" => {
                if parts.len() >= MAX_SHAPE_PARTS {
                    return None;
                }
                let at = next(&mut tokens)?;
                parts.push(Part {
                    points: vec![Point {
                        at,
                        before: None,
                        after: None,
                    }],
                    closed: false,
                });
            }
            "L" | "C" => {
                let part = parts.last_mut().filter(|p| !p.closed)?;
                let point = if command == "L" {
                    Point {
                        at: next(&mut tokens)?,
                        before: None,
                        after: None,
                    }
                } else {
                    let a = next(&mut tokens)?;
                    let b = next(&mut tokens)?;
                    let at = next(&mut tokens)?;
                    if let Some(last) = part.points.last_mut() {
                        last.after = Some(a);
                    }
                    Point {
                        at,
                        before: Some(b),
                        after: None,
                    }
                };
                part.points.push(point);
            }
            "Z" => {
                let part = parts.last_mut().filter(|p| !p.closed)?;
                part.closed = true;
                // A part that comes back to its start ends on its first
                // point.
                let n = part.points.len();
                let (first, last) = (part.points[0].at, part.points[n - 1].at);
                if n > 1 && (first.0 - last.0).hypot(first.1 - last.1) < 1e-6 {
                    let last = part.points.pop()?;
                    part.points[0].before = last.before;
                }
            }
            _ => return None,
        }
        count += 1;
        if count > MAX_POINTS {
            return None;
        }
    }
    (!parts.is_empty()).then_some(parts)
}

/// Where a cubic Bézier curve turns back along an axis, from its ends
/// and control points along it: the values of `t` between 0 and 1 where
/// its derivative is nothing.
fn turns(p0: f64, p1: f64, p2: f64, p3: f64) -> Vec<f64> {
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let roots = if a.abs() < 1e-12 {
        if b.abs() < 1e-12 {
            Vec::new()
        } else {
            vec![-c / b]
        }
    } else {
        let d = b * b - 4.0 * a * c;
        if d < 0.0 {
            Vec::new()
        } else {
            let d = d.sqrt();
            vec![(-b + d) / (2.0 * a), (-b - d) / (2.0 * a)]
        }
    };
    roots.into_iter().filter(|t| *t > 0.0 && *t < 1.0).collect()
}

/// The point of a cubic Bézier curve at `t`.
fn bezier(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
}

/// The box of what the parts draw, as left, top, right and bottom: their
/// points and the furthest their curves go, not their handles, which may
/// reach beyond.
fn drawn_box(parts: &[Part]) -> (f64, f64, f64, f64) {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut take = |(x, y): (f64, f64)| {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    };
    for part in parts {
        let n = part.points.len();
        let pairs = (0..n.saturating_sub(1)).map(|i| (i, i + 1));
        let back = (part.closed && n > 1).then_some((n - 1, 0));
        for point in &part.points {
            take(point.at);
        }
        for (i, j) in pairs.chain(back) {
            let (from, to) = (part.points[i], part.points[j]);
            if from.after.is_none() && to.before.is_none() {
                continue;
            }
            let a = from.after.unwrap_or(from.at);
            let b = to.before.unwrap_or(to.at);
            let xs = (from.at.0, a.0, b.0, to.at.0);
            let ys = (from.at.1, a.1, b.1, to.at.1);
            for t in turns(xs.0, xs.1, xs.2, xs.3)
                .into_iter()
                .chain(turns(ys.0, ys.1, ys.2, ys.3))
            {
                take((
                    bezier(xs.0, xs.1, xs.2, xs.3, t),
                    bezier(ys.0, ys.1, ys.2, ys.3, t),
                ));
            }
        }
    }
    (x0, y0, x1, y1)
}

/// The shape that a path draws, as a pen shape of one part or a shape of
/// several parts, with the size of the box of what it draws in the units
/// of the path.
pub(crate) fn shape_of_path(data: &str) -> Option<(Shape, (f64, f64))> {
    let parts = parts_of_path(data)?;
    if parts.iter().any(|p| p.points.len() < 2) {
        return None;
    }
    let (x0, y0, x1, y1) = drawn_box(&parts);
    let (w, h) = (x1 - x0, y1 - y0);
    if w < 1e-3 || h < 1e-3 {
        return None;
    }
    let scale = f64::from(POLYGON_SCALE);
    let place = |(x, y): (f64, f64)| {
        (
            ((x - x0) / w * scale).round() as i32,
            ((y - y0) / h * scale).round() as i32,
        )
    };
    let points = |part: &Part| -> Arc<[PathPoint]> {
        part.points
            .iter()
            .map(|p| PathPoint {
                at: place(p.at),
                before: p.before.map(place),
                after: p.after.map(place),
            })
            .collect()
    };
    let shape = match &parts[..] {
        [part] => Shape::Path {
            points: points(part),
            closed: part.closed,
        },
        _ if parts.iter().all(|p| p.closed) => Shape::Compound(parts.iter().map(points).collect()),
        _ => return None,
    };
    Some((shape, (w, h)))
}

/// The path of a shape in a frame of `w` by `h` points, flipped across
/// as `flip.0` and up and down as `flip.1` say; none for a line or a
/// connection line, or an outline that cannot be read back.
pub(crate) fn path_of_shape(shape: &Shape, w: f64, h: f64, flip: (bool, bool)) -> Option<String> {
    if shape.is_line() || matches!(shape, Shape::Connection(_)) {
        return None;
    }
    let x = |x: f64| number(if flip.0 { w - x } else { x });
    let y = |y: f64| number(if flip.1 { h - y } else { y });
    let mut path = Vec::new();
    for segment in shape.outline(w, h) {
        match segment {
            PathSegment::Move(px, py) => path.push(format!("M {} {}", x(px), y(py))),
            PathSegment::Line(px, py) => path.push(format!("L {} {}", x(px), y(py))),
            PathSegment::Curve(ax, ay, bx, by, px, py) => path.push(format!(
                "C {} {} {} {} {} {}",
                x(ax),
                y(ay),
                x(bx),
                y(by),
                x(px),
                y(py)
            )),
            PathSegment::Close => path.push("Z".to_owned()),
        }
    }
    let path = path.join(" ");
    // Only what can be read back is kept.
    shape_of_path(&path).map(|_| path)
}

/// A shape of My Shapes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Saved {
    /// What names it in the actions, as long as it is kept.
    pub id: u64,
    pub name: String,
    pub path: String,
}

impl Saved {
    /// What it puts on the page, as large as it was saved.
    pub(crate) fn made(&self) -> Option<Made> {
        let (shape, (w, h)) = shape_of_path(&self.path)?;
        // As large as it was, within a page.
        let longer = w.max(h);
        let (w, h) = if longer > 400.0 {
            fitted(w, h, 400.0)
        } else if longer < 8.0 {
            fitted(w, h, 8.0)
        } else {
            (w, h)
        };
        Some(Made::Saved(shape, (w, h)))
    }

    /// Whether it is found with `query`, by its name.
    pub(crate) fn matches(&self, query: &str) -> bool {
        matches(&self.name, query)
    }

    /// Its line in the file.
    fn line(&self) -> String {
        format!("{:x}\t{}\t{}", self.id, clean_name(&self.name), self.path)
    }
}

/// The first line of the file of My Shapes.
const HEADER: &str = "# Desk My Shapes 1";

/// The most shapes My Shapes keeps, and the largest file it reads.
pub(crate) const MAX_SAVED: usize = 1_000;
const MAX_FILE: u64 = 16 * 1024 * 1024;

/// Where My Shapes are kept.
pub(crate) fn saved_path() -> PathBuf {
    gtk::glib::user_data_dir()
        .join("desk")
        .join("my-shapes.txt")
}

/// A line of the file of My Shapes: a shape this version reads, or one it
/// does not, kept as it is so that changing the others never loses it.
enum Line {
    Shape(Saved),
    Other(String),
}

/// The lines of a file of My Shapes: none when there is no file; an
/// error when it cannot be read whole, so that it is not written over.
fn read_lines(file: &Path) -> std::io::Result<Vec<Line>> {
    let size = match std::fs::metadata(file) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    if size > MAX_FILE {
        return Err(std::io::Error::other("the file of My Shapes is too large"));
    }
    let text = std::fs::read_to_string(file)?;
    let mut lines = Vec::new();
    let mut shapes = 0usize;
    let mut ids = std::collections::HashSet::new();
    for line in text.lines().filter(|l| !l.is_empty() && *l != HEADER) {
        let mut fields = line.splitn(3, '\t');
        let read = match (fields.next(), fields.next(), fields.next()) {
            (Some(id), Some(name), Some(path)) if shapes < MAX_SAVED => u64::from_str_radix(id, 16)
                .ok()
                .filter(|id| !ids.contains(id) && shape_of_path(path).is_some())
                .map(|id| Saved {
                    id,
                    name: clean_name(name),
                    path: path.to_owned(),
                }),
            _ => None,
        };
        match read {
            Some(saved) => {
                shapes += 1;
                ids.insert(saved.id);
                lines.push(Line::Shape(saved));
            }
            None => lines.push(Line::Other(line.to_owned())),
        }
    }
    Ok(lines)
}

/// The shapes of My Shapes in a file, in the order they were saved;
/// lines that are not shapes are left out.
pub(crate) fn read_saved(file: &Path) -> Vec<Saved> {
    read_lines(file)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|line| match line {
            Line::Shape(saved) => Some(saved),
            Line::Other(_) => None,
        })
        .collect()
}

/// Writes the lines of My Shapes to a file, all at once.
fn write_lines(file: &Path, lines: &[Line]) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let mut text = format!("{HEADER}\n");
    for line in lines {
        match line {
            Line::Shape(saved) => text.push_str(&saved.line()),
            Line::Other(other) => text.push_str(other),
        }
        text.push('\n');
    }
    // Written beside it first, so that a failure leaves the old file; by
    // a name of this process, so that two never write the same one.
    let mut part = file.as_os_str().to_owned();
    part.push(format!(".{}.part", std::process::id()));
    let part = PathBuf::from(part);
    std::fs::write(&part, text)?;
    std::fs::rename(&part, file)
}

/// A name on one line, of at most 200 letters.
fn clean_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(200)
        .collect();
    name.trim().to_owned()
}

/// Why a shape was not added to My Shapes.
#[derive(Debug)]
pub(crate) enum SaveError {
    /// It is a line, or its outline cannot be kept.
    Unsupported,
    /// My Shapes holds as many as it keeps.
    Full,
    /// Its file cannot be read or written.
    File(std::io::Error),
}

/// Adds a shape, flipped as `flip` says, to My Shapes in a file, and
/// returns it.
pub(crate) fn add_saved(
    file: &Path,
    name: &str,
    shape: &Shape,
    size: (f64, f64),
    flip: (bool, bool),
) -> Result<Saved, SaveError> {
    let path = path_of_shape(shape, size.0, size.1, flip).ok_or(SaveError::Unsupported)?;
    let mut lines = read_lines(file).map_err(SaveError::File)?;
    let saved: Vec<&Saved> = lines
        .iter()
        .filter_map(|l| match l {
            Line::Shape(s) => Some(s),
            Line::Other(_) => None,
        })
        .collect();
    if saved.len() >= MAX_SAVED {
        return Err(SaveError::Full);
    }
    let mut id = gtk::glib::real_time().unsigned_abs();
    while saved.iter().any(|s| s.id == id) || lines.iter().any(|l| taken(l, id)) {
        id = id.wrapping_add(1);
    }
    let name = clean_name(name);
    let shape = Saved {
        id,
        name: if name.is_empty() {
            tr("My Shape")
        } else {
            name
        },
        path,
    };
    lines.push(Line::Shape(shape.clone()));
    write_lines(file, &lines).map_err(SaveError::File)?;
    Ok(shape)
}

/// Whether a line not read as a shape still holds `id`, so that a new
/// shape does not take it.
fn taken(line: &Line, id: u64) -> bool {
    match line {
        Line::Shape(_) => false,
        Line::Other(text) => {
            text.split('\t')
                .next()
                .and_then(|i| u64::from_str_radix(i, 16).ok())
                == Some(id)
        }
    }
}

/// Changes a shape of My Shapes in a file, leaving the other lines as
/// they are; returns whether it was there and the file was written.
fn change_saved(file: &Path, id: u64, change: impl FnOnce(&mut Vec<Line>, usize)) -> bool {
    let Ok(mut lines) = read_lines(file) else {
        return false;
    };
    let Some(index) = lines
        .iter()
        .position(|l| matches!(l, Line::Shape(s) if s.id == id))
    else {
        return false;
    };
    change(&mut lines, index);
    match write_lines(file, &lines) {
        Ok(()) => true,
        Err(error) => {
            gtk::glib::g_warning!("desk", "cannot write My Shapes: {error}");
            false
        }
    }
}

/// Renames a shape of My Shapes in a file.
pub(crate) fn rename_saved(file: &Path, id: u64, name: &str) -> bool {
    let name = clean_name(name);
    if name.is_empty() {
        return false;
    }
    change_saved(file, id, |lines, index| {
        if let Line::Shape(shape) = &mut lines[index] {
            shape.name = name;
        }
    })
}

/// Takes a shape out of My Shapes in a file.
pub(crate) fn delete_saved(file: &Path, id: u64) -> bool {
    change_saved(file, id, |lines, index| {
        lines.remove(index);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many times the parts of a shape go around a point of its frame,
    /// through their corners and ends of curves.
    fn winding(shape: &Shape, (x, y): (i32, i32)) -> i32 {
        let parts: Vec<Arc<[PathPoint]>> = match shape {
            Shape::Path { points, .. } => vec![points.clone()],
            Shape::Compound(parts) => parts.to_vec(),
            _ => panic!("{shape:?}"),
        };
        let mut winding = 0;
        for part in parts {
            for (i, a) in part.iter().enumerate() {
                let b = part[(i + 1) % part.len()];
                let (ax, ay, bx, by) = (
                    f64::from(a.at.0),
                    f64::from(a.at.1),
                    f64::from(b.at.0),
                    f64::from(b.at.1),
                );
                let (x, y) = (f64::from(x), f64::from(y));
                let side = (bx - ax) * (y - ay) - (x - ax) * (by - ay);
                if ay <= y && by > y && side > 0.0 {
                    winding += 1;
                } else if ay > y && by <= y && side < 0.0 {
                    winding -= 1;
                }
            }
        }
        winding
    }

    #[test]
    fn w113_every_shape_of_the_library_is_made() {
        let mut ids = std::collections::HashSet::new();
        for entry in entries() {
            assert!(ids.insert(entry.id), "{} twice", entry.id);
            assert_ne!(entry.category, Category::Mine);
            let made = entry.made().unwrap_or_else(|| panic!("{}", entry.id));
            if let Made::Shape(shape, (w, h)) = made {
                assert!((w.max(h) - 100.0).abs() < 1e-6, "{}", entry.id);
                if let Shape::Compound(parts) = &shape {
                    assert!(parts.len() > 1);
                }
                // What it makes can be saved to My Shapes, with its parts.
                let path = path_of_shape(&shape, w, h, (false, false)).unwrap();
                let (again, size) = shape_of_path(&path).unwrap();
                assert_eq!(
                    matches!(again, Shape::Compound(_)),
                    matches!(shape, Shape::Compound(_)),
                    "{}",
                    entry.id
                );
                // A path fills its frame; a polygon or a star may not.
                if matches!(entry.outline, Outline::Path(_) | Outline::Built(_)) {
                    assert!(
                        (size.0 - w).abs() < 0.5 && (size.1 - h).abs() < 0.5,
                        "{}",
                        entry.id
                    );
                }
            }
        }
        for category in Category::ALL {
            assert!(
                category == Category::Mine || entries().iter().any(|e| e.category == category),
                "{category:?}"
            );
        }
    }

    #[test]
    fn w113_holes_are_left_open_and_parts_filled() {
        let middle = (POLYGON_SCALE / 2, POLYGON_SCALE / 2);
        for (id, at, covered) in [
            ("ring", middle, false),
            ("ring", (POLYGON_SCALE / 10, POLYGON_SCALE / 2), true),
            ("frame", middle, false),
            ("frame", (POLYGON_SCALE / 20, POLYGON_SCALE / 2), true),
            ("gear", middle, false),
            ("warning", (POLYGON_SCALE / 2, POLYGON_SCALE / 2), false),
            ("warning", (POLYGON_SCALE / 2, POLYGON_SCALE * 9 / 10), true),
            ("sun", middle, true),
            ("flower", middle, true),
            ("padlock", (POLYGON_SCALE / 2, POLYGON_SCALE * 3 / 4), true),
        ] {
            let Some(Made::Shape(shape, _)) = entry(id).and_then(Entry::made) else {
                panic!("{id}");
            };
            assert_eq!(winding(&shape, at) != 0, covered, "{id} at {at:?}");
        }
    }

    #[test]
    fn w113_shapes_are_found_by_their_names_and_words() {
        let found = |query: &str| -> Vec<&str> {
            entries()
                .iter()
                .filter(|e| e.matches(query))
                .map(|e| e.id)
                .collect()
        };
        assert_eq!(found("hea"), ["heart"]);
        assert!(found("arrow").contains(&"arrow-up"));
        assert!(found("ARROW up").contains(&"arrow-up"));
        assert!(!found("arrow up").contains(&"arrow-down"));
        assert!(found("circle").contains(&"ellipse"));
        assert!(found("zzz").is_empty());
        assert!(matches("Coração", "coracao"));
        assert!(matches("ice cream", "cre"));
        assert!(!matches("ice cream", "ream"));
    }

    #[test]
    fn w113_paths_are_read_with_care() {
        assert!(shape_of_path("").is_none());
        assert!(shape_of_path("L 0 0").is_none());
        assert!(shape_of_path("M 0 0 L 10").is_none());
        assert!(shape_of_path("M 0 0 L 10 10 X").is_none());
        assert!(shape_of_path("M 0 0 L NaN 10 L 0 10 Z").is_none());
        assert!(shape_of_path("M 0 0 L 1e9 10 L 0 10 Z").is_none());
        // A line of no height makes no shape.
        assert!(shape_of_path("M 0 0 L 10 0").is_none());
        // Parts of a shape of several are closed.
        assert!(shape_of_path("M 0 0 L 10 0 L 10 10 Z M 20 0 L 30 10").is_none());
        // Nothing is drawn on from a closed part.
        assert!(shape_of_path("M 0 0 L 10 0 L 10 10 Z L 5 5").is_none());
        let many = "M 0 0 L 1 0 L 1 1 Z ".repeat(MAX_SHAPE_PARTS + 1);
        assert!(shape_of_path(&many).is_none());
        let (shape, size) = shape_of_path("M 10 20 L 50 20 L 50 40 Z").unwrap();
        assert_eq!(size, (40.0, 20.0));
        let Shape::Path { points, closed } = shape else {
            panic!("{shape:?}");
        };
        assert!(closed);
        assert_eq!(points[1].at, (POLYGON_SCALE, 0));
        // A part that ends where it began does not repeat its first point.
        let (circle, _) = shape_of_path(&circle(0.0, 0.0, 5.0, false)).unwrap();
        let Shape::Path { points, .. } = circle else {
            panic!();
        };
        assert_eq!(points.len(), 4);
        assert!(points[0].before.is_some());
    }

    #[test]
    fn w113_my_shapes_are_saved_renamed_and_deleted() {
        let folder = std::env::temp_dir().join(format!("desk-my-shapes-{}", std::process::id()));
        let file = folder.join("my-shapes.txt");
        let _ = std::fs::remove_dir_all(&folder);
        assert!(read_saved(&file).is_empty());
        let none = (false, false);
        let oval = add_saved(&file, "Oval\tone\n", &Shape::Ellipse, (60.0, 30.0), none).unwrap();
        assert_eq!(oval.name, "Oval one");
        let Some(Made::Saved(Shape::Path { points, closed }, (w, h))) = oval.made() else {
            panic!("{oval:?}");
        };
        assert!(closed);
        assert_eq!(points.len(), 4);
        assert!((w - 60.0).abs() < 0.01 && (h - 30.0).abs() < 0.01);
        // A line is no shape to keep.
        let line = Shape::Line { rising: false };
        assert!(matches!(
            add_saved(&file, "Line", &line, (10.0, 10.0), none),
            Err(SaveError::Unsupported)
        ));
        let ring = entry("ring").and_then(Entry::made);
        let Some(Made::Shape(ring, size)) = ring else {
            panic!();
        };
        let kept = add_saved(&file, "", &ring, size, none).unwrap();
        assert_eq!(kept.name, tr("My Shape"));
        assert!(matches!(
            kept.made(),
            Some(Made::Saved(Shape::Compound(_), _))
        ));
        assert_ne!(kept.id, oval.id);
        assert_eq!(read_saved(&file), [oval.clone(), kept.clone()]);
        assert!(rename_saved(&file, oval.id, "Egg"));
        assert!(!rename_saved(&file, oval.id, "  "));
        assert_eq!(read_saved(&file)[0].name, "Egg");
        assert!(delete_saved(&file, oval.id));
        assert!(!delete_saved(&file, oval.id));
        assert_eq!(read_saved(&file), std::slice::from_ref(&kept));
        // Lines that are not shapes are left out, and kept when the
        // others change, as one of a later version would be.
        let mut text = std::fs::read_to_string(&file).unwrap();
        let unread = [
            "zz\tBad\tM 0 0",
            "not a shape",
            "2\tLater\tM 0 0 Q 1 0 1 1 Z",
        ];
        for line in unread {
            text.push_str(line);
            text.push('\n');
        }
        text.push_str("1\tSame\tM 0 0 L 1 0 L 1 1 Z\n");
        text.push_str(&format!("{:x}\tTwice\tM 0 0 L 1 0 L 1 1 Z\n", kept.id));
        std::fs::write(&file, text).unwrap();
        let read = read_saved(&file);
        assert_eq!(read.len(), 2);
        assert_eq!(read[1].name, "Same");
        assert!(rename_saved(&file, 1, "Other"));
        assert!(delete_saved(&file, kept.id));
        let added = add_saved(&file, "New", &Shape::Rectangle, (10.0, 10.0), none).unwrap();
        assert_ne!(added.id, 2);
        let text = std::fs::read_to_string(&file).unwrap();
        for line in unread {
            assert!(text.lines().any(|l| l == line), "{line} lost");
        }
        // The shape whose id was taken twice comes back once the first
        // is deleted.
        let names: Vec<String> = read_saved(&file).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["Other", "Twice", "New"]);
        // A flipped shape is kept as it shows.
        let right = desk_model::Shape::Polygon(Arc::from(
            &[(0, 0), (POLYGON_SCALE, POLYGON_SCALE), (0, POLYGON_SCALE)][..],
        ));
        let flipped = add_saved(&file, "Flipped", &right, (10.0, 10.0), (true, false)).unwrap();
        assert!(flipped.path.starts_with("M 10 0 "), "{}", flipped.path);
        // A curve whose handles reach out of it is as large as it draws.
        let (_, size) = shape_of_path("M 0 0 C 0 -10 10 -10 10 0 L 10 10 L 0 10 Z").unwrap();
        assert!(
            (size.0 - 10.0).abs() < 1e-9 && (size.1 - 17.5).abs() < 1e-9,
            "{size:?}"
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn w113_shapes_of_the_library_go_on_the_page_and_back_to_my_shapes() {
        crate::with_gtk(|| {
            let document = desk_model::Document::new_word_processing();
            let editor =
                crate::editing::TextEditor::new(&crate::view::DocumentView::new(), document);
            let shape = |editor: &crate::editing::TextEditor| {
                let info = editor.object_info().unwrap();
                let desk_model::ObjectKind::Shape(shape) = info.object.kind else {
                    panic!();
                };
                (shape, info.object.frame)
            };
            editor.insert_library_shape(entry("ring").and_then(Entry::made).unwrap());
            let (ring, frame) = shape(&editor);
            assert!(matches!(ring, Shape::Compound(ref parts) if parts.len() == 2));
            assert!((frame.width.to_pt() - 100.0).abs() < 0.01);
            let (saved, size, flip) = editor.shape_to_save().unwrap();
            assert_eq!(flip, (false, false));
            assert_eq!(saved, ring);
            assert!((size.0 - 100.0).abs() < 0.01);
            // The shapes of Insert > Shape come as it makes them.
            editor.insert_library_shape(entry("star").and_then(Entry::made).unwrap());
            assert!(matches!(shape(&editor).0, Shape::Star { .. }));
            // A shape of My Shapes comes as large as it was saved.
            let kept = Saved {
                id: 1,
                name: "Wide".to_owned(),
                path: "M 0 0 L 200 0 L 200 50 L 0 50 Z".to_owned(),
            };
            editor.insert_library_shape(kept.made().unwrap());
            let (_, frame) = shape(&editor);
            assert!((frame.width.to_pt() - 200.0).abs() < 0.01);
            assert!((frame.height.to_pt() - 50.0).abs() < 0.01);
            // A line is not saved to My Shapes.
            editor.insert_library_shape(entry("line").and_then(Entry::made).unwrap());
            assert!(editor.shape_to_save().is_none());
        });
    }
}
