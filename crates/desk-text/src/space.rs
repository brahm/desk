// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The space that lines go into.

/// Where the lines of a paragraph may go. A plain column gives every line
/// its whole width; a column with wrapped objects gives each line the
/// pieces the objects leave free at that height.
pub trait LineSpace {
    /// The width of the column, which the paragraph indents are measured
    /// within.
    fn width(&self) -> f32;

    /// The free pieces of the column for a line whose top is at `top` and
    /// that is `height` tall, as `(x, width)` from left to right.
    fn segments(&self, top: f32, height: f32) -> Vec<(f32, f32)>;

    /// Whether nothing below `top` is free, so the layout should stop
    /// looking for room. A column without a bottom never is.
    fn is_exhausted(&self, top: f32) -> bool {
        let _ = top;
        false
    }
}

/// A column of a fixed width with nothing in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Column {
    /// The width.
    pub width: f32,
}

impl Column {
    /// A column of a width, in points.
    pub fn new(width: f32) -> Self {
        Self { width }
    }
}

impl LineSpace for Column {
    fn width(&self) -> f32 {
        self.width
    }

    fn segments(&self, _top: f32, _height: f32) -> Vec<(f32, f32)> {
        vec![(0.0, self.width)]
    }
}
