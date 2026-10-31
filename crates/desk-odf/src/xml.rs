// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! A small XML tree with resolved namespaces, and a writer.
//!
//! ODF streams are read whole into this tree, which keeps only what the
//! readers need: elements, attributes and text, with every name as a
//! namespace URI and a local name. Comments, processing instructions and
//! the document type are dropped, and entities are never expanded beyond
//! the five predefined ones and character references.

use std::fmt::Write as _;

use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::{NsReader, XmlVersion};

use crate::error::OdfError;

/// How deep elements may nest before a stream is refused, so that a
/// hostile file cannot exhaust the stack of the readers.
const MAX_DEPTH: usize = 256;

/// A name with its namespace resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Name {
    pub ns: String,
    pub local: String,
}

impl Name {
    pub fn is(&self, ns: &str, local: &str) -> bool {
        self.ns == ns && self.local == local
    }
}

/// A node of the tree.
#[derive(Clone, Debug)]
pub(crate) enum Node {
    Element(Element),
    Text(String),
}

/// An element.
#[derive(Clone, Debug)]
pub(crate) struct Element {
    pub name: Name,
    pub attrs: Vec<(Name, String)>,
    pub children: Vec<Node>,
}

impl Element {
    /// The value of an attribute.
    pub fn attr(&self, ns: &str, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(name, _)| name.is(ns, local))
            .map(|(_, value)| value.as_str())
    }

    /// The child elements.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    /// The first child element with a name.
    pub fn child(&self, ns: &str, local: &str) -> Option<&Element> {
        self.elements().find(|e| e.name.is(ns, local))
    }

    /// The child elements with a name.
    pub fn children_named<'a>(
        &'a self,
        ns: &'a str,
        local: &'a str,
    ) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements().filter(move |e| e.name.is(ns, local))
    }

    /// All the text inside, in document order.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        for node in &self.children {
            match node {
                Node::Text(text) => out.push_str(text),
                Node::Element(element) => element.collect_text(out),
            }
        }
    }
}

fn resolved(result: ResolveResult<'_>) -> String {
    match result {
        ResolveResult::Bound(ns) => ns.0.to_owned(),
        ResolveResult::Unbound | ResolveResult::Unknown(_) => String::new(),
    }
}

fn predefined(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => return None,
    })
}

/// Parses a stream into its root element.
pub(crate) fn parse(source: &str) -> Result<Element, OdfError> {
    let mut reader = NsReader::from_str(source);
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        let ns = resolved(ns);
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(start) | Event::Empty(start) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(OdfError::Malformed("elements nest too deep".into()));
                }
                let local = start.local_name().as_ref().to_owned();
                let mut attrs = Vec::new();
                for attr in start.attributes() {
                    let attr = attr.map_err(quick_xml::Error::from)?;
                    if attr.key.as_namespace_binding().is_some() {
                        continue;
                    }
                    let (attr_ns, attr_local) = reader.resolver_mut().resolve_attribute(attr.key);
                    let name = Name {
                        ns: resolved(attr_ns),
                        local: attr_local.as_ref().to_owned(),
                    };
                    attrs.push((
                        name,
                        attr.normalized_value(XmlVersion::Explicit1_0)?.into_owned(),
                    ));
                }
                let element = Element {
                    name: Name { ns, local },
                    attrs,
                    children: Vec::new(),
                };
                if empty {
                    close(&mut stack, &mut root, element);
                } else {
                    stack.push(element);
                }
            }
            Event::End(_) => {
                let element = stack
                    .pop()
                    .ok_or_else(|| OdfError::Malformed("unbalanced end tag".into()))?;
                close(&mut stack, &mut root, element);
            }
            Event::Text(text) => push_text(&mut stack, &text.xml10_content()),
            Event::CData(data) => {
                push_text(&mut stack, data.as_ref());
            }
            Event::GeneralRef(reference) => {
                let c = if reference.is_char_ref() {
                    reference.resolve_char_ref()?
                } else {
                    predefined(&reference.xml10_content())
                };
                let c = c.ok_or_else(|| OdfError::Malformed("unknown entity".into()))?;
                push_text(&mut stack, c.encode_utf8(&mut [0; 4]));
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    if !stack.is_empty() {
        return Err(OdfError::Malformed(
            "the stream ends inside an element".into(),
        ));
    }
    root.ok_or_else(|| OdfError::Malformed("the stream has no root element".into()))
}

fn close(stack: &mut [Element], root: &mut Option<Element>, element: Element) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(Node::Element(element)),
        None => *root = Some(element),
    }
}

fn push_text(stack: &mut [Element], text: &str) {
    let Some(parent) = stack.last_mut() else {
        return;
    };
    if let Some(Node::Text(last)) = parent.children.last_mut() {
        last.push_str(text);
    } else {
        parent.children.push(Node::Text(text.to_owned()));
    }
}

/// Writes XML text, or an attribute value when `attribute` is set,
/// escaping what must be escaped. A reader turns a carriage return into a
/// line feed, and tabs and line ends in attribute values into spaces, so
/// those are written as character references to come back as they were.
pub(crate) fn escape(out: &mut String, text: &str, attribute: bool) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\r' => out.push_str("&#13;"),
            '\n' if attribute => out.push_str("&#10;"),
            '\t' if attribute => out.push_str("&#9;"),
            _ => out.push(c),
        }
    }
}

/// Builds an XML stream with prefixed names. The caller declares the
/// namespaces on the root element.
pub(crate) struct Writer {
    out: String,
    open: Vec<&'static str>,
    in_tag: bool,
}

impl Writer {
    pub fn new() -> Self {
        Self {
            out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
            open: Vec::new(),
            in_tag: false,
        }
    }

    fn close_tag(&mut self) {
        if self.in_tag {
            self.out.push('>');
            self.in_tag = false;
        }
    }

    /// Opens an element.
    pub fn start(&mut self, name: &'static str) -> &mut Self {
        self.close_tag();
        write!(self.out, "<{name}").unwrap();
        self.open.push(name);
        self.in_tag = true;
        self
    }

    /// Adds an attribute to the element just opened.
    pub fn attr(&mut self, name: &str, value: &str) -> &mut Self {
        debug_assert!(self.in_tag);
        write!(self.out, " {name}=\"").unwrap();
        escape(&mut self.out, value, true);
        self.out.push('"');
        self
    }

    /// Adds an attribute when there is a value.
    pub fn attr_opt(&mut self, name: &str, value: Option<&str>) -> &mut Self {
        if let Some(value) = value {
            self.attr(name, value);
        }
        self
    }

    /// Writes text inside the current element.
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.close_tag();
        escape(&mut self.out, text, false);
        self
    }

    /// Closes the current element.
    pub fn end(&mut self) -> &mut Self {
        let name = self.open.pop().expect("an element is open");
        if self.in_tag {
            self.out.push_str("/>");
            self.in_tag = false;
        } else {
            write!(self.out, "</{name}>").unwrap();
        }
        self
    }

    /// An element with only text inside.
    pub fn text_element(&mut self, name: &'static str, text: &str) -> &mut Self {
        self.start(name).text(text).end()
    }

    pub fn finish(mut self) -> String {
        assert!(self.open.is_empty(), "elements left open");
        self.close_tag();
        self.out
    }
}
