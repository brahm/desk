// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The records of a source of a mail merge, as a table: the names of its
//! fields and, for each record, its data (MAL-04, MAL-05).

use std::io::Read;

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
    /// The file is of a kind that cannot be read, as a spreadsheet of
    /// another program.
    Unsupported,
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Spreadsheet(error) => write!(f, "{error}"),
            Self::NoTable(name) => write!(f, "the spreadsheet has no table named {name}"),
            Self::TooLarge => write!(f, "the file is too large"),
            Self::Desktop => write!(f, "the contacts of the desktop are read by the window"),
            Self::Unsupported => write!(f, "the file is not a vCard, ODS or CSV file"),
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
    /// Whether records were left out, past the limits of a source.
    pub truncated: bool,
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
            truncated: cards.len() >= vcard::MAX_CARDS,
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
            // A row with no datum in a named column is no record.
            rows: rows
                .iter()
                .map(|row| {
                    columns
                        .iter()
                        .map(|(i, _)| row.get(*i).cloned().unwrap_or_default())
                        .collect::<Vec<_>>()
                })
                .filter(|row| row.iter().any(|c| !c.trim().is_empty()))
                .collect(),
            tables: Vec::new(),
            truncated: false,
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

    /// The datum that a merge field of a record takes: that of its field,
    /// or, kept from Pages '09 (MAL-04, D-003), when a work, home or
    /// other phone, email or address is missing, another of its kind, as
    /// the work address for the home one.
    pub fn datum(&self, record: usize, field: &str) -> Option<&str> {
        if let Some(value) = self.value(record, field) {
            return Some(value);
        }
        let canonical = crate::canonical(field)?;
        let kind = ["Work ", "Home ", "Other "]
            .iter()
            .find_map(|variant| canonical.strip_prefix(variant))?;
        if !matches!(kind, "Phone" | "Email" | "Address") {
            return None;
        }
        std::iter::once(kind.to_owned())
            .chain(["Work ", "Home ", "Other "].map(|v| format!("{v}{kind}")))
            .find_map(|other| self.value(record, &other))
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

/// What a file of a spreadsheet is, by the end of its name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ods,
    FlatOds,
    Text,
}

fn kind(file: &str) -> Option<Kind> {
    let lower = file.to_ascii_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    match extension {
        "ods" | "ots" => Some(Kind::Ods),
        "fods" => Some(Kind::FlatOds),
        "csv" | "tsv" | "tab" | "txt" => Some(Kind::Text),
        _ => None,
    }
}

/// Whether bytes start as a zip package or a compound file of Office
/// does, which are no text.
fn is_binary(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0])
}

/// The records of a source, whose file `open` opens; the caller decides
/// whether a file that the document names may be opened. At most
/// [`MAX_SOURCE_BYTES`] of it are read.
pub fn read_source<R: Read>(
    source: &MergeSource,
    open: impl FnOnce(&str) -> std::io::Result<R>,
) -> Result<Records, SourceError> {
    let (file, kind) = match source {
        MergeSource::Contacts { .. } => return Err(SourceError::Desktop),
        MergeSource::VCard { file } => (file, None),
        MergeSource::Spreadsheet { file, .. } => {
            (file, Some(kind(file).ok_or(SourceError::Unsupported)?))
        }
    };
    let mut bytes = Vec::new();
    open(file)
        .and_then(|reader| {
            reader
                .take(MAX_SOURCE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(SourceError::Io)?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(SourceError::TooLarge);
    }
    let table = match source {
        MergeSource::Spreadsheet { table, .. } => table.as_deref(),
        _ => None,
    };
    match kind {
        None => Ok(Records::of_cards(&vcard::parse(&decode(&bytes)))),
        Some(Kind::Ods | Kind::FlatOds) => {
            let sheets = if kind == Some(Kind::FlatOds) {
                desk_odf::read_sheets(&decode(&bytes))
            } else {
                desk_odf::read_ods(&bytes)
            }
            .map_err(SourceError::Spreadsheet)?;
            let tables = sheets.iter().map(|s| s.name.clone()).collect();
            let sheet = match table {
                Some(name) => sheets
                    .iter()
                    .find(|s| s.name == name)
                    .ok_or_else(|| SourceError::NoTable(name.to_owned()))?,
                // The first table with rows.
                None => match sheets.iter().find(|s| !s.rows.is_empty()) {
                    Some(sheet) => sheet,
                    None => {
                        return Ok(Records {
                            tables,
                            ..Records::default()
                        });
                    }
                },
            };
            Ok(Records {
                tables,
                truncated: sheet.truncated,
                ..Records::of_table(&sheet.rows)
            })
        }
        Some(Kind::Text) => {
            if is_binary(&bytes) {
                return Err(SourceError::Unsupported);
            }
            let rows = crate::csv::parse(&decode(&bytes));
            Ok(Records {
                truncated: rows.len() >= crate::csv::MAX_ROWS,
                ..Records::of_table(&rows)
            })
        }
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
        // A row with data only in a column without a name is no record.
        assert!(Records::of_table(&crate::csv::parse("Nome,,Cidade\n,note,\n")).is_empty());
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
            Ok(&b"BEGIN:VCARD\nN:Lee;Bo\nEND:VCARD\nBEGIN:VCARD\nN:Souza;Ana\nEND:VCARD\n"[..])
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
        let records =
            read_source(&source, |_| Ok(&b"Nome;Cidade\nJo\xe3o;Bras\xedlia\n"[..])).unwrap();
        assert_eq!(records.value(0, "Nome"), Some("João"));
        assert_eq!(records.value(0, "Cidade"), Some("Brasília"));
        assert!(matches!(
            read_source(&MergeSource::Contacts { group: None }, |_| Ok(&b""[..])),
            Err(SourceError::Desktop)
        ));
        assert!(matches!(
            read_source(&source, |_| Err::<&[u8], _>(std::io::Error::other("gone"))),
            Err(SourceError::Io(_))
        ));
        // A workbook of another program is not read as text.
        let workbook = MergeSource::Spreadsheet {
            file: "/x/list.xlsx".into(),
            table: None,
        };
        assert!(matches!(
            read_source(&workbook, |_| Ok(&b"PK\x03\x04"[..])),
            Err(SourceError::Unsupported)
        ));
        assert!(matches!(
            read_source(&source, |_| Ok(&b"PK\x03\x04 zip"[..])),
            Err(SourceError::Unsupported)
        ));
        // Past the limit, the file is not read whole.
        let large = MergeSource::VCard {
            file: "/x/large.vcf".into(),
        };
        assert!(matches!(
            read_source(&large, |_| Ok(std::io::repeat(b'x'))),
            Err(SourceError::TooLarge)
        ));
    }
}
