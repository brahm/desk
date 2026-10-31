// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Running the corpus and the parity report.
//!
//! Each case is laid out and written as PDF by Desk, then compared with
//! the PDF of Pages when there is one. When Pages also saved the case as a
//! document, Desk imports that document, lays it out and compares it with
//! the same PDF, which checks the reader of `.pages` files (DOC-22). The report puts each row of the
//! matrix next to the cases that check it and the tests named after it,
//! in Markdown for people and in JSON for tools.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

use desk_model::Document;

use crate::case::{Case, DEFAULT_POSITION_TOLERANCE};
use crate::compare::{LayoutDiff, PixelDiff, compare_pixels, compare_words, words};
use crate::matrix::Row;

/// What became of a case.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    /// Desk laid it out, and there is no PDF of Pages to compare with yet.
    NoReference {
        /// The pages Desk made.
        pages: usize,
    },
    /// It matches the reference.
    Pass {
        /// How the words compare.
        layout: LayoutDiff,
        /// How the pixels compare.
        pixels: PixelDiff,
    },
    /// It differs from the reference.
    Fail {
        /// Why, in one sentence.
        reason: String,
        /// How the words compare.
        layout: LayoutDiff,
        /// How the pixels compare, when they could be.
        pixels: Option<PixelDiff>,
    },
    /// It could not be laid out, written or compared.
    Error {
        /// What went wrong.
        message: String,
    },
}

impl Outcome {
    /// Whether the case failed or could not run.
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Fail { .. } | Self::Error { .. })
    }

    /// The outcome in a few words, in Portuguese as the report.
    pub fn label(&self) -> &'static str {
        match self {
            Self::NoReference { .. } => "sem referência",
            Self::Pass { .. } => "igual ao Pages",
            Self::Fail { .. } => "diferente do Pages",
            Self::Error { .. } => "erro",
        }
    }
}

/// The result of one case.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CaseResult {
    /// The case, by the name of its folder.
    pub name: String,
    /// Its title.
    pub title: String,
    /// The rows it checks.
    pub rows: Vec<String>,
    /// The PDF that Desk wrote.
    pub pdf: PathBuf,
    /// What became of it.
    #[serde(flatten)]
    pub outcome: Outcome,
    /// What became of the document that Pages saved for it, imported, when
    /// there is one.
    pub from_pages: Option<PagesResult>,
}

impl CaseResult {
    /// Whether the case, or the document of Pages imported, failed or
    /// could not run.
    pub fn is_failure(&self) -> bool {
        self.outcome.is_failure()
            || self
                .from_pages
                .as_ref()
                .is_some_and(|p| p.outcome.is_failure())
    }
}

/// The result of importing the document of a case that Pages saved.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PagesResult {
    /// The PDF that Desk wrote from it.
    pub pdf: PathBuf,
    /// What became of it.
    #[serde(flatten)]
    pub outcome: Outcome,
}

/// Lays out a case, writes its PDF into `out`, and compares it with the
/// reference when there is one.
pub fn run_case(case: &Case, out: &Path) -> CaseResult {
    let out = out.join(&case.name);
    let pdf = out.join("desk.pdf");
    let outcome = match case.spec.document() {
        Ok(document) => run(case, &document, &out, &pdf),
        Err(error) => Err(error.to_string()),
    }
    .unwrap_or_else(|message| Outcome::Error { message });
    let from_pages = case.pages_document().map(|path| {
        let out = out.join("from-pages");
        let pdf = out.join("desk.pdf");
        let outcome = match desk_filters::read_pages_at(&path) {
            Ok(document) => run(case, &document, &out, &pdf),
            Err(error) => Err(format!("o documento do Pages não abriu: {error}")),
        }
        .unwrap_or_else(|message| Outcome::Error { message });
        PagesResult { pdf, outcome }
    });
    CaseResult {
        name: case.name.clone(),
        title: case.spec.title.clone(),
        rows: case.spec.rows.clone(),
        pdf,
        outcome,
        from_pages,
    }
}

