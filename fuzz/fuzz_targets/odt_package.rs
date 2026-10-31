// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whole ODT packages: the zip container and the streams in it.
//!
//! Reading must never panic, and what Desk writes must read back and write
//! out the same again.

#![no_main]

use desk_odf::{read_odt, write_odt};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(document) = read_odt(data) else {
        return;
    };
    let bytes = write_odt(&document).expect("a document that was read can be written");
    let again = read_odt(&bytes).expect("a document Desk wrote reads back");
    let rewritten = write_odt(&again).expect("a document that was read can be written");
    assert_eq!(
        rewritten, bytes,
        "the package changed when read and written"
    );
});
