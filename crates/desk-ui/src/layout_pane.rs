// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The page, layouts and sections in the Inspector, as the Document and
//! Layout inspectors of Pages have them (EST-01, EST-07, EST-10, EST-16,
//! EST-18): the paper, its orientation and margins, the columns of the
//! layout the cursor is in and the space around it, and the page its
//! section starts on, how its pages are numbered and whether its first
//! and left pages have headers and footers of their own (EST-19).
//!
//! The layout or section changed is the one that holds the cursor: the
//! one that started at a break before it, or the first one of the
//! document.

use crate::compact;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use desk_model::{Columns, Layout, Length, MAX_COLUMNS, PARTS, PageSetup, Section, SectionStart};

use crate::editing::Format;
use crate::formatting::EditorOf;
use crate::i18n::tr;
use crate::preferences::Units;
use crate::tab_stops::{self, string_list};

/// The pages a section may start on, in the order of their menu.
const STARTS: [SectionStart; 3] = [
    SectionStart::AnyPage,
    SectionStart::LeftPage,
    SectionStart::RightPage,
];

/// The sizes of paper of the Paper menu, in points, portrait: the
/// common ones of the page setup of macOS.
const PAPERS: [(i32, i32); 7] = [
    (612, 792),
    (612, 1008),
    (792, 1224),
    (842, 1191),
    (595, 842),
    (420, 595),
    (516, 729),
];

fn paper_names() -> Vec<String> {
    vec![
        tr("US Letter"),
        tr("US Legal"),
        tr("Tabloid"),
        tr("A3"),
        tr("A4"),
        tr("A5"),
        tr("JIS B5"),
        tr("Other Size"),
    ]
}

/// The paper of the menu that a page has, in either orientation, or the
/// index of Other Size.
fn paper_of(setup: &PageSetup) -> usize {
    let (w, h) = (setup.width.to_pt(), setup.height.to_pt());
    let (short, long) = (w.min(h), w.max(h));
    PAPERS
        .iter()
        .position(|&(a, b)| (f64::from(a) - short).abs() < 0.5 && (f64::from(b) - long).abs() < 0.5)
        .unwrap_or(PAPERS.len())
}

/// A page of a paper of the menu, keeping its orientation and margins.
pub fn with_paper(setup: &PageSetup, paper: usize) -> PageSetup {
    let Some(&(w, h)) = PAPERS.get(paper) else {
        return *setup;
    };
    let (w, h) = (Length::from_points(w), Length::from_points(h));
    let (width, height) = if setup.is_landscape() { (h, w) } else { (w, h) };
    PageSetup {
        width,
        height,
        ..*setup
    }
}

/// The width of the text of a layout, in points.
pub(crate) fn layout_width(setup: &PageSetup, layout: &Layout) -> f64 {
    (setup.width - setup.margin_left - setup.margin_right - layout.left - layout.right)
        .to_pt()
        .max(1.0)
}

/// The gap between the columns of a layout, in points.
pub fn gap(setup: &PageSetup, layout: &Layout) -> f64 {
    f64::from(layout.columns.gaps[0]) / f64::from(PARTS) * layout_width(setup, layout)
}

/// A layout with `count` columns as wide as each other, `gap` points
/// apart.
pub fn with_columns(setup: &PageSetup, layout: &Layout, count: u8, gap: f64) -> Layout {
    let parts = (gap / layout_width(setup, layout)).clamp(0.0, 0.5) * f64::from(PARTS);
    Layout {
        columns: Columns::equal(count, parts.round() as u32),
        ..*layout
    }
}

