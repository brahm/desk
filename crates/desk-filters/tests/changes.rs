// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! REV-01: the insertions and deletions tracked in the body to and from
//! ODT, as LibreOffice writes them and as Desk does (W148).

use std::io::Write;

use desk_filters::{read_odt, read_odt_logged, write_odt};
use desk_model::{ChangeKind, Document};

/// Keeps a written file for ci/validate-odf.sh, which validates it against
/// the ODF schemas, when `DESK_ODF_OUT` names a directory.
fn keep(name: &str, bytes: &[u8]) {
    if let Some(dir) = std::env::var_os("DESK_ODF_OUT") {
        std::fs::write(std::path::Path::new(&dir).join(name), bytes).unwrap();
    }
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
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" office:version="1.3"><office:body><office:text>"#;
const TAIL: &str = "</office:text></office:body></office:document-content>";

/// Each paragraph of the body, its text with the changes in it: `[+…+]`
/// for text added, `[-…-]` for text deleted, and `¶+` or `¶-` at the end
/// for a break added or deleted; then each change, with its author and
/// date.
fn described(document: &Document) -> (Vec<String>, Vec<String>) {
    let changes = {
        let mut changes: Vec<String> = document
            .changes()
            .iter()
            .map(|c| format!("{:?} {} {:?}", c.kind, c.author, c.date))
            .collect();
        changes.sort();
        changes
    };
    (lines_of(document, document.body().iter()), changes)
}

/// The lines of some paragraphs, as [`described`] gives them.
fn lines_of<'a>(
    document: &Document,
    paragraphs: impl Iterator<Item = &'a desk_model::Paragraph>,
) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in paragraphs {
        let mut line = String::new();
        for run in paragraph.runs() {
            match run
                .attrs
                .change
                .flatten()
                .and_then(|id| document.changes().get(id))
            {
                Some(c) if c.kind == ChangeKind::Insertion => {
                    line += &format!("[+{}+]", run.text());
                }
                Some(_) => line += &format!("[-{}-]", run.text()),
                None => line += run.text(),
            }
        }
        match paragraph
            .attrs()
            .break_change
            .flatten()
            .and_then(|id| document.changes().get(id))
        {
            Some(c) if c.kind == ChangeKind::Insertion => line += "¶+",
            Some(_) => line += "¶-",
            None => {}
        }
        lines.push(line);
    }
    lines
}

/// Insertions in a paragraph and past its end, and deletions in a
/// paragraph and of a break, as LibreOffice writes them.
const LIBREOFFICE: &str = r#"<text:tracked-changes text:track-changes="false">
<text:changed-region xml:id="ct1" text:id="ct1"><text:insertion><office:change-info><dc:creator>Ana</dc:creator><dc:date>2026-11-24T10:00:00</dc:date></office:change-info></text:insertion></text:changed-region>
<text:changed-region xml:id="ct2" text:id="ct2"><text:deletion><office:change-info><dc:creator>Bo</dc:creator><dc:date>2026-11-24T11:00:00</dc:date><text:p>Why it went</text:p></office:change-info><text:p>old </text:p></text:deletion></text:changed-region>
<text:changed-region xml:id="ct3" text:id="ct3"><text:insertion><office:change-info><dc:creator>Ana</dc:creator><dc:date>2026-11-24T12:00:00</dc:date></office:change-info></text:insertion></text:changed-region>
<text:changed-region xml:id="ct4" text:id="ct4"><text:deletion><office:change-info><dc:creator>Bo</dc:creator><dc:date>2026-11-24T13:00:00</dc:date></office:change-info><text:p>end</text:p><text:p>Gone </text:p><text:p>start </text:p></text:deletion></text:changed-region>
</text:tracked-changes>
<text:p>One <text:change-start text:change-id="ct1"/>new<text:change-end text:change-id="ct1"/> and <text:change text:change-id="ct2"/>words.</text:p>
<text:p>Two <text:change-start text:change-id="ct3"/>added</text:p>
<text:p>line<text:change-end text:change-id="ct3"/> here, <text:change text:change-id="ct4"/>after.</text:p>"#;

