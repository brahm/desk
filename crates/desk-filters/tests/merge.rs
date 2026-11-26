// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The merge fields of a mail merge and their source (W105), in ODT and
//! from Pages '09.

use desk_filters::{read_odt, read_pages_at, write_odt};
use desk_model::{CharAttrs, Document, Editor, FIELD_CHARACTER, Field, MergeSource};

/// The fields of the body, in order.
fn fields(document: &Document) -> Vec<Field> {
    document
        .body()
        .iter()
        .flat_map(|p| {
            p.runs()
                .filter_map(|r| r.attrs.field.clone().flatten())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn mal_01_mal_07_mal_08_merge_fields_and_their_source_round_trip() {
    let mut editor = Editor::new(Document::new_word_processing());
    let merge = [
        Field::merge("First Name"),
        Field::Merge {
            name: "Company".into(),
            sender: true,
            single_line: false,
            shown: Some("Desk & Co.".into()),
            origin: None,
        },
        Field::Merge {
            name: "Home Address".into(),
            sender: false,
            single_line: true,
            shown: None,
            origin: None,
        },
        // Filled, an address on one line keeps its lines.
        Field::Merge {
            name: "Work Address".into(),
            sender: false,
            single_line: true,
            shown: Some("1 Infinite Loop\nCupertino CA 95014".into()),
            origin: None,
        },
        // A field of LibreOffice keeps its source and its table.
        Field::Merge {
            name: "City".into(),
            sender: false,
            single_line: false,
            shown: None,
            origin: Some(std::sync::Arc::new(("Addresses".into(), "Clients".into()))),
        },
    ];
    editor
        .edit("Fields", |edit| {
            let mut at = edit.document().start();
            for (text, field) in [
                ("Dear ", 0),
                (", from ", 1),
                (" to ", 2),
                (" at ", 3),
                (", ", 4),
            ] {
                at = edit.insert_text(at, text)?;
                let end = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
                edit.set_character_attrs(
                    at,
                    end,
                    &CharAttrs {
                        field: Some(Some(merge[field].clone())),
                        ..CharAttrs::default()
                    },
                )?;
                at = end;
            }
            let mut settings = edit.document().text_settings().clone();
            settings.merge_source = Some(MergeSource::Contacts {
                group: Some("Clients".into()),
            });
            edit.set_text_settings(settings)
        })
        .unwrap();
    let document = editor.document();
    let bytes = write_odt(document).unwrap();
    if let Some(dir) = std::env::var_os("DESK_ODF_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("merge-fields.odt"), &bytes).unwrap();
    }
    let content = {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut content = String::new();
        zip.by_name("content.xml")
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        content
    };
    // Each is a field of a column of ODF, which shows its name or its
    // datum.
    assert!(
        content.contains(
            r#"text:column-name="First Name" text:database-name="Desk Mail Merge">&lt;First Name&gt;<"#
        ),
        "{content}"
    );
    assert!(content.contains(r#"desk:merge-sender="true""#));
    assert!(content.contains(r#"desk:filled="true">Desk &amp; Co.<"#));
    assert!(content.contains(r#"desk:single-line="true""#));
    assert!(content.contains(
        r#"desk:datum="1 Infinite Loop&#10;Cupertino CA 95014">1 Infinite Loop, Cupertino CA 95014<"#
    ));
    assert!(content.contains(
        r#"text:table-name="Clients" text:table-type="table" text:column-name="City" text:database-name="Addresses">&lt;City&gt;<"#
    ));
    let read = read_odt(&bytes).unwrap();
    assert_eq!(fields(&read), merge);
    assert_eq!(
        read.text_settings().merge_source,
        Some(MergeSource::Contacts {
            group: Some("Clients".into())
        })
    );
    // A spreadsheet and a vCard file as the source.
    for source in [
        MergeSource::Spreadsheet {
            file: "/home/ana/Clients.csv".into(),
            table: None,
        },
        MergeSource::VCard {
            file: "/home/ana/Ana.vcf".into(),
        },
        MergeSource::Contacts { group: None },
    ] {
        let mut editor = Editor::new(document.clone());
        let mut settings = document.text_settings().clone();
        settings.merge_source = Some(source.clone());
        editor
            .edit("Source", |edit| edit.set_text_settings(settings))
            .unwrap();
        let read = read_odt(&write_odt(editor.document()).unwrap()).unwrap();
        assert_eq!(read.text_settings().merge_source, Some(source));
    }
}

#[test]
fn mal_01_an_address_on_one_line_and_a_field_without_data() {
    let address = Field::Merge {
        name: "Work Address".into(),
        sender: false,
        single_line: true,
        shown: Some("1 Infinite Loop\nCupertino CA 95014\n".into()),
        origin: None,
    };
    assert_eq!(address.text(1, 1), "1 Infinite Loop, Cupertino CA 95014");
    assert_eq!(Field::merge("Last Name").text(1, 1), "Last Name");
}

#[test]
fn mal_01_mal_07_the_merge_fields_of_pages_09_are_read() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/pages/mal-01-07-08-fields.pages");
    let document = read_pages_at(&path).unwrap();
    let names: Vec<_> = fields(&document)
        .into_iter()
        .filter_map(|f| match f {
            Field::Merge { name, sender, .. } => Some((name.to_string(), sender)),
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        [
            ("First Name".to_owned(), false),
            ("Last Name".to_owned(), false),
            ("First Name".to_owned(), true),
            ("Company".to_owned(), true),
        ]
    );
    // Saved again, they stay merge fields.
    let read = read_odt(&write_odt(&document).unwrap()).unwrap();
    assert_eq!(fields(&read), fields(&document));
}