/// The width of a column, or the gap after it, set to `points`, as the
/// table of columns of Pages' Layout inspector sets them: the columns
/// stop being as wide as each other, and the column after it, or the one
/// before the last, takes up the difference, keeping at least
/// [`MIN_COLUMN`] points.
pub fn set_column(
    setup: &PageSetup,
    layout: &Layout,
    index: usize,
    gap: bool,
    points: f64,
) -> Layout {
    let width = layout_width(setup, layout);
    let columns = layout.columns;
    let count = usize::from(columns.count.clamp(1, MAX_COLUMNS as u8));
    if index >= count || (gap && index + 1 >= count) || count < 2 {
        return *layout;
    }
    // Uneven, from what the columns are now.
    let spans = columns.spans(width);
    let mut widths: Vec<f64> = spans.iter().map(|(_, w)| *w).collect();
    let mut gaps: Vec<f64> = (0..count)
        .map(|i| match spans.get(i + 1) {
            Some((next, _)) => next - spans[i].0 - spans[i].1,
            None => 0.0,
        })
        .collect();
    let other = if gap || index + 1 < count {
        index + 1
    } else {
        index - 1
    };
    let old = if gap { gaps[index] } else { widths[index] };
    let floor = if gap { 0.0 } else { MIN_COLUMN };
    let most = old + widths[other] - MIN_COLUMN;
    let new = points.clamp(floor, most.max(floor));
    let change = new - old;
    if gap {
        gaps[index] = new;
    } else {
        widths[index] = new;
    }
    widths[other] -= change;
    let part = |points: f64| (points / width * f64::from(PARTS)).round().max(0.0) as u32;
    let mut out = Columns {
        equal: false,
        ..columns
    };
    for i in 0..count {
        out.widths[i] = part(widths[i]);
        out.gaps[i] = part(gaps[i]);
    }
    Layout {
        columns: out,
        ..*layout
    }
}

/// The narrowest a column gets, in points.
pub const MIN_COLUMN: f64 = 18.0;

/// The Page, Layout and Section groups of the Inspector.
pub struct LayoutGroups {
    page_group: compact::Group,
    paper: compact::ComboRow,
    landscape: compact::SwitchRow,
    margin_top: compact::SpinRow,
    margin_bottom: compact::SpinRow,
    margin_left: compact::SpinRow,
    margin_right: compact::SpinRow,
    layout_group: compact::Group,
    section_group: compact::Group,
    columns: compact::SpinRow,
    gap: compact::SpinRow,
    /// Whether the columns are as wide as each other (EST-07).
    equal: compact::SwitchRow,
    /// The width of each column and the gap after it, which uneven
    /// columns have.
    column_rows: compact::ExpanderRow,
    widths: Vec<compact::SpinRow>,
    gaps: Vec<compact::SpinRow>,
    before: compact::SpinRow,
    after: compact::SpinRow,
    left: compact::SpinRow,
    right: compact::SpinRow,
    start: compact::ComboRow,
    continues: compact::SwitchRow,
    first_number: compact::SpinRow,
    /// Whether the first page has a header and footer of its own (EST-19).
    first_different: compact::SwitchRow,
    /// Whether left pages have a header and footer of their own (EST-19).
    left_right_different: compact::SwitchRow,
    /// Whether the section shows the headers and footers of the one
    /// before (EST-12).
    previous_margins: compact::SwitchRow,
    units: Cell<Units>,
    /// The page setup, layout and section shown.
    shown: RefCell<(PageSetup, Layout, Section)>,
    updating: Cell<bool>,
    editor: EditorOf,
}

