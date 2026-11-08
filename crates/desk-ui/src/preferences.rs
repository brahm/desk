// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preferences of the apps, which hold for every document (FMT-17).
//!
//! They live in a key file in the configuration folder of the user,
//! `desk/preferences.ini`, which the three apps share. Every window
//! follows a change at once.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use desk_model::{AutoCorrect, Replacement, Rgba};

use crate::i18n::tr;

/// The units of the rulers and of lengths in the Inspector (FMT-17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    /// Centimeters.
    Centimeters,
    /// Inches.
    Inches,
    /// Points, 72 to the inch.
    Points,
    /// Picas, 6 to the inch.
    Picas,
}

impl Units {
    /// Every unit, in the order of the menu.
    pub const ALL: [Self; 4] = [Self::Centimeters, Self::Inches, Self::Points, Self::Picas];

    /// The points in one unit.
    pub fn points(self) -> f64 {
        match self {
            Self::Centimeters => 72.0 / 2.54,
            Self::Inches => 72.0,
            Self::Points => 1.0,
            Self::Picas => 12.0,
        }
    }

    /// The name in the key file.
    fn key(self) -> &'static str {
        match self {
            Self::Centimeters => "centimeters",
            Self::Inches => "inches",
            Self::Points => "points",
            Self::Picas => "picas",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.key() == key)
    }

    /// The name shown in the preferences.
    pub fn name(self) -> String {
        match self {
            Self::Centimeters => tr("Centimeters"),
            Self::Inches => tr("Inches"),
            Self::Points => tr("Points"),
            Self::Picas => tr("Picas"),
        }
    }

    /// The symbol written after a length.
    pub fn symbol(self) -> String {
        match self {
            Self::Centimeters => tr("cm"),
            Self::Inches => tr("in"),
            Self::Points => tr("pt"),
            Self::Picas => tr("pc"),
        }
    }

    /// How many decimal places a length in this unit shows.
    pub fn digits(self) -> u32 {
        match self {
            Self::Centimeters | Self::Inches => 2,
            Self::Points | Self::Picas => 1,
        }
    }

    /// The step of a field in this unit, in units.
    pub fn step(self) -> f64 {
        match self {
            Self::Centimeters => 0.1,
            Self::Inches => 0.125,
            Self::Points => 1.0,
            Self::Picas => 0.5,
        }
    }

    /// The distance between numbered ticks of a ruler and how many parts
    /// each is divided in, in units.
    pub fn ticks(self) -> (f64, u32) {
        match self {
            Self::Centimeters => (1.0, 4),
            Self::Inches => (1.0, 8),
            Self::Points => (72.0, 6),
            Self::Picas => (6.0, 6),
        }
    }

    /// The units of a country that measures in inches, centimeters
    /// otherwise, as Pages chooses them from the region.
    pub fn of_region(region: &str) -> Self {
        if matches!(region, "US" | "LR" | "MM") {
            Self::Inches
        } else {
            Self::Centimeters
        }
    }
}

/// The preferences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    /// The units of the rulers (FMT-17).
    pub units: Units,
    /// Whether rulers count in percentages of the width of the text
    /// (FMT-17).
    pub percent: bool,
    /// Whether rulers count from the middle of the text instead of its
    /// left edge (FMT-17).
    pub center_origin: bool,
    /// Whether the vertical ruler shows with the horizontal one (FMT-17).
    pub vertical_ruler: bool,
    /// Whether a paragraph that starts with "•", "-", "*", "1.", "a." or
    /// "I." and a space becomes a list on Return (LST-01).
    pub automatic_lists: bool,
    /// Whether straight quotes become curly ones as they are typed
    /// (TXT-06).
    pub smart_quotes: bool,
    /// The corrections made while typing (TXT-07).
    pub autocorrect: AutoCorrect,
    /// Whether misspelled words are marked as they are typed (TXT-10).
    pub check_spelling: bool,
    /// Whether the mistakes of the proofreader are marked as they are
    /// typed (TXT-11).
    pub proofread: bool,
    /// Whether a misspelled word that the dictionary has one suggestion
    /// for is corrected as it is typed (TXT-07).
    pub accept_single_suggestion: bool,
    /// Whether email and web addresses become links as they are typed
    /// (TXT-07, EST-25). As in Pages, it belongs to the computer, not to
    /// the document.
    pub detect_links: bool,
    /// The zoom of a new window, in percent (INT-07).
    pub zoom: u32,
    /// The color of the invisible characters (INT-10).
    pub invisibles_color: Rgba,
    /// Whether the words and pages of the document show at the bottom of
    /// the window (DOC-20).
    pub show_counts: bool,
    /// Whether a saved document holds a picture of its first page, which
    /// file managers show (DOC-09).
    pub include_preview: bool,
    /// The color of the guides that a drag of objects snaps to (OBJ-27).
    pub guide_color: Rgba,
    /// Whether a drag of objects shows where they go, how big they get or
    /// how far they turn, next to the pointer (OBJ-23).
    pub show_size_and_position: bool,
}

