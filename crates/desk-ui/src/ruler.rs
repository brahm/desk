// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rulers over and beside the pages (INT-17, FMT-16, FMT-17).
//!
//! The horizontal ruler shows the margins of the page, and the indents and
//! tab stops of the paragraph at the cursor, which it changes: dragging a
//! marker moves an indent or a stop, a click makes a left stop, a double
//! click on a stop changes its alignment, and dragging a stop off the
//! ruler takes it away. Dragging the edge of a margin moves the margin of
//! the page, and the indents with it; with Alt, the indents stay where
//! they are on the page (FMT-18). The gaps between the columns of the
//! layout are shaded, and dragging an edge of a column changes its width
//! or the gap after it (EST-07). The vertical ruler shows the top and
//! bottom margins. Both count in the units of the preferences.

use std::cell::RefCell;
use std::rc::Rc;

use desk_model::{Length, PageSetup, ParaAttrs, ParaFormat, TabAlignment, TabStop, TabStops};
use gtk::prelude::*;

use crate::formatting::EditorOf;
use crate::i18n::tr;
use crate::preferences::{self, Preferences};
use crate::view::DocumentView;

/// The thickness of a ruler, in pixels.
pub const THICKNESS: i32 = 24;

/// How near a marker the pointer must be to take it, in pixels.
const GRAB: f64 = 5.0;

/// How far past the ruler a stop must be dragged to go away, in pixels.
const TEAR_OFF: f64 = 16.0;

/// The size of the markers, in pixels.
const MARK: f64 = 5.0;

/// What a marker of the ruler stands for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Marker {
    FirstLine,
    Left,
    Right,
    /// A tab stop, by its index.
    Tab(usize),
    /// The left margin of the page, at the left edge of the text.
    LeftMargin,
    /// The right margin of the page, at the right edge of the text.
    RightMargin,
    /// The right edge of a column of the layout, by its index (EST-07).
    ColumnEnd(usize),
    /// The left edge of a column after the first, by its index.
    ColumnStart(usize),
}

/// A marker being dragged.
#[derive(Clone, Copy, Debug)]
struct Drag {
    /// What was pressed, if anything.
    marker: Option<Marker>,
    /// Where the press was, in pixels.
    start: (f64, f64),
    /// Where the marker is now, in points from the left edge of the text.
    position: f64,
    /// Whether the pointer moved enough to count as a drag.
    moved: bool,
    /// Whether a stop is off the ruler, to go when it is let go.
    off: bool,
    /// Whether Alt was held, so that a margin moves without the indents.
    keep_indents: bool,
    /// Whether it drags a guide out onto the pages (OBJ-28).
    guide: bool,
}

struct State {
    setup: PageSetup,
    /// The layout at the cursor, whose columns the ruler shows.
    layout: desk_model::Layout,
    format: Option<ParaFormat>,
    preferences: Preferences,
    drag: Option<Drag>,
}

/// A ruler of a window.
#[derive(Clone)]
pub struct Ruler {
    area: gtk::DrawingArea,
    vertical: bool,
    view: DocumentView,
    state: Rc<RefCell<State>>,
}

impl Ruler {
    /// A ruler for the pages of a view, horizontal or vertical. The
    /// horizontal one changes the paragraphs through the editor.
    pub fn new(view: &DocumentView, editor: EditorOf, vertical: bool) -> Self {
        // A group, since GTK names no plain widget for screen readers.
        let area = gtk::DrawingArea::builder()
            .accessible_role(gtk::AccessibleRole::Group)
            .can_focus(false)
            .css_classes(["desk-ruler"])
            .build();
        if vertical {
            area.set_content_width(THICKNESS);
            area.set_vexpand(true);
        } else {
            area.set_content_height(THICKNESS);
            area.set_hexpand(true);
        }
        area.update_property(&[gtk::accessible::Property::Label(&if vertical {
            tr("Vertical Ruler")
        } else {
            tr("Ruler")
        })]);
        let ruler = Self {
            area,
            vertical,
            view: view.clone(),
            state: Rc::new(RefCell::new(State {
                setup: PageSetup::default(),
                layout: desk_model::Layout::default(),
                format: None,
                preferences: preferences::get(),
                drag: None,
            })),
        };
        let weak = ruler.area.downgrade();
        ruler.view.connect_moved(move || {
            if let Some(area) = weak.upgrade() {
                area.queue_draw();
            }
        });
        let state = Rc::downgrade(&ruler.state);
        let weak = ruler.area.downgrade();
        preferences::connect_changed(move |p| {
            if let (Some(state), Some(area)) = (state.upgrade(), weak.upgrade()) {
                state.borrow_mut().preferences = p.clone();
                area.queue_draw();
            }
        });
        ruler.connect_drawing();
        if vertical {
            ruler.connect_guides(editor);
        } else {
            ruler.connect_pointer(editor);
        }
        ruler
    }

