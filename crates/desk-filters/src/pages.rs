// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pages '09 documents to the model, through `desk-iwork`
//! (DOC-22).
//!
//! Named styles become styles of the model, and anonymous styles, which
//! is how Pages saves direct formatting, become attributes of paragraphs
//! and runs. Pages keeps the properties that a style does not set in its
//! own defaults, which are the defaults of the model except for widow
//! control, on in Pages; styles without a parent get every property set,
//! so that they do not inherit from Body as styles of the model would.
//!
//! What the model cannot hold yet, such as headers or list bullets, is
//! left out and recorded in an [`ImportLog`].

use std::collections::{BTreeSet, HashMap, HashSet};

use desk_iwork::pages::{self as iwork, PAGE_BREAK};
use desk_iwork::sf::{
    self as sf, Family, LineSpacingMode, Properties, StrokePattern, Style as IworkStyle,
    Stylesheet, Value,
};
use desk_model::{
    Alignment, Baseline, BorderSides, BorderStyle, Capitalization, CharAttrs, CharFormat,
    CharacterStyle, Document, FontWeight, LINE_BREAK, LIST_LEVELS, Language, Length, Ligatures,
    LineColor, LineSpacing, LineStyle, ListLabel, ListLevel, ListStyle, Metadata, NumberFormat, Op,
    PageSetup, ParaAttrs, ParaBorder, ParaFormat, Paragraph, ParagraphStyle, Rgba, Run, Shadow,
    StyleId, TabAlignment, TabLeader, TabStop, TabStops, TextSettings,
};
use desk_model::{Columns, Layout, MAX_COLUMNS, PARTS, Section, SectionStart, Start};
use desk_model::{Field, MarginKind, MarginPages, MarginPlace};
use desk_model::{NodeId, Note, NoteFormat, NoteKind, NoteNumbering, NoteSettings};
use desk_model::{TocLevel, TocSettings};

use crate::FilterError;
use crate::log::{Content, Formatting, ImportLog, Warning};

/// The identifier of Body in the templates of Pages.
const BODY_IDENT: &str = "paragraph-style-32";
/// The name of Body.
const BODY_NAME: &str = "Body";
/// The root character style of Pages, which sets nothing.
const NULL_CHARACTER: &str = "character-style-null";

/// Reads a Pages document from the bytes of a flat file or of its index.
pub fn read_pages(bytes: &[u8]) -> Result<Document, FilterError> {
    read_pages_logged(bytes).map(|(document, _)| document)
}

/// Reads a Pages document on disk, a package folder or a flat file.
pub fn read_pages_at(path: &std::path::Path) -> Result<Document, FilterError> {
    read_pages_at_logged(path).map(|(document, _)| document)
}

/// Reads a Pages document from bytes, with what was left out.
pub fn read_pages_logged(bytes: &[u8]) -> Result<(Document, ImportLog), FilterError> {
    import_logged(&desk_iwork::read_pages(bytes)?)
}

/// Reads a Pages document on disk, with what was left out.
pub fn read_pages_at_logged(path: &std::path::Path) -> Result<(Document, ImportLog), FilterError> {
    import_logged(&desk_iwork::read_pages_at(path)?)
}

/// The warning that a skipped element of `desk-iwork` stands for.
fn warning(element: &str) -> Warning {
    let content = match element {
        "headers" => Content::Headers,
        "footers" => Content::Footers,
        "drawables" => Content::FloatingObjects,
        "attachment" | "attachment-ref" => Content::InlineObjects,
        "annotation" | "annotation-field" => Content::Comments,
        "page-number" | "page-count" | "date-time" | "filename" | "merge-field" => Content::Fields,
        "change-ref" | "changed-ref" | "changes-ref" => Content::TrackedChanges,
        other => Content::Other(other.to_owned()),
    };
    Warning::Content(content)
}

