// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Equations on the pages, drawn and written as PDF (W104).

use std::process::Command;
use std::sync::Arc;

use desk_model::{
    Anchor, Document, Editor, Equation, Frame, Metadata, Object, ObjectKind, PageSetup, Wrap,
};
use desk_render::{DisplayList, Painter};
use desk_text::TextSystem;

fn text_of(pdf: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!("desk-render-equations-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.pdf");
    std::fs::write(&path, pdf).unwrap();
    let output = Command::new("pdftotext")
        .arg(&path)
        .arg("-")
        .output()
        .expect("pdftotext is needed for this test (poppler-utils)");
    String::from_utf8(output.stdout).unwrap()
}

const EQUATIONS: [&str; 4] = [
    "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
    "\\sum_{i=1}^{n} i^2 = \\frac{n(n+1)(2n+1)}{6}",
    "\\int_0^\\infty e^{-x^2}\\,dx = \\frac{\\sqrt{\\pi}}{2}",
    "A = \\left[ \\begin{matrix} a & b \\\\ c & d \\end{matrix} \\right], \\quad \
     f(x) = \\begin{cases} \\hat{x} & x > 0 \\\\ \\mathbb{R} & \\text{otherwise} \\end{cases}",
];

#[test]
fn w104_equations_are_drawn_in_their_frames() {
    let mut system = TextSystem::new();
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Equations", |edit| {
            edit.set_page_setup(PageSetup::a4())?;
            let mut top = 72.0;
            for latex in EQUATIONS {
                let mathml = desk_math::latex_to_mathml(latex, true).unwrap();
                let equation = Equation {
                    mathml: mathml.into(),
                    latex: Some(latex.into()),
                };
                let (w, h) = desk_layout::equation_size(edit.document(), &equation, &mut system);
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::Equation(Arc::new(equation)),
                    // Twice as large as it is laid out.
                    Frame::from_pt(72.0, top, f64::from(w) * 2.0, f64::from(h) * 2.0),
                );
                object.anchor = Anchor::Page(0);
                object.wrap = Wrap::Through;
                top += f64::from(h) * 2.0 + 24.0;
                edit.add_object(object)?;
            }
            Ok(())
        })
        .unwrap();
    let layout = desk_layout::paginate(editor.document(), &mut system);
    let page = &layout.pages[0];
    let laid: Vec<_> = page
        .objects
        .iter()
        .filter_map(|o| o.equation.clone())
        .collect();
    assert_eq!(laid.len(), EQUATIONS.len());
    let pages = DisplayList::pages(&layout);
    let pdf = Painter::new()
        .unwrap()
        .pdf_bytes(&pages, &Metadata::default())
        .unwrap();
    if let Ok(dir) = std::env::var("DESK_SNAPSHOTS") {
        let png = Painter::new()
            .unwrap()
            .thumbnail_png(&pages[0], 1600)
            .unwrap();
        std::fs::write(std::path::Path::new(&dir).join("equations.png"), png).unwrap();
    }
    let text = text_of(&pdf);
    for expected in ["otherwise", "∑", "∫", "π", "ℝ"] {
        assert!(text.contains(expected), "{expected} in {text}");
    }
}
