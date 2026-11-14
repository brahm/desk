// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The format bar under the header bar and the Inspector at the side of a
//! window, as in Pages: they show the formatting at the selection and
//! change it.
//!
//! The toggles of the format bar (bold, italic, underline, strikethrough
//! and alignment) are actions of the window, which the keyboard shortcuts
//! also run; the rest calls the editor through `editor`.

use crate::compact;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Alignment, CharAttrs, Length, LineSpacing, ParaAttrs, Rgba, StyleId};
use gtk::{gdk, glib};

use crate::editing::{Format, TextEditor};
use crate::effects::Effects;
use crate::i18n::{n_, tr};
use crate::layout_pane::LayoutGroups;
use crate::lists_pane::ListsGroup;
use crate::preferences::{self, Units};
use crate::tab_stops::{self, TabStopsGroup};

/// Finds the editor of the window, if it shows a document.
pub type EditorOf = Rc<dyn Fn() -> Option<TextEditor>>;

/// The largest font size Pages takes, in points.
const MAX_FONT_SIZE: f64 = 1638.0;

/// The alignment as the target of the `win.align` action.
pub fn alignment_name(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Left | Alignment::Auto => "left",
        Alignment::Center => "center",
        Alignment::Right => "right",
        Alignment::Justify => "justify",
    }
}

/// The alignment that a target of the `win.align` action names.
pub fn alignment_of(name: &str) -> Option<Alignment> {
    Some(match name {
        "left" => Alignment::Left,
        "center" => Alignment::Center,
        "right" => Alignment::Right,
        "justify" => Alignment::Justify,
        _ => return None,
    })
}

/// A color of the model as GDK takes it.
pub fn to_gdk(color: Rgba) -> gdk::RGBA {
    gdk::RGBA::new(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        f32::from(color.a) / 255.0,
    )
}

/// A color of GDK as the model keeps it.
pub fn from_gdk(color: &gdk::RGBA) -> Rgba {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgba {
        r: byte(color.red()),
        g: byte(color.green()),
        b: byte(color.blue()),
        a: byte(color.alpha()),
    }
}

/// The format bar.
pub struct FormatBar {
    root: gtk::Box,
    styles: gtk::DropDown,
    style_ids: RefCell<Vec<StyleId>>,
    /// The character styles, after none, as the format bar of Pages has
    /// them (STY-02).
    characters: gtk::DropDown,
    character_ids: RefCell<Vec<Option<StyleId>>>,
    /// The font family, by name: a font the document names stays named
    /// here even when it is not installed, as in Pages.
    font: gtk::Button,
    size: gtk::SpinButton,
    color: gtk::ColorDialogButton,
    /// What a narrow window leaves out: the alignments and the columns,
    /// which the Inspector and the Format menu also have.
    secondary: [gtk::Widget; 3],
    /// Set while the bar shows the formatting at the selection, so that
    /// showing it does not change it.
    updating: Cell<bool>,
}

impl FormatBar {
    /// A format bar for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let root = gtk::Box::builder()
            .spacing(6)
            .margin_start(6)
            .margin_end(6)
            .margin_top(3)
            .margin_bottom(3)
            .css_classes(["toolbar"])
            .build();

        let styles = gtk::DropDown::builder()
            .model(&gtk::StringList::new(&[]))
            .tooltip_text(tr("Paragraph Style"))
            .enable_search(true)
            .build();
        styles.update_property(&[gtk::accessible::Property::Label(&tr("Paragraph Style"))]);
        root.append(&styles);
        let characters = gtk::DropDown::builder()
            .model(&gtk::StringList::new(&[]))
            .tooltip_text(tr("Character Style"))
            .enable_search(true)
            .build();
        // The button shows the name cut short, so that a long name does
        // not widen the bar; the list shows names whole.
        characters.set_factory(Some(&label_factory(Some(12))));
        characters.set_list_factory(Some(&label_factory(None)));
        characters.update_property(&[gtk::accessible::Property::Label(&tr("Character Style"))]);
        root.append(&characters);

        let font = gtk::Button::builder()
            .action_name("win.fonts")
            .tooltip_text(tr("Font"))
            .width_request(140)
            .build();
        root.append(&font);