/// Whether a property is set to something other than `neutral`, which is
/// what Pages saves when it has no effect.
fn differs(value: &Value, neutral: f64) -> bool {
    match value {
        Value::Null => false,
        Value::Number(n) => *n != neutral,
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// The formatting that a property map sets and the model cannot hold.
fn lost_formatting(properties: &Properties, out: &mut BTreeSet<Formatting>) {
    for (name, value) in &properties.0 {
        let lost = match name.as_str() {
            "listStyle" => matches!(value, Value::ListStyle { images: true, .. })
                .then_some(Formatting::ImageBullets),
            "word_underline" | "word_strikethrough" => {
                differs(value, 0.0).then_some(Formatting::LineDetails)
            }
            "dropCap" => differs(value, 0.0).then_some(Formatting::DropCaps),
            "hidden" => differs(value, 0.0).then_some(Formatting::HiddenText),
            "atsuFontFeatures" => differs(value, 0.0).then_some(Formatting::FontFeatures),
            _ => None,
        };
        out.extend(lost);
    }
}

/// The defaults of Pages for what no style sets.
fn pages_paragraph_defaults() -> ParaFormat {
    ParaFormat {
        widow_control: true,
        // Paragraphs allow hyphenation unless their style says otherwise;
        // the document decides whether Pages hyphenates at all (TXT-09).
        hyphenate: true,
        ..ParaFormat::default()
    }
}

/// The weights that the names of faces say, the longer names first, so
/// that `UltraLight` is not read as `Light`, nor `SemiBold` as `Bold`.
const WEIGHTS: [(&str, u16); 18] = [
    ("ultralight", 100),
    ("extralight", 200),
    ("hairline", 100),
    ("thin", 100),
    ("light", 300),
    ("book", 400),
    ("regular", 400),
    ("roman", 400),
    ("medium", 500),
    ("semibold", 600),
    ("demibold", 600),
    ("demi", 600),
    ("extrabold", 800),
    ("ultrabold", 800),
    ("heavy", 800),
    ("black", 900),
    ("ultra", 900),
    ("bold", 700),
];

/// The family, weight and slant of a font from its PostScript name, which
/// is how Pages names fonts: `LiberationSerif-BoldItalic` is Liberation
/// Serif, bold and italic, and `HelveticaNeue-UltraLight` is Helvetica
/// Neue at weight 100.
pub fn font_face(postscript: &str) -> (String, FontWeight, bool) {
    let (base, style) = postscript.split_once('-').unwrap_or((postscript, ""));
    let base = base
        .strip_suffix("PSMT")
        .or_else(|| base.strip_suffix("MT"))
        .or_else(|| base.strip_suffix("PS"))
        .unwrap_or(base);
    let style = style.to_ascii_lowercase();
    let weight = WEIGHTS
        .iter()
        .find(|(name, _)| style.contains(name))
        .map_or(FontWeight::NORMAL, |&(_, w)| FontWeight::new(w));
    let italic = ["italic", "oblique"].iter().any(|w| style.contains(w));
    // Spaces where a lower case letter or a digit meets a capital, or a
    // lower case letter a digit.
    let mut family = String::new();
    let mut previous: Option<char> = None;
    for c in base.chars() {
        let breaks = match previous {
            // TimesNewRoman, and Bodoni72.
            Some(p) => {
                (c.is_uppercase() && (p.is_lowercase() || p.is_ascii_digit()))
                    || (c.is_ascii_digit() && p.is_lowercase())
            }
            None => false,
        };
        if breaks {
            family.push(' ');
        }
        family.push(c);
        previous = Some(c);
    }
    (family, weight, italic)
}

fn flag(properties: &Properties, name: &str) -> Option<bool> {
    properties.number(name).map(|n| n != 0.0)
}

fn points(properties: &Properties, name: &str) -> Option<Length> {
    properties.number(name).map(Length::from_pt)
}

fn line(properties: &Properties, name: &str) -> Option<LineStyle> {
    properties.number(name).map(|n| match n as i64 {
        0 => LineStyle::None,
        2 => LineStyle::Double,
        _ => LineStyle::Single,
    })
}

fn rgba(color: desk_iwork::sf::Color) -> Rgba {
    let byte = |v: f64| (v * 255.0).round() as u8;
    Rgba {
        r: byte(color.r),
        g: byte(color.g),
        b: byte(color.b),
        a: byte(color.a),
    }
}

/// The character attributes that a property map sets.
fn char_attrs(properties: &Properties) -> CharAttrs {
    let mut attrs = CharAttrs {
        font_size: points(properties, "fontSize").filter(|s| s.to_pt() > 0.0),
        italic: flag(properties, "italic"),
        underline: line(properties, "underline"),
        strikethrough: line(properties, "strikethru"),
        baseline_shift: points(properties, "baselineShift"),
        ..CharAttrs::default()
    };
    if let Some(name) = properties.string("fontName").filter(|n| !n.is_empty()) {
        let (family, weight, italic) = font_face(name);
        attrs.font_family = Some(family.as_str().into());
        attrs.weight = Some(weight);
        if italic {
            attrs.italic = Some(true);
        }
    }
    // Bold turns the face bold, or takes the bold off it.
    if let Some(bold) = flag(properties, "bold") {
        attrs.weight = Some(attrs.weight.unwrap_or_default().with_bold(bold));
    }
    attrs.color = match properties.get("fontColor") {
        Some(Value::Color(color)) => Some(rgba(*color)),
        _ => None,
    };
    attrs.underline_color = match properties.get("underlineColor") {
        Some(Value::Color(color)) => Some(LineColor::Custom(rgba(*color))),
        Some(Value::Null) => Some(LineColor::Text),
        _ => None,
    };
    attrs.strikethrough_color = match properties.get("strikethruColor") {
        Some(Value::Color(color)) => Some(LineColor::Custom(rgba(*color))),
        Some(Value::Null) => Some(LineColor::Text),
        _ => None,
    };
    attrs.baseline = properties.number("superscript").map(|n| match n as i64 {
        1 => Baseline::Superscript,
        2 => Baseline::Subscript,
        _ => Baseline::Normal,
    });
    attrs.capitalization = properties.number("capitalization").map(|n| match n as i64 {
        1 => Capitalization::AllCaps,
        2 => Capitalization::SmallCaps,
        3 => Capitalization::Title,
        _ => Capitalization::None,
    });
    attrs.language = match properties.get("language") {
        // Pages saves locales as Cocoa does, such as pt_BR.
        Some(Value::String(tag)) if !tag.is_empty() => Some(Language::new(&tag.replace('_', "-"))),
        Some(Value::Null) => Some(Language::undetermined()),
        _ => None,
    };
    // Pages saves the tracking as a part of the font size.
    attrs.tracking = properties
        .number("tracking")
        .map(|t| (t * 1000.0).round() as i32);
    attrs.outline = flag(properties, "outline");
    attrs.shadow = match properties.get("textShadow") {
        Some(Value::Shadow(shadow)) => Some(Some(Shadow {
            angle: (shadow.angle.round() as i32).rem_euclid(360),
            offset: Length::from_pt(shadow.offset),
            blur: Length::from_pt(shadow.radius),
            opacity: (shadow.opacity * 100.0).round() as u8,
            color: rgba(shadow.color),
        })),
        Some(Value::Null) => Some(None),
        _ => None,
    };
    attrs.ligatures = properties.number("ligatures").map(|n| match n as i64 {
        0 => Ligatures::None,
        2 => Ligatures::All,
        _ => Ligatures::Default,
    });
    attrs.background = color_or_none(properties, "textBackground");
    attrs
}

/// A color property, or none when it is set to nothing.
fn color_or_none(properties: &Properties, name: &str) -> Option<Option<Rgba>> {
    match properties.get(name) {
        Some(Value::Color(color)) => Some(Some(rgba(*color))),
        Some(Value::Null) => Some(None),
        _ => None,
    }
}

/// The rules or border of a paragraph, which Pages saves as three
/// properties that styles may set apart: the line, which sides have it and
/// its distance from the text. When the nearest styles in `chain` up to
/// `own` set any, the border is set from the three, as they resolve.
fn para_border(chain: &[&IworkStyle], own: usize) -> Option<ParaBorder> {
    const NAMES: [&str; 3] = [
        "paragraphStroke",
        "paragraphBorderType",
        "paragraphRuleOffset",
    ];
    let sets = chain[..own]
        .iter()
        .any(|s| NAMES.iter().any(|n| s.properties.get(n).is_some()));
    if !sets {
        return None;
    }
    let stroke = chain
        .iter()
        .find_map(|s| match s.properties.get(NAMES[0])? {
            Value::Stroke(stroke) => Some(Some(stroke)),
            Value::Null => Some(None),
            _ => None,
        });
    let mut border = ParaBorder {
        offset: Length::from_pt(chain_number(chain, NAMES[2]).unwrap_or(6.0)),
        ..ParaBorder::default()
    };
    let Some(Some(stroke)) = stroke else {
        return Some(border);
    };
    border.sides = match chain_number(chain, NAMES[1]).map(|n| n as i64) {
        Some(1) => BorderSides::Above,
        Some(2) => BorderSides::Below,
        Some(3) => BorderSides::AboveAndBelow,
        Some(4) => BorderSides::Box,
        _ => BorderSides::None,
    };
    border.width = Length::from_pt(stroke.width);
    border.color = rgba(stroke.color);
    border.style = match &stroke.pattern {
        StrokePattern::Solid => BorderStyle::Solid,
        StrokePattern::Empty => {
            border.sides = BorderSides::None;
            BorderStyle::Solid
        }
        StrokePattern::Dashes(dashes) if dashes.first().is_some_and(|d| *d <= 1.0) => {
            BorderStyle::Dotted
        }
        StrokePattern::Dashes(_) => BorderStyle::Dashed,
    };
    Some(border)
}

/// The paragraph attributes that a property map sets, without the first
/// line indent, which depends on the left indent.
fn para_attrs(properties: &Properties) -> ParaAttrs {
    ParaAttrs {
        toc_entry: None,
        hyphenate: flag(properties, "hyphenate"),
        alignment: properties.number("alignment").map(|n| match n as i64 {
            1 => Alignment::Right,
            2 => Alignment::Center,
            3 => Alignment::Justify,
            _ => Alignment::Left,
        }),
        line_spacing: match properties.get("lineSpacing") {
            Some(Value::LineSpacing { mode, amount }) if amount.is_finite() => Some(match mode {
                LineSpacingMode::Minimum => LineSpacing::AtLeast(Length::from_pt(*amount)),
                LineSpacingMode::Absolute => LineSpacing::Exactly(Length::from_pt(*amount)),
                LineSpacingMode::Between => LineSpacing::Between(Length::from_pt(*amount)),
                LineSpacingMode::Relative | LineSpacingMode::Other(_) => LineSpacing::Multiple {
                    percent: (amount.clamp(0.0, 100.0) * 100.0).round() as u32,
                },
            }),
            _ => None,
        },
        space_before: points(properties, "spaceBefore"),
        space_after: points(properties, "spaceAfter"),
        left_indent: points(properties, "leftIndent"),
        right_indent: points(properties, "rightIndent"),
        first_line_indent: None,
        page_break_before: flag(properties, "pageBreakBefore"),
        // Column breaks, sections and layouts come from the body.
        column_break_before: None,
        start: None,
        keep_with_next: flag(properties, "keepWithNext"),
        keep_lines_together: flag(properties, "keepLinesTogether"),
        widow_control: flag(properties, "widowControl"),
        tab_stops: match properties.get("tabs") {
            Some(Value::Tabs(tabs)) => {
                let decimal = properties
                    .string("decimalTab")
                    .and_then(|s| s.chars().next())
                    .unwrap_or_else(decimal_separator);
                Some(TabStops::new(
                    tabs.iter()
                        .filter(|t| t.position.is_finite())
                        .map(|tab| TabStop {
                            position: Length::from_pt(tab.position),
                            alignment: match tab.align.as_str() {
                                "center" => TabAlignment::Center,
                                "right" => TabAlignment::Right,
                                "decimal" => TabAlignment::Decimal,
                                _ => TabAlignment::Left,
                            },
                            leader: match tab.leader.as_deref().and_then(|l| l.chars().next()) {
                                None => TabLeader::None,
                                Some('.' | '\u{2026}') => TabLeader::Dots,
                                Some('_') => TabLeader::Line,
                                Some('\u{2192}' | '>') => TabLeader::Arrows,
                                Some(_) => TabLeader::Dashes,
                            },
                            decimal,
                        }),
                ))
            }
            Some(Value::Null) => Some(TabStops::none()),
            _ => None,
        },
        default_tab_interval: properties
            .number("defaultTabStops")
            .filter(|n| *n > 0.0)
            .map(Length::from_pt),
        background: color_or_none(properties, "paragraphFill"),
        // Set from the chain of styles, by `para_border`.
        border: None,
        // Set by the importer, which makes the list styles.
        list_style: None,
        list_level: None,
        list_start: None,
        list_override: None,
    }
}

/// The decimal separator of the system, which Pages lines decimal tab
/// stops up on unless the document says otherwise: a comma in most
/// languages of Europe and South America, and a point elsewhere. It is read
/// from the locale variables, as the C library does.
fn decimal_separator() -> char {
    const COMMA: &[&str] = &[
        "af", "az", "be", "bg", "bs", "ca", "cs", "da", "de", "el", "es", "et", "eu", "fi", "fo",
        "fr", "gl", "hr", "hu", "hy", "id", "is", "it", "ka", "kk", "ky", "lt", "lv", "mk", "mn",
        "nb", "nl", "nn", "no", "pl", "pt", "ro", "ru", "sk", "sl", "sq", "sr", "sv", "tr", "uk",
        "uz", "vi",
    ];
    // Countries where these languages write a point.
    const POINT: &[&str] = &["de_CH", "es_MX", "es_US", "it_CH"];
    let locale = ["LC_ALL", "LC_NUMERIC", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default();
    let tag = locale.split(['.', '@']).next().unwrap_or_default();
    let language = tag.split(['_', '-']).next().unwrap_or_default();
    if COMMA.contains(&language) && !POINT.contains(&tag) {
        ','
    } else {
        '.'
    }
}

/// The value of a number property over a chain of styles, nearest first.
fn chain_number(chain: &[&IworkStyle], name: &str) -> Option<f64> {
    chain.iter().find_map(|s| s.properties.number(name))
}

/// Pages measures the first line indent from the margin, and the model
/// from the left indent. When the nearest styles in `chain` up to `own`
/// set either, the indent is set from both, as they resolve.
fn first_line_indent(chain: &[&IworkStyle], own: usize) -> Option<Length> {
    let sets = chain[..own].iter().any(|s| {
        s.properties.number("firstLineIndent").is_some()
            || s.properties.number("leftIndent").is_some()
    });
    sets.then(|| {
        let first = chain_number(chain, "firstLineIndent").unwrap_or(0.0);
        let left = chain_number(chain, "leftIndent").unwrap_or(0.0);
        Length::from_pt(first - left)
    })
}

/// Attributes from the first `count` styles of a chain, the nearest last
/// to win.
fn merged(chain: &[&IworkStyle], count: usize) -> (ParaAttrs, CharAttrs) {
    let mut para = ParaAttrs::default();
    let mut character = CharAttrs::default();
    for style in chain[..count].iter().rev() {
        para.merge(&para_attrs(&style.properties));
        character.merge(&char_attrs(&style.properties));
    }
    // Bold is a trait of its own over the face: a face chosen here keeps
    // the bold of the styles it is based on (the TOC entries of
    // est-22-toc-page-links).
    let own_bold = chain[..count]
        .iter()
        .any(|s| flag(&s.properties, "bold").is_some());
    if let Some(weight) = character.weight
        && !own_bold
        && let Some(bold) = chain[count..]
            .iter()
            .find_map(|s| flag(&s.properties, "bold"))
    {
        character.weight = Some(weight.with_bold(bold));
    }
    para.first_line_indent = first_line_indent(chain, count);
    para.border = para_border(chain, count);
    (para, character)
}

/// Whether a style is one of the named styles that the model gets.
fn is_named(style: &IworkStyle) -> bool {
    style.name.is_some() && style.ident.is_some() && style.ident.as_deref() != Some(NULL_CHARACTER)
}

struct Importer<'a> {
    sheet: &'a Stylesheet,
    /// The pictures of the package, by path.
    pictures: &'a [(String, desk_model::Picture)],
    document: Document,
    /// The model style of each named style, by identifier.
    ids: HashMap<(Family, String), StyleId>,
    /// The model list style of each list style met, by its object ID or
    /// identifier; `None` for those without labels.
    lists: HashMap<String, Option<StyleId>>,
    body: StyleId,
    /// The ids of the notes, in the order the body refers to them, and
    /// how many references have been read.
    notes: Vec<NodeId>,
    references: usize,
    /// The note whose text is being read.
    note: Option<NodeId>,
    /// The paragraphs of the body by the names of their marks, which the
    /// entries of tables of contents name (EST-22), and the id that the
    /// next paragraph read takes.
    marks: HashMap<String, NodeId>,
    next_id: Option<NodeId>,
}

/// The levels of a list style of Pages, through its parents.
fn list_levels(
    sheet: &Stylesheet,
    style: &sf::ListStyle,
    pictures: &[(String, desk_model::Picture)],
) -> [ListLevel; LIST_LEVELS] {
    let chain = sheet.list_ancestry(style);
    fn get<'a, T>(
        chain: &[&'a sf::ListStyle],
        property: impl Fn(&'a sf::ListStyle) -> Option<&'a Vec<T>>,
        level: usize,
    ) -> Option<&'a T> {
        chain.iter().find_map(|s| property(s)?.get(level))
    }
    std::array::from_fn(|level| {
        let label = get(&chain, |s| s.labels.as_ref(), level);
        let (label, start) = match label {
            None | Some(sf::ListLabel::None) => (ListLabel::None, 1),
            // The picture, or a bullet in its place when it cannot be read.
            Some(sf::ListLabel::Image(path)) => (
                path.as_deref()
                    .and_then(|path| pictures.iter().find(|(p, _)| p == path))
                    .map_or_else(
                        || ListLabel::bullet("\u{2022}"),
                        |(_, picture)| ListLabel::Image(picture.clone()),
                    ),
                1,
            ),
            Some(sf::ListLabel::Text {
                kind,
                format,
                first,
            }) => {
                let number = match kind.as_str() {
                    "decimal" => Some(NumberFormat::Decimal),
                    "upper-roman" => Some(NumberFormat::UpperRoman),
                    "lower-roman" => Some(NumberFormat::LowerRoman),
                    "upper-alpha" => Some(NumberFormat::UpperAlpha),
                    "lower-alpha" => Some(NumberFormat::LowerAlpha),
                    _ => None,
                };
                match number {
                    Some(number) if format.contains("%L") => {
                        let tiered = format.contains("%P") && level > 0;
                        let format = format.replace("%P", "");
                        let (prefix, suffix) = format.split_once("%L").unwrap_or(("", ""));
                        (
                            ListLabel::Number {
                                format: number,
                                prefix: prefix.into(),
                                suffix: suffix.into(),
                                tiered,
                            },
                            *first,
                        )
                    }
                    _ if format.is_empty() => (ListLabel::None, 1),
                    _ => (ListLabel::bullet(format), 1),
                }
            }
        };
        let geometry = get(&chain, |s| s.geometries.as_ref(), level);
        let indent = |property: fn(&sf::ListStyle) -> Option<&Vec<f64>>| {
            Length::from_pt(get(&chain, property, level).copied().unwrap_or(0.0))
        };
        let mut text_indent = indent(|s| s.text_indents.as_ref());
        if matches!(
            get(&chain, |s| s.labels.as_ref(), level),
            Some(sf::ListLabel::Image(_))
        ) {
            // The picture is a square as tall as 12 point text, scaled,
            // and the text starts after it, as after a text bullet that
            // wide.
            let scale = geometry.map_or(1.0, |g| g.scale);
            text_indent = text_indent.max(Length::from_pt(12.0 * scale));
        }
        ListLevel {
            label,
            start,
            label_indent: indent(|s| s.label_indents.as_ref()),
            text_indent,
            size: geometry.map_or(100, |g| (g.scale * 100.0).round().clamp(1.0, 1000.0) as u16),
            scale_with_text: geometry.is_none_or(|g| g.scale_with_text),
            raise: Length::from_pt(geometry.map_or(0.0, |g| g.offset)),
            color: None,
        }
    })
}

impl<'a> Importer<'a> {
    fn new(
        sheet: &'a Stylesheet,
        pictures: &'a [(String, desk_model::Picture)],
    ) -> Result<Self, FilterError> {
        let document = Document::new_word_processing();
        let body = document.styles().default_paragraph();
        let mut importer = Self {
            sheet,
            pictures,
            document,
            ids: HashMap::new(),
            lists: HashMap::new(),
            body,
            notes: Vec::new(),
            references: 0,
            note: None,
            marks: HashMap::new(),
            next_id: None,
        };
        importer.styles()?;
        Ok(importer)
    }

    fn styles(&mut self) -> Result<(), FilterError> {
        let sheet = self.sheet;
        // The named list styles, as the Styles drawer of Pages lists them.
        for style in &sheet.list_styles {
            if style.name.is_some() && style.parent.is_none() {
                let known = style
                    .ident
                    .as_ref()
                    .is_some_and(|ident| self.lists.contains_key(ident));
                if !known {
                    self.list(style)?;
                }
            }
        }
        // Layouts and sections are not styles of the model: each layout and
        // section of the body takes their properties.
        let named: Vec<&IworkStyle> = sheet
            .styles
            .iter()
            .filter(|s| matches!(s.family, Family::Paragraph | Family::Character) && is_named(s))
            .collect();
        let body_style = named
            .iter()
            .find(|s| s.family == Family::Paragraph && s.ident.as_deref() == Some(BODY_IDENT))
            .or_else(|| {
                named
                    .iter()
                    .find(|s| s.family == Family::Paragraph && s.name.as_deref() == Some(BODY_NAME))
            })
            .copied();
        let mut used: HashSet<(Family, String)> = HashSet::new();
        let mut order = Vec::new();
        for style in &named {
            let ident = style.ident.clone().unwrap_or_default();
            if self.ids.contains_key(&(style.family, ident.clone())) {
                continue;
            }
            let id = if body_style.is_some_and(|b| std::ptr::eq(b, *style)) {
                self.body
            } else {
                self.document.new_id()
            };
            let base = style.name.clone().unwrap_or_default();
            let mut name = base.clone();
            let mut n = 2;
            while !used.insert((style.family, name.clone())) {
                name = format!("{base} {n}");
                n += 1;
            }
            self.ids.insert((style.family, ident), id);
            order.push((*style, id, name));
        }
        if body_style.is_none() {
            used.insert((Family::Paragraph, BODY_NAME.to_owned()));
            let mut body = ParagraphStyle::new(self.body, BODY_NAME);
            body.paragraph = pages_paragraph_defaults().into();
            body.character = CharFormat::default().into();
            self.document.put_style(body)?;
        }

        // Parents first, so that every parent exists when a child is put.
        let mut put: HashSet<StyleId> = HashSet::new();
        while put.len() < order.len() {
            let before = put.len();
            for (style, id, name) in &order {
                if put.contains(id) {
                    continue;
                }
                let chain = sheet.ancestry(style);
                let parent = chain
                    .get(1)
                    .and_then(|p| self.ids.get(&(p.family, p.ident.clone()?)).copied());
                let parent = match parent {
                    Some(parent) if parent != *id => Some(parent),
                    _ => None,
                };
                if parent.is_some_and(|p| !put.contains(&p) && p != self.body) {
                    continue;
                }
                let (mut paragraph, mut character) = merged(&[style], 1);
                if parent.is_none() && style.family == Family::Paragraph {
                    // Every property set, from the defaults of Pages.
                    let mut full: ParaAttrs = pages_paragraph_defaults().into();
                    full.merge(&paragraph);
                    paragraph = full;
                    let mut full: CharAttrs = CharFormat::default().into();
                    full.merge(&character);
                    character = full;
                }
                paragraph.first_line_indent =
                    first_line_indent(&chain, 1).or(paragraph.first_line_indent);
                paragraph.border = para_border(&chain, 1).or(paragraph.border);
                match style.family {
                    Family::Paragraph => {
                        let parent = if *id == self.body {
                            None
                        } else {
                            parent.or(Some(self.body))
                        };
                        self.document.put_style(ParagraphStyle {
                            id: *id,
                            name: name.as_str().into(),
                            shortcut: None,
                            parent,
                            next: None,
                            paragraph,
                            character,
                        })?;
                    }
                    Family::Layout | Family::Section => {}
                    Family::Character => {
                        self.document.put_style(CharacterStyle {
                            id: *id,
                            name: name.as_str().into(),
                            shortcut: None,
                            parent: parent
                                .filter(|_| style.parent.as_deref() != Some(NULL_CHARACTER)),
                            character: merged(&[style], 1).1,
                        })?;
                    }
                }
                put.insert(*id);
            }
            if put.len() == before {
                // A parent that cannot be put, as in a cycle: the rest go
                // under Body.
                for (style, id, name) in &order {
                    if put.insert(*id) {
                        let (paragraph, character) = merged(&[style], 1);
                        if style.family == Family::Paragraph {
                            self.document.put_style(ParagraphStyle {
                                id: *id,
                                name: name.as_str().into(),
                                shortcut: None,
                                parent: (*id != self.body).then_some(self.body),
                                next: None,
                                paragraph,
                                character,
                            })?;
                        }
                    }
                }
            }
        }

        // The next styles, once every style they can name exists.
        for (style, id, _) in &order {
            if style.family != Family::Paragraph {
                continue;
            }
            let next = sheet
                .ancestry(style)
                .iter()
                .find_map(|s| s.properties.get("followingParagraphStyle"))
                .and_then(|value| match value {
                    Value::StyleRef(ident) => {
                        self.ids.get(&(Family::Paragraph, ident.clone())).copied()
                    }
                    _ => None,
                });
            if let Some(next) = next.filter(|next| next != id)
                && let Some(mut paragraph) = self.document.styles().paragraph(*id).cloned()
            {
                paragraph.next = Some(next);
                self.document.put_style(paragraph)?;
            }
            // The list style, which the model inherits as Pages does.
            if let Some(value) = style.properties.get("listStyle")
                && let Some(list) = self.list_value(value)?
                && let Some(mut paragraph) = self.document.styles().paragraph(*id).cloned()
                && paragraph.paragraph.list_style != Some(list)
            {
                paragraph.paragraph.list_style = Some(list);
                self.document.put_style(paragraph)?;
            }
        }
        Ok(())
    }

    /// The model list style for a list style of Pages, made when first
    /// met: a named style keeps its name, and an anonymous one, which is
    /// how Pages saves changes to the list of a paragraph, takes the
    /// style with the same levels or a new one named after its parent.
    /// `None` for a list style without labels, as None.
    fn list(&mut self, style: &sf::ListStyle) -> Result<Option<StyleId>, FilterError> {
        let key = style
            .id
            .clone()
            .or_else(|| style.ident.clone())
            .unwrap_or_default();
        if let Some(found) = self.lists.get(&key) {
            return Ok(*found);
        }
        let sheet = self.sheet;
        let levels = list_levels(sheet, style, self.pictures);
        let found = if levels.iter().all(|l| !l.label.is_some()) {
            None
        } else {
            let styles = self.document.styles();
            let same = styles
                .iter()
                .find(|s| matches!(s, desk_model::Style::List(l) if *l.levels == levels))
                .map(desk_model::Style::id);
            match same {
                Some(id) if style.name.is_none() => Some(id),
                _ => {
                    let base = style
                        .name
                        .clone()
                        .or_else(|| {
                            sheet
                                .list_ancestry(style)
                                .iter()
                                .find_map(|s| s.name.clone())
                        })
                        .unwrap_or_else(|| "List".to_owned());
                    let name = styles.unique_name(desk_model::StyleFamily::List, &base);
                    let id = self.document.new_id();
                    self.document.put_style(ListStyle {
                        id,
                        name: name.as_str().into(),
                        shortcut: None,
                        levels: levels.into(),
                    })?;
                    Some(id)
                }
            }
        };
        self.lists.insert(key, found);
        // A named style is met again by its identifier.
        if let Some(ident) = &style.ident
            && style.name.is_some()
        {
            self.lists.entry(ident.clone()).or_insert(found);
        }
        Ok(found)
    }

    /// The list style that a `listStyle` property names.
    fn list_value(&mut self, value: &Value) -> Result<Option<Option<StyleId>>, FilterError> {
        let sheet = self.sheet;
        Ok(match value {
            Value::Null => Some(None),
            Value::ListStyle { id, ident, .. } => {
                match sheet.list_style(id.as_deref(), ident.as_deref()) {
                    Some(style) => Some(self.list(style)?),
                    None => Some(None),
                }
            }
            _ => None,
        })
    }

    /// The named style that a chain starts from, and how many anonymous
    /// styles come before it.
    fn base(&self, chain: &[&IworkStyle], family: Family) -> (Option<StyleId>, usize) {
        for (i, style) in chain.iter().enumerate() {
            if let Some(ident) = &style.ident
                && let Some(&id) = self.ids.get(&(family, ident.clone()))
            {
                return (Some(id), i);
            }
        }
        (None, chain.len())
    }

    /// The paragraphs of the model for a paragraph of Pages: more than one
    /// when it holds page breaks.
    fn paragraph(
        &mut self,
        source: &iwork::Paragraph,
        start: Option<std::sync::Arc<Start>>,
    ) -> Result<Vec<Paragraph>, FilterError> {
        let sheet = self.sheet;
        let mut style = self.body;
        let mut attrs = ParaAttrs::default();
        let mut base = CharAttrs::default();
        if let Some(found) = source.style.as_deref().and_then(|s| sheet.find(s))
            && found.family == Family::Paragraph
        {
            let chain = sheet.ancestry(found);
            let (named, anonymous) = self.base(&chain, Family::Paragraph);
            style = named.unwrap_or(self.body);
            (attrs, base) = merged(&chain, anonymous);
            if let Some(value) = chain[..anonymous]
                .iter()
                .find_map(|s| s.properties.get("listStyle"))
            {
                attrs.list_style = self.list_value(value)?;
            }
        }
        let styles = self.document.styles();
        let format = styles.resolve_paragraph(style, &attrs);
        if let Some(list) = format.list_style.and_then(|id| styles.list(id)) {
            if source.list_level > 0 {
                attrs.list_level = Some(source.list_level);
            }
            if source.restart_list {
                attrs.list_start = Some(Some(list.level(source.list_level).start));
            }
        }
        let mut pieces: Vec<Vec<Run>> = vec![Vec::new()];
        for run in &source.runs {
            let mut run_style = None;
            let mut run_attrs = base.clone();
            if let Some(found) = run.style.as_deref().and_then(|s| sheet.find(s))
                && found.family == Family::Character
            {
                let chain = sheet.ancestry(found);
                let (named, anonymous) = self.base(&chain, Family::Character);
                run_style = named;
                run_attrs.merge(&merged(&chain, anonymous).1);
            }
            if let Some(href) = &run.link {
                run_attrs.link = Some(Some(link(href).into()));
            }
            if let Some(bookmark) = &run.bookmark {
                run_attrs.bookmark = Some(Some(bookmark.as_str().into()));
            }
            if let Some(field) = &run.field {
                let field = match field {
                    iwork::Field::Note { .. } => {
                        let note = self.notes.get(self.references).copied();
                        self.references += 1;
                        // A reference to no note is left out.
                        let Some(note) = note else { continue };
                        Field::NoteMark { note }
                    }
                    iwork::Field::NoteMark => {
                        let Some(note) = self.note else { continue };
                        Field::NoteMark { note }
                    }
                    iwork::Field::PageOf { bookmark } => {
                        // A page number whose paragraph is gone is left out.
                        let Some(&paragraph) = self.marks.get(bookmark) else {
                            continue;
                        };
                        Field::PageOf { paragraph }
                    }
                    field => model_field(field),
                };
                run_attrs.field = Some(Some(field));
            }
            for (i, piece) in run.text.split(PAGE_BREAK).enumerate() {
                if i > 0 {
                    pieces.push(Vec::new());
                }
                let text: String = piece
                    .chars()
                    .map(|c| match c {
                        '\n' | '\r' => ' ',
                        desk_iwork::pages::COLUMN_BREAK | desk_iwork::pages::LINE_BREAK => {
                            LINE_BREAK
                        }
                        c => c,
                    })
                    .collect();
                if !text.is_empty() {
                    pieces
                        .last_mut()
                        .expect("there is a piece")
                        .push(Run::with_attrs(&text, run_style, run_attrs.clone()));
                }
            }
        }
        let mut out = Vec::with_capacity(pieces.len());
        let next_id = self.next_id.take();
        for (i, runs) in pieces.into_iter().enumerate() {
            let mut attrs = attrs.clone();
            if i > 0 {
                attrs.page_break_before = Some(true);
            } else {
                if source.column_break_before {
                    attrs.column_break_before = Some(true);
                }
                attrs.start = start.clone().map(Some);
            }
            if source.toc {
                let target = runs.iter().find_map(|r| match &r.attrs.field {
                    Some(Some(Field::PageOf { paragraph })) => Some(*paragraph),
                    _ => None,
                });
                attrs.toc_entry = Some(Some(desk_model::TocEntry { target }));
            }
            let id = match next_id {
                Some(id) if i == 0 => id,
                _ => self.document.new_id(),
            };
            out.push(Paragraph::from_runs(id, style, attrs, runs)?);
        }
        Ok(out)
    }
}

/// The value of a property of a layout or section style, or of the
/// nearest ancestor that sets it.
fn style_value<'a>(
    sheet: &'a Stylesheet,
    style: &str,
    family: Family,
    name: &str,
) -> Option<&'a Value> {
    let found = sheet.find(style).filter(|s| s.family == family)?;
    sheet
        .ancestry(found)
        .into_iter()
        .find_map(|s| s.properties.get(name))
}

