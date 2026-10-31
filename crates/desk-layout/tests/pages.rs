// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pagination, named by the IDs of the parity matrix. The paragraphs are
//! made of short lines ended by manual line breaks, so each one has a
//! known number of lines, in Liberation Sans as in the desk-text tests.

use std::sync::Arc;

use desk_layout::{DocumentLayout, paginate, paginate_texts};
use desk_model::{CharFormat, Document, LINE_BREAK, Length, PageSetup, ParaFormat};
use desk_text::{ParagraphText, TextSystem};

/// A page with a body 200pt wide and `body_height` tall, with margins of
/// 20pt at the top, 40pt at the bottom and right and 50pt at the left.
fn setup(body_height: f32) -> PageSetup {
    PageSetup {
        width: Length::from_points(290),
        height: Length::from_pt(f64::from(body_height) + 60.0),
        margin_top: Length::from_points(20),
        margin_bottom: Length::from_points(40),
        margin_left: Length::from_points(50),
        margin_right: Length::from_points(40),
    }
}

fn sans() -> CharFormat {
    CharFormat {
        font_family: Arc::from("Liberation Sans"),
        ..CharFormat::default()
    }
}

fn para(lines: usize, format: ParaFormat) -> ParagraphText {
    let text = vec!["line"; lines].join(&LINE_BREAK.to_string());
    ParagraphText::from_runs(format, sans(), [(text.as_str(), sans())])
}

fn line_height(system: &mut TextSystem) -> f32 {
    let layout = paginate_texts(setup(500.0), &[para(1, ParaFormat::default())], system);
    layout.pages[0].lines[0].line.height
}

/// For each page, the paragraph and line of each line on it.
fn shape(layout: &DocumentLayout) -> Vec<Vec<(usize, usize)>> {
    layout
        .pages
        .iter()
        .map(|p| p.lines.iter().map(|l| (l.paragraph, l.index)).collect())
        .collect()
}

/// For each page, how many lines of each paragraph it has.
fn counts(layout: &DocumentLayout) -> Vec<Vec<(usize, usize)>> {
    shape(layout)
        .into_iter()
        .map(|page| {
            let mut out: Vec<(usize, usize)> = Vec::new();
            for (paragraph, _) in page {
                match out.last_mut() {
                    Some((p, n)) if *p == paragraph => *n += 1,
                    _ => out.push((paragraph, 1)),
                }
            }
            out
        })
        .collect()
}

#[test]
fn an_empty_document_has_one_page() {
    let mut system = TextSystem::new();
    let layout = paginate_texts(setup(100.0), &[], &mut system);
    assert_eq!(layout.pages.len(), 1);
    let layout = paginate(&Document::new_word_processing(), &mut system);
    assert_eq!(layout.pages.len(), 1);
    assert_eq!(layout.pages[0].lines.len(), 1);
}

#[test]
fn est_02_est_03_lines_start_inside_the_margins() {
    let mut system = TextSystem::new();
    let layout = paginate_texts(setup(100.0), &[para(2, ParaFormat::default())], &mut system);
    let first = &layout.pages[0].lines[0];
    assert_eq!(first.origin.0, 50.0);
    assert!((first.top() - 20.0).abs() < 0.01);
    assert_eq!(first.line.width, 200.0);
    let second = &layout.pages[0].lines[1];
    assert!((second.top() - first.bottom()).abs() < 0.01);
}

#[test]
fn lines_flow_onto_new_pages_and_stay_inside_the_body() {
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let body = 10.0 * h + 1.0;
    let texts: Vec<_> = (0..5).map(|_| para(7, ParaFormat::default())).collect();
    let layout = paginate_texts(setup(body), &texts, &mut system);
    assert_eq!(layout.pages.len(), 4);
    for page in &layout.pages[..3] {
        assert_eq!(page.lines.len(), 10);
    }
    assert_eq!(layout.pages[3].lines.len(), 5);
    for page in &layout.pages {
        for line in &page.lines {
            assert!(line.bottom() <= 20.0 + body + 0.01);
        }
        assert!((page.lines[0].top() - 20.0).abs() < 0.01);
    }
}

#[test]
fn fmt_15_the_larger_space_between_paragraphs_applies() {
    let mut system = TextSystem::new();
    let spaced = |before, after| ParaFormat {
        space_before: Length::from_points(before),
        space_after: Length::from_points(after),
        ..ParaFormat::default()
    };
    let texts = [
        para(1, spaced(30, 10)),
        para(1, spaced(4, 0)),
        para(1, spaced(12, 0)),
    ];
    let layout = paginate_texts(setup(500.0), &texts, &mut system);
    let lines = &layout.pages[0].lines;
    // No space before at the top of the page.
    assert!((lines[0].top() - 20.0).abs() < 0.01);
    // max(10, 4) and then max(0, 12).
    assert!((lines[1].top() - lines[0].bottom() - 10.0).abs() < 0.01);
    assert!((lines[2].top() - lines[1].bottom() - 12.0).abs() < 0.01);
}

