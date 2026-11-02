// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Model tests for working with styles (STY-02 to STY-10). Each test name
//! starts with the row ID.

use desk_model::{
    Alignment, CharAttrs, CharacterStyle, Document, Editor, Length, ModelError, ParaAttrs,
    ParagraphStyle, Position, Style, StyleFamily, StyleId,
};

/// A document with "one two three" in a paragraph and "four" in another,
/// and a Heading style that is bold, 18 pt and centered.
fn editor() -> (Editor, StyleId) {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let heading = editor
        .edit("Setup", |edit| {
            edit.insert_text(start, "one two three\nfour")?;
            let id = edit.new_id();
            let mut style = ParagraphStyle::new(id, "Heading");
            style.paragraph.alignment = Some(Alignment::Center);
            style.character.bold = Some(true);
            style.character.font_size = Some(Length::from_points(18));
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })?;
            Ok(id)
        })
        .unwrap();
    (editor, heading)
}

fn paragraph(editor: &Editor, index: usize) -> desk_model::Paragraph {
    editor.document().body().get(index).unwrap().clone()
}

fn at(editor: &Editor, index: usize, offset: usize) -> Position {
    Position::new(paragraph(editor, index).id(), offset)
}

fn look(editor: &Editor, index: usize, offset: usize) -> desk_model::CharFormat {
    editor
        .document()
        .character_formats_at(at(editor, index, offset))
        .unwrap()
        .0
}

fn italic() -> CharAttrs {
    CharAttrs {
        italic: Some(true),
        ..CharAttrs::default()
    }
}

#[test]
fn sty_03_paragraph_style_from_selection_keeps_the_look() {
    let (mut editor, heading) = editor();
    let (a, b) = (at(&editor, 0, 0), at(&editor, 0, 3));
    editor
        .edit("Format", |edit| {
            edit.set_paragraph_style(a, a, heading)?;
            edit.set_character_attrs(a, at_end(edit, a), &italic())?;
            edit.set_paragraph_attrs(
                a,
                a,
                &ParaAttrs {
                    alignment: Some(Alignment::Right),
                    ..ParaAttrs::default()
                },
            )
        })
        .unwrap();
    let before = look(&editor, 0, 0);
    let id = editor
        .edit("New Style", |edit| {
            edit.new_paragraph_style("Mine", a, b, true)
        })
        .unwrap();
    let p = paragraph(&editor, 0);
    assert_eq!(p.style(), id);
    // The style holds the look, so nothing is left over it.
    assert!(p.attrs().is_empty());
    assert!(p.runs().all(|run| run.attrs.is_empty()));
    assert_eq!(look(&editor, 0, 0), before);
    let styles = editor.document().styles();
    assert_eq!(
        styles
            .resolve_paragraph(id, &ParaAttrs::default())
            .alignment,
        Alignment::Right
    );
    assert_eq!(styles.paragraph(id).unwrap().parent, None);
    // A second style cannot take the name.
    let error = editor
        .edit("New Style", |edit| {
            edit.new_paragraph_style("Mine", a, b, false)
        })
        .unwrap_err();
    assert_eq!(error, ModelError::DuplicateStyleName("Mine".into()));
}

fn at_end(edit: &desk_model::Edit, at: Position) -> Position {
    Position::new(
        at.paragraph,
        edit.document().paragraph(at.paragraph).unwrap().len(),
    )
}

#[test]
fn sty_03_character_style_from_selection_takes_the_chosen_attributes() {
    let (mut editor, _) = editor();
    let (a, b) = (at(&editor, 0, 4), at(&editor, 0, 7));
    editor
        .edit("Format", |edit| {
            edit.set_character_attrs(
                a,
                b,
                &CharAttrs {
                    italic: Some(true),
                    bold: Some(true),
                    ..CharAttrs::default()
                },
            )
        })
        .unwrap();
    let (format, paragraph_only) = editor.document().character_formats_at(a).unwrap();
    let attrs = CharAttrs::from(format.clone()).difference(&paragraph_only);
    assert_eq!(
        attrs,
        CharAttrs {
            italic: Some(true),
            bold: Some(true),
            ..CharAttrs::default()
        }
    );
    // Not applied when made: the text keeps its attributes.
    let kept = editor
        .edit("New Style", |edit| {
            edit.new_character_style("Loud", attrs.clone(), a, b, false)
        })
        .unwrap();
    assert!(paragraph(&editor, 0).runs().all(|run| run.style.is_none()));
    // Applied: the text takes the style and loses what the style sets.
    let id = editor
        .edit("New Style", |edit| {
            edit.new_character_style("Loud 2", attrs.clone(), a, b, true)
        })
        .unwrap();
    assert_ne!(kept, id);
    let p = paragraph(&editor, 0);
    let run = p.runs().find(|run| run.text() == "two").unwrap();
    assert_eq!(run.style, Some(id));
    assert!(run.attrs.is_empty());
    assert_eq!(look(&editor, 0, 4), format);
}

