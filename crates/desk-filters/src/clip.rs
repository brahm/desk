// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Objects on the clipboard between the apps of Desk (IOP-27 to IOP-29,
//! IOP-31, IOP-33): a page layout document of one page, written as ODT,
//! with the objects copied on it, their pictures, movies and sounds, and
//! the styles of their text. Any app of Desk reads it back and pastes
//! the objects, a whole slide or page among them, with their text styled
//! as it was.
//!
//! Text copied with its styles goes the same way (IOP-17, IOP-25): a
//! document of the text alone, which another app of Desk pastes with its
//! styles, or with their look only where it keeps no named styles. Other
//! programs get its look as HTML (TXT-04).

use desk_model::{Anchor, Document, Editor, Fragment, Object, PageSetup};

use crate::odt::{FilterError, read_odt, write_odt};

/// The media type of objects that an app of Desk puts on the clipboard.
pub const OBJECTS_TYPE: &str = "application/x-desk-objects";

/// Objects read from the clipboard, with the document whose styles their
/// text names.
#[derive(Clone, Debug)]
pub struct ObjectsClip {
    document: Document,
}

impl ObjectsClip {
    /// The objects, from back to front, each fixed on the page as it was
    /// on its own.
    pub fn objects(&self) -> Vec<Object> {
        self.document.objects().iter().cloned().collect()
    }

    /// The document they came in, whose styles their text names, for
    /// [`desk_model::Edit::adopt_object_styles`].
    pub fn document(&self) -> &Document {
        &self.document
    }
}

/// The bytes that put objects of a document on the clipboard: those
/// fixed on a page or in the text, which go on the page where they were,
/// on paper of the size of the document's.
pub fn write_objects(from: &Document, objects: &[Object]) -> Result<Vec<u8>, FilterError> {
    let mut editor = Editor::new(Document::new_page_layout());
    let setup = PageSetup {
        layout_pages: Some(1),
        ..*from.page_setup()
    };
    editor.edit("Copy", |edit| {
        edit.set_page_setup(setup)?;
        let objects: Vec<Object> = objects
            .iter()
            .map(|o| Object {
                anchor: Anchor::Page(0),
                ..o.clone()
            })
            .collect();
        let objects = edit.adopt_object_styles(from, &objects)?;
        edit.add_copies(&objects)?;
        Ok(())
    })?;
    write_odt(editor.document())
}

/// The objects that an app of Desk put on the clipboard.
pub fn read_objects(bytes: &[u8]) -> Result<ObjectsClip, FilterError> {
    Ok(ObjectsClip {
        document: read_odt(bytes)?,
    })
}

/// The media type of text with its styles that an app of Desk puts on
/// the clipboard.
pub const TEXT_TYPE: &str = "application/x-desk-text";

/// The bytes that put text copied with its styles on the clipboard: a
/// document of the text alone, with the styles it names.
pub fn write_text(fragment: &Fragment) -> Result<Vec<u8>, FilterError> {
    let mut editor = Editor::new(Document::new_word_processing());
    let start = editor.document().start();
    editor.edit("Copy", |edit| {
        edit.paste_fragment(start, fragment).map(drop)
    })?;
    write_odt(editor.document())
}

/// The media types of formatted text that other programs put on the
/// clipboard and Desk reads, the better first: the ODF document that
/// LibreOffice puts there, and HTML (TXT-04).
pub const FOREIGN_TEXT_TYPES: [&str; 2] = [
    "application/x-openoffice-embed-source-xml;windows_formatname=\"Star Embed Source (XML)\"",
    crate::html::HTML_TYPE,
];

/// What puts text copied with its styles on the clipboard for other
/// programs, as media types and their bytes, the better first: HTML with
/// the look of the styles, which word processors, web browsers and mail
/// programs paste (TXT-04).
///
/// The ODF document that LibreOffice puts there stays out: LibreOffice
/// pastes one that comes without RTF beside it as an embedded object in
/// its spreadsheets and presentations, where HTML goes in as text.
pub fn write_foreign_text(
    fragment: &Fragment,
) -> Result<Vec<(&'static str, Vec<u8>)>, FilterError> {
    Ok(vec![(
        crate::html::HTML_TYPE,
        crate::html::write_html(fragment).into_bytes(),
    )])
}

/// The formatted text that another program put on the clipboard as
/// `media_type`, one of [`FOREIGN_TEXT_TYPES`] (TXT-04).
pub fn read_foreign_text(media_type: &str, bytes: &[u8]) -> Result<Fragment, FilterError> {
    if media_type == crate::html::HTML_TYPE {
        // UTF-8, or UTF-16 as some programs put it, with its byte order mark.
        let text = match bytes {
            [0xff, 0xfe, rest @ ..] => String::from_utf16_lossy(
                &rest
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&b| u16::from_le_bytes(b))
                    .collect::<Vec<_>>(),
            ),
            _ => String::from_utf8_lossy(bytes).into_owned(),
        };
        return crate::html::read_html(&text);
    }
    read_text(bytes)
}

