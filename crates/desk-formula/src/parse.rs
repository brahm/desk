// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Reading and writing formulas.

use std::fmt;

use crate::value::{ErrorKind, formula_number};

/// A cell that a formula names, by its column and row from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellRef {
    /// The column, from 0 for A.
    pub col: u32,
    /// The row, from 0 for row 1.
    pub row: u32,
    /// Whether the column stays when the formula is copied: `$A1`.
    pub col_abs: bool,
    /// Whether the row stays when the formula is copied: `A$1`.
    pub row_abs: bool,
}

impl CellRef {
    /// A relative reference to a cell.
    pub const fn new(col: u32, row: u32) -> Self {
        Self {
            col,
            row,
            col_abs: false,
            row_abs: false,
        }
    }

    /// The reference from its name, such as `B12` or `$C$3`.
    pub fn parse(name: &str) -> Option<Self> {
        let bytes = name.as_bytes();
        let mut i = 0;
        let col_abs = bytes.first() == Some(&b'$');
        if col_abs {
            i += 1;
        }
        let letters_start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
        }
        let letters = &name[letters_start..i];
        if letters.is_empty() || letters.len() > 3 {
            return None;
        }
        let row_abs = bytes.get(i) == Some(&b'$');
        if row_abs {
            i += 1;
        }
        let digits = &name[i..];
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let row: u32 = digits.parse().ok()?;
        if row == 0 {
            return None;
        }
        let mut col: u32 = 0;
        for b in letters.bytes() {
            col = col * 26 + u32::from(b.to_ascii_uppercase() - b'A') + 1;
        }
        Some(Self {
            col: col - 1,
            row: row - 1,
            col_abs,
            row_abs,
        })
    }

    /// The letters of a column, from 0 for A.
    pub fn column_name(col: u32) -> String {
        let mut n = col + 1;
        let mut letters = Vec::new();
        while n > 0 {
            let rem = (n - 1) % 26;
            letters.push(b'A' + rem as u8);
            n = (n - 1) / 26;
        }
        letters.reverse();
        String::from_utf8(letters).unwrap_or_default()
    }

    /// The reference moved by columns and rows, as when the formula is
    /// copied; the parts with `$` stay. None when it would leave the
    /// table on the top or the left.
    pub fn moved(self, cols: i64, rows: i64) -> Option<Self> {
        let col = if self.col_abs {
            i64::from(self.col)
        } else {
            i64::from(self.col) + cols
        };
        let row = if self.row_abs {
            i64::from(self.row)
        } else {
            i64::from(self.row) + rows
        };
        Some(Self {
            col: u32::try_from(col).ok()?,
            row: u32::try_from(row).ok()?,
            ..self
        })
    }
}

impl fmt::Display for CellRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.col_abs {
            f.write_str("$")?;
        }
        f.write_str(&Self::column_name(self.col))?;
        if self.row_abs {
            f.write_str("$")?;
        }
        write!(f, "{}", self.row + 1)
    }
}

/// An operator with one operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    /// `-x`.
    Negate,
    /// `+x`.
    Plus,
    /// `x%`, a hundredth of x.
    Percent,
}

/// An operator with two operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    /// `+`.
    Add,
    /// `-`.
    Subtract,
    /// `*`.
    Multiply,
    /// `/`.
    Divide,
    /// `^`.
    Power,
    /// `&`, which joins text.
    Concat,
    /// `=`.
    Equal,
    /// `<>`.
    NotEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessOrEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterOrEqual,
}

impl BinaryOp {
    fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Power => "^",
            Self::Concat => "&",
            Self::Equal => "=",
            Self::NotEqual => "<>",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
        }
    }

    fn precedence(self) -> u8 {
        match self {
            Self::Equal
            | Self::NotEqual
            | Self::Less
            | Self::LessOrEqual
            | Self::Greater
            | Self::GreaterOrEqual => 1,
            Self::Concat => 2,
            Self::Add | Self::Subtract => 3,
            Self::Multiply | Self::Divide => 4,
            Self::Power => 5,
        }
    }
}

