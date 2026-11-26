// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! EST-22, EST-23: a table of contents lists the paragraphs of the styles
//! the document chooses, with the number of their page, and is made again
//! when it is updated.

use std::sync::Arc;

use desk_model::{
    Document, Editor, Field, Layout, ParaAttrs, Position, Section, Start, TocEntry, TocLevel,
    TocScope, TocSettings, tables_of_contents,
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
        .edit("Table", |edit| {
            edit.insert_table_of_contents(0, TocScope::Document)
        })
        .unwrap();
    let document = editor.document();
    let body: Vec<_> = document.body().iter().collect();
    assert_eq!(tables_of_contents(&body), vec![0..2]);
    for (paragraph, target) in body[..2].iter().zip([ids[0], ids[2]]) {
        assert_eq!(paragraph.style(), entry);
        assert_eq!(
            paragraph.attrs().toc_entry,
            Some(Some(TocEntry {
                target: Some(target),
                scope: TocScope::Document,
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
        Some(Some(TocEntry {
            target: None,
            scope: TocScope::Document,
        }))
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

#[test]
fn sty_05_a_style_that_a_table_lists_can_be_deleted_and_the_table_updates() {
    // On the reference machine, Pages 4.3 deletes a style that the table
    // of contents lists: the table stops listing it and updates at once.
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let heading = editor
        .edit("Setup", |edit| {
            let at = edit.insert_text(start, "Chapter one")?;
            let at = edit.split_paragraph(at)?;
            edit.insert_text(at, "Some text")?;
            let heading = edit.new_paragraph_style("MyHead", start, start, true)?;
            let mut settings = edit.document().text_settings().clone();
            settings.toc = TocSettings {
                levels: vec![TocLevel {
                    style: heading,
                    entry: edit.document().styles().default_paragraph(),
                    page_number: true,
                }],
                page_links: false,
            };
            edit.set_text_settings(settings)?;
            edit.insert_table_of_contents(0, TocScope::Document)?;
            Ok(heading)
        })
        .unwrap();
    assert!(
        texts(editor.document())
            .iter()
            .any(|t| t.starts_with("Chapter one\t"))
    );
    editor
        .edit("Delete Style", |edit| edit.delete_style(heading, None))
        .unwrap();
    let document = editor.document();
    assert!(document.styles().get(heading).is_none());
    assert!(!document.text_settings().toc.uses(heading));
    assert!(
        !texts(document)
            .iter()
            .any(|t| t.starts_with("Chapter one\t"))
    );
}

#[test]
fn w100_a_table_lists_the_document_its_section_or_up_to_the_next_table() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Setup", |edit| {
            let mut ids = vec![start.paragraph];
            let mut at = edit.insert_text(start, "One")?;
            for text in ["Two", "Three"] {
                at = edit.split_paragraph(at)?;
                ids.push(at.paragraph);
                at = edit.insert_text(at, text)?;
            }
            let heading = edit.new_paragraph_style("Heading", start, start, false)?;
            for &id in &ids {
                edit.set_paragraph_style(Position::new(id, 0), Position::new(id, 0), heading)?;
            }
            // "Two" starts a second section.
            let two = Position::new(ids[1], 0);
            edit.set_paragraph_attrs(
                two,
                two,
                &ParaAttrs {
                    start: Some(Some(Arc::new(Start::Section(
                        Section::default(),
                        Layout::default(),
                    )))),
                    ..ParaAttrs::default()
                },
            )?;
            let mut settings = edit.document().text_settings().clone();
            settings.toc = TocSettings {
                levels: vec![TocLevel {
                    style: heading,
                    entry: edit.document().styles().default_paragraph(),
                    page_number: false,
                }],
                page_links: false,
            };
            edit.set_text_settings(settings)?;
            // A table in the second section, before "Three".
            edit.insert_table_of_contents(2, TocScope::Document)
        })
        .unwrap();
    let listed = |document: &Document| -> (Vec<String>, Vec<TocScope>) {
        let body: Vec<_> = document.body().iter().collect();
        let range = tables_of_contents(&body)[0].clone();
        body[range]
            .iter()
            .map(|p| {
                let text = p.runs().map(|r| r.text().to_owned()).collect();
                (text, p.attrs().toc_entry.flatten().unwrap().scope)
            })
            .unzip()
    };
    // The whole document, before the table too.
    let (texts, scopes) = listed(editor.document());
    assert_eq!(texts, ["One", "Two", "Three"]);
    assert!(scopes.iter().all(|&s| s == TocScope::Document));

    // Its section; any entry of the table changes it.
    editor
        .edit("Range", |edit| edit.set_toc_scope(3, TocScope::Section))
        .unwrap();
    let (texts, scopes) = listed(editor.document());
    assert_eq!(texts, ["Two", "Three"]);
    assert!(scopes.iter().all(|&s| s == TocScope::Section));

    // Up to the next table: only what follows it.
    editor
        .edit("Range", |edit| edit.set_toc_scope(2, TocScope::NextTable))
        .unwrap();
    let (texts, scopes) = listed(editor.document());
    assert_eq!(texts, ["Three"]);
    assert_eq!(scopes, [TocScope::NextTable]);

    // Updating keeps the range, and undo brings the section back.
    editor
        .edit("Update", |edit| edit.update_tables_of_contents())
        .unwrap();
    assert_eq!(listed(editor.document()).0, ["Three"]);
    editor.undo().unwrap();
    assert_eq!(listed(editor.document()).0, ["Two", "Three"]);
}

#[test]
fn w100_a_table_at_the_start_of_a_section_is_in_it_and_never_joins_another() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let two = editor
        .edit("Setup", |edit| {
            let at = edit.insert_text(start, "One")?;
            let two = edit.split_paragraph(at)?;
            edit.insert_text(two, "Two")?;
            let heading = edit.new_paragraph_style("Heading", start, start, false)?;
            for id in [start.paragraph, two.paragraph] {
                edit.set_paragraph_style(Position::new(id, 0), Position::new(id, 0), heading)?;
            }
            edit.set_paragraph_attrs(
                two,
                two,
                &ParaAttrs {
                    start: Some(Some(Arc::new(Start::Section(
                        Section::default(),
                        Layout::default(),
                    )))),
                    ..ParaAttrs::default()
                },
            )?;
            let mut settings = edit.document().text_settings().clone();
            settings.toc = TocSettings {
                levels: vec![TocLevel {
                    style: heading,
                    entry: edit.document().styles().default_paragraph(),
                    page_number: false,
                }],
                page_links: false,
            };
            edit.set_text_settings(settings)?;
            edit.insert_table_of_contents(1, TocScope::Section)?;
            Ok(two.paragraph)
        })
        .unwrap();
    let document = editor.document();
    // The section starts at the table, which lists "Two" only.
    let body: Vec<_> = document.body().iter().collect();
    assert_eq!(tables_of_contents(&body), vec![1..2]);
    assert!(body[1].attrs().start.is_some());
    assert!(document.paragraph(two).unwrap().attrs().start.is_none());
    assert_eq!(body[1].runs().map(|r| r.text()).collect::<String>(), "Two");
    assert_eq!(document.section_ranges(), vec![0..1, 1..3]);

    // A table in another, or right after one, would join it.
    for index in [1, 2] {
        assert!(
            editor
                .edit("Again", |edit| edit
                    .insert_table_of_contents(index, TocScope::Document))
                .is_err()
        );
    }
}
