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

#[test]
fn est_13_a_long_footnote_goes_on_at_the_foot_of_the_next_page() {
    use desk_model::{CharAttrs, Editor, FIELD_CHARACTER, Field, Note, ParaAttrs, Paragraph, Run};
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let note_lines = 18;
    editor
        .edit("Setup", |edit| {
            edit.set_page_setup(setup(200.0))?;
            let at = edit.insert_text(start, "Text")?;
            let note = edit.new_id();
            let after = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
            let mark = CharAttrs {
                field: Some(Some(Field::NoteMark { note })),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(at, after, &mark)?;
            let at = edit.split_paragraph(after)?;
            edit.insert_text(at, "More")?;
            let body = edit.document().styles().default_paragraph();
            let text = vec!["note"; note_lines].join(&LINE_BREAK.to_string());
            let id = edit.new_id();
            let paragraph =
                Paragraph::from_runs(id, body, ParaAttrs::default(), [Run::new(&text)])?;
            edit.set_note(
                note,
                Some(Note {
                    id: note,
                    mark: None,
                    paragraphs: vec![paragraph].into(),
                }),
            )
        })
        .unwrap();
    let mut system = TextSystem::new();
    let layout = paginate(editor.document(), &mut system);
    let first = &layout.pages[0];
    let here = first.note_lines.len();
    assert!(here > 0 && here < note_lines, "{here}");
    let bottom = first
        .note_lines
        .iter()
        .map(|l| l.origin.1 + l.line.top + l.line.height)
        .fold(0.0f32, f32::max);
    let tops: Vec<f32> = first
        .note_lines
        .iter()
        .map(|l| l.origin.1 + l.line.top)
        .collect();
    assert!(
        bottom <= first.body.1 + 0.5,
        "{bottom} {:?} {here} {tops:?}",
        first.body
    );
    // The mark and its first lines stay together on the first page.
    assert!(first.lines.iter().any(|l| l.paragraph == 0));
    let second = &layout.pages[1];
    assert_eq!(second.note_lines.first().map(|l| l.index), Some(here));
    assert_eq!(here + second.note_lines.len(), note_lines);
    // Above the body of the page: the rest starts below the rule.
    let rule = second.note_rule.expect("a rule").1;
    assert!(second.note_lines[0].origin.1 + second.note_lines[0].line.top > rule);
}

#[test]
fn est_13_a_note_longer_than_a_page_goes_on_over_several() {
    use desk_model::{CharAttrs, Editor, FIELD_CHARACTER, Field, Note, ParaAttrs, Paragraph, Run};
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let note_lines = 40;
    editor
        .edit("Setup", |edit| {
            edit.set_page_setup(setup(200.0))?;
            let at = edit.insert_text(start, "Text")?;
            let note = edit.new_id();
            let after = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
            let mark = CharAttrs {
                field: Some(Some(Field::NoteMark { note })),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(at, after, &mark)?;
            let at = edit.split_paragraph(after)?;
            edit.insert_text(at, "More")?;
            let body = edit.document().styles().default_paragraph();
            let text = vec!["note"; note_lines].join(&LINE_BREAK.to_string());
            let id = edit.new_id();
            let paragraph =
                Paragraph::from_runs(id, body, ParaAttrs::default(), [Run::new(&text)])?;
            edit.set_note(
                note,
                Some(Note {
                    id: note,
                    mark: None,
                    paragraphs: vec![paragraph].into(),
                }),
            )
        })
        .unwrap();
    let mut system = TextSystem::new();
    let layout = paginate(editor.document(), &mut system);
    // Every line of the note shows once, in order, on three pages or
    // more, and none goes below the body of its page.
    let shown: Vec<usize> = layout
        .pages
        .iter()
        .flat_map(|p| p.note_lines.iter().map(|l| l.index))
        .collect();
    assert_eq!(shown, (0..note_lines).collect::<Vec<_>>());
    let with_notes = layout
        .pages
        .iter()
        .filter(|p| !p.note_lines.is_empty())
        .count();
    assert!(with_notes >= 3, "{with_notes}");
    for page in &layout.pages {
        for line in &page.note_lines {
            assert!(line.origin.1 + line.line.top + line.line.height <= page.body.1 + 0.5);
        }
    }
    // The text after the mark comes after the note.
    assert!(
        layout
            .pages
            .iter()
            .any(|p| p.lines.iter().any(|l| l.paragraph == 1))
    );
}

#[test]
fn est_13_a_footnote_that_fits_a_page_moves_whole_with_its_mark() {
    use desk_model::{CharAttrs, Editor, FIELD_CHARACTER, Field, Note, ParaAttrs, Paragraph, Run};
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let note_lines = 9;
    editor
        .edit("Setup", |edit| {
            edit.set_page_setup(setup(200.0))?;
            let before = ["line"; 7].join(&LINE_BREAK.to_string());
            let at = edit.insert_text(start, &before)?;
            let at = edit.split_paragraph(at)?;
            let at = edit.insert_text(at, "Text")?;
            let note = edit.new_id();
            let after = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
            let mark = CharAttrs {
                field: Some(Some(Field::NoteMark { note })),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(at, after, &mark)?;
            let body = edit.document().styles().default_paragraph();
            let text = vec!["note"; note_lines].join(&LINE_BREAK.to_string());
            let id = edit.new_id();
            let paragraph =
                Paragraph::from_runs(id, body, ParaAttrs::default(), [Run::new(&text)])?;
            edit.set_note(
                note,
                Some(Note {
                    id: note,
                    mark: None,
                    paragraphs: vec![paragraph].into(),
                }),
            )
        })
        .unwrap();
    let mut system = TextSystem::new();
    let layout = paginate(editor.document(), &mut system);
    assert_eq!(layout.pages.len(), 2);
    // As in Pages, the line of the mark goes to the next page, and the
    // note goes whole with it.
    let first = &layout.pages[0];
    assert!(first.note_lines.is_empty());
    assert!(first.lines.iter().all(|l| l.paragraph == 0));
    let second = &layout.pages[1];
    assert!(second.lines.iter().any(|l| l.paragraph == 1));
    assert_eq!(second.note_lines.len(), note_lines);
    assert!(second.carried_note.is_none() && first.split_note.is_none());
}

/// A document of one long paragraph on pages of [`setup`], with an object.
fn with_object(wrap: desk_model::Wrap, anchor: Option<usize>) -> Document {
    use desk_model::{Editor, Frame, Object, ObjectKind, Shape};
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Setup", |edit| {
            let at = edit.insert_text(start, "First paragraph.")?;
            let at = edit.split_paragraph(at)?;
            let second = at.paragraph;
            edit.insert_text(at, &"words of text that wrap ".repeat(40))?;
            edit.set_page_setup(setup(400.0))?;
            let mut object = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(100.0, 60.0, 70.0, 60.0),
            );
            object.wrap = wrap;
            object.wrap_margin = Length::ZERO;
            if anchor.is_some() {
                object.anchor = desk_model::Anchor::Paragraph(second);
                object.frame.y = Length::from_points(10);
            }
            edit.add_object(object)
        })
        .unwrap();
    editor.document().clone()
}

