// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! A text document (ODT) as typed values, close to the XML.
//!
//! This covers what Desk reads and writes so far: paragraphs and spans of
//! text, paragraph and text styles with their properties, the page layout
//! and the document information. What else a file holds is dropped when
//! it is read: lists and tables are read as their paragraphs, and frames,
//! notes and fields are left out.

use crate::units::{Color, Length};

/// The line break inside a paragraph, which `text:line-break` holds. It is
/// U+2028 LINE SEPARATOR, as in the Desk model.
pub const LINE_BREAK: char = '\u{2028}';

/// A text document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextDocument {
    /// The document information, from `meta.xml`.
    pub meta: Meta,
    /// The defaults of every paragraph style (`style:default-style`).
    pub default_style: Properties,
    /// The common styles, which the user sees by name.
    pub styles: Vec<Style>,
    /// The automatic styles, which hold the formatting set directly on
    /// paragraphs and text.
    pub automatic_styles: Vec<Style>,
    /// The size and margins of the pages.
    pub page_layout: PageLayout,
    /// The paragraphs of the body.
    pub body: Vec<Paragraph>,
    /// The `TabsRelativeToIndent` setting of `settings.xml`: whether the
    /// positions of tab stops are measured from the indent of the
    /// paragraph. LibreOffice takes it as set when it is missing.
    pub tabs_relative_to_indent: Option<bool>,
}

impl TextDocument {
    /// A style by family and name, among the automatic styles first.
    pub fn style(&self, family: Family, name: &str) -> Option<&Style> {
        self.automatic_styles
            .iter()
            .chain(&self.styles)
            .find(|s| s.family == family && s.name == name)
    }
}

/// The document information.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    /// `dc:title`.
    pub title: Option<String>,
    /// `dc:subject`.
    pub subject: Option<String>,
    /// `dc:description`, the comments.
    pub description: Option<String>,
    /// `meta:keyword`, one per keyword.
    pub keywords: Vec<String>,
    /// `meta:initial-creator`, who made the document.
    pub initial_creator: Option<String>,
    /// `dc:creator`, who saved it last.
    pub creator: Option<String>,
    /// `meta:generator`, the program that saved it.
    pub generator: Option<String>,
}

/// The family of a style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Family {
    /// A paragraph style, which may also set text properties.
    #[default]
    Paragraph,
    /// A text style, for spans.
    Text,
}

/// A style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    /// `style:name`, the name that the content refers to.
    pub name: String,
    /// `style:display-name`, the name shown, when it differs.
    pub display_name: Option<String>,
    /// `style:family`.
    pub family: Family,
    /// `style:parent-style-name`.
    pub parent: Option<String>,
    /// `style:next-style-name`.
    pub next: Option<String>,
    /// Properties of the properties elements.
    pub properties: Properties,
}

impl Style {
    /// The name shown to the user.
    pub fn shown_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

/// The paragraph and text properties of a style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Properties {
    /// `style:paragraph-properties`.
    pub paragraph: ParagraphProperties,
    /// `style:text-properties`.
    pub text: TextProperties,
}

/// A line under or through text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// No line.
    None,
    /// One line.
    Single,
    /// Two lines.
    Double,
}

/// The color of a line through text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineColor {
    /// The color of the text (`font-color`).
    FontColor,
    /// A color of its own.
    Color(Color),
}

/// `style:text-position`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextPosition {
    /// `super`, raised as the application sees fit.
    Super,
    /// `sub`, lowered as the application sees fit.
    Sub,
    /// Raised by a percentage of the font size, lowered when negative.
    Percent(f64),
}

/// The text properties Desk reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextProperties {
    /// The font family, from `style:font-name` through the font face
    /// declarations, or from `fo:font-family`.
    pub font_family: Option<String>,
    /// `fo:font-size`, when it is a length.
    pub font_size: Option<Length>,
    /// `fo:font-size`, when it is a percentage of the parent style.
    pub font_size_percent: Option<f64>,
    /// `fo:font-weight` as a number, 400 for `normal` and 700 for `bold`.
    pub font_weight: Option<u16>,
    /// `fo:font-style`: true for italic or oblique.
    pub italic: Option<bool>,
    /// `style:text-underline-style` and `style:text-underline-type`.
    pub underline: Option<Line>,
    /// `style:text-line-through-style` and `style:text-line-through-type`.
    pub line_through: Option<Line>,
    /// `style:text-line-through-color`.
    pub line_through_color: Option<LineColor>,
    /// `fo:color`.
    pub color: Option<Color>,
    /// `style:text-position`.
    pub position: Option<TextPosition>,
    /// The size of raised or lowered text, as a percentage, from the second
    /// value of `style:text-position`.
    pub position_scale: Option<f64>,
    /// `fo:text-transform`.
    pub transform: Option<TextTransform>,
    /// `fo:font-variant`: true for small capitals.
    pub small_caps: Option<bool>,
    /// `fo:language`.
    pub language: Option<String>,
    /// `fo:country`.
    pub country: Option<String>,
    /// `fo:script`.
    pub script: Option<String>,
}

