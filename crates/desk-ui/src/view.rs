// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The document area: the pages of a document on a grey background, with
//! scrolling and zoom.
//!
//! The pages are drawn from their display lists with the same painter that
//! writes PDF, only those in sight, at the zoom of the window.

use std::cell::RefCell;
use std::rc::Rc;

use desk_render::{DisplayList, Painter};
use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::geometry::{self, PIXELS_PER_POINT, PageRect};

struct State {
    pages: Vec<DisplayList>,
    zoom: f64,
    painter: Option<Painter>,
    on_zoom: Vec<Box<dyn Fn(f64)>>,
}

/// The document area of a window.
#[derive(Clone)]
pub struct DocumentView {
    scrolled: gtk::ScrolledWindow,
    area: gtk::DrawingArea,
    state: Rc<RefCell<State>>,
}

impl DocumentView {
    /// An empty document area.
    pub fn new() -> Self {
        let area = gtk::DrawingArea::new();
        area.add_css_class("desk-document");
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&area)
            .hexpand(true)
            .vexpand(true)
            .build();
        let state = Rc::new(RefCell::new(State {
            pages: Vec::new(),
            zoom: geometry::DEFAULT_ZOOM,
            painter: Painter::new().ok(),
            on_zoom: Vec::new(),
        }));
        let view = Self {
            scrolled,
            area,
            state,
        };
        view.connect_drawing();
        view.connect_zoom_gestures();
        view
    }

    /// The widget to put in a window.
    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.scrolled
    }

    /// Shows the pages of a document.
    pub fn set_pages(&self, pages: Vec<DisplayList>) {
        self.state.borrow_mut().pages = pages;
        self.resize();
    }

    /// How many pages it shows.
    pub fn page_count(&self) -> usize {
        self.state.borrow().pages.len()
    }

    /// The zoom, as a fraction.
    pub fn zoom(&self) -> f64 {
        self.state.borrow().zoom
    }

    /// Sets the zoom, keeping the middle of the view where it was.
    pub fn set_zoom(&self, zoom: f64) {
        let zoom = zoom.clamp(geometry::ZOOM_LEVELS[0], geometry::ZOOM_LEVELS[8]);
        let old = self.zoom();
        if (zoom - old).abs() < 1e-9 {
            return;
        }
        let vadj = self.scrolled.vadjustment();
        let middle = (vadj.value() + vadj.page_size() / 2.0) / old;
        self.state.borrow_mut().zoom = zoom;
        self.resize();
        vadj.set_value(middle * zoom - vadj.page_size() / 2.0);
        // The callbacks may read the view, so they run without the borrow.
        let callbacks = std::mem::take(&mut self.state.borrow_mut().on_zoom);
        for callback in &callbacks {
            callback(zoom);
        }
        self.state.borrow_mut().on_zoom = callbacks;
    }

    /// Calls `f` with the new zoom whenever it changes.
    pub fn connect_zoom_changed(&self, f: impl Fn(f64) + 'static) {
        self.state.borrow_mut().on_zoom.push(Box::new(f));
    }

    /// The zoom at which the widest page fills the width of the view.
    pub fn fit_width_zoom(&self) -> f64 {
        let (width, _) = self.largest_page();
        geometry::fit_width(width, f64::from(self.scrolled.width()))
    }

    /// The zoom at which a whole page fits in the view.
    pub fn fit_page_zoom(&self) -> f64 {
        let (width, height) = self.largest_page();
        geometry::fit_page(
            width,
            height,
            f64::from(self.scrolled.width()),
            f64::from(self.scrolled.height()),
        )
    }

    fn largest_page(&self) -> (f64, f64) {
        let sizes = self.sizes();
        if sizes.is_empty() {
            // US Letter, the paper of the blank templates.
            return (612.0, 792.0);
        }
        sizes.into_iter().fold((0.0, 0.0), |(w, h), (pw, ph)| {
            (f64::max(w, pw), f64::max(h, ph))
        })
    }

    fn sizes(&self) -> Vec<(f64, f64)> {
        self.state
            .borrow()
            .pages
            .iter()
            .map(|p| (f64::from(p.width), f64::from(p.height)))
            .collect()
    }

    fn layout(&self, area_width: f64) -> (Vec<PageRect>, f64, f64) {
        geometry::place_pages(&self.sizes(), self.zoom(), area_width)
    }

    fn resize(&self) {
        let (_, width, height) = self.layout(0.0);
        self.area.set_content_width(width.ceil() as i32);
        self.area.set_content_height(height.ceil() as i32);
        self.area.queue_draw();
    }

    fn connect_drawing(&self) {
        let state = Rc::downgrade(&self.state);
        let scrolled = self.scrolled.downgrade();
        self.area.set_draw_func(move |area, cr, width, _height| {
            let (Some(state), Some(scrolled)) = (state.upgrade(), scrolled.upgrade()) else {
                return;
            };
            let mut state = state.borrow_mut();
            let sizes: Vec<(f64, f64)> = state
                .pages
                .iter()
                .map(|p| (f64::from(p.width), f64::from(p.height)))
                .collect();
            let (rects, _, _) = geometry::place_pages(&sizes, state.zoom, f64::from(width));
            let vadj = scrolled.vadjustment();
            let (top, bottom) = (vadj.value(), vadj.value() + vadj.page_size());
            let scale = state.zoom * PIXELS_PER_POINT;
            let dark = adw_is_dark(area);
            for (index, rect) in rects.iter().enumerate() {
                if rect.y + rect.height < top || rect.y > bottom {
                    continue;
                }
                draw_page_frame(cr, rect, dark);
                let State { pages, painter, .. } = &mut *state;
                if let Some(painter) = painter.as_mut() {
                    cr.save().ok();
                    cr.translate(rect.x, rect.y);
                    cr.scale(scale, scale);
                    cr.rectangle(
                        0.0,
                        0.0,
                        f64::from(pages[index].width),
                        f64::from(pages[index].height),
                    );
                    cr.clip();
                    if let Err(error) = painter.paint(cr, &pages[index]) {
                        glib::g_warning!("desk", "cannot draw page {}: {error}", index + 1);
                    }
                    cr.restore().ok();
                }
            }
        });
        // Only the pages in sight are drawn, so scrolling draws again.
        let area = self.area.downgrade();
        self.scrolled.vadjustment().connect_value_changed(move |_| {
            if let Some(area) = area.upgrade() {
                area.queue_draw();
            }
        });
    }

    fn connect_zoom_gestures(&self) {
        // Control and the scroll wheel zoom, as in the document viewers of
        // GNOME.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let view = self.downgrade();
        scroll.connect_scroll(move |controller, _dx, dy| {
            let control = controller
                .current_event_state()
                .contains(gdk::ModifierType::CONTROL_MASK);
            let Some(view) = view.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !control || dy == 0.0 {
                return glib::Propagation::Proceed;
            }
            let zoom = view.zoom();
            view.set_zoom(if dy < 0.0 {
                geometry::zoom_in(zoom)
            } else {
                geometry::zoom_out(zoom)
            });
            glib::Propagation::Stop
        });
        self.scrolled.add_controller(scroll);

        let pinch = gtk::GestureZoom::new();
        let view = self.downgrade();
        let start = Rc::new(RefCell::new(geometry::DEFAULT_ZOOM));
        let begin = start.clone();
        let begun = view.clone();
        pinch.connect_begin(move |_, _| {
            if let Some(view) = begun.upgrade() {
                *begin.borrow_mut() = view.zoom();
            }
        });
        pinch.connect_scale_changed(move |_, scale| {
            if let Some(view) = view.upgrade() {
                view.set_zoom(*start.borrow() * scale);
            }
        });
        self.scrolled.add_controller(pinch);
    }

    fn downgrade(&self) -> WeakView {
        WeakView {
            scrolled: self.scrolled.downgrade(),
            area: self.area.downgrade(),
            state: Rc::downgrade(&self.state),
        }
    }
}