/// The left and right of the text of each line on the first page beside
/// heights from `y0` to `y1`.
fn beside(layout: &DocumentLayout, y0: f32, y1: f32) -> Vec<(f32, f32)> {
    layout.pages[0]
        .lines
        .iter()
        .filter(|l| l.bottom() > y0 && l.top() < y1)
        .map(|l| {
            let x = l.origin.0 + l.line.x;
            (x, x + l.line.advance)
        })
        .collect()
}

#[test]
fn obj_17_text_goes_around_a_floating_object() {
    let mut system = TextSystem::new();
    let document = with_object(desk_model::Wrap::Both, None);
    let layout = paginate(&document, &mut system);
    assert_eq!(layout.pages[0].objects.len(), 1);
    let lines = beside(&layout, 60.0, 120.0);
    assert!(!lines.is_empty());
    // Every piece beside the object keeps out of it, and there are pieces
    // on both of its sides.
    assert!(
        lines.iter().all(|&(a, b)| b <= 100.5 || a >= 169.5),
        "{lines:?}"
    );
    assert!(lines.iter().any(|&(_, b)| b <= 100.5));
    assert!(lines.iter().any(|&(a, _)| a >= 169.5));

    // Only on its right: nothing on the left.
    let layout = paginate(&with_object(desk_model::Wrap::Right, None), &mut system);
    let lines = beside(&layout, 60.0, 120.0);
    assert!(
        !lines.is_empty() && lines.iter().all(|&(a, _)| a >= 169.5),
        "{lines:?}"
    );

    // Above and below: no line beside it.
    let layout = paginate(
        &with_object(desk_model::Wrap::AboveAndBelow, None),
        &mut system,
    );
    assert!(beside(&layout, 60.5, 119.5).is_empty());

    // Through it: the lines as without it.
    let mut document = with_object(desk_model::Wrap::Through, None);
    let through = paginate(&document, &mut system).pages[0].lines.len();
    let id = document.objects().iter().next().unwrap().id;
    document
        .apply(desk_model::Op::RemoveObject { object: id })
        .unwrap();
    assert_eq!(
        through,
        paginate(&document, &mut system).pages[0].lines.len()
    );
}

