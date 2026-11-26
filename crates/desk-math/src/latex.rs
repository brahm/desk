// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! LaTeX turned into MathML: the mathematics of LaTeX that Edit Equation
//! of Pages 15.4 takes, from letters, numbers and operators to fractions,
//! roots, scripts, fences, accents, matrices and cases.

use std::fmt;

use crate::xml::Node;

/// How deep groups nest at most.
const MAX_DEPTH: usize = 64;

/// Why LaTeX could not be turned into MathML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LatexError {
    /// A command that is not known, without its backslash.
    UnknownCommand(String),
    /// An environment that is not known.
    UnknownEnvironment(String),
    /// A brace, `\left` or `\begin` without its other half.
    Unbalanced,
    /// A command, or a script, without what it applies to.
    MissingArgument(String),
    /// Groups nested too deep.
    TooDeep,
    /// A second superscript or subscript on the same base: `^`, `_` or
    /// `'`.
    DoubleScript(String),
}

impl fmt::Display for LatexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand(name) => write!(f, "unknown command \\{name}"),
            Self::UnknownEnvironment(name) => write!(f, "unknown environment {name}"),
            Self::Unbalanced => write!(f, "a brace or a \\left is not closed"),
            Self::MissingArgument(name) => write!(f, "{name} is missing what it applies to"),
            Self::TooDeep => write!(f, "groups are nested too deep"),
            Self::DoubleScript(name) => write!(f, "{name} is written twice on the same base"),
        }
    }
}

impl std::error::Error for LatexError {}

/// Turns the mathematics of LaTeX into MathML: a `math` element, in
/// display, as an equation on its own line is, or in the line of text.
pub fn latex_to_mathml(latex: &str, display: bool) -> Result<String, LatexError> {
    let mut parser = Parser {
        chars: latex.chars().collect(),
        at: 0,
        depth: 0,
    };
    let children = parser.expression(Stop::End)?;
    let mut math = Node::with("math", children);
    if display {
        math = math.attr("display", "block");
    }
    Ok(math.to_xml())
}

/// What ends an expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    End,
    Brace,
    Right,
    /// The end of an environment, or `&` and `\\` inside it.
    Environment,
    /// The `]` of the index of a root.
    Bracket,
}

/// What stopped an expression.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ended {
    Done,
    Cell,
    Row,
    End(String),
    Right(String),
}

struct Parser {
    chars: Vec<char>,
    at: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    fn expression(&mut self, stop: Stop) -> Result<Vec<Node>, LatexError> {
        let (nodes, ended) = self.until(stop)?;
        match ended {
            Ended::Done => Ok(nodes),
            _ => Err(LatexError::Unbalanced),
        }
    }