#[test]
fn sty_02_character_styles_survive_a_new_paragraph_style() {
    let (mut editor, heading) = editor();
    let (a, b) = (at(&editor, 0, 4), at(&editor, 0, 7));
    let id = editor
        .edit("New Style", |edit| {
            edit.new_character_style("Slanted", italic(), a, b, true)
        })
        .unwrap();
    editor
        .edit("Paragraph Style", |edit| {
            edit.set_paragraph_style(a, a, heading)
        })
        .unwrap();
    let p = paragraph(&editor, 0);
    assert_eq!(p.style(), heading);
    assert!(p.runs().any(|run| run.style == Some(id)));
    assert!(look(&editor, 0, 4).italic && look(&editor, 0, 4).bold);
}

#[test]
fn sty_04_shortcuts_are_unique_and_clear_overrides_on_request() {
    let (mut editor, heading) = editor();
    let body = editor.document().styles().default_paragraph();
    editor
        .edit("Shortcut", |edit| edit.set_style_shortcut(heading, Some(1)))
        .unwrap();
    editor
        .edit("Shortcut", |edit| edit.set_style_shortcut(body, Some(1)))
        .unwrap();
    let styles = editor.document().styles();
    assert_eq!(styles.by_shortcut(1).map(Style::id), Some(body));
    assert_eq!(styles.get(heading).unwrap().shortcut(), None);
    let error = editor
        .edit("Shortcut", |edit| edit.set_style_shortcut(heading, Some(9)))
        .unwrap_err();
    assert_eq!(error, ModelError::InvalidShortcut(9));

    let a = at(&editor, 0, 0);
    let end = at(&editor, 0, 13);
    editor
        .edit("Format", |edit| edit.set_character_attrs(a, end, &italic()))
        .unwrap();
    editor
        .edit("Paragraph Style", |edit| {
            edit.set_paragraph_style(a, a, heading)
        })
        .unwrap();
    assert!(look(&editor, 0, 0).italic);
    editor
        .edit("Paragraph Style", |edit| {
            edit.set_paragraph_style_clean(a, a, heading)
        })
        .unwrap();
    assert!(!look(&editor, 0, 0).italic);
}

#[test]
fn sty_05_rename_and_delete_with_a_replacement() {
    let (mut editor, heading) = editor();
    let a = at(&editor, 0, 0);
    let (child, next_of) = editor
        .edit("Setup", |edit| {
            edit.set_paragraph_style(a, a, heading)?;
            let child = edit.new_id();
            let mut style = ParagraphStyle::new(child, "Subheading");
            style.parent = Some(heading);
            style.character.italic = Some(true);
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })?;
            let next_of = edit.new_id();
            let mut style = ParagraphStyle::new(next_of, "Lead");
            style.next = Some(heading);
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })?;
            Ok((child, next_of))
        })
        .unwrap();
    editor
        .edit("Rename", |edit| edit.rename_style(heading, "Title"))
        .unwrap();
    assert_eq!(
        editor.document().styles().get(heading).unwrap().name(),
        "Title"
    );
    let error = editor
        .edit("Rename", |edit| edit.rename_style(child, "Body"))
        .unwrap_err();
    assert_eq!(error, ModelError::DuplicateStyleName("Body".into()));

    let child_look =
        editor
            .document()
            .styles()
            .resolve_character(child, None, &CharAttrs::default());
    editor
        .edit("Delete Style", |edit| {
            edit.delete_style(heading, Some(next_of))
        })
        .unwrap();
    let doc = editor.document();
    let styles = doc.styles();
    assert!(styles.get(heading).is_none());
    assert_eq!(paragraph(&editor, 0).style(), next_of);
    // The child looks as it did, and the next style moved on.
    assert_eq!(
        styles.resolve_character(child, None, &CharAttrs::default()),
        child_look
    );
    assert_eq!(styles.paragraph(next_of).unwrap().next, Some(next_of));
    // Undo brings it all back.
    editor.undo().unwrap();
    assert_eq!(paragraph(&editor, 0).style(), heading);
    assert_eq!(
        editor.document().styles().paragraph(child).unwrap().parent,
        Some(heading)
    );
    // The default style stays.
    let body = editor.document().styles().default_paragraph();
    let error = editor
        .edit("Delete Style", |edit| edit.delete_style(body, None))
        .unwrap_err();
    assert_eq!(error, ModelError::DefaultStyle);
}

