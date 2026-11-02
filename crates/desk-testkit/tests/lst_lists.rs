// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! LST-02 to LST-05: documents saved by Pages '09 on the reference machine
//! with bulleted, numbered and tiered lists keep their list styles and
//! levels, and put their labels and words where Pages put them in its
//! PDF.

use std::path::{Path, PathBuf};

use desk_model::{Document, ListLabel, StyleFamily};
use desk_render::DisplayList;
use desk_testkit::case::DEFAULT_POSITION_TOLERANCE;
use desk_testkit::compare::{Word, compare_words, words};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages")
}

/// Reads a fixture, writes its PDF and returns the document with the
/// words of Pages and of Desk.
fn render(name: &str) -> (Document, Vec<Vec<Word>>, Vec<Vec<Word>>) {
    let fixtures = fixtures();
    let document = desk_filters::read_pages_at(&fixtures.join(format!("{name}.pages"))).unwrap();
    let layout = desk_layout::paginate(&document, &mut desk_text::TextSystem::new());
    let pages = DisplayList::pages(&layout);
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.pdf"));
    desk_render::Painter::new()
        .unwrap()
        .write_pdf(&pages, document.metadata(), &out)
        .unwrap();
    let reference = words(&fixtures.join(format!("{name}.pdf"))).unwrap();
    let desk = words(&out).unwrap();
    (document, reference, desk)
}

fn check(name: &str, reference: &[Vec<Word>], desk: &[Vec<Word>]) {
    let diff = compare_words(reference, desk);
    assert!(
        diff.passes(DEFAULT_POSITION_TOLERANCE),
        "{name}: {diff:?}\nPages: {:?}\nDesk: {:?}",
        reference[0],
        desk[0]
    );
}

/// The labels of the paragraphs of a document.
fn labels(document: &Document) -> Vec<String> {
    document
        .styles()
        .list_items(document.body().iter())
        .into_iter()
        .map(|item| item.map(|i| i.label).unwrap_or_default())
        .collect()
}

fn levels(document: &Document) -> Vec<u8> {
    let styles = document.styles();
    document
        .body()
        .iter()
        .map(|p| styles.resolve_paragraph(p.style(), p.attrs()).list_level)
        .collect()
}

#[test]
fn lst_02_levels_match_pages() {
    let name = "lst-02-levels";
    let (document, reference, desk) = render(name);
    check(name, &reference, &desk);
    assert_eq!(levels(&document), [0, 1, 2, 0]);
    assert_eq!(labels(&document), ["\u{2022}"; 4]);
}

#[test]
fn lst_03_bullets_match_pages() {
    let name = "lst-03-bullets";
    let (document, reference, desk) = render(name);
    // The words of both lines are where Pages put them, after the image
    // bullet in the second.
    let line = |words: &[Vec<Word>], text: &str| -> Vec<Word> {
        words[0]
            .iter()
            .filter(|w| (w.y - words[0].iter().find(|v| v.text == text).unwrap().y).abs() < 1.0)
            .cloned()
            .collect()
    };
    check(
        name,
        &[line(&reference, "Custom")],
        &[line(&desk, "Custom")],
    );
    let image = |words: &[Vec<Word>]| line(words, "Image");
    check(name, &[image(&reference)], &[image(&desk)]);
    assert_eq!(labels(&document), ["+", ""]);
    let items = document.styles().list_items(document.body().iter());
    let custom = &items[0].as_ref().unwrap().definition;
    assert_eq!(custom.size, 150);
    assert_eq!(custom.label, ListLabel::bullet("+"));
    let picture = &items[1].as_ref().unwrap().definition;
    assert!(matches!(picture.label, ListLabel::Image(_)));
    // The picture is a 12 point square where Pages drew it, a third of it
    // below the baseline.
    let layout = desk_layout::paginate(&document, &mut desk_text::TextSystem::new());
    let page = DisplayList::pages(&layout).remove(0);
    let drawn = page
        .items
        .iter()
        .find_map(|item| match item {
            desk_render::Item::Image {
                x,
                y,
                width,
                height,
                ..
            } => Some((*x, *y, *width, *height)),
            _ => None,
        })
        .expect("the picture is drawn");
    for (desk, pages) in [drawn.0, drawn.1, drawn.2, drawn.3]
        .into_iter()
        .zip([57.0, 74.0, 12.0, 12.0])
    {
        assert!((desk - pages).abs() < 1.0, "{drawn:?}");
    }
}

#[test]
fn lst_04_numbered_lists_match_pages() {
    let name = "lst-04-numbered";
    let (document, reference, desk) = render(name);
    check(name, &reference, &desk);
    assert_eq!(labels(&document), ["1.", "1.", "(I)", "a)"]);
}

#[test]
fn lst_05_tiered_lists_match_pages() {
    let name = "lst-05-tiered";
    let (document, reference, desk) = render(name);
    check(name, &reference, &desk);
    assert_eq!(
        labels(&document),
        ["1.", "1.1.", "1.1.1.", "I.", "A.", "1."]
    );
    let styles = document.styles();
    for name in ["Bullet", "Numbered List", "Harvard", "Legal"] {
        assert!(
            styles.by_name(StyleFamily::List, name).is_some(),
            "no list style {name}"
        );
    }
}