impl LayoutGroups {
    /// The groups for the editor of a window.
    pub fn new(editor: EditorOf, units: Units) -> Rc<Self> {
        let spin = |title: String, min: f64, max: f64| {
            let row = compact::SpinRow::with_range(min, max, 1.0);
            row.set_title(&title);
            row
        };
        let paper = compact::ComboRow::builder()
            .title(tr("Paper"))
            .model(&string_list(paper_names()))
            .build();
        let landscape = compact::SwitchRow::builder().title(tr("Landscape")).build();
        let margin_top = spin(tr("Top Margin"), 0.0, 1.0);
        let margin_bottom = spin(tr("Bottom Margin"), 0.0, 1.0);
        let margin_left = spin(tr("Left Page Margin"), 0.0, 1.0);
        let margin_right = spin(tr("Right Page Margin"), 0.0, 1.0);
        let page_group = compact::Group::builder().title(tr("Page")).build();
        for row in [&margin_top, &margin_bottom, &margin_left, &margin_right] {
            page_group.add(row);
        }
        page_group.add(&paper);
        page_group.add(&landscape);
        let columns = spin(tr("Columns"), 1.0, MAX_COLUMNS as f64);
        let gap = spin(tr("Gap Between Columns"), 0.0, 1.0);
        let equal = compact::SwitchRow::builder()
            .title(tr("Equal Column Width"))
            .active(true)
            .build();
        let column_rows = compact::ExpanderRow::builder()
            .title(tr("Column Widths"))
            .build();
        let mut widths = Vec::new();
        let mut gaps = Vec::new();
        for i in 0..MAX_COLUMNS {
            let n = (i + 1).to_string();
            let width = spin(tr("Column {n}").replace("{n}", &n), 0.0, 1.0);
            column_rows.add_row(&width);
            widths.push(width);
            if i + 1 < MAX_COLUMNS {
                let gap = spin(tr("Gap After Column {n}").replace("{n}", &n), 0.0, 1.0);
                column_rows.add_row(&gap);
                gaps.push(gap);
            }
        }
        let before = spin(tr("Before Layout"), 0.0, 1.0);
        let after = spin(tr("After Layout"), 0.0, 1.0);
        let left = spin(tr("Left Margin"), 0.0, 1.0);
        let right = spin(tr("Right Margin"), 0.0, 1.0);
        let layout_group = compact::Group::builder()
            .title(tr("Layout"))
            .description(tr("The columns and margins of the layout at the cursor"))
            .build();
        layout_group.add(&columns);
        layout_group.add(&gap);
        layout_group.add(&equal);
        layout_group.add(&column_rows);
        for row in [&before, &after, &left, &right] {
            layout_group.add(row);
        }
        let start = compact::ComboRow::builder()
            .title(tr("Section Starts On"))
            .model(&string_list([
                tr("Any Page"),
                tr("Left Page"),
                tr("Right Page"),
            ]))
            .build();
        let continues = compact::SwitchRow::builder()
            .title(tr("Continue Page Numbers"))
            .subtitle(tr("From the section before"))
            .build();
        let first_number = spin(tr("Start Page Numbers At"), 1.0, 9999.0);
        let first_different = compact::SwitchRow::builder()
            .title(tr("Different First Page"))
            .subtitle(tr("With a header and footer of its own"))
            .build();
        let left_right_different = compact::SwitchRow::builder()
            .title(tr("Different Left and Right Pages"))
            .subtitle(tr("Left pages have a header and footer of their own"))
            .build();
        let previous_margins = compact::SwitchRow::builder()
            .title(tr("Use Previous Headers and Footers"))
            .subtitle(tr("Those of the section before, rather than its own"))
            .active(true)
            .build();
        let section_group = compact::Group::builder().title(tr("Section")).build();
        section_group.add(&start);
        section_group.add(&continues);
        section_group.add(&first_number);
        section_group.add(&first_different);
        section_group.add(&left_right_different);
        section_group.add(&previous_margins);

        let this = Rc::new(Self {
            page_group,
            paper,
            landscape,
            margin_top,
            margin_bottom,
            margin_left,
            margin_right,
            layout_group,
            section_group,
            columns,
            gap,
            equal,
            column_rows,
            widths,
            gaps,
            before,
            after,
            left,
            right,
            start,
            continues,
            first_number,
            first_different,
            left_right_different,
            previous_margins,
            units: Cell::new(units),
            shown: RefCell::new(Default::default()),
            updating: Cell::new(false),
            editor,
        });
        this.set_units(units);
        this.connect();
        this
    }

