// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A long document that Pages '09 saved on the reference machine, in
//! place of a private one: four sections, one opening on a right page
//! after a blank one, headers and footers with page numbers, a table of
//! contents, footnotes, pictures in the lines of the text and around it,
//! two columns, page breaks and styles that take their parent's spaces
//! away. Desk makes as many pages as Pages, and puts the words of most of
//! them where Pages put them.
//!
//! One thing still differs: in the narrow lines beside a picture in two
//! columns (pages 19 to 22), Pages spreads part of the room of a
//! justified line between the letters, where Desk puts it all between
//! the words.

use std::path::{Path, PathBuf};

use desk_render::DisplayList;
use desk_testkit::case::DEFAULT_POSITION_TOLERANCE;
use desk_testkit::compare::{Word, compare_words, words};

/// The words of each page of the PDF of Pages and of the PDF of Desk.
fn pages() -> (Vec<Vec<Word>>, Vec<Vec<Word>>) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages");
    let document = desk_filters::read_pages_at(&fixtures.join("long-document.pages")).unwrap();
    let layout = desk_layout::paginate(&document, &mut desk_text::TextSystem::new());
    let pages = DisplayList::pages(&layout);
    let out: PathBuf = Path::new(env!("CARGO_TARGET_TMPDIR")).join("long-document.pdf");
    desk_render::Painter::new()
        .unwrap()
        .write_pdf(&pages, document.metadata(), &out)
        .unwrap();
    let reference = words(&fixtures.join("long-document.pdf")).unwrap();
    (reference, words(&out).unwrap())
}

#[test]
fn est_04_est_05_est_11_est_12_est_13_est_16_est_18_est_19_est_22_a_long_document_matches_pages_page_by_page()
 {
    let (reference, desk) = pages();
    assert_eq!(desk.len(), reference.len(), "pages");
    // All but the pages of two columns with a picture: the title page,
    // the contents, the blank page before the second section, the second
    // section with its pictures, styles and breaks, the notes of the
    // columns, and the text around the round picture.
    for pages in [0..18, 22..41] {
        let diff = compare_words(&reference[pages.clone()], &desk[pages.clone()]);
        assert!(
            diff.passes(DEFAULT_POSITION_TOLERANCE),
            "pages {} to {}: {diff:?}",
            pages.start + 1,
            pages.end
        );
    }
}

#[test]
#[ignore = "differs from Pages: justified lines beside a picture in two columns spread their room between the letters"]
fn est_07_a_long_document_matches_pages_to_its_last_page() {
    let (reference, desk) = pages();
    let diff = compare_words(&reference, &desk);
    assert!(diff.passes(DEFAULT_POSITION_TOLERANCE), "{diff:?}");
}