    /// Nodes up to what stops them.
    fn until(&mut self, stop: Stop) -> Result<(Vec<Node>, Ended), LatexError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(LatexError::TooDeep);
        }
        let result = self.until_inner(stop);
        self.depth -= 1;
        result
    }

    fn until_inner(&mut self, stop: Stop) -> Result<(Vec<Node>, Ended), LatexError> {
        let mut nodes: Vec<Node> = Vec::new();
        loop {
            self.skip_spaces();
            let Some(c) = self.peek() else {
                return if stop == Stop::End {
                    Ok((nodes, Ended::Done))
                } else {
                    Err(LatexError::Unbalanced)
                };
            };
            match c {
                '}' => {
                    self.at += 1;
                    return if stop == Stop::Brace {
                        Ok((nodes, Ended::Done))
                    } else {
                        Err(LatexError::Unbalanced)
                    };
                }
                ']' if stop == Stop::Bracket => {
                    self.at += 1;
                    return Ok((nodes, Ended::Done));
                }
                '&' if stop == Stop::Environment => {
                    self.at += 1;
                    return Ok((nodes, Ended::Cell));
                }
                '^' | '_' => {
                    self.at += 1;
                    let script = self.argument(if c == '^' { "^" } else { "_" })?;
                    let base = nodes.pop().unwrap_or_else(|| Node::new("mrow"));
                    nodes.push(attach(base, script, c == '^')?);
                }
                '\'' => {
                    self.at += 1;
                    let base = nodes.pop().unwrap_or_else(|| Node::new("mrow"));
                    nodes.push(prime(base)?);
                }
                '\\' => {
                    let name = self.command();
                    match name.as_str() {
                        "\\" if stop == Stop::Environment => {
                            // The space after a row, as \\[4pt], is not kept.
                            self.skip_spaces();
                            if self.peek() == Some('[') {
                                let close = self.chars[self.at..]
                                    .iter()
                                    .position(|c| *c == ']')
                                    .ok_or(LatexError::Unbalanced)?;
                                self.at += close + 1;
                            }
                            return Ok((nodes, Ended::Row));
                        }
                        "right" => {
                            let fence = self.delimiter("right")?;
                            return if stop == Stop::Right {
                                Ok((nodes, Ended::Right(fence)))
                            } else {
                                Err(LatexError::Unbalanced)
                            };
                        }
                        "end" => {
                            let name = self.braced_text("end")?;
                            return if stop == Stop::Environment {
                                Ok((nodes, Ended::End(name)))
                            } else {
                                Err(LatexError::Unbalanced)
                            };
                        }
                        _ => {
                            if let Some(node) = self.command_node(&name, &mut nodes)? {
                                nodes.push(node);
                            }
                        }
                    }
                }
                _ => {
                    if let Some(node) = self.atom()? {
                        nodes.push(node);
                    }
                }
            }
        }
    }

    /// One thing that is not a command or a script.
    fn atom(&mut self) -> Result<Option<Node>, LatexError> {
        let Some(c) = self.peek() else {
            return Ok(None);
        };
        self.at += 1;
        Ok(Some(match c {
            '{' => {
                let nodes = self.expression(Stop::Brace)?;
                // A group keeps its scripts to itself: {x^a}^b is not a
                // double superscript.
                if nodes.len() == 1 && SCRIPTED.contains(&nodes[0].name.as_str()) {
                    Node::with("mrow", nodes)
                } else {
                    row(nodes)
                }
            }
            '0'..='9' | '.' => {
                let mut number = c.to_string();
                while let Some(d) = self.peek() {
                    let decimal = d == '.'
                        && self
                            .chars
                            .get(self.at + 1)
                            .is_some_and(char::is_ascii_digit)
                        && !number.contains('.');
                    if d.is_ascii_digit() || decimal {
                        number.push(d);
                        self.at += 1;
                    } else {
                        break;
                    }
                }
                if number == "." {
                    Node::token("mo", ".")
                } else {
                    Node::token("mn", &number)
                }
            }
            '~' => Node::token("mtext", "\u{a0}"),
            '&' => return Err(LatexError::Unbalanced),
            ']' => Node::token("mo", "]").attr("stretchy", "false"),
            '(' | ')' | '[' | '|' => Node::token("mo", &c.to_string()).attr("stretchy", "false"),
            '-' => Node::token("mo", "−"),
            '*' => Node::token("mo", "∗"),
            '+' | '=' | '<' | '>' | ',' | ';' | ':' | '!' | '?' | '/' => {
                Node::token("mo", &c.to_string())
            }
            '#' | '$' | '%' => return Ok(None),
            c if c.is_alphabetic() => Node::token("mi", &c.to_string()),
            c => Node::token("mo", &c.to_string()),
        }))
    }

    /// The name of the command at a backslash, without it.
    fn command(&mut self) -> String {
        self.at += 1;
        let mut name = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
            name.push(c);
            self.at += 1;
        }
        if name.is_empty()
            && let Some(c) = self.peek()
        {
            name.push(c);
            self.at += 1;
        }
        name
    }

    /// The argument of a command or a script: a group, or one thing.
    fn argument(&mut self, of: &str) -> Result<Node, LatexError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(LatexError::TooDeep);
        }
        let result = self.argument_inner(of);
        self.depth -= 1;
        result
    }

    fn argument_inner(&mut self, of: &str) -> Result<Node, LatexError> {
        self.skip_spaces();
        let missing = || LatexError::MissingArgument(of.into());
        match self.peek() {
            None | Some('}' | '&' | '^' | '_') => Err(missing()),
            Some('{') => {
                self.at += 1;
                Ok(row(self.expression(Stop::Brace)?))
            }
            Some('\\') => {
                let name = self.command();
                if matches!(name.as_str(), "right" | "end" | "\\") {
                    return Err(missing());
                }
                let mut none = Vec::new();
                Ok(self
                    .command_node(&name, &mut none)?
                    .unwrap_or_else(|| Node::new("mrow")))
            }
            Some(_) => {
                // A number in an argument is one digit, as in x^23.
                let c = self.peek().unwrap_or(' ');
                if c.is_ascii_digit() {
                    self.at += 1;
                    return Ok(Node::token("mn", &c.to_string()));
                }
                Ok(self.atom()?.unwrap_or_else(|| Node::new("mrow")))
            }
        }
    }

    /// The text in braces after a command, as written.
    fn braced_text(&mut self, of: &str) -> Result<String, LatexError> {
        self.skip_spaces();
        if self.peek() != Some('{') {
            return Err(LatexError::MissingArgument(format!("\\{of}")));
        }
        self.at += 1;
        let mut depth = 0usize;
        let mut text = String::new();
        loop {
            let c = self.peek().ok_or(LatexError::Unbalanced)?;
            self.at += 1;
            match c {
                '{' => depth += 1,
                '}' if depth == 0 => return Ok(text),
                '}' => depth -= 1,
                '\\' => {
                    // An escaped character stands for itself.
                    if let Some(next) = self.peek().filter(|n| "{}\\$&#%_ ".contains(*n)) {
                        self.at += 1;
                        text.push(next);
                        continue;
                    }
                }
                _ => {}
            }
            text.push(c);
        }
    }

    /// The delimiter after `\left`, `\right` or a size: empty for `.`.
    fn delimiter(&mut self, of: &str) -> Result<String, LatexError> {
        self.skip_spaces();
        let missing = || LatexError::MissingArgument(format!("\\{of}"));
        let c = self.peek().ok_or_else(missing)?;
        if c == '\\' {
            let name = self.command();
            return Ok(match name.as_str() {
                "{" | "lbrace" => "{",
                "}" | "rbrace" => "}",
                "|" | "Vert" => "‖",
                "vert" => "|",
                "langle" => "⟨",
                "rangle" => "⟩",
                "lfloor" => "⌊",
                "rfloor" => "⌋",
                "lceil" => "⌈",
                "rceil" => "⌉",
                "lvert" | "rvert" => "|",
                "lVert" | "rVert" => "‖",
                _ => return Err(LatexError::UnknownCommand(name)),
            }
            .into());
        }
        self.at += 1;
        match c {
            '.' => Ok(String::new()),
            '(' | ')' | '[' | ']' | '|' | '/' | '<' | '>' => Ok(match c {
                '<' => "⟨".into(),
                '>' => "⟩".into(),
                c => c.to_string(),
            }),
            _ => Err(missing()),
        }
    }

    /// What a command makes; nothing for a command that only changes
    /// what follows, or spaces of nothing.
    fn command_node(
        &mut self,
        name: &str,
        before: &mut Vec<Node>,
    ) -> Result<Option<Node>, LatexError> {
        if let Some(letter) = greek(name) {
            return Ok(Some(Node::token("mi", letter)));
        }
        if let Some(symbol) = symbol(name) {
            let node = Node::token("mo", symbol);
            return Ok(Some(if LIMITS.contains(&name) {
                node.attr("movablelimits", "true")
            } else {
                node
            }));
        }
        if let Some(letters) = letterlike(name) {
            return Ok(Some(Node::token("mi", letters)));
        }
        if FUNCTIONS.contains(&name) {
            return Ok(Some(Node::token("mi", name)));
        }
        if FUNCTION_LIMITS.contains(&name) {
            let text = match name {
                "liminf" => "lim inf",
                "limsup" => "lim sup",
                other => other,
            };
            return Ok(Some(Node::token("mo", text).attr("movablelimits", "true")));
        }
        if let Some(width) = space(name) {
            return Ok(Some(Node::new("mspace").attr("width", width)));
        }
        if let Some((mark, under)) = accent(name) {
            let base = self.argument(&format!("\\{name}"))?;
            let mark = Node::token("mo", mark);
            return Ok(Some(if under {
                Node::with("munder", vec![base, mark]).attr("accentunder", "true")
            } else {
                Node::with("mover", vec![base, mark]).attr("accent", "true")
            }));
        }
        let missing = || LatexError::MissingArgument(format!("\\{name}"));
        Ok(Some(match name {
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let numerator = self.argument(&format!("\\{name}"))?;
                let denominator = self.argument(&format!("\\{name}"))?;
                let fraction = Node::with("mfrac", vec![numerator, denominator]);
                match name {
                    "dfrac" => Node::with("mstyle", vec![fraction]).attr("displaystyle", "true"),
                    "tfrac" => Node::with("mstyle", vec![fraction]).attr("displaystyle", "false"),
                    _ => fraction,
                }
            }
            "binom" | "dbinom" | "tbinom" => {
                let top = self.argument(&format!("\\{name}"))?;
                let bottom = self.argument(&format!("\\{name}"))?;
                Node::with(
                    "mrow",
                    vec![
                        Node::token("mo", "("),
                        Node::with("mfrac", vec![top, bottom]).attr("linethickness", "0"),
                        Node::token("mo", ")"),
                    ],
                )
            }
            "sqrt" => {
                self.skip_spaces();
                let index = if self.peek() == Some('[') {
                    self.at += 1;
                    Some(row(self.expression(Stop::Bracket)?))
                } else {
                    None
                };
                let radicand = self.argument("\\sqrt")?;
                match index {
                    Some(index) => Node::with("mroot", vec![radicand, index]),
                    None => Node::with("msqrt", vec![radicand]),
                }
            }
            "left" => {
                let open = self.delimiter("left")?;
                let (inner, ended) = self.until(Stop::Right)?;
                let Ended::Right(close) = ended else {
                    return Err(LatexError::Unbalanced);
                };
                let mut children = Vec::new();
                if !open.is_empty() {
                    children.push(fence(&open));
                }
                children.extend(inner);
                if !close.is_empty() {
                    children.push(fence(&close));
                }
                Node::with("mrow", children)
            }
            "middle" => fence(&self.delimiter("middle")?),
            "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl" | "Bigr" | "biggl"
            | "biggr" | "Biggl" | "Biggr" | "bigm" | "Bigm" => {
                let delimiter = self.delimiter(name)?;
                let size = match name.trim_end_matches(['l', 'r', 'm']) {
                    "big" => "1.2em",
                    "Big" => "1.8em",
                    "bigg" => "2.4em",
                    _ => "3em",
                };
                Node::token("mo", &delimiter)
                    .attr("stretchy", "true")
                    .attr("minsize", size)
                    .attr("maxsize", size)
            }
            "text" | "textrm" | "textnormal" | "mbox" | "textit" | "textbf" | "texttt"
            | "textsf" => {
                let text = self.braced_text(name)?;
                let node = Node::token("mtext", &text);
                match name {
                    "textit" => node.attr("mathvariant", "italic"),
                    "textbf" => node.attr("mathvariant", "bold"),
                    _ => node,
                }
            }
            "mathrm" | "mathbf" | "mathit" | "mathbb" | "mathcal" | "mathfrak" | "mathscr"
            | "mathsf" | "mathtt" | "boldsymbol" | "bm" | "operatorname" => {
                let variant = match name {
                    "mathrm" | "operatorname" | "mathsf" | "mathtt" => "normal",
                    "mathbf" => "bold",
                    "mathit" => "italic",
                    "mathbb" => "double-struck",
                    "mathcal" | "mathscr" => "script",
                    "mathfrak" => "fraktur",
                    _ => "bold-italic",
                };
                let inner = self.argument(&format!("\\{name}"))?;
                let node = with_variant(inner, variant);
                if name == "operatorname" {
                    let text = plain_text(&node);
                    Node::token("mi", &text)
                } else {
                    node
                }
            }
            "overline" | "underline" => {
                let base = self.argument(&format!("\\{name}"))?;
                if name == "overline" {
                    Node::with("mover", vec![base, Node::token("mo", "‾")])
                } else {
                    Node::with("munder", vec![base, Node::token("mo", "‾")])
                }
            }
            "overbrace" | "underbrace" => {
                let base = self.argument(&format!("\\{name}"))?;
                if name == "overbrace" {
                    Node::with("mover", vec![base, Node::token("mo", "⏞")])
                } else {
                    Node::with("munder", vec![base, Node::token("mo", "⏟")])
                }
            }
            "overset" | "underset" | "stackrel" => {
                let mark = self.argument(&format!("\\{name}"))?;
                let base = self.argument(&format!("\\{name}"))?;
                let element = if name == "underset" {
                    "munder"
                } else {
                    "mover"
                };
                Node::with(element, vec![base, mark])
            }
            "pmod" => {
                let inner = self.argument("\\pmod")?;
                Node::with(
                    "mrow",
                    vec![
                        Node::new("mspace").attr("width", "1em"),
                        Node::token("mo", "("),
                        Node::token("mi", "mod"),
                        Node::new("mspace").attr("width", "0.333em"),
                        inner,
                        Node::token("mo", ")"),
                    ],
                )
            }
            "not" => {
                let next = self.argument("\\not")?;
                let negated = match plain_text(&next).as_str() {
                    "=" => "≠",
                    "∈" => "∉",
                    "⊂" => "⊄",
                    "<" => "≮",
                    ">" => "≯",
                    "≤" => "≰",
                    "≥" => "≱",
                    "≡" => "≢",
                    "∼" => "≁",
                    "≈" => "≉",
                    _ => return Ok(Some(next)),
                };
                Node::token("mo", negated)
            }
            "begin" => {
                let environment = self.braced_text("begin")?;
                self.environment(&environment)?
            }
            "limits" | "nolimits" | "displaystyle" | "textstyle" | "scriptstyle" | "nonumber"
            | "notag" | "left." | "right." => return Ok(None),
            "{" | "}" | "|" | "#" | "$" | "%" | "&" | "_" => {
                let text = if name == "|" { "‖" } else { name };
                let node = Node::token("mo", text);
                if matches!(name, "{" | "}" | "|") {
                    node.attr("stretchy", "false")
                } else {
                    node
                }
            }
            "\\" => return Ok(None),
            "" => return Err(missing()),
            _ => {
                let _ = before;
                return Err(LatexError::UnknownCommand(name.into()));
            }
        }))
    }

    fn environment(&mut self, name: &str) -> Result<Node, LatexError> {
        let fences = match name {
            "matrix" | "smallmatrix" | "aligned" | "align" | "align*" | "gathered" | "gather"
            | "gather*" | "array" | "split" | "equation" | "equation*" => ("", ""),
            "pmatrix" => ("(", ")"),
            "bmatrix" => ("[", "]"),
            "Bmatrix" => ("{", "}"),
            "vmatrix" => ("|", "|"),
            "Vmatrix" => ("‖", "‖"),
            "cases" => ("{", ""),
            "rcases" => ("", "}"),
            _ => return Err(LatexError::UnknownEnvironment(name.into())),
        };
        if name == "array" {
            // The columns of an array, as {lcr}, are not kept.
            let _ = self.braced_text("begin{array}")?;
        }
        let mut rows: Vec<Node> = Vec::new();
        let mut cells: Vec<Node> = Vec::new();
        loop {
            let (inner, ended) = self.until(Stop::Environment)?;
            cells.push(Node::with("mtd", inner));
            match ended {
                Ended::Cell => {}
                Ended::Row => rows.push(Node::with("mtr", std::mem::take(&mut cells))),
                Ended::End(end) if end == name => {
                    // A last row of one empty cell is the end of the one
                    // before, written with \\.
                    let empty = cells.len() == 1 && cells[0].children.is_empty();
                    if !empty || rows.is_empty() {
                        rows.push(Node::with("mtr", std::mem::take(&mut cells)));
                    }
                    break;
                }
                _ => return Err(LatexError::Unbalanced),
            }
        }
        let mut table = Node::with("mtable", rows);
        match name {
            "cases" | "rcases" | "array" => table = table.attr("columnalign", "left"),
            "aligned" | "align" | "align*" | "split" => {
                // Pairs of columns aligned on their relation, apart from
                // each other.
                table = table
                    .attr("columnalign", "right left right left right left right left")
                    .attr("columnspacing", "0 2em 0 2em 0 2em 0");
            }
            _ => {}
        }
        if name == "equation" || name == "equation*" {
            return Ok(Node::with(
                "mrow",
                table
                    .children
                    .into_iter()
                    .flat_map(|r| r.children)
                    .flat_map(|d| d.children)
                    .collect(),
            ));
        }
        if fences == ("", "") {
            return Ok(table);
        }
        let mut children = Vec::new();
        if !fences.0.is_empty() {
            children.push(fence(fences.0));
        }
        children.push(table);
        if !fences.1.is_empty() {
            children.push(fence(fences.1));
        }
        Ok(Node::with("mrow", children))
    }
}

