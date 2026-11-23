// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! REV-08 and PXV-19: the comments of the body to and from ODT, with their
//! replies, as Desk writes them and as LibreOffice does.

use std::io::Write;

use desk_filters::{read_odt, read_odt_logged, write_odt};
use desk_model::{Comment, Document, Editor, NodeId, Position, Reply};

/// Keeps a written file for ci/validate-odf.sh, which validates it against
/// the ODF schemas, when `DESK_ODF_OUT` names a directory.
fn keep(name: &str, bytes: &[u8]) {
    if let Some(dir) = std::env::var_os("DESK_ODF_OUT") {
        std::fs::write(std::path::Path::new(&dir).join(name), bytes).unwrap();
    }
}

fn comment(author: &str, text: &str, replies: &[(&str, &str)]) -> Comment {
    Comment {
        id: NodeId::new(1).unwrap(),
        author: author.into(),
        date: Some("2026-11-22T20:33:34".into()),
        text: text.into(),
        replies: replies
            .iter()
            .map(|(author, text)| Reply {
                author: (*author).into(),
                date: Some("2026-11-22T20:41:00".into()),
                text: (*text).into(),
            })
            .collect::<Vec<_>>()
            .into(),
    }
}

/// The comments of a document as a reader sees them: the text each is on,
/// its author, text and replies, in the order of the body.
fn described(document: &Document) -> Vec<String> {
    document
        .comment_ranges()
        .into_iter()
        .map(|(id, start, end)| {
            let comment = document.comments().get(id).unwrap();
            let on: String = document
                .paragraph_ranges(start, end)
                .unwrap()
                .into_iter()
                .map(|(p, range)| document.paragraph(p).unwrap().text()[range].to_owned())
                .collect::<Vec<_>>()
                .join("¶");
            let replies: Vec<String> = comment
                .replies
                .iter()
                .map(|r| format!("{}: {} ({:?})", r.author, r.text, r.date))
                .collect();
            format!(
                "[{on}] {}: {:?} ({:?}) {replies:?}",
                comment.author, comment.text, comment.date
            )
        })
        .collect()
}

fn odt(content: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("mimetype", stored).unwrap();
        zip.write_all(b"application/vnd.oasis.opendocument.text")
            .unwrap();
        zip.start_file("content.xml", stored).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    bytes
}

const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:loext="urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0" office:version="1.3"><office:body><office:text>"#;
const TAIL: &str = "</office:text></office:body></office:document-content>";

#[test]
fn rev_08_comments_with_replies_are_kept_in_odt() {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Typing", |edit| {
            let end = edit.insert_text(start, "First line of text")?;
            let next = edit.split_paragraph(end)?;
            edit.insert_text(next, "Second line, a highlight here")
        })
        .unwrap();
    let body = editor.document().body();
    let (p1, p2) = (body.get(0).unwrap().id(), body.get(1).unwrap().id());
    editor
        .edit("Comments", |edit| {
            // Across the two paragraphs, with two replies.
            edit.add_comment(
                Position::new(p1, 6),
                Position::new(p2, 6),
                comment(
                    "Ana",
                    "Two lines\nof comment",
                    &[("Bo", "Yes"), ("Ana", "Done")],
                ),
            )?;
            // Inside the first, overlapping it.
            edit.add_comment(
                Position::new(p1, 0),
                Position::new(p1, 10),
                comment("Bo", "Start", &[]),
            )?;
            // A highlight, a comment without text (PXV-19).
            edit.add_comment(
                Position::new(p2, 15),
                Position::new(p2, 24),
                comment("Ana", "", &[]),
            )?;
            Ok(())
        })
        .unwrap();
    let document = editor.document();
    let bytes = write_odt(document).unwrap();
    keep("rev-08-comments.odt", &bytes);
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(described(&read), described(document));
    assert_eq!(described(&read).len(), 3);
    assert!(
        read.comments()
            .iter()
            .any(|c| c.is_highlight() && c.author.as_ref() == "Ana")
    );
    // A reply names its comment; the text is the same with or without
    // the comments.
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut content = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("content.xml").unwrap(), &mut content)
        .unwrap();
    assert!(content.contains("desk:parent-name=\"c2\""), "{content}");
    assert!(content.contains("<office:annotation-end office:name=\"c2r2\"/>"));
    assert_eq!(
        read.body().iter().map(|p| p.text()).collect::<Vec<_>>(),
        ["First line of text", "Second line, a highlight here"]
    );
}