#[test]
fn rev_01_insertions_and_deletions_of_libreoffice_are_read() {
    let bytes = odt(&format!("{HEAD}{LIBREOFFICE}{TAIL}"));
    let (document, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    let (lines, changes) = described(&document);
    assert_eq!(
        lines,
        [
            "One [+new+] and [-old -]words.",
            "Two [+added+]¶+",
            "[+line+] here, [-end-]¶-",
            "[-Gone -]¶-",
            "[-start -]after.",
        ]
    );
    assert_eq!(
        changes,
        [
            "Deletion Bo Some(\"2026-11-24T11:00:00\")",
            "Deletion Bo Some(\"2026-11-24T13:00:00\")",
            "Insertion Ana Some(\"2026-11-24T10:00:00\")",
            "Insertion Ana Some(\"2026-11-24T12:00:00\")",
        ]
    );
    // Written again, the changes are the same, in regions of their own.
    let written = write_odt(&document).unwrap();
    keep("rev-01-tracked-changes.odt", &written);
    let again = read_odt(&written).unwrap();
    assert_eq!(described(&again), (lines, changes));
}

#[test]
fn rev_01_changes_made_in_desk_are_kept_in_odt() {
    use desk_model::Position;
    let mut document = Document::new_word_processing();
    document
        .transact("Changes", |edit| {
            let start = edit.document().start();
            let end = edit.insert_text(start, "Keep this and that")?;
            let second = edit.split_paragraph(end)?;
            edit.insert_text(second, "Last line")?;
            let first = start.paragraph;
            let added = edit.add_change(ChangeKind::Insertion, "Ana", None)?;
            let gone = edit.add_change(ChangeKind::Deletion, "Bo", Some("2026-11-24T09:30:00"))?;
            edit.mark_change(Position::new(first, 5), Position::new(first, 9), added)?;
            // The end of the paragraph, its break and the start of the
            // next deleted at once.
            edit.mark_change(Position::new(first, 14), Position::new(first, 18), gone)?;
            edit.mark_break_change(first, Some(gone))?;
            edit.mark_change(second, Position::new(second.paragraph, 5), gone)?;
            Ok(())
        })
        .unwrap();
    let before = described(&document);
    assert_eq!(before.0, ["Keep [+this+] and [-that-]¶-", "[-Last -]line"]);
    let bytes = write_odt(&document).unwrap();
    keep("rev-01-changes-of-desk.odt", &bytes);
    // In the file, the deleted text is out of the body, which is one
    // paragraph, as other programs read it with the changes accepted.
    let content = desk_odf::read_odt(&bytes).unwrap();
    assert_eq!(content.body.len(), 1);
    assert_eq!(content.body[0].text(), "Keep this and line");
    assert_eq!(content.tracked_changes.len(), 2);
    let read = read_odt(&bytes).unwrap();
    assert_eq!(described(&read), before);
}

#[test]
fn rev_01_what_comes_after_a_deletion_stays_on_its_paragraph() {
    // A shape anchored to the paragraph after a deletion of three
    // paragraphs, and a comment on deleted text and the text after it.
    let content = format!(
        r#"{HEAD}<text:tracked-changes>
<text:changed-region text:id="d"><text:deletion><office:change-info><dc:creator>Bo</dc:creator><dc:date>2026-11-24T13:00:00</dc:date></office:change-info><text:p>a</text:p><text:p>b</text:p><text:p>c</text:p></text:deletion></text:changed-region>
</text:tracked-changes>
<text:p>One <text:change text:change-id="d"/>two</text:p>
<text:p>Three<draw:rect text:anchor-type="paragraph" svg:width="2cm" svg:height="1cm" svg:x="1cm" svg:y="0.5cm"/></text:p>{TAIL}"#
    );
    let document = read_odt(&odt(&content)).unwrap();
    let (lines, _) = described(&document);
    assert_eq!(lines, ["One [-a-]¶-", "[-b-]¶-", "[-c-]two", "Three"]);
    let object = document.objects().iter().next().unwrap();
    let desk_model::Anchor::Paragraph(at) = object.anchor else {
        panic!("{:?}", object.anchor);
    };
    assert_eq!(document.paragraph(at).unwrap().text(), "Three");
}

#[test]
fn rev_01_a_comment_on_deleted_text_is_kept() {
    use desk_model::{Comment, Position};
    let mut document = Document::new_word_processing();
    document
        .transact("Changes", |edit| {
            let start = edit.document().start();
            edit.insert_text(start, "keep gone after")?;
            let p = start.paragraph;
            let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
            edit.mark_change(Position::new(p, 5), Position::new(p, 10), gone)?;
            edit.add_comment(
                Position::new(p, 5),
                Position::new(p, 15),
                Comment {
                    id: desk_model::NodeId::new(1).unwrap(),
                    author: "Ana".into(),
                    date: None,
                    text: "Why?".into(),
                    replies: std::sync::Arc::new([]),
                },
            )?;
            Ok(())
        })
        .unwrap();
    let read = read_odt(&write_odt(&document).unwrap()).unwrap();
    assert_eq!(described(&read).0, ["keep [-gone -]after"]);
    let places = read.comment_places();
    assert_eq!(places.len(), 1);
    let p = places[0].start.paragraph;
    // On the text after the deleted words, which were taken out of the
    // body where the comment started.
    assert_eq!(places[0].end, Position::new(p, 15));
}

#[test]
fn rev_01_changes_of_cells_or_out_of_order_do_not_stop_a_file() {
    let content = format!(
        r#"{HEAD}<text:tracked-changes>
<text:changed-region text:id="n"><text:deletion><office:change-info><dc:creator>Carl</dc:creator><dc:date>2026-11-24T13:00:00</dc:date></office:change-info><text:p>x</text:p></text:deletion></text:changed-region>
<text:changed-region text:id="i"><text:insertion><office:change-info><dc:creator>Ana</dc:creator><dc:date>2026-11-24T13:00:00</dc:date></office:change-info></text:insertion></text:changed-region>
</text:tracked-changes>
<table:table table:name="T"><table:table-column/><table:table-row><table:table-cell><text:p>Cell <text:change text:change-id="n"/>text</text:p></table:table-cell></table:table-row></table:table>
<text:p>End<text:change-end text:change-id="i"/> first</text:p>
<text:p>then <text:change-start text:change-id="i"/>start</text:p>{TAIL}"#
    );
    let (document, log) = read_odt_logged(&odt(&content)).unwrap();
    // The deletion in the cell of a table is listed as kept as accepted,
    // as the comments of cells' text are, and the insertion whose end
    // comes first marks nothing.
    let found: Vec<_> = log.entries().iter().map(|e| e.count).collect();
    assert_eq!(found, [1], "{:?}", log.entries());
    let (lines, changes) = described(&document);
    assert_eq!(lines, ["\u{fffc}", "End first", "then start"]);
    assert_eq!(changes, ["Insertion Ana Some(\"2026-11-24T13:00:00\")"]);
}

#[test]
fn rev_01_changes_of_notes_and_text_boxes_are_kept_in_odt() {
    let content = format!(
        r#"{HEAD}<text:tracked-changes>
<text:changed-region text:id="n"><text:deletion><office:change-info><dc:creator>Bo</dc:creator><dc:date>2026-11-24T13:00:00</dc:date></office:change-info><text:p>old</text:p><text:p>gone </text:p></text:deletion></text:changed-region>
<text:changed-region text:id="b"><text:insertion><office:change-info><dc:creator>Ana</dc:creator><dc:date>2026-11-24T14:00:00</dc:date></office:change-info></text:insertion></text:changed-region>
</text:tracked-changes>
<text:p>Body<text:note text:id="ftn1" text:note-class="footnote"><text:note-citation>1</text:note-citation><text:note-body><text:p>Note <text:change text:change-id="n"/>text</text:p></text:note-body></text:note><draw:frame text:anchor-type="paragraph" svg:width="4cm" svg:height="2cm" svg:x="1cm" svg:y="1cm"><draw:text-box><text:p>Box <text:change-start text:change-id="b"/>new<text:change-end text:change-id="b"/> text</text:p></draw:text-box></draw:frame></text:p>{TAIL}"#
    );
    let (document, log) = read_odt_logged(&odt(&content)).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    let stories = |document: &Document| -> Vec<Vec<String>> {
        document
            .stories()
            .into_iter()
            .chain(
                document
                    .notes()
                    .iter()
                    .map(|n| desk_model::Story::Note(n.id)),
            )
            .map(|story| lines_of(document, document.story_paragraphs(story).iter()))
            .filter(|lines| lines.iter().any(|l| l.contains('[')))
            .collect()
    };
    let found = stories(&document);
    assert!(
        found.contains(&vec!["Box [+new+] text".to_owned()]),
        "{found:?}"
    );
    assert!(
        found.contains(&vec![
            "\u{fffc}Note [-old-]¶-".to_owned(),
            "[-gone -]text".to_owned()
        ]),
        "{found:?}"
    );
    assert_eq!(described(&document).1.len(), 2);
    // Written and read again, they are the same.
    let written = write_odt(&document).unwrap();
    keep("rev-01-changes-of-stories.odt", &written);
    let again = read_odt(&written).unwrap();
    let mut before = found;
    let mut after = stories(&again);
    before.sort();
    after.sort();
    assert_eq!(after, before);
    assert_eq!(described(&again).1, described(&document).1);
}

#[test]
fn rev_01_changes_of_headers_are_kept_in_odt() {
    use desk_model::{
        CharAttrs, MarginKind, MarginPages, MarginPlace, ParaAttrs, Paragraph, Run, Story,
    };
    let mut document = Document::new_word_processing();
    let place = MarginPlace::new(MarginKind::Header, MarginPages::Default);
    document
        .transact("Header", |edit| {
            let start = edit.document().start();
            edit.insert_text(start, "Body")?;
            let added = edit.add_change(ChangeKind::Insertion, "Ana", None)?;
            let style = edit.document().styles().default_paragraph();
            let changed = CharAttrs {
                change: Some(Some(added)),
                ..CharAttrs::default()
            };
            let header = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs::default(),
                vec![Run::new("Header "), Run::with_attrs("added", None, changed)],
            )?;
            edit.set_header_footer(place, vec![header])
        })
        .unwrap();
    let story = Story::Margin {
        section: None,
        place,
    };
    let lines = |d: &Document| lines_of(d, d.story_paragraphs(story).iter());
    assert_eq!(lines(&document), ["Header [+added+]"]);
    let written = write_odt(&document).unwrap();
    keep("rev-01-changes-of-headers.odt", &written);
    let (again, log) = read_odt_logged(&written).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(lines(&again), ["Header [+added+]"]);
}

