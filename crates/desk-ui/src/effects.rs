// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effects of characters and paragraphs in the Inspector, as the Text
//! inspector of Pages has them: tracking, ligatures, outline and the
//! background of characters (FMT-04, FMT-09, FMT-10, FMT-20), and the
//! background and rules or border of paragraphs (FMT-20, FMT-21).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{
    BorderSides, BorderStyle, CharAttrs, CharFormat, Length, Ligatures, ParaAttrs, ParaBorder,
    ParaFormat, Rgba,
};

use crate::formatting::{EditorOf, from_gdk, labelled, to_gdk};
use crate::i18n::tr;

/// A change that a row makes through the editor.
pub(crate) type Change<T> = Rc<dyn Fn(&T, &crate::editing::TextEditor)>;
use crate::tab_stops::string_list;

/// The ligatures in the order of their menu.
pub const LIGATURES: [Ligatures; 3] = [Ligatures::Default, Ligatures::None, Ligatures::All];

/// The sides in the order of their menu.
const SIDES: [BorderSides; 5] = [
    BorderSides::None,
    BorderSides::Above,
    BorderSides::Below,
    BorderSides::AboveAndBelow,
    BorderSides::Box,
];

/// The styles of lines in the order of their menu.
const STYLES: [BorderStyle; 3] = [BorderStyle::Solid, BorderStyle::Dashed, BorderStyle::Dotted];

/// The name of a choice of ligatures.
pub fn ligatures_name(ligatures: Ligatures) -> String {
    match ligatures {
        Ligatures::Default => tr("Use Default"),
        Ligatures::None => tr("Use None"),
        Ligatures::All => tr("Use All"),
    }
}

fn sides_name(sides: BorderSides) -> String {
    match sides {
        BorderSides::None => tr("None"),
        BorderSides::Above => tr("Rule Above"),
        BorderSides::Below => tr("Rule Below"),
        BorderSides::AboveAndBelow => tr("Rules Above and Below"),
        BorderSides::Box => tr("Border Around"),
    }
}

fn style_name(style: BorderStyle) -> String {
    match style {
        BorderStyle::Solid => tr("Solid"),
        BorderStyle::Dashed => tr("Dashed"),
        BorderStyle::Dotted => tr("Dotted"),
    }
}

/// A row with a switch that turns a color on and the color.
pub struct ColorRow {
    /// The row.
    pub row: adw::ActionRow,
    /// Whether there is a color.
    pub switch: gtk::Switch,
    /// The color.
    pub button: gtk::ColorDialogButton,
}

impl ColorRow {
    /// A row titled `title`, whose color is `default` when turned on.
    pub fn new(title: &str, default: Rgba) -> Self {
        let switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
        labelled(&switch, title);
        let button = gtk::ColorDialogButton::builder()
            .dialog(&gtk::ColorDialog::builder().title(title).build())
            .rgba(&to_gdk(default))
            .valign(gtk::Align::Center)
            .build();
        labelled(&button, &format!("{title}: {}", tr("Color")));
        let row = adw::ActionRow::builder().title(title).build();
        row.add_suffix(&button);
        row.add_suffix(&switch);
        row.set_activatable_widget(Some(&switch));
        Self {
            row,
            switch,
            button,
        }
    }

    /// The color, if turned on.
    pub fn value(&self) -> Option<Rgba> {
        self.switch
            .is_active()
            .then(|| from_gdk(&self.button.rgba()))
    }

    /// Shows a color, or none.
    pub fn show(&self, color: Option<Rgba>) {
        self.switch.set_active(color.is_some());
        self.button.set_sensitive(color.is_some());
        if let Some(color) = color {
            let color = to_gdk(color);
            if self.button.rgba() != color {
                self.button.set_rgba(&color);
            }
        }
    }

    /// Calls `f` when the color is turned on or off or changes.
    pub fn connect_changed(&self, f: impl Fn() + Clone + 'static) {
        let changed = f.clone();
        self.switch.connect_active_notify(move |_| changed());
        self.button.connect_rgba_notify(move |_| f());
    }
}

