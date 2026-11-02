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
    CharacterStyle, DEFAULT_TAB_INTERVAL, Document, FontFeature, FontFeatures, LINE_BREAK,
    Language, Length, Ligatures, LineColor, LineSpacing, LineStyle, Metadata, ModelError, Op,
    PageSetup, ParaAttrs, ParaBorder, ParaFormat, Paragraph, ParagraphStyle, Rgba, Run, Shadow,
    Style, StyleId, TabAlignment, TabLeader, TabStop, TabStops,
};
use desk_odf as odf;

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

/// A shadow from `fo:text-shadow`, which has no opacity: it is opaque.
fn shadow(shadow: odf::TextShadow) -> Option<Shadow> {
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
        opacity: 100,
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
        bold: t.font_weight.map(|w| w >= 600),
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
        // the font size.
        tracking: t
            .letter_spacing
            .map(|l| (l.0 / size.max(1.0) * 1000.0).round() as i32),
        outline: t.outline,
        shadow: t.shadow.map(shadow),
        ligatures,
        font_features,
        background: t.background.map(background),
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
        font_weight: a.bold.map(|b| if b { 700 } else { 400 }),
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
        outline: a.outline,
        shadow: a.shadow.map(odf_shadow),
        background: a.background.map(odf_background),
    }
}

fn para_attrs(p: &odf::ParagraphProperties) -> ParaAttrs {
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
        page_break_before: p.break_before.map(|b| b == odf::Break::Page),
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
        break_before: a.page_break_before.map(|b| {
            if b {
                odf::Break::Page
            } else {
                odf::Break::Auto
            }
        }),
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
    let defaults = &source.default_style;
    let mut default_style = ParagraphStyle::new(default, DEFAULT_NAME);
    default_style.paragraph = para_attrs(&defaults.paragraph);
    default_style.character = char_attrs(&defaults.text, odf_default_size(source));
    if let Some(standard) = standard {
        default_style.name = names[&default].as_str().into();
        default_style
            .paragraph
            .merge(&para_attrs(&standard.properties.paragraph));
        default_style.character.merge(&char_attrs(
            &standard.properties.text,
            odf_default_size(source),
        ));
        default_style.next = standard
            .next
            .as_deref()
            .and_then(|n| ids.get(&(odf::Family::Paragraph, n)).copied());
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
            let built: Style = match style.family {
                odf::Family::Paragraph => ParagraphStyle {
                    id,
                    name: names[&id].as_str().into(),
                    parent: link(&style.parent, odf::Family::Paragraph).or(Some(default)),
                    next: link(&style.next, odf::Family::Paragraph),
                    paragraph: para_attrs(&style.properties.paragraph),
                    character: char_attrs(&style.properties.text, size),
                }
                .into(),
                odf::Family::Text => CharacterStyle {
                    id,
                    name: names[&id].as_str().into(),
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
    for (index, paragraph) in source.body.iter().enumerate() {
        let built = import_paragraph(source, &mut document, &ids, default, paragraph, relative)?;
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
    document.apply(Op::SetPageSetup { page: setup })?;

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

fn import_paragraph(
    source: &odf::TextDocument,
    document: &mut Document,
    ids: &HashMap<(odf::Family, &str), StyleId>,
    default: StyleId,
    paragraph: &odf::Paragraph,
    relative: bool,
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
            attrs = para_attrs(&automatic.properties.paragraph);
            base = char_attrs(&automatic.properties.text, paragraph_size);
        }
    }
    if relative {
        let resolved = document.styles().resolve_paragraph(style, &attrs);
        if tabs_move_at(&attrs, &resolved) {
            attrs.tab_stops = Some(shifted(&resolved.tab_stops, resolved.left_indent));
        }
    }
    let runs = paragraph.spans.iter().map(|span| {
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
        // The model keeps paragraph breaks out of runs.
        let text: String = span
            .text
            .chars()
            .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
            .collect();
        Run::with_attrs(&text, run_style, run_attrs)
    });
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

/// Builds an ODF text document from a document.
pub fn export(document: &Document) -> odf::TextDocument {
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

    let mut out = odf::TextDocument::default();
    for style in styles.iter() {
        let name = names[&style.id()].clone();
        let display_name = (name != style.name()).then(|| style.name().to_owned());
        out.styles.push(match style {
            Style::Paragraph(p) => odf::Style {
                name,
                display_name,
                family: odf::Family::Paragraph,
                parent: p.parent.map(|id| names[&id].clone()),
                next: p.next.map(|id| names[&id].clone()),
                properties: odf::Properties {
                    paragraph: odf_paragraph_properties(
                        &p.paragraph,
                        &styles.resolve_paragraph(p.id, &ParaAttrs::default()),
                        p.id == default,
                    ),
                    text: text_properties(
                        &p.character,
                        &styles.resolve_character(p.id, None, &CharAttrs::default()),
                    ),
                },
            },
            Style::Character(c) => odf::Style {
                name,
                display_name,
                family: odf::Family::Text,
                parent: c.parent.map(|id| names[&id].clone()),
                next: None,
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
    let mut automatic_name =
        |family: odf::Family, parent: Option<String>, properties: odf::Properties| -> String {
            if let Some(existing) = automatic
                .iter()
                .find(|s| s.family == family && s.parent == parent && s.properties == properties)
            {
                return existing.name.clone();
            }
            let prefix = match family {
                odf::Family::Paragraph => "P",
                odf::Family::Text => "T",
            };
            let count = automatic.iter().filter(|s| s.family == family).count();
            let name = format!("{prefix}{}", count + 1);
            automatic.push(odf::Style {
                name: name.clone(),
                display_name: None,
                family,
                parent,
                next: None,
                properties,
            });
            name
        };
    for paragraph in document.body().iter() {
        let style_name = names[&paragraph.style()].clone();
        let style = if paragraph.attrs().is_empty() {
            style_name
        } else {
            automatic_name(
                odf::Family::Paragraph,
                Some(style_name),
                odf::Properties {
                    paragraph: odf_paragraph_properties(
                        paragraph.attrs(),
                        &styles.resolve_paragraph(paragraph.style(), paragraph.attrs()),
                        false,
                    ),
                    text: odf::TextProperties::default(),
                },
            )
        };
        let spans = paragraph
            .runs()
            .map(|run| {
                let named = run.style.map(|id| names[&id].clone());
                let style = if run.attrs.is_empty() {
                    named
                } else {
                    Some(automatic_name(
                        odf::Family::Text,
                        named,
                        odf::Properties {
                            paragraph: odf::ParagraphProperties::default(),
                            text: text_properties(
                                &run.attrs,
                                &styles.resolve_character(paragraph.style(), run.style, &run.attrs),
                            ),
                        },
                    ))
                };
                odf::Span {
                    style,
                    text: run.text().replace(LINE_BREAK, &odf::LINE_BREAK.to_string()),
                }
            })
            .collect();
        out.body.push(odf::Paragraph {
            style: Some(style),
            outline_level: None,
            spans,
        });
    }
    out.automatic_styles = automatic;

    let page = document.page_setup();
    out.page_layout = odf::PageLayout {
        width: Some(odf_length(page.width)),
        height: Some(odf_length(page.height)),
        margin_top: Some(odf_length(page.margin_top)),
        margin_bottom: Some(odf_length(page.margin_bottom)),
        margin_left: Some(odf_length(page.margin_left)),
        margin_right: Some(odf_length(page.margin_right)),
    };
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
