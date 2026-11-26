// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables of contents (EST-22, EST-23).
//!
//! A table of contents is a run of paragraphs of the body, each an entry
//! of it ([`ParaAttrs::toc_entry`]). The document says which paragraph
//! styles it lists, with which style for their entries and whether they
//! show a page number. Each table says which paragraphs it lists, as
//! Pages 15.4 does (W100): those of the whole document, of its section,
//! or those after it up to the next table. Updating a table makes its
//! entries again from them: the text of each, a tab, and a field of the
//! page it is on.
//!
//! [`ParaAttrs::toc_entry`]: crate::ParaAttrs::toc_entry

use crate::error::ModelError;
use crate::format::{CharAttrs, ParaAttrs};
use crate::header::{FIELD_CHARACTER, Field};
use crate::id::NodeId;
use crate::style::StyleId;
use crate::text::{LINE_BREAK, Paragraph, Run};
use crate::transaction::Edit;

/// That a paragraph is an entry of a table of contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TocEntry {
    /// The paragraph that the entry lists; none for the one entry of a
    /// table that lists nothing yet.
    pub target: Option<NodeId>,
    /// Which paragraphs the table lists, the same in each of its entries.
    pub scope: TocScope,
}

/// Which paragraphs a table of contents lists (W100): Insert > Table of
/// Contents and the Range of the Format pane in Pages 15.4.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TocScope {
    /// Those of the whole document, outside tables of contents.
    #[default]
    Document,
    /// Those of the section that the table starts in.
    Section,
    /// Those after the table, up to the next table: "To Next
    /// Occurrence", and every table of Pages '09.
    NextTable,
}

/// One paragraph style that tables of contents list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TocLevel {
    /// The style of the paragraphs listed.
    pub style: StyleId,
    /// The style of their entries.
    pub entry: StyleId,
    /// Whether their entries show the number of their page.
    pub page_number: bool,
}

/// What the tables of contents of a document list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TocSettings {
    /// The styles listed, in the order of their levels.
    pub levels: Vec<TocLevel>,
    /// Whether the numbers of pages are links to their headings, which a
    /// click follows (EST-22).
    pub page_links: bool,
}

impl TocSettings {
    /// The level that lists a paragraph style.
    pub fn level(&self, style: StyleId) -> Option<&TocLevel> {
        self.levels.iter().find(|l| l.style == style)
    }

    /// Whether a style is listed, or lists entries.
    pub fn uses(&self, style: StyleId) -> bool {
        self.levels
            .iter()
            .any(|l| l.style == style || l.entry == style)
    }
}

/// The tables of contents of a body: the range of the indexes of the
/// paragraphs of each, in order.
pub fn tables_of_contents(paragraphs: &[&Paragraph]) -> Vec<std::ops::Range<usize>> {
    let mut out: Vec<std::ops::Range<usize>> = Vec::new();
    for (i, paragraph) in paragraphs.iter().enumerate() {
        if paragraph.attrs().toc_entry.flatten().is_none() {
            continue;
        }
        match out.last_mut() {
            Some(range) if range.end == i => range.end = i + 1,
            _ => out.push(i..i + 1),
        }
    }
    out
}

impl crate::Document {
    /// The table of contents that paragraph `index` of the body is an
    /// entry of: the range of the indexes of its entries.
    pub fn table_of_contents_at(&self, index: usize) -> Option<std::ops::Range<usize>> {
        let body: Vec<&Paragraph> = self.body().iter().collect();
        tables_of_contents(&body)
            .into_iter()
            .find(|range| range.contains(&index))
    }

    /// The entries of the table of contents of paragraph `index` that
    /// have its style: those of its level, which are selected together
    /// (EST-23).
    pub fn entries_of_level(&self, index: usize) -> Vec<usize> {
        let Some(range) = self.table_of_contents_at(index) else {
            return Vec::new();
        };
        let body = self.body();
        let style = body.get(index).map(Paragraph::style);
        range
            .filter(|&i| body.get(i).map(Paragraph::style) == style)
            .collect()
    }

    /// Whether a change of the text from one place of the body to another
    /// would change the text of a table of contents, which only updating
    /// it changes. Taking a whole table away is allowed.
    pub fn toc_protects(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        let inside = |paragraph: usize| self.table_of_contents_at(paragraph);
        let (start, end) = if from <= to { (from, to) } else { (to, from) };
        let touched: Vec<std::ops::Range<usize>> = [inside(start.0), inside(end.0)]
            .into_iter()
            .flatten()
            .collect();
        touched.iter().any(|range| {
            let last = self.body().get(range.end - 1).map_or(0, Paragraph::len);
            let covered = start <= (range.start, 0) && end >= (range.end - 1, last) && end > start;
            !covered
        })
    }
}

