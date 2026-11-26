// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The records of a source of a mail merge, as a table: the names of its
//! fields and, for each record, its data (MAL-04, MAL-05).

use desk_model::MergeSource;

use crate::vcard;

/// The most bytes of a source read.
pub const MAX_SOURCE_BYTES: usize = 64 << 20;

/// Why the records of a source could not be read.
#[derive(Debug)]
pub enum SourceError {
    /// The file could not be read.
    Io(std::io::Error),
    /// The file is not a spreadsheet that can be read.
    Spreadsheet(desk_odf::OdfError),
    /// The spreadsheet has no table of the name.
    NoTable(String),
    /// The file is larger than [`MAX_SOURCE_BYTES`].
    TooLarge,
    /// The contacts of the desktop are read by the window, from
    /// Evolution Data Server.
    Desktop,
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Spreadsheet(error) => write!(f, "{error}"),
            Self::NoTable(name) => write!(f, "the spreadsheet has no table named {name}"),
            Self::TooLarge => write!(f, "the file is too large"),
            Self::Desktop => write!(f, "the contacts of the desktop are read by the window"),
        }
    }
}

impl std::error::Error for SourceError {}

/// The records of a source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Records {
    /// The names of the fields: those of a contact, or the column headers
    /// of a table.
    pub fields: Vec<String>,
    /// The data of each record, in the order of the fields; empty for a
    /// field the record has no datum for.
    pub rows: Vec<Vec<String>>,
    /// The names of the tables of the spreadsheet, for its source to
    /// choose from; none for other sources.
    pub tables: Vec<String>,
}

impl Records {
    /// The records of cards: every field of a contact.
    pub fn of_cards(cards: &[vcard::Card]) -> Self {
        Self {
            fields: crate::CONTACT_FIELDS
                .iter()
                .map(|f| (*f).to_owned())
                .collect(),
            rows: cards
                .iter()
                .map(|card| {
                    crate::CONTACT_FIELDS
                        .iter()
                        .map(|f| card.get(f).unwrap_or_default())
                        .collect()
                })
                .collect(),
            tables: Vec::new(),
        }
    }

    /// The records of a table whose first row names its fields; columns
    /// without a name are left out.
    pub fn of_table(rows: &[Vec<String>]) -> Self {
        let Some((header, rows)) = rows.split_first() else {
            return Self::default();
        };
        let columns: Vec<(usize, String)> = header
            .iter()
            .enumerate()
            .map(|(i, name)| (i, name.trim().to_owned()))
            .filter(|(_, name)| !name.is_empty())
            .collect();
        Self {
            fields: columns.iter().map(|(_, name)| name.clone()).collect(),
            rows: rows
                .iter()
                .filter(|row| row.iter().any(|c| !c.trim().is_empty()))
                .map(|row| {
                    columns
                        .iter()
                        .map(|(i, _)| row.get(*i).cloned().unwrap_or_default())
                        .collect()
                })
                .collect(),
            tables: Vec::new(),
        }
    }

    /// The column of a field: its name in any case, or the column of
    /// another name of the same field of a contact (MAL-09).
    pub fn column(&self, field: &str) -> Option<usize> {
        let field = field.trim();
        self.fields
            .iter()
            .position(|f| f.trim().eq_ignore_ascii_case(field))
            .or_else(|| {
                let wanted = crate::canonical(field)?;
                self.fields
                    .iter()
                    .position(|f| crate::canonical(f) == Some(wanted))
            })
    }

    /// The datum of a field of a record, when it has one.
    pub fn value(&self, record: usize, field: &str) -> Option<&str> {
        let column = self.column(field)?;
        self.rows
            .get(record)?
            .get(column)
            .map(String::as_str)
            .filter(|v| !v.trim().is_empty())
    }

