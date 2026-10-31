// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The mapping from ODF to the document model, with the XML streams as
//! input as in `odt_streams`: style cycles, dangling references and odd
//! values must not panic, and a document imported once must export and
//! import again.

#![no_main]

use desk_filters::odt::{export, import};
use desk_odf::{read_streams, write_streams};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let mut parts = text.splitn(3, '\0');
    let content = parts.next().unwrap_or_default();
    let Ok(source) = read_streams(content, parts.next(), parts.next()) else {
        return;
    };
    let Ok(document) = import(&source) else {
        return;
    };
    let exported = export(&document);
    let written = write_streams(&exported);
    let read = read_streams(&written.content, Some(&written.styles), Some(&written.meta))
        .expect("a document Desk wrote reads back");
    let again = import(&read).expect("a document Desk exported imports again");
    let rewritten = write_streams(&export(&again));
    assert_eq!(rewritten.content, written.content, "content.xml changed");
    assert_eq!(rewritten.styles, written.styles, "styles.xml changed");
    assert_eq!(rewritten.meta, written.meta, "meta.xml changed");
});
