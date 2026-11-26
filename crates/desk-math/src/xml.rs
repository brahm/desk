// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A small reader and writer of the XML of MathML.
//!
//! The MathML that Desk keeps has gone through the reader of `desk-odf`,
//! which writes it back plainly; this reader takes that, and the MathML
//! typed in the equation editor, and gives up on what it cannot read.

use std::fmt::Write as _;

/// The namespace of MathML.
pub const MATHML: &str = "http://www.w3.org/1998/Math/MathML";

/// How deep elements nest at most; a deeper formula is not read.
const MAX_DEPTH: usize = 256;

/// An element of MathML: its local name, attributes, and content, which
/// is elements, or the text of a token element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Node {
    /// The local name, without a prefix.
    pub name: String,
    /// The attributes by local name, in order.
    pub attributes: Vec<(String, String)>,
    /// The elements inside.
    pub children: Vec<Node>,
    /// The text inside, as written.
    pub text: String,
}

impl Node {
    /// An element without attributes or content.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// A token element with its text.
    pub fn token(name: &str, text: &str) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
            ..Self::default()
        }
    }

    /// An element around others.
    pub fn with(name: &str, children: Vec<Node>) -> Self {
        Self {
            name: name.into(),
            children,
            ..Self::default()
        }
    }

    /// The element with an attribute set.
    pub fn attr(mut self, name: &str, value: &str) -> Self {
        self.attributes.retain(|(n, _)| n != name);
        self.attributes.push((name.into(), value.into()));
        self
    }

    /// The value of an attribute.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The text of a token element, with its spaces collapsed as MathML
    /// does.
    pub fn content(&self) -> String {
        self.text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The element as XML, with the namespace of MathML on it when it is
    /// the root.
    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, true);
        out
    }

    fn write(&self, out: &mut String, root: bool) {
        out.push('<');
        out.push_str(&self.name);
        if root {
            let _ = write!(out, " xmlns=\"{MATHML}\"");
        }
        for (name, value) in &self.attributes {
            let _ = write!(out, " {name}=\"");
            escape(out, value);
            out.push('"');
        }
        if self.children.is_empty() && self.text.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        escape(out, &self.text);
        for child in &self.children {
            child.write(out, false);
        }
        let _ = write!(out, "</{}>", self.name);
    }
}

fn escape(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// Reads MathML: a `math` element, whose prefixes are left out. Nothing
/// when it is not XML that this reader takes, or not MathML.
pub fn parse(source: &str) -> Option<Node> {
    let mut reader = Reader { rest: source };
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    loop {
        let text_end = reader.rest.find('<').unwrap_or(reader.rest.len());
        let text = &reader.rest[..text_end];
        if let Some(top) = stack.last_mut() {
            top.text.push_str(&unescape(text)?);
        } else if !text.trim().is_empty() {
            return None;
        }
        reader.rest = &reader.rest[text_end..];
        if reader.rest.is_empty() {
            break;
        }
        if let Some(rest) = reader.rest.strip_prefix("<!--") {
            reader.rest = &rest[rest.find("-->")? + 3..];
        } else if let Some(rest) = reader.rest.strip_prefix("<![CDATA[") {
            let end = rest.find("]]>")?;
            stack.last_mut()?.text.push_str(&rest[..end]);
            reader.rest = &rest[end + 3..];
        } else if reader.rest.starts_with("<?") || reader.rest.starts_with("<!") {
            reader.rest = &reader.rest[reader.rest.find('>')? + 1..];
        } else if let Some(rest) = reader.rest.strip_prefix("</") {
            let end = rest.find('>')?;
            let name = local(rest[..end].trim());
            reader.rest = &rest[end + 1..];
            let node = stack.pop()?;
            if node.name != name {
                return None;
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(node),
                None => root = Some(node),
            }
        } else {
            if root.is_some() {
                return None;
            }
            let (node, empty) = reader.start()?;
            if empty {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => root = Some(node),
                }
            } else {
                stack.push(node);
                if stack.len() > MAX_DEPTH {
                    return None;
                }
            }
        }
    }
    let root = root.filter(|_| stack.is_empty())?;
    (root.name == "math").then_some(root)
}

struct Reader<'a> {
    rest: &'a str,
}

