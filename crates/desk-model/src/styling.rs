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
use crate::list::ListStyle;
use crate::op::Op;
use crate::style::{CharacterStyle, ParagraphStyle, Style, StyleFamily, StyleId};
use crate::text::{Paragraph, Position};
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
    /// The list styles that the paragraph style and the paragraph name.
    pub lists: Vec<ListStyle>,
}

/// Text copied with its styles, to paste into the same or another document
/// keeping its look (TXT-04).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    /// The paragraphs, the first and the last of them maybe in part.
    pub paragraphs: Vec<FragmentParagraph>,
    /// Whether the first paragraph was copied from its start.
    pub whole_start: bool,
    /// The list styles that the paragraphs name.
    pub lists: Vec<ListStyle>,
    /// The notes whose marks the text holds, which a paste copies as
    /// notes of their own (EST-13).
    pub notes: Vec<crate::Note>,
    /// The objects in the text whose marks the text holds, which a paste
    /// copies as objects of their own (OBJ-01).
    pub objects: Vec<crate::Object>,
}

/// A paragraph of a [`Fragment`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FragmentParagraph {
    /// The paragraph style, with every attribute it resolves to and no
    /// parent.
    pub style: ParagraphStyle,
    /// The attributes set on the paragraph itself.
    pub attrs: ParaAttrs,
    /// The text, in runs with their character styles, each with every
    /// attribute its chain sets and no parent, and their attributes.
    pub runs: Vec<(String, Option<CharacterStyle>, CharAttrs)>,
}