fn fence(text: &str) -> Node {
    Node::token("mo", text)
        .attr("fence", "true")
        .attr("stretchy", "true")
}

/// One node for several: itself when there is one.
fn row(mut nodes: Vec<Node>) -> Node {
    if nodes.len() == 1 {
        nodes.pop().unwrap_or_else(|| Node::new("mrow"))
    } else {
        Node::with("mrow", nodes)
    }
}

/// The elements that hold scripts.
const SCRIPTED: &[&str] = &["msub", "msup", "msubsup", "munder", "mover", "munderover"];

fn is_primes(node: &Node) -> bool {
    node.name == "mo" && !node.text.is_empty() && node.text.chars().all(|c| c == '′')
}

/// A script attached to a base, as a superscript or a subscript, joined
/// with the other script when the base already has one. A base cannot
/// take two of the same, as in TeX.
fn attach(base: Node, script: Node, superscript: bool) -> Result<Node, LatexError> {
    let double = || LatexError::DoubleScript(if superscript { "^" } else { "_" }.into());
    // The operator under a limit, as the sum of \sum_{i}.
    let limits = base.children.first().is_some_and(base_has_limits);
    let name = base.name.clone();
    let mut children = base.children.clone();
    Ok(match (name.as_str(), superscript) {
        // x'^2 is x with the primes and 2 as one superscript.
        ("msup" | "msubsup", true) if children.last().is_some_and(is_primes) => {
            let primes = children.pop().unwrap_or_default();
            children.push(Node::with("mrow", vec![primes, script]));
            Node::with(&name, children)
        }
        ("msup" | "msubsup", true) | ("msub" | "msubsup", false) => return Err(double()),
        ("msub", true) => {
            children.push(script);
            Node::with("msubsup", children)
        }
        ("msup", false) => {
            children.insert(1, script);
            Node::with("msubsup", children)
        }
        ("munder", true) if limits => {
            children.push(script);
            Node::with("munderover", children)
        }
        ("mover", false) if limits => {
            children.insert(1, script);
            Node::with("munderover", children)
        }
        ("mover" | "munderover", true) | ("munder" | "munderover", false) if limits => {
            return Err(double());
        }
        _ if base_has_limits(&base) => Node::with(
            if superscript { "mover" } else { "munder" },
            vec![base, script],
        ),
        _ => Node::with(
            if superscript { "msup" } else { "msub" },
            vec![base, script],
        ),
    })
}

