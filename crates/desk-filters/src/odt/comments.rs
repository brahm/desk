// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The comments of the body to and from ODF annotations (W149, REV-08,
//! PXV-19).
//!
//! A comment is an `office:annotation` where the text it is on starts,
//! named, and an `office:annotation-end` of its name where that text ends.
//! Each reply is an annotation of its own on the same text, which names
//! the comment in `desk:parent-name`, as LibreOffice names it in
//! `loext:parent-name` (odf-extensions.md); a reader without the extension
//! sees the replies as comments one after the other. An annotation
//! without an end, as other programs write a comment at a point, is on the
//! word there, as Pages 15.4 puts a comment made at the cursor.

use std::collections::HashMap;
use std::sync::Arc;

use desk_model::{Comment, Document, NodeId, Position, Reply};
use desk_odf as odf;

use super::FilterError;

/// Puts the comments found in the body on the text they are on: by
/// paragraph and offset, in the order of the body, each comment and each
/// end met.
pub(super) fn import(
    document: &mut Document,
    events: Vec<(NodeId, usize, odf::Field)>,
) -> Result<(), FilterError> {
    // Each annotation with the text it is on, in the order of the file.
    let mut open: Vec<(String, Position, odf::Annotation)> = Vec::new();
    let mut found: Vec<(Position, Position, odf::Annotation)> = Vec::new();
    for (paragraph, offset, field) in events {
        let at = Position::new(paragraph, offset);
        match field {
            odf::Field::Annotation(annotation) => match annotation.name.clone() {
                Some(name) => open.push((name, at, *annotation)),
                None => found.push((at, at, *annotation)),
            },
            odf::Field::AnnotationEnd { name } => {
                if let Some(i) = open.iter().position(|(n, _, _)| *n == name) {
                    let (_, start, annotation) = open.remove(i);
                    found.push((start, at, annotation));
                }
            }
            _ => {}
        }
    }
    // One whose end never comes is at its point.
    found.extend(open.into_iter().map(|(_, at, a)| (at, at, a)));
    if found.is_empty() {
        return Ok(());
    }
    // The comments, and their replies: a reply names the comment it
    // answers, or another reply of its thread.
    let names: HashMap<&str, usize> = found
        .iter()
        .enumerate()
        .filter_map(|(i, (_, _, a))| a.name.as_deref().map(|n| (n, i)))
        .collect();
    let root = |mut i: usize| {
        for _ in 0..found.len() {
            match found[i].2.parent.as_deref().and_then(|p| names.get(p)) {
                Some(&parent) if parent != i => i = parent,
                _ => break,
            }
        }
        i
    };
    let mut threads: Vec<(usize, Vec<usize>)> = Vec::new();
    for i in 0..found.len() {
        let r = root(i);
        match threads.iter_mut().find(|(t, _)| *t == r) {
            Some((_, replies)) if r != i => replies.push(i),
            Some(_) => {}
            None if r == i => threads.push((i, Vec::new())),
            None => threads.push((r, vec![i])),
        }
    }
    threads.sort_by_key(|(r, _)| *r);
    document.transact("Comments", |edit| {
        for (first, replies) in &threads {
            let (start, end, annotation) = &found[*first];
            let Some((start, end)) = (if start == end {
                word_at(edit.document(), *start)
            } else {
                Some((*start, *end))
            }) else {
                continue;
            };
            let comment = Comment {
                id: NodeId::new(1).expect("not zero"),
                author: annotation.creator.as_deref().unwrap_or_default().into(),
                date: date(annotation.date.as_deref()),
                text: annotation.text.as_str().into(),
                replies: replies
                    .iter()
                    .map(|i| {
                        let reply = &found[*i].2;
                        Reply {
                            author: reply.creator.as_deref().unwrap_or_default().into(),
                            date: date(reply.date.as_deref()),
                            text: reply.text.as_str().into(),
                        }
                    })
                    .collect(),
            };
            edit.add_comment(start, end, comment)?;
        }
        Ok(())
    })?;
    Ok(())
}

/// A date of ODF as the model keeps it: none for the empty date that
/// LibreOffice writes when it has none.
fn date(date: Option<&str>) -> Option<Arc<str>> {
    date.filter(|d| !d.is_empty() && !d.starts_with("0000"))
        .map(Into::into)
}

/// The word at a point of the body: the letters and digits around it, or
/// the character after it, or before it, when there are none.
fn word_at(document: &Document, at: Position) -> Option<(Position, Position)> {
    let paragraph = document.paragraph(at.paragraph).ok()?;
    let text = paragraph.text();
    let offset = at.offset.min(text.len());
    let word = |c: char| c.is_alphanumeric();
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| word(*c))
        .last()
        .map_or(offset, |(i, _)| i);
    let end = offset
        + text[offset..]
            .char_indices()
            .find(|(_, c)| !word(*c))
            .map_or(text.len() - offset, |(i, _)| i);
    let (start, end) = if start < end {
        (start, end)
    } else if let Some(c) = text[offset..].chars().next() {
        (offset, offset + c.len_utf8())
    } else {
        let c = text[..offset].chars().next_back()?;
        (offset - c.len_utf8(), offset)
    };
    Some((
        Position::new(at.paragraph, start),
        Position::new(at.paragraph, end),
    ))
}

/// The annotations to write in the body, by the paragraph and the offset
/// where they go: at the start of the text of each comment, the comment
/// and its replies; at its end, the end of each.
pub(super) fn export(document: &Document) -> HashMap<(NodeId, usize), Vec<odf::Span>> {
    let mut ends: HashMap<(NodeId, usize), Vec<odf::Span>> = HashMap::new();
    let mut starts: HashMap<(NodeId, usize), Vec<odf::Span>> = HashMap::new();
    let span = |field: odf::Field| odf::Span {
        field: Some(field),
        ..odf::Span::default()
    };
    for (n, (id, start, end)) in document.comment_ranges().into_iter().enumerate() {
        let Some(comment) = document.comments().get(id) else {
            continue;
        };
        let name = format!("c{}", n + 1);
        let mut names = vec![name.clone()];
        let here = starts.entry((start.paragraph, start.offset)).or_default();
        here.push(span(odf::Field::Annotation(Box::new(odf::Annotation {
            name: Some(name.clone()),
            parent: None,
            creator: Some(comment.author.to_string()),
            date: comment.date.as_deref().map(str::to_owned),
            text: comment.text.to_string(),
        }))));
        for (k, reply) in comment.replies.iter().enumerate() {
            let reply_name = format!("{name}r{}", k + 1);
            here.push(span(odf::Field::Annotation(Box::new(odf::Annotation {
                name: Some(reply_name.clone()),
                parent: Some(name.clone()),
                creator: Some(reply.author.to_string()),
                date: reply.date.as_deref().map(str::to_owned),
                text: reply.text.to_string(),
            }))));
            names.push(reply_name);
        }
        ends.entry((end.paragraph, end.offset)).or_default().extend(
            names
                .into_iter()
                .map(|name| span(odf::Field::AnnotationEnd { name })),
        );
    }
    // Where one comment ends and another starts, the first ends first.
    for (key, spans) in starts {
        ends.entry(key).or_default().extend(spans);
    }
    ends
}
