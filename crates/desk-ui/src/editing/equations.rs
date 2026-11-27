// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Equations in the window (W104, PXV-22): one inserted in the line of
//! the text, at the size of its text, or on the page of a page layout
//! document, and the selected one edited again.

use std::sync::Arc;

use desk_model::{Anchor, Equation, Frame, Length, Object, ObjectKind, Story, Wrap};

use gtk::prelude::*;

use super::TextEditor;

/// An equation in a color, unless it has one, or the color is black or
/// not opaque, which MathML does not say.
fn colored(equation: Equation, color: desk_model::Rgba) -> Equation {
    if (color.r, color.g, color.b) == (0, 0, 0) || color.a != 255 {
        return equation;
    }
    with_color(
        equation,
        &format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b),
    )
}

/// The color of an equation as a whole, as its MathML says it.
fn color_of(equation: &Equation) -> Option<String> {
    desk_math::parse(&equation.mathml)?
        .get("mathcolor")
        .map(str::to_owned)
}

/// An equation in a color of MathML, unless it has one.
fn with_color(equation: Equation, color: &str) -> Equation {
    let Some(root) = desk_math::parse(&equation.mathml) else {
        return equation;
    };
    if root.get("mathcolor").is_some() {
        return equation;
    }
    let root = root.attr("mathcolor", color);
    Equation {
        mathml: root.to_xml().into(),
        ..equation
    }
}

impl TextEditor {
    /// The selected equation, when one object is selected and it is one.
    pub fn selected_equation(&self) -> Option<Arc<Equation>> {
        let state = self.state.borrow();
        let [id] = state.objects[..] else {
            return None;
        };
        match &Self::main_document(&state).objects().get(id)?.kind {
            ObjectKind::Equation(equation) => Some(equation.clone()),
            _ => None,
        }
    }

    /// Whether a new equation goes in the line of the text, as in Pages
    /// 15.4 when the cursor is in the text of a word processing document;
    /// on the page otherwise.
    pub fn equation_goes_in_line(&self) -> bool {
        // With an object selected, it goes on the page beside it.
        if self.has_selected_objects() {
            return false;
        }
        match self.story() {
            // A header, a footer or a note takes it in its text, as it
            // takes a picture.
            Some(Story::Margin { .. } | Story::Note(_)) => true,
            Some(Story::TextBox(_) | Story::Caption { .. }) => false,
            None => !self.is_page_layout(),
        }
    }

    /// The size of an equation as it is laid out, in points.
    fn equation_size(&self, equation: &Equation) -> (f64, f64) {
        let document = self.document();
        let (w, h) = crate::with_text_system(|system| {
            desk_layout::equation_size(&document, equation, system)
        });
        (f64::from(w), f64::from(h))
    }

    /// The size of a new equation: as large as the text at the cursor.
    fn new_equation_size(&self, equation: &Equation) -> (f64, f64) {
        let (w, h) = self.equation_size(equation);
        let text = self.format().character.font_size.to_pt();
        let scale = if text > 0.0 {
            text / f64::from(desk_layout::EQUATION_SIZE)
        } else {
            1.0
        };
        ((w * scale).max(1.0), (h * scale).max(1.0))
    }

    /// Inserts an equation: in the line of the text, in place of the
    /// selection, or in the middle of the page shown; and selects it
    /// there.
    pub fn insert_equation(&self, equation: Equation) {
        let (w, h) = self.new_equation_size(&equation);
        if self.equation_goes_in_line() {
            // In the line, it takes the color of the text too, and is no
            // wider than the text, nor taller than two inches in a header,
            // a footer or a note, as a picture.
            let equation = colored(equation, self.format().character.color);
            let (width, height) = {
                let state = self.state.borrow();
                let setup = state.layout.setup;
                (setup.body_width().to_pt(), setup.body_height().to_pt())
            };
            let tallest = if matches!(self.story(), Some(Story::Margin { .. } | Story::Note(_))) {
                144.0
            } else {
                height
            };
            let fit = (width.max(1.0) / w).min(tallest.max(1.0) / h).min(1.0);
            let (w, h) = (w * fit, h * fit);
            let before = self.selection();
            let (start, end) = before.ordered();
            let result = self
                .state
                .borrow_mut()
                .editor
                .edit("Insert Equation", |edit| {
                    let from = super::at_spot(edit.document(), start)?;
                    let to = super::at_spot(edit.document(), end)?;
                    let at = if start == end {
                        from
                    } else {
                        edit.delete(from, to)?
                    };
                    let object = Object::new(
                        edit.new_id(),
                        ObjectKind::Equation(Arc::new(equation)),
                        Frame::from_pt(0.0, 0.0, w, h),
                    );
                    let after = edit.insert_object_in_text(at, object)?;
                    Ok(crate::text_nav::Spot::of(edit.document(), after))
                });
            self.finish_edit(before, result);
            return;
        }
        let (page, frame) = self.placed_frame(|_| (w, h), None);
        self.leave_story(None);
        let selection = self.selection();
        let result = self
            .state
            .borrow_mut()
            .editor
            .edit("Insert Equation", |edit| {
                let id = edit.new_id();
                let mut object = Object::new(id, ObjectKind::Equation(Arc::new(equation)), frame);
                object.anchor = Anchor::Page(page as u32);
                object.wrap = Wrap::AboveAndBelow;
                edit.add_object(object)?;
                Ok(id)
            });
        match result {
            Ok(id) => self.select_new_object(selection, id),
            Err(error) => gtk::glib::g_warning!("desk", "cannot insert an equation: {error}"),
        }
    }