/// A layout of Pages: its columns and margins (EST-07, EST-10).
fn layout(sheet: &Stylesheet, style: Option<&str>) -> Layout {
    let mut out = Layout::default();
    let Some(style) = style else { return out };
    let get = |name| style_value(sheet, style, Family::Layout, name);
    if let Some(Value::Columns { equal, columns }) = get("columns")
        && !columns.is_empty()
    {
        let count = columns.len().min(MAX_COLUMNS);
        let part = |v: f64| (v.clamp(0.0, 1.0) * f64::from(PARTS)).round() as u32;
        let mut set = Columns::equal(count as u8, part(columns[0].1));
        if !*equal {
            set.equal = false;
            for (i, (width, spacing)) in columns.iter().take(count).enumerate() {
                set.widths[i] = part(*width);
                set.gaps[i] = if i + 1 < count { part(*spacing) } else { 0 };
            }
        }
        out.columns = set;
    }
    if let Some(Value::Padding {
        top,
        left,
        bottom,
        right,
    }) = get("layoutMargins")
    {
        let length = |v: f64| Length::from_pt(v.max(0.0));
        out.before = length(*top);
        out.after = length(*bottom);
        out.left = length(*left);
        out.right = length(*right);
    }
    // A layout that is not continuous starts on a new page.
    if let Some(Value::Number(n)) = get("layoutContinuous") {
        out.new_page = *n == 0.0;
    }
    out
}

