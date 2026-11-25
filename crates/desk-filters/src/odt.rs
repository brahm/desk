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
use std::sync::Arc;

use desk_model::{
    Alignment, Baseline, BorderSides, BorderStyle, Capitalization, CharAttrs, CharFormat,
    CharacterStyle, ChartStyle, DEFAULT_TAB_INTERVAL, Document, DropCap, DropCapShape,
    DropCapStyle, Fill, FontFeature, FontFeatures, FontWeight, ImageFill, LINE_BREAK, LIST_LEVELS,
    LabelFollow, Language, Length, Ligatures, LineColor, LineSpacing, LineStyle, ListLabel,
    ListLevel, ListStyle, Metadata, ModelError, NumberFormat, Op, PageSetup, ParaAttrs, ParaBorder,
    ParaFormat, Paragraph, ParagraphSpacing, ParagraphStyle, Rgba, Run, Shadow, Style, StyleId,
    TabAlignment, TabLeader, TabStop, TabStops, TextSettings,
};
use desk_model::{
    FIELD_CHARACTER, Field, Layout, MarginKind, MarginPages, MarginPlace, NodeId, Note, NoteFormat,
    NoteKind, NoteNumbering, NoteSettings, Section, Start, TocEntry, TocLevel, TocSettings,
};
use desk_odf as odf;

use crate::log::{Content, ImportLog, Warning};
use crate::sections;

mod changes;
mod comments;
pub(crate) mod objects;
mod tables;

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

/// Reads an ODT file into a document, with what was left out (INT-22).
pub fn read_odt_logged(bytes: &[u8]) -> Result<(Document, ImportLog), FilterError> {
    let source = odf::read_odt(bytes)?;
    let mut log = ImportLog::default();
    for (element, count) in &source.skipped {
        let content = match element.as_str() {
            "table" => Content::Tables,
            "annotation" => Content::Comments,
            "changed-region" => Content::AcceptedChanges,
            "object" => Content::UnreadObjects,
            other => Content::Other(other.to_owned()),
        };
        log.add(Warning::Content(content), *count);
    }
    let document = import(&source)?;
    // Changes whose marks no text that Desk reads has, as in tables, are
    // left out as accepted.
    // The pieces of one change are regions of their own (B-073).
    let regions = source
        .tracked_changes
        .iter()
        .map(changes::key)
        .collect::<std::collections::HashSet<_>>()
        .len();
    let unread = regions.saturating_sub(document.changes().iter().count());
    if unread > 0 {
        log.add(Warning::Content(Content::AcceptedChanges), unread);
    }
    Ok((document, log))
}

/// Writes a document as an ODT file.
pub fn write_odt(document: &Document) -> Result<Vec<u8>, FilterError> {
    let _work = desk_table::share_work();
    Ok(odf::write_odt(&export(document))?)
}

/// Writes a document as ODT with a preview of its first page, a PNG image
/// (DOC-09).
pub fn write_odt_with_thumbnail(
    document: &Document,
    thumbnail: Option<&[u8]>,
) -> Result<Vec<u8>, FilterError> {
    // The formulas of all the tables of the document share one budget.
    let _work = desk_table::share_work();
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
fn char_attrs(source: &odf::TextDocument, t: &odf::TextProperties, size: f64) -> CharAttrs {
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
        // `capitalize` is Start Case; `desk:title-case` makes it Title
        // Case (docs/odf-extensions.md).
        (_, Some(odf::TextTransform::Capitalize)) => Some(if t.title_case == Some(true) {
            Capitalization::Title
        } else {
            Capitalization::Start
        }),
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
        underline_color: t.underline_color.map(|c| match c {
            odf::LineColor::FontColor => LineColor::Text,
            odf::LineColor::Color(c) => LineColor::Custom(rgba(c)),
        }),
        strikethrough: t.line_through.map(line_style),
        strikethrough_color: t.line_through_color.map(|c| match c {
            odf::LineColor::FontColor => LineColor::Text,
            odf::LineColor::Color(c) => LineColor::Custom(rgba(c)),
        }),
        color: t.color.map(rgba),
        fill: text_fill(source, t),
        fill_page: t.fill_page,
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
        comments: None,
        change: None,
        formatted: None,
        former: None,
    }
}

/// The gradient or picture that fills the letters of ODF text
/// properties: none for `desk:text-fill="none"`, and nothing set when
/// there is no `desk:text-fill` or what it names is missing (PXV-04).
fn text_fill(source: &odf::TextDocument, t: &odf::TextProperties) -> Option<Option<Fill>> {
    match t.fill.as_deref()? {
        "none" => Some(None),
        "gradient" => {
            let name = t.fill_gradient_name.as_deref()?;
            let gradient = source.gradients.iter().find(|g| g.name == name)?;
            Some(Some(objects::model_gradient(gradient)))
        }
        "bitmap" => {
            let picture = objects::fill_picture(source, t.fill_image_name.as_deref()?)?;
            let mode = match t.fill_image_mode.as_deref() {
                Some("scale-to-fit") => ImageFill::ScaleToFit,
                Some("stretch") => ImageFill::Stretch,
                Some("original-size") => ImageFill::OriginalSize,
                Some("tile") => ImageFill::Tile,
                _ => ImageFill::ScaleToFill,
            };
            let tint = t.fill_tint.map(|color| Rgba {
                a: (t.fill_tint_opacity.unwrap_or(100.0).clamp(0.0, 100.0) * 2.55).round() as u8,
                ..rgba(color)
            });
            Some(Some(Fill::Image {
                picture,
                mode,
                tint,
            }))
        }
        _ => None,
    }
}

/// The gradients and pictures that fill letters (PXV-04), which go to
/// `office:styles` beside those of the objects, under names of their own.
#[derive(Default)]
pub(super) struct TextFills {
    gradients: Vec<odf::Gradient>,
    images: Vec<odf::FillImage>,
    pictures: Vec<(String, Vec<u8>)>,
}

impl TextFills {
    /// The name of a gradient like this one, added when there is none yet.
    fn gradient(&mut self, gradient: odf::Gradient) -> String {
        if let Some(existing) = self.gradients.iter().find(|g| {
            odf::Gradient {
                name: String::new(),
                ..(*g).clone()
            } == gradient
        }) {
            return existing.name.clone();
        }
        let name = format!("Text_Gradient_{}", self.gradients.len() + 1);
        self.gradients.push(odf::Gradient {
            name: name.clone(),
            ..gradient
        });
        name
    }

    /// The name of the fill image of a picture, added to the package when
    /// it is not there yet.
    fn image(&mut self, picture: &desk_model::Picture) -> String {
        let href = match self.pictures.iter().find(|(_, b)| b == picture.data()) {
            Some((href, _)) => href.clone(),
            None => {
                let href = format!(
                    "Pictures/text-fill{}.{}",
                    self.pictures.len() + 1,
                    picture.format().extension()
                );
                self.pictures.push((href.clone(), picture.data().to_vec()));
                href
            }
        };
        if let Some(existing) = self.images.iter().find(|i| i.href == href) {
            return existing.name.clone();
        }
        let name = format!("Text_Image_{}", self.images.len() + 1);
        self.images.push(odf::FillImage {
            name: name.clone(),
            href,
        });
        name
    }

    /// Adds the gradients and pictures to a document and its pictures.
    fn finish(self, out: &mut odf::TextDocument, pictures: &mut Vec<(String, Vec<u8>)>) {
        out.gradients.extend(self.gradients);
        out.fill_images.extend(self.images);
        pictures.extend(self.pictures);
    }

    /// The fill attributes of ODF text properties for a fill of letters.
    fn properties(&mut self, fill: Option<&Fill>, t: &mut odf::TextProperties) {
        match fill {
            None => t.fill = Some("none".into()),
            Some(&Fill::Gradient { from, to, angle }) => {
                t.fill = Some("gradient".into());
                t.fill_gradient_name = Some(self.gradient(objects::odf_gradient(from, to, angle)));
            }
            Some(Fill::Advanced(gradient)) => {
                t.fill = Some("gradient".into());
                t.fill_gradient_name =
                    Some(self.gradient(objects::odf_advanced_gradient(gradient)));
            }
            Some(Fill::Image {
                picture,
                mode,
                tint,
            }) => {
                t.fill = Some("bitmap".into());
                t.fill_image_name = Some(self.image(picture));
                t.fill_image_mode = Some(
                    match mode {
                        ImageFill::ScaleToFill => "scale-to-fill",
                        ImageFill::ScaleToFit => "scale-to-fit",
                        ImageFill::Stretch => "stretch",
                        ImageFill::OriginalSize => "original-size",
                        ImageFill::Tile => "tile",
                    }
                    .into(),
                );
                if let Some(tint) = *tint {
                    t.fill_tint = Some(odf_color(tint));
                    t.fill_tint_opacity = Some((f64::from(tint.a) / 2.55 * 10.0).round() / 10.0);
                }
            }
            // A plain color is the color of the text.
            Some(_) => t.fill = Some("none".into()),
        }
    }
}