#[test]
fn obj_01_objects_move_with_their_paragraph() {
    let mut system = TextSystem::new();
    let layout = paginate(&with_object(desk_model::Wrap::Both, Some(1)), &mut system);
    let page = &layout.pages[0];
    let object = &page.objects[0];
    assert_eq!(object.paragraph, Some(1));
    let first = page.lines.iter().find(|l| l.paragraph == 1).unwrap();
    assert!((object.y - (first.top() + 10.0)).abs() < 0.01);
    // Its own paragraph goes around it.
    let lines = beside(&layout, object.y, object.y + object.height);
    assert!(
        lines.iter().all(|&(a, b)| b <= 100.5 || a >= 169.5),
        "{lines:?}"
    );
}

/// A document with a text box of many words, as the text box asks.
fn with_text_box(columns: u8, vertical: desk_model::VerticalAlignment, words: usize) -> Document {
    use desk_model::{Frame, Object, ObjectKind, ParaAttrs, Paragraph, Run, TextBox};
    let mut document = Document::new_word_processing();
    document
        .transact("Box", |edit| {
            let style = edit.document().styles().default_paragraph();
            let text = vec!["word"; words].join(" ");
            let paragraph = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs::default(),
                [Run::new(&text)],
            )?;
            let mut text = TextBox::new(std::sync::Arc::from([paragraph]), Length::from_points(5));
            text.columns = columns;
            text.vertical = vertical;
            let id = edit.new_id();
            let mut object = Object::new(
                id,
                ObjectKind::TextBox(text),
                Frame::from_pt(100.0, 100.0, 300.0, 200.0),
            );
            object.wrap = desk_model::Wrap::Through;
            edit.add_object(object)
        })
        .unwrap();
    document
}