#[test]
fn sty_05_deleting_a_character_style_can_leave_none() {
    let (mut editor, _) = editor();
    let (a, b) = (at(&editor, 0, 4), at(&editor, 0, 7));
    let id = editor
        .edit("New Style", |edit| {
            edit.new_character_style("Slanted", italic(), a, b, true)
        })
        .unwrap();
    editor
        .edit("Delete Style", |edit| edit.delete_style(id, None))
        .unwrap();
    assert!(paragraph(&editor, 0).runs().all(|run| run.style.is_none()));
    assert!(!look(&editor, 0, 4).italic);
}

#[test]
fn sty_06_overrides_redefine_and_revert() {
    let (mut editor, heading) = editor();
    let a = at(&editor, 0, 0);
    let end = at(&editor, 0, 13);
    let second = at(&editor, 1, 0);
    editor
        .edit("Format", |edit| {
            edit.set_paragraph_style(a, second, heading)?;
            edit.set_character_attrs(a, end, &italic())
        })
        .unwrap();
    let doc = editor.document();
    assert!(doc.overrides(a, a).unwrap().paragraph);
    assert!(!doc.overrides(second, second).unwrap().paragraph);
    // Redefining takes the italic into the style, for every paragraph.
    editor
        .edit("Redefine Style", |edit| edit.redefine_style(heading, a, a))
        .unwrap();
    assert!(look(&editor, 1, 0).italic);
    assert!(!editor.document().overrides(a, a).unwrap().paragraph);
    assert!(paragraph(&editor, 0).runs().all(|run| run.attrs.is_empty()));
    // Reverting drops what is set over the style.
    editor
        .edit("Format", |edit| {
            edit.set_paragraph_attrs(
                a,
                a,
                &ParaAttrs {
                    alignment: Some(Alignment::Left),
                    ..ParaAttrs::default()
                },
            )?;
            edit.set_character_attrs(
                a,
                end,
                &CharAttrs {
                    bold: Some(false),
                    ..CharAttrs::default()
                },
            )
        })
        .unwrap();
    assert!(editor.document().overrides(a, a).unwrap().paragraph);
    editor
        .edit("Revert to Defined Style", |edit| edit.revert_to_style(a, a))
        .unwrap();
    assert!(!editor.document().overrides(a, a).unwrap().paragraph);
    assert!(look(&editor, 0, 0).bold);
}

#[test]
fn sty_06_redefining_a_paragraph_style_keeps_character_styles() {
    let (mut editor, heading) = editor();
    let (a, b) = (at(&editor, 0, 4), at(&editor, 0, 7));
    let slanted = editor
        .edit("Setup", |edit| {
            edit.set_paragraph_style(a, a, heading)?;
            let id = edit.new_character_style("Slanted", italic(), a, b, true)?;
            edit.set_character_attrs(
                a,
                b,
                &CharAttrs {
                    bold: Some(false),
                    ..CharAttrs::default()
                },
            )?;
            Ok(id)
        })
        .unwrap();
    editor
        .edit("Redefine Style", |edit| edit.redefine_style(heading, a, a))
        .unwrap();
    let p = paragraph(&editor, 0);
    let run = p.runs().find(|run| run.text() == "two").unwrap();
    assert_eq!(run.style, Some(slanted));
    // The paragraph style did not take the attributes of the character
    // style or of its text.
    let style = editor
        .document()
        .styles()
        .paragraph(heading)
        .unwrap()
        .clone();
    assert_eq!(style.character.italic, None);
    assert_eq!(style.character.bold, Some(true));
    assert!(editor.document().overrides(a, b).unwrap().character);
    // Redefining the character style takes the bold off into it.
    editor
        .edit("Redefine Style", |edit| edit.redefine_style(slanted, a, b))
        .unwrap();
    let style = editor
        .document()
        .styles()
        .character(slanted)
        .unwrap()
        .clone();
    assert_eq!(style.character.bold, Some(false));
    assert!(!editor.document().overrides(a, b).unwrap().character);
}

#[test]
fn sty_07_return_at_the_end_takes_the_next_style() {
    let (mut editor, heading) = editor();
    let body = editor.document().styles().default_paragraph();
    editor
        .edit("Setup", |edit| {
            let mut style = edit.document().styles().paragraph(heading).unwrap().clone();
            style.next = Some(body);
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })?;
            let a = Position::new(edit.document().body().get(0).unwrap().id(), 0);
            edit.set_paragraph_style(a, a, heading)
        })
        .unwrap();
    let end = at(&editor, 0, 13);
    let new = editor
        .edit("Typing", |edit| edit.split_paragraph(end))
        .unwrap();
    assert_eq!(
        editor.document().paragraph(new.paragraph).unwrap().style(),
        body
    );
}

