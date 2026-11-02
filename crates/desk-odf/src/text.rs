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
    /// The function keys that apply styles, which Desk keeps in
    /// `settings.xml` because ODF has no attribute for them.
    pub style_shortcuts: Vec<StyleShortcut>,
    /// The common list styles (`text:list-style` in `office:styles`).
    pub list_styles: Vec<ListStyle>,
    /// The automatic list styles, which other programs write for lists
    /// formatted directly.
    pub automatic_list_styles: Vec<ListStyle>,
    /// The settings of the text that Desk keeps in `settings.xml`.
    pub text_settings: TextSettings,
    /// The pictures of the package that the document links to, by path,
    /// as `Pictures/bullet1.png`, with their bytes.
    pub pictures: Vec<(String, Vec<u8>)>,
    /// The section styles, common and automatic, which the sections of
    /// the body name.
    pub section_styles: Vec<SectionStyle>,
    /// The headers and footers of the master page.
    pub master: MasterPage,
    /// The automatic styles of `styles.xml`, which the paragraphs of the
    /// headers and footers name.
    pub master_styles: Vec<Style>,
}

/// The headers and footers of a master page (`style:master-page`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MasterPage {
    /// `style:header`: the header of every page that has no other.
    pub header: Option<HeaderFooter>,
    /// `style:header-first`: the header of the first page.
    pub header_first: Option<HeaderFooter>,
    /// `style:header-left`: the header of left pages.
    pub header_left: Option<HeaderFooter>,
    /// `style:footer`.
    pub footer: Option<HeaderFooter>,
    /// `style:footer-first`.
    pub footer_first: Option<HeaderFooter>,
    /// `style:footer-left`.
    pub footer_left: Option<HeaderFooter>,
}

/// A header or a footer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeaderFooter {
    /// `style:display`: whether pages show it; false keeps its text
    /// hidden.
    pub display: bool,
    /// Its paragraphs.
    pub paragraphs: Vec<Paragraph>,
}

/// The size of the header or the footer of a page layout
/// (`style:header-footer-properties`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeaderFooterStyle {
    /// `fo:min-height`: the height it takes at least.
    pub min_height: Option<Length>,
    /// The space between it and the body: `fo:margin-bottom` of a
    /// header, `fo:margin-top` of a footer.
    pub spacing: Option<Length>,
}

/// The columns of a section or of the pages (`style:columns`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Columns {
    /// `fo:column-count`.
    pub count: u8,
    /// `fo:column-gap`, between columns as wide as each other, when the
    /// columns are not given one by one.
    pub gap: Option<Length>,
    /// The columns one by one (`style:column`), when given.
    pub columns: Vec<Column>,
}

/// One column (`style:column`): its part of the width, with the space on
/// either side of it, which is half of the gap to its neighbor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Column {
    /// `style:rel-width`: its part of the width, out of the sum of the
    /// parts of every column, gaps included.
    pub rel_width: u32,
    /// `fo:start-indent`.
    pub start_indent: Length,
    /// `fo:end-indent`.
    pub end_indent: Length,
}

/// The page a section starts on (`desk:section-start`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionStart {
    /// `any`: the next page.
    Any,
    /// `left`: the next left page.
    Left,
    /// `right`: the next right page.
    Right,
}

/// A section style (`style:family="section"`), which sets the columns
/// and margins of the text of a section.
///
/// A section of Pages, which starts on a new page and numbers its pages,
/// has the `desk:` attributes of one; the first paragraph of it also
/// breaks the page and sets its number, for other programs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SectionStyle {
    /// `style:name`.
    pub name: String,
    /// `fo:margin-left`.
    pub margin_left: Option<Length>,
    /// `fo:margin-right`.
    pub margin_right: Option<Length>,
    /// `style:columns`.
    pub columns: Option<Columns>,
    /// `desk:space-before`: the space above the section.
    pub space_before: Option<Length>,
    /// `desk:space-after`: the space below the section.
    pub space_after: Option<Length>,
    /// `desk:new-page`: whether the section starts on a new page.
    pub new_page: Option<bool>,
    /// `desk:section-start`: the page that a section of Pages starts on,
    /// when the section is one.
    pub section_start: Option<SectionStart>,
    /// `desk:first-page-different`.
    pub first_page_different: Option<bool>,
    /// `desk:left-right-different`.
    pub left_right_different: Option<bool>,
}

