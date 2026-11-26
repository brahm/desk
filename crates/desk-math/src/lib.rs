// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Equations (W104, PXV-22): LaTeX turned into MathML, and MathML laid
//! out as lines of text, rules and strokes.
//!
//! As in Pages 15.4, an equation is written in LaTeX or in MathML. Desk
//! keeps it as MathML, which this crate lays out with the fonts that the
//! caller measures, as `desk-chart` does for charts; the display list of
//! `desk-render` draws what it gives. The crate depends on nothing else.

mod latex;
mod layout;
mod xml;

pub use latex::{LatexError, latex_to_mathml};
pub use layout::{Drawing, Mark, Measure, draw};
pub use xml::{MATHML, Node, parse};