/// The ODF text properties of character attributes, at a level of styles,
/// a style or a run, whose format resolves to `resolved`, with the
/// gradients and pictures that fill letters added to `fills`.
fn text_properties(
    a: &CharAttrs,
    resolved: &CharFormat,
    fills: &mut TextFills,
) -> odf::TextProperties {
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
    let mut out = odf::TextProperties {
        font_family,
        font_size: a.font_size.map(odf_length),
        font_size_percent: None,
        font_weight: a.weight.map(FontWeight::value),
        italic: a.italic,
        underline: a.underline.map(line),
        underline_color: a.underline_color.map(|c| match c {
            LineColor::Text => odf::LineColor::FontColor,
            LineColor::Custom(c) => odf::LineColor::Color(odf_color(c)),
        }),
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
            Capitalization::Title | Capitalization::Start => odf::TextTransform::Capitalize,
            Capitalization::None | Capitalization::SmallCaps => odf::TextTransform::None,
        }),
        title_case: a.capitalization.and_then(|c| match c {
            Capitalization::Title => Some(true),
            Capitalization::Start => Some(false),
            _ => None,
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
        fill_page: a.fill_page,
        // A paragraph attribute, which the paragraph styles write.
        hyphenate: None,
        desk_hyphenate: None,
        ..odf::TextProperties::default()
    };
    if let Some(fill) = &a.fill {
        fills.properties(fill.as_ref(), &mut out);
    }
    out
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
        // One line or none is no drop cap, which takes away that of the
        // style (PXV-06). The first word counts as its first character.
        drop_cap: p.drop_cap.map(|d| {
            (d.lines >= 2).then(|| DropCap {
                lines: d.lines.min(u32::from(u8::MAX)) as u8,
                characters: d.length.clamp(1, u32::from(u8::MAX)) as u8,
                distance: d.distance.map_or(Length::ZERO, length),
                raised: d
                    .raised
                    .unwrap_or(0)
                    .min(d.lines - 1)
                    .min(u32::from(u8::MAX)) as u8,
                // Other programs wrap the lines by the rectangle.
                follow_shape: d.follow_shape.unwrap_or(false),
                outdent: d.outdent.unwrap_or(0).min(200) as u8,
                background: d.background.map(|b| {
                    let start = DropCapShape::default();
                    let percent = |v: Option<u32>, or: u8| v.map_or(or, |v| v.min(100) as u8);
                    DropCapShape {
                        fill: rgba(b.fill),
                        border_width: b.border_width.map_or(Length::ZERO, length),
                        border_color: b.border_color.map_or(start.border_color, rgba),
                        border_style: b.border_style.map_or(start.border_style, border_style),
                        scale: percent(b.scale, start.scale).max(10),
                        radius: percent(b.radius, start.radius),
                    }
                }),
            })
        }),
        // Lists come from the list elements and list styles.
        list_style: None,
        list_level: None,
        list_start: None,
        list_override: None,
        toc_entry: None,
        break_change: None,
        break_former: None,
        format_change: None,
        formatted: None,
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
        style: border_style(style),
        offset: padding.map_or(Length::ZERO, length),
    })
}

fn border_style(style: odf::BorderLine) -> BorderStyle {
    match style {
        odf::BorderLine::Solid => BorderStyle::Solid,
        odf::BorderLine::Dashed => BorderStyle::Dashed,
        odf::BorderLine::Dotted => BorderStyle::Dotted,
    }
}

fn odf_border_style(style: BorderStyle) -> odf::BorderLine {
    match style {
        BorderStyle::Solid => odf::BorderLine::Solid,
        BorderStyle::Dashed => odf::BorderLine::Dashed,
        BorderStyle::Dotted => odf::BorderLine::Dotted,
    }
}

