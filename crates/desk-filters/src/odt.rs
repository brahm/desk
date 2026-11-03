// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OpenDocument text (ODT) to and from the document model.
//!
//! Common ODF styles become model styles. Automatic styles, which ODF uses
//! for formatting set directly on paragraphs and text, become attributes
//! on the paragraphs and runs, and are made again when writing. The
//! default paragraph style of the model is written as the ODF style named
//! `Standard`, which is what other programs use for it.

use std::collections::{HashMap, HashSet};
use std::fmt;

use desk_model::{
    Alignment, Baseline, BorderSides, BorderStyle, Capitalization, CharAttrs, CharFormat,
    CharacterStyle, DEFAULT_TAB_INTERVAL, Document, FontFeature, FontFeatures, FontWeight,
    LINE_BREAK, LIST_LEVELS, Language, Length, Ligatures, LineColor, LineSpacing, LineStyle,
    ListLabel, ListLevel, ListStyle, Metadata, ModelError, NumberFormat, Op, PageSetup, ParaAttrs,
    ParaBorder, ParaFormat, Paragraph, ParagraphSpacing, ParagraphStyle, Rgba, Run, Shadow, Style,
    StyleId, TabAlignment, TabLeader, TabStop, TabStops, TextSettings,
};
use desk_model::{
    FIELD_CHARACTER, Field, Layout, MarginKind, MarginPages, MarginPlace, NodeId, Note, NoteFormat,
    NoteKind, NoteNumbering, NoteSettings, Section, Start, TocEntry, TocLevel, TocSettings,
};
use desk_odf as odf;

use crate::sections;

/// The ODF name of the default paragraph style.
const DEFAULT_STYLE: &str = "Standard";

/// The name of the default paragraph style of a file without a `Standard`
/// style.
const DEFAULT_NAME: &str = "Body";

/// The size of superscripts and subscripts written, in percent. Desk
/// draws them at this size whatever a file says.
const SCRIPT_SCALE: f64 = 66.0;

/// Why a document could not be read or written.
#[derive(Debug)]
pub enum FilterError {
    /// The ODF package could not be read or written.
    Odf(odf::OdfError),
    /// The iWork document could not be read.
    Iwork(desk_iwork::IworkError),
    /// The document could not be built.
    Model(ModelError),
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Odf(error) => error.fmt(f),
            Self::Iwork(error) => error.fmt(f),
            Self::Model(error) => write!(f, "the document could not be built: {error:?}"),
        }
    }
}

impl std::error::Error for FilterError {}

impl From<odf::OdfError> for FilterError {
    fn from(error: odf::OdfError) -> Self {
        Self::Odf(error)
    }
}

impl From<desk_iwork::IworkError> for FilterError {
    fn from(error: desk_iwork::IworkError) -> Self {
        Self::Iwork(error)
    }
}

impl From<ModelError> for FilterError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

/// Reads an ODT file into a document.
pub fn read_odt(bytes: &[u8]) -> Result<Document, FilterError> {
    import(&odf::read_odt(bytes)?)
}

/// Writes a document as an ODT file.
pub fn write_odt(document: &Document) -> Result<Vec<u8>, FilterError> {
    Ok(odf::write_odt(&export(document))?)
}

/// Writes a document as ODT with a preview of its first page, a PNG image
/// (DOC-09).
pub fn write_odt_with_thumbnail(
    document: &Document,
    thumbnail: Option<&[u8]>,
) -> Result<Vec<u8>, FilterError> {
    Ok(odf::write_odt_with_thumbnail(&export(document), thumbnail)?)
}

fn length(value: odf::Length) -> Length {
    Length::from_pt(value.0)
}

fn odf_length(value: Length) -> odf::Length {
    odf::Length(value.to_pt())
}

fn line_style(line: odf::Line) -> LineStyle {
    match line {
        odf::Line::None => LineStyle::None,
        odf::Line::Single => LineStyle::Single,
        odf::Line::Double => LineStyle::Double,
    }
}

fn rgba(color: odf::Color) -> Rgba {
    Rgba::rgb(color.r, color.g, color.b)
}