impl Fragment {
    /// The text, with a line feed between paragraphs.
    pub fn text(&self) -> String {
        self.paragraphs
            .iter()
            .map(|p| {
                p.runs
                    .iter()
                    .map(|(t, _, _)| t.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
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
    /// Whether a list item has a level of its own over its list style.
    pub list: bool,
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
        let styles = self.styles();
        let own = styles
            .resolve_paragraph(paragraph.style(), &ParaAttrs::default())
            .list_style;
        let direct = paragraph.attrs().list_style.flatten();
        let mut lists: Vec<ListStyle> = Vec::new();
        for id in [own, direct].into_iter().flatten() {
            if let Some(list) = styles.list(id)
                && !lists.iter().any(|l| l.id == id)
            {
                lists.push(list.clone());
            }
        }
        Ok(CopiedStyle {
            lists,
            paragraph: self
                .flat_paragraph_style(paragraph.style())
                .ok_or(ModelError::UnknownStyle(paragraph.style()))?,
            paragraph_attrs: paragraph.attrs().clone(),
            character: style.and_then(|id| self.flat_character_style(id)),
            character_attrs: attrs,
        })
    }

    /// The text between two positions with its styles, for Copy (TXT-04).
    pub fn copy_fragment(&self, start: Position, end: Position) -> Result<Fragment, ModelError> {
        let styles = self.styles();
        let mut lists: Vec<ListStyle> = Vec::new();
        let mut paragraphs = Vec::new();
        for (id, range) in self.paragraph_ranges(start, end)? {
            let paragraph = self.paragraph(id)?;
            let own = styles
                .resolve_paragraph(paragraph.style(), &ParaAttrs::default())
                .list_style;
            let direct = paragraph.attrs().list_style.flatten();
            for list in [own, direct].into_iter().flatten() {
                if let Some(list) = styles.list(list)
                    && !lists.iter().any(|l| l.id == list.id)
                {
                    lists.push(list.clone());
                }
            }
            let runs = paragraph
                .slice(range)?
                .into_iter()
                .map(|run| {
                    let style = run.style.and_then(|s| self.flat_character_style(s));
                    (run.text().to_owned(), style, run.attrs.clone())
                })
                .collect();
            paragraphs.push(FragmentParagraph {
                style: self
                    .flat_paragraph_style(paragraph.style())
                    .ok_or(ModelError::UnknownStyle(paragraph.style()))?,
                attrs: paragraph.attrs().clone(),
                runs,
            });
        }
        let notes = paragraphs
            .iter()
            .flat_map(|p: &FragmentParagraph| &p.runs)
            .filter_map(|(_, _, attrs)| match attrs.field.clone().flatten() {
                Some(crate::Field::NoteMark { note }) => self.notes().get(note).cloned(),
                _ => None,
            })
            .collect();
        let objects = paragraphs
            .iter()
            .flat_map(|p: &FragmentParagraph| &p.runs)
            .filter_map(|(_, _, attrs)| match attrs.field.clone().flatten() {
                Some(crate::Field::ObjectMark { object }) => self.objects().get(object).cloned(),
                _ => None,
            })
            .collect();
        Ok(Fragment {
            paragraphs,
            whole_start: start.offset == 0,
            lists,
            notes,
            objects,
        })
    }

    /// The text that uses a style: whole paragraphs for a paragraph style,
    /// runs for a character style, in the order of the body (STY-09).
    pub fn style_uses(&self, id: StyleId) -> Vec<(NodeId, Range<usize>)> {
        let mut uses: Vec<(NodeId, Range<usize>)> = Vec::new();
        let styles = self.styles();
        let list = matches!(styles.get(id), Some(Style::List(_)));
        for paragraph in self.body().iter() {
            let in_list = list
                && styles
                    .resolve_paragraph(paragraph.style(), paragraph.attrs())
                    .list_style
                    == Some(id);
            if paragraph.style() == id || in_list {
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
            let own = paragraph.attrs();
            overrides.list |= matches!(own.list_override, Some(Some(_)));
            overrides.paragraph |= !ParaAttrs {
                list_override: None,
                ..own.without_structure()
            }
            .is_empty();
            let mut look = |style: Option<StyleId>, attrs: &CharAttrs| {
                if !attrs.without_field().is_empty() {
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

    pub(crate) fn put(&mut self, style: impl Into<Style>) -> Result<(), ModelError> {
        let mut style = style.into();
        // Breaks belong to paragraphs, never to styles.
        // Fields belong to the text.
        match &mut style {
            Style::Paragraph(p) => {
                p.paragraph = p.paragraph.without_structure();
                p.character = p.character.without_field();
            }
            Style::Character(c) => c.character = c.character.without_field(),
            _ => {}
        }
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
        // Where a list starts again belongs to the paragraph.
        style.paragraph.list_start = None;
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
            let kept = self.document().paragraph(*id)?.attrs().structure();
            self.set_own_paragraph_attrs(*id, kept)?;
        }
        self.map_ranges(&ranges, |run| run.attrs = run.attrs.text_bound())
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
                let para_attrs = ParaAttrs {
                    list_start: None,
                    list_override: None,
                    ..paragraph.attrs().without_structure()
                };
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
            // A list style takes the level of its own of the item.
            Some(Style::List(_)) => self.redefine_list_style(start, end),
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
            (StyleFamily::Character | StyleFamily::List, None) => None,
        };
        // Styles that refer to it.
        let referring: Vec<Style> = styles
            .iter()
            .filter(|s| s.id() != id)
            .filter(|s| {
                s.parent() == Some(id)
                    || matches!(s, Style::Paragraph(p)
                        if p.next == Some(id) || p.paragraph.list_style == Some(Some(id)))
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
                (Style::Paragraph(mut p), Style::List(_)) => {
                    p.paragraph.list_style = Some(replacement);
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
            StyleFamily::List => {
                let direct: Vec<(NodeId, ParaAttrs)> = self
                    .document()
                    .body()
                    .iter()
                    .filter(|p| p.attrs().list_style == Some(Some(id)))
                    .map(|p| (p.id(), p.attrs().clone()))
                    .collect();
                for (paragraph, mut attrs) in direct {
                    attrs.list_style = Some(replacement);
                    self.set_own_paragraph_attrs(paragraph, attrs)?;
                }
            }
        }
        self.apply(Op::RemoveStyle { style: id })
    }

    /// Replaces a font family with another in every style and all the
    /// text, as the Warnings window does for a missing font (INT-22).
    pub fn replace_font_family(&mut self, from: &str, to: &str) -> Result<(), ModelError> {
        let matches = |family: &Option<std::sync::Arc<str>>| {
            family
                .as_deref()
                .is_some_and(|f| f.eq_ignore_ascii_case(from))
        };
        let to: std::sync::Arc<str> = to.into();
        let changed: Vec<Style> = self
            .document()
            .styles()
            .iter()
            .filter_map(|style| match style {
                Style::Paragraph(p) if matches(&p.character.font_family) => {
                    let mut p = p.clone();
                    p.character.font_family = Some(to.clone());
                    Some(Style::Paragraph(p))
                }
                Style::Character(c) if matches(&c.character.font_family) => {
                    let mut c = c.clone();
                    c.character.font_family = Some(to.clone());
                    Some(Style::Character(c))
                }
                _ => None,
            })
            .collect();
        for style in changed {
            self.put(style)?;
        }
        // The family that no style sets, which text takes by default.
        let styles = self.document().styles();
        let default = styles.default_paragraph();
        let resolved = styles.resolve_character(default, None, &CharAttrs::default());
        if resolved.font_family.eq_ignore_ascii_case(from)
            && let Some(mut style) = styles.paragraph(default).cloned()
        {
            style.character.font_family = Some(to.clone());
            self.put(style)?;
        }
        let (start, end) = (self.document().start(), self.document().end());
        self.map_runs(start, end, |run| {
            if matches(&run.attrs.font_family) {
                run.attrs.font_family = Some(to.clone());
            }
        })
    }

    /// Pastes copied text at a position with its styles (TXT-04), and
    /// returns the position after it. Styles that the document does not
    /// have by name come with it. The text of the first paragraph joins
    /// the paragraph at the position and keeps its look there, unless that
    /// paragraph is empty and the first paragraph was copied whole; the
    /// other paragraphs take their own styles.
    pub fn paste_fragment(
        &mut self,
        at: Position,
        fragment: &Fragment,
    ) -> Result<Position, ModelError> {
        let mut lists: HashMap<StyleId, StyleId> = HashMap::new();
        for list in &fragment.lists {
            lists.insert(
                list.id,
                self.style_by_name_or_add(Style::List(list.clone()))?,
            );
        }
        let relink = |attrs: &mut ParaAttrs| {
            if let Some(Some(list)) = attrs.list_style {
                attrs.list_style = Some(lists.get(&list).copied());
            }
        };
        // Each mark pasted has a note of its own, a copy of the one it
        // was copied with (EST-13).
        let mut notes: HashMap<NodeId, NodeId> = HashMap::new();
        for note in &fragment.notes {
            let id = self.new_id();
            notes.insert(note.id, id);
        }
        for note in &fragment.notes {
            let id = notes[&note.id];
            let paragraphs = note
                .paragraphs
                .iter()
                .map(|p| {
                    let runs = p.runs().map(|run| {
                        let mut run = run.clone();
                        if let Some(Some(crate::Field::NoteMark { note: old })) = run.attrs.field
                            && let Some(&new) = notes.get(&old)
                        {
                            run.attrs.field = Some(Some(crate::Field::NoteMark { note: new }));
                        }
                        run
                    });
                    Paragraph::from_runs(self.new_id(), p.style(), p.attrs().clone(), runs)
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.set_note(
                id,
                Some(crate::Note {
                    id,
                    mark: note.mark.clone(),
                    paragraphs: paragraphs.into(),
                }),
            )?;
        }
        // And each object in the text, a copy of the one it was copied
        // with (OBJ-01).
        let mut objects: HashMap<NodeId, NodeId> = HashMap::new();
        for object in &fragment.objects {
            let copy = self.renumbered_object(object)?;
            objects.insert(object.id, copy.id);
            self.add_object(copy)?;
        }
        let mut position = at;
        for (i, source) in fragment.paragraphs.iter().enumerate() {
            let mut style = source.style.clone();
            relink(&mut style.paragraph);
            let mut attrs = source.attrs.clone();
            relink(&mut attrs);
            let joins = i == 0
                && !(fragment.whole_start && self.document().paragraph(at.paragraph)?.is_empty());
            if i > 0 {
                position = self.split_paragraph(position)?;
            }
            if !joins {
                let id = self.style_by_name_or_add(Style::Paragraph(style.clone()))?;
                let p = Position::new(position.paragraph, 0);
                self.set_paragraph_style(p, p, id)?;
                self.set_own_paragraph_attrs(position.paragraph, attrs)?;
            }
            let target = self.document().paragraph(position.paragraph)?.style();
            let mut runs = Vec::new();
            for (text, character, run_attrs) in &source.runs {
                let character_id = match character {
                    Some(c) => Some(self.style_by_name_or_add(Style::Character(c.clone()))?),
                    None => None,
                };
                let attrs = if joins {
                    // The look that the text had, over what the paragraph
                    // it joins gives it.
                    let mut look = CharFormat::default();
                    style.character.apply_to(&mut look);
                    if let Some(c) = character {
                        c.character.apply_to(&mut look);
                    }
                    run_attrs.apply_to(&mut look);
                    let base = self.document().styles().resolve_character(
                        target,
                        character_id,
                        &CharAttrs::default(),
                    );
                    CharAttrs::from(look).difference(&base)
                } else {
                    run_attrs.clone()
                };
                let mut attrs = attrs;
                match attrs.field {
                    Some(Some(crate::Field::NoteMark { note })) => {
                        if let Some(&new) = notes.get(&note) {
                            attrs.field = Some(Some(crate::Field::NoteMark { note: new }));
                        }
                    }
                    Some(Some(crate::Field::ObjectMark { object })) => {
                        // A mark copied without its object is left out.
                        let Some(&new) = objects.get(&object) else {
                            continue;
                        };
                        attrs.field = Some(Some(crate::Field::ObjectMark { object: new }));
                    }
                    _ => {}
                }
                runs.push(crate::text::Run::with_attrs(text, character_id, attrs));
            }
            let len: usize = runs.iter().map(crate::text::Run::len).sum();
            if len > 0 {
                self.apply(Op::ReplaceText {
                    paragraph: position.paragraph,
                    range: position.offset..position.offset,
                    runs,
                })?;
            }
            position.offset += len;
        }
        Ok(position)
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
        let mut lists: HashMap<StyleId, StyleId> = HashMap::new();
        for list in &copied.lists {
            lists.insert(
                list.id,
                self.style_by_name_or_add(Style::List(list.clone()))?,
            );
        }
        let relink = |attrs: &mut ParaAttrs| {
            if let Some(Some(list)) = attrs.list_style {
                attrs.list_style = Some(lists.get(&list).copied());
            }
        };
        let mut paragraph = copied.paragraph.clone();
        relink(&mut paragraph.paragraph);
        let mut paragraph_attrs = copied.paragraph_attrs.without_structure();
        relink(&mut paragraph_attrs);
        let paragraph_style = self.style_by_name_or_add(Style::Paragraph(paragraph))?;
        let character_style = match &copied.character {
            Some(style) => Some(self.style_by_name_or_add(Style::Character(style.clone()))?),
            None => None,
        };
        let ranges = self.document().paragraph_ranges(start, end)?;
        for (id, _) in &ranges {
            let p = Position::new(*id, 0);
            self.set_paragraph_style(p, p, paragraph_style)?;
            let mut attrs = paragraph_attrs.clone();
            attrs.merge(&self.document().paragraph(*id)?.attrs().structure());
            self.set_own_paragraph_attrs(*id, attrs)?;
        }
        if start != end {
            let attrs = copied.character_attrs.clone();
            self.map_ranges(&ranges, |run| {
                run.style = character_style;
                let kept = run.attrs.text_bound();
                run.attrs = attrs.without_field();
                run.attrs.merge(&kept);
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
            Style::List(l) => Style::List(ListStyle {
                id,
                shortcut: None,
                ..l
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
                StyleFamily::List => Style::List(ListStyle {
                    id: target,
                    shortcut,
                    ..source.styles().list(id).expect("a list style").clone()
                }),
            }
            .with_name(&name);
            map.insert(id, target);
            order.push((id, target, flat));
        }
        // List styles first, since paragraph styles name them.
        for (_, _, flat) in &order {
            if flat.family() == StyleFamily::List {
                self.put(flat.clone())?;
            }
        }
        // The list styles that paragraph styles name, by import, by name
        // or added.
        for (_, _, flat) in &mut order {
            if let Style::Paragraph(p) = flat {
                self.relink_list(source, &mut p.paragraph, &mut map)?;
            }
        }
        for (_, _, flat) in &order {
            if flat.family() != StyleFamily::List {
                self.put(flat.clone())?;
            }
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
                Style::Character(_) | Style::List(_) => None,
            };
            let mut linked = flat.clone();
            if let Style::Paragraph(p) = &mut linked {
                p.next = next;
            }
            if parent.is_some() {
                let with_parent = match (&linked, original) {
                    (Style::Paragraph(p), Style::Paragraph(o)) => {
                        let mut paragraph = o.paragraph.clone();
                        if let Some(Some(list)) = paragraph.list_style {
                            paragraph.list_style = Some(map.get(&list).copied());
                        }
                        Style::Paragraph(ParagraphStyle {
                            parent,
                            paragraph,
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

    /// Points the list style that attributes from another document name
    /// to the list style of this document: the one imported for it, the
    /// one with its name, or a copy added.
    fn relink_list(
        &mut self,
        source: &Document,
        attrs: &mut ParaAttrs,
        map: &mut HashMap<StyleId, StyleId>,
    ) -> Result<(), ModelError> {
        let Some(Some(list)) = attrs.list_style else {
            return Ok(());
        };
        let target = match map.get(&list) {
            Some(target) => Some(*target),
            None => match source.styles().list(list) {
                Some(style) => {
                    let target = self.style_by_name_or_add(Style::List(style.clone()))?;
                    map.insert(list, target);
                    Some(target)
                }
                None => None,
            },
        };
        attrs.list_style = Some(target);
        Ok(())
    }
}