/// A prime on a base, as f', which more primes join.
fn prime(base: Node) -> Result<Node, LatexError> {
    let mut children = base.children.clone();
    match base.name.as_str() {
        "msup" | "msubsup" => match children.last_mut() {
            Some(last) if is_primes(last) => {
                last.text.push('′');
                Ok(Node::with(&base.name, children))
            }
            _ => Err(LatexError::DoubleScript("'".into())),
        },
        "msub" => {
            children.push(Node::token("mo", "′"));
            Ok(Node::with("msubsup", children))
        }
        _ => Ok(Node::with("msup", vec![base, Node::token("mo", "′")])),
    }
}

/// Whether a base takes its scripts above and below in display: the
/// large operators and the functions as lim.
fn base_has_limits(base: &Node) -> bool {
    base.name == "mo" && base.get("movablelimits") == Some("true")
}

/// The node with a variant of letters, which `mi` and `mtext` show.
fn with_variant(node: Node, variant: &str) -> Node {
    match node.name.as_str() {
        "mi" | "mn" | "mtext" | "mo" => node.attr("mathvariant", variant),
        _ => Node {
            children: node
                .children
                .into_iter()
                .map(|c| with_variant(c, variant))
                .collect(),
            ..node
        },
    }
}

fn plain_text(node: &Node) -> String {
    let mut text = node.text.clone();
    for child in &node.children {
        text.push_str(&plain_text(child));
    }
    text
}