/// The color of invisible characters until it is changed.
pub const INVISIBLES_COLOR: Rgba = Rgba::rgb(64, 115, 217);

/// The color of the alignment guides until it is changed: the yellow of
/// Pages.
pub const GUIDE_COLOR: Rgba = Rgba::rgb(245, 194, 18);

impl Default for Preferences {
    fn default() -> Self {
        // The region of the first language, as in pt_BR or en_US.
        let region = glib::language_names()
            .first()
            .and_then(|l| {
                l.split(['.', '@'])
                    .next()?
                    .split('_')
                    .nth(1)
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        Self {
            units: Units::of_region(&region),
            percent: false,
            center_origin: false,
            vertical_ruler: false,
            automatic_lists: true,
            smart_quotes: true,
            autocorrect: AutoCorrect::default(),
            check_spelling: true,
            proofread: false,
            accept_single_suggestion: false,
            detect_links: true,
            zoom: 100,
            invisibles_color: INVISIBLES_COLOR,
            show_counts: true,
            include_preview: true,
            guide_color: GUIDE_COLOR,
            show_size_and_position: true,
        }
    }
}

const RULERS: &str = "rulers";
const EDITING: &str = "editing";
const WINDOW: &str = "window";
const SAVING: &str = "saving";

/// A color as `#rrggbb`, or `#rrggbbaa` when it is not opaque.
fn color_key(color: Rgba) -> String {
    let Rgba { r, g, b, a } = color;
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

fn color_from_key(key: &str) -> Option<Rgba> {
    let hex = key.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Rgba::rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: byte(6)?,
        }),
        _ => None,
    }
}

impl Preferences {
    /// Reads preferences from a key file; what it lacks keeps its default.
    fn read(file: &glib::KeyFile) -> Self {
        let mut out = Self::default();
        if let Some(units) = file
            .string(RULERS, "units")
            .ok()
            .and_then(|u| Units::from_key(&u))
        {
            out.units = units;
        }
        let flag = |key: &str, default: bool| file.boolean(RULERS, key).unwrap_or(default);
        out.percent = flag("percent", out.percent);
        out.center_origin = flag("center-origin", out.center_origin);
        out.vertical_ruler = flag("vertical-ruler", out.vertical_ruler);
        let editing = |key: &str, default: bool| file.boolean(EDITING, key).unwrap_or(default);
        out.automatic_lists = editing("automatic-lists", out.automatic_lists);
        out.smart_quotes = editing("smart-quotes", out.smart_quotes);
        out.check_spelling = editing("check-spelling", out.check_spelling);
        out.proofread = editing("proofread", out.proofread);
        out.accept_single_suggestion =
            editing("accept-single-suggestion", out.accept_single_suggestion);
        out.detect_links = editing("detect-links", out.detect_links);
        if let Ok(zoom) = file.integer(RULERS, "zoom") {
            out.zoom = zoom.clamp(10, 1000) as u32;
        }
        if let Some(color) = file
            .string(WINDOW, "invisibles-color")
            .ok()
            .and_then(|c| color_from_key(&c))
        {
            out.invisibles_color = color;
        }
        out.show_counts = file
            .boolean(WINDOW, "show-counts")
            .unwrap_or(out.show_counts);
        out.include_preview = file
            .boolean(SAVING, "include-preview")
            .unwrap_or(out.include_preview);
        if let Some(color) = file
            .string(EDITING, "guide-color")
            .ok()
            .and_then(|c| color_from_key(&c))
        {
            out.guide_color = color;
        }
        out.show_size_and_position = editing("show-size-and-position", out.show_size_and_position);
        let correct = &mut out.autocorrect;
        correct.capitalization = editing("fix-capitalization", correct.capitalization);
        correct.ordinals = editing("superscript-ordinals", correct.ordinals);
        correct.replace = editing("replace-text", correct.replace);
        if let Ok(list) = file.string(EDITING, "replacements") {
            correct.replacements = list
                .lines()
                .filter_map(|entry| {
                    let mut parts = entry.splitn(3, '\t');
                    let enabled = parts.next()? == "1";
                    let from = parts.next()?;
                    let to = parts.next()?;
                    Some(Replacement {
                        enabled,
                        ..Replacement::new(from, to)
                    })
                })
                .collect();
        }
        out
    }

