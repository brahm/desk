// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an import of Pages or ODT left out, in the language of the interface, as
//! the Warnings window lists it (INT-22).

use desk_filters::log::{Content, Entry, Formatting, Warning};
use desk_ui::i18n::{tr, trn};

/// A warning of an import, as a sentence.
pub fn describe(entry: &Entry) -> String {
    let count = entry.count as u64;
    let n = |text: String| text.replace("{n}", &count.to_string());
    match &entry.warning {
        Warning::Content(content) => {
            let what = match content {
                Content::Headers => tr("headers"),
                Content::Footers => tr("footers"),
                Content::FloatingObjects => tr("objects placed on the page"),
                Content::InlineObjects => tr("objects in the text"),
                Content::Footnotes => tr("footnotes"),
                Content::Comments => tr("comments"),
                Content::Fields => tr("fields"),
                Content::TablesOfContents => tr("tables of contents"),
                Content::TrackedChanges => tr("tracked changes, whose text is kept as plain text"),
                Content::Bookmarks => tr("bookmarks, whose text is kept"),
                Content::Tables => tr("tables, whose text is kept as paragraphs"),
                Content::AcceptedChanges => tr("tracked changes, which are kept as accepted"),
                Content::UnreadObjects => tr("objects such as charts and embedded documents"),
                Content::Other(name) => tr("content of the kind “{name}”").replace("{name}", name),
            };
            n(tr("Left out {what} ({n}).")).replace("{what}", &what)
        }
        Warning::Formatting(formatting) => {
            let what = match formatting {
                Formatting::ImageBullets => tr("image bullets, which are drawn as text bullets"),
                Formatting::LineDetails => tr("lines under or through words only"),
                Formatting::DropCaps => tr("drop caps"),
                Formatting::HiddenText => tr("hidden text, which is shown"),
                Formatting::FontFeatures => tr("font features"),
            };
            n(trn(
                "Left out {what}, in {n} paragraph.",
                "Left out {what}, in {n} paragraphs.",
                count,
            ))
            .replace("{what}", &what)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_22_warnings_read_as_the_log_says_them() {
        // In English, the same sentences as the log of the import.
        for warning in [
            Warning::Content(Content::Headers),
            Warning::Content(Content::Other("sf:thing".into())),
            Warning::Formatting(Formatting::DropCaps),
        ] {
            for count in [1, 3] {
                let entry = Entry {
                    warning: warning.clone(),
                    count,
                };
                // Only where the interface is in English.
                if tr("headers") == "headers" {
                    assert_eq!(describe(&entry), format!("{entry}."));
                }
            }
        }
    }
}
