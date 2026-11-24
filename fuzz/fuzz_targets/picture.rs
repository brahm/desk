// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pictures that documents carry: any bytes must read as a picture or
//! as none, without a panic, and a TIFF or GIF picture comes out within the
//! size asked for (review RV, batch 6: the converters had no target, and
//! batch 1 found overflows in them).

#![no_main]

use desk_filters::picture::{picture_within, scaled_when_read};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    const SIDE: u32 = 256;
    let Some(picture) = picture_within(data, SIDE) else {
        return;
    };
    assert!(picture.width() > 0 && picture.height() > 0);
    if scaled_when_read(data) {
        assert!(picture.width() <= SIDE && picture.height() <= SIDE);
    }
});
