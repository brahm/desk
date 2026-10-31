// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Lengths, percentages and colors as ODF writes them.

use std::fmt;

/// A length, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Length(pub f64);

impl Length {
    /// Parses an ODF length such as `2.54cm`, `0.5in` or `12pt`.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let split = value
            .find(|c: char| c.is_ascii_alphabetic() || c == '%')
            .unwrap_or(value.len());
        let (number, unit) = value.split_at(split);
        let number: f64 = number.parse().ok()?;
        let points = match unit {
            "pt" => number,
            "in" => number * 72.0,
            "cm" => number * 72.0 / 2.54,
            "mm" => number * 72.0 / 25.4,
            "pc" => number * 12.0,
            "px" => number * 0.75,
            _ => return None,
        };
        points.is_finite().then_some(Self(points))
    }
}

impl fmt::Display for Length {
    /// Writes the length in points, with up to three decimals.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}pt", decimal(self.0))
    }
}

/// A number with up to three decimals and no trailing zeros.
pub(crate) fn decimal(value: f64) -> String {
    let text = format!("{:.3}", value);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// Parses a percentage such as `150%`.
pub(crate) fn parse_percent(value: &str) -> Option<f64> {
    let number: f64 = value.trim().strip_suffix('%')?.trim().parse().ok()?;
    number.is_finite().then_some(number)
}

/// An RGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Color {
    /// Parses a color written as `#rrggbb`.
    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.trim().strip_prefix('#')?;
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self {
            r: channel(0)?,
            g: channel(2)?,
            b: channel(4)?,
        })
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_in_every_unit() {
        assert_eq!(Length::parse("12pt"), Some(Length(12.0)));
        assert_eq!(Length::parse("1in"), Some(Length(72.0)));
        assert!((Length::parse("2.54cm").unwrap().0 - 72.0).abs() < 1e-9);
        assert!((Length::parse("25.4mm").unwrap().0 - 72.0).abs() < 1e-9);
        assert_eq!(Length::parse("-0.5in"), Some(Length(-36.0)));
        assert_eq!(Length::parse("10%"), None);
        assert_eq!(Length::parse("pt"), None);
        assert_eq!(Length(12.5).to_string(), "12.5pt");
        assert_eq!(Length(595.2756).to_string(), "595.276pt");
        assert_eq!(Length(-0.0001).to_string(), "0pt");
    }

    #[test]
    fn colors() {
        let color = Color::parse("#1A2b3c").unwrap();
        assert_eq!((color.r, color.g, color.b), (0x1a, 0x2b, 0x3c));
        assert_eq!(color.to_string(), "#1a2b3c");
        assert_eq!(Color::parse("red"), None);
        assert_eq!(Color::parse("#12345"), None);
    }
}
