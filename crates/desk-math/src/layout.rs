// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The layout of MathML: boxes of text set in rows, fractions, roots,
//! scripts, limits and tables, after the rules of TeX in a simpler form.
//!
//! The caller measures the width of text in its fonts; heights are kept
//! as fractions of the size, so that the same equation lays out the same
//! with any font.

use crate::xml::{Node, parse};

/// Measures the width of text at a size, in italic or bold.
pub type Measure<'a> = &'a dyn Fn(&str, f32, bool, bool) -> f32;

/// What an equation draws, from the top left corner of its box.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// The width.
    pub width: f32,
    /// The height.
    pub height: f32,
    /// How far the baseline is from the top, which the line of text that
    /// holds the equation shares.
    pub baseline: f32,
    /// What it draws, in order.
    pub marks: Vec<Mark>,
}

/// One thing that an equation draws.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// Text on a baseline.
    Text {
        /// Where the text starts.
        x: f32,
        /// Where its baseline is.
        y: f32,
        /// The text.
        text: String,
        /// Its size, in points.
        size: f32,
        /// Whether it is italic.
        italic: bool,
        /// Whether it is bold.
        bold: bool,
    },
    /// A filled rectangle: the bar of a fraction, or the line over a root.
    Rule {
        /// The left.
        x: f32,
        /// The top.
        y: f32,
        /// The width.
        width: f32,
        /// The height.
        height: f32,
    },
    /// A line through points: a radical sign, or a delimiter that grows.
    Stroke {
        /// The points.
        points: Vec<(f32, f32)>,
        /// The width of the line.
        width: f32,
    },
}

impl Mark {
    fn moved(mut self, dx: f32, dy: f32) -> Self {
        match &mut self {
            Mark::Text { x, y, .. } | Mark::Rule { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            Mark::Stroke { points, .. } => {
                for p in points {
                    p.0 += dx;
                    p.1 += dy;
                }
            }
        }
        self
    }
}

/// Lays out MathML at a size in points. An empty drawing when it is not
/// MathML that can be read.
pub fn draw(mathml: &str, size: f32, measure: Measure<'_>) -> Drawing {
    let Some(root) = parse(mathml) else {
        return Drawing::default();
    };
    let display = root.get("display") == Some("block")
        || root.get("displaystyle").is_some_and(|v| v == "true");
    let context = Context {
        size,
        base: size,
        display,
        level: 0,
        italic: None,
        bold: false,
    };
    let laid = Layout { measure }.row(&root.children, &context);
    let pad = size * 0.08;
    Drawing {
        width: laid.width + 2.0 * pad,
        height: laid.ascent + laid.descent + 2.0 * pad,
        baseline: laid.ascent + pad,
        marks: laid
            .marks
            .into_iter()
            .map(|m| m.moved(pad, laid.ascent + pad))
            .collect(),
    }
}

/// How a box spaces itself from its neighbours in a row, as in TeX.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Ordinary,
    LargeOperator,
    /// The name of a function, as sin, spaced as a large operator but
    /// for the parenthesis right after it.
    Function,
    Binary,
    Relation,
    Open,
    Close,
    Punctuation,
}

/// A box laid out: marks from its left end on its baseline, which `y`
/// counts downwards from.
#[derive(Clone, Debug)]
struct Laid {
    width: f32,
    ascent: f32,
    descent: f32,
    marks: Vec<Mark>,
    class: Class,
    /// A large operator whose limits go beside it out of display.
    movable: bool,
    /// Space before and after it, when it is an operator.
    space: Option<(f32, f32)>,
    /// A fence that grows with what it encloses.
    fence: Option<char>,
    /// How much further right a superscript goes than a subscript, past
    /// the slant of an integral.
    slant: f32,
}

impl Laid {
    fn empty() -> Self {
        Self {
            width: 0.0,
            ascent: 0.0,
            descent: 0.0,
            marks: Vec::new(),
            class: Class::Ordinary,
            movable: false,
            space: None,
            fence: None,
            slant: 0.0,
        }
    }

    fn put(&mut self, other: Laid, dx: f32, dy: f32) {
        self.marks
            .extend(other.marks.into_iter().map(|m| m.moved(dx, dy)));
    }
}

#[derive(Clone, Debug)]
struct Context {
    size: f32,
    base: f32,
    display: bool,
    level: u8,
    /// The style that `mathvariant` gives the text inside.
    italic: Option<bool>,
    bold: bool,
}

impl Context {
    /// The context of scripts and of what a fraction holds out of
    /// display.
    fn script(&self) -> Self {
        Self {
            size: (self.size * 0.71).max(self.base * 0.5),
            display: false,
            level: self.level.saturating_add(1),
            ..self.clone()
        }
    }

    /// The context of what a fraction holds.
    fn fraction(&self) -> Self {
        if self.display {
            Self {
                display: false,
                ..self.clone()
            }
        } else {
            self.script()
        }
    }

    /// The height of the axis that fractions and operators sit on.
    fn axis(&self) -> f32 {
        self.size * 0.25
    }

    fn rule(&self) -> f32 {
        (self.size * 0.05).max(0.4)
    }
}

const ASCENT: f32 = 0.72;
const DESCENT: f32 = 0.22;

struct Layout<'a> {
    measure: Measure<'a>,
}

