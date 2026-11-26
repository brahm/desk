// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The tables of a spreadsheet (ODS), as text, which a mail merge reads
//! its records from (W105, MAL-05): each sheet by its name, its rows and
//! the text of their cells.

use std::io::{Cursor, Read};

use zip::ZipArchive;
use zip::result::ZipError;

use crate::budget::{MAX_DOCUMENT_BYTES, MAX_DOCUMENT_CELLS};
use crate::error::OdfError;
use crate::ns::{OFFICE, TABLE, TEXT};
use crate::package::MAX_STREAM;
use crate::table::MAX_COLUMNS;
use crate::xml::Element;

/// The media type of a spreadsheet.
pub const SPREADSHEET_TYPE: &str = "application/vnd.oasis.opendocument.spreadsheet";

/// The most rows read from a sheet, as many as a list of separated values
/// gives a mail merge.
pub const MAX_SHEET_ROWS: usize = 100_000;

/// A sheet of a spreadsheet, as text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    /// `table:name`.
    pub name: String,
    /// The rows, each the text of its cells, without the empty cells at
    /// the end of a row or the empty rows at the end of the sheet. The
    /// paragraphs of a cell are lines of its text.
    pub rows: Vec<Vec<String>>,
    /// Whether rows or cells were left out, past the limits of a sheet or
    /// of the document.
    pub truncated: bool,
}

/// Reads the sheets of a spreadsheet (ODS) from the bytes of its package.
pub fn read_ods(bytes: &[u8]) -> Result<Vec<Sheet>, OdfError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let mut read = |name: &'static str| -> Result<Option<String>, OdfError> {
        let file = match archive.by_name(name) {
            Ok(file) => file,
            Err(ZipError::FileNotFound) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_STREAM + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_STREAM {
            return Err(OdfError::TooLarge(name));
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| OdfError::Malformed(format!("{name} is not UTF-8")))
    };
    if let Some(media_type) = read("mimetype")? {
        let media_type = media_type.trim();
        if media_type != SPREADSHEET_TYPE
            && media_type != "application/vnd.oasis.opendocument.spreadsheet-template"
        {
            return Err(OdfError::WrongType(media_type.to_owned()));
        }
    }
    let content = read("content.xml")?.ok_or(OdfError::Missing("content.xml"))?;
    read_sheets(&content)
}

/// What the sheets of one document may still take: cells, and the bytes
/// of the copies that repetition makes.
struct Budget {
    cells: usize,
    bytes: usize,
}

impl Budget {
    /// Takes a cell, or a copy of one of `bytes`; false when nothing is
    /// left for it.
    fn take(&mut self, bytes: usize) -> bool {
        if self.cells == 0 || self.bytes < bytes {
            return false;
        }
        self.cells -= 1;
        self.bytes -= bytes;
        true
    }

    /// Takes cells and bytes together, or nothing when fewer are left.
    fn take_many(&mut self, cells: usize, bytes: usize) -> bool {
        if self.cells < cells || self.bytes < bytes {
            return false;
        }
        self.cells -= cells;
        self.bytes -= bytes;
        true
    }
}

/// Reads the sheets of the `content.xml` of a spreadsheet.
pub fn read_sheets(content: &str) -> Result<Vec<Sheet>, OdfError> {
    let root = crate::xml::parse(content)?;
    let Some(spreadsheet) = root
        .child(OFFICE, "body")
        .and_then(|b| b.child(OFFICE, "spreadsheet"))
    else {
        return Err(OdfError::Missing("office:spreadsheet"));
    };
    let mut budget = Budget {
        cells: MAX_DOCUMENT_CELLS,
        bytes: MAX_DOCUMENT_BYTES,
    };
    Ok(spreadsheet
        .children_named(TABLE, "table")
        .map(|table| {
            let mut sheet = Sheet {
                name: table.attr(TABLE, "name").unwrap_or_default().to_owned(),
                ..Sheet::default()
            };
            // Empty rows wait for a row with text after them, so that the
            // empty rows that fill a sheet to its end take nothing.
            let mut empty = 0;
            read_rows(table, &mut sheet, &mut empty, &mut budget);
            sheet
        })
        .collect())
}

/// How many times an element repeats, by its attribute, at least once.
fn repeated(e: &Element, name: &str) -> usize {
    e.attr(TABLE, name)
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1)
}

/// The rows of a table, or of a group of rows or of its header rows.
fn read_rows(e: &Element, sheet: &mut Sheet, empty: &mut usize, budget: &mut Budget) {
    for child in e.elements() {
        if child.name.ns != TABLE || sheet.truncated {
            continue;
        }
        match child.name.local.as_str() {
            "table-header-rows" | "table-row-group" | "table-rows" => {
                read_rows(child, sheet, empty, budget);
            }
            "table-row" => {
                let times = repeated(child, "number-rows-repeated");
                let (row, cut) = read_row(child, budget);
                sheet.truncated |= cut;
                if row.is_empty() {
                    *empty = empty.saturating_add(times);
                    continue;
                }
                let bytes: usize = row.iter().map(String::len).sum();
                for copy in 0..times {
                    if sheet.rows.len() + *empty >= MAX_SHEET_ROWS {
                        sheet.truncated = true;
                        return;
                    }
                    sheet.rows.extend(std::iter::repeat_n(Vec::new(), *empty));
                    *empty = 0;
                    // The row as it was written is paid for; its copies
                    // pay their bytes again.
                    if copy > 0 && !budget.take_many(row.len(), bytes) {
                        sheet.truncated = true;
                        return;
                    }
                    sheet.rows.push(row.clone());
                }
            }
            _ => {}
        }
    }
}