/// A section of Pages: how it starts and numbers its pages (EST-16,
/// EST-18, EST-19).
fn section(sheet: &Stylesheet, style: Option<&str>) -> Section {
    let mut out = Section::default();
    let Some(style) = style else { return out };
    let number = |name| match style_value(sheet, style, Family::Section, name) {
        Some(Value::Number(n)) => Some(*n),
        _ => None,
    };
    // Kind 1 starts the numbers again at the start number; 0 counts on.
    if number("sectionPageNumberKind") == Some(1.0) {
        out.first_number = Some(number("sectionPageNumberStart").unwrap_or(1.0).max(0.0) as u32);
    }
    out.first_page_different = number("pageMasterFirstPageDifferent").is_some_and(|n| n != 0.0);
    out.left_right_different =
        number("pageMasterLeftRightPagesDifferent").is_some_and(|n| n != 0.0);
    // Pages saves 1 for a right page (est-12-16-section-headers).
    out.start = match number("startKind") {
        Some(1.0) => SectionStart::RightPage,
        Some(2.0) => SectionStart::LeftPage,
        _ => SectionStart::AnyPage,
    };
    // "Use previous headers and footers", on unless the section says.
    out.own_margins = number("inheritPreviousHF") == Some(0.0);
    out
}

/// How Pages writes a page number or count (`sf:format`), as the menu of
/// the field offers: 1, 2, 3; a, b, c; A, B, C; i, ii, iii; I, II, III.
fn number_format(format: Option<&str>) -> NumberFormat {
    match format {
        Some("lower-roman") => NumberFormat::LowerRoman,
        Some("upper-roman") => NumberFormat::UpperRoman,
        Some("lower-alpha" | "lower-letter" | "lower-latin") => NumberFormat::LowerAlpha,
        Some("upper-alpha" | "upper-letter" | "upper-latin") => NumberFormat::UpperAlpha,
        _ => NumberFormat::Decimal,
    }
}

