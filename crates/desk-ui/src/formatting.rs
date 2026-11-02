// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The format bar under the header bar and the Inspector at the side of a
//! window, as in Pages: they show the formatting at the selection and
//! change it.
//!
//! The toggles of the format bar (bold, italic, underline, strikethrough
//! and alignment) are actions of the window, which the keyboard shortcuts
//! also run; the rest calls the editor through `editor`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use desk_model::{Alignment, CharAttrs, Length, LineSpacing, ParaAttrs, Rgba, StyleId};
use gtk::{gdk, glib};

use crate::editing::{Format, TextEditor};
use crate::i18n::{n_, tr};
use crate::preferences::{self, Units};
use crate::tab_stops::{self, TabStopsGroup};

/// Finds the editor of the window, if it shows a document.
pub type EditorOf = Rc<dyn Fn() -> Option<TextEditor>>;

/// The largest font size Pages takes, in points.
const MAX_FONT_SIZE: f64 = 1638.0;

/// The alignment as the target of the `win.align` action.
pub fn alignment_name(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Left | Alignment::Auto => "left",
        Alignment::Center => "center",
        Alignment::Right => "right",
        Alignment::Justify => "justify",
    }
}

/// The alignment that a target of the `win.align` action names.
pub fn alignment_of(name: &str) -> Option<Alignment> {
    Some(match name {
        "left" => Alignment::Left,
        "center" => Alignment::Center,
        "right" => Alignment::Right,
        "justify" => Alignment::Justify,
        _ => return None,
    })
}

/// A color of the model as GDK takes it.
pub fn to_gdk(color: Rgba) -> gdk::RGBA {
    gdk::RGBA::new(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        f32::from(color.a) / 255.0,
    )
}

/// A color of GDK as the model keeps it.
pub fn from_gdk(color: &gdk::RGBA) -> Rgba {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgba {
        r: byte(color.red()),
        g: byte(color.green()),
        b: byte(color.blue()),
        a: byte(color.alpha()),
    }
}

/// The format bar.
pub struct FormatBar {
    root: gtk::Box,
    styles: gtk::DropDown,
    style_ids: RefCell<Vec<StyleId>>,
    /// The font family, by name: a font the document names stays named
    /// here even when it is not installed, as in Pages.
    font: gtk::Button,
    size: gtk::SpinButton,
    color: gtk::ColorDialogButton,
    /// Set while the bar shows the formatting at the selection, so that
    /// showing it does not change it.
    updating: Cell<bool>,
}

impl FormatBar {
    /// A format bar for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let root = gtk::Box::builder()
            .spacing(6)
            .margin_start(6)
            .margin_end(6)
            .margin_top(3)
            .margin_bottom(3)
            .css_classes(["toolbar"])
            .build();

        let styles = gtk::DropDown::builder()
            .model(&gtk::StringList::new(&[]))
            .tooltip_text(tr("Paragraph Style"))
            .enable_search(true)
            .build();
        styles.update_property(&[gtk::accessible::Property::Label(&tr("Paragraph Style"))]);
        root.append(&styles);

        let font = gtk::Button::builder()
            .tooltip_text(tr("Font"))
            .width_request(140)
            .build();
        root.append(&font);

        let size = gtk::SpinButton::with_range(1.0, MAX_FONT_SIZE, 1.0);
        size.set_digits(0);
        size.set_tooltip_text(Some(&tr("Font Size")));
        size.update_property(&[gtk::accessible::Property::Label(&tr("Font Size"))]);
        root.append(&size);

        let color = gtk::ColorDialogButton::new(Some(
            gtk::ColorDialog::builder()
                .title(tr("Text Color"))
                .modal(true)
                .with_alpha(true)
                .build(),
        ));
        color.set_tooltip_text(Some(&tr("Text Color")));
        labelled(&color, &tr("Text Color"));
        root.append(&color);

        let toggles = gtk::Box::builder().css_classes(["linked"]).build();
        for (icon, action, tooltip) in [
            ("format-text-bold-symbolic", "win.bold", n_("Bold")),
            ("format-text-italic-symbolic", "win.italic", n_("Italic")),
            (
                "format-text-underline-symbolic",
                "win.underline",
                n_("Underline"),
            ),
            (
                "format-text-strikethrough-symbolic",
                "win.strikethrough",
                n_("Strikethrough"),
            ),
        ] {
            let button = gtk::ToggleButton::builder()
                .icon_name(icon)
                .action_name(action)
                .tooltip_text(tr(tooltip))
                .build();
            labelled(&button, &tr(tooltip));
            toggles.append(&button);
        }
        root.append(&toggles);

