// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The compact rows of the Inspector. Pages fits many controls in a narrow
//! window, a label and its control on one line; the rows of Libadwaita are
//! about 50 pixels high and made the Inspector a long scroll. These rows
//! are plain GTK: each is a line with its label in a column that every row
//! of its group shares, so a group reads as a grid, inside a card in the
//! style of Adwaita.
//!
//! They follow the API of the Libadwaita rows they replace, so a pane
//! changes little: a row dereferences to its control (a spin button, a
//! switch, a drop-down or an entry), and has its title, subtitle,
//! visibility and sensitivity. The subtitle of a spin row is the unit
//! after the value, that of an action row a caption under its title, and
//! that of the others their tooltip.

#![allow(
    dead_code,
    reason = "the rows offer the API of the Libadwaita rows they replace, of which each pane uses a part"
)]

use std::ops::Deref;

use adw::prelude::*;
use gtk::{gio, glib};

/// Anything a group holds as a row.
pub trait Row {
    /// The line of the row.
    fn row_widget(&self) -> &gtk::Widget;
    /// The label of the row, which lines up with the labels of the other
    /// rows of the group.
    fn title_label(&self) -> Option<&gtk::Label>;
}

/// The symbol of the unit that a subtitle starts with, such as “pt” for
/// “Points” or “Points above the baseline”.
fn unit_symbol(subtitle: &str) -> Option<String> {
    use crate::i18n::tr;
    let units = [
        (tr("Points"), tr("pt")),
        (tr("Centimeters"), tr("cm")),
        (tr("Inches"), tr("in")),
        (tr("Picas"), tr("pc")),
        (tr("Percent"), "%".to_owned()),
        (tr("Degrees"), "°".to_owned()),
        (tr("Seconds"), tr("s")),
    ];
    units.into_iter().find_map(|(name, symbol)| {
        let rest = subtitle.strip_prefix(name.as_str())?;
        (rest.is_empty() || rest.starts_with([' ', ','])).then_some(symbol)
    })
}

/// What every row has: its line, the title, and the subtitle.
#[derive(Clone)]
struct Line {
    root: gtk::Box,
    title: gtk::Label,
    subtitle: gtk::Label,
}

impl Line {
    fn new(title: &str) -> Self {
        let root = gtk::Box::builder()
            .spacing(8)
            .margin_start(10)
            .margin_end(10)
            .margin_top(3)
            .margin_bottom(3)
            .accessible_role(gtk::AccessibleRole::Group)
            .css_classes(["desk-compact-row"])
            .build();
        let title_label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(16)
            .build();
        let subtitle = gtk::Label::builder()
            .xalign(0.0)
            .visible(false)
            .css_classes(["dim-label", "caption"])
            .build();
        let line = Self {
            root,
            title: title_label,
            subtitle,
        };
        line.set_title(title);
        line
    }

    /// Sets the title, which is markup as in Libadwaita.
    fn set_title(&self, title: &str) {
        self.title.set_markup(title);
        self.root
            .update_property(&[gtk::accessible::Property::Label(&self.title.text())]);
    }
}