    /// How many records there are.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether there is no record.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Text in UTF-8, or else in Windows-1252, as spreadsheets and old
/// address books export it; a byte order mark says UTF-16.
pub fn decode(bytes: &[u8]) -> String {
    if let Some((encoding, _)) = encoding_rs::Encoding::for_bom(bytes) {
        return encoding.decode(bytes).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// Whether a file is a spreadsheet of ODF, by its name.
fn is_ods(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower.ends_with(".ods") || lower.ends_with(".ots") || lower.ends_with(".fods")
}

/// The records of a source, whose file `read` reads; the caller decides
/// whether a file that the document names may be read.
pub fn read_source(
    source: &MergeSource,
    read: impl FnOnce(&str) -> std::io::Result<Vec<u8>>,
) -> Result<Records, SourceError> {
    let file = match source {
        MergeSource::Contacts { .. } => return Err(SourceError::Desktop),
        MergeSource::VCard { file } | MergeSource::Spreadsheet { file, .. } => file,
    };
    let bytes = read(file).map_err(SourceError::Io)?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(SourceError::TooLarge);
    }
    match source {
        MergeSource::VCard { .. } => Ok(Records::of_cards(&vcard::parse(&decode(&bytes)))),
        MergeSource::Spreadsheet { table, .. } if is_ods(file) => {
            let sheets = if file.to_ascii_lowercase().ends_with(".fods") {
                let content = decode(&bytes);
                desk_odf::read_sheets(&content)
            } else {
                desk_odf::read_ods(&bytes)
            }
            .map_err(SourceError::Spreadsheet)?;
            let sheet = match table.as_deref() {
                Some(name) => sheets
                    .iter()
                    .find(|s| s.name == name)
                    .ok_or_else(|| SourceError::NoTable(name.to_owned()))?,
                // The first table with rows.
                None => match sheets.iter().find(|s| !s.rows.is_empty()) {
                    Some(sheet) => sheet,
                    None => {
                        return Ok(Records {
                            tables: sheets.iter().map(|s| s.name.clone()).collect(),
                            ..Records::default()
                        });
                    }
                },
            };
            Ok(Records {
                tables: sheets.iter().map(|s| s.name.clone()).collect(),
                ..Records::of_table(&sheet.rows)
            })
        }
        // Any other file is text of separated values.
        _ => Ok(Records::of_table(&crate::csv::parse(&decode(&bytes)))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mal_05_a_table_names_its_fields_in_its_first_row() {
        let rows = crate::csv::parse("Nome,,Cidade,E-mail\nAna,x,Lisboa,ana@x\n,,,\nBo,,,\n");
        let records = Records::of_table(&rows);
        assert_eq!(records.fields, ["Nome", "Cidade", "E-mail"]);
        assert_eq!(records.len(), 2);
        assert_eq!(records.value(0, "Cidade"), Some("Lisboa"));
        // A field of a contact finds the column of another of its names.
        assert_eq!(records.value(0, "First Name"), Some("Ana"));
        assert_eq!(records.value(0, "Email"), Some("ana@x"));
        assert_eq!(records.value(1, "City"), None);
        assert_eq!(records.value(5, "Nome"), None);
        assert_eq!(records.value(0, "Phone"), None);
    }

    #[test]
    fn mal_04_the_records_of_a_vcard_file_and_of_a_csv_file() {
        let source = MergeSource::VCard {
            file: "/x/cards.vcf".into(),
        };
        let records = read_source(&source, |file| {
            assert_eq!(file, "/x/cards.vcf");
            Ok(b"BEGIN:VCARD\nN:Lee;Bo\nEND:VCARD\nBEGIN:VCARD\nN:Souza;Ana\nEND:VCARD\n".to_vec())
        })
        .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records.value(1, "First Name"), Some("Ana"));
        assert_eq!(records.value(1, "Name"), Some("Ana Souza"));
        let source = MergeSource::Spreadsheet {
            file: "/x/list.csv".into(),
            table: None,
        };
        // Windows-1252, as an old spreadsheet exports it.
        let records = read_source(&source, |_| {
            Ok(b"Nome;Cidade\nJo\xe3o;Bras\xedlia\n".to_vec())
        })
        .unwrap();
        assert_eq!(records.value(0, "Nome"), Some("João"));
        assert_eq!(records.value(0, "Cidade"), Some("Brasília"));
        assert!(matches!(
            read_source(&MergeSource::Contacts { group: None }, |_| Ok(Vec::new())),
            Err(SourceError::Desktop)
        ));
        assert!(matches!(
            read_source(&source, |_| Err(std::io::Error::other("gone"))),
            Err(SourceError::Io(_))
        ));
    }
}
