// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Fonts window (INT-15), as the Fonts panel that Pages opens with
//! Show Fonts: collections, families, typefaces and sizes, a bar of
//! effects (underline, strikethrough, text and paragraph colors and a
//! shadow with opacity, blur, offset and angle, FMT-03), and the
//! typography of the font (FMT-08, FMT-09).
//!
//! It stays open beside the document, and shows and changes the
//! formatting at the selection.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use desk_model::{
    CharAttrs, CharFormat, FontFeature, FontFeatures, Length, LineStyle, ParaAttrs, ParaFormat,
    Rgba, Shadow,
};

use crate::effects::{Change, ColorRow, LIGATURES, ligatures_name};
use crate::formatting::{EditorOf, from_gdk, labelled, to_gdk};
use crate::i18n::tr;
use crate::tab_stops::string_list;

/// The sizes in the list, as in the Fonts panel.
const SIZES: [u32; 16] = [
    9, 10, 11, 12, 13, 14, 18, 24, 36, 48, 64, 72, 96, 144, 288, 1638,
];

/// The largest font size, as Pages takes it.
const MAX_SIZE: f64 = 1638.0;

/// The styles of lines in the menus of underline and strikethrough.
const LINES: [LineStyle; 3] = [LineStyle::None, LineStyle::Single, LineStyle::Double];

/// The faces the model holds, as bold and italic.
const FACES: [(bool, bool); 4] = [(false, false), (false, true), (true, false), (true, true)];

/// The features of OpenType fonts that the Typography section offers,
/// with their names (FMT-08).
fn features() -> [(&'static str, String); 12] {
    [
        ("onum", tr("Old-Style Figures")),
        ("lnum", tr("Lining Figures")),
        ("pnum", tr("Proportional Figures")),
        ("tnum", tr("Tabular Figures")),
        ("frac", tr("Fractions")),
        ("zero", tr("Slashed Zero")),
        ("smcp", tr("Small Capitals from Lower Case")),
        ("c2sc", tr("Small Capitals from Capitals")),
        ("case", tr("Case-Sensitive Forms")),
        ("swsh", tr("Swashes")),
        ("salt", tr("Stylistic Alternates")),
        ("ss01", tr("Stylistic Set 1")),
    ]
}

/// The collections of fonts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Collection {
    All,
    FixedWidth,
    Recent,
}

const COLLECTIONS: [Collection; 3] = [Collection::All, Collection::FixedWidth, Collection::Recent];

fn collection_name(collection: Collection) -> String {
    match collection {
        Collection::All => tr("All Fonts"),
        Collection::FixedWidth => tr("Fixed Width"),
        Collection::Recent => tr("Recently Used"),
    }
}

fn face_name(face: (bool, bool)) -> String {
    match face {
        (false, false) => tr("Regular"),
        (false, true) => tr("Italic"),
        (true, false) => tr("Bold"),
        (true, true) => tr("Bold Italic"),
    }
}

fn line_name(line: LineStyle) -> String {
    match line {
        LineStyle::None => tr("None"),
        LineStyle::Single => tr("Single"),
        LineStyle::Double => tr("Double"),
    }
}

thread_local! {
    /// The families chosen in this session, the latest first.
    static RECENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A list of names to choose one of.
fn name_list(title: &str) -> (gtk::ListBox, gtk::ScrolledWindow) {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(["navigation-sidebar"])
        .build();
    labelled(&list, title);
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .min_content_height(220)
        .build();
    (list, scrolled)
}

fn fill(list: &gtk::ListBox, names: &[String]) {
    list.remove_all();
    for name in names {
        let label = gtk::Label::builder()
            .label(name)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        list.append(&label);
    }
}

fn row_name(row: &gtk::ListBoxRow) -> Option<String> {
    row.child()
        .and_downcast::<gtk::Label>()
        .map(|l| l.label().to_string())
}

fn select_named(list: &gtk::ListBox, name: &str) {
    let mut i = 0;
    while let Some(row) = list.row_at_index(i) {
        if row_name(&row).is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            list.select_row(Some(&row));
            return;
        }
        i += 1;
    }
    list.unselect_all();
}

