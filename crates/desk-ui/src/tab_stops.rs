// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tab stops in the Inspector (FMT-16): the distance between default
//! stops, and each stop with its position, alignment, leader and decimal
//! separator, as the Text inspector of Pages has them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Length, ParaAttrs, ParaFormat, TabAlignment, TabLeader, TabStop, TabStops};

use crate::formatting::{EditorOf, labelled};
use crate::i18n::tr;
use crate::preferences::Units;

/// The alignments in the order of their menu.
const ALIGNMENTS: [TabAlignment; 4] = [
    TabAlignment::Left,
    TabAlignment::Center,
    TabAlignment::Right,
    TabAlignment::Decimal,
];

/// The leaders in the order of their menu.
const LEADERS: [TabLeader; 5] = [
    TabLeader::None,
    TabLeader::Dots,
    TabLeader::Dashes,
    TabLeader::Line,
    TabLeader::Arrows,
];

/// The farthest a stop may be, in points: the width of the largest page.
const MAX_POSITION: f64 = 1584.0;

fn alignment_name(alignment: TabAlignment) -> String {
    match alignment {
        TabAlignment::Left => tr("Left"),
        TabAlignment::Center => tr("Center"),
        TabAlignment::Right => tr("Right"),
        TabAlignment::Decimal => tr("Decimal"),
    }
}

fn leader_name(leader: TabLeader) -> String {
    match leader {
        TabLeader::None => tr("None"),
        TabLeader::Dots => tr("Dots"),
        TabLeader::Dashes => tr("Dashes"),
        TabLeader::Line => tr("Line"),
        TabLeader::Arrows => tr("Arrows"),
    }
}

pub(crate) fn string_list(names: impl IntoIterator<Item = String>) -> gtk::StringList {
    let names: Vec<String> = names.into_iter().collect();
    gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>())
}

/// A length in units, as a field shows it.
pub fn in_units(length: Length, units: Units) -> f64 {
    length.to_pt() / units.points()
}

/// A length from a field in units.
pub fn from_units(value: f64, units: Units) -> Length {
    Length::from_pt(value * units.points())
}

/// Sets a field in units: its range, step and digits.
pub fn set_units(row: &adw::SpinRow, units: Units, min_points: f64, max_points: f64) {
    let adjustment = row.adjustment();
    adjustment.set_lower(min_points / units.points());
    adjustment.set_upper(max_points / units.points());
    adjustment.set_step_increment(units.step());
    adjustment.set_page_increment(units.step() * 10.0);
    row.set_digits(units.digits());
    row.set_subtitle(&units.name());
}

/// The rows of one stop.
struct StopRows {
    expander: adw::ExpanderRow,
    position: adw::SpinRow,
    alignment: adw::ComboRow,
    leader: adw::ComboRow,
    decimal: adw::EntryRow,
}

/// The tab stops of the Inspector.
pub struct TabStopsGroup {
    group: adw::PreferencesGroup,
    default_interval: adw::SpinRow,
    rows: RefCell<Vec<StopRows>>,
    /// The stops shown.
    stops: RefCell<TabStops>,
    units: Cell<Units>,
    editor: EditorOf,
    updating: Cell<bool>,
}