/// A link of Pages as the model keeps it. Pages links another document by
/// its absolute path (`pages:///Users/…/Other.pages#bookmark`), which the
/// model keeps as a file URI, so that it opens where the file still is.
fn link(href: &str) -> String {
    match href.strip_prefix("pages://") {
        Some(rest) if rest.starts_with('/') => format!("file://{rest}"),
        _ => href.to_owned(),
    }
}

/// The field of the model for a field of Pages (EST-27).
fn model_field(field: &iwork::Field) -> Field {
    match field {
        iwork::Field::PageNumber { format } => Field::PageNumber {
            format: number_format(format.as_deref()),
        },
        iwork::Field::PageCount { format } => Field::PageCount {
            format: number_format(format.as_deref()),
        },
        iwork::Field::DateTime {
            format,
            shown,
            update,
        } => Field::DateTime {
            pattern: format.as_str().into(),
            shown: shown.as_str().into(),
            update: *update,
        },
        // Without `sf:display-format`, what it shows is guessed from its
        // text.
        iwork::Field::FileName { shown, display } => Field::FileName {
            shown: shown.as_str().into(),
            path: display.map_or_else(|| shown.contains('/'), |d| d & 1 != 0),
            extension: display.map_or_else(
                || shown.rsplit('/').next().is_some_and(|n| n.contains('.')),
                |d| d & 2 != 0,
            ),
        },
        // The importer numbers the notes.
        iwork::Field::Note { .. } | iwork::Field::NoteMark | iwork::Field::PageOf { .. } => {
            Field::PAGE_NUMBER
        }
    }
}