impl Default for DocumentView {
    fn default() -> Self {
        Self::new()
    }
}

struct WeakView {
    scrolled: glib::WeakRef<gtk::ScrolledWindow>,
    area: glib::WeakRef<gtk::DrawingArea>,
    state: std::rc::Weak<RefCell<State>>,
}

impl WeakView {
    fn upgrade(&self) -> Option<DocumentView> {
        Some(DocumentView {
            scrolled: self.scrolled.upgrade()?,
            area: self.area.upgrade()?,
            state: self.state.upgrade()?,
        })
    }
}

impl Clone for WeakView {
    fn clone(&self) -> Self {
        Self {
            scrolled: self.scrolled.clone(),
            area: self.area.clone(),
            state: self.state.clone(),
        }
    }
}

/// Whether the window uses the dark style, which darkens the background
/// around the pages but never the pages themselves.
fn adw_is_dark(_widget: &gtk::DrawingArea) -> bool {
    adw::StyleManager::default().is_dark()
}

/// The shadow and white sheet of a page.
fn draw_page_frame(cr: &gtk::cairo::Context, rect: &PageRect, dark: bool) {
    let shadow = if dark { 0.5 } else { 0.18 };
    for (spread, alpha) in [(3.0, shadow * 0.35), (1.5, shadow * 0.6), (0.5, shadow)] {
        cr.set_source_rgba(0.0, 0.0, 0.0, alpha);
        cr.rectangle(
            rect.x - spread,
            rect.y - spread + 1.0,
            rect.width + 2.0 * spread,
            rect.height + 2.0 * spread,
        );
        cr.fill().ok();
    }
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.rectangle(rect.x, rect.y, rect.width, rect.height);
    cr.fill().ok();
}
