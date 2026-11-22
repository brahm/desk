// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The limits near the right edge of a column that Pages 15.4 keeps in
//! print (step 19m of W1).

use desk_layout::{paginate, paginate_screen};
use desk_model::{
    CharAttrs, Document, Editor, Length, ListLevel, ParaAttrs, Position, TabAlignment, TabStop,
    TabStops,
};
use desk_text::TextSystem;

fn column(document: &Document) -> f32 {
    let page = document.page_setup();
    (page.width - page.margin_left - page.margin_right).to_pt() as f32
}

#[test]
fn fmt_16_a_tab_stop_stops_short_of_the_edge() {
    // A left stop 25 points before the edge at most, a right one 1.
    for (alignment, room) in [(TabAlignment::Left, 25.0), (TabAlignment::Right, 1.0)] {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        let width = column(editor.document());
        editor
            .edit("Tab", |edit| {
                let end = edit.insert_text(start, "a\tb")?;
                edit.set_paragraph_attrs(
                    start,
                    end,
                    &ParaAttrs {
                        tab_stops: Some(TabStops::new([TabStop {
                            alignment,
                            ..TabStop::left(Length::from_pt(f64::from(width) + 2.0))
                        }])),
                        ..ParaAttrs::default()
                    },
                )
            })
            .unwrap();
        let mut system = TextSystem::new();
        let print = paginate(editor.document(), &mut system);
        let line = &print.pages[0].lines[0].line;
        // The run holds "ab": where "b" starts, and where it ends.
        let run = line.runs.last().unwrap();
        let b = run.glyphs.last().unwrap().x;
        let end = run.x + run.advance;
        match alignment {
            TabAlignment::Left => assert!((b - (width - room)).abs() < 0.01, "{b}"),
            _ => assert!((end - (width - room)).abs() < 0.01, "{end}"),
        }
    }
}

#[test]
fn lst_an_item_whose_stop_is_near_the_edge_has_its_label_alone() {
    // A label 400 points in, its text 60 after it: the first line would
    // start 10 points or less before the edge, so it holds the label
    // alone and the text starts on the next, 18 points before the edge at
    // most.
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let width = column(editor.document());
    editor
        .edit("Typing", |edit| edit.insert_text(start, "- one"))
        .unwrap();
    let first = editor.document().body().get(0).unwrap().id();
    editor
        .edit("List", |edit| edit.list_from_marker(first).map(|_| ()))
        .unwrap();
    editor
        .edit("Indents", |edit| {
            let style = edit
                .document()
                .body()
                .get(0)
                .unwrap()
                .attrs()
                .list_style
                .flatten()
                .unwrap();
            let list = edit.document().styles().list(style).unwrap();
            let level = ListLevel {
                label_indent: Length::from_pt(f64::from(width) - 68.0),
                text_indent: Length::from_pt(60.0),
                ..list.level(0).clone()
            };
            edit.set_list_level(style, 0, level)?;
            let end = Position {
                paragraph: first,
                offset: 3,
            };
            edit.set_character_attrs(
                edit.document().start(),
                end,
                &CharAttrs {
                    font_family: Some("Helvetica".into()),
                    ..CharAttrs::default()
                },
            )
        })
        .unwrap();
    let mut system = TextSystem::new();
    let print = paginate(editor.document(), &mut system);
    let line = &print.pages[0].lines[0].line;
    let (label, text) = (&line.runs[0], &line.runs[1]);
    assert!((label.x - (width - 68.0)).abs() < 0.01, "{}", label.x);
    assert!((text.x - (width - 18.0)).abs() < 0.01, "{}", text.x);
    assert!(text.baseline > label.baseline + 1.0);
    // Screen View has no such limit: the label stays on the line of its
    // text, the point lower that the bullet is drawn.
    let screen = paginate_screen(editor.document(), 1000.0, 1.0, &mut system);
    let line = &screen.pages[0].lines[0].line;
    let (label, text) = (&line.runs[0], &line.runs[1]);
    assert!((text.baseline - line.baseline).abs() < 0.01);
    assert!((label.baseline - (line.baseline + 1.0)).abs() < 0.01);
}

/// A document of one list item with the text `text`, its label `label`
/// points in and its text `indent` after it, and the tab stops `stops`.
fn list_item(text: &str, label: f32, indent: f32, stops: &[f32]) -> Editor {
    list_item_with("- ", text, label, indent, stops)
}

/// The same, with the marker typed before the text.
fn list_item_with(marker: &str, text: &str, label: f32, indent: f32, stops: &[f32]) -> Editor {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Typing", |edit| {
            edit.insert_text(start, &format!("{marker}{text}"))
        })
        .unwrap();
    let first = editor.document().body().get(0).unwrap().id();
    editor
        .edit("List", |edit| edit.list_from_marker(first).map(|_| ()))
        .unwrap();
    editor
        .edit("Indents", |edit| {
            let style = edit
                .document()
                .body()
                .get(0)
                .unwrap()
                .attrs()
                .list_style
                .flatten()
                .unwrap();
            let list = edit.document().styles().list(style).unwrap();
            let level = ListLevel {
                label_indent: Length::from_pt(f64::from(label)),
                text_indent: Length::from_pt(f64::from(indent)),
                ..list.level(0).clone()
            };
            edit.set_list_level(style, 0, level)?;
            let start = edit.document().start();
            let end = Position {
                paragraph: first,
                offset: text.len(),
            };
            edit.set_character_attrs(
                start,
                end,
                &CharAttrs {
                    font_family: Some("Helvetica".into()),
                    ..CharAttrs::default()
                },
            )?;
            edit.set_paragraph_attrs(
                start,
                end,
                &ParaAttrs {
                    tab_stops: Some(TabStops::new(
                        stops
                            .iter()
                            .map(|&p| TabStop::left(Length::from_pt(f64::from(p)))),
                    )),
                    ..ParaAttrs::default()
                },
            )
        })
        .unwrap();
    editor
}

