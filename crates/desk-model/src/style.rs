// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Named styles and their inheritance.
//!
//! Styles inherit from a parent of the same family, as in ODF. Resolving
//! the formatting of a run of text goes from the built-in defaults through
//! the chain of the paragraph style, then the chain of the character style,
//! and ends with the attributes set on the run itself.

use std::sync::Arc;

use imbl::OrdMap;

use crate::Length;
use crate::error::ModelError;
use crate::format::{CharAttrs, CharFormat, LineSpacing, ParaAttrs, ParaFormat, TabStops};
use crate::id::NodeId;
use crate::list::ListStyle;

/// The identifier of a style. Styles are nodes, so a style keeps its
/// identifier when it is renamed.
pub type StyleId = NodeId;

/// The kind of thing a style applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StyleFamily {
    /// Paragraph styles.
    Paragraph,
    /// Character styles.
    Character,
    /// List styles, which set the bullets, numbers and indents of lists.
    List,
}

/// A paragraph style. It also sets character attributes for the text of
/// the paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParagraphStyle {
    /// The identifier.
    pub id: StyleId,
    /// The name shown to the user.
    pub name: Arc<str>,
    /// The style it inherits from.
    pub parent: Option<StyleId>,
    /// The style of the paragraph that Return starts at the end of a
    /// paragraph with this style. `None` keeps the same style.
    pub next: Option<StyleId>,
    /// The shortcut that applies the style, from 1 for Ctrl+1 to 8 for Ctrl+8
    /// (STY-04).
    pub shortcut: Option<u8>,
    /// The paragraph attributes.
    pub paragraph: ParaAttrs,
    /// The character attributes for the text of the paragraph.
    pub character: CharAttrs,
}

impl ParagraphStyle {
    /// Makes a paragraph style that sets nothing.
    pub fn new(id: StyleId, name: &str) -> Self {
        Self {
            id,
            name: name.into(),
            parent: None,
            next: None,
            shortcut: None,
            paragraph: ParaAttrs::default(),
            character: CharAttrs::default(),
        }
    }
}

/// A character style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterStyle {
    /// The identifier.
    pub id: StyleId,
    /// The name shown to the user.
    pub name: Arc<str>,
    /// The style it inherits from.
    pub parent: Option<StyleId>,
    /// The shortcut that applies the style, from 1 for Ctrl+1 to 8 for Ctrl+8
    /// (STY-04).
    pub shortcut: Option<u8>,
    /// The character attributes.
    pub character: CharAttrs,
}

impl CharacterStyle {
    /// Makes a character style that sets nothing.
    pub fn new(id: StyleId, name: &str) -> Self {
        Self {
            id,
            name: name.into(),
            parent: None,
            shortcut: None,
            character: CharAttrs::default(),
        }
    }
}

/// A style of any family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Style {
    /// A paragraph style, boxed, as styles of text are much larger than
    /// list styles.
    Paragraph(Box<ParagraphStyle>),
    /// A character style, boxed too.
    Character(Box<CharacterStyle>),
    /// A list style.
    List(ListStyle),
}

impl Style {
    /// The identifier.
    pub fn id(&self) -> StyleId {
        match self {
            Self::Paragraph(style) => style.id,
            Self::Character(style) => style.id,
            Self::List(style) => style.id,
        }
    }

    /// The name shown to the user.
    pub fn name(&self) -> &str {
        match self {
            Self::Paragraph(style) => &style.name,
            Self::Character(style) => &style.name,
            Self::List(style) => &style.name,
        }
    }

    /// The style it inherits from.
    pub fn parent(&self) -> Option<StyleId> {
        match self {
            Self::Paragraph(style) => style.parent,
            Self::Character(style) => style.parent,
            Self::List(_) => None,
        }
    }

    /// The shortcut that applies the style, Ctrl and a digit from 1 to 8.
    pub fn shortcut(&self) -> Option<u8> {
        match self {
            Self::Paragraph(style) => style.shortcut,
            Self::Character(style) => style.shortcut,
            Self::List(style) => style.shortcut,
        }
    }

    /// The same style with another name.
    pub fn with_name(mut self, name: &str) -> Self {
        match &mut self {
            Self::Paragraph(style) => style.name = name.into(),
            Self::Character(style) => style.name = name.into(),
            Self::List(style) => style.name = name.into(),
        }
        self
    }

    /// The same style with another shortcut.
    pub fn with_shortcut(mut self, key: Option<u8>) -> Self {
        match &mut self {
            Self::Paragraph(style) => style.shortcut = key,
            Self::Character(style) => style.shortcut = key,
            Self::List(style) => style.shortcut = key,
        }
        self
    }