        let size = gtk::SpinButton::with_range(1.0, MAX_FONT_SIZE, 1.0);
        size.set_digits(0);
        size.set_tooltip_text(Some(&tr("Font Size")));
        size.update_property(&[gtk::accessible::Property::Label(&tr("Font Size"))]);
        root.append(&size);

        let color = gtk::ColorDialogButton::new(Some(
            gtk::ColorDialog::builder()
                .title(tr("Text Color"))
                .modal(true)
                .with_alpha(true)
                .build(),
        ));
        color.set_tooltip_text(Some(&tr("Text Color")));
        labelled(&color, &tr("Text Color"));
        let colors = gtk::Box::builder().css_classes(["linked"]).build();
        colors.append(&color);
        colors.append(&crate::eyedropper::button(&color));
        root.append(&colors);

        let toggles = gtk::Box::builder().css_classes(["linked"]).build();
        for (icon, action, tooltip) in [
            ("format-text-bold-symbolic", "win.bold", n_("Bold")),
            ("format-text-italic-symbolic", "win.italic", n_("Italic")),
            (
                "format-text-underline-symbolic",
                "win.underline",
                n_("Underline"),
            ),
            (
                "format-text-strikethrough-symbolic",
                "win.strikethrough",
                n_("Strikethrough"),
            ),
        ] {
            let button = gtk::ToggleButton::builder()
                .icon_name(icon)
                .action_name(action)
                .tooltip_text(tr(tooltip))
                .build();
            labelled(&button, &tr(tooltip));
            toggles.append(&button);
        }
        root.append(&toggles);

        let alignments = gtk::Box::builder().css_classes(["linked"]).build();
        for (icon, target, tooltip) in [
            ("format-justify-left-symbolic", "left", n_("Align Left")),
            ("format-justify-center-symbolic", "center", n_("Center")),
            ("format-justify-right-symbolic", "right", n_("Align Right")),
            ("format-justify-fill-symbolic", "justify", n_("Justify")),
        ] {
            let button = gtk::ToggleButton::builder()
                .icon_name(icon)
                .tooltip_text(tr(tooltip))
                .build();
            labelled(&button, &tr(tooltip));
            button.set_action_name(Some("win.align"));
            button.set_action_target_value(Some(&target.to_variant()));
            alignments.append(&button);
        }
        root.append(&alignments);

        // Up to four columns of the layout at the cursor, as the toolbar
        // of Pages offers them; the Inspector has more (EST-07).
        let columns_menu = gtk::gio::Menu::new();
        for (count, label) in [
            (1, n_("1 Column")),
            (2, n_("2 Columns")),
            (3, n_("3 Columns")),
            (4, n_("4 Columns")),
        ] {
            let item = gtk::gio::MenuItem::new(Some(&tr(label)), None);
            item.set_action_and_target_value(Some("win.columns"), Some(&count.to_variant()));
            columns_menu.append_item(&item);
        }
        let columns = gtk::MenuButton::builder()
            .icon_name("view-dual-symbolic")
            .menu_model(&columns_menu)
            .tooltip_text(tr("Columns"))
            .build();
        labelled(&columns, &tr("Columns"));
        root.append(&columns);

        let spacer = gtk::Box::builder().hexpand(true).build();
        root.append(&spacer);
        let format = gtk::MenuButton::builder()
            .label(tr("Format"))
            .menu_model(&crate::window::format_menu())
            .tooltip_text(tr("Format"))
            .build();
        format.set_always_show_arrow(true);
        root.append(&format);

