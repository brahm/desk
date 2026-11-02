// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Model tests for lists (LST-01 to LST-05). Each test name starts with
//! the row ID.

use desk_model::{
    Document, Editor, ListLabel, ListStyle, NumberFormat, ParagraphStyle, Position, Style,
    StyleFamily, StyleId,
};

/// A document with five paragraphs, "a" to "e".
fn editor() -> Editor {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Setup", |edit| edit.insert_text(start, "a\nb\nc\nd\ne"))
        .unwrap();
    editor
}

fn at(editor: &Editor, index: usize) -> Position {
    Position::new(editor.document().body().get(index).unwrap().id(), 0)
}

fn add(editor: &mut Editor, make: fn(StyleId, &str) -> ListStyle, name: &str) -> StyleId {
    editor
        .edit("List", |edit| edit.list_style_or_add(name, make))
        .unwrap()
}

/// The labels of the paragraphs, empty for those outside a list.
fn labels(editor: &Editor) -> Vec<String> {
    let document = editor.document();
    document
        .styles()
        .list_items(document.body().iter())
        .into_iter()
        .map(|item| item.map(|i| i.label).unwrap_or_default())
        .collect()
}

fn levels(editor: &Editor) -> Vec<u8> {
    let document = editor.document();
    document
        .body()
        .iter()
        .map(|p| {
            document
                .styles()
                .resolve_paragraph(p.style(), p.attrs())
                .list_level
        })
        .collect()
}

#[test]
fn lst_01_a_marker_makes_a_list() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Typing", |edit| edit.insert_text(start, "3. third\n- dash"))
        .unwrap();
    let first = at(&editor, 0).paragraph;
    let second = at(&editor, 1).paragraph;
    let removed = editor
        .edit("List", |edit| edit.list_from_marker(first))
        .unwrap();
    assert_eq!(removed, Some(3));
    editor
        .edit("List", |edit| edit.list_from_marker(second))
        .unwrap();
    let document = editor.document();
    assert_eq!(document.body().get(0).unwrap().text(), "third");
    assert_eq!(labels(&editor), ["3.", "-"]);
    let styles = document.styles();
    assert!(styles.by_name(StyleFamily::List, "Numbered List").is_some());
    assert!(styles.by_name(StyleFamily::List, "Dash").is_some());
    // A paragraph already in a list keeps its text.
    let again = editor
        .edit("List", |edit| edit.list_from_marker(first))
        .unwrap();
    assert_eq!(again, None);
}

#[test]
fn lst_02_tab_moves_items_with_their_children() {
    let mut editor = editor();
    let bullet = add(&mut editor, ListStyle::bullet, "Bullet");
    let (a, e) = (at(&editor, 0), at(&editor, 4));
    editor
        .edit("List", |edit| edit.set_list_style(a, e, Some(bullet)))
        .unwrap();
    // b and c one level in, c one more.
    let (b, c) = (at(&editor, 1), at(&editor, 2));
    editor
        .edit("Level", |edit| edit.change_list_level(b, c, 1, false))
        .unwrap();
    editor
        .edit("Level", |edit| edit.change_list_level(c, c, 1, false))
        .unwrap();
    assert_eq!(levels(&editor), [0, 1, 2, 0, 0]);
    // Tab on b takes c, its child, along; Option-Tab would not.
    editor
        .edit("Level", |edit| edit.change_list_level(b, b, 1, true))
        .unwrap();
    assert_eq!(levels(&editor), [0, 2, 3, 0, 0]);
    editor
        .edit("Level", |edit| edit.change_list_level(b, b, -1, false))
        .unwrap();
    assert_eq!(levels(&editor), [0, 1, 3, 0, 0]);
    // Levels stay between the first and the ninth.
    editor
        .edit("Level", |edit| edit.change_list_level(a, a, -1, false))
        .unwrap();
    assert_eq!(levels(&editor)[0], 0);
    // Out of the list: the paragraph has no label.
    editor
        .edit("No list", |edit| edit.set_list_style(e, e, None))
        .unwrap();
    assert_eq!(labels(&editor)[4], "");
    // Return in an item makes another item.
    let end = Position::new(a.paragraph, 1);
    editor
        .edit("Return", |edit| edit.split_paragraph(end))
        .unwrap();
    assert_eq!(labels(&editor)[1], "\u{2022}");
}

