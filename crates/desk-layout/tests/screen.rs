// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Screen View (PXV-01): the text in one flow as wide as the window.

use desk_layout::{SCREEN_MARGIN, paginate, paginate_screen};
use desk_model::{
    Anchor, Document, Editor, Frame, Length, MarginPages, Object, ObjectKind, ParaAttrs, Shape,
};
use desk_text::TextSystem;

#[test]
fn pxv_01_screen_view_flows_the_text_without_pages() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let (fixed, moving, wide, master) = editor
        .edit("Text", |edit| {
            let end = edit.insert_text(start, &"words of text that wrap ".repeat(300))?;
            let second = edit.split_paragraph(end)?;
            let end = edit.insert_text(second, &"more words after a break ".repeat(100))?;
            // The second paragraph starts a new page.
            edit.set_paragraph_attrs(
                second,
                end,
                &ParaAttrs {
                    page_break_before: Some(true),
                    ..ParaAttrs::default()
                },
            )?;
            let first = edit.document().body().get(0).unwrap().id();
            let mut add = |anchor, width| {
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::Shape(Shape::Rectangle),
                    Frame::from_pt(300.0, 400.0, width, 80.0),
                );
                object.anchor = anchor;
                let id = object.id;
                edit.add_object(object).map(|()| id)
            };
            let fixed = add(Anchor::Page(0), 100.0)?;
            let moving = add(Anchor::Paragraph(first), 100.0)?;
            let wide = add(Anchor::Paragraph(first), 2000.0)?;
            let master = add(Anchor::Master(None, MarginPages::Default), 50.0)?;
            Ok((fixed, moving, wide, master))
        })
        .unwrap();
    let document = editor.document();
    let mut system = TextSystem::new();
    let pages = paginate(document, &mut system);
    assert!(pages.pages.len() >= 3, "{}", pages.pages.len());

    let screen = paginate_screen(document, 500.0, &mut system);
    // One page as wide as asked, as tall as its text.
    assert_eq!(screen.pages.len(), 1);
    assert_eq!(screen.setup.width, Length::from_pt(500.0));
    let page = &screen.pages[0];
    let last = page.lines.last().unwrap().bottom();
    let height = screen.setup.height.to_pt() as f32;
    assert!(height > last && height < last + 200.0, "{height} {last}");
    // Both paragraphs, without the page break between them, and no
    // headers or footers.
    let second = page.lines.iter().position(|l| l.paragraph == 1).unwrap();
    let objects: f32 = page.objects.iter().map(|o| o.height).sum();
    let gap = page.lines[second].top() - page.lines[second - 1].bottom();
    assert!(gap < objects + 40.0, "{gap}");
    assert!(page.margin_lines.is_empty());
    for line in &page.lines {
        assert!((line.origin.0 - SCREEN_MARGIN as f32).abs() < 0.01);
    }
    // The objects that move with the first paragraph show in the flow
    // after it, across the column at most (the Mac, 2026-11-18); the one
    // fixed on a page and the one of the masters do not show (the Mac,
    // 2026-11-21).
    let shown: Vec<_> = page.objects.iter().map(|o| o.object.id).collect();
    assert!(shown.contains(&moving) && shown.contains(&wide));
    assert!(!shown.contains(&fixed) && !shown.contains(&master));
    let wide = page.objects.iter().find(|o| o.object.id == wide).unwrap();
    assert!((wide.width - (500.0 - 2.0 * SCREEN_MARGIN as f32)).abs() < 0.5);
    assert!((wide.x - SCREEN_MARGIN as f32).abs() < 0.5);
    // The text goes around them: no line runs under an object.
    for object in &page.objects {
        for line in &page.lines {
            assert!(
                line.bottom() <= object.y + 0.5 || line.top() >= object.y + object.height - 0.5,
                "a line at {} under an object at {}",
                line.top(),
                object.y
            );
        }
    }
}
