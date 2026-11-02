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

use crate::error::ModelError;
use crate::format::{CharAttrs, CharFormat, ParaAttrs, ParaFormat};
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
    /// The function key that applies the style, from 1 for F1 to 8 for F8
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
    /// The function key that applies the style, from 1 for F1 to 8 for F8
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
    /// A paragraph style.
    Paragraph(ParagraphStyle),
    /// A character style.
    Character(CharacterStyle),
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

    /// The function key that applies the style, from 1 to 8.
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
        Self::Character(style)
    }
}

/// The styles of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Styles {
    styles: OrdMap<StyleId, Style>,
    default_paragraph: StyleId,
}

impl Styles {
    /// Makes a set of styles with only the default paragraph style.
    pub(crate) fn new(default_paragraph: ParagraphStyle) -> Self {
        let id = default_paragraph.id;
        let mut styles = OrdMap::new();
        styles.insert(id, Style::Paragraph(default_paragraph));
        Self {
            styles,
            default_paragraph: id,
        }
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

    /// The style that a function key applies, from 1 for F1 to 8 for F8.
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
        format
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
        format
    }
}
