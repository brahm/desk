// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The reader of Pages documents and their mapping to the model, from the
//! bytes of an index (plain, compressed or in a flat file): style cycles,
//! dangling references and odd values must not panic, and an imported
//! document must write as ODT and read back.

#![no_main]

use desk_filters::{read_odt, read_pages, write_odt};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(document) = read_pages(data) else {
        return;
    };
    let written = write_odt(&document).expect("an imported document writes as ODT");
    read_odt(&written).expect("a document Desk wrote reads back");
});
