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

use desk_model::{
    ChangeKind, Document, Formatting, NodeId, ObjectKind, Op, ParaFormatting, Paragraph, Position,
};
use desk_odf as odf;

use super::FilterError;

/// The changes met in the body, by paragraph and offset, in its order.
pub(super) type Events = Vec<(NodeId, usize, odf::Field)>;

/// What the deletions of a text took, by region: the paragraphs, already
/// read as paragraphs of the document, and in each the marks of the
/// changes their text was in before (B-070), by offset.
pub(super) struct Taken {
    /// The paragraphs.
    pub deleted: HashMap<String, Vec<Paragraph>>,
    /// The marks in each paragraph.
    pub formers: HashMap<String, Vec<Vec<(usize, odf::Field)>>>,
}

/// What makes regions one change: its kind and the name that the
/// regions of a change that another interrupts share, or the region
/// alone (B-073).
pub(super) type ChangeKey<'a> = (ChangeKind, Result<&'a str, &'a str>);

/// The change that a region is part of, as [`ChangeKey`] tells them.
pub(super) fn key(region: &odf::ChangedRegion) -> ChangeKey<'_> {
    (
        kind_of(region.kind),
        region.change_name.as_deref().ok_or(region.id.as_str()),
    )
}

/// The date of a region, when it is known.
fn known_date(region: &odf::ChangedRegion) -> Option<Arc<str>> {
    super::comments::date(region.date.as_deref()).filter(|d| &**d != odf::UNKNOWN_DATE)
}