        let alignments = gtk::Box::builder().css_classes(["linked"]).build();
        for (icon, target, tooltip) in [
            ("format-justify-left-symbolic", "left", n_("Align Left")),
            ("format-justify-center-symbolic", "center", n_("Center")),
            ("format-justify-right-symbolic", "right", n_("Align Right")),
            ("format-justify-fill-symbolic", "justify", n_("Justify")),
        ] {
            let button = gtk::ToggleButton::builder()
                .icon_name(icon)
                .tooltip_text(tr(tooltip))
                .build();
            labelled(&button, &tr(tooltip));
            button.set_action_name(Some("win.align"));
            button.set_action_target_value(Some(&target.to_variant()));
            alignments.append(&button);
        }
        root.append(&alignments);

        let spacer = gtk::Box::builder().hexpand(true).build();
        root.append(&spacer);
        let inspector = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-right-symbolic")
            .action_name("win.inspector")
            .tooltip_text(tr("Inspector"))
            .build();
        labelled(&inspector, &tr("Inspector"));
        root.append(&inspector);

        let this = Rc::new(Self {
            root,
            styles,
            style_ids: RefCell::new(Vec::new()),
            font,
            size,
            color,
            updating: Cell::new(false),
        });
        this.connect(editor);
        this
    }

    /// The widget to put under the header bar.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn connect(self: &Rc<Self>, editor: EditorOf) {
        let weak = Rc::downgrade(self);
        let of = editor.clone();
        self.styles.connect_selected_notify(move |dropdown| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let id = this
                .style_ids
                .borrow()
                .get(dropdown.selected() as usize)
                .copied();
            if let (Some(id), Some(editor)) = (id, of()) {
                editor.set_paragraph_style(id);
                editor.focus();
            }
        });

        let of = editor.clone();
        self.font.connect_clicked(move |button| {
            let current = button.label().unwrap_or_default();
            let installed = button
                .pango_context()
                .list_families()
                .into_iter()
                .find(|f| f.name().eq_ignore_ascii_case(&current));
            let window = button.root().and_downcast::<gtk::Window>();
            let of = of.clone();
            gtk::FontDialog::builder()
                .title(tr("Font"))
                .modal(true)
                .build()
                .choose_family(
                    window.as_ref(),
                    installed.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    move |result| {
                        let (Ok(family), Some(editor)) = (result, of()) else {
                            return;
                        };
                        editor.set_character_attrs(
                            "Font",
                            &CharAttrs {
                                font_family: Some(Arc::from(family.name().as_str())),
                                ..CharAttrs::default()
                            },
                        );
                        editor.focus();
                    },
                );
        });

        let weak = Rc::downgrade(self);
        let of = editor.clone();
        self.size.connect_value_changed(move |spin| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = of() {
                editor.set_character_attrs(
                    "Font Size",
                    &CharAttrs {
                        font_size: Some(Length::from_pt(spin.value())),
                        ..CharAttrs::default()
                    },
                );
            }
        });

        let weak = Rc::downgrade(self);
        self.color.connect_rgba_notify(move |button| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = editor() {
                editor.set_character_attrs(
                    "Text Color",
                    &CharAttrs {
                        color: Some(from_gdk(&button.rgba())),
                        ..CharAttrs::default()
                    },
                );
                editor.focus();
            }
        });
    }

    /// Shows the paragraph styles of a document.
    pub fn set_styles(&self, styles: &[(StyleId, String)]) {
        self.updating.set(true);
        let names: Vec<&str> = styles.iter().map(|(_, n)| n.as_str()).collect();
        self.styles.set_model(Some(&gtk::StringList::new(&names)));
        *self.style_ids.borrow_mut() = styles.iter().map(|(id, _)| *id).collect();
        self.updating.set(false);
    }

    /// Shows the formatting at the selection.
    pub fn show(&self, format: &Format) {
        self.updating.set(true);
        let index = format
            .style
            .and_then(|s| self.style_ids.borrow().iter().position(|id| *id == s));
        self.styles
            .set_selected(index.map_or(gtk::INVALID_LIST_POSITION, |i| i as u32));
        let character = &format.character;
        self.font.set_label(&character.font_family);
        let size = character.font_size.to_pt();
        if (self.size.value() - size).abs() > 0.01 {
            self.size.set_value(size);
        }
        let color = to_gdk(character.color);
        if self.color.rgba() != color {
            self.color.set_rgba(&color);
        }
        self.updating.set(false);
    }
}