/// Writes the rules or border of a paragraph as ODF borders, with
/// `none` on the sides without a line.
fn odf_border(p: &mut odf::ParagraphProperties, border: &ParaBorder) {
    let line = odf::Border::Line {
        width: odf_length(border.width),
        style: odf_border_style(border.style),
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
        drop_cap: a.drop_cap.map(|cap| match cap {
            Some(cap) => odf::DropCap {
                length: u32::from(cap.characters),
                lines: u32::from(cap.lines),
                distance: Some(odf_length(cap.distance)),
                raised: (cap.raised > 0).then_some(u32::from(cap.raised)),
                follow_shape: Some(cap.follow_shape),
                outdent: (cap.outdent > 0).then_some(u32::from(cap.outdent)),
                background: cap.background.map(|b| odf::DropCapBackground {
                    fill: odf_color(b.fill),
                    border_width: (b.border_width > Length::ZERO)
                        .then(|| odf_length(b.border_width)),
                    border_color: (b.border_width > Length::ZERO)
                        .then(|| odf_color(b.border_color)),
                    border_style: (b.border_width > Length::ZERO
                        && b.border_style != BorderStyle::Solid)
                        .then(|| odf_border_style(b.border_style)),
                    scale: Some(u32::from(b.scale)),
                    radius: Some(u32::from(b.radius)),
                }),
            },
            None => odf::DropCap {
                length: 1,
                lines: 1,
                ..odf::DropCap::default()
            },
        }),
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
                            followed_by: follow(level.followed_by.as_deref()),
                            ..ListLevel::new(ListLabel::None, index, Length::ZERO)
                        }
                    }
                    None => ListLevel {
                        label: ListLabel::bullet("\u{2022}"),
                        label_indent: level.space_before.map_or(Length::ZERO, length),
                        text_indent: level.min_label_width.map_or(Length::ZERO, length),
                        followed_by: follow(level.followed_by.as_deref()),
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
            followed_by: follow(level.followed_by.as_deref()),
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
                    let href = match pictures.iter().find(|(_, b)| b == picture.data()) {
                        Some((href, _)) => href.clone(),
                        None => {
                            let href = format!(
                                "Pictures/bullet{}.{}",
                                pictures.len() + 1,
                                picture.format().extension()
                            );
                            pictures.push((href.clone(), picture.data().to_vec()));
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
                followed_by: match level.followed_by {
                    LabelFollow::Tab => None,
                    LabelFollow::Space => Some("space".to_owned()),
                    LabelFollow::Nothing => Some("nothing".to_owned()),
                },
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
    // A page layout document has no body: the empty paragraphs that give
    // other programs its pages are left out (INT-03).
    if let Some(pages) = source.text_settings.layout_pages {
        let mut source = source.clone();
        source.body.truncate(1);
        if let Some(first) = source.body.first_mut() {
            first.spans.clear();
            first.frames.clear();
        }
        let mut document = import_text(&source)?;
        changes::prune(&mut document)?;
        let mut setup = *document.page_setup();
        setup.layout_pages = Some(pages);
        document.apply(Op::SetPageSetup { page: setup })?;
        return Ok(document);
    }
    let mut document = import_text(source)?;
    changes::prune(&mut document)?;
    Ok(document)
}

/// A word processing document from its ODF.
fn import_text(source: &odf::TextDocument) -> Result<Document, FilterError> {
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
    // Those of items that deletions took too (B-068), but not those that
    // a list has as its own.
    let items = || {
        source
            .body
            .iter()
            .chain(source.tracked_changes.iter().flat_map(|r| &r.deleted))
            .filter_map(|p| p.list.as_ref())
    };
    let own: HashSet<&str> = items().map(|m| m.style.as_str()).collect();
    let overriding: HashSet<&str> = items()
        .filter_map(|m| m.style_override.as_deref())
        .filter(|name| !own.contains(name))
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
    default_style.character = char_attrs(source, &defaults.text, odf_default_size(source));
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
            source,
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
                odf::Family::List
                | odf::Family::Graphic
                | odf::Family::Table
                | odf::Family::TableColumn
                | odf::Family::TableRow
                | odf::Family::TableCell => continue,
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
                    character: char_attrs(source, &style.properties.text, size),
                }
                .into(),
                odf::Family::Text => CharacterStyle {
                    id,
                    name: names[&id].as_str().into(),
                    shortcut,
                    parent: link(&style.parent, odf::Family::Text),
                    character: char_attrs(source, &style.properties.text, size),
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
    // Where a header or footer that is off would go, which Desk keeps.
    let kept = |value: &Option<String>| value.as_deref().and_then(odf::Length::parse).map(length);
    if !setup.headers
        && let Some(margin) = kept(&source.text_settings.header_margin)
    {
        setup.header_margin = margin;
    }
    if !setup.footers
        && let Some(margin) = kept(&source.text_settings.footer_margin)
    {
        setup.footer_margin = margin;
    }
    let page_layout = page_layout(source, &setup);
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
    // Those of the paragraphs that deletions took, which keep their
    // bookmarks (B-068).
    let mut taken_ids = TakenIds::new();
    for region in &source.tracked_changes {
        for (i, paragraph) in region.deleted.iter().enumerate() {
            let id = document.new_id();
            taken_ids.insert((region.id.clone(), i), id);
            for mark in &paragraph.marks {
                marks.entry(mark.clone()).or_insert(id);
            }
        }
    }
    // Where the comments of the body start and end, in order: the
    // paragraph, the offset and the comment or its end (W149).
    let mut comments: Vec<(NodeId, usize, odf::Field)> = Vec::new();
    for (index, paragraph) in source.body.iter().enumerate() {
        let mut found = Vec::new();
        let mut built = import_paragraph(
            source,
            &mut document,
            &ids,
            default,
            paragraph,
            relative,
            &marks,
            Some(&mut found),
        )?;
        comments.extend(found.into_iter().map(|(at, f)| (body_ids[index], at, f)));
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
    // The marks of tracked changes came with the comments (W148).
    let (tracked, comments): (Vec<_>, Vec<_>) = comments
        .into_iter()
        .partition(|(_, _, field)| changes::is_mark(field));
    comments::import(&mut document, comments)?;
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
    // The pictures of their objects too, when they have any (EST-12).
    let drawn = std::iter::once(&source.master)
        .chain(source.masters.iter().map(|(_, page)| page))
        .flat_map(odf::MasterPage::paragraphs)
        .any(|paragraph| paragraph.objects().next().is_some());
    let styled = odf::TextDocument {
        default_style: source.default_style.clone(),
        styles: source.styles.clone(),
        automatic_styles: source.master_styles.clone(),
        pictures: if drawn {
            source.pictures.clone()
        } else {
            Vec::new()
        },
        gradients: source.gradients.clone(),
        markers: source.markers.clone(),
        fill_images: source.fill_images.clone(),
        ..odf::TextDocument::default()
    };
    let mut parts: Vec<(Option<NodeId>, &odf::MasterPage)> = vec![(None, master)];
    for (id, name) in &own_masters {
        if let Some((_, page)) = source.masters.iter().find(|(n, _)| n == name) {
            parts.push((Some(*id), page));
        }
    }
    for &(section, master) in &parts {
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
            // Its comments, put on its text once it is there (W149).
            let mut comments: Vec<(NodeId, usize, odf::Field)> = Vec::new();
            for paragraph in &part.paragraphs {
                let mut found = Vec::new();
                let built = import_paragraph(
                    &styled,
                    &mut document,
                    &ids,
                    default,
                    paragraph,
                    relative,
                    &Marks::new(),
                    Some(&mut found),
                )?;
                comments.extend(found.into_iter().map(|(at, f)| (built.id(), at, f)));
                paragraphs.push(built);
            }
            // The objects in its lines (EST-12).
            objects::import_story_inline_objects(
                &styled,
                &mut document,
                &part.paragraphs,
                &mut paragraphs,
                &mut |document, paragraph| {
                    import_paragraph(
                        &styled,
                        document,
                        &ids,
                        default,
                        paragraph,
                        relative,
                        &Marks::new(),
                        None,
                    )
                },
            )?;
            // A header that a deletion emptied still has its change.
            if paragraphs.iter().all(Paragraph::is_empty)
                && !comments.iter().any(|(_, _, field)| changes::is_mark(field))
            {
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
            let (tracked, comments) = split_tracked(comments);
            comments::import(&mut document, comments)?;
            // The regions are in content.xml, with its styles.
            import_tracked(
                source,
                &source.tracked_changes,
                &mut document,
                &ids,
                &lists,
                &Marks::new(),
                &TakenIds::new(),
                default,
                relative,
                tracked,
            )?;
        }
    }

    document.apply(Op::SetPageSetup { page: setup })?;

    // The objects, once the body and the pages they go on are there, and
    // the comments of their text (W149).
    let mut object_comments: Vec<(NodeId, usize, odf::Field)> = Vec::new();
    let moved = objects::import_objects(source, &mut document, &mut |document, paragraph| {
        let mut found = Vec::new();
        let built = import_paragraph(
            source,
            document,
            &ids,
            default,
            paragraph,
            relative,
            &Marks::new(),
            Some(&mut found),
        )?;
        object_comments.extend(found.into_iter().map(|(at, f)| (built.id(), at, f)));
        Ok(built)
    })?;
    // The objects of the masters of the sections (INT-05).
    let setup = *document.page_setup();
    let footer_height = source
        .page_layout
        .footer
        .and_then(|f| f.min_height)
        .map_or(0.0, |h| length(h).to_pt());
    for (section, master) in parts {
        // The objects of the paragraphs of the header and the footer,
        // from their tops: the header margin below the top of the paper,
        // and the height of the footer above its margin.
        let frames = |part: &Option<odf::HeaderFooter>| -> Vec<odf::DrawObject> {
            part.iter()
                .flat_map(|p| &p.paragraphs)
                .flat_map(|p| p.frames.iter().cloned())
                .collect()
        };
        let framed = [
            (frames(&master.header), setup.header_margin.to_pt()),
            (
                frames(&master.footer),
                (setup.height - setup.footer_margin).to_pt() - footer_height,
            ),
        ];
        objects::import_master_objects(
            source,
            master,
            section,
            &framed,
            &mut document,
            &mut |document, paragraph| {
                let mut found = Vec::new();
                let built = import_paragraph(
                    &styled,
                    document,
                    &ids,
                    default,
                    paragraph,
                    relative,
                    &Marks::new(),
                    Some(&mut found),
                )?;
                object_comments.extend(found.into_iter().map(|(at, f)| (built.id(), at, f)));
                Ok(built)
            },
        )?;
    }
    let (object_tracked, object_comments) = split_tracked(object_comments);
    comments::import(&mut document, object_comments)?;
    import_tracked(
        source,
        &source.tracked_changes,
        &mut document,
        &ids,
        &lists,
        &Marks::new(),
        &TakenIds::new(),
        default,
        relative,
        object_tracked,
    )?;

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
            notes: note_settings(text, &source.body, &source.notes_configuration),
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
            // Paused, the document says it tracks none (W148).
            tracking: if source.track_changes {
                desk_model::Tracking::On
            } else if text.tracking_paused {
                desk_model::Tracking::Paused
            } else {
                desk_model::Tracking::Off
            },
            guides: fixed_guides(&text.guides),
            drop_cap_styles: text.drop_cap_styles.as_ref().map_or_else(
                || DropCapStyle::library().to_vec(),
                |styles| styles.iter().filter_map(|s| drop_cap_style(s)).collect(),
            ),
            chart_styles: text
                .chart_styles
                .as_ref()
                .map(|styles| {
                    styles
                        .iter()
                        .filter_map(|s| chart_style(s))
                        .collect::<Vec<_>>()
                })
                .filter(|styles| !styles.is_empty())
                .unwrap_or_else(|| ChartStyle::library().to_vec()),
            custom_formats: text
                .custom_formats
                .iter()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect(),
            author_colors: text
                .author_colors
                .iter()
                .filter_map(|line| author_color(line))
                .collect(),
        },
    })?;

    // Last, as the paragraphs that deletions took join the body, and what
    // came before finds paragraphs by their place in it (W148). Only the
    // deletions whose marks the body has are read.
    import_tracked(
        source,
        &source.tracked_changes,
        &mut document,
        &ids,
        &lists,
        &marks,
        &taken_ids,
        default,
        relative,
        tracked,
    )?;
    // Then the objects that changes moved, with the paragraphs they were
    // at all there.
    changes::import_moves(source, &mut document, &moved)?;

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
    document.set_view(view_settings(&source.view));
    Ok(document)
}