/// A part of a formula.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A number.
    Number(f64),
    /// Text, written between double quotes.
    Text(String),
    /// TRUE or FALSE.
    Bool(bool),
    /// An error written in the formula, or a reference that was deleted.
    Error(ErrorKind),
    /// A cell.
    Cell(CellRef),
    /// The cells of a rectangle, from one corner to the other.
    Range(CellRef, CellRef),
    /// The text of a header cell, which names the cells of its column or
    /// row (TAB-12, TAB-32).
    Header(String),
    /// An operator with one operand.
    Unary(UnaryOp, Box<Expr>),
    /// An operator with two operands.
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// A function, by its name in capitals, with its arguments.
    Call(String, Vec<Expr>),
}

/// Which way rows or columns are added or deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Rows.
    Rows,
    /// Columns.
    Columns,
}

/// Why a formula could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Where, in bytes from the start of the text.
    pub at: usize,
    /// What was wrong.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at {})", self.message, self.at)
    }
}

impl std::error::Error for ParseError {}

/// A formula of a cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    /// What it computes.
    pub expr: Expr,
}

impl Formula {
    /// Reads a formula, with or without the `=` it starts with.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let body = text.trim_start();
        let offset = text.len() - body.len();
        let body = body.strip_prefix('=').unwrap_or(body);
        let offset = offset + usize::from(text.trim_start().starts_with('='));
        let tokens = tokenize(body).map_err(|mut e| {
            e.at += offset;
            e
        })?;
        let mut parser = Parser {
            tokens: &tokens,
            pos: 0,
            depth: 0,
        };
        let expr = parser.expr(0).map_err(|mut e| {
            e.at += offset;
            e
        })?;
        if let Some((at, token)) = tokens.get(parser.pos) {
            return Err(ParseError {
                at: at + offset,
                message: format!("unexpected {token:?}"),
            });
        }
        Ok(Self { expr })
    }

    /// The ranges that the formula reads, single cells as ranges of one
    /// cell; headers are left out, as only the table knows them.
    pub fn references(&self) -> Vec<(CellRef, CellRef)> {
        let mut out = Vec::new();
        visit(&self.expr, &mut |e| match e {
            Expr::Cell(c) => out.push((*c, *c)),
            Expr::Range(a, b) => out.push((*a, *b)),
            _ => {}
        });
        out
    }

    /// The header names the formula uses.
    pub fn headers(&self) -> Vec<String> {
        let mut out = Vec::new();
        visit(&self.expr, &mut |e| {
            if let Expr::Header(h) = e {
                out.push(h.clone());
            }
        });
        out
    }

    /// The formula copied to a cell `cols` columns and `rows` rows away:
    /// relative references move along, absolute ones stay (TAB-19).
    pub fn moved(&self, cols: i64, rows: i64) -> Self {
        Self {
            expr: map_refs(&self.expr, &mut |r| r.moved(cols, rows)),
        }
    }

    /// The formula after `count` rows or columns were added before index
    /// `at` (`count` > 0) or deleted from `at` on (`count` < 0): the
    /// references that follow move, ranges grow or shrink, and a cell
    /// that was deleted becomes `#REF!` (TAB-10, TAB-11).
    pub fn shifted(&self, axis: Axis, at: u32, count: i64) -> Self {
        Self {
            expr: shift_expr(&self.expr, axis, at, count),
        }
    }
}

impl fmt::Display for Formula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "={}", ExprDisplay(&self.expr, 0))
    }
}

fn visit(expr: &Expr, f: &mut impl FnMut(&Expr)) {
    f(expr);
    match expr {
        Expr::Unary(_, a) => visit(a, f),
        Expr::Binary(_, a, b) => {
            visit(a, f);
            visit(b, f);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| visit(a, f)),
        _ => {}
    }
}

fn map_refs(expr: &Expr, f: &mut impl FnMut(CellRef) -> Option<CellRef>) -> Expr {
    match expr {
        Expr::Cell(c) => f(*c).map_or(Expr::Error(ErrorKind::Reference), Expr::Cell),
        Expr::Range(a, b) => match (f(*a), f(*b)) {
            (Some(a), Some(b)) => Expr::Range(a, b),
            _ => Expr::Error(ErrorKind::Reference),
        },
        Expr::Unary(op, a) => Expr::Unary(*op, Box::new(map_refs(a, f))),
        Expr::Binary(op, a, b) => {
            Expr::Binary(*op, Box::new(map_refs(a, f)), Box::new(map_refs(b, f)))
        }
        Expr::Call(name, args) => {
            Expr::Call(name.clone(), args.iter().map(|a| map_refs(a, f)).collect())
        }
        other => other.clone(),
    }
}

