// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bar at the bottom of the window of interface B (GNM-13): the zoom
//! at the start (INT-07), the position in the middle between the arrows,
//! with Go to Page and Navigate By in its menu (INT-19), and at the end
//! the count of the document, or of the selection when text is selected
//! (DOC-20).

use adw::prelude::*;
use desk_model::Statistics;
use gtk::gio;

use crate::editing::Navigate;
use crate::geometry;
use crate::i18n::{tr, trc, trn};

/// What the count at the end of the bar counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Count {
    Words,
    Characters,
    CharactersWithoutSpaces,
    Paragraphs,
    Pages,
}

impl Count {
    /// Every kind, in the order of the menu.
    pub const ALL: [Self; 5] = [
        Self::Words,
        Self::Characters,
        Self::CharactersWithoutSpaces,
        Self::Paragraphs,
        Self::Pages,
    ];

    /// The name of the kind in the state of the action and in the
    /// preferences.
    pub fn name(self) -> &'static str {
        match self {
            Self::Words => "words",
            Self::Characters => "characters",
            Self::CharactersWithoutSpaces => "characters-without-spaces",
            Self::Paragraphs => "paragraphs",
            Self::Pages => "pages",
        }
    }

    /// The kind with a name, or words.
    pub fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|k| k.name() == name)
            .unwrap_or(Self::Words)
    }

    /// The item of the menu that chooses it.
    fn menu_label(self) -> String {
        match self {
            Self::Words => tr("_Words"),
            Self::Characters => tr("_Characters"),
            Self::CharactersWithoutSpaces => tr("Characters _Without Spaces"),
            Self::Paragraphs => tr("_Paragraphs"),
            Self::Pages => tr("Pa_ges"),
        }
    }

    /// How many of the kind some text has.
    fn of(self, statistics: &Statistics) -> usize {
        match self {
            Self::Words => statistics.words,
            Self::Characters => statistics.characters,
            Self::CharactersWithoutSpaces => statistics.characters_without_spaces,
            Self::Paragraphs => statistics.paragraphs,
            Self::Pages => 0,
        }
    }
}

/// A number with its thousands apart, as 26,171 or, in Portuguese,
/// 26.171.
pub fn grouped(n: usize) -> String {
    let separator = trc("thousands separator", ",");
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(&separator);
        }
        out.push(digit);
    }
    out
}

/// The count at the end of the bar: of the document, or "12 of 3,402
/// words" when text is selected. Pages are always those of the document.
pub fn count_label(
    kind: Count,
    all: &Statistics,
    selected: Option<&Statistics>,
    pages: usize,
) -> String {
    if kind == Count::Pages {
        return plural_of(kind, false, pages).replace("{n}", &grouped(pages));
    }
    let total = kind.of(all);
    let text = match selected {
        Some(selected) => plural_of(kind, true, total).replace("{s}", &grouped(kind.of(selected))),
        None => plural_of(kind, false, total),
    };
    text.replace("{n}", &grouped(total))
}

/// The plural form of the count, with `{n}` for the total and `{s}` for
/// the selection.
fn plural_of(kind: Count, selection: bool, n: usize) -> String {
    let n = n as u64;
    match (kind, selection) {
        (Count::Words, false) => trn("{n} word", "{n} words", n),
        (Count::Words, true) => trn("{s} of {n} word", "{s} of {n} words", n),
        (Count::Characters, false) => trn("{n} character", "{n} characters", n),
        (Count::Characters, true) => trn("{s} of {n} character", "{s} of {n} characters", n),
        (Count::CharactersWithoutSpaces, false) => trn(
            "{n} character without spaces",
            "{n} characters without spaces",
            n,
        ),
        (Count::CharactersWithoutSpaces, true) => trn(
            "{s} of {n} character without spaces",
            "{s} of {n} characters without spaces",
            n,
        ),
        (Count::Paragraphs, false) => trn("{n} paragraph", "{n} paragraphs", n),
        (Count::Paragraphs, true) => trn("{s} of {n} paragraph", "{s} of {n} paragraphs", n),
        (Count::Pages, _) => trn("{n} page", "{n} pages", n),
    }
}

/// The position in the middle of the bar.
pub fn position_label(page: usize, count: usize) -> String {
    tr("Page {page} of {count}")
        .replace("{page}", &grouped(page))
        .replace("{count}", &grouped(count))
}

/// The name of what the arrows go to, in the Navigate By menu.
fn navigate_label(kind: Navigate) -> String {
    match kind {
        Navigate::Page => tr("Page"),
        Navigate::Section => tr("Section"),
        Navigate::Note => tr("Note"),
        Navigate::Link => tr("Link"),
        Navigate::Bookmark => tr("Bookmark"),
        Navigate::Style => tr("Paragraph Style"),
    }
}

