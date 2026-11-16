// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The canvas of a sheet: the sheet with its tables and charts, at the
//! zoom of the window. A click selects a cell of a table, or the table by
//! its name; the arrows move the cell in focus and Shift and the arrows
//! make a range. The table with the focus shows its reference tabs, the
//! letters of the columns and the numbers of the rows that formulas use
//! (NTAB-07).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::workbook::{COLUMN_WIDTH, Grid, ItemKind, ROW_HEIGHT, Sheet, SheetItem};
use gtk::{gdk, glib};

use crate::geometry::PIXELS_PER_POINT;
use crate::i18n::tr;

/// The room around the items of a sheet, in points.
const MARGIN: f64 = 18.0;
/// The thickness of the reference tabs, in points.
const TAB: f64 = 14.0;
/// The height of the name of a table over it, in points.
const NAME_HEIGHT: f64 = 18.0;

/// A cell of a table: its row and its column, from 0.
pub(super) type CellAt = (u32, u32);

/// What is selected on the sheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Selection {
    /// Nothing.
    #[default]
    None,
    /// An item, as a whole.
    Item(usize),
    /// A range of cells of a table: where it started and the cell in
    /// focus.
    Cells {
        item: usize,
        anchor: CellAt,
        focus: CellAt,
    },
}

impl Selection {
    /// The item selected, alone or by its cells.
    pub fn item(self) -> Option<usize> {
        match self {
            Self::None => None,
            Self::Item(item) | Self::Cells { item, .. } => Some(item),
        }
    }

    /// The first and the last row and column of the range of cells.
    pub fn range(self) -> Option<(CellAt, CellAt)> {
        match self {
            Self::Cells { anchor, focus, .. } => Some((
                (anchor.0.min(focus.0), anchor.1.min(focus.1)),
                (anchor.0.max(focus.0), anchor.1.max(focus.1)),
            )),
            _ => None,
        }
    }

    /// How many cells the range has.
    pub fn cells(self) -> Option<u64> {
        self.range()
            .map(|(a, b)| u64::from(b.0 - a.0 + 1) * u64::from(b.1 - a.1 + 1))
    }
}

/// The letters of a column, as formulas name it: A to Z, then AA.
pub(super) fn column_name(mut column: u32) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (column % 26) as u8);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).expect("letters are ASCII")
}

/// The name of a cell, as "B2".
pub(super) fn cell_name((row, column): CellAt) -> String {
    format!("{}{}", column_name(column), row + 1)
}

/// The name of a range of cells, as "B2:F7", or of one cell.
pub(super) fn range_name(first: CellAt, last: CellAt) -> String {
    if first == last {
        cell_name(first)
    } else {
        format!("{}:{}", cell_name(first), cell_name(last))
    }
}

/// Where a point of the sheet falls, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Cell(usize, CellAt),
    Item(usize),
}

/// What the canvas reports.
pub(super) type Selected = Rc<dyn Fn(Selection)>;
/// A press of the secondary button, at a point of the widget.
pub(super) type ContextMenu = Rc<dyn Fn(f64, f64)>;

/// The canvas of the sheet shown.
pub(super) struct Canvas {
    area: gtk::DrawingArea,
    scroller: gtk::ScrolledWindow,
    sheet: RefCell<Sheet>,
    selection: Cell<Selection>,
    zoom: Cell<f64>,
    selected: RefCell<Option<Selected>>,
    context_menu: RefCell<Option<ContextMenu>>,
}

impl Canvas {
    pub fn new() -> Rc<Self> {
        let area = gtk::DrawingArea::builder()
            .accessible_role(gtk::AccessibleRole::Grid)
            .focusable(true)
            .can_focus(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        area.update_property(&[gtk::accessible::Property::Label(&tr("Sheet"))]);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&area)
            .hexpand(true)
            .vexpand(true)
            .build();
        let this = Rc::new(Self {
            area,
            scroller,
            sheet: RefCell::new(Sheet::empty("")),
            selection: Cell::new(Selection::None),
            zoom: Cell::new(crate::geometry::DEFAULT_ZOOM),
            selected: RefCell::new(None),
            context_menu: RefCell::new(None),
        });
        let weak = Rc::downgrade(&this);
        this.area.set_draw_func(move |area, cr, width, height| {
            if let Some(this) = weak.upgrade() {
                this.draw(area, cr, width, height);
            }
        });
        this.connect_input();
        this
    }