/// The text of a paragraph as an entry lists it: without its fields,
/// marks of notes and line breaks.
fn entry_text(paragraph: &Paragraph) -> String {
    paragraph
        .runs()
        .filter(|r| r.attrs.field.is_none())
        .map(|r| r.text().replace([LINE_BREAK, '\t'], " "))
        .collect::<String>()
        .trim()
        .to_owned()
}

impl Edit {
    /// Makes the entries of every table of contents again (EST-22).
    pub fn update_tables_of_contents(&mut self) -> Result<(), ModelError> {
        let settings = self.document().text_settings().toc.clone();
        loop_tables(self, &settings)
    }

    /// Gives each level whose entries take the default paragraph style a
    /// style of its own, "TOC" and the name of the style listed, with a
    /// right tab stop at the edge of the text for the page number, where a
    /// leader line can go (EST-23).
    pub fn add_toc_entry_styles(&mut self) -> Result<(), ModelError> {
        let document = self.document();
        let default = document.styles().default_paragraph();
        let page = document.page_setup();
        let edge = page.width - page.margin_left - page.margin_right;
        let mut settings = document.text_settings().clone();
        let mut changed = false;
        for i in 0..settings.toc.levels.len() {
            let level = settings.toc.levels[i];
            if level.entry != default {
                continue;
            }
            let styles = self.document().styles();
            let listed = styles
                .get(level.style)
                .map(|s| s.name().to_owned())
                .unwrap_or_default();
            let name = format!("TOC {listed}");
            let entry = match styles.by_name(crate::StyleFamily::Paragraph, &name) {
                Some(style) => style.id(),
                None => {
                    let id = self.new_id();
                    let mut style = crate::ParagraphStyle::new(id, &name);
                    style.parent = Some(default);
                    let stop = crate::TabStop {
                        alignment: crate::TabAlignment::Right,
                        ..crate::TabStop::left(edge)
                    };
                    style.paragraph.tab_stops = Some(crate::TabStops::new([stop]));
                    // Each level a step further in.
                    let indent = crate::Length::from_points(18 * i as i32);
                    if i > 0 {
                        style.paragraph.left_indent = Some(indent);
                    }
                    self.put(style)?;
                    id
                }
            };
            settings.toc.levels[i].entry = entry;
            changed = true;
        }
        if changed {
            self.set_text_settings(settings)?;
        }
        Ok(())
    }

    /// Changes which paragraphs the table of contents of paragraph
    /// `index` lists, and makes its entries again (W100).
    pub fn set_toc_scope(&mut self, index: usize, scope: TocScope) -> Result<(), ModelError> {
        let Some(range) = self.document().table_of_contents_at(index) else {
            return Ok(());
        };
        for i in range {
            let paragraph = self.document().body().get(i).cloned().expect("in the body");
            let mut attrs = paragraph.attrs().clone();
            let Some(Some(entry)) = attrs.toc_entry else {
                continue;
            };
            if entry.scope == scope {
                continue;
            }
            attrs.toc_entry = Some(Some(TocEntry { scope, ..entry }));
            self.apply(crate::Op::SetParagraphFormat {
                paragraph: paragraph.id(),
                style: paragraph.style(),
                attrs,
            })?;
        }
        self.update_tables_of_contents()
    }

    /// Puts an empty table of contents before a paragraph, listing what
    /// `scope` says, and fills it. A section or a break that starts at
    /// the paragraph starts at the table instead, which is then in that
    /// section. Neighbouring entries are one table, so a table goes
    /// neither in another nor right after one.
    pub fn insert_table_of_contents(
        &mut self,
        index: usize,
        scope: TocScope,
    ) -> Result<(), ModelError> {
        let document = self.document();
        let next_to = |i: usize| document.table_of_contents_at(i).is_some();
        if next_to(index) || (index > 0 && next_to(index - 1)) {
            return Err(ModelError::TableOfContentsNextToAnother);
        }
        let moved = document.body().get(index).cloned().map(|paragraph| {
            let attrs = paragraph.attrs();
            let starts = ParaAttrs {
                start: attrs.start.clone(),
                column_break_before: attrs.column_break_before,
                ..ParaAttrs::default()
            };
            let rest = ParaAttrs {
                start: None,
                column_break_before: None,
                ..attrs.clone()
            };
            (paragraph, starts, rest)
        });
        let settings = document.text_settings().toc.clone();
        let style = settings
            .levels
            .first()
            .map_or(document.styles().default_paragraph(), |l| l.entry);
        let id = self.new_id();
        let paragraph = Paragraph::from_runs(
            id,
            style,
            ParaAttrs {
                toc_entry: Some(Some(TocEntry {
                    target: None,
                    scope,
                })),
                ..moved
                    .as_ref()
                    .map(|(_, starts, _)| starts.clone())
                    .unwrap_or_default()
            },
            std::iter::empty(),
        )?;
        if let Some((old, starts, rest)) = moved
            && starts != ParaAttrs::default()
        {
            self.apply(crate::Op::SetParagraphFormat {
                paragraph: old.id(),
                style: old.style(),
                attrs: rest,
            })?;
        }
        self.apply(crate::Op::InsertParagraph { index, paragraph })?;
        loop_tables(self, &settings)
    }
}

