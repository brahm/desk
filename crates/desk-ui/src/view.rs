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

use crate::area::DocumentArea;
use crate::geometry::{self, PIXELS_PER_POINT, PageRect};
use crate::text_nav::{CaretRect, Invisible, InvisibleMark, SpanRect};

struct State {
    pages: Vec<DisplayList>,
    zoom: f64,
    painter: Option<Painter>,
    on_zoom: Vec<Box<dyn Fn(f64)>>,
    on_pages: Vec<Box<dyn Fn()>>,
    /// The selection and the cursor, drawn over the pages.
    selection: Vec<SpanRect>,
    caret: Option<CaretRect>,
    /// Whether the cursor shows now: it blinks, and hides without focus.
    caret_on: bool,
    /// The marks of the invisible characters, when they show (INT-10).
    invisibles: Vec<InvisibleMark>,
}

/// The document area of a window.
#[derive(Clone)]
pub struct DocumentView {
    scrolled: gtk::ScrolledWindow,
    area: gtk::DrawingArea,
    /// Holds the text being composed with an input method, over the area.
    fixed: gtk::Fixed,
    preedit: gtk::Label,
    state: Rc<RefCell<State>>,
}

impl DocumentView {
    /// An empty document area.
    pub fn new() -> Self {
        let area: gtk::DrawingArea = DocumentArea::default().upcast();
        area.set_focusable(true);
        area.set_can_focus(true);
        area.add_css_class("desk-document");
        area.set_cursor_from_name(Some("text"));
        let fixed = gtk::Fixed::builder().can_target(false).build();
        let preedit = gtk::Label::builder().visible(false).build();
        preedit.add_css_class("desk-preedit");
        fixed.put(&preedit, 0.0, 0.0);
        let overlay = gtk::Overlay::builder().child(&area).build();
        overlay.add_overlay(&fixed);
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&overlay)
            .hexpand(true)
            .vexpand(true)
            .build();
        let state = Rc::new(RefCell::new(State {
            pages: Vec::new(),
            zoom: geometry::DEFAULT_ZOOM,
            painter: Painter::new().ok(),
            on_zoom: Vec::new(),
            on_pages: Vec::new(),
            selection: Vec::new(),
            caret: None,
            caret_on: true,
            invisibles: Vec::new(),
        }));
        let view = Self {
            scrolled,
            area,
            fixed,
            preedit,
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
        let callbacks = std::mem::take(&mut self.state.borrow_mut().on_pages);
        for callback in &callbacks {
            callback();
        }
        self.state.borrow_mut().on_pages = callbacks;
    }

    /// The widget that takes the keyboard and the pointer.
    pub fn area(&self) -> &gtk::DrawingArea {
        &self.area
    }

    /// The widget that gives screen readers the text.
    pub fn access(&self) -> DocumentArea {
        self.area
            .clone()
            .downcast()
            .expect("the area is a document area")
    }

    /// Sets the selection and the cursor drawn over the pages.
    pub fn set_marks(&self, selection: Vec<SpanRect>, caret: Option<CaretRect>) {
        {
            let mut state = self.state.borrow_mut();
            state.selection = selection;
            state.caret = caret;
            state.caret_on = true;
        }
        self.area.queue_draw();
    }

    /// Sets the marks of the invisible characters; none hides them.
    pub fn set_invisibles(&self, marks: Vec<InvisibleMark>) {
        self.state.borrow_mut().invisibles = marks;
        self.area.queue_draw();
    }

    /// Shows or hides the cursor, for blinking.
    pub fn set_caret_on(&self, on: bool) {
        let changed = {
            let mut state = self.state.borrow_mut();
            let changed = state.caret_on != on;
            state.caret_on = on;
            changed
        };
        if changed {
            self.area.queue_draw();
        }
    }

