// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! List styles and the numbering of list items (LST-02 to LST-05).
//!
//! A list style sets, for each of its nine levels, the label that starts
//! an item, a bullet or a number, and where the label and the text go. It
//! sets nothing about the look of the text, which comes from the
//! paragraph (STY-01). A paragraph is an item of a list when it resolves
//! to a list style whose level has a label; its level and where it starts
//! counting are attributes of the paragraph.
//!
//! Numbers count per list style: an item takes the number after the last
//! item of its style and level, and an item at a level resets the levels
//! below it, so that 1, 1.1, 1.2, 2 counts as an outline does. Paragraphs
//! outside the list do not break the count, as in Pages, where the next
//! list continues from the previous one unless it is set to start again.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::ModelError;
use crate::format::{ParaAttrs, ParaFormat};
use crate::id::NodeId;
use crate::op::Op;
use crate::picture::Picture;
use crate::style::{Style, StyleFamily, StyleId, Styles};
use crate::text::{Paragraph, Position};
use crate::transaction::Edit;
use crate::units::{Length, Rgba};

/// How many levels a list has, as in Pages.
pub const LIST_LEVELS: usize = 9;

/// How a number is written (LST-04).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NumberFormat {
    /// 1, 2, 3.
    #[default]
    Decimal,
    /// I, II, III.
    UpperRoman,
    /// i, ii, iii.
    LowerRoman,
    /// A, B, C.
    UpperAlpha,
    /// a, b, c.
    LowerAlpha,
}

impl NumberFormat {
    /// Writes a number. Roman numbers past 3999 and zero are written in
    /// decimal, and letters go on after z as aa, ab.
    pub fn write(self, n: u32) -> String {
        match self {
            Self::Decimal => n.to_string(),
            Self::UpperRoman => roman(n).unwrap_or_else(|| n.to_string()),
            Self::LowerRoman => roman(n).map_or_else(|| n.to_string(), |r| r.to_lowercase()),
            Self::UpperAlpha => alpha(n).unwrap_or_else(|| n.to_string()),
            Self::LowerAlpha => alpha(n).map_or_else(|| n.to_string(), |a| a.to_lowercase()),
        }
    }
}

fn roman(mut n: u32) -> Option<String> {
    if n == 0 || n > 3999 {
        return None;
    }
    const DIGITS: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, digits) in DIGITS {
        while n >= value {
            out.push_str(digits);
            n -= value;
        }
    }
    Some(out)
}

fn alpha(mut n: u32) -> Option<String> {
    if n == 0 {
        return None;
    }
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(char::from(b'A' + (n % 26) as u8));
        n /= 26;
    }
    Some(out.into_iter().rev().collect())
}

/// What starts an item of a list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ListLabel {
    /// Nothing: the level is not a list.
    #[default]
    None,
    /// A bullet: any text, such as "•" or "+" (LST-03).
    Bullet(Arc<str>),
    /// An image bullet (LST-03), as high as the text at the size of the
    /// level.
    Image(Picture),
    /// A number (LST-04), between a prefix and a suffix, as "(" and ")".
    /// A tiered number starts with the label of the item above it, as
    /// 1.1 under 1. does in Legal (LST-05).
    Number {
        /// How the number is written.
        format: NumberFormat,
        /// The text before the number.
        prefix: Arc<str>,
        /// The text after the number.
        suffix: Arc<str>,
        /// Whether the label of the level above comes first.
        tiered: bool,
    },
}

impl ListLabel {
    /// A bullet.
    pub fn bullet(text: &str) -> Self {
        Self::Bullet(text.into())
    }

    /// A number with a suffix, as "1.".
    pub fn number(format: NumberFormat, suffix: &str) -> Self {
        Self::Number {
            format,
            prefix: "".into(),
            suffix: suffix.into(),
            tiered: false,
        }
    }

