// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Working with styles: making them from the selection, redefining and
//! reverting, renaming and deleting, shortcuts, copying and pasting the
//! style of text, finding where a style is used and importing styles from
//! another document (STY-02 to STY-10).

use std::collections::HashMap;
use std::ops::Range;

use crate::document::Document;
use crate::error::ModelError;
use crate::format::{CharAttrs, CharFormat, ParaAttrs};
use crate::id::NodeId;
use crate::op::Op;
use crate::style::{CharacterStyle, ParagraphStyle, Style, StyleFamily, StyleId};
use crate::text::Position;
use crate::transaction::Edit;

/// The style of some text, as Copy Style takes it, to paste over other
/// text of the same or of another document (STY-08).
///
/// It holds the definitions of the styles as well as their names, so that
/// pasting into a document without them adds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopiedStyle {
    /// The paragraph style, with every attribute it resolves to and no
    /// parent.
    pub paragraph: ParagraphStyle,
    /// The attributes set on the paragraph itself.
    pub paragraph_attrs: ParaAttrs,
    /// The character style, with every attribute it resolves to and no
    /// parent.
    pub character: Option<CharacterStyle>,
    /// The attributes set on the text itself.
    pub character_attrs: CharAttrs,
}

/// Where the formatting of some text differs from its styles: the red
/// arrows of the Styles drawer (STY-06).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Overrides {
    /// Whether the paragraph or text without a character style sets
    /// attributes over the paragraph style.
    pub paragraph: bool,
    /// Whether text with a character style sets attributes over it.
    pub character: bool,
}

impl Document {
    /// A paragraph style with the attributes that its chain resolves to,
    /// and no parent: what a style looks like on its own.
    fn flat_paragraph_style(&self, id: StyleId) -> Option<ParagraphStyle> {
        let styles = self.styles();
        let style = styles.paragraph(id)?;
        Some(ParagraphStyle {
            parent: None,
            next: None,
            paragraph: styles.resolve_paragraph(id, &ParaAttrs::default()).into(),
            character: styles
                .resolve_character(id, None, &CharAttrs::default())
                .into(),
            ..style.clone()
        })
    }

    /// A character style with the attributes that its chain sets, and no
    /// parent.
    fn flat_character_style(&self, id: StyleId) -> Option<CharacterStyle> {
        let styles = self.styles();
        let style = styles.character(id)?;
        let mut character = CharAttrs::default();
        let mut chain = vec![style];
        while let Some(parent) = chain.last().and_then(|s| s.parent)
            && let Some(parent) = styles.character(parent)
            && chain.len() <= styles.len()
        {
            chain.push(parent);
        }
        for style in chain.iter().rev() {
            character.merge(&style.character);
        }
        Some(CharacterStyle {
            parent: None,
            character,
            ..style.clone()
        })
    }

    /// The character style and attributes of the character after a
    /// position, or before it at the end of a paragraph.
    fn run_at(&self, at: Position) -> Result<(Option<StyleId>, CharAttrs), ModelError> {
        let paragraph = self.paragraph(at.paragraph)?;
        let offset = if at.offset < paragraph.len() {
            // The character after the position.
            at.offset + 1
        } else {
            at.offset
        };
        Ok(paragraph.format_at(offset))
    }

    /// The character formatting at a position, resolved, and the
    /// formatting that the paragraph style alone gives there: a new
    /// character style takes the attributes where they differ (STY-03).
    pub fn character_formats_at(
        &self,
        at: Position,
    ) -> Result<(CharFormat, CharFormat), ModelError> {
        let paragraph = self.paragraph(at.paragraph)?;
        let (style, attrs) = self.run_at(at)?;
        let styles = self.styles();
        Ok((
            styles.resolve_character(paragraph.style(), style, &attrs),
            styles.resolve_character(paragraph.style(), None, &CharAttrs::default()),
        ))
    }