    /// The family.
    pub fn family(&self) -> StyleFamily {
        match self {
            Self::Paragraph(_) => StyleFamily::Paragraph,
            Self::Character(_) => StyleFamily::Character,
            Self::List(_) => StyleFamily::List,
        }
    }
}

impl From<ParagraphStyle> for Style {
    fn from(style: ParagraphStyle) -> Self {
        Self::Paragraph(Box::new(style))
    }
}

impl From<Box<ParagraphStyle>> for Style {
    fn from(style: Box<ParagraphStyle>) -> Self {
        Self::Paragraph(style)
    }
}

impl From<ListStyle> for Style {
    fn from(style: ListStyle) -> Self {
        Self::List(style)
    }
}

impl From<CharacterStyle> for Style {
    fn from(style: CharacterStyle) -> Self {
        Self::Character(Box::new(style))
    }
}

impl From<Box<CharacterStyle>> for Style {
    fn from(style: Box<CharacterStyle>) -> Self {
        Self::Character(style)
    }
}

/// The styles of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Styles {
    styles: OrdMap<StyleId, Style>,
    default_paragraph: StyleId,
    /// The Text Size of Screen View, in thousandths, while the styles
    /// resolve formatting as it shows (PXV-01); never saved.
    screen: Option<u32>,
}

impl Styles {
    /// Makes a set of styles with only the default paragraph style.
    pub(crate) fn new(default_paragraph: ParagraphStyle) -> Self {
        let id = default_paragraph.id;
        let mut styles = OrdMap::new();
        styles.insert(id, Style::Paragraph(Box::new(default_paragraph)));
        Self {
            styles,
            default_paragraph: id,
            screen: None,
        }
    }

    /// Resolves formatting as Screen View shows it at a Text Size of
    /// `zoom`, 1 for 100%, or as the pages do: see [`screen_font_size`].
    pub fn set_screen(&mut self, zoom: Option<f64>) {
        self.screen = zoom.map(|z| (z * 1000.0).round().max(1.0) as u32);
    }

    /// The paragraph style of new paragraphs, which cannot be removed.
    pub fn default_paragraph(&self) -> StyleId {
        self.default_paragraph
    }

    /// A style by identifier.
    pub fn get(&self, id: StyleId) -> Option<&Style> {
        self.styles.get(&id)
    }

    /// A paragraph style by identifier.
    pub fn paragraph(&self, id: StyleId) -> Option<&ParagraphStyle> {
        match self.get(id) {
            Some(Style::Paragraph(style)) => Some(style),
            _ => None,
        }
    }

    /// A character style by identifier.
    pub fn character(&self, id: StyleId) -> Option<&CharacterStyle> {
        match self.get(id) {
            Some(Style::Character(style)) => Some(style),
            _ => None,
        }
    }

    /// A list style by identifier.
    pub fn list(&self, id: StyleId) -> Option<&ListStyle> {
        match self.get(id) {
            Some(Style::List(style)) => Some(style),
            _ => None,
        }
    }

    /// The first style with this name in a family.
    pub fn by_name(&self, family: StyleFamily, name: &str) -> Option<&Style> {
        self.iter()
            .find(|style| style.family() == family && style.name() == name)
    }

    /// The style that a shortcut applies, from 1 for Ctrl+1 to 8 for Ctrl+8.
    pub fn by_shortcut(&self, key: u8) -> Option<&Style> {
        self.iter().find(|style| style.shortcut() == Some(key))
    }

    /// A name that no style of the family has: the name itself, or the
    /// name followed by the first free number from 2, as "Body 2".
    pub fn unique_name(&self, family: StyleFamily, name: &str) -> String {
        if self.by_name(family, name).is_none() {
            return name.to_owned();
        }
        (2..)
            .map(|n| format!("{name} {n}"))
            .find(|candidate| self.by_name(family, candidate).is_none())
            .expect("a free number")
    }

    /// All styles, in the order of their identifiers.
    pub fn iter(&self) -> impl Iterator<Item = &Style> {
        self.styles.values()
    }

