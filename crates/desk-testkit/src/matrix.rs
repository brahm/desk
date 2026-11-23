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
    /// The phase that completes the row, or `None` when the row is out of
    /// scope.
    pub phase: Option<Phase>,
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
    /// How Desk adapts the feature to GNOME, where the decisions of
    /// 2026-11-13 and 2026-11-14 changed its form, or empty when Desk does
    /// it as the iWork app does.
    pub gnome: String,
}

/// The phase that completes a row: a number of the old plan, from 1 to
/// 11, for the rows of the surveys of '09, or the id of a phase of the
/// current roadmap, such as `W1`, for the rows written from iWork 15.4.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(untagged)]
pub enum Phase {
    /// A phase of the old plan.
    Old(u8),
    /// A phase of `docs/roadmap.md`, such as `W1` or `T2`.
    Roadmap(String),
}

impl Phase {
    /// The phase of the old plan, if it is one.
    pub fn old(&self) -> Option<u8> {
        match self {
            Self::Old(n) => Some(*n),
            Self::Roadmap(_) => None,
        }
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Old(n) => write!(f, "{n}"),
            Self::Roadmap(id) => f.write_str(id),
        }
    }
}

/// Whether a text is the id of a phase of the roadmap: a capital and a
/// digit, such as `W1`.
fn is_roadmap_phase(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_uppercase() && bytes[1].is_ascii_digit()
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

const HEADER: &str = "id\tphase\tfeature\texpected\tpages\todf\tgnome";

/// The header of the matrices of Desk Slides and Desk Tables, which also
/// say what the other surveys share.
const SHARED_HEADER: &str = "id\tphase\tfeature\texpected\tpages\tshared\todf\tgnome";

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
        let (id, phase, feature, expected, pages, shared, odf, gnome) = match fields[..] {
            [id, phase, feature, expected, pages, odf, gnome] if !shared_column => {
                (id, phase, feature, expected, pages, "", odf, gnome)
            }
            [id, phase, feature, expected, pages, shared, odf, gnome] if shared_column => {
                (id, phase, feature, expected, pages, shared, odf, gnome)
            }
            _ => {
                let want = if shared_column { 8 } else { 7 };
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
            p if is_roadmap_phase(p) => Some(Phase::Roadmap(p.into())),
            p => Some(Phase::Old(
                p.parse()
                    .map_err(|_| error(format!("{p:?} is not a phase")))?,
            )),
        };
        rows.push(Row {
            id: id.into(),
            phase,
            feature: feature.into(),
            expected: expected.into(),
            guide_pages: pages.into(),
            shared: shared.into(),
            odf: odf.into(),
            gnome: gnome.into(),
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
        let text = "# comment\nid\tphase\tfeature\texpected\tpages\todf\tgnome\n\
                    FMT-15\t1\tSpace\tThe larger wins\t72\tNative\t\n\
                    EXP-11\t-\tiWork.com\tShut down\t200\t-\tOut\n\
                    PXV-06\tW1\tDrop cap\t3 lines\t15.4\tNative\t\n";
        let rows = parse(text).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].id, "FMT-15");
        assert_eq!(rows[0].phase, Some(Phase::Old(1)));
        assert_eq!(rows[1].phase, None);
        assert_eq!(rows[2].phase, Some(Phase::Roadmap("W1".into())));
        assert_eq!(rows[2].phase.as_ref().unwrap().to_string(), "W1");
        // The old phases sort before those of the roadmap.
        assert!(Phase::Old(11) < Phase::Roadmap("B1".into()));
        assert_eq!(rows[0].gnome, "");
        assert_eq!(rows[1].gnome, "Out");
    }

    #[test]
    fn refuses_bad_lines() {
        let header = "id\tphase\tfeature\texpected\tpages\todf\tgnome\n";
        assert_eq!(parse("id\tphase\n").unwrap_err().line, 1);
        let short = format!("{header}FMT-15\t1\n");
        assert_eq!(parse(&short).unwrap_err().line, 2);
        let twice = format!("{header}FMT-15\t1\ta\tb\tc\td\t\nFMT-15\t1\ta\tb\tc\td\t\n");
        assert_eq!(parse(&twice).unwrap_err().line, 3);
        for phase in ["W", "W12", "w1", "1W"] {
            let bad = format!("{header}FMT-15\t{phase}\ta\tb\tc\td\t\n");
            assert_eq!(parse(&bad).unwrap_err().line, 2, "{phase}");
        }
        assert!(!is_id("FMT-1"));
        assert!(!is_id("fmt-15"));
    }

    #[test]
    fn the_matrix_of_the_corpus_reads() {
        let text = include_str!("../../../corpus/matrix.tsv");
        let rows = parse(text).unwrap();
        assert_eq!(rows.len(), 330);
        let count = |prefix: &str| rows.iter().filter(|r| r.id.starts_with(prefix)).count();
        assert_eq!(count("IOP"), 33);
        assert_eq!(count("GNM"), 14);
        assert_eq!(count("VMI"), 4);
        // The features of Pages 15, written from Pages 15.4 (D-020).
        assert_eq!(count("PXV"), 19);
        let survey = rows.len() - count("IOP") - count("GNM") - count("VMI") - count("PXV");
        assert_eq!(survey, 260);
        let roadmap = include_str!("../../../docs/roadmap.md");
        for row in &rows {
            match &row.phase {
                Some(Phase::Old(p)) => assert!(*p <= 11, "{}", row.id),
                Some(Phase::Roadmap(id)) => {
                    // The features of Pages 15, and the rows of review written
                    // again from Pages 15.4 (W2).
                    assert!(
                        row.id.starts_with("PXV") || row.id.starts_with("REV"),
                        "{}",
                        row.id
                    );
                    assert!(
                        roadmap.contains(&format!("| **{id}** ")),
                        "{} names {id}",
                        row.id
                    );
                }
                None => {}
            }
        }
        let gnome = |id: &str| &rows.iter().find(|r| r.id == id).unwrap().gnome;
        // The decisions of 2026-11-13 that still stand: one Inspector and a
        // header bar without customization.
        assert!(gnome("INT-13").contains("One Inspector"));
        assert!(gnome("INT-11").contains("No customization"));
        // The decisions of 2026-11-16: automatic saving with versions, and
        // window M+, with the objects in the header bar, the Format and
        // Document choice in the Inspector and no format bar.
        assert!(gnome("DOC-11").contains("Automatic saving and versions"));
        assert!(gnome("INT-11").contains("six object buttons"));
        assert!(gnome("INT-13").contains("Format and Document"));
        assert!(gnome("INT-12").contains("no format bar"));
        assert!(gnome("INT-18").contains("navigation sidebar"));
    }

    #[test]
    fn reads_the_shared_column() {
        let text = "id\tphase\tfeature\texpected\tpages\tshared\todf\tgnome\n\
                    KSLD-04\t2\tGroups\tCollapses\t50\tKeynote only\tExtension\t\n";
        let rows = parse(text).unwrap();
        assert_eq!(rows[0].id, "KSLD-04");
        assert_eq!(rows[0].shared, "Keynote only");
        assert_eq!(rows[0].odf, "Extension");
        let six = "id\tphase\tfeature\texpected\tpages\tshared\todf\tgnome\n\
                   KSLD-04\t2\ta\tb\tc\td\te\n";
        assert_eq!(parse(six).unwrap_err().line, 2);
        let writer = "id\tphase\tfeature\texpected\tpages\todf\tgnome\nKSLD-04\t2\ta\tb\tc\td\t\n";
        assert_eq!(parse(writer).unwrap_err().line, 2);
        assert!(is_app_id("NCEL-12"));
        assert!(!is_app_id("XCEL-12"));
        assert_eq!(
            ids_in("INT-14 to INT-16, NDOC-02"),
            ["INT-14", "INT-16", "NDOC-02"]
        );
    }

    #[test]
    fn the_matrices_of_slides_and_tables_read_and_name_rows_that_exist() {
        let writer = parse(include_str!("../../../corpus/matrix.tsv")).unwrap();
        let slides = parse(include_str!("../../../corpus/slides-matrix.tsv")).unwrap();
        let tables = parse(include_str!("../../../corpus/tables-matrix.tsv")).unwrap();
        assert_eq!(slides.len(), 181);
        assert_eq!(tables.len(), 181);
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
            ("KGNM", 15),
            ("KVMI", 5),
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
            ("NGNM", 14),
            ("NVMI", 3),
        ] {
            assert_eq!(count(&tables, area), n, "{area}");
        }
        let all: Vec<&Row> = writer.iter().chain(&slides).chain(&tables).collect();
        for row in slides.iter().chain(&tables) {
            let old = row.phase.as_ref().map(|p| p.old());
            assert!(
                old.is_none_or(|p| p.is_some_and(|p| (1..=9).contains(&p))),
                "{}",
                row.id
            );
            assert!(!row.shared.is_empty(), "{}", row.id);
            for id in ids_in(&row.shared) {
                assert!(all.iter().any(|r| r.id == id), "{} names {id}", row.id);
            }
        }
    }
}
