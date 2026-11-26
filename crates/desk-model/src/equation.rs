// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Equations (W104, PXV-22).
//!
//! As in Pages 15.4, an equation is written in LaTeX or in MathML and
//! sits in the line of the text, or floats on the page of a page layout
//! document. Desk keeps it as MathML, which ODF stores and other programs
//! read, with the LaTeX it was written in when it was written in LaTeX,
//! to edit it again as written.

use std::sync::Arc;

/// An equation: its MathML, and the LaTeX it came from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Equation {
    /// The MathML of the equation: a `math` element of the MathML
    /// namespace, as text.
    pub mathml: Arc<str>,
    /// The LaTeX that it was written in, when it was.
    pub latex: Option<Arc<str>>,
}

impl Equation {
    /// An equation of MathML.
    pub fn from_mathml(mathml: &str) -> Self {
        Self {
            mathml: mathml.into(),
            latex: None,
        }
    }

    /// What the editor shows to change it: the LaTeX it was written in,
    /// or its MathML.
    pub fn source(&self) -> &str {
        self.latex.as_deref().unwrap_or(&self.mathml)
    }
}
