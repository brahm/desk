// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The character sets that the dictionaries of Hunspell and LibreOffice
//! name: many are in ISO 8859, as the Greek, Polish, Slovenian and
//! Norwegian ones in Debian and Ubuntu.

use encoding_rs::Encoding;

/// The encoding of a character set as a dictionary names it, as
/// `UTF-8`, `ISO8859-2` or `microsoft-cp1251`.
pub(crate) fn named(name: &str) -> Option<&'static Encoding> {
    let label = name
        .trim()
        .trim_start_matches('\u{feff}')
        .to_ascii_lowercase()
        .replace("iso8859", "iso-8859")
        .replace("microsoft-cp", "windows-");
    Encoding::for_label(label.as_bytes())
}

/// The text of a file in an encoding. Bytes that are not of the encoding
/// become U+FFFD, as in the comments in Latin-1 of the Hungarian
/// dictionary, which says that it is in UTF-8.
pub(crate) fn decode(bytes: &[u8], encoding: &'static Encoding) -> String {
    encoding.decode_with_bom_removal(bytes).0.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_of_the_dictionaries_are_known() {
        assert_eq!(named("UTF-8"), Some(encoding_rs::UTF_8));
        assert_eq!(named("ISO8859-2"), Some(encoding_rs::ISO_8859_2));
        assert_eq!(named("ISO8859-7 "), Some(encoding_rs::ISO_8859_7));
        assert_eq!(named("microsoft-cp1251"), Some(encoding_rs::WINDOWS_1251));
        assert_eq!(named("KOI8-R"), Some(encoding_rs::KOI8_R));
        assert_eq!(named("nonsense"), None);
    }
}