/// The Inspector: the spacing and indents of the paragraphs (the Text
/// inspector of Pages).
pub struct Inspector {
    root: gtk::ScrolledWindow,
    line_spacing: adw::SpinRow,
    before: adw::SpinRow,
    after: adw::SpinRow,
    first_line: adw::SpinRow,
    left: adw::SpinRow,
    right: adw::SpinRow,
    tabs: Rc<TabStopsGroup>,
    /// The units of the indents, those of the rulers.
    units: Cell<Units>,
    /// The paragraph format shown, to show again in other units.
    shown: RefCell<Option<Format>>,
    updating: Cell<bool>,
}

/// What a row of the Inspector sets.
#[derive(Clone, Copy)]
enum Field {
    LineSpacing,
    Before,
    After,
    FirstLine,
    Left,
    Right,
}

impl Field {
    fn name(self) -> &'static str {
        match self {
            Self::LineSpacing => "Line Spacing",
            Self::Before => "Space Before",
            Self::After => "Space After",
            Self::FirstLine | Self::Left | Self::Right => "Indent",
        }
    }

    /// Whether the field is a length in the units of the rulers; the
    /// others are in lines or points, as in Pages.
    fn in_units(self) -> bool {
        matches!(self, Self::FirstLine | Self::Left | Self::Right)
    }

    /// The attributes for a value of the field, in its units.
    fn attrs(self, value: f64, units: Units) -> ParaAttrs {
        let length = Some(if self.in_units() {
            tab_stops::from_units(value, units)
        } else {
            Length::from_pt(value)
        });
        let mut attrs = ParaAttrs::default();
        match self {
            Self::LineSpacing => {
                attrs.line_spacing = Some(LineSpacing::Multiple {
                    percent: (value * 100.0).round() as u32,
                });
            }
            Self::Before => attrs.space_before = length,
            Self::After => attrs.space_after = length,
            Self::FirstLine => attrs.first_line_indent = length,
            Self::Left => attrs.left_indent = length,
            Self::Right => attrs.right_indent = length,
        }
        attrs
    }
}

impl Inspector {
    /// An Inspector for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let row = |title: &str, subtitle: &str, min: f64, max: f64, step: f64, digits: u32| {
            let row = adw::SpinRow::with_range(min, max, step);
            row.set_title(title);
            row.set_subtitle(subtitle);
            row.set_digits(digits);
            row
        };
        let line_spacing = row(&tr("Line Spacing"), &tr("Lines"), 0.5, 10.0, 0.1, 1);
        let before = row(&tr("Before Paragraph"), &tr("Points"), 0.0, 1584.0, 1.0, 0);
        let after = row(&tr("After Paragraph"), &tr("Points"), 0.0, 1584.0, 1.0, 0);
        let units = preferences::get().units;
        let first_line = row(&tr("First Line"), "", 0.0, 1.0, 1.0, 0);
        let left = row(&tr("Left"), "", 0.0, 1.0, 1.0, 0);
        let right = row(&tr("Right"), "", 0.0, 1.0, 1.0, 0);
        let tabs = TabStopsGroup::new(editor.clone(), units);

        let spacing = adw::PreferencesGroup::builder()
            .title(tr("Spacing"))
            .build();
        spacing.add(&line_spacing);
        spacing.add(&before);
        spacing.add(&after);
        let indents = adw::PreferencesGroup::builder()
            .title(tr("Indents"))
            .build();
        indents.add(&first_line);
        indents.add(&left);
        indents.add(&right);
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        content.append(&spacing);
        content.append(&indents);
        content.append(tabs.widget());
        let root = gtk::ScrolledWindow::builder()
            .child(&content)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(280)
            .build();

        let this = Rc::new(Self {
            root,
            line_spacing,
            before,
            after,
            first_line,
            left,
            right,
            tabs,
            units: Cell::new(units),
            shown: RefCell::new(None),
            updating: Cell::new(false),
        });
        this.set_units(units);
        let weak = Rc::downgrade(&this);
        preferences::connect_changed(move |p| {
            if let Some(this) = weak.upgrade() {
                this.set_units(p.units);
            }
        });
        for (row, field) in [
            (&this.line_spacing, Field::LineSpacing),
            (&this.before, Field::Before),
            (&this.after, Field::After),
            (&this.first_line, Field::FirstLine),
            (&this.left, Field::Left),
            (&this.right, Field::Right),
        ] {
            let weak = Rc::downgrade(&this);
            let editor = editor.clone();
            row.connect_value_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    let attrs = field.attrs(row.value(), this.units.get());
                    editor.set_paragraph_attrs(field.name(), &attrs);
                }
            });
        }
        this
    }

    /// The widget to put at the side of the window.
    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    /// Measures the indents in other units.
    fn set_units(&self, units: Units) {
        self.updating.set(true);
        self.units.set(units);
        tab_stops::set_units(&self.first_line, units, -1584.0, 1584.0);
        tab_stops::set_units(&self.left, units, 0.0, 1584.0);
        tab_stops::set_units(&self.right, units, 0.0, 1584.0);
        self.updating.set(false);
        self.tabs.set_units(units);
        let shown = self.shown.borrow().clone();
        if let Some(format) = shown {
            self.show(&format);
        }
    }

    /// Shows the formatting at the selection.
    pub fn show(&self, format: &Format) {
        *self.shown.borrow_mut() = Some(format.clone());
        self.updating.set(true);
        let paragraph = &format.paragraph;
        let set = |row: &adw::SpinRow, value: f64| {
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        };
        match paragraph.line_spacing {
            LineSpacing::Multiple { percent } => {
                self.line_spacing.set_sensitive(true);
                set(&self.line_spacing, f64::from(percent) / 100.0);
            }
            // Spacing in points is set in phase 4, with the rest of the
            // Text inspector.
            _ => self.line_spacing.set_sensitive(false),
        }
        set(&self.before, paragraph.space_before.to_pt());
        set(&self.after, paragraph.space_after.to_pt());
        let units = self.units.get();
        set(
            &self.first_line,
            tab_stops::in_units(paragraph.first_line_indent, units),
        );
        set(
            &self.left,
            tab_stops::in_units(paragraph.left_indent, units),
        );
        set(
            &self.right,
            tab_stops::in_units(paragraph.right_indent, units),
        );
        self.updating.set(false);
        self.tabs.show(paragraph);
    }
}

