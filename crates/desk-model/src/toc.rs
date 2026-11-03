// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables of contents (EST-22, EST-23).
//!
//! A table of contents is a run of paragraphs of the body, each an entry
//! of it ([`ParaAttrs::toc_entry`]). The document says which paragraph
//! styles it lists, with which style for their entries and whether they
//! show a page number. Updating a table makes its entries again from the
//! paragraphs after it, up to the next table: the text of each, a tab,
//! and a field of the page it is on.
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

    /// Puts an empty table of contents before a paragraph, and fills it.
    pub fn insert_table_of_contents(&mut self, index: usize) -> Result<(), ModelError> {
        let document = self.document();
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
                toc_entry: Some(Some(TocEntry { target: None })),
                ..ParaAttrs::default()
            },
            std::iter::empty(),
        )?;
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
        let (range, entries) = {
            let document = edit.document();
            let body: Vec<&Paragraph> = document.body().iter().collect();
            let found = tables_of_contents(&body);
            let range = found[n].clone();
            let end = found.get(n + 1).map_or(body.len(), |r| r.start);
            let entries: Vec<(StyleId, NodeId, String, bool)> = body[range.end..end]
                .iter()
                .filter_map(|p| {
                    let level = settings.level(p.style())?;
                    let text = entry_text(p);
                    (!text.is_empty()).then(|| (level.entry, p.id(), text, level.page_number))
                })
                .collect();
            (range, entries)
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
                toc_entry: Some(Some(TocEntry { target: None })),
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