impl Layout<'_> {
    fn text(&self, text: &str, c: &Context, italic: bool, bold: bool) -> Laid {
        let width = (self.measure)(text, c.size, italic, bold);
        // Italic letters lean past their width.
        let width = if italic { width + c.size * 0.04 } else { width };
        Laid {
            width,
            ascent: c.size * ASCENT,
            descent: c.size * DESCENT,
            marks: vec![Mark::Text {
                x: 0.0,
                y: 0.0,
                text: text.into(),
                size: c.size,
                italic,
                bold,
            }],
            ..Laid::empty()
        }
    }

    fn node(&self, node: &Node, c: &Context) -> Laid {
        let c = &styled(node, c);
        match node.name.as_str() {
            "mi" => {
                let text = variant(&node.content(), node.get("mathvariant"));
                let single = text.chars().count() == 1;
                let upright_greek = text.chars().all(|ch| ('Α'..='Ω').contains(&ch));
                let italic = c.italic.unwrap_or(single && !upright_greek);
                let mut laid = self.text(&text, c, italic, c.bold);
                // A name of a function, as sin or log, keeps a thin space
                // from what it applies to, as in TeX.
                if !single && text.chars().all(char::is_alphabetic) {
                    laid.class = Class::Function;
                }
                laid
            }
            "mn" | "mtext" | "ms" => {
                let mut text = node.content();
                if node.name == "ms" {
                    text = format!("\u{201c}{text}\u{201d}");
                }
                self.text(&text, c, c.italic.unwrap_or(false), c.bold)
            }
            "mo" => self.operator(node, c),
            "mspace" => Laid {
                width: node
                    .get("width")
                    .and_then(|w| length(w, c.size))
                    .unwrap_or(0.0)
                    .max(0.0),
                ascent: node
                    .get("height")
                    .and_then(|h| length(h, c.size))
                    .unwrap_or(0.0),
                descent: node
                    .get("depth")
                    .and_then(|d| length(d, c.size))
                    .unwrap_or(0.0),
                ..Laid::empty()
            },
            "mfrac" => self.fraction(node, c),
            "msqrt" => {
                let inner = self.row(&node.children, c);
                self.root(inner, None, c)
            }
            "mroot" => {
                let inner = self.child(node, 0, c);
                let index_context = c.script().script();
                let index = self.child(node, 1, &index_context);
                self.root(inner, Some(index), c)
            }
            "msub" | "msup" | "msubsup" => {
                let base = self.child(node, 0, c);
                let script = c.script();
                let (sub, sup) = match node.name.as_str() {
                    "msub" => (Some(self.child(node, 1, &script)), None),
                    "msup" => (None, Some(self.child(node, 1, &script))),
                    _ => (
                        Some(self.child(node, 1, &script)),
                        Some(self.child(node, 2, &script)),
                    ),
                };
                self.scripts(base, sub, sup, c)
            }
            "munder" | "mover" | "munderover" => self.limits(node, c),
            "mtable" => self.table(node, c),
            "mfenced" => self.fenced(node, c),
            "mphantom" => {
                let mut laid = self.row(&node.children, c);
                laid.marks.clear();
                laid
            }
            "semantics" | "maction" => node
                .children
                .iter()
                .find(|n| !n.name.starts_with("annotation"))
                .map_or_else(Laid::empty, |n| self.node(n, c)),
            "annotation" | "annotation-xml" | "none" | "mprescripts" => Laid::empty(),
            "mpadded" => {
                let mut laid = self.row(&node.children, c);
                if let Some(width) = node.get("width").and_then(|w| length(w, c.size)) {
                    laid.width = width.max(0.0);
                }
                laid
            }
            // mrow, mstyle, menclose, merror, and what is not known.
            _ => {
                let mut laid = self.row(&node.children, c);
                if node.children.len() == 1 {
                    // A row of one operator spaces as it does.
                    return laid;
                }
                laid.class = Class::Ordinary;
                laid.space = None;
                laid.movable = false;
                laid.fence = None;
                laid
            }
        }
    }

    fn child(&self, node: &Node, at: usize, c: &Context) -> Laid {
        node.children
            .get(at)
            .map_or_else(Laid::empty, |n| self.node(n, c))
    }

    fn operator(&self, node: &Node, c: &Context) -> Laid {
        let raw = node.content();
        let text = match raw.as_str() {
            "-" => "−".to_owned(),
            "*" => "∗".to_owned(),
            "'" => "′".to_owned(),
            // The invisible operators of MathML take no room.
            "\u{2061}" | "\u{2062}" | "\u{2063}" | "\u{2064}" => {
                return Laid::empty();
            }
            _ => raw,
        };
        let one = {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Some(ch),
                _ => None,
            }
        };
        let large = one.is_some_and(|ch| LARGE.contains(ch));
        let integral = one.is_some_and(|ch| INTEGRALS.contains(ch));
        let named = text.chars().count() > 1 && text.chars().all(char::is_alphabetic);
        let class = match one {
            _ if large || named => Class::LargeOperator,
            Some(ch) if RELATIONS.contains(ch) => Class::Relation,
            Some(ch) if BINARY.contains(ch) => Class::Binary,
            Some(ch) if OPENING.contains(ch) => Class::Open,
            Some(ch) if CLOSING.contains(ch) => Class::Close,
            Some(ch) if PUNCTUATION.contains(ch) => Class::Punctuation,
            _ => Class::Ordinary,
        };
        let mut laid = if large && c.display {
            let mut big = c.clone();
            big.size = c.size * if integral { 1.8 } else { 1.45 };
            let mut laid = self.text(&text, &big, false, c.bold);
            // Centred on the axis.
            let height = big.size * (ASCENT + DESCENT);
            let shift = (laid.ascent - laid.descent) / 2.0 - c.axis();
            laid.ascent = height / 2.0 + c.axis();
            laid.descent = height / 2.0 - c.axis();
            laid.marks = laid
                .marks
                .into_iter()
                .map(|m| m.moved(0.0, shift))
                .collect();
            laid
        } else {
            self.text(&text, c, false, c.bold)
        };
        laid.class = class;
        if integral {
            laid.slant = laid.width * 0.45;
        }
        laid.movable = (large && !integral) || named || node.get("movablelimits") == Some("true");
        if node.get("movablelimits") == Some("false") {
            laid.movable = false;
        }
        let fence = one.filter(|ch| {
            OPENING.contains(*ch) || CLOSING.contains(*ch) || *ch == '|' || *ch == '‖'
        });
        let minsize = node
            .get("minsize")
            .and_then(|v| length(v, c.size))
            .filter(|v| *v > 0.0);
        if let (Some(fence), Some(height)) = (fence, minsize) {
            // A delimiter of a size, as \big( makes, keeps it.
            let (class, movable) = (laid.class, laid.movable);
            laid = delimiter(fence, height / 2.0, c);
            laid.class = class;
            laid.movable = movable;
        } else if node.get("stretchy") != Some("false") {
            laid.fence = fence;
        }
        let explicit = |name: &str| node.get(name).and_then(|v| length(v, c.size));
        if explicit("lspace").is_some() || explicit("rspace").is_some() {
            laid.space = Some((
                explicit("lspace").unwrap_or(0.0),
                explicit("rspace").unwrap_or(0.0),
            ));
        }
        laid
    }

    /// Children set in a row, spaced by their classes, with the fences
    /// grown to what they enclose.
    fn row(&self, children: &[Node], c: &Context) -> Laid {
        let mut items: Vec<Laid> = children.iter().map(|n| self.node(n, c)).collect();
        if items.len() == 1 {
            return items.pop().unwrap_or_else(Laid::empty);
        }
        let inner = items
            .iter()
            .filter(|l| l.fence.is_none())
            .fold((0.0f32, 0.0f32), |(a, d), l| {
                (a.max(l.ascent), d.max(l.descent))
            });
        let half = (inner.0 - c.axis()).max(inner.1 + c.axis());
        if 2.0 * half > c.size * 1.25 {
            for item in &mut items {
                if let Some(fence) = item.fence {
                    let (class, space) = (item.class, item.space);
                    *item = delimiter(fence, half, c);
                    item.class = class;
                    item.space = space;
                }
            }
        }
        let mut row = Laid::empty();
        let mut previous: Option<Class> = None;
        // The space after the box before, which what comes next may drop.
        let mut pending = 0.0;
        let count = items.len();
        for (i, item) in items.into_iter().enumerate() {
            let mut class = item.class;
            // A sign with nothing before it, as the minus of -x, is not a
            // binary operator.
            if class == Class::Binary
                && (i + 1 == count
                    || matches!(
                        previous,
                        None | Some(
                            Class::Binary
                                | Class::Relation
                                | Class::Open
                                | Class::Punctuation
                                | Class::LargeOperator
                                | Class::Function
                        )
                    ))
            {
                class = Class::Ordinary;
            }
            let (before, after) = item.space.unwrap_or_else(|| spacing(class, c));
            if let Some(previous) = previous {
                // A function takes no space before a fence or a comma, as
                // the sin of sin(x).
                let tight = previous == Class::Function
                    && matches!(class, Class::Open | Class::Close | Class::Punctuation);
                row.width += before + if tight { 0.0 } else { pending };
            }
            row.ascent = row.ascent.max(item.ascent);
            row.descent = row.descent.max(item.descent);
            let x = row.width;
            row.width += item.width;
            pending = if i + 1 < count { after } else { 0.0 };
            row.put(item, x, 0.0);
            previous = Some(class);
        }
        row
    }

    fn fenced(&self, node: &Node, c: &Context) -> Laid {
        let open = node.get("open").unwrap_or("(");
        let close = node.get("close").unwrap_or(")");
        let separators: Vec<char> = node
            .get("separators")
            .unwrap_or(",")
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        let mut row = Vec::new();
        if !open.is_empty() {
            row.push(Node::token("mo", open));
        }
        for (i, child) in node.children.iter().enumerate() {
            if i > 0
                && let Some(sep) = separators.get(i - 1).or(separators.last())
            {
                row.push(Node::token("mo", &sep.to_string()));
            }
            row.push(child.clone());
        }
        if !close.is_empty() {
            row.push(Node::token("mo", close));
        }
        self.row(&row, c)
    }

    fn fraction(&self, node: &Node, c: &Context) -> Laid {
        let inner = c.fraction();
        let numerator = self.child(node, 0, &inner);
        let denominator = self.child(node, 1, &inner);
        let thickness = match node.get("linethickness") {
            Some("thin") => c.rule() / 2.0,
            Some("medium") => c.rule(),
            Some("thick") => c.rule() * 2.0,
            // A bare number is a multiple of the default rule.
            Some(value) => match value.trim().parse::<f32>() {
                Ok(times) => times * c.rule(),
                Err(_) => length(value, c.size).unwrap_or(c.rule()),
            },
            None => c.rule(),
        }
        .clamp(0.0, c.size);
        let thickness = if thickness.is_finite() {
            thickness
        } else {
            c.rule()
        };
        let gap = if c.display {
            c.size * 0.15
        } else {
            c.size * 0.1
        };
        let pad = c.size * 0.12;
        let width = numerator.width.max(denominator.width) + 2.0 * pad;
        let up = c.axis() + thickness / 2.0 + gap + numerator.descent;
        let down = -c.axis() + thickness / 2.0 + gap + denominator.ascent;
        let mut laid = Laid {
            width,
            ascent: up + numerator.ascent,
            descent: down + denominator.descent,
            ..Laid::empty()
        };
        if thickness > 0.0 {
            laid.marks.push(Mark::Rule {
                x: pad / 2.0,
                y: -c.axis() - thickness / 2.0,
                width: width - pad,
                height: thickness,
            });
        }
        let nx = (width - numerator.width) / 2.0;
        let dx = (width - denominator.width) / 2.0;
        laid.put(numerator, nx, -up);
        laid.put(denominator, dx, down);
        laid
    }

    fn root(&self, inner: Laid, index: Option<Laid>, c: &Context) -> Laid {
        let thickness = c.rule();
        let gap = c.size * 0.12;
        let top = inner.ascent.max(c.size * ASCENT) + gap + thickness;
        let bottom = inner.descent.max(c.size * DESCENT);
        let sign = c.size * 0.6;
        let lead = index
            .as_ref()
            .map_or(0.0, |i| (i.width - sign * 0.45).max(0.0));
        let height = top + bottom;
        let tick = (lead + sign * 0.08, bottom - height * 0.42);
        let points = vec![
            tick,
            (lead + sign * 0.2, tick.1 - c.size * 0.06),
            (lead + sign * 0.45, bottom),
            (lead + sign, -top + thickness / 2.0),
            (
                lead + sign + inner.width + c.size * 0.1,
                -top + thickness / 2.0,
            ),
        ];
        let mut laid = Laid {
            width: lead + sign + inner.width + c.size * 0.15,
            ascent: top,
            descent: bottom,
            ..Laid::empty()
        };
        laid.marks.push(Mark::Stroke {
            points,
            width: thickness,
        });
        if let Some(index) = index {
            let raise = -tick.1 + index.descent + c.size * 0.1;
            laid.ascent = laid.ascent.max(raise + index.ascent);
            let x = (lead + sign * 0.45 - index.width).max(0.0);
            laid.put(index, x, -raise);
        }
        laid.put(inner, lead + sign + c.size * 0.05, 0.0);
        laid
    }

    fn scripts(&self, base: Laid, sub: Option<Laid>, sup: Option<Laid>, c: &Context) -> Laid {
        let script = c.script();
        let mut laid = Laid {
            class: base.class,
            space: base.space,
            ..Laid::empty()
        };
        let (base_width, base_ascent, base_descent) = (base.width, base.ascent, base.descent);
        let slant = base.slant;
        laid.width = base_width;
        laid.ascent = base_ascent;
        laid.descent = base_descent;
        laid.put(base, 0.0, 0.0);
        let mut up = sup.as_ref().map_or(0.0, |s| {
            (base_ascent - script.size * 0.35)
                .max(c.size * 0.4)
                .max(s.descent + c.size * 0.15)
        });
        let mut down = sub.as_ref().map_or(0.0, |s| {
            (base_descent + script.size * 0.1)
                .max(c.size * 0.15)
                .max(s.ascent - c.size * 0.4)
        });
        if let (Some(sup), Some(sub)) = (&sup, &sub) {
            // The two scripts keep apart.
            let clash = c.size * 0.2 - ((up - sup.descent) - (sub.ascent - down));
            if clash > 0.0 {
                up += clash / 2.0;
                down += clash / 2.0;
            }
        }
        let x = base_width + c.size * 0.03;
        let mut end = laid.width;
        if let Some(sup) = sup {
            laid.ascent = laid.ascent.max(up + sup.ascent);
            end = end.max(x + slant / 2.0 + sup.width);
            laid.put(sup, x + slant / 2.0, -up);
        }
        if let Some(sub) = sub {
            laid.descent = laid.descent.max(down + sub.descent);
            end = end.max(x - slant / 2.0 + sub.width);
            laid.put(sub, x - slant / 2.0, down);
        }
        laid.width = end + c.size * 0.05;
        laid
    }

    fn limits(&self, node: &Node, c: &Context) -> Laid {
        let base = self.child(node, 0, c);
        let accent_over = node.get("accent") == Some("true");
        let accent_under = node.get("accentunder") == Some("true");
        let (under_at, over_at) = match node.name.as_str() {
            "munder" => (Some(1), None),
            "mover" => (None, Some(1)),
            _ => (Some(1), Some(2)),
        };
        if base.movable && !c.display {
            let script = c.script();
            let sub = under_at.map(|i| self.child(node, i, &script));
            let sup = over_at.map(|i| self.child(node, i, &script));
            return self.scripts(base, sub, sup, c);
        }
        let over_node = over_at.and_then(|i| node.children.get(i));
        let under_node = under_at.and_then(|i| node.children.get(i));
        let over = over_node.map(|n| self.decoration(n, &base, accent_over, c));
        let under = under_node.map(|n| self.decoration(n, &base, accent_under, c));
        let width = [
            Some(base.width),
            over.as_ref().map(|o| o.width),
            under.as_ref().map(|u| u.width),
        ]
        .into_iter()
        .flatten()
        .fold(0.0f32, f32::max);
        let mut laid = Laid {
            width,
            ascent: base.ascent,
            descent: base.descent,
            class: base.class,
            space: base.space,
            ..Laid::empty()
        };
        let gap = c.size * 0.1;
        if let Some(over) = over {
            let accent = accent_over || is_accent(over_node);
            let up = base.ascent + over.descent + if accent { 0.0 } else { gap };
            laid.ascent = laid.ascent.max(up + over.ascent);
            laid.put(over.clone(), (width - over.width) / 2.0, -up);
        }
        if let Some(under) = under {
            let down = base.descent + under.ascent + gap;
            laid.descent = laid.descent.max(down + under.descent);
            laid.put(under.clone(), (width - under.width) / 2.0, down);
        }
        let x = (width - base.width) / 2.0;
        laid.put(base, x, 0.0);
        laid
    }

    /// What goes over or under a base: a line, an accent, or a limit.
    fn decoration(&self, node: &Node, base: &Laid, accent: bool, c: &Context) -> Laid {
        let text = node.content();
        if node.name == "mo" && matches!(text.as_str(), "¯" | "‾" | "_" | "―" | "─" | "̅") {
            let thickness = c.rule();
            return Laid {
                width: base.width,
                ascent: thickness + c.size * 0.05,
                descent: 0.0,
                marks: vec![Mark::Rule {
                    x: 0.0,
                    y: -thickness,
                    width: base.width,
                    height: thickness,
                }],
                ..Laid::empty()
            };
        }
        let accent = accent || is_accent(Some(node));
        let arrow = matches!(text.as_str(), "→" | "⃗" | "←" | "↔");
        let context = if accent && !arrow {
            c.clone()
        } else {
            c.script()
        };
        // The marks that ASCII writes as large as letters.
        let mark = match text.as_str() {
            "^" if accent => Some("ˆ"),
            "~" if accent => Some("˜"),
            _ => None,
        };
        let mut laid = match mark {
            Some(mark) => self.node(&Node::token("mo", mark), &context),
            None => self.node(node, &context),
        };
        if accent {
            // An accent is as high as its mark, which sits high in the
            // line of a letter, and an arrow in the middle of it.
            let drop = if arrow { 0.0 } else { c.size * 0.35 };
            laid.ascent = c.size * 0.3;
            laid.descent = 0.0;
            laid.marks = laid.marks.into_iter().map(|m| m.moved(0.0, drop)).collect();
        }
        laid
    }

    fn table(&self, node: &Node, c: &Context) -> Laid {
        let inner = Context {
            display: false,
            ..c.clone()
        };
        let rows: Vec<Vec<Laid>> = node
            .children
            .iter()
            .filter(|r| r.name == "mtr" || r.name == "mlabeledtr")
            .map(|r| {
                r.children
                    .iter()
                    .filter(|d| d.name == "mtd")
                    .map(|d| self.row(&d.children, &inner))
                    .collect()
            })
            .collect();
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![0.0f32; columns];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.width);
            }
        }
        let aligns: Vec<&str> = node
            .get("columnalign")
            .unwrap_or("center")
            .split_whitespace()
            .collect();
        let spacing: Vec<f32> = node
            .get("columnspacing")
            .unwrap_or("0.8em")
            .split_whitespace()
            // A value that cannot be read is the default gap.
            .map(|v| length(v, c.size).unwrap_or(c.size * 0.8).max(0.0))
            .collect();
        let column_gap = |i: usize| {
            spacing
                .get(i)
                .or(spacing.last())
                .copied()
                .unwrap_or(c.size * 0.8)
        };
        let row_gap = c.size * 0.3;
        let mut laid = Laid::empty();
        let mut y = 0.0f32;
        let mut placed = Vec::new();
        for row in rows {
            let ascent = row.iter().map(|l| l.ascent).fold(c.size * ASCENT, f32::max);
            let descent = row
                .iter()
                .map(|l| l.descent)
                .fold(c.size * DESCENT, f32::max);
            y += ascent;
            let mut x = 0.0;
            for (i, cell) in row.into_iter().enumerate() {
                let align = aligns.get(i).or(aligns.last()).copied().unwrap_or("center");
                let dx = match align {
                    "left" => 0.0,
                    "right" => widths[i] - cell.width,
                    _ => (widths[i] - cell.width) / 2.0,
                };
                placed.push((cell, x + dx, y));
                x += widths[i] + column_gap(i);
            }
            y += descent + row_gap;
        }
        let height = (y - row_gap).max(0.0);
        laid.width = widths.iter().sum::<f32>()
            + (0..columns.saturating_sub(1)).map(column_gap).sum::<f32>();
        // Centred on the axis.
        let top = height / 2.0 + c.axis();
        laid.ascent = top;
        laid.descent = height - top;
        for (cell, x, y) in placed {
            laid.put(cell, x, y - top);
        }
        laid
    }
}

