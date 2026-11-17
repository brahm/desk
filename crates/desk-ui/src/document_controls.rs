// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The controls over the document of window M+, in place of the bottom
//! bar of interface B (GNM-13, step 3 of M+): the zoom between Zoom Out
//! and Zoom In over the end corner (INT-07), and over the foot the count
//! of the document, or of the selection when text is selected (DOC-20).
//! Go to Page and Navigate By, which the bar had in the menu of its
//! position, are in the View menu (INT-19); the arrows of the bar went
//! with it, and their keys stay.

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

/// The name of a kind of place, in the Navigate By menu.
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

/// Fills the items that go to the previous and the next place, named
/// after where they go.
fn fill_steps(steps: &gio::Menu, slides: bool, kind: Navigate) {
    let (previous, next) = if slides {
        (tr("_Previous Slide"), tr("_Next Slide"))
    } else {
        step_labels(kind)
    };
    steps.remove_all();
    steps.append(Some(&previous), Some("win.navigate(false)"));
    steps.append(Some(&next), Some("win.navigate(true)"));
}

/// The items of the View menu that go to the previous and the next place
/// of a kind.
fn step_labels(kind: Navigate) -> (String, String) {
    match kind {
        Navigate::Page => (tr("_Previous Page"), tr("_Next Page")),
        Navigate::Section => (tr("_Previous Section"), tr("_Next Section")),
        Navigate::Note => (tr("_Previous Note"), tr("_Next Note")),
        Navigate::Link => (tr("_Previous Link"), tr("_Next Link")),
        Navigate::Bookmark => (tr("_Previous Bookmark"), tr("_Next Bookmark")),
        Navigate::Style => (
            tr("_Previous Paragraph of the Style"),
            tr("_Next Paragraph of the Style"),
        ),
    }
}

