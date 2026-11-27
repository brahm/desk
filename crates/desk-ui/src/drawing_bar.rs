// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bar of drawing of Desk Writer (W117, PXV-26), under the header bar
//! as window M+ puts the bars of iWork (D-008), while a drawing is drawn:
//! the tools of Pages 15.4, a pen, a pencil, a crayon and the fill, with
//! the eraser; the colour and the width of the next line; and Done, which
//! turns drawing off. Pages shows its tools along the bottom of an iPad;
//! on GNOME the pointer or a stylus draws (D-005).

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{DrawingTool, Rgba};
use gtk::gdk;

use crate::editing::{DRAWING_WIDTHS, DrawingPick, DrawingTools, TextEditor};
use crate::formatting::EditorOf;
use crate::i18n::tr;

/// The bar.
pub struct DrawingBar {
    revealer: gtk::Revealer,
    picks: Vec<(DrawingPick, gtk::ToggleButton)>,
    color: gtk::ColorDialogButton,
    width: gtk::SpinButton,
    editor: EditorOf,
    updating: Cell<bool>,
}

/// The name of what a press does, as the bar shows it.
fn pick_name(pick: DrawingPick) -> String {
    match pick {
        DrawingPick::Tool(DrawingTool::Pen) => tr("_Pen"),
        DrawingPick::Tool(DrawingTool::Pencil) => tr("Pen_cil"),
        DrawingPick::Tool(DrawingTool::Crayon) => tr("C_rayon"),
        DrawingPick::Tool(DrawingTool::Fill) => tr("_Fill"),
        DrawingPick::Eraser => tr("_Eraser"),
    }
}

fn to_gdk(color: Rgba) -> gdk::RGBA {
    gdk::RGBA::new(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        f32::from(color.a) / 255.0,
    )
}

fn of_gdk(color: &gdk::RGBA) -> Rgba {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgba {
        r: byte(color.red()),
        g: byte(color.green()),
        b: byte(color.blue()),
        a: byte(color.alpha()),
    }
}

impl DrawingBar {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let bar = gtk::Box::builder()
            .spacing(6)
            .margin_start(6)
            .margin_end(6)
            .margin_top(3)
            .margin_bottom(3)
            .build();
        bar.add_css_class("toolbar");
        let tools = gtk::Box::builder().build();
        tools.add_css_class("linked");
        let mut picks = Vec::new();
        let mut first: Option<gtk::ToggleButton> = None;
        for pick in DrawingTool::ALL
            .into_iter()
            .map(DrawingPick::Tool)
            .chain([DrawingPick::Eraser])
        {
            let button = gtk::ToggleButton::builder()
                .label(pick_name(pick))
                .use_underline(true)
                .build();
            if let Some(first) = &first {
                button.set_group(Some(first));
            } else {
                first = Some(button.clone());
            }
            tools.append(&button);
            picks.push((pick, button));
        }
        crate::formatting::labelled(&tools, &tr("Drawing Tools"));
        bar.append(&tools);
        let color = gtk::ColorDialogButton::builder()
            .dialog(&gtk::ColorDialog::builder().with_alpha(true).build())
            .tooltip_text(tr("Color"))
            .build();
        crate::formatting::labelled(&color, &tr("Color"));
        bar.append(&color);
        let width_label = gtk::Label::builder()
            .label(tr("_Width"))
            .use_underline(true)
            .build();
        let width =
            gtk::SpinButton::with_range(*DRAWING_WIDTHS.start(), *DRAWING_WIDTHS.end(), 0.5);
        width.set_digits(1);
        width_label.set_mnemonic_widget(Some(&width));
        crate::formatting::labelled(&width, &tr("Width"));
        bar.append(&width_label);
        bar.append(&width);
        let hint = gtk::Label::builder()
            .label(tr("Draw with the pointer or a stylus"))
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        hint.add_css_class("dim-label");
        bar.append(&hint);
        let done = gtk::Button::builder()
            .label(tr("_Done"))
            .use_underline(true)
            .action_name("win.finish-drawing")
            .build();
        done.add_css_class("suggested-action");
        bar.append(&done);
        let revealer = gtk::Revealer::builder()
            .child(&bar)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();
        let this = Rc::new(Self {
            revealer,
            picks,
            color,
            width,
            editor,
            updating: Cell::new(false),
        });
        for (pick, button) in &this.picks {
            let weak = Rc::downgrade(&this);
            let pick = *pick;
            button.connect_toggled(move |button| {
                if !button.is_active() {
                    return;
                }
                if let Some(this) = weak.upgrade() {
                    this.change(|tools| tools.pick = pick);
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.color.connect_rgba_notify(move |button| {
                if let Some(this) = weak.upgrade() {
                    let color = of_gdk(&button.rgba());
                    this.change(|tools| tools.color = color);
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.width.connect_value_changed(move |spin| {
                if let Some(this) = weak.upgrade() {
                    let width = spin.value();
                    this.change(|tools| tools.width = width);
                }
            });
        }
        this
    }

    /// The bar, to put under the header bar.
    pub fn widget(&self) -> &gtk::Revealer {
        &self.revealer
    }

    /// Changes the tools of the editor as `change` does.
    fn change(&self, change: impl FnOnce(&mut DrawingTools)) {
        if self.updating.get() {
            return;
        }
        if let Some(editor) = (self.editor)() {
            let mut tools = editor.drawing_tools();
            change(&mut tools);
            editor.set_drawing_tools(tools);
        }
    }

    /// Shows the bar while a drawing is drawn, with its tools.
    pub fn show(&self, editor: Option<&TextEditor>) {
        let Some(editor) = editor.filter(|e| e.drawing_on()) else {
            self.revealer.set_reveal_child(false);
            return;
        };
        self.revealer.set_reveal_child(true);
        let tools = editor.drawing_tools();
        self.updating.set(true);
        for (pick, button) in &self.picks {
            if *pick == tools.pick && !button.is_active() {
                button.set_active(true);
            }
        }
        if of_gdk(&self.color.rgba()) != tools.color {
            self.color.set_rgba(&to_gdk(tools.color));
        }
        if (self.width.value() - tools.width).abs() > f64::EPSILON {
            self.width.set_value(tools.width);
        }
        self.updating.set(false);
    }
}