    /// Takes the style of the text at a position, for Paste Style.
    pub fn copy_style(&self, at: Position) -> Result<CopiedStyle, ModelError> {
        let paragraph = self.paragraph(at.paragraph)?;
        let (style, attrs) = self.run_at(at)?;
        Ok(CopiedStyle {
            paragraph: self
                .flat_paragraph_style(paragraph.style())
                .ok_or(ModelError::UnknownStyle(paragraph.style()))?,
            paragraph_attrs: paragraph.attrs().clone(),
            character: style.and_then(|id| self.flat_character_style(id)),
            character_attrs: attrs,
        })
    }

    /// The text that uses a style: whole paragraphs for a paragraph style,
    /// runs for a character style, in the order of the body (STY-09).
    pub fn style_uses(&self, id: StyleId) -> Vec<(NodeId, Range<usize>)> {
        let mut uses: Vec<(NodeId, Range<usize>)> = Vec::new();
        for paragraph in self.body().iter() {
            if paragraph.style() == id {
                uses.push((paragraph.id(), 0..paragraph.len()));
                continue;
            }
            let mut start = 0;
            for run in paragraph.runs() {
                let end = start + run.len();
                if run.style == Some(id) {
                    match uses.last_mut() {
                        Some((p, range)) if *p == paragraph.id() && range.end == start => {
                            range.end = end;
                        }
                        _ => uses.push((paragraph.id(), start..end)),
                    }
                }
                start = end;
            }
        }
        uses
    }

    /// Where the formatting between two positions differs from its
    /// styles. A selection that is only a cursor looks at the character
    /// after it.
    pub fn overrides(&self, start: Position, end: Position) -> Result<Overrides, ModelError> {
        let mut overrides = Overrides::default();
        let ranges = self.paragraph_ranges(start, end)?;
        for (id, range) in ranges {
            let paragraph = self.paragraph(id)?;
            overrides.paragraph |= !paragraph.attrs().is_empty();
            let mut look = |style: Option<StyleId>, attrs: &CharAttrs| {
                if !attrs.is_empty() {
                    if style.is_some() {
                        overrides.character = true;
                    } else {
                        overrides.paragraph = true;
                    }
                }
            };
            if range.is_empty() {
                let (style, attrs) = self.run_at(Position::new(id, range.start))?;
                look(style, &attrs);
                continue;
            }
            let mut offset = 0;
            for run in paragraph.runs() {
                let run_end = offset + run.len();
                if offset < range.end && run_end > range.start {
                    look(run.style, &run.attrs);
                }
                offset = run_end;
            }
        }
        Ok(overrides)
    }
}

impl Edit {
    /// The ranges of the paragraphs between two positions. A cursor takes
    /// its whole paragraph.
    fn ranges_or_paragraph(
        &self,
        start: Position,
        end: Position,
    ) -> Result<Vec<(NodeId, Range<usize>)>, ModelError> {
        let mut ranges = self.document().paragraph_ranges(start, end)?;
        if start == end
            && let Some((id, range)) = ranges.first_mut()
        {
            *range = 0..self.document().paragraph(*id)?.len();
        }
        Ok(ranges)
    }

    /// Changes the runs of each range.
    fn map_ranges(
        &mut self,
        ranges: &[(NodeId, Range<usize>)],
        change: impl Fn(&mut crate::text::Run),
    ) -> Result<(), ModelError> {
        for (id, range) in ranges {
            if range.is_empty() {
                continue;
            }
            self.map_runs(
                Position::new(*id, range.start),
                Position::new(*id, range.end),
                &change,
            )?;
        }
        Ok(())
    }

    /// Sets the attributes of a paragraph itself, keeping its style.
    fn set_own_paragraph_attrs(&mut self, id: NodeId, attrs: ParaAttrs) -> Result<(), ModelError> {
        let paragraph = self.document().paragraph(id)?;
        if paragraph.attrs() != &attrs {
            let style = paragraph.style();
            self.apply(Op::SetParagraphFormat {
                paragraph: id,
                style,
                attrs,
            })?;
        }
        Ok(())
    }

    fn put(&mut self, style: impl Into<Style>) -> Result<(), ModelError> {
        let style = style.into();
        if self.document().styles().get(style.id()).as_ref() != Some(&&style) {
            self.apply(Op::PutStyle { style })?;
        }
        Ok(())
    }

