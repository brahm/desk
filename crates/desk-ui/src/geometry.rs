// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the pages of a document sit in the document area, and the zoom
//! levels, apart from GTK so that they can be tested without a display.

/// Logical pixels per point at a zoom of 100%: the document shows at its
/// real size on a screen of 96 pixels per inch, as in the document viewers
/// of GNOME.
pub const PIXELS_PER_POINT: f64 = 96.0 / 72.0;

/// The space around and between pages, in logical pixels.
pub const PAGE_GAP: f64 = 24.0;

/// The zoom levels of the zoom menu, as fractions (INT-07). Pages '09
/// offers the same, from 25% to 400%.
pub const ZOOM_LEVELS: [f64; 9] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];

/// The zoom of a new window.
pub const DEFAULT_ZOOM: f64 = 1.0;

/// The next zoom level above `zoom`, or the largest.
pub fn zoom_in(zoom: f64) -> f64 {
    ZOOM_LEVELS
        .iter()
        .copied()
        .find(|&level| level > zoom + 1e-6)
        .unwrap_or(ZOOM_LEVELS[ZOOM_LEVELS.len() - 1])
}

/// The next zoom level below `zoom`, or the smallest.
pub fn zoom_out(zoom: f64) -> f64 {
    ZOOM_LEVELS
        .iter()
        .rev()
        .copied()
        .find(|&level| level < zoom - 1e-6)
        .unwrap_or(ZOOM_LEVELS[0])
}

/// The sizes of the text of Screen View, as View > Text Size of Pages
/// 15.4 steps through them with Bigger and Smaller, from 80% to 310%
/// (PXV-01).
pub const TEXT_SIZES: [f64; 12] = [
    0.8, 0.85, 0.9, 1.0, 1.1, 1.2, 1.35, 1.6, 1.9, 2.35, 2.75, 3.1,
];

/// The next text size of Screen View above `size`, or the largest.
pub fn text_bigger(size: f64) -> f64 {
    TEXT_SIZES
        .iter()
        .copied()
        .find(|&s| s > size + 1e-6)
        .unwrap_or(TEXT_SIZES[TEXT_SIZES.len() - 1])
}

/// The next text size of Screen View below `size`, or the smallest.
pub fn text_smaller(size: f64) -> f64 {
    TEXT_SIZES
        .iter()
        .rev()
        .copied()
        .find(|&s| s < size - 1e-6)
        .unwrap_or(TEXT_SIZES[0])
}

/// The text size of Text Size > Auto in Screen View, for a window as
/// wide as `width` logical pixels. Pages 15.4 sets 120% in a window 1031
/// points wide and 100% in one 560 points wide, the text flowing to the
/// window in both (PXV-01); Desk takes the larger from 800 on.
pub fn text_size_auto(width: f64) -> f64 {
    if width >= 800.0 { 1.2 } else { 1.0 }
}

/// A zoom as people read it, such as "125%".
pub fn zoom_label(zoom: f64) -> String {
    format!("{}%", (zoom * 100.0).round())
}

/// How the zoom follows the size of the window (INT-08).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// The pages fill the width.
    Width,
    /// A whole page shows.
    Page,
}

/// The zoom at which a page of `page_width` points fills `width` logical
/// pixels with the gap on each side, or two pages side by side with the
/// gap between them.
pub fn fit_width(page_width: f64, width: f64, columns: usize) -> f64 {
    let columns = columns.clamp(1, 2) as f64;
    let available = (width - 2.0 * PAGE_GAP - (columns - 1.0) * PAGE_GAP).max(1.0);
    (available / (columns * page_width * PIXELS_PER_POINT)).clamp(ZOOM_LEVELS[0], ZOOM_LEVELS[8])
}