/// The navigation section of the View menu (INT-19, KINT-11): Go to Page,
/// a field held by the item `go-to-page`, the previous and the next place,
/// and Navigate By; in Desk Slides Go to Slide, the previous and the next
/// slide, and the first and the last.
fn navigation_menu(slides: bool, steps: &gio::Menu) -> gio::Menu {
    let menu = gio::Menu::new();
    let go = gio::Menu::new();
    let item = gio::MenuItem::new(None, None);
    item.set_attribute_value("custom", Some(&"go-to-page".to_variant()));
    go.append_item(&item);
    menu.append_section(None, &go);
    menu.append_section(None, steps);
    if slides {
        let ends = gio::Menu::new();
        ends.append(Some(&tr("_First Slide")), Some("win.go-to-slide('first')"));
        ends.append(Some(&tr("_Last Slide")), Some("win.go-to-slide('last')"));
        menu.append_section(None, &ends);
        return menu;
    }
    let by = gio::Menu::new();
    for kind in Navigate::ALL {
        let item = gio::MenuItem::new(Some(&navigate_label(kind)), None);
        item.set_action_and_target_value(Some("win.navigate-by"), Some(&kind.name().to_variant()));
        by.append_item(&item);
    }
    let submenu = gio::Menu::new();
    submenu.append_submenu(Some(&tr("Navigate _By")), &by);
    menu.append_section(None, &submenu);
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

/// A pill of the osd style of GNOME, over a corner or the foot of the
/// document (GNM-13).
fn pill(halign: gtk::Align) -> gtk::Box {
    gtk::Box::builder()
        .spacing(2)
        .halign(halign)
        .valign(gtk::Align::End)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(12)
        .css_classes(["osd", "toolbar", "desk-osd-pill"])
        .build()
}

/// The controls over the document.
pub struct Controls {
    /// The zoom, between Zoom Out and Zoom In, over the end corner.
    pub zoom: gtk::MenuButton,
    zoom_pill: gtk::Box,
    zoom_parts: [gtk::Widget; 3],
    /// The count, over the foot of the document.
    pub count: gtk::MenuButton,
    count_pill: gtk::Box,
    /// Go to Page, the item `go-to-page` of the View menu.
    pub go_to: gtk::Box,
    /// The field of Go to Page.
    pub page_entry: gtk::Entry,
    /// "of 41", after the field.
    page_count: gtk::Label,
    /// The navigation section of the View menu.
    pub navigation: gio::Menu,
    steps: gio::Menu,
    slides: bool,
    /// Whether a document shows, and whether the preferences show the
    /// count.
    shown: std::cell::Cell<bool>,
    counted: std::cell::Cell<bool>,
}

impl Controls {
    /// The controls of Desk Writer, or with `slides` those of Desk Slides,
    /// which have no count and go to slides (KGNM-14).
    pub fn new_for(slides: bool) -> Self {
        let zoom_out = gtk::Button::builder()
            .icon_name("zoom-out-symbolic")
            .action_name("win.zoom")
            .action_target(&"out".to_variant())
            .tooltip_text(tr("Zoom Out"))
            .build();
        crate::formatting::labelled(&zoom_out, &tr("Zoom Out"));
        let zoom_in = gtk::Button::builder()
            .icon_name("zoom-in-symbolic")
            .action_name("win.zoom")
            .action_target(&"in".to_variant())
            .tooltip_text(tr("Zoom In"))
            .build();
        crate::formatting::labelled(&zoom_in, &tr("Zoom In"));
        // A label as the child, so that no arrow shows beside it.
        let zoom_label = gtk::Label::builder()
            .label(geometry::zoom_label(geometry::DEFAULT_ZOOM))
            .css_classes(["numeric"])
            .build();
        let zoom = gtk::MenuButton::builder()
            .child(&zoom_label)
            .menu_model(&zoom_menu())
            .direction(gtk::ArrowType::Up)
            .tooltip_text(tr("Zoom"))
            .css_classes(["flat"])
            .build();
        let zoom_pill = pill(gtk::Align::End);
        zoom_pill.set_visible(false);
        zoom_pill.append(&zoom_out);
        zoom_pill.append(&zoom);
        zoom_pill.append(&zoom_in);

        let count_label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["numeric"])
            .build();
        let count = gtk::MenuButton::builder()
            .child(&count_label)
            .menu_model(&count_menu())
            .direction(gtk::ArrowType::Up)
            .tooltip_text(tr("Word Count"))
            .css_classes(["flat"])
            .build();
        let count_pill = pill(gtk::Align::Center);
        count_pill.append(&count);
        count_pill.set_visible(false);

        let page_entry = gtk::Entry::builder()
            .width_chars(4)
            .max_width_chars(5)
            .input_purpose(gtk::InputPurpose::Digits)
            .xalign(0.5)
            .build();
        let names = if slides {
            (tr("Go to Slide"), tr("_Go to Slide"))
        } else {
            (tr("Go to Page"), tr("_Go to Page"))
        };
        crate::formatting::labelled(&page_entry, &names.0);
        let go_label = gtk::Label::builder()
            .label(names.1)
            .use_underline(true)
            .mnemonic_widget(&page_entry)
            .xalign(0.0)
            .hexpand(true)
            .build();
        let page_count = gtk::Label::builder()
            .css_classes(["numeric", "dim-label"])
            .build();
        let go_to = gtk::Box::builder()
            .spacing(6)
            .margin_start(12)
            .margin_end(6)
            .margin_top(3)
            .margin_bottom(3)
            .build();
        go_to.append(&go_label);
        go_to.append(&page_entry);
        go_to.append(&page_count);
        let steps = gio::Menu::new();
        fill_steps(&steps, slides, Navigate::Page);
        Self {
            navigation: navigation_menu(slides, &steps),
            steps,
            slides,
            shown: std::cell::Cell::new(false),
            counted: std::cell::Cell::new(true),
            zoom_parts: [zoom_out.upcast(), zoom.clone().upcast(), zoom_in.upcast()],
            zoom,
            zoom_pill,
            count,
            count_pill,
            go_to,
            page_entry,
            page_count,
        }
    }

    /// Puts the controls over the document, in `overlay`.
    pub fn attach(&self, overlay: &gtk::Overlay) {
        overlay.add_overlay(&self.count_pill);
        overlay.add_overlay(&self.zoom_pill);
    }

    /// Shows or hides the controls, which show only with a document.
    pub fn set_shown(&self, shown: bool) {
        self.shown.set(shown);
        self.zoom_pill.set_visible(shown);
        self.show_count_pill();
    }

    /// Shows the count or not, as the preferences say (DOC-20).
    pub fn set_counted(&self, counted: bool) {
        self.counted.set(counted);
        self.show_count_pill();
    }

    /// Desk Slides has no count (KGNM-14).
    fn show_count_pill(&self) {
        self.count_pill
            .set_visible(self.shown.get() && self.counted.get() && !self.slides);
    }

    /// Puts `widget` in the pill of the zoom, as the size of the slides
    /// of the light table (KINT-09).
    pub fn add_to_zoom(&self, widget: &impl IsA<gtk::Widget>) {
        self.zoom_pill.append(widget);
    }

    /// Shows the zoom, or leaves in its pill only what was added to it.
    pub fn set_zoom_visible(&self, visible: bool) {
        for part in &self.zoom_parts {
            part.set_visible(visible);
        }
    }

    /// Puts the count over the zoom, at the end, while `breakpoint`
    /// applies: in a window too narrow for the two side by side.
    pub fn stack_in(&self, breakpoint: &adw::Breakpoint) {
        breakpoint.add_setter(
            &self.count_pill,
            "halign",
            Some(&gtk::Align::End.to_value()),
        );
        breakpoint.add_setter(&self.count_pill, "margin-bottom", Some(&60.to_value()));
    }

    /// Shows the number of pages or slides after the field of Go to Page.
    pub fn show_page_count(&self, count: usize) {
        let text = tr("of {count}").replace("{count}", &grouped(count));
        if self.page_count.label() != text {
            self.page_count.set_label(&text);
        }
    }

    /// Names the items that go to the previous and the next place after
    /// what they go to.
    pub fn show_navigate(&self, kind: Navigate) {
        fill_steps(&self.steps, self.slides, kind);
    }

    /// Shows the zoom.
    pub fn show_zoom(&self, zoom: f64) {
        if let Some(label) = self.zoom.child().and_downcast::<gtk::Label>() {
            let text = geometry::zoom_label(zoom);
            label.set_label(&text);
            crate::formatting::labelled(&self.zoom, &text);
        }
    }

    /// Shows the count.
    pub fn show_count(&self, text: &str) {
        let Some(label) = self.count.child().and_downcast::<gtk::Label>() else {
            return;
        };
        if label.label() != text {
            label.set_label(text);
            crate::formatting::labelled(&self.count, text);
        }
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
    fn doc_07_the_count_names_the_words_and_the_pages() {
        // The count stays in full screen, with the words and the pages.
        let all = statistics(1, 4);
        assert_eq!(count_label(Count::Words, &all, None, 3), "1 word");
        assert_eq!(count_label(Count::Pages, &all, None, 1), "1 page");
    }

    #[test]
    fn int_19_the_view_menu_goes_to_the_places_of_each_kind() {
        for kind in Navigate::ALL {
            let (previous, next) = step_labels(kind);
            assert_ne!(previous, next);
            assert!(!navigate_label(kind).is_empty());
        }
        let steps = gio::Menu::new();
        fill_steps(&steps, false, Navigate::Section);
        assert_eq!(steps.n_items(), 2);
        let menu = navigation_menu(false, &steps);
        assert_eq!(menu.n_items(), 3);
        let slides = navigation_menu(true, &steps);
        assert_eq!(slides.n_items(), 3);
    }
}
