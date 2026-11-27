// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The popover of the Shape button, as the shape library of Pages 15.4
//! (W113): the categories at the side, the shapes of the one chosen, a
//! search field that finds shapes of them all, and My Shapes, whose
//! shapes can be renamed and deleted. The pen and the connection line are
//! at the bottom.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use desk_model::{PathSegment, Shape};
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::i18n::tr;
use crate::shape_library::{self, Category, Made};

/// The side of a shape of the grid, in pixels.
const TILE: i32 = 44;

/// A shape the grid shows: its name, what it puts on the page, and the
/// value of the action that inserts it.
struct Tile {
    name: String,
    made: Made,
    value: String,
    /// The id of a shape of My Shapes.
    saved: Option<u64>,
}

/// The shape library in the popover of the Shape button.
pub(crate) struct ShapePopover {
    popover: gtk::Popover,
    search: gtk::SearchEntry,
    categories: gtk::ListBox,
    grid: gtk::FlowBox,
    empty: gtk::Label,
    category: Cell<Category>,
    /// The menus of the shapes of My Shapes, taken off when the grid is
    /// made again.
    menus: RefCell<Vec<gtk::PopoverMenu>>,
}

impl ShapePopover {
    pub(crate) fn new() -> Rc<Self> {
        let search = gtk::SearchEntry::builder()
            .placeholder_text(tr("Search Shapes"))
            .build();
        crate::formatting::labelled(&search, &tr("Search Shapes"));
        let categories = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        for category in Category::ALL {
            let label = gtk::Label::builder()
                .label(category.name())
                .xalign(0.0)
                .build();
            categories.append(&label);
        }
        crate::formatting::labelled(&categories, &tr("Categories"));
        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .min_children_per_line(4)
            .max_children_per_line(6)
            .valign(gtk::Align::Start)
            .build();
        let empty = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .css_classes(["dim-label"])
            .vexpand(true)
            .visible(false)
            .build();
        let shapes = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        shapes.append(&grid);
        shapes.append(&empty);
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&shapes)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();
        let side = gtk::ScrolledWindow::builder()
            .child(&categories)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(130)
            .build();
        let middle = gtk::Box::builder().spacing(6).vexpand(true).build();
        middle.append(&side);
        middle.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        middle.append(&scrolled);
        let drawn = gtk::Box::builder()
            .spacing(6)
            .halign(gtk::Align::End)
            .build();
        for (label, action) in [
            (tr("Pe_n"), "win.draw-with-pen"),
            (tr("Connection L_ine"), "win.insert-connection"),
        ] {
            let button = gtk::Button::builder()
                .label(label)
                .use_underline(true)
                .action_name(action)
                .build();
            drawn.append(&button);
        }
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .width_request(460)
            .height_request(380)
            .build();
        content.append(&search);
        content.append(&middle);
        content.append(&drawn);
        let popover = gtk::Popover::builder().child(&content).build();
        let this = Rc::new(Self {
            popover,
            search,
            categories,
            grid,
            empty,
            category: Cell::new(Category::Basic),
            menus: RefCell::new(Vec::new()),
        });
        {
            let weak = Rc::downgrade(&this);
            this.categories.connect_row_selected(move |_, row| {
                let (Some(this), Some(row)) = (weak.upgrade(), row) else {
                    return;
                };
                let Some(&category) = usize::try_from(row.index())
                    .ok()
                    .and_then(|i| Category::ALL.get(i))
                else {
                    return;
                };
                this.category.set(category);
                if !this.search.text().is_empty() {
                    this.search.set_text("");
                }
                this.fill();
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.search.connect_search_changed(move |search| {
                let Some(this) = weak.upgrade() else { return };
                // The shapes found are of all the categories.
                if search.text().trim().is_empty() {
                    this.select(this.category.get());
                } else {
                    this.categories.unselect_all();
                }
                this.fill();
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.popover.connect_show(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.select(this.category.get());
                    this.fill();
                    this.search.grab_focus();
                }
            });
        }
        this
    }

    /// The popover, to give the Shape button.
    pub(crate) fn popover(&self) -> &gtk::Popover {
        &self.popover
    }

    /// Opens on My Shapes, as after a shape is saved to it.
    pub(crate) fn show_mine(&self) {
        self.category.set(Category::Mine);
    }

    /// Shows the shapes again, as after one of My Shapes changed: once the
    /// menu that asked for the change is done, since the grid takes it
    /// away.
    pub(crate) fn refresh(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(this) = weak.upgrade()
                && this.popover.is_visible()
            {
                this.fill();
            }
        });
    }

    fn select(&self, category: Category) {
        let index = Category::ALL.iter().position(|c| *c == category);
        let row = index.and_then(|i| self.categories.row_at_index(i32::try_from(i).ok()?));
        if row.as_ref() != self.categories.selected_row().as_ref() {
            self.categories.select_row(row.as_ref());
        }
    }

    /// The shapes to show: those found from the search field in all the
    /// categories, or those of the category chosen.
    fn tiles(&self) -> Vec<Tile> {
        let query = self.search.text();
        let searching = !query.trim().is_empty();
        let mut tiles: Vec<Tile> = shape_library::entries()
            .iter()
            .filter(|e| {
                if searching {
                    e.matches(&query)
                } else {
                    e.category == self.category.get()
                }
            })
            .filter_map(|e| {
                Some(Tile {
                    name: e.name(),
                    made: e.made()?,
                    value: e.id.to_owned(),
                    saved: None,
                })
            })
            .collect();
        if searching || self.category.get() == Category::Mine {
            let saved = shape_library::read_saved(&shape_library::saved_path());
            tiles.extend(
                saved
                    .into_iter()
                    .filter(|s| !searching || s.matches(&query))
                    .filter_map(|s| {
                        Some(Tile {
                            made: s.made()?,
                            value: format!("mine:{:x}", s.id),
                            saved: Some(s.id),
                            name: s.name,
                        })
                    }),
            );
        }
        tiles
    }

    /// Makes the grid again.
    fn fill(&self) {
        for menu in self.menus.take() {
            menu.unparent();
        }
        self.grid.remove_all();
        let tiles = self.tiles();
        for tile in &tiles {
            self.grid.append(&self.button(tile));
        }
        self.empty.set_visible(tiles.is_empty());
        if tiles.is_empty() {
            self.empty
                .set_label(&if self.search.text().trim().is_empty() {
                    tr("Save a shape here with Arrange > Shape > Save to My Shapes")
                } else {
                    tr("No Shapes Found")
                });
        }
    }

    /// A button that inserts a shape, with its picture.
    fn button(&self, tile: &Tile) -> gtk::Button {
        let picture = gtk::DrawingArea::builder()
            .content_width(TILE)
            .content_height(TILE)
            .build();
        let made = tile.made.clone();
        picture.set_draw_func(move |area, cairo, w, h| draw(area, cairo, &made, w, h));
        let button = gtk::Button::builder()
            .child(&picture)
            .tooltip_text(&tile.name)
            .css_classes(["flat"])
            .action_name("win.insert-library-shape")
            .action_target(&tile.value.to_variant())
            .build();
        button.update_property(&[gtk::accessible::Property::Label(&tile.name)]);
        let popover = self.popover.downgrade();
        button.connect_clicked(move |_| {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
        });
        if let Some(id) = tile.saved {
            let menu = gio::Menu::new();
            let target = format!("{id:x}");
            menu.append(
                Some(&tr("_Rename…")),
                Some(&format!("win.rename-my-shape('{target}')")),
            );
            menu.append(
                Some(&tr("_Delete")),
                Some(&format!("win.delete-my-shape('{target}')")),
            );
            let menu = gtk::PopoverMenu::from_model(Some(&menu));
            menu.set_parent(&button);
            menu.set_has_arrow(false);
            let click = gtk::GestureClick::builder().button(3).build();
            let shown = menu.clone();
            click.connect_pressed(move |gesture, _, x, y| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                shown.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                shown.popup();
            });
            button.add_controller(click);
            self.menus.borrow_mut().push(menu);
        }
        button
    }
}