/// The zoom at which a whole page of `page_width` by `page_height` points
/// fits in `width` by `height` logical pixels, or two side by side.
pub fn fit_page(page_width: f64, page_height: f64, width: f64, height: f64, columns: usize) -> f64 {
    let tall = (height - 2.0 * PAGE_GAP).max(1.0) / (page_height * PIXELS_PER_POINT);
    fit_width(page_width, width, columns)
        .min(tall)
        .max(ZOOM_LEVELS[0])
}

/// The zoom at which a box of `box_width` by `box_height` points fills
/// `width` by `height` logical pixels with the gap around it, as Zoom to
/// Selection and Zoom to Fit Content set it; it is not one of the levels.
pub fn fit_box(box_width: f64, box_height: f64, width: f64, height: f64) -> f64 {
    let wide = (width - 2.0 * PAGE_GAP).max(1.0) / (box_width.max(1.0) * PIXELS_PER_POINT);
    let tall = (height - 2.0 * PAGE_GAP).max(1.0) / (box_height.max(1.0) * PIXELS_PER_POINT);
    wide.min(tall).clamp(ZOOM_LEVELS[0], ZOOM_LEVELS[8])
}

/// The box around `points`, as its left, top, right and bottom edges.
pub fn bounds(points: impl IntoIterator<Item = (f64, f64)>) -> Option<(f64, f64, f64, f64)> {
    points.into_iter().fold(None, |bounds, (x, y)| {
        Some(match bounds {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        })
    })
}

/// A page placed in the document area, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageRect {
    /// The left edge.
    pub x: f64,
    /// The top edge.
    pub y: f64,
    /// The width.
    pub width: f64,
    /// The height.
    pub height: f64,
}

