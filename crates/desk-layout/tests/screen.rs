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
    // Pages 15.4 (steps 19h and 19i of W1), in Helvetica at 12 points and
    // a Text Size of 160%: the text shows at 13.5 points, 21.6 on the
    // screen, where the ascent, descent and leading of Helvetica (0.77002,
    // 0.22998 and 0.2 of the size) round to 17, 5 and 4 points; the line
    // without its leading is 26 - 4.32 = 21.68. A single line is 1.1 of
    // it and the leading, double 2.0 of it. In print the same line is
    // 14 - 2.4 = 11.6, so a space of 12 points holds 4 quarters of it and
    // one of 6 points 2, each a quarter of 21.68 on the screen.
    use desk_model::{CharAttrs, LineSpacing, TabStop, TabStops};
    let zoom = 1.6_f32;
    let text = |spacing: LineSpacing, after: f64| {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Text", |edit| {
                let mid = edit.insert_text(start, &"words of text that wrap ".repeat(30))?;
                let mid = edit.split_paragraph(mid)?;
                let end = edit.insert_text(mid, &"more words of text ".repeat(30))?;
                edit.set_character_attrs(
                    start,
                    end,
                    &CharAttrs {
                        font_family: Some("Helvetica".into()),
                        font_size: Some(Length::from_pt(12.0)),
                        ..CharAttrs::default()
                    },
                )?;
                edit.set_paragraph_attrs(
                    start,
                    end,
                    &ParaAttrs {
                        line_spacing: Some(spacing),
                        space_after: Some(Length::from_pt(after)),
                        ..ParaAttrs::default()
                    },
                )?;
                // The font and size of the paragraph style space the
                // paragraphs.
                let style = edit.document().styles().default_paragraph();
                edit.redefine_style(style, start, end)
            })
            .unwrap();
        let mut system = TextSystem::new();
        let screen = paginate_screen(editor.document(), 1000.0, f64::from(zoom), &mut system);
        let lines = &screen.pages[0].lines;
        let top = |l: &desk_layout::PlacedLine| l.origin.1 + l.line.top;
        let pitch = top(&lines[1]) - top(&lines[0]);
        let last = lines.iter().rfind(|l| l.paragraph == 0).unwrap();
        let next = lines.iter().find(|l| l.paragraph == 1).unwrap();
        let gap = top(next) - (top(last) + last.line.height);
        // What a level adds goes under the text.
        let over = lines[1].line.baseline - lines[1].line.top;
        assert!((over - 16.712 / zoom).abs() < 0.01, "{over}");
        (pitch, gap)
    };
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    let (single, gap) = text(LineSpacing::SINGLE, 12.0);
    assert!(close(single, (1.1 * 21.68 + 4.32) / zoom), "{single}");
    assert!(close(gap, 21.68 / zoom), "{gap}");
    let (double, gap) = text(LineSpacing::DOUBLE, 6.0);
    assert!(close(double, (2.0 * 21.68 + 4.32) / zoom), "{double}");
    assert!(close(gap, 2.0 * 21.68 / 4.0 / zoom), "{gap}");
    let (lines, _) = text(LineSpacing::Multiple { percent: 80 }, 0.0);
    assert!(close(lines, single), "{lines}");
    // The levels change half way between them, ties to the lower one.
    let (lines, _) = text(LineSpacing::Multiple { percent: 117 }, 0.0);
    assert!(close(lines, single), "{lines}");
    let (lines, _) = text(LineSpacing::Multiple { percent: 118 }, 0.0);
    assert!(close(lines, (1.25 * 21.68 + 4.32) / zoom), "{lines}");
    // A distance counts against the unrounded line of the font on the
    // screen, 1.2 times 21.6 points.
    let natural = 1.2 * 21.6;
    let exactly = |lines: f32| LineSpacing::Exactly(Length::from_pt(f64::from(natural * lines)));
    assert!(close(text(exactly(1.17), 0.0).0, single));
    assert!(close(
        text(exactly(1.2), 0.0).0,
        (1.25 * 21.68 + 4.32) / zoom
    ));
    // Exactly twice the single line on the screen is double; at least one
    // point is single.
    let twice = Length::from_pt(f64::from(2.0 * single * zoom));
    assert!(close(text(LineSpacing::Exactly(twice), 0.0).0, double));
    assert!(close(
        text(LineSpacing::AtLeast(Length::from_pt(1.0)), 0.0).0,
        single
    ));

    // Indents and tabs are the same at every size: no left indent, a
    // first line to the right at 36 points on the screen, a hanging one
    // at the edge, and tabs every 36 points whatever the stops.
    let mut document = Document::new_word_processing();
    let style = document.styles().default_paragraph();
    document.set_screen_styles(Some(f64::from(zoom)));
    let resolve = |attrs: ParaAttrs| document.styles().resolve_paragraph(style, &attrs);
    let format = resolve(ParaAttrs {
        left_indent: Some(Length::from_pt(50.0)),
        first_line_indent: Some(Length::from_pt(0.05)),
        tab_stops: Some(TabStops::new([TabStop::left(Length::from_pt(100.0))])),
        default_tab_interval: Some(Length::from_pt(72.0)),
        ..ParaAttrs::default()
    });
    assert_eq!(format.left_indent, Length::ZERO);
    assert!((format.first_line_indent.to_pt() - 36.0 / f64::from(zoom)).abs() < 0.01);
    assert_eq!(format.tab_stops, TabStops::none());
    assert!((format.default_tab_interval.to_pt() - 36.0 / f64::from(zoom)).abs() < 0.01);
    let hanging = resolve(ParaAttrs {
        left_indent: Some(Length::from_pt(36.0)),
        first_line_indent: Some(Length::from_pt(-18.0)),
        ..ParaAttrs::default()
    });
    assert_eq!(hanging.left_indent, Length::ZERO);
    assert_eq!(hanging.first_line_indent, Length::ZERO);
}