impl Reader<'_> {
    /// A start tag, and whether it is empty.
    fn start(&mut self) -> Option<(Node, bool)> {
        let rest = &self.rest[1..];
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        if !plain(&rest[..name_end]) {
            return None;
        }
        let name = local(&rest[..name_end]);
        if name.is_empty() {
            return None;
        }
        let mut node = Node::new(name);
        let mut rest = &rest[name_end..];
        loop {
            rest = rest.trim_start();
            if let Some(after) = rest.strip_prefix("/>") {
                self.rest = after;
                return Some((node, true));
            }
            if let Some(after) = rest.strip_prefix('>') {
                self.rest = after;
                return Some((node, false));
            }
            let eq = rest.find('=')?;
            let attribute = rest[..eq].trim();
            rest = rest[eq + 1..].trim_start();
            let quote = rest.chars().next().filter(|q| *q == '"' || *q == '\'')?;
            let close = rest[1..].find(quote)? + 1;
            let value = unescape(&rest[1..close])?;
            rest = &rest[close + 1..];
            if !plain(attribute) {
                return None;
            }
            // Namespaces are not kept: every element is of MathML.
            if attribute == "xmlns" || attribute.starts_with("xmlns:") {
                continue;
            }
            let attribute = local(attribute);
            if node.get(attribute).is_none() {
                node.attributes.push((attribute.into(), value));
            }
        }
    }
}

/// Whether a name is one that XML takes, of the letters, digits and
/// marks that MathML names use.
fn plain(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Text with its references replaced; nothing when one is not known.
fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let end = after.find(';')?;
        let name = &after[..end];
        let c = match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(hex) = name.strip_prefix("#x").or(name.strip_prefix("#X")) {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse().ok()
                } else {
                    named(name).map(u32::from)
                };
                code.and_then(char::from_u32)?
            }
        };
        out.push(c);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// The entities of MathML that formulas use most, which other programs
/// write by name.
fn named(name: &str) -> Option<char> {
    Some(match name {
        "nbsp" => '\u{a0}',
        "minus" => '−',
        "times" => '×',
        "div" => '÷',
        "pm" | "PlusMinus" => '±',
        "InvisibleTimes" | "it" => '\u{2062}',
        "ApplyFunction" | "af" => '\u{2061}',
        "InvisibleComma" | "ic" => '\u{2063}',
        "infin" | "infinity" => '∞',
        "sum" | "Sum" => '∑',
        "prod" | "Product" => '∏',
        "int" | "Integral" => '∫',
        "le" | "leq" => '≤',
        "ge" | "geq" => '≥',
        "ne" => '≠',
        "approx" => '≈',
        "rarr" | "rightarrow" | "RightArrow" => '→',
        "larr" | "leftarrow" | "LeftArrow" => '←',
        "part" | "PartialD" => '∂',
        "alpha" => 'α',
        "beta" => 'β',
        "gamma" => 'γ',
        "delta" => 'δ',
        "pi" => 'π',
        "theta" => 'θ',
        "lambda" => 'λ',
        "mu" => 'μ',
        "sigma" => 'σ',
        "omega" => 'ω',
        "sdot" | "middot" | "CenterDot" => '·',
        "lang" | "langle" | "LeftAngleBracket" => '⟨',
        "rang" | "rangle" | "RightAngleBracket" => '⟩',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mathml_reads_with_prefixes_and_references() {
        let node = parse(
            "<?xml version=\"1.0\"?><m:math xmlns:m=\"http://www.w3.org/1998/Math/MathML\">\
             <m:mi>x</m:mi><m:mo>&lt;</m:mo><m:mn>&#x33;</m:mn><mo>&minus;</mo></m:math>",
        )
        .unwrap();
        assert_eq!(node.name, "math");
        let texts: Vec<_> = node.children.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["x", "<", "3", "−"]);
    }

    #[test]
    fn what_is_not_mathml_is_not_read() {
        assert!(parse("<svg/>").is_none());
        assert!(parse("<math><mi>x</mo></math>").is_none());
        assert!(parse("<math><mi>x</mi>").is_none());
        assert!(parse("<math/><math/>").is_none());
        assert!(parse("<math><mi>&bogus;</mi></math>").is_none());
        // Names are names, so what is read is written back the same.
        assert!(parse("<math><mi a\"/><x/=\"v\">x</mi></math>").is_none());
        assert!(parse("<math><m<i>x</m<i></math>").is_none());
        let deep = "<mrow>".repeat(300) + &"</mrow>".repeat(300);
        assert!(parse(&format!("<math>{deep}</math>")).is_none());
    }

    #[test]
    fn mathml_written_reads_back() {
        let node = Node::with(
            "math",
            vec![
                Node::token("mi", "a&b"),
                Node::token("mo", "<").attr("stretchy", "false"),
            ],
        );
        let xml = node.to_xml();
        assert!(xml.starts_with("<math xmlns=\"http://www.w3.org/1998/Math/MathML\">"));
        assert_eq!(parse(&xml).unwrap(), node);
    }
}