    fn write(&self, file: &glib::KeyFile) {
        file.set_string(RULERS, "units", self.units.key());
        file.set_boolean(RULERS, "percent", self.percent);
        file.set_boolean(RULERS, "center-origin", self.center_origin);
        file.set_boolean(RULERS, "vertical-ruler", self.vertical_ruler);
        file.set_boolean(EDITING, "automatic-lists", self.automatic_lists);
        file.set_boolean(EDITING, "smart-quotes", self.smart_quotes);
        file.set_boolean(EDITING, "check-spelling", self.check_spelling);
        file.set_boolean(EDITING, "proofread", self.proofread);
        file.set_boolean(
            EDITING,
            "accept-single-suggestion",
            self.accept_single_suggestion,
        );
        file.set_boolean(EDITING, "detect-links", self.detect_links);
        file.set_integer(RULERS, "zoom", self.zoom as i32);
        file.set_string(
            WINDOW,
            "invisibles-color",
            &color_key(self.invisibles_color),
        );
        file.set_boolean(WINDOW, "show-counts", self.show_counts);
        file.set_boolean(SAVING, "include-preview", self.include_preview);
        file.set_string(EDITING, "guide-color", &color_key(self.guide_color));
        file.set_boolean(
            EDITING,
            "show-size-and-position",
            self.show_size_and_position,
        );
        let correct = &self.autocorrect;
        file.set_boolean(EDITING, "fix-capitalization", correct.capitalization);
        file.set_boolean(EDITING, "superscript-ordinals", correct.ordinals);
        file.set_boolean(EDITING, "replace-text", correct.replace);
        let list: Vec<String> = correct
            .replacements
            .iter()
            .map(|r| format!("{}\t{}\t{}", u8::from(r.enabled), r.from, r.to))
            .collect();
        // One replacement on each line, which the key file escapes.
        file.set_string(EDITING, "replacements", &list.join("\n"));
    }
}

fn path() -> PathBuf {
    glib::user_config_dir().join("desk").join("preferences.ini")
}

/// Something to call when the preferences change.
type Listener = Box<dyn Fn(&Preferences)>;

struct Store {
    preferences: Preferences,
    listeners: Vec<Listener>,
}

thread_local! {
    static STORE: RefCell<Option<Store>> = const { RefCell::new(None) };
}

fn with_store<T>(f: impl FnOnce(&mut Store) -> T) -> T {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        let store = store.get_or_insert_with(|| {
            let file = glib::KeyFile::new();
            let preferences = if file
                .load_from_file(path(), glib::KeyFileFlags::NONE)
                .is_ok()
            {
                Preferences::read(&file)
            } else {
                Preferences::default()
            };
            Store {
                preferences,
                listeners: Vec::new(),
            }
        });
        f(store)
    })
}

/// The preferences as they are now.
pub fn get() -> Preferences {
    with_store(|s| s.preferences.clone())
}

/// Changes the preferences, saves them and tells every listener.
pub fn set(preferences: Preferences) {
    if get() == preferences {
        return;
    }
    with_store(|s| s.preferences = preferences.clone());
    let file = glib::KeyFile::new();
    // Keep what other versions wrote.
    let _ = file.load_from_file(path(), glib::KeyFileFlags::KEEP_COMMENTS);
    preferences.write(&file);
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(error) = file.save_to_file(&path) {
        glib::g_warning!("desk", "Cannot save the preferences: {error}");
    }
    // The listeners may read the preferences, so they run without the
    // borrow.
    let listeners = with_store(|s| std::mem::take(&mut s.listeners));
    for listener in &listeners {
        listener(&preferences);
    }
    with_store(|s| {
        let added = std::mem::replace(&mut s.listeners, listeners);
        s.listeners.extend(added);
    });
}