#[test]
fn pxv_01_screen_view_puts_list_labels_18_points_in_at_each_level() {
    // Pages 15.4 (step 19i of W1): the label of a list item shows 18
    // points on the screen further in at each level, whatever the indents
    // of the list.
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Typing", |edit| edit.insert_text(start, "- one\nthree"))
        .unwrap();
    let first = editor.document().body().get(0).unwrap().id();
    editor
        .edit("List", |edit| edit.list_from_marker(first).map(|_| ()))
        .unwrap();
    let second = editor.document().body().get(1).unwrap().id();
    editor
        .edit("Level", |edit| {
            let b = desk_model::Position {
                paragraph: second,
                offset: 0,
            };
            let list = edit
                .document()
                .body()
                .get(0)
                .unwrap()
                .attrs()
                .list_style
                .flatten();
            edit.set_list_style(b, b, list)?;
            edit.set_paragraph_attrs(
                b,
                b,
                &ParaAttrs {
                    list_level: Some(2),
                    ..ParaAttrs::default()
                },
            )?;
            Ok(())
        })
        .unwrap();
    let mut system = TextSystem::new();
    let zoom = 1.35_f32;
    let screen = paginate_screen(editor.document(), 1000.0, f64::from(zoom), &mut system);
    let side = SCREEN_MARGIN.max((1000.0 - 664.0) / 2.0) as f32;
    let label = |paragraph: usize| {
        let line = screen.pages[0]
            .lines
            .iter()
            .find(|l| l.paragraph == paragraph)
            .unwrap();
        line.origin.0 + line.line.runs[0].x - side
    };
    assert!(label(0).abs() < 0.01, "{}", label(0));
    // The text 18/17 of its size on the screen after the label.
    let line = &screen.pages[0].lines[0].line;
    let gap = line.runs[1].x - line.runs[0].x;
    assert!(
        (gap - 18.0 / 17.0 * line.runs[1].size).abs() < 0.01,
        "{gap}"
    );
    assert!((label(1) - 36.0 / zoom).abs() < 0.01, "{}", label(1));
}

#[test]
fn pxv_01_list_text_goes_to_a_tab_stop_and_screen_view_moves_its_label() {
    // Pages 15.4 (step 19k of W1): in print the text of an item goes to
    // the next tab stop when the room after its label is no wider than
    // the digit 2 of the label's font at the size of the text, 0.556 em
    // in Helvetica; Screen View then moves the label and the first line
    // 36 points on the screen.
    let lay_out = |size: f64, followed_by: desk_model::LabelFollow| {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
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
                let level = desk_model::ListLevel {
                    label_indent: Length::from_pt(18.0),
                    text_indent: Length::from_pt(18.0),
                    followed_by,
                    ..list.level(0).clone()
                };
                edit.set_list_level(style, 0, level)?;
                let start = edit.document().start();
                let end = desk_model::Position {
                    paragraph: first,
                    offset: 3,
                };
                edit.set_character_attrs(
                    start,
                    end,
                    &desk_model::CharAttrs {
                        font_family: Some("Helvetica".into()),
                        font_size: Some(Length::from_pt(size)),
                        ..desk_model::CharAttrs::default()
                    },
                )
            })
            .unwrap();
        let mut system = TextSystem::new();
        let print = paginate(editor.document(), &mut system);
        let line = print.pages[0].lines[0].line.clone();
        let printed = line.runs[1].x - line.runs[0].x;
        let zoom = 1.25_f32;
        let screen = paginate_screen(editor.document(), 1000.0, f64::from(zoom), &mut system);
        let side = SCREEN_MARGIN.max((1000.0 - 664.0) / 2.0) as f32;
        let placed = &screen.pages[0].lines[0];
        let label = placed.origin.0 + placed.line.runs[0].x - side;
        (printed, label, zoom)
    };
    // At 12 points the digit leaves room: the text is at the indent.
    use desk_model::LabelFollow;
    let (printed, label, _) = lay_out(12.0, LabelFollow::Tab);
    assert!((printed - 18.0).abs() < 0.01, "{printed}");
    assert!(label.abs() < 0.01, "{label}");
    // At 36 points it is 20 points wide: the text goes to the default
    // stop at 72, and Screen View moves the label in.
    let (printed, label, zoom) = lay_out(36.0, LabelFollow::Tab);
    assert!((printed - 54.0).abs() < 0.01, "{printed}");
    assert!((label - 36.0 / zoom).abs() < 0.01, "{label}");
    // After a space or nothing the text starts where the digit ends.
    for follow in [LabelFollow::Space, LabelFollow::Nothing] {
        let (printed, label, _) = lay_out(12.0, follow);
        assert!((printed - 0.556 * 12.0).abs() < 0.01, "{printed}");
        assert!(label.abs() < 0.01, "{label}");
    }
}
