// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The object styles of the selected object, at the top of the Style tab
//! of the Inspector (MOD-04,
//! MOD-07): a preview of each style of the object's kind, which applies
//! it, and the commands that update, make, rename and delete styles and
//! make one the default.
//!
//! The previews run `win.object-style` with the identifier of their style,
//! and the commands the other actions of the window, so that the popover,
//! the Inspector, the keyboard and the command search do the same.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Fill, GraphicStyle, ObjectStyle, ObjectStyleKind, Rgba};
use gtk::{cairo, gio};

use crate::compact;
use crate::editing::ObjectStylesInfo;
use crate::formatting::labelled;
use crate::i18n::{n_, tr};

/// The width and the height of a preview, in pixels.
const PREVIEW: (i32, i32) = (56, 40);

/// The names of the styles of the library, translated where Desk made
/// them; a name the user gave stays as it is.
pub fn style_name(name: &str) -> String {
    const LIBRARY: [&str; 20] = [
        n_("Basic"),
        n_("Blue"),
        n_("Green"),
        n_("Orange"),
        n_("Red Outline"),
        n_("Gray"),
        n_("Thick"),
        n_("Dashed"),
        n_("Dotted"),
        n_("Red"),
        n_("Plain"),
        n_("Outline"),
        n_("Yellow"),
        n_("Light Blue"),
        n_("Card"),
        n_("Thin Border"),
        n_("Shadow"),
        n_("Reflection"),
        n_("Photo"),
        n_("Gallery"),
    ];
    if LIBRARY.contains(&name) {
        tr(name)
    } else {
        name.to_owned()
    }
}

/// The target of `win.object-style` that applies a style.
pub fn style_target(style: desk_model::StyleId) -> String {
    style.get().to_string()
}

/// The style that a target of `win.object-style` names.
pub fn style_of_target(target: &str) -> Option<desk_model::StyleId> {
    target.parse().ok().and_then(desk_model::NodeId::new)
}

/// The commands on object styles, as a menu.
pub fn commands_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let style = gio::Menu::new();
    style.append(
        Some(&tr("_Update Style from Selection")),
        Some("win.update-object-style"),
    );
    style.append(
        Some(&tr("_New Style from Selection")),
        Some("win.new-object-style"),
    );
    style.append(
        Some(&tr("Define as _Default Style")),
        Some("win.default-object-style"),
    );
    style.append(
        Some(&tr("Set as Default _Text Box Appearance")),
        Some("win.default-text-box"),
    );
    menu.append_section(None, &style);
    let manage = gio::Menu::new();
    manage.append(Some(&tr("_Rename Style…")), Some("win.rename-object-style"));
    manage.append(Some(&tr("De_lete Style")), Some("win.delete-object-style"));
    menu.append_section(None, &manage);
    menu
}

/// Draws a style on a preview: a rounded square for a shape, a text box
/// or a picture, a slanted line for a line.
fn draw_preview(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    kind: ObjectStyleKind,
    look: &GraphicStyle,
) {
    let rgba = |c: Rgba, opacity: f64| {
        cr.set_source_rgba(
            f64::from(c.r) / 255.0,
            f64::from(c.g) / 255.0,
            f64::from(c.b) / 255.0,
            f64::from(c.a) / 255.0 * opacity,
        );
    };
    let opacity = f64::from(look.opacity) / 100.0;
    let (x, y, w, h) = (8.0, 6.0, width - 16.0, height - 16.0);
    let line = look.stroke.map(|s| (s.width.to_pt().clamp(0.75, 4.0), s));
    if kind == ObjectStyleKind::Line {
        if let Some((width, stroke)) = line {
            rgba(stroke.color, opacity);
            cr.set_line_width(width);
            match stroke.style {
                desk_model::BorderStyle::Dashed => cr.set_dash(&[width * 3.0, width * 2.0], 0.0),
                desk_model::BorderStyle::Dotted => cr.set_dash(&[width, width * 1.5], 0.0),
                desk_model::BorderStyle::Solid => cr.set_dash(&[], 0.0),
            }
            cr.move_to(x, y + h);
            cr.line_to(x + w, y);
            let _ = cr.stroke();
        }
        return;
    }
    let rect = |dx: f64, dy: f64| {
        let r = 4.0;
        cr.new_sub_path();
        cr.arc(x + dx + w - r, y + dy + r, r, -90f64.to_radians(), 0.0);
        cr.arc(x + dx + w - r, y + dy + h - r, r, 0.0, 90f64.to_radians());
        cr.arc(
            x + dx + r,
            y + dy + h - r,
            r,
            90f64.to_radians(),
            180f64.to_radians(),
        );
        cr.arc(
            x + dx + r,
            y + dy + r,
            r,
            180f64.to_radians(),
            270f64.to_radians(),
        );
        cr.close_path();
    };
    if let Some(shadow) = look.shadow {
        rgba(
            shadow.color,
            opacity * f64::from(shadow.opacity) / 100.0 * 0.6,
        );
        rect(3.0, 3.0);
        let _ = cr.fill();
    }
    if let Some(reflection) = look.reflection {
        rgba(
            Rgba::rgb(0x9a, 0x99, 0x96),
            opacity * f64::from(reflection) / 100.0,
        );
        cr.rectangle(x + 2.0, y + h + 2.0, w - 4.0, 3.0);
        let _ = cr.fill();
    }
    let fill = match &look.fill {
        Some(Fill::Color(color)) => Some(*color),
        Some(Fill::Gradient { from, .. }) => Some(*from),
        Some(Fill::Advanced(gradient)) => gradient.stops.first().map(|s| s.color),
        // A picture, or a picture filling a shape, shows as a pale gray.
        Some(_) => Some(Rgba::rgb(0xde, 0xdd, 0xda)),
        None if kind == ObjectStyleKind::Image => Some(Rgba::rgb(0xde, 0xdd, 0xda)),
        None => None,
    };
    if let Some(color) = fill {
        rgba(color, opacity);
        rect(0.0, 0.0);
        let _ = cr.fill();
    }
    if let Some(frame) = look.frame {
        // A picture frame, as a wide pale border with a darker edge.
        let wide = f64::from(frame.scale) / 100.0 * 3.0;
        rgba(Rgba::WHITE, opacity);
        cr.set_line_width(wide);
        rect(0.0, 0.0);
        let _ = cr.stroke();
        rgba(Rgba::rgb(0x77, 0x76, 0x7b), opacity);
        cr.set_line_width(0.75);
        cr.rectangle(x - wide / 2.0, y - wide / 2.0, w + wide, h + wide);
        let _ = cr.stroke();
    } else if let Some((width, stroke)) = line {
        rgba(stroke.color, opacity);
        cr.set_line_width(width);
        match stroke.style {
            desk_model::BorderStyle::Dashed => cr.set_dash(&[width * 3.0, width * 2.0], 0.0),
            desk_model::BorderStyle::Dotted => cr.set_dash(&[width, width * 1.5], 0.0),
            desk_model::BorderStyle::Solid => cr.set_dash(&[], 0.0),
        }
        rect(0.0, 0.0);
        let _ = cr.stroke();
    } else if fill.is_none() {
        // No fill and no line, as a plain text box: a faint outline, so
        // that the preview shows where the object is.
        cr.set_source_rgba(0.5, 0.5, 0.5, 0.5);
        cr.set_line_width(1.0);
        cr.set_dash(&[2.0, 2.0], 0.0);
        rect(0.0, 0.0);
        let _ = cr.stroke();
    }
}

