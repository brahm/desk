// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What both engines share: the test case, the shape to wrap around, the
//! hyphenation and the result format.

use hyphenation::{Hyphenator, Language, Load, Standard};

/// U+00AD SOFT HYPHEN, inserted at each hyphenation point.
pub const SOFT_HYPHEN: char = '\u{ad}';

pub const FONT_FAMILY: &str = "Liberation Serif";
pub const FONT_SIZE: f32 = 11.0;
/// The width of the text column, in points.
pub const COLUMN_WIDTH: f32 = 340.0;
/// The narrowest piece of a line worth filling, as in a page layout app.
pub const MIN_SEGMENT: f32 = 36.0;

pub const TEXT: &str = "O processador de texto precisa quebrar as linhas de um parágrafo \
justificado em volta de uma imagem redonda, colocada no meio da coluna, de modo que o texto \
corra pelos dois lados dela. Cada linha tem uma largura diferente, que depende da altura em que \
a linha cai e da curva da imagem, e a altura da linha só se conhece depois de saber quais \
palavras couberam nela. Quando uma palavra comprida não cabe no fim da linha, a hifenização \
procura um ponto de quebra pelos padrões do português, e o hífen que aparece no fim da linha \
também ocupa espaço. Assim se testam, de uma vez, as larguras variáveis, a justificação de cada \
pedaço de linha, a hifenização e o reposicionamento da linha quando ela fica mais alta do que se \
esperava. Depois da imagem, as linhas voltam a ocupar a coluna inteira, e o parágrafo termina com \
uma linha curta, que não se justifica.";

/// A circle that text wraps around on both sides.
pub struct Circle {
    pub cx: f32,
    pub cy: f32,
    pub r: f32,
    /// The gap kept between the text and the shape.
    pub pad: f32,
}

pub const CIRCLE: Circle = Circle {
    cx: 170.0,
    cy: 120.0,
    r: 58.0,
    pad: 6.0,
};

impl Circle {
    /// The horizontal extent the shape blocks in a band of lines, if any.
    fn blocked(&self, y0: f32, y1: f32) -> Option<(f32, f32)> {
        let r = self.r + self.pad;
        if y1 <= self.cy - r || y0 >= self.cy + r {
            return None;
        }
        let dy = self.cy.clamp(y0, y1) - self.cy;
        let half = (r * r - dy * dy).max(0.0).sqrt();
        Some((self.cx - half, self.cx + half))
    }

    /// The pieces of the column free for a line between two heights,
    /// left to right.
    pub fn segments(&self, y0: f32, y1: f32) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        match self.blocked(y0, y1) {
            None => out.push((0.0, COLUMN_WIDTH)),
            Some((a, b)) => {
                if a > MIN_SEGMENT {
                    out.push((0.0, a));
                }
                if COLUMN_WIDTH - b > MIN_SEGMENT {
                    out.push((b, COLUMN_WIDTH - b));
                }
            }
        }
        out
    }
}

/// Inserts soft hyphens at the hyphenation points of the Portuguese
/// patterns, the TeX ones from hyph-utf8.
pub fn hyphenate(text: &str) -> String {
    let dictionary = Standard::from_embedded(Language::Portuguese).expect("embedded patterns");
    let mut out = String::with_capacity(text.len() * 11 / 10);
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let core: String = word.chars().filter(|c| c.is_alphabetic()).collect();
        if core.chars().count() < 5 {
            out.push_str(word);
            continue;
        }
        let start = word.find(|c: char| c.is_alphabetic()).unwrap_or(0);
        let breaks = dictionary
            .hyphenate(&word[start..start + core.len()])
            .breaks;
        let mut last = 0;
        out.push_str(&word[..start]);
        let body = &word[start..];
        for b in breaks {
            out.push_str(&body[last..b]);
            out.push(SOFT_HYPHEN);
            last = b;
        }
        out.push_str(&body[last..]);
    }
    out
}

/// A font, by its file bytes and its index in a collection.
#[derive(Clone)]
pub struct FontBytes {
    pub data: std::sync::Arc<Vec<u8>>,
    pub index: u32,
}

/// One glyph placed on the page.
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

/// One line, or one piece of a line beside the shape.
pub struct PlacedLine {
    /// The text of the line, without soft hyphens.
    pub text: String,
    /// The free piece of the column the line went into.
    pub x: f32,
    pub width: f32,
    /// The width the glyphs take, the hyphen included.
    pub used: f32,
    pub top: f32,
    pub bottom: f32,
    pub glyphs: Vec<PlacedGlyph>,
    pub hyphenated: bool,
}

pub struct Laid {
    pub font: FontBytes,
    pub lines: Vec<PlacedLine>,
}

/// The hyphen glyph and its advance, from the font itself.
pub fn hyphen_glyph(font: &FontBytes) -> (u32, f32) {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};
    let font = skrifa::FontRef::from_index(&font.data, font.index).expect("font");
    let id = font.charmap().map('-').expect("the font has a hyphen");
    let advance = font
        .glyph_metrics(Size::new(FONT_SIZE), LocationRef::default())
        .advance_width(id)
        .unwrap_or(0.0);
    (id.to_u32(), advance)
}

/// Checks a layout: lines inside their pieces, clear of the shape, and in
/// order down the column.
pub fn check(laid: &Laid) -> Vec<String> {
    let mut problems = Vec::new();
    for (i, line) in laid.lines.iter().enumerate() {
        if line.used > line.width + 0.01 {
            problems.push(format!(
                "line {i} overflows by {:.2}pt: {:?}",
                line.used - line.width,
                line.text
            ));
        }
        if let Some((a, b)) = CIRCLE.blocked(line.top, line.bottom) {
            let (x0, x1) = (line.x, line.x + line.used);
            if x0 < b && x1 > a {
                problems.push(format!("line {i} runs into the shape: {:?}", line.text));
            }
        }
    }
    problems
}
