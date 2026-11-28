// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The text layers of USD (`usda`): the prims defined in them with their
//! attributes, relationships and references, read into a [`Layer`].
//!
//! What a model needs is read and the rest is skipped by its brackets:
//! variant sets, dictionaries and the metadata that is not
//! `defaultPrim`, `upAxis`, `active`, `references`, `payload` or the
//! `interpolation` of a primvar.

use super::{Attribute, Layer, Prim, Reference, Value, keeps_samples};
use crate::{Error, Result, limits, malformed};

/// A token of the text.
#[derive(Clone, Debug, PartialEq)]
enum Token {
    /// A name, a keyword or a number as written, as `point3f`,
    /// `xformOp:translate`, `1.5e-3` or `-inf`.
    Word(String),
    /// A string between quotes.
    Text(String),
    /// An asset path between `@`.
    Asset(String),
    /// A path between `<` and `>`.
    Path(String),
    /// One of `( ) [ ] { } = , : ;`.
    Punct(u8),
}

/// The tokens of a text, without its comments.
fn tokens(text: &str) -> Result<Vec<Token>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    let at_byte = |i: usize| bytes.get(i).copied();
    while let Some(b) = at_byte(at) {
        match b {
            b' ' | b'\t' | b'\r' | b'\n' => at += 1,
            b'#' => {
                while at_byte(at).is_some_and(|b| b != b'\n') {
                    at += 1;
                }
            }
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'=' | b',' | b':' | b';' => {
                out.push(Token::Punct(b));
                at += 1;
            }
            b'"' | b'\'' => {
                // A string of three quotes may span lines.
                let triple = bytes.get(at..at + 3) == Some(&[b, b, b]);
                let open = if triple { 3 } else { 1 };
                let mut value = Vec::new();
                let mut i = at + open;
                loop {
                    let c = at_byte(i).ok_or_else(|| malformed("a string without its end"))?;
                    if c == b'\\' {
                        let next = at_byte(i + 1).ok_or_else(|| malformed("a string cut short"))?;
                        value.push(match next {
                            b'n' => b'\n',
                            b't' => b'\t',
                            other => other,
                        });
                        i += 2;
                        continue;
                    }
                    if c == b && (!triple || bytes.get(i..i + 3) == Some(&[b, b, b])) {
                        i += open;
                        break;
                    }
                    if c == b'\n' && !triple {
                        return Err(malformed("a string without its end"));
                    }
                    value.push(c);
                    i += 1;
                }
                out.push(Token::Text(String::from_utf8_lossy(&value).into_owned()));
                at = i;
            }
            b'@' => {
                let triple = bytes.get(at..at + 3) == Some(b"@@@");
                let open = if triple { 3 } else { 1 };
                let rest = bytes.get(at + open..).unwrap_or_default();
                let end = if triple {
                    rest.windows(3).position(|w| w == b"@@@")
                } else {
                    rest.iter().position(|c| *c == b'@')
                }
                .ok_or_else(|| malformed("an asset path without its end"))?;
                let value = rest.get(..end).unwrap_or_default();
                out.push(Token::Asset(String::from_utf8_lossy(value).into_owned()));
                at += open + end + open;
            }
            b'<' => {
                let rest = bytes.get(at + 1..).unwrap_or_default();
                let end = rest
                    .iter()
                    .position(|c| *c == b'>')
                    .ok_or_else(|| malformed("a path without its end"))?;
                let value = rest.get(..end).unwrap_or_default();
                out.push(Token::Path(String::from_utf8_lossy(value).into_owned()));
                at += end + 2;
            }
            _ => {
                let start = at;
                while at_byte(at).is_some_and(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-' | b'+' | b'!' | b'|')
                        // A colon inside a name, as `xformOp:translate`,
                        // and not the one of a time sample.
                        || (c == b':' && at_byte(at + 1).is_some_and(|n| n.is_ascii_alphabetic() || n == b'_'))
                }) {
                    at += 1;
                }
                if at == start {
                    return Err(malformed(format!("a byte {b:#x} out of place")));
                }
                let word = text.get(start..at).unwrap_or_default();
                out.push(Token::Word(word.to_owned()));
            }
        }
    }
    Ok(out)
}

