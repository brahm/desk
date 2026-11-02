// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! FMT-16: the tab stops of a document saved by Pages '09 on the reference
//! machine fall where Pages put them in its PDF.
//!
//! Pages lines decimal stops up on the separator of the system, a comma on
//! the reference machine, which is in Portuguese, so the test runs itself
//! again in that locale.

use std::path::Path;

use desk_testkit::case::DEFAULT_POSITION_TOLERANCE;
use desk_testkit::compare::{compare_words, words};

#[test]
fn fmt_16_tabs_fall_where_pages_put_them() {
    if std::env::var_os("LC_ALL").is_none_or(|v| v != "pt_BR.UTF-8") {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fmt_16_tabs_fall_where_pages_put_them",
                "--nocapture",
            ])
            .env("LC_ALL", "pt_BR.UTF-8")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages");
    let document = desk_filters::read_pages_at(&fixtures.join("fmt-16-tabs.pages")).unwrap();
    let layout = desk_layout::paginate(&document, &mut desk_text::TextSystem::new());
    let pages = desk_render::DisplayList::pages(&layout);
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fmt-16-tabs.pdf");
    desk_render::Painter::new()
        .unwrap()
        .write_pdf(&pages, document.metadata(), &out)
        .unwrap();

    let mut reference = words(&fixtures.join("fmt-16-tabs.pdf")).unwrap();
    // Pages writes each tab as a "!" in the text of its PDF.
    for word in reference.iter_mut().flatten() {
        if let Some(text) = word.text.strip_suffix('!') {
            word.text = text.to_owned();
        }
    }
    let desk = words(&out).unwrap();
    let diff = compare_words(&reference, &desk);
    assert!(
        diff.passes(DEFAULT_POSITION_TOLERANCE),
        "{diff:?}\nPages: {:?}\nDesk: {:?}",
        reference[0],
        desk[0]
    );
}
