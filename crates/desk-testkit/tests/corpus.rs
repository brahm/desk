// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The corpus loads, and cases compare with a reference as they should.

use std::path::{Path, PathBuf};

use desk_testkit::case::REFERENCE_PDF;
use desk_testkit::report::{Report, run_case};
use desk_testkit::{Case, Outcome, Status, matrix};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

/// A fresh directory under the target directory of the tests.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A copy of a case of the corpus, to give it a reference.
fn copy_case(name: &str, to: &Path) -> Case {
    let dir = to.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        corpus().join("writer").join(name).join("case.toml"),
        dir.join("case.toml"),
    )
    .unwrap();
    Case::load(dir).unwrap()
}

#[test]
fn every_case_loads_and_checks_rows_of_the_matrix() {
    let rows =
        matrix::parse(&std::fs::read_to_string(corpus().join("matrix.tsv")).unwrap()).unwrap();
    let cases = Case::load_all(corpus().join("writer")).unwrap();
    assert!(!cases.is_empty());
    for case in &cases {
        for id in &case.spec.rows {
            assert!(
                rows.iter().any(|r| &r.id == id),
                "{} checks {id}, which the matrix does not have",
                case.name
            );
        }
        case.spec.document().unwrap();
    }
}

#[test]
fn a_case_matches_its_own_pdf_and_not_another() {
    let dir = scratch("compare");
    let out = dir.join("out");
    let case = copy_case("fmt-12-alignment", &dir.join("cases"));

    // Without a reference, Desk lays it out.
    let result = run_case(&case, &out);
    assert_eq!(result.outcome, Outcome::NoReference { pages: 1 });

    // Its own PDF as the reference: the same, to the point and the pixel.
    std::fs::copy(&result.pdf, case.dir.join(REFERENCE_PDF)).unwrap();
    let result = run_case(&case, &out);
    let Outcome::Pass { layout, pixels } = &result.outcome else {
        panic!("{:?}", result.outcome);
    };
    assert_eq!(layout.max_offset, 0.0);
    assert_eq!(pixels.worst(), 0.0);
    assert!(pixels.images[0].is_file());

    // The PDF of another case: different words.
    let other = copy_case("fmt-18-indents", &dir.join("cases"));
    let other_pdf = run_case(&other, &out).pdf;
    std::fs::copy(other_pdf, case.dir.join(REFERENCE_PDF)).unwrap();
    let result = run_case(&case, &out);
    let Outcome::Fail { reason, pixels, .. } = &result.outcome else {
        panic!("{:?}", result.outcome);
    };
    assert!(reason.contains("palavra"), "{reason}");
    assert!(pixels.as_ref().unwrap().worst() > 1.0);

    let report = Report::new(
        vec![matrix::Row {
            id: "FMT-12".into(),
            phase: Some(1),
            feature: "Alinhamento".into(),
            expected: String::new(),
            guide_pages: String::new(),
            odf: String::new(),
        }],
        vec![result],
        &Default::default(),
        &Default::default(),
        &Default::default(),
    );
    assert_eq!(report.rows[0].status, Status::Differs);
    assert!(report.has_failures());
    assert!(report.to_markdown(&out).contains("diferente do Pages"));
    assert!(report.to_json().contains("\"status\": \"differs\""));
}

/// Each paragraph of a document, resolved: its text, its format and the
/// format of each run.
fn resolved(document: &desk_model::Document) -> Vec<String> {
    let styles = document.styles();
    document
        .body()
        .iter()
        .map(|paragraph| {
            let mut format = styles.resolve_paragraph(paragraph.style(), paragraph.attrs());
            // Pages has widow control on by default, and the cases only
            // say where it matters.
            format.widow_control = false;
            // A word breaks only where both the paragraph and the document
            // allow it, and the cases say so on the paragraph.
            format.hyphenate &= document.text_settings().hyphenation;
            let mut line = format!("{format:?}");
            for run in paragraph.runs() {
                let character = styles.resolve_character(paragraph.style(), run.style, &run.attrs);
                line.push_str(&format!("\n  {:?} {character:?}", run.text()));
            }
            line
        })
        .collect()
}

#[test]
fn doc_22_documents_of_pages_import_as_their_cases() {
    for case in Case::load_all(corpus().join("writer")).unwrap() {
        let Some(path) = case.pages_document() else {
            continue;
        };
        let (imported, log) = desk_filters::read_pages_at_logged(&path).unwrap();
        // The cases use only what the model holds, so nothing is left out.
        assert!(log.is_empty(), "{}: {:?}", case.name, log.entries());
        let built = case.spec.document().unwrap();
        assert_eq!(resolved(&imported), resolved(&built), "{}", case.name);
        // The cases have no headers or footers, so where they would go
        // does not matter.
        assert!(imported.headers_footers().is_empty(), "{}", case.name);
        let built_setup = built.page_setup();
        let setup = desk_model::PageSetup {
            headers: built_setup.headers,
            footers: built_setup.footers,
            header_margin: built_setup.header_margin,
            footer_margin: built_setup.footer_margin,
            ..*imported.page_setup()
        };
        assert_eq!(&setup, built_setup, "{}", case.name);
    }
}

#[test]
fn the_report_tells_tests_that_compare_with_pages_from_the_others() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let compared = desk_testkit::report::compared_by_row(&crates).unwrap();
    assert!(compared["EST-07"].contains(&"est_07_est_08_columns_match_pages".to_owned()));
    // Tests of the layout of text alone do not read documents of Pages.
    assert!(!compared.contains_key("TXT-03"));
    let tests = desk_testkit::report::tests_by_row(&crates).unwrap();
    assert!(tests.contains_key("TXT-03"));

    let row = |id: &str| matrix::Row {
        id: id.into(),
        phase: Some(2),
        feature: String::new(),
        expected: String::new(),
        guide_pages: String::new(),
        odf: String::new(),
    };
    let report = Report::new(
        vec![row("EST-07"), row("TXT-03")],
        Vec::new(),
        &tests,
        &compared,
        &Default::default(),
    );
    assert_eq!(report.rows[0].status, Status::Compared);
    assert_eq!(report.rows[1].status, Status::Tested);
    assert!(
        report
            .to_markdown(Path::new("."))
            .contains("comparado com o PDF do Pages")
    );
}

#[test]
fn the_report_tells_documents_of_pages_that_still_differ() {
    // Every document compared with Pages matches it now, so no test is
    // kept from running; a row whose test would be is shown as different.
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    assert!(
        desk_testkit::report::differing_by_row(&crates)
            .unwrap()
            .is_empty()
    );
    let compared = desk_testkit::report::compared_by_row(&crates).unwrap();
    let differing = [("OBJ-12".to_string(), vec!["obj_12_test".to_string()])]
        .into_iter()
        .collect();
    let report = Report::new(
        vec![matrix::Row {
            id: "OBJ-12".into(),
            phase: Some(6),
            feature: String::new(),
            expected: String::new(),
            guide_pages: String::new(),
            odf: String::new(),
        }],
        Vec::new(),
        &Default::default(),
        &compared,
        &differing,
    );
    assert_eq!(report.rows[0].status, Status::Differs);
    assert!(!report.has_failures());
    assert!(report.to_markdown(Path::new(".")).contains("ainda diferem"));
}