/// The settings of the text of a document, which ODF has no place for and
/// Desk keeps in `settings.xml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextSettings {
    /// Whether the document is hyphenated, when it says.
    pub hyphenation: Option<bool>,
    /// Whether the document uses ligatures, when it says.
    pub ligatures: Option<bool>,
    /// The words never hyphenated.
    pub never_hyphenate: Vec<String>,
    /// Whether the space between two paragraphs is the larger of the space
    /// below the first and the space above the second, as in Pages, rather
    /// than their sum, as in other programs. Desk writes it as true.
    pub larger_spacing: Option<bool>,
}

/// A list style (`text:list-style`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListStyle {
    /// `style:name`.
    pub name: String,
    /// `style:display-name`, the name shown, when it differs.
    pub display_name: Option<String>,
    /// The levels it sets, in any order.
    pub levels: Vec<ListLevelStyle>,
}

impl ListStyle {
    /// The name shown to the user.
    pub fn shown_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

/// What starts the items of a level.
#[derive(Clone, Debug, PartialEq)]
pub enum ListLabel {
    /// `text:list-level-style-bullet`, with its `text:bullet-char`.
    Bullet(char),
    /// `text:list-level-style-number`.
    Number {
        /// `style:num-format`: `1`, `I`, `i`, `A`, `a`, or empty for no
        /// number, when only the prefix and the suffix show.
        format: String,
        /// `style:num-prefix`.
        prefix: String,
        /// `style:num-suffix`.
        suffix: String,
        /// `text:display-levels`: how many levels the label shows, as 3
        /// for 1.1.1.
        display_levels: u8,
        /// `text:start-value`.
        start: u32,
    },
    /// `text:list-level-style-image`: a picture of the package.
    Image {
        /// `xlink:href`: the path of the picture in the package, as
        /// `Pictures/bullet1.png`.
        href: String,
        /// `fo:width` of `style:list-level-properties`.
        width: Option<Length>,
        /// `fo:height` of `style:list-level-properties`.
        height: Option<Length>,
        /// `desk:raise`: how far the picture is raised from where it
        /// stands, lowered when negative, which ODF has no attribute for.
        raise: Option<Length>,
    },
}

/// A level of a list style.
#[derive(Clone, Debug, PartialEq)]
pub struct ListLevelStyle {
    /// `text:level`, from 1.
    pub level: u8,
    /// The label.
    pub label: ListLabel,
    /// `text:space-before` of `style:list-level-properties`: from the
    /// indent of the paragraph to the label. With the label alignment
    /// mode of LibreOffice, from its margin and text indent.
    pub space_before: Option<Length>,
    /// `text:min-label-width`: from the label to the text, or the width
    /// of the label when it is wider.
    pub min_label_width: Option<Length>,
    /// `text:bullet-relative-size`, in percent; for a picture,
    /// `desk:relative-size`, its height in percent of the size of the text,
    /// which ODF has no attribute for.
    pub relative_size: Option<f64>,
    /// `style:text-properties` of the label.
    pub text: TextProperties,
}

/// A function key that applies a style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleShortcut {
    /// The function key, from 1 for F1 to 8 for F8.
    pub key: u8,
    /// The family of the style.
    pub family: Family,
    /// The `style:name` of the style.
    pub style: String,
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
    /// A list style (`text:list-style`), which is not a `style:style`.
    List,
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
    /// `style:list-style-name`, of a paragraph style: empty for no list.
    pub list_style: Option<String>,
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
    /// `fo:letter-spacing`, zero for `normal`.
    pub letter_spacing: Option<Length>,
    /// `style:text-outline`.
    pub outline: Option<bool>,
    /// `fo:text-shadow`.
    pub shadow: Option<TextShadow>,
    /// `fo:background-color`.
    pub background: Option<Background>,
    /// `fo:hyphenate`: whether words break with hyphens at the ends of
    /// lines.
    pub hyphenate: Option<bool>,
    /// `desk:hyphenate`: whether the paragraph allows hyphens when its
    /// document hyphenates, written when the document does not, which
    /// then writes `fo:hyphenate` false for other programs.
    pub desk_hyphenate: Option<bool>,
    /// `desk:shadow-opacity`: the opacity of `fo:text-shadow`, in percent,
    /// which ODF has no field for.
    pub shadow_opacity: Option<f64>,
    /// `desk:tracking`: the spacing of letters in thousandths of the font
    /// size, as Pages measures it, beside the length of
    /// `fo:letter-spacing` that other programs read.
    pub tracking: Option<i32>,
}