/// How the notes of a document show, from the settings Desk keeps, or as
/// the notes of other programs' files are: endnotes at the end of the
/// document when they are endnotes.
fn note_settings(
    text: &odf::TextSettings,
    body: &[odf::Paragraph],
    configuration: &[odf::NotesConfiguration],
) -> NoteSettings {
    let defaults = NoteSettings::default();
    // What other apps say, when Desk's own settings do not.
    let footnotes = configuration.iter().find(|c| c.class == "footnote");
    let format = text.note_format.clone().or_else(|| {
        match footnotes.and_then(|c| c.num_format.as_deref()) {
            Some("i" | "I") => Some("roman".to_owned()),
            _ => None,
        }
    });
    let numbering = text.note_numbering.clone().or_else(|| {
        match footnotes.and_then(|c| c.start_numbering_at.as_deref()) {
            Some("page") => Some("page".to_owned()),
            Some("chapter") => Some("section".to_owned()),
            _ => None,
        }
    });
    let text = &odf::TextSettings {
        note_format: format,
        note_numbering: numbering,
        ..text.clone()
    };
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

/// The configuration of footnotes and endnotes that other apps read: how
/// they are numbered, and where footnotes go.
fn notes_configuration(settings: NoteSettings) -> Vec<odf::NotesConfiguration> {
    let num_format = match settings.format {
        NoteFormat::Arabic => Some("1"),
        NoteFormat::Roman => Some("i"),
        // ODF has no symbols: other apps number them.
        NoteFormat::Symbols => None,
    };
    let start = match settings.numbering {
        NoteNumbering::Continuous => "document",
        NoteNumbering::Page => "page",
        NoteNumbering::Section => "chapter",
    };
    ["footnote", "endnote"]
        .into_iter()
        .map(|class| odf::NotesConfiguration {
            class: class.to_owned(),
            num_format: num_format.map(str::to_owned),
            start_numbering_at: Some(start.to_owned()),
            footnotes_position: (class == "footnote").then(|| "page".to_owned()),
        })
        .collect()
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

/// Puts the tracked changes whose marks some text has in the document:
/// those of the body, a note, a header or footer, or text boxes and
/// shapes (W148). The paragraphs that its deletions took are read as those
/// of the text, from `source`; the regions are those of the body of the
/// file.
#[allow(clippy::too_many_arguments)]
fn import_tracked(
    source: &odf::TextDocument,
    regions: &[odf::ChangedRegion],
    document: &mut Document,
    ids: &HashMap<(odf::Family, &str), StyleId>,
    lists: &HashMap<&str, StyleId>,
    marks: &Marks,
    taken_ids: &TakenIds,
    default: StyleId,
    relative: bool,
    events: changes::Events,
) -> Result<(), FilterError> {
    if events.is_empty() {
        return Ok(());
    }
    let mut met: HashSet<String> = changes::marked(&events)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut deleted = HashMap::new();
    // The marks in the text that deletions took: the changes it was in
    // before, by paragraph (B-070).
    let mut formers: HashMap<String, Vec<Vec<(usize, odf::Field)>>> = HashMap::new();
    for region in regions {
        if region.kind != odf::ChangeKind::Deletion || !met.contains(&region.id) {
            continue;
        }
        let mut paragraphs = Vec::new();
        let mut inner = Vec::new();
        for (i, paragraph) in region.deleted.iter().enumerate() {
            let mut found = Vec::new();
            let mut built = import_paragraph(
                source,
                document,
                ids,
                default,
                paragraph,
                relative,
                marks,
                Some(&mut found),
            )?;
            found.retain(|(_, field)| changes::is_mark(field));
            inner.push(found);
            let mut attrs = built.attrs().clone();
            // The list it was an item of, for when the deletion is
            // rejected (B-067), with the style its item overrides the
            // list with (B-068).
            if let Some(member) = &paragraph.list
                && let Some(&list) = lists.get(member.style.as_str())
            {
                if document
                    .styles()
                    .resolve_paragraph(built.style(), &attrs)
                    .list_style
                    != Some(list)
                {
                    attrs.list_style = Some(Some(list));
                }
                if member.level > 0 {
                    attrs.list_level = Some(member.level);
                }
                if let Some(start) = member.start {
                    attrs.list_start = Some(Some(start));
                }
                if let Some(own) = member.style_override.as_deref().and_then(|name| {
                    source
                        .automatic_list_styles
                        .iter()
                        .chain(&source.list_styles)
                        .find(|s| s.name == name)
                }) && let Some(style) = document.styles().list(list)
                {
                    let level = usize::from(member.level).min(LIST_LEVELS - 1);
                    let own = list_levels(own, &source.pictures)[level].clone();
                    if own.label.is_some() && &own != style.level(member.level) {
                        attrs.list_override = Some(Some(std::sync::Arc::new(own)));
                    }
                }
            }
            // Its entry of a table of contents and the section it starts
            // (B-068).
            if paragraph.toc {
                let target = built.runs().find_map(|r| match &r.attrs.field {
                    Some(Some(Field::PageOf { paragraph })) => Some(*paragraph),
                    _ => None,
                });
                attrs.toc_entry = Some(Some(TocEntry { target }));
            }
            if let Some(name) = paragraph.section.as_deref() {
                // The number its pages start at, from its own style, as in
                // the body.
                let page_number = paragraph
                    .style
                    .as_deref()
                    .and_then(|name| {
                        source
                            .automatic_styles
                            .iter()
                            .find(|s| s.family == odf::Family::Paragraph && s.name == name)
                    })
                    .and_then(|s| s.properties.paragraph.page_number)
                    .flatten();
                let restart = |layout: Layout| {
                    page_number.map(|n| {
                        Start::Section(
                            Section {
                                first_number: Some(n),
                                ..Section::default()
                            },
                            layout,
                        )
                    })
                };
                let setup = *document.page_setup();
                let start = if name.is_empty() {
                    // The end of a section: the layout of the pages again.
                    let layout = page_layout(source, &setup);
                    Some(restart(layout).unwrap_or(Start::Layout(layout)))
                } else {
                    source
                        .section_styles
                        .iter()
                        .find(|s| s.name == name)
                        .map(|style| {
                            let layout = sections::layout(style, &setup);
                            match sections::section(style, page_number) {
                                Some(section) => Start::Section(section, layout),
                                None => restart(layout).unwrap_or(Start::Layout(layout)),
                            }
                        })
                };
                if let Some(start) = start {
                    if matches!(start, Start::Section(..)) {
                        // The break of a section is part of it.
                        attrs.page_break_before = None;
                    }
                    attrs.start = Some(Some(std::sync::Arc::new(start)));
                }
            }
            // The identifier its bookmarks were given (B-068).
            let id = taken_ids
                .get(&(region.id.clone(), i))
                .copied()
                .unwrap_or(built.id());
            if &attrs != built.attrs() || id != built.id() {
                built = Paragraph::from_runs(
                    id,
                    built.style(),
                    attrs,
                    built.runs().cloned().collect::<Vec<_>>(),
                )?;
            }
            paragraphs.push(built);
        }
        // The objects in the lines it took (REV-01, OBJ-01).
        objects::import_story_inline_objects(
            source,
            document,
            &region.deleted,
            &mut paragraphs,
            &mut |document, paragraph| {
                import_paragraph(
                    source,
                    document,
                    ids,
                    default,
                    paragraph,
                    relative,
                    &Marks::new(),
                    None,
                )
            },
        )?;
        for (_, field) in inner.iter().flatten() {
            if let odf::Field::ChangeStart { id } = field {
                met.insert(id.clone());
            }
        }
        formers.insert(region.id.clone(), inner);
        deleted.insert(region.id.clone(), paragraphs);
    }
    // The formatting that text had before its changes of formatting.
    let previous: HashMap<String, Arc<desk_model::Formatting>> = regions
        .iter()
        .filter(|r| met.contains(&r.id))
        .filter_map(|r| {
            // Empty for text that had no formatting of its own; a name
            // the file does not have leaves it unknown.
            let name = r.previous_style.as_deref()?;
            let (style, attrs) = if name.is_empty() {
                (None, CharAttrs::default())
            } else {
                span_format(source, ids, name, odf_default_size(source))?
            };
            Some((
                r.id.clone(),
                Arc::new(desk_model::Formatting { style, attrs }),
            ))
        })
        .collect();
    // The style and formatting that paragraphs had before their changes
    // of formatting, unknown for an empty name or one the file does not
    // have.
    let paragraphs: HashMap<String, Option<Arc<desk_model::ParaFormatting>>> = regions
        .iter()
        .filter(|r| met.contains(&r.id))
        .filter_map(|r| {
            let name = r.previous_paragraph_style.as_deref()?;
            let before = paragraph_format(source, ids, lists, default, name)
                .map(|(style, attrs)| Arc::new(desk_model::ParaFormatting { style, attrs }));
            Some((r.id.clone(), before))
        })
        .collect();
    changes::import(
        document,
        regions,
        changes::Taken { deleted, formers },
        &previous,
        &paragraphs,
        events,
    )
}

/// The marks of tracked changes among the events of some text, apart
/// from its comments.
fn split_tracked(
    events: Vec<(NodeId, usize, odf::Field)>,
) -> (changes::Events, Vec<(NodeId, usize, odf::Field)>) {
    events
        .into_iter()
        .partition(|(_, _, field)| changes::is_mark(field))
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
    // Its comments, after its mark in its first paragraph (W149).
    let mut comments: Vec<(NodeId, usize, odf::Field)> = Vec::new();
    for (i, paragraph) in note.body.iter().enumerate() {
        let mut found = Vec::new();
        let built = import_paragraph(
            source,
            document,
            ids,
            default,
            paragraph,
            relative,
            &Marks::new(),
            Some(&mut found),
        )?;
        let shift = if i == 0 {
            FIELD_CHARACTER.len_utf8()
        } else {
            0
        };
        comments.extend(found.into_iter().map(|(at, f)| (built.id(), at + shift, f)));
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
    // The objects in its lines (EST-13).
    objects::import_story_inline_objects(
        source,
        document,
        &note.body,
        &mut paragraphs,
        &mut |document, paragraph| {
            import_paragraph(
                source,
                document,
                ids,
                default,
                paragraph,
                relative,
                &Marks::new(),
                None,
            )
        },
    )?;
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
    let (tracked, comments) = split_tracked(comments);
    comments::import(document, comments)?;
    import_tracked(
        source,
        &source.tracked_changes,
        document,
        ids,
        &HashMap::new(),
        &Marks::new(),
        &TakenIds::new(),
        default,
        relative,
        tracked,
    )?;
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
        // Notes, page references, objects and comments are read on their
        // own.
        odf::Field::Note(_)
        | odf::Field::PageOf { .. }
        | odf::Field::Object(_)
        | odf::Field::Annotation(_)
        | odf::Field::AnnotationEnd { .. }
        | odf::Field::ChangeStart { .. }
        | odf::Field::ChangeEnd { .. }
        | odf::Field::Change { .. } => Field::FileName {
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
        // The body exports notes, page references and objects on their
        // own.
        Field::NoteMark { .. } | Field::PageOf { .. } | Field::ObjectMark { .. } => (
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

/// The layout outside any section: the columns of the pages.
fn page_layout(source: &odf::TextDocument, setup: &PageSetup) -> Layout {
    Layout {
        columns: source
            .page_layout
            .columns
            .as_ref()
            .map_or(desk_model::Columns::ONE, |c| {
                sections::columns(
                    c,
                    (setup.width - setup.margin_left - setup.margin_right).to_pt(),
                )
            }),
        ..Layout::default()
    }
}

/// The identifiers of the paragraphs that deletions took, by the region and
/// their place in it, given ahead so that the page references to their
/// bookmarks find them (B-068).
type TakenIds = HashMap<(String, usize), NodeId>;

/// The paragraph style and the formatting that a paragraph style of ODF
/// names: a common style, or an automatic one over its parent or the
/// default style, with the list style it sets among `lists`; none for a
/// name that the file does not have.
fn paragraph_format(
    source: &odf::TextDocument,
    ids: &HashMap<(odf::Family, &str), StyleId>,
    lists: &HashMap<&str, StyleId>,
    default: StyleId,
    name: &str,
) -> Option<(StyleId, ParaAttrs)> {
    if let Some(&id) = ids.get(&(odf::Family::Paragraph, name)) {
        return Some((id, ParaAttrs::default()));
    }
    let automatic = source
        .automatic_styles
        .iter()
        .find(|s| s.family == odf::Family::Paragraph && s.name == name)?;
    let style = automatic
        .parent
        .as_deref()
        .and_then(|p| ids.get(&(odf::Family::Paragraph, p)).copied())
        .unwrap_or(default);
    let mut attrs = para_attrs(&automatic.properties.paragraph, &automatic.properties.text);
    // The list style it sets, empty for none.
    if let Some(list) = &automatic.list_style {
        attrs.list_style = Some(lists.get(list.as_str()).copied());
    }
    Some((style, attrs))
}

/// The character style and the formatting that a text style of ODF names:
/// a common style, or an automatic one over its parent, in text of the
/// size `size` around it; none for a name that the file does not have.
fn span_format(
    source: &odf::TextDocument,
    ids: &HashMap<(odf::Family, &str), StyleId>,
    name: &str,
    size: f64,
) -> Option<(Option<StyleId>, CharAttrs)> {
    if let Some(&id) = ids.get(&(odf::Family::Text, name)) {
        return Some((Some(id), CharAttrs::default()));
    }
    let automatic = source
        .automatic_styles
        .iter()
        .find(|s| s.family == odf::Family::Text && s.name == name)?;
    let style = automatic
        .parent
        .as_deref()
        .and_then(|p| ids.get(&(odf::Family::Text, p)).copied());
    let size = odf_font_size(source, odf::Family::Text, Some(name)).unwrap_or(size);
    Some((style, char_attrs(source, &automatic.properties.text, size)))
}

#[allow(clippy::too_many_arguments)]
fn import_paragraph(
    source: &odf::TextDocument,
    document: &mut Document,
    ids: &HashMap<(odf::Family, &str), StyleId>,
    default: StyleId,
    paragraph: &odf::Paragraph,
    relative: bool,
    marks: &Marks,
    mut comments: Option<&mut Vec<(usize, odf::Field)>>,
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
            base = char_attrs(source, &automatic.properties.text, paragraph_size);
        }
    }
    if relative {
        let resolved = document.styles().resolve_paragraph(style, &attrs);
        if tabs_move_at(&attrs, &resolved) {
            attrs.tab_stops = Some(shifted(&resolved.tab_stops, resolved.left_indent));
        }
    }
    let mut runs: Vec<Run> = Vec::with_capacity(paragraph.spans.len());
    for span in &paragraph.spans {
        // Comments, where the text they are on starts and ends: read where
        // `comments` takes them, as in the body (W149).
        if let Some(
            field @ (odf::Field::Annotation(_)
            | odf::Field::AnnotationEnd { .. }
            | odf::Field::ChangeStart { .. }
            | odf::Field::ChangeEnd { .. }
            | odf::Field::Change { .. }),
        ) = &span.field
        {
            if let Some(comments) = comments.as_deref_mut() {
                comments.push((runs.iter().map(Run::len).sum(), field.clone()));
            }
            continue;
        }
        let mut run_style = None;
        let mut run_attrs = base.clone();
        if let Some((style, attrs)) = span
            .style
            .as_deref()
            .and_then(|name| span_format(source, ids, name, paragraph_size))
        {
            run_style = style;
            run_attrs.merge(&attrs);
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
        if let Some(odf::Field::Object(_)) = &span.field {
            // The object itself comes with the other objects, by the
            // identifier of its mark.
            let object = document.new_id();
            run_attrs.field = Some(Some(Field::ObjectMark { object }));
            runs.push(Run::with_attrs(
                &FIELD_CHARACTER.to_string(),
                run_style,
                run_attrs,
            ));
            continue;
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
    marks: &CommentMarks,
    track: Option<&mut Track>,
    mut style: impl FnMut(&Run) -> Option<String>,
) -> Vec<odf::Span> {
    let mut spans = Vec::new();
    let mut offset = 0;
    // Where a tracked change starts and ends, by its identifier, which
    // `changes::export` makes regions of (W148). The change open at the
    // start comes from the break before, and the one at the end goes on
    // past the break when it took it.
    // A change of formatting starts again where the formatting before it
    // differs, as its mark names that formatting.
    let track_on = track.is_some();
    let mut open = (track.as_deref().and_then(|t| t.change), None);
    let mut switch =
        |spans: &mut Vec<odf::Span>,
         to: (Option<NodeId>, Option<Arc<desk_model::Formatting>>),
         run: Option<&Run>,
         previous: &mut dyn FnMut(&desk_model::Formatting) -> Option<String>| {
            if track.is_none() || open == to {
                return;
            }
            let link = run.and_then(|r| r.attrs.link.clone().flatten());
            let bookmark = run.and_then(|r| r.attrs.bookmark.clone().flatten());
            let mark = |field| odf::Span {
                field: Some(field),
                link: link.as_deref().map(str::to_owned),
                bookmark: bookmark.as_deref().map(str::to_owned),
                ..odf::Span::default()
            };
            if let Some(id) = open.0 {
                spans.push(mark(odf::Field::ChangeEnd {
                    id: id.get().to_string(),
                }));
            }
            if let Some(id) = to.0 {
                // The mark of a change of formatting carries the style of the
                // formatting before, which `changes::export` puts on its
                // region: no name for text that had none of its own.
                spans.push(odf::Span {
                    style: to
                        .1
                        .as_deref()
                        .map(|before| previous(before).unwrap_or_default()),
                    ..mark(odf::Field::ChangeStart {
                        id: id.get().to_string(),
                    })
                });
            }
            open = to;
        };
    // The comments that start and end before a run (W149).
    let comments = |spans: &mut Vec<odf::Span>, at: usize, run: Option<&Run>| {
        let Some(marks) = marks.get(&(paragraph.id(), at)) else {
            return;
        };
        let link = run.and_then(|r| r.attrs.link.clone().flatten());
        let bookmark = run.and_then(|r| r.attrs.bookmark.clone().flatten());
        for mark in marks {
            spans.push(odf::Span {
                link: link.as_deref().map(str::to_owned),
                bookmark: bookmark.as_deref().map(str::to_owned),
                ..mark.clone()
            });
        }
    };
    let mut last = None;
    // The change that deleted text was in before, whose marks go inside
    // those of the deletion (B-070), with the formatting before that its
    // mark names. It goes on past a break the deletion took, as the
    // deletion does, and starts again where the formatting before differs.
    let mut former_open = track.as_deref().and_then(|t| t.former.clone());
    // The change open, as `switch` leaves it.
    let mut before_change = track.as_deref().and_then(|t| t.change);
    let came = before_change;
    let close_former = |spans: &mut Vec<odf::Span>,
                        former_open: &mut Option<(NodeId, Option<Arc<desk_model::Formatting>>)>,
                        run: Option<&Run>| {
        if let Some((id, _)) = former_open.take() {
            spans.push(odf::Span {
                field: Some(odf::Field::ChangeEnd {
                    id: id.get().to_string(),
                }),
                link: run
                    .and_then(|r| r.attrs.link.clone().flatten())
                    .map(|l| l.to_string()),
                bookmark: run
                    .and_then(|r| r.attrs.bookmark.clone().flatten())
                    .map(|b| b.to_string()),
                ..odf::Span::default()
            });
        }
    };
    for run in paragraph.runs() {
        let change = run.attrs.change.flatten();
        let former = if track_on {
            run.attrs
                .former
                .flatten()
                .map(|id| (id, run.attrs.formatted.clone().flatten()))
        } else {
            None
        };
        // A change of the deletion closes the change before inside it.
        if former_open.is_some() && (former_open != former || before_change != change) {
            close_former(&mut spans, &mut former_open, Some(run));
        }
        before_change = change;
        // The formatting before is that of the change before, when there
        // is one.
        let formatted = if former.is_none() {
            run.attrs.formatted.clone().flatten()
        } else {
            None
        };
        switch(&mut spans, (change, formatted), Some(run), &mut |before| {
            style(&Run::with_attrs("", before.style, before.attrs.clone()))
        });
        if let Some((id, before)) = &former
            && former_open.is_none()
        {
            let previous = before.as_deref().map(|before| {
                style(&Run::with_attrs("", before.style, before.attrs.clone())).unwrap_or_default()
            });
            spans.push(odf::Span {
                style: previous,
                field: Some(odf::Field::ChangeStart {
                    id: id.get().to_string(),
                }),
                link: run.attrs.link.clone().flatten().map(|l| l.to_string()),
                bookmark: run.attrs.bookmark.clone().flatten().map(|b| b.to_string()),
                ..odf::Span::default()
            });
            former_open = former;
        }
        comments(&mut spans, offset, Some(run));
        offset += run.len();
        last = Some(run);
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
            Some(Some(Field::ObjectMark { object })) => {
                // Where the object is written in its place, in the body, a
                // note, a header or a footer (OBJ-01, EST-12, EST-13).
                let mut placeholder = odf::DrawObject::new(odf::DrawKind::Group(Vec::new()));
                placeholder.name = Some(objects::inline_placeholder(*object));
                spans.push(odf::Span {
                    field: Some(odf::Field::Object(Box::new(placeholder))),
                    ..odf::Span::default()
                });
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
    comments(&mut spans, offset, last);
    let ends = paragraph.attrs().break_change.flatten();
    // The insertion that a deleted break was in before (B-072).
    let break_former = paragraph
        .attrs()
        .break_former
        .flatten()
        .filter(|_| track_on && ends.is_some())
        .map(|id| (id, None));
    // The change before goes on past the break only when the deletion it
    // is in took the break too. An insertion goes on only when the break
    // was in it before, as reading takes one open past a break to hold
    // it; a change of formatting, which no break is in, goes on.
    let insertion = former_open
        .as_ref()
        .is_some_and(|(id, _)| track.as_deref().is_some_and(|t| t.insertions.contains(id)));
    if before_change != ends
        || ends.is_none()
        || (insertion && former_open != break_former)
        || (break_former.is_some() && former_open != break_former)
    {
        close_former(&mut spans, &mut former_open, None);
    }
    switch(&mut spans, (ends, None), None, &mut |_| None);
    if let Some((id, _)) = break_former
        && former_open.is_none()
    {
        spans.push(odf::Span {
            field: Some(odf::Field::ChangeStart {
                id: id.get().to_string(),
            }),
            ..odf::Span::default()
        });
        former_open = break_former;
    }
    if let Some(track) = track {
        track.change = open.0;
        track.former = former_open;
        track.came = came;
    }
    spans
}

/// The tracked changes open past the break of a paragraph, which go on in
/// the next (W148): the change, and the change that its deleted text was
/// in before, with the formatting before that the mark of that one names
/// (B-070); and the change that was open at the start of the paragraph
/// [`export_spans`] exported last, which [`paragraph_change`] reads for
/// that paragraph (B-069).
pub(super) struct Track {
    change: Option<NodeId>,
    /// The change open at the start of the paragraph last exported.
    came: Option<NodeId>,
    former: Option<(NodeId, Option<Arc<desk_model::Formatting>>)>,
    /// The insertions of the document, the changes a break can be in
    /// before a deletion takes it (B-072).
    insertions: std::collections::HashSet<NodeId>,
}

impl Track {
    /// No change open yet, in a text of a document.
    pub(super) fn new(document: &Document) -> Self {
        Self {
            change: None,
            came: None,
            former: None,
            insertions: document
                .changes()
                .iter()
                .filter(|c| c.kind == desk_model::ChangeKind::Insertion)
                .map(|c| c.id)
                .collect(),
        }
    }

    /// The deletion of the break before the paragraph last exported,
    /// which joins it to the one before: none for another change.
    pub(super) fn came_deleted(&self, document: &Document) -> Option<NodeId> {
        self.came.filter(|&id| {
            document
                .changes()
                .get(id)
                .is_some_and(|c| c.kind == desk_model::ChangeKind::Deletion)
        })
    }
}

/// Puts the marks of the change of the formatting of a paragraph around
/// the spans of its text, the first naming the paragraph style that
/// `name` gives for the formatting before, or none when it is unknown
/// (W148). Their names are `p` and the identifier of the change, apart
/// from those of the changes of the text, as `changes::export` reads
/// them. A paragraph that a deletion joins to another keeps them around
/// the text it shows, out of the deletion: after the end of the change
/// that `came` from the break before, and before the start of the one
/// that takes its own break (B-069).
fn paragraph_change(
    paragraph: &Paragraph,
    came: Option<NodeId>,
    spans: &mut Vec<odf::Span>,
    mut name: impl FnMut(&desk_model::ParaFormatting) -> String,
) {
    let Some(id) = paragraph.attrs().format_change.flatten() else {
        return;
    };
    let is = |span: &odf::Span, end: bool, change: NodeId| {
        let change = change.get().to_string();
        match &span.field {
            Some(odf::Field::ChangeEnd { id }) => end && *id == change,
            Some(odf::Field::ChangeStart { id }) => !end && *id == change,
            _ => false,
        }
    };
    let start = came
        .and_then(|came| spans.iter().position(|s| is(s, true, came)))
        .map_or(0, |at| at + 1);
    let end = paragraph
        .attrs()
        .break_change
        .flatten()
        .and_then(|ends| spans.iter().rposition(|s| is(s, false, ends)))
        .filter(|&at| at >= start)
        .unwrap_or(spans.len());
    let before = paragraph.attrs().formatted.clone().flatten();
    let id = format!("p{}", id.get());
    // In the link and the bookmark of the text they are in, so as not to
    // break them.
    let around = |spans: &[odf::Span], at: usize| {
        spans
            .get(at)
            .map(|s| (s.link.clone(), s.bookmark.clone()))
            .unwrap_or_default()
    };
    let (link, bookmark) = around(spans, end);
    spans.insert(
        end,
        odf::Span {
            field: Some(odf::Field::ChangeEnd { id: id.clone() }),
            link,
            bookmark,
            ..odf::Span::default()
        },
    );
    let (link, bookmark) = around(spans, start);
    spans.insert(
        start,
        odf::Span {
            style: Some(before.as_deref().map(&mut name).unwrap_or_default()),
            field: Some(odf::Field::ChangeStart { id }),
            link,
            bookmark,
            ..odf::Span::default()
        },
    );
}

/// The annotations of the comments, by the paragraph and the offset where
/// they go (W149).
type CommentMarks = HashMap<(NodeId, usize), Vec<odf::Span>>;

/// The body of a page layout document, which has none of its own: as
/// many empty paragraphs as it has pages, each but the first after a page
/// break, so that other programs show its pages (INT-03).
fn page_layout_body(out: &mut odf::TextDocument, automatic: &mut Vec<odf::Style>, pages: u32) {
    out.text_settings.layout_pages = Some(pages);
    let mut first = out.body.first().cloned().unwrap_or_default();
    first.spans.clear();
    first.frames.clear();
    let name = "DeskPage".to_owned();
    let mut style = odf::Style {
        name: name.clone(),
        family: odf::Family::Paragraph,
        parent: first.style.clone(),
        ..odf::Style::default()
    };
    if let Some(base) = automatic
        .iter()
        .find(|s| Some(&s.name) == first.style.as_ref())
    {
        style.parent.clone_from(&base.parent);
        style.properties = base.properties.clone();
    }
    style.properties.paragraph.break_before = Some(odf::Break::Page);
    automatic.push(style);
    let frames = std::mem::take(&mut out.body[0].frames);
    out.body.truncate(1);
    out.body[0].frames = frames;
    for _ in 1..pages.max(1) {
        out.body.push(odf::Paragraph {
            style: Some(name.clone()),
            ..first.clone()
        });
    }
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
    fills: &mut TextFills,
    comments: &CommentMarks,
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
        // The tracked change open past a break (W148).
        let mut tracked = Track::new(document);
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
                        ..Default::default()
                    },
                )
            };
            let spans = export_spans(paragraph, None, comments, Some(&mut tracked), |run| {
                let attrs = run.attrs.without_field();
                let named = run.style.and_then(|id| names.get(&id).cloned());
                if attrs.is_empty() {
                    return named;
                }
                let properties = odf::Properties {
                    paragraph: odf::ParagraphProperties::default(),
                    text: text_properties(
                        &attrs,
                        &styles.resolve_character(paragraph.style(), run.style, &run.attrs),
                        fills,
                    ),
                    ..Default::default()
                };
                // A text style without a parent names none.
                Some(match named {
                    Some(parent) => automatic_name(odf::Family::Text, parent, properties),
                    None => automatic_name(odf::Family::Text, String::new(), properties),
                })
            });
            let mut spans = spans;
            paragraph_change(
                paragraph,
                tracked.came_deleted(document),
                &mut spans,
                |before| {
                    let parent = names
                        .get(&before.style)
                        .cloned()
                        .unwrap_or_else(|| names[&styles.default_paragraph()].clone());
                    let own = ParaAttrs {
                        list_level: None,
                        list_start: None,
                        list_override: None,
                        list_style: None,
                        ..before.attrs.without_structure()
                    };
                    if own.is_empty() {
                        return parent;
                    }
                    automatic_name(
                        odf::Family::Paragraph,
                        parent,
                        odf::Properties {
                            paragraph: odf_paragraph_properties(
                                &own,
                                &styles.resolve_paragraph(before.style, &own),
                                false,
                            ),
                            ..Default::default()
                        },
                    )
                },
            );
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
        objects: Vec::new(),
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

/// How a document showed, from the view settings of LibreOffice: the
/// zoom, how many pages side by side, and Desk's outline (INT-08, DOC-16).
fn view_settings(view: &odf::ViewSettings) -> Option<desk_model::ViewSettings> {
    if view.is_empty() {
        return None;
    }
    let zoom = match view.zoom_type {
        Some(2) => desk_model::ViewZoom::FitPage,
        Some(1 | 3 | 4) => desk_model::ViewZoom::FitWidth,
        _ => desk_model::ViewZoom::Percent(
            view.zoom_factor
                .and_then(|f| u16::try_from(f).ok())
                .filter(|f| *f > 0)
                .unwrap_or(100),
        ),
    };
    Some(desk_model::ViewSettings {
        zoom,
        two_pages: view.columns.is_some_and(|c| c >= 2),
        outline: view.outline.unwrap_or(false),
        outline_options: desk_model::OutlineOptions {
            levels: view
                .outline_levels
                .and_then(|l| u8::try_from(l).ok())
                .filter(|l| (1..=desk_model::MAX_OUTLINE_LEVEL).contains(l)),
            first_line: view.outline_first_line.unwrap_or(false),
            thumbnails: view.outline_thumbnails.unwrap_or(false),
        },
        markup: view
            .markup
            .as_deref()
            .and_then(desk_model::Markup::from_key)
            .unwrap_or_default(),
    })
}

fn odf_view(view: desk_model::ViewSettings) -> odf::ViewSettings {
    let (zoom_type, factor) = match view.zoom {
        desk_model::ViewZoom::Percent(p) => (0, i16::try_from(p).unwrap_or(100)),
        desk_model::ViewZoom::FitPage => (2, 100),
        desk_model::ViewZoom::FitWidth => (3, 100),
    };
    odf::ViewSettings {
        zoom_type: Some(zoom_type),
        zoom_factor: Some(factor),
        columns: Some(if view.two_pages { 2 } else { 1 }),
        outline: Some(view.outline),
        outline_levels: view.outline_options.levels.map(i16::from),
        outline_first_line: Some(view.outline_options.first_line),
        outline_thumbnails: Some(view.outline_options.thumbnails),
        // Written only when it is not the markup every document opens in.
        markup: (view.markup != desk_model::Markup::Markup).then(|| view.markup.key().to_owned()),
    }
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
    // The object styles, as graphic styles named for their kind and
    // name, as two kinds may have styles of one name (MOD-04).
    for style in document.object_styles().iter() {
        let base = odf_name(&format!("{} {}", style.kind.name(), style.name));
        let mut name = base.clone();
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        taken.insert(name.clone());
        names.insert(style.id, name);
    }

    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    let mut fills = TextFills::default();
    let mut out = odf::TextDocument {
        text_settings: odf::TextSettings {
            hyphenation: (!settings.hyphenation).then_some(false),
            ligatures: (!settings.ligatures).then_some(false),
            never_hyphenate: settings.never_hyphenate.iter().cloned().collect(),
            // Other programs add the two spaces between paragraphs, so a
            // document that does not say takes the sum (FMT-15).
            larger_spacing: Some(settings.spacing == ParagraphSpacing::Larger),
            links_inactive: settings.links_inactive.then_some(true),
            tracking_paused: settings.tracking == desk_model::Tracking::Paused,
            guides: settings
                .guides
                .iter()
                .map(|g| {
                    let kind = if g.vertical { "v" } else { "h" };
                    match g.page {
                        Some(page) => format!("{kind} {} {}", odf_length(g.at), page + 1),
                        None => format!("{kind} {}", odf_length(g.at)),
                    }
                })
                .collect(),
            // Only when they are not those of a new document.
            drop_cap_styles: (settings.drop_cap_styles != DropCapStyle::library()).then(|| {
                settings
                    .drop_cap_styles
                    .iter()
                    .map(odf_drop_cap_style)
                    .collect()
            }),
            chart_styles: (settings.chart_styles != ChartStyle::library())
                .then(|| settings.chart_styles.iter().map(odf_chart_style).collect()),
            custom_formats: settings
                .custom_formats
                .iter()
                .filter_map(|f| serde_json::to_string(f).ok())
                .collect(),
            author_colors: settings
                .author_colors
                .iter()
                .filter_map(|(author, color)| {
                    serde_json::to_string(&serde_json::json!({
                        "author": author,
                        "color": color.key(),
                    }))
                    .ok()
                })
                .collect(),
            ..odf_note_settings(settings.notes)
        },
        notes_configuration: notes_configuration(settings.notes),
        view: document.view().map(odf_view).unwrap_or_default(),
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
                            &mut fills,
                        )
                    },
                    ..Default::default()
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
                        &mut fills,
                    ),
                    ..Default::default()
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
            odf::Family::Graphic => "gr",
            odf::Family::Table => "ta",
            odf::Family::TableColumn => "co",
            odf::Family::TableRow => "ro",
            odf::Family::TableCell => "ce",
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
    let comment_marks = comments::export(document);
    // The tracked change open past the break of the paragraph before.
    let mut tracked = Track::new(document);
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
            break_change: None,
            break_former: None,
            format_change: None,
            formatted: None,
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
                    ..Default::default()
                },
            )
        };
        // A section with headers and footers of its own starts its master
        // page, which its style names below.
        if let Some(master) = masters.get(&paragraph.id()) {
            with_master.push((out.body.len(), master.clone()));
        }
        let spans = export_spans(
            paragraph,
            Some(&mut note_refs),
            &comment_marks,
            Some(&mut tracked),
            |run| {
                let named = run.style.and_then(|id| names.get(&id).cloned());
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
                                &mut fills,
                            ),
                            ..Default::default()
                        },
                    ))
                }
            },
        );
        let mut spans = spans;
        paragraph_change(
            paragraph,
            tracked.came_deleted(document),
            &mut spans,
            |before| {
                let parent = names
                    .get(&before.style)
                    .cloned()
                    .unwrap_or_else(|| names[&styles.default_paragraph()].clone());
                // The level and start of an item are in the list elements; a
                // list style that no longer is names none.
                let own = ParaAttrs {
                    list_level: None,
                    list_start: None,
                    list_override: None,
                    ..before.attrs.without_structure()
                };
                if own.is_empty() {
                    return parent;
                }
                let (hyphenate, desk_hyphenate) =
                    odf_hyphenate(own.hyphenate, settings.hyphenation);
                automatic_name(
                    odf::Family::Paragraph,
                    Some(parent),
                    own.list_style.map(|list| {
                        list.and_then(|id| names.get(&id).cloned())
                            .unwrap_or_default()
                    }),
                    odf::Properties {
                        paragraph: odf_paragraph_properties(
                            &own,
                            &styles.resolve_paragraph(before.style, &own),
                            false,
                        ),
                        text: odf::TextProperties {
                            hyphenate,
                            desk_hyphenate,
                            ..odf::TextProperties::default()
                        },
                        ..Default::default()
                    },
                )
            },
        );
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
            frames: Vec::new(),
        });
    }
    // The text of each note, in the note that the body holds (EST-13).
    let mut bodies: HashMap<String, Vec<odf::Paragraph>> = HashMap::new();
    for (n, id) in note_refs.order.iter().enumerate() {
        let note = document.notes().get(*id).expect("the body refers to it");
        let mut paragraphs = Vec::new();
        // The tracked change open past a break (W148).
        let mut tracked = Track::new(document);
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
                        ..Default::default()
                    },
                )
            };
            let spans = export_spans(paragraph, None, &comment_marks, Some(&mut tracked), |run| {
                let named = run.style.and_then(|id| names.get(&id).cloned());
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
                            &mut fills,
                        ),
                        ..Default::default()
                    },
                ))
            });
            let mut spans = spans;
            paragraph_change(
                paragraph,
                tracked.came_deleted(document),
                &mut spans,
                |before| {
                    let parent = names
                        .get(&before.style)
                        .cloned()
                        .unwrap_or_else(|| names[&styles.default_paragraph()].clone());
                    // The level and start of an item are in the list elements; a
                    // list style that no longer is names none.
                    let own = ParaAttrs {
                        list_level: None,
                        list_start: None,
                        list_override: None,
                        ..before.attrs.without_structure()
                    };
                    if own.is_empty() {
                        return parent;
                    }
                    let (hyphenate, desk_hyphenate) =
                        odf_hyphenate(own.hyphenate, settings.hyphenation);
                    automatic_name(
                        odf::Family::Paragraph,
                        Some(parent),
                        own.list_style.map(|list| {
                            list.and_then(|id| names.get(&id).cloned())
                                .unwrap_or_default()
                        }),
                        odf::Properties {
                            paragraph: odf_paragraph_properties(
                                &own,
                                &styles.resolve_paragraph(before.style, &own),
                                false,
                            ),
                            text: odf::TextProperties {
                                hyphenate,
                                desk_hyphenate,
                                ..odf::TextProperties::default()
                            },
                            ..Default::default()
                        },
                    )
                },
            );
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
    objects::export_objects(
        &mut objects::Exporter {
            document,
            names: &names,
            automatic: &mut automatic,
            pictures: &mut pictures,
            gradients: Vec::new(),
            markers: Vec::new(),
            fill_images: Vec::new(),
            charts: Vec::new(),
            text_fills: &mut fills,
            comments: &comment_marks,
            master: false,
        },
        &mut out,
    );
    if let Some(pages) = document.page_setup().layout_pages {
        page_layout_body(&mut out, &mut automatic, pages);
    }
    out.automatic_styles = automatic;
    out.section_styles = section_styles;
    export_margins(document, &names, &mut fills, &comment_marks, &mut out);
    objects::export_master_objects(
        document,
        &names,
        &section_masters(document),
        &mut pictures,
        &mut fills,
        &comment_marks,
        &mut out,
    );
    fills.finish(&mut out, &mut pictures);
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
    let default = PageSetup::DEFAULT_HEADER_MARGIN;
    if out.master.header.as_ref().is_some_and(|h| h.display) {
        out.page_layout.margin_top = Some(odf_length(page.header_margin.min(page.margin_top)));
        out.page_layout.header = Some(room(page.margin_top, page.header_margin));
    } else if page.header_margin != default {
        out.text_settings.header_margin = Some(odf_length(page.header_margin).to_string());
    }
    if out.master.footer.as_ref().is_some_and(|f| f.display) {
        out.page_layout.margin_bottom =
            Some(odf_length(page.footer_margin.min(page.margin_bottom)));
        out.page_layout.footer = Some(room(page.margin_bottom, page.footer_margin));
    } else if page.footer_margin != default {
        out.text_settings.footer_margin = Some(odf_length(page.footer_margin).to_string());
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
    // Last, as it joins paragraphs whose breaks deletions took (W148).
    out.tracked_changes = changes::export_all(&mut out, document);
    out.track_changes = document.text_settings().tracking == desk_model::Tracking::On;
    out
}