/// The methods every row has, as the Libadwaita rows have them.
macro_rules! row_methods {
    ($type:ty) => {
        impl $type {
            /// The line of the row, to put in a box.
            pub fn widget(&self) -> &gtk::Box {
                &self.line.root
            }

            /// Sets the title.
            pub fn set_title(&self, title: &str) {
                self.line.set_title(title);
            }

            /// The title, as text.
            pub fn title(&self) -> glib::GString {
                self.line.title.text()
            }

            /// The subtitle.
            pub fn subtitle(&self) -> Option<glib::GString> {
                Some(self.line.subtitle.label())
            }

            /// Shows or hides the row.
            pub fn set_visible(&self, visible: bool) {
                self.line.root.set_visible(visible);
            }

            /// Whether the row shows.
            pub fn is_visible(&self) -> bool {
                self.line.root.is_visible()
            }

            /// Lets the row be used, or not.
            pub fn set_sensitive(&self, sensitive: bool) {
                self.line.root.set_sensitive(sensitive);
            }

            /// Whether the row can be used.
            pub fn is_sensitive(&self) -> bool {
                self.line.root.is_sensitive()
            }

            /// Sets the tooltip of the whole row.
            pub fn set_tooltip_text(&self, text: Option<&str>) {
                self.line.root.set_tooltip_text(text);
            }

            /// Adds a style class to the row.
            pub fn add_css_class(&self, class: &str) {
                self.line.root.add_css_class(class);
            }

            /// Removes a style class from the row.
            pub fn remove_css_class(&self, class: &str) {
                self.line.root.remove_css_class(class);
            }

            /// The line of the row as a widget.
            pub fn upcast_ref(&self) -> &gtk::Widget {
                self.line.root.upcast_ref()
            }

            /// The window of the row, once it is in one.
            pub fn root(&self) -> Option<gtk::Root> {
                self.line.root.root()
            }
        }

        impl Row for $type {
            fn row_widget(&self) -> &gtk::Widget {
                self.line.root.upcast_ref()
            }

            fn title_label(&self) -> Option<&gtk::Label> {
                Some(&self.line.title)
            }
        }
    };
}

/// Widgets after the control of a row, such as a button.
macro_rules! row_suffixes {
    ($type:ty) => {
        impl $type {
            /// Puts a widget at the end of the row.
            pub fn add_suffix(&self, widget: &impl IsA<gtk::Widget>) {
                widget.as_ref().set_valign(gtk::Align::Center);
                self.line.root.append(widget);
            }

            /// Puts a widget at the start of the row.
            pub fn add_prefix(&self, widget: &impl IsA<gtk::Widget>) {
                widget.as_ref().set_valign(gtk::Align::Center);
                self.line.root.prepend(widget);
            }
        }
    };
}

/// A titled group of rows, in a card.
#[derive(Clone)]
pub struct Group {
    root: gtk::Box,
    header: gtk::Box,
    title: gtk::Label,
    description: gtk::Label,
    card: gtk::Box,
    labels: gtk::SizeGroup,
}

impl Default for Group {
    fn default() -> Self {
        Self::new()
    }
}

impl Group {
    /// An empty group with no title.
    pub fn new() -> Self {
        let title = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .visible(false)
            .css_classes(["heading"])
            .build();
        let header = gtk::Box::builder().spacing(6).build();
        header.append(&title);
        let description = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .visible(false)
            .css_classes(["dim-label", "caption"])
            .build();
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["card", "desk-compact-group"])
            .build();
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .accessible_role(gtk::AccessibleRole::Group)
            .build();
        root.append(&header);
        root.append(&description);
        root.append(&card);
        Self {
            root,
            header,
            title,
            description,
            card,
            labels: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
        }
    }

    /// A builder of a group.
    pub fn builder() -> GroupBuilder {
        GroupBuilder::default()
    }

    /// The group, to put in a box.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Adds a row at the end.
    pub fn add(&self, row: &impl Row) {
        self.card.append(row.row_widget());
        if let Some(label) = row.title_label() {
            self.labels.add_widget(label);
        }
        self.card.set_visible(true);
    }

    /// Adds any widget at the end, as a row of its own.
    pub fn add_widget(&self, widget: &impl IsA<gtk::Widget>) {
        self.card.append(widget);
    }

    /// Takes out a row.
    pub fn remove(&self, row: &impl Row) {
        if let Some(label) = row.title_label() {
            self.labels.remove_widget(label);
        }
        self.card.remove(row.row_widget());
    }

    /// Sets the title.
    /// Sets the title, which is markup as in Libadwaita.
    pub fn set_title(&self, title: &str) {
        self.title.set_markup(title);
        self.title.set_visible(!title.is_empty());
        self.root
            .update_property(&[gtk::accessible::Property::Label(&self.title.text())]);
    }

    /// The title, as text.
    pub fn title(&self) -> glib::GString {
        self.title.text()
    }

    /// Sets the text under the title.
    pub fn set_description(&self, description: Option<&str>) {
        let text = description.unwrap_or_default();
        self.description.set_label(text);
        self.description.set_visible(!text.is_empty());
    }

    /// Puts a widget at the end of the title.
    pub fn set_header_suffix(&self, suffix: Option<&impl IsA<gtk::Widget>>) {
        if let Some(suffix) = suffix {
            self.header.append(suffix);
        }
    }

    /// Shows or hides the group.
    pub fn set_visible(&self, visible: bool) {
        self.root.set_visible(visible);
    }

    /// Whether the group shows.
    pub fn is_visible(&self) -> bool {
        self.root.is_visible()
    }

    /// Lets the group be used, or not.
    pub fn set_sensitive(&self, sensitive: bool) {
        self.root.set_sensitive(sensitive);
    }

    /// The window of the group, once it is in one.
    pub fn root(&self) -> Option<gtk::Root> {
        self.root.root()
    }

    /// Calls `f` when the group shows or hides.
    pub fn connect_visible_notify(&self, f: impl Fn(&gtk::Box) + 'static) -> glib::SignalHandlerId {
        self.root.connect_visible_notify(f)
    }
}