    /// The widget, which scrolls.
    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.scroller
    }

    /// Gives the canvas the keyboard focus.
    pub fn focus(&self) {
        self.area.grab_focus();
    }

    /// Calls `f` with what the person selects.
    pub fn connect_selected(&self, f: impl Fn(Selection) + 'static) {
        *self.selected.borrow_mut() = Some(Rc::new(f));
    }

    /// Calls `f` when the context menu is asked for, at a point of the
    /// widget.
    pub fn connect_context_menu(&self, f: impl Fn(f64, f64) + 'static) {
        *self.context_menu.borrow_mut() = Some(Rc::new(f));
    }

    /// The widget the context menu points at.
    pub fn area(&self) -> &gtk::DrawingArea {
        &self.area
    }

    /// Shows a sheet, with a selection on it.
    pub fn show(&self, sheet: &Sheet, selection: Selection) {
        *self.sheet.borrow_mut() = sheet.clone();
        self.selection.set(selection);
        // A screen reader hears the sheet, and the size of the table
        // selected, as the reference tabs show it.
        let description = selection
            .item()
            .and_then(|i| sheet.items.get(i))
            .map(describe)
            .unwrap_or_default();
        self.area.update_property(&[
            gtk::accessible::Property::Label(&sheet.name),
            gtk::accessible::Property::Description(&description),
        ]);
        self.resize();
        self.area.queue_draw();
    }

    /// The zoom, 1 being the actual size.
    pub fn zoom(&self) -> f64 {
        self.zoom.get()
    }

    pub fn set_zoom(&self, zoom: f64) {
        self.zoom.set(zoom);
        self.resize();
        self.area.queue_draw();
    }

    /// Pixels of the widget for a point of the sheet.
    fn scale(&self) -> f64 {
        PIXELS_PER_POINT * self.zoom.get()
    }

    fn resize(&self) {
        let (width, height) = self.sheet.borrow().extent();
        let scale = self.scale();
        self.area
            .set_content_width(((width + 2.0 * MARGIN) * scale).ceil() as i32);
        self.area
            .set_content_height(((height + 2.0 * MARGIN) * scale).ceil() as i32);
    }

    /// Brings the item into sight.
    pub fn scroll_to(&self, item: usize) {
        let Some(origin) = self.sheet.borrow().items.get(item).map(|i| i.origin) else {
            return;
        };
        let scale = self.scale();
        let x = (origin.0 + MARGIN - TAB * 1.5 - NAME_HEIGHT).max(0.0) * scale;
        let y = (origin.1 + MARGIN - TAB - NAME_HEIGHT).max(0.0) * scale;
        let h = self.scroller.hadjustment();
        let v = self.scroller.vadjustment();
        // Once the canvas has the size of what it shows now.
        glib::idle_add_local_once(move || {
            if x < h.value() || x > h.value() + h.page_size() {
                h.set_value(x);
            }
            if y < v.value() || y > v.value() + v.page_size() {
                v.set_value(y);
            }
        });
    }

    fn report(&self, selection: Selection) {
        let selected = self.selected.borrow().clone();
        if let Some(selected) = selected {
            selected(selection);
        }
    }

    /// What is at a point of the widget.
    fn hit(&self, x: f64, y: f64) -> Option<Hit> {
        let scale = self.scale();
        let (x, y) = (x / scale - MARGIN, y / scale - MARGIN);
        let sheet = self.sheet.borrow();
        // The items drawn last are over the others.
        for (index, item) in sheet.items.iter().enumerate().rev() {
            let (width, height) = item.size();
            let (left, top) = item.origin;
            if let ItemKind::Table(grid) = &item.kind
                && (left..left + width).contains(&x)
                && (top..top + height).contains(&y)
            {
                let column = (((x - left) / COLUMN_WIDTH) as u32).min(grid.columns - 1);
                let row = (((y - top) / ROW_HEIGHT) as u32).min(grid.rows - 1);
                return Some(Hit::Cell(index, (row, column)));
            }
            // The name and the reference tabs over a table select it whole.
            let over = if item.is_table() {
                TAB + NAME_HEIGHT
            } else {
                0.0
            };
            if (left - TAB..left + width).contains(&x) && (top - over..top + height).contains(&y) {
                return Some(Hit::Item(index));
            }
        }
        None
    }

    fn connect_input(self: &Rc<Self>) {
        let click = gtk::GestureClick::builder().button(0).build();
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |gesture, _, x, y| {
            let Some(this) = weak.upgrade() else { return };
            this.area.grab_focus();
            let extend = gesture
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK);
            let hit = this.hit(x, y);
            let current = this.selection.get();
            let selection = match hit {
                Some(Hit::Cell(item, cell)) => match current {
                    Selection::Cells {
                        item: was, anchor, ..
                    } if extend && was == item => Selection::Cells {
                        item,
                        anchor,
                        focus: cell,
                    },
                    // A secondary click inside the range keeps it.
                    Selection::Cells { item: was, .. }
                        if gesture.current_button() == gdk::BUTTON_SECONDARY
                            && was == item
                            && contains(current, cell) =>
                    {
                        current
                    }
                    _ => Selection::Cells {
                        item,
                        anchor: cell,
                        focus: cell,
                    },
                },
                Some(Hit::Item(item)) => Selection::Item(item),
                None => Selection::None,
            };
            if selection != current {
                this.selection.set(selection);
                this.area.queue_draw();
                this.report(selection);
            }
            if gesture.current_button() == gdk::BUTTON_SECONDARY && selection != Selection::None {
                let menu = this.context_menu.borrow().clone();
                if let Some(menu) = menu {
                    menu(x, y);
                }
            }
        });
        self.area.add_controller(click);

        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            this.key(key, state)
        });
        self.area.add_controller(keys);
    }

    /// Moves the cell in focus with the keyboard: the arrows, Tab and
    /// Return, as in Numbers; with Shift, the arrows make a range. Escape
    /// selects the table, and then nothing. Menu and Shift+F10 open the
    /// context menu (NINT-16).
    fn key(&self, key: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        let selection = self.selection.get();
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        if state.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK) {
            return glib::Propagation::Proceed;
        }
        if key == gdk::Key::Menu || (shift && key == gdk::Key::F10) {
            if selection != Selection::None {
                let menu = self.context_menu.borrow().clone();
                if let (Some(menu), Some((x, y))) = (menu, self.focus_point()) {
                    menu(x, y);
                }
                return glib::Propagation::Stop;
            }
            return glib::Propagation::Proceed;
        }
        // Delete takes away the table or the chart selected as a whole.
        if matches!(selection, Selection::Item(_))
            && matches!(
                key,
                gdk::Key::Delete | gdk::Key::KP_Delete | gdk::Key::BackSpace
            )
        {
            let _ = self.area.activate_action("win.delete-item", None);
            return glib::Propagation::Stop;
        }
        let next = match (selection, key) {
            (Selection::Cells { .. }, gdk::Key::Escape) => {
                Selection::Item(selection.item().unwrap_or_default())
            }
            (Selection::Item(_), gdk::Key::Escape) => Selection::None,
            // Return on a table goes into its first cell.
            (Selection::Item(item), gdk::Key::Return | gdk::Key::KP_Enter)
                if self.grid(item).is_some() =>
            {
                Selection::Cells {
                    item,
                    anchor: (0, 0),
                    focus: (0, 0),
                }
            }
            (
                Selection::Cells {
                    item,
                    anchor,
                    focus,
                },
                _,
            ) => {
                let Some(grid) = self.grid(item) else {
                    return glib::Propagation::Proceed;
                };
                let (row, column) = focus;
                let moved = match key {
                    gdk::Key::Up | gdk::Key::KP_Up => (row.saturating_sub(1), column),
                    gdk::Key::Down | gdk::Key::KP_Down | gdk::Key::Return | gdk::Key::KP_Enter => {
                        ((row + 1).min(grid.rows - 1), column)
                    }
                    gdk::Key::Left | gdk::Key::KP_Left => (row, column.saturating_sub(1)),
                    gdk::Key::Right | gdk::Key::KP_Right | gdk::Key::Tab => {
                        (row, (column + 1).min(grid.columns - 1))
                    }
                    gdk::Key::ISO_Left_Tab => (row, column.saturating_sub(1)),
                    _ => return glib::Propagation::Proceed,
                };
                let arrow = !matches!(
                    key,
                    gdk::Key::Tab | gdk::Key::ISO_Left_Tab | gdk::Key::Return | gdk::Key::KP_Enter
                );
                Selection::Cells {
                    item,
                    anchor: if shift && arrow { anchor } else { moved },
                    focus: moved,
                }
            }
            _ => return glib::Propagation::Proceed,
        };
        if next != selection {
            self.selection.set(next);
            self.area.queue_draw();
            self.report(next);
        }
        glib::Propagation::Stop
    }

    /// The grid of a table on the sheet.
    fn grid(&self, item: usize) -> Option<Grid> {
        match self.sheet.borrow().items.get(item)?.kind {
            ItemKind::Table(grid) => Some(grid),
            ItemKind::Chart => None,
        }
    }

    /// The point of the widget at the middle of the cell in focus, or of
    /// the item selected.
    fn focus_point(&self) -> Option<(f64, f64)> {
        let sheet = self.sheet.borrow();
        let selection = self.selection.get();
        let item = sheet.items.get(selection.item()?)?;
        let scale = self.scale();
        let (x, y) = match selection {
            Selection::Cells { focus, .. } => (
                item.origin.0 + (f64::from(focus.1) + 0.5) * COLUMN_WIDTH,
                item.origin.1 + (f64::from(focus.0) + 0.5) * ROW_HEIGHT,
            ),
            _ => item.origin,
        };
        Some(((x + MARGIN) * scale, (y + MARGIN) * scale))
    }

    fn draw(&self, area: &gtk::DrawingArea, cr: &gtk::cairo::Context, width: i32, height: i32) {
        // The sheet is paper: white in the light and the dark style, as the
        // pages of Desk Writer.
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
        let _ = cr.fill();
        let scale = self.scale();
        cr.scale(scale, scale);
        cr.translate(MARGIN, MARGIN);
        let _ = area;
        let accent = adw::StyleManager::default().accent_color_rgba();
        let sheet = self.sheet.borrow();
        let selection = self.selection.get();
        for (index, item) in sheet.items.iter().enumerate() {
            let selected = selection.item() == Some(index);
            match &item.kind {
                ItemKind::Table(grid) => {
                    draw_table(cr, item, grid, selected.then_some(selection), &accent);
                }
                ItemKind::Chart => draw_chart(cr, item, selected, &accent),
            }
        }
    }
}