fn loop_tables(edit: &mut Edit, settings: &TocSettings) -> Result<(), ModelError> {
    let count = {
        let body: Vec<&Paragraph> = edit.document().body().iter().collect();
        tables_of_contents(&body).len()
    };
    // Each table, from the first: the ones after it move as it changes.
    for n in 0..count {
        let (range, scope, entries) = {
            let document = edit.document();
            let body: Vec<&Paragraph> = document.body().iter().collect();
            let found = tables_of_contents(&body);
            let range = found[n].clone();
            let scope = body[range.start]
                .attrs()
                .toc_entry
                .flatten()
                .map(|e| e.scope)
                .unwrap_or_default();
            let listed = match scope {
                TocScope::Document => 0..body.len(),
                TocScope::Section => document
                    .section_ranges()
                    .into_iter()
                    .find(|r| r.contains(&range.start))
                    .unwrap_or(0..body.len()),
                TocScope::NextTable => range.end..found.get(n + 1).map_or(body.len(), |r| r.start),
            };
            let entries: Vec<(StyleId, NodeId, String, bool)> = body[listed]
                .iter()
                .filter(|p| p.attrs().toc_entry.flatten().is_none())
                .filter_map(|p| {
                    let level = settings.level(p.style())?;
                    let text = entry_text(p);
                    (!text.is_empty()).then(|| (level.entry, p.id(), text, level.page_number))
                })
                .collect();
            (range, scope, entries)
        };
        let old: Vec<Paragraph> = {
            let body = edit.document().body();
            range
                .clone()
                .map(|i| body.get(i).cloned().expect("in the body"))
                .collect()
        };
        let first = old.first().expect("a table has an entry");
        // What starts at the table stays at its first entry.
        let structure = ParaAttrs {
            toc_entry: None,
            ..first.attrs().structure()
        };
        let mut built = Vec::new();
        for (i, (style, target, text, page_number)) in entries.iter().enumerate() {
            let mut attrs = ParaAttrs {
                toc_entry: Some(Some(TocEntry {
                    target: Some(*target),
                    scope,
                })),
                ..ParaAttrs::default()
            };
            if i == 0 {
                attrs.merge(&structure);
            }
            let mut runs = vec![Run::new(text)];
            if *page_number {
                runs.push(Run::new("\t"));
                runs.push(Run::with_attrs(
                    &FIELD_CHARACTER.to_string(),
                    None,
                    CharAttrs {
                        field: Some(Some(Field::PageOf { paragraph: *target })),
                        ..CharAttrs::default()
                    },
                ));
            }
            let id = edit.new_id();
            built.push(Paragraph::from_runs(id, *style, attrs, runs)?);
        }
        if built.is_empty() {
            // A table that lists nothing keeps one empty entry.
            let mut attrs = ParaAttrs {
                toc_entry: Some(Some(TocEntry {
                    target: None,
                    scope,
                })),
                ..ParaAttrs::default()
            };
            attrs.merge(&structure);
            let id = edit.new_id();
            built.push(Paragraph::from_runs(
                id,
                first.style(),
                attrs,
                std::iter::empty(),
            )?);
        }
        let same = built.len() == old.len()
            && built.iter().zip(&old).all(|(a, b)| {
                a.style() == b.style() && a.attrs() == b.attrs() && a.runs().eq(b.runs())
            });
        if same {
            continue;
        }
        for (i, paragraph) in built.into_iter().enumerate() {
            edit.apply(crate::Op::InsertParagraph {
                index: range.start + i,
                paragraph,
            })?;
        }
        for paragraph in old {
            edit.apply(crate::Op::RemoveParagraph {
                paragraph: paragraph.id(),
            })?;
        }
    }
    Ok(())
}