fn odf_color(color: Rgba) -> odf::Color {
    odf::Color {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

/// The family of a font name as LibreOffice writes it, with the features
/// of the font after a colon, as in `Linux Libertine G:liga=0&smcp`, and
/// the ligatures and features it sets (FMT-08, FMT-09).
fn font_name(name: &str) -> (&str, Option<Ligatures>, Option<FontFeatures>) {
    let Some((family, settings)) = name.split_once(':') else {
        return (name, None, None);
    };
    let mut ligatures = None;
    let mut features = Vec::new();
    for setting in settings.split('&') {
        let (tag, value) = match setting.split_once('=') {
            Some((tag, value)) => (tag.trim(), value.trim().parse().unwrap_or(1)),
            None => (setting.trim(), 1),
        };
        match (tag, value) {
            ("liga", 0) => ligatures = Some(Ligatures::None),
            ("liga", _) => ligatures = ligatures.or(Some(Ligatures::Default)),
            ("dlig", v) if v > 0 => ligatures = Some(Ligatures::All),
            ("clig", _) | ("dlig", _) => {}
            _ => features.extend(FontFeature::new(tag, value)),
        }
    }
    let features = (!features.is_empty()).then(|| FontFeatures::new(features));
    (family, ligatures, features)
}

/// The font name of a family with ligatures and features, as LibreOffice
/// writes it.
fn odf_font_name(
    family: &str,
    ligatures: Option<Ligatures>,
    features: Option<&FontFeatures>,
) -> String {
    let mut settings: Vec<String> = match ligatures {
        Some(Ligatures::Default) => vec!["liga".into()],
        Some(Ligatures::None) => vec!["liga=0".into(), "clig=0".into()],
        Some(Ligatures::All) => vec!["dlig".into()],
        None => Vec::new(),
    };
    for feature in features.map(FontFeatures::features).unwrap_or_default() {
        settings.push(match feature.value {
            1 => feature.tag().to_owned(),
            v => format!("{}={v}", feature.tag()),
        });
    }
    if settings.is_empty() {
        family.to_owned()
    } else {
        format!("{family}:{}", settings.join("&"))
    }
}

/// A shadow from `fo:text-shadow`, with the opacity of
/// `desk:shadow-opacity`, opaque when the file has none.
fn shadow(shadow: odf::TextShadow, opacity: Option<f64>) -> Option<Shadow> {
    let odf::TextShadow::Shadow {
        dx,
        dy,
        blur,
        color,
    } = shadow
    else {
        return None;
    };
    let angle = (-dy.0).atan2(dx.0).to_degrees().round() as i32;
    Some(Shadow {
        angle: angle.rem_euclid(360),
        offset: Length::from_pt(dx.0.hypot(dy.0)),
        blur: blur.map_or(Length::ZERO, length),
        opacity: opacity.map_or(100, |o| o.round().clamp(0.0, 100.0) as u8),
        color: color.map_or(Rgba::BLACK, rgba),
    })
}

fn odf_shadow(shadow: Option<Shadow>) -> odf::TextShadow {
    match shadow {
        None => odf::TextShadow::None,
        Some(shadow) => {
            let (dx, dy) = shadow.displacement();
            odf::TextShadow::Shadow {
                dx: odf::Length(dx),
                dy: odf::Length(dy),
                blur: Some(odf_length(shadow.blur)),
                color: Some(odf_color(shadow.color)),
            }
        }
    }
}

fn background(background: odf::Background) -> Option<Rgba> {
    match background {
        odf::Background::Transparent => None,
        odf::Background::Color(color) => Some(rgba(color)),
    }
}

fn odf_background(background: Option<Rgba>) -> odf::Background {
    background.map_or(odf::Background::Transparent, |c| {
        odf::Background::Color(odf_color(c))
    })
}

/// The character attributes of ODF text properties, whose text is
/// `size` points large. Font sizes in percent of the parent style are not
/// kept.
fn char_attrs(t: &odf::TextProperties, size: f64) -> CharAttrs {
    let language = t.language.as_ref().map(|language| {
        // `zxx` is "no linguistic content", as Desk writes the undetermined
        // language.
        if language == "zxx" {
            return Language::undetermined();
        }
        let mut tag = language.clone();
        if let Some(script) = &t.script {
            tag.push('-');
            tag.push_str(script);
        }
        if let Some(country) = &t.country {
            tag.push('-');
            tag.push_str(country);
        }
        Language::new(&tag)
    });
    let capitalization = match (t.small_caps, t.transform) {
        (_, Some(odf::TextTransform::Uppercase)) => Some(Capitalization::AllCaps),
        (Some(true), _) => Some(Capitalization::SmallCaps),
        (_, Some(odf::TextTransform::Capitalize)) => Some(Capitalization::Title),
        (Some(false), _) | (_, Some(odf::TextTransform::None | odf::TextTransform::Lowercase)) => {
            Some(Capitalization::None)
        }
        (None, None) => None,
    };
    let (family, ligatures, font_features) = match t.font_family.as_deref() {
        Some(name) => font_name(name),
        None => ("", None, None),
    };
    CharAttrs {
        font_family: t.font_family.as_ref().map(|_| family.into()),
        font_size: t.font_size.map(length),
        weight: t.font_weight.map(FontWeight::new),
        italic: t.italic,
        underline: t.underline.map(line_style),
        strikethrough: t.line_through.map(line_style),
        strikethrough_color: t.line_through_color.map(|c| match c {
            odf::LineColor::FontColor => LineColor::Text,
            odf::LineColor::Color(c) => LineColor::Custom(rgba(c)),
        }),
        color: t.color.map(rgba),
        baseline: t.position.map(|p| match p {
            odf::TextPosition::Super => Baseline::Superscript,
            odf::TextPosition::Sub => Baseline::Subscript,
            odf::TextPosition::Percent(p) if p > 0.0 => Baseline::Superscript,
            odf::TextPosition::Percent(p) if p < 0.0 => Baseline::Subscript,
            odf::TextPosition::Percent(_) => Baseline::Normal,
        }),
        baseline_shift: None,
        capitalization,
        language,
        // The spacing of ODF is a length, the tracking of Pages a part of
        // the font size, which Desk keeps beside it.
        tracking: t.tracking.or_else(|| {
            t.letter_spacing
                .map(|l| (l.0 / size.max(1.0) * 1000.0).round() as i32)
        }),
        outline: t.outline,
        shadow: t.shadow.map(|s| shadow(s, t.shadow_opacity)),
        ligatures,
        font_features,
        background: t.background.map(background),
        field: None,
        link: None,
        bookmark: None,
    }
}

/// The ODF text properties of character attributes, at a level of styles,
/// a style or a run, whose format resolves to `resolved`.
fn text_properties(a: &CharAttrs, resolved: &CharFormat) -> odf::TextProperties {
    let (language, script, country) = match &a.language {
        Some(language) if language.tag() != "und" => {
            let mut parts = language.tag().split('-');
            let lang = parts.next().map(str::to_owned);
            let mut script = None;
            let mut country = None;
            for part in parts {
                if part.len() == 4 && script.is_none() && country.is_none() {
                    script = Some(part.to_owned());
                } else if country.is_none() {
                    country = Some(part.to_owned());
                }
            }
            (lang, script, country)
        }
        Some(_) => (Some("zxx".to_owned()), None, None),
        None => (None, None, None),
    };
    let line = |style: LineStyle| match style {
        LineStyle::None => odf::Line::None,
        LineStyle::Single => odf::Line::Single,
        LineStyle::Double => odf::Line::Double,
    };
    // Ligatures and features go with the font name, which they need.
    let font_family = if a.font_family.is_some()
        || a.ligatures.is_some()
        || a.font_features.as_ref().is_some_and(|f| !f.is_empty())
    {
        Some(odf_font_name(
            a.font_family.as_deref().unwrap_or(&resolved.font_family),
            a.ligatures,
            a.font_features.as_ref(),
        ))
    } else {
        None
    };
    odf::TextProperties {
        font_family,
        font_size: a.font_size.map(odf_length),
        font_size_percent: None,
        font_weight: a.weight.map(FontWeight::value),
        italic: a.italic,
        underline: a.underline.map(line),
        line_through: a.strikethrough.map(line),
        line_through_color: a.strikethrough_color.map(|c| match c {
            LineColor::Text => odf::LineColor::FontColor,
            LineColor::Custom(c) => odf::LineColor::Color(odf_color(c)),
        }),
        color: a.color.map(odf_color),
        position: a.baseline.map(|b| match b {
            Baseline::Normal => odf::TextPosition::Percent(0.0),
            Baseline::Superscript => odf::TextPosition::Super,
            Baseline::Subscript => odf::TextPosition::Sub,
        }),
        position_scale: a.baseline.map(|b| match b {
            Baseline::Normal => 100.0,
            Baseline::Superscript | Baseline::Subscript => SCRIPT_SCALE,
        }),
        transform: a.capitalization.map(|c| match c {
            Capitalization::AllCaps => odf::TextTransform::Uppercase,
            Capitalization::Title => odf::TextTransform::Capitalize,
            Capitalization::None | Capitalization::SmallCaps => odf::TextTransform::None,
        }),
        small_caps: a.capitalization.map(|c| c == Capitalization::SmallCaps),
        language,
        country,
        script,
        letter_spacing: a
            .tracking
            .map(|t| odf::Length(f64::from(t) / 1000.0 * resolved.font_size.to_pt())),
        tracking: a.tracking,
        outline: a.outline,
        shadow: a.shadow.map(odf_shadow),
        shadow_opacity: a
            .shadow
            .flatten()
            .filter(|s| s.opacity < 100)
            .map(|s| f64::from(s.opacity)),
        background: a.background.map(odf_background),
        // A paragraph attribute, which the paragraph styles write.
        hyphenate: None,
        desk_hyphenate: None,
    }
}

fn para_attrs(p: &odf::ParagraphProperties, t: &odf::TextProperties) -> ParaAttrs {
    let line_spacing = if let Some(at_least) = p.line_height_at_least {
        Some(LineSpacing::AtLeast(length(at_least)))
    } else if let Some(between) = p.line_spacing {
        Some(LineSpacing::Between(length(between)))
    } else {
        p.line_height.map(|h| match h {
            odf::LineHeight::Normal => LineSpacing::SINGLE,
            odf::LineHeight::Percent(percent) => LineSpacing::Multiple {
                percent: percent.round().clamp(1.0, 10_000.0) as u32,
            },
            odf::LineHeight::Exactly(l) => LineSpacing::Exactly(length(l)),
        })
    };
    let widow_control = match (p.widows, p.orphans) {
        (None, None) => None,
        (widows, orphans) => Some(widows.unwrap_or(0) >= 2 || orphans.unwrap_or(0) >= 2),
    };
    ParaAttrs {
        hyphenate: t.desk_hyphenate.or(t.hyphenate),
        alignment: p.align.map(|a| match a {
            odf::TextAlign::Start | odf::TextAlign::Left => Alignment::Left,
            odf::TextAlign::Center => Alignment::Center,
            odf::TextAlign::End | odf::TextAlign::Right => Alignment::Right,
            odf::TextAlign::Justify => Alignment::Justify,
        }),
        line_spacing,
        space_before: p.margin_top.map(length),
        space_after: p.margin_bottom.map(length),
        first_line_indent: p.text_indent.map(length),
        left_indent: p.margin_left.map(length),
        right_indent: p.margin_right.map(length),
        page_break_before: p.break_before.and_then(|b| match b {
            odf::Break::Page => Some(true),
            odf::Break::Column => None,
            _ => Some(false),
        }),
        column_break_before: p
            .break_before
            .and_then(|b| (b == odf::Break::Column).then_some(true)),
        // Sections and layouts come from the section elements.
        start: None,
        keep_with_next: p.keep_with_next,
        keep_lines_together: p.keep_together,
        widow_control,
        tab_stops: p
            .tab_stops
            .as_ref()
            .map(|stops| TabStops::new(stops.iter().map(tab_stop))),
        default_tab_interval: p
            .tab_stop_distance
            .map(length)
            .filter(|l| l.millipoints() > 0),
        background: p.background.map(background),
        border: para_border(p),
        // Lists come from the list elements and list styles.
        list_style: None,
        list_level: None,
        list_start: None,
        list_override: None,
        toc_entry: None,
    }
}

/// The rules or border of a paragraph from its ODF borders: lines above
/// and below are rules, and lines on all four sides a border.
fn para_border(p: &odf::ParagraphProperties) -> Option<ParaBorder> {
    let sides = [p.border_top, p.border_bottom, p.border_left, p.border_right];
    if sides.iter().all(Option::is_none) {
        return None;
    }
    let line = |b: Option<odf::Border>| match b {
        Some(odf::Border::Line {
            width,
            style,
            color,
        }) => Some((width, style, color)),
        _ => None,
    };
    let [top, bottom, left, right] = sides.map(line);
    let kind = match (
        top.is_some(),
        bottom.is_some(),
        left.is_some() && right.is_some(),
    ) {
        (true, true, true) => BorderSides::Box,
        (true, true, false) => BorderSides::AboveAndBelow,
        (true, false, _) => BorderSides::Above,
        (false, true, _) => BorderSides::Below,
        (false, false, _) => return Some(ParaBorder::default()),
    };
    let (padding, first) = if top.is_some() {
        (p.padding_top, top)
    } else {
        (p.padding_bottom, bottom)
    };
    let (width, style, color) = first?;
    Some(ParaBorder {
        sides: kind,
        width: length(width),
        color: rgba(color),
        style: match style {
            odf::BorderLine::Solid => BorderStyle::Solid,
            odf::BorderLine::Dashed => BorderStyle::Dashed,
            odf::BorderLine::Dotted => BorderStyle::Dotted,
        },
        offset: padding.map_or(Length::ZERO, length),
    })
}

/// Writes the rules or border of a paragraph as ODF borders, with
/// `none` on the sides without a line.
fn odf_border(p: &mut odf::ParagraphProperties, border: &ParaBorder) {
    let line = odf::Border::Line {
        width: odf_length(border.width),
        style: match border.style {
            BorderStyle::Solid => odf::BorderLine::Solid,
            BorderStyle::Dashed => odf::BorderLine::Dashed,
            BorderStyle::Dotted => odf::BorderLine::Dotted,
        },
        color: odf_color(border.color),
    };
    let sides = border.sides;
    let boxed = sides == BorderSides::Box;
    let side = |on: bool| {
        if on {
            (Some(line), Some(odf_length(border.offset)))
        } else {
            (Some(odf::Border::None), None)
        }
    };
    (p.border_top, p.padding_top) = side(sides.above());
    (p.border_bottom, p.padding_bottom) = side(sides.below());
    (p.border_left, p.padding_left) = side(boxed);
    (p.border_right, p.padding_right) = side(boxed);
}

fn tab_stop(stop: &odf::TabStop) -> TabStop {
    let leader = match stop.leader_text {
        Some('.' | '\u{2026}') => TabLeader::Dots,
        Some('-' | '\u{2013}' | '\u{2014}') => TabLeader::Dashes,
        Some('_') => TabLeader::Line,
        Some('\u{2192}') => TabLeader::Arrows,
        _ => match stop.leader_style {
            odf::LeaderStyle::None => TabLeader::None,
            odf::LeaderStyle::Solid => TabLeader::Line,
            odf::LeaderStyle::Dotted => TabLeader::Dots,
            odf::LeaderStyle::Dash => TabLeader::Dashes,
        },
    };
    TabStop {
        position: length(stop.position),
        alignment: match stop.kind {
            odf::TabType::Left => TabAlignment::Left,
            odf::TabType::Center => TabAlignment::Center,
            odf::TabType::Right => TabAlignment::Right,
            odf::TabType::Char => TabAlignment::Decimal,
        },
        leader,
        decimal: stop.char.unwrap_or('.'),
    }
}

fn odf_tab_stop(stop: &TabStop) -> odf::TabStop {
    let (leader_style, leader_text) = match stop.leader {
        TabLeader::None => (odf::LeaderStyle::None, None),
        TabLeader::Dots => (odf::LeaderStyle::Dotted, Some('.')),
        TabLeader::Dashes => (odf::LeaderStyle::Dash, Some('-')),
        TabLeader::Line => (odf::LeaderStyle::Solid, Some('_')),
        TabLeader::Arrows => (odf::LeaderStyle::Solid, Some('\u{2192}')),
    };
    odf::TabStop {
        position: odf_length(stop.position),
        kind: match stop.alignment {
            TabAlignment::Left => odf::TabType::Left,
            TabAlignment::Center => odf::TabType::Center,
            TabAlignment::Right => odf::TabType::Right,
            TabAlignment::Decimal => odf::TabType::Char,
        },
        char: (stop.alignment == TabAlignment::Decimal).then_some(stop.decimal),
        leader_style,
        leader_text,
    }
}

/// The tab stops moved by a distance, as between the left edge of the
/// text, where the model measures them from, and the indent, where ODF
/// does.
fn shifted(stops: &TabStops, by: Length) -> TabStops {
    TabStops::new(stops.stops().iter().map(|s| TabStop {
        position: s.position + by,
        ..*s
    }))
}

/// Whether a level of styles, a style or a paragraph, must say where its
/// tab stops are, because the indent they are measured from in ODF
/// changes there.
fn tabs_move_at(attrs: &ParaAttrs, resolved: &ParaFormat) -> bool {
    (attrs.tab_stops.is_some() || attrs.left_indent.is_some()) && !resolved.tab_stops.is_empty()
}

fn paragraph_properties(a: &ParaAttrs) -> odf::ParagraphProperties {
    let mut p = odf::ParagraphProperties {
        align: a.alignment.map(|a| match a {
            Alignment::Left | Alignment::Auto => odf::TextAlign::Start,
            Alignment::Center => odf::TextAlign::Center,
            Alignment::Right => odf::TextAlign::End,
            Alignment::Justify => odf::TextAlign::Justify,
        }),
        margin_top: a.space_before.map(odf_length),
        margin_bottom: a.space_after.map(odf_length),
        margin_left: a.left_indent.map(odf_length),
        margin_right: a.right_indent.map(odf_length),
        text_indent: a.first_line_indent.map(odf_length),
        break_before: match (a.page_break_before, a.column_break_before) {
            (Some(true), _) => Some(odf::Break::Page),
            (_, Some(true)) => Some(odf::Break::Column),
            (Some(false), _) | (_, Some(false)) => Some(odf::Break::Auto),
            (None, None) => None,
        },
        keep_with_next: a.keep_with_next,
        keep_together: a.keep_lines_together,
        widows: a.widow_control.map(|w| if w { 2 } else { 0 }),
        orphans: a.widow_control.map(|w| if w { 2 } else { 0 }),
        tab_stops: a
            .tab_stops
            .as_ref()
            .map(|stops| stops.stops().iter().map(odf_tab_stop).collect()),
        tab_stop_distance: a.default_tab_interval.map(odf_length),
        background: a.background.map(odf_background),
        ..odf::ParagraphProperties::default()
    };
    if let Some(border) = &a.border {
        odf_border(&mut p, border);
    }
    // A section starts a page, numbered from its first number or on from
    // the page before, for other programs.
    if let Some(Some(start)) = &a.start
        && let Start::Section(section, _) = &**start
    {
        p.break_before = Some(odf::Break::Page);
        p.page_number = Some(section.first_number);
    }
    match a.line_spacing {
        Some(LineSpacing::Multiple { percent }) => {
            p.line_height = Some(odf::LineHeight::Percent(f64::from(percent)));
        }
        Some(LineSpacing::Exactly(l)) => {
            p.line_height = Some(odf::LineHeight::Exactly(odf_length(l)))
        }
        Some(LineSpacing::AtLeast(l)) => p.line_height_at_least = Some(odf_length(l)),
        Some(LineSpacing::Between(l)) => p.line_spacing = Some(odf_length(l)),
        None => {}
    }
    p
}

/// The font size that a style of an ODF document sets, itself or through
/// its parents, in points.
fn odf_font_size(
    source: &odf::TextDocument,
    family: odf::Family,
    name: Option<&str>,
) -> Option<f64> {
    let mut name = name;
    // The chain is short; the bound keeps a cycle from looping forever.
    for _ in 0..64 {
        let style = source.style(family, name?)?;
        if let Some(size) = style.properties.text.font_size {
            return Some(size.0);
        }
        name = style.parent.as_deref();
    }
    None
}

/// The size of the text of the default style of an ODF document.
fn odf_default_size(source: &odf::TextDocument) -> f64 {
    odf_font_size(source, odf::Family::Paragraph, Some(DEFAULT_STYLE))
        .or(source.default_style.text.font_size.map(|s| s.0))
        .unwrap_or(CharFormat::default().font_size.to_pt())
}

/// The size that the raise of a list label is measured against, since
/// ODF raises labels by a percentage of their size and the model by a
/// length: 12 points, the size of the text Pages measures labels for.
const LABEL_RAISE_BASE: f64 = 12.0;

/// The size that the size of a list label that does not scale with the
/// text is a part of, as in the model: 12 points.
const LABEL_FIXED_BASE: f64 = 12.0;

/// The levels of a list style of ODF; the levels it does not set have no
/// label.
fn list_levels(style: &odf::ListStyle, pictures: &[(String, Vec<u8>)]) -> [ListLevel; LIST_LEVELS] {
    std::array::from_fn(|index| {
        let Some(level) = style
            .levels
            .iter()
            .find(|l| usize::from(l.level) == index + 1)
        else {
            return ListLevel::new(ListLabel::None, index, Length::ZERO);
        };
        let (label, start) = match &level.label {
            odf::ListLabel::Bullet(c) => (ListLabel::bullet(&c.to_string()), 1),
            odf::ListLabel::Image {
                href,
                height,
                raise,
                ..
            } => {
                let picture = pictures
                    .iter()
                    .find(|(path, _)| path == href)
                    .and_then(|(_, bytes)| crate::picture::picture(bytes));
                // A picture of its own height does not scale with the
                // text; without the picture, a bullet takes its place.
                let level = match picture {
                    Some(picture) => {
                        let (size, scale) = match (level.relative_size, height) {
                            (Some(percent), _) => (percent, true),
                            (None, Some(height)) => (height.0 / LABEL_FIXED_BASE * 100.0, false),
                            (None, None) => (100.0, true),
                        };
                        ListLevel {
                            label: ListLabel::Image(picture),
                            label_indent: level.space_before.map_or(Length::ZERO, length),
                            text_indent: level.min_label_width.map_or(Length::ZERO, length),
                            size: size.round().clamp(1.0, 1000.0) as u16,
                            scale_with_text: scale,
                            raise: raise.map_or(Length::ZERO, length),
                            ..ListLevel::new(ListLabel::None, index, Length::ZERO)
                        }
                    }
                    None => ListLevel {
                        label: ListLabel::bullet("\u{2022}"),
                        label_indent: level.space_before.map_or(Length::ZERO, length),
                        text_indent: level.min_label_width.map_or(Length::ZERO, length),
                        ..ListLevel::new(ListLabel::None, index, Length::ZERO)
                    },
                };
                return level;
            }
            odf::ListLabel::Number {
                format,
                prefix,
                suffix,
                display_levels,
                start,
            } => {
                let number = match format.as_str() {
                    "1" => Some(NumberFormat::Decimal),
                    "I" => Some(NumberFormat::UpperRoman),
                    "i" => Some(NumberFormat::LowerRoman),
                    "A" => Some(NumberFormat::UpperAlpha),
                    "a" => Some(NumberFormat::LowerAlpha),
                    _ => None,
                };
                let label = match number {
                    Some(format) => ListLabel::Number {
                        format,
                        prefix: prefix.as_str().into(),
                        suffix: suffix.as_str().into(),
                        tiered: *display_levels > 1,
                    },
                    // Without a number, the prefix and the suffix are a
                    // bullet of more than one character.
                    None if prefix.is_empty() && suffix.is_empty() => ListLabel::None,
                    None => ListLabel::bullet(&format!("{prefix}{suffix}")),
                };
                (label, *start)
            }
        };
        // A label of a size of its own, in points, does not scale with the
        // text (LST-03); its size is a part of 12 points.
        let fixed = level
            .text
            .font_size
            .filter(|_| level.relative_size.is_none() && level.text.font_size_percent.is_none());
        let size = match fixed {
            Some(points) => (points.0 / LABEL_FIXED_BASE * 100.0)
                .round()
                .clamp(1.0, 1000.0) as u16,
            None => level
                .relative_size
                .or(level.text.font_size_percent)
                .map_or(100, |p| p.round().clamp(1.0, 1000.0) as u16),
        };
        let raise = match level.text.position {
            Some(odf::TextPosition::Percent(p)) => Length::from_pt(p * LABEL_RAISE_BASE / 100.0),
            _ => Length::ZERO,
        };
        ListLevel {
            label,
            start,
            label_indent: level.space_before.map_or(Length::ZERO, length),
            text_indent: level.min_label_width.map_or(Length::ZERO, length),
            size,
            scale_with_text: fixed.is_none(),
            raise,
            color: level.text.color.map(rgba),
        }
    })
}

/// A list style of the model as one of ODF.
fn odf_list_style(
    style: &ListStyle,
    name: String,
    pictures: &mut Vec<(String, Vec<u8>)>,
) -> odf::ListStyle {
    let display_name = (name != *style.name).then(|| style.name.to_string());
    let levels = style
        .levels
        .iter()
        .enumerate()
        .map(|(index, level)| {
            let number =
                |format: &str, prefix: &str, suffix: &str, tiered: bool| odf::ListLabel::Number {
                    format: format.to_owned(),
                    prefix: prefix.to_owned(),
                    suffix: suffix.to_owned(),
                    display_levels: if tiered { index as u8 + 1 } else { 1 },
                    start: level.start,
                };
            let label = match &level.label {
                ListLabel::None => number("", "", "", false),
                ListLabel::Image(picture) => {
                    let href = match pictures.iter().find(|(_, b)| b == picture.png()) {
                        Some((href, _)) => href.clone(),
                        None => {
                            let href = format!("Pictures/bullet{}.png", pictures.len() + 1);
                            pictures.push((href.clone(), picture.png().to_vec()));
                            href
                        }
                    };
                    let height = f64::from(level.size) / 100.0 * LABEL_FIXED_BASE;
                    odf::ListLabel::Image {
                        href,
                        width: Some(odf::Length(height * picture.aspect())),
                        height: Some(odf::Length(height)),
                        raise: (level.raise != Length::ZERO).then(|| odf_length(level.raise)),
                    }
                }
                ListLabel::Bullet(text) => {
                    let mut chars = text.chars();
                    match (chars.next(), chars.next()) {
                        (Some(c), None) => odf::ListLabel::Bullet(c),
                        _ => number("", text, "", false),
                    }
                }
                ListLabel::Number {
                    format,
                    prefix,
                    suffix,
                    tiered,
                } => number(
                    match format {
                        NumberFormat::Decimal => "1",
                        NumberFormat::UpperRoman => "I",
                        NumberFormat::LowerRoman => "i",
                        NumberFormat::UpperAlpha => "A",
                        NumberFormat::LowerAlpha => "a",
                    },
                    prefix,
                    suffix,
                    *tiered,
                ),
            };
            let sized = level.size != 100 && level.scale_with_text;
            odf::ListLevelStyle {
                level: index as u8 + 1,
                relative_size: match label {
                    odf::ListLabel::Bullet(_) => sized.then_some(f64::from(level.size)),
                    odf::ListLabel::Image { .. } => {
                        level.scale_with_text.then_some(f64::from(level.size))
                    }
                    odf::ListLabel::Number { .. } => None,
                },
                label,
                space_before: Some(odf_length(level.label_indent)),
                min_label_width: Some(odf_length(level.text_indent)),
                text: odf::TextProperties {
                    font_size_percent: sized.then_some(f64::from(level.size)),
                    font_size: (!level.scale_with_text)
                        .then(|| odf::Length(f64::from(level.size) / 100.0 * LABEL_FIXED_BASE)),
                    color: level.color.map(odf_color),
                    position: (level.raise != Length::ZERO).then(|| {
                        odf::TextPosition::Percent(level.raise.to_pt() / LABEL_RAISE_BASE * 100.0)
                    }),
                    ..odf::TextProperties::default()
                },
            }
        })
        .collect();
    odf::ListStyle {
        name,
        display_name,
        levels,
    }
}

/// Builds a document from an ODF text document.
pub fn import(source: &odf::TextDocument) -> Result<Document, FilterError> {
    let mut document = Document::new_word_processing();
    let default = document.styles().default_paragraph();

    // Identifiers and names for the common styles first, so that parents
    // and next styles can refer to any of them.
    let mut ids: HashMap<(odf::Family, &str), StyleId> = HashMap::new();
    let mut used_names: HashSet<(odf::Family, String)> = HashSet::new();
    let mut names: HashMap<StyleId, String> = HashMap::new();
    let standard = source
        .styles
        .iter()
        .find(|s| s.family == odf::Family::Paragraph && s.name == DEFAULT_STYLE);
    // Without a Standard style the default keeps the name of a new
    // document, which no other style may then take.
    if standard.is_none() {
        used_names.insert((odf::Family::Paragraph, DEFAULT_NAME.to_owned()));
    }
    for style in &source.styles {
        let id = if Some(style) == standard {
            default
        } else {
            document.new_id()
        };
        ids.insert((style.family, style.name.as_str()), id);
        let mut name = style.shown_name().to_owned();
        let mut n = 2;
        while !used_names.insert((style.family, name.clone())) {
            name = format!("{} {n}", style.shown_name());
            n += 1;
        }
        names.insert(id, name);
    }
    // List styles, which paragraph styles name. Automatic ones take the
    // style with the same levels, or come in as new styles.
    let mut lists: HashMap<&str, StyleId> = HashMap::new();
    // The automatic list styles that only items override their list
    // with: their levels go on the items.
    let overriding: HashSet<&str> = source
        .body
        .iter()
        .filter_map(|p| p.list.as_ref()?.style_override.as_deref())
        .collect();
    for (automatic, style) in source.list_styles.iter().map(|s| (false, s)).chain(
        source
            .automatic_list_styles
            .iter()
            .filter(|s| !overriding.contains(s.name.as_str()))
            .map(|s| (true, s)),
    ) {
        let levels = list_levels(style, &source.pictures);
        if levels.iter().all(|l| !l.label.is_some()) {
            continue;
        }
        let same = document
            .styles()
            .iter()
            .find(|s| matches!(s, Style::List(l) if *l.levels == levels))
            .map(Style::id);
        let id = match same {
            Some(id) if automatic => id,
            _ => {
                let base = if automatic {
                    "List"
                } else {
                    style.shown_name()
                };
                let name = document
                    .styles()
                    .unique_name(desk_model::StyleFamily::List, base);
                let id = document.new_id();
                document.put_style(ListStyle {
                    id,
                    name: name.as_str().into(),
                    shortcut: source
                        .style_shortcuts
                        .iter()
                        .find(|s| s.family == odf::Family::List && s.style == style.name)
                        .map(|s| s.key),
                    levels: levels.into(),
                })?;
                id
            }
        };
        lists.entry(style.name.as_str()).or_insert(id);
    }
    let list_of = |name: &Option<String>| -> Option<Option<StyleId>> {
        name.as_deref().map(|n| lists.get(n).copied())
    };

    let defaults = &source.default_style;
    let mut default_style = ParagraphStyle::new(default, DEFAULT_NAME);
    default_style.paragraph = para_attrs(&defaults.paragraph, &defaults.text);
    default_style.character = char_attrs(&defaults.text, odf_default_size(source));
    if let Some(standard) = standard {
        default_style.name = names[&default].as_str().into();
        default_style.shortcut = source
            .style_shortcuts
            .iter()
            .find(|s| s.family == odf::Family::Paragraph && s.style == standard.name)
            .map(|s| s.key);
        default_style.paragraph.merge(&para_attrs(
            &standard.properties.paragraph,
            &standard.properties.text,
        ));
        default_style.character.merge(&char_attrs(
            &standard.properties.text,
            odf_default_size(source),
        ));
        default_style.next = standard
            .next
            .as_deref()
            .and_then(|n| ids.get(&(odf::Family::Paragraph, n)).copied());
        if let Some(list) = list_of(&standard.list_style) {
            default_style.paragraph.list_style = Some(list);
        }
    }

    // Twice: without parents and next styles, then with them, dropping a
    // reference that would make a cycle or point to the wrong family.
    for pass in 0..2 {
        for style in &source.styles {
            let id = ids[&(style.family, style.name.as_str())];
            if id == default {
                continue;
            }
            let size = odf_font_size(source, style.family, Some(&style.name))
                .unwrap_or_else(|| odf_default_size(source));
            let link = |name: &Option<String>, family| {
                name.as_deref()
                    .and_then(|n| ids.get(&(family, n)).copied())
                    .filter(|_| pass == 1)
            };
            let shortcut = source
                .style_shortcuts
                .iter()
                .find(|s| s.family == style.family && s.style == style.name)
                .map(|s| s.key);
            let built: Style = match style.family {
                odf::Family::List => continue,
                odf::Family::Paragraph => ParagraphStyle {
                    id,
                    name: names[&id].as_str().into(),
                    shortcut,
                    parent: link(&style.parent, odf::Family::Paragraph).or(Some(default)),
                    next: link(&style.next, odf::Family::Paragraph),
                    paragraph: ParaAttrs {
                        list_style: list_of(&style.list_style),
                        ..para_attrs(&style.properties.paragraph, &style.properties.text)
                    },
                    character: char_attrs(&style.properties.text, size),
                }
                .into(),
                odf::Family::Text => CharacterStyle {
                    id,
                    name: names[&id].as_str().into(),
                    shortcut,
                    parent: link(&style.parent, odf::Family::Text),
                    character: char_attrs(&style.properties.text, size),
                }
                .into(),
            };
            if document.put_style(built.clone()).is_err() {
                let mut plain = built;
                match &mut plain {
                    Style::Paragraph(p) => {
                        p.parent = Some(default);
                        p.next = None;
                    }
                    Style::Character(c) => c.parent = None,
                    Style::List(_) => {}
                }
                document.put_style(plain)?;
            }
        }
    }
    // Desk writes the distance of the default stops even when it is the
    // default of the model, for other programs, which have defaults of
    // their own.
    if default_style.paragraph.default_tab_interval == Some(DEFAULT_TAB_INTERVAL) {
        default_style.paragraph.default_tab_interval = None;
    }
    document.put_style(default_style)?;

    // ODF measures tab stops from the indent, unless the settings say
    // otherwise, and the model from the left edge of the text.
    let relative = source.tabs_relative_to_indent.unwrap_or(true);

    // The paragraphs, after the empty one of a new document, which goes.
    let first = document
        .body()
        .get(0)
        .expect("a new document has a paragraph")
        .id();
    // The list styles that a list was met with, for lists that start
    // again rather than going on from the one before.
    let mut listed: HashSet<StyleId> = HashSet::new();

    let page = &source.page_layout;
    let mut setup = PageSetup::default();
    let set = |field: &mut Length, value: Option<odf::Length>| {
        if let Some(value) = value.filter(|v| v.0 >= 0.0) {
            *field = length(value);
        }
    };
    set(&mut setup.width, page.width);
    set(&mut setup.height, page.height);
    set(&mut setup.margin_top, page.margin_top);
    set(&mut setup.margin_bottom, page.margin_bottom);
    set(&mut setup.margin_left, page.margin_left);
    set(&mut setup.margin_right, page.margin_right);
    setup.facing_pages = page.mirrored == Some(true);
    // The margin of ODF is above the header, and the body starts below
    // it (EST-12).
    let master = &source.master;
    let shown = |part: &Option<odf::HeaderFooter>| part.as_ref().is_some_and(|p| p.display);
    setup.headers = shown(&master.header);
    setup.footers = shown(&master.footer);
    let size = |style: &odf::HeaderFooterStyle| {
        length(style.min_height.unwrap_or_default()) + length(style.spacing.unwrap_or_default())
    };
    if setup.headers
        && let Some(style) = &page.header
    {
        setup.header_margin = setup.margin_top;
        setup.margin_top = setup.margin_top + size(style);
    }
    if setup.footers
        && let Some(style) = &page.footer
    {
        setup.footer_margin = setup.margin_bottom;
        setup.margin_bottom = setup.margin_bottom + size(style);
    }
    // The columns of the pages are the layout outside any section.
    let page_layout = Layout {
        columns: page.columns.as_ref().map_or(desk_model::Columns::ONE, |c| {
            sections::columns(
                c,
                (setup.width - setup.margin_left - setup.margin_right).to_pt(),
            )
        }),
        ..Layout::default()
    };
    setup.layout = page_layout;
    let mut current_layout = page_layout;
    // The sections with master pages of their own, by their first
    // paragraph.
    let mut own_masters: Vec<(NodeId, String)> = Vec::new();

    // The ids of the paragraphs of the body, ahead, for the page
    // references to them.
    let body_ids: Vec<NodeId> = source.body.iter().map(|_| document.new_id()).collect();
    let mut marks = Marks::new();
    for (paragraph, id) in source.body.iter().zip(&body_ids) {
        for mark in &paragraph.marks {
            marks.entry(mark.clone()).or_insert(*id);
        }
    }
    for (index, paragraph) in source.body.iter().enumerate() {
        let mut built = import_paragraph(
            source,
            &mut document,
            &ids,
            default,
            paragraph,
            relative,
            &marks,
        )?;
        // Its id, and whether it is an entry of a table of contents.
        {
            let mut attrs = built.attrs().clone();
            if paragraph.toc {
                let target = built.runs().find_map(|r| match &r.attrs.field {
                    Some(Some(Field::PageOf { paragraph })) => Some(*paragraph),
                    _ => None,
                });
                attrs.toc_entry = Some(Some(TocEntry { target }));
            }
            built = Paragraph::from_runs(
                body_ids[index],
                built.style(),
                attrs,
                built.runs().cloned().collect::<Vec<_>>(),
            )?;
        }
        // The section or layout that starts here.
        let own = paragraph.style.as_deref().and_then(|name| {
            source
                .automatic_styles
                .iter()
                .find(|s| s.family == odf::Family::Paragraph && s.name == name)
        });
        let page_number = own.and_then(|s| s.properties.paragraph.page_number);
        // A master page other than the one of the first section gives the
        // section headers and footers of its own (EST-16).
        let own_master = own
            .and_then(|s| s.master_page.as_deref())
            .filter(|name| index > 0 && source.masters.iter().any(|(n, _)| n == name));
        let breaks = own_master.is_some()
            || own.is_some_and(|s| s.properties.paragraph.break_before == Some(odf::Break::Page));
        let restart = |layout: Layout| {
            page_number.flatten().map(|n| {
                Start::Section(
                    Section {
                        first_number: Some(n),
                        ..Section::default()
                    },
                    layout,
                )
            })
        };
        let start = match paragraph.section.as_deref() {
            Some("") => Some(restart(page_layout).unwrap_or(Start::Layout(page_layout))),
            Some(name) => source
                .section_styles
                .iter()
                .find(|s| s.name == name)
                .map(|style| {
                    let layout = sections::layout(style, &setup);
                    match sections::section(style, page_number.flatten()) {
                        Some(section) => Start::Section(section, layout),
                        None => restart(layout).unwrap_or(Start::Layout(layout)),
                    }
                }),
            // A page that numbers its pages again starts a section.
            None if breaks || index == 0 => restart(current_layout),
            None => None,
        };
        // A master page of its own starts a section, which has its
        // headers and footers.
        let start = match (own_master, start) {
            (Some(name), start) => {
                own_masters.push((built.id(), name.to_owned()));
                Some(match start {
                    Some(Start::Section(section, layout)) => Start::Section(
                        Section {
                            own_margins: true,
                            ..section
                        },
                        layout,
                    ),
                    other => {
                        let layout = other.map_or(current_layout, |s| *s.layout());
                        Start::Section(
                            Section {
                                first_number: page_number.flatten(),
                                own_margins: true,
                                ..Section::default()
                            },
                            layout,
                        )
                    }
                })
            }
            (None, start) => start,
        };
        if let Some(start) = start {
            current_layout = *start.layout();
            if index == 0 {
                if let Start::Section(section, _) = start {
                    setup.section = section;
                }
                setup.layout = current_layout;
            } else {
                let mut attrs = built.attrs().clone();
                if matches!(start, Start::Section(..)) {
                    // The break of a section is part of it.
                    attrs.page_break_before = None;
                }
                attrs.start = Some(Some(std::sync::Arc::new(start)));
                built = Paragraph::from_runs(
                    built.id(),
                    built.style(),
                    attrs,
                    built.runs().cloned().collect::<Vec<_>>(),
                )?;
            }
        }
        // The list style that the automatic style of the paragraph sets.
        if let Some(automatic) = paragraph.style.as_deref().and_then(|name| {
            source
                .automatic_styles
                .iter()
                .find(|s| s.family == odf::Family::Paragraph && s.name == name)
        }) && let Some(own) = list_of(&automatic.list_style)
        {
            let attrs = ParaAttrs {
                list_style: Some(own),
                ..built.attrs().clone()
            };
            built = Paragraph::from_runs(
                built.id(),
                built.style(),
                attrs,
                built.runs().cloned().collect::<Vec<_>>(),
            )?;
        }
        if let Some(member) = &paragraph.list
            && let Some(&list) = lists.get(member.style.as_str())
        {
            let mut attrs = built.attrs().clone();
            let styles = document.styles();
            let resolved = styles.resolve_paragraph(built.style(), &attrs);
            if resolved.list_style != Some(list) {
                attrs.list_style = Some(Some(list));
            }
            if member.level > 0 {
                attrs.list_level = Some(member.level);
            }
            let restart = member.first && !member.continues && listed.contains(&list);
            if let Some(start) = member.start {
                attrs.list_start = Some(Some(start));
            } else if restart && let Some(style) = styles.list(list) {
                attrs.list_start = Some(Some(style.level(member.level).start));
            }
            if let Some(own) = member.style_override.as_deref().and_then(|name| {
                source
                    .automatic_list_styles
                    .iter()
                    .chain(&source.list_styles)
                    .find(|s| s.name == name)
            }) && let Some(style) = styles.list(list)
            {
                let level = usize::from(member.level).min(LIST_LEVELS - 1);
                let own = list_levels(own, &source.pictures)[level].clone();
                if own.label.is_some() && &own != style.level(member.level) {
                    attrs.list_override = Some(Some(std::sync::Arc::new(own)));
                }
            }
            listed.insert(list);
            built = Paragraph::from_runs(
                built.id(),
                built.style(),
                attrs,
                built.runs().cloned().collect::<Vec<_>>(),
            )?;
        }
        document.apply(Op::InsertParagraph {
            index: index + 1,
            paragraph: built,
        })?;
    }
    if !source.body.is_empty() {
        document.apply(Op::RemoveParagraph { paragraph: first })?;
    }
    // The stops of the styles move after those of the paragraphs, which
    // are measured from the indents the styles had as read.
    if relative {
        let styles = document.styles();
        let moved: Vec<Style> = styles
            .iter()
            .filter_map(|style| {
                let Style::Paragraph(p) = style else {
                    return None;
                };
                let resolved = styles.resolve_paragraph(p.id, &ParaAttrs::default());
                tabs_move_at(&p.paragraph, &resolved).then(|| {
                    let mut p = p.clone();
                    p.paragraph.tab_stops =
                        Some(shifted(&resolved.tab_stops, resolved.left_indent));
                    Style::Paragraph(p)
                })
            })
            .collect();
        for style in moved {
            document.put_style(style)?;
        }
    }

    // The headers and footers, whose paragraphs name the automatic
    // styles of the styles.
    let styled = odf::TextDocument {
        default_style: source.default_style.clone(),
        styles: source.styles.clone(),
        automatic_styles: source.master_styles.clone(),
        ..odf::TextDocument::default()
    };
    let mut parts: Vec<(Option<NodeId>, &odf::MasterPage)> = vec![(None, master)];
    for (id, name) in &own_masters {
        if let Some((_, page)) = source.masters.iter().find(|(n, _)| n == name) {
            parts.push((Some(*id), page));
        }
    }
    for (section, master) in parts {
        for (place, part) in [
            (MarginPages::Default, &master.header, MarginKind::Header),
            (MarginPages::First, &master.header_first, MarginKind::Header),
            (MarginPages::Left, &master.header_left, MarginKind::Header),
            (MarginPages::Default, &master.footer, MarginKind::Footer),
            (MarginPages::First, &master.footer_first, MarginKind::Footer),
            (MarginPages::Left, &master.footer_left, MarginKind::Footer),
        ]
        .map(|(pages, part, kind)| (MarginPlace::new(kind, pages), part))
        {
            let Some(part) = part else {
                continue;
            };
            let mut paragraphs = Vec::new();
            for paragraph in &part.paragraphs {
                paragraphs.push(import_paragraph(
                    &styled,
                    &mut document,
                    &ids,
                    default,
                    paragraph,
                    relative,
                    &Marks::new(),
                )?);
            }
            if paragraphs.iter().all(Paragraph::is_empty) {
                continue;
            }
            // Other programs show the first and left ones whenever they
            // are there; Desk, when the section asks for them.
            if section.is_none() && setup.section == Section::default() {
                match place.pages {
                    MarginPages::First => setup.section.first_page_different = true,
                    MarginPages::Left => setup.section.left_right_different = true,
                    MarginPages::Default => {}
                }
            }
            document.apply(Op::SetHeaderFooter {
                section,
                place,
                paragraphs,
            })?;
        }
    }

    document.apply(Op::SetPageSetup { page: setup })?;

    let text = &source.text_settings;
    document.apply(Op::SetTextSettings {
        settings: TextSettings {
            hyphenation: text.hyphenation.unwrap_or(true),
            ligatures: text.ligatures.unwrap_or(true),
            never_hyphenate: text
                .never_hyphenate
                .iter()
                .map(|w| w.to_lowercase())
                .collect(),
            spacing: if text.larger_spacing.unwrap_or(false) {
                ParagraphSpacing::Larger
            } else {
                ParagraphSpacing::Sum
            },
            notes: note_settings(text, &source.body),
            toc: TocSettings {
                levels: source
                    .toc_levels
                    .iter()
                    .filter_map(|level| {
                        let find = |name: &str| ids.get(&(odf::Family::Paragraph, name)).copied();
                        let style = find(&level.style)?;
                        Some(TocLevel {
                            style,
                            entry: find(&level.entry).unwrap_or(default),
                            page_number: level.page_number,
                        })
                    })
                    .collect(),
                page_links: source.toc_levels.iter().any(|l| l.link),
            },
            links_inactive: text.links_inactive.unwrap_or(false),
        },
    })?;

    let meta = &source.meta;
    document.apply(Op::SetMetadata {
        metadata: Metadata {
            title: meta.title.clone().unwrap_or_default(),
            author: meta
                .initial_creator
                .clone()
                .or_else(|| meta.creator.clone())
                .unwrap_or_default(),
            comments: meta.description.clone().unwrap_or_default(),
            keywords: meta.keywords.clone(),
        },
    })?;
    Ok(document)
}

/// How the notes of a document show, from the settings Desk keeps, or as
/// the notes of other programs' files are: endnotes at the end of the
/// document when they are endnotes.
fn note_settings(text: &odf::TextSettings, body: &[odf::Paragraph]) -> NoteSettings {
    let defaults = NoteSettings::default();
    let endnotes = body
        .iter()
        .flat_map(|p| &p.spans)
        .any(|s| matches!(&s.field, Some(odf::Field::Note(note)) if note.endnote));
    NoteSettings {
        kind: match text.note_kind.as_deref() {
            Some("document") => NoteKind::DocumentEndnotes,
            Some("section") => NoteKind::SectionEndnotes,
            Some(_) => NoteKind::Footnotes,
            None if endnotes => NoteKind::DocumentEndnotes,
            None => NoteKind::Footnotes,
        },
        format: match text.note_format.as_deref() {
            Some("roman") => NoteFormat::Roman,
            Some("symbols") => NoteFormat::Symbols,
            _ => NoteFormat::Arabic,
        },
        numbering: match text.note_numbering.as_deref() {
            Some("page") => NoteNumbering::Page,
            Some("section") => NoteNumbering::Section,
            _ => NoteNumbering::Continuous,
        },
        gap: text
            .note_gap
            .as_deref()
            .and_then(odf::Length::parse)
            .map_or(defaults.gap, length),
    }
}

/// The settings Desk keeps for the notes of a document, when they are not
/// the defaults.
fn odf_note_settings(settings: NoteSettings) -> odf::TextSettings {
    let defaults = NoteSettings::default();
    let given = |differs: bool, value: &str| differs.then(|| value.to_owned());
    odf::TextSettings {
        note_kind: given(
            settings.kind != defaults.kind,
            match settings.kind {
                NoteKind::Footnotes => "footnotes",
                NoteKind::DocumentEndnotes => "document",
                NoteKind::SectionEndnotes => "section",
            },
        ),
        note_format: given(
            settings.format != defaults.format,
            match settings.format {
                NoteFormat::Arabic => "arabic",
                NoteFormat::Roman => "roman",
                NoteFormat::Symbols => "symbols",
            },
        ),
        note_numbering: given(
            settings.numbering != defaults.numbering,
            match settings.numbering {
                NoteNumbering::Continuous => "continuous",
                NoteNumbering::Page => "page",
                NoteNumbering::Section => "section",
            },
        ),
        note_gap: given(
            settings.gap != defaults.gap,
            &odf_length(settings.gap).to_string(),
        ),
        ..odf::TextSettings::default()
    }
}

/// Adds the note of the model for a note of ODF, whose text starts with
/// its mark in the format of the citation in the body, and returns its
/// id.
#[allow(clippy::too_many_arguments)]
fn import_note(
    source: &odf::TextDocument,
    document: &mut Document,
    ids: &HashMap<(odf::Family, &str), StyleId>,
    default: StyleId,
    note: &odf::Note,
    mark_attrs: &CharAttrs,
    mark_style: Option<StyleId>,
    relative: bool,
) -> Result<NodeId, FilterError> {
    let id = document.new_id();
    let mut paragraphs = Vec::with_capacity(note.body.len().max(1));
    for (i, paragraph) in note.body.iter().enumerate() {
        let built = import_paragraph(
            source,
            document,
            ids,
            default,
            paragraph,
            relative,
            &Marks::new(),
        )?;
        if i > 0 {
            paragraphs.push(built);
            continue;
        }
        let mark = Run::with_attrs(
            &FIELD_CHARACTER.to_string(),
            mark_style,
            CharAttrs {
                field: Some(Some(Field::NoteMark { note: id })),
                ..mark_attrs.clone()
            },
        );
        let runs = std::iter::once(mark).chain(built.runs().cloned());
        paragraphs.push(Paragraph::from_runs(
            built.id(),
            built.style(),
            built.attrs().clone(),
            runs,
        )?);
    }
    if paragraphs.is_empty() {
        paragraphs.push(Paragraph::new(document.new_id(), default));
    }
    document.apply(Op::SetNote {
        id,
        note: Some(Note {
            id,
            mark: note.label.as_deref().map(Into::into),
            paragraphs: paragraphs.into(),
        }),
    })?;
    Ok(id)
}

/// How a number of a page is written, from `style:num-format`.
fn number_format(format: Option<&str>) -> NumberFormat {
    match format {
        Some("i") => NumberFormat::LowerRoman,
        Some("I") => NumberFormat::UpperRoman,
        Some("a") => NumberFormat::LowerAlpha,
        Some("A") => NumberFormat::UpperAlpha,
        _ => NumberFormat::Decimal,
    }
}

/// The `style:num-format` of a number of a page, unless it is in digits.
fn num_format(format: NumberFormat) -> Option<String> {
    let format = match format {
        NumberFormat::Decimal => return None,
        NumberFormat::LowerRoman => "i",
        NumberFormat::UpperRoman => "I",
        NumberFormat::LowerAlpha => "a",
        NumberFormat::UpperAlpha => "A",
    };
    Some(format.to_owned())
}

/// The field of the model for a field of ODF that shows a text.
fn model_field(field: &odf::Field, shown: &str) -> Field {
    match field {
        odf::Field::PageNumber { num_format } => Field::PageNumber {
            format: number_format(num_format.as_deref()),
        },
        odf::Field::PageCount { num_format } => Field::PageCount {
            format: number_format(num_format.as_deref()),
        },
        odf::Field::Date { pattern, fixed } => Field::DateTime {
            pattern: pattern.as_deref().unwrap_or_default().into(),
            shown: shown.into(),
            update: !fixed,
        },
        odf::Field::FileName { display } => Field::FileName {
            shown: shown.into(),
            path: matches!(display.as_str(), "full" | "path"),
            extension: matches!(display.as_str(), "full" | "name-and-extension"),
        },
        // Notes and page references are read on their own.
        odf::Field::Note(_) | odf::Field::PageOf { .. } => Field::FileName {
            shown: shown.into(),
            path: false,
            extension: true,
        },
    }
}

/// The field of ODF for a field of the model, with the text it shows.
fn odf_field(field: &Field) -> (odf::Field, String) {
    match field {
        Field::PageNumber { format } => (
            odf::Field::PageNumber {
                num_format: num_format(*format),
            },
            field.text(1, 1),
        ),
        Field::PageCount { format } => (
            odf::Field::PageCount {
                num_format: num_format(*format),
            },
            field.text(1, 1),
        ),
        Field::DateTime {
            pattern,
            shown,
            update,
        } => (
            odf::Field::Date {
                pattern: (!pattern.is_empty()).then(|| pattern.to_string()),
                fixed: !update,
            },
            shown.to_string(),
        ),
        Field::FileName {
            shown,
            path,
            extension,
        } => {
            // ODF has no folder without the extension: the name shows
            // as it is.
            let display = match (path, extension) {
                (true, _) => "full",
                (false, true) => "name-and-extension",
                (false, false) => "name",
            };
            (
                odf::Field::FileName {
                    display: display.to_owned(),
                },
                shown.to_string(),
            )
        }
        // The body exports notes and page references on their own.
        Field::NoteMark { .. } | Field::PageOf { .. } => (
            odf::Field::FileName {
                display: "name".to_owned(),
            },
            String::new(),
        ),
    }
}

/// The paragraphs of the body by the names of their bookmarks of one
/// point, which page references name (EST-22).
type Marks = HashMap<String, NodeId>;

fn import_paragraph(
    source: &odf::TextDocument,
    document: &mut Document,
    ids: &HashMap<(odf::Family, &str), StyleId>,
    default: StyleId,
    paragraph: &odf::Paragraph,
    relative: bool,
    marks: &Marks,
) -> Result<Paragraph, FilterError> {
    // A paragraph style that is automatic holds direct formatting over a
    // common style, its parent.
    let mut style = default;
    let mut attrs = ParaAttrs::default();
    let mut base = CharAttrs::default();
    // The size of the text of the paragraph, for the spacing of letters.
    let paragraph_size = odf_font_size(source, odf::Family::Paragraph, paragraph.style.as_deref())
        .unwrap_or_else(|| odf_default_size(source));
    if let Some(name) = &paragraph.style {
        if let Some(&id) = ids.get(&(odf::Family::Paragraph, name.as_str())) {
            style = id;
        } else if let Some(automatic) = source
            .automatic_styles
            .iter()
            .find(|s| s.family == odf::Family::Paragraph && &s.name == name)
        {
            style = automatic
                .parent
                .as_deref()
                .and_then(|p| ids.get(&(odf::Family::Paragraph, p)).copied())
                .unwrap_or(default);
            attrs = para_attrs(&automatic.properties.paragraph, &automatic.properties.text);
            base = char_attrs(&automatic.properties.text, paragraph_size);
        }
    }
    if relative {
        let resolved = document.styles().resolve_paragraph(style, &attrs);
        if tabs_move_at(&attrs, &resolved) {
            attrs.tab_stops = Some(shifted(&resolved.tab_stops, resolved.left_indent));
        }
    }
    let mut runs = Vec::with_capacity(paragraph.spans.len());
    for span in &paragraph.spans {
        let mut run_style = None;
        let mut run_attrs = base.clone();
        if let Some(name) = &span.style {
            if let Some(&id) = ids.get(&(odf::Family::Text, name.as_str())) {
                run_style = Some(id);
            } else if let Some(automatic) = source
                .automatic_styles
                .iter()
                .find(|s| s.family == odf::Family::Text && &s.name == name)
            {
                run_style = automatic
                    .parent
                    .as_deref()
                    .and_then(|p| ids.get(&(odf::Family::Text, p)).copied());
                let size =
                    odf_font_size(source, odf::Family::Text, Some(name)).unwrap_or(paragraph_size);
                run_attrs.merge(&char_attrs(&automatic.properties.text, size));
            }
        }
        if let Some(odf::Field::Note(note)) = &span.field {
            let id = import_note(
                source, document, ids, default, note, &run_attrs, run_style, relative,
            )?;
            run_attrs.field = Some(Some(Field::NoteMark { note: id }));
            runs.push(Run::with_attrs(
                &FIELD_CHARACTER.to_string(),
                run_style,
                run_attrs,
            ));
            continue;
        }
        if let Some(link) = &span.link {
            run_attrs.link = Some(Some(link.as_str().into()));
        }
        if let Some(bookmark) = &span.bookmark {
            run_attrs.bookmark = Some(Some(bookmark.as_str().into()));
        }
        if let Some(odf::Field::PageOf { name }) = &span.field {
            // A reference to a bookmark that is not at a paragraph of the
            // body keeps only its text.
            if let Some(&target) = marks.get(name) {
                run_attrs.field = Some(Some(Field::PageOf { paragraph: target }));
                runs.push(Run::with_attrs(
                    &FIELD_CHARACTER.to_string(),
                    run_style,
                    run_attrs,
                ));
            } else {
                runs.push(Run::with_attrs(&span.text, run_style, run_attrs));
            }
            continue;
        }
        if let Some(field) = &span.field {
            run_attrs.field = Some(Some(model_field(field, &span.text)));
            runs.push(Run::with_attrs(
                &FIELD_CHARACTER.to_string(),
                run_style,
                run_attrs,
            ));
            continue;
        }
        // The model keeps paragraph breaks out of runs.
        let text: String = span
            .text
            .chars()
            .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
            .collect();
        runs.push(Run::with_attrs(&text, run_style, run_attrs));
    }
    let id = document.new_id();
    Ok(Paragraph::from_runs(id, style, attrs, runs)?)
}

/// Turns a style name into an ODF name: ASCII letters, digits, `-`, `_`
/// and `.` stay, and anything else becomes `_xx_`, as LibreOffice writes
/// `_20_` for a space. The name shown stays in `style:display-name`.
fn odf_name(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_alphabetic()
            || c == '_'
            || (i > 0 && (c.is_ascii_digit() || matches!(c, '-' | '.')))
        {
            out.push(c);
        } else {
            out.push_str(&format!("_{:x}_", u32::from(c)));
        }
    }
    if out.is_empty() { "_".to_owned() } else { out }
}

/// The ODF properties of a level of styles, a style or a paragraph, whose
/// format resolves to `resolved`, with the tab stops measured from the
/// indent. The default style says how far apart the default stops are,
/// which other programs set otherwise.
fn odf_paragraph_properties(
    attrs: &ParaAttrs,
    resolved: &ParaFormat,
    default: bool,
) -> odf::ParagraphProperties {
    let mut properties = paragraph_properties(attrs);
    if tabs_move_at(attrs, resolved) {
        let stops = shifted(&resolved.tab_stops, -resolved.left_indent);
        properties.tab_stops = Some(stops.stops().iter().map(odf_tab_stop).collect());
    }
    if default {
        properties.tab_stop_distance = Some(odf_length(resolved.default_tab_interval));
    }
    properties
}

/// `fo:hyphenate` and `desk:hyphenate` of a paragraph or a style that sets
/// hyphenation. In a document that is not hyphenated, other programs read
/// that no paragraph is, and Desk what the paragraph allows.
fn odf_hyphenate(hyphenate: Option<bool>, document: bool) -> (Option<bool>, Option<bool>) {
    match hyphenate {
        Some(true) if !document => (Some(false), Some(true)),
        other => (other, None),
    }
}

/// The notes that the body refers to, in order, as they are exported.
struct NoteRefs<'a> {
    document: &'a Document,
    order: Vec<NodeId>,
    /// The names of the bookmarks of one point at the paragraphs that
    /// page references name.
    marks: HashMap<NodeId, String>,
}