#[test]
fn sty_08_paste_style_brings_the_style_into_another_document() {
    let (mut source, heading) = editor();
    let (a, b) = (at(&source, 0, 4), at(&source, 0, 7));
    source
        .edit("Setup", |edit| {
            edit.set_paragraph_style(a, a, heading)?;
            edit.new_character_style("Slanted", italic(), a, b, true)?;
            Ok(())
        })
        .unwrap();
    let copied = source.document().copy_style(a).unwrap();
    assert_eq!(&*copied.paragraph.name, "Heading");
    assert_eq!(copied.character.as_ref().map(|c| &*c.name), Some("Slanted"));

    let mut target = Editor::new(Document::new_word_processing());
    let start = target.document().start();
    target
        .edit("Typing", |edit| {
            edit.insert_text(start, "plain text").map(drop)
        })
        .unwrap();
    let (x, y) = (at(&target, 0, 0), at(&target, 0, 5));
    let text_before = paragraph(&target, 0).text();
    target
        .edit("Paste Style", |edit| edit.paste_style(x, y, &copied))
        .unwrap();
    assert_eq!(paragraph(&target, 0).text(), text_before);
    let styles = target.document().styles().clone();
    let pasted = styles
        .by_name(StyleFamily::Paragraph, "Heading")
        .unwrap()
        .id();
    assert_eq!(paragraph(&target, 0).style(), pasted);
    let format = look(&target, 0, 0);
    assert!(format.bold && format.italic);
    assert_eq!(format.font_size, Length::from_points(18));
    // The text after the selection only takes the paragraph style.
    assert!(!look(&target, 0, 7).italic);
}

#[test]
fn sty_09_all_uses_of_a_style() {
    let (mut editor, heading) = editor();
    let (a, b) = (at(&editor, 0, 4), at(&editor, 0, 7));
    let second = at(&editor, 1, 0);
    let slanted = editor
        .edit("Setup", |edit| {
            edit.set_paragraph_style(second, second, heading)?;
            edit.new_character_style("Slanted", italic(), a, b, true)
        })
        .unwrap();
    let doc = editor.document();
    assert_eq!(doc.style_uses(heading), vec![(second.paragraph, 0..4)]);
    assert_eq!(doc.style_uses(slanted), vec![(a.paragraph, 4..7)]);
}

#[test]
fn sty_10_import_replaces_or_renames_duplicates() {
    let (source, heading) = editor();
    let mut source = source;
    let character = source
        .edit("Setup", |edit| {
            let id = edit.new_id();
            let mut style = CharacterStyle::new(id, "Slanted");
            style.character = italic();
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })?;
            Ok(id)
        })
        .unwrap();
    let source_body = source.document().styles().default_paragraph();
    let source = source.document().clone();

    let mut target = Editor::new(Document::new_word_processing());
    let body = target.document().styles().default_paragraph();
    // Without replacing: the Body of the source comes as Body 2.
    let ids = target
        .edit("Import Styles", |edit| {
            edit.import_styles(&source, &[source_body, heading, character], false)
        })
        .unwrap();
    let styles = target.document().styles().clone();
    assert_eq!(styles.get(ids[0]).unwrap().name(), "Body 2");
    assert_ne!(ids[0], body);
    assert_eq!(styles.get(ids[1]).unwrap().name(), "Heading");
    let look = styles.resolve_character(ids[1], None, &CharAttrs::default());
    assert!(look.bold);
    assert_eq!(look.font_size, Length::from_points(18));
    assert!(styles.character(ids[2]).unwrap().character.italic == Some(true));
    // Again, replacing: the styles keep their identifiers.
    let mut changed = source.clone();
    changed
        .transact("Change", |edit| {
            let mut style = edit.document().styles().paragraph(heading).unwrap().clone();
            style.character.font_size = Some(Length::from_points(30));
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })
        })
        .unwrap();
    let again = target
        .edit("Import Styles", |edit| {
            edit.import_styles(&changed, &[heading], true)
        })
        .unwrap();
    assert_eq!(again[0], ids[1]);
    let styles = target.document().styles();
    assert_eq!(
        styles
            .resolve_character(ids[1], None, &CharAttrs::default())
            .font_size,
        Length::from_points(30)
    );
    assert_eq!(
        styles
            .iter()
            .filter(|s| s.family() == StyleFamily::Paragraph && s.name().starts_with("Heading"))
            .count(),
        1
    );
}