/// Calls `f` whenever the preferences change.
pub fn connect_changed(f: impl Fn(&Preferences) + 'static) {
    with_store(|s| s.listeners.push(Box::new(f)));
}

/// Changes the preferences with `f`.
fn change(f: impl FnOnce(&mut Preferences)) {
    let mut preferences = get();
    f(&mut preferences);
    set(preferences);
}

/// A switch of the preferences.
fn switch(
    title: &str,
    subtitle: &str,
    active: bool,
    f: impl Fn(&mut Preferences, bool) + 'static,
) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(title)
        .active(active)
        .build();
    if !subtitle.is_empty() {
        row.set_subtitle(subtitle);
    }
    row.connect_active_notify(move |row| {
        let active = row.is_active();
        change(|p| f(p, active));
    });
    row
}

/// Shows the preferences over a window.
pub fn show(parent: &impl IsA<gtk::Widget>) {
    let current = get();
    let units = adw::ComboRow::builder()
        .title(tr("Ruler Units"))
        .model(&gtk::StringList::new(
            &Units::ALL
                .map(Units::name)
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        ))
        .selected(
            Units::ALL
                .iter()
                .position(|u| *u == current.units)
                .unwrap_or(0) as u32,
        )
        .build();
    units.connect_selected_notify(|row| {
        let units = Units::ALL
            .get(row.selected() as usize)
            .copied()
            .unwrap_or(Units::Centimeters);
        change(|p| p.units = units);
    });
    let rulers = adw::PreferencesGroup::builder()
        .title(tr("Rulers"))
        .description(tr("For every document"))
        .build();
    rulers.add(&units);
    rulers.add(&switch(
        &tr("Show Units as Percentage"),
        &tr("Rulers count in percentages of the width of the text"),
        current.percent,
        |p, on| p.percent = on,
    ));
    rulers.add(&switch(
        &tr("Place Origin at Center"),
        &tr("Rulers count from the middle of the text"),
        current.center_origin,
        |p, on| p.center_origin = on,
    ));
    rulers.add(&switch(
        &tr("Show Vertical Ruler"),
        &tr("Along with the horizontal ruler"),
        current.vertical_ruler,
        |p, on| p.vertical_ruler = on,
    ));
    let zooms: Vec<String> = crate::geometry::ZOOM_LEVELS
        .iter()
        .map(|z| crate::geometry::zoom_label(*z))
        .collect();
    let zoom = adw::ComboRow::builder()
        .title(tr("Default Zoom"))
        .subtitle(tr("Of new windows"))
        .model(&gtk::StringList::new(
            &zooms.iter().map(String::as_str).collect::<Vec<_>>(),
        ))
        .selected(
            crate::geometry::ZOOM_LEVELS
                .iter()
                .position(|z| (z * 100.0).round() as u32 == current.zoom)
                .unwrap_or(3) as u32,
        )
        .build();
    zoom.connect_selected_notify(|row| {
        if let Some(level) = crate::geometry::ZOOM_LEVELS.get(row.selected() as usize) {
            change(|p| p.zoom = (level * 100.0).round() as u32);
        }
    });
    rulers.add(&zoom);
    let editing = adw::PreferencesGroup::builder()
        .title(tr("Editing"))
        .build();
    editing.add(&switch(
        &tr("Automatically Detect Lists"),
        &tr("“1.”, “a.”, “I.”, “•”, “-” or “*” and a space start a list on Return"),
        current.automatic_lists,
        |p, on| p.automatic_lists = on,
    ));
    let correction = adw::PreferencesGroup::builder()
        .title(tr("Auto-Correction"))
        .description(tr("While typing, in every document"))
        .build();
    correction.add(&switch(
        &tr("Use Smart Quotes"),
        &tr("Straight quotes become curly quotes"),
        current.smart_quotes,
        |p, on| p.smart_quotes = on,
    ));
    correction.add(&switch(
        &tr("Fix Capitalization"),
        &tr("Sentences start with a capital, and words with one capital"),
        current.autocorrect.capitalization,
        |p, on| p.autocorrect.capitalization = on,
    ));
    correction.add(&switch(
        &tr("Superscript Numerical Suffixes"),
        &tr("As in 1st and 2nd"),
        current.autocorrect.ordinals,
        |p, on| p.autocorrect.ordinals = on,
    ));
    correction.add(&switch(
        &tr("Detect Email and Web Addresses"),
        &tr("They become links as they are typed"),
        current.detect_links,
        |p, on| p.detect_links = on,
    ));
    correction.add(&switch(
        &tr("Use Spelling Suggestion"),
        &tr("A misspelled word with one suggestion is corrected"),
        current.accept_single_suggestion,
        |p, on| p.accept_single_suggestion = on,
    ));
    correction.add(&replacements_row(&current.autocorrect));
    let window = adw::PreferencesGroup::builder().title(tr("Window")).build();
    window.add(&switch(
        &tr("Show Word and Page Count"),
        &tr("At the bottom of the window"),
        current.show_counts,
        |p, on| p.show_counts = on,
    ));
    let color = gtk::ColorDialogButton::builder()
        .dialog(&gtk::ColorDialog::new())
        .rgba(&crate::formatting::to_gdk(current.invisibles_color))
        .valign(gtk::Align::Center)
        .build();
    color.connect_rgba_notify(|button| {
        let color = crate::formatting::from_gdk(&button.rgba());
        change(|p| p.invisibles_color = color);
    });
    let invisibles = adw::ActionRow::builder()
        .title(tr("Invisibles Color"))
        .subtitle(tr("Of spaces, tabs and breaks when they are shown"))
        .build();
    invisibles.add_suffix(&color);
    invisibles.set_activatable_widget(Some(&color));
    window.add(&invisibles);
    window.add(&switch(
        &tr("Include Preview in Document"),
        &tr("A picture of the first page, which file managers show"),
        current.include_preview,
        |p, on| p.include_preview = on,
    ));
    // As the General pane of Pages lays out its alignment guides.
    let objects = adw::PreferencesGroup::builder()
        .title(tr("Objects"))
        .build();
    let guide_button = gtk::ColorDialogButton::builder()
        .dialog(&gtk::ColorDialog::new())
        .rgba(&crate::formatting::to_gdk(current.guide_color))
        .valign(gtk::Align::Center)
        .build();
    guide_button.connect_rgba_notify(|button| {
        let color = crate::formatting::from_gdk(&button.rgba());
        change(|p| p.guide_color = color);
    });
    let guides = adw::ActionRow::builder()
        .title(tr("Alignment Guides Color"))
        .subtitle(tr("Of the guides that show while objects are dragged"))
        .build();
    guides.add_suffix(&guide_button);
    guides.set_activatable_widget(Some(&guide_button));
    objects.add(&guides);
    objects.add(&switch(
        &tr("Show Size and Position When Moving Objects"),
        &tr("Next to the pointer, while an object is dragged"),
        current.show_size_and_position,
        |p, on| p.show_size_and_position = on,
    ));
    let page = adw::PreferencesPage::builder()
        .title(tr("General"))
        .icon_name("preferences-system-symbolic")
        .build();
    page.add(&rulers);
    page.add(&editing);
    page.add(&correction);
    page.add(&objects);
    page.add(&window);
    let dialog = adw::PreferencesDialog::builder()
        .title(tr("Preferences"))
        .build();
    dialog.add(&page);
    dialog.present(Some(parent));
}