    /// The number of styles.
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    /// Whether there are no styles. A document always has its default
    /// paragraph style, so this is false for the styles of a document.
    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }

    /// Checks that an identifier names a style of the family.
    pub(crate) fn check_family(&self, id: StyleId, family: StyleFamily) -> Result<(), ModelError> {
        match self.get(id) {
            None => Err(ModelError::UnknownStyle(id)),
            Some(style) if style.family() != family => Err(ModelError::WrongStyleFamily {
                style: id,
                expected: family,
            }),
            Some(_) => Ok(()),
        }
    }

    /// Checks that a style can be added, or replace the style with the
    /// same identifier.
    pub(crate) fn check_put(&self, style: &Style) -> Result<(), ModelError> {
        let id = style.id();
        if let Some(old) = self.get(id)
            && old.family() != style.family()
        {
            return Err(ModelError::WrongStyleFamily {
                style: id,
                expected: old.family(),
            });
        }
        if let Some(key) = style.shortcut()
            && !(1..=8).contains(&key)
        {
            return Err(ModelError::InvalidShortcut(key));
        }
        if let Style::Paragraph(paragraph) = style
            && let Some(next) = paragraph.next
            && next != id
        {
            self.check_family(next, StyleFamily::Paragraph)?;
        }
        if let Style::Paragraph(paragraph) = style
            && let Some(Some(list)) = paragraph.paragraph.list_style
        {
            self.check_family(list, StyleFamily::List)?;
        }
        // Walk up from the new parent: reaching the style itself means the
        // change would make a cycle.
        let mut parent = style.parent();
        let mut steps = 0;
        while let Some(current) = parent {
            if current == id {
                return Err(ModelError::StyleCycle(id));
            }
            self.check_family(current, style.family())?;
            steps += 1;
            if steps > self.styles.len() {
                return Err(ModelError::StyleCycle(id));
            }
            parent = self.get(current).and_then(Style::parent);
        }
        Ok(())
    }

    /// Whether a style is the parent or the next style of another style.
    pub(crate) fn is_referenced(&self, id: StyleId) -> bool {
        self.iter().any(|style| {
            style.id() != id
                && (style.parent() == Some(id)
                    || matches!(style, Style::Paragraph(p)
                        if p.next == Some(id) || p.paragraph.list_style == Some(Some(id))))
        })
    }

    pub(crate) fn insert(&mut self, style: Style) -> Option<Style> {
        self.styles.insert(style.id(), style)
    }

    pub(crate) fn remove(&mut self, id: StyleId) -> Option<Style> {
        self.styles.remove(&id)
    }

    /// The chain of a style, from the style itself up to its root.
    fn chain(&self, id: StyleId) -> impl Iterator<Item = &Style> {
        let mut current = self.get(id);
        let mut steps = 0;
        std::iter::from_fn(move || {
            let style = current?;
            steps += 1;
            // Cycles are refused when styles change; the bound only keeps a
            // bug from looping forever.
            current = if steps > self.styles.len() {
                None
            } else {
                style.parent().and_then(|parent| self.get(parent))
            };
            Some(style)
        })
    }

    /// Resolves the paragraph formatting of a paragraph with a style and
    /// its own attributes.
    pub fn resolve_paragraph(&self, style: StyleId, direct: &ParaAttrs) -> ParaFormat {
        let mut format = ParaFormat::default();
        let chain: Vec<_> = self.chain(style).collect();
        for style in chain.iter().rev() {
            if let Style::Paragraph(style) = style {
                style.paragraph.apply_to(&mut format);
            }
        }
        direct.apply_to(&mut format);
        // Screen View sets no left indent, and a first line indent of its
        // own, 36 points on the screen whatever the Text Size (the Mac,
        // step 19g of W1).
        // A hanging first line, as of a list, keeps its indents: the Mac
        // showed only a first line indented to the right.
        if let Some(zoom) = self.screen
            && format.first_line_indent >= Length::ZERO
        {
            format.left_indent = Length::ZERO;
            if format.first_line_indent > Length::ZERO {
                format.first_line_indent = Length::from_pt(36.0 * 1000.0 / f64::from(zoom));
            }
        }
        if let Some(zoom) = self.screen {
            let zoom = f64::from(zoom) / 1000.0;
            let size = self.paragraph_font_size(&chain);
            screen_spacing(&mut format, size, zoom);
        }
        format
    }

    /// The font size, in points, that the style chain of a paragraph sets,
    /// as it prints.
    fn paragraph_font_size(&self, chain: &[&Style]) -> f64 {
        let mut format = CharFormat::default();
        for style in chain.iter().rev() {
            if let Style::Paragraph(style) = style {
                style.character.apply_to(&mut format);
            }
        }
        format.font_size.to_pt()
    }

    /// Resolves the character formatting of a run of text, from the style
    /// of its paragraph, its character style and its own attributes.
    pub fn resolve_character(
        &self,
        paragraph_style: StyleId,
        character_style: Option<StyleId>,
        direct: &CharAttrs,
    ) -> CharFormat {
        let mut format = CharFormat::default();
        let paragraph: Vec<_> = self.chain(paragraph_style).collect();
        for style in paragraph.iter().rev() {
            if let Style::Paragraph(style) = style {
                style.character.apply_to(&mut format);
            }
        }
        if let Some(id) = character_style {
            let character: Vec<_> = self.chain(id).collect();
            for style in character.iter().rev() {
                if let Style::Character(style) = style {
                    style.character.apply_to(&mut format);
                }
            }
        }
        direct.apply_to(&mut format);
        if self.screen.is_some() {
            format.font_size = Length::from_pt(screen_font_size(format.font_size.to_pt()));
        }
        format
    }
}