/// `fo:text-transform`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextTransform {
    /// `none`.
    None,
    /// `uppercase`.
    Uppercase,
    /// `lowercase`.
    Lowercase,
    /// `capitalize`.
    Capitalize,
}

/// `fo:text-align`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    /// `start`.
    Start,
    /// `left`.
    Left,
    /// `center`.
    Center,
    /// `end`.
    End,
    /// `right`.
    Right,
    /// `justify`.
    Justify,
}

/// `fo:line-height`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    /// `normal`.
    Normal,
    /// A percentage of the normal height.
    Percent(f64),
    /// An exact height.
    Exactly(Length),
}

/// `fo:break-before`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Break {
    /// `auto`: no break.
    Auto,
    /// `page`.
    Page,
    /// `column`.
    Column,
}

/// The paragraph properties Desk reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParagraphProperties {
    /// `fo:text-align`.
    pub align: Option<TextAlign>,
    /// `fo:line-height`.
    pub line_height: Option<LineHeight>,
    /// `style:line-height-at-least`.
    pub line_height_at_least: Option<Length>,
    /// `style:line-spacing`, the space added between lines.
    pub line_spacing: Option<Length>,
    /// `fo:margin-top`, the space above.
    pub margin_top: Option<Length>,
    /// `fo:margin-bottom`, the space below.
    pub margin_bottom: Option<Length>,
    /// `fo:margin-left`.
    pub margin_left: Option<Length>,
    /// `fo:margin-right`.
    pub margin_right: Option<Length>,
    /// `fo:text-indent`, the indent of the first line.
    pub text_indent: Option<Length>,
    /// `fo:break-before`.
    pub break_before: Option<Break>,
    /// `fo:keep-with-next`: true for `always`.
    pub keep_with_next: Option<bool>,
    /// `fo:keep-together`: true for `always`.
    pub keep_together: Option<bool>,
    /// `fo:widows`.
    pub widows: Option<u32>,
    /// `fo:orphans`.
    pub orphans: Option<u32>,
    /// `style:tab-stops`: the stops, empty when the element is there with
    /// none, which takes away those of the parent style.
    pub tab_stops: Option<Vec<TabStop>>,
    /// `style:tab-stop-distance`, the distance between default stops.
    pub tab_stop_distance: Option<Length>,
}

/// `style:type` of a tab stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabType {
    /// `left`.
    #[default]
    Left,
    /// `center`.
    Center,
    /// `right`.
    Right,
    /// `char`: aligned on `style:char`.
    Char,
}

/// `style:leader-style` of a tab stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LeaderStyle {
    /// `none`.
    #[default]
    None,
    /// `solid`.
    Solid,
    /// `dotted`.
    Dotted,
    /// `dash`, and the other dashed styles.
    Dash,
}

/// `style:tab-stop`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabStop {
    /// `style:position`, from the left margin of the paragraph, or from
    /// its indent when [`TextDocument::tabs_relative_to_indent`] is set.
    pub position: Length,
    /// `style:type`.
    pub kind: TabType,
    /// `style:char`, for a `char` stop.
    pub char: Option<char>,
    /// `style:leader-style`.
    pub leader_style: LeaderStyle,
    /// `style:leader-text`.
    pub leader_text: Option<char>,
}

/// The size and margins of the pages.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PageLayout {
    /// `fo:page-width`.
    pub width: Option<Length>,
    /// `fo:page-height`.
    pub height: Option<Length>,
    /// `fo:margin-top`.
    pub margin_top: Option<Length>,
    /// `fo:margin-bottom`.
    pub margin_bottom: Option<Length>,
    /// `fo:margin-left`.
    pub margin_left: Option<Length>,
    /// `fo:margin-right`.
    pub margin_right: Option<Length>,
}

/// A paragraph or heading of the body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paragraph {
    /// `text:style-name`.
    pub style: Option<String>,
    /// `text:outline-level`, for a heading (`text:h`).
    pub outline_level: Option<u8>,
    /// The text, in spans of one style each.
    pub spans: Vec<Span>,
}

impl Paragraph {
    /// The text of the paragraph.
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// A piece of a paragraph with one text style, or none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// `text:style-name` of the span, a text style.
    pub style: Option<String>,
    /// The text, with spaces as they are shown, tabs as U+0009 and line
    /// breaks as [`LINE_BREAK`].
    pub text: String,
}