/// The guides fixed on the pages, from the settings of the document
/// (OBJ-28): `h` or `v`, the distance from the edge of the paper, and the
/// page from 1, which a guide on every page has not. One that cannot be
/// read is left out.
/// A style of drop caps from its line in the settings: words of a name,
/// `=` and a value, as `lines=3 weight=700 color=#000000`, which
/// [`odf_drop_cap_style`] writes.
fn drop_cap_style(line: &str) -> Option<DropCapStyle> {
    let mut style = DropCapStyle::library()[0];
    let mut shape: Option<DropCapShape> = None;
    let number = |v: &str, max: u32| {
        v.trim_end_matches('%')
            .parse::<u32>()
            .ok()
            .map(|n| n.min(max))
    };
    for word in line.split_whitespace() {
        let (name, value) = word.split_once('=')?;
        let cap = &mut style.cap;
        match name {
            "lines" => cap.lines = number(value, 255)?.max(2) as u8,
            "characters" => cap.characters = number(value, 255)?.max(1) as u8,
            "raised" => cap.raised = number(value, 255)? as u8,
            "wrap" => cap.follow_shape = value == "shape",
            "distance" => cap.distance = length(odf::Length::parse(value)?),
            "outdent" => cap.outdent = number(value, 200)? as u8,
            "weight" => style.weight = desk_model::FontWeight::new(number(value, 900)? as u16),
            "color" => style.color = rgba(odf::Color::parse(value)?),
            "fill" => {
                shape.get_or_insert_with(DropCapShape::default).fill =
                    rgba(odf::Color::parse(value)?)
            }
            "border" => {
                shape.get_or_insert_with(DropCapShape::default).border_width =
                    length(odf::Length::parse(value)?)
            }
            "border-color" => {
                shape.get_or_insert_with(DropCapShape::default).border_color =
                    rgba(odf::Color::parse(value)?)
            }
            "border-style" => {
                shape.get_or_insert_with(DropCapShape::default).border_style = match value {
                    "dashed" => BorderStyle::Dashed,
                    "dotted" => BorderStyle::Dotted,
                    _ => BorderStyle::Solid,
                }
            }
            "scale" => {
                shape.get_or_insert_with(DropCapShape::default).scale =
                    number(value, 100)?.max(10) as u8
            }
            "radius" => {
                shape.get_or_insert_with(DropCapShape::default).radius = number(value, 100)? as u8
            }
            _ => {}
        }
    }
    style.cap.raised = style.cap.raised.min(style.cap.lines - 1);
    style.cap.background = shape;
    Some(style)
}

