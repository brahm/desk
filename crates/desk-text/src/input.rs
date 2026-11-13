// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The text of a paragraph as the layout sees it.
//!
//! The model keeps the text as it was typed. Capitalization (FMT-07) is
//! applied here, when the text is laid out, and so are the ligatures that
//! Pages makes with some fonts (DOC-05), so the laid out text can differ
//! from the model text. Offsets in the results are always model offsets.

use std::ops::Range;

use desk_model::{
    Capitalization, CharFormat, Length, Ligatures, LineStyle, ListItem, ListLabel, ParaFormat,
    Paragraph, Picture, Styles, TextSettings,
};

use crate::fonts::pages_font;
use crate::layout::SOFT_HYPHEN;

/// The byte offsets in a text where its words may break with a hyphen,
/// in order: none for the undetermined language or a language without
/// patterns, nor in words that have soft hyphens of their own.
fn hyphen_points(text: &str, language: &str, settings: &TextSettings) -> Vec<usize> {
    if language == "und" {
        return Vec::new();
    }
    let Some(patterns) = desk_lang::patterns(language) else {
        return Vec::new();
    };
    desk_lang::words(text)
        .into_iter()
        .filter(|range| {
            let word = &text[range.clone()];
            !word.contains(SOFT_HYPHEN) && settings.may_hyphenate(word)
        })
        .flat_map(|range| {
            patterns
                .hyphenate(&text[range.clone()])
                .into_iter()
                .map(move |point| range.start + point)
        })
        .collect()
}

/// U+FB01 LATIN SMALL LIGATURE FI.
pub(crate) const FI: char = '\u{fb01}';
/// U+FB02 LATIN SMALL LIGATURE FL.
pub(crate) const FL: char = '\u{fb02}';
/// U+FB00 LATIN SMALL LIGATURE FF.
pub(crate) const FF: char = '\u{fb00}';
/// U+FB03 LATIN SMALL LIGATURE FFI.
pub(crate) const FFI: char = '\u{fb03}';
/// U+FB04 LATIN SMALL LIGATURE FFL.
pub(crate) const FFL: char = '\u{fb04}';

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

/// The label of a list item: a bullet or a number, drawn before the
/// first line of the paragraph (LST-02 to LST-05).
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// The text, as "•" or "1.1.".
    pub text: String,
    /// The index of its format in [`ParagraphText::formats`].
    pub format: u32,
    /// The distance from the left indent of the paragraph to the label.
    pub indent: f32,
    /// The distance from the label to the text, or the width of the label
    /// when it is wider.
    pub text_indent: f32,
    /// How far the label is raised above the baseline.
    pub raise: f32,
    /// The picture of an image bullet and its height, in points, drawn
    /// instead of the text (LST-03).
    pub image: Option<(Picture, f32)>,
}

/// What the fields of a paragraph show: the number of its page and the
/// number of pages (EST-27).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldValues {
    /// The number that the page of the paragraph shows.
    pub page: u32,
    /// The number of pages of the document.
    pub pages: u32,
    /// The marks of the notes, by note (EST-13).
    pub notes: Option<std::sync::Arc<std::collections::HashMap<desk_model::NodeId, String>>>,
    /// The numbers of the pages that paragraphs of the body start on, by
    /// paragraph (EST-22).
    pub pages_of: Option<std::sync::Arc<std::collections::HashMap<desk_model::NodeId, u32>>>,
    /// The width and height of the objects in the text, by object
    /// (OBJ-01): the mark of one that is not here shows nothing.
    pub objects:
        Option<std::sync::Arc<std::collections::HashMap<desk_model::NodeId, (Length, Length)>>>,
}

impl Default for FieldValues {
    fn default() -> Self {
        Self {
            page: 1,
            pages: 1,
            notes: None,
            pages_of: None,
            objects: None,
        }
    }
}

