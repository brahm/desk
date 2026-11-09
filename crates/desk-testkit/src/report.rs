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
    /// What the import left out, as the Warnings window would list it.
    pub warnings: Vec<String>,
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
        let mut warnings = Vec::new();
        let outcome = match desk_filters::read_pages_at_logged(&path) {
            Ok((document, log)) => {
                warnings = log.entries().iter().map(ToString::to_string).collect();
                run(case, &document, &out, &pdf)
            }
            Err(error) => Err(format!("o documento do Pages não abriu: {error}")),
        }
        .unwrap_or_else(|message| Outcome::Error { message });
        PagesResult {
            pdf,
            outcome,
            warnings,
        }
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
    /// Tests lay out documents that Pages saved and find them as in the
    /// PDF that Pages made of them.
    Compared,
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
            Self::Compared => "comparado com o PDF do Pages",
            Self::AwaitsReference => "sem referência",
            Self::Tested => "só testes",
            Self::Pending => "pendente",
        }
    }

    const ALL: [Self; 6] = [
        Self::Matches,
        Self::Differs,
        Self::Compared,
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
    /// Those of them that compare a document of Pages with its PDF.
    pub compared: Vec<String>,
    /// Those that compare a document of Pages that Desk does not lay out
    /// as Pages did yet.
    pub differing: Vec<String>,
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
    /// Builds the report from the results of the cases, the tests named
    /// after each row, those of them that compare with Pages and those
    /// known to differ from it.
    pub fn new(
        matrix: Vec<Row>,
        cases: Vec<CaseResult>,
        tests: &BTreeMap<String, Vec<String>>,
        compared: &BTreeMap<String, Vec<String>>,
        differing: &BTreeMap<String, Vec<String>>,
    ) -> Self {
        let rows = matrix
            .into_iter()
            .map(|row| {
                let checking: Vec<&CaseResult> =
                    cases.iter().filter(|c| c.rows.contains(&row.id)).collect();
                let tests = tests.get(&row.id).cloned().unwrap_or_default();
                let compared = compared.get(&row.id).cloned().unwrap_or_default();
                let differing = differing.get(&row.id).cloned().unwrap_or_default();
                let status = if checking.iter().any(|c| c.is_failure()) || !differing.is_empty() {
                    Status::Differs
                } else if checking
                    .iter()
                    .any(|c| matches!(c.outcome, Outcome::Pass { .. }))
                {
                    Status::Matches
                } else if !compared.is_empty() {
                    Status::Compared
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
                    compared,
                    differing,
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

    /// What the README says of the matrix: how many rows have tests,
    /// how many of them check what Pages laid out, and how many of those
    /// still differ, or that all of them match.
    pub fn summary(&self) -> String {
        let count =
            |keep: &dyn Fn(Status) -> bool| self.rows.iter().filter(|r| keep(r.status)).count();
        let tested = count(&|s| s != Status::Pending);
        let against = count(&|s| matches!(s, Status::Matches | Status::Compared | Status::Differs));
        let differ = count(&|s| s == Status::Differs);
        let rest = match differ {
            0 => "all of them alike".to_string(),
            1 => "of which 1 still differs".to_string(),
            differ => format!("of which {differ} still differ"),
        };
        format!(
            "{tested} of the {} rows of its parity matrix have tests, {against} of them against what Pages itself laid out, {rest}",
            self.rows.len()
        )
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
                if !pages.warnings.is_empty() {
                    let _ = writeln!(
                        md,
                        "  Deixado de fora na importação: {}.",
                        pages.warnings.join("; ")
                    );
                }
            }
        }
        if !self.unknown_rows.is_empty() {
            let _ = writeln!(
                md,
                "\nLinhas que os casos citam e a matriz não tem: {}.",
                self.unknown_rows.join(", ")
            );
        }

        md.push_str("\n## Linhas\n\n| ID | Fase | Funcionalidade | Situação | Casos | Testes | Comparados com o Pages |\n|---|---:|---|---|---|---:|---:|\n");
        for row in &self.rows {
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} | {} |",
                row.row.id,
                row.row.phase.map_or("-".to_owned(), |p| p.to_string()),
                row.row.feature.replace('|', "\\|"),
                row.status.label(),
                row.cases.join(", "),
                row.tests.len(),
                row.compared.len()
            );
        }
        let differing: Vec<&RowReport> = self
            .rows
            .iter()
            .filter(|r| !r.differing.is_empty())
            .collect();
        if !differing.is_empty() {
            md.push_str("\n## Documentos do Pages que ainda diferem\n\n");
            for row in differing {
                let _ = writeln!(md, "- {}: {}", row.row.id, row.differing.join(", "));
            }
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
    // The checks of the UI tests, which run the app through the
    // accessibility tree, name their rows at the end of what they check.
    let ui = crates
        .parent()
        .map(|root| root.join("tests/ui"))
        .filter(|dir| dir.is_dir());
    if let Some(ui) = ui {
        for entry in std::fs::read_dir(ui)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "py") {
                scan_ui_checks(&std::fs::read_to_string(&path)?, &mut found);
            }
        }
    }
    for tests in found.values_mut() {
        tests.sort();
        tests.dedup();
    }
    Ok(found)
}