/// A chart style of the `ChartStyles` item of the settings (W080): the
/// fills and the text as JSON, as `desk:chart` keeps them.
fn chart_style(line: &str) -> Option<ChartStyle> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let fills: Vec<desk_model::chart::SeriesFill> =
        serde_json::from_value(value.get("fills")?.clone()).ok()?;
    if fills.is_empty() {
        return None;
    }
    let text = value
        .get("text")
        .and_then(|t| serde_json::from_value(t.clone()).ok())
        .unwrap_or_default();
    Some(ChartStyle { fills, text })
}

/// A chart style as a line of the `ChartStyles` item.
fn odf_chart_style(style: &ChartStyle) -> String {
    serde_json::json!({ "fills": style.fills, "text": style.text }).to_string()
}

fn odf_drop_cap_style(style: &DropCapStyle) -> String {
    let cap = &style.cap;
    let mut line = format!(
        "lines={} characters={} raised={} wrap={} distance={} outdent={}% weight={} color={}",
        cap.lines,
        cap.characters,
        cap.raised,
        if cap.follow_shape {
            "shape"
        } else {
            "rectangle"
        },
        odf_length(cap.distance),
        cap.outdent,
        style.weight.value(),
        odf_color(style.color),
    );
    if let Some(shape) = cap.background {
        let border_style = match shape.border_style {
            BorderStyle::Solid => "solid",
            BorderStyle::Dashed => "dashed",
            BorderStyle::Dotted => "dotted",
        };
        line.push_str(&format!(
            " fill={} border={} border-color={} border-style={border_style} scale={}% radius={}%",
            odf_color(shape.fill),
            odf_length(shape.border_width),
            odf_color(shape.border_color),
            shape.scale,
            shape.radius,
        ));
    }
    line
}