/// `fo:text-shadow`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextShadow {
    /// `none`.
    None,
    /// A shadow, written as in CSS: the offsets right and down, then the
    /// blur and the color when there are.
    Shadow {
        /// The offset to the right.
        dx: Length,
        /// The offset down.
        dy: Length,
        /// The radius of the blur.
        blur: Option<Length>,
        /// The color, the color of the text when missing.
        color: Option<Color>,
    },
}

/// `fo:background-color`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    /// `transparent`.
    Transparent,
    /// A color.
    Color(Color),
}

/// The style of a border line, from `fo:border` and its sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderLine {
    /// `solid`, and the styles Desk draws as solid, as `double`.
    Solid,
    /// `dashed`.
    Dashed,
    /// `dotted`.
    Dotted,
}

/// A side of `fo:border`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Border {
    /// `none`, or a hidden line.
    None,
    /// A line.
    Line {
        /// The width.
        width: Length,
        /// The style.
        style: BorderLine,
        /// The color.
        color: Color,
    },
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
    /// `style:page-number`: the number of the page that a paragraph
    /// starting a page has, or `Some(None)` for `auto`, which counts on.
    pub page_number: Option<Option<u32>>,
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
    /// `fo:background-color`.
    pub background: Option<Background>,
    /// `fo:border-top`, or `fo:border`.
    pub border_top: Option<Border>,
    /// `fo:border-bottom`, or `fo:border`.
    pub border_bottom: Option<Border>,
    /// `fo:border-left`, or `fo:border`.
    pub border_left: Option<Border>,
    /// `fo:border-right`, or `fo:border`.
    pub border_right: Option<Border>,
    /// `fo:padding-top`, or `fo:padding`: the distance between the text
    /// and its top border.
    pub padding_top: Option<Length>,
    /// `fo:padding-bottom`, or `fo:padding`.
    pub padding_bottom: Option<Length>,
    /// `fo:padding-left`, or `fo:padding`.
    pub padding_left: Option<Length>,
    /// `fo:padding-right`, or `fo:padding`.
    pub padding_right: Option<Length>,
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
#[derive(Clone, Debug, Default, PartialEq)]
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
    /// `style:page-usage="mirrored"`: whether left and right pages swap
    /// their margins, as pages facing each other.
    pub mirrored: Option<bool>,
    /// `style:columns`: the columns of every page.
    pub columns: Option<Columns>,
    /// `style:header-style`, when the pages have a header.
    pub header: Option<HeaderFooterStyle>,
    /// `style:footer-style`, when the pages have a footer.
    pub footer: Option<HeaderFooterStyle>,
}

/// A paragraph or heading of the body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paragraph {
    /// `text:style-name`.
    pub style: Option<String>,
    /// `text:outline-level`, for a heading (`text:h`).
    pub outline_level: Option<u8>,
    /// The list the paragraph is an item of, when it is in a `text:list`.
    pub list: Option<ListMember>,
    /// The section style of the `text:section` that starts at the
    /// paragraph, which goes on to the next one that starts. An empty
    /// name, when read, is the end of a section, after which the text
    /// takes the columns of the pages again.
    pub section: Option<String>,
    /// The text, in spans of one style each.
    pub spans: Vec<Span>,
}

/// Where a paragraph is in a list.
///
/// Neighbors with the same list style are written in one `text:list`, with
/// the deeper levels nested in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListMember {
    /// `text:style-name` of the list: its list style.
    pub style: String,
    /// The level, from 0 for the outermost list.
    pub level: u8,
    /// `text:start-value` of the list item.
    pub start: Option<u32>,
    /// `text:style-override` of the list item: a list style whose level
    /// the item has instead of the one of its list.
    pub style_override: Option<String>,
    /// Whether it is the first item of its outermost `text:list`.
    pub first: bool,
    /// `text:continue-numbering` of its outermost `text:list`: whether
    /// the numbers go on from the list before it with the same style.
    pub continues: bool,
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
    /// breaks as [`LINE_BREAK`]. For a field, the text it shows.
    pub text: String,
    /// The field that the span is, when it is one.
    pub field: Option<Field>,
}

/// A field of the text, which shows a value that the program fills in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    /// `text:page-number`, of the current page.
    PageNumber,
    /// `text:page-count`.
    PageCount,
    /// `text:date`, fixed at the date it shows. `desk:pattern` keeps the
    /// pattern of the date in the symbols of Unicode.
    Date {
        /// The pattern, as `EEEE, d MMMM y`.
        pattern: Option<String>,
    },
    /// `text:file-name`, fixed at the name it shows.
    FileName,
}
