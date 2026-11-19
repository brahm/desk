// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The styles of drop caps of a document, as the Drop Cap Styles of Pages
//! 15.4 keeps them (PXV-06): six of them in a new document, the looks that
//! the user saves after them, and the commands that apply, redefine and
//! delete one. A style has no name: Pages shows each as a picture.
//!
//! A style sets the drop cap of a paragraph and the weight and color of
//! its first characters, which are those of the letters; the family and
//! the size stay those of the text.

use crate::error::ModelError;
use crate::format::{CharAttrs, DropCap, DropCapShape, FontWeight, ParaAttrs};
use crate::text::Position;
use crate::transaction::Edit;
use crate::units::Rgba;

/// A look of drop caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DropCapStyle {
    /// The drop cap.
    pub cap: DropCap,
    /// The weight of the letters.
    pub weight: FontWeight,
    /// The color of the letters.
    pub color: Rgba,
}

impl DropCapStyle {
    /// The six styles of Pages 15.4, in the order of its menu: plain;
    /// bold and raised a line, two lines tall; bold, four lines tall,
    /// with the text following the letter; bold and raised; bold, raised
    /// and half out in the margin; and white on a black square.
    pub fn library() -> [Self; 6] {
        let cap = |lines: u8, raised: u8| DropCap {
            lines,
            raised,
            ..DropCap::default()
        };
        let bold = |cap: DropCap| Self {
            cap,
            weight: FontWeight::BOLD,
            color: Rgba::BLACK,
        };
        [
            Self {
                cap: cap(3, 0),
                weight: FontWeight::NORMAL,
                color: Rgba::BLACK,
            },
            bold(cap(2, 1)),
            bold(DropCap {
                follow_shape: true,
                ..cap(4, 0)
            }),
            bold(cap(3, 1)),
            bold(DropCap {
                outdent: 50,
                ..cap(3, 1)
            }),
            Self {
                cap: DropCap {
                    background: Some(DropCapShape {
                        fill: Rgba::BLACK,
                        ..DropCapShape::default()
                    }),
                    ..cap(3, 0)
                },
                weight: FontWeight::MEDIUM,
                color: Rgba::WHITE,
            },
        ]
    }

    /// The formatting it gives the letters.
    pub fn letters(&self) -> CharAttrs {
        CharAttrs {
            weight: Some(self.weight),
            color: Some(self.color),
            ..CharAttrs::default()
        }
    }
}

impl crate::Document {
    /// The styles of drop caps, in the order of their menu.
    pub fn drop_cap_styles(&self) -> &[DropCapStyle] {
        &self.text_settings().drop_cap_styles
    }

    /// The look of the drop cap of a paragraph, as a style would keep it:
    /// its drop cap and the weight and color of its first character.
    pub fn drop_cap_style_at(&self, paragraph: crate::NodeId) -> Option<DropCapStyle> {
        let found = self.paragraph(paragraph).ok()?;
        let format = self
            .styles()
            .resolve_paragraph(found.style(), found.attrs());
        let cap = format.drop_cap?;
        let (style, attrs) = found.format_at(0);
        let letters = self
            .styles()
            .resolve_character(found.style(), style, &attrs);
        Some(DropCapStyle {
            cap,
            weight: letters.weight,
            color: letters.color,
        })
    }
}

impl Edit {
    /// Sets the styles of drop caps.
    pub fn set_drop_cap_styles(&mut self, styles: Vec<DropCapStyle>) -> Result<(), ModelError> {
        let settings = crate::TextSettings {
            drop_cap_styles: styles,
            ..self.document().text_settings().clone()
        };
        self.set_text_settings(settings)
    }

    /// Gives the paragraphs between two positions the drop cap of a style,
    /// and their first characters its weight and color.
    pub fn apply_drop_cap_style(
        &mut self,
        start: Position,
        end: Position,
        style: &DropCapStyle,
    ) -> Result<(), ModelError> {
        let paragraphs = self.document().paragraph_ranges(start, end)?;
        for (id, _) in paragraphs {
            let at = Position::new(id, 0);
            self.set_paragraph_attrs(
                at,
                at,
                &ParaAttrs {
                    drop_cap: Some(Some(style.cap)),
                    ..ParaAttrs::default()
                },
            )?;
            let text = self.document().paragraph(id)?.text();
            let end = text
                .char_indices()
                .nth(usize::from(style.cap.characters.max(1)))
                .map_or(text.len(), |(i, _)| i);
            if end > 0 {
                self.set_character_attrs(at, Position::new(id, end), &style.letters())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::DropCapStyle;
    use crate::{Document, Editor, FontWeight, Position, Rgba};

    #[test]
    fn pxv_06_a_style_gives_a_paragraph_its_drop_cap_and_its_letters() {
        let mut editor = Editor::new(Document::new_word_processing());
        assert_eq!(editor.document().drop_cap_styles(), DropCapStyle::library());
        let id = editor.document().body().get(0).unwrap().id();
        let at = Position::new(id, 0);
        editor
            .edit("Typing", |edit| {
                edit.insert_text(at, "Once upon a time").map(drop)
            })
            .unwrap();
        // White on a black square.
        let square = DropCapStyle::library()[5];
        editor
            .edit("Drop Cap Style", |edit| {
                edit.apply_drop_cap_style(at, at, &square)
            })
            .unwrap();
        let document = editor.document();
        assert_eq!(document.drop_cap_style_at(id), Some(square));
        // Only the first character takes its weight and color.
        let paragraph = document.paragraph(id).unwrap();
        let (_, second) = paragraph.format_at(2);
        assert_eq!(second.weight, None);
        // Plain again: black and regular.
        let plain = DropCapStyle::library()[0];
        editor
            .edit("Drop Cap Style", |edit| {
                edit.apply_drop_cap_style(at, at, &plain)
            })
            .unwrap();
        let style = editor.document().drop_cap_style_at(id).unwrap();
        assert_eq!(style, plain);
        assert_eq!(
            (style.weight, style.color),
            (FontWeight::NORMAL, Rgba::BLACK)
        );
        // A style saved after the others, which undo takes away.
        editor
            .edit("New Drop Cap Style", |edit| {
                let mut styles = edit.document().drop_cap_styles().to_vec();
                styles.push(square);
                edit.set_drop_cap_styles(styles)
            })
            .unwrap();
        assert_eq!(editor.document().drop_cap_styles().len(), 7);
        editor.undo().unwrap();
        assert_eq!(editor.document().drop_cap_styles().len(), 6);
    }
}