/// The Fonts window of a document window.
pub struct FontsWindow {
    window: gtk::Window,
    collections: gtk::ListBox,
    families: gtk::ListBox,
    faces: gtk::ListBox,
    sizes: gtk::ListBox,
    size: gtk::SpinButton,
    search: gtk::SearchEntry,
    underline: gtk::DropDown,
    strikethrough: gtk::DropDown,
    color: gtk::ColorDialogButton,
    background: ColorRow,
    shadow: adw::SwitchRow,
    opacity: adw::SpinRow,
    blur: adw::SpinRow,
    offset: adw::SpinRow,
    angle: adw::SpinRow,
    ligatures: adw::ComboRow,
    features: Vec<(&'static str, adw::SwitchRow)>,
    /// Every installed family and whether it has a fixed width.
    installed: Vec<(String, bool)>,
    shown: RefCell<Option<(CharFormat, ParaFormat)>>,
    updating: Cell<bool>,
}

impl FontsWindow {
    /// The Fonts window over a window, for its editor.
    pub fn new(parent: &impl IsA<gtk::Window>, editor: EditorOf) -> Rc<Self> {
        let mut installed: Vec<(String, bool)> = parent
            .upcast_ref::<gtk::Window>()
            .pango_context()
            .list_families()
            .iter()
            .map(|f| (f.name().to_string(), f.is_monospace()))
            .collect();
        installed.sort_by_key(|(name, _)| name.to_lowercase());
        installed.dedup_by(|a, b| a.0 == b.0);

        let (collections, collections_view) = name_list(&tr("Collection"));
        fill(&collections, &COLLECTIONS.map(collection_name));
        let (families, families_view) = name_list(&tr("Family"));
        let (faces, faces_view) = name_list(&tr("Typeface"));
        fill(&faces, &FACES.map(face_name));
        let (sizes, sizes_view) = name_list(&tr("Size"));
        fill(&sizes, &SIZES.map(|s| s.to_string()));
        let size = gtk::SpinButton::with_range(1.0, MAX_SIZE, 1.0);
        size.set_digits(1);
        labelled(&size, &tr("Font Size"));
        let search = gtk::SearchEntry::builder()
            .placeholder_text(tr("Search Fonts"))
            .build();

        let column = |title: &str, top: Option<&gtk::Widget>, view: &gtk::ScrolledWindow| {
            let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
            column.append(
                &gtk::Label::builder()
                    .label(title)
                    .xalign(0.0)
                    .css_classes(["heading"])
                    .build(),
            );
            if let Some(top) = top {
                column.append(top);
            }
            column.append(view);
            column
        };
        let lists = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        lists.append(&column(&tr("Collection"), None, &collections_view));
        lists.append(&column(
            &tr("Family"),
            Some(search.upcast_ref()),
            &families_view,
        ));
        lists.append(&column(&tr("Typeface"), None, &faces_view));
        lists.append(&column(&tr("Size"), Some(size.upcast_ref()), &sizes_view));

        // The bar of effects, over the lists, as in the Fonts panel.
        let underline = gtk::DropDown::new(
            Some(string_list(LINES.map(line_name))),
            None::<gtk::Expression>,
        );
        underline.set_tooltip_text(Some(&tr("Underline")));
        labelled(&underline, &tr("Underline"));
        let strikethrough = gtk::DropDown::new(
            Some(string_list(LINES.map(line_name))),
            None::<gtk::Expression>,
        );
        strikethrough.set_tooltip_text(Some(&tr("Strikethrough")));
        labelled(&strikethrough, &tr("Strikethrough"));
        let color = gtk::ColorDialogButton::builder()
            .dialog(&gtk::ColorDialog::builder().title(tr("Text Color")).build())
            .tooltip_text(tr("Text Color"))
            .build();
        labelled(&color, &tr("Text Color"));
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        for (label, widget) in [
            (tr("Underline"), underline.upcast_ref::<gtk::Widget>()),
            (tr("Strikethrough"), strikethrough.upcast_ref()),
            (tr("Text"), color.upcast_ref()),
        ] {
            bar.append(&gtk::Label::new(Some(&label)));
            bar.append(widget);
        }

        let background = ColorRow::new(&tr("Paragraph Background"), Rgba::rgb(255, 255, 117));
        let shadow = adw::SwitchRow::builder().title(tr("Shadow")).build();
        let spin = |title: String, subtitle: String, min: f64, max: f64, step: f64| {
            let row = adw::SpinRow::with_range(min, max, step);
            row.set_title(&title);
            row.set_subtitle(&subtitle);
            row
        };
        let opacity = spin(tr("Shadow Opacity"), tr("Percent"), 0.0, 100.0, 1.0);
        let blur = spin(tr("Shadow Blur"), tr("Points"), 0.0, 100.0, 1.0);
        let offset = spin(tr("Shadow Offset"), tr("Points"), 0.0, 100.0, 1.0);
        let angle = spin(tr("Shadow Angle"), tr("Degrees"), 0.0, 359.0, 1.0);
        angle.set_wrap(true);
        let effects = adw::PreferencesGroup::builder()
            .title(tr("Effects"))
            .build();
        effects.add(&background.row);
        effects.add(&shadow);
        effects.add(&opacity);
        effects.add(&blur);
        effects.add(&offset);
        effects.add(&angle);

        let ligatures = adw::ComboRow::builder()
            .title(tr("Ligatures"))
            .model(&string_list(LIGATURES.map(ligatures_name)))
            .build();
        let typography = adw::PreferencesGroup::builder()
            .title(tr("Typography"))
            .description(tr("Features of the font, where it has them"))
            .build();
        typography.add(&ligatures);
        let expander = adw::ExpanderRow::builder()
            .title(tr("Font Features"))
            .build();
        let features: Vec<(&'static str, adw::SwitchRow)> = features()
            .into_iter()
            .map(|(tag, name)| {
                let row = adw::SwitchRow::builder().title(name).subtitle(tag).build();
                expander.add_row(&row);
                (tag, row)
            })
            .collect();
        typography.add(&expander);

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        content.append(&bar);
        content.append(&lists);
        content.append(&effects);
        content.append(&typography);
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&content)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&scrolled));
        let window = gtk::Window::builder()
            .title(tr("Fonts"))
            .transient_for(parent)
            .default_width(640)
            .default_height(640)
            .hide_on_close(true)
            .child(&toolbar)
            .build();