#[test]
fn rev_01_a_list_item_whose_break_before_was_deleted_is_kept() {
    use desk_model::Position;
    let content = format!(
        r#"{HEAD}<text:p>Intro</text:p><text:list text:style-name="L1"><text:list-item><text:p>Item</text:p></text:list-item></text:list>{TAIL}"#
    )
    .replace(
        "<office:body>",
        r#"<office:automatic-styles><text:list-style style:name="L1" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"><text:list-level-style-bullet text:level="1" text:bullet-char="•"/></text:list-style></office:automatic-styles><office:body>"#,
    );
    let mut document = read_odt(&odt(&content)).unwrap();
    let list = |document: &Document, index: usize| {
        let paragraph = document.body().iter().nth(index).unwrap();
        let styles = document.styles();
        let list = styles
            .resolve_paragraph(paragraph.style(), paragraph.attrs())
            .list_style?;
        Some(styles.list(list)?.levels.clone())
    };
    let item = list(&document, 1);
    assert!(item.is_some());
    document
        .transact("Delete", |edit| {
            let first = edit.document().start().paragraph;
            let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
            edit.mark_change(Position::new(first, 4), Position::new(first, 5), gone)?;
            edit.mark_break_change(first, Some(gone))?;
            Ok(())
        })
        .unwrap();
    let before = described(&document);
    assert_eq!(before.0, ["Intr[-o-]¶-", "Item"]);
    let bytes = write_odt(&document).unwrap();
    keep("rev-01-list-item-joined.odt", &bytes);
    let content = desk_odf::read_odt(&bytes).unwrap();
    assert_eq!(content.body.len(), 1);
    assert!(content.tracked_changes[0].deleted[1].list.is_some());
    let read = read_odt(&bytes).unwrap();
    assert_eq!(described(&read), before);
    assert_eq!(list(&read, 0), None);
    assert_eq!(list(&read, 1), item);
}

