// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The parity matrices: the rows of the surveys of Pages, Keynote and
//! Numbers '09, each with the phase of the roadmap that completes it, as
//! `corpus/matrix.tsv` (Desk Writer), `corpus/slides-matrix.tsv` (Desk
//! Slides) and `corpus/tables-matrix.tsv` (Desk Tables) hold them.

use std::fmt;

use serde::Serialize;

/// A row of the matrix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Row {
    /// The ID, such as `FMT-15`, or `KSLD-04` in the matrices of Desk
    /// Slides and Desk Tables.
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
    /// The rows of the other surveys that describe the same feature, or
    /// that the feature is only of Keynote or Numbers; empty in the matrix
    /// of Desk Writer, which has no such column.
    pub shared: String,
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

/// The header of the matrices of Desk Slides and Desk Tables, which also
/// say what the other surveys share.
const SHARED_HEADER: &str = "id\tphase\tfeature\texpected\tpages\tshared\todf";

/// Reads a matrix. Lines that start with `#` are comments, and the first
/// other line is the header.
pub fn parse(text: &str) -> Result<Vec<Row>, MatrixError> {
    let mut rows = Vec::new();
    let mut header = false;
    let mut shared_column = false;
    for (i, line) in text.lines().enumerate() {
        let error = |message: String| MatrixError {
            line: i + 1,
            message,
        };
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if !header {
            shared_column = line == SHARED_HEADER;
            if line != HEADER && !shared_column {
                return Err(error(format!(
                    "the header must be {HEADER:?} or {SHARED_HEADER:?}"
                )));
            }
            header = true;
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let (id, phase, feature, expected, pages, shared, odf) = match fields[..] {
            [id, phase, feature, expected, pages, odf] if !shared_column => {
                (id, phase, feature, expected, pages, "", odf)
            }
            [id, phase, feature, expected, pages, shared, odf] if shared_column => {
                (id, phase, feature, expected, pages, shared, odf)
            }
            _ => {
                let want = if shared_column { 7 } else { 6 };
                return Err(error(format!("{} fields instead of {want}", fields.len())));
            }
        };
        if !(is_id(id) || shared_column && is_app_id(id)) {
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
            shared: shared.into(),
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

/// Whether a text is a row ID of the survey of Keynote or Numbers: K or
/// N, three capitals, a dash and two digits, such as `KSLD-04`.
pub fn is_app_id(text: &str) -> bool {
    matches!(text.as_bytes().first(), Some(b'K' | b'N')) && is_id(&text[1..])
}

/// The row IDs a text names, such as INT-14 and INT-16 in
/// "INT-14 a INT-16", in the order it names them.
pub fn ids_in(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|word| is_id(word) || is_app_id(word))
        .collect()
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
        assert_eq!(rows.len(), 304);
        let count = |prefix: &str| rows.iter().filter(|r| r.id.starts_with(prefix)).count();
        assert_eq!(count("IOP"), 33);
        assert_eq!(count("GNM"), 11);
        assert_eq!(rows.len() - count("IOP") - count("GNM"), 260);
        assert!(rows.iter().all(|r| r.phase.is_none_or(|p| p <= 11)));
    }

    #[test]
    fn reads_the_shared_column() {
        let text = "id\tphase\tfeature\texpected\tpages\tshared\todf\n\
                    KSLD-04\t2\tGrupos\tRecolhe\t50\tSó Keynote\tExtensão\n";
        let rows = parse(text).unwrap();
        assert_eq!(rows[0].id, "KSLD-04");
        assert_eq!(rows[0].shared, "Só Keynote");
        assert_eq!(rows[0].odf, "Extensão");
        let six = "id\tphase\tfeature\texpected\tpages\tshared\todf\n\
                   KSLD-04\t2\ta\tb\tc\td\n";
        assert_eq!(parse(six).unwrap_err().line, 2);
        let writer = "id\tphase\tfeature\texpected\tpages\todf\nKSLD-04\t2\ta\tb\tc\td\n";
        assert_eq!(parse(writer).unwrap_err().line, 2);
        assert!(is_app_id("NCEL-12"));
        assert!(!is_app_id("XCEL-12"));
        assert_eq!(
            ids_in("INT-14 a INT-16, NDOC-02"),
            ["INT-14", "INT-16", "NDOC-02"]
        );
    }

    #[test]
    fn the_matrices_of_slides_and_tables_read_and_name_rows_that_exist() {
        let writer = parse(include_str!("../../../corpus/matrix.tsv")).unwrap();
        let slides = parse(include_str!("../../../corpus/slides-matrix.tsv")).unwrap();
        let tables = parse(include_str!("../../../corpus/tables-matrix.tsv")).unwrap();
        assert_eq!(slides.len(), 161);
        assert_eq!(tables.len(), 164);
        let count =
            |rows: &[Row], prefix: &str| rows.iter().filter(|r| r.id.starts_with(prefix)).count();
        for (area, n) in [
            ("KINT", 18),
            ("KDOC", 17),
            ("KSLD", 15),
            ("KTXT", 13),
            ("KOBJ", 12),
            ("KTRN", 8),
            ("KCMP", 12),
            ("KLNK", 8),
            ("KGRF", 3),
            ("KTAB", 10),
            ("KAPR", 13),
            ("KEXP", 11),
            ("KMES", 21),
        ] {
            assert_eq!(count(&slides, area), n, "{area}");
        }
        for (area, n) in [
            ("NINT", 17),
            ("NDOC", 15),
            ("NTAB", 30),
            ("NCEL", 21),
            ("NEST", 6),
            ("NFOR", 16),
            ("NGRF", 19),
            ("NOBJ", 14),
            ("NCON", 4),
            ("NIMP", 14),
            ("NMOD", 8),
        ] {
            assert_eq!(count(&tables, area), n, "{area}");
        }
        let all: Vec<&Row> = writer.iter().chain(&slides).chain(&tables).collect();
        for row in slides.iter().chain(&tables) {
            assert!(row.phase.is_none_or(|p| (1..=9).contains(&p)), "{}", row.id);
            assert!(!row.shared.is_empty(), "{}", row.id);
            for id in ids_in(&row.shared) {
                assert!(all.iter().any(|r| r.id == id), "{} names {id}", row.id);
            }
        }
    }
}
