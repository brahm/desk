// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Writing Tools (INT-20): the counts of the document and of the
//! selection, and looking the selection up in a dictionary and thesaurus,
//! on the web and in Wikipedia, from the main and context menus. Pages
//! uses the Dictionary app and Spotlight of the Mac; Desk opens the same
//! searches in the browser, in the language of the interface.

use adw::prelude::*;
use desk_model::Statistics;
use gtk::glib;

use crate::i18n::tr;

/// The places to look text up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// The dictionary and thesaurus: Wiktionary.
    Dictionary,
    /// A web search.
    Web,
    /// Wikipedia.
    Wikipedia,
}

impl Lookup {
    /// The address that looks up a text, in a language such as "pt".
    pub fn url(self, text: &str, language: &str) -> String {
        let text = glib::Uri::escape_string(text.trim(), None, false);
        match self {
            Self::Dictionary => {
                format!("https://{language}.wiktionary.org/w/index.php?search={text}")
            }
            Self::Web => format!("https://www.google.com/search?q={text}"),
            Self::Wikipedia => {
                format!("https://{language}.wikipedia.org/w/index.php?search={text}")
            }
        }
    }
}

/// The language of the interface, as "pt" for pt_BR, or "en".
pub fn interface_language() -> String {
    glib::language_names()
        .iter()
        .map(|l| {
            l.split(['_', '.', '@'])
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .find(|l| l.len() >= 2 && l.chars().all(|c| c.is_ascii_lowercase()))
        .unwrap_or_else(|| "en".to_owned())
}

/// Opens a text in a place to look it up.
pub fn look_up(parent: &impl IsA<gtk::Window>, lookup: Lookup, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let url = lookup.url(text, &interface_language());
    gtk::UriLauncher::new(&url).launch(Some(parent), gtk::gio::Cancellable::NONE, |result| {
        if let Err(error) = result {
            glib::g_warning!("desk", "cannot open the browser: {error}");
        }
    });
}

/// The rows of the counts, as (title, value).
pub fn rows(statistics: &Statistics, lines: usize, pages: Option<usize>) -> Vec<(String, String)> {
    let mut out = vec![
        (tr("Words"), statistics.words.to_string()),
        (tr("Characters"), statistics.characters.to_string()),
        (
            tr("Characters Without Spaces"),
            statistics.characters_without_spaces.to_string(),
        ),
        (tr("Paragraphs"), statistics.paragraphs.to_string()),
        (tr("Lines"), lines.to_string()),
    ];
    if let Some(pages) = pages {
        out.push((tr("Pages"), pages.to_string()));
    }
    out
}

/// Shows the counts of the document, and of the selection when there is
/// one.
pub fn show_statistics(
    parent: &impl IsA<gtk::Widget>,
    all: (&Statistics, usize),
    selected: Option<(&Statistics, usize)>,
    pages: usize,
) {
    let page = adw::PreferencesPage::new();
    let group = |title: &str, (statistics, lines): (&Statistics, usize), pages: Option<usize>| {
        let group = adw::PreferencesGroup::builder().title(title).build();
        for (title, value) in rows(statistics, lines, pages) {
            let row = adw::ActionRow::builder().title(title).build();
            let value = gtk::Label::builder()
                .label(value)
                .css_classes(["numeric"])
                .selectable(true)
                .build();
            row.add_suffix(&value);
            group.add(&row);
        }
        group
    };
    if let Some(selected) = selected {
        page.add(&group(&tr("Selection"), selected, None));
    }
    page.add(&group(&tr("Document"), all, Some(pages)));
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&page));
    let dialog = adw::Dialog::builder()
        .title(tr("Statistics"))
        .content_width(360)
        .child(&view)
        .build();
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_20_lookups_escape_the_text() {
        assert_eq!(
            Lookup::Wikipedia.url("São Paulo", "pt"),
            "https://pt.wikipedia.org/w/index.php?search=S%C3%A3o%20Paulo"
        );
        assert_eq!(
            Lookup::Web.url(" a&b ", "en"),
            "https://www.google.com/search?q=a%26b"
        );
        assert!(
            Lookup::Dictionary
                .url("casa", "pt")
                .starts_with("https://pt.wiktionary.org/")
        );
    }

    #[test]
    fn int_20_statistics_have_pages_for_the_document() {
        let statistics = Statistics {
            paragraphs: 2,
            words: 5,
            characters: 30,
            characters_without_spaces: 25,
        };
        assert_eq!(rows(&statistics, 7, None).len(), 5);
        let rows = rows(&statistics, 7, Some(3));
        assert_eq!(rows.last().map(|r| r.1.as_str()), Some("3"));
        // DOC-20: the lines, before the pages.
        assert_eq!(rows[rows.len() - 2].1, "7");
    }
}