fn fixed_guides(guides: &[String]) -> Vec<desk_model::FixedGuide> {
    let mut out: Vec<desk_model::FixedGuide> = guides
        .iter()
        .filter_map(|g| {
            let mut words = g.split_whitespace();
            let vertical = match words.next()? {
                "v" => true,
                "h" => false,
                _ => return None,
            };
            let at = length(odf::Length::parse(words.next()?)?);
            let page = match words.next() {
                Some(page) => Some(page.parse::<u32>().ok()?.checked_sub(1)?),
                None => None,
            };
            Some(desk_model::FixedGuide { page, vertical, at })
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// What follows a list label, from `text:label-followed-by`.
fn follow(value: Option<&str>) -> LabelFollow {
    match value {
        Some("space") => LabelFollow::Space,
        Some("nothing") => LabelFollow::Nothing,
        _ => LabelFollow::Tab,
    }
}

/// A line of the `AuthorColors` item of `settings.xml`: a JSON object
/// with `author` and `color`, the name of one of the ten colours (W150).
fn author_color(line: &str) -> Option<(String, desk_model::AuthorColor)> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let author = value.get("author")?.as_str()?;
    let color = desk_model::AuthorColor::from_key(value.get("color")?.as_str()?)?;
    (!author.is_empty()).then(|| (author.to_owned(), color))
}