        let this = Rc::new(Self {
            root,
            styles,
            style_ids: RefCell::new(Vec::new()),
            characters: characters.clone(),
            character_ids: RefCell::new(Vec::new()),
            font,
            size,
            color,
            secondary: [alignments.upcast(), columns.upcast(), characters.upcast()],
            updating: Cell::new(false),
        });
        this.connect(editor);
        this
    }

    /// Leaves out the secondary controls, on a narrow window.
    pub fn set_compact(&self, compact: bool) {
        for widget in &self.secondary {
            widget.set_visible(!compact);
        }
    }

    /// The font button, where the Fonts popover opens.
    pub fn font_button(&self) -> &gtk::Button {
        &self.font
    }

    /// Opens the color chooser of the text color.
    pub fn choose_color(&self) {
        self.color.activate();
    }

    /// The widget to put under the header bar.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn connect(self: &Rc<Self>, editor: EditorOf) {
        let weak = Rc::downgrade(self);
        let of = editor.clone();
        self.styles.connect_selected_notify(move |dropdown| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let id = this
                .style_ids
                .borrow()
                .get(dropdown.selected() as usize)
                .copied();
            if let (Some(id), Some(editor)) = (id, of()) {
                editor.set_paragraph_style(id);
                editor.focus();
            }
        });

        let weak = Rc::downgrade(self);
        let of = editor.clone();
        self.characters.connect_selected_notify(move |dropdown| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let id = this
                .character_ids
                .borrow()
                .get(dropdown.selected() as usize)
                .copied();
            if let (Some(id), Some(editor)) = (id, of()) {
                editor.set_character_style(id);
                editor.focus();
            }
        });

        let weak = Rc::downgrade(self);
        let of = editor.clone();
        self.size.connect_value_changed(move |spin| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = of() {
                editor.set_character_attrs(
                    "Font Size",
                    &CharAttrs {
                        font_size: Some(Length::from_pt(spin.value())),
                        ..CharAttrs::default()
                    },
                );
            }
        });

        let weak = Rc::downgrade(self);
        self.color.connect_rgba_notify(move |button| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = editor() {
                editor.set_character_attrs(
                    "Text Color",
                    &CharAttrs {
                        color: Some(from_gdk(&button.rgba())),
                        ..CharAttrs::default()
                    },
                );
                editor.focus();
            }
        });
    }

    /// Shows the paragraph styles of a document. Returns whether they
    /// changed.
    pub fn set_styles(&self, styles: &[(StyleId, String)]) -> bool {
        let ids: Vec<StyleId> = styles.iter().map(|(id, _)| *id).collect();
        let model = self
            .styles
            .model()
            .and_then(|m| m.downcast::<gtk::StringList>().ok());
        let same_names = model.is_some_and(|m| {
            m.n_items() as usize == styles.len()
                && styles
                    .iter()
                    .enumerate()
                    .all(|(i, (_, n))| m.string(i as u32).as_deref() == Some(n.as_str()))
        });
        if same_names && *self.style_ids.borrow() == ids {
            return false;
        }
        self.updating.set(true);
        let names: Vec<&str> = styles.iter().map(|(_, n)| n.as_str()).collect();
        self.styles.set_model(Some(&gtk::StringList::new(&names)));
        *self.style_ids.borrow_mut() = ids;
        self.updating.set(false);
        true
    }

    /// Shows the character styles of a document, after none. Returns
    /// whether they changed.
    pub fn set_character_styles(&self, styles: &[(StyleId, String)]) -> bool {
        let ids: Vec<Option<StyleId>> = std::iter::once(None)
            .chain(styles.iter().map(|(id, _)| Some(*id)))
            .collect();
        let mut names = vec![tr("No Character Style")];
        names.extend(styles.iter().map(|(_, n)| n.clone()));
        let model = self
            .characters
            .model()
            .and_then(|m| m.downcast::<gtk::StringList>().ok());
        let same_names = model.is_some_and(|m| {
            m.n_items() as usize == names.len()
                && names
                    .iter()
                    .enumerate()
                    .all(|(i, n)| m.string(i as u32).as_deref() == Some(n.as_str()))
        });
        if same_names && *self.character_ids.borrow() == ids {
            return false;
        }
        self.updating.set(true);
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        self.characters
            .set_model(Some(&gtk::StringList::new(&names)));
        *self.character_ids.borrow_mut() = ids;
        self.updating.set(false);
        true
    }

    /// Shows the character style at the selection.
    pub fn show_character_style(&self, style: Option<StyleId>) {
        self.updating.set(true);
        let index = self
            .character_ids
            .borrow()
            .iter()
            .position(|id| *id == style);
        self.characters
            .set_selected(index.map_or(gtk::INVALID_LIST_POSITION, |i| i as u32));
        self.updating.set(false);
    }

    /// Shows the formatting at the selection.
    pub fn show(&self, format: &Format) {
        self.updating.set(true);
        let index = format
            .style
            .and_then(|s| self.style_ids.borrow().iter().position(|id| *id == s));
        self.styles
            .set_selected(index.map_or(gtk::INVALID_LIST_POSITION, |i| i as u32));
        let character = &format.character;
        self.font.set_label(&character.font_family);
        let size = character.font_size.to_pt();
        if (self.size.value() - size).abs() > 0.01 {
            self.size.set_value(size);
        }
        let color = to_gdk(character.color);
        if self.color.rgba() != color {
            self.color.set_rgba(&color);
        }
        self.updating.set(false);
    }
}