fn run(case: &Case, document: &Document, out: &Path, pdf: &Path) -> Result<Outcome, String> {
    let layout = desk_layout::paginate(document, &mut desk_text::TextSystem::new());
    let pages = desk_render::DisplayList::pages(&layout);
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    desk_render::Painter::new()
        .and_then(|mut painter| painter.write_pdf(&pages, document.metadata(), pdf))
        .map_err(|e| e.to_string())?;
    let Some(reference) = case.reference() else {
        return Ok(Outcome::NoReference { pages: pages.len() });
    };
    let reference_words = words(&reference).map_err(|e| e.to_string())?;
    let desk_words = words(pdf).map_err(|e| e.to_string())?;
    let layout = compare_words(&reference_words, &desk_words);
    let tolerance = case
        .spec
        .position_tolerance
        .unwrap_or(DEFAULT_POSITION_TOLERANCE);
    let pixels = compare_pixels(&reference, pdf, out);
    let reason = if layout.reference_pages != layout.desk_pages {
        Some(format!(
            "Desk fez {} páginas e o Pages, {}",
            layout.desk_pages, layout.reference_pages
        ))
    } else if let Some(m) = &layout.first_mismatch {
        Some(format!(
            "na página {}, a palavra {} é {:?} no Pages e {:?} no Desk",
            m.page,
            m.index + 1,
            m.reference.as_deref().unwrap_or("(nenhuma)"),
            m.desk.as_deref().unwrap_or("(nenhuma)")
        ))
    } else if layout.max_offset > tolerance {
        Some(format!(
            "uma palavra está a {:.2} pt de onde o Pages a pôs, e o limite é {tolerance} pt",
            layout.max_offset
        ))
    } else {
        None
    };
    let pixels = match pixels {
        Ok(pixels) => pixels,
        Err(error) if reason.is_some() => {
            return Ok(Outcome::Fail {
                reason: format!(
                    "{}; e os pixels não foram comparados: {error}",
                    reason.unwrap()
                ),
                layout,
                pixels: None,
            });
        }
        Err(error) => return Err(error.to_string()),
    };
    let reason = reason.or_else(|| {
        let limit = case.spec.pixel_tolerance?;
        (pixels.worst() > limit).then(|| {
            format!(
                "{:.2}% dos pixels de uma página diferem, e o limite é {limit}%",
                pixels.worst()
            )
        })
    });
    Ok(match reason {
        Some(reason) => Outcome::Fail {
            reason,
            layout,
            pixels: Some(pixels),
        },
        None => Outcome::Pass { layout, pixels },
    })
}

/// Where a row of the matrix stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Its cases match Pages.
    Matches,
    /// One of its cases differs from Pages, or could not run.
    Differs,
    /// It has cases, still without a PDF of Pages.
    AwaitsReference,
    /// Only tests check it.
    Tested,
    /// Nothing checks it yet.
    Pending,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Self::Matches => "igual ao Pages",
            Self::Differs => "diferente do Pages",
            Self::AwaitsReference => "sem referência",
            Self::Tested => "só testes",
            Self::Pending => "pendente",
        }
    }

    const ALL: [Self; 5] = [
        Self::Matches,
        Self::Differs,
        Self::AwaitsReference,
        Self::Tested,
        Self::Pending,
    ];
}

/// A row of the matrix, with what checks it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RowReport {
    /// The row.
    #[serde(flatten)]
    pub row: Row,
    /// Where it stands.
    pub status: Status,
    /// The cases that check it.
    pub cases: Vec<String>,
    /// The tests named after it.
    pub tests: Vec<String>,
}

/// The parity report.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Report {
    /// The cases, in name order.
    pub cases: Vec<CaseResult>,
    /// The rows of the matrix, in its order.
    pub rows: Vec<RowReport>,
    /// Rows that cases name but the matrix does not have.
    pub unknown_rows: Vec<String>,
}