    /// The page under a point of the area and the point on it, in points.
    /// Between pages, the nearest page.
    pub fn page_at(&self, x: f64, y: f64) -> Option<(usize, f64, f64)> {
        let (rects, _, _) = self.layout(f64::from(self.area.width()));
        let scale = self.zoom() * PIXELS_PER_POINT;
        let index = rects
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let d = |r: &&PageRect| {
                    if y < r.y {
                        r.y - y
                    } else if y > r.y + r.height {
                        y - r.y - r.height
                    } else {
                        0.0
                    }
                };
                d(a).total_cmp(&d(b))
            })?
            .0;
        let rect = rects[index];
        Some((index, (x - rect.x) / scale, (y - rect.y) / scale))
    }

    /// Where a point of a page falls in the area.
    pub fn to_area(&self, page: usize, x: f64, y: f64) -> Option<(f64, f64)> {
        let (rects, _, _) = self.layout(f64::from(self.area.width()));
        let rect = rects.get(page)?;
        let scale = self.zoom() * PIXELS_PER_POINT;
        Some((rect.x + x * scale, rect.y + y * scale))
    }

    /// Where a point of a page falls in the part of the view in sight, in
    /// pixels from its top left corner.
    pub fn to_visible(&self, page: usize, x: f64, y: f64) -> Option<(f64, f64)> {
        let (ax, ay) = self.to_area(page, x, y)?;
        Some((
            ax - self.scrolled.hadjustment().value(),
            ay - self.scrolled.vadjustment().value(),
        ))
    }

    /// How many pixels one point of a page takes at this zoom.
    pub fn scale(&self) -> f64 {
        self.zoom() * PIXELS_PER_POINT
    }

    /// The cursor, if it shows.
    pub fn caret(&self) -> Option<CaretRect> {
        self.state.borrow().caret
    }

    /// Calls `f` whenever the pages move in the view: on scrolling, on a
    /// change of zoom or of size, and when the pages change.
    pub fn connect_moved(&self, f: impl Fn() + 'static) {
        let f = Rc::new(f);
        for adjustment in [self.scrolled.hadjustment(), self.scrolled.vadjustment()] {
            let f = f.clone();
            adjustment.connect_value_changed(move |_| f());
        }
        let resized = f.clone();
        self.area.connect_resize(move |_, _, _| resized());
        let zoomed = f.clone();
        self.connect_zoom_changed(move |_| zoomed());
        self.state.borrow_mut().on_pages.push(Box::new(move || f()));
    }

    /// Scrolls so that a cursor is in sight.
    pub fn scroll_to(&self, caret: &CaretRect) {
        let Some((_, top)) = self.to_area(caret.page, caret.x, caret.top) else {
            return;
        };
        let Some((_, bottom)) = self.to_area(caret.page, caret.x, caret.bottom) else {
            return;
        };
        let vadj = self.scrolled.vadjustment();
        let margin = 24.0;
        if top - margin < vadj.value() {
            vadj.set_value(top - margin);
        } else if bottom + margin > vadj.value() + vadj.page_size() {
            vadj.set_value(bottom + margin - vadj.page_size());
        }
    }

    /// Shows the text being composed with an input method at the cursor,
    /// or hides it when `text` is empty.
    pub fn set_preedit(&self, text: &str) {
        let caret = self.state.borrow().caret;
        let place = caret.and_then(|c| self.to_area(c.page, c.x, c.top));
        match place {
            Some((x, y)) if !text.is_empty() => {
                let escaped = glib::markup_escape_text(text);
                self.preedit.set_markup(&format!("<u>{escaped}</u>"));
                self.fixed.move_(&self.preedit, x, y);
                self.preedit.set_visible(true);
            }
            _ => self.preedit.set_visible(false),
        }
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
                draw_marks(cr, &state, index, rect, scale, area.has_focus());
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

    pub(crate) fn downgrade(&self) -> WeakView {
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

pub(crate) struct WeakView {
    scrolled: glib::WeakRef<gtk::ScrolledWindow>,
    area: glib::WeakRef<gtk::DrawingArea>,
    state: std::rc::Weak<RefCell<State>>,
}

impl WeakView {
    pub(crate) fn upgrade(&self) -> Option<DocumentView> {
        let area = self.area.upgrade()?;
        let overlay = area.parent()?;
        let fixed = overlay.last_child()?.downcast::<gtk::Fixed>().ok()?;
        let preedit = fixed.first_child()?.downcast::<gtk::Label>().ok()?;
        Some(DocumentView {
            scrolled: self.scrolled.upgrade()?,
            area,
            fixed,
            preedit,
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

/// The selection and the cursor on a page.
fn draw_marks(
    cr: &gtk::cairo::Context,
    state: &State,
    page: usize,
    rect: &PageRect,
    scale: f64,
    focused: bool,
) {
    draw_invisibles(cr, state, page, rect, scale);
    // The accent color of GNOME, lighter without focus.
    let alpha = if focused { 0.3 } else { 0.15 };
    cr.set_source_rgba(0.21, 0.52, 0.89, alpha);
    for span in state.selection.iter().filter(|s| s.page == page) {
        cr.rectangle(
            rect.x + span.x0 * scale,
            rect.y + span.y0 * scale,
            (span.x1 - span.x0) * scale,
            (span.y1 - span.y0) * scale,
        );
    }
    cr.fill().ok();
    let Some(caret) = state.caret.filter(|c| c.page == page) else {
        return;
    };
    if !focused || !state.caret_on || !state.selection.is_empty() {
        return;
    }
    cr.set_source_rgb(0.0, 0.0, 0.0);
    let x = (rect.x + caret.x * scale).round() + 0.5;
    cr.set_line_width(1.0);
    cr.move_to(x, rect.y + caret.top * scale);
    cr.line_to(x, rect.y + caret.bottom * scale);
    cr.stroke().ok();
}

/// The marks of the invisible characters of a page, in the blue of Pages.
fn draw_invisibles(
    cr: &gtk::cairo::Context,
    state: &State,
    page: usize,
    rect: &PageRect,
    scale: f64,
) {
    let mut marks = state
        .invisibles
        .iter()
        .filter(|m| m.page == page)
        .peekable();
    if marks.peek().is_none() {
        return;
    }
    cr.set_source_rgb(0.25, 0.45, 0.85);
    cr.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    for mark in marks {
        cr.set_font_size((mark.ascent * 0.7 * scale).max(4.0));
        let text = mark.kind.mark();
        let Ok(extents) = cr.text_extents(text) else {
            continue;
        };
        let x = match mark.kind {
            Invisible::Paragraph | Invisible::LineBreak => mark.x0 * scale + 1.0,
            _ => (mark.x0 + mark.x1) / 2.0 * scale - extents.x_advance() / 2.0,
        };
        cr.move_to(rect.x + x, rect.y + mark.baseline * scale);
        cr.show_text(text).ok();
    }
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
