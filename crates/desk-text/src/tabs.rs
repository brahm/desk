// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where tabs go (FMT-16).
//!
//! A tab reaches the first stop past the place it starts, and the text
//! after it, up to the next tab or the end of the line, lines up with the
//! stop: it starts there, ends there, is centered on it, or has its decimal
//! separator there. Past the last stop of the paragraph, tabs go to the
//! default stops, which are evenly spaced from the left edge of the text.
//! Before the left indent, as on the first line of a hanging indent, a tab
//! goes to the indent when no stop comes first.
//!
//! Positions are distances from the left edge of the text, as the ruler
//! shows them.

use desk_model::{ParaFormat, TabAlignment, TabLeader};

/// How close two positions must be to be the same, so that a tab right at
/// a stop goes to the next one.
const EPSILON: f32 = 0.01;

/// The tab stops of a paragraph, in points.
#[derive(Clone, Debug)]
pub(crate) struct TabRuler {
    stops: Vec<Stop>,
    interval: f32,
    indent: f32,
    /// A stop that comes after the custom ones and before the default
    /// ones, as in a list item (step 19n of W1).
    hidden: Option<f32>,
}

/// A stop, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stop {
    pub position: f32,
    pub alignment: TabAlignment,
    pub decimal: char,
    pub leader: TabLeader,
}

impl Stop {
    /// A left stop without a leader, as the default stops are.
    fn plain(position: f32) -> Self {
        Self {
            position,
            alignment: TabAlignment::Left,
            decimal: '.',
            leader: TabLeader::None,
        }
    }
}

/// A piece of a line: a tab or a cluster of text, in order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Piece {
    /// Whether it is a tab.
    pub tab: bool,
    /// Its width, which for a tab is ignored.
    pub advance: f32,
    /// Its first character.
    pub first: char,
    /// How a space after it would kern with it: Pages measures text
    /// aligned on a stop up to the start of what follows, a tab or the
    /// end of the paragraph, which it draws as a space (step 19n of W1).
    pub kern: f32,
}

/// Where a tab went: where it starts and how wide it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TabPlace {
    pub start: f32,
    pub width: f32,
    pub stop: Stop,
}

/// A tab waiting for the text after it to be measured.
struct Pending {
    start: f32,
    stop: Stop,
    /// The width of the text after the tab so far.
    text: f32,
    /// The width of that text up to the middle of its decimal
    /// separator, once found.
    before_decimal: Option<f32>,
    /// How the last piece of that text kerns with a space after it.
    kern: f32,
}

impl TabRuler {
    /// The stops of a paragraph format.
    pub fn new(format: &ParaFormat) -> Self {
        Self {
            stops: format
                .tab_stops
                .stops()
                .iter()
                .map(|s| Stop {
                    position: s.position.to_pt() as f32,
                    alignment: s.alignment,
                    decimal: s.decimal,
                    leader: s.leader,
                })
                .collect(),
            interval: (format.default_tab_interval.to_pt() as f32).max(1.0),
            indent: format.left_indent.to_pt() as f32,
            hidden: None,
        }
    }

    /// The same stops for a list item, or for Screen View: no stop at the
    /// left indent, and a `hidden` one after the custom stops (step 19n of
    /// W1).
    pub fn listed(mut self, hidden: Option<f32>) -> Self {
        self.indent = f32::NEG_INFINITY;
        self.hidden = hidden;
        self
    }

    /// The same stops, which Pages 15.4 keeps short of the edge of a
    /// column `width` wide, whatever the right indent: a left stop 25
    /// points before it at most, a centered or decimal one 12.5, a right
    /// one 1 (step 19m of W1, in steps of a twip).
    pub fn clamped(mut self, width: f32) -> Self {
        for stop in &mut self.stops {
            let room = match stop.alignment {
                TabAlignment::Left => 25.0,
                TabAlignment::Center | TabAlignment::Decimal => 12.5,
                TabAlignment::Right => 1.0,
            };
            stop.position = stop.position.min(width - room);
        }
        self
    }

    /// The stop that a tab starting at `x` goes to.
    pub fn next(&self, x: f32) -> Stop {
        let custom = self
            .stops
            .iter()
            .find(|s| s.position > x + EPSILON)
            .copied();
        if x + EPSILON < self.indent && custom.is_none_or(|s| s.position > self.indent) {
            return Stop::plain(self.indent);
        }
        custom
            .or_else(|| self.hidden.filter(|&h| h > x + EPSILON).map(Stop::plain))
            .unwrap_or_else(|| {
                Stop::plain((((x + EPSILON) / self.interval).floor() + 1.0) * self.interval)
            })
    }

