// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pagination, named by the IDs of the parity matrix. The paragraphs are
//! made of short lines ended by manual line breaks, so each one has a
//! known number of lines, in Liberation Sans as in the desk-text tests.

use std::sync::Arc;

use desk_layout::{DocumentLayout, paginate, paginate_texts, paginate_texts_spaced};
use desk_model::{
    CharFormat, Columns, Document, LINE_BREAK, Layout, Length, PageSetup, ParaFormat, Section,
    SectionStart, Start,
};
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
        ..PageSetup::default()
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

    // Documents of other programs add the two (FMT-15).
    let layout = paginate_texts_spaced(setup(500.0), &texts, &mut system, true);
    let lines = &layout.pages[0].lines;
    assert!((lines[0].top() - 20.0).abs() < 0.01);
    assert!((lines[1].top() - lines[0].bottom() - 14.0).abs() < 0.01);
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

#[test]
fn doc_20_lines_count_in_the_document_and_the_selection() {
    let mut system = TextSystem::new();
    let height = line_height(&mut system);
    // Two lines on each page, so the paragraphs break across pages.
    let texts = [
        para(3, ParaFormat::default()),
        para(1, ParaFormat::default()),
        para(2, ParaFormat::default()),
    ];
    let layout = paginate_texts(setup(height * 2.0 + 1.0), &texts, &mut system);
    assert!(layout.pages.len() > 1);
    assert_eq!(layout.line_count(), 6);
    // The whole first paragraph, but not the start of the second.
    assert_eq!(layout.lines_between((0, 0), (1, 0)), 3);
    // The second paragraph and the first letter of the third.
    assert_eq!(layout.lines_between((1, 0), (2, 1)), 2);
    // A selection inside one line.
    assert_eq!(layout.lines_between((0, 1), (0, 2)), 1);
}

fn starting(start: Start) -> ParaFormat {
    ParaFormat {
        start: Some(Arc::new(start)),
        ..ParaFormat::default()
    }
}

fn columns(count: u8) -> Layout {
    Layout {
        columns: Columns::equal(count, Columns::DEFAULT_GAP),
        ..Layout::default()
    }
}

#[test]
fn est_07_text_fills_the_columns_one_after_the_other() {
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let mut page = setup(h * 3.0 + 1.0);
    page.layout = columns(2);
    let layout = paginate_texts(page, &[para(8, ParaFormat::default())], &mut system);
    assert_eq!(counts(&layout), [vec![(0, 6)], vec![(0, 2)]]);
    let lines = &layout.pages[0].lines;
    // The body is 200pt wide: two columns of 95pt with a gap of 10pt.
    assert_eq!(lines[0].origin.0, 50.0);
    assert_eq!(lines[3].origin.0, 155.0);
    assert_eq!(lines[3].top(), lines[0].top());
    assert_eq!(lines[3].line.width, lines[0].line.width);
}

#[test]
fn est_08_a_column_break_starts_the_next_column() {
    let mut system = TextSystem::new();
    let mut page = setup(500.0);
    page.layout = columns(2);
    let broken = ParaFormat {
        column_break_before: true,
        ..ParaFormat::default()
    };
    let texts = [
        para(1, ParaFormat::default()),
        para(1, broken.clone()),
        para(1, broken),
    ];
    let layout = paginate_texts(page, &texts, &mut system);
    assert_eq!(counts(&layout), [vec![(0, 1), (1, 1)], vec![(2, 1)]]);
    let lines = &layout.pages[0].lines;
    assert_eq!(lines[1].origin.0, 155.0);
    assert_eq!(lines[1].top(), lines[0].top());
}

