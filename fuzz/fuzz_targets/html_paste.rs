// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The HTML that other programs put on the clipboard (TXT-04): any text
//! must read as a fragment, without a panic, that pastes into a document.

#![no_main]

use desk_filters::html::read_html;
use desk_model::{Document, Editor};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let html = String::from_utf8_lossy(data);
    let fragment = read_html(&html).expect("HTML always reads");
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor
        .edit("Paste", |edit| edit.paste_fragment(start, &fragment).map(drop))
        .expect("what HTML reads as pastes");
});
