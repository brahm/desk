// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bar of the advanced gradient of the Graphic inspector, as in Pages
//! (OBJ-41): the gradient from its first color to its last, with a mark
//! under each color. A click on a mark chooses that color, a drag moves
//! it, and a click between the marks adds a color there. The arrow keys
//! choose the color before or after.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use desk_model::{Gradient, GradientStop, Rgba};
use gtk::prelude::*;

use crate::i18n::tr;

/// How wide the bar is, and how tall, in pixels.
const WIDTH: i32 = 140;
const HEIGHT: i32 = 26;
/// How far in from the ends the gradient starts, so that the marks fit.
const INSET: f64 = 6.0;
/// How tall the gradient is, above the marks.
const BAND: f64 = 14.0;
/// How near a mark a click must be to take it, in pixels.
const REACH: f64 = 6.0;

/// The bar, with the colors it shows and the one chosen.
pub struct GradientBar {
    area: gtk::DrawingArea,
    stops: RefCell<Vec<GradientStop>>,
    current: Cell<usize>,
    /// What to do when the colors change, from the bar.
    on_change: RefCell<Option<Rc<dyn Fn()>>>,
    /// What to do when another color is chosen.
    on_select: RefCell<Option<Rc<dyn Fn()>>>,
}

impl GradientBar {
    pub fn new() -> Rc<Self> {
        let area = gtk::DrawingArea::builder()
            .content_width(WIDTH)
            .content_height(HEIGHT)
            .valign(gtk::Align::Center)
            .focusable(true)
            .build();
        crate::formatting::labelled(&area, &tr("Gradient Colors"));
        let this = Rc::new(Self {
            area,
            stops: RefCell::new(Vec::new()),
            current: Cell::new(0),
            on_change: RefCell::new(None),
            on_select: RefCell::new(None),
        });
        let weak = Rc::downgrade(&this);
        this.area.set_draw_func(move |_, cr, w, h| {
            if let Some(this) = weak.upgrade() {
                this.draw(cr, f64::from(w), f64::from(h));
            }
        });
        let drag = gtk::GestureDrag::new();
        let weak = Rc::downgrade(&this);
        drag.connect_drag_begin(move |_, x, _| {
            if let Some(this) = weak.upgrade() {
                this.area.grab_focus();
                this.press(x);
            }
        });
        let weak = Rc::downgrade(&this);
        drag.connect_drag_update(move |gesture, dx, _| {
            let Some(this) = weak.upgrade() else { return };
            let Some((x, _)) = gesture.start_point() else {
                return;
            };
            this.move_current(this.at(x + dx));
        });
        this.area.add_controller(drag);
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&this);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(this) = weak.upgrade() else {
                return gtk::glib::Propagation::Proceed;
            };
            let count = this.stops.borrow().len();
            let current = this.current.get();
            let next = match key {
                gtk::gdk::Key::Left if current > 0 => current - 1,
                gtk::gdk::Key::Right if current + 1 < count => current + 1,
                _ => return gtk::glib::Propagation::Proceed,
            };
            this.select(next);
            gtk::glib::Propagation::Stop
        });
        this.area.add_controller(keys);
        this
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn connect_change(&self, f: impl Fn() + 'static) {
        *self.on_change.borrow_mut() = Some(Rc::new(f));
    }

    pub fn connect_select(&self, f: impl Fn() + 'static) {
        *self.on_select.borrow_mut() = Some(Rc::new(f));
    }

    /// The colors, in order.
    pub fn stops(&self) -> Vec<GradientStop> {
        self.stops.borrow().clone()
    }

    /// Shows other colors, keeping the chosen one where it can.
    pub fn set_stops(&self, mut stops: Vec<GradientStop>) {
        stops.sort_by_key(|s| s.at);
        let current = self.current.get().min(stops.len().saturating_sub(1));
        *self.stops.borrow_mut() = stops;
        self.current.set(current);
        self.area.queue_draw();
    }

    /// The chosen color, by its place in order.
    pub fn current(&self) -> usize {
        self.current.get()
    }

    /// Changes the chosen color.
    pub fn set_current(&self, stop: GradientStop) {
        let index = self.current.get();
        let mut stops = self.stops.borrow_mut();
        let Some(slot) = stops.get_mut(index) else {
            return;
        };
        *slot = stop;
        // It stays chosen where it goes in order.
        stops.sort_by_key(|s| s.at);
        let at = stops
            .iter()
            .position(|s| *s == stop)
            .unwrap_or(index.min(stops.len() - 1));
        drop(stops);
        self.current.set(at);
        self.area.queue_draw();
    }

    /// Adds a color halfway between the chosen one and the next, or the
    /// one before when it is the last.
    pub fn add(&self) {
        let (a, b) = {
            let stops = self.stops.borrow();
            let i = self.current.get();
            match (stops.get(i), stops.get(i + 1)) {
                (Some(a), Some(b)) => (a.at, b.at),
                (Some(a), None) if i > 0 => (stops[i - 1].at, a.at),
                _ => (0, 1000),
            }
        };
        self.insert((a + b) / 2);
        self.changed();
    }

    /// Takes out the chosen color, while two are left at least.
    pub fn remove(&self) {
        {
            let mut stops = self.stops.borrow_mut();
            if stops.len() <= 2 {
                return;
            }
            let i = self.current.get().min(stops.len() - 1);
            stops.remove(i);
            self.current.set(i.saturating_sub(1));
        }
        self.area.queue_draw();
        self.changed();
        self.selected();
    }

    fn insert(&self, at: u16) {
        let color = self.color_at(at);
        let mut stops = self.stops.borrow_mut();
        stops.push(GradientStop::new(at, color));
        stops.sort_by_key(|s| s.at);
        let index = stops.iter().rposition(|s| s.at == at).unwrap_or(0);
        drop(stops);
        self.current.set(index);
        self.area.queue_draw();
        self.selected();
    }

    /// The color of the gradient at a place, in thousandths of the way.
    fn color_at(&self, at: u16) -> Rgba {
        let stops = self.stops.borrow();
        let after = stops.iter().position(|s| s.at >= at);
        match after {
            Some(0) => stops[0].color,
            Some(i) => {
                let (a, b) = (stops[i - 1], stops[i]);
                let t = f64::from(at - a.at) / f64::from((b.at - a.at).max(1));
                let mix =
                    |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
                Rgba {
                    r: mix(a.color.r, b.color.r),
                    g: mix(a.color.g, b.color.g),
                    b: mix(a.color.b, b.color.b),
                    a: mix(a.color.a, b.color.a),
                }
            }
            None => stops.last().map_or(Rgba::BLACK, |s| s.color),
        }
    }

    /// The place of a point of the bar, in thousandths of the way.
    fn at(&self, x: f64) -> u16 {
        let width = f64::from(self.area.width()).max(2.0 * INSET + 1.0) - 2.0 * INSET;
        (((x - INSET) / width) * 1000.0).round().clamp(0.0, 1000.0) as u16
    }

    fn x(&self, at: u16, width: f64) -> f64 {
        INSET + (width - 2.0 * INSET) * f64::from(at) / 1000.0
    }

    fn press(&self, x: f64) {
        let width = f64::from(self.area.width());
        let near = {
            let stops = self.stops.borrow();
            stops
                .iter()
                .enumerate()
                .map(|(i, s)| (i, (self.x(s.at, width) - x).abs()))
                .filter(|(_, d)| *d <= REACH)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        };
        match near {
            Some(i) => self.select(i),
            None => {
                self.insert(self.at(x));
                self.changed();
            }
        }
    }

    fn move_current(&self, at: u16) {
        let stop = {
            let stops = self.stops.borrow();
            let Some(stop) = stops.get(self.current.get()) else {
                return;
            };
            GradientStop { at, ..*stop }
        };
        if stop.at == at && self.stops.borrow().get(self.current.get()) == Some(&stop) {
            return;
        }
        self.set_current(stop);
        self.changed();
        self.selected();
    }

    fn select(&self, index: usize) {
        self.current.set(index);
        self.area.queue_draw();
        self.selected();
    }

    fn changed(&self) {
        let on = self.on_change.borrow().clone();
        if let Some(on) = on {
            on();
        }
    }

    fn selected(&self) {
        let on = self.on_select.borrow().clone();
        if let Some(on) = on {
            on();
        }
    }

    fn draw(&self, cr: &gtk::cairo::Context, width: f64, _height: f64) {
        let stops = self.stops.borrow();
        let gradient = gtk::cairo::LinearGradient::new(INSET, 0.0, width - INSET, 0.0);
        let colors = Gradient {
            stops: stops.clone(),
            angle: 0,
            center: None,
        }
        .color_stops();
        for (at, c) in colors {
            gradient.add_color_stop_rgba(
                at,
                f64::from(c.r) / 255.0,
                f64::from(c.g) / 255.0,
                f64::from(c.b) / 255.0,
                f64::from(c.a) / 255.0,
            );
        }
        cr.rectangle(INSET, 0.5, width - 2.0 * INSET, BAND);
        let _ = cr.set_source(&gradient);
        let _ = cr.fill_preserve();
        cr.set_source_rgba(0.5, 0.5, 0.5, 0.8);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
        for (i, stop) in stops.iter().enumerate() {
            let x = self.x(stop.at, width);
            cr.move_to(x, BAND + 1.0);
            cr.line_to(x + 5.0, BAND + 9.0);
            cr.line_to(x - 5.0, BAND + 9.0);
            cr.close_path();
            let c = stop.color;
            cr.set_source_rgb(
                f64::from(c.r) / 255.0,
                f64::from(c.g) / 255.0,
                f64::from(c.b) / 255.0,
            );
            let _ = cr.fill_preserve();
            if i == self.current.get() {
                cr.set_source_rgb(0.21, 0.52, 0.89);
                cr.set_line_width(2.0);
            } else {
                cr.set_source_rgba(0.3, 0.3, 0.3, 0.9);
                cr.set_line_width(1.0);
            }
            let _ = cr.stroke();
        }
    }
}
