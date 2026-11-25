// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colours of the authors of comments and changes (W150, REV-04).
//!
//! As in Pages 15.4, each author of a document has one of ten colours,
//! which the text under their comments, highlights and changes takes.
//! The author of this computer chooses theirs for the document (View >
//! Comments & Changes > Author Color in Pages); it is yellow until then.
//! Every other author, such as those of a file that came from someone
//! else, takes a colour of their own, the first one not taken, in the
//! order of their first comment. A colour chosen is a setting of the
//! document.

use crate::Rgba;
use crate::error::ModelError;
use crate::transaction::Edit;

/// The colour of an author, from the ten of Pages 15.4, in the order of
/// its menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AuthorColor {
    /// Yellow, the colour of the author of this computer until they
    /// choose one.
    #[default]
    Yellow,
    /// Green.
    Green,
    /// Blue.
    Blue,
    /// Pink.
    Pink,
    /// Purple.
    Purple,
    /// Teal.
    Teal,
    /// Orange.
    Orange,
    /// Red.
    Red,
    /// Brown.
    Brown,
    /// Gray blue.
    GrayBlue,
}

impl AuthorColor {
    /// The ten, in the order of the menu.
    pub const ALL: [Self; 10] = [
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Pink,
        Self::Purple,
        Self::Teal,
        Self::Orange,
        Self::Red,
        Self::Brown,
        Self::GrayBlue,
    ];

    /// Its name in files and actions: `yellow`, `gray-blue`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Pink => "pink",
            Self::Purple => "purple",
            Self::Teal => "teal",
            Self::Orange => "orange",
            Self::Red => "red",
            Self::Brown => "brown",
            Self::GrayBlue => "gray-blue",
        }
    }

    /// The colour of that name.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }

    /// The light colour that marks the text, as a highlighter: it
    /// multiplies with the page, so the text stays as dark as it is.
    /// Tints of the palette of GNOME.
    pub fn highlight(self) -> Rgba {
        match self {
            Self::Yellow => Rgba::rgb(255, 232, 115),
            Self::Green => Rgba::rgb(160, 240, 170),
            Self::Blue => Rgba::rgb(170, 205, 245),
            Self::Pink => Rgba::rgb(255, 180, 220),
            Self::Purple => Rgba::rgb(220, 180, 240),
            Self::Teal => Rgba::rgb(150, 230, 225),
            Self::Orange => Rgba::rgb(255, 200, 130),
            Self::Red => Rgba::rgb(255, 165, 160),
            Self::Brown => Rgba::rgb(220, 195, 165),
            Self::GrayBlue => Rgba::rgb(190, 205, 225),
        }
    }

    /// The strong colour of the name of the author and of the marks in
    /// the margin, from the palette of GNOME.
    pub fn ink(self) -> Rgba {
        match self {
            Self::Yellow => Rgba::rgb(229, 165, 10),
            Self::Green => Rgba::rgb(38, 162, 105),
            Self::Blue => Rgba::rgb(28, 113, 216),
            Self::Pink => Rgba::rgb(214, 51, 132),
            Self::Purple => Rgba::rgb(129, 61, 156),
            Self::Teal => Rgba::rgb(33, 144, 164),
            Self::Orange => Rgba::rgb(230, 97, 0),
            Self::Red => Rgba::rgb(192, 28, 40),
            Self::Brown => Rgba::rgb(134, 94, 60),
            Self::GrayBlue => Rgba::rgb(94, 108, 130),
        }
    }
}

/// The colour of every author of a document, for the author of this
/// computer (W150), worked out once.
#[derive(Clone, Debug, Default)]
pub struct AuthorPalette {
    colors: std::collections::HashMap<String, AuthorColor>,
    /// The colours no author chose.
    free: Vec<AuthorColor>,
    /// How many authors took a free colour.
    placed: usize,
}

impl AuthorPalette {
    /// The colour of an author: the one chosen for them, or else a free
    /// one; a name the document does not have takes the next free colour.
    pub fn color(&self, author: &str) -> AuthorColor {
        self.colors
            .get(author)
            .copied()
            .unwrap_or_else(|| free_color(&self.free, self.placed))
    }
}

fn free_color(free: &[AuthorColor], place: usize) -> AuthorColor {
    if free.is_empty() {
        AuthorColor::ALL[place % AuthorColor::ALL.len()]
    } else {
        free[place % free.len()]
    }
}

impl crate::Document {
    /// The colours that authors chose for this document, by name.
    pub fn author_colors(&self) -> &[(String, AuthorColor)] {
        &self.text_settings().author_colors
    }

    /// The colours of the authors, with `local` the name of the author of
    /// this computer, of comments and tracked changes: the one chosen for
    /// each; for the others the colours
    /// no one chose, in their order (yellow first), the local author
    /// first and then the others by name. The order of names, unlike the
    /// identifiers of comments, is the same once the document is saved
    /// and opened again.
    pub fn author_palette(&self, local: &str) -> AuthorPalette {
        self.author_palette_of(local, &[])
    }