#[test]
fn rev_08_replies_of_libreoffice_join_their_comment() {
    // As LibreOffice 24.2 writes a comment with a reply: the reply first,
    // naming its comment in loext:parent-name, and an empty date.
    let content = format!(
        r#"{HEAD}<text:p>Hello <office:annotation office:name="__Annotation__1" loext:parent-name="__Annotation__0" loext:resolved="false"><dc:creator>Bo</dc:creator><dc:date>0000-00-00T00:00:00</dc:date><text:p>A reply</text:p></office:annotation><office:annotation office:name="__Annotation__0" loext:resolved="false"><dc:creator>Ana</dc:creator><dc:date>2026-11-22T09:14:00</dc:date><text:p>First comment</text:p></office:annotation>commented<office:annotation-end office:name="__Annotation__1"/><office:annotation-end office:name="__Annotation__0"/> world here.</text:p>{TAIL}"#
    );
    let (document, log) = read_odt_logged(&odt(&content)).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(
        described(&document),
        [
            r#"[commented] Ana: "First comment" (Some("2026-11-22T09:14:00")) ["Bo: A reply (None)"]"#
        ]
    );
    assert_eq!(
        document.body().get(0).unwrap().text(),
        "Hello commented world here."
    );
}

#[test]
fn rev_08_a_comment_at_a_point_is_on_the_word_there() {
    let content = format!(
        r#"{HEAD}<text:p>One two<office:annotation><dc:creator>Ana</dc:creator><text:p>At the end of two</text:p></office:annotation> three</text:p><text:p><office:annotation office:name="x"><text:p>Empty range</text:p></office:annotation><office:annotation-end office:name="x"/>four five</text:p><text:p>six <office:annotation office:name="y"><text:p>Never ends</text:p></office:annotation> seven</text:p>{TAIL}"#
    );
    let document = read_odt(&odt(&content)).unwrap();
    let on: Vec<String> = described(&document)
        .into_iter()
        .map(|d| d.split(']').next().unwrap().to_owned())
        .collect();
    assert_eq!(on, ["[two", "[four", "[seven"]);
    // No author reads as none, not as a name.
    assert!(document.comments().iter().any(|c| c.author.is_empty()));
}

