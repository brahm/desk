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
    /// One page to a row, or two side by side (INT-08).
    columns: usize,
    /// Whether the pages face each other, which side by side makes
    /// spreads (EST-11).
    book: bool,
    /// Whether the zoom follows the size of the window (INT-08).
    fit: Option<geometry::Fit>,
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
    /// The color they are drawn in.
    invisibles_color: desk_model::Rgba,
    /// The lines under misspelled words and mistakes (TXT-10, TXT-11).
    squiggles: Vec<Squiggle>,
    /// The color around the pages that full screen chose (DOC-07); none
    /// leaves the grey of the window.
    backdrop: Option<desk_model::Rgba>,
    /// The outlines and handles of the selected objects (OBJ-03).
    objects: Vec<ObjectMark>,
    /// The alignment guides that a drag of objects snaps to (OBJ-05).
    guides: Vec<Guide>,
    path: Option<PathMark>,
}

/// A selected object, drawn over the pages: its outline and its handles,
/// in points on its page.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectMark {
    /// The page it is on.
    pub page: usize,
    /// The corners of its frame, rotated, in order around it; a line has
    /// two.
    pub outline: Vec<(f64, f64)>,
    /// Where its handles are.
    pub handles: Vec<(f64, f64)>,
    /// Whether it is locked, which its handles show.
    pub locked: bool,
}

/// A shape being drawn with the pen or having its points edited: its
/// outline, its points and the handles of its curves (OBJ-09, OBJ-10).
#[derive(Clone, Debug, PartialEq)]
pub struct PathMark {
    /// The page it is on.
    pub page: usize,
    /// The outline, in points on the page.
    pub outline: Vec<desk_model::PathSegment>,
    /// Where its points are, and whether each is selected.
    pub points: Vec<((f64, f64), bool)>,
    /// The handles of its curves, from their point.
    pub handles: Vec<((f64, f64), (f64, f64))>,
}

/// An alignment guide, from one point of a page to another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    /// The page it is on.
    pub page: usize,
    /// Where it starts.
    pub from: (f64, f64),
    /// Where it ends.
    pub to: (f64, f64),
}