        let this = Rc::new(Self {
            window,
            collections,
            families,
            faces,
            sizes,
            size,
            search,
            underline,
            strikethrough,
            color,
            background,
            shadow,
            opacity,
            blur,
            offset,
            angle,
            ligatures,
            features,
            installed,
            shown: RefCell::new(None),
            updating: Cell::new(false),
        });
        this.collections
            .select_row(this.collections.row_at_index(0).as_ref());
        this.show_families();
        this.connect(editor);
        this
    }

    /// The families of the collection chosen that match the search.
    fn show_families(&self) {
        let collection = self
            .collections
            .selected_row()
            .map_or(Collection::All, |r| {
                COLLECTIONS[(r.index().max(0) as usize).min(COLLECTIONS.len() - 1)]
            });
        let query = self.search.text().to_lowercase();
        let names: Vec<String> = match collection {
            Collection::Recent => RECENT.with(|r| r.borrow().clone()),
            _ => self
                .installed
                .iter()
                .filter(|(_, mono)| collection == Collection::All || *mono)
                .map(|(name, _)| name.clone())
                .collect(),
        };
        let names: Vec<String> = names
            .into_iter()
            .filter(|n| n.to_lowercase().contains(&query))
            .collect();
        let was = self.updating.replace(true);
        fill(&self.families, &names);
        if let Some((character, _)) = &*self.shown.borrow() {
            select_named(&self.families, &character.font_family);
        }
        self.updating.set(was);
    }

    fn connect(self: &Rc<Self>, editor: EditorOf) {
        // Calls `f` with the editor, unless the window is being shown.
        let with = |this: &Rc<Self>, f: Change<Self>| {
            let weak = Rc::downgrade(this);
            let editor = editor.clone();
            move || {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    f(&this, &editor);
                }
            }
        };
        let character = |name: &'static str, attrs: fn(&Self) -> Option<CharAttrs>| {
            with(
                self,
                Rc::new(move |this, editor| {
                    if let Some(attrs) = attrs(this) {
                        editor.set_character_attrs(name, &attrs);
                    }
                }),
            )
        };

        let weak = Rc::downgrade(self);
        let changed = move || {
            if let Some(this) = weak.upgrade() {
                this.show_families();
            }
        };
        let again = changed.clone();
        self.collections.connect_row_selected(move |_, _| again());
        self.search.connect_search_changed(move |_| changed());

        let family = character("Font", |this| {
            let name = this.families.selected_row().and_then(|r| row_name(&r))?;
            RECENT.with(|r| {
                let mut recent = r.borrow_mut();
                recent.retain(|n| n != &name);
                recent.insert(0, name.clone());
                recent.truncate(20);
            });
            Some(CharAttrs {
                font_family: Some(Arc::from(name.as_str())),
                ..CharAttrs::default()
            })
        });
        self.families.connect_row_selected(move |_, row| {
            if row.is_some() {
                family();
            }
        });
        let face = character("Typeface", |this| {
            let row = this.faces.selected_row()?;
            let (bold, italic) = FACES[(row.index().max(0) as usize).min(FACES.len() - 1)];
            Some(CharAttrs {
                bold: Some(bold),
                italic: Some(italic),
                ..CharAttrs::default()
            })
        });
        self.faces.connect_row_selected(move |_, row| {
            if row.is_some() {
                face();
            }
        });
        let weak = Rc::downgrade(self);
        self.sizes.connect_row_selected(move |_, row| {
            let (Some(this), Some(row)) = (weak.upgrade(), row) else {
                return;
            };
            if this.updating.get() {
                return;
            }
            if let Some(size) = row_name(row).and_then(|n| n.parse::<f64>().ok()) {
                this.size.set_value(size);
            }
        });
        let size = character("Font Size", |this| {
            Some(CharAttrs {
                font_size: Some(Length::from_pt(this.size.value())),
                ..CharAttrs::default()
            })
        });
        self.size.connect_value_changed(move |_| size());
        let underline = character("Underline", |this| {
            Some(CharAttrs {
                underline: LINES.get(this.underline.selected() as usize).copied(),
                ..CharAttrs::default()
            })
        });
        self.underline.connect_selected_notify(move |_| underline());
        let strikethrough = character("Strikethrough", |this| {
            Some(CharAttrs {
                strikethrough: LINES.get(this.strikethrough.selected() as usize).copied(),
                ..CharAttrs::default()
            })
        });
        self.strikethrough
            .connect_selected_notify(move |_| strikethrough());
        let color = character("Text Color", |this| {
            Some(CharAttrs {
                color: Some(from_gdk(&this.color.rgba())),
                ..CharAttrs::default()
            })
        });
        self.color.connect_rgba_notify(move |_| color());
        self.background.connect_changed(with(
            self,
            Rc::new(|this, editor| {
                this.background
                    .button
                    .set_sensitive(this.background.switch.is_active());
                editor.set_paragraph_attrs(
                    "Paragraph Background",
                    &ParaAttrs {
                        background: Some(this.background.value()),
                        ..ParaAttrs::default()
                    },
                );
            }),
        ));
        let shadow = character("Shadow", |this| {
            let on = this.shadow.is_active();
            this.show_shadow_rows(on);
            Some(CharAttrs {
                shadow: Some(on.then(|| {
                    Shadow {
                        angle: this.angle.value().round() as i32,
                        offset: Length::from_pt(this.offset.value()),
                        blur: Length::from_pt(this.blur.value()),
                        opacity: this.opacity.value().round() as u8,
                        color: this
                            .shown
                            .borrow()
                            .as_ref()
                            .and_then(|(c, _)| c.shadow)
                            .map_or(Rgba::BLACK, |s| s.color),
                    }
                })),
                ..CharAttrs::default()
            })
        });
        let again = shadow.clone();
        self.shadow.connect_active_notify(move |_| again());
        for row in [&self.opacity, &self.blur, &self.offset, &self.angle] {
            let again = shadow.clone();
            row.connect_value_notify(move |_| again());
        }
        let ligatures = character("Ligatures", |this| {
            Some(CharAttrs {
                ligatures: LIGATURES.get(this.ligatures.selected() as usize).copied(),
                ..CharAttrs::default()
            })
        });
        self.ligatures.connect_selected_notify(move |_| ligatures());
        for (tag, row) in &self.features {
            let tag = *tag;
            let changed = with(
                self,
                Rc::new(move |this, editor| {
                    let on = this
                        .features
                        .iter()
                        .find(|(t, _)| *t == tag)
                        .is_some_and(|(_, r)| r.is_active());
                    let current = this
                        .shown
                        .borrow()
                        .as_ref()
                        .map_or_else(FontFeatures::none, |(c, _)| c.font_features.clone());
                    let features = if on {
                        FontFeature::new(tag, 1).map_or(current.clone(), |f| current.with(f))
                    } else {
                        current.without(tag)
                    };
                    editor.set_character_attrs(
                        "Typography",
                        &CharAttrs {
                            font_features: Some(features),
                            ..CharAttrs::default()
                        },
                    );
                }),
            );
            row.connect_active_notify(move |_| changed());
        }
    }

    fn show_shadow_rows(&self, on: bool) {
        for row in [&self.opacity, &self.blur, &self.offset, &self.angle] {
            row.set_sensitive(on);
        }
    }

    /// Shows the window, over its document window.
    pub fn present(&self) {
        self.window.present();
    }

    /// Whether the window is showing.
    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    /// Shows the formatting at the selection.
    pub fn show(&self, character: &CharFormat, paragraph: &ParaFormat) {
        let shown = (character.clone(), paragraph.clone());
        if self.shown.borrow().as_ref() == Some(&shown) {
            return;
        }
        *self.shown.borrow_mut() = Some(shown);
        self.updating.set(true);
        select_named(&self.families, &character.font_family);
        let face = FACES
            .iter()
            .position(|f| *f == (character.bold, character.italic))
            .unwrap_or(0);
        self.faces
            .select_row(self.faces.row_at_index(face as i32).as_ref());
        let size = character.font_size.to_pt();
        if (self.size.value() - size).abs() > 0.01 {
            self.size.set_value(size);
        }
        select_named(&self.sizes, &format!("{}", size));
        let line = |style| LINES.iter().position(|l| *l == style).unwrap_or(0) as u32;
        self.underline.set_selected(line(character.underline));
        self.strikethrough
            .set_selected(line(character.strikethrough));
        let color = to_gdk(character.color);
        if self.color.rgba() != color {
            self.color.set_rgba(&color);
        }
        self.background.show(paragraph.background);
        let shadow = character.shadow;
        self.shadow.set_active(shadow.is_some());
        let shadow = shadow.unwrap_or_default();
        self.opacity.set_value(f64::from(shadow.opacity));
        self.blur.set_value(shadow.blur.to_pt());
        self.offset.set_value(shadow.offset.to_pt());
        self.angle.set_value(f64::from(shadow.angle));
        self.show_shadow_rows(character.shadow.is_some());
        self.ligatures.set_selected(
            LIGATURES
                .iter()
                .position(|l| *l == character.ligatures)
                .unwrap_or(0) as u32,
        );
        for (tag, row) in &self.features {
            row.set_active(character.font_features.get(tag).is_some_and(|v| v > 0));
        }
        self.updating.set(false);
    }
}