/// The pages stacked from top to bottom, centred in an area `area_width`
/// pixels wide, one to a row or two side by side (INT-08), and the size
/// that the area needs to show them all. Facing pages side by side make
/// spreads, as a book opens: the first page alone on the right, then each
/// left page with the right page after it (EST-11).
pub fn place_pages(
    sizes: &[(f64, f64)],
    zoom: f64,
    area_width: f64,
    columns: usize,
    book: bool,
) -> (Vec<PageRect>, f64, f64) {
    let scale = zoom * PIXELS_PER_POINT;
    let two = columns >= 2;
    let widest = sizes.iter().map(|&(w, _)| w * scale).fold(0.0, f64::max);
    let row = if two { 2.0 * widest + PAGE_GAP } else { widest };
    let content_width = (row + 2.0 * PAGE_GAP).max(area_width);
    let middle = content_width / 2.0;
    let mut y = PAGE_GAP;
    let mut rects = Vec::with_capacity(sizes.len());
    // The rows: their first page, and whether it is on the right.
    let mut rows: Vec<(usize, usize, bool)> = Vec::new();
    let mut start = 0;
    while start < sizes.len() {
        let (count, right) = if !two {
            (1, false)
        } else if book && start == 0 {
            (1, true)
        } else {
            (2.min(sizes.len() - start), false)
        };
        rows.push((start, count, right));
        start += count;
    }
    for (start, count, right) in rows {
        let mut tallest: f64 = 0.0;
        for i in 0..count {
            let (w, h) = sizes[start + i];
            let width = w * scale;
            let height = h * scale;
            let x = if !two {
                middle - width / 2.0
            } else if i == 0 && !right {
                middle - PAGE_GAP / 2.0 - width
            } else {
                middle + PAGE_GAP / 2.0
            };
            rects.push(PageRect {
                x: x.floor(),
                y,
                width,
                height,
            });
            tallest = tallest.max(height);
        }
        y += tallest + PAGE_GAP;
    }
    (rects, content_width, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_07_zoom_steps_through_the_levels_of_pages() {
        assert_eq!(zoom_in(1.0), 1.25);
        assert_eq!(zoom_in(1.1), 1.25);
        assert_eq!(zoom_in(4.0), 4.0);
        assert_eq!(zoom_out(1.0), 0.75);
        assert_eq!(zoom_out(0.25), 0.25);
        assert_eq!(zoom_label(1.25), "125%");
    }

    #[test]
    fn int_07_fit_width_and_fit_page() {
        // A4 is 595 points wide: 793 pixels at 100%.
        let zoom = fit_width(595.0, 793.0 + 2.0 * PAGE_GAP, 1);
        assert!((zoom - 1.0).abs() < 0.01, "{zoom}");
        let page = fit_page(
            595.0,
            842.0,
            2000.0,
            842.0 * PIXELS_PER_POINT / 2.0 + 48.0,
            1,
        );
        assert!((page - 0.5).abs() < 0.01, "{page}");
        // Two pages side by side fit at half the zoom, less the gap.
        let two = fit_width(595.0, 2.0 * 793.0 + 3.0 * PAGE_GAP, 2);
        assert!((two - 1.0).abs() < 0.01, "{two}");
    }

    #[test]
    fn pages_stack_centred_with_gaps() {
        let (rects, width, height) =
            place_pages(&[(612.0, 792.0), (612.0, 792.0)], 0.5, 1000.0, 1, false);
        assert_eq!(width, 1000.0);
        assert_eq!(rects[0].y, PAGE_GAP);
        assert_eq!(rects[1].y, PAGE_GAP * 2.0 + 792.0 * PIXELS_PER_POINT * 0.5);
        assert_eq!(rects[0].x, ((1000.0 - 408.0) / 2.0_f64).floor());
        assert_eq!(height, rects[1].y + rects[1].height + PAGE_GAP);
        // A narrow area grows to the widest page.
        let (_, width, _) = place_pages(&[(612.0, 792.0)], 1.0, 100.0, 1, false);
        assert_eq!(width, 816.0 + 2.0 * PAGE_GAP);
    }

    #[test]
    fn int_08_est_11_two_pages_side_by_side() {
        let page = (612.0, 792.0);
        let (rects, width, height) = place_pages(&[page, page, page], 0.5, 0.0, 2, false);
        assert_eq!(width, 2.0 * 408.0 + 3.0 * PAGE_GAP);
        assert_eq!(rects[0].y, rects[1].y);
        assert_eq!(rects[0].x, PAGE_GAP);
        assert_eq!(rects[1].x, PAGE_GAP + 408.0 + PAGE_GAP);
        // The third starts the next row, on the left.
        assert_eq!(rects[2].x, rects[0].x);
        assert_eq!(rects[2].y, rects[0].y + 528.0 + PAGE_GAP);
        assert_eq!(height, rects[2].y + 528.0 + PAGE_GAP);
        // Facing pages: the first alone on the right, then spreads.
        let (rects, _, _) = place_pages(&[page, page, page], 0.5, 0.0, 2, true);
        assert_eq!(rects[0].x, PAGE_GAP + 408.0 + PAGE_GAP);
        assert_eq!(rects[1].y, rects[2].y);
        assert!(rects[1].y > rects[0].y);
        assert_eq!(rects[1].x, PAGE_GAP);
    }

    #[test]
    fn zoom_to_selection_fits_the_box_in_the_view() {
        // A box of 300 by 100 points in a view of 848 by 448 pixels: the
        // width decides, 800 / 400 pixels.
        let zoom = fit_box(300.0, 100.0, 848.0, 448.0);
        assert!((zoom - 2.0).abs() < 1e-9);
        // A tiny box stops at the largest level, a huge one at the smallest.
        assert_eq!(fit_box(1.0, 1.0, 848.0, 448.0), ZOOM_LEVELS[8]);
        assert_eq!(fit_box(1e5, 1e5, 848.0, 448.0), ZOOM_LEVELS[0]);
        assert_eq!(
            bounds([(10.0, 40.0), (30.0, 5.0), (20.0, 20.0)]),
            Some((10.0, 5.0, 30.0, 40.0))
        );
        assert_eq!(bounds([]), None);
    }
}