/// An index after `count` rows or columns are added at `at` or deleted
/// from it; none when the index itself is deleted.
fn shift_index(i: u32, at: u32, count: i64) -> Option<u32> {
    if i < at {
        return Some(i);
    }
    if count >= 0 {
        return u32::try_from(i64::from(i) + count).ok();
    }
    let deleted_end = i64::from(at) - count;
    if i64::from(i) < deleted_end {
        None
    } else {
        u32::try_from(i64::from(i) + count).ok()
    }
}

fn shift_ref(r: CellRef, axis: Axis, at: u32, count: i64) -> Option<CellRef> {
    match axis {
        Axis::Rows => shift_index(r.row, at, count).map(|row| CellRef { row, ..r }),
        Axis::Columns => shift_index(r.col, at, count).map(|col| CellRef { col, ..r }),
    }
}

fn shift_expr(expr: &Expr, axis: Axis, at: u32, count: i64) -> Expr {
    match expr {
        Expr::Cell(c) => {
            shift_ref(*c, axis, at, count).map_or(Expr::Error(ErrorKind::Reference), Expr::Cell)
        }
        Expr::Range(a, b) => {
            // The ends of a range move apart or together; a range all of
            // whose cells are deleted becomes #REF!.
            let index = |r: &CellRef| match axis {
                Axis::Rows => r.row,
                Axis::Columns => r.col,
            };
            let (lo, hi) = (index(a).min(index(b)), index(a).max(index(b)));
            let new_lo = if count < 0 {
                let end = i64::from(at) - count;
                if i64::from(lo) >= i64::from(at) && i64::from(lo) < end {
                    u32::try_from(i64::from(at)).ok()
                } else {
                    shift_index(lo, at, count)
                }
            } else {
                shift_index(lo, at, count)
            };
            let new_hi = if count < 0 {
                let end = i64::from(at) - count;
                if i64::from(hi) >= i64::from(at) && i64::from(hi) < end {
                    at.checked_sub(1)
                } else {
                    shift_index(hi, at, count)
                }
            } else {
                shift_index(hi, at, count)
            };
            match (new_lo, new_hi) {
                (Some(lo2), Some(hi2)) if lo2 <= hi2 => {
                    let set = |r: CellRef, v: u32| match axis {
                        Axis::Rows => CellRef { row: v, ..r },
                        Axis::Columns => CellRef { col: v, ..r },
                    };
                    let (first, last) = if index(a) <= index(b) { (a, b) } else { (b, a) };
                    Expr::Range(set(*first, lo2), set(*last, hi2))
                }
                _ => Expr::Error(ErrorKind::Reference),
            }
        }
        Expr::Unary(op, a) => Expr::Unary(*op, Box::new(shift_expr(a, axis, at, count))),
        Expr::Binary(op, a, b) => Expr::Binary(
            *op,
            Box::new(shift_expr(a, axis, at, count)),
            Box::new(shift_expr(b, axis, at, count)),
        ),
        Expr::Call(name, args) => Expr::Call(
            name.clone(),
            args.iter()
                .map(|a| shift_expr(a, axis, at, count))
                .collect(),
        ),
        other => other.clone(),
    }
}

struct ExprDisplay<'a>(&'a Expr, u8);

