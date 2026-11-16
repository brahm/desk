// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The formatting of the text of a selected shape or text box, in the Text
//! tab of the Inspector (OBJ-13): the font, its size, style and color, the
//! alignment and the list, as the Text inspector of Pages sets them for
//! all the text of a selected object. The rows change the whole text of
//! each selected shape and text box; with the text being edited, the
//! Inspector shows the tabs of text instead.
//!
//! The same groups, made a second time, are the Font and Alignment groups
//! of the Style tab of text in window M+ (step 2), where they take the
//! place of the format bar of interface B; [`ObjectTextGroups::show_text`]
//! shows them for selected text.
//!
//! The style and alignment toggles and the list run the actions of the
//! window, which format the text of the selected objects as they format
//! selected text; the rest calls the editor through `editor`.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{CharAttrs, Length};

use crate::compact;
use crate::editing::Format;
use crate::formatting::{EditorOf, from_gdk, labelled, to_gdk};
use crate::i18n::{n_, tr};

/// The largest font size Pages takes, in points.
const MAX_FONT_SIZE: f64 = 1638.0;

/// The style toggles: an icon, the action and a name.
const STYLES: [(&str, &str, &str); 4] = [
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
];

/// The alignments: an icon, the target of `win.align` and a name.
const ALIGNMENTS: [(&str, &str, &str); 4] = [
    ("format-justify-left-symbolic", "left", n_("Align Left")),
    ("format-justify-center-symbolic", "center", n_("Center")),
    ("format-justify-right-symbolic", "right", n_("Align Right")),
    ("format-justify-fill-symbolic", "justify", n_("Justify")),
];

/// The Font and Alignment groups of the Text tab of an object.
pub struct ObjectTextGroups {
    font: compact::Group,
    alignment: compact::Group,
    /// The family, on the button that opens the Fonts popover.
    family: gtk::Button,
    family_label: gtk::Label,
    size: compact::SpinRow,
    color: gtk::ColorDialogButton,
    /// The button of the Colors popover, which shows `color`.
    colors: gtk::MenuButton,
    list: compact::ComboRow,
    updating: Cell<bool>,
}

impl ObjectTextGroups {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let font = compact::Group::builder().title(tr("Font")).build();
        let family_label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(16)
            .build();
        let family = gtk::Button::builder()
            .child(&family_label)
            .action_name("win.fonts")
            .valign(gtk::Align::Center)
            .tooltip_text(tr("Fonts"))
            .build();
        labelled(&family, &tr("Font Family"));
        let family_row = compact::ActionRow::builder().title(tr("Family")).build();
        family_row.add_suffix(&family);
        font.add(&family_row);

        let size = compact::SpinRow::with_range(1.0, MAX_FONT_SIZE, 1.0);
        size.set_title(&tr("Size"));
        size.set_subtitle(&tr("Points"));
        size.set_digits(0);
        font.add(&size);

        let toggles = gtk::Box::builder()
            .css_classes(["linked"])
            .valign(gtk::Align::Center)
            .build();
        for (icon, action, name) in STYLES {
            let button = gtk::ToggleButton::builder()
                .icon_name(icon)
                .action_name(action)
                .tooltip_text(tr(name))
                .build();
            labelled(&button, &tr(name));
            toggles.append(&button);
        }
        let style_row = compact::ActionRow::builder().title(tr("Style")).build();
        style_row.add_suffix(&toggles);
        font.add(&style_row);

        let color = gtk::ColorDialogButton::new(Some(
            gtk::ColorDialog::builder()
                .title(tr("Text Color"))
                .modal(true)
                .with_alpha(true)
                .build(),
        ));
        color.set_valign(gtk::Align::Center);
        labelled(&color, &tr("Text Color"));
        let color_row = compact::ActionRow::builder()
            .title(tr("Text Color"))
            .build();
        // The Colors popover opens from the row (INT-16, step 2 of M+).
        let colors = crate::color_popover::button(&color, &tr("Text Color"));
        color_row.add_suffix(&colors);
        font.add(&color_row);

