// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The notes in the Inspector (EST-13, EST-14, EST-15): where they go, how
//! they are numbered, the space between them, and the text and mark of
//! the note at the cursor.
//!
//! The text of a note is a row, as a header is in
//! [`crate::headers_pane`], without the mark it starts with. A mark of
//! its own replaces the number of the note; an empty one gives the number
//! back.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{
    Baseline, CharAttrs, Edit, FIELD_CHARACTER, Field, ModelError, Note, NoteFormat, NoteKind,
    NoteNumbering, NoteSettings, Paragraph, Run,
};

use crate::editing::Format;
use crate::formatting::EditorOf;
use crate::i18n::tr;
use crate::preferences::Units;
use crate::tab_stops::{self, string_list};

/// Whether a run is the mark of a note.
fn is_mark(run: &Run) -> bool {
    matches!(run.attrs.field, Some(Some(Field::NoteMark { .. })))
}

/// A paragraph without the mark of a note it starts with.
fn without_mark(paragraph: &Paragraph) -> Paragraph {
    let runs: Vec<Run> = paragraph
        .runs()
        .skip_while(|r| is_mark(r))
        .cloned()
        .collect();
    Paragraph::from_runs(
        paragraph.id(),
        paragraph.style(),
        paragraph.attrs().clone(),
        runs,
    )
    .unwrap_or_else(|_| paragraph.clone())
}

/// The text of a note, as its row shows it: without its mark.
pub fn text(note: &Note) -> String {
    let paragraphs: Vec<Paragraph> = note.paragraphs.iter().map(without_mark).collect();
    let text = crate::headers_pane::text(&paragraphs);
    text.strip_prefix(' ').unwrap_or(&text).to_owned()
}

/// The paragraphs of a note for the text of its row, after the mark it
/// starts with, with the formatting of its old paragraphs.
pub fn paragraphs(
    edit: &mut Edit,
    old: &Note,
    text: &str,
    file_name: &str,
) -> Result<Vec<Paragraph>, ModelError> {
    let first = old.paragraphs.first();
    let mark = first
        .and_then(|p| p.runs().find(|r| is_mark(r)).cloned())
        .unwrap_or_else(|| {
            Run::with_attrs(
                &FIELD_CHARACTER.to_string(),
                None,
                CharAttrs {
                    field: Some(Some(Field::NoteMark { note: old.id })),
                    baseline: Some(Baseline::Superscript),
                    ..CharAttrs::default()
                },
            )
        });
    let plain: Vec<Paragraph> = old.paragraphs.iter().map(without_mark).collect();
    let mut paragraphs = crate::headers_pane::paragraphs(edit, text, &plain, file_name)?;
    let style = first.map_or_else(
        || edit.document().styles().default_paragraph(),
        Paragraph::style,
    );
    if paragraphs.is_empty() {
        let id = edit.new_id();
        paragraphs.push(Paragraph::new(id, style));
    }
    // The mark, and a space after it as Pages writes.
    let head = &paragraphs[0];
    let mut runs = vec![mark];
    let mut rest = head.runs().cloned().peekable();
    if let Some(run) = rest.peek()
        && !run.text().starts_with(' ')
    {
        runs.push(Run::with_attrs(" ", run.style, run.attrs.clone()));
    }
    runs.extend(rest);
    paragraphs[0] = Paragraph::from_runs(head.id(), head.style(), head.attrs().clone(), runs)?;
    Ok(paragraphs)
}

const KINDS: [NoteKind; 3] = [
    NoteKind::Footnotes,
    NoteKind::DocumentEndnotes,
    NoteKind::SectionEndnotes,
];
const FORMATS: [NoteFormat; 3] = [NoteFormat::Arabic, NoteFormat::Roman, NoteFormat::Symbols];
const NUMBERINGS: [NoteNumbering; 3] = [
    NoteNumbering::Continuous,
    NoteNumbering::Page,
    NoteNumbering::Section,
];