/// The tooltips of the arrows, which say where they go.
pub fn arrow_tooltips(kind: Navigate) -> (String, String) {
    match kind {
        Navigate::Page => (tr("Previous Page"), tr("Next Page")),
        Navigate::Section => (tr("Previous Section"), tr("Next Section")),
        Navigate::Note => (tr("Previous Note"), tr("Next Note")),
        Navigate::Link => (tr("Previous Link"), tr("Next Link")),
        Navigate::Bookmark => (tr("Previous Bookmark"), tr("Next Bookmark")),
        Navigate::Style => (
            tr("Previous Paragraph of the Style"),
            tr("Next Paragraph of the Style"),
        ),
    }
}

/// The menu of the position: Go to Page, a field held by the item
/// `go-to-page`, and Navigate By (INT-19).
fn position_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let go = gio::Menu::new();
    let item = gio::MenuItem::new(None, None);
    item.set_attribute_value("custom", Some(&"go-to-page".to_variant()));
    go.append_item(&item);
    menu.append_section(None, &go);
    let by = gio::Menu::new();
    for kind in Navigate::ALL {
        let item = gio::MenuItem::new(Some(&navigate_label(kind)), None);
        item.set_action_and_target_value(Some("win.navigate-by"), Some(&kind.name().to_variant()));
        by.append_item(&item);
    }
    menu.append_section(Some(&tr("Navigate By")), &by);
    menu
}

/// The menu of the count: what it counts, and Show Statistics (DOC-20).
fn count_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let kinds = gio::Menu::new();
    for kind in Count::ALL {
        let item = gio::MenuItem::new(Some(&kind.menu_label()), None);
        item.set_action_and_target_value(Some("win.count"), Some(&kind.name().to_variant()));
        kinds.append_item(&item);
    }
    menu.append_section(None, &kinds);
    let statistics = gio::Menu::new();
    statistics.append(Some(&tr("Show _Statistics")), Some("win.statistics"));
    menu.append_section(None, &statistics);
    menu
}

/// The zoom menu, at the bottom and in the View menu (INT-07).
pub fn zoom_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let levels = gio::Menu::new();
    for level in geometry::ZOOM_LEVELS {
        let item = gio::MenuItem::new(Some(&geometry::zoom_label(level)), None);
        item.set_action_and_target_value(Some("win.zoom"), Some(&level.to_string().to_variant()));
        levels.append_item(&item);
    }
    menu.append_section(None, &levels);
    let fit = gio::Menu::new();
    for (label, target) in [
        (tr("Fit _Width"), "fit-width"),
        (tr("Fit _Page"), "fit-page"),
    ] {
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.zoom"), Some(&target.to_variant()));
        fit.append_item(&item);
    }
    menu.append_section(None, &fit);
    let layout = gio::Menu::new();
    for (label, target) in [(tr("_One Page"), "one"), (tr("_Two Pages"), "two")] {
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.page-layout"), Some(&target.to_variant()));
        layout.append_item(&item);
    }
    menu.append_section(None, &layout);
    menu
}

/// A flat menu button that opens upward, with a label that can give way.
fn menu_button(tooltip: &str) -> (gtk::MenuButton, gtk::Label) {
    let label = gtk::Label::builder()
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["numeric"])
        .build();
    let button = gtk::MenuButton::builder()
        .child(&label)
        .direction(gtk::ArrowType::Up)
        .tooltip_text(tooltip)
        .css_classes(["flat"])
        .build();
    (button, label)
}

/// The widgets of the bar.
pub struct BottomBar {
    pub bar: gtk::ActionBar,
    pub zoom: gtk::MenuButton,
    pub previous: gtk::Button,
    pub next: gtk::Button,
    pub position: gtk::MenuButton,
    position_label: gtk::Label,
    /// Go to Page, in the menu of the position.
    pub page_entry: gtk::Entry,
    pub count: gtk::MenuButton,
    count_label: gtk::Label,
}

