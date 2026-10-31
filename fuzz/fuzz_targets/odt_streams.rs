// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The XML streams of a text document, without the zip package around
//! them, so that the fuzzer spends its time on the XML. The input holds
//! `content.xml`, `styles.xml` and `meta.xml` separated by NUL bytes,
//! which XML cannot contain.
//!
//! Reading must never panic, and what Desk writes must read back and write
//! out the same again. The first write may round lengths, so the check is
//! on the written streams rather than on the document read.

#![no_main]

use desk_odf::{read_streams, write_streams};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let mut parts = text.splitn(3, '\0');
    let content = parts.next().unwrap_or_default();
    let styles = parts.next();
    let meta = parts.next();
    let Ok(document) = read_streams(content, styles, meta) else {
        return;
    };
    let written = write_streams(&document);
    let again = read_streams(&written.content, Some(&written.styles), Some(&written.meta))
        .expect("a document Desk wrote reads back");
    let rewritten = write_streams(&again);
    assert_eq!(rewritten.content, written.content, "content.xml changed");
    assert_eq!(rewritten.styles, written.styles, "styles.xml changed");
    assert_eq!(rewritten.meta, written.meta, "meta.xml changed");
});