/// The reader of the tokens of a layer.
struct Parser {
    tokens: Vec<Token>,
    at: usize,
    prims: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Result<Token> {
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .ok_or_else(|| malformed("the layer ends too soon"))?;
        self.at += 1;
        Ok(token)
    }

    fn is_punct(&self, p: u8) -> bool {
        self.peek() == Some(&Token::Punct(p))
    }

    fn eat(&mut self, p: u8) -> bool {
        let found = self.is_punct(p);
        if found {
            self.at += 1;
        }
        found
    }

    fn expect(&mut self, p: u8) -> Result<()> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(malformed(format!("`{}` was expected", p as char)))
        }
    }

    fn word(&mut self) -> Result<String> {
        match self.next()? {
            Token::Word(w) => Ok(w),
            other => Err(malformed(format!("a name was expected, not {other:?}"))),
        }
    }

    /// Skips a whole value or bracketed group, whatever it holds.
    fn skip_value(&mut self) -> Result<()> {
        let mut depth = 0usize;
        loop {
            match self.next()? {
                Token::Punct(b'(' | b'[' | b'{') => depth += 1,
                Token::Punct(b')' | b']' | b'}') => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| malformed("a bracket closed twice"))?;
                }
                _ => {}
            }
            if depth == 0 {
                return Ok(());
            }
            if depth > limits::DEPTH {
                return Err(Error::TooLarge);
            }
        }
    }

    /// A value: a number, a string, an asset, a path, a tuple, a list, a
    /// dictionary (skipped) or `None`.
    fn value(&mut self, depth: usize) -> Result<Option<Value>> {
        if depth > limits::DEPTH {
            return Err(Error::TooLarge);
        }
        Ok(match self.peek() {
            Some(Token::Punct(b'{')) => {
                self.skip_value()?;
                None
            }
            Some(Token::Punct(b'(')) => {
                self.at += 1;
                let mut items = Vec::new();
                while !self.eat(b')') {
                    items.push(self.value(depth + 1)?);
                    if !self.eat(b',') && !self.is_punct(b')') {
                        return Err(malformed("`,` or `)` was expected in a tuple"));
                    }
                }
                Some(tuple_of(items))
            }
            Some(Token::Punct(b'[')) => {
                self.at += 1;
                let mut items = Vec::new();
                while !self.eat(b']') {
                    items.push(self.value(depth + 1)?);
                    if items.len() > limits::CORNERS {
                        return Err(Error::TooLarge);
                    }
                    if !self.eat(b',') && !self.is_punct(b']') {
                        return Err(malformed("`,` or `]` was expected in a list"));
                    }
                }
                list_of(items)
            }
            _ => match self.next()? {
                Token::Text(t) => Some(Value::Text(t)),
                Token::Asset(a) => Some(Value::Asset(a)),
                Token::Path(p) => Some(Value::Text(p)),
                Token::Word(w) => word_value(&w),
                Token::Punct(p) => return Err(malformed(format!("`{}` out of place", p as char))),
            },
        })
    }

    /// The metadata of a prim, between brackets, into it.
    fn prim_metadata(&mut self, prim: &mut Prim, layer: Option<&mut Layer>) -> Result<()> {
        let mut layer = layer;
        self.expect(b'(')?;
        while !self.eat(b')') {
            match self.next()? {
                // A doc string alone.
                Token::Text(_) => continue,
                Token::Word(mut key) => {
                    // The operation of a list, as `prepend references`.
                    if matches!(
                        key.as_str(),
                        "prepend" | "append" | "add" | "delete" | "reorder"
                    ) {
                        let op = key;
                        key = self.word()?;
                        if op == "delete" || op == "reorder" {
                            self.expect(b'=')?;
                            self.skip_value()?;
                            continue;
                        }
                    }
                    if !self.eat(b'=') {
                        continue;
                    }
                    match key.as_str() {
                        "references" | "payload" => self.references(prim)?,
                        _ => {
                            let value = self.value(0)?;
                            match (key.as_str(), value) {
                                ("active", Some(v)) => {
                                    prim.active = !matches!(v, Value::Bool(false))
                                }
                                ("defaultPrim", Some(Value::Text(t))) => {
                                    if let Some(layer) = layer.as_deref_mut() {
                                        layer.default_prim = Some(t);
                                    }
                                }
                                ("upAxis", Some(Value::Text(t))) => {
                                    if let Some(layer) = layer.as_deref_mut() {
                                        layer.z_up = t == "Z";
                                    }
                                }
                                (
                                    name @ ("startTimeCode" | "timeCodesPerSecond"
                                    | "framesPerSecond"),
                                    Some(v),
                                ) => {
                                    if let Some(layer) = layer.as_deref_mut() {
                                        let n = v.number();
                                        match name {
                                            "startTimeCode" => layer.start = n,
                                            "timeCodesPerSecond" => layer.rate = n,
                                            _ => layer.frames = n,
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
            self.eat(b';');
        }
        Ok(())
    }

    /// The references or the payload of a prim: one, a list, or `None`.
    fn references(&mut self, prim: &mut Prim) -> Result<()> {
        let list = self.eat(b'[');
        loop {
            if list && self.eat(b']') {
                break;
            }
            let reference = match self.peek().cloned() {
                Some(Token::Asset(asset)) => {
                    self.at += 1;
                    let prim = match self.peek().cloned() {
                        Some(Token::Path(p)) => {
                            self.at += 1;
                            Some(p)
                        }
                        _ => None,
                    };
                    Some(Reference { asset, prim })
                }
                Some(Token::Path(p)) => {
                    self.at += 1;
                    Some(Reference {
                        asset: String::new(),
                        prim: Some(p),
                    })
                }
                _ => {
                    self.skip_value_or_word()?;
                    None
                }
            };
            // The layer offset or the data of a reference.
            if self.is_punct(b'(') {
                self.skip_value()?;
            }
            if let Some(r) = reference {
                prim.references.push(r);
            }
            if !list || !self.eat(b',') {
                if list {
                    self.expect(b']')?;
                }
                break;
            }
        }
        Ok(())
    }

    /// The metadata of an attribute, between brackets: its interpolation.
    fn attribute_metadata(&mut self, attribute: &mut Attribute) -> Result<()> {
        self.expect(b'(')?;
        while !self.eat(b')') {
            match self.next()? {
                Token::Word(key) if self.eat(b'=') => {
                    let value = self.value(0)?;
                    if let (true, Some(Value::Text(t))) = (key == "interpolation", value) {
                        attribute.interpolation = Some(t);
                    }
                }
                Token::Punct(b'(' | b'[' | b'{') => {
                    self.at -= 1;
                    self.skip_value()?;
                }
                _ => {}
            }
            self.eat(b';');
        }
        Ok(())
    }

    /// The body of a prim, between braces: its properties and children.
    fn prim_body(&mut self, prim: &mut Prim, depth: usize) -> Result<()> {
        if depth > limits::DEPTH {
            return Err(Error::TooLarge);
        }
        self.expect(b'{')?;
        while !self.eat(b'}') {
            let word = match self.peek() {
                Some(Token::Word(w)) => w.clone(),
                Some(Token::Punct(b';')) => {
                    self.at += 1;
                    continue;
                }
                _ => return Err(malformed("a property or a prim was expected")),
            };
            match word.as_str() {
                "def" | "over" | "class" => {
                    self.at += 1;
                    let child = self.prim(word == "class", depth + 1)?;
                    if let Some(child) = child {
                        prim.children.push(child);
                    }
                }
                "variantSet" => {
                    // Variants are left out.
                    self.at += 1;
                    self.next()?;
                    self.expect(b'=')?;
                    self.skip_value()?;
                }
                "reorder" => {
                    self.at += 1;
                    self.word()?;
                    self.expect(b'=')?;
                    self.skip_value()?;
                }
                _ => self.property(prim)?,
            }
        }
        Ok(())
    }

    /// A prim, after its specifier: its type, name, metadata and body.
    /// A class is read and left out.
    fn prim(&mut self, class: bool, depth: usize) -> Result<Option<Prim>> {
        self.prims += 1;
        if self.prims > limits::PRIMS {
            return Err(Error::TooLarge);
        }
        let type_name = match self.peek() {
            Some(Token::Word(_)) => self.word()?,
            _ => String::new(),
        };
        let name = match self.next()? {
            Token::Text(t) => t,
            _ => return Err(malformed("a prim without its name")),
        };
        let mut prim = Prim::new(name);
        prim.type_name = type_name;
        if self.is_punct(b'(') {
            self.prim_metadata(&mut prim, None)?;
        }
        self.prim_body(&mut prim, depth)?;
        Ok((!class).then_some(prim))
    }

    /// A property: an attribute or a relationship.
    fn property(&mut self, prim: &mut Prim) -> Result<()> {
        let mut word = self.word()?;
        while matches!(
            word.as_str(),
            "custom" | "uniform" | "varying" | "config" | "prepend" | "append" | "add" | "delete"
        ) {
            word = self.word()?;
        }
        if word == "rel" {
            let name = self.word()?;
            let mut targets = Vec::new();
            if self.eat(b'=') {
                match self.value(0)? {
                    Some(Value::Text(t)) => targets.push(t),
                    Some(Value::Texts(ts)) => targets.extend(ts),
                    _ => {}
                }
            }
            if self.is_punct(b'(') {
                self.skip_value()?;
            }
            let name = name.trim_end_matches(".default").to_owned();
            prim.relationships.entry(name).or_default().extend(targets);
            return Ok(());
        }
        // A type, `[]` for an array, then the name.
        if self.eat(b'[') {
            self.expect(b']')?;
        }
        let full = self.word()?;
        let (name, suffix) = match full.rsplit_once('.') {
            Some((n, s @ ("connect" | "timeSamples" | "spline"))) => (n.to_owned(), s),
            _ => (full, ""),
        };
        let ty = word;
        let attribute = prim.attributes.entry(name.clone()).or_default();
        if self.eat(b'=') {
            match suffix {
                "connect" => match self.value(0)? {
                    Some(Value::Text(t)) => attribute.connection = Some(t),
                    Some(Value::Texts(ts)) => attribute.connection = ts.into_iter().next(),
                    _ => {}
                },
                "timeSamples" => {
                    // The first sample, when there is no default; all of
                    // those of a transform op.
                    let keep = keeps_samples(&name);
                    let samples = self.samples(keep)?;
                    let first = samples.first().map(|(_, v)| v.clone());
                    if attribute.value.is_none() {
                        attribute.value = first.map(|v| typed(&ty, v));
                    }
                    if keep {
                        attribute.samples = samples
                            .into_iter()
                            .map(|(t, v)| (t, typed(&ty, v)))
                            .collect();
                    }
                }
                "spline" => self.skip_value()?,
                _ => attribute.value = self.value(0)?.map(|v| typed(&ty, v)),
            }
        }
        if self.is_punct(b'(') {
            self.attribute_metadata(attribute)?;
        }
        Ok(())
    }

    /// The samples of a dictionary of time samples, by their times, in
    /// order: all of them when `all`, else the first, the rest skipped. A
    /// sample of `None` is left out.
    fn samples(&mut self, all: bool) -> Result<Vec<(f64, Value)>> {
        self.expect(b'{')?;
        let mut samples = Vec::new();
        let mut taken = false;
        while !self.eat(b'}') {
            // The time, then its value.
            let time = match self.next()? {
                Token::Word(w) => w.parse::<f64>().ok().filter(|t| t.is_finite()),
                _ => None,
            };
            self.expect(b':')?;
            if (taken && !all) || samples.len() >= limits::ANIMATION_KEYS {
                self.skip_value_or_word()?;
            } else {
                let value = self.value(0)?;
                taken = true;
                if let (Some(t), Some(v)) = (time, value) {
                    samples.push((t, v));
                }
            }
            self.eat(b',');
        }
        samples.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(samples)
    }

    fn skip_value_or_word(&mut self) -> Result<()> {
        if matches!(self.peek(), Some(Token::Punct(b'(' | b'[' | b'{'))) {
            self.skip_value()
        } else {
            self.next().map(|_| ())
        }
    }
}

/// The value of a word: a number, a boolean, or a token as `None`.
fn word_value(word: &str) -> Option<Value> {
    match word {
        "None" => None,
        "true" => Some(Value::Bool(true)),
        "false" => Some(Value::Bool(false)),
        "inf" => Some(Value::Float(f64::INFINITY)),
        "-inf" => Some(Value::Float(f64::NEG_INFINITY)),
        "nan" => Some(Value::Float(f64::NAN)),
        _ => {
            if let Ok(i) = word.parse::<i64>() {
                Some(Value::Int(i))
            } else if let Ok(f) = word.parse::<f64>() {
                Some(Value::Float(f))
            } else {
                Some(Value::Text(word.to_owned()))
            }
        }
    }
}

/// A tuple of numbers, or of tuples for a matrix.
fn tuple_of(items: Vec<Option<Value>>) -> Value {
    let rows: Vec<Vec<f64>> = items
        .iter()
        .filter_map(|v| match v {
            Some(Value::Tuple(t)) => Some(t.clone()),
            _ => None,
        })
        .collect();
    if rows.len() == 4 && rows.iter().all(|r| r.len() == 4) {
        let mut m = [0.0; 16];
        for (i, v) in rows.iter().flatten().enumerate() {
            if let Some(slot) = m.get_mut(i) {
                *slot = *v;
            }
        }
        return Value::Matrix(Box::new(m));
    }
    Value::Tuple(
        items
            .iter()
            .map(|v| v.as_ref().and_then(Value::number).unwrap_or(0.0))
            .collect(),
    )
}

/// An array of the values of a list.
/// A value as its declared type makes it: an array of whole numbers
/// written for a type of numbers that are not, as `float[] a = [0, 1]`,
/// is an array of numbers.
fn typed(ty: &str, value: Value) -> Value {
    let integer = matches!(ty, "int" | "uint" | "int64" | "uint64" | "uchar" | "bool");
    match value {
        Value::Ints(ints) if !integer && !ty.is_empty() => Value::Floats {
            dim: 1,
            data: ints.iter().map(|i| *i as f32).collect(),
        },
        other => other,
    }
}

fn list_of(items: Vec<Option<Value>>) -> Option<Value> {
    let first = items.iter().flatten().next();
    Some(match first {
        None => Value::Texts(Vec::new()),
        Some(Value::Int(_)) if items.iter().all(|v| matches!(v, Some(Value::Int(_)))) => {
            Value::Ints(
                items
                    .iter()
                    .filter_map(|v| match v {
                        Some(Value::Int(i)) => Some(i32::try_from(*i).unwrap_or(i32::MAX)),
                        _ => None,
                    })
                    .collect(),
            )
        }
        Some(Value::Int(_) | Value::Float(_)) => Value::Floats {
            dim: 1,
            data: items
                .iter()
                .map(|v| v.as_ref().and_then(Value::number).unwrap_or(0.0) as f32)
                .collect(),
        },
        Some(Value::Tuple(t)) => {
            let dim = t.len().max(1);
            let mut data = Vec::with_capacity(items.len() * dim);
            for v in &items {
                let mut tuple = match v {
                    Some(Value::Tuple(t)) => t.iter().map(|x| *x as f32).collect(),
                    _ => Vec::new(),
                };
                tuple.resize(dim, 0.0);
                data.extend(tuple);
            }
            Value::Floats {
                dim,
                data: data.into(),
            }
        }
        Some(Value::Matrix(_)) => Value::Matrices(
            items
                .iter()
                .filter_map(|v| match v {
                    Some(Value::Matrix(m)) => Some(**m),
                    _ => None,
                })
                .collect(),
        ),
        Some(_) => Value::Texts(
            items
                .iter()
                .filter_map(|v| v.as_ref().and_then(Value::text).map(str::to_owned))
                .collect(),
        ),
    })
}

/// Reads a text layer.
pub(crate) fn read(text: &str) -> Result<Layer> {
    if !text.trim_start().starts_with("#usda") {
        return Err(Error::Unknown);
    }
    let mut parser = Parser {
        tokens: tokens(text)?,
        at: 0,
        prims: 0,
    };
    let mut layer = Layer::new();
    if parser.is_punct(b'(') {
        let mut root = Prim::new("");
        parser.prim_metadata(&mut root, Some(&mut layer))?;
    }
    while let Some(token) = parser.peek() {
        match token {
            Token::Word(w) if matches!(w.as_str(), "def" | "over" | "class") => {
                let class = w == "class";
                parser.at += 1;
                if let Some(prim) = parser.prim(class, 0)? {
                    layer.root.children.push(prim);
                }
            }
            _ => return Err(malformed("a prim was expected at the top of the layer")),
        }
    }
    Ok(layer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_a_text_layer_reads_its_prims_and_values() {
        let layer = read(
            r#"#usda 1.0
(
    defaultPrim = "Root"
    upAxis = "Z"
    doc = """Made
by hand"""
)

def Xform "Root" (
    kind = "component"
)
{
    def Mesh "Cube" (
        prepend apiSchemas = ["MaterialBindingAPI"]
        prepend references = @./part.usdc@</Part>
    )
    {
        int[] faceVertexCounts = [3]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1.5e0, -inf)]
        texCoord2f[] primvars:st = [(0, 0), (1, 0), (0, 1)] (
            interpolation = "faceVarying"
        )
        matrix4d xformOp:transform = ( (1, 0, 0, 0), (0, 1, 0, 0), (0, 0, 1, 0), (5, 6, 7, 1) )
        float3 xformOp:rotateXYZ.timeSamples = { 0: (0, 45, 0), 10: (0, 90, 0) }
        uniform token[] xformOpOrder = ["xformOp:transform"]
        rel material:binding = </Root/Looks/Red>
        color3f inputs:diffuseColor.connect = </Root/Looks/T.outputs:rgb>
        asset inputs:file = @textures/a.png@ # a comment
    }
    class "Hidden" {}
    def "Off" ( active = false ) {}
}
"#,
        )
        .unwrap();
        assert_eq!(layer.default_prim.as_deref(), Some("Root"));
        assert!(layer.z_up);
        let cube = layer.root.find("/Root/Cube").unwrap();
        assert_eq!(cube.type_name, "Mesh");
        assert_eq!(
            cube.value("faceVertexCounts"),
            Some(&Value::Ints(vec![3].into()))
        );
        let Some(Value::Floats { dim, data }) = cube.value("points") else {
            panic!("points")
        };
        assert_eq!((*dim, data.len()), (3, 9));
        assert_eq!(data[7], 1.5);
        assert_eq!(
            cube.attributes["primvars:st"].interpolation.as_deref(),
            Some("faceVarying")
        );
        let Some(Value::Matrix(m)) = cube.value("xformOp:transform") else {
            panic!("matrix")
        };
        assert_eq!(&m[12..15], &[5.0, 6.0, 7.0]);
        assert_eq!(
            cube.value("xformOp:rotateXYZ"),
            Some(&Value::Tuple(vec![0.0, 45.0, 0.0]))
        );
        assert_eq!(cube.relationships["material:binding"], ["/Root/Looks/Red"]);
        assert_eq!(
            cube.attributes["inputs:diffuseColor"].connection.as_deref(),
            Some("/Root/Looks/T.outputs:rgb")
        );
        assert_eq!(
            cube.value("inputs:file"),
            Some(&Value::Asset("textures/a.png".into()))
        );
        assert_eq!(
            cube.references,
            [Reference {
                asset: "./part.usdc".into(),
                prim: Some("/Part".into())
            }]
        );
        // A class is left out; an inactive prim is kept, marked.
        assert!(layer.root.find("/Root/Hidden").is_none());
        assert!(!layer.root.find("/Root/Off").unwrap().active);
    }

    #[test]
    fn w116_broken_text_layers_are_errors() {
        for text in [
            "#usda 1.0\ndef Mesh \"A\" {",
            "#usda 1.0\ndef Mesh \"A\" { point3f[] points = [(0, 0 }",
            "#usda 1.0\ndef \"A\" { string s = \"open }",
            "#usda 1.0\n)",
            "not usd",
        ] {
            assert!(read(text).is_err(), "{text}");
        }
        // Deep nesting is refused rather than followed.
        let deep = format!("#usda 1.0\ndef \"A\" {{ int a = {} }}", "(".repeat(100_000));
        assert!(read(&deep).is_err());
    }
}
