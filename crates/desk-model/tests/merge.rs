// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preview and the merged document of a mail merge (W105, MAL-04).

use desk_model::{
    CharAttrs, Document, Editor, FIELD_CHARACTER, Field, MergeSource, Tracking, merge_document,
};

/// A letter to a first name, from a company.
fn letter() -> Document {
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Letter", |edit| {
            let mut at = edit.document().start();
            for (text, field) in [
                ("Dear ", Field::merge("First Name")),
                (
                    ", from ",
                    Field::Merge {
                        name: "Company".into(),
                        sender: true,
                        single_line: false,
                        shown: None,
                        origin: None,
                    },
                ),
            ] {
                at = edit.insert_text(at, text)?;
                let end = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
                edit.set_character_attrs(
                    at,
                    end,
                    &CharAttrs {
                        field: Some(Some(field)),
                        ..CharAttrs::default()
                    },
                )?;
                at = end;
            }
            let at = edit.split_paragraph(at)?;
            edit.insert_text(at, "Yours.")?;
            let mut settings = edit.document().text_settings().clone();
            settings.merge_source = Some(MergeSource::Contacts { group: None });
            settings.tracking = Tracking::On;
            edit.set_text_settings(settings)
        })
        .unwrap();
    editor.document().clone()
}

/// The text of each paragraph of the body, with the fields as they show.
fn texts(document: &Document) -> Vec<String> {
    document
        .body()
        .iter()
        .map(|p| {
            p.runs()
                .map(|r| match r.attrs.field.clone().flatten() {
                    Some(field) => field.text(1, 1),
                    None => r.text().to_string(),
                })
                .collect()
        })
        .collect()
}

const NAMES: [&str; 3] = ["Ana", "Bruno", "Carla"];

fn value(record: usize, name: &str, sender: bool) -> Option<String> {
    match (name, sender) {
        ("First Name", false) => Some(NAMES[record].into()),
        ("Company", true) => Some("Desk".into()),
        _ => None,
    }
}

#[test]
fn mal_04_the_preview_fills_the_fields_with_a_record() {
    let mut editor = Editor::new(letter());
    assert_eq!(texts(editor.document())[0], "Dear First Name, from Company");
    editor
        .edit("Preview", |edit| {
            edit.fill_merge_fields(&|name, sender| value(1, name, sender))
        })
        .unwrap();
    assert_eq!(
        texts(editor.document()),
        ["Dear Bruno, from Desk", "Yours."]
    );
}

#[test]
fn mal_04_mal_01_the_merged_document_has_the_body_once_for_each_record() {
    let letter = letter();
    let merged = merge_document(&letter, 3, &value).unwrap();
    assert_eq!(
        texts(&merged),
        [
            "Dear Ana, from Desk",
            "Yours.",
            "Dear Bruno, from Desk",
            "Yours.",
            "Dear Carla, from Desk",
            "Yours.",
        ]
    );
    // Each letter starts a page.
    let breaks: Vec<_> = merged
        .body()
        .iter()
        .map(|p| p.attrs().page_break_before == Some(true))
        .collect();
    assert_eq!(breaks, [false, false, true, false, true, false]);
    // Only the sender fields stay fields (MAL-01).
    let fields: Vec<_> = merged
        .body()
        .iter()
        .flat_map(|p| {
            p.runs()
                .filter_map(|r| r.attrs.field.clone().flatten())
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(fields.len(), 3);
    assert!(
        fields
            .iter()
            .all(|f| matches!(f, Field::Merge { sender: true, .. }))
    );
    assert_eq!(merged.text_settings().tracking, Tracking::Off);
    assert!(merged.changes().is_empty());
    assert_eq!(merged.text_settings().merge_source, None);
    // The document merged from stays as it was.
    assert_eq!(texts(&letter)[0], "Dear First Name, from Company");
    // No records, the first letter alone stays with its names.
    let none = merge_document(&letter, 1, &|_, _, _| None).unwrap();
    assert_eq!(texts(&none), ["Dear First Name, from Company", "Yours."]);
}

#[test]
fn mal_04_the_notes_of_each_letter_have_its_data() {
    use desk_model::{Note, ParaAttrs, Paragraph, Run};
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Note", |edit| {
            let at = edit.insert_text(edit.document().start(), "Letter")?;
            let note = edit.new_id();
            let after = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
            let mark = CharAttrs {
                field: Some(Some(Field::NoteMark { note })),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(at, after, &mark)?;
            let style = edit.document().styles().default_paragraph();
            let text = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs::default(),
                [
                    Run::new("To "),
                    Run::with_attrs(
                        &FIELD_CHARACTER.to_string(),
                        None,
                        CharAttrs {
                            field: Some(Some(Field::merge("First Name"))),
                            ..CharAttrs::default()
                        },
                    ),
                ],
            )?;
            edit.set_note(
                note,
                Some(Note {
                    id: note,
                    mark: None,
                    paragraphs: vec![text].into(),
                }),
            )
        })
        .unwrap();
    let merged = merge_document(editor.document(), 2, &value).unwrap();
    let mut notes: Vec<String> = merged
        .notes()
        .iter()
        .map(|n| n.paragraphs.iter().map(|p| p.text()).collect())
        .collect();
    notes.sort();
    assert_eq!(notes, ["To Ana", "To Bruno"]);
}
