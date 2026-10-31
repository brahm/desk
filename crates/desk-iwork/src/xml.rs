// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! A small XML tree with resolved namespaces.
//!
//! The index of an iWork document is read whole into this tree, which
//! keeps only what the reader needs: elements, attributes and text, with
//! every name as a namespace URI and a local name. Comments, processing instructions and
//! the document type are dropped, and entities are never expanded beyond
//! the five predefined ones and character references.

use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::{NsReader, XmlVersion};

use crate::error::IworkError;

/// How deep elements may nest before a stream is refused, so that a
/// malicious file cannot exhaust the stack of the readers.
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

    /// Every element inside this one, in document order, without
    /// recursion so that deep documents cannot overflow the stack.
    pub fn descendants(&self) -> impl Iterator<Item = &Element> {
        let mut out = Vec::new();
        let mut stack: Vec<&Element> = self.elements().collect();
        stack.reverse();
        while let Some(element) = stack.pop() {
            out.push(element);
            let start = stack.len();
            stack.extend(element.elements());
            stack[start..].reverse();
        }
        out.into_iter()
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
pub(crate) fn parse(source: &str) -> Result<Element, IworkError> {
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
                    return Err(IworkError::Malformed("elements nest too deep".into()));
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
                    .ok_or_else(|| IworkError::Malformed("unbalanced end tag".into()))?;
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
                let c = c.ok_or_else(|| IworkError::Malformed("unknown entity".into()))?;
                push_text(&mut stack, c.encode_utf8(&mut [0; 4]));
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    if !stack.is_empty() {
        return Err(IworkError::Malformed(
            "the stream ends inside an element".into(),
        ));
    }
    root.ok_or_else(|| IworkError::Malformed("the stream has no root element".into()))
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