    /// Whether the level has a label.
    pub fn is_some(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// A level of a list style.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ListLevel {
    /// The label.
    pub label: ListLabel,
    /// The number of the first item (LST-04), 1 for a level without
    /// numbers.
    pub start: u32,
    /// The distance from the left indent of the paragraph to the label.
    pub label_indent: Length,
    /// The distance from the label to the text, or the width of the label
    /// when it is wider. Every line of the item starts with the text.
    pub text_indent: Length,
    /// The size of the label, in percent (LST-03): of the size of the text
    /// when it scales with the text, and of 12 points otherwise.
    pub size: u16,
    /// Whether the label scales with the text.
    pub scale_with_text: bool,
    /// How far the label is raised above the baseline, lowered when
    /// negative (LST-03).
    pub raise: Length,
    /// The color of the label, or the color of the text when `None`.
    pub color: Option<Rgba>,
}

impl ListLevel {
    /// A level with a label at the indents of Pages for a level, from 0:
    /// the label 18 points further in at each level.
    pub fn new(label: ListLabel, level: usize, text_indent: Length) -> Self {
        Self {
            start: 1,
            label,
            label_indent: Length::from_points(18 * level as i32),
            text_indent,
            size: 100,
            scale_with_text: true,
            raise: Length::ZERO,
            color: None,
        }
    }
}

impl Default for ListLevel {
    fn default() -> Self {
        Self::new(ListLabel::None, 0, Length::ZERO)
    }
}

/// A list style (STY-01).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListStyle {
    /// The identifier.
    pub id: StyleId,
    /// The name shown to the user.
    pub name: Arc<str>,
    /// The shortcut that applies the style, from 1 for Ctrl+1 to 8 for Ctrl+8
    /// (STY-04).
    pub shortcut: Option<u8>,
    /// The levels, from the first.
    pub levels: Arc<[ListLevel; LIST_LEVELS]>,
}

impl ListStyle {
    /// A list style from a level for each depth.
    pub fn from_levels(id: StyleId, name: &str, level: impl Fn(usize) -> ListLevel) -> Self {
        Self {
            id,
            name: name.into(),
            shortcut: None,
            levels: Arc::new(std::array::from_fn(level)),
        }
    }

    /// Bullets, as the Bullet style of Pages: "•" a point low, with the
    /// text 9 points after it.
    pub fn bullet(id: StyleId, name: &str) -> Self {
        Self::from_levels(id, name, |level| ListLevel {
            raise: Length::from_points(-1),
            ..ListLevel::new(ListLabel::bullet("\u{2022}"), level, Length::from_points(9))
        })
    }

    /// Numbers, as the Numbered List style of Pages: 1. at every level.
    pub fn numbered(id: StyleId, name: &str) -> Self {
        Self::from_levels(id, name, |level| {
            ListLevel::new(
                ListLabel::number(NumberFormat::Decimal, "."),
                level,
                Length::from_points(18),
            )
        })
    }

    /// The Harvard style of Pages (LST-05): I., A., 1., a), (1), (a), i),
    /// (i) and i.
    pub fn harvard(id: StyleId, name: &str) -> Self {
        use NumberFormat::*;
        let labels: [(NumberFormat, &str, &str); LIST_LEVELS] = [
            (UpperRoman, "", "."),
            (UpperAlpha, "", "."),
            (Decimal, "", "."),
            (LowerAlpha, "", ")"),
            (Decimal, "(", ")"),
            (LowerAlpha, "(", ")"),
            (LowerRoman, "", ")"),
            (LowerRoman, "(", ")"),
            (LowerRoman, "", "."),
        ];
        Self::from_levels(id, name, |level| {
            let (format, prefix, suffix) = labels[level];
            let label = ListLabel::Number {
                format,
                prefix: prefix.into(),
                suffix: suffix.into(),
                tiered: false,
            };
            let indent = if level == 0 { 23.4 } else { 18.0 };
            ListLevel::new(label, level, Length::from_pt(indent))
        })
    }

