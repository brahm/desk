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
//! writing takes it out again. A change of formatting is the text
//! between the marks of its `text:format-change`, whose `desk:style-name`
//! names the formatting the text had before.

use std::collections::HashMap;
use std::sync::Arc;

use desk_model::{ChangeKind, Document, Formatting, NodeId, Op, Paragraph, Position};
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

/// The regions that marks of some text name where their text starts.
pub(super) fn marked(events: &Events) -> std::collections::HashSet<&str> {
    events
        .iter()
        .filter_map(|(_, _, field)| match field {
            odf::Field::ChangeStart { id } | odf::Field::Change { id } => Some(id.as_str()),
            _ => None,
        })
        .collect()
}

/// Puts the insertions and deletions whose marks some text of the
/// document has, the body or its stories, in the document: each region a
/// change, the text between the marks of an insertion in it,
/// and the paragraphs that a deletion took, already read as paragraphs of
/// the document (`deleted`, by region), back in their place.
pub(super) fn import(
    document: &mut Document,
    regions: &[odf::ChangedRegion],
    mut deleted: HashMap<String, Vec<Paragraph>>,
    previous: &HashMap<String, Arc<Formatting>>,
    events: Events,
) -> Result<(), FilterError> {
    // The regions whose marks the text has.
    let met: std::collections::HashSet<String> =
        marked(&events).into_iter().map(str::to_owned).collect();
    let read: Vec<&odf::ChangedRegion> = regions
        .iter()
        .filter(|r| met.contains(r.id.as_str()))
        .collect();
    if read.is_empty() || events.is_empty() {
        return Ok(());
    }
    let texts = super::comments::by_text(document, events);
    document.transact("Tracked Changes", |edit| {
        let mut ids: HashMap<&str, (NodeId, ChangeKind)> = HashMap::new();
        for region in &read {
            let kind = match region.kind {
                odf::ChangeKind::Deletion => ChangeKind::Deletion,
                odf::ChangeKind::FormatChange => ChangeKind::Format,
                odf::ChangeKind::Insertion => ChangeKind::Insertion,
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
        for (story, events) in texts {
            match story {
                None => put(edit, &ids, &mut deleted, previous, events)?,
                Some(story) => {
                    edit.edit_story(story, |edit| {
                        put(edit, &ids, &mut deleted, previous, events)
                    })?;
                }
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// Puts the changes whose marks a text has on it: the body, or a story
/// edited as a body.
fn put(
    edit: &mut desk_model::Edit,
    ids: &HashMap<&str, (NodeId, ChangeKind)>,
    deleted: &mut HashMap<String, Vec<Paragraph>>,
    previous: &HashMap<String, Arc<Formatting>>,
    events: Events,
) -> Result<(), desk_model::ModelError> {
    // The insertions and changes of formatting first, which leave the
    // text as it is.
    let mut open: Vec<(NodeId, Position)> = Vec::new();
    let mut deletions: Vec<(NodeId, Position, String)> = Vec::new();
    for (paragraph, offset, field) in &events {
        let at = Position::new(*paragraph, *offset);
        match field {
            odf::Field::ChangeStart { id } => {
                if let Some(&(change, ChangeKind::Insertion | ChangeKind::Format)) =
                    ids.get(id.as_str())
                {
                    open.push((change, at));
                }
            }
            odf::Field::ChangeEnd { id } => {
                let Some(&(change, kind)) = ids.get(id.as_str()) else {
                    continue;
                };
                let Some(i) = open.iter().position(|(c, _)| *c == change) else {
                    continue;
                };
                let (_, start) = open.remove(i);
                // An end before its start, as a file may have, marks
                // nothing.
                let body = edit.document().body();
                let (Some(first), Some(last)) =
                    (body.index_of(start.paragraph), body.index_of(at.paragraph))
                else {
                    continue;
                };
                if (last, at.offset) < (first, start.offset) {
                    continue;
                }
                let breaks: Vec<NodeId> = (first..last)
                    .filter_map(|i| body.get(i).map(Paragraph::id))
                    .collect();
                if kind == ChangeKind::Format {
                    // Breaks have no formatting of their own to change.
                    edit.mark_formatted(start, at, change, previous.get(id).cloned())?;
                    continue;
                }
                edit.mark_change(start, at, change)?;
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
        let fits = edit
            .document()
            .paragraph(at.paragraph)
            .is_ok_and(|p| at.offset <= p.len() && p.is_boundary(at.offset));
        if fits {
            restore(edit, change, at, paragraphs)?;
        }
    }
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
/// the regions of insertions, after those already in `regions` (W148).
/// The text comes with the marks that [`super::export_spans`] puts, which
/// name changes by their identifiers.
fn export(
    body: &mut Vec<odf::Paragraph>,
    document: &Document,
    regions: &mut Vec<odf::ChangedRegion>,
) {
    let has_marks = body
        .iter()
        .flat_map(|p| &p.spans)
        .any(|s| s.field.as_ref().is_some_and(is_mark));
    if !has_marks {
        return;
    }
    let mut out: Vec<odf::Paragraph> = Vec::with_capacity(body.len());
    // The paragraph being written, which a deletion of its break joins to
    // the next.
    let mut current: Option<odf::Paragraph> = None;
    // The open deletion, by its region, with the paragraph it is taking.
    let mut deleting: Option<(usize, odf::Paragraph)> = None;
    let mut inserting: Option<String> = None;
    // A region for the change a mark starts, with the automatic style of
    // the formatting before that the mark of a change of formatting
    // carries.
    let region = |regions: &mut Vec<odf::ChangedRegion>,
                  id: &str,
                  previous: Option<&String>|
     -> Option<(usize, ChangeKind)> {
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
                ChangeKind::Format => odf::ChangeKind::FormatChange,
            },
            creator: Some(change.author.to_string()),
            date: change.date.as_deref().map(str::to_owned),
            deleted: Vec::new(),
            previous_style: previous
                .filter(|_| change.kind == ChangeKind::Format)
                .cloned(),
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
            *taking = taken(&paragraph);
        }
        for span in paragraph.spans {
            match &span.field {
                Some(odf::Field::ChangeStart { id }) => {
                    match region(regions, id, span.style.as_ref()) {
                        Some((index, ChangeKind::Deletion)) => {
                            let name = regions[index].id.clone();
                            here.spans.push(mark(odf::Field::Change { id: name }));
                            // Its first paragraph joins the one of the
                            // mark, which keeps its own list and level.
                            deleting = Some((
                                index,
                                odf::Paragraph {
                                    style: paragraph.style.clone(),
                                    ..odf::Paragraph::default()
                                },
                            ));
                        }
                        // Text added or formatted stays, between its marks.
                        Some((index, ChangeKind::Insertion | ChangeKind::Format)) => {
                            let name = regions[index].id.clone();
                            here.spans.push(odf::Span {
                                field: Some(odf::Field::ChangeStart { id: name.clone() }),
                                style: None,
                                ..span.clone()
                            });
                            inserting = Some(name);
                        }
                        None => {}
                    }
                }
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
                // Comments stay in the text, where deleted text was.
                Some(odf::Field::Annotation(_) | odf::Field::AnnotationEnd { .. }) => {
                    here.spans.push(span);
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
}

/// A paragraph that a deletion takes whole after the break before it,
/// empty: its style, the level of its heading and the list it is an item
/// of, which the region keeps for when the deletion is rejected (B-067).
fn taken(paragraph: &odf::Paragraph) -> odf::Paragraph {
    odf::Paragraph {
        style: paragraph.style.clone(),
        outline_level: paragraph.outline_level,
        list: paragraph.list.clone(),
        ..odf::Paragraph::default()
    }
}

/// Takes the deleted text of every text of a document written as ODF out
/// to the regions of its deletions, as [`export`] does for one: the body,
/// the notes in it, the headers and footers, and the text of text boxes
/// and shapes; returns the regions (W148).
pub(super) fn export_all(
    out: &mut odf::TextDocument,
    document: &Document,
) -> Vec<odf::ChangedRegion> {
    let mut regions = Vec::new();
    text(&mut out.body, document, &mut regions);
    for object in &mut out.page_objects {
        object_text(object, document, &mut regions);
    }
    let masters = std::iter::once(&mut out.master).chain(out.masters.iter_mut().map(|(_, m)| m));
    for master in masters {
        for object in &mut master.objects {
            object_text(object, document, &mut regions);
        }
        for part in [
            &mut master.header,
            &mut master.header_first,
            &mut master.header_left,
            &mut master.footer,
            &mut master.footer_first,
            &mut master.footer_left,
        ]
        .into_iter()
        .flatten()
        {
            text(&mut part.paragraphs, document, &mut regions);
        }
    }
    // The regions are written in content.xml: the automatic styles of
    // styles.xml that the text of headers and footers took there, and
    // their formatting before a change, go with them.
    let named = regions
        .iter()
        .flat_map(|r| &r.deleted)
        .flat_map(|p| {
            p.style
                .iter()
                .chain(p.spans.iter().filter_map(|s| s.style.as_ref()))
        })
        .chain(regions.iter().filter_map(|r| r.previous_style.as_ref()));
    let mut wanted: Vec<odf::Style> = Vec::new();
    for name in named {
        let missing = !out.automatic_styles.iter().any(|s| &s.name == name)
            && !wanted.iter().any(|s| &s.name == name);
        if missing && let Some(style) = out.master_styles.iter().find(|s| &s.name == name) {
            wanted.push(style.clone());
        }
    }
    out.automatic_styles.extend(wanted);
    regions
}

/// One text, and the notes and objects in it.
fn text(
    paragraphs: &mut Vec<odf::Paragraph>,
    document: &Document,
    regions: &mut Vec<odf::ChangedRegion>,
) {
    // The notes and objects first, while those in deleted text are still
    // in the text.
    for paragraph in paragraphs.iter_mut() {
        for span in &mut paragraph.spans {
            match &mut span.field {
                Some(odf::Field::Note(note)) => text(&mut note.body, document, regions),
                Some(odf::Field::Object(object)) => object_text(object, document, regions),
                _ => {}
            }
        }
        for object in &mut paragraph.frames {
            object_text(object, document, regions);
        }
    }
    export(paragraphs, document, regions);
}

/// The text of an object, and of those in a group.
fn object_text(
    object: &mut odf::DrawObject,
    document: &Document,
    regions: &mut Vec<odf::ChangedRegion>,
) {
    text(&mut object.text, document, regions);
    match &mut object.kind {
        odf::DrawKind::TextBox { paragraphs, .. } => text(paragraphs, document, regions),
        odf::DrawKind::Group(children) => {
            for child in children {
                object_text(child, document, regions);
            }
        }
        _ => {}
    }
}