/// The context inside an element, after its attributes of style.
fn styled(node: &Node, c: &Context) -> Context {
    let mut c = c.clone();
    if let Some(display) = node.get("displaystyle") {
        c.display = display == "true";
    }
    if let Some(level) = node.get("scriptlevel")
        && let Ok(level) = level.trim_start_matches('+').parse::<i8>()
    {
        for _ in 0..level.clamp(0, 4) {
            c = c.script();
        }
    }
    if let Some(variant) = node.get("mathvariant") {
        c.italic = Some(variant.contains("italic"));
        c.bold = variant.contains("bold");
    }
    c
}

/// Letters in a variant that Unicode has its own letters for.
fn variant(text: &str, variant: Option<&str>) -> String {
    let (capital, small): (u32, u32) = match variant {
        Some("double-struck") => (0x1D538, 0x1D552),
        Some("script") => (0x1D49C, 0x1D4B6),
        Some("fraktur") => (0x1D504, 0x1D51E),
        _ => return text.into(),
    };
    text.chars()
        .map(|ch| {
            // The letters that Unicode had before its mathematical ones.
            let older = match (variant, ch) {
                (Some("double-struck"), 'C') => Some('ℂ'),
                (Some("double-struck"), 'H') => Some('ℍ'),
                (Some("double-struck"), 'N') => Some('ℕ'),
                (Some("double-struck"), 'P') => Some('ℙ'),
                (Some("double-struck"), 'Q') => Some('ℚ'),
                (Some("double-struck"), 'R') => Some('ℝ'),
                (Some("double-struck"), 'Z') => Some('ℤ'),
                (Some("script"), 'B') => Some('ℬ'),
                (Some("script"), 'E') => Some('ℰ'),
                (Some("script"), 'F') => Some('ℱ'),
                (Some("script"), 'H') => Some('ℋ'),
                (Some("script"), 'I') => Some('ℐ'),
                (Some("script"), 'L') => Some('ℒ'),
                (Some("script"), 'M') => Some('ℳ'),
                (Some("script"), 'R') => Some('ℛ'),
                (Some("fraktur"), 'C') => Some('ℭ'),
                (Some("fraktur"), 'H') => Some('ℌ'),
                (Some("fraktur"), 'I') => Some('ℑ'),
                (Some("fraktur"), 'R') => Some('ℜ'),
                (Some("fraktur"), 'Z') => Some('ℨ'),
                _ => None,
            };
            older.unwrap_or_else(|| match ch {
                'A'..='Z' => char::from_u32(capital + (ch as u32 - 'A' as u32)).unwrap_or(ch),
                'a'..='z' => char::from_u32(small + (ch as u32 - 'a' as u32)).unwrap_or(ch),
                _ => ch,
            })
        })
        .collect()
}