    fn check_name(
        &self,
        family: StyleFamily,
        name: &str,
        id: Option<StyleId>,
    ) -> Result<(), ModelError> {
        match self.document().styles().by_name(family, name) {
            Some(other) if Some(other.id()) != id => {
                Err(ModelError::DuplicateStyleName(name.to_owned()))
            }
            _ => Ok(()),
        }
    }

    /// Makes a paragraph style from the paragraph at a position, with
    /// every attribute that its text there has and no parent, as Pages
    /// starts new styles from Free Form (STY-03). When `apply` is true,
    /// the paragraphs between the positions take it and lose the
    /// attributes it now holds.
    pub fn new_paragraph_style(
        &mut self,
        name: &str,
        start: Position,
        end: Position,
        apply: bool,
    ) -> Result<StyleId, ModelError> {
        self.check_name(StyleFamily::Paragraph, name, None)?;
        let document = self.document();
        let paragraph = document.paragraph(start.paragraph)?;
        let styles = document.styles();
        let (run_style, run_attrs) = document.run_at(start)?;
        // The look of a character style stays with the character style.
        let own = if run_style.is_some() {
            CharAttrs::default()
        } else {
            run_attrs
        };
        let character = styles.resolve_character(paragraph.style(), None, &own);
        let format = styles.resolve_paragraph(paragraph.style(), paragraph.attrs());
        let id = self.new_id();
        let mut style = ParagraphStyle::new(id, name);
        style.paragraph = format.into();
        style.character = character.into();
        self.put(style)?;
        if apply {
            self.set_paragraph_style(start, end, id)?;
            // Whole paragraphs take the style, so all their text loses
            // what it repeats.
            let last = self.document().paragraph(end.paragraph)?.len();
            self.drop_redundant(
                Position::new(start.paragraph, 0),
                Position::new(end.paragraph, last),
            )?;
        }
        Ok(id)
    }

    /// Makes a character style with some attributes (STY-03). When
    /// `apply` is true, the text between the positions takes it and loses
    /// those attributes of its own.
    pub fn new_character_style(
        &mut self,
        name: &str,
        attrs: CharAttrs,
        start: Position,
        end: Position,
        apply: bool,
    ) -> Result<StyleId, ModelError> {
        self.check_name(StyleFamily::Character, name, None)?;
        let id = self.new_id();
        let mut style = CharacterStyle::new(id, name);
        style.character = attrs.clone();
        self.put(style)?;
        if apply && start != end {
            self.set_character_style(start, end, Some(id))?;
            self.drop_redundant(start, end)?;
        }
        Ok(id)
    }

