// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Screen View (PXV-01): the text in one flow as wide as the window.

use desk_layout::{SCREEN_MARGIN, paginate, paginate_screen};
use std::sync::Arc;

use desk_model::{
    Anchor, Document, Editor, Frame, Length, MarginPages, Object, ObjectKind, ParaAttrs, Paragraph,
    Run, Shape, TextBox, Wrap,
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

/// An object of the test below, 100 × 60 points somewhere on the page.
fn object(id: desk_model::NodeId, kind: ObjectKind, text: Option<TextBox>) -> Object {
    let mut object = Object::new(id, kind, Frame::from_pt(380.0, 120.0, 100.0, 60.0));
    object.text = text;
    object
}

#[test]
fn pxv_01_screen_view_places_the_objects_as_pages_15_does() {
    // What Pages 15.4 does (step 19e of W1): an object that moves with the
    // text shows whatever its wrap, after the paragraph of its anchor; a
    // text box or a rectangle with text takes the width of the column and
    // the height of its text; the rest keep their size, centred; an
    // object fixed on its page is hidden.
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let ids = editor
        .edit("Objects", |edit| {
            let end = edit.insert_text(start, "P1 first paragraph.")?;
            let p2 = edit.split_paragraph(end)?;
            let end = edit.insert_text(p2, "P2 second paragraph.")?;
            let p3 = edit.split_paragraph(end)?;
            edit.insert_text(p3, "P3 third paragraph.")?;
            let body: Vec<_> = edit.document().body().iter().map(|p| p.id()).collect();
            let style = edit.document().styles().default_paragraph();
            let words = |edit: &mut desk_model::Edit, text: &str| {
                Paragraph::from_runs(edit.new_id(), style, ParaAttrs::default(), [Run::new(text)])
                    .map(|p| Arc::<[Paragraph]>::from([p]))
            };
            let mut ids = Vec::new();
            // A text box after the first paragraph with each wrap but
            // Inline: all show.
            for wrap in [
                Wrap::Both,
                Wrap::Largest,
                Wrap::AboveAndBelow,
                Wrap::Through,
                Wrap::Behind,
            ] {
                let text = TextBox::new(words(edit, "TB")?, Length::from_points(4));
                let mut o = object(edit.new_id(), ObjectKind::TextBox(text), None);
                o.wrap = wrap;
                o.anchor = Anchor::Paragraph(body[0]);
                ids.push(o.id);
                edit.add_object(o)?;
            }
            // After the second: a rectangle with text, an empty
            // rectangle and an oval with text.
            let text = TextBox::in_shape(words(edit, "RECT")?);
            let mut o = object(edit.new_id(), Shape::Rectangle.into(), Some(text));
            o.anchor = Anchor::Paragraph(body[1]);
            ids.push(o.id);
            edit.add_object(o)?;
            let mut o = object(edit.new_id(), Shape::Rectangle.into(), None);
            o.anchor = Anchor::Paragraph(body[1]);
            ids.push(o.id);
            edit.add_object(o)?;
            let text = TextBox::in_shape(words(edit, "OVAL")?);
            let mut o = object(edit.new_id(), Shape::Ellipse.into(), Some(text));
            o.anchor = Anchor::Paragraph(body[1]);
            ids.push(o.id);
            edit.add_object(o)?;
            // A text box fixed on the page.
            let text = TextBox::new(words(edit, "SOP")?, Length::from_points(4));
            let o = object(edit.new_id(), ObjectKind::TextBox(text), None);
            ids.push(o.id);
            edit.add_object(o)?;
            Ok(ids)
        })
        .unwrap();
    let mut system = TextSystem::new();
    let screen = paginate_screen(editor.document(), 600.0, &mut system);
    let page = &screen.pages[0];
    let column = 600.0 - 2.0 * SCREEN_MARGIN as f32;
    let placed = |id| page.objects.iter().find(|o| o.object.id == id);
    let top_of = |paragraph| {
        page.lines
            .iter()
            .find(|l| l.paragraph == paragraph)
            .unwrap()
            .top()
    };
    let bottom_of = |paragraph| {
        page.lines
            .iter()
            .filter(|l| l.paragraph == paragraph)
            .map(|l| l.bottom())
            .fold(0.0, f32::max)
    };
    // The text boxes go after P1, across the column, as tall as a line
    // and the inset, one under the other in their order.
    let mut y = bottom_of(0) - 0.5;
    for &id in &ids[..5] {
        let o = placed(id).expect("a text box that moves with the text shows");
        assert!((o.width - column).abs() < 0.5, "{}", o.width);
        assert!((o.x - SCREEN_MARGIN as f32).abs() < 0.5);
        assert!(o.height > 8.0 && o.height < 30.0, "{}", o.height);
        assert!(o.y >= y && o.y + o.height <= top_of(1), "{} {y}", o.y);
        y = o.y + o.height;
    }
    // After P2: the rectangle with text across the column, the empty
    // rectangle and the oval at their size, centred.
    let rect = placed(ids[5]).unwrap();
    assert!((rect.width - column).abs() < 0.5 && rect.height < 30.0);
    for &id in &ids[6..8] {
        let o = placed(id).unwrap();
        assert!((o.width - 100.0).abs() < 0.5 && (o.height - 60.0).abs() < 0.5);
        assert!((o.x + o.width / 2.0 - 300.0).abs() < 0.5, "{}", o.x);
    }
    for &id in &ids[5..8] {
        let o = placed(id).unwrap();
        assert!(o.y >= bottom_of(1) - 0.5 && o.y + o.height <= top_of(2) + 0.5);
    }
    // The text box fixed on its page is hidden.
    assert!(placed(ids[8]).is_none());
}