#[test]
fn lst_03_bullets_take_any_text() {
    let mut editor = editor();
    let bullet = add(&mut editor, ListStyle::bullet, "Bullet");
    let (a, b) = (at(&editor, 0), at(&editor, 1));
    let mut level = editor.document().styles().list(bullet).unwrap().levels[0].clone();
    level.label = ListLabel::bullet("+");
    level.size = 150;
    editor
        .edit("Bullet", |edit| {
            edit.set_list_style(a, b, Some(bullet))?;
            edit.set_list_level(bullet, 0, level.clone())
        })
        .unwrap();
    assert_eq!(&labels(&editor)[..2], ["+", "+"]);
    let items = editor
        .document()
        .styles()
        .list_items(editor.document().body().iter());
    assert_eq!(items[0].as_ref().unwrap().definition.size, 150);
}

#[test]
fn lst_04_numbers_count_on_and_start_again() {
    let mut editor = editor();
    let numbered = add(&mut editor, ListStyle::numbered, "Numbered List");
    let (a, b, d, e) = (
        at(&editor, 0),
        at(&editor, 1),
        at(&editor, 3),
        at(&editor, 4),
    );
    editor
        .edit("List", |edit| {
            edit.set_list_style(a, b, Some(numbered))?;
            edit.set_list_style(d, e, Some(numbered))
        })
        .unwrap();
    // The list after the paragraph outside it continues from the first.
    assert_eq!(labels(&editor), ["1.", "2.", "", "3.", "4."]);
    editor
        .edit("Start", |edit| edit.set_list_start(d.paragraph, Some(1)))
        .unwrap();
    assert_eq!(labels(&editor), ["1.", "2.", "", "1.", "2."]);
    // Roman numbers and letters, with a prefix and a suffix.
    let mut level = editor.document().styles().list(numbered).unwrap().levels[0].clone();
    level.label = ListLabel::Number {
        format: NumberFormat::UpperRoman,
        prefix: "(".into(),
        suffix: ")".into(),
        tiered: false,
    };
    level.start = 3;
    editor
        .edit("Roman", |edit| edit.set_list_level(numbered, 0, level))
        .unwrap();
    editor
        .edit("Start", |edit| edit.set_list_start(d.paragraph, None))
        .unwrap();
    assert_eq!(labels(&editor), ["(III)", "(IV)", "", "(V)", "(VI)"]);
    // Return keeps counting.
    editor
        .edit("Return", |edit| {
            edit.set_list_start(a.paragraph, Some(1))?;
            edit.split_paragraph(Position::new(a.paragraph, 1))
        })
        .unwrap();
    assert_eq!(&labels(&editor)[..3], ["(I)", "(II)", "(III)"]);
}

#[test]
fn lst_05_tiered_numbers_take_the_label_above() {
    let mut editor = editor();
    let legal = add(&mut editor, ListStyle::legal, "Legal");
    let (a, e) = (at(&editor, 0), at(&editor, 4));
    editor
        .edit("List", |edit| {
            edit.set_list_style(a, e, Some(legal))?;
            for (index, by) in [(1, 1), (2, 2), (3, 1)] {
                let p = Position::new(edit.document().body().get(index).unwrap().id(), 0);
                edit.change_list_level(p, p, by, false)?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(labels(&editor), ["1.", "1.1.", "1.1.1.", "1.2.", "2."]);

    let harvard = add(&mut editor, ListStyle::harvard, "Harvard");
    editor
        .edit("Harvard", |edit| edit.set_list_style(a, e, Some(harvard)))
        .unwrap();
    assert_eq!(labels(&editor), ["I.", "A.", "1.", "B.", "II."]);
}

#[test]
fn sty_01_list_styles_are_a_family_of_their_own() {
    let mut editor = editor();
    let bullet = add(&mut editor, ListStyle::bullet, "Bullet");
    // A paragraph style that names a list style makes lists.
    let id = editor
        .edit("Style", |edit| {
            let id = edit.new_id();
            let mut style = ParagraphStyle::new(id, "Bulleted");
            style.paragraph.list_style = Some(Some(bullet));
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })?;
            let a = Position::new(edit.document().body().get(0).unwrap().id(), 0);
            edit.set_paragraph_style(a, a, id)?;
            Ok(id)
        })
        .unwrap();
    assert_eq!(labels(&editor)[0], "\u{2022}");
    // The list style cannot go while it is used, and deleting it takes
    // the lists out.
    let result = editor.edit("Remove", |edit| {
        edit.apply(desk_model::Op::RemoveStyle { style: bullet })
    });
    assert!(result.is_err());
    editor
        .edit("Delete", |edit| edit.delete_style(bullet, None))
        .unwrap();
    assert_eq!(labels(&editor)[0], "");
    assert!(matches!(
        editor.document().styles().get(id),
        Some(Style::Paragraph(_))
    ));
    // A paragraph style cannot name a paragraph style as its list.
    let result = editor.edit("Wrong", |edit| {
        let mut style = edit.document().styles().paragraph(id).unwrap().clone();
        style.paragraph.list_style = Some(Some(id));
        edit.apply(desk_model::Op::PutStyle {
            style: style.into(),
        })
    });
    assert!(result.is_err());
}

#[test]
fn sty_08_10_list_styles_travel_with_the_styles_that_name_them() {
    let mut source = editor();
    let bullet = add(&mut source, ListStyle::bullet, "Bullet");
    let style = source
        .edit("Style", |edit| {
            let id = edit.new_id();
            let mut style = ParagraphStyle::new(id, "Bulleted");
            style.paragraph.list_style = Some(Some(bullet));
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })?;
            let a = Position::new(edit.document().body().get(0).unwrap().id(), 0);
            edit.set_paragraph_style(a, a, id)?;
            Ok(id)
        })
        .unwrap();
    let source = source.document().clone();

    // Import.
    let mut target = editor();
    let imported = target
        .edit("Import", |edit| {
            edit.import_styles(&source, &[style], false)
        })
        .unwrap();
    let styles = target.document().styles();
    let list = styles.by_name(StyleFamily::List, "Bullet").unwrap().id();
    assert_eq!(
        styles.paragraph(imported[0]).unwrap().paragraph.list_style,
        Some(Some(list))
    );

    // Copy and paste the style of an item.
    let mut target = editor();
    let copied = source.copy_style(source.start()).unwrap();
    let a = at(&target, 2);
    target
        .edit("Paste", |edit| edit.paste_style(a, a, &copied))
        .unwrap();
    assert_eq!(labels(&target)[2], "\u{2022}");
}