/// Builds a [`Group`].
#[derive(Default)]
pub struct GroupBuilder {
    title: Option<String>,
    description: Option<String>,
    header_suffix: Option<gtk::Widget>,
    visible: Option<bool>,
}

impl GroupBuilder {
    /// The title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The text under the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// A widget at the end of the title.
    pub fn header_suffix(mut self, suffix: &impl IsA<gtk::Widget>) -> Self {
        self.header_suffix = Some(suffix.clone().upcast());
        self
    }

    /// Whether the group shows.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    /// The group.
    pub fn build(self) -> Group {
        let group = Group::new();
        group.set_header_suffix(self.header_suffix.as_ref());
        if let Some(title) = self.title {
            group.set_title(&title);
        }
        group.set_description(self.description.as_deref());
        if let Some(visible) = self.visible {
            group.set_visible(visible);
        }
        group
    }
}

/// A row with a number: its title, the spin button, and the unit.
#[derive(Clone)]
pub struct SpinRow {
    line: Line,
    spin: gtk::SpinButton,
}

row_methods!(SpinRow);
row_suffixes!(SpinRow);

impl SpinRow {
    /// A row for numbers from `min` to `max` in steps of `step`.
    pub fn with_range(min: f64, max: f64, step: f64) -> Self {
        let spin = gtk::SpinButton::with_range(min, max, step);
        spin.set_width_chars(6);
        spin.set_valign(gtk::Align::Center);
        let line = Line::new("");
        line.root.append(&line.title);
        line.root.append(&spin);
        spin.update_relation(&[gtk::accessible::Relation::LabelledBy(&[line
            .title
            .upcast_ref()])]);
        line.subtitle.set_valign(gtk::Align::Center);
        line.root.append(&line.subtitle);
        Self { line, spin }
    }

    /// Sets what the value measures. A unit shows after the value as its
    /// symbol, as in the fields of Pages, and the whole text is the
    /// tooltip and the description of the spin button.
    pub fn set_subtitle(&self, subtitle: &str) {
        let symbol = unit_symbol(subtitle);
        let shown = symbol.as_deref().unwrap_or(subtitle);
        self.line.subtitle.set_label(shown);
        self.line.subtitle.set_visible(!shown.is_empty());
        let explained = (!subtitle.is_empty() && shown != subtitle).then_some(subtitle);
        self.line.root.set_tooltip_text(explained);
        self.spin
            .update_property(&[gtk::accessible::Property::Description(subtitle)]);
    }

    /// Sets the title and gives the spin button the same name.
    pub fn set_title_and_name(&self, title: &str) {
        self.set_title(title);
    }

    /// The spin button.
    pub fn spin(&self) -> &gtk::SpinButton {
        &self.spin
    }
}

impl Deref for SpinRow {
    type Target = gtk::SpinButton;

    fn deref(&self) -> &gtk::SpinButton {
        &self.spin
    }
}