#[test]
fn rev_01_a_header_that_a_deletion_emptied_keeps_it_and_its_formatting() {
    use desk_model::{
        CharAttrs, Length, MarginKind, MarginPages, MarginPlace, ParaAttrs, Paragraph, Run, Story,
    };
    let mut document = Document::new_word_processing();
    let place = MarginPlace::new(MarginKind::Header, MarginPages::Default);
    let big = Some(Length::from_points(20));
    document
        .transact("Header", |edit| {
            let start = edit.document().start();
            edit.insert_text(start, "Body")?;
            let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
            let style = edit.document().styles().default_paragraph();
            let changed = CharAttrs {
                change: Some(Some(gone)),
                font_size: big,
                ..CharAttrs::default()
            };
            let header = Paragraph::from_runs(
                edit.new_id(),
                style,
                ParaAttrs::default(),
                vec![Run::with_attrs("gone", None, changed)],
            )?;
            edit.set_header_footer(place, vec![header])
        })
        .unwrap();
    let story = Story::Margin {
        section: None,
        place,
    };
    let lines = |d: &Document| lines_of(d, d.story_paragraphs(story).iter());
    assert_eq!(lines(&document), ["[-gone-]"]);
    let written = write_odt(&document).unwrap();
    keep("rev-01-header-emptied.odt", &written);
    // The style of the deleted text is in content.xml, with the region.
    let content = desk_odf::read_odt(&written).unwrap();
    let span = &content.tracked_changes[0].deleted[0].spans[0];
    let name = span.style.as_deref().unwrap();
    assert!(content.automatic_styles.iter().any(|s| s.name == name));
    let (again, log) = read_odt_logged(&written).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(lines(&again), ["[-gone-]"]);
    let run = again.story_paragraphs(story)[0]
        .runs()
        .next()
        .unwrap()
        .clone();
    assert_eq!(run.attrs.font_size, big);
}