    /// Changes the selected equation, whose frame keeps the scale it had:
    /// a larger equation takes a larger frame.
    pub fn change_equation(&self, equation: Equation) {
        let Some(old) = self.selected_equation() else {
            return;
        };
        // An equation in a color keeps it, as written again without one.
        let equation = match color_of(&old) {
            Some(color) => with_color(equation, &color),
            None => equation,
        };
        let (old_w, old_h) = self.equation_size(&old);
        let (new_w, new_h) = self.equation_size(&equation);
        let Some(mut object) = self.object_info().map(|info| info.object) else {
            return;
        };
        if object.locked {
            return;
        }
        let frame = object.frame;
        let scale = if old_h > 0.0 && old_w > 0.0 {
            (frame.width.to_pt() / old_w).min(frame.height.to_pt() / old_h)
        } else {
            1.0
        };
        object.frame.width = Length::from_pt((new_w * scale).max(1.0));
        object.frame.height = Length::from_pt((new_h * scale).max(1.0));
        object.kind = ObjectKind::Equation(Arc::new(equation));
        self.arrange_objects("Edit Equation", move |edit, _| {
            edit.replace_object(object)?;
            Ok(None)
        });
    }

    /// Opens the equation editor for a new equation: Insert > Equation.
    pub fn insert_equation_dialog(&self) {
        self.open_equation_editor(None);
    }

    /// Opens the equation editor on the selected equation: Edit Equation.
    /// Nothing happens when no equation alone is selected, or it is
    /// locked.
    pub fn edit_equation_dialog(&self) {
        let Some(equation) = self.selected_equation() else {
            return;
        };
        if self.object_info().is_some_and(|info| info.object.locked) {
            gtk::gdk::Display::default().inspect(gtk::gdk::Display::beep);
            return;
        }
        self.open_equation_editor(Some(equation));
    }

    fn open_equation_editor(&self, editing: Option<Arc<Equation>>) {
        let parent = self.view.area().root().and_downcast::<gtk::Window>();
        // An equation in the line of the text is set as text, one on its
        // own as a display; one edited keeps how it was set.
        let display = editing.as_ref().map_or(!self.equation_goes_in_line(), |e| {
            desk_math::parse(&e.mathml).is_some_and(|root| root.get("display") == Some("block"))
        });
        let weak = self.downgrade();
        let document = self.document();
        crate::equation_editor::open(
            parent.as_ref(),
            document,
            editing.as_deref().cloned(),
            display,
            move |equation| {
                let Some(this) = weak.upgrade() else { return };
                if editing.is_some() {
                    this.change_equation(equation);
                } else {
                    this.insert_equation(equation);
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use desk_model::{Anchor, Document, Equation, ObjectKind};

    use super::TextEditor;

    fn equation(latex: &str) -> Equation {
        Equation {
            mathml: desk_math::latex_to_mathml(latex, false).unwrap().into(),
            latex: Some(latex.into()),
        }
    }

    #[test]
    fn w104_an_equation_goes_in_the_line_of_the_text_and_is_edited_again() {
        crate::with_gtk(|| {
            let view = crate::view::DocumentView::new();
            let editor = TextEditor::new(&view, Document::new_word_processing());
            assert!(editor.equation_goes_in_line());
            editor.insert_equation(equation("x^2"));
            let document = editor.document();
            let object = document
                .objects()
                .iter()
                .find(|o| matches!(o.kind, ObjectKind::Equation(_)))
                .expect("the equation is in the document")
                .clone();
            assert_eq!(object.anchor, Anchor::InText);
            let (w, h) = (object.frame.width.to_pt(), object.frame.height.to_pt());
            assert!(w > 5.0 && h > 5.0, "{w} × {h}");
            // Edited again, a wider equation takes a wider frame of the
            // same scale.
            editor.select_objects(vec![object.id]);
            assert_eq!(
                editor.selected_equation().unwrap().latex.as_deref(),
                Some("x^2")
            );
            editor.change_equation(equation("x^2 + y^2 + z^2"));
            let changed = editor.document().objects().get(object.id).unwrap().clone();
            assert!(changed.frame.width.to_pt() > w * 2.0);
            let ObjectKind::Equation(changed) = &changed.kind else {
                panic!("still an equation");
            };
            assert_eq!(changed.latex.as_deref(), Some("x^2 + y^2 + z^2"));
        });
    }

    #[test]
    fn w104_an_equation_in_the_line_takes_the_color_of_the_text() {
        let red = super::colored(equation("x"), desk_model::Rgba::rgb(0xcc, 0, 0x10));
        assert!(
            red.mathml.contains("mathcolor=\"#cc0010\""),
            "{}",
            red.mathml
        );
        assert_eq!(red.latex.as_deref(), Some("x"));
        assert_eq!(super::color_of(&red).as_deref(), Some("#cc0010"));
        let faint = desk_model::Rgba {
            a: 64,
            ..desk_model::Rgba::rgb(0xcc, 0, 0)
        };
        assert!(
            !super::colored(equation("x"), faint)
                .mathml
                .contains("mathcolor")
        );
        let black = super::colored(equation("x"), desk_model::Rgba::BLACK);
        assert!(!black.mathml.contains("mathcolor"));
    }

    #[test]
    fn w104_an_equation_floats_on_the_page_of_a_page_layout_document() {
        crate::with_gtk(|| {
            let view = crate::view::DocumentView::new();
            let editor = TextEditor::new(&view, Document::new_page_layout());
            assert!(!editor.equation_goes_in_line());
            editor.insert_equation(equation("\\frac{1}{2}"));
            let [id] = editor.selected_objects()[..] else {
                panic!("the equation is selected");
            };
            let object = editor.document().objects().get(id).unwrap().clone();
            assert_eq!(object.anchor, Anchor::Page(0));
            assert!(matches!(object.kind, ObjectKind::Equation(_)));
        });
    }
}