fn is_accent(node: Option<&Node>) -> bool {
    node.is_some_and(|n| {
        n.name == "mo"
            && matches!(
                n.content().as_str(),
                "^" | "ˆ" | "˜" | "~" | "˙" | "¨" | "´" | "`" | "→" | "⃗" | "ˇ" | "˘" | "°"
            )
    })
}

/// The space before and after a box of a class, in a row.
fn spacing(class: Class, c: &Context) -> (f32, f32) {
    let em = c.size;
    let scripted = c.level > 0;
    match class {
        Class::Relation if !scripted => (em * 0.2778, em * 0.2778),
        Class::Binary if !scripted => (em * 0.2222, em * 0.2222),
        Class::LargeOperator | Class::Function => (em * 0.1667, em * 0.1667),
        Class::Punctuation => (0.0, em * 0.1667),
        _ => (0.0, 0.0),
    }
}

/// A length of MathML: in em, ex, points, pixels or percent of the size,
/// or one of the named spaces.
fn length(value: &str, size: f32) -> Option<f32> {
    let value = value.trim();
    let named = match value {
        "veryverythinmathspace" => Some(1.0),
        "verythinmathspace" => Some(2.0),
        "thinmathspace" => Some(3.0),
        "mediummathspace" => Some(4.0),
        "thickmathspace" => Some(5.0),
        "verythickmathspace" => Some(6.0),
        "veryverythickmathspace" => Some(7.0),
        "negativeveryverythinmathspace" => Some(-1.0),
        "negativeverythinmathspace" => Some(-2.0),
        "negativethinmathspace" => Some(-3.0),
        "negativemediummathspace" => Some(-4.0),
        "negativethickmathspace" => Some(-5.0),
        "negativeverythickmathspace" => Some(-6.0),
        "negativeveryverythickmathspace" => Some(-7.0),
        _ => None,
    };
    if let Some(eighteenths) = named {
        return Some(size * eighteenths / 18.0);
    }
    let split = value
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+'))
        .unwrap_or(value.len());
    let number: f32 = value[..split].parse().ok()?;
    let length = match value[split..].trim() {
        "em" => number * size,
        "ex" => number * size * 0.43,
        "pt" | "" => number,
        "px" => number * 0.75,
        "mm" => number * 72.0 / 25.4,
        "cm" => number * 72.0 / 2.54,
        "in" => number * 72.0,
        "%" => number / 100.0 * size,
        _ => return None,
    };
    length
        .is_finite()
        .then(|| length.clamp(-size * 100.0, size * 100.0))
}