/// The text with its styles that an app of Desk put on the clipboard.
pub fn read_text(bytes: &[u8]) -> Result<Fragment, FilterError> {
    let document = read_odt(bytes)?;
    Ok(document.copy_fragment(document.start(), document.end())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{Alignment, FontWeight, Length, ParagraphStyle, StyleFamily};

    #[test]
    fn iop_17_iop_25_text_goes_between_apps_with_its_styles() {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Setup", |edit| {
                edit.insert_text(start, "Título\ntexto")?;
                let id = edit.new_id();
                let mut style = ParagraphStyle::new(id, "Heading");
                style.paragraph.alignment = Some(Alignment::Center);
                style.character.weight = Some(FontWeight::BOLD);
                style.character.font_size = Some(Length::from_points(18));
                edit.apply(desk_model::Op::PutStyle {
                    style: style.into(),
                })?;
                let at = edit.document().start();
                edit.set_paragraph_style(at, at, id)
            })
            .unwrap();
        let document = editor.document();
        let fragment = document
            .copy_fragment(document.start(), document.end())
            .unwrap();
        let read = read_text(&write_text(&fragment).unwrap()).unwrap();
        assert_eq!(read.text(), "Título\ntexto");
        assert_eq!(&*read.paragraphs[0].style.name, "Heading");

        // Pasted in another document, it brings its style along.
        let mut target = Editor::new(Document::new_word_processing());
        let at = target.document().start();
        target
            .edit("Paste", |edit| edit.paste_fragment(at, &read).map(drop))
            .unwrap();
        let styles = target.document().styles();
        let heading = styles.by_name(StyleFamily::Paragraph, "Heading").unwrap();
        let first = target.document().body().get(0).unwrap();
        assert_eq!(first.style(), heading.id());
        let format = styles.resolve_character(first.style(), None, &Default::default());
        assert!(format.weight.is_bold());
    }

    #[test]
    fn txt_04_text_copied_goes_to_other_programs_with_its_formatting() {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Setup", |edit| {
                edit.insert_text(start, "Título\num texto")?;
                let id = edit.new_id();
                let mut style = ParagraphStyle::new(id, "Heading");
                style.paragraph.alignment = Some(Alignment::Center);
                style.character.weight = Some(FontWeight::BOLD);
                style.character.font_size = Some(Length::from_points(18));
                edit.apply(desk_model::Op::PutStyle {
                    style: style.into(),
                })?;
                let at = edit.document().start();
                edit.set_paragraph_style(at, at, id)?;
                let italic = desk_model::CharAttrs {
                    italic: Some(true),
                    ..Default::default()
                };
                let second = edit.document().body().get(1).unwrap().id();
                edit.set_character_attrs(
                    desk_model::Position::new(second, 0),
                    desk_model::Position::new(second, 2),
                    &italic,
                )
            })
            .unwrap();
        let document = editor.document();
        let fragment = document
            .copy_fragment(document.start(), document.end())
            .unwrap();
        let flavors = write_foreign_text(&fragment).unwrap();
        let types: Vec<&str> = flavors.iter().map(|(t, _)| *t).collect();
        assert_eq!(types, [crate::html::HTML_TYPE]);
        // Each one reads back as the text with its look: what Desk reads
        // of other programs is what it gives them.
        for (media_type, bytes) in &flavors {
            let read = read_foreign_text(media_type, bytes).unwrap();
            assert_eq!(read.text(), "Título\num texto", "{media_type}");
            let mut target = Editor::new(Document::new_word_processing());
            let at = target.document().start();
            target
                .edit("Paste", |edit| edit.paste_fragment(at, &read).map(drop))
                .unwrap();
            let target = target.document();
            let look = |paragraph: usize, offset: usize| {
                target
                    .character_formats_at(desk_model::Position::new(
                        target.body().get(paragraph).unwrap().id(),
                        offset,
                    ))
                    .unwrap()
                    .0
            };
            assert!(look(0, 1).weight.is_bold(), "{media_type}");
            assert_eq!(
                look(0, 1).font_size,
                Length::from_points(18),
                "{media_type}"
            );
            assert!(look(1, 1).italic, "{media_type}");
            assert!(!look(1, 4).italic, "{media_type}");
            let first = target.body().get(0).unwrap();
            let first = target
                .styles()
                .resolve_paragraph(first.style(), first.attrs());
            assert_eq!(first.alignment, Alignment::Center, "{media_type}");
        }
    }
}