const LIMITS: &[&str] = &[
    "sum",
    "prod",
    "coprod",
    "bigcup",
    "bigcap",
    "bigoplus",
    "bigotimes",
    "bigodot",
    "bigvee",
    "bigwedge",
    "bigsqcup",
];

const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "coth", "log", "ln", "lg", "exp", "det", "dim", "ker", "gcd", "deg", "arg", "hom", "Pr",
];

const FUNCTION_LIMITS: &[&str] = &["lim", "liminf", "limsup", "max", "min", "sup", "inf"];

fn greek(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ϵ",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "omicron" => "ο",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "varrho" => "ϱ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "ϕ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        _ => return None,
    })
}

/// Letters and constants that are not operators.
fn letterlike(name: &str) -> Option<&'static str> {
    Some(match name {
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "hbar" => "ℏ",
        "ell" => "ℓ",
        "Re" => "ℜ",
        "Im" => "ℑ",
        "aleph" => "ℵ",
        "emptyset" | "varnothing" => "∅",
        "imath" => "ı",
        "jmath" => "ȷ",
        "wp" => "℘",
        "angle" => "∠",
        "triangle" => "△",
        "top" => "⊤",
        "bot" => "⊥",
        "prime" => "′",
        "dagger" => "†",
        _ => return None,
    })
}