/// What tables of contents list, from the paragraph styles of Pages
/// (EST-22, EST-23): those shown in them (`showInTOC`), each with the
/// style of its entries (`tocStyle`), which shows page numbers unless it
/// says not to (`SLTOCShowPageNumber`).
fn toc_settings(
    sheet: &Stylesheet,
    ids: &HashMap<(Family, String), StyleId>,
    page_links: bool,
) -> TocSettings {
    let mut levels: Vec<TocLevel> = Vec::new();
    for style in sheet
        .styles
        .iter()
        .filter(|s| s.family == Family::Paragraph)
    {
        let Some(ident) = &style.ident else { continue };
        let Some(&id) = ids.get(&(Family::Paragraph, ident.clone())) else {
            continue;
        };
        let chain = sheet.ancestry(style);
        let get = |name: &str| chain.iter().find_map(|s| s.properties.get(name));
        let shown = matches!(get("showInTOC"), Some(Value::Number(n)) if *n != 0.0);
        let Some(Value::StyleRef(entry)) = get("tocStyle") else {
            continue;
        };
        if !shown || levels.iter().any(|l| l.style == id) {
            continue;
        }
        let Some(entry_style) = sheet.find(entry) else {
            continue;
        };
        // Choosing the style of the entries in the TOC Inspector points to
        // a style of its own over the named one (est-22-toc-page-links).
        let Some(&entry_id) = sheet
            .ancestry(entry_style)
            .iter()
            .find_map(|s| ids.get(&(Family::Paragraph, s.ident.clone()?)))
        else {
            continue;
        };
        let page_number = !matches!(
            sheet
                .ancestry(entry_style)
                .iter()
                .find_map(|s| s.properties.get("SLTOCShowPageNumber")),
            Some(Value::Number(n)) if *n == 0.0
        );
        levels.push(TocLevel {
            style: id,
            entry: entry_id,
            page_number,
        });
    }
    // In the order of the levels of their entries, by name.
    let name = |id: StyleId| -> String {
        sheet
            .styles
            .iter()
            .find(|s| ids.get(&(s.family, s.ident.clone().unwrap_or_default())) == Some(&id))
            .and_then(|s| s.name.clone())
            .unwrap_or_default()
    };
    levels.sort_by_key(|l| name(l.entry));
    TocSettings { levels, page_links }
}

