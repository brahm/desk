// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! W101: links suggest as bookmarks the paragraphs of chosen styles.

use desk_model::{Document, Editor, Position};

#[test]
fn w101_links_suggest_the_paragraphs_of_the_styles_chosen() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let (two, three, body) = editor
        .edit("Setup", |edit| {
            let mut ids = vec![start.paragraph];
            let mut at = edit.insert_text(start, "Chapter")?;
            for text in ["Part", "Text", "Detail", ""] {
                at = edit.split_paragraph(at)?;
                ids.push(at.paragraph);
                at = edit.insert_text(at, text)?;
            }
            let style = |edit: &mut desk_model::Edit, name, id| {
                let at = Position::new(id, 0);
                let style = edit.new_paragraph_style(name, at, at, false)?;
                edit.set_paragraph_style(at, at, style)?;
                Ok::<_, desk_model::ModelError>(style)
            };
            let two = style(edit, "Heading 2", ids[1])?;
            let three = style(edit, "Heading 3", ids[3])?;
            let at = Position::new(ids[4], 0);
            edit.set_paragraph_style(at, at, three)?;
            let body = edit.document().styles().default_paragraph();
            Ok((two, three, body))
        })
        .unwrap();
    // A new document suggests Heading 2 and Heading 3, with text.
    let document = editor.document();
    assert_eq!(document.bookmark_suggestion_styles(), [two, three]);
    assert_eq!(
        document.suggested_bookmarks(),
        [(1, "Part".to_owned()), (3, "Detail".to_owned())]
    );

    // Chosen styles replace them.
    editor
        .edit("Choose", |edit| {
            let mut settings = edit.document().text_settings().clone();
            settings.bookmark_suggestions = Some(vec![body, three]);
            edit.set_text_settings(settings)
        })
        .unwrap();
    let suggested: Vec<usize> = editor
        .document()
        .suggested_bookmarks()
        .into_iter()
        .map(|(i, _)| i)
        .collect();
    assert_eq!(suggested, [0, 2, 3]);

    // A deleted style is no longer suggested.
    editor
        .edit("Delete", |edit| edit.delete_style(three, None))
        .unwrap();
    assert_eq!(
        editor.document().text_settings().bookmark_suggestions,
        Some(vec![body])
    );
}