impl FieldValues {
    /// The text that a field shows.
    pub fn text(&self, field: &desk_model::Field) -> String {
        match field {
            desk_model::Field::NoteMark { note } => self
                .notes
                .as_ref()
                .and_then(|n| n.get(note).cloned())
                .unwrap_or_default(),
            desk_model::Field::PageOf { paragraph } => self
                .pages_of
                .as_ref()
                .and_then(|p| p.get(paragraph))
                .map_or_else(|| "1".to_owned(), u32::to_string),
            desk_model::Field::ObjectMark { object } => {
                if self.object_size(*object).is_some() {
                    OBJECT_CHARACTER.to_string()
                } else {
                    String::new()
                }
            }
            other => other.text(self.page, self.pages),
        }
    }

    /// The width and height of an object in the text, in points.
    pub fn object_size(&self, object: desk_model::NodeId) -> Option<(f32, f32)> {
        let (w, h) = self.objects.as_ref()?.get(&object)?;
        Some((w.to_pt() as f32, h.to_pt() as f32))
    }
}

/// The character that stands for an object in the laid out text: a space
/// that does not break, made as wide as the object.
pub(crate) const OBJECT_CHARACTER: char = '\u{a0}';

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
    label: Option<Label>,
    /// The objects in the text: the format of the mark of each, which is
    /// its own, and its width and height (OBJ-01).
    pub(crate) objects: Vec<(u32, f32, f32)>,
}

impl ParagraphText {
    /// Prepares a paragraph of a document, resolving its styles.
    pub fn new(styles: &Styles, paragraph: &Paragraph) -> Self {
        Self::new_with(styles, &TextSettings::default(), paragraph)
    }

    /// Prepares a paragraph of a document with the settings of its text:
    /// whether it is hyphenated and has ligatures at all, and the words
    /// never hyphenated.
    pub fn new_with(styles: &Styles, settings: &TextSettings, paragraph: &Paragraph) -> Self {
        Self::new_with_values(styles, settings, paragraph, FieldValues::default())
    }

    /// Prepares a paragraph of a document, as [`ParagraphText::new_with`]
    /// does, with its fields showing these values.
    pub fn new_with_values(
        styles: &Styles,
        settings: &TextSettings,
        paragraph: &Paragraph,
        values: FieldValues,
    ) -> Self {
        let base = styles.resolve_character(paragraph.style(), None, &Default::default());
        let format = styles.resolve_paragraph(paragraph.style(), paragraph.attrs());
        let runs = paragraph.runs().map(|run| {
            let format = styles.resolve_character(paragraph.style(), run.style, &run.attrs);
            (run.text(), format)
        });
        Self::from_runs_in(settings, values, format, base, runs)
    }

    /// Prepares a paragraph of a document that is an item of a list, as
    /// [`Styles::list_items`] numbers them, or not in a list with `None`.
    pub fn new_in_list(styles: &Styles, paragraph: &Paragraph, item: Option<&ListItem>) -> Self {
        Self::new_in_document(styles, &TextSettings::default(), paragraph, item)
    }

    /// Prepares a paragraph of a document, with the settings of its text,
    /// as an item of a list or not in a list with `None`.
    pub fn new_in_document(
        styles: &Styles,
        settings: &TextSettings,
        paragraph: &Paragraph,
        item: Option<&ListItem>,
    ) -> Self {
        Self::new_in_document_with(styles, settings, paragraph, item, FieldValues::default())
    }

    /// Prepares a paragraph of a document, as
    /// [`ParagraphText::new_in_document`] does, with its fields showing
    /// these values.
    pub fn new_in_document_with(
        styles: &Styles,
        settings: &TextSettings,
        paragraph: &Paragraph,
        item: Option<&ListItem>,
        values: FieldValues,
    ) -> Self {
        let text = Self::new_with_values(styles, settings, paragraph, values);
        match item {
            Some(item) => text.with_label(item),
            None => text,
        }
    }

