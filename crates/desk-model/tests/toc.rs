// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! EST-22, EST-23: a table of contents lists the paragraphs of the styles
//! the document chooses, with the number of their page, and is made again
//! when it is updated.

use desk_model::{
    Document, Editor, Field, Position, TocEntry, TocLevel, TocSettings, tables_of_contents,
};

fn texts(document: &Document) -> Vec<String> {
    document
        .body()
        .iter()
        .map(|p| p.runs().map(|r| r.text().to_owned()).collect())
        .collect()
}

#[test]
fn est_22_est_23_a_table_lists_its_styles_and_updates() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let (heading, entry, ids) = editor
        .edit("Setup", |edit| {
            let mut ids = vec![start.paragraph];
            let mut at = edit.insert_text(start, "Chapter one")?;
            for text in ["Some text", "Chapter two"] {
                at = edit.split_paragraph(at)?;
                ids.push(at.paragraph);
                at = edit.insert_text(at, text)?;
            }
            let heading = edit.new_paragraph_style("Heading", start, start, false)?;
            let entry = edit.new_paragraph_style("TOC Heading", start, start, false)?;
            for &id in [ids[0], ids[2]].iter() {
                edit.set_paragraph_style(Position::new(id, 0), Position::new(id, 0), heading)?;
            }
            let mut settings = edit.document().text_settings().clone();
            settings.toc = TocSettings {
                levels: vec![TocLevel {
                    style: heading,
                    entry,
                    page_number: true,
                }],
                page_links: false,
            };
            edit.set_text_settings(settings)?;
            Ok((heading, entry, ids))
        })
        .unwrap();
    assert!(editor.document().text_settings().toc.uses(heading));

    editor
        .edit("Table", |edit| edit.insert_table_of_contents(0))
        .unwrap();
    let document = editor.document();
    let body: Vec<_> = document.body().iter().collect();
    assert_eq!(tables_of_contents(&body), vec![0..2]);
    for (paragraph, target) in body[..2].iter().zip([ids[0], ids[2]]) {
        assert_eq!(paragraph.style(), entry);
        assert_eq!(
            paragraph.attrs().toc_entry,
            Some(Some(TocEntry {
                target: Some(target)
            }))
        );
        let field = paragraph
            .runs()
            .find_map(|r| r.attrs.field.clone().flatten());
        assert_eq!(field, Some(Field::PageOf { paragraph: target }));
    }
    assert_eq!(
        texts(document)[..2],
        ["Chapter one\t\u{fffc}", "Chapter two\t\u{fffc}"]
    );

    // The entries of a level go together, and only updating changes
    // their text; the whole table can be taken away.
    assert_eq!(document.entries_of_level(1), [0, 1]);
    assert!(document.entries_of_level(2).is_empty());
    let end_of = |i: usize| document.body().get(i).unwrap().len();
    assert!(document.toc_protects((0, 2), (0, 2)));
    assert!(document.toc_protects((1, end_of(1)), (2, 0)));
    assert!(!document.toc_protects((2, 0), (2, 3)));
    assert!(!document.toc_protects((0, 0), (1, end_of(1))));

    // A changed heading is listed again once the table is updated.
    editor
        .edit("Rename", |edit| {
            edit.insert_text(Position::new(ids[2], "Chapter two".len()), "!")
                .map(|_| ())
        })
        .unwrap();
    editor
        .edit("Update", |edit| edit.update_tables_of_contents())
        .unwrap();
    assert_eq!(texts(editor.document())[1], "Chapter two!\t\u{fffc}");
    // Updating again changes nothing.
    let before: Vec<_> = editor.document().body().iter().map(|p| p.id()).collect();
    editor
        .edit("Update", |edit| edit.update_tables_of_contents())
        .unwrap();
    let after: Vec<_> = editor.document().body().iter().map(|p| p.id()).collect();
    assert_eq!(before, after);
    assert_eq!(texts(editor.document())[1], "Chapter two!\t\u{fffc}");

    // A table with nothing to list keeps one empty entry.
    editor
        .edit("Off", |edit| {
            let mut settings = edit.document().text_settings().clone();
            settings.toc.levels.clear();
            edit.set_text_settings(settings)?;
            edit.update_tables_of_contents()
        })
        .unwrap();
    let document = editor.document();
    let body: Vec<_> = document.body().iter().collect();
    assert_eq!(tables_of_contents(&body), vec![0..1]);
    assert_eq!(
        body[0].attrs().toc_entry,
        Some(Some(TocEntry { target: None }))
    );
    assert_eq!(texts(document)[1], "Chapter one");

    // Undo brings the entries back.
    editor.undo().unwrap();
    assert_eq!(texts(editor.document())[1], "Chapter two!\t\u{fffc}");
}

#[test]
fn est_23_entries_get_styles_with_a_tab_for_the_leader() {
    use desk_model::{StyleFamily, TabAlignment};
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Setup", |edit| {
            let heading = edit.new_paragraph_style("Heading", start, start, true)?;
            let default = edit.document().styles().default_paragraph();
            let mut settings = edit.document().text_settings().clone();
            settings.toc = TocSettings {
                levels: vec![TocLevel {
                    style: heading,
                    entry: default,
                    page_number: true,
                }],
                page_links: true,
            };
            edit.set_text_settings(settings)?;
            edit.add_toc_entry_styles()
        })
        .unwrap();
    let document = editor.document();
    let styles = document.styles();
    let entry = styles
        .by_name(StyleFamily::Paragraph, "TOC Heading")
        .expect("made");
    assert_eq!(document.text_settings().toc.levels[0].entry, entry.id());
    let format = styles.resolve_paragraph(entry.id(), &Default::default());
    let stop = format.tab_stops.stops()[0];
    assert_eq!(stop.alignment, TabAlignment::Right);
    let page = document.page_setup();
    assert_eq!(
        stop.position,
        page.width - page.margin_left - page.margin_right
    );
}
