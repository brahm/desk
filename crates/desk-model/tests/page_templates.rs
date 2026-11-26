// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! W095: page templates of page layout documents.

use std::sync::Arc;

use desk_model::{
    Anchor, Document, Editor, Frame, Length, Object, ObjectKind, Op, Paragraph, Placeholder,
    PlaceholderKind, Run, Shape, TextBox,
};

fn text_box(document: &mut Document, page: u32, text: &str) -> Object {
    let style = document.styles().default_paragraph();
    let paragraph = Paragraph::from_runs(
        document.new_id(),
        style,
        Default::default(),
        [Run::new(text)],
    )
    .unwrap();
    let mut object = Object::new(
        document.new_id(),
        ObjectKind::TextBox(TextBox::new(Arc::from([paragraph]), Length::ZERO)),
        Frame::from_pt(72.0, 72.0, 200.0, 50.0),
    );
    object.anchor = Anchor::Page(page);
    object
}

fn texts(document: &Document, page: u32) -> Vec<String> {
    document
        .objects()
        .iter()
        .filter(|o| o.anchor == Anchor::Page(page))
        .map(|o| o.paragraphs().map(|p| p.text()).collect::<String>())
        .collect()
}

#[test]
fn w095_pages_come_from_templates_whose_placeholders_keep_their_content() {
    let mut document = Document::new_page_layout();
    // Page 0: a title placeholder with a tag, and a line.
    let mut title = text_box(&mut document, 0, "Title");
    title.placeholder = Some(Placeholder {
        kind: PlaceholderKind::Text,
        tag: Some("title".into()),
    });
    let mut line = Object::new(
        document.new_id(),
        ObjectKind::Shape(Shape::Rectangle),
        Frame::from_pt(72.0, 140.0, 200.0, 2.0),
    );
    line.anchor = Anchor::Page(0);
    for (index, object) in [title, line].into_iter().enumerate() {
        document.apply(Op::InsertObject { index, object }).unwrap();
    }
    let mut editor = Editor::new(document);

    // A template from the page, and a page from it.
    editor
        .edit("Template", |edit| {
            edit.add_page_template_from_page(0, "Chapter")
        })
        .unwrap();
    let page = editor
        .edit("Add Page", |edit| edit.add_page_from_template(0, "Chapter"))
        .unwrap();
    assert_eq!(page, 1);
    let document = editor.document();
    assert_eq!(document.page_setup().layout_pages, Some(2));
    assert_eq!(
        document.page_templates().of_page(1).map(|n| &**n),
        Some("Chapter")
    );
    assert_eq!(texts(document, 1), ["Title", ""]);
    let copied = document
        .objects()
        .iter()
        .filter(|o| o.anchor == Anchor::Page(1))
        .all(|o| o.template_object.is_some());
    assert!(copied);

    // The placeholder's text is typed over, and the line moved away;
    // reapplying puts the line back and keeps the text.
    let (placeholder, line) = {
        let on: Vec<&Object> = editor
            .document()
            .objects()
            .iter()
            .filter(|o| o.anchor == Anchor::Page(1))
            .collect();
        (on[0].clone(), on[1].clone())
    };
    let mut typed = placeholder.clone();
    let style = editor.document().styles().default_paragraph();
    typed.text_box_mut().unwrap().paragraphs = Arc::from([Paragraph::from_runs(
        placeholder.paragraphs().next().unwrap().id(),
        style,
        Default::default(),
        [Run::new("Chapter one")],
    )
    .unwrap()]);
    let mut moved = line.clone();
    moved.frame.y = Length::from_points(500);
    editor
        .edit("Type", |edit| {
            edit.replace_object(typed)?;
            edit.replace_object(moved)
        })
        .unwrap();
    editor
        .edit("Reapply", |edit| edit.apply_page_template(1, "Chapter"))
        .unwrap();
    let document = editor.document();
    assert_eq!(texts(document, 1), ["Chapter one", ""]);
    let back = document
        .objects()
        .iter()
        .find(|o| o.anchor == Anchor::Page(1) && o.placeholder.is_none())
        .unwrap();
    assert_eq!(back.frame.y, Length::from_points(140));

    // Another template with a placeholder of the same tag takes the text.
    let mut other = text_box(&mut editor.document().clone(), 0, "Heading");
    other.placeholder = Some(Placeholder {
        kind: PlaceholderKind::Text,
        tag: Some("title".into()),
    });
    editor
        .edit("Other", |edit| {
            edit.put_page_template(desk_model::PageTemplate {
                name: "Simple".into(),
                objects: vec![other],
            })?;
            edit.apply_page_template(1, "Simple")
        })
        .unwrap();
    assert_eq!(texts(editor.document(), 1), ["Chapter one"]);

    // Pages keep their templates when pages move, are copied and go.
    editor
        .edit("Move", |edit| edit.move_layout_pages(&[1], 0).map(|_| ()))
        .unwrap();
    let of = |d: &Document, p| d.page_templates().of_page(p).map(|n| n.to_string());
    assert_eq!(of(editor.document(), 0).as_deref(), Some("Simple"));
    assert_eq!(of(editor.document(), 1), None);
    editor
        .edit("Duplicate", |edit| {
            edit.duplicate_layout_pages(&[0]).map(|_| ())
        })
        .unwrap();
    assert_eq!(of(editor.document(), 1).as_deref(), Some("Simple"));
    editor
        .edit("Delete", |edit| edit.delete_layout_page(0))
        .unwrap();
    assert_eq!(of(editor.document(), 0).as_deref(), Some("Simple"));
    assert_eq!(of(editor.document(), 1), None);
}