/// The tests named after each row that compare with Pages: those of the
/// files of `crates/desk-testkit/tests` that compare words with
/// `compare_words`, which lay out documents that Pages saved on the
/// reference machine and compare them with the PDF that Pages made.
pub fn compared_by_row(crates: &Path) -> std::io::Result<BTreeMap<String, Vec<String>>> {
    comparisons(crates, false)
}

/// The tests that lay out a document of Pages known to differ from its
/// PDF, as [`compared_by_row`] finds them, kept from running with
/// `#[ignore = "differs from Pages: …"]` until Desk matches it. Their
/// rows are reported as different from Pages.
pub fn differing_by_row(crates: &Path) -> std::io::Result<BTreeMap<String, Vec<String>>> {
    comparisons(crates, true)
}

/// What an ignored test says when it compares a document that Desk does
/// not lay out as Pages yet.
pub const DIFFERS: &str = "differs from Pages";

fn comparisons(crates: &Path, differing: bool) -> std::io::Result<BTreeMap<String, Vec<String>>> {
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let dir = crates.join("desk-testkit/tests");
    if !dir.is_dir() {
        return Ok(found);
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path)?;
            if text.contains("compare_words(") {
                for (name, ignored) in marked_tests(&text) {
                    let differs = ignored.is_some_and(|reason| reason.starts_with(DIFFERS));
                    if differs == differing {
                        for id in ids_in_name(&name) {
                            found.entry(id).or_default().push(name.clone());
                        }
                    }
                }
            }
        }
    }
    for tests in found.values_mut() {
        tests.sort();
        tests.dedup();
    }
    Ok(found)
}

/// The checks of a UI test that name rows, as
/// `check("Duplicate opens a copy (DOC-14)", ...)`, by what they check.
fn scan_ui_checks(text: &str, found: &mut BTreeMap<String, Vec<String>>) {
    let mut rest = text;
    while let Some(at) = rest.find("check(") {
        rest = &rest[at + "check(".len()..];
        let Some(open) = rest.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some(end) = open.find('"') else { break };
        let what = &open[..end];
        let Some(ids) = what
            .strip_suffix(')')
            .and_then(|w| w.rsplit_once('(').map(|(_, ids)| ids))
        else {
            continue;
        };
        let ids: Vec<&str> = ids.split(", ").collect();
        if ids.iter().all(|id| crate::matrix::is_id(id)) {
            for id in ids {
                found
                    .entry(id.to_owned())
                    .or_default()
                    .push(format!("ui: {what}"));
            }
        }
    }
}

fn scan_tests(text: &str, found: &mut BTreeMap<String, Vec<String>>) {
    for (name, _) in marked_tests(text) {
        for id in ids_in_name(&name) {
            found.entry(id).or_default().push(name.clone());
        }
    }
}

/// The tests of a file, each with what its `#[ignore]` says, if it is
/// ignored: empty for a bare `#[ignore]`.
fn marked_tests(text: &str) -> Vec<(String, Option<String>)> {
    let mut tests = Vec::new();
    let mut test_next = false;
    let mut ignored = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("#[test]") {
            test_next = true;
            ignored = None;
            continue;
        }
        if test_next && let Some(rest) = line.strip_prefix("#[ignore") {
            let reason = rest
                .split_once('"')
                .and_then(|(_, r)| r.rsplit_once('"'))
                .map_or("", |(r, _)| r);
            ignored = Some(reason.to_owned());
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
        tests.push((name.to_owned(), ignored.take()));
    }
    tests
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

/// Where the README holds the summary of the report.
const README_START: &str = "<!-- parity -->";
const README_END: &str = "<!-- /parity -->";

/// The README with `summary` between its parity marks, or `None` when it
/// has no marks.
pub fn readme_with(readme: &str, summary: &str) -> Option<String> {
    let start = readme.find(README_START)? + README_START.len();
    let end = start + readme[start..].find(README_END)?;
    Some(format!("{}{summary}{}", &readme[..start], &readme[end..]))
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
        let readme = "Some 1 of 2 rows<!-- parity -->old<!-- /parity -->.";
        assert_eq!(
            readme_with(readme, "new").as_deref(),
            Some("Some 1 of 2 rows<!-- parity -->new<!-- /parity -->.")
        );
        assert_eq!(readme_with("no marks", "new"), None);
        let marked = marked_tests(
            "#[test]\n#[ignore = \"differs from Pages: words move\"]\nfn obj_12_text() {}\n#[test]\nfn obj_13_box() {}\n",
        );
        assert_eq!(
            marked,
            [
                (
                    "obj_12_text".to_owned(),
                    Some("differs from Pages: words move".to_owned())
                ),
                ("obj_13_box".to_owned(), None),
            ]
        );
        assert_eq!(found["EST-05"], ["est_05_widows"]);
        assert_eq!(found["FMT-12"], ["fmt_12_align"]);
        assert!(!found.contains_key("EST-04"));

        let mut found = BTreeMap::new();
        scan_ui_checks(
            "check(\"Duplicate opens a copy (DOC-14)\", True)\ncheck(\n    \"two rows (FMT-01, FMT-02)\",\n    x)\ncheck(\"no row (here)\", y)\ncheck(what, z)\n",
            &mut found,
        );
        assert_eq!(found["DOC-14"], ["ui: Duplicate opens a copy (DOC-14)"]);
        assert_eq!(found["FMT-02"], ["ui: two rows (FMT-01, FMT-02)"]);
        assert_eq!(found.len(), 3);
    }
}