    /// The same, when the local author wrote under the names of `former`
    /// before, which take no colour of their own (B-066).
    pub fn author_palette_of(&self, local: &str, former: &[String]) -> AuthorPalette {
        let chosen = self.author_colors();
        let mut colors: std::collections::HashMap<String, AuthorColor> =
            chosen.iter().cloned().collect();
        let free: Vec<AuthorColor> = AuthorColor::ALL
            .into_iter()
            .filter(|c| !chosen.iter().any(|(_, t)| t == c))
            .collect();
        let mut others = std::collections::BTreeSet::new();
        let names = self.comments().iter().flat_map(|comment| {
            std::iter::once(&comment.author).chain(comment.replies.iter().map(|r| &r.author))
        });
        for name in names.chain(self.changes().iter().map(|c| &c.author)) {
            if &**name != local
                && !colors.contains_key(&**name)
                && !former.iter().any(|f| f == &**name)
            {
                others.insert(&**name);
            }
        }
        let mut placed = 0;
        let unchosen = (!colors.contains_key(local)).then_some(local);
        for name in unchosen.into_iter().chain(others) {
            colors.insert(name.to_owned(), free_color(&free, placed));
            placed += 1;
        }
        AuthorPalette {
            colors,
            free,
            placed,
        }
    }

    /// The colour of one author, as [`crate::Document::author_palette`] gives it.
    pub fn author_color(&self, author: &str, local: &str) -> AuthorColor {
        self.author_palette(local).color(author)
    }
}

impl Edit {
    /// Chooses the colour of an author for this document.
    pub fn set_author_color(&mut self, author: &str, color: AuthorColor) -> Result<(), ModelError> {
        let mut colors = self.document().author_colors().to_vec();
        match colors.iter_mut().find(|(name, _)| name == author) {
            Some((_, c)) => *c = color,
            None => colors.push((author.to_owned(), color)),
        }
        let settings = crate::TextSettings {
            author_colors: colors,
            ..self.document().text_settings().clone()
        };
        self.set_text_settings(settings)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{Comment, Document, NodeId, Position};

    fn comment_by(document: &mut Document, author: &str) {
        document
            .transact("Comment", |edit| {
                let id = edit.document().body().get(0).unwrap().id();
                edit.add_comment(
                    Position::new(id, 0),
                    Position::new(id, 1),
                    Comment {
                        id: NodeId::new(1).unwrap(),
                        author: author.into(),
                        date: None,
                        text: "x".into(),
                        replies: Arc::new([]),
                    },
                )
                .map(|_| ())
            })
            .unwrap();
    }

    #[test]
    fn b_066_earlier_names_of_the_local_author_take_no_colour() {
        let mut document = Document::new_word_processing();
        document
            .transact("Typing", |edit| {
                let start = edit.document().start();
                edit.insert_text(start, "one two").map(|_| ())
            })
            .unwrap();
        comment_by(&mut document, "Ana");
        comment_by(&mut document, "Bo");
        // Ana is Ana Souza now: Bo keeps the colour after hers.
        let palette = document.author_palette_of("Ana Souza", &["Ana".to_owned()]);
        assert_eq!(palette.color("Ana Souza"), AuthorColor::Yellow);
        assert_eq!(palette.color("Bo"), AuthorColor::Green);
    }

    #[test]
    fn w150_each_author_has_a_colour_and_the_local_one_chooses_theirs() {
        let mut document = Document::new_word_processing();
        document
            .transact("Typing", |edit| {
                let start = edit.document().start();
                edit.insert_text(start, "one two").map(|_| ())
            })
            .unwrap();
        comment_by(&mut document, "Desk");
        comment_by(&mut document, "Ana");
        comment_by(&mut document, "Bo");
        // The local author is yellow; the others take the colours after
        // it, by name, which saving and opening again does not change.
        assert_eq!(document.author_color("Ana", "Ana"), AuthorColor::Yellow);
        assert_eq!(document.author_color("Bo", "Ana"), AuthorColor::Green);
        assert_eq!(document.author_color("Desk", "Ana"), AuthorColor::Blue);
        // Choosing green for the local author gives the others what is
        // free, and the choice is kept in the document, in its undo.
        document
            .transact("Author Color", |edit| {
                edit.set_author_color("Ana", AuthorColor::Green)
            })
            .unwrap();
        assert_eq!(document.author_color("Ana", "Ana"), AuthorColor::Green);
        assert_eq!(document.author_color("Bo", "Ana"), AuthorColor::Yellow);
        assert_eq!(document.author_color("Desk", "Ana"), AuthorColor::Blue);
        assert_eq!(
            document.author_colors(),
            [("Ana".to_owned(), AuthorColor::Green)]
        );
        // Another user of the file, who chose nothing, does not share the
        // colour Ana chose: they take the first one free.
        assert_eq!(document.author_color("Cy", "Cy"), AuthorColor::Yellow);
        document
            .transact("Author Color", |edit| {
                edit.set_author_color("Bo", AuthorColor::Yellow)
            })
            .unwrap();
        assert_eq!(document.author_color("Cy", "Cy"), AuthorColor::Blue);
        assert_eq!(document.author_color("Bo", "Cy"), AuthorColor::Yellow);
        assert_eq!(document.author_color("Ana", "Cy"), AuthorColor::Green);
        for color in AuthorColor::ALL {
            assert_eq!(AuthorColor::from_key(color.key()), Some(color));
        }
    }
}