    /// The Legal style of Pages (LST-05): 1., 1.1., 1.1.1., with the text
    /// further from deeper labels.
    pub fn legal(id: StyleId, name: &str) -> Self {
        const INDENTS: [f64; LIST_LEVELS] =
            [18.0, 32.4, 43.2, 52.56, 62.64, 72.0, 82.8, 92.16, 102.96];
        Self::from_levels(id, name, |level| {
            let label = ListLabel::Number {
                format: NumberFormat::Decimal,
                prefix: "".into(),
                suffix: ".".into(),
                // The first level has no level above it.
                tiered: level > 0,
            };
            ListLevel::new(label, level, Length::from_pt(INDENTS[level]))
        })
    }

    /// A level, the last one past the end.
    pub fn level(&self, level: u8) -> &ListLevel {
        &self.levels[usize::from(level).min(LIST_LEVELS - 1)]
    }

    /// Whether any level has a label.
    pub fn has_labels(&self) -> bool {
        self.levels.iter().any(|l| l.label.is_some())
    }
}

/// An item of a list: a paragraph with a label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// The list style.
    pub style: StyleId,
    /// The level, from 0.
    pub level: u8,
    /// The number, for a numbered level.
    pub number: Option<u32>,
    /// The text of the label, as "1.1." or "•".
    pub label: String,
    /// The definition of the level.
    pub definition: ListLevel,
}

/// Counts the items of the lists of a sequence of paragraphs, as they
/// follow each other in a text flow.
#[derive(Default)]
pub struct Numbering {
    counters: HashMap<StyleId, [Option<(u32, String)>; LIST_LEVELS]>,
}

impl Numbering {
    /// A numbering with nothing counted yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The item that the next paragraph is, with its resolved format, or
    /// `None` when it is not in a list.
    pub fn next(&mut self, styles: &Styles, format: &ParaFormat) -> Option<ListItem> {
        let id = format.list_style?;
        let style = styles.list(id)?;
        let level = usize::from(format.list_level).min(LIST_LEVELS - 1);
        let definition = format
            .list_override
            .as_deref()
            .unwrap_or(&style.levels[level])
            .clone();
        if !definition.label.is_some() {
            return None;
        }
        let counters = self.counters.entry(id).or_default();
        let number = format.list_start.unwrap_or(match &counters[level] {
            Some((n, _)) => n.saturating_add(1),
            None => definition.start,
        });
        let label = match &definition.label {
            ListLabel::None => String::new(),
            ListLabel::Bullet(text) => text.to_string(),
            ListLabel::Image(_) => String::new(),
            ListLabel::Number {
                format,
                prefix,
                suffix,
                tiered,
            } => {
                let above = if *tiered && level > 0 {
                    counters[level - 1]
                        .as_ref()
                        .map_or("", |(_, label)| label.as_str())
                } else {
                    ""
                };
                format!("{above}{prefix}{}{suffix}", format.write(number))
            }
        };
        counters[level] = Some((number, label.clone()));
        for below in &mut counters[level + 1..] {
            *below = None;
        }
        Some(ListItem {
            style: id,
            level: level as u8,
            number: matches!(definition.label, ListLabel::Number { .. }).then_some(number),
            label,
            definition,
        })
    }
}

impl Styles {
    /// The list items of a sequence of paragraphs, one entry for each.
    pub fn list_items<'a>(
        &self,
        paragraphs: impl IntoIterator<Item = &'a Paragraph>,
    ) -> Vec<Option<ListItem>> {
        let mut numbering = Numbering::new();
        paragraphs
            .into_iter()
            .map(|paragraph| {
                let format = self.resolve_paragraph(paragraph.style(), paragraph.attrs());
                numbering.next(self, &format)
            })
            .collect()
    }
}

/// A list that the start of a paragraph asks for, as "1. " does when
/// automatic lists are on (LST-01).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    /// The bytes of the text that the marker takes, its space included.
    pub len: usize,
    /// The kind of list.
    pub kind: MarkerKind,
}

/// The kind of list a marker asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    /// Bullets of this text: "•", "-", "*" or "+".
    Bullet(char),
    /// Numbers of this format and a point, starting at this number.
    Number(NumberFormat, u32),
    /// Numbers and a parenthesis, as "1)", starting at this number.
    Parenthesized(u32),
}