/// The Inspector: the spacing and indents of the paragraphs (the Text
/// inspector of Pages).
pub struct Inspector {
    root: gtk::Box,
    /// The panes, kept for the handlers that follow what is selected.
    _panes: Rc<InspectorPanes>,
    line_spacing: compact::SpinRow,
    before: compact::SpinRow,
    after: compact::SpinRow,
    first_line: compact::SpinRow,
    left: compact::SpinRow,
    right: compact::SpinRow,
    /// Whether words break with hyphens (TXT-09).
    hyphenate: compact::SwitchRow,
    /// Whether the document is hyphenated and has ligatures at all.
    document_hyphenation: compact::SwitchRow,
    document_ligatures: compact::SwitchRow,
    /// Whether pages face each other (EST-11).
    facing_pages: compact::SwitchRow,
    tabs: Rc<TabStopsGroup>,
    layouts: Rc<LayoutGroups>,
    headers: Rc<crate::headers_pane::HeadersGroup>,
    notes: Rc<crate::notes_pane::NotesGroup>,
    links: Rc<crate::links_pane::LinksGroup>,
    toc: Rc<crate::links_pane::TocGroup>,
    lists: Rc<ListsGroup>,
    effects: Rc<Effects>,
    /// The Metrics and Wrap inspectors of the selected objects.
    objects: Rc<crate::objects_pane::ObjectGroups>,
    /// The units of the indents, those of the rulers.
    units: Cell<Units>,
    /// The paragraph format shown, to show again in other units.
    shown: RefCell<Option<Format>>,
    updating: Cell<bool>,
}

/// What a row of the Inspector sets.
#[derive(Clone, Copy)]
enum Field {
    LineSpacing,
    Before,
    After,
    FirstLine,
    Left,
    Right,
}

impl Field {
    fn name(self) -> &'static str {
        match self {
            Self::LineSpacing => "Line Spacing",
            Self::Before => "Space Before",
            Self::After => "Space After",
            Self::FirstLine | Self::Left | Self::Right => "Indent",
        }
    }

    /// Whether the field is a length in the units of the rulers; the
    /// others are in lines or points, as in Pages.
    fn in_units(self) -> bool {
        matches!(self, Self::FirstLine | Self::Left | Self::Right)
    }

    /// The attributes for a value of the field, in its units.
    fn attrs(self, value: f64, units: Units) -> ParaAttrs {
        let length = Some(if self.in_units() {
            tab_stops::from_units(value, units)
        } else {
            Length::from_pt(value)
        });
        let mut attrs = ParaAttrs::default();
        match self {
            Self::LineSpacing => {
                attrs.line_spacing = Some(LineSpacing::Multiple {
                    percent: (value * 100.0).round() as u32,
                });
            }
            Self::Before => attrs.space_before = length,
            Self::After => attrs.space_after = length,
            Self::FirstLine => attrs.first_line_indent = length,
            Self::Left => attrs.left_indent = length,
            Self::Right => attrs.right_indent = length,
        }
        attrs
    }
}

