// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! EST-07 to EST-12, EST-16, EST-18, EST-19 and EST-27: documents saved by
//! Pages '09 on the reference machine with columns, layouts, facing pages,
//! sections, headers, footers and fields put their words where Pages put
//! them in its PDF, on as many pages.

use std::path::{Path, PathBuf};

use desk_render::DisplayList;
use desk_testkit::case::DEFAULT_POSITION_TOLERANCE;
use desk_testkit::compare::{Word, compare_words, words};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages")
}

/// Reads a fixture, writes its PDF and returns the words of Pages and of
/// Desk, with the numbers of the pages of Desk.
fn render(name: &str) -> (Vec<Vec<Word>>, Vec<Vec<Word>>, Vec<u32>) {
    let fixtures = fixtures();
    let document = desk_filters::read_pages_at(&fixtures.join(format!("{name}.pages"))).unwrap();
    let layout = desk_layout::paginate(&document, &mut desk_text::TextSystem::new());
    let numbers = layout.pages.iter().map(|p| p.number).collect();
    let pages = DisplayList::pages(&layout);
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.pdf"));
    desk_render::Painter::new()
        .unwrap()
        .write_pdf(&pages, document.metadata(), &out)
        .unwrap();
    let reference = words(&fixtures.join(format!("{name}.pdf"))).unwrap();
    let desk = words(&out).unwrap();
    (reference, desk, numbers)
}

fn check(name: &str) -> Vec<u32> {
    let (reference, desk, numbers) = render(name);
    assert_eq!(desk.len(), reference.len(), "{name}: pages");
    let diff = compare_words(&reference, &desk);
    assert!(
        diff.passes(DEFAULT_POSITION_TOLERANCE),
        "{name}: {diff:?}\nPages: {reference:?}\nDesk: {desk:?}"
    );
    numbers
}

#[test]
fn est_07_est_08_columns_match_pages() {
    check("est-07-08-columns");
}

#[test]
fn est_09_est_10_layouts_match_pages() {
    check("est-09-10-layouts");
}

#[test]
fn est_11_facing_pages_match_pages() {
    check("est-11-facing-pages");
}

#[test]
fn est_16_est_18_sections_match_pages() {
    let name = "est-16-18-sections";
    let (reference, desk, numbers) = render(name);
    // The second section starts a page, numbered from 5.
    assert_eq!(numbers, [1, 5]);
    assert_eq!(desk.len(), reference.len(), "{name}: pages");
    // Every word is where Pages put it, the page number of the second
    // section in its field (EST-27) among them.
    let diff = compare_words(&reference, &desk);
    assert!(diff.passes(DEFAULT_POSITION_TOLERANCE), "{diff:?}");
}

#[test]
fn est_12_headers_and_footers_match_pages() {
    check("est-12-headers-footers");
}

#[test]
fn est_19_first_left_and_right_pages_match_pages() {
    check("est-19-first-left-right");
}

#[test]
fn est_27_fields_match_pages() {
    check("est-27-fields");
}

#[test]
fn est_13_footnotes_match_pages() {
    check("est-13-footnotes");
}

#[test]
fn est_13_endnotes_of_the_document_match_pages() {
    check("est-13-endnotes-document");
}

#[test]
fn est_13_endnotes_of_sections_match_pages() {
    check("est-13-endnotes-section");
}

#[test]
fn est_14_est_15_marks_of_notes_match_pages() {
    check("est-14-15-note-marks");
}