impl MarkerKind {
    /// The name of the list style that the marker makes.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Bullet('\u{2022}') => "Bullet",
            Self::Bullet('-') => "Dash",
            Self::Bullet('+') => "Plus",
            Self::Bullet(_) => "Asterisk",
            Self::Number(NumberFormat::Decimal, _) => "Numbered List",
            Self::Number(NumberFormat::UpperRoman, _) => "Roman",
            Self::Number(NumberFormat::LowerRoman, _) => "Lower Roman",
            Self::Number(NumberFormat::UpperAlpha, _) => "Upper Lettered",
            Self::Number(NumberFormat::LowerAlpha, _) => "Lettered",
            Self::Parenthesized(_) => "Numbered Parenthesis",
        }
    }

    /// The list style that the marker makes.
    pub fn style(&self, id: StyleId) -> ListStyle {
        let name = self.name();
        match self {
            Self::Bullet('\u{2022}') => ListStyle::bullet(id, name),
            Self::Bullet(c) => {
                let mut style = ListStyle::bullet(id, name);
                let label = ListLabel::Bullet(c.to_string().into());
                style.levels = Arc::new(std::array::from_fn(|level| ListLevel {
                    label: label.clone(),
                    ..style.levels[level].clone()
                }));
                style
            }
            Self::Number(NumberFormat::Decimal, _) => ListStyle::numbered(id, name),
            Self::Number(format, _) => ListStyle::from_levels(id, name, |level| {
                ListLevel::new(
                    ListLabel::number(*format, "."),
                    level,
                    Length::from_points(18),
                )
            }),
            Self::Parenthesized(_) => ListStyle::from_levels(id, name, |level| {
                ListLevel::new(
                    ListLabel::number(NumberFormat::Decimal, ")"),
                    level,
                    Length::from_points(18),
                )
            }),
        }
    }
}

/// The marker at the start of a paragraph's text, if it starts with
/// "•", "-", "*" or "+", a number, "a", "A", "i" or "I" and a point, or a
/// number and a parenthesis, followed by a space and more text (LST-01).
pub fn detect_marker(text: &str) -> Option<Marker> {
    let (head, rest) = text.split_once(' ')?;
    if rest.trim().is_empty() {
        return None;
    }
    let len = head.len() + 1;
    let mut chars = head.chars();
    let kind = match (chars.next()?, chars.as_str()) {
        (c @ ('\u{2022}' | '-' | '*' | '+'), "") => MarkerKind::Bullet(c),
        (_, _) if head.ends_with(')') => {
            let digits = &head[..head.len() - 1];
            if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            MarkerKind::Parenthesized(digits.parse().ok()?)
        }
        (_, _) => {
            let number = head.strip_suffix('.')?;
            match number {
                "a" => MarkerKind::Number(NumberFormat::LowerAlpha, 1),
                "A" => MarkerKind::Number(NumberFormat::UpperAlpha, 1),
                "i" => MarkerKind::Number(NumberFormat::LowerRoman, 1),
                "I" => MarkerKind::Number(NumberFormat::UpperRoman, 1),
                digits if !digits.is_empty() && digits.len() <= 6 => {
                    if !digits.bytes().all(|b| b.is_ascii_digit()) {
                        return None;
                    }
                    MarkerKind::Number(NumberFormat::Decimal, digits.parse().ok()?)
                }
                _ => return None,
            }
        }
    };
    Some(Marker { len, kind })
}

impl Edit {
    /// Sets the attributes of a paragraph itself.
    fn set_list_attrs(
        &mut self,
        id: NodeId,
        change: impl FnOnce(&mut ParaAttrs),
    ) -> Result<(), ModelError> {
        let paragraph = self.document().paragraph(id)?;
        let mut attrs = paragraph.attrs().clone();
        change(&mut attrs);
        if &attrs != paragraph.attrs() {
            let style = paragraph.style();
            self.apply(Op::SetParagraphFormat {
                paragraph: id,
                style,
                attrs,
            })?;
        }
        Ok(())
    }