    /// The same paragraph as an item of a list, with the label of the
    /// item. The label takes the font of the first character, at the size
    /// and in the color of its level, without lines or a background.
    pub fn with_label(mut self, item: &ListItem) -> Self {
        let level = &item.definition;
        let first = self
            .ranges
            .first()
            .map_or(&self.formats[0], |r| &self.formats[r.format as usize]);
        let mut format = first.clone();
        let base = if level.scale_with_text {
            first.font_size.to_pt()
        } else {
            12.0
        };
        format.font_size = Length::from_pt(base * f64::from(level.size) / 100.0);
        if let Some(color) = level.color {
            format.color = color;
        }
        format.underline = LineStyle::None;
        format.strikethrough = LineStyle::None;
        format.background = None;
        format.capitalization = Capitalization::None;
        let size = format.font_size.to_pt() as f32;
        let index = match self.formats.iter().position(|f| f == &format) {
            Some(index) => index,
            None => {
                self.formats.push(format);
                self.formats.len() - 1
            }
        } as u32;
        let image = match &level.label {
            ListLabel::Image(picture) => Some((picture.clone(), size)),
            _ => None,
        };
        self.label = Some(Label {
            image,
            text: item.label.clone(),
            format: index,
            indent: level.label_indent.to_pt() as f32,
            text_indent: level.text_indent.to_pt() as f32,
            raise: level.raise.to_pt() as f32,
        });
        self
    }

    /// The label, for an item of a list.
    pub fn label(&self) -> Option<&Label> {
        self.label.as_ref()
    }

    /// Prepares text from pieces with resolved formats. `base` is the
    /// format that an empty paragraph takes, for the height of its line.
    pub fn from_runs<'a>(
        paragraph: ParaFormat,
        base: CharFormat,
        runs: impl IntoIterator<Item = (&'a str, CharFormat)>,
    ) -> Self {
        Self::from_runs_with(&TextSettings::default(), paragraph, base, runs)
    }

    /// Prepares text from pieces with resolved formats, with the settings
    /// of the text of its document.
    pub fn from_runs_with<'a>(
        settings: &TextSettings,
        paragraph: ParaFormat,
        base: CharFormat,
        runs: impl IntoIterator<Item = (&'a str, CharFormat)>,
    ) -> Self {
        Self::from_runs_in(settings, FieldValues::default(), paragraph, base, runs)
    }

