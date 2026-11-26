// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The records of a spreadsheet of ODF as the source of a mail merge
//! (MAL-05).

use std::io::Write;

use desk_contacts::{Records, SourceError, read_source};
use desk_model::MergeSource;

fn spreadsheet() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/vnd.oasis.opendocument.spreadsheet")
        .unwrap();
    zip.start_file("content.xml", stored).unwrap();
    let row = |cells: &[&str]| {
        let cells: String = cells
            .iter()
            .map(|c| format!("<table:table-cell><text:p>{c}</text:p></table:table-cell>"))
            .collect();
        format!("<table:table-row>{cells}</table:table-row>")
    };
    let content = format!(
        r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet><table:table table:name="Notes"/><table:table table:name="2026">{}{}{}</table:table><table:table table:name="2025">{}{}</table:table></office:spreadsheet></office:body></office:document-content>"#,
        row(&["Empresa", "Cidade"]),
        row(&["Desk", "Lisboa"]),
        row(&["Brahm", "São Paulo"]),
        row(&["Company"]),
        row(&["Old"]),
    );
    zip.write_all(content.as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

fn read(table: Option<&str>) -> Result<Records, SourceError> {
    let source = MergeSource::Spreadsheet {
        file: "/home/ana/Clients.ods".into(),
        table: table.map(Into::into),
    };
    read_source(&source, |_| Ok(std::io::Cursor::new(spreadsheet())))
}

#[test]
fn mal_05_a_table_of_a_spreadsheet_gives_the_records() {
    // Without a table, the first with rows.
    let records = read(None).unwrap();
    assert_eq!(records.tables, ["Notes", "2026", "2025"]);
    assert_eq!(records.fields, ["Empresa", "Cidade"]);
    assert_eq!(records.len(), 2);
    assert_eq!(records.value(1, "Company"), Some("Brahm"));
    assert_eq!(records.value(1, "City"), Some("São Paulo"));
    let records = read(Some("2025")).unwrap();
    assert_eq!(records.value(0, "Company"), Some("Old"));
    assert!(matches!(read(Some("2024")), Err(SourceError::NoTable(_))));
}

#[test]
fn mal_04_a_missing_address_takes_another_of_its_kind() {
    let records = Records::of_table(&[
        vec!["Name".into(), "Work Address".into(), "Home Phone".into()],
        vec!["Ana".into(), "1 Rua A".into(), String::new()],
    ]);
    assert_eq!(records.datum(0, "Home Address"), Some("1 Rua A"));
    assert_eq!(records.datum(0, "Endereço residencial"), Some("1 Rua A"));
    assert_eq!(records.datum(0, "Work Address"), Some("1 Rua A"));
    assert_eq!(records.datum(0, "Address"), Some("1 Rua A"));
    // A phone has no other to take, and a name is no kind.
    assert_eq!(records.datum(0, "Work Phone"), None);
    assert_eq!(records.datum(0, "Company"), None);
}
