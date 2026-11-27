// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Mail merge in the editor (W105): merge fields put at the cursor
//! (MAL-07), the source of the document chosen (MAL-04, MAL-05), and the
//! preview, which shows the fields filled with the data of a record and
//! leaves the document as it is (MAL-04).

use std::rc::Rc;

use desk_model::{Document, Editor, Field, MergeSource};
use gtk::gio;
use gtk::prelude::*;

use super::TextEditor;

/// The datum of a merge field of the record previewed, by the name of
/// its field and whether it is a sender field.
pub type MergePreview = Rc<dyn Fn(&str, bool) -> Option<String>>;

/// A merge field with a datum.
fn with_datum(field: &Field, datum: String) -> Option<Field> {
    let Field::Merge {
        name,
        sender,
        single_line,
        shown,
        origin,
    } = field
    else {
        return None;
    };
    (shown.as_deref() != Some(datum.as_str())).then(|| Field::Merge {
        name: name.clone(),
        sender: *sender,
        single_line: *single_line,
        shown: Some(datum.into()),
        origin: origin.clone(),
    })
}

/// A copy of the document with its merge fields as they show: those of
/// a recipient filled by the preview, while it is on, and the sender
/// fields that no card filled by the person's own card (MAL-01); none
/// when nothing fills them.
pub(super) fn filled(
    document: &Document,
    preview: Option<&MergePreview>,
    sender: Option<&MergePreview>,
) -> Option<Document> {
    if preview.is_none() && sender.is_none() {
        return None;
    }
    let mut editor = Editor::new(document.clone());
    editor
        .edit("Preview", |edit| {
            edit.map_fields(&|field| {
                let Field::Merge {
                    name,
                    sender: is_sender,
                    shown,
                    ..
                } = field
                else {
                    return None;
                };
                let datum = match (is_sender, preview) {
                    (false, Some(preview)) => preview(name, false),
                    (true, _) if shown.is_none() => sender.and_then(|s| s(name, true)),
                    _ => None,
                }?;
                with_datum(field, datum)
            })
        })
        .ok()?;
    Some(editor.document().clone())
}

/// The datum of a field of a card, or nothing.
fn card_datum(card: &desk_contacts::vcard::Card, name: &str) -> String {
    card.get(name).unwrap_or_default()
}

impl TextEditor {
    /// Puts a merge field at the cursor, of a recipient or of the sender
    /// (MAL-07).
    pub fn insert_merge_field(&self, name: &str, sender: bool) {
        self.insert_field(Field::Merge {
            name: name.into(),
            sender,
            single_line: false,
            shown: None,
            origin: None,
        });
    }

    /// Chooses where the data of the merge fields come from, or none
    /// (MAL-04): Populate With.
    pub fn set_merge_source(&self, source: Option<MergeSource>) {
        self.format_edit("Mail Merge Source", move |edit| {
            let mut settings = edit.document().text_settings().clone();
            settings.merge_source = source;
            edit.set_text_settings(settings)
        });
    }

    /// Shows the merge fields filled with the data of a record, or as
    /// they are, without changing the document (MAL-04).
    pub fn set_merge_preview(&self, preview: Option<MergePreview>) {
        let was = self.state.borrow().merge_preview.is_some();
        if !was && preview.is_none() {
            return;
        }
        // The preview fills the document, not a header or note being
        // edited on its own.
        if self.story().is_some() {
            self.leave_story(None);
        }
        self.state.borrow_mut().merge_preview = preview;
        self.lay_out_again();
    }

    /// Whether the preview of a mail merge shows; an edit ends it.
    pub fn merge_previewing(&self) -> bool {
        self.state.borrow().merge_preview.is_some()
    }

    /// Fills the sender fields that no card filled with the person's own
    /// card, as the document shows them and without changing it, as
    /// Pages '09 did when a document opened (MAL-01). It asks the
    /// contacts of the desktop once, if the document has sender fields.
    pub fn fill_sender_fields(&self) {
        {
            let mut state = self.state.borrow_mut();
            if state.sender_asked || !has_sender_fields(state.editor.document()) {
                return;
            }
            state.sender_asked = true;
        }
        let weak = self.downgrade();
        crate::desktop_contacts::with_own_card(move |card| {
            let (Some(this), Some(card)) = (weak.upgrade(), card) else {
                return;
            };
            let sender: MergePreview = Rc::new(move |name, _| Some(card_datum(&card, name)));
            this.state.borrow_mut().merge_sender = Some(sender);
            this.lay_out_again();
        });
    }

