// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pages '09 and '08 documents to the model, through `desk-iwork`
//! (DOC-22).
//!
//! Named styles become styles of the model, and anonymous styles, which
//! is how Pages saves direct formatting, become attributes of paragraphs
//! and runs. Pages keeps the properties that a style does not set in its
//! own defaults, which are the defaults of the model except for widow
//! control, on in Pages; styles without a parent get every property set,
//! so that they do not inherit from Body as styles of the model would.

use std::collections::{HashMap, HashSet};

use desk_iwork::pages::{self as iwork, PAGE_BREAK};
use desk_iwork::sf::{Family, LineSpacingMode, Properties, Style as IworkStyle, Stylesheet, Value};
use desk_model::{
    Alignment, Baseline, Capitalization, CharAttrs, CharFormat, CharacterStyle, Document,
    LINE_BREAK, Language, Length, LineColor, LineSpacing, LineStyle, Metadata, Op, PageSetup,
    ParaAttrs, ParaFormat, Paragraph, ParagraphStyle, Rgba, Run, StyleId,
};

use crate::FilterError;

/// The identifier of Body in the templates of Pages.
const BODY_IDENT: &str = "paragraph-style-32";
/// The name of Body.
const BODY_NAME: &str = "Body";
/// The root character style of Pages, which sets nothing.
const NULL_CHARACTER: &str = "character-style-null";

/// Reads a Pages document from the bytes of a flat file or of its index.
pub fn read_pages(bytes: &[u8]) -> Result<Document, FilterError> {
    import(&desk_iwork::read_pages(bytes)?)
}

/// Reads a Pages document on disk, a package folder or a flat file.
pub fn read_pages_at(path: &std::path::Path) -> Result<Document, FilterError> {
    import(&desk_iwork::read_pages_at(path)?)
}

/// The defaults of Pages for what no style sets.
fn pages_paragraph_defaults() -> ParaFormat {
    ParaFormat {
        widow_control: true,
        ..ParaFormat::default()
    }
}

/// The family, weight and slant of a font from its PostScript name, which
/// is how Pages names fonts: `LiberationSerif-BoldItalic` is Liberation
/// Serif, bold and italic.
pub fn font_face(postscript: &str) -> (String, bool, bool) {
    let (base, style) = postscript.split_once('-').unwrap_or((postscript, ""));
    let base = base
        .strip_suffix("PSMT")
        .or_else(|| base.strip_suffix("MT"))
        .or_else(|| base.strip_suffix("PS"))
        .unwrap_or(base);
    let style = style.to_ascii_lowercase();
    let bold = ["bold", "black", "heavy"].iter().any(|w| style.contains(w));
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
    (family, bold, italic)
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
        bold: flag(properties, "bold"),
        italic: flag(properties, "italic"),
        underline: line(properties, "underline"),
        strikethrough: line(properties, "strikethru"),
        baseline_shift: points(properties, "baselineShift"),
        ..CharAttrs::default()
    };
    if let Some(name) = properties.string("fontName").filter(|n| !n.is_empty()) {
        let (family, bold, italic) = font_face(name);
        attrs.font_family = Some(family.as_str().into());
        if bold {
            attrs.bold = Some(true);
        }
        if italic {
            attrs.italic = Some(true);
        }
    }
    attrs.color = match properties.get("fontColor") {
        Some(Value::Color(color)) => Some(rgba(*color)),
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
    attrs
}

/// The paragraph attributes that a property map sets, without the first
/// line indent, which depends on the left indent.
fn para_attrs(properties: &Properties) -> ParaAttrs {
    ParaAttrs {
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
        keep_with_next: flag(properties, "keepWithNext"),
        keep_lines_together: flag(properties, "keepLinesTogether"),
        widow_control: flag(properties, "widowControl"),
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
    para.first_line_indent = first_line_indent(chain, count);
    (para, character)
}

/// Whether a style is one of the named styles that the model gets.
fn is_named(style: &IworkStyle) -> bool {
    style.name.is_some() && style.ident.is_some() && style.ident.as_deref() != Some(NULL_CHARACTER)
}

struct Importer<'a> {
    sheet: &'a Stylesheet,
    document: Document,
    /// The model style of each named style, by identifier.
    ids: HashMap<(Family, String), StyleId>,
    body: StyleId,
}

impl<'a> Importer<'a> {
    fn new(sheet: &'a Stylesheet) -> Result<Self, FilterError> {
        let document = Document::new_word_processing();
        let body = document.styles().default_paragraph();
        let mut importer = Self {
            sheet,
            document,
            ids: HashMap::new(),
            body,
        };
        importer.styles()?;
        Ok(importer)
    }

    fn styles(&mut self) -> Result<(), FilterError> {
        let sheet = self.sheet;
        let named: Vec<&IworkStyle> = sheet.styles.iter().filter(|s| is_named(s)).collect();
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
                            parent,
                            next: None,
                            paragraph,
                            character,
                        })?;
                    }
                    Family::Character => {
                        self.document.put_style(CharacterStyle {
                            id: *id,
                            name: name.as_str().into(),
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
        Ok(())
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
    fn paragraph(&mut self, source: &iwork::Paragraph) -> Result<Vec<Paragraph>, FilterError> {
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
        for (i, runs) in pieces.into_iter().enumerate() {
            let mut attrs = attrs.clone();
            if i > 0 {
                attrs.page_break_before = Some(true);
            }
            let id = self.document.new_id();
            out.push(Paragraph::from_runs(id, style, attrs, runs)?);
        }
        Ok(out)
    }
}

/// Maps a Pages document to the model.
pub fn import(source: &iwork::PagesDocument) -> Result<Document, FilterError> {
    let mut importer = Importer::new(&source.stylesheet)?;
    let first = importer
        .document
        .body()
        .get(0)
        .expect("a new document has a paragraph")
        .id();
    let mut index = 1;
    for paragraph in &source.body {
        for built in importer.paragraph(paragraph)? {
            importer.document.apply(Op::InsertParagraph {
                index,
                paragraph: built,
            })?;
            index += 1;
        }
    }
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
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_faces_come_from_postscript_names() {
        assert_eq!(
            font_face("LiberationSerif"),
            ("Liberation Serif".into(), false, false)
        );
        assert_eq!(
            font_face("LiberationSans-BoldItalic"),
            ("Liberation Sans".into(), true, true)
        );
        assert_eq!(
            font_face("TimesNewRomanPSMT"),
            ("Times New Roman".into(), false, false)
        );
        assert_eq!(
            font_face("Helvetica-Oblique"),
            ("Helvetica".into(), false, true)
        );
        assert_eq!(font_face("ArialMT"), ("Arial".into(), false, false));
        assert_eq!(
            font_face("Bodoni72-Book"),
            ("Bodoni 72".into(), false, false)
        );
        assert_eq!(
            font_face("HelveticaNeue-Bold"),
            ("Helvetica Neue".into(), true, false)
        );
    }
}
