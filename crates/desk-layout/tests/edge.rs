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