impl Inspector {
    /// An Inspector for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let row = |title: &str, subtitle: &str, min: f64, max: f64, step: f64, digits: u32| {
            let row = compact::SpinRow::with_range(min, max, step);
            row.set_title(title);
            row.set_subtitle(subtitle);
            row.set_digits(digits);
            row
        };
        let line_spacing = row(&tr("Line Spacing"), &tr("Lines"), 0.5, 10.0, 0.1, 1);
        let before = row(&tr("Before Paragraph"), &tr("Points"), 0.0, 1584.0, 1.0, 0);
        let after = row(&tr("After Paragraph"), &tr("Points"), 0.0, 1584.0, 1.0, 0);
        let units = preferences::get().units;
        let first_line = row(&tr("First Line"), "", 0.0, 1.0, 1.0, 0);
        let left = row(&tr("Left"), "", 0.0, 1.0, 1.0, 0);
        let right = row(&tr("Right"), "", 0.0, 1.0, 1.0, 0);
        let tabs = TabStopsGroup::new(editor.clone(), units);
        let lists = ListsGroup::new(editor.clone(), units);
        let effects = Effects::new(editor.clone());
        let layouts = LayoutGroups::new(editor.clone(), units);
        let headers = crate::headers_pane::HeadersGroup::new(editor.clone(), units);
        let notes = crate::notes_pane::NotesGroup::new(editor.clone(), units);
        let links = crate::links_pane::LinksGroup::new(editor.clone());
        let toc = crate::links_pane::TocGroup::new(editor.clone());
        let objects = crate::objects_pane::ObjectGroups::new(editor.clone(), units);

        let spacing = compact::Group::builder().title(tr("Spacing")).build();
        spacing.add(&line_spacing);
        spacing.add(&before);
        spacing.add(&after);
        let indents = compact::Group::builder().title(tr("Indents")).build();
        indents.add(&first_line);
        indents.add(&left);
        indents.add(&right);
        let hyphenate = compact::SwitchRow::builder()
            .title(tr("Hyphenate Words"))
            .subtitle(tr("At the ends of lines, in the language of the text"))
            .build();
        let hyphenation = compact::Group::builder().title(tr("Hyphenation")).build();
        hyphenation.add(&hyphenate);
        let document_hyphenation = compact::SwitchRow::builder()
            .title(tr("Hyphenate the Document"))
            .subtitle(tr("In every paragraph that allows it"))
            .build();
        let document_ligatures = compact::SwitchRow::builder()
            .title(tr("Use Ligatures"))
            .subtitle(tr("In the whole document"))
            .build();
        let facing_pages = compact::SwitchRow::builder()
            .title(tr("Facing Pages"))
            .subtitle(tr("Left and right pages swap their margins, as in a book"))
            .build();
        let document = compact::Group::builder().title(tr("Document")).build();
        document.add(&document_hyphenation);
        document.add(&document_ligatures);
        document.add(&facing_pages);
        // The panes of the Inspector of Pages, in its order (C of the
        // interface plan). Table, Chart and QuickTime show only when such
        // an object is selected; Desk Writer has QuickTime so far.
        let [metrics, shape, picture, media, graphic, wrap, text_box] = objects.groups();
        let [character, text_shadow, borders] = effects.groups();
        let [page, layout, section] = layouts.groups();
        let [link, bookmarks] = links.groups();
        let panes = InspectorPanes::new(&[
            (
                "document",
                n_("Document"),
                "x-office-document-symbolic",
                &[
                    page,
                    &document,
                    headers.widget(),
                    notes.widget(),
                    toc.widget(),
                ],
            ),
            (
                "layout",
                n_("Layout"),
                "view-dual-symbolic",
                &[layout, section],
            ),
            ("wrap", n_("Wrap"), "format-indent-more-symbolic", &[wrap]),
            (
                "text",
                n_("Text"),
                "insert-text-symbolic",
                &[
                    &spacing,
                    &indents,
                    lists.widget(),
                    tabs.widget(),
                    &hyphenation,
                    character,
                    borders,
                    text_box,
                ],
            ),
            (
                "graphic",
                n_("Graphic"),
                "applications-graphics-symbolic",
                &[graphic, shape, picture, text_shadow],
            ),
            (
                "metrics",
                n_("Metrics"),
                "zoom-fit-best-symbolic",
                &[metrics],
            ),
            (
                "link",
                n_("Link"),
                "insert-link-symbolic",
                &[link, bookmarks],
            ),
            (
                "quicktime",
                n_("QuickTime"),
                "video-x-generic-symbolic",
                &[media],
            ),
        ]);
        panes.show_only_with("quicktime", media);
        let root = panes.widget().clone();

        let this = Rc::new(Self {
            root,
            _panes: panes,
            line_spacing,
            before,
            after,
            first_line,
            left,
            right,
            hyphenate,
            document_hyphenation,
            document_ligatures,
            facing_pages,
            tabs,
            layouts,
            headers,
            notes,
            links,
            toc,
            lists,
            effects,
            objects,
            units: Cell::new(units),
            shown: RefCell::new(None),
            updating: Cell::new(false),
        });
        this.set_units(units);
        let weak = Rc::downgrade(&this);
        preferences::connect_changed(move |p| {
            if let Some(this) = weak.upgrade() {
                this.set_units(p.units);
            }
        });
        let weak = Rc::downgrade(&this);
        let hyphenate_editor = editor.clone();
        this.hyphenate.connect_active_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = hyphenate_editor() {
                editor.set_paragraph_attrs(
                    "Hyphenation",
                    &ParaAttrs {
                        hyphenate: Some(row.is_active()),
                        ..ParaAttrs::default()
                    },
                );
            }
        });
        for (row, hyphenation) in [
            (&this.document_hyphenation, true),
            (&this.document_ligatures, false),
        ] {
            let weak = Rc::downgrade(&this);
            let editor = editor.clone();
            row.connect_active_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let Some(editor) = editor() else { return };
                let on = row.is_active();
                if hyphenation {
                    editor.change_text_settings("Hyphenation", |s| s.hyphenation = on);
                } else {
                    editor.change_text_settings("Ligatures", |s| s.ligatures = on);
                }
            });
        }
        let weak = Rc::downgrade(&this);
        let facing_editor = editor.clone();
        this.facing_pages.connect_active_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = facing_editor() {
                let setup = desk_model::PageSetup {
                    facing_pages: row.is_active(),
                    ..editor.format().setup
                };
                editor.set_page_setup("Facing Pages", setup);
            }
        });
        for (row, field) in [
            (&this.line_spacing, Field::LineSpacing),
            (&this.before, Field::Before),
            (&this.after, Field::After),
            (&this.first_line, Field::FirstLine),
            (&this.left, Field::Left),
            (&this.right, Field::Right),
        ] {
            let weak = Rc::downgrade(&this);
            let editor = editor.clone();
            row.connect_value_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    let attrs = field.attrs(row.value(), this.units.get());
                    editor.set_paragraph_attrs(field.name(), &attrs);
                }
            });
        }
        this
    }

    /// The widget to put at the side of the window.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Measures the indents in other units.
    fn set_units(&self, units: Units) {
        self.updating.set(true);
        self.units.set(units);
        tab_stops::set_units(&self.first_line, units, -1584.0, 1584.0);
        tab_stops::set_units(&self.left, units, 0.0, 1584.0);
        tab_stops::set_units(&self.right, units, 0.0, 1584.0);
        self.updating.set(false);
        self.tabs.set_units(units);
        self.lists.set_units(units);
        self.layouts.set_units(units);
        self.headers.set_units(units);
        self.notes.set_units(units);
        self.objects.set_units(units);
        let shown = self.shown.borrow().clone();
        if let Some(format) = shown {
            self.show(&format);
        }
    }

    /// Shows the formatting at the selection.
    pub fn show(&self, format: &Format) {
        *self.shown.borrow_mut() = Some(format.clone());
        self.objects.show_format(format);
        self.updating.set(true);
        let paragraph = &format.paragraph;
        let set = |row: &compact::SpinRow, value: f64| {
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        };
        match paragraph.line_spacing {
            LineSpacing::Multiple { percent } => {
                self.line_spacing.set_sensitive(true);
                set(&self.line_spacing, f64::from(percent) / 100.0);
            }
            // Spacing in points is set in phase 4, with the rest of the
            // Text inspector.
            _ => self.line_spacing.set_sensitive(false),
        }
        set(&self.before, paragraph.space_before.to_pt());
        set(&self.after, paragraph.space_after.to_pt());
        let units = self.units.get();
        set(
            &self.first_line,
            tab_stops::in_units(paragraph.first_line_indent, units),
        );
        set(
            &self.left,
            tab_stops::in_units(paragraph.left_indent, units),
        );
        set(
            &self.right,
            tab_stops::in_units(paragraph.right_indent, units),
        );
        if self.hyphenate.is_active() != paragraph.hyphenate {
            self.hyphenate.set_active(paragraph.hyphenate);
        }
        let settings = &format.text_settings;
        if self.document_hyphenation.is_active() != settings.hyphenation {
            self.document_hyphenation.set_active(settings.hyphenation);
        }
        if self.document_ligatures.is_active() != settings.ligatures {
            self.document_ligatures.set_active(settings.ligatures);
        }
        if self.facing_pages.is_active() != format.setup.facing_pages {
            self.facing_pages.set_active(format.setup.facing_pages);
        }
        self.updating.set(false);
        self.tabs.show(paragraph);
        self.lists.show(format);
        self.layouts.show(format);
        self.headers.show(format);
        self.notes.show(format);
        self.toc.show(format);
        self.links.show(format);
        self.effects.show(&format.character, paragraph);
    }
}