    /// Lays out the pieces of a line that starts at `x` and may reach
    /// `limit`: where each tab goes, and where the line ends.
    pub fn lay(
        &self,
        pieces: impl IntoIterator<Item = Piece>,
        x: f32,
        limit: f32,
    ) -> (Vec<TabPlace>, f32) {
        let mut tabs = Vec::new();
        let mut x = x;
        let mut pending: Option<Pending> = None;
        let resolve = |pending: Pending, tabs: &mut Vec<TabPlace>| -> f32 {
            let stop = pending.stop;
            // The text up to the start of what follows it.
            let aligned = pending.text + pending.kern;
            let start = match stop.alignment {
                TabAlignment::Left => stop.position,
                TabAlignment::Right => stop.position - aligned,
                TabAlignment::Center => stop.position - aligned / 2.0,
                // On the middle of the separator, as kerned (step 19n).
                TabAlignment::Decimal => stop.position - pending.before_decimal.unwrap_or(aligned),
            };
            // A tab never goes back, nor past the end of the line.
            let end = start.max(pending.start).min(limit.max(pending.start));
            tabs.push(TabPlace {
                start: pending.start,
                width: end - pending.start,
                stop,
            });
            end + pending.text
        };
        for piece in pieces {
            if piece.tab {
                if let Some(p) = pending.take() {
                    x = resolve(p, &mut tabs);
                }
                pending = Some(Pending {
                    start: x,
                    stop: self.next(x),
                    text: 0.0,
                    before_decimal: None,
                    kern: 0.0,
                });
            } else if let Some(p) = &mut pending {
                if p.stop.alignment == TabAlignment::Decimal
                    && p.before_decimal.is_none()
                    && piece.first == p.stop.decimal
                {
                    p.before_decimal = Some(p.text + piece.advance / 2.0);
                }
                p.text += piece.advance;
                p.kern = piece.kern;
            } else {
                x += piece.advance;
            }
        }
        if let Some(p) = pending.take() {
            x = resolve(p, &mut tabs);
        }
        (tabs, x)
    }
}

#[cfg(test)]
mod tests {
    use desk_model::{Length, TabStop, TabStops};

    use super::*;

    fn ruler(stops: &[(i32, TabAlignment)], indent: i32) -> TabRuler {
        TabRuler::new(&ParaFormat {
            tab_stops: TabStops::new(stops.iter().map(|&(p, alignment)| TabStop {
                alignment,
                decimal: ',',
                ..TabStop::left(Length::from_points(p))
            })),
            left_indent: Length::from_points(indent),
            ..ParaFormat::default()
        })
    }

    fn text(s: &str) -> Vec<Piece> {
        s.chars()
            .map(|c| Piece {
                tab: c == '\t',
                advance: 10.0,
                first: c,
                kern: 0.0,
            })
            .collect()
    }

    #[test]
    fn fmt_16_each_alignment_lines_up_with_its_stop() {
        let lay = |alignment, s: &str| ruler(&[(100, alignment)], 0).lay(text(s), 0.0, 400.0);
        // "ab" ends at 20, and "cdef" is 40 wide.
        let (tabs, end) = lay(TabAlignment::Left, "ab\tcdef");
        assert_eq!((tabs[0].start, tabs[0].width, end), (20.0, 80.0, 140.0));
        let (tabs, end) = lay(TabAlignment::Right, "ab\tcdef");
        assert_eq!((tabs[0].width, end), (40.0, 100.0));
        let (tabs, end) = lay(TabAlignment::Center, "ab\tcdef");
        assert_eq!((tabs[0].width, end), (60.0, 120.0));
        // The middle of the separator of "12,50" is at the stop.
        let (tabs, end) = lay(TabAlignment::Decimal, "ab\t12,50");
        assert_eq!((tabs[0].width, end), (55.0, 125.0));
        // Without a separator, the text ends at the stop.
        let (_, end) = lay(TabAlignment::Decimal, "ab\t1250");
        assert_eq!(end, 100.0);
    }

    #[test]
    fn fmt_16_default_stops_come_after_the_last_stop() {
        let ruler = ruler(&[(50, TabAlignment::Left)], 0);
        let (tabs, _) = ruler.lay(text("\ta\ta\ta"), 0.0, 400.0);
        let starts: Vec<f32> = tabs.iter().map(|t| t.start + t.width).collect();
        assert_eq!(starts, [50.0, 72.0, 108.0]);
        // A tab right at a stop goes to the next one.
        assert_eq!(ruler.next(72.0).position, 108.0);
    }

    #[test]
    fn fmt_16_a_tab_goes_to_a_hanging_indent_and_never_back() {
        let ruler = ruler(&[], 30);
        assert_eq!(ruler.next(10.0).position, 30.0);
        // Text wider than the space before a right stop pushes it on.
        let ruler = super::tests::ruler(&[(30, TabAlignment::Right)], 0);
        let (tabs, end) = ruler.lay(text("ab\tcdef"), 0.0, 400.0);
        assert_eq!((tabs[0].width, end), (0.0, 60.0));
        // A tab stops at the end of the line.
        let ruler = super::tests::ruler(&[(300, TabAlignment::Left)], 0);
        let (tabs, _) = ruler.lay(text("ab\tc"), 0.0, 200.0);
        assert_eq!(tabs[0].width, 180.0);
    }
}
