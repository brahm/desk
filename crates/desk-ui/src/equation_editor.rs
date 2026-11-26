// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The equation editor (W104, PXV-22): Insert Equation and Edit Equation
//! of Pages 15.4, where an equation is written in LaTeX or MathML with a
//! preview of it as it is written.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use desk_math::LatexError;
use desk_model::{Document, Equation};
use gtk::gio;

use crate::i18n::tr;

/// What is written, as an equation, or why it is not one.
fn equation_of(source: &str, latex: bool, display: bool) -> Result<Equation, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err(String::new());
    }
    if latex {
        let mathml = desk_math::latex_to_mathml(source, display).map_err(|e| latex_error(&e))?;
        return Ok(Equation {
            mathml: mathml.into(),
            latex: Some(source.into()),
        });
    }
    // MathML is kept as it reads, written again plainly.
    let root = desk_math::parse(source).ok_or_else(|| tr("This is not MathML."))?;
    Ok(Equation::from_mathml(&root.to_xml()))
}

fn latex_error(error: &LatexError) -> String {
    match error {
        LatexError::UnknownCommand(name) => tr("Unknown command \\{name}").replace("{name}", name),
        LatexError::UnknownEnvironment(name) => {
            tr("Unknown environment {name}").replace("{name}", name)
        }
        LatexError::Unbalanced => tr("A brace, \\left or \\begin is not closed."),
        LatexError::MissingArgument(name) => {
            tr("{name} is missing what it applies to.").replace("{name}", name)
        }
        LatexError::TooDeep => tr("The groups are nested too deep."),
        LatexError::DoubleScript(name) => {
            tr("{name} is written twice on the same base.").replace("{name}", name)
        }
    }
}

/// Opens the editor on an equation, or for a new one set as a display or
/// in the line; `done` gets the equation written.
pub fn open(
    parent: Option<&gtk::Window>,
    document: Document,
    editing: Option<Equation>,
    display: bool,
    done: impl Fn(Equation) + 'static,
) {
    let new = editing.is_none();
    let dialog = adw::AlertDialog::new(
        Some(&if new {
            tr("Insert Equation")
        } else {
            tr("Edit Equation")
        }),
        Some(&tr("Write the equation in LaTeX or MathML.")),
    );
    let format = adw::ToggleGroup::builder()
        .homogeneous(true)
        .halign(gtk::Align::Center)
        .build();
    format.add(adw::Toggle::builder().name("latex").label("LaTeX").build());
    format.add(
        adw::Toggle::builder()
            .name("mathml")
            .label("MathML")
            .build(),
    );
    let latex = editing.as_ref().is_none_or(|e| e.latex.is_some());
    format.set_active_name(Some(if latex { "latex" } else { "mathml" }));
    let buffer = gtk::TextBuffer::new(None);
    if let Some(equation) = &editing {
        buffer.set_text(equation.source());
    }
    let text = gtk::TextView::builder()
        .buffer(&buffer)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    text.update_property(&[gtk::accessible::Property::Label(&tr("Equation"))]);
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text)
        .min_content_height(96)
        .max_content_height(240)
        .propagate_natural_height(true)
        .css_classes(["card"])
        .build();
    let preview = gtk::DrawingArea::builder()
        .content_height(120)
        .hexpand(true)
        .css_classes(["card"])
        .build();
    preview.update_property(&[gtk::accessible::Property::Label(&tr("Preview"))]);
    let message = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .css_classes(["error", "caption"])
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.append(&format);
    content.append(&scrolled);
    content.append(&preview);
    content.append(&message);
    dialog.set_extra_child(Some(&content));
    dialog.add_response("cancel", &tr("_Cancel"));
    let accept = if new { tr("_Insert") } else { tr("_Update") };
    dialog.add_response("accept", &accept);
    dialog.set_response_appearance("accept", adw::ResponseAppearance::Suggested);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("accept"));

    // The equation written, and its layout for the preview.
    let written: Rc<RefCell<Option<Equation>>> = Rc::new(RefCell::new(None));
    let laid: Rc<RefCell<Option<desk_layout::EquationLayout>>> = Rc::new(RefCell::new(None));
    let painter = Rc::new(RefCell::new(desk_render::Painter::new().ok()));
    {
        let laid = laid.clone();
        preview.set_draw_func(move |area, cr, width, height| {
            // The page is white whatever the theme, as the equation is
            // black on it.
            cr.set_source_rgb(1.0, 1.0, 1.0);
            let _ = cr.paint();
            let Some(laid) = laid.borrow().clone() else {
                return;
            };
            let list = desk_render::DisplayList::of_equation(&laid);
            let (w, h) = (f64::from(width) - 16.0, f64::from(height) - 16.0);
            let scale = (w / f64::from(list.width))
                .min(h / f64::from(list.height))
                .min(2.0 * f64::from(area.scale_factor()).max(1.0))
                .max(0.0);
            let _ = cr.save();
            cr.translate(
                (f64::from(width) - f64::from(list.width) * scale) / 2.0,
                (f64::from(height) - f64::from(list.height) * scale) / 2.0,
            );
            cr.scale(scale, scale);
            if let Some(painter) = painter.borrow_mut().as_mut() {
                let _ = painter.paint(cr, &list);
            }
            let _ = cr.restore();
        });
    }
    let update = {
        let (written, laid, buffer, format, preview, message, dialog) = (
            written.clone(),
            laid.clone(),
            buffer.clone(),
            format.clone(),
            preview.clone(),
            message.clone(),
            dialog.downgrade(),
        );
        move || {
            let source = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            let latex = format.active_name().as_deref() == Some("latex");
            let result = equation_of(&source, latex, display);
            match &result {
                Ok(equation) => {
                    message.set_label("");
                    *laid.borrow_mut() = Some(crate::with_text_system(|system| {
                        desk_layout::lay_out_equation(&document, equation, system)
                    }));
                }
                Err(why) => {
                    message.set_label(why);
                    // The last equation that was one stays in sight.
                }
            }
            message.set_visible(!message.label().is_empty());
            if let Some(dialog) = dialog.upgrade() {
                dialog.set_response_enabled("accept", result.is_ok());
            }
            *written.borrow_mut() = result.ok();
            preview.queue_draw();
        }
    };
    let update = Rc::new(update);
    update();
    {
        let update = update.clone();
        buffer.connect_changed(move |_| update());
    }
    {
        let update = update.clone();
        format.connect_active_name_notify(move |_| update());
    }
    dialog.choose(parent, gio::Cancellable::NONE, move |response| {
        if response == "accept"
            && let Some(equation) = written.borrow_mut().take()
        {
            done(equation);
        }
    });
    text.grab_focus();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w104_what_is_written_is_an_equation_or_says_why_not() {
        let equation = equation_of("x^2", true, false).unwrap();
        assert_eq!(equation.latex.as_deref(), Some("x^2"));
        assert!(equation.mathml.contains("<msup>"));
        let equation = equation_of("<math><mi>y</mi></math>", false, false).unwrap();
        assert_eq!(equation.latex, None);
        assert!(equation.mathml.starts_with("<math xmlns="));
        assert_eq!(equation_of("  ", true, false).unwrap_err(), "");
        assert!(
            equation_of("\\foo", true, false)
                .unwrap_err()
                .contains("\\foo")
        );
        assert!(equation_of("<svg/>", false, false).is_err());
    }
}
