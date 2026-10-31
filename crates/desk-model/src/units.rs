// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lengths and colors.

use std::fmt;
use std::ops::{Add, Neg, Sub};

/// A length, stored as a whole number of thousandths of a point.
///
/// Whole numbers keep the model comparable and hashable, so two documents
/// with the same content are equal. A thousandth of a point is finer than
/// any unit that the ODF or iWork files use in practice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Length(i32);

impl Length {
    /// A length of zero.
    pub const ZERO: Self = Self(0);

    /// Makes a length from thousandths of a point.
    pub const fn from_millipoints(millipoints: i32) -> Self {
        Self(millipoints)
    }

    /// Makes a length from whole points.
    pub const fn from_points(points: i32) -> Self {
        Self(points * 1000)
    }

    /// Makes a length from points, rounded to the nearest thousandth.
    pub fn from_pt(points: f64) -> Self {
        Self((points * 1000.0).round() as i32)
    }

    /// Makes a length from inches.
    pub fn from_in(inches: f64) -> Self {
        Self::from_pt(inches * 72.0)
    }

    /// Makes a length from centimeters.
    pub fn from_cm(centimeters: f64) -> Self {
        Self::from_pt(centimeters * 72.0 / 2.54)
    }

    /// Makes a length from millimeters.
    pub fn from_mm(millimeters: f64) -> Self {
        Self::from_cm(millimeters / 10.0)
    }

    /// The length in thousandths of a point.
    pub const fn millipoints(self) -> i32 {
        self.0
    }

    /// The length in points.
    pub fn to_pt(self) -> f64 {
        f64::from(self.0) / 1000.0
    }
}

impl Add for Length {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Length {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl Neg for Length {
    type Output = Self;

    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl fmt::Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}pt", self.to_pt())
    }
}

/// A color in sRGB with straight alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// Red, from 0 to 255.
    pub r: u8,
    /// Green, from 0 to 255.
    pub g: u8,
    /// Blue, from 0 to 255.
    pub b: u8,
    /// Opacity, from 0 (transparent) to 255 (opaque).
    pub a: u8,
}

impl Rgba {
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    /// Makes an opaque color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_convert_to_points() {
        assert_eq!(Length::from_in(1.0), Length::from_points(72));
        assert_eq!(Length::from_cm(2.54), Length::from_points(72));
        assert_eq!(Length::from_mm(210.0).millipoints(), 595_276);
        assert_eq!(Length::from_pt(10.5).to_pt(), 10.5);
        assert_eq!(
            Length::from_points(3) - Length::from_points(5),
            -Length::from_points(2)
        );
    }
}
