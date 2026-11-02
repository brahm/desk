// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preferences of the apps, which hold for every document (FMT-17).
//!
//! They live in a key file in the configuration folder of the user,
//! `desk/preferences.ini`, which the three apps share. Every window
//! follows a change at once.

use std::cell::RefCell;
use std::path::PathBuf;

use adw::prelude::*;
use gtk::glib;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

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
        }
    }
}

const RULERS: &str = "rulers";

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
        out
    }

    fn write(&self, file: &glib::KeyFile) {
        file.set_string(RULERS, "units", self.units.key());
        file.set_boolean(RULERS, "percent", self.percent);
        file.set_boolean(RULERS, "center-origin", self.center_origin);
        file.set_boolean(RULERS, "vertical-ruler", self.vertical_ruler);
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
    with_store(|s| s.preferences)
}

/// Changes the preferences, saves them and tells every listener.
pub fn set(preferences: Preferences) {
    if get() == preferences {
        return;
    }
    with_store(|s| s.preferences = preferences);
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
    let percent = adw::SwitchRow::builder()
        .title(tr("Show Units as Percentage"))
        .subtitle(tr("Rulers count in percentages of the width of the text"))
        .active(current.percent)
        .build();
    let center = adw::SwitchRow::builder()
        .title(tr("Place Origin at Center"))
        .subtitle(tr("Rulers count from the middle of the text"))
        .active(current.center_origin)
        .build();
    let vertical = adw::SwitchRow::builder()
        .title(tr("Show Vertical Ruler"))
        .subtitle(tr("Along with the horizontal ruler"))
        .active(current.vertical_ruler)
        .build();
    let rulers = adw::PreferencesGroup::builder()
        .title(tr("Rulers"))
        .description(tr("For every document"))
        .build();
    rulers.add(&units);
    rulers.add(&percent);
    rulers.add(&center);
    rulers.add(&vertical);
    let page = adw::PreferencesPage::builder()
        .title(tr("General"))
        .icon_name("preferences-system-symbolic")
        .build();
    page.add(&rulers);
    let dialog = adw::PreferencesDialog::builder()
        .title(tr("Preferences"))
        .build();
    dialog.add(&page);

    let update = {
        let (units, percent, center, vertical) = (
            units.downgrade(),
            percent.downgrade(),
            center.downgrade(),
            vertical.downgrade(),
        );
        move || {
            let (Some(units), Some(percent), Some(center), Some(vertical)) = (
                units.upgrade(),
                percent.upgrade(),
                center.upgrade(),
                vertical.upgrade(),
            ) else {
                return;
            };
            set(Preferences {
                units: Units::ALL
                    .get(units.selected() as usize)
                    .copied()
                    .unwrap_or(Units::Centimeters),
                percent: percent.is_active(),
                center_origin: center.is_active(),
                vertical_ruler: vertical.is_active(),
            });
        }
    };
    let changed = update.clone();
    units.connect_selected_notify(move |_| changed());
    for row in [&percent, &center, &vertical] {
        let changed = update.clone();
        row.connect_active_notify(move |_| changed());
    }
    dialog.present(Some(parent));
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
        };
        let file = glib::KeyFile::new();
        preferences.write(&file);
        assert_eq!(Preferences::read(&file), preferences);
        // What a file lacks keeps its default.
        let empty = glib::KeyFile::new();
        assert!(!Preferences::read(&empty).percent);
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
