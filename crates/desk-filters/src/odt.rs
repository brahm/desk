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
    Alignment, Baseline, Capitalization, CharAttrs, CharacterStyle, Document, LINE_BREAK, Language,
    Length, LineColor, LineSpacing, LineStyle, Metadata, ModelError, Op, PageSetup, ParaAttrs,
    Paragraph, ParagraphStyle, Rgba, Run, Style, StyleId,
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

/// The character attributes of ODF text properties. Font sizes in
/// percent of the parent style are not kept.
fn char_attrs(t: &odf::TextProperties) -> CharAttrs {
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
    CharAttrs {
        font_family: t.font_family.as_deref().map(Into::into),
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
    }
}

fn text_properties(a: &CharAttrs) -> odf::TextProperties {
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
    odf::TextProperties {
        font_family: a.font_family.as_deref().map(str::to_owned),
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
    }
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
        ..odf::ParagraphProperties::default()
    };
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
    default_style.character = char_attrs(&defaults.text);
    if let Some(standard) = standard {
        default_style.name = names[&default].as_str().into();
        default_style
            .paragraph
            .merge(&para_attrs(&standard.properties.paragraph));
        default_style
            .character
            .merge(&char_attrs(&standard.properties.text));
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
                    character: char_attrs(&style.properties.text),
                }
                .into(),
                odf::Family::Text => CharacterStyle {
                    id,
                    name: names[&id].as_str().into(),
                    parent: link(&style.parent, odf::Family::Text),
                    character: char_attrs(&style.properties.text),
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
    document.put_style(default_style)?;

    // The paragraphs, after the empty one of a new document, which goes.
    let first = document
        .body()
        .get(0)
        .expect("a new document has a paragraph")
        .id();
    for (index, paragraph) in source.body.iter().enumerate() {
        let built = import_paragraph(source, &mut document, &ids, default, paragraph)?;
        document.apply(Op::InsertParagraph {
            index: index + 1,
            paragraph: built,
        })?;
    }
    if !source.body.is_empty() {
        document.apply(Op::RemoveParagraph { paragraph: first })?;
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
) -> Result<Paragraph, FilterError> {
    // A paragraph style that is automatic holds direct formatting over a
    // common style, its parent.
    let mut style = default;
    let mut attrs = ParaAttrs::default();
    let mut base = CharAttrs::default();
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
            base = char_attrs(&automatic.properties.text);
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
                run_attrs.merge(&char_attrs(&automatic.properties.text));
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
                    paragraph: paragraph_properties(&p.paragraph),
                    text: text_properties(&p.character),
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
                    text: text_properties(&c.character),
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
                    paragraph: paragraph_properties(paragraph.attrs()),
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
                            text: text_properties(&run.attrs),
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
