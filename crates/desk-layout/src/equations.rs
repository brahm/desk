// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Equations laid out with the fonts of the document (W104, PXV-22).
//!
//! `desk-math` lays out the MathML of an equation; its text is measured
//! and set here in a serif font, as the equations of Pages are, at a size
//! that the frame of the equation then scales.

use desk_model::{Alignment, CharAttrs, Document, Equation, FontWeight, Rgba};
use desk_text::TextSystem;

use crate::PlacedLine;
use crate::tables::{ChartLabel, lines_width, shift, text_lines};

/// The font of equations.
pub const EQUATION_FONT: &str = "Liberation Serif";

/// The size that an equation is laid out at, before its frame scales it.
pub const EQUATION_SIZE: f32 = 12.0;

/// An equation laid out at [`EQUATION_SIZE`], from the top left corner of
/// its box: its marks, with the text of its text marks as lines.
#[derive(Clone, Debug, Default)]
pub struct EquationLayout {
    /// What the equation draws; its text marks are also in `labels`.
    pub drawing: desk_math::Drawing,
    /// The text, laid out, in the order of the text marks.
    pub labels: Vec<ChartLabel>,
}

impl EquationLayout {
    /// How a frame of a size scales the equation and where it puts it:
    /// the scale, and the left and top of the equation in the frame.
    pub fn fit(&self, width: f32, height: f32) -> (f32, f32, f32) {
        let (w, h) = (self.drawing.width, self.drawing.height);
        if w <= 0.0 || h <= 0.0 {
            return (1.0, 0.0, 0.0);
        }
        let scale = (width / w).min(height / h).max(0.0);
        (scale, (width - w * scale) / 2.0, (height - h * scale) / 2.0)
    }
}

fn attrs(size: f32, italic: bool, bold: bool, color: [u8; 3]) -> CharAttrs {
    CharAttrs {
        font_family: Some(EQUATION_FONT.into()),
        font_size: Some(desk_model::Length::from_pt(f64::from(size))),
        weight: Some(if bold {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        }),
        italic: Some(italic),
        color: Some(Rgba::rgb(color[0], color[1], color[2])),
        ..CharAttrs::default()
    }
}

/// Lays out an equation, measuring its text with the fonts of the
/// document.
pub fn lay_out_equation(
    document: &Document,
    equation: &Equation,
    system: &mut TextSystem,
) -> EquationLayout {
    let cell = std::cell::RefCell::new(&mut *system);
    let measure = |text: &str, size: f32, italic: bool, bold: bool| -> f32 {
        let lines = text_lines(
            document,
            text,
            &attrs(size, italic, bold, [0, 0, 0]),
            Alignment::Left,
            100_000.0,
            &mut cell.borrow_mut(),
        );
        lines_width(&lines)
    };
    let drawing = desk_math::draw(&equation.mathml, EQUATION_SIZE, &measure);
    let system = cell.into_inner();
    let labels = drawing
        .marks
        .iter()
        .filter_map(|mark| {
            let desk_math::Mark::Text {
                x,
                y,
                text,
                size,
                italic,
                bold,
                color,
            } = mark
            else {
                return None;
            };
            let mut lines = text_lines(
                document,
                text,
                &attrs(*size, *italic, *bold, *color),
                Alignment::Left,
                100_000.0,
                system,
            );
            let baseline = lines.first().map_or(0.0, PlacedLine::baseline);
            shift(&mut lines, 0.0, -baseline);
            Some(ChartLabel {
                lines,
                at: (*x, *y),
                angle: 0.0,
            })
        })
        .collect();
    EquationLayout { drawing, labels }
}

/// The size of an equation as it is laid out, which a new frame for it
/// takes.
pub fn equation_size(
    document: &Document,
    equation: &Equation,
    system: &mut TextSystem,
) -> (f32, f32) {
    let laid = lay_out_equation(document, equation, system);
    (laid.drawing.width, laid.drawing.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w104_an_equation_lays_out_its_text_in_the_fonts() {
        let document = Document::new_word_processing();
        let mut system = TextSystem::new();
        let mathml = desk_math::latex_to_mathml("\\frac{a^2}{b} + \\sqrt{x}", true).unwrap();
        let equation = Equation::from_mathml(&mathml);
        let laid = lay_out_equation(&document, &equation, &mut system);
        assert_eq!(laid.labels.len(), 5);
        assert!(laid.labels.iter().all(|l| !l.lines.is_empty()));
        let (w, h) = equation_size(&document, &equation, &mut system);
        assert!(w > 20.0 && h > 20.0, "{w} × {h}");
        // A frame twice as wide and as high scales it twice, and a wider
        // one centres it.
        let (scale, dx, dy) = laid.fit(w * 2.0, h * 2.0);
        assert!((scale - 2.0).abs() < 1e-4 && dx.abs() < 1e-3 && dy.abs() < 1e-3);
        let (scale, dx, _) = laid.fit(w * 3.0, h);
        assert!((scale - 1.0).abs() < 1e-4 && (dx - w).abs() < 1e-3);
    }

    #[test]
    fn w104_mathml_that_cannot_be_read_draws_nothing() {
        let document = Document::new_word_processing();
        let laid = lay_out_equation(
            &document,
            &Equation::from_mathml("<math"),
            &mut TextSystem::new(),
        );
        assert!(laid.labels.is_empty());
        assert_eq!(laid.fit(10.0, 10.0), (1.0, 0.0, 0.0));
    }
}