impl NoteRefs<'_> {
    /// The note of ODF that a mark in the body stands for, with the text
    /// of its citation, its body left to fill.
    fn note(&mut self, id: NodeId) -> Option<(odf::Field, String)> {
        let note = self.document.notes().get(id)?;
        let settings = self.document.text_settings().notes;
        self.order.push(id);
        let n = self.order.len();
        let shown = match &note.mark {
            Some(mark) => mark.to_string(),
            None => {
                let numbered = self
                    .order
                    .iter()
                    .filter(|id| {
                        self.document
                            .notes()
                            .get(**id)
                            .is_some_and(|n| n.mark.is_none())
                    })
                    .count();
                settings.format.label(numbered as u32)
            }
        };
        let field = odf::Field::Note(Box::new(odf::Note {
            id: Some(format!("ftn{n}")),
            endnote: settings.kind != NoteKind::Footnotes,
            label: note.mark.as_deref().map(str::to_owned),
            body: Vec::new(),
        }));
        Some((field, shown))
    }
}

/// The spans of ODF for the runs of a paragraph, with the text style that
/// `style` gives each run. A field is a span of its own for each of its
/// characters. The marks of notes become notes of ODF when `notes` takes
/// them, as in the body; elsewhere they are left out, since a note of ODF
/// shows its own mark at the start of its text.
fn export_spans(
    paragraph: &Paragraph,
    mut notes: Option<&mut NoteRefs<'_>>,
    mut style: impl FnMut(&Run) -> Option<String>,
) -> Vec<odf::Span> {
    let mut spans = Vec::new();
    for run in paragraph.runs() {
        let name = style(run);
        let link = run.attrs.link.clone().flatten().map(|l| l.to_string());
        let bookmark = run.attrs.bookmark.clone().flatten().map(|b| b.to_string());
        let first = spans.len();
        match &run.attrs.field {
            Some(Some(Field::PageOf { paragraph })) => {
                let name_of = notes.as_deref().and_then(|n| n.marks.get(paragraph));
                for _ in run.text().chars() {
                    spans.push(odf::Span {
                        style: name.clone(),
                        text: "1".to_owned(),
                        field: name_of.map(|name| odf::Field::PageOf { name: name.clone() }),
                        ..odf::Span::default()
                    });
                }
            }
            Some(Some(Field::NoteMark { note })) => {
                let Some(notes) = notes.as_deref_mut() else {
                    continue;
                };
                for _ in run.text().chars() {
                    if let Some((field, shown)) = notes.note(*note) {
                        spans.push(odf::Span {
                            style: name.clone(),
                            text: shown,
                            field: Some(field),
                            ..odf::Span::default()
                        });
                    }
                }
            }
            Some(Some(field)) => {
                let (field, shown) = odf_field(field);
                for _ in run.text().chars() {
                    spans.push(odf::Span {
                        style: name.clone(),
                        text: shown.clone(),
                        field: Some(field.clone()),
                        ..odf::Span::default()
                    });
                }
            }
            _ => spans.push(odf::Span {
                style: name,
                text: run.text().replace(LINE_BREAK, &odf::LINE_BREAK.to_string()),
                ..odf::Span::default()
            }),
        }
        for span in &mut spans[first..] {
            span.link.clone_from(&link);
            span.bookmark.clone_from(&bookmark);
        }
    }
    spans
}