    fn connect(self: &Rc<Self>) {
        for row in [
            &self.margin_top,
            &self.margin_bottom,
            &self.margin_left,
            &self.margin_right,
        ] {
            let weak = Rc::downgrade(self);
            row.connect_value_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && !this.updating.get()
                {
                    this.page_changed();
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.paper.connect_selected_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.page_changed();
            }
        });
        let weak = Rc::downgrade(self);
        self.landscape.connect_active_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.page_changed();
            }
        });
        for row in [
            &self.columns,
            &self.gap,
            &self.before,
            &self.after,
            &self.left,
            &self.right,
        ] {
            let weak = Rc::downgrade(self);
            row.connect_value_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && !this.updating.get()
                {
                    this.layout_changed();
                }
            });
        }
        for (gap, rows) in [(false, &self.widths), (true, &self.gaps)] {
            for (index, row) in rows.iter().enumerate() {
                let weak = Rc::downgrade(self);
                row.connect_value_notify(move |row| {
                    if let Some(this) = weak.upgrade()
                        && !this.updating.get()
                    {
                        this.column_changed(index, gap, row.value());
                    }
                });
            }
        }
        let weak = Rc::downgrade(self);
        self.equal.connect_active_notify(move |row| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                if !row.is_active() {
                    // Uneven, but as wide as before until a width changes.
                    let (setup, shown, _) = *this.shown.borrow();
                    let layout = set_column(&setup, &shown, 0, false, {
                        let spans = shown.columns.spans(layout_width(&setup, &shown));
                        spans.first().map_or(0.0, |s| s.1)
                    });
                    if let Some(editor) = (this.editor)() {
                        editor.set_layout("Layout", layout);
                    }
                    return;
                }
                // Equal again, with the gap shown.
                let (setup, shown, _) = *this.shown.borrow();
                let layout = with_columns(&setup, &shown, shown.columns.count, gap(&setup, &shown));
                if let Some(editor) = (this.editor)() {
                    editor.set_layout("Layout", layout);
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.start.connect_selected_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.section_changed();
            }
        });
        for row in [
            &self.first_different,
            &self.left_right_different,
            &self.previous_margins,
        ] {
            let weak = Rc::downgrade(self);
            row.connect_active_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && !this.updating.get()
                {
                    this.section_changed();
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.continues.connect_active_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.section_changed();
            }
        });
        let weak = Rc::downgrade(self);
        self.first_number.connect_value_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.section_changed();
            }
        });
    }

    /// The groups, in order.
    pub fn groups(&self) -> [&compact::Group; 3] {
        [&self.page_group, &self.layout_group, &self.section_group]
    }

    /// Measures the lengths in other units.
    pub fn set_units(&self, units: Units) {
        self.updating.set(true);
        self.units.set(units);
        for row in self.widths.iter().chain(&self.gaps) {
            tab_stops::set_units(row, units, 0.0, 1584.0);
        }
        for row in [
            &self.gap,
            &self.before,
            &self.after,
            &self.left,
            &self.right,
            &self.margin_top,
            &self.margin_bottom,
            &self.margin_left,
            &self.margin_right,
        ] {
            tab_stops::set_units(row, units, 0.0, 1584.0);
        }
        self.updating.set(false);
        let (setup, layout, section) = *self.shown.borrow();
        self.show_structure(setup, layout, section);
    }

    /// Shows the layout and section at the selection.
    pub fn show(&self, format: &Format) {
        // The first section has headers and footers of its own (EST-12).
        self.updating.set(true);
        self.previous_margins.set_visible(!format.first_section);
        self.updating.set(false);
        self.show_structure(format.setup, format.layout, format.section);
    }

    fn show_structure(&self, setup: PageSetup, layout: Layout, section: Section) {
        *self.shown.borrow_mut() = (setup, layout, section);
        self.updating.set(true);
        let units = self.units.get();
        let set = |row: &compact::SpinRow, value: f64| {
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        };
        let paper = paper_of(&setup) as u32;
        if self.paper.selected() != paper {
            self.paper.set_selected(paper);
        }
        if self.landscape.is_active() != setup.is_landscape() {
            self.landscape.set_active(setup.is_landscape());
        }
        // With facing pages, the left margin is the inside one.
        let (left, right) = if setup.facing_pages {
            (tr("Inside Margin"), tr("Outside Margin"))
        } else {
            (tr("Left Page Margin"), tr("Right Page Margin"))
        };
        self.margin_left.set_title(&left);
        self.margin_right.set_title(&right);
        set(
            &self.margin_top,
            tab_stops::in_units(setup.margin_top, units),
        );
        set(
            &self.margin_bottom,
            tab_stops::in_units(setup.margin_bottom, units),
        );
        set(
            &self.margin_left,
            tab_stops::in_units(setup.margin_left, units),
        );
        set(
            &self.margin_right,
            tab_stops::in_units(setup.margin_right, units),
        );
        set(&self.columns, f64::from(layout.columns.count));
        self.gap.set_sensitive(layout.columns.count > 1);
        set(
            &self.gap,
            tab_stops::in_units(Length::from_pt(gap(&setup, &layout)), units),
        );
        let count = usize::from(layout.columns.count.max(1));
        let uneven = count > 1 && !layout.columns.equal;
        if self.equal.is_active() == uneven {
            self.equal.set_active(!uneven);
        }
        self.equal.set_sensitive(count > 1);
        self.gap.set_sensitive(count > 1 && !uneven);
        self.column_rows.set_visible(count > 1);
        let spans = layout.columns.spans(layout_width(&setup, &layout));
        for (i, row) in self.widths.iter().enumerate() {
            row.set_visible(i < count);
            row.set_sensitive(uneven);
            if let Some((_, w)) = spans.get(i) {
                set(row, tab_stops::in_units(Length::from_pt(*w), units));
            }
        }
        for (i, row) in self.gaps.iter().enumerate() {
            row.set_visible(i + 1 < count);
            row.set_sensitive(uneven);
            if let (Some(this), Some(next)) = (spans.get(i), spans.get(i + 1)) {
                let gap = next.0 - this.0 - this.1;
                set(row, tab_stops::in_units(Length::from_pt(gap), units));
            }
        }
        set(&self.before, tab_stops::in_units(layout.before, units));
        set(&self.after, tab_stops::in_units(layout.after, units));
        set(&self.left, tab_stops::in_units(layout.left, units));
        set(&self.right, tab_stops::in_units(layout.right, units));
        let start = STARTS.iter().position(|s| *s == section.start).unwrap_or(0) as u32;
        if self.start.selected() != start {
            self.start.set_selected(start);
        }
        let continues = section.first_number.is_none();
        if self.continues.is_active() != continues {
            self.continues.set_active(continues);
        }
        self.first_number.set_sensitive(!continues);
        if self.first_different.is_active() != section.first_page_different {
            self.first_different
                .set_active(section.first_page_different);
        }
        if self.left_right_different.is_active() != section.left_right_different {
            self.left_right_different
                .set_active(section.left_right_different);
        }
        if self.previous_margins.is_active() == section.own_margins {
            self.previous_margins.set_active(!section.own_margins);
        }
        set(
            &self.first_number,
            f64::from(section.first_number.unwrap_or(1)),
        );
        self.updating.set(false);
    }

    fn page_changed(&self) {
        let (shown, _, _) = *self.shown.borrow();
        let units = self.units.get();
        let length = |row: &compact::SpinRow| tab_stops::from_units(row.value(), units);
        let mut setup = PageSetup {
            margin_top: length(&self.margin_top),
            margin_bottom: length(&self.margin_bottom),
            margin_left: length(&self.margin_left),
            margin_right: length(&self.margin_right),
            ..shown
        };
        if self.landscape.is_active() != setup.is_landscape() {
            setup = setup.rotated();
        }
        setup = with_paper(&setup, self.paper.selected() as usize);
        // The body keeps some room, as the ruler keeps it.
        let room = Length::from_points(36);
        if setup.body_width() < room || setup.body_height() < room {
            self.show_structure(shown, self.shown.borrow().1, self.shown.borrow().2);
            return;
        }
        if setup != shown
            && let Some(editor) = (self.editor)()
        {
            editor.set_page_setup("Page Setup", setup);
        }
    }

    fn layout_changed(&self) {
        let (setup, shown, _) = *self.shown.borrow();
        let units = self.units.get();
        let length = |row: &compact::SpinRow| tab_stops::from_units(row.value(), units);
        let margins = Layout {
            before: length(&self.before),
            after: length(&self.after),
            left: length(&self.left),
            right: length(&self.right),
            ..shown
        };
        let count = self.columns.value().round() as u8;
        let gap_changed =
            (tab_stops::from_units(self.gap.value(), units).to_pt() - gap(&setup, &shown)).abs()
                > 0.01;
        // Uneven columns stay as they are until their number or gap
        // changes.
        let layout = if count != shown.columns.count || gap_changed {
            let gap = tab_stops::from_units(self.gap.value(), units).to_pt();
            with_columns(&setup, &margins, count, gap)
        } else {
            margins
        };
        if layout != shown
            && let Some(editor) = (self.editor)()
        {
            editor.set_layout("Layout", layout);
        }
    }

    /// A column or the gap after it changed width in the table of
    /// columns.
    fn column_changed(&self, index: usize, gap: bool, value: f64) {
        let (setup, shown, _) = *self.shown.borrow();
        let points = tab_stops::from_units(value, self.units.get()).to_pt();
        let layout = set_column(&setup, &shown, index, gap, points);
        if layout != shown
            && let Some(editor) = (self.editor)()
        {
            editor.set_layout("Column Width", layout);
        }
    }

    fn section_changed(&self) {
        let (_, _, shown) = *self.shown.borrow();
        let section = Section {
            start: STARTS
                .get(self.start.selected() as usize)
                .copied()
                .unwrap_or_default(),
            first_number: (!self.continues.is_active())
                .then(|| self.first_number.value().round() as u32),
            first_page_different: self.first_different.is_active(),
            left_right_different: self.left_right_different.is_active(),
            own_margins: !self.previous_margins.is_active() && self.previous_margins.is_visible(),
        };
        if section != shown
            && let Some(editor) = (self.editor)()
        {
            editor.set_section("Section", section);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_07_a_column_wider_makes_the_next_one_narrower() {
        let setup = PageSetup::a4();
        let three = with_columns(&setup, &Layout::default(), 3, 12.0);
        let width = layout_width(&setup, &three);
        let before = three.columns.spans(width);
        let wider = set_column(&setup, &three, 0, false, before[0].1 + 30.0);
        assert!(!wider.columns.equal);
        let after = wider.columns.spans(width);
        assert!((after[0].1 - before[0].1 - 30.0).abs() < 0.01);
        assert!((after[1].1 - before[1].1 + 30.0).abs() < 0.01);
        assert!((after[2].1 - before[2].1).abs() < 0.01);
        // The last column takes from the one before it.
        let last = set_column(&setup, &three, 2, false, before[2].1 + 10.0);
        let spans = last.columns.spans(width);
        assert!((spans[1].1 - before[1].1 + 10.0).abs() < 0.01);
        // A wider gap makes the next column narrower, and no column gets
        // narrower than the least.
        let gap = set_column(&setup, &three, 0, true, 10_000.0);
        let spans = gap.columns.spans(width);
        assert!((spans[1].1 - MIN_COLUMN).abs() < 0.01);
        assert!((spans[2].0 + spans[2].1 - width).abs() < 0.01);
    }

    #[test]
    fn est_01_paper_keeps_its_orientation() {
        let a4 = PageSetup::a4();
        assert_eq!(paper_of(&a4), 4);
        assert_eq!(paper_of(&a4.rotated()), 4);
        let letter = with_paper(&a4.rotated(), 0);
        assert!(letter.is_landscape());
        assert_eq!(letter.width, Length::from_points(792));
        assert_eq!(letter.margin_top, a4.rotated().margin_top);
        let custom = PageSetup::with_size(Length::from_points(500), Length::from_points(500));
        assert_eq!(paper_of(&custom), PAPERS.len());
        assert_eq!(with_paper(&custom, PAPERS.len()), custom);
    }

    #[test]
    fn est_07_columns_keep_the_gap_in_points() {
        let setup = PageSetup::default();
        let layout = with_columns(&setup, &Layout::default(), 3, 18.0);
        assert_eq!(layout.columns.count, 3);
        assert!((gap(&setup, &layout) - 18.0).abs() < 0.01);
        let spans = layout.columns.spans(layout_width(&setup, &layout));
        assert!((spans[1].0 - spans[0].1 - 18.0).abs() < 0.01);
    }
}
