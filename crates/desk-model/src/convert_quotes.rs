// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Converting the quotes already typed into those of the style of smart
//! quotes, as Replace All and Replace in Selection of the substitutions of
//! macOS do for Pages 15.4 (PXV-08).

use crate::{Edit, ModelError, Op, Position, QuoteStyle, Run};

impl Edit {
    /// Makes the quotes between two positions, straight or curly, those of
    /// `style`, keeping their formatting. Fields stay as they are.
    pub fn convert_quotes(
        &mut self,
        start: Position,
        end: Position,
        style: QuoteStyle,
    ) -> Result<(), ModelError> {
        for (id, range) in self.document().paragraph_ranges(start, end)? {
            let paragraph = self.document().paragraph(id)?;
            let mut before = paragraph
                .text()
                .get(..range.start)
                .unwrap_or_default()
                .to_owned();
            let old = paragraph.slice(range.clone())?;
            let runs: Vec<Run> = old
                .iter()
                .map(|run| {
                    if matches!(run.attrs.field, Some(Some(_))) {
                        before.push_str(run.text());
                        return run.clone();
                    }
                    let changed = style.convert(&before, run.text());
                    before.push_str(&changed);
                    Run::with_attrs(&changed, run.style, run.attrs.clone())
                })
                .collect();
            if runs == old {
                continue;
            }
            self.apply(Op::ReplaceText {
                paragraph: id,
                range,
                runs,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Document, Editor, Position, QuoteStyle, Quotes};

    #[test]
    fn pxv_08_converting_quotes_keeps_the_text_around_them() {
        let mut editor = Editor::new(Document::new_word_processing());
        let paragraph = editor.document().start().paragraph;
        editor
            .edit("Typing", |edit| {
                edit.insert_text(Position::new(paragraph, 0), "He said \"it's 'fine'\" twice")
            })
            .unwrap();
        let german = QuoteStyle {
            double: Quotes::DOUBLE[1],
            single: Quotes::SINGLE[1],
        };
        editor
            .edit("Convert Quotes", |edit| {
                let end = Position::new(paragraph, edit.document().paragraph(paragraph)?.len());
                edit.convert_quotes(Position::new(paragraph, 0), end, german)
            })
            .unwrap();
        assert_eq!(
            editor.document().paragraph(paragraph).unwrap().text(),
            "He said „it’s ‚fine‘“ twice"
        );
        editor.undo().unwrap();
        assert_eq!(
            editor.document().paragraph(paragraph).unwrap().text(),
            "He said \"it's 'fine'\" twice"
        );
    }
}