#[test]
fn est_09_est_10_a_layout_starts_below_the_one_before() {
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let mut page = setup(500.0);
    page.layout = Layout {
        after: Length::from_points(10),
        ..columns(2)
    };
    let next = Layout {
        before: Length::from_points(6),
        left: Length::from_points(20),
        right: Length::from_points(30),
        ..Layout::default()
    };
    let texts = [
        para(3, ParaFormat::default()),
        para(1, starting(Start::Layout(next))),
    ];
    let layout = paginate_texts(page, &texts, &mut system);
    assert_eq!(layout.pages.len(), 1);
    let lines = &layout.pages[0].lines;
    // The three lines fill the first column; the next layout starts
    // below it, after the space after and before.
    let below = lines[2].bottom();
    assert!((lines[3].top() - (below + 16.0)).abs() < 0.01, "{h}");
    assert_eq!(lines[3].origin.0, 70.0);
    let new_page = Layout {
        new_page: true,
        ..next
    };
    let texts = [
        para(1, ParaFormat::default()),
        para(1, starting(Start::Layout(new_page))),
    ];
    let layout = paginate_texts(setup(500.0), &texts, &mut system);
    assert_eq!(counts(&layout), [vec![(0, 1)], vec![(1, 1)]]);
    assert!((layout.pages[1].lines[0].top() - 26.0).abs() < 0.01);
}

#[test]
fn est_11_facing_pages_mirror_the_margins() {
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let mut page = setup(h + 1.0);
    page.facing_pages = true;
    let layout = paginate_texts(page, &[para(3, ParaFormat::default())], &mut system);
    let x: Vec<f32> = layout.pages.iter().map(|p| p.lines[0].origin.0).collect();
    assert_eq!(x, [50.0, 40.0, 50.0]);
    let left: Vec<bool> = layout.pages.iter().map(|p| p.left).collect();
    assert_eq!(left, [false, true, false]);
    assert_eq!(layout.pages[1].margins, (40.0, 50.0));
}

#[test]
fn est_16_est_18_sections_start_pages_and_number_them() {
    let mut system = TextSystem::new();
    let restart = Section {
        first_number: Some(10),
        ..Section::default()
    };
    // The third page is a right page: a section that starts on a left one
    // leaves it blank.
    let left = Section {
        start: SectionStart::LeftPage,
        ..Section::default()
    };
    let texts = [
        para(1, ParaFormat::default()),
        para(1, starting(Start::Section(restart, Layout::default()))),
        para(1, ParaFormat::default()),
        para(1, starting(Start::Section(left, columns(2)))),
    ];
    let layout = paginate_texts(setup(500.0), &texts, &mut system);
    assert_eq!(
        counts(&layout),
        [vec![(0, 1)], vec![(1, 1), (2, 1)], vec![], vec![(3, 1)]]
    );
    let numbers: Vec<u32> = layout.pages.iter().map(|p| p.number).collect();
    assert_eq!(numbers, [1, 10, 11, 12]);
    let sections: Vec<usize> = layout.pages.iter().map(|p| p.section).collect();
    assert_eq!(sections, [0, 1, 1, 2]);
    let firsts: Vec<bool> = layout.pages.iter().map(|p| p.first_of_section).collect();
    assert_eq!(firsts, [true, true, false, true]);
}

#[test]
fn est_05_keep_with_next_moves_to_the_next_column() {
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let mut page = setup(h * 3.0 + 1.0);
    page.layout = columns(2);
    let kept = ParaFormat {
        keep_with_next: true,
        ..ParaFormat::default()
    };
    let texts = [
        para(2, ParaFormat::default()),
        para(1, kept),
        para(2, ParaFormat::default()),
    ];
    let layout = paginate_texts(page, &texts, &mut system);
    assert_eq!(counts(&layout), [vec![(0, 2), (1, 1), (2, 2)]]);
    let lines = &layout.pages[0].lines;
    assert_eq!(lines[2].origin.0, 155.0);
}