/// The size, in points at a Text Size of 100%, at which Screen View of
/// Pages 15.4 shows text of a size: one curve for every font, with a floor
/// of 10 points and a ceiling of 26, measured on the owner's Mac for 23
/// sizes from 4 to 96 points within 0.1 point (step 19g of W1).
pub fn screen_font_size(size: f64) -> f64 {
    match size {
        s if s <= 8.0 => 10.0,
        s if s <= 11.0 => s + 2.0,
        s if s <= 24.0 => 13.0 + (s - 11.0) / 2.0,
        s if s <= 50.0 => 19.5 + (s - 24.0) / 4.0,
        _ => 26.0,
    }
}

/// The height of a single line of a font, in ems, that Screen View of
/// Pages 15.4 counts with: about 1.20 for Helvetica Neue and 1.15 to 1.19
/// for Times New Roman (step 19h of W1).
const SCREEN_LINE_EMS: f64 = 1.2;

/// The four line spacings Screen View of Pages 15.4 shows, as multiples of
/// single in percent, with the multiple from which each starts: whatever
/// the paragraph sets, it shows the level its multiple falls in (step 19h
/// of W1, 520 measurements).
const SCREEN_LINE_LEVELS: [(f64, u32); 4] = [(0.0, 100), (1.16, 114), (1.36, 135), (1.75, 179)];

/// The share of a printed line that makes one step of space before or
/// after a paragraph in Screen View, the most steps, and the size of a
/// step as a share of the line on the screen (step 19h of W1, 263 gaps;
/// the size of a step varied from 0.83 to 0.92 of a quarter line).
const SCREEN_SPACE_STEP: f64 = 0.2425;
const SCREEN_SPACE_STEPS: f64 = 4.0;
const SCREEN_SPACE_SHARE: f64 = 0.89 / 4.0;

/// The distance between the tab stops of Screen View, in points on the
/// screen, whatever the Text Size, the font size or the stops that the
/// paragraph sets (step 19h of W1).
const SCREEN_TAB: f64 = 36.0;

/// Sets the line spacing, the space before and after and the tabs of a
/// paragraph whose text prints at `size` points as Screen View of Pages
/// 15.4 shows them at a Text Size of `zoom`.
fn screen_spacing(format: &mut ParaFormat, size: f64, zoom: f64) {
    let single = SCREEN_LINE_EMS * size;
    let screen_single = SCREEN_LINE_EMS * screen_font_size(size);
    // The printed distance between baselines, which the space before and
    // after count in.
    let printed = match format.line_spacing {
        LineSpacing::Multiple { percent } => single * f64::from(percent) / 100.0,
        LineSpacing::AtLeast(at_least) => at_least.to_pt().max(single),
        LineSpacing::Exactly(exactly) => exactly.to_pt(),
        LineSpacing::Between(between) => single + between.to_pt(),
    };
    // A distance is compared with the line on the screen, in points on
    // the screen, so a fixed one falls a level as the Text Size grows;
    // "Between" counts as the whole line there.
    let multiple = match format.line_spacing {
        LineSpacing::Multiple { percent } => f64::from(percent) / 100.0,
        LineSpacing::AtLeast(length)
        | LineSpacing::Exactly(length)
        | LineSpacing::Between(length) => length.to_pt() / (screen_single * zoom),
    };
    let percent = SCREEN_LINE_LEVELS
        .iter()
        .rev()
        .find(|(from, _)| multiple >= *from)
        .map_or(100, |(_, percent)| *percent);
    format.line_spacing = LineSpacing::Multiple { percent };
    let line = screen_single * f64::from(percent) / 100.0;
    let space = |value: Length| {
        if printed <= 0.0 {
            return Length::ZERO;
        }
        let steps = (value.to_pt() / (SCREEN_SPACE_STEP * printed))
            .round()
            .clamp(0.0, SCREEN_SPACE_STEPS);
        Length::from_pt(steps * SCREEN_SPACE_SHARE * line)
    };
    format.space_before = space(format.space_before);
    format.space_after = space(format.space_after);
    format.tab_stops = TabStops::none();
    format.default_tab_interval = Length::from_pt(SCREEN_TAB / zoom);
}
