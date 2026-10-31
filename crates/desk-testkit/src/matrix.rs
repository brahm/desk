// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The parity matrix: the rows of the survey of Pages '09, each with the
//! phase of the roadmap that completes it, as `corpus/matrix.tsv` holds
//! them.

use std::fmt;

use serde::Serialize;

/// A row of the matrix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Row {
    /// The ID, such as `FMT-15`.
    pub id: String,
    /// The phase of the roadmap that completes the row, or `None` when the
    /// row is out of scope.
    pub phase: Option<u8>,
    /// The feature.
    pub feature: String,
    /// What Pages does.
    pub expected: String,
    /// The pages of the user guide that describe it.
    pub guide_pages: String,
    /// How it is kept in ODF.
    pub odf: String,
}

/// Why the matrix could not be read.
#[derive(Debug, PartialEq, Eq)]
pub struct MatrixError {
    /// The line, from 1.
    pub line: usize,
    /// What is wrong with it.
    pub message: String,
}

impl fmt::Display for MatrixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for MatrixError {}

const HEADER: &str = "id\tphase\tfeature\texpected\tpages\todf";

/// Reads the matrix. Lines that start with `#` are comments, and the
/// first other line is the header.
pub fn parse(text: &str) -> Result<Vec<Row>, MatrixError> {
    let mut rows = Vec::new();
    let mut header = false;
    for (i, line) in text.lines().enumerate() {
        let error = |message: String| MatrixError {
            line: i + 1,
            message,
        };
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if !header {
            if line != HEADER {
                return Err(error(format!("the header must be {HEADER:?}")));
            }
            header = true;
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let [id, phase, feature, expected, pages, odf] = fields[..] else {
            return Err(error(format!("{} fields instead of 6", fields.len())));
        };
        if !is_id(id) {
            return Err(error(format!("{id:?} is not an ID like FMT-15")));
        }
        if rows.iter().any(|r: &Row| r.id == id) {
            return Err(error(format!("{id} is listed twice")));
        }
        let phase = match phase {
            "-" => None,
            p => Some(
                p.parse()
                    .map_err(|_| error(format!("{p:?} is not a phase")))?,
            ),
        };
        rows.push(Row {
            id: id.into(),
            phase,
            feature: feature.into(),
            expected: expected.into(),
            guide_pages: pages.into(),
            odf: odf.into(),
        });
    }
    Ok(rows)
}

/// Whether a text is a row ID: three capitals, a dash and two digits.
pub fn is_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 6
        && bytes[..3].iter().all(u8::is_ascii_uppercase)
        && bytes[3] == b'-'
        && bytes[4..].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_rows_and_phases() {
        let text = "# comment\nid\tphase\tfeature\texpected\tpages\todf\n\
                    FMT-15\t1\tEspaço\tO maior vale\t72\tNativo\n\
                    EXP-11\t-\tiWork.com\tFechado\t200\t-\n";
        let rows = parse(text).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "FMT-15");
        assert_eq!(rows[0].phase, Some(1));
        assert_eq!(rows[1].phase, None);
    }

    #[test]
    fn refuses_bad_lines() {
        let header = "id\tphase\tfeature\texpected\tpages\todf\n";
        assert_eq!(parse("id\tphase\n").unwrap_err().line, 1);
        let short = format!("{header}FMT-15\t1\n");
        assert_eq!(parse(&short).unwrap_err().line, 2);
        let twice = format!("{header}FMT-15\t1\ta\tb\tc\td\nFMT-15\t1\ta\tb\tc\td\n");
        assert_eq!(parse(&twice).unwrap_err().line, 3);
        assert!(!is_id("FMT-1"));
        assert!(!is_id("fmt-15"));
    }

    #[test]
    fn the_matrix_of_the_corpus_reads() {
        let text = include_str!("../../../corpus/matrix.tsv");
        let rows = parse(text).unwrap();
        assert_eq!(rows.len(), 271);
        let pages = rows.iter().filter(|r| !r.id.starts_with("IOP")).count();
        assert_eq!(pages, 260);
        assert!(rows.iter().all(|r| r.phase.is_none_or(|p| p <= 11)));
    }
}
