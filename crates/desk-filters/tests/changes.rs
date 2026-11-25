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
/// for text added, `[-…-]` for text deleted, `[~…~]` for text formatted, and `¶+` or `¶-` at the end
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
                Some(c) if c.kind == ChangeKind::Format => {
                    line += &format!("[~{}~]", run.text());
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
    // The file tracks no changes, and nor does the document.
    assert_eq!(document.text_settings().tracking, desk_model::Tracking::Off);
}

#[test]
fn rev_03_whether_changes_are_tracked_is_kept_in_odt() {
    use desk_model::Tracking;
    for tracking in [Tracking::On, Tracking::Paused, Tracking::Off] {
        let mut document = Document::new_word_processing();
        let settings = desk_model::TextSettings {
            tracking,
            ..document.text_settings().clone()
        };
        document
            .transact("Track Changes", |edit| edit.set_text_settings(settings))
            .unwrap();
        let bytes = write_odt(&document).unwrap();
        keep(&format!("rev-03-tracking-{tracking:?}.odt"), &bytes);
        // Paused, other programs see that changes are not tracked.
        let content = desk_odf::read_odt(&bytes).unwrap();
        assert_eq!(content.track_changes, tracking == Tracking::On);
        let read = read_odt(&bytes).unwrap();
        assert_eq!(read.text_settings().tracking, tracking);
    }
    // A file of LibreOffice that tracks changes.
    let on = LIBREOFFICE.replace(r#"track-changes="false""#, r#"track-changes="true""#);
    let read = read_odt(&odt(&format!("{HEAD}{on}{TAIL}"))).unwrap();
    assert_eq!(read.text_settings().tracking, Tracking::On);
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
    // as the comments of cells' text are, and so is the insertion whose
    // end comes first, which marks nothing.
    let found: Vec<_> = log.entries().iter().map(|e| e.count).collect();
    assert_eq!(found, [2], "{:?}", log.entries());
    let (lines, changes) = described(&document);
    assert_eq!(lines, ["\u{fffc}", "End first", "then start"]);
    assert!(changes.is_empty(), "{changes:?}");
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

#[test]
fn rev_01_changes_of_formatting_keep_the_formatting_before() {
    use desk_model::{CharAttrs, Position};
    let mut document = Document::new_word_processing();
    let first = document.body().iter().next().unwrap().id();
    let italic = CharAttrs {
        italic: Some(true),
        ..CharAttrs::default()
    };
    let underlined = CharAttrs {
        underline: Some(desk_model::LineStyle::Single),
        ..CharAttrs::default()
    };
    document
        .transact("Format", |edit| {
            let start = edit.document().start();
            edit.insert_text(start, "plain italic rest")?;
            edit.set_character_attrs(Position::new(first, 6), Position::new(first, 12), &italic)?;
            let change = edit.add_change(ChangeKind::Format, "Ana", Some("2026-11-25T09:00:00"))?;
            // Underlined over plain and italic text: each keeps what it had.
            edit.mark_format_change(Position::new(first, 0), Position::new(first, 12), change)?;
            edit.set_character_attrs(
                Position::new(first, 0),
                Position::new(first, 12),
                &underlined,
            )
        })
        .unwrap();
    let before = described(&document);
    assert_eq!(before.0, ["[~plain ~][~italic~] rest"]);
    let befores = |d: &Document| -> Vec<Option<(Option<desk_model::LineStyle>, Option<bool>)>> {
        d.body()
            .iter()
            .next()
            .unwrap()
            .runs()
            .map(|r| {
                r.attrs
                    .formatted
                    .clone()
                    .flatten()
                    .map(|f| (f.attrs.underline, f.attrs.italic))
            })
            .collect()
    };
    assert_eq!(
        befores(&document),
        [Some((None, None)), Some((None, Some(true))), None]
    );
    let bytes = write_odt(&document).unwrap();
    keep("rev-01-changes-of-formatting.odt", &bytes);
    let content = desk_odf::read_odt(&bytes).unwrap();
    assert_eq!(content.tracked_changes.len(), 2);
    assert!(
        content
            .tracked_changes
            .iter()
            .all(|r| r.kind == desk_odf::ChangeKind::FormatChange && r.previous_style.is_some())
    );
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(
        read.body().iter().next().unwrap().text(),
        "plain italic rest"
    );
    assert_eq!(befores(&read), befores(&document));
    let lines = described(&read);
    assert_eq!(lines.0, before.0);
}

#[test]
fn rev_01_changes_of_formatting_of_libreoffice_are_read() {
    let content = format!(
        r#"{HEAD}<text:tracked-changes>
<text:changed-region text:id="f"><text:format-change><office:change-info><dc:creator>Ana</dc:creator><dc:date>2026-11-25T10:00:00</dc:date></office:change-info></text:format-change></text:changed-region>
</text:tracked-changes>
<text:p>Some <text:change-start text:change-id="f"/>bold<text:change-end text:change-id="f"/> text</text:p>{TAIL}"#
    );
    let (document, log) = read_odt_logged(&odt(&content)).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    let (lines, changes) = described(&document);
    assert_eq!(lines, ["Some [~bold~] text"]);
    assert_eq!(changes, ["Format Ana Some(\"2026-11-25T10:00:00\")"]);
    // What it was before is not known: written again, the region has no
    // formatting before either.
    let written = write_odt(&document).unwrap();
    let again = desk_odf::read_odt(&written).unwrap();
    assert_eq!(again.tracked_changes.len(), 1);
    assert_eq!(again.tracked_changes[0].previous_style, None);
    assert_eq!(
        described(&read_odt(&written).unwrap()),
        described(&document)
    );
}

#[test]
fn rev_01_changes_of_formatting_around_others_or_of_nothing() {
    let region = |id: &str, kind: &str, style: &str| {
        format!(
            r#"<text:changed-region text:id="{id}"><text:{kind}{style}><office:change-info><dc:creator>Ana</dc:creator><dc:date>2026-11-25T10:00:00</dc:date></office:change-info></text:{kind}></text:changed-region>"#
        )
    };
    let content = format!(
        r#"{HEAD}<text:tracked-changes xmlns:desk="https://brahm.com.br/ns/desk/1.0">{}{}{}</text:tracked-changes>
<text:p><text:change-start text:change-id="f"/>ab<text:change-start text:change-id="i"/>cd<text:change-end text:change-id="i"/>ef<text:change-end text:change-id="f"/></text:p>
<text:p><text:change-start text:change-id="e"/><text:change-end text:change-id="e"/></text:p>{TAIL}"#,
        region("f", "format-change", r#" desk:style-name="Missing""#),
        region("i", "insertion", ""),
        region("e", "format-change", ""),
    );
    let (document, log) = read_odt_logged(&odt(&content)).unwrap();
    // The insertion keeps its text; the change of an empty paragraph is
    // left out, and listed so.
    let (lines, changes) = described(&document);
    assert_eq!(lines, ["[~ab~][+cd+][~ef~]", ""]);
    assert_eq!(changes.len(), 2);
    assert_eq!(log.entries().len(), 1, "{:?}", log.entries());
    // A style the file does not have leaves the formatting before unknown.
    let first = document.body().iter().next().unwrap();
    assert!(
        first
            .runs()
            .filter(|r| r.text() != "cd")
            .all(|r| r.attrs.formatted == Some(None))
    );
}

#[test]
fn rev_01_changes_of_paragraph_styles_keep_the_style_before() {
    use desk_model::{Alignment, ParaAttrs, Position};
    let mut document = Document::new_word_processing();
    let first = document.body().iter().next().unwrap().id();
    let heading = document.new_id();
    document
        .put_style(desk_model::ParagraphStyle::new(heading, "Heading"))
        .unwrap();
    let body_style = document.body().iter().next().unwrap().style();
    document
        .transact("Format", |edit| {
            let start = edit.document().start();
            let end = edit.insert_text(start, "Centered text")?;
            let second = edit.split_paragraph(end)?;
            edit.insert_text(second, "Second")?;
            let centered = ParaAttrs {
                alignment: Some(Alignment::Center),
                hyphenate: Some(true),
                ..ParaAttrs::default()
            };
            let p = edit.document().paragraph(first)?;
            let style = p.style();
            edit.apply(desk_model::Op::SetParagraphFormat {
                paragraph: first,
                style,
                attrs: centered,
            })?;
            let change = edit.add_change(ChangeKind::Format, "Ana", None)?;
            edit.mark_paragraph_format_change(first, change)?;
            let at = Position::new(first, 0);
            edit.set_paragraph_style(at, at, heading)
        })
        .unwrap();
    let state = |d: &Document| {
        let p = d.body().iter().next().unwrap();
        let before = p.attrs().formatted.clone().flatten().unwrap();
        let name = |id| d.styles().get(id).unwrap().name().to_string();
        (
            name(p.style()),
            p.attrs().format_change.flatten().is_some(),
            name(before.style),
            (before.attrs.alignment, before.attrs.hyphenate),
            d.body().iter().nth(1).unwrap().attrs().format_change,
        )
    };
    let made = state(&document);
    assert_eq!(
        made.2,
        document
            .styles()
            .get(body_style)
            .unwrap()
            .name()
            .to_string()
    );
    assert_eq!(made.3, (Some(Alignment::Center), Some(true)));
    // Only that paragraph is in the change.
    assert_eq!(made.4, None);
    let bytes = write_odt(&document).unwrap();
    keep("rev-01-changes-of-paragraphs.odt", &bytes);
    let content = desk_odf::read_odt(&bytes).unwrap();
    assert_eq!(content.tracked_changes.len(), 1);
    assert!(
        content.tracked_changes[0]
            .previous_paragraph_style
            .is_some()
    );
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(state(&read), made);
    assert_eq!(
        read.body().iter().map(|p| p.text()).collect::<Vec<_>>(),
        ["Centered text", "Second"]
    );
}

#[test]
fn b_070_deleted_text_keeps_the_change_it_was_in_in_odt() {
    use desk_model::Position;
    let mut document = Document::new_word_processing();
    let first = document.body().iter().next().unwrap().id();
    document
        .transact("Changes", |edit| {
            let start = edit.document().start();
            edit.insert_text(start, "one two three four")?;
            let added =
                edit.add_change(ChangeKind::Insertion, "Bo", Some("2026-11-24T09:30:00"))?;
            let formatted = edit.add_change(ChangeKind::Format, "Bo", None)?;
            let gone = edit.add_change(ChangeKind::Deletion, "Ana", None)?;
            // Ana deleted "one two three": "two " was Bo's, and "three"
            // formatted by Bo.
            edit.mark_change(Position::new(first, 0), Position::new(first, 13), gone)?;
            edit.mark_former(
                Position::new(first, 4),
                Position::new(first, 8),
                added,
                None,
            )?;
            edit.mark_former(
                Position::new(first, 8),
                Position::new(first, 13),
                formatted,
                None,
            )?;
            Ok(())
        })
        .unwrap();
    // Each run: its text, the kind of its change and of the one before.
    let formers = |d: &Document| -> Vec<(String, Option<ChangeKind>, Option<ChangeKind>)> {
        let kind =
            |id: Option<desk_model::NodeId>| id.and_then(|id| d.changes().get(id)).map(|c| c.kind);
        d.body()
            .iter()
            .next()
            .unwrap()
            .runs()
            .map(|r| {
                (
                    r.text().to_owned(),
                    kind(r.attrs.change.flatten()),
                    kind(r.attrs.former.flatten()),
                )
            })
            .collect()
    };
    let before = formers(&document);
    assert_eq!(
        before,
        [
            ("one ".into(), Some(ChangeKind::Deletion), None),
            (
                "two ".into(),
                Some(ChangeKind::Deletion),
                Some(ChangeKind::Insertion)
            ),
            (
                "three".into(),
                Some(ChangeKind::Deletion),
                Some(ChangeKind::Format)
            ),
            (" four".into(), None, None),
        ]
    );
    let bytes = write_odt(&document).unwrap();
    keep("b-070-former-changes.odt", &bytes);
    // Other programs read the text as if the deletion were accepted.
    let content = desk_odf::read_odt(&bytes).unwrap();
    assert_eq!(content.body[0].text(), " four");
    assert_eq!(content.tracked_changes.len(), 3);
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(formers(&read), before);
    assert_eq!(described(&read).1, described(&document).1);
}

/// B-070 review: the change that deleted text was in comes back as one
/// change when the deletion takes a break in it, and the formatting before
/// of a change of formatting comes back for each run, in ODT.
#[test]
fn b_070_a_change_in_deleted_text_comes_back_whole_in_odt() {
    use desk_model::{CharAttrs, FontWeight, Formatting, Position};
    use std::sync::Arc;
    let mut document = Document::new_word_processing();
    let first = document.body().iter().next().unwrap().id();
    document
        .transact("Changes", |edit| {
            let start = edit.document().start();
            edit.insert_text(start, "one two\nABC three")?;
            let second = edit.document().body().iter().nth(1).unwrap().id();
            let added =
                edit.add_change(ChangeKind::Insertion, "Bo", Some("2026-11-24T09:30:00"))?;
            let formatted =
                edit.add_change(ChangeKind::Format, "Bo", Some("2026-11-24T09:31:00"))?;
            let gone = edit.add_change(ChangeKind::Deletion, "Ana", None)?;
            // Ana deleted "two", the break and "ABC": "two", the break
            // and "A" were Bo's (B-072), and "B" was bold and "C" italic
            // before Bo underlined them.
            edit.mark_change(Position::new(first, 4), Position::new(first, 7), gone)?;
            edit.mark_break_change(first, Some(gone))?;
            edit.mark_break_former(first, added)?;
            edit.mark_change(Position::new(second, 0), Position::new(second, 3), gone)?;
            edit.mark_former(
                Position::new(first, 4),
                Position::new(first, 7),
                added,
                None,
            )?;
            edit.mark_former(
                Position::new(second, 0),
                Position::new(second, 1),
                added,
                None,
            )?;
            let bold = Formatting {
                style: None,
                attrs: CharAttrs {
                    weight: Some(FontWeight::BOLD),
                    ..CharAttrs::default()
                },
            };
            let italic = Formatting {
                style: None,
                attrs: CharAttrs {
                    italic: Some(true),
                    ..CharAttrs::default()
                },
            };
            edit.mark_former(
                Position::new(second, 1),
                Position::new(second, 2),
                formatted,
                Some(Arc::new(bold)),
            )?;
            edit.mark_former(
                Position::new(second, 2),
                Position::new(second, 3),
                formatted,
                Some(Arc::new(italic)),
            )?;
            Ok(())
        })
        .unwrap();
    // Each deleted run: its text, the author of the change it was in
    // before, and the formatting before of that one.
    let formers = |d: &Document| -> Vec<(String, String, Option<Formatting>)> {
        d.body()
            .iter()
            .flat_map(|p| p.runs())
            .filter_map(|r| {
                let former = d.changes().get(r.attrs.former.flatten()?)?;
                Some((
                    r.text().to_owned(),
                    former.author.to_string(),
                    r.attrs.formatted.clone().flatten().map(|f| (*f).clone()),
                ))
            })
            .collect()
    };
    let before = formers(&document);
    assert_eq!(before.len(), 4, "{before:?}");
    let bytes = write_odt(&document).unwrap();
    keep("b-070-former-across-break.odt", &bytes);
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(formers(&read), before);
    // Bo's insertion is one change, across the break the deletion took.
    let insertions = |d: &Document| {
        d.changes()
            .iter()
            .filter(|c| c.kind == ChangeKind::Insertion)
            .count()
    };
    assert_eq!(insertions(&read), 1);
    let first = read.body().iter().next().unwrap();
    let added = read
        .changes()
        .iter()
        .find(|c| c.kind == ChangeKind::Insertion)
        .unwrap()
        .id;
    assert_eq!(first.attrs().break_former.flatten(), Some(added));
}

/// B-069: the change of the formatting of a paragraph stays on its
/// paragraph in ODT when a tracked deletion took the break before it,
/// the break after it, or the whole paragraph.
#[test]
fn b_069_a_change_of_a_paragraph_joined_by_a_deletion_stays_on_it_in_odt() {
    use desk_model::Position;
    let mut document = Document::new_word_processing();
    document
        .transact("Changes", |edit| {
            let start = edit.document().start();
            let mut at = edit.insert_text(start, "one")?;
            for text in ["two", "three", "four"] {
                at = edit.split_paragraph(at)?;
                at = edit.insert_text(at, text)?;
            }
            let ids: Vec<_> = edit.document().body().iter().map(|p| p.id()).collect();
            let gone = edit.add_change(ChangeKind::Deletion, "Ana", Some("2026-11-24T09:00:00"))?;
            // Ana deleted the break after "one", all of "two" and its
            // break, and the break after "three"; Bo changed the
            // formatting of "one", "two" and "four".
            edit.mark_break_change(ids[0], Some(gone))?;
            edit.mark_change(Position::new(ids[1], 0), Position::new(ids[1], 3), gone)?;
            edit.mark_break_change(ids[1], Some(gone))?;
            for &i in &[0, 1, 3] {
                let bo = edit.add_change(
                    ChangeKind::Format,
                    "Bo",
                    Some(&format!("2026-11-24T09:3{i}:00")),
                )?;
                edit.mark_paragraph_formatted(ids[i], bo, None)?;
            }
            let other = edit.add_change(ChangeKind::Deletion, "Cy", None)?;
            edit.mark_break_change(ids[2], Some(other))?;
            Ok(())
        })
        .unwrap();
    // Each paragraph: its text and the author of the change of its
    // formatting.
    let formats = |d: &Document| -> Vec<(String, Option<String>)> {
        d.body()
            .iter()
            .map(|p| {
                (
                    p.text(),
                    p.attrs()
                        .format_change
                        .flatten()
                        .and_then(|id| d.changes().get(id))
                        .map(|c| c.author.to_string()),
                )
            })
            .collect()
    };
    let before = formats(&document);
    assert_eq!(
        before,
        [
            ("one".into(), Some("Bo".into())),
            ("two".into(), Some("Bo".into())),
            ("three".into(), None),
            ("four".into(), Some("Bo".into())),
        ]
    );
    let bytes = write_odt(&document).unwrap();
    keep("b-069-paragraph-changes-joined.odt", &bytes);
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(formats(&read), before);
    assert_eq!(described(&read), described(&document));
}

/// B-069 review: a change of a paragraph whose marks take in a deletion
/// of breaks, as another program may write it, stays on the paragraph of
/// its start and does not reach the paragraphs the deletion took.
#[test]
fn b_069_a_change_of_a_paragraph_around_a_deletion_stays_on_its_paragraph() {
    let content = format!(
        r#"{HEAD}<text:tracked-changes>
<text:changed-region text:id="p"><text:format-change xmlns:desk="https://brahm.com.br/ns/desk/1.0" desk:paragraph-style-name=""><office:change-info><dc:creator>Bo</dc:creator><dc:date>2026-11-24T12:00:00</dc:date></office:change-info></text:format-change></text:changed-region>
<text:changed-region text:id="d"><text:deletion><office:change-info><dc:creator>Ana</dc:creator><dc:date>2026-11-24T13:00:00</dc:date></office:change-info><text:p/><text:p>two</text:p><text:p/></text:deletion></text:changed-region>
</text:tracked-changes>
<text:p><text:change-start text:change-id="p"/>one<text:change text:change-id="d"/>three<text:change-end text:change-id="p"/></text:p>{TAIL}"#
    );
    let document = read_odt(&odt(&content)).unwrap();
    let formatted: Vec<(String, bool)> = document
        .body()
        .iter()
        .map(|p| (p.text(), p.attrs().format_change.flatten().is_some()))
        .collect();
    assert_eq!(
        formatted,
        [
            ("one".into(), true),
            ("two".into(), false),
            ("three".into(), false)
        ]
    );
}

/// B-072: a paragraph break that another author added keeps that
/// insertion in ODT when a tracked deletion takes it, apart from the
/// insertion of the text before it.
#[test]
fn b_072_a_deleted_break_keeps_its_insertion_in_odt() {
    use desk_model::Position;
    let mut document = Document::new_word_processing();
    document
        .transact("Changes", |edit| {
            let start = edit.document().start();
            let mut at = edit.insert_text(start, "one two")?;
            for text in ["three", "four"] {
                at = edit.split_paragraph(at)?;
                at = edit.insert_text(at, text)?;
            }
            let ids: Vec<_> = edit.document().body().iter().map(|p| p.id()).collect();
            let added =
                edit.add_change(ChangeKind::Insertion, "Bo", Some("2026-11-24T09:30:00"))?;
            let gone = edit.add_change(ChangeKind::Deletion, "Ana", Some("2026-11-24T10:00:00"))?;
            // Bo added "two" and the break after "three"; Ana deleted
            // "two" and both breaks.
            edit.mark_change(Position::new(ids[0], 4), Position::new(ids[0], 7), gone)?;
            edit.mark_former(
                Position::new(ids[0], 4),
                Position::new(ids[0], 7),
                added,
                None,
            )?;
            edit.mark_break_change(ids[0], Some(gone))?;
            edit.mark_break_change(ids[1], Some(gone))?;
            edit.mark_break_former(ids[1], added)?;
            Ok(())
        })
        .unwrap();
    let bytes = write_odt(&document).unwrap();
    keep("b-072-deleted-break.odt", &bytes);
    let (mut read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    // Only the break after "three" is in Bo's insertion: the one after
    // "one two" is in none.
    let bo = |d: &Document, id: Option<desk_model::NodeId>| {
        id.and_then(|id| d.changes().get(id))
            .map(|c| (c.kind, c.author.to_string()))
    };
    let breaks: Vec<_> = read
        .body()
        .iter()
        .map(|p| bo(&read, p.attrs().break_former.flatten()))
        .collect();
    assert_eq!(
        breaks,
        [None, Some((ChangeKind::Insertion, "Bo".to_owned())), None]
    );
    // Rejecting Bo's insertions takes "two" and the break after "three",
    // which Ana's deletion still holds.
    let added: Vec<_> = read
        .changes()
        .iter()
        .filter(|c| c.kind == ChangeKind::Insertion)
        .map(|c| c.id)
        .collect();
    read.transact("Resolve", |edit| {
        edit.resolve_changes(false, |c| added.contains(&c))
            .map(|_| ())
    })
    .unwrap();
    let texts: Vec<_> = read.body().iter().map(|p| p.text().to_owned()).collect();
    assert_eq!(texts, ["one ", "threefour"]);
}

/// B-072 review: a change of formatting that deleted text was in goes on
/// past a break the deletion took, which no change of formatting holds,
/// and comes back from ODT as one change.
#[test]
fn b_072_a_change_of_formatting_goes_on_past_a_deleted_break_in_odt() {
    use desk_model::Position;
    let mut document = Document::new_word_processing();
    document
        .transact("Changes", |edit| {
            let start = edit.document().start();
            let at = edit.insert_text(start, "one two")?;
            let at = edit.split_paragraph(at)?;
            edit.insert_text(at, "three four")?;
            let ids: Vec<_> = edit.document().body().iter().map(|p| p.id()).collect();
            let formatted =
                edit.add_change(ChangeKind::Format, "Bo", Some("2026-11-24T09:30:00"))?;
            let gone = edit.add_change(ChangeKind::Deletion, "Ana", Some("2026-11-24T10:00:00"))?;
            // Bo formatted "two" and "three"; Ana deleted them and the
            // break between them.
            edit.mark_change(Position::new(ids[0], 4), Position::new(ids[0], 7), gone)?;
            edit.mark_break_change(ids[0], Some(gone))?;
            edit.mark_change(Position::new(ids[1], 0), Position::new(ids[1], 5), gone)?;
            for (p, from, to) in [(ids[0], 4, 7), (ids[1], 0, 5)] {
                edit.mark_former(
                    Position::new(p, from),
                    Position::new(p, to),
                    formatted,
                    None,
                )?;
            }
            Ok(())
        })
        .unwrap();
    let bytes = write_odt(&document).unwrap();
    keep("b-072-format-across-break.odt", &bytes);
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    let formats = read
        .changes()
        .iter()
        .filter(|c| c.kind == ChangeKind::Format)
        .count();
    assert_eq!(formats, 1);
    assert!(read.body().iter().all(|p| p.attrs().break_former.is_none()));
}

/// B-073: a change that another change interrupts, an insertion part of
/// which another author deleted, comes back from ODT as one change, and
/// changes of the same author and date stay apart.
#[test]
fn b_073_a_change_another_interrupts_comes_back_as_one() {
    use desk_model::Position;
    let mut document = Document::new_word_processing();
    let first = document.body().iter().next().unwrap().id();
    let (bo, _) = document
        .transact("Changes", |edit| {
            let start = edit.document().start();
            edit.insert_text(start, "one two three four five")?;
            let bo = edit.add_change(ChangeKind::Insertion, "Bo", Some("2026-11-24T09:30:00"))?;
            let ana = edit.add_change(ChangeKind::Deletion, "Ana", Some("2026-11-24T10:00:00"))?;
            let cy = edit.add_change(ChangeKind::Insertion, "Cy", Some("2026-11-24T11:00:00"))?;
            let dee = edit.add_change(ChangeKind::Insertion, "Cy", Some("2026-11-24T11:00:00"))?;
            // Bo added "one two three", whose "two" Ana deleted; Cy
            // added "four" and, as another change in the same second,
            // "five".
            edit.mark_change(Position::new(first, 0), Position::new(first, 14), bo)?;
            edit.mark_change(Position::new(first, 4), Position::new(first, 7), ana)?;
            edit.mark_former(Position::new(first, 4), Position::new(first, 7), bo, None)?;
            edit.mark_change(Position::new(first, 14), Position::new(first, 18), cy)?;
            edit.mark_change(Position::new(first, 19), Position::new(first, 23), dee)?;
            Ok(bo)
        })
        .unwrap();
    let _ = bo;
    let bytes = write_odt(&document).unwrap();
    keep("b-073-interrupted.odt", &bytes);
    let (mut read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    let of = |d: &Document, author: &str| {
        d.changes()
            .iter()
            .filter(|c| &*c.author == author)
            .map(|c| c.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(of(&read, "Bo").len(), 1);
    assert_eq!(of(&read, "Ana").len(), 1);
    assert_eq!(of(&read, "Cy").len(), 2);
    // Rejecting Bo's one insertion takes all it added.
    let bo = of(&read, "Bo")[0];
    read.transact("Resolve", |edit| {
        edit.resolve_changes(false, |c| c == bo).map(|_| ())
    })
    .unwrap();
    assert_eq!(read.body().iter().next().unwrap().text(), "four five");
}

/// B-068: a paragraph whose break before it a deletion took keeps in ODT
/// the section it starts and its entry of a table of contents, and an
/// item the style it overrides its list with.
#[test]
fn b_068_a_paragraph_joined_by_a_deletion_keeps_its_structure_in_odt() {
    use desk_model::{ParaAttrs, Position, Section, Start, TocEntry};
    use std::sync::Arc;
    let mut document = Document::new_word_processing();
    document
        .transact("Text", |edit| {
            let start = edit.document().start();
            let mut at = edit.insert_text(start, "Intro")?;
            for text in ["Chapter", "Entry", "After"] {
                at = edit.split_paragraph(at)?;
                at = edit.insert_text(at, text)?;
            }
            let ids: Vec<_> = edit.document().body().iter().map(|p| p.id()).collect();
            let section = Start::Section(Section::default(), desk_model::Layout::default());
            let set = |edit: &mut desk_model::Edit, p, attrs: &ParaAttrs| {
                edit.set_paragraph_attrs(Position::new(p, 0), Position::new(p, 0), attrs)
            };
            set(
                edit,
                ids[1],
                &ParaAttrs {
                    start: Some(Some(Arc::new(section))),
                    ..ParaAttrs::default()
                },
            )?;
            set(
                edit,
                ids[2],
                &ParaAttrs {
                    toc_entry: Some(Some(TocEntry { target: None })),
                    ..ParaAttrs::default()
                },
            )?;
            // The breaks before "Chapter" and "Entry", deleted.
            let gone = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
            edit.mark_break_change(ids[0], Some(gone))?;
            edit.mark_break_change(ids[1], Some(gone))?;
            Ok(())
        })
        .unwrap();
    let structure = |document: &Document| {
        document
            .body()
            .iter()
            .map(|p| {
                (
                    p.text(),
                    matches!(
                        p.attrs().start.clone().flatten().as_deref(),
                        Some(Start::Section(..))
                    ),
                    p.attrs().toc_entry.flatten().is_some(),
                )
            })
            .collect::<Vec<_>>()
    };
    let before = structure(&document);
    assert_eq!(
        before,
        [
            ("Intro".to_owned(), false, false),
            ("Chapter".to_owned(), true, false),
            ("Entry".to_owned(), false, true),
            ("After".to_owned(), false, false),
        ]
    );
    let bytes = write_odt(&document).unwrap();
    keep("b-068-structure-joined.odt", &bytes);
    let (read, log) = read_odt_logged(&bytes).unwrap();
    assert!(log.is_empty(), "{:?}", log.entries());
    assert_eq!(structure(&read), before);
}

/// B-068: an item that a deletion joins to the paragraph before keeps the
/// style that overrides its list.
#[test]
fn b_068_a_joined_item_keeps_the_style_overriding_its_list() {
    use desk_model::Position;
    let content = format!(
        r#"{HEAD}<text:p>Intro</text:p><text:list text:style-name="L1"><text:list-item text:style-override="L2"><text:p>Item</text:p></text:list-item></text:list>{TAIL}"#
    )
    .replace(
        "<office:body>",
        r#"<office:automatic-styles><text:list-style style:name="L1" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"><text:list-level-style-bullet text:level="1" text:bullet-char="•"/></text:list-style><text:list-style style:name="L2" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"><text:list-level-style-bullet text:level="1" text:bullet-char="★"/></text:list-style></office:automatic-styles><office:body>"#,
    );
    let mut document = read_odt(&odt(&content)).unwrap();
    let own = |document: &Document| {
        document
            .body()
            .iter()
            .nth(1)
            .and_then(|p| p.attrs().list_override.clone().flatten())
    };
    let item = own(&document);
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
    let bytes = write_odt(&document).unwrap();
    keep("b-068-override-joined.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    assert_eq!(own(&read), item);
}

#[test]
fn rev_01_objects_put_in_and_deleted_from_the_text_are_kept_in_odt() {
    use desk_model::{FIELD_CHARACTER, Frame, Object, Position, Shape};
    let mut document = Document::new_word_processing();
    document
        .transact("Changes", |edit| {
            let start = edit.document().start();
            edit.insert_text(start, "ab cd")?;
            let p = start.paragraph;
            let oval = |edit: &mut desk_model::Edit| {
                Object::new(
                    edit.new_id(),
                    Shape::Ellipse.into(),
                    Frame::from_pt(0.0, 0.0, 20.0, 10.0),
                )
            };
            let (one, two) = (oval(edit), oval(edit));
            edit.insert_object_in_text(Position::new(p, 4), two)?;
            edit.insert_object_in_text(Position::new(p, 1), one)?;
            let mark = FIELD_CHARACTER.len_utf8();
            let ins = edit.add_change(ChangeKind::Insertion, "Ana", None)?;
            let del = edit.add_change(ChangeKind::Deletion, "Bo", None)?;
            edit.mark_change(Position::new(p, 1), Position::new(p, 1 + mark), ins)?;
            edit.mark_change(
                Position::new(p, 4 + mark),
                Position::new(p, 4 + 2 * mark),
                del,
            )?;
            Ok(())
        })
        .unwrap();
    let before = described(&document);
    let bytes = write_odt(&document).unwrap();
    keep("rev-01-objects-in-changes.odt", &bytes);
    let read = read_odt(&bytes).unwrap();
    assert_eq!(described(&read), before);
    // Both objects come back, the one in the deletion too, each where
    // its mark is, as large as it was.
    let marks = read.object_marks();
    assert_eq!(marks.len(), 2);
    assert_eq!(read.objects().iter().count(), 2);
    for &id in marks.keys() {
        let object = read.objects().get(id).unwrap();
        assert_eq!(object.frame, Frame::from_pt(0.0, 0.0, 20.0, 10.0));
    }
    // Accepted, the deletion takes its object.
    let mut read = read;
    read.transact("Accept", |edit| edit.resolve_changes(true, |_| true))
        .unwrap();
    assert_eq!(read.objects().iter().count(), 1);
}

#[test]
fn rev_01_objects_moved_or_resized_are_kept_in_odt() {
    use desk_model::{Anchor, Frame, Length, Object, Shape, TextSettings, Tracking};
    let mut editor = desk_model::Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    let (fixed, moving) = editor
        .edit("Objects", |edit| {
            let end = edit.insert_text(start, "One")?;
            let next = edit.split_paragraph(end)?;
            edit.insert_text(next, "Two")?;
            let mut fixed = Object::new(
                edit.new_id(),
                Shape::Ellipse.into(),
                Frame::from_pt(100.0, 100.0, 50.0, 40.0),
            );
            fixed.anchor = Anchor::Page(0);
            let mut moving = Object::new(
                edit.new_id(),
                Shape::Rectangle.into(),
                Frame::from_pt(90.0, 10.0, 30.0, 30.0),
            );
            moving.anchor = Anchor::Paragraph(next.paragraph);
            let ids = (fixed.id, moving.id);
            edit.add_object(fixed)?;
            edit.add_object(moving)?;
            Ok(ids)
        })
        .unwrap();
    let first = editor.document().body().get(0).unwrap().id();
    editor.set_author("Ana");
    let settings = TextSettings {
        tracking: Tracking::On,
        ..editor.document().text_settings().clone()
    };
    editor
        .edit("Track Changes", |edit| edit.set_text_settings(settings))
        .unwrap();
    editor
        .edit("Move", |edit| {
            edit.move_objects(&[fixed], Length::from_pt(20.0), Length::from_pt(-10.0))?;
            edit.place_object(
                moving,
                Anchor::Paragraph(first),
                Frame::from_pt(80.0, 5.0, 60.0, 45.0),
            )
        })
        .unwrap();
    let document = editor.document();
    let bytes = write_odt(document).unwrap();
    keep("rev-01-objects-moved.odt", &bytes);
    // One region, of a change of formatting, which the objects name.
    let content = desk_odf::read_odt(&bytes).unwrap();
    assert_eq!(content.tracked_changes.len(), 1);
    assert_eq!(
        content.tracked_changes[0].kind,
        desk_odf::ChangeKind::FormatChange
    );
    let read = read_odt(&bytes).unwrap();
    assert_eq!(read.changes().len(), 1);
    let change = read.changes().iter().next().unwrap();
    assert_eq!(change.kind, ChangeKind::Placement);
    assert_eq!(change.author.as_ref(), "Ana");
    // Each object where it is, with where it was: the second moved from
    // the second paragraph to the first.
    let second = read.body().get(1).unwrap().id();
    let placed: Vec<_> = read
        .objects()
        .iter()
        .map(|o| {
            (
                o.anchor,
                o.frame,
                o.moved.map(|m| (m.change, m.anchor, m.frame)),
            )
        })
        .collect();
    let read_first = read.body().get(0).unwrap().id();
    assert_eq!(
        placed,
        [
            (
                Anchor::Page(0),
                Frame::from_pt(120.0, 90.0, 50.0, 40.0),
                Some((
                    change.id,
                    Anchor::Page(0),
                    Frame::from_pt(100.0, 100.0, 50.0, 40.0)
                ))
            ),
            (
                Anchor::Paragraph(read_first),
                Frame::from_pt(80.0, 5.0, 60.0, 45.0),
                Some((
                    change.id,
                    Anchor::Paragraph(second),
                    Frame::from_pt(90.0, 10.0, 30.0, 30.0)
                ))
            ),
        ]
    );
    // Rejected, they are back where they were.
    let mut read = read;
    read.transact("Reject", |edit| edit.resolve_changes(false, |_| true))
        .unwrap();
    let placed: Vec<_> = read
        .objects()
        .iter()
        .map(|o| (o.anchor, o.frame, o.moved))
        .collect();
    assert_eq!(
        placed,
        [
            (
                Anchor::Page(0),
                Frame::from_pt(100.0, 100.0, 50.0, 40.0),
                None
            ),
            (
                Anchor::Paragraph(second),
                Frame::from_pt(90.0, 10.0, 30.0, 30.0),
                None
            ),
        ]
    );
}