#[test]
fn obj_13_obj_16_fmt_13_fmt_19_text_boxes_flow_through_columns_and_sit_where_asked() {
    use desk_model::VerticalAlignment;
    let mut system = TextSystem::new();
    // Enough text to fill one column and go on in the next.
    let layout = paginate(&with_text_box(2, VerticalAlignment::Top, 150), &mut system);
    let lines = &layout.pages[0].objects[0].lines;
    let lefts: Vec<f32> = lines.iter().map(|l| l.origin.0).collect();
    // Two columns of (290 - 18) / 2 = 136 points, 154 apart.
    assert!(
        lefts.contains(&105.0) && lefts.contains(&259.0),
        "{lefts:?}"
    );
    let second = lines.iter().find(|l| l.origin.0 == 259.0).unwrap();
    assert!((second.top() - 105.0).abs() < 0.01);
    assert!(
        lines
            .iter()
            .all(|l| l.bottom() <= 295.01 || l.origin.0 == 259.0)
    );
    // A short text in the middle and at the bottom of the box.
    let top = paginate(&with_text_box(1, VerticalAlignment::Top, 3), &mut system);
    let line = &top.pages[0].objects[0].lines[0];
    assert!((line.top() - 105.0).abs() < 0.01);
    // Pages centers the text from the top of its first line to the
    // baseline of its last (FMT-19).
    let block = line.baseline() - line.top();
    let middle = paginate(&with_text_box(1, VerticalAlignment::Middle, 3), &mut system);
    let line = &middle.pages[0].objects[0].lines[0];
    assert!((line.top() - (105.0 + (190.0 - block) / 2.0)).abs() < 0.01);
    let bottom = paginate(&with_text_box(1, VerticalAlignment::Bottom, 3), &mut system);
    let line = &bottom.pages[0].objects[0].lines[0];
    assert!((line.bottom() - 295.0).abs() < 0.01);
}

#[test]
fn obj_15_text_goes_on_in_linked_boxes() {
    let mut system = TextSystem::new();
    let mut document = with_text_box(1, desk_model::VerticalAlignment::Top, 150);
    let first = document.objects().iter().next().unwrap().id;
    document
        .transact("Link", |edit| {
            let style = edit.document().styles().default_paragraph();
            let empty = desk_model::Paragraph::new(edit.new_id(), style);
            let id = edit.new_id();
            let mut object = desk_model::Object::new(
                id,
                desk_model::ObjectKind::TextBox(desk_model::TextBox::new(
                    Arc::from([empty]),
                    Length::from_points(5),
                )),
                desk_model::Frame::from_pt(100.0, 400.0, 300.0, 200.0),
            );
            object.wrap = desk_model::Wrap::Through;
            edit.add_object(object)?;
            edit.link_text_box(first, id)
        })
        .unwrap();
    let layout = paginate(&document, &mut system);
    let objects = &layout.pages[0].objects;
    let (one, two) = (&objects[0].lines, &objects[1].lines);
    assert!(!one.is_empty() && !two.is_empty());
    // The first box is full, and the second goes on from its top with the
    // lines that follow.
    assert!(one.iter().all(|l| l.bottom() <= 295.01));
    assert!((two[0].top() - 405.0).abs() < 0.01);
    assert_eq!(two[0].paragraph, 0);
    assert_eq!(two[0].index, one.last().unwrap().index + 1);
}

#[test]
fn obj_12_fmt_13_text_sits_in_the_middle_of_a_shape() {
    use desk_model::{Frame, Object, ParaAttrs, Paragraph, Run, TextBox};
    let mut system = TextSystem::new();
    let mut document = Document::new_word_processing();
    document
        .transact("Oval", |edit| {
            let style = edit.document().styles().default_paragraph();
            let paragraph =
                Paragraph::from_runs(edit.new_id(), style, ParaAttrs::default(), [Run::new("Hi")])?;
            let id = edit.new_id();
            let mut object = Object::new(
                id,
                desk_model::Shape::Ellipse.into(),
                Frame::from_pt(100.0, 100.0, 200.0, 100.0),
            );
            object.text = Some(TextBox::in_shape(Arc::from([paragraph])));
            object.wrap = desk_model::Wrap::Through;
            edit.add_object(object)
        })
        .unwrap();
    let layout = paginate(&document, &mut system);
    let lines = &layout.pages[0].objects[0].lines;
    assert_eq!(lines.len(), 1);
    // In the rectangle inside the oval, and in the middle from top to
    // bottom, from the top of the line to its baseline.
    let side = (1.0 - std::f32::consts::FRAC_1_SQRT_2) / 2.0;
    assert!((lines[0].origin.0 - (104.0 + 200.0 * side)).abs() < 0.01);
    let middle = (lines[0].top() + lines[0].baseline()) / 2.0;
    assert!((middle - 150.0).abs() < 0.01, "{middle}");
}