/// Names a button that shows only an icon, for screen readers.
pub fn labelled(widget: &impl IsA<gtk::Accessible>, label: &str) {
    widget.update_property(&[gtk::accessible::Property::Label(label)]);
}

/// The value of a boolean action's state.
pub fn state_of(action: &gtk::gio::SimpleAction) -> bool {
    action
        .state()
        .and_then(|s| s.get::<bool>())
        .unwrap_or(false)
}

/// Sets the state of a stateful action without running it.
pub fn set_state(window: &impl IsA<gtk::gio::ActionMap>, name: &str, value: glib::Variant) {
    if let Some(action) = window
        .lookup_action(name)
        .and_then(|a| a.downcast::<gtk::gio::SimpleAction>().ok())
        && action.state().as_ref() != Some(&value)
    {
        action.set_state(&value);
    }
}

/// The panes of the Inspector, one at a time, chosen by a row of toggles at
/// its top as in Pages. A pane whose groups all hide says that nothing it
/// sets is selected.
/// The toggle of a pane of the Inspector.
fn pane_toggle(name: &str, title: &str, icon: &str) -> adw::Toggle {
    adw::Toggle::builder()
        .name(name)
        .icon_name(icon)
        .tooltip(title)
        .build()
}

struct InspectorPanes {
    root: gtk::Box,
    toggles: adw::ToggleGroup,
    stack: gtk::Stack,
    /// The names, titles and icons of the panes, in order.
    panes: Vec<(String, String, String)>,
    /// The panes left out while nothing they set is selected.
    hidden: RefCell<std::collections::HashSet<String>>,
}

