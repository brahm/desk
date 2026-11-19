// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Drop Cap Styles of the Drop Cap group, as in Pages 15.4 (PXV-06): a
//! button that opens a picture of each style of the document, which
//! applies it, and a button that saves the drop cap of the paragraph at
//! the cursor as a new style. A secondary click on a style offers to
//! apply it, to redefine it from the selection and to delete it.

use std::rc::Rc;

use adw::prelude::*;
use desk_model::{BorderStyle, DropCapStyle, Rgba};
use gtk::{cairo, gio, glib};

use crate::formatting::{EditorOf, labelled};
use crate::i18n::tr;

/// The width and the height of a picture of a style, in pixels.
const PREVIEW: (i32, i32) = (64, 48);

/// Draws a style as Pages shows it: an A as tall as its lines, gray lines
/// of text beside it and below it.
fn draw_preview(cr: &cairo::Context, width: f64, height: f64, style: &DropCapStyle) {
    let rgba = |c: Rgba| {
        cr.set_source_rgba(
            f64::from(c.r) / 255.0,
            f64::from(c.g) / 255.0,
            f64::from(c.b) / 255.0,
            f64::from(c.a) / 255.0,
        );
    };
    let cap = &style.cap;
    // The lines of text, from the top, five of them.
    let (left, right, top) = (8.0, width - 6.0, 8.0);
    let pitch = (height - 2.0 * top) / 5.0;
    let lines = f64::from(cap.lines.clamp(2, 5));
    let raised = f64::from(cap.raised.min(cap.lines - 1));
    let tall = (lines - 1.0) * pitch + pitch * 0.7;
    let size = tall / 0.72;
    let text_top = top + raised * pitch;
    cr.select_font_face(
        "Sans",
        cairo::FontSlant::Normal,
        if style.weight.is_bold() {
            cairo::FontWeight::Bold
        } else {
            cairo::FontWeight::Normal
        },
    );
    let scale = cap
        .background
        .map_or(1.0, |s| f64::from(s.scale.clamp(10, 100)) / 100.0);
    cr.set_font_size(size * scale);
    let advance = cr.text_extents("A").map_or(size * 0.6, |e| e.x_advance());
    let box_width = match cap.background {
        Some(_) => tall.max(advance + tall * (1.0 - scale)),
        None => advance,
    };
    let x = left - box_width * f64::from(cap.outdent.min(200)) / 100.0 / 2.0;
    let baseline = top + (lines - 1.0) * pitch + pitch * 0.7;
    if let Some(shape) = cap.background {
        let r = f64::from(shape.radius.min(100)) / 100.0 * tall.min(box_width) / 2.0;
        let (bx, by) = (x, baseline - tall);
        cr.new_sub_path();
        cr.arc(bx + box_width - r, by + r, r, -90f64.to_radians(), 0.0);
        cr.arc(
            bx + box_width - r,
            by + tall - r,
            r,
            0.0,
            90f64.to_radians(),
        );
        cr.arc(
            bx + r,
            by + tall - r,
            r,
            90f64.to_radians(),
            180f64.to_radians(),
        );
        cr.arc(bx + r, by + r, r, 180f64.to_radians(), 270f64.to_radians());
        cr.close_path();
        rgba(shape.fill);
        let _ = cr.fill_preserve();
        let border = shape.border_width.to_pt();
        if border > 0.0 {
            rgba(shape.border_color);
            cr.set_line_width(border.clamp(0.75, 2.0));
            match shape.border_style {
                BorderStyle::Dashed => cr.set_dash(&[3.0, 2.0], 0.0),
                BorderStyle::Dotted => cr.set_dash(&[1.0, 1.5], 0.0),
                BorderStyle::Solid => cr.set_dash(&[], 0.0),
            }
            let _ = cr.stroke();
        } else {
            cr.new_path();
        }
    }
    rgba(style.color);
    let margin = tall * (1.0 - scale) / 2.0;
    cr.move_to(x + (box_width - advance) / 2.0, baseline - margin);
    let _ = cr.show_text("A");
    // The text: beside the letter on its lines, then from the margin.
    cr.set_source_rgba(0.6, 0.6, 0.6, 1.0);
    let beside = (x + box_width + 3.0).max(left);
    for i in 0..5 {
        let y = text_top + f64::from(i) * pitch + pitch * 0.3;
        if y + pitch * 0.4 > height - 2.0 {
            break;
        }
        let start = if f64::from(i) < lines - raised {
            beside
        } else {
            left
        };
        cr.rectangle(start, y, right - start, pitch * 0.4);
    }
    let _ = cr.fill();
}

