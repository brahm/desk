// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OBJ-01 to OBJ-42: documents with objects saved by Pages '09 on the
//! reference machine put their words where Pages put them in its PDF, on
//! as many pages, and draw their objects as Pages drew them, to a few
//! pixels in a hundred.
//!
//! The documents with text in an oval, a text box in the text and wraps
//! do not match yet: their tests are kept from running with
//! `#[ignore = "differs from Pages: …"]`, which the parity report reads
//! to show their rows as different from Pages, until they match.

use std::path::{Path, PathBuf};

use desk_render::DisplayList;
use desk_testkit::case::DEFAULT_POSITION_TOLERANCE;
use desk_testkit::compare::{compare_pixels, compare_words, words};

/// The share of the pixels of a page that may differ, in percent: the
/// substitutes of the fonts of Pages and the smoothing of edges.
const PIXEL_TOLERANCE: f64 = 2.0;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages")
}

/// Lays out a fixture, writes it as PDF, and compares its words and its
/// pixels with the PDF of Pages.
fn check(name: &str) {
    let fixtures = fixtures();
    let document = desk_filters::read_pages_at(&fixtures.join(format!("{name}.pages"))).unwrap();
    let layout = desk_layout::paginate(&document, &mut desk_text::TextSystem::new());
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
    assert!(
        pixels.worst() <= PIXEL_TOLERANCE,
        "{name}: {:.1} % of the pixels differ, see {:?}",
        pixels.worst(),
        pixels.images
    );
}

#[test]
fn obj_01_floating_and_inline_objects_match_pages() {
    check("obj-01-floating-inline");
}

#[test]
fn obj_04_masks_match_pages() {
    check("obj-04-mask");
}

#[test]
fn obj_06_instant_alpha_matches_pages() {
    check("obj-06-instant-alpha");
}

#[test]
fn obj_07_adjusted_pictures_match_pages() {
    check("obj-07-adjust-image");
}

#[test]
fn obj_08_obj_11_shapes_match_pages() {
    check("obj-08-11-shapes");
}

#[test]
fn obj_09_shapes_drawn_with_the_pen_match_pages() {
    check("obj-09-pen");
}

#[test]
fn obj_10_editable_shapes_match_pages() {
    check("obj-10-editable");
}

#[test]
fn obj_24_layers_match_pages() {
    check("obj-24-layers");
}

#[test]
fn obj_25_background_objects_match_pages() {
    check("obj-25-background");
}

#[test]
fn obj_33_positions_and_sizes_match_pages() {
    check("obj-29-33-position-size");
}

#[test]
fn obj_30_groups_match_pages() {
    check("obj-30-group");
}

#[test]
fn obj_32_locked_objects_match_pages() {
    check("obj-32-lock");
}

#[test]
fn obj_34_flipped_and_turned_objects_match_pages() {
    check("obj-34-flip-rotate");
}

#[test]
fn obj_35_strokes_match_pages() {
    check("obj-35-stroke");
}

#[test]
fn obj_36_picture_frames_match_pages() {
    check("obj-36-frame");
}

#[test]
fn obj_37_obj_38_obj_39_shadows_reflections_and_opacity_match_pages() {
    check("obj-37-38-39-effects");
}

#[test]
fn obj_40_obj_41_fills_match_pages() {
    check("obj-40-41-fills");
}

#[test]
fn obj_42_image_fills_match_pages() {
    check("obj-42-image-fill");
}

#[test]
fn obj_02_pictures_of_every_format_match_pages() {
    check("obj-02-image-formats");
}

#[test]
fn obj_13_fmt_13_text_boxes_match_pages() {
    check("obj-13-text-box");
}

#[test]
fn obj_15_linked_text_boxes_match_pages() {
    check("obj-15-linked-boxes");
}

#[test]
fn obj_16_fmt_13_fmt_19_columns_and_alignment_of_text_boxes_match_pages() {
    check("obj-16-fmt-13-19-box-format");
}

#[test]
fn obj_18_sounds_and_movies_in_the_text_match_pages() {
    check("obj-18-audio-video");
}

#[test]
fn obj_19_movies_match_pages() {
    check("obj-19-movie-settings");
}

#[test]
fn obj_42_tinted_image_fills_match_pages() {
    check("obj-42-tinted-image-fill");
}

#[test]
#[ignore = "differs from Pages: the text in the oval breaks otherwise"]
fn obj_12_text_in_shapes_matches_pages() {
    check("obj-12-text-in-shape");
}

#[test]
#[ignore = "differs from Pages: the text of the box in the text moves"]
fn obj_14_inline_text_boxes_match_pages() {
    check("obj-14-inline-text-box");
}

#[test]
#[ignore = "differs from Pages: words move up to 418 pt and 9 % of the pixels differ"]
fn obj_17_wraps_match_pages() {
    check("obj-17-wrap");
}