/// A row that turns something on or off.
#[derive(Clone)]
pub struct SwitchRow {
    line: Line,
    switch: gtk::Switch,
}

row_methods!(SwitchRow);
row_suffixes!(SwitchRow);

impl SwitchRow {
    /// A row with no title, off.
    pub fn new() -> Self {
        let switch = gtk::Switch::builder()
            .valign(gtk::Align::Center)
            .halign(gtk::Align::End)
            .hexpand(true)
            .build();
        let line = Line::new("");
        line.root.append(&line.title);
        line.root.append(&switch);
        switch.update_relation(&[gtk::accessible::Relation::LabelledBy(&[line
            .title
            .upcast_ref()])]);
        Self { line, switch }
    }

    /// A builder of a row.
    pub fn builder() -> SwitchRowBuilder {
        SwitchRowBuilder::default()
    }

    /// Sets the text that explains the row, as its tooltip.
    pub fn set_subtitle(&self, subtitle: &str) {
        self.line.subtitle.set_label(subtitle);
        self.line
            .root
            .set_tooltip_markup((!subtitle.is_empty()).then_some(subtitle));
    }
}

impl Default for SwitchRow {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for SwitchRow {
    type Target = gtk::Switch;

    fn deref(&self) -> &gtk::Switch {
        &self.switch
    }
}

/// Builds a [`SwitchRow`].
#[derive(Default)]
pub struct SwitchRowBuilder {
    title: Option<String>,
    subtitle: Option<String>,
    active: Option<bool>,
    visible: Option<bool>,
}

impl SwitchRowBuilder {
    /// The title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The text that explains the row.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Whether it is on.
    pub fn active(mut self, active: bool) -> Self {
        self.active = Some(active);
        self
    }

    /// Whether the row shows.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    /// The row.
    pub fn build(self) -> SwitchRow {
        let row = SwitchRow::new();
        if let Some(title) = self.title {
            row.set_title(&title);
        }
        if let Some(subtitle) = self.subtitle {
            row.set_subtitle(&subtitle);
        }
        if let Some(active) = self.active {
            row.set_active(active);
        }
        if let Some(visible) = self.visible {
            row.set_visible(visible);
        }
        row
    }
}

/// A row that chooses one of a list.
#[derive(Clone)]
pub struct ComboRow {
    line: Line,
    dropdown: gtk::DropDown,
}

row_methods!(ComboRow);
row_suffixes!(ComboRow);

impl ComboRow {
    /// A row with an empty list.
    pub fn new() -> Self {
        let dropdown = gtk::DropDown::builder()
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();
        let line = Line::new("");
        line.root.append(&line.title);
        line.root.append(&dropdown);
        dropdown.update_relation(&[gtk::accessible::Relation::LabelledBy(&[line
            .title
            .upcast_ref()])]);
        Self { line, dropdown }
    }

    /// A builder of a row.
    pub fn builder() -> ComboRowBuilder {
        ComboRowBuilder::default()
    }

    /// Sets the text that explains the row, as its tooltip.
    pub fn set_subtitle(&self, subtitle: &str) {
        self.line.subtitle.set_label(subtitle);
        self.line
            .root
            .set_tooltip_markup((!subtitle.is_empty()).then_some(subtitle));
    }
}

impl Default for ComboRow {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for ComboRow {
    type Target = gtk::DropDown;

    fn deref(&self) -> &gtk::DropDown {
        &self.dropdown
    }
}

/// Builds a [`ComboRow`].
#[derive(Default)]
pub struct ComboRowBuilder {
    title: Option<String>,
    subtitle: Option<String>,
    model: Option<gio::ListModel>,
    selected: Option<u32>,
    enable_search: Option<bool>,
    expression: Option<gtk::Expression>,
    visible: Option<bool>,
}

impl ComboRowBuilder {
    /// The title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The text that explains the row.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// The choices.
    pub fn model(mut self, model: &impl IsA<gio::ListModel>) -> Self {
        self.model = Some(model.clone().upcast());
        self
    }