#[test]
fn obj_01_an_object_in_the_text_stands_on_its_line() {
    use desk_model::{Frame, Object, Position};
    let mut system = TextSystem::new();
    let mut document = Document::new_word_processing();
    let id = document
        .transact("Inline", |edit| {
            let start = edit.document().start();
            let at = edit.insert_text(start, "Before after")?;
            let object = Object::new(
                edit.new_id(),
                desk_model::Shape::Rectangle.into(),
                Frame::from_pt(0.0, 0.0, 60.0, 50.0),
            );
            let id = object.id;
            edit.insert_object_in_text(Position::new(at.paragraph, 7), object)?;
            Ok(id)
        })
        .unwrap()
        .0;
    let layout = paginate(&document, &mut system);
    let page = &layout.pages[0];
    assert_eq!(page.objects.len(), 1);
    let placed = &page.objects[0];
    assert_eq!(placed.object.id, id);
    let line = &page.lines[0];
    // Where the mark is, on the baseline of the line, which is as tall
    // as the object.
    assert!((placed.x - (line.origin.0 + line.line.caret_x(7))).abs() < 0.01);
    assert!((placed.y + placed.height - line.baseline()).abs() < 0.01);
    assert!(line.line.baseline - line.line.top >= 50.0 - 0.01);
    assert_eq!(layout.pages[0].lines.len(), 1);
}

#[test]
fn int_05_master_objects_show_on_every_page_of_their_section_behind_the_text() {
    use desk_model::{
        Anchor, Editor, Frame, Object, ObjectKind, ParaAttrs, Position, Section, Shape,
    };
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let (second, ids) = editor
        .edit("Setup", |edit| {
            edit.set_page_setup(setup(400.0))?;
            let at = edit.insert_text(start, &"words of text that wrap ".repeat(120))?;
            let at = edit.split_paragraph(at)?;
            edit.insert_text(at, "The second section.")?;
            edit.set_paragraph_attrs(
                Position::new(at.paragraph, 0),
                Position::new(at.paragraph, 0),
                &ParaAttrs {
                    start: Some(Some(Arc::new(Start::Section(
                        Section {
                            own_margins: true,
                            ..Section::default()
                        },
                        Layout::default(),
                    )))),
                    ..ParaAttrs::default()
                },
            )?;
            let second = edit.document().body().get(1).unwrap().id();
            let mut ids = Vec::new();
            for section in [None, Some(second)] {
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::Shape(Shape::Rectangle),
                    Frame::from_pt(100.0, 60.0, 200.0, 200.0),
                );
                object.anchor = Anchor::Master(section, desk_model::MarginPages::Default);
                ids.push(object.id);
                edit.add_object(object)?;
            }
            Ok((second, ids))
        })
        .unwrap();
    let mut system = TextSystem::new();
    let layout = paginate(editor.document(), &mut system);
    assert!(layout.pages.len() >= 3, "{}", layout.pages.len());
    let last = layout.pages.len() - 1;
    for (index, page) in layout.pages.iter().enumerate() {
        let shown: Vec<_> = page.objects.iter().map(|o| o.object.id).collect();
        let expected = if index == last { ids[1] } else { ids[0] };
        assert_eq!(shown, [expected], "page {index}");
        assert_eq!(
            layout.section_margins[page.section],
            (index == last).then_some(second)
        );
    }
    // The text does not go around them: it is laid out as without them.
    let mut bare = editor.document().clone();
    let mut edit = Editor::new(bare.clone());
    edit.edit("Remove", |e| {
        for id in &ids {
            e.remove_object(*id)?;
        }
        Ok(())
    })
    .unwrap();
    bare = edit.document().clone();
    let without = paginate(&bare, &mut system);
    let lines = |l: &DocumentLayout| -> Vec<(f32, f32, f32)> {
        l.pages[0]
            .lines
            .iter()
            .map(|l| (l.origin.0 + l.line.x, l.origin.1, l.line.advance))
            .collect()
    };
    assert_eq!(lines(&layout), lines(&without));
}