/// What an item is, for a screen reader: "Table 1, 45 rows by 13
/// columns".
fn describe(item: &SheetItem) -> String {
    match &item.kind {
        ItemKind::Table(grid) => tr("{name}, {rows} rows by {columns} columns")
            .replace("{name}", &item.name)
            .replace("{rows}", &grid.rows.to_string())
            .replace("{columns}", &grid.columns.to_string()),
        ItemKind::Chart => item.name.clone(),
    }
}

/// Whether a cell is in the range of a selection.
fn contains(selection: Selection, (row, column): CellAt) -> bool {
    selection.range().is_some_and(|(first, last)| {
        (first.0..=last.0).contains(&row) && (first.1..=last.1).contains(&column)
    })
}

fn set_color(cr: &gtk::cairo::Context, color: &gdk::RGBA, alpha: f64) {
    cr.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()) * alpha,
    );
}

/// Draws a table: its name when shown, its headers and footers, its grid,
/// and, with the focus, its reference tabs and the cells selected.
fn draw_table(
    cr: &gtk::cairo::Context,
    item: &SheetItem,
    grid: &Grid,
    selection: Option<Selection>,
    accent: &gdk::RGBA,
) {
    let (left, top) = item.origin;
    let (width, height) = grid.size();
    let with_cells = selection.and_then(Selection::range);
    // The name over the table, and over its tabs while they show (NTAB-09).
    if item.name_shown {
        let above = if with_cells.is_some() { TAB } else { 0.0 };
        cr.set_source_rgb(0.1, 0.1, 0.1);
        cr.select_font_face(
            "Adwaita Sans",
            gtk::cairo::FontSlant::Normal,
            gtk::cairo::FontWeight::Bold,
        );
        cr.set_font_size(11.0);
        cr.move_to(left, top - above - 5.0);
        let _ = cr.show_text(&item.name);
    }
    // The header rows and columns, and the footer rows, in the gray of the
    // Basic style.
    cr.set_source_rgb(0.90, 0.91, 0.93);
    let header_height = f64::from(grid.header_rows) * ROW_HEIGHT;
    let footer_height = f64::from(grid.footer_rows) * ROW_HEIGHT;
    let header_width = f64::from(grid.header_columns) * COLUMN_WIDTH;
    cr.rectangle(left, top, width, header_height);
    cr.rectangle(left, top + height - footer_height, width, footer_height);
    cr.rectangle(left, top, header_width, height);
    let _ = cr.fill();
    if let Some((first, last)) = with_cells {
        set_color(cr, accent, 0.18);
        cr.rectangle(
            left + f64::from(first.1) * COLUMN_WIDTH,
            top + f64::from(first.0) * ROW_HEIGHT,
            f64::from(last.1 - first.1 + 1) * COLUMN_WIDTH,
            f64::from(last.0 - first.0 + 1) * ROW_HEIGHT,
        );
        let _ = cr.fill();
    }
    // The grid.
    cr.set_source_rgb(0.75, 0.75, 0.77);
    cr.set_line_width(0.5);
    for row in 0..=grid.rows {
        let y = top + f64::from(row) * ROW_HEIGHT;
        cr.move_to(left, y);
        cr.line_to(left + width, y);
    }
    for column in 0..=grid.columns {
        let x = left + f64::from(column) * COLUMN_WIDTH;
        cr.move_to(x, top);
        cr.line_to(x, top + height);
    }
    let _ = cr.stroke();
    // The lines under the headers and over the footers are thicker.
    cr.set_line_width(1.0);
    cr.set_source_rgb(0.55, 0.56, 0.6);
    if grid.header_rows > 0 {
        cr.move_to(left, top + header_height);
        cr.line_to(left + width, top + header_height);
    }
    if grid.footer_rows > 0 {
        cr.move_to(left, top + height - footer_height);
        cr.line_to(left + width, top + height - footer_height);
    }
    if grid.header_columns > 0 {
        cr.move_to(left + header_width, top);
        cr.line_to(left + header_width, top + height);
    }
    let _ = cr.stroke();
    match selection {
        Some(Selection::Cells { focus, .. }) => {
            if let Some((first, last)) = with_cells {
                draw_tabs(cr, item, grid, first, last, accent);
            }
            // The cell in focus.
            set_color(cr, accent, 1.0);
            cr.set_line_width(2.0);
            cr.rectangle(
                left + f64::from(focus.1) * COLUMN_WIDTH,
                top + f64::from(focus.0) * ROW_HEIGHT,
                COLUMN_WIDTH,
                ROW_HEIGHT,
            );
            let _ = cr.stroke();
        }
        Some(Selection::Item(_)) => {
            // The table as a whole, with its handles.
            set_color(cr, accent, 1.0);
            cr.set_line_width(1.5);
            cr.rectangle(left, top, width, height);
            let _ = cr.stroke();
            for (x, y) in [
                (left, top),
                (left + width, top),
                (left, top + height),
                (left + width, top + height),
            ] {
                cr.arc(x, y, 3.0, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
        }
        _ => {}
    }
}

/// The reference tabs of a table: the letters of the columns over it and
/// the numbers of the rows beside it, those of the range lit (NTAB-07).
fn draw_tabs(
    cr: &gtk::cairo::Context,
    item: &SheetItem,
    grid: &Grid,
    first: CellAt,
    last: CellAt,
    accent: &gdk::RGBA,
) {
    let (left, top) = item.origin;
    let (width, height) = grid.size();
    cr.set_source_rgb(0.93, 0.93, 0.94);
    cr.rectangle(left, top - TAB, width, TAB);
    cr.rectangle(left - TAB * 1.5, top, TAB * 1.5, height);
    let _ = cr.fill();
    set_color(cr, accent, 0.35);
    cr.rectangle(
        left + f64::from(first.1) * COLUMN_WIDTH,
        top - TAB,
        f64::from(last.1 - first.1 + 1) * COLUMN_WIDTH,
        TAB,
    );
    cr.rectangle(
        left - TAB * 1.5,
        top + f64::from(first.0) * ROW_HEIGHT,
        TAB * 1.5,
        f64::from(last.0 - first.0 + 1) * ROW_HEIGHT,
    );
    let _ = cr.fill();
    cr.set_source_rgb(0.35, 0.35, 0.38);
    cr.select_font_face(
        "Adwaita Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    cr.set_font_size(8.0);
    for column in 0..grid.columns {
        let name = column_name(column);
        let x = left + (f64::from(column) + 0.5) * COLUMN_WIDTH;
        centered(cr, &name, x, top - TAB / 2.0);
    }
    for row in 0..grid.rows {
        let y = top + (f64::from(row) + 0.5) * ROW_HEIGHT;
        centered(cr, &(row + 1).to_string(), left - TAB * 0.75, y);
    }
}

/// Draws text centered on a point.
fn centered(cr: &gtk::cairo::Context, text: &str, x: f64, y: f64) {
    if let Ok(extents) = cr.text_extents(text) {
        cr.move_to(
            x - extents.width() / 2.0 - extents.x_bearing(),
            y - extents.height() / 2.0 - extents.y_bearing(),
        );
        let _ = cr.show_text(text);
    }
}

/// Draws the frame of a chart.
fn draw_chart(cr: &gtk::cairo::Context, item: &SheetItem, selected: bool, accent: &gdk::RGBA) {
    let (left, top) = item.origin;
    let (width, height) = item.size();
    cr.set_source_rgb(0.75, 0.75, 0.77);
    cr.set_line_width(0.5);
    cr.rectangle(left, top, width, height);
    let _ = cr.stroke();
    if selected {
        set_color(cr, accent, 1.0);
        cr.set_line_width(1.5);
        cr.rectangle(left, top, width, height);
        let _ = cr.stroke();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntab_07_columns_are_named_by_letters() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(51), "AZ");
        assert_eq!(column_name(52), "BA");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
        assert_eq!(cell_name((1, 5)), "F2");
        assert_eq!(range_name((1, 1), (6, 5)), "B2:F7");
        assert_eq!(range_name((0, 0), (0, 0)), "A1");
    }

    #[test]
    fn ngnm_13_a_range_counts_its_cells() {
        let range = Selection::Cells {
            item: 0,
            anchor: (6, 5),
            focus: (1, 5),
        };
        assert_eq!(range.range(), Some(((1, 5), (6, 5))));
        assert_eq!(range.cells(), Some(6));
        assert!(contains(range, (3, 5)));
        assert!(!contains(range, (3, 4)));
        assert_eq!(Selection::Item(2).cells(), None);
        assert_eq!(Selection::Item(2).item(), Some(2));
        assert_eq!(Selection::None.item(), None);
    }
}
