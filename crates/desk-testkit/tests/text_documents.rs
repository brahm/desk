// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Documents of text saved by Pages '09 on the reference machine that no
//! other test of this folder draws: they put their words where Pages put
//! them in its PDF, on as many pages, and draw as Pages drew them, to a
//! few pixels in a hundred.

use std::path::{Path, PathBuf};

use desk_render::DisplayList;
use desk_testkit::case::DEFAULT_POSITION_TOLERANCE;
use desk_testkit::compare::{compare_pixels, compare_words, words};

/// The share of the pixels of a page that may differ, in percent: these
/// are pages mostly of text, where the glyphs of the fonts that stand for
/// those of the Mac differ from them.
const PIXEL_TOLERANCE: f64 = 7.0;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages")
}

/// Lays out a fixture, writes it as PDF, and compares its words and its
/// pixels with the PDF of Pages.
fn check(name: &str) {
    let fixtures = fixtures();
    let document = desk_filters::read_pages_at(&fixtures.join(format!("{name}.pages"))).unwrap();
    let layout = desk_layout::paginate_view(&document, &mut desk_text::TextSystem::new());
    let pages = DisplayList::pages(&layout);
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let out = tmp.join(format!("{name}.pdf"));
    desk_render::Painter::new()
        .unwrap()
        .write_pdf(&pages, document.metadata(), &out)
        .unwrap();
    let pdf = fixtures.join(format!("{name}.pdf"));
    let reference = words(&pdf).unwrap();
    let desk = words(&out).unwrap();
    assert_eq!(desk.len(), reference.len(), "{name}: pages");
    let diff = compare_words(&reference, &desk);
    assert!(
        diff.passes(DEFAULT_POSITION_TOLERANCE),
        "{name}: {diff:?}\nPages: {reference:?}\nDesk: {desk:?}"
    );
    let pixels = compare_pixels(&pdf, &out, &tmp.join(format!("{name}-pixels"))).unwrap();
    let worst = pixels.worst();
    assert!(
        worst <= PIXEL_TOLERANCE,
        "{name}: {worst:.1} % of the pixels differ ({:?}), see {:?}",
        pixels.pages,
        pixels.images
    );
}

#[test]
fn int_02_a_word_processing_document_matches_pages() {
    check("int-02-word-processing");
}

#[test]
fn int_05_est_21_a_master_object_matches_pages() {
    check("int-05-est-21-master-object");
}

#[test]
fn doc_22_a_document_saved_as_a_single_file_matches_pages() {
    check("doc-22-single-file");
}

#[test]
fn txt_09_hyphenated_lines_match_pages() {
    check("txt-09-hyphenation");
}

#[test]
fn txt_09_words_never_hyphenated_match_pages() {
    check("txt-09-never-hyphenate");
}

#[test]
fn fmt_07_capitalization_matches_pages() {
    check("fmt-07-capitalization");
}

#[test]
fn fmt_15_paragraph_spacing_matches_pages() {
    check("fmt-15-paragraph-spacing");
}

#[test]
fn fmt_22_the_language_of_text_matches_pages() {
    check("fmt-22-language");
}

/// The zoom and the page mode that Pages saved with the document change
/// the window, not the pages.
#[test]
fn a_document_saved_with_its_view_state_matches_pages() {
    check("int-07-08-view-state");
}

/// The author, the title and the keywords change the file, not the pages.
#[test]
fn a_document_with_its_information_filled_in_matches_pages() {
    check("doc-19-20-info");
}

#[test]
fn doc_05_the_fonts_of_the_templates_match_pages() {
    check("doc-05-apple-fonts");
}

#[test]
fn doc_05_the_fonts_of_the_templates_at_other_sizes_match_pages() {
    check("doc-05-apple-fonts-sizes");
}

#[test]
fn txt_14_doc_16_an_outline_prints_as_in_pages() {
    check("txt-14-outline-long");
}

#[test]
fn txt_14_the_same_document_out_of_outline_prints_as_in_pages() {
    check("txt-14-outline-long-normal");
}

#[test]
fn txt_14_an_outline_of_the_first_lines_prints_as_in_pages() {
    check("txt-14-outline-first-line");
}

#[test]
fn txt_14_an_outline_of_two_levels_prints_as_in_pages() {
    check("txt-14-outline-levels");
}

#[test]
fn txt_14_an_outline_with_miniatures_prints_as_in_pages() {
    check("txt-14-outline-thumbnails");
}

#[test]
fn txt_14_an_a4_outline_breaks_pages_as_in_pages() {
    check("txt-14-outline-a4-default");
}

#[test]
fn txt_14_doc_16_a_document_saved_as_an_outline_prints_as_in_pages() {
    check("txt-14-doc-16-outline");
}

#[test]
fn fmt_14_line_spacing_matches_pages() {
    check("fmt-14-line-spacing");
}