#[test]
fn lst_03_items_change_over_their_list_style() {
    let mut editor = editor();
    let bullet = add(&mut editor, ListStyle::bullet, "Bullet");
    let (a, c) = (at(&editor, 0), at(&editor, 2));
    editor
        .edit("List", |edit| edit.set_list_style(a, c, Some(bullet)))
        .unwrap();
    // The second item gets a bullet of its own; the style stays.
    let b = at(&editor, 1);
    editor
        .edit("Bullet", |edit| {
            edit.change_list_items(b, b, |level| level.label = ListLabel::bullet("+"))
        })
        .unwrap();
    assert_eq!(&labels(&editor)[..3], ["\u{2022}", "+", "\u{2022}"]);
    let style = editor.document().styles().list(bullet).unwrap().clone();
    assert_eq!(style.levels[0].label, ListLabel::bullet("\u{2022}"));
    let overrides = |editor: &Editor, at: Position| editor.document().overrides(at, at).unwrap();
    assert!(overrides(&editor, b).list);
    assert!(!overrides(&editor, a).list);
    // Tab keeps its bullet, at the indents of the deeper level.
    editor
        .edit("Tab", |edit| edit.change_list_level(b, b, 1, false))
        .unwrap();
    let items = editor
        .document()
        .styles()
        .list_items(editor.document().body().iter());
    let item = items[1].as_ref().unwrap();
    assert_eq!(item.label, "+");
    assert_eq!(item.definition.label_indent, style.levels[1].label_indent);
    editor
        .edit("Tab", |edit| edit.change_list_level(b, b, -1, false))
        .unwrap();
    // Redefining the style from the item gives every item its bullet.
    editor
        .edit("Redefine", |edit| edit.redefine_style(bullet, b, b))
        .unwrap();
    assert_eq!(&labels(&editor)[..3], ["+", "+", "+"]);
    assert!(!overrides(&editor, b).list);
    // An item changed back to its style has nothing of its own.
    editor
        .edit("Bullet", |edit| {
            edit.change_list_items(a, a, |level| level.size = 150)
        })
        .unwrap();
    assert!(overrides(&editor, a).list);
    editor
        .edit("Revert", |edit| edit.revert_to_list_style(a, a))
        .unwrap();
    assert!(!overrides(&editor, a).list);
    editor
        .edit("Bullet", |edit| {
            edit.change_list_items(a, a, |level| level.size = 150)?;
            edit.change_list_items(a, a, |level| level.size = 100)
        })
        .unwrap();
    assert!(!overrides(&editor, a).list);
}
