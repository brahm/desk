// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs the corpus and writes the parity report.
//!
//! Usage: `desk-parity [--update-readme | --check-readme] [OUT]`, from
//! anywhere in the repository. It writes `report.md`, `report.json` and,
//! for each case, the PDF of Desk and the diff images into OUT,
//! `target/parity` by default, and fails when a case, or the document that
//! Pages saved for it, differs from Pages or could not run.
//!
//! With `--update-readme` it writes how many rows have tests into the
//! README, between its parity marks; with `--check-readme` it fails when
//! the README says otherwise, as the CI runs it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use desk_testkit::report::{
    Report, compared_by_row, differing_by_row, parse_review, run_case, tests_by_row,
};
use desk_testkit::{Case, matrix};

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("desk-parity: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", root.display()))?;
    let (flags, paths): (Vec<_>, Vec<_>) = std::env::args_os()
        .skip(1)
        .partition(|a| a.to_string_lossy().starts_with("--"));
    let readme_mode = match flags.first().map(|f| f.to_string_lossy().into_owned()) {
        None => None,
        Some(flag) if flag == "--update-readme" || flag == "--check-readme" => Some(flag),
        Some(flag) => return Err(format!("unknown option {flag}")),
    };
    let out = paths
        .into_iter()
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/parity"));
    let corpus = root.join("corpus");

    let matrix_path = corpus.join("matrix.tsv");
    let text = std::fs::read_to_string(&matrix_path)
        .map_err(|e| format!("{}: {e}", matrix_path.display()))?;
    let rows = matrix::parse(&text).map_err(|e| format!("{}: {e}", matrix_path.display()))?;
    let cases = Case::load_all(corpus.join("writer")).map_err(|e| e.to_string())?;
    let tests = tests_by_row(&root.join("crates")).map_err(|e| e.to_string())?;
    let compared = compared_by_row(&root.join("crates")).map_err(|e| e.to_string())?;
    let differing = differing_by_row(&root.join("crates")).map_err(|e| e.to_string())?;

    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let results = cases.iter().map(|case| run_case(case, &out)).collect();
    let review_path = corpus.join("vm-review.tsv");
    let review = std::fs::read_to_string(&review_path)
        .map_err(|e| format!("{}: {e}", review_path.display()))
        .and_then(|text| {
            parse_review(&text).map_err(|e| format!("{}: {e}", review_path.display()))
        })?;
    let report = Report::new(rows, results, &tests, &compared, &differing).with_review(&review);
    for (name, text) in [
        ("report.md", report.to_markdown(&out)),
        ("report.json", report.to_json()),
    ] {
        let path = out.join(name);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    for case in &report.cases {
        match &case.from_pages {
            Some(pages) => println!(
                "{}: {}; from the Pages document: {}",
                case.name,
                case.outcome.label(),
                pages.outcome.label()
            ),
            None => println!("{}: {}", case.name, case.outcome.label()),
        }
    }
    println!("The report is in {}.", out.join("report.md").display());
    let mut readme_ok = true;
    if let Some(mode) = readme_mode {
        let path = root.join("README.md");
        let readme =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let updated = desk_testkit::report::readme_with(&readme, &report.summary())
            .ok_or_else(|| format!("{}: no parity marks", path.display()))?;
        if mode == "--update-readme" {
            std::fs::write(&path, updated).map_err(|e| format!("{}: {e}", path.display()))?;
        } else if updated != readme {
            eprintln!("README.md is out of date; run desk-parity --update-readme");
            readme_ok = false;
        }
    }
    Ok(!report.has_failures() && readme_ok)
}
