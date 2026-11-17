// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bars of Desk Tables around the canvas and what the cells format
//! with: the Cell and Text tabs of the Format pane, which hold what the
//! format bar of interface B had (NINT-08, step 2 of window M+), the
//! formula bar under the header bar, and the bottom bar with the zoom, the
//! sheet in sight and the instant calculations (NINT-04, NGNM-13).
//!
//! The controls that set the format of cells, the content of the formula
//! bar and the results of the instant calculations work on the cells of
//! the table engine, and stay off until a sheet holds them (NINT-10 and
//! NINT-13 in phase 4 of Desk Tables).

use adw::prelude::*;
use gtk::gio;

use crate::formatting;
use crate::geometry;
use crate::i18n::{tr, trn};

/// What the format bar of the cells held in interface B (NINT-08), as the
/// Cell and Text tabs of the Format pane of window M+ (step 2): the data
/// format, the decimals, the fill, the borders, wrapping and merging in
/// Cell; the font, its size and style, the text color and the alignment
/// in Text. The controls stay off until a sheet holds the cells (NINT-10
/// and NINT-13 in phase 4 of Desk Tables), as they were on the bar.
pub(super) struct CellPanes {
    pub cell: gtk::Box,
    pub text: gtk::Box,
}

impl CellPanes {
    pub fn new() -> Self {
        // Cell: what the cells hold and how they look.
        let format_label = adw::ButtonContent::builder()
            .icon_name("desk-number-format-symbolic")
            .label(tr("Number"))
            .build();
        let format = gtk::MenuButton::builder()
            .child(&format_label)
            .menu_model(&data_format_menu())
            .valign(gtk::Align::Center)
            .build();
        formatting::labelled(&format, &tr("Data Format"));
        let decimals = gtk::Box::builder()
            .css_classes(["linked"])
            .valign(gtk::Align::Center)
            .build();
        decimals.append(&button(
            "value-decrease-symbolic",
            &tr("Fewer Decimals"),
            "cell.fewer-decimals",
        ));
        decimals.append(&button(
            "value-increase-symbolic",
            &tr("More Decimals"),
            "cell.more-decimals",
        ));
        let data = adw::PreferencesGroup::builder()
            .title(tr("Data Format"))
            .build();
        data.add(&row(&tr("Format"), &format));
        data.add(&row(&tr("Decimals"), &decimals));
        let looks = adw::PreferencesGroup::builder()
            .title(tr("Fill and Borders"))
            .build();
        looks.add(&row(
            &tr("Cell Fill"),
            &button("desk-fill-symbolic", &tr("Cell Fill"), "cell.fill"),
        ));
        looks.add(&row(
            &tr("Cell Borders"),
            &button("desk-borders-symbolic", &tr("Cell Borders"), "cell.borders"),
        ));
        let more = gtk::MenuButton::builder()
            .icon_name("view-more-horizontal-symbolic")
            .menu_model(&more_menu())
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        formatting::labelled(&more, &tr("More Cell Options"));
        let cells = adw::PreferencesGroup::builder()
            .title(tr("Cells"))
            .header_suffix(&more)
            .build();
        for (title, action) in [
            (tr("Wrap Text in Cell"), "cell.wrap"),
            (tr("Merge Cells"), "cell.merge"),
        ] {
            let go = gtk::Button::builder()
                .icon_name("go-next-symbolic")
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            go.set_detailed_action_name(action);
            formatting::labelled(&go, &title);
            let line = adw::ActionRow::builder()
                .title(title)
                .activatable_widget(&go)
                .build();
            line.add_suffix(&go);
            cells.add(&line);
        }
        let cell = page();
        cell.append(&data);
        cell.append(&looks);
        cell.append(&cells);

        // Text: the font of the cells and its alignment.
        let font = gtk::MenuButton::builder()
            .label("Adwaita Sans")
            .valign(gtk::Align::Center)
            .sensitive(false)
            .build();
        formatting::labelled(&font, &tr("Font"));
        let size = gtk::SpinButton::with_range(1.0, 999.0, 1.0);
        size.set_value(10.0);
        size.set_sensitive(false);
        size.set_valign(gtk::Align::Center);
        formatting::labelled(&size, &tr("Font Size"));
        let styles = gtk::Box::builder()
            .css_classes(["linked"])
            .valign(gtk::Align::Center)
            .build();
        for (icon, label, action) in [
            ("format-text-bold-symbolic", tr("Bold"), "cell.bold"),
            ("format-text-italic-symbolic", tr("Italic"), "cell.italic"),
            (
                "format-text-underline-symbolic",
                tr("Underline"),
                "cell.underline",
            ),
        ] {
            styles.append(&button(icon, &label, action));
        }
        let font_group = adw::PreferencesGroup::builder().title(tr("Font")).build();
        font_group.add(&row(&tr("Family"), &font));
        font_group.add(&row(&tr("Size"), &size));
        font_group.add(&row(&tr("Style"), &styles));
        font_group.add(&row(
            &tr("Text Color"),
            &button("desk-text-color-symbolic", &tr("Text Color"), "cell.color"),
        ));
        let aligns = gtk::Box::builder()
            .css_classes(["linked"])
            .valign(gtk::Align::Center)
            .build();
        for (icon, label, value) in [
            ("format-justify-left-symbolic", tr("Align Left"), "left"),
            ("format-justify-center-symbolic", tr("Center"), "center"),
            ("format-justify-right-symbolic", tr("Align Right"), "right"),
            ("format-justify-fill-symbolic", tr("Justify"), "justify"),
        ] {
            aligns.append(&button(icon, &label, &format!("cell.align('{value}')")));
        }
        let alignment = adw::PreferencesGroup::builder()
            .title(tr("Alignment"))
            .build();
        alignment.add(&row(&tr("Horizontal"), &aligns));
        let text = page();
        text.append(&font_group);
        text.append(&alignment);
        Self { cell, text }
    }
}