impl InspectorPanes {
    /// The panes: their names, titles, icons and groups.
    #[allow(clippy::type_complexity)]
    fn new(panes: &[(&str, &str, &str, &[&compact::Group])]) -> Rc<Self> {
        let toggles = adw::ToggleGroup::builder()
            .halign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let stack = gtk::Stack::builder().vhomogeneous(false).build();
        for (name, title, icon, groups) in panes {
            let title = tr(title);
            toggles.add(pane_toggle(name, &title, icon));
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(18)
                .margin_top(12)
                .margin_bottom(12)
                .margin_start(12)
                .margin_end(12)
                .build();
            let heading = gtk::Label::builder()
                .label(&title)
                .xalign(0.0)
                .css_classes(["title-4"])
                .build();
            content.append(&heading);
            let empty = gtk::Label::builder()
                .label(tr("Nothing that this pane sets is selected."))
                .wrap(true)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build();
            content.append(&empty);
            for group in *groups {
                content.append(group.widget());
            }
            let all: Vec<compact::Group> = groups.iter().map(|g| (*g).clone()).collect();
            let show_empty = {
                let empty = empty.downgrade();
                let all = all.clone();
                move || {
                    if let Some(empty) = empty.upgrade() {
                        empty.set_visible(!all.iter().any(compact::Group::is_visible));
                    }
                }
            };
            show_empty();
            let show_empty = Rc::new(show_empty);
            for group in &all {
                let show_empty = show_empty.clone();
                group.connect_visible_notify(move |_| show_empty());
            }
            let scrolled = gtk::ScrolledWindow::builder()
                .child(&content)
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vexpand(true)
                .build();
            stack.add_titled(&scrolled, Some(name), &title);
        }
        toggles.set_active_name(Some("text"));
        stack.set_visible_child_name("text");
        let shown = stack.downgrade();
        toggles.connect_active_name_notify(move |toggles| {
            if let (Some(stack), Some(name)) = (shown.upgrade(), toggles.active_name()) {
                stack.set_visible_child_name(&name);
            }
        });
        let bar = gtk::Box::builder()
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        toggles.set_hexpand(true);
        bar.append(&toggles);
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(300)
            .build();
        root.append(&bar);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&stack);
        Rc::new(Self {
            root,
            toggles,
            stack,
            panes: panes
                .iter()
                .map(|(name, title, icon, _)| ((*name).to_owned(), tr(title), (*icon).to_owned()))
                .collect(),
            hidden: RefCell::new(std::collections::HashSet::new()),
        })
    }

    fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Shows the toggle of a pane only while `group` shows, as Pages
    /// shows the QuickTime inspector only for a movie or a sound.
    fn show_only_with(self: &Rc<Self>, name: &str, group: &compact::Group) {
        let weak = Rc::downgrade(self);
        let name = name.to_owned();
        let update = move |visible: bool| {
            let Some(this) = weak.upgrade() else { return };
            let changed = if visible {
                this.hidden.borrow_mut().remove(&name)
            } else {
                this.hidden.borrow_mut().insert(name.clone())
            };
            if changed {
                this.refresh_toggles();
            }
        };
        update(group.is_visible());
        group.connect_visible_notify(move |group| update(group.is_visible()));
    }

    /// Puts back the toggles of the panes that are not left out, in their
    /// order, keeping the pane shown unless it is left out.
    fn refresh_toggles(&self) {
        let active = self.toggles.active_name();
        let hidden = self.hidden.borrow();
        self.toggles.remove_all();
        for (name, title, icon) in &self.panes {
            if !hidden.contains(name) {
                self.toggles.add(pane_toggle(name, title, icon));
            }
        }
        let active = active
            .filter(|name| !hidden.contains(name.as_str()))
            .map_or_else(|| "metrics".to_owned(), |name| name.to_string());
        self.toggles.set_active_name(Some(&active));
        self.stack.set_visible_child_name(&active);
    }
}