/// A delimiter drawn as a line, `half` above and below the axis.
fn delimiter(fence: char, half: f32, c: &Context) -> Laid {
    let axis = c.axis();
    let (top, bottom) = (-(axis + half), half - axis);
    let w = c.size * 0.35;
    let width = (c.size * 0.04).max(0.5);
    let mid = -axis;
    let arc = |from: f32, to: f32| -> Vec<(f32, f32)> {
        // Half an ellipse from the top to the bottom, bulging to `to`.
        (0..=16)
            .map(|i| {
                let t = std::f32::consts::PI * i as f32 / 16.0;
                (from + (to - from) * t.sin(), mid - (half) * t.cos())
            })
            .collect()
    };
    let strokes: Vec<Vec<(f32, f32)>> = match fence {
        '(' => vec![arc(w * 0.85, w * 0.15)],
        ')' => vec![arc(w * 0.15, w * 0.85)],
        '[' => vec![vec![
            (w * 0.8, top),
            (w * 0.25, top),
            (w * 0.25, bottom),
            (w * 0.8, bottom),
        ]],
        ']' => vec![vec![
            (w * 0.2, top),
            (w * 0.75, top),
            (w * 0.75, bottom),
            (w * 0.2, bottom),
        ]],
        '⌈' => vec![vec![(w * 0.8, top), (w * 0.25, top), (w * 0.25, bottom)]],
        '⌉' => vec![vec![(w * 0.2, top), (w * 0.75, top), (w * 0.75, bottom)]],
        '⌊' => vec![vec![(w * 0.25, top), (w * 0.25, bottom), (w * 0.8, bottom)]],
        '⌋' => vec![vec![(w * 0.75, top), (w * 0.75, bottom), (w * 0.2, bottom)]],
        '{' | '}' => {
            let (a, b) = if fence == '{' {
                (w * 0.85, w * 0.45)
            } else {
                (w * 0.15, w * 0.55)
            };
            let tip = if fence == '{' { w * 0.1 } else { w * 0.9 };
            let r = (half * 0.15).min(w * 0.4);
            vec![vec![
                (a, top),
                (b, top + r),
                (b, mid - r),
                (tip, mid),
                (b, mid + r),
                (b, bottom - r),
                (a, bottom),
            ]]
        }
        '⟨' | '〈' => vec![vec![(w * 0.8, top), (w * 0.2, mid), (w * 0.8, bottom)]],
        '⟩' | '〉' => vec![vec![(w * 0.2, top), (w * 0.8, mid), (w * 0.2, bottom)]],
        '‖' => vec![
            vec![(w * 0.3, top), (w * 0.3, bottom)],
            vec![(w * 0.7, top), (w * 0.7, bottom)],
        ],
        _ => vec![vec![(w * 0.5, top), (w * 0.5, bottom)]],
    };
    Laid {
        width: w,
        ascent: -top,
        descent: bottom,
        marks: strokes
            .into_iter()
            .map(|points| Mark::Stroke { points, width })
            .collect(),
        ..Laid::empty()
    }
}