/// A row of the Format pane with its control at the end.
fn row(title: &str, control: &impl IsA<gtk::Widget>) -> adw::ActionRow {
    let line = adw::ActionRow::builder().title(title).build();
    line.add_suffix(control);
    line
}

/// A column of groups, as a tab of the Inspector holds them.
fn page() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build()
}

/// A flat button of the Format pane with a tooltip and an accessible name.
fn button(icon: &str, label: &str, action: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(label)
        .valign(gtk::Align::Center)
        .build();
    button.set_detailed_action_name(action);
    formatting::labelled(&button, label);
    button
}

/// The data formats of a cell, as the format bar of Numbers had them.
fn data_format_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let common = gio::Menu::new();
    for (label, value) in [
        (tr("_Automatic"), "automatic"),
        (tr("_Number"), "number"),
        (tr("_Currency"), "currency"),
        (tr("_Percentage"), "percentage"),
        (tr("Check_box"), "checkbox"),
    ] {
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("cell.data-format"), Some(&value.to_variant()));
        common.append_item(&item);
    }
    menu.append_section(None, &common);
    let more = gio::Menu::new();
    for (label, value) in [
        (tr("_Date and Time"), "date"),
        (tr("D_uration"), "duration"),
        (tr("_Fraction"), "fraction"),
        (tr("_Scientific"), "scientific"),
        (tr("_Text"), "text"),
    ] {
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("cell.data-format"), Some(&value.to_variant()));
        more.append_item(&item);
    }
    menu.append_submenu(Some(&tr("_More Formats")), &more);
    menu
}

/// The More menu of the Cells group, which Search Commands finds too.
fn more_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let cells = gio::Menu::new();
    cells.append(Some(&tr("_Wrap Text in Cell")), Some("cell.wrap"));
    cells.append(Some(&tr("_Merge Cells")), Some("cell.merge"));
    menu.append_section(None, &cells);
    let style = gio::Menu::new();
    style.append(Some(&tr("_Copy Style")), Some("cell.copy-style"));
    style.append(Some(&tr("_Paste Style")), Some("cell.paste-style"));
    menu.append_section(None, &style);
    let table = gio::Menu::new();
    table.append(Some(&tr("Show Table _Name")), Some("win.show-table-name"));
    table.append(
        Some(&tr("Show in _Inspector")),
        Some("win.show-in-inspector"),
    );
    menu.append_section(None, &table);
    menu
}

/// The functions of the Function button of the formula bar, and of the
/// Insert menu.
pub(super) fn function_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let common = gio::Menu::new();
    for (label, value) in [
        (tr("_Sum"), "sum"),
        (tr("_Average"), "average"),
        (tr("_Minimum"), "min"),
        (tr("Ma_ximum"), "max"),
        (tr("_Count"), "count"),
        (tr("_Product"), "product"),
    ] {
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("cell.function"), Some(&value.to_variant()));
        common.append_item(&item);
    }
    menu.append_section(None, &common);
    let more = gio::Menu::new();
    more.append(Some(&tr("More _Functions…")), Some("cell.function-browser"));
    menu.append_section(None, &more);
    menu
}

/// The formula bar, under the header bar: the cell in focus, the Function
/// button and the content of the cell.
pub(super) struct FormulaBar {
    pub bar: gtk::Box,
    /// The name of the cell in focus, such as "B2", or of the table.
    pub reference: gtk::Label,
}

impl FormulaBar {
    pub fn new() -> Self {
        let bar = gtk::Box::builder()
            .spacing(4)
            .css_classes(["toolbar", "desk-formula-bar"])
            .accessible_role(gtk::AccessibleRole::Toolbar)
            .build();
        formatting::labelled(&bar, &tr("Formula Bar"));
        let reference = gtk::Label::builder()
            .width_chars(6)
            .xalign(0.0)
            .margin_start(6)
            .css_classes(["numeric", "heading"])
            .build();
        reference.set_tooltip_text(Some(&tr("Cell in Focus")));
        bar.append(&reference);
        let function = gtk::MenuButton::builder()
            .icon_name("desk-function-symbolic")
            .menu_model(&function_menu())
            .tooltip_text(tr("Function"))
            .build();
        formatting::labelled(&function, &tr("Function"));
        bar.append(&function);
        let entry = gtk::Entry::builder().hexpand(true).sensitive(false).build();
        formatting::labelled(&entry, &tr("Cell Content"));
        bar.append(&entry);
        Self { bar, reference }
    }