/// The previews of the styles of the selected object's kind.
pub struct StylePicker {
    root: gtk::FlowBox,
    /// The styles shown, to make the previews again only when they change.
    shown: RefCell<Vec<ObjectStyle>>,
}

impl StylePicker {
    pub fn new() -> Rc<Self> {
        let root = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(3)
            .max_children_per_line(3)
            .homogeneous(true)
            .column_spacing(6)
            .row_spacing(6)
            .halign(gtk::Align::Center)
            .build();
        labelled(&root, &tr("Object Styles"));
        Rc::new(Self {
            root,
            shown: RefCell::new(Vec::new()),
        })
    }

    pub fn widget(&self) -> &gtk::FlowBox {
        &self.root
    }

    /// Shows the styles of the kind of the selected object.
    pub fn show(&self, info: &ObjectStylesInfo) {
        if *self.shown.borrow() == info.styles {
            return;
        }
        *self.shown.borrow_mut() = info.styles.clone();
        self.root.remove_all();
        for style in &info.styles {
            let area = gtk::DrawingArea::builder()
                .content_width(PREVIEW.0)
                .content_height(PREVIEW.1)
                .build();
            let kind = style.kind;
            let look = style.graphic.clone();
            area.set_draw_func(move |_, cr, width, height| {
                draw_preview(cr, f64::from(width), f64::from(height), kind, &look);
            });
            let name = style_name(&style.name);
            let button = gtk::ToggleButton::builder()
                .child(&area)
                .tooltip_text(&name)
                .css_classes(["flat"])
                .build();
            labelled(&button, &name);
            button.set_action_name(Some("win.object-style"));
            button.set_action_target_value(Some(&style_target(style.id).to_variant()));
            self.root.append(&button);
        }
    }
}

/// The group of object styles at the top of the Style tab of the
/// Inspector.
pub struct StylesGroup {
    group: compact::Group,
    picker: Rc<StylePicker>,
}

impl StylesGroup {
    pub fn new() -> Rc<Self> {
        let picker = StylePicker::new();
        let more = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .menu_model(&commands_menu())
            .tooltip_text(tr("Style Commands"))
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        labelled(&more, &tr("Style Commands"));
        let group = compact::Group::builder()
            .title(tr("Object Styles"))
            .header_suffix(&more)
            .build();
        group.add_widget(picker.widget());
        Rc::new(Self { group, picker })
    }

    pub fn group(&self) -> &compact::Group {
        &self.group
    }

    /// Shows the styles of the selected object, with the name of its own
    /// and whether it was changed since it took it.
    pub fn show(&self, info: Option<&crate::editing::ObjectInfo>) {
        let styles = info.map(|i| &i.styles).filter(|s| !s.styles.is_empty());
        self.group.set_visible(styles.is_some());
        let Some(styles) = styles else { return };
        self.picker.show(styles);
        let current = styles
            .current
            .and_then(|id| styles.styles.iter().find(|s| s.id == id));
        let description = match current {
            Some(style) if styles.modified => {
                format!("{} ({})", style_name(&style.name), tr("changed"))
            }
            Some(style) if Some(style.id) == styles.default => {
                format!("{} ({})", style_name(&style.name), tr("default"))
            }
            Some(style) => style_name(&style.name),
            None => tr("No style"),
        };
        self.group.set_description(Some(&description));
        self.group
            .set_sensitive(info.is_some_and(|i| !i.object.locked));
    }
}