#[test]
fn w095_templates_keep_their_connectors_and_leave_what_the_user_copied() {
    use desk_model::Connection;
    let mut document = Document::new_page_layout();
    let a = text_box(&mut document, 0, "A");
    let b = text_box(&mut document, 0, "B");
    let mut line = Object::new(
        document.new_id(),
        ObjectKind::Shape(Shape::Connection(Connection {
            start: Some(a.id),
            end: Some(b.id),
            from: (0, 0),
            to: (10_000, 10_000),
            bend: Length::ZERO,
        })),
        Frame::from_pt(72.0, 72.0, 100.0, 100.0),
    );
    line.anchor = Anchor::Page(0);
    for (index, object) in [a, b, line].into_iter().enumerate() {
        document.apply(Op::InsertObject { index, object }).unwrap();
    }
    let mut editor = Editor::new(document);
    editor
        .edit("Template", |edit| {
            edit.add_page_template_from_page(0, "Joined")
        })
        .unwrap();
    // The connector of the template joins the boxes of the template,
    // whose paragraphs are not those of the page.
    let template = editor
        .document()
        .page_templates()
        .get("Joined")
        .unwrap()
        .clone();
    let ids: Vec<_> = template.objects.iter().map(|o| o.id).collect();
    let ObjectKind::Shape(Shape::Connection(joined)) = &template.objects[2].kind else {
        panic!("a connector");
    };
    assert_eq!((joined.start, joined.end), (Some(ids[0]), Some(ids[1])));
    let page_paragraph = editor
        .document()
        .objects()
        .iter()
        .next()
        .unwrap()
        .paragraphs()
        .next()
        .unwrap()
        .id();
    assert_ne!(
        template.objects[0].paragraphs().next().unwrap().id(),
        page_paragraph
    );

    // A page from it has its connector joined too.
    editor
        .edit("Add Page", |edit| edit.add_page_from_template(0, "Joined"))
        .unwrap();
    let on: Vec<Object> = editor
        .document()
        .objects()
        .iter()
        .filter(|o| o.anchor == Anchor::Page(1))
        .cloned()
        .collect();
    let ObjectKind::Shape(Shape::Connection(copied)) = &on[2].kind else {
        panic!("a connector");
    };
    assert_eq!((copied.start, copied.end), (Some(on[0].id), Some(on[1].id)));

    // A duplicate made by the user stays when the template is applied
    // again.
    editor
        .edit("Duplicate", |edit| {
            edit.duplicate_objects(&[on[0].id], Length::from_points(10), Length::ZERO)
                .map(|_| ())
        })
        .unwrap();
    editor
        .edit("Reapply", |edit| edit.apply_page_template(1, "Joined"))
        .unwrap();
    let count = |d: &Document| {
        d.objects()
            .iter()
            .filter(|o| o.anchor == Anchor::Page(1))
            .count()
    };
    assert_eq!(count(editor.document()), 4);

    // Pages past the end and templates that do not exist change nothing.
    let before = editor.document().clone();
    editor
        .edit("Nothing", |edit| {
            edit.apply_page_template(10, "Joined")?;
            edit.add_page_from_template(0, "Missing").map(|_| ())
        })
        .unwrap();
    assert_eq!(editor.document().objects().len(), before.objects().len());
    assert_eq!(editor.document().page_setup().layout_pages, Some(2));

    // Taking the template away leaves its copies as plain objects, which
    // another template does not take away.
    editor
        .edit("Remove", |edit| {
            edit.remove_page_template("Joined")?;
            edit.put_page_template(desk_model::PageTemplate {
                name: "Empty".into(),
                objects: Vec::new(),
            })?;
            edit.apply_page_template(1, "Empty")
        })
        .unwrap();
    assert_eq!(count(editor.document()), 4);
    assert!(
        editor
            .document()
            .objects()
            .iter()
            .all(|o| o.template_object.is_none())
    );
}
