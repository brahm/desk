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
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" office:version="1.3"><office:body><office:text>"#;
const TAIL: &str = "</office:text></office:body></office:document-content>";

/// Each paragraph of the body, its text with the changes in it: `[+…+]`
/// for text added, `[-…-]` for text deleted, and `¶+` or `¶-` at the end
/// for a break added or deleted; then each change, with its author and
/// date.
fn described(document: &Document) -> (Vec<String>, Vec<String>) {
    let mut lines = Vec::new();
    for paragraph in document.body().iter() {
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
    let mut changes: Vec<String> = document
        .changes()
        .iter()
        .map(|c| format!("{:?} {} {:?}", c.kind, c.author, c.date))
        .collect();
    changes.sort();
    (lines, changes)
}

/// Insertions in a paragraph and past its end, and deletions in a
/// paragraph and of a break, as LibreOffice writes them.
const LIBREOFFICE: &str = r#"<text:tracked-changes text:track-changes="false">
<text:changed-region xml:id="ct1" text:id="ct1"><text:insertion><office:change-info><dc:creator>Ana</dc:creator><dc:date>2026-11-24T10:00:00</dc:date></office:change-info></text:insertion></text:changed-region>
<text:changed-region xml:id="ct2" text:id="ct2"><text:deletion><office:change-info><dc:creator>Bo</dc:creator><dc:date>2026-11-24T11:00:00</dc:date></office:change-info><text:p>old </text:p></text:deletion></text:changed-region>
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