    /// The choice made.
    pub fn selected(mut self, selected: u32) -> Self {
        self.selected = Some(selected);
        self
    }

    /// Whether the list can be searched.
    pub fn enable_search(mut self, enable: bool) -> Self {
        self.enable_search = Some(enable);
        self
    }

    /// How a choice is named, for the search.
    pub fn expression(mut self, expression: impl AsRef<gtk::Expression>) -> Self {
        self.expression = Some(expression.as_ref().clone());
        self
    }

    /// Kept for the API of Libadwaita; the drop-down shows the names.
    pub fn use_subtitle(self, _use: bool) -> Self {
        self
    }

    /// Whether the row shows.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    /// The row.
    pub fn build(self) -> ComboRow {
        let row = ComboRow::new();
        if let Some(title) = self.title {
            row.set_title(&title);
        }
        if let Some(subtitle) = self.subtitle {
            row.set_subtitle(&subtitle);
        }
        if let Some(expression) = self.expression {
            row.set_expression(Some(&expression));
        }
        if let Some(model) = self.model {
            row.set_model(Some(&model));
        }
        if let Some(selected) = self.selected {
            row.set_selected(selected);
        }
        if let Some(enable) = self.enable_search {
            row.set_enable_search(enable);
        }
        if let Some(visible) = self.visible {
            row.set_visible(visible);
        }
        row.dropdown
            .update_property(&[gtk::accessible::Property::Label(&row.title())]);
        row
    }
}

/// A row with a title and widgets after it, such as buttons.
#[derive(Clone)]
pub struct ActionRow {
    line: Line,
    prefixes: gtk::Box,
    suffixes: gtk::Box,
}

row_methods!(ActionRow);

impl ActionRow {
    /// A row with no title.
    pub fn new() -> Self {
        let line = Line::new("");
        let prefixes = gtk::Box::builder().spacing(6).visible(false).build();
        let titles = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .build();
        titles.append(&line.title);
        titles.append(&line.subtitle);
        let suffixes = gtk::Box::builder()
            .spacing(6)
            .hexpand(true)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Center)
            .build();
        line.root.append(&prefixes);
        line.root.append(&titles);
        line.root.append(&suffixes);
        Self {
            line,
            prefixes,
            suffixes,
        }
    }

    /// A builder of a row.
    pub fn builder() -> ActionRowBuilder {
        ActionRowBuilder::default()
    }

    /// Sets the caption under the title, which is markup as in
    /// Libadwaita.
    pub fn set_subtitle(&self, subtitle: &str) {
        self.line.subtitle.set_markup(subtitle);
        self.line.subtitle.set_visible(!subtitle.is_empty());
    }

    /// Puts a widget after the title.
    pub fn add_suffix(&self, widget: &impl IsA<gtk::Widget>) {
        self.suffixes.append(widget);
    }

    /// Puts a widget before the title.
    pub fn add_prefix(&self, widget: &impl IsA<gtk::Widget>) {
        self.prefixes.append(widget);
        self.prefixes.set_visible(true);
    }

    /// Makes a click on the title activate a widget of the row.
    pub fn set_activatable_widget(&self, widget: Option<&impl IsA<gtk::Widget>>) {
        let Some(widget) = widget else { return };
        // The control is named by the row, as in Libadwaita.
        widget
            .as_ref()
            .update_relation(&[gtk::accessible::Relation::LabelledBy(&[self
                .line
                .title
                .upcast_ref()])]);
        let target = widget.as_ref().downgrade();
        let click = gtk::GestureClick::new();
        click.connect_released(move |gesture, _, _, _| {
            if let Some(target) = target.upgrade() {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                target.activate();
            }
        });
        self.line.title.add_controller(click);
    }

    /// Calls `f` when the row is clicked.
    pub fn connect_activated(&self, f: impl Fn(&Self) + 'static) {
        let this = self.clone();
        let click = gtk::GestureClick::new();
        click.connect_released(move |_, _, _, _| f(&this));
        self.line.root.add_controller(click);
    }
}

impl Default for ActionRow {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds an [`ActionRow`].
#[derive(Default)]
pub struct ActionRowBuilder {
    title: Option<String>,
    subtitle: Option<String>,
    activatable_widget: Option<gtk::Widget>,
    visible: Option<bool>,
}

impl ActionRowBuilder {
    /// The title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The caption under the title.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Kept for the API of Libadwaita.
    pub fn activatable(self, _activatable: bool) -> Self {
        self
    }