impl Drop for ShapePopover {
    fn drop(&mut self) {
        for menu in self.menus.take() {
            menu.unparent();
        }
    }
}

/// Draws a shape in its tile, as large as fits, in the color of the text.
fn draw(area: &gtk::DrawingArea, cairo: &gtk::cairo::Context, made: &Made, w: i32, h: i32) {
    let (shape, size) = match made {
        Made::New(new) => match new.shape() {
            Some(shape) => (shape, (100.0, 100.0)),
            None => (Shape::Line { rising: false }, (100.0, 100.0)),
        },
        Made::Shape(shape, size) | Made::Saved(shape, size) => (shape.clone(), *size),
    };
    let margin = 3.0;
    let (bw, bh) = (f64::from(w) - 2.0 * margin, f64::from(h) - 2.0 * margin);
    let scale = (bw / size.0).min(bh / size.1);
    let (sw, sh) = (size.0 * scale, size.1 * scale);
    let (ox, oy) = (margin + (bw - sw) / 2.0, margin + (bh - sh) / 2.0);
    let color = area.color();
    cairo.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()) * 0.8,
    );
    for segment in shape.outline(sw, sh) {
        match segment {
            PathSegment::Move(x, y) => cairo.move_to(ox + x, oy + y),
            PathSegment::Line(x, y) => cairo.line_to(ox + x, oy + y),
            PathSegment::Curve(ax, ay, bx, by, x, y) => {
                cairo.curve_to(ox + ax, oy + ay, ox + bx, oy + by, ox + x, oy + y);
            }
            PathSegment::Close => cairo.close_path(),
        }
    }
    let drawn = if shape.is_line() {
        cairo.set_line_width(2.0);
        cairo.stroke()
    } else {
        cairo.set_fill_rule(gtk::cairo::FillRule::Winding);
        cairo.fill()
    };
    if let Err(error) = drawn {
        glib::g_debug!("desk", "cannot draw a shape: {error}");
    }
}
