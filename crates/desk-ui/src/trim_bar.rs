// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bar of Edit Movie in the Movie tab of Pages 15.4 (W118): the
//! length of a movie or a sound, with the part that plays between two
//! handles to drag, and the poster frame of a movie on it. The Start,
//! Stop and Poster Frame rows under it set the same times with the
//! keyboard.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;

/// How near a mark the pointer takes it, in pixels.
const REACH: f64 = 12.0;

/// The space at each side of the bar, so that a handle at an end shows
/// whole, in pixels.
const SIDE: f64 = 8.0;

/// What the bar shows, in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Times {
    /// How long the movie or sound lasts.
    pub length: u32,
    /// Where it starts playing.
    pub start: u32,
    /// Where it stops.
    pub stop: u32,
    /// Where the poster frame is, for a movie.
    pub poster: Option<u32>,
}

/// A mark of the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Start,
    Stop,
    Poster,
}

impl Times {
    /// The times with `mark` moved to `at`: the start stays before the
    /// stop, and the poster frame within the length.
    pub fn moved(self, mark: Mark, at: u32) -> Self {
        let mut times = self;
        let at = at.min(self.length);
        match mark {
            Mark::Start => times.start = at.min(self.stop.saturating_sub(1)),
            Mark::Stop => times.stop = at.max(self.start + 1).min(self.length.max(1)),
            Mark::Poster => times.poster = self.poster.map(|_| at),
        }
        times
    }