/// The names of the master pages of the sections that have headers and
/// footers of their own, by the paragraph that starts each (EST-16).
fn section_masters(document: &Document) -> HashMap<NodeId, String> {
    let mut out = HashMap::new();
    for paragraph in document.body().iter().skip(1) {
        if let Some(Some(start)) = &paragraph.attrs().start
            && let Start::Section(section, _) = &**start
            && section.own_margins
        {
            let name = format!("Desk{}", out.len() + 1);
            out.insert(paragraph.id(), name);
        }
    }
    out
}

/// The headers and footers of a document in its master pages, the first
/// section's in "Standard" and those of later sections in their own,
/// with the automatic styles of their direct formatting.
fn export_margins(
    document: &Document,
    names: &HashMap<StyleId, String>,
    out: &mut odf::TextDocument,
) {
    let styles = document.styles();
    let setup = document.page_setup();
    let mut automatic: Vec<odf::Style> = Vec::new();
    let mut automatic_name = |family: odf::Family, parent: String, properties: odf::Properties| {
        if let Some(existing) = automatic.iter().find(|s| {
            s.family == family && s.parent.as_ref() == Some(&parent) && s.properties == properties
        }) {
            return existing.name.clone();
        }
        let prefix = match family {
            odf::Family::Text => "MT",
            _ => "MP",
        };
        let count = automatic.iter().filter(|s| s.family == family).count();
        let name = format!("{prefix}{}", count + 1);
        automatic.push(odf::Style {
            name: name.clone(),
            display_name: None,
            family,
            parent: Some(parent),
            next: None,
            list_style: None,
            master_page: None,
            properties,
        });
        name
    };
    let mut part = |margins: &desk_model::HeadersFooters,
                    place: MarginPlace,
                    shown: bool|
     -> Option<odf::HeaderFooter> {
        let paragraphs = margins.get(place);
        let empty = paragraphs.iter().all(Paragraph::is_empty);
        if empty && (place.pages != MarginPages::Default || !shown) {
            return None;
        }
        let mut out = Vec::new();
        for paragraph in paragraphs {
            let style_name = names[&paragraph.style()].clone();
            let own = ParaAttrs {
                list_level: None,
                list_start: None,
                list_override: None,
                list_style: None,
                ..paragraph.attrs().without_structure()
            };
            let style = if own.is_empty() {
                style_name
            } else {
                automatic_name(
                    odf::Family::Paragraph,
                    style_name,
                    odf::Properties {
                        paragraph: odf_paragraph_properties(
                            &own,
                            &styles.resolve_paragraph(paragraph.style(), &own),
                            false,
                        ),
                        text: odf::TextProperties::default(),
                    },
                )
            };
            let spans = export_spans(paragraph, None, |run| {
                let attrs = run.attrs.without_field();
                let named = run.style.map(|id| names[&id].clone());
                if attrs.is_empty() {
                    return named;
                }
                let properties = odf::Properties {
                    paragraph: odf::ParagraphProperties::default(),
                    text: text_properties(
                        &attrs,
                        &styles.resolve_character(paragraph.style(), run.style, &run.attrs),
                    ),
                };
                // A text style without a parent names none.
                Some(match named {
                    Some(parent) => automatic_name(odf::Family::Text, parent, properties),
                    None => automatic_name(odf::Family::Text, String::new(), properties),
                })
            });
            out.push(odf::Paragraph {
                style: Some(style),
                spans,
                ..odf::Paragraph::default()
            });
        }
        if out.is_empty() {
            out.push(odf::Paragraph {
                style: Some(names[&styles.default_paragraph()].clone()),
                ..odf::Paragraph::default()
            });
        }
        Some(odf::HeaderFooter {
            display: shown,
            paragraphs: out,
        })
    };
    let header = |pages| MarginPlace::new(MarginKind::Header, pages);
    let footer = |pages| MarginPlace::new(MarginKind::Footer, pages);
    let mut master = |margins: &desk_model::HeadersFooters| odf::MasterPage {
        header: part(margins, header(MarginPages::Default), setup.headers),
        header_first: part(margins, header(MarginPages::First), setup.headers),
        header_left: part(margins, header(MarginPages::Left), setup.headers),
        footer: part(margins, footer(MarginPages::Default), setup.footers),
        footer_first: part(margins, footer(MarginPages::First), setup.footers),
        footer_left: part(margins, footer(MarginPages::Left), setup.footers),
    };
    out.master = master(document.headers_footers());
    let mut masters: Vec<(NodeId, String)> = section_masters(document).into_iter().collect();
    masters.sort_by_key(|(_, name)| name[4..].parse::<usize>().unwrap_or(0));
    out.masters = masters
        .into_iter()
        .map(|(id, name)| (name, master(document.section_headers_footers(Some(id)))))
        .collect();
    // Text styles without a parent.
    for style in &mut automatic {
        if style.parent.as_deref() == Some("") {
            style.parent = None;
        }
    }
    out.master_styles = automatic;
}