/// The groups of effects in the Inspector.
pub struct Effects {
    character: adw::PreferencesGroup,
    paragraph: adw::PreferencesGroup,
    tracking: adw::SpinRow,
    ligatures: adw::ComboRow,
    outline: adw::SwitchRow,
    text_background: ColorRow,
    paragraph_background: ColorRow,
    sides: adw::ComboRow,
    style: adw::ComboRow,
    width: adw::SpinRow,
    color: ColorRow,
    offset: adw::SpinRow,
    /// The border shown, which a change of one of its rows changes.
    border: Cell<ParaBorder>,
    shown: RefCell<Option<(CharFormat, ParaFormat)>>,
    updating: Cell<bool>,
}

impl Effects {
    /// The effects for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let tracking = adw::SpinRow::with_range(-50.0, 500.0, 1.0);
        tracking.set_title(&tr("Character Spacing"));
        tracking.set_subtitle(&tr("Percent"));
        let ligatures = adw::ComboRow::builder()
            .title(tr("Ligatures"))
            .model(&string_list(LIGATURES.map(ligatures_name)))
            .build();
        let outline = adw::SwitchRow::builder().title(tr("Outline")).build();
        let text_background = ColorRow::new(&tr("Character Background"), Rgba::rgb(255, 255, 0));
        let character = adw::PreferencesGroup::builder()
            .title(tr("Characters"))
            .build();
        character.add(&tracking);
        character.add(&ligatures);
        character.add(&outline);
        character.add(&text_background.row);

        let paragraph_background =
            ColorRow::new(&tr("Paragraph Background"), Rgba::rgb(255, 255, 117));
        let sides = adw::ComboRow::builder()
            .title(tr("Borders and Rules"))
            .model(&string_list(SIDES.map(sides_name)))
            .build();
        let style = adw::ComboRow::builder()
            .title(tr("Line Style"))
            .model(&string_list(STYLES.map(style_name)))
            .build();
        let width = adw::SpinRow::with_range(0.25, 20.0, 0.25);
        width.set_title(&tr("Line Width"));
        width.set_subtitle(&tr("Points"));
        width.set_digits(2);
        let color = ColorRow::new(&tr("Line Color"), Rgba::BLACK);
        // The line always has a color: the switch only says it.
        color.switch.set_visible(false);
        let offset = adw::SpinRow::with_range(0.0, 100.0, 1.0);
        offset.set_title(&tr("Offset"));
        offset.set_subtitle(&tr("Distance from the text, in points"));
        offset.set_digits(1);
        let paragraph = adw::PreferencesGroup::builder()
            .title(tr("Background and Borders"))
            .build();
        paragraph.add(&paragraph_background.row);
        paragraph.add(&sides);
        paragraph.add(&style);
        paragraph.add(&width);
        paragraph.add(&color.row);
        paragraph.add(&offset);

