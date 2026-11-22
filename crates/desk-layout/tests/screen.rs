// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Screen View (PXV-01): the text in one flow as wide as the window.

use desk_layout::{SCREEN_MARGIN, paginate, paginate_screen};
use desk_model::{
    Anchor, Document, Editor, Frame, Length, MarginPages, Object, ObjectKind, ParaAttrs, Paragraph,
    Run, Shape, TextBox, Wrap,
};
use desk_text::TextSystem;
use std::sync::Arc;

#[test]
fn pxv_01_screen_view_flows_the_text_without_pages() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let (fixed, moving, wide, master, boxed) = editor
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
            // A text box with a few words, tall and narrow, anchored to
            // the last paragraph.
            let last = edit.document().body().get(1).unwrap().id();
            let style = edit.document().styles().default_paragraph();
            let paragraph = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs::default(),
                [Run::new("A few words in a box")],
            )?;
            let text = TextBox::new(std::sync::Arc::from([paragraph]), Length::from_pt(5.0));
            let mut object = Object::new(
                edit.new_id(),
                ObjectKind::TextBox(text),
                Frame::from_pt(100.0, 100.0, 120.0, 300.0),
            );
            object.anchor = Anchor::Paragraph(last);
            let boxed = object.id;
            edit.add_object(object)?;
            Ok((fixed, moving, wide, master, boxed))
        })
        .unwrap();
    let document = editor.document();
    let mut system = TextSystem::new();
    let pages = paginate(document, &mut system);
    assert!(pages.pages.len() >= 3, "{}", pages.pages.len());

    let screen = paginate_screen(document, 500.0, 1.0, &mut system);
    // One page as wide as asked, as tall as its text.
    assert_eq!(screen.pages.len(), 1);
    assert_eq!(screen.setup.width, Length::from_pt(500.0));
    let page = &screen.pages[0];
    let last = page.lines.last().unwrap().bottom();
    let height = screen.setup.height.to_pt() as f32;
    assert!(height > last && height < last + 200.0, "{height} {last}");
    // The text box of the last paragraph comes after its text, across
    // the column and as high as its text (the Mac, 2026-11-21).
    let boxed = page.objects.iter().find(|o| o.object.id == boxed).unwrap();
    assert!(boxed.y >= last, "{} {last}", boxed.y);
    assert!((boxed.width - (500.0 - 2.0 * SCREEN_MARGIN as f32)).abs() < 0.5);
    assert!(boxed.height < 40.0, "{}", boxed.height);
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
    // An empty rectangle keeps its size, in the middle of the column.
    let moving = page.objects.iter().find(|o| o.object.id == moving).unwrap();
    assert!((moving.width - 100.0).abs() < 0.5);
    assert!((moving.x + 50.0 - 250.0).abs() < 0.5, "{}", moving.x);
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
    let screen = paginate_screen(editor.document(), 600.0, 1.0, &mut system);
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

#[test]
fn pxv_01_screen_view_keeps_its_line_and_sizes_the_objects_in_the_text() {
    // Pages 15.4 (step 19f of W1): the column of Screen View is 664 points
    // at most, in the middle of a wider window; a table in the text is
    // that wide whatever its own width, its columns in proportion; a
    // shape in the text keeps its size on the screen whatever the Text
    // Size.
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let (table, shape) = editor
        .edit("Objects", |edit| {
            let end = edit.insert_text(start, &"words of text that wrap ".repeat(60))?;
            let mut table = desk_model::Table::new(2, 2);
            table.set_width(0, 100_000);
            table.set_width(1, 300_000);
            let table = Object::new(
                edit.new_id(),
                ObjectKind::Table(Arc::new(table)),
                Frame::from_pt(0.0, 0.0, 400.0, 40.0),
            );
            let table_id = table.id;
            let end = edit.insert_object_in_text(end, table)?;
            let shape = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(0.0, 0.0, 120.0, 60.0),
            );
            let shape_id = shape.id;
            edit.insert_object_in_text(end, shape)?;
            Ok((table_id, shape_id))
        })
        .unwrap();
    let mut system = TextSystem::new();
    let screen = paginate_screen(editor.document(), 1200.0, 1.2, &mut system);
    let page = &screen.pages[0];
    // The lines are 664 points long at most, in the middle.
    let side = (1200.0 - 664.0) / 2.0;
    for line in &page.lines {
        assert!(
            (line.origin.0 - side as f32).abs() < 0.01,
            "{}",
            line.origin.0
        );
        assert!(line.line.width <= 664.5, "{}", line.line.width);
    }
    let placed = |id| page.objects.iter().find(|o| o.object.id == id).unwrap();
    let table = placed(table);
    assert!((table.width - 664.0).abs() < 0.5, "{}", table.width);
    let ObjectKind::Table(cells) = &table.object.kind else {
        unreachable!()
    };
    assert_eq!((cells.width(0), cells.width(1)), (166_000, 498_000));
    let shape = placed(shape);
    assert!((shape.width - 100.0).abs() < 0.5, "{}", shape.width);
    assert!((shape.height - 50.0).abs() < 0.5, "{}", shape.height);
}