/// Keeps the Fonts window of a document window, made when first shown.
#[derive(Default)]
pub struct FontsSlot(RefCell<Option<Rc<FontsWindow>>>);

impl FontsSlot {
    /// Shows the Fonts window, making it the first time.
    pub fn present(&self, parent: &impl IsA<gtk::Window>, editor: EditorOf) -> Rc<FontsWindow> {
        let fonts = self
            .0
            .borrow_mut()
            .get_or_insert_with(|| FontsWindow::new(parent, editor))
            .clone();
        fonts.present();
        fonts
    }

    /// The Fonts window, if it is showing.
    pub fn visible(&self) -> Option<Rc<FontsWindow>> {
        self.0.borrow().clone().filter(|f| f.is_visible())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_15_fmt_08_choices_have_names_and_valid_tags() {
        assert!(FACES.map(face_name).iter().all(|n| !n.is_empty()));
        assert!(LINES.map(line_name).iter().all(|n| !n.is_empty()));
        assert!(
            COLLECTIONS
                .map(collection_name)
                .iter()
                .all(|n| !n.is_empty())
        );
        for (tag, _) in features() {
            assert!(FontFeature::new(tag, 1).is_some(), "{tag}");
        }
        assert!(SIZES.windows(2).all(|w| w[0] < w[1]));
    }
}