    fn paragraph_ids(&self, start: Position, end: Position) -> Result<Vec<NodeId>, ModelError> {
        Ok(self
            .document()
            .paragraph_ranges(start, end)?
            .into_iter()
            .map(|(id, _)| id)
            .collect())
    }

    /// Makes the paragraphs between two positions items of a list style,
    /// or takes them out of their list with `None` (LST-02).
    pub fn set_list_style(
        &mut self,
        start: Position,
        end: Position,
        style: Option<StyleId>,
    ) -> Result<(), ModelError> {
        if let Some(style) = style {
            self.document()
                .styles()
                .check_family(style, StyleFamily::List)?;
        }
        for id in self.paragraph_ids(start, end)? {
            let document = self.document();
            let paragraph = document.paragraph(id)?;
            let inherited = document
                .styles()
                .resolve_paragraph(paragraph.style(), &ParaAttrs::default())
                .list_style;
            self.set_list_attrs(id, |attrs| {
                attrs.list_style = (inherited != style).then_some(style);
                if style.is_none() {
                    attrs.list_level = None;
                    attrs.list_start = None;
                }
            })?;
        }
        Ok(())
    }

    /// The list style with the name of a style, or the style added with a
    /// new identifier: how a new list gets a style of Pages, as Bullet.
    pub fn list_style_or_add(
        &mut self,
        name: &str,
        make: impl FnOnce(StyleId, &str) -> ListStyle,
    ) -> Result<StyleId, ModelError> {
        if let Some(found) = self.document().styles().by_name(StyleFamily::List, name) {
            return Ok(found.id());
        }
        let id = self.new_id();
        self.apply(Op::PutStyle {
            style: Style::List(make(id, name)),
        })?;
        Ok(id)
    }

    /// Moves the items between two positions `by` levels deeper, or out
    /// when negative, between the first level and the ninth (LST-02).
    /// With `children`, the items after them that are deeper than the
    /// first move too, as Tab does; without, only the items themselves
    /// move, as Option-Tab does in Pages and Alt+Shift+Right in Desk. Paragraphs outside a list stay.
    pub fn change_list_level(
        &mut self,
        start: Position,
        end: Position,
        by: i32,
        children: bool,
    ) -> Result<(), ModelError> {
        let document = self.document();
        let styles = document.styles();
        let body = document.body();
        let format = |p: &Paragraph| styles.resolve_paragraph(p.style(), p.attrs());
        let in_list = |f: &ParaFormat| {
            f.list_style
                .and_then(|id| styles.list(id))
                .is_some_and(ListStyle::has_labels)
        };
        let mut ids = Vec::new();
        let mut top = None;
        for id in self.paragraph_ids(start, end)? {
            let f = format(document.paragraph(id)?);
            if in_list(&f) {
                top = Some(top.map_or(f.list_level, |t: u8| t.min(f.list_level)));
                ids.push(id);
            }
        }
        if children
            && let Some(top) = top
            && let Some(mut index) = body.index_of(end.paragraph)
        {
            index += 1;
            while let Some(p) = body.get(index) {
                let f = format(p);
                if !in_list(&f) || f.list_level <= top {
                    break;
                }
                ids.push(p.id());
                index += 1;
            }
        }
        let levels: Vec<(NodeId, u8, Option<Arc<ListLevel>>)> = ids
            .iter()
            .map(|&id| {
                let f = format(document.paragraph(id).expect("found above"));
                let new = (i32::from(f.list_level) + by).clamp(0, LIST_LEVELS as i32 - 1) as u8;
                // An item with a level of its own keeps its label, at the
                // indents of the level it goes to.
                let moved = f.list_override.as_ref().and_then(|own| {
                    let style = styles.list(f.list_style?)?;
                    let to = style.level(new);
                    Some(Arc::new(ListLevel {
                        label_indent: to.label_indent,
                        text_indent: to.text_indent,
                        ..(**own).clone()
                    }))
                });
                (id, new, moved)
            })
            .collect();
        for (id, level, moved) in levels {
            self.set_list_attrs(id, |attrs| {
                attrs.list_level = Some(level);
                if let Some(moved) = moved {
                    attrs.list_override = Some(Some(moved));
                }
            })?;
        }
        Ok(())
    }