impl BottomBar {
    pub fn new() -> Self {
        let zoom = gtk::MenuButton::builder()
            .label(geometry::zoom_label(geometry::DEFAULT_ZOOM))
            .menu_model(&zoom_menu())
            .direction(gtk::ArrowType::Up)
            .tooltip_text(tr("Zoom"))
            .css_classes(["flat"])
            .build();
        let (previous_tip, next_tip) = arrow_tooltips(Navigate::Page);
        let previous = gtk::Button::builder()
            .icon_name("go-up-symbolic")
            .tooltip_text(previous_tip)
            .action_name("win.navigate")
            .action_target(&false.to_variant())
            .css_classes(["flat"])
            .build();
        let next = gtk::Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text(next_tip)
            .action_name("win.navigate")
            .action_target(&true.to_variant())
            .css_classes(["flat"])
            .build();
        let (position, position_label) = menu_button(&tr("Go To and Navigate By"));
        let page_entry = gtk::Entry::builder()
            .width_chars(4)
            .max_width_chars(5)
            .input_purpose(gtk::InputPurpose::Digits)
            .xalign(0.5)
            .build();
        crate::formatting::labelled(&page_entry, &tr("Go to Page"));
        let go_label = gtk::Label::builder()
            .label(tr("_Go to Page"))
            .use_underline(true)
            .mnemonic_widget(&page_entry)
            .xalign(0.0)
            .hexpand(true)
            .build();
        let go = gtk::Box::builder()
            .spacing(12)
            .margin_start(12)
            .margin_end(6)
            .margin_top(3)
            .margin_bottom(3)
            .build();
        go.append(&go_label);
        go.append(&page_entry);
        let popover = gtk::PopoverMenu::from_model(Some(&position_menu()));
        popover.add_child(&go, "go-to-page");
        position.set_popover(Some(&popover));
        let center = gtk::Box::builder().spacing(2).build();
        center.append(&previous);
        center.append(&position);
        center.append(&next);
        let (count, count_label) = menu_button(&tr("Word Count"));
        count.set_menu_model(Some(&count_menu()));
        // The menus at the ends open toward the middle of the window.
        if let Some(popover) = count.popover() {
            popover.set_halign(gtk::Align::End);
        }
        if let Some(popover) = zoom.popover() {
            popover.set_halign(gtk::Align::Start);
        }
        let bar = gtk::ActionBar::new();
        bar.pack_start(&zoom);
        bar.set_center_widget(Some(&center));
        bar.pack_end(&count);
        bar.set_revealed(false);
        Self {
            bar,
            zoom,
            previous,
            next,
            position,
            position_label,
            page_entry,
            count,
            count_label,
        }
    }

    /// Shows the page in sight and the number of pages.
    pub fn show_position(&self, page: usize, count: usize) {
        let text = position_label(page, count);
        if self.position_label.label() != text {
            self.position_label.set_label(&text);
            crate::formatting::labelled(&self.position, &text);
        }
    }

    /// Shows the count.
    pub fn show_count(&self, text: &str) {
        if self.count_label.label() != text {
            self.count_label.set_label(text);
            crate::formatting::labelled(&self.count, text);
        }
    }

    /// Says on the arrows where they go.
    pub fn show_navigate(&self, kind: Navigate) {
        let (previous, next) = arrow_tooltips(kind);
        self.previous.set_tooltip_text(Some(&previous));
        self.next.set_tooltip_text(Some(&next));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statistics(words: usize, characters: usize) -> Statistics {
        Statistics {
            words,
            characters,
            characters_without_spaces: characters - words.saturating_sub(1),
            paragraphs: 1,
            ..Statistics::default()
        }
    }

    #[test]
    fn doc_20_the_count_is_of_the_selection_when_there_is_one() {
        let all = statistics(26171, 150_000);
        assert_eq!(grouped(26171), "26,171");
        assert_eq!(grouped(171), "171");
        assert_eq!(grouped(1_000_000), "1,000,000");
        assert_eq!(count_label(Count::Words, &all, None, 41), "26,171 words");
        let one = statistics(1, 4);
        assert_eq!(count_label(Count::Words, &one, None, 1), "1 word");
        let selected = statistics(12, 60);
        assert_eq!(
            count_label(Count::Words, &all, Some(&selected), 41),
            "12 of 26,171 words"
        );
        assert_eq!(
            count_label(Count::Characters, &all, Some(&selected), 41),
            "60 of 150,000 characters"
        );
        assert_eq!(
            count_label(Count::Pages, &all, Some(&selected), 41),
            "41 pages"
        );
        assert_eq!(count_label(Count::Paragraphs, &one, None, 1), "1 paragraph");
        for kind in Count::ALL {
            assert_eq!(Count::from_name(kind.name()), kind);
        }
        assert_eq!(Count::from_name("lines"), Count::Words);
    }

    #[test]
    fn doc_07_the_bar_names_the_words_and_the_pages() {
        // The bar stays in full screen, with the words and the pages.
        let all = statistics(1, 4);
        assert_eq!(count_label(Count::Words, &all, None, 3), "1 word");
        assert_eq!(position_label(1, 3), "Page 1 of 3");
        assert_eq!(count_label(Count::Pages, &all, None, 1), "1 page");
    }

    #[test]
    fn int_19_the_position_names_the_page_and_the_pages() {
        assert_eq!(position_label(6, 41), "Page 6 of 41");
        for kind in Navigate::ALL {
            let (previous, next) = arrow_tooltips(kind);
            assert_ne!(previous, next);
            assert!(!navigate_label(kind).is_empty());
        }
    }
}