#[test]
fn est_19_the_first_page_and_the_left_pages_have_masters_of_their_own() {
    use desk_model::{Anchor, Editor, Frame, MarginPages, Object, ObjectKind, Section, Shape};
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let ids = editor
        .edit("Setup", |edit| {
            edit.set_page_setup(PageSetup {
                section: Section {
                    first_page_different: true,
                    left_right_different: true,
                    ..Section::default()
                },
                ..setup(400.0)
            })?;
            edit.insert_text(start, &"words of text that wrap ".repeat(250))?;
            let mut ids = Vec::new();
            for pages in [MarginPages::Default, MarginPages::First, MarginPages::Left] {
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::Shape(Shape::Rectangle),
                    Frame::from_pt(100.0, 60.0, 50.0, 50.0),
                );
                object.anchor = Anchor::Master(None, pages);
                ids.push(object.id);
                edit.add_object(object)?;
            }
            Ok(ids)
        })
        .unwrap();
    let layout = paginate(editor.document(), &mut TextSystem::new());
    assert!(layout.pages.len() >= 4, "{}", layout.pages.len());
    for (index, page) in layout.pages.iter().enumerate() {
        let shown: Vec<_> = page.objects.iter().map(|o| o.object.id).collect();
        // The first page, then left and right pages in turn.
        let expected = match index {
            0 => ids[1],
            i if i % 2 == 1 => ids[2],
            _ => ids[0],
        };
        assert_eq!(shown, [expected], "page {index}");
    }
}

#[test]
fn int_03_a_page_layout_document_has_its_pages_and_no_body_text() {
    use desk_model::{Anchor, Editor, Frame, Object, ObjectKind, Shape};
    let mut editor = Editor::new(Document::new_page_layout());
    let ids = editor
        .edit("Pages", |edit| {
            let mut setup = *edit.document().page_setup();
            setup.layout_pages = Some(3);
            edit.set_page_setup(setup)?;
            let mut ids = Vec::new();
            // The last is on a page past the last, and goes on the last.
            for page in [0, 2, 7] {
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::Shape(Shape::Rectangle),
                    Frame::from_pt(100.0, 100.0, 80.0, 60.0),
                );
                object.anchor = Anchor::Page(page);
                ids.push(object.id);
                edit.add_object(object)?;
            }
            Ok(ids)
        })
        .unwrap();
    let mut system = TextSystem::new();
    let layout = paginate(editor.document(), &mut system);
    assert_eq!(layout.pages.len(), 3);
    let on: Vec<Vec<_>> = layout
        .pages
        .iter()
        .map(|p| p.objects.iter().map(|o| o.object.id).collect())
        .collect();
    assert_eq!(on, [vec![ids[0]], vec![], vec![ids[1], ids[2]]]);
    assert!(layout.pages.iter().all(|p| p.lines.is_empty()));
    let numbers: Vec<u32> = layout.pages.iter().map(|p| p.number).collect();
    assert_eq!(numbers, [1, 2, 3]);
}