    /// Removes the attributes set on the paragraphs and the text between
    /// two positions that only repeat what their styles give.
    fn drop_redundant(&mut self, start: Position, end: Position) -> Result<(), ModelError> {
        let ranges = self.ranges_or_paragraph(start, end)?;
        for (id, range) in ranges {
            let document = self.document().clone();
            let styles = document.styles();
            let paragraph = document.paragraph(id)?;
            let style = paragraph.style();
            let base = styles.resolve_paragraph(style, &ParaAttrs::default());
            self.set_own_paragraph_attrs(id, paragraph.attrs().difference(&base))?;
            if !range.is_empty() {
                self.map_runs(
                    Position::new(id, range.start),
                    Position::new(id, range.end),
                    |run| {
                        let base =
                            styles.resolve_character(style, run.style, &CharAttrs::default());
                        run.attrs = run.attrs.difference(&base);
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Gives the paragraphs between two positions a paragraph style and
    /// removes every attribute set over their styles, as a style shortcut
    /// with Option does (STY-04). Character styles stay.
    pub fn set_paragraph_style_clean(
        &mut self,
        start: Position,
        end: Position,
        style: StyleId,
    ) -> Result<(), ModelError> {
        self.set_paragraph_style(start, end, style)?;
        self.revert_to_style(start, end)
    }

    /// Reverts the text between two positions to its styles: Revert to
    /// Defined Style (STY-06). The paragraphs lose the attributes set on
    /// them, and their text, all of it for a cursor, loses the attributes
    /// set on it. Character styles stay.
    pub fn revert_to_style(&mut self, start: Position, end: Position) -> Result<(), ModelError> {
        let ranges = self.ranges_or_paragraph(start, end)?;
        for (id, _) in &ranges {
            self.set_own_paragraph_attrs(*id, ParaAttrs::default())?;
        }
        self.map_ranges(&ranges, |run| run.attrs = CharAttrs::default())
    }

    /// Takes the attributes set over a style at a position into the style:
    /// Redefine Style from Selection (STY-06). For a paragraph style, the
    /// attributes of the paragraph and of its text without a character
    /// style; the paragraphs between the positions with that style then
    /// lose them, while character styles stay. For a character style, the
    /// attributes of the text there; the text between the positions with
    /// that style loses them.
    pub fn redefine_style(
        &mut self,
        id: StyleId,
        start: Position,
        end: Position,
    ) -> Result<(), ModelError> {
        let document = self.document();
        let (run_style, run_attrs) = document.run_at(start)?;
        match document.styles().get(id).cloned() {
            None => Err(ModelError::UnknownStyle(id)),
            Some(Style::Paragraph(mut style)) => {
                let paragraph = document.paragraph(start.paragraph)?;
                let para_attrs = paragraph.attrs().clone();
                let char_attrs = if run_style.is_none() {
                    run_attrs
                } else {
                    CharAttrs::default()
                };
                style.paragraph.merge(&para_attrs);
                style.character.merge(&char_attrs);
                self.put(style)?;
                let ranges = self.ranges_or_paragraph(start, end)?;
                for (paragraph, range) in &ranges {
                    let current = self.document().paragraph(*paragraph)?;
                    if current.style() != id {
                        continue;
                    }
                    let mut own = current.attrs().clone();
                    own.remove(&para_attrs);
                    self.set_own_paragraph_attrs(*paragraph, own)?;
                    self.map_ranges(&[(*paragraph, range.clone())], |run| {
                        if run.style.is_none() {
                            run.attrs.remove(&char_attrs);
                        }
                    })?;
                }
                Ok(())
            }
            Some(Style::Character(mut style)) => {
                let attrs = if run_style == Some(id) {
                    run_attrs
                } else {
                    CharAttrs::default()
                };
                style.character.merge(&attrs);
                self.put(style)?;
                let ranges = self.ranges_or_paragraph(start, end)?;
                self.map_ranges(&ranges, |run| {
                    if run.style == Some(id) {
                        run.attrs.remove(&attrs);
                    }
                })
            }
        }
    }

    /// Renames a style (STY-05). Two styles of a family cannot share a
    /// name.
    pub fn rename_style(&mut self, id: StyleId, name: &str) -> Result<(), ModelError> {
        let style = self
            .document()
            .styles()
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownStyle(id))?;
        let name = name.trim();
        if name.is_empty() {
            return Err(ModelError::DuplicateStyleName(String::new()));
        }
        self.check_name(style.family(), name, Some(id))?;
        self.put(style.with_name(name))
    }

    /// Sets the function key of a style, from 1 for F1 to 8 for F8, or
    /// removes it. A style that had the key loses it (STY-04).
    pub fn set_style_shortcut(&mut self, id: StyleId, key: Option<u8>) -> Result<(), ModelError> {
        let styles = self.document().styles();
        let style = styles
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownStyle(id))?;
        if let Some(key) = key
            && let Some(other) = styles.by_shortcut(key).cloned()
            && other.id() != id
        {
            self.put(other.with_shortcut(None))?;
        }
        self.put(style.with_shortcut(key))
    }

    /// Deletes a style (STY-05). The text that uses it takes the
    /// replacement: for a paragraph style, another paragraph style, or
    /// the default one when there is none; for a character style, another
    /// character style or none. Styles that inherit from it take its
    /// attributes and its parent, so they look as they did, and styles
    /// that it was the next style of take the replacement.
    pub fn delete_style(
        &mut self,
        id: StyleId,
        replacement: Option<StyleId>,
    ) -> Result<(), ModelError> {
        let document = self.document();
        let styles = document.styles();
        if id == styles.default_paragraph() {
            return Err(ModelError::DefaultStyle);
        }
        let style = styles
            .get(id)
            .cloned()
            .ok_or(ModelError::UnknownStyle(id))?;
        let family = style.family();
        let replacement = match (family, replacement) {
            (_, Some(other)) if other == id => return Err(ModelError::StyleInUse(id)),
            (_, Some(other)) => {
                styles.check_family(other, family)?;
                Some(other)
            }
            (StyleFamily::Paragraph, None) => Some(styles.default_paragraph()),
            (StyleFamily::Character, None) => None,
        };
        // Styles that refer to it.
        let referring: Vec<Style> = styles
            .iter()
            .filter(|s| s.id() != id)
            .filter(|s| {
                s.parent() == Some(id) || matches!(s, Style::Paragraph(p) if p.next == Some(id))
            })
            .cloned()
            .collect();
        for other in referring {
            let other = match (other, &style) {
                (Style::Paragraph(mut p), Style::Paragraph(deleted)) => {
                    if p.parent == Some(id) {
                        let mut paragraph = deleted.paragraph.clone();
                        paragraph.merge(&p.paragraph);
                        p.paragraph = paragraph;
                        let mut character = deleted.character.clone();
                        character.merge(&p.character);
                        p.character = character;
                        p.parent = deleted.parent;
                    }
                    if p.next == Some(id) {
                        p.next = replacement;
                    }
                    Style::Paragraph(p)
                }
                (Style::Character(mut c), Style::Character(deleted)) => {
                    let mut character = deleted.character.clone();
                    character.merge(&c.character);
                    c.character = character;
                    c.parent = deleted.parent;
                    Style::Character(c)
                }
                (other, _) => other,
            };
            self.put(other)?;
        }
        // The text that uses it.
        let uses = self.document().style_uses(id);
        match family {
            StyleFamily::Paragraph => {
                let replacement = replacement.expect("a paragraph style replaces another");
                for (paragraph, range) in uses {
                    let p = Position::new(paragraph, range.start);
                    self.set_paragraph_style(p, p, replacement)?;
                }
            }
            StyleFamily::Character => {
                self.map_ranges(&uses, |run| {
                    if run.style == Some(id) {
                        run.style = replacement;
                    }
                })?;
            }
        }
        self.apply(Op::RemoveStyle { style: id })
    }

    /// Pastes a copied style over the text between two positions (STY-08):
    /// the paragraphs take its paragraph style and attributes, and the
    /// text, when there is a selection, its character style and
    /// attributes. A style that the document does not have by name comes
    /// with it.
    pub fn paste_style(
        &mut self,
        start: Position,
        end: Position,
        copied: &CopiedStyle,
    ) -> Result<(), ModelError> {
        let paragraph_style =
            self.style_by_name_or_add(Style::Paragraph(copied.paragraph.clone()))?;
        let character_style = match &copied.character {
            Some(style) => Some(self.style_by_name_or_add(Style::Character(style.clone()))?),
            None => None,
        };
        let ranges = self.document().paragraph_ranges(start, end)?;
        for (id, _) in &ranges {
            let p = Position::new(*id, 0);
            self.set_paragraph_style(p, p, paragraph_style)?;
            self.set_own_paragraph_attrs(*id, copied.paragraph_attrs.clone())?;
        }
        if start != end {
            let attrs = copied.character_attrs.clone();
            self.map_ranges(&ranges, |run| {
                run.style = character_style;
                run.attrs = attrs.clone();
            })?;
        }
        Ok(())
    }

    /// The style of the document with the name of a style, or the style
    /// itself added with a new identifier.
    fn style_by_name_or_add(&mut self, style: Style) -> Result<StyleId, ModelError> {
        if let Some(found) = self
            .document()
            .styles()
            .by_name(style.family(), style.name())
        {
            return Ok(found.id());
        }
        let id = self.new_id();
        let style = match style {
            Style::Paragraph(p) => Style::Paragraph(ParagraphStyle {
                id,
                shortcut: None,
                ..p
            }),
            Style::Character(c) => Style::Character(CharacterStyle {
                id,
                shortcut: None,
                ..c
            }),
        };
        self.put(style)?;
        Ok(id)
    }

    /// Imports styles from another document (STY-10). A style with the
    /// name of one of the document replaces it when `replace` is true, and
    /// otherwise comes with a new name, as "Body 2". Parents and next
    /// styles that are imported too, or that the document has by name,
    /// stay; otherwise the style takes the attributes they gave it.
    /// Shortcuts do not come along. Returns the identifiers of the
    /// imported styles, in the order of `ids`.
    pub fn import_styles(
        &mut self,
        source: &Document,
        ids: &[StyleId],
        replace: bool,
    ) -> Result<Vec<StyleId>, ModelError> {
        // The identifier and name that each imported style takes.
        let mut map: HashMap<StyleId, StyleId> = HashMap::new();
        let mut order: Vec<(StyleId, StyleId, Style)> = Vec::new();
        for &id in ids {
            if map.contains_key(&id) {
                continue;
            }
            let source_style = source
                .styles()
                .get(id)
                .ok_or(ModelError::UnknownStyle(id))?;
            let family = source_style.family();
            let styles = self.document().styles();
            let existing = styles.by_name(family, source_style.name()).map(Style::id);
            let (target, name, shortcut) = match existing {
                Some(existing) if replace => (
                    existing,
                    source_style.name().to_owned(),
                    styles.get(existing).and_then(Style::shortcut),
                ),
                _ => {
                    // Not a name taken in the document, nor by a style
                    // imported before.
                    let taken = |name: &str| {
                        styles.by_name(family, name).is_some()
                            || order
                                .iter()
                                .any(|(_, _, s)| s.family() == family && s.name() == name)
                    };
                    let base = source_style.name();
                    let name = if taken(base) {
                        (2..)
                            .map(|n| format!("{base} {n}"))
                            .find(|name| !taken(name))
                            .expect("a free number")
                    } else {
                        base.to_owned()
                    };
                    (self.new_id(), name, None)
                }
            };
            // First on its own, with every attribute its chain gives it.
            let flat = match family {
                StyleFamily::Paragraph => Style::Paragraph(ParagraphStyle {
                    id: target,
                    shortcut,
                    ..source.flat_paragraph_style(id).expect("a paragraph style")
                }),
                StyleFamily::Character => Style::Character(CharacterStyle {
                    id: target,
                    shortcut,
                    ..source.flat_character_style(id).expect("a character style")
                }),
            }
            .with_name(&name);
            map.insert(id, target);
            order.push((id, target, flat));
        }
        for (_, _, flat) in &order {
            self.put(flat.clone())?;
        }
        // The style in the document for a style of the source, by import
        // or by name.
        let find = |edit: &Edit, id: StyleId| {
            map.get(&id).copied().or_else(|| {
                let style = source.styles().get(id)?;
                edit.document()
                    .styles()
                    .by_name(style.family(), style.name())
                    .map(Style::id)
            })
        };
        // Then with their parents and next styles, where the document has
        // them.
        for (id, _, flat) in &order {
            let original = source.styles().get(*id).expect("checked above");
            let parent = original.parent().and_then(|p| find(self, p));
            let next = match original {
                Style::Paragraph(p) => p.next.and_then(|n| find(self, n)),
                Style::Character(_) => None,
            };
            let mut linked = flat.clone();
            if let Style::Paragraph(p) = &mut linked {
                p.next = next;
            }
            if parent.is_some() {
                let with_parent = match (&linked, original) {
                    (Style::Paragraph(p), Style::Paragraph(o)) => {
                        Style::Paragraph(ParagraphStyle {
                            parent,
                            paragraph: o.paragraph.clone(),
                            character: o.character.clone(),
                            ..p.clone()
                        })
                    }
                    (Style::Character(c), Style::Character(o)) => {
                        Style::Character(CharacterStyle {
                            parent,
                            character: o.character.clone(),
                            ..c.clone()
                        })
                    }
                    _ => unreachable!("the same family"),
                };
                // A parent that would make a cycle, as when a replaced
                // style is the parent of its own parent, gives way to the
                // flat attributes.
                match self.put(with_parent) {
                    Ok(()) => continue,
                    Err(ModelError::StyleCycle(_) | ModelError::WrongStyleFamily { .. }) => {}
                    Err(error) => return Err(error),
                }
            }
            self.put(linked)?;
        }
        Ok(ids.iter().map(|id| map[id]).collect())
    }
}
