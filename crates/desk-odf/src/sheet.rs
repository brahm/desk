// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The tables of a spreadsheet (ODS), as text, which a mail merge reads
//! its records from (W105, MAL-05): each sheet by its name, its rows and
//! the text of their cells.

use std::io::{Cursor, Read};

use zip::ZipArchive;
use zip::result::ZipError;

use crate::budget::MAX_DOCUMENT_CELLS;
use crate::error::OdfError;
use crate::ns::{OFFICE, TABLE, TEXT};
use crate::package::MAX_STREAM;
use crate::table::{MAX_COLUMNS, MAX_ROWS};
use crate::xml::Element;

/// The media type of a spreadsheet.
pub const SPREADSHEET_TYPE: &str = "application/vnd.oasis.opendocument.spreadsheet";

/// A sheet of a spreadsheet, as text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    /// `table:name`.
    pub name: String,
    /// The rows, each the text of its cells, without the empty cells at
    /// the end of a row or the empty rows at the end of the sheet. The
    /// paragraphs of a cell are lines of its text.
    pub rows: Vec<Vec<String>>,
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

/// Reads the sheets of the `content.xml` of a spreadsheet.
pub fn read_sheets(content: &str) -> Result<Vec<Sheet>, OdfError> {
    let root = crate::xml::parse(content)?;
    let Some(spreadsheet) = root
        .child(OFFICE, "body")
        .and_then(|b| b.child(OFFICE, "spreadsheet"))
    else {
        return Err(OdfError::Missing("office:spreadsheet"));
    };
    let mut budget = MAX_DOCUMENT_CELLS;
    Ok(spreadsheet
        .children_named(TABLE, "table")
        .map(|table| {
            let mut rows = Vec::new();
            read_rows(table, &mut rows, &mut budget);
            while rows.last().is_some_and(|r: &Vec<String>| r.is_empty()) {
                rows.pop();
            }
            Sheet {
                name: table.attr(TABLE, "name").unwrap_or_default().to_owned(),
                rows,
            }
        })
        .collect())
}

/// How many times an element repeats, by its attribute, from 1 to `most`.
fn repeated(e: &Element, name: &str, most: usize) -> usize {
    e.attr(TABLE, name)
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, most.max(1))
}

/// The rows of a table, or of a group of rows or of its header rows.
fn read_rows(e: &Element, rows: &mut Vec<Vec<String>>, budget: &mut usize) {
    for child in e.elements() {
        if child.name.ns != TABLE {
            continue;
        }
        match child.name.local.as_str() {
            "table-header-rows" | "table-row-group" | "table-rows" => {
                read_rows(child, rows, budget);
            }
            "table-row" => {
                let row = read_row(child, budget);
                // Spreadsheets fill the sheet with repeated empty rows;
                // those at the end are taken away after.
                let times = repeated(child, "number-rows-repeated", MAX_ROWS);
                let times = if row.is_empty() {
                    times.min(MAX_ROWS.saturating_sub(rows.len()))
                } else {
                    times.min(*budget / row.len().max(1) + 1)
                };
                for _ in 0..times {
                    if rows.len() >= MAX_ROWS {
                        return;
                    }
                    if !row.is_empty() {
                        if *budget < row.len() {
                            return;
                        }
                        *budget -= row.len();
                    }
                    rows.push(row.clone());
                }
            }
            _ => {}
        }
    }
}

/// The text of the cells of a row, without the empty ones at its end.
fn read_row(row: &Element, budget: &mut usize) -> Vec<String> {
    let mut cells = Vec::new();
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
        let times = repeated(cell, "number-columns-repeated", MAX_COLUMNS);
        let times = if text.is_empty() {
            times.min(MAX_COLUMNS.saturating_sub(cells.len()))
        } else {
            times.min(*budget + 1)
        };
        for _ in 0..times {
            if cells.len() >= MAX_COLUMNS {
                break;
            }
            cells.push(text.clone());
        }
    }
    while cells.last().is_some_and(String::is_empty) {
        cells.pop();
    }
    cells
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
        assert_eq!(
            sheets[1],
            Sheet {
                name: "Empty".into(),
                rows: Vec::new()
            }
        );
        assert!(read_sheets("<x/>").is_err());
        assert!(read_ods(b"not a zip").is_err());
    }
}