        let this = Rc::new(Self {
            character,
            paragraph,
            tracking,
            ligatures,
            outline,
            text_background,
            paragraph_background,
            sides,
            style,
            width,
            color,
            offset,
            border: Cell::new(ParaBorder::default()),
            shown: RefCell::new(None),
            updating: Cell::new(false),
        });
        this.connect(editor);
        this
    }

    fn connect(self: &Rc<Self>, editor: EditorOf) {
        // Runs `f` with the editor, unless the rows are being shown.
        let with = |this: &Rc<Self>, f: Change<Self>| {
            let weak = Rc::downgrade(this);
            let editor = editor.clone();
            move || {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    f(&this, &editor);
                }
            }
        };
        let changed = with(
            self,
            Rc::new(|this, editor| {
                editor.set_character_attrs(
                    "Character Spacing",
                    &CharAttrs {
                        tracking: Some((this.tracking.value() * 10.0).round() as i32),
                        ..CharAttrs::default()
                    },
                );
            }),
        );
        self.tracking.connect_value_notify(move |_| changed());
        let changed = with(
            self,
            Rc::new(|this, editor| {
                let ligatures = LIGATURES[(this.ligatures.selected() as usize).min(2)];
                editor.set_character_attrs(
                    "Ligatures",
                    &CharAttrs {
                        ligatures: Some(ligatures),
                        ..CharAttrs::default()
                    },
                );
            }),
        );
        self.ligatures.connect_selected_notify(move |_| changed());
        let changed = with(
            self,
            Rc::new(|this, editor| {
                editor.set_character_attrs(
                    "Outline",
                    &CharAttrs {
                        outline: Some(this.outline.is_active()),
                        ..CharAttrs::default()
                    },
                );
            }),
        );
        self.outline.connect_active_notify(move |_| changed());
        self.text_background.connect_changed(with(
            self,
            Rc::new(|this, editor| {
                this.text_background
                    .button
                    .set_sensitive(this.text_background.switch.is_active());
                editor.set_character_attrs(
                    "Character Background",
                    &CharAttrs {
                        background: Some(this.text_background.value()),
                        ..CharAttrs::default()
                    },
                );
            }),
        ));
        self.paragraph_background.connect_changed(with(
            self,
            Rc::new(|this, editor| {
                this.paragraph_background
                    .button
                    .set_sensitive(this.paragraph_background.switch.is_active());
                editor.set_paragraph_attrs(
                    "Paragraph Background",
                    &ParaAttrs {
                        background: Some(this.paragraph_background.value()),
                        ..ParaAttrs::default()
                    },
                );
            }),
        ));
        let border_changed = with(
            self,
            Rc::new(|this, editor| {
                let border = ParaBorder {
                    sides: SIDES[(this.sides.selected() as usize).min(SIDES.len() - 1)],
                    style: STYLES[(this.style.selected() as usize).min(STYLES.len() - 1)],
                    width: Length::from_pt(this.width.value()),
                    color: from_gdk(&this.color.button.rgba()),
                    offset: Length::from_pt(this.offset.value()),
                };
                if border == this.border.get() {
                    return;
                }
                this.border.set(border);
                this.show_border_rows(border);
                editor.set_paragraph_attrs(
                    "Borders",
                    &ParaAttrs {
                        border: Some(border),
                        ..ParaAttrs::default()
                    },
                );
            }),
        );
        let changed = border_changed.clone();
        self.sides.connect_selected_notify(move |_| changed());
        let changed = border_changed.clone();
        self.style.connect_selected_notify(move |_| changed());
        let changed = border_changed.clone();
        self.width.connect_value_notify(move |_| changed());
        let changed = border_changed.clone();
        self.offset.connect_value_notify(move |_| changed());
        self.color
            .button
            .connect_rgba_notify(move |_| border_changed());
    }

    /// The groups, to add to the Inspector.
    pub fn groups(&self) -> [&adw::PreferencesGroup; 2] {
        [&self.character, &self.paragraph]
    }

    /// The details of the line only matter when there is one.
    fn show_border_rows(&self, border: ParaBorder) {
        let on = border.sides != BorderSides::None;
        for row in [
            self.style.upcast_ref::<gtk::Widget>(),
            self.width.upcast_ref(),
            self.color.row.upcast_ref(),
            self.offset.upcast_ref(),
        ] {
            row.set_sensitive(on);
        }
    }

    /// Shows the formatting at the selection.
    pub fn show(&self, character: &CharFormat, paragraph: &ParaFormat) {
        let shown = (character.clone(), paragraph.clone());
        if self.shown.borrow().as_ref() == Some(&shown) {
            return;
        }
        *self.shown.borrow_mut() = Some(shown);
        self.updating.set(true);
        let tracking = f64::from(character.tracking) / 10.0;
        if (self.tracking.value() - tracking).abs() > 0.01 {
            self.tracking.set_value(tracking);
        }
        let index = |i: Option<usize>| i.unwrap_or(0) as u32;
        self.ligatures.set_selected(index(
            LIGATURES.iter().position(|l| *l == character.ligatures),
        ));
        self.outline.set_active(character.outline);
        self.text_background.show(character.background);
        self.paragraph_background.show(paragraph.background);
        let border = paragraph.border;
        self.border.set(border);
        self.sides
            .set_selected(index(SIDES.iter().position(|s| *s == border.sides)));
        self.style
            .set_selected(index(STYLES.iter().position(|s| *s == border.style)));
        self.width.set_value(border.width.to_pt());
        self.offset.set_value(border.offset.to_pt());
        self.color.button.set_rgba(&to_gdk(border.color));
        self.show_border_rows(border);
        self.updating.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_choice_has_a_name() {
        let names: Vec<String> = LIGATURES.map(ligatures_name).into();
        assert_eq!(names.len(), 3);
        assert!(SIDES.map(sides_name).iter().all(|n| !n.is_empty()));
        assert!(STYLES.map(style_name).iter().all(|n| !n.is_empty()));
    }
}
