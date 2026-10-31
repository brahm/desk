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
    );
    assert_eq!(report.rows[0].status, Status::Differs);
    assert!(report.has_failures());
    assert!(report.to_markdown(&out).contains("diferente do Pages"));
    assert!(report.to_json().contains("\"status\": \"differs\""));
}