#[test]
fn rev_08_comments_of_notes_headers_and_text_boxes_are_kept_in_odt() {
    use std::sync::Arc;

    use desk_model::{
        CharAttrs, FIELD_CHARACTER, Field, Frame, Length, MarginKind, MarginPages, MarginPlace,
        Note, Object, ObjectKind, ParaAttrs, Paragraph, Run, Story, TextBox,
    };
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let place = MarginPlace::new(MarginKind::Header, MarginPages::Default);
    let (note, header, boxed) = editor
        .edit("Texts", |edit| {
            let style = edit.document().styles().default_paragraph();
            let paragraph = |edit: &mut desk_model::Edit, runs: Vec<Run>| {
                Paragraph::from_runs(edit.new_id(), style, ParaAttrs::default(), runs)
            };
            // A footnote, its mark in the body.
            let at = edit.insert_text(start, "Body")?;
            let note = edit.new_id();
            let after = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
            let mark = CharAttrs {
                field: Some(Some(Field::NoteMark { note })),
                ..CharAttrs::default()
            };
            edit.set_character_attrs(at, after, &mark)?;
            let text = paragraph(
                edit,
                vec![
                    Run::with_attrs(&FIELD_CHARACTER.to_string(), None, mark),
                    Run::new(" Note text"),
                ],
            )?;
            edit.set_note(
                note,
                Some(Note {
                    id: note,
                    mark: None,
                    paragraphs: vec![text].into(),
                }),
            )?;
            // A header.
            let header = paragraph(edit, vec![Run::new("Header text")])?;
            edit.set_header_footer(place, vec![header])?;
            // A text box.
            let text = paragraph(edit, vec![Run::new("Boxed text")])?;
            let object = Object::new(
                edit.new_id(),
                ObjectKind::TextBox(TextBox::new(Arc::from([text]), Length::from_points(4))),
                Frame::from_pt(300.0, 400.0, 120.0, 60.0),
            );
            let boxed = object.id;
            edit.add_object(object)?;
            Ok((
                note,
                Story::Margin {
                    section: None,
                    place,
                },
                boxed,
            ))
        })
        .unwrap();
    // A comment in each, the one of the note with a reply.
    for (story, from, to, made) in [
        (
            Story::Note(note),
            4,
            8,
            comment("Ana", "On the note", &[("Bo", "Yes")]),
        ),
        (header, 0, 6, comment("Bo", "On the header", &[])),
        (Story::TextBox(boxed), 6, 10, comment("Ana", "", &[])),
    ] {
        editor
            .edit("Comment", |edit| {
                edit.edit_story(story, |edit| {
                    let p = edit.document().body().get(0).unwrap().id();
                    edit.add_comment(Position::new(p, from), Position::new(p, to), made)
                })
            })
            .unwrap();
    }
    let places = |document: &Document| -> Vec<String> {
        document
            .comment_places()
            .into_iter()
            .map(|place| {
                let story = place.story.unwrap();
                let paragraph = document
                    .story_paragraphs(story)
                    .into_iter()
                    .find(|p| p.id() == place.start.paragraph)
                    .unwrap();
                let comment = document.comments().get(place.id).unwrap();
                let kind = match story {
                    Story::Note(_) => "note",
                    Story::Margin { .. } => "header",
                    Story::TextBox(_) => "box",
                };
                format!(
                    "{kind} [{}] {}: {:?} {}",
                    &paragraph.text()[place.start.offset..place.end.offset],
                    comment.author,
                    comment.text,
                    comment.replies.len()
                )
            })
            .collect()
    };
    let document = editor.document();
    assert_eq!(
        places(document),
        [
            r#"header [Header] Bo: "On the header" 0"#,
            r#"note [Note] Ana: "On the note" 1"#,
            r#"box [text] Ana: "" 0"#,
        ]
    );
    let bytes = write_odt(document).unwrap();
    keep("rev-08-comments-in-stories.odt", &bytes);
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(places(&read), places(document));
    // The texts are the same, without the comments.
    assert_eq!(
        read.notes().iter().next().unwrap().paragraphs[0].text(),
        format!("{FIELD_CHARACTER} Note text")
    );
    assert_eq!(read.headers_footers().get(place)[0].text(), "Header text");
}

#[test]
fn rev_08_a_comment_on_an_object_is_kept_in_odt() {
    use desk_model::{Frame, Object, ObjectKind, Shape};
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let (shape, id) = editor
        .edit("Shape", |edit| {
            edit.insert_text(start, "Body")?;
            let object = Object::new(
                edit.new_id(),
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(72.0, 72.0, 100.0, 50.0),
            );
            let shape = object.id;
            edit.add_object(object)?;
            let id = edit
                .add_object_comment(shape, comment("Ana", "On the shape", &[("Bo", "Agreed")]))?;
            Ok((shape, id))
        })
        .unwrap();
    let document = editor.document();
    assert_eq!(document.object_comments(), [(id, shape)]);
    let bytes = write_odt(document).unwrap();
    keep("rev-08-comment-on-an-object.odt", &bytes);
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    let found = read.object_comments();
    assert_eq!(found.len(), 1);
    let comment = read.comments().get(found[0].0).unwrap();
    assert_eq!(
        (comment.author.as_ref(), comment.text.as_ref()),
        ("Ana", "On the shape")
    );
    assert_eq!(comment.replies.len(), 1);
    assert_eq!(comment.replies[0].text.as_ref(), "Agreed");
    assert!(matches!(
        read.objects().get(found[0].1).unwrap().kind,
        ObjectKind::Shape(Shape::Rectangle)
    ));
    // In the extension, which a reader without it leaves out.
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut content = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("content.xml").unwrap(), &mut content)
        .unwrap();
    assert!(content.contains("<desk:annotation desk:name="), "{content}");
    assert!(!content.contains("<office:annotation"));
}