/// The Notes group of the Inspector.
pub struct NotesGroup {
    group: adw::PreferencesGroup,
    kind: adw::ComboRow,
    format: adw::ComboRow,
    numbering: adw::ComboRow,
    gap: adw::SpinRow,
    text: adw::EntryRow,
    mark: adw::EntryRow,
    units: Cell<Units>,
    shown: RefCell<(NoteSettings, Option<Note>)>,
    updating: Cell<bool>,
    editor: EditorOf,
}

impl NotesGroup {
    /// The group for the editor of a window.
    pub fn new(editor: EditorOf, units: Units) -> Rc<Self> {
        let kind = adw::ComboRow::builder()
            .title(tr("Notes"))
            .model(&string_list([
                tr("Footnotes"),
                tr("Endnotes at the End of the Document"),
                tr("Endnotes at the End of Each Section"),
            ]))
            .build();
        let format = adw::ComboRow::builder()
            .title(tr("Format"))
            .model(&string_list([
                "1, 2, 3".to_owned(),
                "i, ii, iii".to_owned(),
                "*, †, ‡".to_owned(),
            ]))
            .build();
        let numbering = adw::ComboRow::builder()
            .title(tr("Numbering"))
            .model(&string_list([
                tr("Continuous"),
                tr("Restart on Each Page"),
                tr("Restart on Each Section"),
            ]))
            .build();
        let gap = adw::SpinRow::with_range(0.0, 1.0, 1.0);
        gap.set_title(&tr("Space Between Notes"));
        let text = adw::EntryRow::builder()
            .title(tr("Note at the Cursor"))
            .show_apply_button(true)
            .build();
        let mark = adw::EntryRow::builder()
            .title(tr("Mark of Its Own"))
            .show_apply_button(true)
            .build();
        let group = adw::PreferencesGroup::builder()
            .title(tr("Footnotes and Endnotes"))
            .description(tr(
                "Insert ▸ Footnote adds a note at the cursor. Its text is edited here.",
            ))
            .build();
        for row in [&kind, &format, &numbering] {
            group.add(row);
        }
        group.add(&gap);
        group.add(&text);
        group.add(&mark);
        let this = Rc::new(Self {
            group,
            kind,
            format,
            numbering,
            gap,
            text,
            mark,
            units: Cell::new(units),
            shown: RefCell::new(Default::default()),
            updating: Cell::new(false),
            editor,
        });
        this.set_units(units);
        this.connect();
        this
    }