/// The text of the cells of a row, without the empty ones at its end, and
/// whether cells were left out.
fn read_row(row: &Element, budget: &mut Budget) -> (Vec<String>, bool) {
    let mut cells = Vec::new();
    let mut empty = 0usize;
    let mut cut = false;
    for cell in row.elements() {
        if !(cell.name.is(TABLE, "table-cell") || cell.name.is(TABLE, "covered-table-cell")) {
            continue;
        }
        let text = cell
            .elements()
            .filter(|p| p.name.is(TEXT, "p") || p.name.is(TEXT, "h"))
            .map(|p| crate::read::paragraph(p).text())
            .collect::<Vec<_>>()
            .join("\n");
        let times = repeated(cell, "number-columns-repeated");
        // Empty cells wait for a cell with text, as empty rows do.
        if text.is_empty() {
            empty = empty.saturating_add(times);
            continue;
        }
        for copy in 0..times {
            if cells.len() + empty >= MAX_COLUMNS {
                return (cells, true);
            }
            for _ in 0..empty {
                if !budget.take(0) {
                    return (cells, true);
                }
                cells.push(String::new());
            }
            empty = 0;
            if !budget.take(if copy > 0 { text.len() } else { 0 }) {
                cut = true;
                break;
            }
            cells.push(text.clone());
        }
        if cut {
            break;
        }
    }
    (cells, cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: &str = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet>"#;

    #[test]
    fn mal_05_the_sheets_of_a_spreadsheet_as_text() {
        let content = format!(
            r#"{START}<table:table table:name="Clients"><table:table-column table:number-columns-repeated="3"/><table:table-header-rows><table:table-row><table:table-cell><text:p>First Name</text:p></table:table-cell><table:table-cell/><table:table-cell><text:p>City</text:p></table:table-cell><table:table-cell table:number-columns-repeated="16380"/></table:table-row></table:table-header-rows><table:table-row table:number-rows-repeated="2"><table:table-cell table:number-columns-repeated="2"><text:p>Ana</text:p></table:table-cell><table:table-cell><text:p>Rua A</text:p><text:p>Lisboa</text:p></table:table-cell></table:table-row><table:table-row table:number-rows-repeated="1048573"><table:table-cell table:number-columns-repeated="16384"/></table:table-row></table:table><table:table table:name="Empty"/></office:spreadsheet></office:body></office:document-content>"#
        );
        let sheets = read_sheets(&content).unwrap();
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[0].name, "Clients");
        assert_eq!(
            sheets[0].rows,
            [
                vec!["First Name", "", "City"],
                vec!["Ana", "Ana", "Rua A\nLisboa"],
                vec!["Ana", "Ana", "Rua A\nLisboa"],
            ]
        );
        assert!(!sheets[0].truncated);
        assert_eq!(
            sheets[1],
            Sheet {
                name: "Empty".into(),
                ..Sheet::default()
            }
        );
        assert!(read_sheets("<x/>").is_err());
        // More rows than a table of a text document has, as a list of
        // recipients may.
        let content = format!(
            r#"{START}<table:table table:name="Long"><table:table-row table:number-rows-repeated="20000"><table:table-cell><text:p>x</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#
        );
        let sheets = read_sheets(&content).unwrap();
        assert_eq!(sheets[0].rows.len(), 20_000);
        assert!(!sheets[0].truncated);
        // Copies of a large text stop at the bytes of the document, and
        // the sheet says it was cut.
        let text = "x".repeat(64 << 10);
        let content = format!(
            r#"{START}<table:table table:name="Bomb"><table:table-row table:number-rows-repeated="2000"><table:table-cell table:number-columns-repeated="1024"><text:p>{text}</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#
        );
        let sheets = read_sheets(&content).unwrap();
        assert!(sheets[0].truncated);
        let bytes: usize = sheets[0].rows.iter().flatten().map(String::len).sum();
        assert!(bytes <= MAX_DOCUMENT_BYTES + text.len(), "{bytes}");
        // Rows and cells past the limits are left out and said to be.
        let content = format!(
            r#"{START}<table:table table:name="Wide"><table:table-row table:number-rows-repeated="200000"><table:table-cell table:number-columns-repeated="2"><text:p>y</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#
        );
        let sheets = read_sheets(&content).unwrap();
        assert!(sheets[0].truncated);
        assert_eq!(sheets[0].rows.len(), MAX_SHEET_ROWS);
        assert!(read_ods(b"not a zip").is_err());
    }
}