/// How the notes of a document show, from those of Pages (EST-13,
/// EST-15).
fn note_settings(properties: iwork::NoteProperties) -> NoteSettings {
    let defaults = NoteSettings::default();
    NoteSettings {
        kind: match properties.kind {
            1 => NoteKind::DocumentEndnotes,
            2 => NoteKind::SectionEndnotes,
            _ => NoteKind::Footnotes,
        },
        format: match properties.format {
            1 => NoteFormat::Roman,
            2 => NoteFormat::Symbols,
            _ => NoteFormat::Arabic,
        },
        numbering: match properties.numbering {
            1 => NoteNumbering::Page,
            2 => NoteNumbering::Section,
            _ => NoteNumbering::Continuous,
        },
        gap: properties
            .gap
            .map_or(defaults.gap, |g| Length::from_pt(g.max(0.0))),
    }
}

/// The header or footer of the model that a header or footer of Pages is,
/// by its name.
/// Pages names those of the first section as `SFWPDefaultOddHeaderIdentifier`
/// and those of a later section after its style, as
/// `section-style-16-first-page-footer`.
fn margin_place(kind: MarginKind, name: &str) -> Option<MarginPlace> {
    let name = name.to_ascii_lowercase();
    let pages = if name.contains("odd") {
        MarginPages::Default
    } else if name.contains("first") {
        MarginPages::First
    } else if name.contains("even") {
        MarginPages::Left
    } else {
        return None;
    };
    Some(MarginPlace::new(kind, pages))
}

/// Maps a Pages document to the model.
pub fn import(source: &iwork::PagesDocument) -> Result<Document, FilterError> {
    import_logged(source).map(|(document, _)| document)
}

/// The formatting of a paragraph of Pages that the model cannot hold: what
/// the styles of the paragraph and of its runs set.
fn paragraph_losses(sheet: &Stylesheet, paragraph: &iwork::Paragraph) -> BTreeSet<Formatting> {
    let mut lost = BTreeSet::new();
    let styles = std::iter::once(&paragraph.style).chain(paragraph.runs.iter().map(|r| &r.style));
    for style in styles {
        if let Some(found) = style.as_deref().and_then(|s| sheet.find(s)) {
            for style in sheet.ancestry(found) {
                lost_formatting(&style.properties, &mut lost);
            }
        }
    }
    lost
}