#[test]
fn pxv_01_screen_view_shows_text_and_objects_at_the_sizes_of_pages() {
    // Pages 15.4 (step 19g of W1): one curve gives the size of text on
    // the screen at 100%, which the Text Size multiplies; the left indent
    // goes, and a first line indent is 36 points on the screen; a picture
    // or a shape after its paragraph keeps its size on the screen.
    use desk_model::screen_font_size;
    for (print, shown) in [
        (4.0, 10.0),
        (9.0, 11.0),
        (11.0, 13.0),
        (16.0, 15.5),
        (24.0, 19.5),
        (36.0, 22.5),
        (48.0, 25.5),
        (96.0, 26.0),
    ] {
        assert!((screen_font_size(print) - shown).abs() < 1e-9, "{print}");
    }
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let shape = editor
        .edit("Text", |edit| {
            let end = edit.insert_text(start, &"words of text that wrap ".repeat(40))?;
            edit.set_character_attrs(
                start,
                end,
                &desk_model::CharAttrs {
                    font_size: Some(Length::from_pt(16.0)),
                    ..desk_model::CharAttrs::default()
                },
            )?;
            edit.set_paragraph_attrs(
                start,
                end,
                &ParaAttrs {
                    left_indent: Some(Length::from_pt(85.0)),
                    first_line_indent: Some(Length::from_pt(142.0)),
                    ..ParaAttrs::default()
                },
            )?;
            let first = edit.document().body().get(0).unwrap().id();
            let mut shape = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Ellipse),
                Frame::from_pt(0.0, 0.0, 100.0, 100.0),
            );
            shape.anchor = Anchor::Paragraph(first);
            let id = shape.id;
            edit.add_object(shape)?;
            Ok(id)
        })
        .unwrap();
    let mut system = TextSystem::new();
    let zoom = 1.25;
    let screen = paginate_screen(editor.document(), 900.0, zoom, &mut system);
    let page = &screen.pages[0];
    let side = SCREEN_MARGIN.max((900.0 - 664.0) / 2.0) as f32;
    let first = &page.lines[0];
    assert!(
        (first.line.runs[0].size - 15.5).abs() < 0.01,
        "{}",
        first.line.runs[0].size
    );
    // The first line starts 36 points on the screen in, the others at the
    // edge of the column.
    let indent = first.origin.0 + first.line.runs[0].x - side;
    assert!((indent - 36.0 / zoom as f32).abs() < 0.5, "{indent}");
    let second = &page.lines[1];
    assert!((second.origin.0 + second.line.runs[0].x - side).abs() < 0.5);
    let shape = page.objects.iter().find(|o| o.object.id == shape).unwrap();
    assert!(
        (shape.width - 100.0 / zoom as f32).abs() < 0.5,
        "{}",
        shape.width
    );
}

#[test]
fn pxv_01_screen_view_spaces_lines_paragraphs_and_tabs_as_pages_15_does() {
    // Pages 15.4 (step 19h of W1): a line spacing shows as one of four
    // levels of single, a fixed one compared with the line on the screen;
    // space before and after shows in up to four steps of a quarter line;
    // tabs go to stops every 36 points on the screen, whatever the stops
    // of the paragraph.
    use desk_model::{LineSpacing, TabStop, TabStops, screen_font_size};
    let mut document = Document::new_word_processing();
    let style = document.styles().default_paragraph();
    let size = document
        .styles()
        .resolve_character(style, None, &desk_model::CharAttrs::default())
        .font_size
        .to_pt();
    let zoom = 1.6;
    document.set_screen_styles(Some(zoom));
    let screen_single = 1.2 * screen_font_size(size);
    let resolve = |attrs: ParaAttrs| document.styles().resolve_paragraph(style, &attrs);
    let spacing = |spacing| match resolve(ParaAttrs {
        line_spacing: Some(spacing),
        ..ParaAttrs::default()
    })
    .line_spacing
    {
        LineSpacing::Multiple { percent } => percent,
        other => panic!("{other:?}"),
    };
    assert_eq!(spacing(LineSpacing::Multiple { percent: 80 }), 100);
    assert_eq!(spacing(LineSpacing::Multiple { percent: 125 }), 114);
    assert_eq!(spacing(LineSpacing::Multiple { percent: 150 }), 135);
    assert_eq!(spacing(LineSpacing::DOUBLE), 179);
    assert_eq!(spacing(LineSpacing::Multiple { percent: 250 }), 179);
    // A fixed spacing of twice the line on the screen at this Text Size
    // is double, and one shorter than single is single.
    let twice = Length::from_pt(2.0 * screen_single * zoom);
    assert_eq!(spacing(LineSpacing::Exactly(twice)), 179);
    assert_eq!(spacing(LineSpacing::AtLeast(Length::from_pt(1.0))), 100);
    let format = resolve(ParaAttrs {
        space_before: Some(Length::from_pt(0.25 * 1.2 * size)),
        space_after: Some(Length::from_pt(100.0 * size)),
        left_indent: Some(Length::from_pt(50.0)),
        tab_stops: Some(TabStops::new([TabStop::left(Length::from_pt(100.0))])),
        default_tab_interval: Some(Length::from_pt(72.0)),
        ..ParaAttrs::default()
    });
    let step = 0.89 / 4.0 * screen_single;
    assert!((format.space_before.to_pt() - step).abs() < 0.01);
    assert!((format.space_after.to_pt() - 4.0 * step).abs() < 0.01);
    assert_eq!(format.left_indent, Length::ZERO);
    assert_eq!(format.tab_stops, TabStops::none());
    assert!((format.default_tab_interval.to_pt() - 36.0 / zoom).abs() < 0.01);
}