/// The Drop Cap Styles button.
pub struct StylesButton {
    button: gtk::MenuButton,
    flow: gtk::FlowBox,
    new: gtk::Button,
    editor: EditorOf,
}

impl StylesButton {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(3)
            .max_children_per_line(3)
            .homogeneous(true)
            .column_spacing(6)
            .row_spacing(6)
            .build();
        labelled(&flow, &tr("Drop Cap Styles"));
        let new = gtk::Button::builder()
            .label(tr("Save as New Style"))
            .css_classes(["flat"])
            .action_name("drop-cap.new")
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(
            &gtk::Label::builder()
                .label(tr("Drop Cap Styles"))
                .css_classes(["heading"])
                .build(),
        );
        let scroll = gtk::ScrolledWindow::builder()
            .child(&flow)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(320)
            .build();
        content.append(&scroll);
        content.append(&new);
        let popover = gtk::Popover::builder().child(&content).build();
        let button = gtk::MenuButton::builder()
            .icon_name("view-grid-symbolic")
            .popover(&popover)
            .tooltip_text(tr("Drop Cap Styles"))
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        labelled(&button, &tr("Drop Cap Styles"));
        let this = Rc::new(Self {
            button,
            flow,
            new,
            editor,
        });
        let weak = Rc::downgrade(&this);
        popover.connect_show(move |_| {
            if let Some(this) = weak.upgrade() {
                this.fill();
            }
        });
        this.button
            .insert_action_group("drop-cap", Some(&this.actions()));
        this
    }

    /// The button, for the header of the group.
    pub fn widget(&self) -> &gtk::MenuButton {
        &self.button
    }

    /// The actions of the popover: apply, redefine and delete a style by
    /// its place, and save a new one.
    fn actions(self: &Rc<Self>) -> gio::SimpleActionGroup {
        let group = gio::SimpleActionGroup::new();
        let index = glib::VariantTy::new("u").expect("a variant type");
        let add = |name: &str, act: fn(&Self, usize)| {
            let action = gio::SimpleAction::new(name, Some(index));
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, value| {
                if let (Some(this), Some(i)) = (weak.upgrade(), value.and_then(u32::from_variant)) {
                    act(&this, i as usize);
                }
            });
            group.add_action(&action);
        };
        add("apply", Self::apply);
        add("redefine", Self::redefine);
        add("delete", Self::delete);
        let new = gio::SimpleAction::new("new", None);
        let weak = Rc::downgrade(self);
        new.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.save_new();
            }
        });
        group.add_action(&new);
        group
    }

    fn apply(&self, index: usize) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        if let Some(style) = editor.drop_cap_styles().get(index).copied() {
            editor.apply_drop_cap_style(style);
        }
        self.button.popdown();
    }

    fn redefine(&self, index: usize) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let Some(current) = editor.drop_cap_style() else {
            return;
        };
        let mut styles = editor.drop_cap_styles();
        if let Some(style) = styles.get_mut(index) {
            *style = current;
            editor.set_drop_cap_styles("Redefine Style", styles);
            self.fill();
        }
    }

    fn delete(&self, index: usize) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let mut styles = editor.drop_cap_styles();
        if index < styles.len() {
            styles.remove(index);
            editor.set_drop_cap_styles("Delete Style", styles);
            self.fill();
        }
    }

    fn save_new(&self) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let Some(current) = editor.drop_cap_style() else {
            return;
        };
        let mut styles = editor.drop_cap_styles();
        styles.push(current);
        editor.set_drop_cap_styles("New Style", styles);
        self.fill();
    }

    /// Shows the styles of the document, and whether the paragraph at the
    /// cursor has a drop cap to save.
    fn fill(&self) {
        self.flow.remove_all();
        let Some(editor) = (self.editor)() else {
            return;
        };
        let current = editor.drop_cap_style();
        self.new.set_sensitive(current.is_some());
        for (index, style) in editor.drop_cap_styles().into_iter().enumerate() {
            let area = gtk::DrawingArea::builder()
                .content_width(PREVIEW.0)
                .content_height(PREVIEW.1)
                .build();
            area.set_draw_func(move |_, cr, width, height| {
                // A white page under the picture, in a dark theme too.
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.paint().ok();
                draw_preview(cr, f64::from(width), f64::from(height), &style);
            });
            let name = format!("{} {}", tr("Drop Cap Style"), index + 1);
            let button = gtk::ToggleButton::builder()
                .child(&area)
                .tooltip_text(&name)
                .css_classes(["flat"])
                .active(current == Some(style))
                .build();
            labelled(&button, &name);
            let target = (index as u32).to_variant();
            button.set_action_name(Some("drop-cap.apply"));
            button.set_action_target_value(Some(&target));
            // A secondary click: apply, redefine or delete.
            let menu = gio::Menu::new();
            let item = |label: &str, action: &str| {
                let item = gio::MenuItem::new(Some(label), None);
                item.set_action_and_target_value(Some(action), Some(&target));
                menu.append_item(&item);
            };
            item(&tr("Apply Drop Cap Style"), "drop-cap.apply");
            item(&tr("Redefine Style from Selection"), "drop-cap.redefine");
            item(&tr("Delete Style"), "drop-cap.delete");
            let context = gtk::PopoverMenu::from_model(Some(&menu));
            context.set_parent(&button);
            context.set_has_arrow(false);
            let click = gtk::GestureClick::builder().button(3).build();
            let shown = context.downgrade();
            click.connect_pressed(move |_, _, _, _| {
                if let Some(context) = shown.upgrade() {
                    context.popup();
                }
            });
            button.add_controller(click);
            // The menu goes with its button.
            button.connect_destroy(move |_| context.unparent());
            self.flow.append(&button);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pxv_06_each_style_of_the_library_has_a_picture() {
        let (w, h) = (PREVIEW.0 * 2, PREVIEW.1 * 2);
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w * 3, h * 2).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.paint().unwrap();
            for (i, style) in DropCapStyle::library().iter().enumerate() {
                cr.save().unwrap();
                cr.translate(f64::from(w * (i as i32 % 3)), f64::from(h * (i as i32 / 3)));
                cr.scale(2.0, 2.0);
                draw_preview(&cr, f64::from(PREVIEW.0), f64::from(PREVIEW.1), style);
                cr.restore().unwrap();
            }
        }
        // Every picture draws something dark: its letter or its square.
        let stride = surface.stride() as usize;
        let data = surface.take_data().unwrap();
        for i in 0..6 {
            let (x0, y0) = ((i % 3) * w as usize, (i / 3) * h as usize);
            let dark = (y0..y0 + h as usize)
                .any(|y| (x0..x0 + w as usize).any(|x| data[y * stride + x * 4] < 64));
            assert!(dark, "style {}", i + 1);
        }
        if let Ok(dir) = std::env::var("DESK_PREVIEW_OUT") {
            let surface = cairo::ImageSurface::create_for_data(
                data.to_vec(),
                cairo::Format::ARgb32,
                w * 3,
                h * 2,
                stride as i32,
            )
            .unwrap();
            let mut file = std::fs::File::create(format!("{dir}/drop-cap-styles.png")).unwrap();
            surface.write_to_png(&mut file).unwrap();
        }
    }
}