impl Report {
    /// Builds the report from the results of the cases and the tests
    /// named after each row.
    pub fn new(
        matrix: Vec<Row>,
        cases: Vec<CaseResult>,
        tests: &BTreeMap<String, Vec<String>>,
    ) -> Self {
        let rows = matrix
            .into_iter()
            .map(|row| {
                let checking: Vec<&CaseResult> =
                    cases.iter().filter(|c| c.rows.contains(&row.id)).collect();
                let tests = tests.get(&row.id).cloned().unwrap_or_default();
                let status = if checking.iter().any(|c| c.is_failure()) {
                    Status::Differs
                } else if checking
                    .iter()
                    .any(|c| matches!(c.outcome, Outcome::Pass { .. }))
                {
                    Status::Matches
                } else if !checking.is_empty() {
                    Status::AwaitsReference
                } else if !tests.is_empty() {
                    Status::Tested
                } else {
                    Status::Pending
                };
                RowReport {
                    status,
                    cases: checking.iter().map(|c| c.name.clone()).collect(),
                    tests,
                    row,
                }
            })
            .collect::<Vec<_>>();
        let mut unknown_rows: Vec<String> = cases
            .iter()
            .flat_map(|c| &c.rows)
            .filter(|id| !rows.iter().any(|r| &r.row.id == *id))
            .cloned()
            .collect();
        unknown_rows.sort();
        unknown_rows.dedup();
        Self {
            cases,
            rows,
            unknown_rows,
        }
    }

    /// Whether a case failed, or names a row the matrix does not have.
    pub fn has_failures(&self) -> bool {
        !self.unknown_rows.is_empty() || self.cases.iter().any(CaseResult::is_failure)
    }

    /// The report as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("the report serializes")
    }

    /// The report as Markdown, with paths relative to `base`.
    pub fn to_markdown(&self, base: &Path) -> String {
        let mut md = String::new();
        let relative = |p: &Path| p.strip_prefix(base).unwrap_or(p).display().to_string();
        md.push_str("# Paridade do Desk Writer com o Pages '09\n\n");

        md.push_str("## Por fase\n\n| Fase |");
        for status in Status::ALL {
            let _ = write!(md, " {} |", status.label());
        }
        md.push_str(" Linhas |\n|---|");
        md.push_str(&"---:|".repeat(Status::ALL.len() + 1));
        md.push('\n');
        // Out of scope last.
        let mut phases: BTreeMap<u16, Vec<&RowReport>> = BTreeMap::new();
        for row in &self.rows {
            let key = row.row.phase.map_or(u16::MAX, u16::from);
            phases.entry(key).or_default().push(row);
        }
        let count =
            |rows: &[&RowReport], status| rows.iter().filter(|r| r.status == status).count();
        let all: Vec<&RowReport> = self.rows.iter().collect();
        for (&phase, rows) in &phases {
            let name = if phase == u16::MAX {
                "fora do escopo".to_owned()
            } else {
                phase.to_string()
            };
            let _ = write!(md, "| {name} |");
            for status in Status::ALL {
                let _ = write!(md, " {} |", count(rows, status));
            }
            let _ = writeln!(md, " {} |", rows.len());
        }
        let _ = write!(md, "| **total** |");
        for status in Status::ALL {
            let _ = write!(md, " **{}** |", count(&all, status));
        }
        let _ = writeln!(md, " **{}** |\n", all.len());

        md.push_str("## Casos\n\n");
        if self.cases.is_empty() {
            md.push_str("Nenhum caso no corpus.\n\n");
        }
        for case in &self.cases {
            let _ = writeln!(
                md,
                "- **{}** ({}): {}. Linhas: {}. PDF do Desk: `{}`.",
                case.name,
                case.title,
                case.outcome.label(),
                case.rows.join(", "),
                relative(&case.pdf)
            );
            details(&mut md, &case.outcome, &relative);
            if let Some(pages) = &case.from_pages {
                let _ = writeln!(
                    md,
                    "  Do documento do Pages: {}. PDF do Desk: `{}`.",
                    pages.outcome.label(),
                    relative(&pages.pdf)
                );
                details(&mut md, &pages.outcome, &relative);
            }
        }
        if !self.unknown_rows.is_empty() {
            let _ = writeln!(
                md,
                "\nLinhas que os casos citam e a matriz não tem: {}.",
                self.unknown_rows.join(", ")
            );
        }

        md.push_str("\n## Linhas\n\n| ID | Fase | Funcionalidade | Situação | Casos | Testes |\n|---|---:|---|---|---|---:|\n");
        for row in &self.rows {
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} |",
                row.row.id,
                row.row.phase.map_or("-".to_owned(), |p| p.to_string()),
                row.row.feature.replace('|', "\\|"),
                row.status.label(),
                row.cases.join(", "),
                row.tests.len()
            );
        }
        md
    }
}