/// Maps a Pages document to the model, with what was left out.
pub fn import_logged(source: &iwork::PagesDocument) -> Result<(Document, ImportLog), FilterError> {
    let mut log = ImportLog::default();
    for (element, count) in &source.skipped {
        log.add(warning(element), *count);
    }
    // Image bullets are kept when their pictures can be read.
    let pictures: Vec<(String, desk_model::Picture)> = source
        .files
        .iter()
        .filter_map(|(path, bytes)| Some((path.clone(), crate::picture::picture(bytes)?)))
        .collect();
    let images_kept = source
        .file_paths()
        .iter()
        .all(|path| pictures.iter().any(|(p, _)| p == path));
    // Each paragraph counts once for each kind of formatting it loses.
    for paragraph in &source.body {
        for formatting in paragraph_losses(&source.stylesheet, paragraph) {
            if formatting == Formatting::ImageBullets && images_kept {
                continue;
            }
            log.add(Warning::Formatting(formatting), 1);
        }
    }

    let mut importer = Importer::new(&source.stylesheet, &pictures)?;
    importer.notes = source
        .notes
        .iter()
        .map(|_| importer.document.new_id())
        .collect();
    let first = importer
        .document
        .body()
        .get(0)
        .expect("a new document has a paragraph")
        .id();
    // The first section and layout are those of the page setup; each
    // later one starts at its first paragraph.
    let sheet = &source.stylesheet;
    let first_section = source
        .body
        .first()
        .map_or_else(Section::default, |p| section(sheet, p.section.as_deref()));
    let first_layout = source
        .body
        .first()
        .map_or_else(Layout::default, |p| layout(sheet, p.layout.as_deref()));
    // The ids of the paragraphs that entries of tables of contents name,
    // ahead of them.
    let mut ahead: HashMap<usize, NodeId> = HashMap::new();
    for (i, paragraph) in source.body.iter().enumerate() {
        for mark in &paragraph.marks {
            let id = *ahead.entry(i).or_insert_with(|| importer.document.new_id());
            importer.marks.entry(mark.clone()).or_insert(id);
        }
    }
    let mut index = 1;
    // The sections after the first with headers and footers of their
    // own, by their style, with the paragraph that starts each.
    let mut own_sections: HashMap<&str, NodeId> = HashMap::new();
    for (i, paragraph) in source.body.iter().enumerate() {
        importer.next_id = ahead.get(&i).copied();
        let start = match (&paragraph.section, &paragraph.layout) {
            _ if i == 0 => None,
            (Some(s), l) => Some(Start::Section(
                section(sheet, Some(s)),
                layout(sheet, l.as_deref()),
            )),
            (None, Some(l)) => Some(Start::Layout(layout(sheet, Some(l)))),
            (None, None) => None,
        };
        let own = match (&start, &paragraph.section) {
            (Some(Start::Section(s, _)), Some(style)) if s.own_margins => Some(style.as_str()),
            _ => None,
        };
        let built = importer.paragraph(paragraph, start.map(std::sync::Arc::new))?;
        if let (Some(style), Some(first)) = (own, built.first()) {
            own_sections.insert(style, first.id());
        }
        for built in built {
            importer.document.apply(Op::InsertParagraph {
                index,
                paragraph: built,
            })?;
            index += 1;
        }
    }
    // The headers and footers, with the styles of the body.
    for (kind, parts) in [
        (MarginKind::Header, &source.headers),
        (MarginKind::Footer, &source.footers),
    ] {
        for (name, paragraphs) in parts {
            let Some(place) = margin_place(kind, name) else {
                continue;
            };
            // Those of a later section are named after its style; those of
            // a section that uses the headers before it are not shown.
            let section = if name.starts_with("SFWPDefault") {
                None
            } else {
                match own_sections
                    .iter()
                    .find(|(style, _)| name.starts_with(&format!("{style}-")))
                {
                    Some((_, id)) => Some(*id),
                    None => continue,
                }
            };
            let mut built = Vec::new();
            for paragraph in paragraphs {
                built.extend(importer.paragraph(paragraph, None)?);
            }
            if built.iter().all(Paragraph::is_empty) {
                built.clear();
            }
            importer.document.apply(Op::SetHeaderFooter {
                section,
                place,
                paragraphs: built,
            })?;
        }
    }
    // The notes, with the styles of the body, and the marks of their own
    // that the body gives them.
    let marks: Vec<Option<&str>> = source
        .body
        .iter()
        .flat_map(|p| &p.runs)
        .filter_map(|r| match &r.field {
            Some(iwork::Field::Note { mark }) => Some(mark.as_deref()),
            _ => None,
        })
        .collect();
    for (i, paragraphs) in source.notes.iter().enumerate() {
        let id = importer.notes[i];
        importer.note = Some(id);
        let mut built = Vec::new();
        for paragraph in paragraphs {
            built.extend(importer.paragraph(paragraph, None)?);
        }
        importer.note = None;
        if built.is_empty() {
            built.push(Paragraph::new(importer.document.new_id(), importer.body));
        }
        importer.document.apply(Op::SetNote {
            id,
            note: Some(Note {
                id,
                mark: marks.get(i).copied().flatten().map(Into::into),
                paragraphs: built.into(),
            }),
        })?;
    }
    let toc_ids = importer.ids.clone();
    let mut document = importer.document;
    if index > 1 {
        document.apply(Op::RemoveParagraph { paragraph: first })?;
    }

    let page = source.page;
    let length = |v: f64| Length::from_pt(v);
    document.apply(Op::SetPageSetup {
        page: PageSetup {
            width: length(page.width),
            height: length(page.height),
            margin_top: length(page.top),
            margin_bottom: length(page.bottom),
            margin_left: length(page.left),
            margin_right: length(page.right),
            facing_pages: page.facing,
            section: first_section,
            layout: first_layout,
            headers: source.has_headers,
            footers: source.has_footers,
            header_margin: length(page.header),
            footer_margin: length(page.footer),
            layout_pages: None,
        },
    })?;
    // Whether Pages hyphenates and forms ligatures at all, from the
    // Document Inspector (TXT-09, FMT-09).
    document.apply(Op::SetTextSettings {
        settings: TextSettings {
            hyphenation: source.auto_hyphenation,
            ligatures: source.use_ligatures,
            notes: note_settings(source.note_properties),
            toc: toc_settings(&source.stylesheet, &toc_ids, source.toc_links),
            ..TextSettings::default()
        },
    })?;
    let meta = &source.metadata;
    document.apply(Op::SetMetadata {
        metadata: Metadata {
            title: meta.title.clone(),
            author: meta.authors.join(", "),
            comments: meta.comment.clone(),
            keywords: meta.keywords.clone(),
        },
    })?;
    Ok((document, log))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_faces_come_from_postscript_names() {
        assert_eq!(
            font_face("LiberationSerif"),
            ("Liberation Serif".into(), FontWeight::NORMAL, false)
        );
        assert_eq!(
            font_face("LiberationSans-BoldItalic"),
            ("Liberation Sans".into(), FontWeight::BOLD, true)
        );
        assert_eq!(
            font_face("TimesNewRomanPSMT"),
            ("Times New Roman".into(), FontWeight::NORMAL, false)
        );
        assert_eq!(
            font_face("Helvetica-Oblique"),
            ("Helvetica".into(), FontWeight::NORMAL, true)
        );
        assert_eq!(
            font_face("ArialMT"),
            ("Arial".into(), FontWeight::NORMAL, false)
        );
        assert_eq!(
            font_face("Bodoni72-Book"),
            ("Bodoni 72".into(), FontWeight::NORMAL, false)
        );
        assert_eq!(
            font_face("HelveticaNeue-Bold"),
            ("Helvetica Neue".into(), FontWeight::BOLD, false)
        );
        // Pages '09 documents use the light faces of Helvetica Neue.
        for (name, weight, italic) in [
            ("HelveticaNeue-UltraLight", 100, false),
            ("HelveticaNeue-LightItalic", 300, true),
            ("HelveticaNeue-Medium", 500, false),
            ("Futura-CondensedExtraBold", 800, false),
            ("Avenir-Black", 900, false),
            ("Optima-ExtraBlack", 900, false),
            ("Baskerville-SemiBold", 600, false),
        ] {
            assert_eq!(font_face(name).1, FontWeight::new(weight), "{name}");
            assert_eq!(font_face(name).2, italic, "{name}");
        }
    }
}