const LARGE: &str = "∑∏∐⋃⋂⨁⨂⨀⋁⋀∫∬∭∮∯∰";
const INTEGRALS: &str = "∫∬∭∮∯∰";
const RELATIONS: &str = "=<>≤≥≠≈≡∼≃≅∝→←↔⇒⇐⇔↦∈∉∋⊂⊃⊆⊇⊄≪≫≺≻⪯⪰∣∥⊥⊢⊨:≔";
const BINARY: &str = "+−±∓×÷·∗∘∙∪∩∧∨⊕⊗⊖⊙∖⋅⋆△▽†‡⨯";
const OPENING: &str = "([{⟨〈⌈⌊";
const CLOSING: &str = ")]}⟩〉⌉⌋";
const PUNCTUATION: &str = ",;";

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character half the size wide.
    fn measure(text: &str, size: f32, _: bool, _: bool) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }

    fn texts(drawing: &Drawing) -> Vec<&str> {
        drawing
            .marks
            .iter()
            .filter_map(|m| match m {
                Mark::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    const NS: &str = "xmlns=\"http://www.w3.org/1998/Math/MathML\"";

    #[test]
    fn w104_a_row_is_spaced_by_its_operators() {
        let drawing = draw(
            &format!(
                "<math {NS}><mi>a</mi><mo>+</mo><mi>b</mi><mo>=</mo><mo>-</mo><mn>1</mn></math>"
            ),
            10.0,
            &measure,
        );
        assert_eq!(texts(&drawing), ["a", "+", "b", "=", "−", "1"]);
        let xs: Vec<f32> = drawing
            .marks
            .iter()
            .map(|m| match m {
                Mark::Text { x, .. } => *x,
                _ => 0.0,
            })
            .collect();
        // The plus has room on both sides, the minus after the equals sign
        // none before the one.
        assert!(xs[1] - xs[0] > 5.0 + 2.0);
        assert!((xs[5] - xs[4] - 5.0).abs() < 0.01);
        let Mark::Text { italic, .. } = &drawing.marks[0] else {
            panic!()
        };
        assert!(*italic);
    }

    #[test]
    fn w104_a_fraction_has_its_bar_between_its_parts() {
        let drawing = draw(
            &format!("<math {NS}><mfrac><mn>1</mn><mn>2</mn></mfrac></math>"),
            10.0,
            &measure,
        );
        let ys: Vec<f32> = drawing
            .marks
            .iter()
            .map(|m| match m {
                Mark::Text { y, .. } | Mark::Rule { y, .. } => *y,
                Mark::Stroke { .. } => 0.0,
            })
            .collect();
        // The bar, then the numerator above it and the denominator below.
        assert!(matches!(drawing.marks[0], Mark::Rule { .. }));
        assert!(ys[1] < ys[0] && ys[0] < ys[2]);
        assert!(drawing.height > 15.0);
        assert!(drawing.baseline > 0.0 && drawing.baseline < drawing.height);
    }

    #[test]
    fn w104_a_root_and_its_scripts_are_drawn() {
        let drawing = draw(
            &format!(
                "<math {NS}><msqrt><msup><mi>x</mi><mn>2</mn></msup></msqrt>\
                 <mroot><mi>y</mi><mn>3</mn></mroot></math>"
            ),
            10.0,
            &measure,
        );
        assert_eq!(
            drawing
                .marks
                .iter()
                .filter(|m| matches!(m, Mark::Stroke { .. }))
                .count(),
            2
        );
        let size_of = |t: &str| {
            drawing.marks.iter().find_map(|m| match m {
                Mark::Text { text, size, .. } if text == t => Some(*size),
                _ => None,
            })
        };
        assert!(size_of("2").unwrap() < 10.0);
        assert!(size_of("3").unwrap() < size_of("2").unwrap());
    }

    #[test]
    fn w104_limits_go_under_in_display_and_beside_in_the_line() {
        let sum = |display: &str| {
            draw(
                &format!(
                    "<math {NS} display=\"{display}\"><munderover><mo>∑</mo>\
                     <mrow><mi>i</mi><mo>=</mo><mn>1</mn></mrow><mi>n</mi></munderover></math>"
                ),
                10.0,
                &measure,
            )
        };
        let block = sum("block");
        let inline = sum("inline");
        assert!(block.height > inline.height);
        assert!(block.width < inline.width);
    }

    #[test]
    fn w104_fences_grow_with_what_they_hold() {
        let drawing = draw(
            &format!(
                "<math {NS}><mrow><mo>(</mo><mfrac><mi>a</mi><mi>b</mi></mfrac><mo>)</mo></mrow>\
                 <mrow><mo>(</mo><mi>c</mi><mo>)</mo></mrow></math>"
            ),
            10.0,
            &measure,
        );
        // The parentheses of the fraction are lines; those of c, in a row
        // of their own, are text.
        assert_eq!(
            drawing
                .marks
                .iter()
                .filter(|m| matches!(m, Mark::Stroke { .. }))
                .count(),
            2
        );
        assert_eq!(texts(&drawing), ["a", "b", "(", "c", ")"]);
    }

    #[test]
    fn w104_a_table_lays_out_its_cells_in_columns() {
        let drawing = draw(
            &format!(
                "<math {NS}><mtable><mtr><mtd><mn>1</mn></mtd><mtd><mn>22</mn></mtd></mtr>\
                 <mtr><mtd><mn>333</mn></mtd><mtd><mn>4</mn></mtd></mtr></mtable></math>"
            ),
            10.0,
            &measure,
        );
        let at = |t: &str| {
            drawing.marks.iter().find_map(|m| match m {
                Mark::Text { text, x, y, .. } if text == t => Some((*x, *y)),
                _ => None,
            })
        };
        let (one, four) = (at("1").unwrap(), at("4").unwrap());
        assert!(one.1 < four.1);
        assert!(one.0 < four.0);
        assert!((at("22").unwrap().1 - one.1).abs() < 0.01);
    }

    #[test]
    fn w104_what_is_not_mathml_draws_nothing() {
        assert_eq!(draw("<svg/>", 10.0, &measure), Drawing::default());
        let drawing = draw(
            &format!("<math {NS}><mi mathvariant=\"double-struck\">R</mi></math>"),
            10.0,
            &measure,
        );
        assert_eq!(texts(&drawing), ["ℝ"]);
    }

    #[test]
    fn w104_the_deepest_mathml_read_lays_out() {
        let open = "<mfrac><msqrt><mrow>".repeat(84);
        let close = "</mrow></msqrt><mn>1</mn></mfrac>".repeat(84);
        let drawing = draw(
            &format!("<math {NS}>{open}<mi>x</mi>{close}</math>"),
            10.0,
            &measure,
        );
        assert!(drawing.width > 0.0);
    }

    #[test]
    fn w104_from_the_review() {
        let x_of = |drawing: &Drawing, t: &str| {
            drawing.marks.iter().find_map(|m| match m {
                Mark::Text { text, x, .. } if text == t => Some(*x),
                _ => None,
            })
        };
        // A function keeps a thin space from its argument, and none from
        // its parenthesis.
        let sin = draw(
            &format!("<math {NS}><mi>sin</mi><mi>x</mi></math>"),
            10.0,
            &measure,
        );
        assert!(x_of(&sin, "x").unwrap() - x_of(&sin, "sin").unwrap() > 15.0 + 1.0);
        let paren = draw(
            &format!("<math {NS}><mi>sin</mi><mo>(</mo></math>"),
            10.0,
            &measure,
        );
        assert!((x_of(&paren, "(").unwrap() - x_of(&paren, "sin").unwrap() - 15.0).abs() < 0.01);
        // A sum keeps its space before a parenthesis.
        let sum = draw(
            &format!("<math {NS}><mo>∑</mo><mo>(</mo></math>"),
            10.0,
            &measure,
        );
        assert!(x_of(&sum, "(").unwrap() - x_of(&sum, "∑").unwrap() > 5.0 + 1.0);
        // A rule of no number, and a bare number times the rule.
        let bar = |thickness: &str| {
            let drawing = draw(
                &format!(
                    "<math {NS}><mfrac linethickness=\"{thickness}\"><mn>1</mn><mn>2</mn></mfrac></math>"
                ),
                12.0,
                &measure,
            );
            assert!(drawing.height.is_finite() && drawing.width.is_finite());
            drawing.marks.iter().find_map(|m| match m {
                Mark::Rule { height, .. } => Some(*height),
                _ => None,
            })
        };
        assert_eq!(bar("NaN"), Some(0.6));
        assert_eq!(bar("2"), Some(1.2));
        assert_eq!(bar("3pt"), Some(3.0));
        // A delimiter of a size is drawn at that size beside little text.
        let big = draw(
            &format!("<math {NS}><mo minsize=\"2.4em\" stretchy=\"true\">(</mo><mi>x</mi></math>"),
            10.0,
            &measure,
        );
        assert!(matches!(big.marks[0], Mark::Stroke { .. }));
        assert!(big.height > 24.0);
        // Columns without space between them.
        let table = draw(
            &format!(
                "<math {NS}><mtable columnspacing=\"0\"><mtr><mtd><mi>a</mi></mtd>\
                 <mtd><mi>b</mi></mtd></mtr></mtable></math>"
            ),
            10.0,
            &measure,
        );
        assert!((x_of(&table, "b").unwrap() - x_of(&table, "a").unwrap() - 5.4).abs() < 0.01);
    }

    #[test]
    fn w104_lengths_of_mathml() {
        assert_eq!(length("2em", 10.0), Some(20.0));
        assert_eq!(length("thickmathspace", 18.0), Some(5.0));
        assert_eq!(length("12pt", 10.0), Some(12.0));
        assert_eq!(length("1e999em", 10.0), None);
        assert_eq!(length("wide", 10.0), None);
    }
}
