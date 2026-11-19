// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The language and region of a document, as File > Advanced > Language &
//! Region sets them in Pages 15.4 (PXV-09). They are the language of the
//! default paragraph style, which the text takes for its dictionaries and
//! the tables and date fields take for how they write numbers and dates.

use crate::{Document, Edit, Language, ModelError, Op, Style};

impl Document {
    /// The language and region of the document, as a BCP 47 tag such as
    /// `pt-BR`; undetermined, `und`, when they follow the system.
    pub fn language(&self) -> Language {
        let styles = self.styles();
        styles
            .paragraph(styles.default_paragraph())
            .and_then(|style| style.character.language.clone())
            .unwrap_or_else(Language::undetermined)
    }
}

impl Edit {
    /// Sets the language and region of the document; the undetermined
    /// language makes them follow the system again.
    pub fn set_document_language(&mut self, language: Language) -> Result<(), ModelError> {
        let styles = self.document().styles();
        let Some(mut style) = styles.paragraph(styles.default_paragraph()).cloned() else {
            return Ok(());
        };
        let language = (language != Language::undetermined()).then_some(language);
        if style.character.language == language {
            return Ok(());
        }
        style.character.language = language;
        self.apply(Op::PutStyle {
            style: Style::from(style),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{Document, Editor, Language};

    #[test]
    fn pxv_09_the_document_keeps_its_language_and_region() {
        let mut editor = Editor::new(Document::new_word_processing());
        assert_eq!(editor.document().language(), Language::undetermined());
        editor
            .edit("Language & Region", |edit| {
                edit.set_document_language(Language::new("en-BR"))
            })
            .unwrap();
        assert_eq!(editor.document().language(), Language::new("en-BR"));
        editor.undo().unwrap();
        assert_eq!(editor.document().language(), Language::undetermined());
    }
}