#[test]
fn rev_01_a_note_in_deleted_text_keeps_its_own_changes() {
    let content = format!(
        r#"{HEAD}<text:tracked-changes>
<text:changed-region text:id="d"><text:deletion><office:change-info><dc:creator>Bo</dc:creator><dc:date>2026-11-24T13:00:00</dc:date></office:change-info><text:p>cut<text:note text:id="ftn1" text:note-class="footnote"><text:note-citation>1</text:note-citation><text:note-body><text:p>Note <text:change-start text:change-id="i"/>new<text:change-end text:change-id="i"/></text:p></text:note-body></text:note></text:p></text:deletion></text:changed-region>
<text:changed-region text:id="i"><text:insertion><office:change-info><dc:creator>Ana</dc:creator><dc:date>2026-11-24T14:00:00</dc:date></office:change-info></text:insertion></text:changed-region>
</text:tracked-changes>
<text:p>Body <text:change text:change-id="d"/>end</text:p>{TAIL}"#
    );
    let (document, log) = read_odt_logged(&odt(&content)).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    let note = |document: &Document| {
        let note = document.notes().iter().next().unwrap().id;
        lines_of(
            document,
            document
                .story_paragraphs(desk_model::Story::Note(note))
                .iter(),
        )
    };
    assert_eq!(note(&document), ["\u{fffc}Note [+new+]"]);
    assert_eq!(described(&document).1.len(), 2);
    let written = write_odt(&document).unwrap();
    keep("rev-01-note-in-deleted-text.odt", &written);
    // Every mark in the file names a region of it.
    let content = desk_odf::read_odt(&written).unwrap();
    assert_eq!(content.tracked_changes.len(), 2);
    let (again, log) = read_odt_logged(&written).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(note(&again), note(&document));
    assert_eq!(described(&again), described(&document));
}
