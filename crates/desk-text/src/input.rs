// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The text of a paragraph as the layout sees it.
//!
//! The model keeps the text as it was typed. Capitalization (FMT-07) is
//! applied here, when the text is laid out, so the laid out text can differ
//! from the model text. Offsets in the results are always model offsets.

use std::ops::Range;

use desk_model::{Capitalization, CharFormat, ParaFormat, Paragraph, Styles};

/// The size of synthesized small capitals, relative to the font size.
///
/// To be checked against Pages in the reference VM.
pub const SMALL_CAPS_SCALE: f32 = 0.8;

/// A piece of the laid out text with one format.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StyledRange {
    /// The range in the laid out text.
    pub range: Range<usize>,
    /// The index of the format in [`ParagraphText::formats`].
    pub format: u32,
    /// Whether these are lower-case letters drawn as small capitals.
    pub small_caps: bool,
}

/// A paragraph ready for layout: its text with capitalization applied, its
/// character formats and its paragraph format.
#[derive(Clone, Debug)]
pub struct ParagraphText {
    text: String,
    /// The model offset of each character start of the laid out text, and
    /// of its end, when the two texts differ. Empty when they are the same.
    offsets: Vec<(usize, usize)>,
    formats: Vec<CharFormat>,
    pub(crate) ranges: Vec<StyledRange>,
    paragraph: ParaFormat,
}

impl ParagraphText {
    /// Prepares a paragraph of a document, resolving its styles.
    pub fn new(styles: &Styles, paragraph: &Paragraph) -> Self {
        let base = styles.resolve_character(paragraph.style(), None, &Default::default());
        let format = styles.resolve_paragraph(paragraph.style(), paragraph.attrs());
        let runs = paragraph.runs().map(|run| {
            let format = styles.resolve_character(paragraph.style(), run.style, &run.attrs);
            (run.text(), format)
        });
        Self::from_runs(format, base, runs)
    }

    /// Prepares text from pieces with resolved formats. `base` is the
    /// format that an empty paragraph takes, for the height of its line.
    pub fn from_runs<'a>(
        paragraph: ParaFormat,
        base: CharFormat,
        runs: impl IntoIterator<Item = (&'a str, CharFormat)>,
    ) -> Self {
        let mut out = Self {
            text: String::new(),
            offsets: Vec::new(),
            formats: vec![base],
            ranges: Vec::new(),
            paragraph,
        };
        let mut model_offset = 0;
        let mut transformed = false;
        let mut previous: Option<char> = None;
        for (text, format) in runs {
            let index = match out.formats.iter().position(|f| f == &format) {
                Some(index) => index,
                None => {
                    out.formats.push(format.clone());
                    out.formats.len() - 1
                }
            } as u32;
            for c in text.chars() {
                let start = out.text.len();
                let word_start = !previous.is_some_and(char::is_alphanumeric);
                let small = format.capitalization == Capitalization::SmallCaps && c.is_lowercase();
                let upper = match format.capitalization {
                    Capitalization::None => false,
                    Capitalization::AllCaps | Capitalization::SmallCaps => true,
                    Capitalization::Title => word_start,
                };
                if upper && c.is_lowercase() {
                    out.text.extend(c.to_uppercase());
                    transformed = true;
                } else {
                    out.text.push(c);
                }
                out.offsets.push((start, model_offset));
                out.push_range(start..out.text.len(), index, small);
                model_offset += c.len_utf8();
                previous = Some(c);
            }
        }
        out.offsets.push((out.text.len(), model_offset));
        if !transformed {
            out.offsets.clear();
        }
        out
    }

    fn push_range(&mut self, range: Range<usize>, format: u32, small_caps: bool) {
        match self.ranges.last_mut() {
            Some(last)
                if last.range.end == range.start
                    && last.format == format
                    && last.small_caps == small_caps =>
            {
                last.range.end = range.end;
            }
            _ => self.ranges.push(StyledRange {
                range,
                format,
                small_caps,
            }),
        }
    }

    /// The text as it is laid out, with capitalization applied.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The paragraph format.
    pub fn paragraph(&self) -> &ParaFormat {
        &self.paragraph
    }

    /// The character formats, by index. The first one is the base format
    /// of the paragraph.
    pub fn formats(&self) -> &[CharFormat] {
        &self.formats
    }

    /// The model offset of an offset in the laid out text.
    pub fn to_model(&self, offset: usize) -> usize {
        if self.offsets.is_empty() {
            return offset;
        }
        let index = self.offsets.partition_point(|&(laid, _)| laid <= offset);
        self.offsets[index.saturating_sub(1)].1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(capitalization: Capitalization) -> CharFormat {
        CharFormat {
            capitalization,
            ..CharFormat::default()
        }
    }

    #[test]
    fn capitalization_maps_back_to_model_offsets() {
        let text = ParagraphText::from_runs(
            ParaFormat::default(),
            CharFormat::default(),
            [
                ("straße ", format(Capitalization::AllCaps)),
                ("são paulo", format(Capitalization::Title)),
            ],
        );
        assert_eq!(text.text(), "STRASSE São Paulo");
        // "SS" comes from one "ß", which is two bytes in the model.
        assert_eq!(text.to_model(4), 4);
        assert_eq!(text.to_model(5), 4);
        assert_eq!(text.to_model(6), 6);
        assert_eq!(text.to_model(text.text().len()), "straße são paulo".len());
    }

    #[test]
    fn small_caps_mark_lower_case_letters() {
        let text = ParagraphText::from_runs(
            ParaFormat::default(),
            CharFormat::default(),
            [("Ab", format(Capitalization::SmallCaps))],
        );
        assert_eq!(text.text(), "AB");
        let small: Vec<_> = text.ranges.iter().map(|r| r.small_caps).collect();
        assert_eq!(small, [false, true]);
    }

    #[test]
    fn identical_text_keeps_offsets() {
        let text = ParagraphText::from_runs(
            ParaFormat::default(),
            CharFormat::default(),
            [("abc", CharFormat::default())],
        );
        assert_eq!(text.to_model(2), 2);
        assert_eq!(text.ranges.len(), 1);
        assert_eq!(text.formats().len(), 1);
    }
}
