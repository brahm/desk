// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Formulas: the MathML of an embedded object of
//! `application/vnd.oasis.opendocument.formula`, as LibreOffice Math
//! writes equations (ODF 1.3 part 3, section 4; W104).
//!
//! The MathML is kept as text, made again from what was read: the
//! elements and attributes of the MathML namespace and their text, so
//! that what a file brought in (a doctype, entities, instructions or
//! other namespaces) never goes on to the next one.

use crate::error::OdfError;
use crate::xml::{self, Element, Node};

/// The media type of a formula in the manifest.
pub const FORMULA_TYPE: &str = "application/vnd.oasis.opendocument.formula";

/// The namespace of MathML.
pub const MATHML: &str = "http://www.w3.org/1998/Math/MathML";

/// How deep elements of MathML can be, as deep as the reader of XML
/// goes; a deeper formula is not read.
const MAX_DEPTH: usize = 256;

/// The MathML of a formula, from the `content.xml` of its object or from
/// MathML written by hand: its `math` element made again with the MathML
/// namespace as the default one; none when it is not MathML.
pub fn read_formula(text: &str) -> Result<Option<String>, OdfError> {
    let root = xml::parse(text)?;
    // LibreOffice writes `math:math`; MathML written by hand, `math`.
    if !root.name.is(MATHML, "math") {
        return Ok(None);
    }
    let mut out = String::new();
    if !write_element(&mut out, &root, 0, true) {
        return Ok(None);
    }
    Ok(Some(out))
}

/// Whether a name can be written as it is: a name of MathML, without a
/// prefix, which a colon would make of it, and not one that XML keeps
/// for itself, as `xmlns`.
fn plain_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("xml"))
}

/// Writes an element of MathML and what is in it, leaving out what is
/// not MathML; false when it is nested too deep to keep.
fn write_element(out: &mut String, e: &Element, depth: usize, root: bool) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    if e.name.ns != MATHML || !plain_name(&e.name.local) {
        return true;
    }
    out.push('<');
    out.push_str(&e.name.local);
    if root {
        out.push_str(" xmlns=\"");
        out.push_str(MATHML);
        out.push('"');
    }
    let mut written: Vec<&str> = Vec::new();
    for (name, value) in &e.attrs {
        // Attributes of MathML have no namespace; one written twice, with
        // a prefix and without, is written once.
        if (!name.ns.is_empty() && name.ns != MATHML)
            || !plain_name(&name.local)
            || written.contains(&name.local.as_str())
        {
            continue;
        }
        written.push(&name.local);
        out.push(' ');
        out.push_str(&name.local);
        out.push_str("=\"");
        xml::escape(out, value, true);
        out.push('"');
    }
    if e.children.is_empty() {
        out.push_str("/>");
        return true;
    }
    out.push('>');
    for child in &e.children {
        match child {
            Node::Text(text) => xml::escape(out, text, false),
            Node::Element(child) => {
                if !write_element(out, child, depth + 1, false) {
                    return false;
                }
            }
        }
    }
    out.push_str("</");
    out.push_str(&e.name.local);
    out.push('>');
    true
}

/// The `content.xml` of a formula object: the MathML, with its
/// declaration.
pub(crate) fn write_formula(mathml: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{mathml}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w104_mathml_is_kept_without_what_is_not_mathml() {
        let text = r#"<?xml version="1.0"?>
<math:math xmlns:math="http://www.w3.org/1998/Math/MathML" xmlns:x="urn:x"
  display="block"><math:semantics><math:mrow><math:mi>x</math:mi>
  <math:mo>&lt;</math:mo><x:evil/><math:mn x:a="1">2</math:mn></math:mrow>
  <math:annotation encoding="StarMath 5.0">x &lt; 2</math:annotation>
  </math:semantics></math:math>"#;
        let mathml = read_formula(text).unwrap().unwrap();
        assert!(
            mathml.starts_with(
                r#"<math xmlns="http://www.w3.org/1998/Math/MathML" display="block">"#
            )
        );
        assert!(mathml.contains("<mo>&lt;</mo>"));
        assert!(mathml.contains("<mn>2</mn>"));
        assert!(!mathml.contains("evil"));
        // Read again, it is the same.
        assert_eq!(read_formula(&mathml).unwrap().unwrap(), mathml);
        // Not MathML.
        assert_eq!(read_formula("<p/>").unwrap(), None);
    }

    #[test]
    fn w104_names_with_prefixes_inside_do_not_carry_other_namespaces() {
        // A colon in the local name of a MathML element or attribute
        // would write another prefix, or a declaration of one.
        let text = r##"<m:math xmlns:m="http://www.w3.org/1998/Math/MathML" display="block"
  m:display="inline"><m:m2:x:evil m:xmlns:m2="http://www.w3.org/1998/Math/MathML"
  m:m2:xmlns:x="urn:evil" m:xmlns="urn:evil" m:xml:base="urn:evil"/><m:mi m:href="#a">x</m:mi></m:math>"##;
        let mathml = read_formula(text).unwrap().unwrap();
        assert_eq!(
            mathml,
            r##"<math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><mi href="#a">x</mi></math>"##
        );
        assert_eq!(read_formula(&mathml).unwrap().unwrap(), mathml);
    }
}
