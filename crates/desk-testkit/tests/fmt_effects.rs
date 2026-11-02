// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! FMT-03, FMT-04, FMT-09, FMT-10, FMT-20 and FMT-21: documents saved by
//! Pages '09 on the reference machine with shadows, outlines, ligatures,
//! tracking, backgrounds and rules put their words where Pages put them
//! in its PDF, and draw their effects where Pages drew them.

use std::path::{Path, PathBuf};

use desk_render::{DisplayList, Item};
use desk_testkit::case::DEFAULT_POSITION_TOLERANCE;
use desk_testkit::compare::{compare_words, words};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages")
}

/// Lays out a fixture, writes it as PDF and checks its words against the
/// PDF of Pages; gives back the display list of its first page.
fn check(name: &str) -> DisplayList {
    let fixtures = fixtures();
    let document = desk_filters::read_pages_at(&fixtures.join(format!("{name}.pages"))).unwrap();
    let layout = desk_layout::paginate(&document, &mut desk_text::TextSystem::new());
    let pages = DisplayList::pages(&layout);
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.pdf"));
    desk_render::Painter::new()
        .unwrap()
        .write_pdf(&pages, document.metadata(), &out)
        .unwrap();
    let reference = words(&fixtures.join(format!("{name}.pdf"))).unwrap();
    let desk = words(&out).unwrap();
    let diff = compare_words(&reference, &desk);
    assert!(
        diff.passes(DEFAULT_POSITION_TOLERANCE),
        "{name}: {diff:?}\nPages: {:?}\nDesk: {:?}",
        reference[0],
        desk[0]
    );
    pages.into_iter().next().unwrap()
}

/// A line drawn: its points, whether it is closed and its dashes.
type Stroke = (Vec<(f32, f32)>, bool, Vec<f32>);

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

#[test]
fn fmt_10_tracking_spreads_letters_as_pages_does() {
    check("fmt-10-tracking");
}

#[test]
fn fmt_09_ligatures_follow_the_setting() {
    check("fmt-09-ligatures");
}

#[test]
fn fmt_03_fmt_04_shadows_and_outlines_are_drawn() {
    let list = check("fmt-03-04-shadow-outline");
    let shadows: Vec<_> = list
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Shadow {
                offset,
                blur,
                color,
                ..
            } => Some((*offset, *blur, color.a)),
            _ => None,
        })
        .collect();
    // Angle 300, offset 4, radius 3 and opacity 60% in the document.
    assert!(!shadows.is_empty());
    let ((dx, dy), blur, alpha) = shadows[0];
    assert!(close(dx, 2.0) && close(dy, 3.464), "{dx} {dy}");
    assert!(close(blur, 3.0));
    assert_eq!(alpha, 153);
    // Pages strokes the outlined text at a hundredth of its size, 24
    // points.
    let outlines: Vec<f32> = list
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Glyphs {
                outline: Some(width),
                ..
            } => Some(*width),
            _ => None,
        })
        .collect();
    assert!(!outlines.is_empty());
    assert!(outlines.iter().all(|w| close(*w, 0.24)));
}

#[test]
fn fmt_20_backgrounds_fall_where_pages_drew_them() {
    let list = check("fmt-20-backgrounds");
    let rects: Vec<(f32, f32, f32, f32)> = list
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Rect {
                x,
                y,
                width,
                height,
                ..
            } => Some((*x, *y, x + width, y + height)),
            _ => None,
        })
        .collect();
    // From the PDF of Pages: the paragraph from margin to margin over its
    // line, and the characters over theirs, on whole points.
    let paragraph = (56.69, 70.76, 538.31, 84.76);
    let characters = (56.69, 57.0, 175.43, 71.0);
    for (x0, y0, x1, y1) in [paragraph, characters] {
        assert!(
            rects.iter().any(|r| (r.0 - x0).abs() < 0.1
                && (r.1 - y0).abs() < 0.1
                && (r.2 - x1).abs() < 0.5
                && (r.3 - y1).abs() < 0.1),
            "no rectangle at {:?} in {rects:?}",
            (x0, y0, x1, y1)
        );
    }
}

#[test]
fn fmt_21_rules_and_borders_fall_where_pages_drew_them() {
    let list = check("fmt-21-borders-rules");
    let strokes: Vec<Stroke> = list
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Stroke {
                points,
                closed,
                dashes,
                ..
            } => Some((points.clone(), *closed, dashes.clone())),
            _ => None,
        })
        .collect();
    let rule = |y: f32| vec![(56.69292, y), (538.3071, y)];
    let expected = [
        (rule(57.5), false, vec![]),
        (rule(98.5), false, vec![]),
        (rule(99.5), false, vec![]),
        (rule(126.5), false, vec![]),
        (
            vec![
                (50.19292, 127.5),
                (544.8071, 127.5),
                (544.8071, 154.5),
                (50.19292, 154.5),
            ],
            true,
            vec![],
        ),
        (rule(175.5), false, vec![2.0, 2.0]),
    ];
    assert_eq!(strokes.len(), expected.len(), "{strokes:?}");
    for ((points, closed, dashes), (want, want_closed, want_dashes)) in
        strokes.iter().zip(&expected)
    {
        assert_eq!(closed, want_closed);
        assert_eq!(dashes, want_dashes);
        assert!(
            points
                .iter()
                .zip(want)
                .all(|(a, b)| close(a.0, b.0) && close(a.1, b.1)),
            "{points:?} is not {want:?}"
        );
    }
}