/// A document with these paragraphs, the first header of the document
/// and the headers of the sections that start at the given paragraphs.
fn with_headers(
    texts: &[&str],
    header: &str,
    sections: &[(usize, bool, Option<&str>)],
) -> Document {
    use desk_model::{
        Editor, MarginKind, MarginPages, MarginPlace, ParaAttrs, Paragraph, Position, Run,
    };
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Setup", |edit| {
            let mut ids = vec![start.paragraph];
            let mut at = edit.insert_text(start, texts[0])?;
            for text in &texts[1..] {
                at = edit.split_paragraph(at)?;
                ids.push(at.paragraph);
                at = edit.insert_text(at, text)?;
            }
            let mut page = setup(200.0);
            page.headers = true;
            page.header_margin = Length::from_points(10);
            edit.set_page_setup(page)?;
            let body = edit.document().styles().default_paragraph();
            let place = MarginPlace::new(MarginKind::Header, MarginPages::Default);
            let paragraph = |edit: &mut desk_model::Edit, text: &str| {
                let id = edit.new_id();
                Paragraph::from_runs(id, body, ParaAttrs::default(), [Run::new(text)])
            };
            let first = paragraph(edit, header)?;
            edit.set_header_footer(place, vec![first])?;
            for &(index, own, text) in sections {
                let id = ids[index];
                edit.set_paragraph_attrs(
                    Position::new(id, 0),
                    Position::new(id, 0),
                    &ParaAttrs {
                        start: Some(Some(Arc::new(Start::Section(
                            Section {
                                own_margins: own,
                                ..Section::default()
                            },
                            Layout::default(),
                        )))),
                        ..ParaAttrs::default()
                    },
                )?;
                if let Some(text) = text {
                    let own = paragraph(edit, text)?;
                    edit.set_section_header_footer(Some(id), place, vec![own])?;
                }
            }
            Ok(())
        })
        .unwrap();
    editor.into_document()
}

/// The words of the header of each page.
fn headers(layout: &DocumentLayout) -> Vec<usize> {
    layout.pages.iter().map(|p| p.margin_lines.len()).collect()
}

#[test]
fn est_12_a_tall_header_pushes_the_body_down() {
    let mut system = TextSystem::new();
    let tall = ["line"; 4].join(&LINE_BREAK.to_string());
    let document = with_headers(&["Text"], &tall, &[]);
    let layout = paginate(&document, &mut system);
    let page = &layout.pages[0];
    let header_bottom = page
        .margin_lines
        .iter()
        .map(|l| l.origin.1 + l.line.top + l.line.height)
        .fold(0.0f32, f32::max);
    assert!(header_bottom > 20.0, "the header reaches past the margin");
    assert!((page.body.0 - header_bottom).abs() < 0.5, "{:?}", page.body);
    assert!(page.lines[0].top() >= header_bottom - 0.01);
    // A shorter header moves the body less, and never above the margin.
    let document = with_headers(&["Text"], "Short", &[]);
    let layout = paginate(&document, &mut system);
    let short = &layout.pages[0];
    let short_bottom = short
        .margin_lines
        .iter()
        .map(|l| l.origin.1 + l.line.top + l.line.height)
        .fold(0.0f32, f32::max);
    assert!(short_bottom < header_bottom);
    assert!(
        (short.body.0 - short_bottom.max(20.0)).abs() < 0.5,
        "{:?}",
        short.body
    );
    assert!(short.body.0 < page.body.0);
}

#[test]
fn est_12_est_16_sections_show_their_own_headers_or_the_ones_before() {
    let mut system = TextSystem::new();
    let two = ["line"; 2].join(&LINE_BREAK.to_string());
    let three = ["line"; 3].join(&LINE_BREAK.to_string());
    let document = with_headers(
        &["One", "Two", "Three", "Four"],
        "Header",
        &[
            (1, true, Some(&two)),
            (2, false, None),
            (3, true, Some(&three)),
        ],
    );
    let layout = paginate(&document, &mut system);
    assert_eq!(layout.pages.len(), 4);
    // The third section uses the headers of the second.
    assert_eq!(headers(&layout), [1, 2, 2, 3]);
}
