// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Equations typed in LaTeX or MathML, or read from a file (W104): any
//! text must turn into MathML or an error, and lay out, without a panic;
//! the MathML that LaTeX turns into must read back.

#![no_main]

use libfuzzer_sys::fuzz_target;

fn measure(text: &str, size: f32, _: bool, _: bool) -> f32 {
    text.chars().count() as f32 * size * 0.5
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let drawing = desk_math::draw(&text, 12.0, &measure);
    assert!(drawing.width.is_finite() && drawing.height.is_finite());
    for display in [false, true] {
        if let Ok(mathml) = desk_math::latex_to_mathml(&text, display) {
            assert!(desk_math::parse(&mathml).is_some(), "{mathml}");
            let drawing = desk_math::draw(&mathml, 12.0, &measure);
            assert!(drawing.width.is_finite() && drawing.height.is_finite());
        }
    }
});