    /// Prepares text from pieces with resolved formats, with the settings
    /// of the text of its document and the values of its fields. A field
    /// takes the place of its character, whole: an offset inside its text
    /// is the offset of the field.
    pub fn from_runs_in<'a>(
        settings: &TextSettings,
        values: FieldValues,
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
            label: None,
            objects: Vec::new(),
        };
        let mut model_offset = 0;
        let mut transformed = false;
        let mut previous: Option<char> = None;
        let hyphenate = out.paragraph.hyphenate && settings.hyphenation;
        for (text, mut format) in runs {
            // A document without ligatures has none anywhere (FMT-09).
            if !settings.ligatures {
                format.ligatures = Ligatures::None;
            }
            // Where words may break with a hyphen, by the patterns of
            // their language (TXT-09).
            let hyphens = if hyphenate && format.field.is_none() {
                hyphen_points(text, format.language.tag(), settings)
            } else {
                Vec::new()
            };
            let run_start = model_offset;
            let index = match out.formats.iter().position(|f| f == &format) {
                Some(index) => index,
                None => {
                    out.formats.push(format.clone());
                    out.formats.len() - 1
                }
            } as u32;
            // Pages joins fi and fl in the fonts of the Mac that have those
            // ligatures, ff and ffi in some faces, and ffl in fewer, and no
            // other letters (DOC-05).
            let pages = (format.capitalization == Capitalization::None
                && format.ligatures != Ligatures::None)
                .then(|| pages_font(&format.font_family, format.weight.value(), format.italic))
                .flatten();
            let ligatures = pages.is_some_and(|f| f.ligatures);
            let ff_ligatures = pages.is_some_and(|f| f.ff_ligatures);
            let ffl_ligatures = pages.is_some_and(|f| f.ffl_ligatures);
            let mut chars = text.chars().peekable();
            while let Some(c) = chars.next() {
                if let Some(field) = &format.field {
                    let start = out.text.len();
                    let shown = values.text(field);
                    out.text.push_str(&shown);
                    out.offsets.push((start, model_offset));
                    if !shown.is_empty() {
                        out.push_range(start..out.text.len(), index, false);
                    }
                    if let desk_model::Field::ObjectMark { object } = field
                        && let Some((w, h)) = values.object_size(*object)
                        && !shown.is_empty()
                    {
                        out.objects.push((index, w, h));
                    }
                    model_offset += c.len_utf8();
                    transformed = true;
                    previous = shown.chars().last();
                    continue;
                }
                if hyphens.binary_search(&(model_offset - run_start)).is_ok() {
                    let start = out.text.len();
                    out.text.push(SOFT_HYPHEN);
                    out.offsets.push((start, model_offset));
                    out.push_range(start..out.text.len(), index, false);
                    transformed = true;
                }
                let start = out.text.len();
                let mut ahead = chars.clone();
                let ligature = match (c, ahead.next(), ahead.next()) {
                    ('f', Some('f'), Some('i')) if ff_ligatures => Some((FFI, 3)),
                    ('f', Some('f'), Some('l')) if ffl_ligatures => Some((FFL, 3)),
                    ('f', Some('f'), _) if ff_ligatures => Some((FF, 2)),
                    ('f', Some('i'), _) if ligatures => Some((FI, 2)),
                    ('f', Some('l'), _) if ligatures => Some((FL, 2)),
                    _ => None,
                };
                if let Some((ligature, letters)) = ligature {
                    for _ in 1..letters {
                        previous = chars.next();
                    }
                    out.text.push(ligature);
                    out.offsets.push((start, model_offset));
                    out.push_range(start..out.text.len(), index, false);
                    model_offset += letters;
                    transformed = true;
                    continue;
                }
                let word_start = !previous.is_some_and(char::is_alphanumeric);
                // As in Pages (FMT-07), everything but capitals is drawn
                // small, spaces included.
                let small = format.capitalization == Capitalization::SmallCaps && !c.is_uppercase();
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

    /// The width and height of the object whose mark has a format, if
    /// one has it.
    pub(crate) fn object_box(&self, format: u32) -> Option<(f32, f32)> {
        self.objects
            .iter()
            .find(|(f, _, _)| *f == format)
            .map(|&(_, w, h)| (w, h))
    }

    /// The paragraph format.
    pub fn paragraph(&self) -> &ParaFormat {
        &self.paragraph
    }

    /// The paragraph format, to change it for where the paragraph goes, as
    /// the space above a note (EST-13).
    pub fn paragraph_mut(&mut self) -> &mut ParaFormat {
        &mut self.paragraph
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
    fn small_caps_mark_all_but_capitals() {
        let text = ParagraphText::from_runs(
            ParaFormat::default(),
            CharFormat::default(),
            [("Ab Cd", format(Capitalization::SmallCaps))],
        );
        assert_eq!(text.text(), "AB CD");
        let small: Vec<_> = text.ranges.iter().map(|r| r.small_caps).collect();
        assert_eq!(small, [false, true, false, true]);
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

    #[test]
    fn est_27_a_field_shows_its_value_whole() {
        let mut field = format(Capitalization::None);
        field.field = Some(desk_model::Field::PAGE_NUMBER);
        let text = ParagraphText::from_runs_in(
            &TextSettings::default(),
            FieldValues {
                page: 12,
                pages: 20,
                notes: None,
                pages_of: None,
                objects: None,
            },
            ParaFormat::default(),
            CharFormat::default(),
            [
                ("p. ", CharFormat::default()),
                ("\u{fffc}", field),
                (".", CharFormat::default()),
            ],
        );
        assert_eq!(text.text(), "p. 12.");
        assert_eq!(text.to_model(3), 3);
        assert_eq!(text.to_model(4), 3);
        assert_eq!(text.to_model(5), 6);
    }
}