/// Names a button that shows only an icon, for screen readers.
pub fn labelled(widget: &impl IsA<gtk::Accessible>, label: &str) {
    widget.update_property(&[gtk::accessible::Property::Label(label)]);
}

/// The value of a boolean action's state.
pub fn state_of(action: &gtk::gio::SimpleAction) -> bool {
    action
        .state()
        .and_then(|s| s.get::<bool>())
        .unwrap_or(false)
}

/// Sets the state of a stateful action without running it.
pub fn set_state(window: &impl IsA<gtk::gio::ActionMap>, name: &str, value: glib::Variant) {
    if let Some(action) = window
        .lookup_action(name)
        .and_then(|a| a.downcast::<gtk::gio::SimpleAction>().ok())
        && action.state().as_ref() != Some(&value)
    {
        action.set_state(&value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_go_to_gdk_and_back() {
        for color in [
            Rgba::BLACK,
            Rgba::rgb(200, 30, 7),
            Rgba {
                a: 128,
                ..Rgba::rgb(1, 2, 3)
            },
        ] {
            assert_eq!(from_gdk(&to_gdk(color)), color);
        }
    }

    #[test]
    fn alignments_name_their_actions() {
        for alignment in [
            Alignment::Left,
            Alignment::Center,
            Alignment::Right,
            Alignment::Justify,
        ] {
            assert_eq!(alignment_of(alignment_name(alignment)), Some(alignment));
        }
        assert_eq!(alignment_name(Alignment::Auto), "left");
        assert_eq!(alignment_of("middle"), None);
    }

    #[test]
    fn inspector_rows_set_one_attribute() {
        let attrs = Field::LineSpacing.attrs(1.5, Units::Inches);
        assert_eq!(
            attrs.line_spacing,
            Some(LineSpacing::Multiple { percent: 150 })
        );
        // Indents are in the units of the rulers, and spacing in points.
        assert_eq!(
            Field::FirstLine
                .attrs(0.25, Units::Inches)
                .first_line_indent,
            Some(Length::from_pt(18.0))
        );
        assert_eq!(
            Field::Before.attrs(6.0, Units::Inches).space_before,
            Some(Length::from_pt(6.0))
        );
        assert!(
            Field::Before
                .attrs(6.0, Units::Points)
                .left_indent
                .is_none()
        );
    }
}