/// A line under text that the spelling checker or the proofreader marks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Squiggle {
    /// Where the text is.
    pub span: SpanRect,
    /// Whether it is a mistake of the proofreader, drawn in green, rather
    /// than a misspelling, in red.
    pub proofreading: bool,
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
            columns: 1,
            book: false,
            fit: None,
            painter: Painter::new().ok(),
            on_zoom: Vec::new(),
            on_pages: Vec::new(),
            selection: Vec::new(),
            caret: None,
            caret_on: true,
            invisibles: Vec::new(),
            invisibles_color: crate::preferences::INVISIBLES_COLOR,
            backdrop: None,
            objects: Vec::new(),
            guides: Vec::new(),
            path: None,
            squiggles: Vec::new(),
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
        // A zoom that fits the window follows it.
        for adjustment in [view.scrolled.hadjustment(), view.scrolled.vadjustment()] {
            let weak = view.downgrade();
            adjustment.connect_page_size_notify(move |_| {
                if let Some(view) = weak.upgrade() {
                    view.refit();
                }
            });
        }
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

    /// Sets the selected objects and the guides drawn over the pages.
    pub fn set_object_marks(&self, objects: Vec<ObjectMark>, guides: Vec<Guide>) {
        {
            let mut state = self.state.borrow_mut();
            if state.objects == objects && state.guides == guides {
                return;
            }
            state.objects = objects;
            state.guides = guides;
        }
        self.area.queue_draw();
    }

    /// Shows a shape being drawn with the pen or edited, or none.
    pub fn set_path_mark(&self, mark: Option<PathMark>) {
        if self.state.borrow().path == mark {
            return;
        }
        self.state.borrow_mut().path = mark;
        self.area.queue_draw();
    }

    /// Sets the lines under misspelled words and mistakes.
    pub fn set_squiggles(&self, squiggles: Vec<Squiggle>) {
        let changed = {
            let mut state = self.state.borrow_mut();
            let changed = state.squiggles != squiggles;
            state.squiggles = squiggles;
            changed
        };
        if changed {
            self.area.queue_draw();
        }
    }

    /// Sets the color of the invisible characters (INT-10).
    pub fn set_invisibles_color(&self, color: desk_model::Rgba) {
        self.state.borrow_mut().invisibles_color = color;
        self.area.queue_draw();
    }

    /// Sets the color around the pages, or gives back the window's
    /// (DOC-07).
    pub fn set_backdrop(&self, color: Option<desk_model::Rgba>) {
        self.state.borrow_mut().backdrop = color;
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
                    let dy = if y < r.y {
                        r.y - y
                    } else if y > r.y + r.height {
                        y - r.y - r.height
                    } else {
                        0.0
                    };
                    // Two pages side by side are as far down.
                    let dx = if x < r.x {
                        r.x - x
                    } else if x > r.x + r.width {
                        x - r.x - r.width
                    } else {
                        0.0
                    };
                    dy + dx
                };
                d(a).total_cmp(&d(b))
            })?
            .0;
        let rect = rects[index];
        Some((index, (x - rect.x) / scale, (y - rect.y) / scale))
    }

    /// The page in the middle of the part of the view in sight.
    pub fn current_page(&self) -> Option<usize> {
        let (h, v) = (self.scrolled.hadjustment(), self.scrolled.vadjustment());
        self.page_at(
            h.value() + h.page_size() / 2.0,
            v.value() + v.page_size() / 3.0,
        )
        .map(|(page, _, _)| page)
    }

    /// Scrolls so that the top of a page is at the top of the view.
    pub fn scroll_to_page(&self, page: usize) {
        let (rects, _, _) = self.layout(f64::from(self.area.width()));
        if let Some(rect) = rects.get(page) {
            self.scrolled
                .vadjustment()
                .set_value(rect.y - geometry::PAGE_GAP / 2.0);
        }
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

    /// Sets the zoom, keeping the middle of the view where it was. The
    /// zoom no longer follows the window.
    pub fn set_zoom(&self, zoom: f64) {
        self.state.borrow_mut().fit = None;
        self.apply_zoom(zoom);
    }

    /// Makes the zoom fit the window, and follow it, or not.
    pub fn set_fit(&self, fit: Option<geometry::Fit>) {
        self.state.borrow_mut().fit = fit;
        self.refit();
    }

    /// How the zoom follows the window, if it does.
    pub fn fit(&self) -> Option<geometry::Fit> {
        self.state.borrow().fit
    }

    /// Shows one page to a row, or two side by side (INT-08).
    pub fn set_columns(&self, columns: usize) {
        let columns = columns.clamp(1, 2);
        if self.columns() == columns {
            return;
        }
        self.state.borrow_mut().columns = columns;
        self.resize();
        self.refit();
    }

    /// Shows facing pages side by side as spreads (EST-11).
    pub fn set_book(&self, book: bool) {
        if self.state.borrow().book != book {
            self.state.borrow_mut().book = book;
            self.resize();
        }
    }

    /// How many pages show side by side.
    pub fn columns(&self) -> usize {
        self.state.borrow().columns
    }

    fn refit(&self) {
        let zoom = match self.fit() {
            Some(geometry::Fit::Width) => self.fit_width_zoom(),
            Some(geometry::Fit::Page) => self.fit_page_zoom(),
            None => return,
        };
        if self.scrolled.width() > 0 {
            self.apply_zoom(zoom);
        }
    }

    fn apply_zoom(&self, zoom: f64) {
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
        geometry::fit_width(width, f64::from(self.scrolled.width()), self.columns())
    }

    /// The zoom at which a whole page fits in the view.
    pub fn fit_page_zoom(&self) -> f64 {
        let (width, height) = self.largest_page();
        geometry::fit_page(
            width,
            height,
            f64::from(self.scrolled.width()),
            f64::from(self.scrolled.height()),
            self.columns(),
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
        let book = self.state.borrow().book;
        geometry::place_pages(&self.sizes(), self.zoom(), area_width, self.columns(), book)
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
            let (rects, _, _) = geometry::place_pages(
                &sizes,
                state.zoom,
                f64::from(width),
                state.columns,
                state.book,
            );
            let vadj = scrolled.vadjustment();
            let (top, bottom) = (vadj.value(), vadj.value() + vadj.page_size());
            let scale = state.zoom * PIXELS_PER_POINT;
            let dark = adw_is_dark(area);
            if let Some(color) = state.backdrop {
                cr.set_source_rgba(
                    f64::from(color.r) / 255.0,
                    f64::from(color.g) / 255.0,
                    f64::from(color.b) / 255.0,
                    f64::from(color.a) / 255.0,
                );
                cr.paint().ok();
            }
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
    draw_squiggles(cr, state, page, rect, scale);
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
    draw_objects(cr, state, page, rect, scale);
    let Some(caret) = state.caret.filter(|c| c.page == page) else {
        return;
    };
    if !focused || !state.caret_on || !state.selection.is_empty() || !state.objects.is_empty() {
        return;
    }
    cr.set_source_rgb(0.0, 0.0, 0.0);
    let x = (rect.x + caret.x * scale).round() + 0.5;
    cr.set_line_width(1.0);
    cr.move_to(x, rect.y + caret.top * scale);
    cr.line_to(x, rect.y + caret.bottom * scale);
    cr.stroke().ok();
}

/// The selected objects, with their outline in the accent color and
/// white handles, or grey crosses when they are locked, as Pages draws
/// them; and the guides, in yellow.
fn draw_objects(cr: &gtk::cairo::Context, state: &State, page: usize, rect: &PageRect, scale: f64) {
    let at = |(x, y): (f64, f64)| (rect.x + x * scale, rect.y + y * scale);
    cr.save().ok();
    cr.set_line_width(1.0);
    cr.set_source_rgb(0.96, 0.76, 0.07);
    for guide in state.guides.iter().filter(|g| g.page == page) {
        let (x0, y0) = at(guide.from);
        let (x1, y1) = at(guide.to);
        cr.move_to(x0.round() + 0.5, y0.round() + 0.5);
        cr.line_to(x1.round() + 0.5, y1.round() + 0.5);
    }
    cr.stroke().ok();
    for mark in state.objects.iter().filter(|m| m.page == page) {
        cr.set_source_rgb(0.21, 0.52, 0.89);
        for (i, &point) in mark.outline.iter().enumerate() {
            let (x, y) = at(point);
            if i == 0 {
                cr.move_to(x, y);
            } else {
                cr.line_to(x, y);
            }
        }
        if mark.outline.len() > 2 {
            cr.close_path();
        }
        cr.stroke().ok();
        const HALF: f64 = 3.5;
        for &point in &mark.handles {
            let (x, y) = at(point);
            let (x, y) = (x.round() + 0.5, y.round() + 0.5);
            if mark.locked {
                cr.set_source_rgb(0.45, 0.45, 0.45);
                cr.move_to(x - HALF, y - HALF);
                cr.line_to(x + HALF, y + HALF);
                cr.move_to(x + HALF, y - HALF);
                cr.line_to(x - HALF, y + HALF);
                cr.stroke().ok();
            } else {
                cr.rectangle(x - HALF, y - HALF, 2.0 * HALF, 2.0 * HALF);
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.fill_preserve().ok();
                cr.set_source_rgb(0.21, 0.52, 0.89);
                cr.stroke().ok();
            }
        }
    }
    if let Some(mark) = state.path.as_ref().filter(|m| m.page == page) {
        use desk_model::PathSegment;
        cr.set_source_rgb(0.21, 0.52, 0.89);
        for segment in &mark.outline {
            match *segment {
                PathSegment::Move(x, y) => {
                    let (x, y) = at((x, y));
                    cr.move_to(x, y);
                }
                PathSegment::Line(x, y) => {
                    let (x, y) = at((x, y));
                    cr.line_to(x, y);
                }
                PathSegment::Curve(ax, ay, bx, by, x, y) => {
                    let (ax, ay) = at((ax, ay));
                    let (bx, by) = at((bx, by));
                    let (x, y) = at((x, y));
                    cr.curve_to(ax, ay, bx, by, x, y);
                }
                PathSegment::Close => cr.close_path(),
            }
        }
        cr.stroke().ok();
        // The handles, in gray, with a dot at their end.
        cr.set_source_rgb(0.45, 0.45, 0.45);
        for &(from, to) in &mark.handles {
            let (x0, y0) = at(from);
            let (x1, y1) = at(to);
            cr.move_to(x0, y0);
            cr.line_to(x1, y1);
            cr.stroke().ok();
            cr.arc(x1, y1, 2.5, 0.0, std::f64::consts::TAU);
            cr.fill().ok();
        }
        // The points, in red as Pages shows them, filled when selected.
        const HALF: f64 = 3.0;
        for &(point, selected) in &mark.points {
            let (x, y) = at(point);
            let (x, y) = (x.round() + 0.5, y.round() + 0.5);
            cr.rectangle(x - HALF, y - HALF, 2.0 * HALF, 2.0 * HALF);
            if selected {
                cr.set_source_rgb(0.88, 0.11, 0.14);
            } else {
                cr.set_source_rgb(1.0, 1.0, 1.0);
            }
            cr.fill_preserve().ok();
            cr.set_source_rgb(0.88, 0.11, 0.14);
            cr.stroke().ok();
        }
    }
    cr.restore().ok();
}

/// The dashed lines under misspelled words, in red, and mistakes, in
/// green, as Pages draws them.
fn draw_squiggles(
    cr: &gtk::cairo::Context,
    state: &State,
    page: usize,
    rect: &PageRect,
    scale: f64,
) {
    for proofreading in [false, true] {
        let mut lines = state
            .squiggles
            .iter()
            .filter(|s| s.span.page == page && s.proofreading == proofreading)
            .peekable();
        if lines.peek().is_none() {
            continue;
        }
        if proofreading {
            cr.set_source_rgb(0.18, 0.62, 0.25);
        } else {
            cr.set_source_rgb(0.86, 0.12, 0.12);
        }
        cr.set_line_width(1.5);
        cr.set_dash(&[2.0, 2.0], 0.0);
        for line in lines {
            let span = &line.span;
            // A little above the bottom of the line, under the descenders.
            let y = (rect.y + span.y1 * scale - 1.5).round() + 0.25;
            cr.move_to(rect.x + span.x0 * scale, y);
            cr.line_to(rect.x + span.x1 * scale, y);
        }
        cr.stroke().ok();
        cr.set_dash(&[], 0.0);
    }
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
    let color = state.invisibles_color;
    cr.set_source_rgba(
        f64::from(color.r) / 255.0,
        f64::from(color.g) / 255.0,
        f64::from(color.b) / 255.0,
        f64::from(color.a) / 255.0,
    );
    cr.select_font_face(
        "Sans",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Normal,
    );
    for mark in marks {
        let size = (mark.ascent * 0.7 * scale).max(4.0);
        let Some(text) = mark.kind.mark() else {
            draw_break(
                cr,
                mark.kind,
                rect.x + mark.x0 * scale + 1.0,
                rect.y + mark.baseline * scale,
                size,
            );
            continue;
        };
        cr.set_font_size(size);
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

/// The mark of a break, as the table of invisibles of Pages draws it: a
/// sheet with a folded corner for a page, a frame split in two for a
/// column, a frame with a band above for a layout, and a sheet with a
/// bar and lines for a section. It stands on the baseline at `x`, `size`
/// high.
fn draw_break(cr: &gtk::cairo::Context, kind: Invisible, x: f64, baseline: f64, size: f64) {
    let (h, w) = (size, size * 0.75);
    let (left, top) = (x, baseline - h);
    cr.set_line_width((size / 10.0).max(1.0));
    let line = |x0: f64, y0: f64, x1: f64, y1: f64| {
        cr.move_to(left + x0, top + y0);
        cr.line_to(left + x1, top + y1);
    };
    match kind {
        Invisible::PageBreak => {
            let fold = w * 0.35;
            cr.move_to(left, top);
            cr.line_to(left + w - fold, top);
            cr.line_to(left + w, top + fold);
            cr.line_to(left + w, top + h);
            cr.line_to(left, top + h);
            cr.close_path();
            line(w - fold, 0.0, w - fold, fold);
            cr.line_to(left + w, top + fold);
        }
        Invisible::ColumnBreak => {
            cr.rectangle(left, top, w, h);
            line(w / 2.0, 0.0, w / 2.0, h);
        }
        Invisible::LayoutBreak => {
            cr.rectangle(left, top, w, h);
            line(0.0, h * 0.25, w, h * 0.25);
            line(w / 2.0, h * 0.25, w / 2.0, h);
        }
        Invisible::SectionBreak => {
            cr.rectangle(left, top, w, h);
            cr.stroke().ok();
            cr.rectangle(left, top, w * 0.3, h);
            cr.fill().ok();
            line(w * 0.5, h * 0.35, w * 0.8, h * 0.35);
            line(w * 0.5, h * 0.55, w * 0.8, h * 0.55);
        }
        _ => {}
    }
    cr.stroke().ok();
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