    /// Sets the number that an item starts its list again at, or lets it
    /// count on from the item before it with `None` (LST-04).
    pub fn set_list_start(
        &mut self,
        paragraph: NodeId,
        start: Option<u32>,
    ) -> Result<(), ModelError> {
        self.set_list_attrs(paragraph, |attrs| attrs.list_start = start.map(Some))
    }

    /// Changes the levels of the items between two positions, each from the
    /// level it has, as the bullets and numbering of the Inspector do
    /// (LST-03 to LST-05): the items get levels of their own over their
    /// list style, which [`Edit::redefine_list_style`] takes into it. An
    /// item whose level comes back to the one of its style loses its own.
    pub fn change_list_items(
        &mut self,
        start: Position,
        end: Position,
        change: impl Fn(&mut ListLevel),
    ) -> Result<(), ModelError> {
        for id in self.paragraph_ids(start, end)? {
            let document = self.document();
            let styles = document.styles();
            let p = document.paragraph(id)?;
            let format = styles.resolve_paragraph(p.style(), p.attrs());
            let Some(style) = format.list_style.and_then(|id| styles.list(id)) else {
                continue;
            };
            let defined = style.level(format.list_level).clone();
            if !defined.label.is_some() {
                continue;
            }
            let mut level = format.list_override.as_deref().unwrap_or(&defined).clone();
            change(&mut level);
            let own = (level != defined).then(|| Arc::new(level));
            self.set_list_attrs(id, |attrs| {
                attrs.list_override = own.map(Some);
            })?;
        }
        Ok(())
    }

    /// Takes the level of its own of the item at a position into its list
    /// style: Redefine List Style from Selection (STY-06). The items
    /// between the positions with that style and level lose their own.
    pub fn redefine_list_style(
        &mut self,
        start: Position,
        end: Position,
    ) -> Result<(), ModelError> {
        let document = self.document();
        let styles = document.styles();
        let p = document.paragraph(start.paragraph)?;
        let format = styles.resolve_paragraph(p.style(), p.attrs());
        let (Some(style), Some(own)) = (format.list_style, format.list_override.clone()) else {
            return Ok(());
        };
        let level = format.list_level;
        self.set_list_level(style, usize::from(level), (*own).clone())?;
        for id in self.paragraph_ids(start, end)? {
            let document = self.document();
            let p = document.paragraph(id)?;
            let f = document.styles().resolve_paragraph(p.style(), p.attrs());
            if f.list_style == Some(style) && f.list_level == level {
                self.set_list_attrs(id, |attrs| attrs.list_override = None)?;
            }
        }
        Ok(())
    }

    /// Takes the levels of their own from the items between two positions:
    /// Revert to Defined Style for a list style (STY-06).
    pub fn revert_to_list_style(
        &mut self,
        start: Position,
        end: Position,
    ) -> Result<(), ModelError> {
        for id in self.paragraph_ids(start, end)? {
            self.set_list_attrs(id, |attrs| attrs.list_override = None)?;
        }
        Ok(())
    }

    /// Changes a level of a list style, for every list with the style
    /// (LST-03 to LST-05).
    pub fn set_list_level(
        &mut self,
        style: StyleId,
        level: usize,
        definition: ListLevel,
    ) -> Result<(), ModelError> {
        let mut list = self
            .document()
            .styles()
            .list(style)
            .cloned()
            .ok_or(ModelError::UnknownStyle(style))?;
        if level >= LIST_LEVELS || list.levels[level] == definition {
            return Ok(());
        }
        Arc::make_mut(&mut list.levels)[level] = definition;
        self.apply(Op::PutStyle {
            style: Style::List(list),
        })
    }