    fn connect(self: &Rc<Self>) {
        for row in [&self.kind, &self.format, &self.numbering] {
            let weak = Rc::downgrade(self);
            row.connect_selected_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && !this.updating.get()
                {
                    this.settings_changed();
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.gap.connect_value_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.settings_changed();
            }
        });
        let weak = Rc::downgrade(self);
        self.text.connect_apply(move |row| {
            if let Some(this) = weak.upgrade()
                && let Some(note) = this.shown.borrow().1.as_ref().map(|n| n.id)
                && let Some(editor) = (this.editor)()
            {
                editor.set_note_text(note, &row.text());
            }
        });
        let weak = Rc::downgrade(self);
        self.mark.connect_apply(move |row| {
            if let Some(this) = weak.upgrade()
                && let Some(note) = this.shown.borrow().1.as_ref().map(|n| n.id)
                && let Some(editor) = (this.editor)()
            {
                editor.set_note_mark(note, &row.text());
            }
        });
    }

    /// The group.
    pub fn widget(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    /// Measures the space between notes in other units.
    pub fn set_units(&self, units: Units) {
        self.updating.set(true);
        self.units.set(units);
        tab_stops::set_units(&self.gap, units, 0.0, 144.0);
        self.updating.set(false);
        let (settings, note) = self.shown.borrow().clone();
        self.show_parts(settings, note);
    }

    /// Shows how the notes of the document show, and the note at the
    /// cursor.
    pub fn show(&self, format: &Format) {
        self.show_parts(format.text_settings.notes, format.note.clone());
    }

    fn show_parts(&self, settings: NoteSettings, note: Option<Note>) {
        *self.shown.borrow_mut() = (settings, note.clone());
        self.updating.set(true);
        let select = |row: &adw::ComboRow, index: Option<usize>| {
            let index = index.unwrap_or(0) as u32;
            if row.selected() != index {
                row.set_selected(index);
            }
        };
        select(&self.kind, KINDS.iter().position(|k| *k == settings.kind));
        select(
            &self.format,
            FORMATS.iter().position(|f| *f == settings.format),
        );
        select(
            &self.numbering,
            NUMBERINGS.iter().position(|n| *n == settings.numbering),
        );
        let gap = tab_stops::in_units(settings.gap, self.units.get());
        if (self.gap.value() - gap).abs() > 0.001 {
            self.gap.set_value(gap);
        }
        self.text.set_visible(note.is_some());
        self.mark.set_visible(note.is_some());
        if let Some(note) = &note {
            let value = text(note);
            if self.text.text() != value {
                self.text.set_text(&value);
            }
            let mark = note.mark.as_deref().unwrap_or_default();
            if self.mark.text() != mark {
                self.mark.set_text(mark);
            }
        }
        self.updating.set(false);
    }

    fn settings_changed(&self) {
        let (shown, _) = self.shown.borrow().clone();
        let pick = |row: &adw::ComboRow| row.selected() as usize;
        let settings = NoteSettings {
            kind: KINDS.get(pick(&self.kind)).copied().unwrap_or(shown.kind),
            format: FORMATS
                .get(pick(&self.format))
                .copied()
                .unwrap_or(shown.format),
            numbering: NUMBERINGS
                .get(pick(&self.numbering))
                .copied()
                .unwrap_or(shown.numbering),
            gap: tab_stops::from_units(self.gap.value(), self.units.get()),
        };
        if settings != shown
            && let Some(editor) = (self.editor)()
        {
            editor.change_text_settings("Notes", |text| text.notes = settings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{Document, Editor, ParaAttrs};

    #[test]
    fn est_13_the_text_of_a_note_goes_to_its_row_and_back() {
        let mut editor = Editor::new(Document::new_word_processing());
        let body = editor.document().styles().default_paragraph();
        let mut id = None;
        editor
            .edit("Note", |edit| {
                let note = edit.new_id();
                let paragraph = edit.new_id();
                let mark = Run::with_attrs(
                    &FIELD_CHARACTER.to_string(),
                    None,
                    CharAttrs {
                        field: Some(Some(Field::NoteMark { note })),
                        ..CharAttrs::default()
                    },
                );
                let paragraph = Paragraph::from_runs(
                    paragraph,
                    body,
                    ParaAttrs::default(),
                    [mark, Run::new(" A note.")],
                )?;
                id = Some(note);
                edit.set_note(
                    note,
                    Some(Note {
                        id: note,
                        mark: None,
                        paragraphs: vec![paragraph].into(),
                    }),
                )
            })
            .unwrap();
        let id = id.unwrap();
        let note = editor.document().notes().get(id).unwrap().clone();
        assert_eq!(text(&note), "A note.");
        editor
            .edit("Note", |edit| {
                let paragraphs = paragraphs(edit, &note, "Another ¶ Two", "")?;
                edit.set_note(
                    id,
                    Some(Note {
                        paragraphs: paragraphs.into(),
                        ..note.clone()
                    }),
                )
            })
            .unwrap();
        let changed = editor.document().notes().get(id).unwrap();
        assert_eq!(changed.paragraphs.len(), 2);
        // The note still starts with its mark.
        let first = changed.paragraphs[0].runs().next().unwrap();
        assert!(is_mark(first));
        assert_eq!(changed.paragraphs[0].text(), "\u{fffc} Another");
        assert_eq!(text(changed), "Another ¶ Two");
    }
}