    /// The mark nearest `x` of a bar `width` wide, if one is within
    /// reach: a handle before the poster frame.
    fn mark_at(self, x: f64, width: f64) -> Option<Mark> {
        let near = |ms: u32| (x_of(ms, self.length, width) - x).abs();
        let handles = [
            (Mark::Start, near(self.start)),
            (Mark::Stop, near(self.stop)),
        ];
        let handle = handles
            .into_iter()
            .filter(|(_, d)| *d <= REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        handle.map(|(m, _)| m).or_else(|| {
            self.poster
                .filter(|p| near(*p) <= REACH)
                .map(|_| Mark::Poster)
        })
    }
}

/// Where a time is on a bar `width` wide.
fn x_of(ms: u32, length: u32, width: f64) -> f64 {
    let part = if length == 0 {
        0.0
    } else {
        f64::from(ms.min(length)) / f64::from(length)
    };
    SIDE + part * (width - 2.0 * SIDE).max(0.0)
}

/// The time at `x` of a bar `width` wide.
fn ms_at(x: f64, length: u32, width: f64) -> u32 {
    let room = (width - 2.0 * SIDE).max(1.0);
    let part = ((x - SIDE) / room).clamp(0.0, 1.0);
    (part * f64::from(length)).round() as u32
}

/// What a drag of a mark calls, with the mark and its time.
type Moved = Box<dyn Fn(Mark, u32)>;

/// The bar.
pub struct TrimBar {
    area: gtk::DrawingArea,
    times: Cell<Option<Times>>,
    dragged: Cell<Option<Mark>>,
    moved: RefCell<Option<Moved>>,
}

impl TrimBar {
    pub fn new() -> Rc<Self> {
        let area = gtk::DrawingArea::builder()
            .content_height(36)
            .hexpand(true)
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        let this = Rc::new(Self {
            area,
            times: Cell::new(None),
            dragged: Cell::new(None),
            moved: RefCell::new(None),
        });
        {
            let weak = Rc::downgrade(&this);
            this.area.set_draw_func(move |area, cr, width, height| {
                if let Some(this) = weak.upgrade() {
                    this.draw(area, cr, f64::from(width), f64::from(height));
                }
            });
        }
        let drag = gtk::GestureDrag::new();
        {
            let weak = Rc::downgrade(&this);
            drag.connect_drag_begin(move |gesture, x, _| {
                let Some(this) = weak.upgrade() else { return };
                let width = f64::from(this.area.width());
                let mark = this.times.get().and_then(|t| t.mark_at(x, width));
                this.dragged.set(mark);
                if mark.is_none() {
                    gesture.set_state(gtk::EventSequenceState::Denied);
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            drag.connect_drag_update(move |gesture, dx, _| {
                let Some(this) = weak.upgrade() else { return };
                let (Some(mark), Some(times), Some((x, _))) =
                    (this.dragged.get(), this.times.get(), gesture.start_point())
                else {
                    return;
                };
                let width = f64::from(this.area.width());
                this.times
                    .set(Some(times.moved(mark, ms_at(x + dx, times.length, width))));
                this.area.queue_draw();
            });
        }
        {
            let weak = Rc::downgrade(&this);
            drag.connect_drag_end(move |_, _, _| {
                let Some(this) = weak.upgrade() else { return };
                let (Some(mark), Some(times)) = (this.dragged.take(), this.times.get()) else {
                    return;
                };
                let at = match mark {
                    Mark::Start => times.start,
                    Mark::Stop => times.stop,
                    Mark::Poster => times.poster.unwrap_or(0),
                };
                if let Some(moved) = &*this.moved.borrow() {
                    moved(mark, at);
                }
            });
        }
        this.area.add_controller(drag);
        let motion = gtk::EventControllerMotion::new();
        {
            let weak = Rc::downgrade(&this);
            motion.connect_motion(move |_, x, _| {
                let Some(this) = weak.upgrade() else { return };
                let width = f64::from(this.area.width());
                let over = this.times.get().and_then(|t| t.mark_at(x, width));
                this.area.set_cursor_from_name(over.map(|_| "ew-resize"));
            });
        }
        this.area.add_controller(motion);
        this
    }

    /// The bar, to put in a group.
    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    /// Calls `f` with a mark and its time when a drag of it ends.
    pub fn connect_moved(&self, f: impl Fn(Mark, u32) + 'static) {
        self.moved.replace(Some(Box::new(f)));
    }

    /// Shows the times, unless a mark is being dragged.
    pub fn show(&self, times: Times) {
        if self.dragged.get().is_none() {
            self.times.set(Some(times));
            self.area.queue_draw();
        }
    }

    fn draw(&self, area: &gtk::DrawingArea, cr: &gtk::cairo::Context, width: f64, height: f64) {
        let Some(times) = self.times.get() else {
            return;
        };
        let fg = area.color();
        let accent = adw::StyleManager::default().accent_color_rgba();
        let x = |ms: u32| x_of(ms, times.length, width);
        let (top, bottom) = (height * 0.3, height * 0.8);
        let rounded = |cr: &gtk::cairo::Context, x0: f64, x1: f64| {
            let r = 4.0f64.min((x1 - x0) / 2.0).max(0.0);
            cr.new_sub_path();
            cr.arc(x1 - r, top + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
            cr.arc(x1 - r, bottom - r, r, 0.0, std::f64::consts::FRAC_PI_2);
            cr.arc(
                x0 + r,
                bottom - r,
                r,
                std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
            );
            cr.arc(
                x0 + r,
                top + r,
                r,
                std::f64::consts::PI,
                1.5 * std::f64::consts::PI,
            );
            cr.close_path();
        };
        // The whole length, and the part that plays.
        cr.set_source_rgba(
            f64::from(fg.red()),
            f64::from(fg.green()),
            f64::from(fg.blue()),
            0.15,
        );
        rounded(cr, x(0), x(times.length));
        let _ = cr.fill();
        let (a, b) = (x(times.start), x(times.stop));
        cr.set_source_rgba(
            f64::from(accent.red()),
            f64::from(accent.green()),
            f64::from(accent.blue()),
            0.35,
        );
        rounded(cr, a, b);
        let _ = cr.fill();
        // The handles, as Pages draws them, at the ends of the part.
        cr.set_source_rgb(
            f64::from(accent.red()),
            f64::from(accent.green()),
            f64::from(accent.blue()),
        );
        for at in [a, b] {
            cr.rectangle(at - 2.5, top - 2.0, 5.0, bottom - top + 4.0);
            let _ = cr.fill();
        }
        // The poster frame, as a mark above the bar.
        if let Some(poster) = times.poster {
            let px = x(poster);
            cr.set_source_rgb(
                f64::from(fg.red()),
                f64::from(fg.green()),
                f64::from(fg.blue()),
            );
            cr.move_to(px - 5.0, 1.0);
            cr.line_to(px + 5.0, 1.0);
            cr.line_to(px, top - 3.0);
            cr.close_path();
            let _ = cr.fill();
            cr.set_line_width(1.0);
            cr.move_to(px, top);
            cr.line_to(px, bottom);
            let _ = cr.stroke();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w118_the_handles_keep_the_start_before_the_stop() {
        let times = Times {
            length: 10_000,
            start: 1_000,
            stop: 8_000,
            poster: Some(2_000),
        };
        assert_eq!(times.moved(Mark::Start, 9_000).start, 7_999);
        assert_eq!(times.moved(Mark::Stop, 500).stop, 1_001);
        assert_eq!(times.moved(Mark::Stop, 20_000).stop, 10_000);
        assert_eq!(times.moved(Mark::Poster, 20_000).poster, Some(10_000));
        // A sound has no poster frame to move.
        let sound = Times {
            poster: None,
            ..times
        };
        assert_eq!(sound.moved(Mark::Poster, 3_000).poster, None);
    }

    #[test]
    fn w118_the_pointer_takes_the_nearest_mark() {
        let times = Times {
            length: 10_000,
            start: 0,
            stop: 10_000,
            poster: Some(5_000),
        };
        let width = 216.0;
        assert_eq!(times.mark_at(SIDE + 3.0, width), Some(Mark::Start));
        assert_eq!(times.mark_at(width - SIDE, width), Some(Mark::Stop));
        assert_eq!(times.mark_at(width / 2.0, width), Some(Mark::Poster));
        assert_eq!(times.mark_at(60.0, width), None);
        assert_eq!(ms_at(width / 2.0, 10_000, width), 5_000);
        assert_eq!(ms_at(-50.0, 10_000, width), 0);
    }
}
