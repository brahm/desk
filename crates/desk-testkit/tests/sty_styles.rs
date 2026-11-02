// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! STY-01, STY-02, STY-06 and STY-07: a document saved by Pages '09 on the
//! reference machine with paragraph and character styles, a next style,
//! an override and a renamed style keeps its styles and puts its words
//! where Pages put them in its PDF.

use std::path::{Path, PathBuf};

use desk_model::StyleFamily;
use desk_render::DisplayList;
use desk_testkit::case::DEFAULT_POSITION_TOLERANCE;
use desk_testkit::compare::{compare_words, words};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages")
}

#[test]
fn sty_01_07_styles_match_pages() {
    let name = "sty-01-07-styles";
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
    let diff = compare_words(&reference, &desk);
    assert!(
        diff.passes(DEFAULT_POSITION_TOLERANCE),
        "{name}: {diff:?}\nPages: {:?}\nDesk: {:?}",
        reference[0],
        desk[0]
    );
    let styles = document.styles();
    let style_of = |text: &str| {
        let paragraph = document
            .body()
            .iter()
            .find(|p| p.text() == text)
            .unwrap_or_else(|| panic!("no paragraph {text:?}"));
        styles.get(paragraph.style()).unwrap().name().to_owned()
    };
    // STY-01 and STY-07: a custom paragraph style with a next style.
    assert_eq!(style_of("Custom paragraph style"), "My Paragraph");
    assert_eq!(
        style_of("Next paragraph takes the following style"),
        "My Next"
    );
    let mine = styles
        .by_name(StyleFamily::Paragraph, "My Paragraph")
        .unwrap();
    let next = styles.by_name(StyleFamily::Paragraph, "My Next").unwrap();
    let desk_model::Style::Paragraph(mine) = mine else {
        unreachable!()
    };
    assert_eq!(mine.next, Some(next.id()));
    // STY-01 and STY-02: a character style inside a paragraph of another
    // style.
    let character = styles
        .by_name(StyleFamily::Character, "My Character")
        .unwrap();
    let paragraph = document
        .body()
        .iter()
        .find(|p| p.text() == "Custom character style in this paragraph")
        .unwrap();
    assert!(
        paragraph
            .runs()
            .any(|run| run.style == Some(character.id()))
    );
    // STY-06: the override is set on the run, over the paragraph style.
    let paragraph = document
        .body()
        .iter()
        .find(|p| p.text() == "Override on a styled paragraph")
        .unwrap();
    assert!(paragraph.runs().any(|run| run.attrs.color.is_some()));
    // STY-05: the renamed style keeps its new name.
    assert_eq!(style_of("Renamed style"), "Renamed Caption");
    assert_eq!(style_of("Heading 1 style"), "Heading 1");
}