    /// Names the cell in focus, or nothing.
    pub fn show_reference(&self, name: &str) {
        self.reference.set_label(name);
        formatting::labelled(&self.reference, name);
    }
}

/// The names of the instant calculations, in their order (NINT-13).
fn calculation_names() -> [String; 5] {
    [tr("SUM"), tr("AVERAGE"), tr("MIN"), tr("MAX"), tr("COUNT")]
}

/// The bar at the bottom: the zoom at the start, the sheet in sight between
/// arrows in the middle, and at the end the instant calculations and the
/// count of the cells selected (NINT-04, NINT-13, NGNM-13).
pub(super) struct BottomBar {
    pub bar: gtk::ActionBar,
    pub zoom: gtk::MenuButton,
    pub position: gtk::Label,
    /// The instant calculations of the cells selected.
    pub calculations: gtk::Box,
    pub count: gtk::Label,
}

impl BottomBar {
    pub fn new() -> Self {
        let bar = gtk::ActionBar::new();
        let zoom = gtk::MenuButton::builder()
            .label(geometry::zoom_label(geometry::DEFAULT_ZOOM))
            .menu_model(&super::header::zoom_menu())
            .direction(gtk::ArrowType::Up)
            .tooltip_text(tr("Zoom"))
            .css_classes(["flat"])
            .build();
        formatting::labelled(&zoom, &tr("Zoom"));
        bar.pack_start(&zoom);
        let previous = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .action_name("win.previous-sheet")
            .tooltip_text(tr("Previous Sheet"))
            .css_classes(["flat"])
            .build();
        formatting::labelled(&previous, &tr("Previous Sheet"));
        let next = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .action_name("win.next-sheet")
            .tooltip_text(tr("Next Sheet"))
            .css_classes(["flat"])
            .build();
        formatting::labelled(&next, &tr("Next Sheet"));
        let position = gtk::Label::builder()
            .css_classes(["numeric"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let center = gtk::Box::builder().spacing(6).build();
        center.append(&previous);
        center.append(&position);
        center.append(&next);
        bar.set_center_widget(Some(&center));
        let calculations = gtk::Box::builder()
            .spacing(6)
            .sensitive(false)
            .accessible_role(gtk::AccessibleRole::Group)
            .build();
        formatting::labelled(&calculations, &tr("Instant Calculations"));
        for name in calculation_names() {
            let chip = gtk::Box::builder()
                .spacing(5)
                .css_classes(["desk-calc-chip"])
                .build();
            chip.append(
                &gtk::Label::builder()
                    .label(&name)
                    .css_classes(["caption", "dim-label"])
                    .build(),
            );
            chip.append(
                &gtk::Label::builder()
                    .label("–")
                    .css_classes(["caption", "numeric"])
                    .build(),
            );
            calculations.append(&chip);
        }
        let count = gtk::Label::builder()
            .css_classes(["numeric", "heading"])
            .margin_start(6)
            .margin_end(6)
            .build();
        let end = gtk::Box::builder().spacing(6).build();
        end.append(&calculations);
        end.append(&count);
        bar.pack_end(&end);
        Self {
            bar,
            zoom,
            position,
            calculations,
            count,
        }
    }

    /// Shows the sheet in sight: its name, and where it is among them.
    pub fn show_position(&self, name: &str, index: usize, count: usize) {
        let text = position_label(name, index, count);
        self.position.set_label(&text);
        formatting::labelled(&self.position, &text);
    }

    /// Shows how many cells are selected, or nothing.
    pub fn show_count(&self, cells: Option<u64>) {
        let text = cells.map(cells_label).unwrap_or_default();
        self.count.set_label(&text);
        self.count.set_visible(cells.is_some());
        // The calculations show with a selection of cells (NINT-13).
        self.calculations.set_visible(cells.is_some());
    }
}

/// "Despesas (2 de 4)": the sheet in sight and its place.
pub(super) fn position_label(name: &str, index: usize, count: usize) -> String {
    tr("{name} ({n} of {count})")
        .replace("{name}", name)
        .replace("{n}", &(index + 1).to_string())
        .replace("{count}", &count.to_string())
}

/// "6 células".
pub(super) fn cells_label(cells: u64) -> String {
    trn("{n} cell", "{n} cells", cells)
        .replace("{n}", &crate::document_controls::grouped(cells as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ngnm_13_the_bottom_bar_names_the_sheet_and_the_cells() {
        assert_eq!(position_label("Despesas", 1, 4), "Despesas (2 of 4)");
        assert_eq!(cells_label(1), "1 cell");
        assert_eq!(cells_label(6), "6 cells");
    }
}
