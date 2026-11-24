// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tracked changes of the body to and from ODF (W148, REV-01).
//!
//! An insertion is the text between the `text:change-start` and the
//! `text:change-end` of its `text:changed-region`; one that goes on past
//! the end of a paragraph has inserted its break. A deletion is a
//! `text:change` where the text was, and the region keeps the paragraphs
//! it took: the text after the first joined the paragraph where the mark
//! is. The model keeps deleted text in its place, marked, and the breaks
//! it took on the paragraphs they end, so reading puts it back there and
//! writing takes it out again.

use std::collections::HashMap;

use desk_model::{ChangeKind, Document, NodeId, Op, Paragraph, Position};
use desk_odf as odf;

use super::FilterError;

/// The changes met in the body, by paragraph and offset, in its order.
pub(super) type Events = Vec<(NodeId, usize, odf::Field)>;

/// Whether a field of the text is the mark of a tracked change.
pub(super) fn is_mark(field: &odf::Field) -> bool {
    matches!(
        field,
        odf::Field::ChangeStart { .. } | odf::Field::ChangeEnd { .. } | odf::Field::Change { .. }
    )
}

/// Puts the insertions and deletions of the body in the document: each
/// region a change, the text between the marks of an insertion in it,
/// and the paragraphs that a deletion took, already read as paragraphs of
/// the document (`deleted`, by region), back in their place.
pub(super) fn import(
    document: &mut Document,
    regions: &[odf::ChangedRegion],
    mut deleted: HashMap<String, Vec<Paragraph>>,
    events: Events,
) -> Result<(), FilterError> {
    let read: Vec<&odf::ChangedRegion> = regions
        .iter()
        .filter(|r| r.kind != odf::ChangeKind::FormatChange)
        .collect();
    if read.is_empty() || events.is_empty() {
        return Ok(());
    }
    document.transact("Tracked Changes", |edit| {
        let mut ids: HashMap<&str, (NodeId, ChangeKind)> = HashMap::new();
        for region in &read {
            let kind = match region.kind {
                odf::ChangeKind::Deletion => ChangeKind::Deletion,
                _ => ChangeKind::Insertion,
            };
            let date =
                super::comments::date(region.date.as_deref()).filter(|d| &**d != odf::UNKNOWN_DATE);
            let id = edit.add_change(
                kind,
                region.creator.as_deref().unwrap_or_default(),
                date.as_deref(),
            )?;
            ids.insert(region.id.as_str(), (id, kind));
        }
        // The insertions first, which leave the text as it is.
        let mut open: Vec<(NodeId, Position)> = Vec::new();
        let mut deletions: Vec<(NodeId, Position, String)> = Vec::new();
        for (paragraph, offset, field) in &events {
            let at = Position::new(*paragraph, *offset);
            match field {
                odf::Field::ChangeStart { id } => {
                    if let Some(&(change, ChangeKind::Insertion)) = ids.get(id.as_str()) {
                        open.push((change, at));
                    }
                }
                odf::Field::ChangeEnd { id } => {
                    let Some(&(change, _)) = ids.get(id.as_str()) else {
                        continue;
                    };
                    let Some(i) = open.iter().position(|(c, _)| *c == change) else {
                        continue;
                    };
                    let (_, start) = open.remove(i);
                    edit.mark_change(start, at, change)?;
                    // The breaks it went past.
                    let body = edit.document().body();
                    let (Some(first), Some(last)) =
                        (body.index_of(start.paragraph), body.index_of(at.paragraph))
                    else {
                        continue;
                    };
                    let breaks: Vec<NodeId> = (first..last)
                        .filter_map(|i| body.get(i).map(Paragraph::id))
                        .collect();
                    for paragraph in breaks {
                        edit.mark_break_change(paragraph, Some(change))?;
                    }
                }
                odf::Field::Change { id } => {
                    if let Some(&(change, ChangeKind::Deletion)) = ids.get(id.as_str()) {
                        deletions.push((change, at, id.clone()));
                    }
                }
                _ => {}
            }
        }
        // Then the deletions, from the last, so that the places of those
        // before stay where they are.
        for (change, at, region) in deletions.into_iter().rev() {
            let Some(paragraphs) = deleted.remove(&region) else {
                continue;
            };
            restore(edit, change, at, paragraphs)?;
        }
        Ok(())
    })?;
    Ok(())
}