impl fmt::Display for ExprDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parent = self.1;
        match self.0 {
            Expr::Number(n) => f.write_str(&formula_number(*n)),
            Expr::Text(t) => write!(f, "\"{}\"", t.replace('"', "\"\"")),
            Expr::Bool(b) => f.write_str(if *b { "TRUE" } else { "FALSE" }),
            Expr::Error(e) => f.write_str(e.code()),
            Expr::Cell(c) => write!(f, "{c}"),
            Expr::Range(a, b) => write!(f, "{a}:{b}"),
            Expr::Header(h) => write!(f, "'{}'", h.replace('\'', "''")),
            Expr::Unary(UnaryOp::Percent, a) => write!(f, "{}%", ExprDisplay(a, 7)),
            Expr::Unary(op, a) => {
                let sign = if *op == UnaryOp::Negate { "-" } else { "+" };
                // A sign under a percent takes parentheses: -(1)% reads
                // as the sign of the percent.
                if parent > 6 {
                    write!(f, "({sign}{})", ExprDisplay(a, 6))
                } else {
                    write!(f, "{sign}{}", ExprDisplay(a, 6))
                }
            }
            Expr::Binary(op, a, b) => {
                let p = op.precedence();
                let open = p < parent;
                if open {
                    f.write_str("(")?;
                }
                write!(
                    f,
                    "{}{}{}",
                    ExprDisplay(a, p),
                    op.symbol(),
                    ExprDisplay(b, p + 1)
                )?;
                if open {
                    f.write_str(")")?;
                }
                Ok(())
            }
            Expr::Call(name, args) => {
                write!(f, "{name}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{}", ExprDisplay(a, 0))?;
                }
                f.write_str(")")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Text(String),
    Name(String),
    Quoted(String),
    Error(ErrorKind),
    Op(&'static str),
    Open,
    Close,
    Comma,
    Colon,
}

fn tokenize(text: &str) -> Result<Vec<(usize, Token)>, ParseError> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let err = |at: usize, message: &str| ParseError {
        at,
        message: message.to_owned(),
    };
    // The character at an index, or none past the end.
    let ch = |i: usize| chars.get(i).map_or('\0', |c| c.1);
    while let Some(&(at, c)) = chars.get(i) {
        match c {
            c if c.is_whitespace() => i += 1,
            '0'..='9' | '.' => {
                while ch(i).is_ascii_digit() || ch(i) == '.' {
                    i += 1;
                }
                if matches!(ch(i), 'e' | 'E') {
                    let mut j = i + 1;
                    if matches!(ch(j), '+' | '-') {
                        j += 1;
                    }
                    if ch(j).is_ascii_digit() {
                        i = j;
                        while ch(i).is_ascii_digit() {
                            i += 1;
                        }
                    }
                }
                // A name that starts with digits, as 2006Q1, is a header.
                if ch(i).is_alphabetic() || ch(i) == '_' {
                    while is_name_char(ch(i)) {
                        i += 1;
                    }
                    let end = chars.get(i).map_or(text.len(), |c| c.0);
                    out.push((at, Token::Name(text[at..end].to_owned())));
                    continue;
                }
                let end = chars.get(i).map_or(text.len(), |c| c.0);
                let n: f64 = text[at..end].parse().map_err(|_| err(at, "bad number"))?;
                // 1E999 is past the largest number, and could not be
                // written back.
                if !n.is_finite() {
                    return Err(err(at, "number too large"));
                }
                out.push((at, Token::Number(n)));
            }
            '"' | '\'' => {
                let quote = c;
                i += 1;
                let mut s = String::new();
                loop {
                    let Some(&(_, ch)) = chars.get(i) else {
                        return Err(err(at, "unterminated text"));
                    };
                    i += 1;
                    if ch == quote {
                        if chars.get(i).map(|c| c.1) == Some(quote) {
                            s.push(quote);
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    s.push(ch);
                }
                out.push((
                    at,
                    if quote == '"' {
                        Token::Text(s)
                    } else {
                        Token::Quoted(s)
                    },
                ));
            }
            '#' => {
                i += 1;
                while ch(i).is_ascii_alphanumeric() || "/!?".contains(ch(i)) {
                    i += 1;
                }
                let end = chars.get(i).map_or(text.len(), |c| c.0);
                let code = &text[at..end];
                let kind = ErrorKind::from_code(code).ok_or_else(|| err(at, "unknown error"))?;
                out.push((at, Token::Error(kind)));
            }
            '(' => {
                out.push((at, Token::Open));
                i += 1;
            }
            ')' => {
                out.push((at, Token::Close));
                i += 1;
            }
            ',' | ';' => {
                out.push((at, Token::Comma));
                i += 1;
            }
            ':' => {
                out.push((at, Token::Colon));
                i += 1;
            }
            '<' | '>' => {
                let next = chars.get(i + 1).map(|c| c.1);
                let op = match (c, next) {
                    ('<', Some('=')) => "<=",
                    ('<', Some('>')) => "<>",
                    ('>', Some('=')) => ">=",
                    ('<', _) => "<",
                    _ => ">",
                };
                i += op.len();
                out.push((at, Token::Op(op)));
            }
            '≤' | '≥' | '≠' => {
                out.push((
                    at,
                    Token::Op(match c {
                        '≤' => "<=",
                        '≥' => ">=",
                        _ => "<>",
                    }),
                ));
                i += 1;
            }
            '+' | '-' | '*' | '/' | '^' | '&' | '=' | '%' | '×' | '÷' | '−' => {
                out.push((
                    at,
                    Token::Op(match c {
                        '+' => "+",
                        '-' | '−' => "-",
                        '*' | '×' => "*",
                        '/' | '÷' => "/",
                        '^' => "^",
                        '&' => "&",
                        '=' => "=",
                        _ => "%",
                    }),
                ));
                i += 1;
            }
            c if is_name_char(c) => {
                while is_name_char(ch(i)) {
                    i += 1;
                }
                let end = chars.get(i).map_or(text.len(), |c| c.0);
                out.push((at, Token::Name(text[at..end].to_owned())));
            }
            _ => return Err(err(at, "unexpected character")),
        }
    }
    Ok(out)
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$' || c == '.'
}

/// How deep a formula may nest, in parentheses, operators, signs and
/// arguments, before it is refused: deeper, the parser and then the
/// evaluator and the writer, which recurse, would overflow the stack. A
/// file can hold any formula.
pub const MAX_NESTING: usize = 512;

struct Parser<'a> {
    tokens: &'a [(usize, Token)],
    pos: usize,
    /// How deep the expression being read now is.
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.1)
    }

    fn at(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map_or_else(|| self.tokens.last().map_or(0, |t| t.0 + 1), |t| t.0)
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            at: self.at(),
            message: message.to_owned(),
        }
    }

    fn binary_op(&self) -> Option<BinaryOp> {
        Some(match self.peek()? {
            Token::Op(op) => match *op {
                "+" => BinaryOp::Add,
                "-" => BinaryOp::Subtract,
                "*" => BinaryOp::Multiply,
                "/" => BinaryOp::Divide,
                "^" => BinaryOp::Power,
                "&" => BinaryOp::Concat,
                "=" => BinaryOp::Equal,
                "<>" => BinaryOp::NotEqual,
                "<" => BinaryOp::Less,
                "<=" => BinaryOp::LessOrEqual,
                ">" => BinaryOp::Greater,
                ">=" => BinaryOp::GreaterOrEqual,
                _ => return None,
            },
            _ => return None,
        })
    }

    /// Goes one level deeper, or refuses a formula that nests too deep.
    fn enter(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(self.error("the formula nests too deep"));
        }
        Ok(())
    }

    fn expr(&mut self, min: u8) -> Result<Expr, ParseError> {
        let depth = self.depth;
        let mut left = self.unary()?;
        while let Some(op) = self.binary_op() {
            let p = op.precedence();
            if p < min.max(1) {
                break;
            }
            // Each operator puts what came before one level deeper.
            self.enter()?;
            self.pos += 1;
            let right = self.expr(p + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        self.depth = depth;
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        self.enter()?;
        let e = self.unary_inner();
        self.depth -= 1;
        e
    }

    fn unary_inner(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(Token::Op("-")) => {
                self.pos += 1;
                Ok(Expr::Unary(UnaryOp::Negate, Box::new(self.unary()?)))
            }
            Some(Token::Op("+")) => {
                self.pos += 1;
                Ok(Expr::Unary(UnaryOp::Plus, Box::new(self.unary()?)))
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let depth = self.depth;
        let mut e = self.primary()?;
        while self.peek() == Some(&Token::Op("%")) {
            self.enter()?;
            self.pos += 1;
            e = Expr::Unary(UnaryOp::Percent, Box::new(e));
        }
        self.depth = depth;
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.error("formula ends too soon"));
        };
        self.pos += 1;
        match token {
            Token::Number(n) => Ok(Expr::Number(n)),
            Token::Text(t) => Ok(Expr::Text(t)),
            Token::Quoted(q) => Ok(Expr::Header(q)),
            Token::Error(e) => Ok(Expr::Error(e)),
            Token::Open => {
                // A parenthesis costs the stack more than a sign: it
                // counts twice.
                self.enter()?;
                let e = self.expr(0)?;
                self.depth -= 1;
                if self.peek() != Some(&Token::Close) {
                    return Err(self.error("missing )"));
                }
                self.pos += 1;
                Ok(e)
            }
            Token::Name(name) => {
                if self.peek() == Some(&Token::Open) {
                    self.pos += 1;
                    self.enter()?;
                    let mut args = Vec::new();
                    if self.peek() == Some(&Token::Close) {
                        self.pos += 1;
                    } else {
                        loop {
                            args.push(self.expr(0)?);
                            match self.peek() {
                                Some(Token::Comma) => self.pos += 1,
                                Some(Token::Close) => {
                                    self.pos += 1;
                                    break;
                                }
                                _ => return Err(self.error("missing ) or ,")),
                            }
                        }
                    }
                    self.depth -= 1;
                    return Ok(Expr::Call(name.to_uppercase(), args));
                }
                if let Some(a) = CellRef::parse(&name) {
                    if self.peek() == Some(&Token::Colon) {
                        self.pos += 1;
                        let Some(Token::Name(other)) = self.peek().cloned() else {
                            return Err(self.error("a cell must follow :"));
                        };
                        let b = CellRef::parse(&other).ok_or_else(|| self.error("bad cell"))?;
                        self.pos += 1;
                        return Ok(Expr::Range(a, b));
                    }
                    return Ok(Expr::Cell(a));
                }
                match name.to_uppercase().as_str() {
                    "TRUE" | "VERDADEIRO" => Ok(Expr::Bool(true)),
                    "FALSE" | "FALSO" => Ok(Expr::Bool(false)),
                    _ => Ok(Expr::Header(name)),
                }
            }
            _ => {
                self.pos -= 1;
                Err(self.error("unexpected token"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) -> String {
        Formula::parse(text).unwrap().to_string()
    }

    #[test]
    fn tab_32_formulas_read_and_write() {
        assert_eq!(round_trip("=1+2*3"), "=1+2*3");
        assert_eq!(round_trip("=(1+2)*3"), "=(1+2)*3");
        assert_eq!(round_trip("= sum( b2 : $C$5 ; 3 )"), "=SUM(B2:$C$5,3)");
        assert_eq!(round_trip("=A1&\"x\"\"y\""), "=A1&\"x\"\"y\"");
        assert_eq!(round_trip("=-A1^2"), "=-A1^2");
        assert_eq!(round_trip("=50%"), "=50%");
        assert_eq!(round_trip("=SUM('Total 2006')"), "=SUM('Total 2006')");
        assert_eq!(round_trip("=SUM(Vendas)"), "=SUM('Vendas')");
        assert_eq!(round_trip("=A1<>B1"), "=A1<>B1");
        assert_eq!(round_trip("=1-(2-3)"), "=1-(2-3)");
        assert!(Formula::parse("=SUM(1,").is_err());
        assert!(Formula::parse("=(1").is_err());
    }

    #[test]
    fn tab_19_copies_move_relative_references() {
        let f = Formula::parse("=A1+$B$1+C$2+$D3").unwrap();
        assert_eq!(f.moved(1, 2).to_string(), "=B3+$B$1+D$2+$D5");
    }

    #[test]
    fn tab_10_tab_11_rows_added_and_deleted_move_references() {
        let f = Formula::parse("=SUM(A2:A5)+A7").unwrap();
        assert_eq!(f.shifted(Axis::Rows, 3, 2).to_string(), "=SUM(A2:A7)+A9");
        assert_eq!(f.shifted(Axis::Rows, 2, -2).to_string(), "=SUM(A2:A3)+A5");
        assert_eq!(
            f.shifted(Axis::Rows, 6, -1).to_string(),
            "=SUM(A2:A5)+#REF!"
        );
        assert_eq!(f.shifted(Axis::Rows, 1, -4).to_string(), "=SUM(#REF!)+A3");
        let c = Formula::parse("=B1").unwrap();
        assert_eq!(c.shifted(Axis::Columns, 0, 1).to_string(), "=C1");
    }

    #[test]
    fn column_names() {
        assert_eq!(CellRef::column_name(0), "A");
        assert_eq!(CellRef::column_name(25), "Z");
        assert_eq!(CellRef::column_name(26), "AA");
        assert_eq!(CellRef::parse("AA10"), Some(CellRef::new(26, 9)));
        assert_eq!(CellRef::parse("A0"), None);
    }

    #[test]
    fn a_number_past_the_largest_is_refused() {
        // Found by fuzzing formula_parse: 1E999 was read as infinity,
        // written as inf and read back as a header.
        assert!(Formula::parse("=1E999+1").is_err());
        assert!(Formula::parse("=1E308").is_ok());
    }

    #[test]
    fn a_sign_under_a_percent_keeps_its_parentheses() {
        // Found by fuzzing formula_parse: written as =--(1+2)%, it read
        // back as the sign of the percent.
        for text in ["=-(-(1+2))%", "=(-A1)%", "=-A1%"] {
            let f = Formula::parse(text).unwrap();
            assert_eq!(Formula::parse(&f.to_string()).unwrap(), f, "{text}");
        }
        assert_eq!(Formula::parse("=(-A1)%").unwrap().to_string(), "=(-A1)%");
    }

    #[test]
    fn small_and_large_constants_are_written_in_full() {
        // A formula written back after a row was inserted lost its small
        // constants: =A1*1E-16 became =A2*0 (review RV, batch 2).
        assert_eq!(
            Formula::parse("=A1*1E-16").unwrap().moved(0, 1).to_string(),
            "=A2*1E-16"
        );
        for text in [
            "=1.2345E-14",
            "=0.1",
            "=123.456",
            "=1E300",
            "=0.00001",
            "=3",
        ] {
            let once = round_trip(text);
            let n = |t: &str| match Formula::parse(t).unwrap().expr {
                Expr::Number(n) => n,
                other => panic!("{other:?}"),
            };
            assert_eq!(n(&once).to_bits(), n(text).to_bits(), "{text} -> {once}");
        }
    }

    #[test]
    fn formulas_that_nest_too_deep_are_refused() {
        // 200,000 parentheses, signs or operators overflowed the stack of
        // the parser, and then of the evaluator (review RV, batch 2).
        let deep = format!("={}1{}", "(".repeat(200_000), ")".repeat(200_000));
        assert!(Formula::parse(&deep).is_err());
        assert!(Formula::parse(&format!("={}1", "-".repeat(200_000))).is_err());
        assert!(Formula::parse(&format!("=1{}", "%".repeat(200_000))).is_err());
        assert!(Formula::parse(&format!("=1{}", "+1".repeat(200_000))).is_err());
        assert!(
            Formula::parse_odf(&format!(
                "of:={}1{}",
                "(".repeat(200_000),
                ")".repeat(200_000)
            ))
            .is_err()
        );
        let within = format!("={}1{}", "(".repeat(100), ")".repeat(100));
        assert!(Formula::parse(&within).is_ok());
        // A long sum of cells, as people write, stays within.
        let sum = format!("=A1{}", "+A1".repeat(400));
        assert!(Formula::parse(&sum).is_ok());
    }

    #[test]
    fn the_deepest_formula_allowed_is_evaluated_and_written() {
        // In a thread with a small stack, as the tests have, in a build
        // without optimization.
        struct Cells;
        impl crate::Context for Cells {
            fn cell(&self, _: CellRef) -> crate::Value {
                crate::Value::Number(1.0)
            }
            fn header(&self, _: &str) -> Option<(CellRef, CellRef)> {
                None
            }
        }
        let n = MAX_NESTING / 3 - 1;
        for text in [
            format!("={}A1{}", "(-".repeat(n), ")".repeat(n)),
            format!("={}A1{}", "ABS(".repeat(n), ")".repeat(n)),
            format!(
                "={}A1{}",
                "(".repeat(MAX_NESTING / 2 - 1),
                ")".repeat(MAX_NESTING / 2 - 1)
            ),
            format!("=A1{}", "^A1".repeat(MAX_NESTING - 2)),
            format!("=A1{}", "-A1".repeat(MAX_NESTING - 2)),
        ] {
            let formula = Formula::parse(&text).unwrap();
            assert!(!formula.to_string().is_empty());
            let _ = crate::evaluate(&formula, &Cells);
        }
    }
}