    /// The widget that a click on the title activates.
    pub fn activatable_widget(mut self, widget: &impl IsA<gtk::Widget>) -> Self {
        self.activatable_widget = Some(widget.clone().upcast());
        self
    }

    /// Whether the row shows.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    /// The row.
    pub fn build(self) -> ActionRow {
        let row = ActionRow::new();
        if let Some(title) = self.title {
            row.set_title(&title);
        }
        if let Some(subtitle) = self.subtitle {
            row.set_subtitle(&subtitle);
        }
        row.set_activatable_widget(self.activatable_widget.as_ref());
        if let Some(visible) = self.visible {
            row.set_visible(visible);
        }
        row
    }
}

/// A row to type text in.
#[derive(Clone)]
pub struct EntryRow {
    line: Line,
    entry: gtk::Entry,
}

row_methods!(EntryRow);
row_suffixes!(EntryRow);

impl EntryRow {
    /// A row with no title.
    pub fn new() -> Self {
        let entry = gtk::Entry::builder()
            .valign(gtk::Align::Center)
            .hexpand(true)
            .width_chars(8)
            .build();
        let line = Line::new("");
        line.root.append(&line.title);
        line.root.append(&entry);
        entry.update_relation(&[gtk::accessible::Relation::LabelledBy(&[line
            .title
            .upcast_ref()])]);
        Self { line, entry }
    }

    /// A builder of a row.
    pub fn builder() -> EntryRowBuilder {
        EntryRowBuilder::default()
    }

    /// Sets the text that explains the row, as its tooltip.
    pub fn set_subtitle(&self, subtitle: &str) {
        self.line.subtitle.set_label(subtitle);
        self.line
            .root
            .set_tooltip_markup((!subtitle.is_empty()).then_some(subtitle));
    }

    /// Calls `f` when the text is confirmed with Enter or by leaving the
    /// entry, as the apply button of Libadwaita does.
    pub fn connect_apply(&self, f: impl Fn(&Self) + 'static) {
        let f = std::rc::Rc::new(f);
        let this = self.clone();
        let apply = f.clone();
        self.entry.connect_activate(move |_| apply(&this));
        let this = self.clone();
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave(move |_| f(&this));
        self.entry.add_controller(focus);
    }
}

impl Default for EntryRow {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for EntryRow {
    type Target = gtk::Entry;

    fn deref(&self) -> &gtk::Entry {
        &self.entry
    }
}

/// Builds an [`EntryRow`].
#[derive(Default)]
pub struct EntryRowBuilder {
    title: Option<String>,
    text: Option<String>,
    visible: Option<bool>,
}

impl EntryRowBuilder {
    /// The title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The text.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Kept for the API of Libadwaita: the text applies with Enter.
    pub fn show_apply_button(self, _show: bool) -> Self {
        self
    }

    /// Whether the row shows.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    /// The row.
    pub fn build(self) -> EntryRow {
        let row = EntryRow::new();
        if let Some(title) = self.title {
            row.set_title(&title);
            row.entry
                .update_property(&[gtk::accessible::Property::Label(&title)]);
        }
        if let Some(text) = self.text {
            row.set_text(&text);
        }
        if let Some(visible) = self.visible {
            row.set_visible(visible);
        }
        row
    }
}

/// A row that opens to show more rows.
#[derive(Clone)]
pub struct ExpanderRow {
    line: Line,
    expander: gtk::Expander,
    suffixes: gtk::Box,
    rows: gtk::Box,
    labels: gtk::SizeGroup,
}

row_methods!(ExpanderRow);

impl ExpanderRow {
    /// A closed row with no title.
    pub fn new() -> Self {
        let line = Line::new("");
        let header = gtk::Box::builder().spacing(6).build();
        let titles = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        titles.append(&line.title);
        titles.append(&line.subtitle);
        let suffixes = gtk::Box::builder().spacing(6).build();
        header.append(&titles);
        header.append(&suffixes);
        let rows = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .margin_top(3)
            .build();
        let expander = gtk::Expander::builder()
            .label_widget(&header)
            .child(&rows)
            .hexpand(true)
            .build();
        line.root.append(&expander);
        Self {
            line,
            expander,
            suffixes,
            rows,
            labels: gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal),
        }
    }