#[test]
fn lst_a_first_line_exactly_10_points_before_the_edge_keeps_its_text() {
    // The text indent ends 10 points before the edge, a tie, worked out
    // in single precision as Pages does: 60 / 12 is exact and keeps the
    // text on the line of the label, while 58 / 12 rounds up, past the
    // limit, and leaves the label alone (step 19n of W1).
    let width = column(&Document::new_word_processing());
    for (indent, alone) in [(60.0, false), (58.0, true)] {
        let editor = list_item("one", width - 10.0 - indent, indent, &[]);
        let mut system = TextSystem::new();
        let print = paginate(editor.document(), &mut system);
        let line = &print.pages[0].lines[0].line;
        let (label, text) = (&line.runs[0], &line.runs[1]);
        let x = if alone { width - 18.0 } else { width - 10.0 };
        assert!((text.x - x).abs() < 0.01, "{indent} {}", text.x);
        assert_eq!(text.baseline > label.baseline + 1.0, alone, "{indent}");
    }
}

#[test]
fn lst_a_later_tab_goes_to_the_hidden_stop_of_the_item() {
    // The label's tab takes the custom stop at 20, before the indent at
    // 100; the next tab goes to 100 - 20 = 80, before the default stop at
    // 72 that would come first (step 19n of W1).
    let editor = list_item("one\ttwo", 0.0, 100.0, &[20.0]);
    let mut system = TextSystem::new();
    let print = paginate(editor.document(), &mut system);
    let line = &print.pages[0].lines[0].line;
    let text = line.runs.last().unwrap();
    assert!((text.x - 20.0).abs() < 0.01, "{}", text.x);
    let two = text.glyphs[text.glyphs.len() - 3].x;
    assert!((two - 80.0).abs() < 0.01, "{two}");
}

#[test]
fn pxv_01_screen_view_tabs_go_to_the_next_36_points() {
    // Neither the custom stop, nor the indent, nor the hidden stop of the
    // item: the next 36 points from the edge (step 19n of W1).
    let editor = list_item("o\ttwo", 0.0, 100.0, &[20.0]);
    let mut system = TextSystem::new();
    let screen = paginate_screen(editor.document(), 1000.0, 1.0, &mut system);
    let line = &screen.pages[0].lines[0].line;
    let text = line.runs.last().unwrap();
    let two = text.glyphs[text.glyphs.len() - 3].x;
    assert!((two - 36.0).abs() < 0.01, "{two}");
}

#[test]
fn lst_the_end_of_a_wide_label_decides_too() {
    // The text 7 points after a label "1." 10.008 points wide starts at
    // the end of the label, which keeps its text while it ends 10 points
    // or more before the edge (step 19o of W1).
    let width = column(&Document::new_word_processing());
    for (room, alone) in [(20.018, false), (19.998, true)] {
        let editor = list_item_with("1. ", "one", width - room, 7.0, &[]);
        let mut system = TextSystem::new();
        let print = paginate(editor.document(), &mut system);
        let line = &print.pages[0].lines[0].line;
        let (label, text) = (&line.runs[0], &line.runs[1]);
        assert!((label.advance - 10.008).abs() < 0.001, "{}", label.advance);
        assert_eq!(text.baseline > label.baseline + 1.0, alone, "{room}");
    }
}

#[test]
fn lst_a_margin_that_rounds_up_as_a_float_breaks_the_tie() {
    // The same tie 10 points before the edge, on a page whose left margin
    // is 72.1 points, which a float rounds down, or 72.3, which it rounds
    // up: only the second takes the text off the line (step 19o of W1).
    for (margin, alone) in [(72.1, false), (72.3, true)] {
        let mut editor = list_item("one", 0.0, 60.0, &[]);
        let mut page = *editor.document().page_setup();
        page.margin_left = Length::from_pt(margin);
        let width = (page.width - page.margin_left - page.margin_right).to_pt() as f32;
        editor
            .edit("Margins", |edit| {
                edit.set_page_setup(page)?;
                let style = edit
                    .document()
                    .body()
                    .get(0)
                    .unwrap()
                    .attrs()
                    .list_style
                    .flatten()
                    .unwrap();
                let list = edit.document().styles().list(style).unwrap();
                let level = ListLevel {
                    label_indent: Length::from_pt(f64::from(width) - 70.0),
                    ..list.level(0).clone()
                };
                edit.set_list_level(style, 0, level)
            })
            .unwrap();
        let mut system = TextSystem::new();
        let print = paginate(editor.document(), &mut system);
        let line = &print.pages[0].lines[0].line;
        let (label, text) = (&line.runs[0], &line.runs[1]);
        assert_eq!(text.baseline > label.baseline + 1.0, alone, "{margin}");
    }
}