/// Puts the paragraphs that a deletion took back where its mark is,
/// marked as deleted: the first joins the text before the mark, the last
/// the text after it, and the breaks between them are deleted too.
fn restore(
    edit: &mut desk_model::Edit,
    change: NodeId,
    at: Position,
    paragraphs: Vec<Paragraph>,
) -> Result<(), desk_model::ModelError> {
    let marked = |p: &Paragraph| -> Vec<desk_model::Run> {
        p.runs()
            .cloned()
            .map(|mut run| {
                run.attrs.change = Some(Some(change));
                run
            })
            .collect()
    };
    let Some((first, rest)) = paragraphs.split_first() else {
        return Ok(());
    };
    if rest.is_empty() {
        return edit.apply(Op::ReplaceText {
            paragraph: at.paragraph,
            range: at.offset..at.offset,
            runs: marked(first),
        });
    }
    let body = edit.document().body();
    let index = body
        .index_of(at.paragraph)
        .ok_or(desk_model::ModelError::UnknownParagraph(at.paragraph))?;
    let paragraph = edit.document().paragraph(at.paragraph)?;
    let len = paragraph.len();
    let tail = paragraph.slice(at.offset..len)?;
    let ends = paragraph.attrs().break_change;
    // The first paragraph taken ends the one of the mark, with its break.
    edit.apply(Op::ReplaceText {
        paragraph: at.paragraph,
        range: at.offset..len,
        runs: marked(first),
    })?;
    edit.mark_break_change(at.paragraph, Some(change))?;
    let (last, middle) = rest.split_last().expect("there are two or more");
    let mut place = index + 1;
    for p in middle {
        let mut attrs = p.attrs().clone();
        attrs.break_change = Some(Some(change));
        let id = edit.new_id();
        edit.apply(Op::InsertParagraph {
            index: place,
            paragraph: Paragraph::from_runs(id, p.style(), attrs, marked(p))?,
        })?;
        place += 1;
    }
    // The last one takes the text after the mark, and ends where the
    // paragraph of the mark ended.
    let mut attrs = last.attrs().clone();
    attrs.break_change = ends;
    let mut runs = marked(last);
    runs.extend(tail);
    let id = edit.new_id();
    edit.apply(Op::InsertParagraph {
        index: place,
        paragraph: Paragraph::from_runs(id, last.style(), attrs, runs)?,
    })?;
    Ok(())
}

/// Takes the deleted text of the body out to the regions of its
/// deletions, joining the paragraphs whose breaks they took, and names
/// the regions of insertions; returns the regions (W148). The body comes
/// with the marks that [`super::export_spans`] puts, which name changes
/// by their identifiers.
pub(super) fn export(
    body: &mut Vec<odf::Paragraph>,
    document: &Document,
) -> Vec<odf::ChangedRegion> {
    let mut regions: Vec<odf::ChangedRegion> = Vec::new();
    let has_marks = body
        .iter()
        .flat_map(|p| &p.spans)
        .any(|s| s.field.as_ref().is_some_and(is_mark));
    if !has_marks {
        return regions;
    }
    let mut out: Vec<odf::Paragraph> = Vec::with_capacity(body.len());
    // The paragraph being written, which a deletion of its break joins to
    // the next.
    let mut current: Option<odf::Paragraph> = None;
    // The open deletion, by its region, with the paragraph it is taking.
    let mut deleting: Option<(usize, odf::Paragraph)> = None;
    let mut inserting: Option<String> = None;
    let region = |regions: &mut Vec<odf::ChangedRegion>, id: &str| -> Option<(usize, ChangeKind)> {
        let change = id
            .parse::<u64>()
            .ok()
            .and_then(NodeId::new)
            .and_then(|id| document.changes().get(id))?;
        regions.push(odf::ChangedRegion {
            id: format!("ct{}", regions.len() + 1),
            kind: match change.kind {
                ChangeKind::Insertion => odf::ChangeKind::Insertion,
                ChangeKind::Deletion => odf::ChangeKind::Deletion,
            },
            creator: Some(change.author.to_string()),
            date: change.date.as_deref().map(str::to_owned),
            deleted: Vec::new(),
        });
        Some((regions.len() - 1, change.kind))
    };
    let mark = |field: odf::Field| odf::Span {
        field: Some(field),
        ..odf::Span::default()
    };
    for paragraph in body.drain(..) {
        let mut here = match current.take() {
            // Joined to the paragraph before: what is anchored to it and
            // the bookmarks at it go there.
            Some(mut here) => {
                here.frames.extend(paragraph.frames.iter().cloned());
                here.marks.extend(paragraph.marks.iter().cloned());
                here
            }
            None => odf::Paragraph {
                spans: Vec::new(),
                ..paragraph.clone()
            },
        };
        if let Some((_, taking)) = &mut deleting {
            *taking = odf::Paragraph {
                style: paragraph.style.clone(),
                ..odf::Paragraph::default()
            };
        }
        for span in paragraph.spans {
            match &span.field {
                Some(odf::Field::ChangeStart { id }) => match region(&mut regions, id) {
                    Some((index, ChangeKind::Deletion)) => {
                        let name = regions[index].id.clone();
                        here.spans.push(mark(odf::Field::Change { id: name }));
                        deleting = Some((
                            index,
                            odf::Paragraph {
                                style: paragraph.style.clone(),
                                ..odf::Paragraph::default()
                            },
                        ));
                    }
                    Some((index, ChangeKind::Insertion)) => {
                        let name = regions[index].id.clone();
                        here.spans.push(odf::Span {
                            field: Some(odf::Field::ChangeStart { id: name.clone() }),
                            ..span.clone()
                        });
                        inserting = Some(name);
                    }
                    None => {}
                },
                Some(odf::Field::ChangeEnd { .. }) => {
                    if let Some((index, taking)) = deleting.take() {
                        regions[index].deleted.push(taking);
                    } else if let Some(name) = inserting.take() {
                        here.spans.push(odf::Span {
                            field: Some(odf::Field::ChangeEnd { id: name }),
                            ..span.clone()
                        });
                    }
                }
                _ => match &mut deleting {
                    Some((_, taking)) => taking.spans.push(span),
                    None => here.spans.push(span),
                },
            }
        }
        match &mut deleting {
            // The deletion took the break: the next paragraph joins this.
            Some((index, taking)) => {
                regions[*index].deleted.push(std::mem::take(taking));
                current = Some(here);
            }
            None => out.push(here),
        }
    }
    out.extend(current);
    *body = out;
    regions
}