    /// A builder of a row.
    pub fn builder() -> ExpanderRowBuilder {
        ExpanderRowBuilder::default()
    }

    /// Sets the caption under the title, which is markup as in
    /// Libadwaita.
    pub fn set_subtitle(&self, subtitle: &str) {
        self.line.subtitle.set_markup(subtitle);
        self.line.subtitle.set_visible(!subtitle.is_empty());
    }

    /// Adds a row inside.
    pub fn add_row(&self, row: &impl Row) {
        let widget = row.row_widget();
        widget.set_margin_start(0);
        widget.set_margin_end(0);
        self.rows.append(widget);
        if let Some(label) = row.title_label() {
            self.labels.add_widget(label);
        }
    }

    /// Takes out a row.
    pub fn remove(&self, row: &impl Row) {
        if let Some(label) = row.title_label() {
            self.labels.remove_widget(label);
        }
        self.rows.remove(row.row_widget());
    }

    /// Puts a widget at the end of the title.
    pub fn add_suffix(&self, widget: &impl IsA<gtk::Widget>) {
        self.suffixes.append(widget);
    }

    /// Opens or closes the row.
    pub fn set_expanded(&self, expanded: bool) {
        self.expander.set_expanded(expanded);
    }

    /// Whether the row is open.
    pub fn is_expanded(&self) -> bool {
        self.expander.is_expanded()
    }

    /// Calls `f` when the row opens or closes.
    pub fn connect_expanded_notify(&self, f: impl Fn(&gtk::Expander) + 'static) {
        self.expander.connect_expanded_notify(f);
    }
}

impl Default for ExpanderRow {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds an [`ExpanderRow`].
#[derive(Default)]
pub struct ExpanderRowBuilder {
    title: Option<String>,
    subtitle: Option<String>,
    expanded: Option<bool>,
}

impl ExpanderRowBuilder {
    /// The title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The caption under the title.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Whether it starts open.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    /// The row.
    pub fn build(self) -> ExpanderRow {
        let row = ExpanderRow::new();
        if let Some(title) = self.title {
            row.set_title(&title);
        }
        if let Some(subtitle) = self.subtitle {
            row.set_subtitle(&subtitle);
        }
        if let Some(expanded) = self.expanded {
            row.set_expanded(expanded);
        }
        row
    }
}

/// Lets a plain widget be a row of a group, with no label to line up.
impl Row for gtk::Widget {
    fn row_widget(&self) -> &gtk::Widget {
        self
    }

    fn title_label(&self) -> Option<&gtk::Label> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() -> bool {
        gtk::init().is_ok()
    }

    #[test]
    fn rows_line_up_and_keep_their_values() {
        if !init() {
            return;
        }
        let group = Group::builder().title("Spacing").build();
        let spin = SpinRow::with_range(0.0, 10.0, 0.5);
        spin.set_title("Before Paragraph");
        spin.set_subtitle("Points");
        spin.set_value(4.5);
        let switch = SwitchRow::builder().title("Shadow").active(true).build();
        group.add(&spin);
        group.add(&switch);
        assert_eq!(spin.value(), 4.5);
        assert!(switch.is_active());
        assert_eq!(group.title(), "Spacing");
        assert_eq!(group.labels.widgets().len(), 2);
        // A row is one line, much lower than a row of Libadwaita.
        let (_, natural, _, _) = spin.widget().measure(gtk::Orientation::Vertical, -1);
        assert!(natural < 44, "{natural}");
    }
}