    /// Makes a list style named `name` with the levels of another
    /// (STY-02).
    pub fn new_list_style(&mut self, name: &str, from: StyleId) -> Result<StyleId, ModelError> {
        let styles = self.document().styles();
        if styles.by_name(StyleFamily::List, name).is_some() {
            return Err(ModelError::DuplicateStyleName(name.to_owned()));
        }
        let levels = styles
            .list(from)
            .ok_or(ModelError::UnknownStyle(from))?
            .levels
            .clone();
        let id = self.new_id();
        self.apply(Op::PutStyle {
            style: Style::List(ListStyle {
                id,
                name: name.into(),
                shortcut: None,
                levels,
            }),
        })?;
        Ok(id)
    }

    /// Turns a paragraph that starts with a marker, as "1. ", into an item
    /// of a list without the marker (LST-01). Returns the length of the
    /// marker that was removed, or `None` when there was none.
    pub fn list_from_marker(&mut self, paragraph: NodeId) -> Result<Option<usize>, ModelError> {
        let document = self.document();
        let p = document.paragraph(paragraph)?;
        let format = document.styles().resolve_paragraph(p.style(), p.attrs());
        if format
            .list_style
            .and_then(|id| document.styles().list(id))
            .is_some_and(ListStyle::has_labels)
        {
            return Ok(None);
        }
        let Some(marker) = detect_marker(&p.text()) else {
            return Ok(None);
        };
        self.delete(
            Position::new(paragraph, 0),
            Position::new(paragraph, marker.len),
        )?;
        let kind = marker.kind.clone();
        let style = self.list_style_or_add(kind.name(), |id, _| kind.style(id))?;
        let at = Position::new(paragraph, 0);
        self.set_list_style(at, at, Some(style))?;
        if let MarkerKind::Number(_, n) | MarkerKind::Parenthesized(n) = marker.kind {
            self.set_list_start(paragraph, Some(n))?;
        }
        Ok(Some(marker.len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_as_their_format_says() {
        assert_eq!(NumberFormat::UpperRoman.write(1994), "MCMXCIV");
        assert_eq!(NumberFormat::LowerRoman.write(4), "iv");
        assert_eq!(NumberFormat::UpperRoman.write(0), "0");
        assert_eq!(NumberFormat::LowerAlpha.write(1), "a");
        assert_eq!(NumberFormat::LowerAlpha.write(26), "z");
        assert_eq!(NumberFormat::UpperAlpha.write(27), "AA");
        assert_eq!(NumberFormat::UpperAlpha.write(28), "AB");
        assert_eq!(NumberFormat::Decimal.write(12), "12");
    }

    #[test]
    fn lst_01_markers_are_found_at_the_start() {
        let kind = |text: &str| detect_marker(text).map(|m| (m.kind, m.len));
        assert_eq!(kind("- item"), Some((MarkerKind::Bullet('-'), 2)));
        assert_eq!(
            kind("\u{2022} item"),
            Some((MarkerKind::Bullet('\u{2022}'), 4))
        );
        assert_eq!(kind("* item"), Some((MarkerKind::Bullet('*'), 2)));
        // Pages also takes "+" and a number with a parenthesis.
        assert_eq!(kind("+ item"), Some((MarkerKind::Bullet('+'), 2)));
        assert_eq!(kind("3) item"), Some((MarkerKind::Parenthesized(3), 3)));
        assert_eq!(kind(") item"), None);
        assert_eq!(kind("a) item"), None);
        assert_eq!(
            kind("12. item"),
            Some((MarkerKind::Number(NumberFormat::Decimal, 12), 4))
        );
        assert_eq!(
            kind("a. item"),
            Some((MarkerKind::Number(NumberFormat::LowerAlpha, 1), 3))
        );
        assert_eq!(
            kind("I. item"),
            Some((MarkerKind::Number(NumberFormat::UpperRoman, 1), 3))
        );
        assert_eq!(kind("- "), None);
        assert_eq!(kind("b. item"), None);
        assert_eq!(kind("1.5 item"), None);
        assert_eq!(kind("word item"), None);
    }
}
