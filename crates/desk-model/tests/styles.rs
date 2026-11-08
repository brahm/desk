// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Model tests for working with styles (STY-02 to STY-10). Each test name
//! starts with the row ID.

use desk_model::FontWeight;
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
            style.character.weight = Some(FontWeight::BOLD);
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
                    weight: Some(FontWeight::BOLD),
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
            weight: Some(FontWeight::BOLD),
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
    assert!(look(&editor, 0, 4).italic && look(&editor, 0, 4).weight.is_bold());
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
                    weight: Some(FontWeight::NORMAL),
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
    assert!(look(&editor, 0, 0).weight.is_bold());
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
                    weight: Some(FontWeight::NORMAL),
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
    assert_eq!(style.character.weight, Some(FontWeight::BOLD));
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
    assert_eq!(style.character.weight, Some(FontWeight::NORMAL));
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
    assert!(format.weight.is_bold() && format.italic);
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
    assert!(look.weight.is_bold());
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

#[test]
fn txt_04_paste_keeps_the_styles_of_the_copied_text() {
    let (mut source, heading) = editor();
    let (a, b, c) = (at(&source, 0, 0), at(&source, 0, 4), at(&source, 0, 7));
    source
        .edit("Format", |edit| {
            edit.set_paragraph_style(a, a, heading)?;
            // "two" in italics.
            edit.set_character_attrs(b, c, &italic())
        })
        .unwrap();
    // "two three" of the heading and "fo" of the next paragraph.
    let fragment = source
        .document()
        .copy_fragment(at(&source, 0, 4), at(&source, 1, 2))
        .unwrap();
    assert_eq!(fragment.text(), "two three\nfo");
    assert!(!fragment.whole_start);

    // Into the middle of a paragraph of a document without Heading.
    let mut target = Editor::new(Document::new_word_processing());
    let start = target.document().start();
    target
        .edit("Typing", |edit| edit.insert_text(start, "abcd").map(|_| ()))
        .unwrap();
    let middle = at(&target, 0, 2);
    let end = target
        .edit("Paste", |edit| edit.paste_fragment(middle, &fragment))
        .unwrap();
    assert_eq!(paragraph(&target, 0).text(), "abtwo three");
    assert_eq!(paragraph(&target, 1).text(), "focd");
    // The text that joins the paragraph keeps its look there.
    let joined = look(&target, 0, 2);
    assert!(joined.weight.is_bold());
    assert_eq!(joined.font_size, Length::from_points(18));
    assert!(look(&target, 0, 3).italic);
    assert!(!look(&target, 0, 0).weight.is_bold());
    // The next paragraph takes its own style, which comes along by name.
    let styles = target.document().styles();
    let default = styles.default_paragraph();
    assert_eq!(paragraph(&target, 1).style(), default);
    assert!(styles.by_name(StyleFamily::Paragraph, "Heading").is_none());
    assert_eq!(end, at(&target, 1, 2));

    // A whole paragraph into an empty one takes its style.
    let whole = source
        .document()
        .copy_fragment(at(&source, 0, 0), at(&source, 0, 13))
        .unwrap();
    let mut empty = Editor::new(Document::new_word_processing());
    let start = empty.document().start();
    empty
        .edit("Paste", |edit| edit.paste_fragment(start, &whole))
        .unwrap();
    let styles = empty.document().styles();
    let added = styles
        .by_name(StyleFamily::Paragraph, "Heading")
        .expect("the style comes along");
    assert_eq!(paragraph(&empty, 0).style(), added.id());
    assert!(paragraph(&empty, 0).runs().next().unwrap().attrs.is_empty());
    assert!(look(&empty, 0, 4).italic);
}

#[test]
fn int_22_a_missing_font_is_replaced_everywhere() {
    let (mut editor, heading) = editor();
    let (a, b) = (at(&editor, 0, 0), at(&editor, 1, 2));
    let palatino = CharAttrs {
        font_family: Some("Palatino".into()),
        ..CharAttrs::default()
    };
    editor
        .edit("Format", |edit| {
            edit.set_paragraph_style(a, a, heading)?;
            let mut style = edit.document().styles().paragraph(heading).unwrap().clone();
            style.character.font_family = Some("palatino".into());
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })?;
            edit.set_character_attrs(a, b, &palatino)
        })
        .unwrap();
    editor
        .edit("Replace Font", |edit| {
            edit.replace_font_family("Palatino", "TeX Gyre Pagella")
        })
        .unwrap();
    let styles = editor.document().styles();
    assert_eq!(
        styles
            .paragraph(heading)
            .unwrap()
            .character
            .font_family
            .as_deref(),
        Some("TeX Gyre Pagella")
    );
    for (index, offset) in [(0, 0), (1, 1), (1, 3)] {
        assert_eq!(
            &*look(&editor, index, offset).font_family,
            if (index, offset) == (1, 3) {
                "Helvetica"
            } else {
                "TeX Gyre Pagella"
            },
            "{index} {offset}"
        );
    }
    // The family that text has when no style sets one.
    editor
        .edit("Replace Font", |edit| {
            edit.replace_font_family("Helvetica", "TeX Gyre Heros")
        })
        .unwrap();
    assert_eq!(&*look(&editor, 1, 3).font_family, "TeX Gyre Heros");
    // One step of the history.
    editor.undo().unwrap();
    assert_eq!(&*look(&editor, 1, 3).font_family, "Helvetica");
}

