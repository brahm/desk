// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an import could not bring into the document.
//!
//! An import keeps what the model can hold and records the rest here, so
//! that the Warnings window can list it when the document opens, as
//! iWork's Document Warnings does. Each warning says what was left out and
//! how often.

use std::fmt;

/// Content that an import left out.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Content {
    /// Headers with text.
    Headers,
    /// Footers with text.
    Footers,
    /// Objects placed on the page rather than in the text: images,
    /// shapes, text boxes, tables and charts.
    FloatingObjects,
    /// Objects in the text, such as images and tables that flow with it.
    InlineObjects,
    /// Footnotes and endnotes.
    Footnotes,
    /// Comments.
    Comments,
    /// Fields, such as page numbers, dates and merge fields.
    Fields,
    /// Tables of contents.
    TablesOfContents,
    /// Tracked changes: their text is kept as plain text, deletions too.
    TrackedChanges,
    /// Bookmarks: their text is kept.
    Bookmarks,
    /// Layouts of more than one column, whose text flows in one.
    ColumnLayouts,
    /// Content of another kind, by the name the source gives it.
    Other(String),
}

/// Formatting that an import left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Formatting {
    /// Bullets and numbers of lists.
    Lists,
    /// The spacing of characters (tracking).
    CharacterSpacing,
    /// Shadows of text.
    TextShadows,
    /// Outlined text.
    OutlinedText,
    /// The color and width of underlines and strikethroughs, and lines
    /// under words only.
    LineDetails,
    /// The background color of text.
    TextHighlights,
    /// The background color of paragraphs.
    ParagraphBackgrounds,
    /// Borders and rules of paragraphs.
    ParagraphBorders,
    /// Drop caps.
    DropCaps,
    /// Hidden text, which is shown.
    HiddenText,
    /// Ligature and other typographic settings of fonts.
    FontFeatures,
    /// Automatic hyphenation: the words break only at soft hyphens.
    Hyphenation,
}

/// A setting of the whole document that an import left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Setting {
    /// Facing pages: left pages keep the margins of right pages.
    FacingPages,
}

/// What a warning is about.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Warning {
    /// Content that was left out.
    Content(Content),
    /// Formatting that was left out; the text is kept.
    Formatting(Formatting),
    /// A setting of the document that was left out.
    Setting(Setting),
}

/// A warning and how many times it applies: how many objects for content,
/// how many paragraphs for formatting, and once for a setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// What was left out.
    pub warning: Warning,
    /// How many times.
    pub count: usize,
}

/// The warnings of an import, in the order they were first met.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportLog {
    entries: Vec<Entry>,
}

impl ImportLog {
    /// Records that something was left out `count` more times.
    pub fn add(&mut self, warning: Warning, count: usize) {
        if count == 0 {
            return;
        }
        match self.entries.iter_mut().find(|e| e.warning == warning) {
            Some(entry) => entry.count += count,
            None => self.entries.push(Entry { warning, count }),
        }
    }

    /// The warnings.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Whether nothing was left out.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl fmt::Display for Content {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Headers => f.write_str("headers"),
            Self::Footers => f.write_str("footers"),
            Self::FloatingObjects => f.write_str("objects placed on the page"),
            Self::InlineObjects => f.write_str("objects in the text"),
            Self::Footnotes => f.write_str("footnotes"),
            Self::Comments => f.write_str("comments"),
            Self::Fields => f.write_str("fields"),
            Self::TablesOfContents => f.write_str("tables of contents"),
            Self::TrackedChanges => {
                f.write_str("tracked changes, whose text is kept as plain text")
            }
            Self::Bookmarks => f.write_str("bookmarks, whose text is kept"),
            Self::ColumnLayouts => f.write_str("columns, whose text flows in one"),
            Self::Other(name) => write!(f, "content of the kind \u{201c}{name}\u{201d}"),
        }
    }
}

impl fmt::Display for Formatting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Lists => "list bullets and numbers",
            Self::CharacterSpacing => "character spacing",
            Self::TextShadows => "text shadows",
            Self::OutlinedText => "outlined text",
            Self::LineDetails => "the color and width of lines through and under text",
            Self::TextHighlights => "text highlights",
            Self::ParagraphBackgrounds => "paragraph backgrounds",
            Self::ParagraphBorders => "paragraph borders and rules",
            Self::DropCaps => "drop caps",
            Self::HiddenText => "hidden text, which is shown",
            Self::FontFeatures => "font features and ligature settings",
            Self::Hyphenation => "automatic hyphenation",
        })
    }
}

impl fmt::Display for Setting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FacingPages => "facing pages, so left pages have the margins of right pages",
        })
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.count;
        match &self.warning {
            Warning::Content(content) => {
                write!(f, "Left out {content} ({count})")
            }
            Warning::Formatting(formatting) => {
                let paragraphs = if count == 1 {
                    "paragraph"
                } else {
                    "paragraphs"
                };
                write!(f, "Left out {formatting}, in {count} {paragraphs}")
            }
            Warning::Setting(setting) => write!(f, "Left out {setting}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_add_up_in_the_order_met() {
        let mut log = ImportLog::default();
        assert!(log.is_empty());
        log.add(Warning::Content(Content::Headers), 1);
        log.add(Warning::Formatting(Formatting::Lists), 2);
        log.add(Warning::Content(Content::Headers), 2);
        log.add(Warning::Content(Content::Footers), 0);
        log.add(Warning::Setting(Setting::FacingPages), 1);
        let lines: Vec<String> = log.entries().iter().map(ToString::to_string).collect();
        assert_eq!(
            lines,
            [
                "Left out headers (3)",
                "Left out list bullets and numbers, in 2 paragraphs",
                "Left out facing pages, so left pages have the margins of right pages"
            ]
        );
    }
}