/// The details of an outcome, under its line in the list of cases.
fn details(md: &mut String, outcome: &Outcome, relative: &dyn Fn(&Path) -> String) {
    match outcome {
        Outcome::NoReference { pages } => {
            let _ = writeln!(md, "  O Desk fez {pages} página(s).");
        }
        Outcome::Pass { layout, pixels } => {
            let _ = writeln!(
                md,
                "  {} página(s); a maior distância de uma palavra é {:.2} pt; até {:.2}% dos pixels diferem.",
                layout.desk_pages,
                layout.max_offset,
                pixels.worst()
            );
        }
        Outcome::Fail { reason, pixels, .. } => {
            let _ = writeln!(md, "  Motivo: {reason}.");
            if let Some(pixels) = pixels {
                let images: Vec<String> = pixels
                    .images
                    .iter()
                    .map(|p| format!("`{}`", relative(p)))
                    .collect();
                let _ = writeln!(md, "  Diferenças: {}.", images.join(", "));
            }
        }
        Outcome::Error { message } => {
            let _ = writeln!(md, "  Erro: {message}.");
        }
    }
}

/// The tests named after each row: Rust test functions whose names start
/// with the IDs, in lower case, such as `fmt_15_est_04_...` for FMT-15 and
/// EST-04, in the `.rs` files under `crates`.
pub fn tests_by_row(crates: &Path) -> std::io::Result<BTreeMap<String, Vec<String>>> {
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut stack = vec![crates.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "target") {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path)?;
                scan_tests(&text, &mut found);
            }
        }
    }
    for tests in found.values_mut() {
        tests.sort();
        tests.dedup();
    }
    Ok(found)
}

fn scan_tests(text: &str, found: &mut BTreeMap<String, Vec<String>>) {
    let mut test_next = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("#[test]") {
            test_next = true;
            continue;
        }
        if !test_next || line.starts_with("#[") || line.starts_with("//") {
            continue;
        }
        test_next = false;
        let Some(name) = line
            .strip_prefix("fn ")
            .or_else(|| line.strip_prefix("pub fn "))
            .and_then(|rest| rest.split('(').next())
        else {
            continue;
        };
        for id in ids_in_name(name) {
            found.entry(id).or_default().push(name.to_owned());
        }
    }
}

/// The IDs at the start of a test name, such as FMT-15 and EST-04 in
/// `fmt_15_est_04_spacing`.
fn ids_in_name(name: &str) -> Vec<String> {
    let parts: Vec<&str> = name.split('_').collect();
    let mut ids = Vec::new();
    for pair in parts.chunks(2) {
        let [area, number] = pair else { break };
        let id = format!("{}-{number}", area.to_ascii_uppercase());
        if area.len() == 3
            && area.bytes().all(|b| b.is_ascii_lowercase())
            && crate::matrix::is_id(&id)
        {
            ids.push(id);
        } else {
            break;
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_names_give_their_rows() {
        assert_eq!(ids_in_name("fmt_15_est_04_spacing"), ["FMT-15", "EST-04"]);
        assert_eq!(ids_in_name("doc_19_information"), ["DOC-19"]);
        assert!(ids_in_name("lines_flow").is_empty());
        assert!(ids_in_name("an_empty_document").is_empty());

        let mut found = BTreeMap::new();
        scan_tests(
            "#[test]\nfn est_05_widows() {}\n#[test]\n#[ignore]\nfn fmt_12_align() {}\nfn est_04_helper() {}\n",
            &mut found,
        );
        assert_eq!(found["EST-05"], ["est_05_widows"]);
        assert_eq!(found["FMT-12"], ["fmt_12_align"]);
        assert!(!found.contains_key("EST-04"));
    }
}