#[test]
fn fmt_15_space_before_is_dropped_at_the_top_of_a_page() {
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let spaced = ParaFormat {
        space_before: Length::from_points(24),
        ..ParaFormat::default()
    };
    let texts = [para(9, ParaFormat::default()), para(2, spaced)];
    let layout = paginate_texts(setup(10.0 * h + 1.0), &texts, &mut system);
    assert_eq!(counts(&layout), [vec![(0, 9)], vec![(1, 2)]]);
    assert!((layout.pages[1].lines[0].top() - 20.0).abs() < 0.01);
}

#[test]
fn est_04_page_break_before() {
    let mut system = TextSystem::new();
    let breaking = ParaFormat {
        page_break_before: true,
        ..ParaFormat::default()
    };
    let texts = [
        para(1, breaking.clone()),
        para(2, ParaFormat::default()),
        para(1, breaking),
    ];
    let layout = paginate_texts(setup(500.0), &texts, &mut system);
    // The first paragraph is already at the top of a page.
    assert_eq!(counts(&layout), [vec![(0, 1), (1, 2)], vec![(2, 1)]]);
}

#[test]
fn est_05_keep_lines_together() {
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let together = ParaFormat {
        keep_lines_together: true,
        ..ParaFormat::default()
    };
    let texts = [para(7, ParaFormat::default()), para(5, together.clone())];
    let layout = paginate_texts(setup(10.0 * h + 1.0), &texts, &mut system);
    assert_eq!(counts(&layout), [vec![(0, 7)], vec![(1, 5)]]);

    // Taller than a page: it breaks anyway.
    let texts = [para(3, ParaFormat::default()), para(14, together)];
    let layout = paginate_texts(setup(10.0 * h + 1.0), &texts, &mut system);
    assert_eq!(counts(&layout), [vec![(0, 3), (1, 7)], vec![(1, 7)]]);
}

#[test]
fn est_05_widow_control() {
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let body = 10.0 * h + 1.0;
    let widows = ParaFormat {
        widow_control: true,
        ..ParaFormat::default()
    };
    let plain = ParaFormat::default();

    // One line would be left alone at the top of the next page.
    let layout = paginate_texts(
        setup(body),
        &[para(5, plain.clone()), para(6, widows.clone())],
        &mut system,
    );
    assert_eq!(counts(&layout), [vec![(0, 5), (1, 4)], vec![(1, 2)]]);
    let layout = paginate_texts(
        setup(body),
        &[para(5, plain.clone()), para(6, plain.clone())],
        &mut system,
    );
    assert_eq!(counts(&layout), [vec![(0, 5), (1, 5)], vec![(1, 1)]]);

    // One line would be left alone at the bottom of the page.
    let layout = paginate_texts(
        setup(body),
        &[para(9, plain.clone()), para(4, widows.clone())],
        &mut system,
    );
    assert_eq!(counts(&layout), [vec![(0, 9)], vec![(1, 4)]]);

    // Three lines with room for two: moving one leaves the first alone, so
    // all three move.
    let layout = paginate_texts(setup(body), &[para(8, plain), para(3, widows)], &mut system);
    assert_eq!(counts(&layout), [vec![(0, 8)], vec![(1, 3)]]);
}

#[test]
fn est_05_keep_with_next() {
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let body = 10.0 * h + 1.0;
    let heading = ParaFormat {
        keep_with_next: true,
        ..ParaFormat::default()
    };
    let plain = ParaFormat::default();
    let together = ParaFormat {
        keep_lines_together: true,
        ..ParaFormat::default()
    };

    // The heading fits, but the paragraph after it does not start there.
    let texts = [
        para(8, plain.clone()),
        para(1, heading.clone()),
        para(3, together),
    ];
    let layout = paginate_texts(setup(body), &texts, &mut system);
    assert_eq!(counts(&layout), [vec![(0, 8)], vec![(1, 1), (2, 3)]]);

    // Two headings in a row move together.
    let texts = [
        para(8, plain.clone()),
        para(1, heading.clone()),
        para(1, heading.clone()),
        para(3, plain.clone()),
    ];
    let layout = paginate_texts(setup(body), &texts, &mut system);
    assert_eq!(
        counts(&layout),
        [vec![(0, 8)], vec![(1, 1), (2, 1), (3, 3)]]
    );

    // When the next paragraph starts on the same page, nothing moves.
    let texts = [
        para(7, plain.clone()),
        para(1, heading.clone()),
        para(3, plain.clone()),
    ];
    let layout = paginate_texts(setup(body), &texts, &mut system);
    assert_eq!(
        counts(&layout),
        [vec![(0, 7), (1, 1), (2, 2)], vec![(2, 1)]]
    );

    // A chain that fills a page from its top breaks normally.
    let texts: Vec<_> = (0..12).map(|_| para(1, heading.clone())).collect();
    let layout = paginate_texts(setup(body), &texts, &mut system);
    assert_eq!(layout.pages.len(), 2);
    assert_eq!(layout.pages[0].lines.len(), 10);
}

#[test]
fn a_line_taller_than_the_page_overflows_it() {
    let mut system = TextSystem::new();
    let texts = [para(3, ParaFormat::default())];
    let layout = paginate_texts(setup(5.0), &texts, &mut system);
    assert_eq!(counts(&layout), [vec![(0, 1)], vec![(0, 1)], vec![(0, 1)]]);
}