/// Operators, relations, arrows and dots.
fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "sum" => "∑",
        "prod" => "∏",
        "coprod" => "∐",
        "int" => "∫",
        "iint" => "∬",
        "iiint" => "∭",
        "oint" => "∮",
        "bigcup" => "⋃",
        "bigcap" => "⋂",
        "bigoplus" => "⨁",
        "bigotimes" => "⨂",
        "bigodot" => "⨀",
        "bigvee" => "⋁",
        "bigwedge" => "⋀",
        "bigsqcup" => "⨆",
        "pm" => "±",
        "mp" => "∓",
        "times" => "×",
        "div" => "÷",
        "cdot" => "⋅",
        "ast" => "∗",
        "star" => "⋆",
        "circ" => "∘",
        "bullet" => "∙",
        "cup" => "∪",
        "cap" => "∩",
        "setminus" => "∖",
        "wedge" | "land" => "∧",
        "vee" | "lor" => "∨",
        "oplus" => "⊕",
        "ominus" => "⊖",
        "otimes" => "⊗",
        "odot" => "⊙",
        "neg" | "lnot" => "¬",
        "leq" | "le" => "≤",
        "geq" | "ge" => "≥",
        "neq" | "ne" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "simeq" => "≃",
        "cong" => "≅",
        "propto" => "∝",
        "ll" => "≪",
        "gg" => "≫",
        "prec" => "≺",
        "succ" => "≻",
        "preceq" => "⪯",
        "succeq" => "⪰",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "supset" => "⊃",
        "subseteq" => "⊆",
        "supseteq" => "⊇",
        "mid" => "∣",
        "parallel" => "∥",
        "perp" => "⊥",
        "vdash" => "⊢",
        "models" => "⊨",
        "coloneqq" => "≔",
        "to" | "rightarrow" => "→",
        "gets" | "leftarrow" => "←",
        "leftrightarrow" => "↔",
        "Rightarrow" | "implies" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" | "iff" => "⇔",
        "mapsto" => "↦",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "longrightarrow" => "⟶",
        "longleftarrow" => "⟵",
        "Longrightarrow" => "⟹",
        "forall" => "∀",
        "exists" => "∃",
        "nexists" => "∄",
        "ldots" | "dots" | "dotsc" => "…",
        "cdots" | "dotsb" => "⋯",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "therefore" => "∴",
        "because" => "∵",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "lbrace" => "{",
        "rbrace" => "}",
        "vert" | "lvert" | "rvert" => "|",
        "Vert" | "lVert" | "rVert" => "‖",
        "backslash" => "∖",
        "colon" => ":",
        "degree" => "°",
        _ => return None,
    })
}