impl TabStopsGroup {
    /// The tab stops of the Inspector of a window.
    pub fn new(editor: EditorOf, units: Units) -> Rc<Self> {
        let group = adw::PreferencesGroup::builder().title(tr("Tabs")).build();
        let add = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text(tr("Add Tab Stop"))
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        labelled(&add, &tr("Add Tab Stop"));
        group.set_header_suffix(Some(&add));
        let default_interval = adw::SpinRow::with_range(0.0, 1.0, 1.0);
        default_interval.set_title(&tr("Default Tabs"));
        group.add(&default_interval);

        let this = Rc::new(Self {
            group,
            default_interval,
            rows: RefCell::new(Vec::new()),
            stops: RefCell::new(TabStops::none()),
            units: Cell::new(units),
            editor,
            updating: Cell::new(false),
        });
        this.set_units(units);

        let weak = Rc::downgrade(&this);
        this.default_interval.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() || row.value() <= 0.0 {
                return;
            }
            let length = from_units(row.value(), this.units.get());
            this.apply(ParaAttrs {
                default_tab_interval: Some(length),
                ..ParaAttrs::default()
            });
        });
        let weak = Rc::downgrade(&this);
        add.connect_clicked(move |_| {
            let Some(this) = weak.upgrade() else { return };
            let stops = this.stops.borrow().clone();
            // One unit past the last stop, or at the first default stop.
            let units = this.units.get();
            let position = stops.stops().last().map_or_else(
                || Length::from_pt(this.default_interval.value() * units.points()),
                |last| last.position + Length::from_pt(units.points()),
            );
            this.set_stops(stops.with(TabStop::left(position)));
        });
        this
    }

    /// The widget to put in the Inspector.
    pub fn widget(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    /// Measures the fields in other units.
    pub fn set_units(&self, units: Units) {
        self.updating.set(true);
        let interval = from_units(self.default_interval.value(), self.units.get());
        self.units.set(units);
        set_units(&self.default_interval, units, 1.0, MAX_POSITION);
        self.default_interval.set_value(in_units(interval, units));
        for rows in self.rows.borrow().iter() {
            let position = from_units(rows.position.value(), self.units.get());
            set_units(&rows.position, units, 0.0, MAX_POSITION);
            rows.position.set_value(in_units(position, units));
        }
        self.updating.set(false);
        let stops = self.stops.borrow().clone();
        self.update_titles(&stops);
    }

    /// Shows the stops of a paragraph.
    pub fn show(self: &Rc<Self>, format: &ParaFormat) {
        self.updating.set(true);
        let units = self.units.get();
        let interval = in_units(format.default_tab_interval, units);
        if (self.default_interval.value() - interval).abs() > 1e-6 {
            self.default_interval.set_value(interval);
        }
        let stops = format.tab_stops.clone();
        if self.rows.borrow().len() != stops.stops().len() {
            self.rebuild(stops.stops().len());
        }
        for (rows, stop) in self.rows.borrow().iter().zip(stops.stops()) {
            let position = in_units(stop.position, units);
            if (rows.position.value() - position).abs() > 1e-6 {
                rows.position.set_value(position);
            }
            let alignment = ALIGNMENTS
                .iter()
                .position(|a| *a == stop.alignment)
                .unwrap_or(0);
            rows.alignment.set_selected(alignment as u32);
            let leader = LEADERS.iter().position(|l| *l == stop.leader).unwrap_or(0);
            rows.leader.set_selected(leader as u32);
            let decimal = stop.decimal.to_string();
            if rows.decimal.text() != decimal {
                rows.decimal.set_text(&decimal);
            }
            rows.decimal
                .set_visible(stop.alignment == TabAlignment::Decimal);
        }
        self.update_titles(&stops);
        *self.stops.borrow_mut() = stops;
        self.updating.set(false);
    }

    fn update_titles(&self, stops: &TabStops) {
        let units = self.units.get();
        for (rows, stop) in self.rows.borrow().iter().zip(stops.stops()) {
            let position = in_units(stop.position, units);
            rows.expander.set_title(&format!(
                "{} {}",
                format_value(position, units.digits()),
                units.symbol()
            ));
            rows.expander.set_subtitle(&alignment_name(stop.alignment));
        }
    }

    /// Makes rows for `count` stops.
    fn rebuild(self: &Rc<Self>, count: usize) {
        for rows in self.rows.borrow_mut().drain(..) {
            self.group.remove(&rows.expander);
        }
        let units = self.units.get();
        for index in 0..count {
            let expander = adw::ExpanderRow::new();
            let position = adw::SpinRow::with_range(0.0, 1.0, 1.0);
            position.set_title(&tr("Position"));
            set_units(&position, units, 0.0, MAX_POSITION);
            let alignment = adw::ComboRow::builder()
                .title(tr("Alignment"))
                .model(&string_list(ALIGNMENTS.map(alignment_name)))
                .build();
            let decimal = adw::EntryRow::builder()
                .title(tr("Decimal Character"))
                .build();
            let leader = adw::ComboRow::builder()
                .title(tr("Leader"))
                .model(&string_list(LEADERS.map(leader_name)))
                .build();
            let remove = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(tr("Remove Tab Stop"))
                .css_classes(["flat"])
                .valign(gtk::Align::Center)
                .build();
            labelled(&remove, &tr("Remove Tab Stop"));
            expander.add_suffix(&remove);
            expander.add_row(&position);
            expander.add_row(&alignment);
            expander.add_row(&decimal);
            expander.add_row(&leader);
            self.group.add(&expander);

            let weak = Rc::downgrade(self);
            position.connect_value_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                let at = from_units(row.value(), this.units.get());
                this.change(index, |s| s.position = at);
            });
            let weak = Rc::downgrade(self);
            alignment.connect_selected_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                let Some(&alignment) = ALIGNMENTS.get(row.selected() as usize) else {
                    return;
                };
                this.change(index, |s| s.alignment = alignment);
            });
            let weak = Rc::downgrade(self);
            leader.connect_selected_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                let Some(&leader) = LEADERS.get(row.selected() as usize) else {
                    return;
                };
                this.change(index, |s| s.leader = leader);
            });
            let weak = Rc::downgrade(self);
            decimal.connect_changed(move |entry| {
                let Some(this) = weak.upgrade() else { return };
                if let Some(c) = entry.text().chars().next() {
                    this.change(index, |s| s.decimal = c);
                }
            });
            let weak = Rc::downgrade(self);
            remove.connect_clicked(move |_| {
                let Some(this) = weak.upgrade() else { return };
                let stops = this.stops.borrow().clone();
                if let Some(stop) = stops.stops().get(index) {
                    this.set_stops(stops.without(stop.position));
                }
            });
            self.rows.borrow_mut().push(StopRows {
                expander,
                position,
                alignment,
                leader,
                decimal,
            });
        }
    }

    /// Changes the stop at `index`.
    fn change(&self, index: usize, f: impl FnOnce(&mut TabStop)) {
        if self.updating.get() {
            return;
        }
        let stops = self.stops.borrow().clone();
        let Some(&old) = stops.stops().get(index) else {
            return;
        };
        let mut stop = old;
        f(&mut stop);
        if stop != old {
            self.set_stops(stops.without(old.position).with(stop));
        }
    }

    fn set_stops(&self, stops: TabStops) {
        self.apply(ParaAttrs {
            tab_stops: Some(stops),
            ..ParaAttrs::default()
        });
    }

    fn apply(&self, attrs: ParaAttrs) {
        if let Some(editor) = (self.editor)() {
            editor.set_paragraph_attrs(&tr("Tab Stops"), &attrs);
        }
    }
}

/// A number with at most `digits` decimals, without trailing zeros.
pub fn format_value(value: f64, digits: u32) -> String {
    let text = format!("{:.*}", digits as usize, value);
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_16_lengths_show_in_the_units_of_the_rulers() {
        let length = Length::from_points(72);
        assert_eq!(in_units(length, Units::Inches), 1.0);
        assert!((in_units(length, Units::Centimeters) - 2.54).abs() < 1e-9);
        assert_eq!(from_units(6.0, Units::Picas), length);
        assert_eq!(format_value(2.5, 2), "2.5");
        assert_eq!(format_value(3.0, 1), "3");
        assert_eq!(format_value(36.0, 0), "36");
    }
}
