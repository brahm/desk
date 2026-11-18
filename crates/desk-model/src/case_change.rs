// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Changing the case of the characters themselves, as Edit >
//! Transformations does in Pages 15.4 (W054): unlike capitalization
//! (FMT-07), which only changes how the text is drawn, these retype it.

use crate::{Edit, ModelError, Op, Position, Run};

/// A change of the case of the characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseChange {
    /// Make Upper Case: every letter in upper case.
    Upper,
    /// Make Lower Case: every letter in lower case.
    Lower,
    /// Capitalize: the first letter of every word in upper case and the
    /// others in lower case, a word being what lies between spaces, tabs
    /// and line breaks, as in macOS.
    Capitalize,
}

impl CaseChange {
    /// The text changed, given whether the character before it ends a
    /// word; and whether its last character ends one.
    pub fn apply(self, text: &str, after_space: bool) -> (String, bool) {
        let mut out = String::with_capacity(text.len());
        let mut word_start = after_space;
        for c in text.chars() {
            match self {
                Self::Upper => out.extend(c.to_uppercase()),
                Self::Lower => out.extend(c.to_lowercase()),
                Self::Capitalize if word_start => out.extend(c.to_uppercase()),
                Self::Capitalize => out.extend(c.to_lowercase()),
            }
            word_start = c.is_whitespace();
        }
        (out, word_start)
    }
}

impl Edit {
    /// Changes the case of the text between two positions, keeping its
    /// formatting, and gives the position where the changed text now ends,
    /// which moves when a letter changes length (ß becomes SS). Fields,
    /// such as page numbers and the marks of notes, stay as they are.
    pub fn change_case(
        &mut self,
        start: Position,
        end: Position,
        case: CaseChange,
    ) -> Result<Position, ModelError> {
        let mut end = end;
        for (id, range) in self.document().paragraph_ranges(start, end)? {
            let paragraph = self.document().paragraph(id)?;
            let text = paragraph.text();
            let mut after_space = text
                .get(..range.start)
                .and_then(|t| t.chars().next_back())
                .is_none_or(char::is_whitespace);
            let old = paragraph.slice(range.clone())?;
            let runs: Vec<Run> = old
                .iter()
                .map(|run| {
                    if matches!(run.attrs.field, Some(Some(_))) {
                        after_space = false;
                        return run.clone();
                    }
                    let (changed, space) = case.apply(run.text(), after_space);
                    after_space = space;
                    Run::with_attrs(&changed, run.style, run.attrs.clone())
                })
                .collect();
            if runs == old {
                continue;
            }
            let length: usize = runs.iter().map(Run::len).sum();
            if id == end.paragraph {
                end.offset = range.start + length;
            }
            self.apply(Op::ReplaceText {
                paragraph: id,
                range,
                runs,
            })?;
        }
        Ok(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CharAttrs, Document, Editor, FontWeight};

    #[test]
    fn pxv_05_the_case_of_the_characters_changes() {
        let (text, end) = CaseChange::Upper.apply("straße", true);
        assert_eq!(text, "STRASSE");
        assert!(!end);
        assert_eq!(CaseChange::Lower.apply("São PAULO", true).0, "são paulo");
        assert_eq!(
            CaseChange::Capitalize
                .apply("the qUICK brown\tfox's den", true)
                .0,
            "The Quick Brown\tFox's Den"
        );
        // Inside a word, the first letter stays small.
        assert_eq!(CaseChange::Capitalize.apply("ORLD", false).0, "orld");
    }

    #[test]
    fn pxv_05_changing_case_keeps_the_formatting_and_moves_the_end() {
        let mut editor = Editor::new(Document::new_word_processing());
        let paragraph = editor.document().start().paragraph;
        let bold = CharAttrs {
            weight: Some(FontWeight::BOLD),
            ..CharAttrs::default()
        };
        editor
            .edit("Typing", |edit| {
                edit.insert_text(Position::new(paragraph, 0), "große Straße")
            })
            .unwrap();
        editor
            .edit("Bold", |edit| {
                edit.set_character_attrs(
                    Position::new(paragraph, 7),
                    Position::new(paragraph, 14),
                    &bold,
                )
            })
            .unwrap();
        let end = editor
            .edit("Make Upper Case", |edit| {
                edit.change_case(
                    Position::new(paragraph, 0),
                    Position::new(paragraph, 14),
                    CaseChange::Upper,
                )
            })
            .unwrap();
        let p = editor.document().paragraph(paragraph).unwrap();
        assert_eq!(p.text(), "GROSSE STRASSE");
        assert_eq!(end, Position::new(paragraph, 14));
        let runs: Vec<&Run> = p.runs().collect();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text(), "GROSSE ");
        assert_eq!(runs[1].text(), "STRASSE");
        assert_eq!(runs[1].attrs.weight, Some(FontWeight::BOLD));
        editor.undo().unwrap();
        assert_eq!(
            editor.document().paragraph(paragraph).unwrap().text(),
            "große Straße"
        );
    }
}