fn space(name: &str) -> Option<&'static str> {
    Some(match name {
        "," | "thinspace" => "0.1667em",
        ":" | ">" | "medspace" => "0.2222em",
        ";" | "thickspace" => "0.2778em",
        "!" | "negthinspace" => "-0.1667em",
        " " => "0.25em",
        "quad" => "1em",
        "qquad" => "2em",
        _ => return None,
    })
}

/// The mark of an accent, and whether it goes under.
fn accent(name: &str) -> Option<(&'static str, bool)> {
    Some(match name {
        "hat" | "widehat" => ("^", false),
        "check" => ("ˇ", false),
        "tilde" | "widetilde" => ("~", false),
        "acute" => ("´", false),
        "grave" => ("`", false),
        "dot" => ("˙", false),
        "ddot" => ("¨", false),
        "breve" => ("˘", false),
        "bar" => ("¯", false),
        "vec" | "overrightarrow" => ("→", false),
        "underbar" => ("‾", true),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    fn mathml(latex: &str) -> String {
        let xml = latex_to_mathml(latex, false).unwrap();
        // What it writes reads back as MathML.
        assert!(parse(&xml).is_some(), "{xml}");
        xml.trim_start_matches("<math xmlns=\"http://www.w3.org/1998/Math/MathML\">")
            .trim_end_matches("</math>")
            .to_owned()
    }

    #[test]
    fn w104_letters_numbers_and_operators() {
        assert_eq!(
            mathml("x + 2.5 = -y"),
            "<mi>x</mi><mo>+</mo><mn>2.5</mn><mo>=</mo><mo>−</mo><mi>y</mi>"
        );
    }

    #[test]
    fn w104_scripts_join_on_their_base() {
        assert_eq!(
            mathml("x_i^2"),
            "<msubsup><mi>x</mi><mi>i</mi><mn>2</mn></msubsup>"
        );
        assert_eq!(
            mathml("e^{i\\pi}"),
            "<msup><mi>e</mi><mrow><mi>i</mi><mi>π</mi></mrow></msup>"
        );
        assert_eq!(
            mathml("x^23"),
            "<msup><mi>x</mi><mn>2</mn></msup><mn>3</mn>"
        );
        assert_eq!(mathml("f'"), "<msup><mi>f</mi><mo>′</mo></msup>");
        assert_eq!(
            mathml("f''^2"),
            "<msup><mi>f</mi><mrow><mo>′′</mo><mn>2</mn></mrow></msup>"
        );
        assert_eq!(
            mathml("x_1'"),
            "<msubsup><mi>x</mi><mn>1</mn><mo>′</mo></msubsup>"
        );
        assert_eq!(
            mathml("{x^a}^b"),
            "<msup><mrow><msup><mi>x</mi><mi>a</mi></msup></mrow><mi>b</mi></msup>"
        );
        // A line over a letter takes scripts beside it.
        assert_eq!(
            mathml("\\overline{x}_i"),
            "<msub><mover><mi>x</mi><mo>‾</mo></mover><mi>i</mi></msub>"
        );
    }

    #[test]
    fn w104_fractions_roots_and_large_operators() {
        assert_eq!(
            mathml("\\frac{a}{b}"),
            "<mfrac><mi>a</mi><mi>b</mi></mfrac>"
        );
        assert_eq!(
            mathml("\\sqrt[3]{x}"),
            "<mroot><mi>x</mi><mn>3</mn></mroot>"
        );
        assert_eq!(
            mathml("\\sum_{i=1}^n i"),
            "<munderover><mo movablelimits=\"true\">∑</mo><mrow><mi>i</mi><mo>=</mo><mn>1</mn></mrow>\
             <mi>n</mi></munderover><mi>i</mi>"
        );
        assert_eq!(
            mathml("\\int_0^1"),
            "<msubsup><mo>∫</mo><mn>0</mn><mn>1</mn></msubsup>"
        );
        assert_eq!(
            mathml("\\lim_{x\\to 0}"),
            "<munder><mo movablelimits=\"true\">lim</mo><mrow><mi>x</mi><mo>→</mo><mn>0</mn></mrow></munder>"
        );
    }

    #[test]
    fn w104_fences_text_accents_and_variants() {
        assert_eq!(
            mathml("\\left( x \\right."),
            "<mrow><mo fence=\"true\" stretchy=\"true\">(</mo><mi>x</mi></mrow>"
        );
        assert_eq!(mathml("\\text{if } x"), "<mtext>if </mtext><mi>x</mi>");
        assert_eq!(
            mathml("\\hat{x}"),
            "<mover accent=\"true\"><mi>x</mi><mo>^</mo></mover>"
        );
        assert_eq!(
            mathml("\\mathbb{R}"),
            "<mi mathvariant=\"double-struck\">R</mi>"
        );
        assert_eq!(mathml("\\sin x"), "<mi>sin</mi><mi>x</mi>");
        assert_eq!(mathml("a \\not= b"), "<mi>a</mi><mo>≠</mo><mi>b</mi>");
        assert_eq!(mathml("a<b"), "<mi>a</mi><mo>&lt;</mo><mi>b</mi>");
    }

    #[test]
    fn w104_matrices_and_cases() {
        assert_eq!(
            mathml("\\begin{pmatrix} 1 & 0 \\\\ 0 & 1 \\end{pmatrix}"),
            "<mrow><mo fence=\"true\" stretchy=\"true\">(</mo><mtable>\
             <mtr><mtd><mn>1</mn></mtd><mtd><mn>0</mn></mtd></mtr>\
             <mtr><mtd><mn>0</mn></mtd><mtd><mn>1</mn></mtd></mtr></mtable>\
             <mo fence=\"true\" stretchy=\"true\">)</mo></mrow>"
        );
        let aligned = mathml("\\begin{aligned} a &= b \\\\[2pt] c &= d \\end{aligned}");
        assert!(!aligned.contains("<mi>p</mi>"), "{aligned}");
        assert_eq!(aligned.matches("<mtr>").count(), 2);
        let cases =
            mathml("f(x) = \\begin{cases} 1 & x > 0 \\\\ 0 & \\text{otherwise} \\\\ \\end{cases}");
        assert_eq!(cases.matches("<mtr>").count(), 2);
        assert!(cases.contains("<mtable columnalign=\"left\">"));
    }

    #[test]
    fn w104_what_is_wrong_is_said() {
        fn error(latex: &str) -> LatexError {
            latex_to_mathml(latex, false).unwrap_err()
        }
        assert_eq!(error("\\foo"), LatexError::UnknownCommand("foo".into()));
        assert_eq!(error("{x"), LatexError::Unbalanced);
        assert_eq!(error("x}"), LatexError::Unbalanced);
        assert_eq!(error("\\left( x"), LatexError::Unbalanced);
        assert_eq!(error("x^"), LatexError::MissingArgument("^".into()));
        assert_eq!(
            error("\\frac{a}"),
            LatexError::MissingArgument("\\frac".into())
        );
        assert_eq!(
            error("\\begin{foo}"),
            LatexError::UnknownEnvironment("foo".into())
        );
        assert_eq!(
            error("\\begin{matrix} 1 \\end{pmatrix}"),
            LatexError::Unbalanced
        );
        assert_eq!(error(&"{".repeat(500)), LatexError::TooDeep);
        assert_eq!(error(&"\\sqrt".repeat(500)), LatexError::TooDeep);
        assert_eq!(error("a & b"), LatexError::Unbalanced);
        assert_eq!(error("x^a^b"), LatexError::DoubleScript("^".into()));
        assert_eq!(error("x_a_b"), LatexError::DoubleScript("_".into()));
        assert_eq!(error("x^2'"), LatexError::DoubleScript("'".into()));
        assert_eq!(error("\\sum_a_b"), LatexError::DoubleScript("_".into()));
        // However many primes, the MathML is as deep.
        let primes = latex_to_mathml(&format!("x{}", "'".repeat(1000)), false).unwrap();
        assert!(parse(&primes).is_some());
    }

    #[test]
    fn w104_display_is_kept_on_the_root() {
        let xml = latex_to_mathml("x", true).unwrap();
        assert!(xml.contains("display=\"block\""));
    }
}