/// The kind of change that a region is.
fn kind_of(kind: odf::ChangeKind) -> ChangeKind {
    match kind {
        odf::ChangeKind::Deletion => ChangeKind::Deletion,
        odf::ChangeKind::FormatChange => ChangeKind::Format,
        odf::ChangeKind::Insertion => ChangeKind::Insertion,
    }
}

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
    mut taken: Taken,
    previous: &HashMap<String, Arc<Formatting>>,
    paragraphs: &HashMap<String, Option<Arc<ParaFormatting>>>,
    events: Events,
) -> Result<(), FilterError> {
    // The regions whose marks the text has, or the text deletions took.
    let mut met: std::collections::HashSet<String> =
        marked(&events).into_iter().map(str::to_owned).collect();
    for (_, field) in taken.formers.values().flatten().flatten() {
        if let odf::Field::ChangeStart { id } = field {
            met.insert(id.clone());
        }
    }
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
        // The regions of one change: the pieces of a change that another
        // interrupts are regions of their own (B-073).
        let mut changes: HashMap<ChangeKey<'_>, NodeId> = HashMap::new();
        for region in &read {
            let kind = kind_of(region.kind);
            let key = key(region);
            let id = match changes.get(&key) {
                Some(&id) => id,
                None => {
                    let date = known_date(region);
                    let id = edit.add_change(
                        kind,
                        region.creator.as_deref().unwrap_or_default(),
                        date.as_deref(),
                    )?;
                    changes.insert(key, id);
                    id
                }
            };
            ids.insert(region.id.as_str(), (id, kind));
        }
        for (story, events) in texts {
            match story {
                None => put(edit, &ids, &mut taken, previous, paragraphs, events)?,
                Some(story) => {
                    edit.edit_story(story, |edit| {
                        put(edit, &ids, &mut taken, previous, paragraphs, events)
                    })?;
                }
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// Leaves out the changes that no text of a document read carries: those
/// whose marks enclose no text, as a change of the formatting of an
/// empty paragraph, or whose text is in another change. The log counts
/// them, as the changes it does not find (W148).
pub(super) fn prune(document: &mut Document) -> Result<(), FilterError> {
    let mut carried: std::collections::HashSet<NodeId> = document
        .change_places()
        .into_iter()
        .map(|(_, span)| span.id)
        .collect();
    // The changes that deleted text was in before (B-070).
    let mut texts: Vec<Vec<Paragraph>> = vec![document.body().iter().cloned().collect()];
    texts.extend(
        document
            .stories()
            .into_iter()
            .map(|story| document.story_paragraphs(story)),
    );
    for paragraphs in &texts {
        for paragraph in paragraphs {
            carried.extend(paragraph.runs().filter_map(|r| r.attrs.former.flatten()));
            carried.extend(paragraph.attrs().break_former.flatten());
        }
    }
    // Notes too, wherever their marks are.
    for note in document.notes().iter() {
        let paragraphs = document.story_paragraphs(desk_model::Story::Note(note.id));
        for paragraph in paragraphs.iter() {
            carried.extend(paragraph.runs().filter_map(|r| r.attrs.change.flatten()));
            carried.extend(paragraph.runs().filter_map(|r| r.attrs.former.flatten()));
            carried.extend(paragraph.attrs().break_change.flatten());
            carried.extend(paragraph.attrs().break_former.flatten());
            carried.extend(paragraph.attrs().format_change.flatten());
        }
    }
    // And objects, those that changes moved, and the cells of tables.
    carried.extend(
        document
            .objects()
            .iter()
            .flat_map(desk_model::Object::changes),
    );
    let unused: Vec<NodeId> = document
        .changes()
        .iter()
        .map(|c| c.id)
        .filter(|id| !carried.contains(id))
        .collect();
    if unused.is_empty() {
        return Ok(());
    }
    document.transact("Tracked Changes", |edit| {
        for id in unused {
            edit.set_change(id, None)?;
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
    taken: &mut Taken,
    previous: &HashMap<String, Arc<Formatting>>,
    paragraphs: &HashMap<String, Option<Arc<ParaFormatting>>>,
    events: Events,
) -> Result<(), desk_model::ModelError> {
    let paragraphs_of = paragraphs;
    // The insertions and changes of formatting first, which leave the
    // text as it is.
    // By region, as the regions of one change are its pieces (B-073).
    let mut open: Vec<(&str, Position, usize)> = Vec::new();
    let mut deletions: Vec<(NodeId, Position, String, usize)> = Vec::new();
    // The changes of paragraphs, put on them once the deletions have
    // split the paragraphs they joined (B-069): each with the places of
    // its marks and their order among the marks.
    let mut of_paragraphs = Vec::new();
    for (order, (paragraph, offset, field)) in events.iter().enumerate() {
        let at = Position::new(*paragraph, *offset);
        match field {
            odf::Field::ChangeStart { id } => {
                if let Some((_, ChangeKind::Insertion | ChangeKind::Format)) = ids.get(id.as_str())
                {
                    open.push((id.as_str(), at, order));
                }
            }
            odf::Field::ChangeEnd { id } => {
                let Some(&(change, kind)) = ids.get(id.as_str()) else {
                    continue;
                };
                let Some(i) = open.iter().position(|(r, _, _)| *r == id.as_str()) else {
                    continue;
                };
                let (_, start, from) = open.remove(i);
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
                if let Some(before) = paragraphs.get(id.as_str()) {
                    // The paragraphs after the first that its marks take
                    // in, as they are before the deletions are back.
                    let rest: Vec<NodeId> = (first + 1..=last)
                        .filter_map(|i| body.get(i).map(Paragraph::id))
                        .collect();
                    of_paragraphs.push((change, before.clone(), (start, from), rest));
                    continue;
                }
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
                    deletions.push((change, at, id.clone(), order));
                }
            }
            _ => {}
        }
    }
    // Then the deletions, from the last, so that the places of those
    // before stay where they are.
    // Where the text after each mark of a deletion put back went: the
    // mark, its order, and the paragraph and offset of that text.
    let mut moves: Vec<(Position, usize, NodeId, usize)> = Vec::new();
    for (change, at, region, order) in deletions.into_iter().rev() {
        let Some(paragraphs) = taken.deleted.remove(&region) else {
            continue;
        };
        let fits = edit
            .document()
            .paragraph(at.paragraph)
            .is_ok_and(|p| at.offset <= p.len() && p.is_boundary(at.offset));
        if fits {
            let starts = restore(edit, change, at, paragraphs)?;
            if let Some(&(tail, offset)) = starts.last() {
                moves.push((at, order, tail.paragraph, offset));
            }
            let formers = taken.formers.remove(&region).unwrap_or_default();
            mark_formers(edit, ids, previous, paragraphs_of, &starts, &formers)?;
        }
    }
    // A change of paragraphs is on each paragraph its marks are in, empty
    // ones too, where they are once the deletions are back.
    let moved = |(mut place, order): (Position, usize)| {
        for &(at, mark, tail, offset) in &moves {
            if place.paragraph == at.paragraph
                && (place.offset > at.offset || place.offset == at.offset && order > mark)
            {
                place = Position::new(tail, place.offset - at.offset + offset);
            }
        }
        place
    };
    // The paragraph of its start, where the deletion before it put that,
    // and those whose text its marks take in, but not the paragraphs
    // that the deletions in it put back.
    for (change, before, start, rest) in of_paragraphs {
        let first = moved(start).paragraph;
        for paragraph in std::iter::once(first).chain(rest) {
            edit.mark_paragraph_formatted(paragraph, change, before.clone())?;
        }
    }
    Ok(())
}

/// Puts the text that a deletion took back in the changes it was in
/// before (B-070), from the marks of their regions in it: `starts` are
/// where the text of each paragraph it took is now, and where it ends.
fn mark_formers(
    edit: &mut desk_model::Edit,
    ids: &HashMap<&str, (NodeId, ChangeKind)>,
    previous: &HashMap<String, Arc<Formatting>>,
    paragraphs: &HashMap<String, Option<Arc<ParaFormatting>>>,
    starts: &[(Position, usize)],
    formers: &[Vec<(usize, odf::Field)>],
) -> Result<(), desk_model::ModelError> {
    // The change open, which goes on in the next paragraph.
    let mut open: Option<(NodeId, Option<Arc<Formatting>>)> = None;
    for (k, ((start, end), marks)) in starts.iter().zip(formers).enumerate() {
        let end = *end;
        let len = edit.document().paragraph(start.paragraph)?.len();
        let mut from = start.offset;
        let put = |edit: &mut desk_model::Edit,
                   open: &Option<(NodeId, Option<Arc<Formatting>>)>,
                   from: usize,
                   to: usize|
         -> Result<(), desk_model::ModelError> {
            let Some((id, before)) = open.clone() else {
                return Ok(());
            };
            if from >= to {
                return Ok(());
            }
            edit.mark_former(
                Position::new(start.paragraph, from),
                Position::new(start.paragraph, to),
                id,
                before,
            )
        };
        for (offset, field) in marks {
            let at = (start.offset + offset).min(len);
            match field {
                // A paragraph the deletion took whole keeps the change of
                // its formatting (B-069).
                odf::Field::ChangeStart { id } if paragraphs.contains_key(id.as_str()) => {
                    if let Some(&(change, _)) = ids.get(id.as_str()) {
                        edit.mark_paragraph_formatted(
                            start.paragraph,
                            change,
                            paragraphs[id.as_str()].clone(),
                        )?;
                    }
                }
                odf::Field::ChangeStart { id } => {
                    if let Some(&(change, ChangeKind::Insertion | ChangeKind::Format)) =
                        ids.get(id.as_str())
                    {
                        put(edit, &open, from, at)?;
                        open = Some((change, previous.get(id).cloned()));
                        from = at;
                    }
                }
                odf::Field::ChangeEnd { id }
                    if open.as_ref().is_some_and(|(c, _)| {
                        ids.get(id.as_str()).is_some_and(|(i, _)| i == c)
                    }) =>
                {
                    put(edit, &open, from, at)?;
                    open = None;
                    from = at;
                }
                _ => {}
            }
        }
        // Up to the end of the text the deletion took in the paragraph.
        put(edit, &open, from, end.min(len))?;
        // The break the deletion took after it, in an insertion still
        // open there (B-072).
        if k + 1 < starts.len()
            && let Some((id, _)) = open
            && ids
                .values()
                .any(|&(c, kind)| c == id && kind == ChangeKind::Insertion)
        {
            edit.mark_break_former(start.paragraph, id)?;
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
) -> Result<Vec<(Position, usize)>, desk_model::ModelError> {
    let marked = |p: &Paragraph| -> Vec<desk_model::Run> {
        p.runs()
            .cloned()
            .map(|mut run| {
                run.attrs.change = Some(Some(change));
                run.attrs.formatted = None;
                run.attrs.former = None;
                run
            })
            .collect()
    };
    let Some((first, rest)) = paragraphs.split_first() else {
        return Ok(Vec::new());
    };
    if rest.is_empty() {
        edit.apply(Op::ReplaceText {
            paragraph: at.paragraph,
            range: at.offset..at.offset,
            runs: marked(first),
        })?;
        return Ok(vec![(at, at.offset + first.len())]);
    }
    let body = edit.document().body();
    let index = body
        .index_of(at.paragraph)
        .ok_or(desk_model::ModelError::UnknownParagraph(at.paragraph))?;
    let paragraph = edit.document().paragraph(at.paragraph)?;
    let len = paragraph.len();
    let tail = paragraph.slice(at.offset..len)?;
    let ends = paragraph.attrs().break_change;
    let former = paragraph.attrs().break_former;
    // The first paragraph taken ends the one of the mark, with its break.
    edit.apply(Op::ReplaceText {
        paragraph: at.paragraph,
        range: at.offset..len,
        runs: marked(first),
    })?;
    edit.mark_break_change(at.paragraph, Some(change))?;
    let mut ranges = vec![(at, at.offset + first.len())];
    let (last, middle) = rest.split_last().expect("there are two or more");
    let mut place = index + 1;
    // They keep their identifiers, which page references name (B-068).
    for p in middle {
        let mut attrs = p.attrs().clone();
        attrs.break_change = Some(Some(change));
        attrs.break_former = None;
        let id = p.id();
        edit.apply(Op::InsertParagraph {
            index: place,
            paragraph: Paragraph::from_runs(id, p.style(), attrs, marked(p))?,
        })?;
        ranges.push((Position::new(id, 0), p.len()));
        place += 1;
    }
    // The last one takes the text after the mark, and ends where the
    // paragraph of the mark ended.
    let mut attrs = last.attrs().clone();
    attrs.break_change = ends;
    attrs.break_former = former;
    let mut runs = marked(last);
    runs.extend(tail);
    let id = last.id();
    edit.apply(Op::InsertParagraph {
        index: place,
        paragraph: Paragraph::from_runs(id, last.style(), attrs, runs)?,
    })?;
    ranges.push((Position::new(id, 0), last.len()));
    Ok(ranges)
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
    // The other open changes, which may overlap, as a change of the
    // formatting of a paragraph does the changes of its text: the name
    // of each mark, with the name of its region.
    let mut open: Vec<(String, String)> = Vec::new();
    // A region for the change a mark starts, with the automatic style of
    // the formatting before that the mark of a change of formatting
    // carries. The marks of the changes of paragraphs have names of
    // their own, `p` and the identifier (see `paragraph_change`).
    let region = |regions: &mut Vec<odf::ChangedRegion>,
                  id: &str,
                  previous: Option<&String>|
     -> Option<(usize, ChangeKind)> {
        let (of_paragraph, number) = match id.strip_prefix('p') {
            Some(number) => (true, number),
            None => (false, id),
        };
        let change = number
            .parse::<u64>()
            .ok()
            .and_then(NodeId::new)
            .and_then(|id| document.changes().get(id))?;
        let format = change.kind == ChangeKind::Format;
        regions.push(odf::ChangedRegion {
            id: format!("ct{}", regions.len() + 1),
            kind: match change.kind {
                ChangeKind::Insertion => odf::ChangeKind::Insertion,
                ChangeKind::Deletion => odf::ChangeKind::Deletion,
                // A change of the place of objects or of cells has a
                // region of its own, which no text marks (see
                // `object_text`).
                ChangeKind::Format | ChangeKind::Placement | ChangeKind::Cell => {
                    odf::ChangeKind::FormatChange
                }
            },
            creator: Some(change.author.to_string()),
            date: change.date.as_deref().map(str::to_owned),
            deleted: Vec::new(),
            previous_style: previous.filter(|_| format && !of_paragraph).cloned(),
            previous_paragraph_style: previous.filter(|_| format && of_paragraph).cloned(),
            // Named for now; `export_all` keeps the names of changes
            // with more than one region.
            change_name: Some(format!("c{}", change.id.get())),
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
            // Its bookmarks stay with it, in the region (B-068).
            Some(mut here) => {
                here.frames.extend(paragraph.frames.iter().cloned());
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
                            open.push((id.clone(), String::new()));
                        }
                        // Text added or formatted stays, between its marks;
                        // in text deleted, the change it was in before
                        // goes with it (B-070).
                        Some((
                            index,
                            ChangeKind::Insertion
                            | ChangeKind::Format
                            | ChangeKind::Placement
                            | ChangeKind::Cell,
                        )) => {
                            let name = regions[index].id.clone();
                            let start = odf::Span {
                                field: Some(odf::Field::ChangeStart { id: name.clone() }),
                                style: None,
                                ..span.clone()
                            };
                            match &mut deleting {
                                Some((_, taking)) => taking.spans.push(start),
                                None => here.spans.push(start),
                            }
                            open.push((id.clone(), name));
                        }
                        None => {}
                    }
                }
                Some(odf::Field::ChangeEnd { id }) => {
                    let Some(at) = open.iter().position(|(mark, _)| mark == id) else {
                        continue;
                    };
                    let (_, name) = open.remove(at);
                    if name.is_empty() {
                        if let Some((index, taking)) = deleting.take() {
                            regions[index].deleted.push(taking);
                        }
                    } else {
                        let end = odf::Span {
                            field: Some(odf::Field::ChangeEnd { id: name }),
                            ..span.clone()
                        };
                        match &mut deleting {
                            Some((_, taking)) => taking.spans.push(end),
                            None => here.spans.push(end),
                        }
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
/// of (B-067), and the section it starts and whether it is an entry of a
/// table of contents (B-068), which the region keeps for when the
/// deletion is rejected.
fn taken(paragraph: &odf::Paragraph) -> odf::Paragraph {
    odf::Paragraph {
        style: paragraph.style.clone(),
        outline_level: paragraph.outline_level,
        list: paragraph.list.clone(),
        section: paragraph.section.clone(),
        toc: paragraph.toc,
        marks: paragraph.marks.clone(),
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
    // Only the regions of a change that has more than one share its name
    // (B-073).
    let mut count: HashMap<String, usize> = HashMap::new();
    for name in regions.iter().filter_map(|r| r.change_name.clone()) {
        *count.entry(name).or_default() += 1;
    }
    for region in &mut regions {
        if region
            .change_name
            .as_ref()
            .is_some_and(|name| count[name] < 2)
        {
            region.change_name = None;
        }
    }
    // A region for each change of the cells of tables, which the table
    // that `desk:table` keeps names by the identifier of the change, as
    // the region does in its name (REV-05).
    for object in document.objects().iter() {
        let ObjectKind::Table(table) = &object.kind else {
            continue;
        };
        for r in 0..table.rows() {
            for c in 0..table.cols() {
                let Some(tracked) = &table.cell(r, c).tracked else {
                    continue;
                };
                let name = format!("c{}", tracked.change.get());
                let Some(change) = NodeId::new(tracked.change.get())
                    .and_then(|id| document.changes().get(id))
                    .filter(|c| c.kind == ChangeKind::Cell)
                else {
                    continue;
                };
                if regions
                    .iter()
                    .any(|r| r.change_name.as_deref() == Some(name.as_str()))
                {
                    continue;
                }
                regions.push(odf::ChangedRegion {
                    id: format!("ct{}", regions.len() + 1),
                    kind: odf::ChangeKind::FormatChange,
                    creator: Some(change.author.to_string()),
                    date: change.date.as_deref().map(str::to_owned),
                    change_name: Some(name),
                    ..odf::ChangedRegion::default()
                });
            }
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
        .chain(regions.iter().filter_map(|r| r.previous_style.as_ref()))
        .chain(
            regions
                .iter()
                .filter_map(|r| r.previous_paragraph_style.as_ref()),
        );
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
    moved_region(object, document, regions);
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

/// Names in `desk:moved-from` of an object the region of the change that
/// moved it, a change of formatting that no text marks, one for all the
/// objects of the change (W148, REV-01).
fn moved_region(
    object: &mut odf::DrawObject,
    document: &Document,
    regions: &mut Vec<odf::ChangedRegion>,
) {
    let Some(value) = object.moved_from.take() else {
        return;
    };
    let Some((token, rest)) = value.split_once(' ') else {
        return;
    };
    let Some(change) = token
        .strip_prefix('c')
        .and_then(|n| n.parse::<u64>().ok())
        .and_then(NodeId::new)
        .and_then(|id| document.changes().get(id))
    else {
        return;
    };
    let name = format!("c{}", change.id.get());
    let at = match regions
        .iter()
        .position(|r| r.change_name.as_deref() == Some(name.as_str()))
    {
        Some(at) => at,
        None => {
            regions.push(odf::ChangedRegion {
                id: format!("ct{}", regions.len() + 1),
                kind: odf::ChangeKind::FormatChange,
                creator: Some(change.author.to_string()),
                date: change.date.as_deref().map(str::to_owned),
                change_name: Some(name),
                ..odf::ChangedRegion::default()
            });
            regions.len() - 1
        }
    };
    object.moved_from = Some(format!("{} {rest}", regions[at].id));
}

/// Puts the objects that tracked changes moved, with what their
/// `desk:moved-from` says, in those changes, once the body has the
/// paragraphs that deletions took (W148, REV-01). One that names no
/// change of formatting, or a paragraph the body has not, is in none.
pub(super) fn import_moves(
    source: &odf::TextDocument,
    document: &mut Document,
    moved: &[(NodeId, &str)],
) -> Result<(), FilterError> {
    if moved.is_empty() {
        return Ok(());
    }
    let body: Vec<NodeId> = document.body().iter().map(Paragraph::id).collect();
    document.transact("Tracked Changes", |edit| {
        let mut changes: HashMap<ChangeKey<'_>, NodeId> = HashMap::new();
        for &(id, value) in moved {
            let Some((name, anchor, frame)) = super::objects::parse_moved_from(value) else {
                continue;
            };
            let Some(region) = source
                .tracked_changes
                .iter()
                .find(|r| r.id == name && r.kind == odf::ChangeKind::FormatChange)
            else {
                continue;
            };
            let anchor = match anchor {
                Ok(page) => desk_model::Anchor::Page(page),
                Err(index) => match body.get(index) {
                    Some(&paragraph) => desk_model::Anchor::Paragraph(paragraph),
                    None => continue,
                },
            };
            let Some(mut object) = edit.document().objects().get(id).cloned() else {
                continue;
            };
            let change = match changes.get(&key(region)) {
                Some(&change) => change,
                None => {
                    let date = known_date(region);
                    let change = edit.add_change(
                        ChangeKind::Placement,
                        region.creator.as_deref().unwrap_or_default(),
                        date.as_deref(),
                    )?;
                    changes.insert(key(region), change);
                    change
                }
            };
            object.moved = Some(desk_model::Moved {
                change,
                anchor,
                frame,
            });
            edit.replace_object(object)?;
        }
        Ok(())
    })?;
    Ok(())
}

/// Puts the cells of tables that `desk:table` says tracked changes
/// changed in those changes: each names the change by the identifier it
/// had where the file was written, which the region of the change has in
/// its name (W148, REV-05). A cell whose change the file has not is in
/// none.
pub(super) fn import_cells(
    source: &odf::TextDocument,
    document: &mut Document,
) -> Result<(), FilterError> {
    let tracked: Vec<NodeId> = document
        .objects()
        .iter()
        .filter(|o| match &o.kind {
            ObjectKind::Table(table) => (0..table.rows())
                .any(|r| (0..table.cols()).any(|c| table.cell(r, c).tracked.is_some())),
            _ => false,
        })
        .map(|o| o.id)
        .collect();
    if tracked.is_empty() {
        return Ok(());
    }
    let regions: HashMap<&str, &odf::ChangedRegion> = source
        .tracked_changes
        .iter()
        .filter(|r| r.kind == odf::ChangeKind::FormatChange)
        .filter_map(|r| Some((r.change_name.as_deref()?, r)))
        .collect();
    document.transact("Tracked Changes", |edit| {
        let mut changes: HashMap<ChangeKey<'_>, NodeId> = HashMap::new();
        for id in tracked {
            let Some(mut object) = edit.document().objects().get(id).cloned() else {
                continue;
            };
            let ObjectKind::Table(table) = &mut object.kind else {
                continue;
            };
            let table = std::sync::Arc::make_mut(table);
            for r in 0..table.rows() {
                for c in 0..table.cols() {
                    let Some(tracked) = table.cell(r, c).tracked.clone() else {
                        continue;
                    };
                    let name = format!("c{}", tracked.change.get());
                    let change = match regions.get(name.as_str()) {
                        Some(region) => match changes.get(&key(region)) {
                            Some(&change) => Some(change),
                            None => {
                                let date = known_date(region);
                                let change = edit.add_change(
                                    ChangeKind::Cell,
                                    region.creator.as_deref().unwrap_or_default(),
                                    date.as_deref(),
                                )?;
                                changes.insert(key(region), change);
                                Some(change)
                            }
                        },
                        None => None,
                    };
                    table.cell_mut(r, c).tracked = change.and_then(|change| {
                        Some(std::sync::Arc::new(desk_model::table::Tracked {
                            change: std::num::NonZeroU64::new(change.get())?,
                            ..(*tracked).clone()
                        }))
                    });
                }
            }
            edit.replace_object(object)?;
        }
        Ok(())
    })?;
    Ok(())
}
