// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Going from page to page, and from one section, link, bookmark or
//! paragraph of a style to the next, as the arrows at the bottom of the
//! window do (INT-19).

use desk_model::{Document, Field};

use super::{Selection, Spot, TextEditor};

/// What the arrows go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigate {
    /// The next page.
    Page,
    /// The start of the next section.
    Section,
    /// The next link.
    Link,
    /// The next bookmark.
    Bookmark,
    /// The next paragraph of the style of the one with the cursor.
    Style,
}

impl Navigate {
    /// Every kind, in the order of the menu.
    pub const ALL: [Self; 5] = [
        Self::Page,
        Self::Section,
        Self::Link,
        Self::Bookmark,
        Self::Style,
    ];

    /// The name of the kind in the state of the action.
    pub fn name(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Section => "section",
            Self::Link => "link",
            Self::Bookmark => "bookmark",
            Self::Style => "style",
        }
    }

    /// The kind with a name, or pages.
    pub fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|k| k.name() == name)
            .unwrap_or(Self::Page)
    }
}

/// The spots where a kind of place starts, in order.
fn places(document: &Document, kind: Navigate, here: usize) -> Vec<Spot> {
    let body = document.body();
    match kind {
        Navigate::Page => Vec::new(),
        Navigate::Section => document
            .section_ranges()
            .into_iter()
            .map(|r| Spot::new(r.start, 0))
            .collect(),
        Navigate::Style => {
            let Some(style) = body.get(here).map(desk_model::Paragraph::style) else {
                return Vec::new();
            };
            body.iter()
                .enumerate()
                .filter(|(_, p)| p.style() == style)
                .map(|(i, _)| Spot::new(i, 0))
                .collect()
        }
        Navigate::Link | Navigate::Bookmark => {
            let mut out = Vec::new();
            for (index, paragraph) in body.iter().enumerate() {
                let mut offset = 0;
                let mut before = false;
                for run in paragraph.runs() {
                    let is = match kind {
                        Navigate::Link => run.attrs.link.clone().flatten().is_some(),
                        _ => run.attrs.bookmark.clone().flatten().is_some(),
                    } && !matches!(&run.attrs.field, Some(Some(Field::NoteMark { .. })));
                    if is && !before {
                        out.push(Spot::new(index, offset));
                    }
                    before = is;
                    offset += run.len();
                }
            }
            out
        }
    }
}

/// The first place after `from`, or the last before it.
fn next_place(places: &[Spot], from: Spot, forward: bool) -> Option<Spot> {
    if forward {
        places.iter().copied().find(|&s| s > from)
    } else {
        places.iter().rev().copied().find(|&s| s < from)
    }
}

impl TextEditor {
    /// Puts the cursor at the next place of a kind, or at the one before,
    /// and shows it. False when there is none.
    pub fn navigate(&self, kind: Navigate, forward: bool) -> bool {
        self.leave_story(None);
        if kind == Navigate::Page {
            let Some(page) = self.cursor_page() else {
                return false;
            };
            let count = self.pages().len();
            let target = if forward {
                (page + 1 < count).then_some(page + 1)
            } else {
                page.checked_sub(1)
            };
            return target.is_some_and(|page| {
                self.go_to_page(page);
                true
            });
        }
        let (start, end) = self.selection().ordered();
        let from = if forward { end } else { start };
        let target = {
            let state = self.state.borrow();
            let list = places(state.editor.document(), kind, start.paragraph);
            next_place(&list, from, forward)
        };
        match target {
            Some(spot) => {
                self.select(Selection::cursor(spot));
                true
            }
            None => false,
        }
    }

    /// Shows a page, from 0, with the cursor at the start of its text
    /// (Go to Page).
    pub fn go_to_page(&self, page: usize) {
        self.leave_story(None);
        if let Some((start, _)) = self.page_text_range(page) {
            self.select(Selection::cursor(start));
        }
        self.view.scroll_to_page(page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{CharAttrs, Position};

    #[test]
    fn int_19_the_arrows_go_to_the_next_link() {
        let mut document = Document::new_word_processing();
        document
            .transact("Links", |edit| {
                let first = edit.document().body().get(0).unwrap().id();
                let end = edit.insert_text(Position::new(first, 0), "a link and another link")?;
                let link = CharAttrs {
                    link: Some(Some("https://a.b".into())),
                    ..CharAttrs::default()
                };
                edit.set_character_attrs(Position::new(first, 2), Position::new(first, 6), &link)?;
                edit.set_character_attrs(Position::new(first, 19), end, &link)?;
                Ok(())
            })
            .unwrap();
        let links = places(&document, Navigate::Link, 0);
        assert_eq!(links, [Spot::new(0, 2), Spot::new(0, 19)]);
        assert_eq!(
            next_place(&links, Spot::new(0, 2), true),
            Some(Spot::new(0, 19))
        );
        assert_eq!(next_place(&links, Spot::new(0, 19), true), None);
        assert_eq!(
            next_place(&links, Spot::new(0, 19), false),
            Some(Spot::new(0, 2))
        );
        assert_eq!(Navigate::from_name("style"), Navigate::Style);
        assert_eq!(places(&document, Navigate::Section, 0), [Spot::new(0, 0)]);
    }
}