        let alignment = compact::Group::builder().title(tr("Alignment")).build();
        let alignments = gtk::Box::builder()
            .css_classes(["linked"])
            .valign(gtk::Align::Center)
            .build();
        for (icon, target, name) in ALIGNMENTS {
            let button = gtk::ToggleButton::builder()
                .icon_name(icon)
                .tooltip_text(tr(name))
                .build();
            labelled(&button, &tr(name));
            button.set_action_name(Some("win.align"));
            button.set_action_target_value(Some(&target.to_variant()));
            alignments.append(&button);
        }
        let horizontal = compact::ActionRow::builder()
            .title(tr("Horizontal"))
            .build();
        horizontal.add_suffix(&alignments);
        alignment.add(&horizontal);
        let names: Vec<String> = crate::format_pane::LISTS
            .iter()
            .map(|(_, name)| tr(name))
            .collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let list = compact::ComboRow::builder()
            .title(tr("List"))
            .model(&gtk::StringList::new(&names))
            .build();
        alignment.add(&list);

        let this = Rc::new(Self {
            font,
            alignment,
            family,
            family_label,
            size,
            color,
            colors,
            list,
            updating: Cell::new(false),
        });

        let weak = Rc::downgrade(&this);
        let of = editor.clone();
        this.size.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = of() {
                editor.set_character_attrs(
                    "Font Size",
                    &CharAttrs {
                        font_size: Some(Length::from_pt(row.value())),
                        ..CharAttrs::default()
                    },
                );
            }
        });
        let weak = Rc::downgrade(&this);
        let of = editor.clone();
        this.color.connect_rgba_notify(move |button| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = of() {
                editor.set_character_attrs(
                    "Text Color",
                    &CharAttrs {
                        color: Some(from_gdk(&button.rgba())),
                        ..CharAttrs::default()
                    },
                );
            }
        });
        let weak = Rc::downgrade(&this);
        this.list.connect_selected_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let Some((target, _)) = crate::format_pane::LISTS.get(row.selected() as usize) else {
                return;
            };
            let _ = row.activate_action("win.list-style", Some(&target.to_variant()));
        });
        this
    }

    /// The groups, to put in the Text tab.
    pub fn groups(&self) -> [&compact::Group; 2] {
        [&self.font, &self.alignment]
    }

    /// The button of the family, which the Fonts popover opens on while
    /// an object is selected.
    pub fn family_button(&self) -> &gtk::Button {
        &self.family
    }

    /// Leaves out the List row, where a pane of its own sets the list,
    /// as the Format pane of text does (step 2 of window M+).
    pub fn hide_list(&self) {
        self.list.set_visible(false);
    }

    /// The button of the Colors popover of the text color, which
    /// `win.colors` opens.
    pub fn colors_button(&self) -> &gtk::MenuButton {
        &self.colors
    }

    /// Shows the formatting of the text of the selected object. A shape
    /// with no text has no characters to format yet, only its paragraph.
    pub fn show(&self, format: &Format, has_text: bool) {
        let Some(info) = &format.object else {
            return;
        };
        let editable = !info.object.locked;
        self.font.set_sensitive(editable && has_text);
        self.font.set_description(
            (!has_text)
                .then(|| tr("Type in the shape to format its text"))
                .as_deref(),
        );
        self.alignment.set_sensitive(editable);
        self.show_characters(format);
    }

    /// Shows the formatting at selected text, in the Format pane of text
    /// (step 2 of window M+), where these groups take the place of the
    /// format bar of interface B.
    pub fn show_text(&self, format: &Format) {
        self.show_characters(format);
    }

    /// Shows the font, its size and color, and the list at the selection.
    fn show_characters(&self, format: &Format) {
        self.updating.set(true);
        let character = &format.character;
        self.family_label.set_label(&character.font_family);
        let size = character.font_size.to_pt();
        if (self.size.value() - size).abs() > 0.01 {
            self.size.set_value(size);
        }
        let color = to_gdk(character.color);
        if self.color.rgba() != color {
            self.color.set_rgba(&color);
        }
        let list = crate::lists_pane::list_target(format.list.as_ref());
        let index = crate::format_pane::LISTS
            .iter()
            .position(|(target, _)| *target == list)
            .unwrap_or(0) as u32;
        if self.list.selected() != index {
            self.list.set_selected(index);
        }
        self.updating.set(false);
    }
}