/// The table of replacements: a switch for the table, and each
/// replacement with its own switch, which can be removed, and a row to
/// add one (TXT-07).
fn replacements_row(current: &AutoCorrect) -> adw::ExpanderRow {
    let expander = adw::ExpanderRow::builder()
        .title(tr("Symbol and Text Substitution"))
        .subtitle(tr("Such as “(c)” for “©”, when typed as a word"))
        .show_enable_switch(true)
        .enable_expansion(current.replace)
        .build();
    expander.connect_enable_expansion_notify(|row| {
        let on = row.enables_expansion();
        change(|p| p.autocorrect.replace = on);
    });
    let rows = Rc::new(RefCell::new(Vec::new()));
    fill_replacements(&expander, &rows);
    expander
}

/// Shows the replacements as the table has them now, then the row that
/// adds one.
fn fill_replacements(expander: &adw::ExpanderRow, rows: &Rc<RefCell<Vec<gtk::Widget>>>) {
    for row in rows.borrow_mut().drain(..) {
        expander.remove(&row);
    }
    let again = |expander: &adw::ExpanderRow, rows: &Rc<RefCell<Vec<gtk::Widget>>>| {
        let (expander, rows) = (expander.downgrade(), Rc::downgrade(rows));
        move || {
            let (expander, rows) = (expander.clone(), rows.clone());
            glib::idle_add_local_once(move || {
                if let (Some(expander), Some(rows)) = (expander.upgrade(), rows.upgrade()) {
                    fill_replacements(&expander, &rows);
                }
            });
        }
    };
    for (index, replacement) in get().autocorrect.replacements.iter().enumerate() {
        let row = adw::SwitchRow::builder()
            .title(format!("{} \u{2192} {}", replacement.from, replacement.to))
            .use_markup(false)
            .active(replacement.enabled)
            .build();
        row.connect_active_notify(move |row| {
            let on = row.is_active();
            change(|p| {
                if let Some(r) = p.autocorrect.replacements.get_mut(index) {
                    r.enabled = on;
                }
            });
        });
        let remove = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text(tr("Remove"))
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        crate::formatting::labelled(&remove, &tr("Remove"));
        let refill = again(expander, rows);
        remove.connect_clicked(move |_| {
            change(|p| {
                if index < p.autocorrect.replacements.len() {
                    p.autocorrect.replacements.remove(index);
                }
            });
            refill();
        });
        row.add_prefix(&remove);
        expander.add_row(&row);
        rows.borrow_mut().push(row.upcast());
    }
    let add = adw::EntryRow::builder()
        .title(tr("Add: text, a space and its replacement"))
        .show_apply_button(true)
        .build();
    let refill = again(expander, rows);
    add.connect_apply(move |entry| {
        let text = entry.text();
        if let Some((from, to)) = text.trim().split_once(char::is_whitespace)
            && !to.trim().is_empty()
        {
            let replacement = Replacement::new(from, to.trim());
            change(|p| p.autocorrect.replacements.push(replacement));
            refill();
        }
    });
    expander.add_row(&add);
    rows.borrow_mut().push(add.upcast());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_17_preferences_go_to_a_key_file_and_back() {
        let preferences = Preferences {
            units: Units::Picas,
            percent: true,
            center_origin: true,
            vertical_ruler: true,
            automatic_lists: false,
            smart_quotes: false,
            check_spelling: false,
            proofread: true,
            autocorrect: AutoCorrect {
                ordinals: false,
                replacements: vec![
                    Replacement::new("(c)", "©"),
                    Replacement {
                        enabled: false,
                        ..Replacement::new("a b", "tab\there")
                    },
                ],
                ..AutoCorrect::default()
            },
            accept_single_suggestion: true,
            detect_links: false,
            zoom: 150,
            invisibles_color: Rgba {
                r: 200,
                g: 10,
                b: 0,
                a: 128,
            },
            show_counts: false,
            include_preview: false,
            guide_color: Rgba::rgb(10, 200, 30),
            show_size_and_position: false,
        };
        let file = glib::KeyFile::new();
        preferences.write(&file);
        assert_eq!(Preferences::read(&file), preferences);
        // What a file lacks keeps its default.
        let empty = glib::KeyFile::new();
        assert!(!Preferences::read(&empty).percent);
        assert!(Preferences::read(&empty).automatic_lists);
        assert!(Preferences::read(&empty).show_counts);
        assert!(Preferences::read(&empty).detect_links);
        assert!(Preferences::read(&empty).include_preview);
        assert_eq!(Preferences::read(&empty).zoom, 100);
        assert_eq!(Preferences::read(&empty).invisibles_color, INVISIBLES_COLOR);
        assert_eq!(Preferences::read(&empty).guide_color, GUIDE_COLOR);
        assert!(Preferences::read(&empty).show_size_and_position);
        assert_eq!(
            Preferences::read(&empty).autocorrect,
            AutoCorrect::default()
        );
    }

    #[test]
    fn fmt_17_units_measure_in_points() {
        assert_eq!(Units::Inches.points(), 72.0);
        assert_eq!(Units::Picas.points(), 12.0);
        assert!((Units::Centimeters.points() * 2.54 - 72.0).abs() < 1e-9);
        assert_eq!(Units::of_region("US"), Units::Inches);
        assert_eq!(Units::of_region("BR"), Units::Centimeters);
    }
}