    /// Fills the merge fields with a card dropped on one of them (MAL-02,
    /// MAL-03): on a sender field, every sender field takes the card; on
    /// a recipient field, every recipient field does, and another card
    /// replaces its data. The fields stay fields, and Undo takes the
    /// data away.
    pub fn fill_with_card(&self, card: &desk_contacts::vcard::Card, sender: bool) {
        let card = card.clone();
        self.format_edit("Merge Contact", move |edit| {
            edit.map_fields(&|field| match field {
                Field::Merge {
                    name, sender: s, ..
                } if *s == sender => with_datum(field, card_datum(&card, name)),
                _ => None,
            })
        });
    }

    /// Whether the merge field under a point of the view is a sender
    /// field; none when there is no merge field there.
    pub(super) fn merge_field_at(&self, x: f64, y: f64) -> Option<bool> {
        let spot = self.spot_at(x, y)?;
        let state = self.state.borrow();
        let paragraph = state.editor.document().body().get(spot.paragraph)?;
        let mut offset = 0;
        for run in paragraph.runs() {
            let end = offset + run.len();
            if let Some(Some(Field::Merge { sender, .. })) = &run.attrs.field
                && offset <= spot.offset
                && spot.offset <= end
            {
                return Some(*sender);
            }
            offset = end;
        }
        None
    }

    /// Takes a vCard file dropped on a merge field: its first card fills
    /// the fields. Whether it was one.
    pub(super) fn drop_card(&self, file: &gio::File, x: f64, y: f64) -> bool {
        let is_vcard = file
            .basename()
            .and_then(|n| n.extension().map(|e| e.eq_ignore_ascii_case("vcf")))
            .unwrap_or(false);
        if !is_vcard {
            return false;
        }
        let Some(sender) = self.merge_field_at(x, y) else {
            return false;
        };
        let Ok((bytes, _)) = file.load_contents(gio::Cancellable::NONE) else {
            return false;
        };
        if bytes.len() > desk_contacts::MAX_SOURCE_BYTES {
            return false;
        }
        let text = desk_contacts::decode(&bytes);
        match desk_contacts::vcard::parse(&text).first() {
            Some(card) => {
                self.fill_with_card(card, sender);
                true
            }
            None => false,
        }
    }
}

/// Whether a document has sender fields, in its body or its headers and
/// footers.
fn has_sender_fields(document: &Document) -> bool {
    let sender = |p: &desk_model::Paragraph| {
        p.runs()
            .any(|r| matches!(r.attrs.field, Some(Some(Field::Merge { sender: true, .. }))))
    };
    document.body().iter().any(sender)
        || document
            .all_headers_footers()
            .iter()
            .any(|(_, h)| h.paragraphs().any(sender))
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::FIELD_CHARACTER;

    #[test]
    fn mal_01_the_own_card_fills_the_sender_fields_that_no_card_filled() {
        let mut editor = Editor::new(Document::new_word_processing());
        editor
            .edit("Fields", |edit| {
                let mut at = edit.document().start();
                for (shown, sender) in [(None, true), (Some("Desk"), true), (None, false)] {
                    let end = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
                    edit.set_character_attrs(
                        at,
                        end,
                        &desk_model::CharAttrs {
                            field: Some(Some(Field::Merge {
                                name: "Company".into(),
                                sender,
                                single_line: false,
                                shown: shown.map(Into::into),
                                origin: None,
                            })),
                            ..desk_model::CharAttrs::default()
                        },
                    )?;
                    at = end;
                }
                Ok(())
            })
            .unwrap();
        assert!(has_sender_fields(editor.document()));
        let own: MergePreview = Rc::new(|_, _| Some("Own".into()));
        let shown = filled(editor.document(), None, Some(&own)).unwrap();
        let texts: Vec<String> = shown
            .body()
            .get(0)
            .unwrap()
            .runs()
            .filter_map(|r| r.attrs.field.clone().flatten())
            .map(|f| f.text(1, 1))
            .collect();
        assert_eq!(texts, ["Own", "Desk", "Company"]);
        assert!(filled(editor.document(), None, None).is_none());
    }
}