    /// The widget.
    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    /// Shows the page setup, the columns of the layout and the format of
    /// the paragraph at the cursor.
    pub fn show(&self, setup: PageSetup, layout: desk_model::Layout, format: &ParaFormat) {
        {
            let mut state = self.state.borrow_mut();
            state.setup = setup;
            state.layout = layout;
            state.format = Some(format.clone());
        }
        self.area.queue_draw();
    }

    /// The page the ruler measures: the page of the cursor.
    fn page(&self) -> usize {
        self.view.caret().map_or(0, |c| c.page)
    }

    /// Where a distance from the left edge of the page is on the ruler.
    fn to_pixels(&self, points: f64) -> Option<f64> {
        let page = self.page();
        let (x, y) = self.view.to_visible(page, points, points)?;
        Some(if self.vertical { y } else { x })
    }

    fn connect_drawing(&self) {
        let ruler = self.downgrade();
        self.area.set_draw_func(move |area, cr, width, height| {
            let Some(ruler) = ruler.upgrade() else { return };
            let (width, height) = (f64::from(width), f64::from(height));
            let state = ruler.state.borrow();
            let setup = state.setup;
            let preferences = state.preferences.clone();
            let fg = area.color();
            let ink = |cr: &gtk::cairo::Context, alpha: f64| {
                cr.set_source_rgba(
                    f64::from(fg.red()),
                    f64::from(fg.green()),
                    f64::from(fg.blue()),
                    f64::from(fg.alpha()) * alpha,
                );
            };
            let along = if ruler.vertical { height } else { width };
            let across = if ruler.vertical { width } else { height };
            // A rectangle from `a` to `b` along the ruler, across all of it.
            let band = |cr: &gtk::cairo::Context, a: f64, b: f64| {
                if ruler.vertical {
                    cr.rectangle(0.0, a, across, b - a);
                } else {
                    cr.rectangle(a, 0.0, b - a, across);
                }
            };
            let (page_size, start_margin, end_margin) = if ruler.vertical {
                (setup.height, setup.margin_top, setup.margin_bottom)
            } else {
                (setup.width, setup.margin_left, setup.margin_right)
            };
            let page_size = page_size.to_pt();
            let (Some(page_start), Some(page_end)) =
                (ruler.to_pixels(0.0), ruler.to_pixels(page_size))
            else {
                return;
            };
            let text_start = start_margin.to_pt();
            let text_end = page_size - end_margin.to_pt();
            // The page, light, with its margins shaded.
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.85);
            band(cr, page_start.max(0.0), page_end.min(along));
            let _ = cr.fill();
            ink(cr, 0.12);
            for (a, b) in [(0.0, text_start), (text_end, page_size)] {
                if let (Some(a), Some(b)) = (ruler.to_pixels(a), ruler.to_pixels(b)) {
                    band(cr, a, b);
                }
            }
            let _ = cr.fill();
            // The gaps between the columns, lighter.
            if !ruler.vertical {
                ink(cr, 0.07);
                let edges = column_edges(&setup, &state.layout);
                for pair in edges.chunks(2) {
                    if let [(Marker::ColumnEnd(_), a), (Marker::ColumnStart(_), b)] = pair
                        && let (Some(a), Some(b)) = (
                            ruler.to_pixels(text_start + a),
                            ruler.to_pixels(text_start + b),
                        )
                    {
                        band(cr, a, b);
                    }
                }
                let _ = cr.fill();
            }

            // The ticks, numbered from the origin.
            // Only the horizontal ruler counts from the middle.
            let origin = if preferences.center_origin && !ruler.vertical {
                (text_start + text_end) / 2.0
            } else {
                text_start
            };
            let (major, parts) = if preferences.percent {
                ((text_end - text_start) / 10.0, 2)
            } else {
                let (major, parts) = preferences.units.ticks();
                (major * preferences.units.points(), parts)
            };
            let minor = major / f64::from(parts);
            if minor * ruler.view.scale() < 3.0 {
                return;
            }
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.75);
            cr.set_line_width(1.0);
            cr.set_font_size(9.0);
            let first = ((0.0 - origin) / minor).floor() as i64;
            let last = ((page_size - origin) / minor).ceil() as i64;
            for k in first..=last {
                let points = origin + k as f64 * minor;
                let Some(p) = ruler.to_pixels(points) else {
                    continue;
                };
                let p = p.round() + 0.5;
                let numbered = k.rem_euclid(i64::from(parts)) == 0;
                let half = parts % 2 == 0 && k.rem_euclid(i64::from(parts)) == i64::from(parts / 2);
                let length = if numbered {
                    across * 0.45
                } else if half {
                    across * 0.3
                } else {
                    across * 0.18
                };
                if ruler.vertical {
                    cr.move_to(across - length, p);
                    cr.line_to(across, p);
                } else {
                    cr.move_to(p, across - length);
                    cr.line_to(p, across);
                }
                let _ = cr.stroke();
                if numbered && k != 0 {
                    let value = (k / i64::from(parts)).abs();
                    let label = if preferences.percent {
                        format!("{}", value * 10)
                    } else {
                        let units = (k as f64 / f64::from(parts)).abs() * major
                            / preferences.units.points();
                        format_number(units)
                    };
                    if ruler.vertical {
                        cr.move_to(2.0, p - 2.0);
                    } else {
                        cr.move_to(p + 2.0, 10.0);
                    }
                    let _ = cr.show_text(&label);
                }
            }
            if ruler.vertical {
                return;
            }

            // The indents and the stops of the paragraph.
            let Some(format) = &state.format else { return };
            let drag = state.drag;
            let accent = (0.21, 0.52, 0.89);
            let markers = markers(format, text_end - text_start);
            for (marker, position) in markers {
                let (position, faded) = match drag {
                    Some(Drag {
                        marker: Some(m),
                        moved: true,
                        position: p,
                        off,
                        ..
                    }) if m == marker => (p, off),
                    _ => (position, false),
                };
                let Some(x) = ruler.to_pixels(text_start + position) else {
                    continue;
                };
                let x = x.round() + 0.5;
                cr.set_source_rgba(accent.0, accent.1, accent.2, if faded { 0.35 } else { 1.0 });
                match marker {
                    Marker::FirstLine => {
                        cr.move_to(x - MARK, 1.0);
                        cr.line_to(x + MARK, 1.0);
                        cr.line_to(x, 1.0 + MARK + 1.0);
                        cr.close_path();
                        let _ = cr.fill();
                    }
                    Marker::Left | Marker::Right => {
                        cr.move_to(x - MARK, across - 1.0);
                        cr.line_to(x + MARK, across - 1.0);
                        cr.line_to(x, across - 2.0 - MARK);
                        cr.close_path();
                        let _ = cr.fill();
                    }
                    Marker::Tab(index) => {
                        let stop = format.tab_stops.stops()[index];
                        draw_tab(cr, x, across, stop.alignment);
                    }
                    Marker::LeftMargin
                    | Marker::RightMargin
                    | Marker::ColumnEnd(_)
                    | Marker::ColumnStart(_) => {}
                }
            }
            // A margin being dragged: a line where it goes.
            if let Some(Drag {
                marker:
                    Some(
                        Marker::LeftMargin
                        | Marker::RightMargin
                        | Marker::ColumnEnd(_)
                        | Marker::ColumnStart(_),
                    ),
                moved: true,
                position,
                ..
            }) = drag
                && let Some(x) = ruler.to_pixels(text_start + position)
            {
                let x = x.round() + 0.5;
                cr.set_source_rgba(accent.0, accent.1, accent.2, 1.0);
                cr.set_line_width(2.0);
                cr.move_to(x, 0.0);
                cr.line_to(x, across);
                let _ = cr.stroke();
                cr.set_line_width(1.0);
            }
        });
    }

    /// Lets a drag from the ruler beside the pages bring out a guide down
    /// them (OBJ-28).
    fn connect_guides(&self, editor: EditorOf) {
        let drag = gtk::GestureDrag::builder()
            .button(gtk::gdk::BUTTON_PRIMARY)
            .build();
        let ruler = self.downgrade();
        let edit = editor.clone();
        drag.connect_drag_update(move |gesture, dx, dy| {
            let (Some(ruler), Some(editor), Some((x, y))) =
                (ruler.upgrade(), edit(), gesture.start_point())
            else {
                return;
            };
            if x + dx > f64::from(THICKNESS) + TEAR_OFF
                || ruler.state.borrow().drag.is_some_and(|d| d.guide)
            {
                ruler.state.borrow_mut().drag = Some(Drag {
                    marker: None,
                    start: (x, y),
                    position: 0.0,
                    moved: true,
                    off: false,
                    keep_indents: false,
                    guide: true,
                });
                editor.drag_guide_from_ruler(&ruler.area, true, x + dx, y + dy);
            }
        });
        let ruler = self.downgrade();
        drag.connect_drag_end(move |_, _, _| {
            let Some(ruler) = ruler.upgrade() else { return };
            let guide = ruler
                .state
                .borrow_mut()
                .drag
                .take()
                .is_some_and(|d| d.guide);
            if guide && let Some(editor) = editor() {
                editor.end_guide_drag();
            }
        });
        self.area.add_controller(drag);
    }

    fn connect_pointer(&self, editor: EditorOf) {
        let drag = gtk::GestureDrag::builder()
            .button(gtk::gdk::BUTTON_PRIMARY)
            .build();
        let ruler = self.downgrade();
        drag.connect_drag_begin(move |gesture, x, y| {
            let Some(ruler) = ruler.upgrade() else { return };
            let keep_indents = gesture
                .current_event_state()
                .contains(gtk::gdk::ModifierType::ALT_MASK);
            let marker = ruler.marker_at(x, y, keep_indents);
            let position = ruler.position_at(x).unwrap_or(0.0);
            ruler.state.borrow_mut().drag = Some(Drag {
                marker,
                start: (x, y),
                position,
                moved: false,
                off: false,
                keep_indents,
                guide: false,
            });
        });
        let ruler = self.downgrade();
        let edit = editor.clone();
        drag.connect_drag_update(move |_, dx, dy| {
            let Some(ruler) = ruler.upgrade() else { return };
            let Some(mut drag) = ruler.state.borrow().drag else {
                return;
            };
            let x = drag.start.0 + dx;
            let y = drag.start.1 + dy;
            // A drag from the ruler down onto the pages brings out a guide
            // across them (OBJ-28).
            drag.guide |= drag.marker.is_none() && y > f64::from(THICKNESS) + TEAR_OFF;
            if drag.guide {
                if let Some(editor) = edit() {
                    editor.drag_guide_from_ruler(&ruler.area, false, x, y);
                }
                ruler.state.borrow_mut().drag = Some(drag);
                return;
            }
            drag.moved |= dx.abs() > 2.0;
            if let Some(position) = ruler.position_at(x) {
                drag.position = ruler.snap(position);
            }
            drag.off = matches!(drag.marker, Some(Marker::Tab(_)))
                && (y < -TEAR_OFF || y > f64::from(THICKNESS) + TEAR_OFF);
            drag.moved |= drag.off;
            ruler.state.borrow_mut().drag = Some(drag);
            ruler.area.queue_draw();
        });
        let ruler = self.downgrade();
        let edit = editor.clone();
        drag.connect_drag_end(move |_, _, _| {
            let Some(ruler) = ruler.upgrade() else { return };
            let (drag, format) = {
                let mut state = ruler.state.borrow_mut();
                (state.drag.take(), state.format.clone())
            };
            ruler.area.queue_draw();
            if drag.is_some_and(|d| d.guide) {
                if let Some(editor) = edit() {
                    editor.end_guide_drag();
                }
                return;
            }
            let (Some(drag), Some(format), Some(editor)) = (drag, format, edit()) else {
                return;
            };
            let position = Length::from_pt(drag.position);
            let stops = &format.tab_stops;
            let (name, attrs) = match (drag.marker, drag.moved) {
                // A click on the ruler makes a left stop there.
                (None, false) => {
                    let Some(at) = ruler.position_at(drag.start.0) else {
                        return;
                    };
                    let at = Length::from_pt(ruler.snap(at));
                    if at.millipoints() <= 0 {
                        return;
                    }
                    (tr("Tab Stops"), tabs(stops.with(TabStop::left(at))))
                }
                (Some(Marker::Tab(index)), true) => {
                    let stop = stops.stops()[index];
                    let without = stops.without(stop.position);
                    let moved = if drag.off {
                        without
                    } else {
                        without.with(TabStop { position, ..stop })
                    };
                    (tr("Tab Stops"), tabs(moved))
                }
                (Some(Marker::Left), true) => (
                    tr("Indent"),
                    ParaAttrs {
                        left_indent: Some(position.max(Length::ZERO)),
                        ..ParaAttrs::default()
                    },
                ),
                (Some(Marker::FirstLine), true) => (
                    tr("Indent"),
                    ParaAttrs {
                        first_line_indent: Some(position - format.left_indent),
                        ..ParaAttrs::default()
                    },
                ),
                (Some(marker @ (Marker::LeftMargin | Marker::RightMargin)), true) => {
                    let setup = ruler.state.borrow().setup;
                    let Some((setup, shift)) = moved_margin(setup, marker, drag.position) else {
                        return;
                    };
                    let shift = drag.keep_indents.then_some(shift);
                    editor.set_margins(&tr("Margins"), setup, shift);
                    return;
                }
                (Some(Marker::ColumnEnd(index)), true) => {
                    let (setup, layout) = {
                        let state = ruler.state.borrow();
                        (state.setup, state.layout)
                    };
                    let start = column_edges(&setup, &layout)
                        .into_iter()
                        .find(|(m, _)| *m == Marker::ColumnStart(index))
                        .map_or(layout.left.to_pt(), |(_, p)| p);
                    let start = if index == 0 {
                        layout.left.to_pt()
                    } else {
                        start
                    };
                    let layout = crate::layout_pane::set_column(
                        &setup,
                        &layout,
                        index,
                        false,
                        drag.position - start,
                    );
                    editor.set_layout(&tr("Columns"), layout);
                    return;
                }
                (Some(Marker::ColumnStart(index)), true) => {
                    let (setup, layout) = {
                        let state = ruler.state.borrow();
                        (state.setup, state.layout)
                    };
                    let Some(end) = column_edges(&setup, &layout)
                        .into_iter()
                        .find(|(m, _)| *m == Marker::ColumnEnd(index - 1))
                        .map(|(_, p)| p)
                    else {
                        return;
                    };
                    let layout = crate::layout_pane::set_column(
                        &setup,
                        &layout,
                        index - 1,
                        true,
                        drag.position - end,
                    );
                    editor.set_layout(&tr("Columns"), layout);
                    return;
                }
                (Some(Marker::Right), true) => {
                    let setup = ruler.state.borrow().setup;
                    let width = setup.body_width();
                    (
                        tr("Indent"),
                        ParaAttrs {
                            right_indent: Some((width - position).max(Length::ZERO)),
                            ..ParaAttrs::default()
                        },
                    )
                }
                _ => return,
            };
            editor.set_paragraph_attrs(&name, &attrs);
        });
        self.area.add_controller(drag);

        // A double click on a stop changes its alignment.
        let click = gtk::GestureClick::builder()
            .button(gtk::gdk::BUTTON_PRIMARY)
            .build();
        let ruler = self.downgrade();
        click.connect_pressed(move |_, presses, x, y| {
            if presses != 2 {
                return;
            }
            let Some(ruler) = ruler.upgrade() else { return };
            let (Some(Marker::Tab(index)), Some(format), Some(editor)) = (
                ruler.marker_at(x, y, false),
                ruler.state.borrow().format.clone(),
                editor(),
            ) else {
                return;
            };
            let stop = format.tab_stops.stops()[index];
            let alignment = match stop.alignment {
                TabAlignment::Left => TabAlignment::Center,
                TabAlignment::Center => TabAlignment::Right,
                TabAlignment::Right => TabAlignment::Decimal,
                TabAlignment::Decimal => TabAlignment::Left,
            };
            let stops = format.tab_stops.with(TabStop { alignment, ..stop });
            editor.set_paragraph_attrs(&tr("Tab Stops"), &tabs(stops));
        });
        self.area.add_controller(click);
    }

    /// The marker under a point of the ruler. The edges of the margins
    /// come after the markers of the paragraph, unless `margins_first`.
    fn marker_at(&self, x: f64, y: f64, margins_first: bool) -> Option<Marker> {
        let state = self.state.borrow();
        let format = state.format.as_ref()?;
        let text_start = state.setup.margin_left.to_pt();
        let width = state.setup.body_width().to_pt();
        let thickness = f64::from(THICKNESS);
        let band = |marker: Marker| match marker {
            Marker::FirstLine => y < thickness / 3.0,
            Marker::Tab(_) => (thickness / 3.0..thickness * 2.0 / 3.0 + 2.0).contains(&y),
            Marker::Left | Marker::Right => y >= thickness / 2.0,
            Marker::LeftMargin
            | Marker::RightMargin
            | Marker::ColumnEnd(_)
            | Marker::ColumnStart(_) => true,
        };
        let near = |position: f64| {
            let at = self.to_pixels(text_start + position)?;
            let distance = (at - x).abs();
            (distance <= GRAB).then_some(distance)
        };
        let margin = [(Marker::LeftMargin, 0.0), (Marker::RightMargin, width)]
            .into_iter()
            .chain(column_edges(&state.setup, &state.layout))
            .filter_map(|(marker, position)| Some((marker, near(position)?)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(marker, _)| marker);
        if margins_first && margin.is_some() {
            return margin;
        }
        let paragraph = markers(format, width)
            .into_iter()
            .filter(|&(marker, _)| band(marker))
            .filter_map(|(marker, position)| {
                let at = self.to_pixels(text_start + position)?;
                let distance = (at - x).abs();
                (distance <= GRAB).then_some((marker, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(marker, _)| marker);
        paragraph.or(margin)
    }

    /// The distance from the left edge of the text of a point of the
    /// ruler, in points.
    fn position_at(&self, x: f64) -> Option<f64> {
        let text_start = self.state.borrow().setup.margin_left.to_pt();
        let zero = self.to_pixels(0.0)?;
        Some((x - zero) / self.view.scale() - text_start)
    }

    /// A position moved to the nearest small tick of the ruler.
    fn snap(&self, position: f64) -> f64 {
        let state = self.state.borrow();
        let preferences = state.preferences.clone();
        let setup = state.setup;
        let width = setup.body_width().to_pt();
        let origin = if preferences.center_origin {
            width / 2.0
        } else {
            0.0
        };
        let minor = if preferences.percent {
            width / 20.0
        } else {
            let (major, parts) = preferences.units.ticks();
            major * preferences.units.points() / f64::from(parts)
        };
        origin + ((position - origin) / minor).round() * minor
    }

    fn downgrade(&self) -> WeakRuler {
        WeakRuler {
            area: self.area.downgrade(),
            vertical: self.vertical,
            view: self.view.downgrade(),
            state: Rc::downgrade(&self.state),
        }
    }
}

/// A ruler that does not keep its widget alive.
struct WeakRuler {
    area: gtk::glib::WeakRef<gtk::DrawingArea>,
    vertical: bool,
    view: crate::view::WeakView,
    state: std::rc::Weak<RefCell<State>>,
}

impl WeakRuler {
    fn upgrade(&self) -> Option<Ruler> {
        Some(Ruler {
            area: self.area.upgrade()?,
            vertical: self.vertical,
            view: self.view.upgrade()?,
            state: self.state.upgrade()?,
        })
    }
}

/// The edges of the columns of a layout between the first and the last,
/// in points from the left edge of the text: the end of each column but
/// the last, each followed by the start of the next.
fn column_edges(setup: &PageSetup, layout: &desk_model::Layout) -> Vec<(Marker, f64)> {
    if layout.columns.count < 2 {
        return Vec::new();
    }
    let left = layout.left.to_pt();
    let spans = layout
        .columns
        .spans(crate::layout_pane::layout_width(setup, layout));
    let mut out = Vec::new();
    for (i, pair) in spans.windows(2).enumerate() {
        out.push((Marker::ColumnEnd(i), left + pair[0].0 + pair[0].1));
        out.push((Marker::ColumnStart(i + 1), left + pair[1].0));
    }
    out
}

/// The markers of a paragraph in text `width` points wide, and their
/// places, in points from the left edge of the text.
fn markers(format: &ParaFormat, width: f64) -> Vec<(Marker, f64)> {
    let left = format.left_indent.to_pt();
    let mut out = vec![
        (Marker::Left, left),
        (Marker::FirstLine, left + format.first_line_indent.to_pt()),
        (Marker::Right, width - format.right_indent.to_pt()),
    ];
    out.extend(
        format
            .tab_stops
            .stops()
            .iter()
            .enumerate()
            .map(|(i, s)| (Marker::Tab(i), s.position.to_pt())),
    );
    out
}

/// The narrowest text that dragging a margin leaves, in points.
const MIN_TEXT_WIDTH: f64 = 36.0;

/// The page setup with a margin dragged to `position`, in points from the
/// left edge of the text, and how far each edge of the text moved in, in
/// points: what the indents lose to stay where they are on the page.
fn moved_margin(
    setup: PageSetup,
    marker: Marker,
    position: f64,
) -> Option<(PageSetup, (f64, f64))> {
    let width = setup.body_width().to_pt();
    let mut out = setup;
    let shift = match marker {
        Marker::LeftMargin => {
            let delta = position
                .max(-setup.margin_left.to_pt())
                .min(width - MIN_TEXT_WIDTH);
            out.margin_left = Length::from_pt(setup.margin_left.to_pt() + delta);
            (delta, 0.0)
        }
        Marker::RightMargin => {
            let delta = (width - position)
                .max(-setup.margin_right.to_pt())
                .min(width - MIN_TEXT_WIDTH);
            out.margin_right = Length::from_pt(setup.margin_right.to_pt() + delta);
            (0.0, delta)
        }
        _ => return None,
    };
    (out != setup).then_some((out, shift))
}

/// Attributes that set the tab stops.
fn tabs(stops: TabStops) -> ParaAttrs {
    ParaAttrs {
        tab_stops: Some(stops),
        ..ParaAttrs::default()
    }
}

/// Draws the mark of a tab stop at `x`, as Pages draws them: a corner
/// opening to the side the text goes, or a cross for center and decimal.
fn draw_tab(cr: &gtk::cairo::Context, x: f64, across: f64, alignment: TabAlignment) {
    let top = across / 3.0;
    let bottom = across * 2.0 / 3.0;
    cr.set_line_width(2.0);
    cr.move_to(x, top);
    cr.line_to(x, bottom);
    match alignment {
        TabAlignment::Left => {
            cr.line_to(x + MARK, bottom);
        }
        TabAlignment::Right => {
            cr.line_to(x - MARK, bottom);
        }
        TabAlignment::Center | TabAlignment::Decimal => {
            cr.move_to(x - MARK, bottom);
            cr.line_to(x + MARK, bottom);
        }
    }
    let _ = cr.stroke();
    if alignment == TabAlignment::Decimal {
        cr.arc(x + 3.0, top + 2.0, 1.5, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
    cr.set_line_width(1.0);
}

/// A number of a ruler, without trailing zeros.
fn format_number(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_17_the_ruler_marks_indents_and_stops() {
        let format = ParaFormat {
            left_indent: Length::from_points(36),
            first_line_indent: Length::from_points(-18),
            tab_stops: TabStops::new([TabStop::left(Length::from_points(100))]),
            ..ParaFormat::default()
        };
        assert_eq!(
            markers(&format, 400.0),
            [
                (Marker::Left, 36.0),
                (Marker::FirstLine, 18.0),
                (Marker::Right, 400.0),
                (Marker::Tab(0), 100.0),
            ]
        );
        assert_eq!(format_number(2.0), "2");
        assert_eq!(format_number(2.5), "2.5");
    }

    #[test]
    fn fmt_18_dragging_a_margin_moves_the_edge_of_the_text() {
        let setup = PageSetup {
            width: Length::from_points(600),
            margin_left: Length::from_points(72),
            margin_right: Length::from_points(72),
            ..PageSetup::default()
        };
        // The left margin 18 pt to the right.
        let (moved, shift) = moved_margin(setup, Marker::LeftMargin, 18.0).unwrap();
        assert_eq!(moved.margin_left, Length::from_points(90));
        assert_eq!(moved.margin_right, Length::from_points(72));
        assert_eq!(shift, (18.0, 0.0));
        // The right margin 36 pt to the right: 36 pt narrower margin.
        let (moved, shift) = moved_margin(setup, Marker::RightMargin, 456.0 + 36.0).unwrap();
        assert_eq!(moved.margin_right, Length::from_points(36));
        assert_eq!(shift, (0.0, -36.0));
        // Not past the edge of the paper, nor over the other margin.
        let (moved, _) = moved_margin(setup, Marker::LeftMargin, -100.0).unwrap();
        assert_eq!(moved.margin_left, Length::ZERO);
        let (moved, _) = moved_margin(setup, Marker::LeftMargin, 1000.0).unwrap();
        assert_eq!(moved.body_width(), Length::from_points(36));
        assert!(moved_margin(setup, Marker::LeftMargin, 0.0).is_none());
    }
}
