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

/// A copy of the document with its merge fields filled by the preview:
/// those of a recipient, and the sender fields that no card filled;
/// none when they cannot be filled.
pub(super) fn filled(document: &Document, preview: &MergePreview) -> Option<Document> {
    let mut editor = Editor::new(document.clone());
    editor
        .edit("Preview", |edit| {
            edit.map_fields(&|field| {
                let Field::Merge {
                    name,
                    sender,
                    shown,
                    ..
                } = field
                else {
                    return None;
                };
                if *sender && shown.is_some() {
                    return None;
                }
                with_datum(field, preview(name, *sender)?)
            })
        })
        .ok()?;
    Some(editor.document().clone())
}

/// A merge field without a datum, showing its name.
fn without_datum(field: &Field) -> Option<Field> {
    match field {
        Field::Merge {
            name,
            sender,
            single_line,
            shown: Some(_),
            origin,
        } => Some(Field::Merge {
            name: name.clone(),
            sender: *sender,
            single_line: *single_line,
            shown: None,
            origin: origin.clone(),
        }),
        _ => None,
    }
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

    /// Fills the sender fields with the person's own card, as they ask
    /// (MAL-01): the card of their name among the contacts of the
    /// desktop. Pages '09 did it when a document opened; Desk asks the
    /// desktop only when the person does, since a document could show
    /// the card where they would not see it. Whether a card was found
    /// comes back to `done`.
    pub fn fill_sender_with_own_card(&self, done: impl FnOnce(bool) + 'static) {
        let weak = self.downgrade();
        crate::desktop_contacts::with_cards(move |_| {
            let (Some(this), Some(card)) =
                (weak.upgrade(), crate::desktop_contacts::own_card_now())
            else {
                done(false);
                return;
            };
            this.fill_with_card(&card, true);
            done(true);
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
                // A field the card has no datum for shows its name again.
                Field::Merge {
                    name, sender: s, ..
                } if *s == sender => match card.get(name).filter(|d| !d.trim().is_empty()) {
                    Some(datum) => with_datum(field, datum),
                    None => without_datum(field),
                },
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
        let mut after = None;
        for run in paragraph.runs() {
            let end = offset + run.len();
            if let Some(Some(Field::Merge { sender, .. })) = &run.attrs.field {
                // The field that starts at the point, before one that
                // ends there.
                if spot.offset == offset {
                    return Some(*sender);
                }
                if spot.offset == end {
                    after = Some(*sender);
                }
            }
            offset = end;
        }
        after
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
        // The size first, so that a file too large is not read.
        let size = file
            .query_info(
                gio::FILE_ATTRIBUTE_STANDARD_SIZE,
                gio::FileQueryInfoFlags::NONE,
                gio::Cancellable::NONE,
            )
            .map(|info| info.size());
        if !size.is_ok_and(|s| (0..=desk_contacts::MAX_SOURCE_BYTES as i64).contains(&s)) {
            return false;
        }
        let Ok((bytes, _)) = file.load_contents(gio::Cancellable::NONE) else {
            return false;
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::FIELD_CHARACTER;

    #[test]
    fn mal_01_mal_04_the_preview_fills_the_sender_fields_that_no_card_filled() {
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
        let preview: MergePreview =
            Rc::new(|_, sender| Some(if sender { "Own" } else { "Ana" }.into()));
        let shown = filled(editor.document(), &preview).unwrap();
        let texts: Vec<String> = shown
            .body()
            .get(0)
            .unwrap()
            .runs()
            .filter_map(|r| r.attrs.field.clone().flatten())
            .map(|f| f.text(1, 1))
            .collect();
        assert_eq!(texts, ["Own", "Desk", "Ana"]);
        let named = without_datum(&Field::Merge {
            name: "Phone".into(),
            sender: false,
            single_line: false,
            shown: Some("1".into()),
            origin: None,
        })
        .unwrap();
        assert_eq!(named.text(1, 1), "Phone");
    }
}