#[test]
fn est_12_an_object_in_the_line_of_a_header_shows_on_every_page() {
    use desk_model::{
        Anchor, CharAttrs, Editor, FIELD_CHARACTER, Field, Frame, MarginKind, MarginPages,
        MarginPlace, Object, ObjectKind, ParaAttrs, Paragraph, Run, Shape,
    };
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let logo = editor
        .edit("Header", |edit| {
            edit.set_page_setup(setup(400.0))?;
            edit.insert_text(start, &"words of text that wrap ".repeat(250))?;
            let logo = edit.new_id();
            let style = edit.document().styles().default_paragraph();
            let mark = FIELD_CHARACTER.to_string();
            let header = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs::default(),
                vec![
                    Run::new("Logo "),
                    Run::with_attrs(
                        &mark,
                        None,
                        CharAttrs {
                            field: Some(Some(Field::ObjectMark { object: logo })),
                            ..CharAttrs::default()
                        },
                    ),
                ],
            )?;
            edit.set_header_footer(
                MarginPlace::new(MarginKind::Header, MarginPages::Default),
                vec![header],
            )?;
            let mut object = Object::new(
                logo,
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(0.0, 0.0, 30.0, 12.0),
            );
            object.anchor = Anchor::InText;
            edit.add_object(object)?;
            Ok(logo)
        })
        .unwrap();
    let layout = paginate(editor.document(), &mut TextSystem::new());
    assert!(layout.pages.len() >= 3, "{}", layout.pages.len());
    for (index, page) in layout.pages.iter().enumerate() {
        let placed: Vec<_> = page
            .objects
            .iter()
            .filter(|o| o.object.id == logo)
            .collect();
        assert_eq!(placed.len(), 1, "page {index}");
        let placed = placed[0];
        // After the word, in the line of the header, above the body.
        let line = &page.margin_lines[0];
        assert!(
            placed.x > line.origin.0 + 20.0,
            "page {index}: {}",
            placed.x
        );
        assert!(placed.y >= line.top() - 0.01, "page {index}: {}", placed.y);
        assert!(placed.y + 12.0 <= line.bottom() + 0.01, "page {index}");
        assert!(line.bottom() <= page.lines[0].top() + 0.01, "page {index}");
    }
}

#[test]
fn est_07_est_13_a_note_in_the_second_column_takes_room_from_the_first() {
    use desk_model::{CharAttrs, Editor, FIELD_CHARACTER, Field, Note, ParaAttrs, Paragraph, Run};
    let mut system = TextSystem::new();
    let h = line_height(&mut system);
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Setup", |edit| {
            let mut page = setup(h * 10.0 + 1.0);
            page.layout = columns(2);
            edit.set_page_setup(page)?;
            // Ten lines fill the first column; the mark is in the second.
            let text = ["line"; 10].join(&LINE_BREAK.to_string());
            let at = edit.insert_text(start, &text)?;
            let at = edit.split_paragraph(at)?;
            let at = edit.insert_text(at, "Text")?;
            let note = edit.new_id();
            let after = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
            let mark = CharAttrs {
                field: Some(Some(Field::NoteMark { note })),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(at, after, &mark)?;
            let body = edit.document().styles().default_paragraph();
            let text = ["note"; 2].join(&LINE_BREAK.to_string());
            let id = edit.new_id();
            let paragraph =
                Paragraph::from_runs(id, body, ParaAttrs::default(), [Run::new(&text)])?;
            edit.set_note(
                note,
                Some(Note {
                    id: note,
                    mark: None,
                    paragraphs: vec![paragraph].into(),
                }),
            )
        })
        .unwrap();
    let layout = paginate(editor.document(), &mut system);
    let first = &layout.pages[0];
    assert_eq!(first.note_lines.len(), 2);
    let rule = first.note_rule.expect("a rule").1;
    // The note runs across both columns, so the first column ends above
    // it, and its last lines go to the second column.
    let left: Vec<_> = first.lines.iter().filter(|l| l.origin.0 < 100.0).collect();
    assert!(left.len() < 10, "{}", left.len());
    for line in &left {
        assert!(line.origin.1 + line.line.top + line.line.height <= rule + 0.5);
    }
    assert!(first.lines.iter().any(|l| l.paragraph == 1));
}