/// Builds an ODF text document from a document.
pub fn export(document: &Document) -> odf::TextDocument {
    let settings = document.text_settings();
    let styles = document.styles();
    let default = styles.default_paragraph();
    let mut names: HashMap<StyleId, String> = HashMap::new();
    let mut taken: HashSet<String> = HashSet::new();
    for style in styles.iter() {
        let name = if style.id() == default {
            DEFAULT_STYLE.to_owned()
        } else {
            let mut name = odf_name(style.name());
            let base = name.clone();
            let mut n = 2;
            while name == DEFAULT_STYLE || taken.contains(&name) {
                name = format!("{base}_{n}");
                n += 1;
            }
            name
        };
        taken.insert(name.clone());
        names.insert(style.id(), name);
    }

    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    let mut out = odf::TextDocument {
        text_settings: odf::TextSettings {
            hyphenation: (!settings.hyphenation).then_some(false),
            ligatures: (!settings.ligatures).then_some(false),
            never_hyphenate: settings.never_hyphenate.iter().cloned().collect(),
            // Other programs add the two spaces between paragraphs, so a
            // document that does not say takes the sum (FMT-15).
            larger_spacing: Some(settings.spacing == ParagraphSpacing::Larger),
            links_inactive: settings.links_inactive.then_some(true),
            ..odf_note_settings(settings.notes)
        },
        ..odf::TextDocument::default()
    };
    out.toc_levels = settings
        .toc
        .levels
        .iter()
        .map(|level| odf::TocLevel {
            style: names[&level.style].clone(),
            entry: names[&level.entry].clone(),
            page_number: level.page_number,
            link: settings.toc.page_links && level.page_number,
        })
        .collect();
    for style in styles.iter() {
        if let Some(key) = style.shortcut() {
            out.style_shortcuts.push(odf::StyleShortcut {
                key,
                family: match style.family() {
                    desk_model::StyleFamily::Paragraph => odf::Family::Paragraph,
                    desk_model::StyleFamily::Character => odf::Family::Text,
                    desk_model::StyleFamily::List => odf::Family::List,
                },
                style: names[&style.id()].clone(),
            });
        }
    }
    out.style_shortcuts.sort_by_key(|s| s.key);
    for style in styles.iter() {
        let name = names[&style.id()].clone();
        let display_name = (name != style.name()).then(|| style.name().to_owned());
        out.styles.push(match style {
            Style::List(l) => {
                out.list_styles.push(odf_list_style(l, name, &mut pictures));
                continue;
            }
            Style::Paragraph(p) => odf::Style {
                name,
                display_name,
                family: odf::Family::Paragraph,
                parent: p.parent.map(|id| names[&id].clone()),
                next: p.next.map(|id| names[&id].clone()),
                list_style: p
                    .paragraph
                    .list_style
                    .map(|list| list.map_or_else(String::new, |id| names[&id].clone())),
                master_page: None,
                properties: odf::Properties {
                    paragraph: odf_paragraph_properties(
                        &p.paragraph,
                        &styles.resolve_paragraph(p.id, &ParaAttrs::default()),
                        p.id == default,
                    ),
                    text: odf::TextProperties {
                        hyphenate: odf_hyphenate(p.paragraph.hyphenate, settings.hyphenation).0,
                        desk_hyphenate: odf_hyphenate(p.paragraph.hyphenate, settings.hyphenation)
                            .1,
                        ..text_properties(
                            &p.character,
                            &styles.resolve_character(p.id, None, &CharAttrs::default()),
                        )
                    },
                },
            },
            Style::Character(c) => odf::Style {
                name,
                display_name,
                family: odf::Family::Text,
                parent: c.parent.map(|id| names[&id].clone()),
                next: None,
                list_style: None,
                master_page: None,
                properties: odf::Properties {
                    paragraph: odf::ParagraphProperties::default(),
                    text: text_properties(
                        &c.character,
                        &styles.resolve_character(default, Some(c.id), &CharAttrs::default()),
                    ),
                },
            },
        });
    }

    // Automatic styles for direct formatting, one per distinct format.
    let mut automatic: Vec<odf::Style> = Vec::new();
    let mut automatic_name = |family: odf::Family,
                              parent: Option<String>,
                              list_style: Option<String>,
                              properties: odf::Properties|
     -> String {
        if let Some(existing) = automatic.iter().find(|s| {
            s.family == family
                && s.parent == parent
                && s.list_style == list_style
                && s.properties == properties
        }) {
            return existing.name.clone();
        }
        let prefix = match family {
            odf::Family::Paragraph => "P",
            odf::Family::Text => "T",
            odf::Family::List => "L",
        };
        let count = automatic.iter().filter(|s| s.family == family).count();
        let name = format!("{prefix}{}", count + 1);
        automatic.push(odf::Style {
            name: name.clone(),
            display_name: None,
            family,
            parent,
            next: None,
            list_style,
            master_page: None,
            properties,
        });
        name
    };
    // The lists of the body: which list style each paragraph is an item
    // of, and where each run of neighbors in one list starts.
    let items = styles.list_items(document.body().iter());
    let mut listed: HashSet<StyleId> = HashSet::new();
    let mut previous: Option<StyleId> = None;
    // The automatic list styles of the items with levels of their own.
    let mut overrides: HashMap<[ListLevel; LIST_LEVELS], String> = HashMap::new();
    // The layouts and sections, as section styles: the first one when it
    // is not the plain one, and each one that starts at a paragraph.
    let setup = *document.page_setup();
    let mut section_styles: Vec<odf::SectionStyle> = Vec::new();
    let mut marks: HashMap<NodeId, String> = HashMap::new();
    for paragraph in document.body().iter() {
        for run in paragraph.runs() {
            if let Some(Some(Field::PageOf { paragraph })) = &run.attrs.field {
                let n = marks.len() + 1;
                marks
                    .entry(*paragraph)
                    .or_insert_with(|| format!("_Toc{n}"));
            }
        }
    }
    let mut note_refs = NoteRefs {
        document,
        order: Vec::new(),
        marks,
    };
    // The master pages of the sections with headers and footers of their
    // own, by the paragraph that starts each (EST-16).
    let masters = section_masters(document);
    let mut with_master: Vec<(usize, String)> = Vec::new();
    for (index, (paragraph, item)) in document.body().iter().zip(&items).enumerate() {
        let start = paragraph
            .attrs()
            .start
            .clone()
            .flatten()
            .map(|s| *s)
            .or_else(|| {
                (index == 0
                    && (setup.section != Section::default() || setup.layout != Layout::default()))
                .then_some(Start::Section(setup.section, setup.layout))
            });
        let section_name = start.map(|start| {
            let name = format!("Sect{}", section_styles.len() + 1);
            let section = match &start {
                Start::Section(section, _) => Some(section),
                Start::Layout(_) => None,
            };
            section_styles.push(sections::section_style(
                name.clone(),
                &setup,
                start.layout(),
                section,
            ));
            name
        });
        // The number of the first page, on the first paragraph.
        let first_number = setup.section.first_number.filter(|_| index == 0);
        let style_name = names[&paragraph.style()].clone();
        // The level, the start and the level of its own of an item are
        // in the list elements.
        let own = ParaAttrs {
            list_level: None,
            list_start: None,
            list_override: None,
            toc_entry: None,
            ..paragraph.attrs().clone()
        };
        let style = if own.is_empty() && first_number.is_none() {
            style_name
        } else {
            automatic_name(
                odf::Family::Paragraph,
                Some(style_name),
                own.list_style
                    .map(|list| list.map_or_else(String::new, |id| names[&id].clone())),
                odf::Properties {
                    paragraph: {
                        let mut properties = odf_paragraph_properties(
                            paragraph.attrs(),
                            &styles.resolve_paragraph(paragraph.style(), paragraph.attrs()),
                            false,
                        );
                        if first_number.is_some() {
                            properties.page_number = Some(first_number);
                        }
                        properties
                    },
                    text: {
                        let (hyphenate, desk_hyphenate) =
                            odf_hyphenate(paragraph.attrs().hyphenate, settings.hyphenation);
                        odf::TextProperties {
                            hyphenate,
                            desk_hyphenate,
                            ..odf::TextProperties::default()
                        }
                    },
                },
            )
        };
        // A section with headers and footers of its own starts its master
        // page, which its style names below.
        if let Some(master) = masters.get(&paragraph.id()) {
            with_master.push((out.body.len(), master.clone()));
        }
        let spans = export_spans(paragraph, Some(&mut note_refs), |run| {
            let named = run.style.map(|id| names[&id].clone());
            let attrs = run.attrs.without_field();
            if attrs.is_empty() {
                named
            } else {
                Some(automatic_name(
                    odf::Family::Text,
                    named,
                    None,
                    odf::Properties {
                        paragraph: odf::ParagraphProperties::default(),
                        text: text_properties(
                            &attrs,
                            &styles.resolve_character(paragraph.style(), run.style, &run.attrs),
                        ),
                    },
                ))
            }
        });
        let list = item.as_ref().map(|item| {
            let first = previous != Some(item.style);
            let start = paragraph.attrs().list_start.flatten();
            let style_override =
                paragraph
                    .attrs()
                    .list_override
                    .clone()
                    .flatten()
                    .and_then(|own| {
                        let style = styles.list(item.style)?;
                        let mut levels = (*style.levels).clone();
                        levels[usize::from(item.level)] = (*own).clone();
                        let key = levels.clone();
                        let count = overrides.len();
                        let name = overrides
                            .entry(key)
                            .or_insert_with(|| {
                                let name = format!("LO{}", count + 1);
                                out.automatic_list_styles.push(odf_list_style(
                                    &ListStyle {
                                        levels: levels.into(),
                                        ..style.clone()
                                    },
                                    name.clone(),
                                    &mut pictures,
                                ));
                                name
                            })
                            .clone();
                        Some(name)
                    });
            let member = odf::ListMember {
                style: names[&item.style].clone(),
                level: item.level,
                start,
                style_override,
                first,
                continues: first && start.is_none() && listed.contains(&item.style),
            };
            listed.insert(item.style);
            member
        });
        previous = item.as_ref().map(|i| i.style);
        out.body.push(odf::Paragraph {
            style: Some(style),
            outline_level: None,
            list,
            section: section_name,
            marks: note_refs
                .marks
                .get(&paragraph.id())
                .cloned()
                .into_iter()
                .collect(),
            toc: paragraph.attrs().toc_entry.flatten().is_some(),
            spans,
        });
    }
    // The text of each note, in the note that the body holds (EST-13).
    let mut bodies: HashMap<String, Vec<odf::Paragraph>> = HashMap::new();
    for (n, id) in note_refs.order.iter().enumerate() {
        let note = document.notes().get(*id).expect("the body refers to it");
        let mut paragraphs = Vec::new();
        for paragraph in note.paragraphs.iter() {
            let own = paragraph.attrs().without_structure();
            let style_name = names[&paragraph.style()].clone();
            let style = if own.is_empty() {
                style_name
            } else {
                automatic_name(
                    odf::Family::Paragraph,
                    Some(style_name),
                    None,
                    odf::Properties {
                        paragraph: odf_paragraph_properties(
                            &own,
                            &styles.resolve_paragraph(paragraph.style(), &own),
                            false,
                        ),
                        text: odf::TextProperties::default(),
                    },
                )
            };
            let spans = export_spans(paragraph, None, |run| {
                let named = run.style.map(|id| names[&id].clone());
                let attrs = run.attrs.without_field();
                if attrs.is_empty() {
                    return named;
                }
                Some(automatic_name(
                    odf::Family::Text,
                    named,
                    None,
                    odf::Properties {
                        paragraph: odf::ParagraphProperties::default(),
                        text: text_properties(
                            &attrs,
                            &styles.resolve_character(paragraph.style(), run.style, &run.attrs),
                        ),
                    },
                ))
            });
            paragraphs.push(odf::Paragraph {
                style: Some(style),
                spans,
                ..odf::Paragraph::default()
            });
        }
        bodies.insert(format!("ftn{}", n + 1), paragraphs);
    }
    for span in out.body.iter_mut().flat_map(|p| p.spans.iter_mut()) {
        if let Some(odf::Field::Note(note)) = &mut span.field
            && let Some(body) = note.id.as_ref().and_then(|id| bodies.remove(id))
        {
            note.body = body;
        }
    }
    for (index, master) in with_master {
        let paragraph = &mut out.body[index];
        let style = paragraph.style.clone().unwrap_or_default();
        let base = automatic.iter().find(|s| s.name == style).cloned();
        let n = automatic
            .iter()
            .filter(|s| s.family == odf::Family::Paragraph)
            .count();
        let name = format!("P{}", n + 1);
        let mut own = base.unwrap_or_else(|| odf::Style {
            family: odf::Family::Paragraph,
            parent: Some(style),
            ..odf::Style::default()
        });
        own.name.clone_from(&name);
        own.master_page = Some(master);
        automatic.push(own);
        paragraph.style = Some(name);
    }
    out.automatic_styles = automatic;
    out.section_styles = section_styles;
    export_margins(document, &names, &mut out);
    out.pictures = pictures;

    let page = document.page_setup();
    out.page_layout = odf::PageLayout {
        width: Some(odf_length(page.width)),
        height: Some(odf_length(page.height)),
        margin_top: Some(odf_length(page.margin_top)),
        margin_bottom: Some(odf_length(page.margin_bottom)),
        margin_left: Some(odf_length(page.margin_left)),
        margin_right: Some(odf_length(page.margin_right)),
        mirrored: page.facing_pages.then_some(true),
        columns: None,
        header: None,
        footer: None,
    };
    // The margin of ODF is above the header, whose height takes the rest
    // of the margin of the body (EST-12).
    let room = |body: Length, edge: Length| odf::HeaderFooterStyle {
        min_height: Some(odf_length(Length::from_pt((body - edge).to_pt().max(0.0)))),
        spacing: Some(odf::Length(0.0)),
    };
    if out.master.header.as_ref().is_some_and(|h| h.display) {
        out.page_layout.margin_top = Some(odf_length(page.header_margin.min(page.margin_top)));
        out.page_layout.header = Some(room(page.margin_top, page.header_margin));
    }
    if out.master.footer.as_ref().is_some_and(|f| f.display) {
        out.page_layout.margin_bottom =
            Some(odf_length(page.footer_margin.min(page.margin_bottom)));
        out.page_layout.footer = Some(room(page.margin_bottom, page.footer_margin));
    }
    let meta = document.metadata();
    let some = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    out.meta = odf::Meta {
        title: some(&meta.title),
        subject: None,
        description: some(&meta.comments),
        keywords: meta.keywords.clone(),
        initial_creator: some(&meta.author),
        creator: some(&meta.author),
        generator: Some(format!("Desk/{}", env!("CARGO_PKG_VERSION"))),
    };
    out
}