/// Shows the strings of a list as labels, cut short to `chars`
/// characters when it is given.
fn label_factory(chars: Option<i32>) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::builder().xalign(0.0).build();
        if let Some(chars) = chars {
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(chars);
        }
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk::StringObject>()
            .map(|s| s.string().to_string())
            .unwrap_or_default();
        if let Some(label) = item.child().and_downcast::<gtk::Label>() {
            label.set_label(&text);
        }
    });
    factory
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_go_to_gdk_and_back() {
        for color in [
            Rgba::BLACK,
            Rgba::rgb(200, 30, 7),
            Rgba {
                a: 128,
                ..Rgba::rgb(1, 2, 3)
            },
        ] {
            assert_eq!(from_gdk(&to_gdk(color)), color);
        }
    }

    #[test]
    fn alignments_name_their_actions() {
        for alignment in [
            Alignment::Left,
            Alignment::Center,
            Alignment::Right,
            Alignment::Justify,
        ] {
            assert_eq!(alignment_of(alignment_name(alignment)), Some(alignment));
        }
        assert_eq!(alignment_name(Alignment::Auto), "left");
        assert_eq!(alignment_of("middle"), None);
    }

    #[test]
    fn inspector_rows_set_one_attribute() {
        let attrs = Field::LineSpacing.attrs(1.5, Units::Inches);
        assert_eq!(
            attrs.line_spacing,
            Some(LineSpacing::Multiple { percent: 150 })
        );
        // Indents are in the units of the rulers, and spacing in points.
        assert_eq!(
            Field::FirstLine
                .attrs(0.25, Units::Inches)
                .first_line_indent,
            Some(Length::from_pt(18.0))
        );
        assert_eq!(
            Field::Before.attrs(6.0, Units::Inches).space_before,
            Some(Length::from_pt(6.0))
        );
        assert!(
            Field::Before
                .attrs(6.0, Units::Points)
                .left_indent
                .is_none()
        );
    }
}