#[test]
fn sty_10_text_in_locked_objects_takes_the_imported_style_and_keeps_its_look() {
    use std::sync::Arc;

    use desk_model::{Frame, Object, ObjectKind, Paragraph, Run, TextBox};
    let (mut target, heading) = editor();
    let ids = target
        .edit("Boxes", |edit| {
            let mut ids = Vec::new();
            for locked in [false, true] {
                let text = Paragraph::from_runs(
                    edit.new_id(),
                    heading,
                    ParaAttrs::default(),
                    vec![Run::new("Title")],
                )?;
                let mut object = Object::new(
                    edit.new_id(),
                    ObjectKind::TextBox(TextBox::new(Arc::from([text]), Length::from_points(4))),
                    Frame::from_pt(72.0, 72.0, 200.0, 80.0),
                );
                object.locked = locked;
                ids.push(object.id);
                edit.add_object(object)?;
            }
            Ok(ids)
        })
        .unwrap();
    // A Heading of 30 points, left aligned, from another document.
    let (mut source, theirs) = editor();
    source
        .edit("Change", |edit| {
            let mut style = edit.document().styles().paragraph(theirs).unwrap().clone();
            style.character.font_size = Some(Length::from_points(30));
            style.paragraph.alignment = Some(Alignment::Left);
            edit.apply(desk_model::Op::PutStyle {
                style: style.into(),
            })
        })
        .unwrap();
    target
        .edit("Import Styles", |edit| {
            edit.import_styles(source.document(), &[theirs], true)
        })
        .unwrap();
    let document = target.document();
    let styles = document.styles();
    let look = |id| {
        let object = document.objects().get(id).unwrap();
        let p = &object.text_box().unwrap().paragraphs[0];
        let run = p.runs().next().unwrap();
        (
            p.style(),
            styles.resolve_paragraph(p.style(), p.attrs()).alignment,
            styles
                .resolve_character(p.style(), run.style, &run.attrs)
                .font_size,
        )
    };
    // The unlocked box looks as the new style; the locked one as before.
    assert_eq!(
        look(ids[0]),
        (heading, Alignment::Left, Length::from_points(30))
    );
    assert_eq!(
        look(ids[1]),
        (heading, Alignment::Center, Length::from_points(18))
    );
}

#[test]
fn iop_17_iop_25_text_pasted_where_styles_are_not_kept_keeps_its_look() {
    // Heading text with a character style, copied whole, pasted into a
    // document that takes no named styles from it.
    let (mut source, heading) = editor();
    let (a, b, end) = (at(&source, 0, 0), at(&source, 0, 3), at(&source, 0, 13));
    source
        .edit("Setup", |edit| {
            edit.set_paragraph_style(a, a, heading)?;
            edit.new_character_style("Slanted", italic(), a, b, true)?;
            Ok(())
        })
        .unwrap();
    let fragment = source.document().copy_fragment(a, end).unwrap();

    let mut target = Editor::new(Document::new_word_processing());
    let start = target.document().start();
    let styles_before = target.document().styles().iter().count();
    target
        .edit("Paste", |edit| {
            edit.paste_fragment_resolved(start, &fragment).map(drop)
        })
        .unwrap();
    let document = target.document();
    assert_eq!(paragraph(&target, 0).text(), "one two three");
    // No style came with the text.
    assert_eq!(document.styles().iter().count(), styles_before);
    assert!(
        document
            .styles()
            .by_name(StyleFamily::Paragraph, "Heading")
            .is_none()
    );
    assert!(
        document
            .styles()
            .by_name(StyleFamily::Character, "Slanted")
            .is_none()
    );
    assert_eq!(
        paragraph(&target, 0).style(),
        document.styles().default_paragraph()
    );
    // It looks as it did: bold, 18 pt and centered, the first word slanted.
    let first = look(&target, 0, 1);
    assert!(first.weight.is_bold() && first.italic);
    assert_eq!(first.font_size, Length::from_points(18));
    assert!(!look(&target, 0, 6).italic);
    let p = paragraph(&target, 0);
    let format = document.styles().resolve_paragraph(p.style(), p.attrs());
    assert_eq!(format.alignment, Alignment::Center);
}
