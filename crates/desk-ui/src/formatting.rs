// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Inspector at the side of a window, as in Pages: it shows the
//! formatting at the selection and changes it. Its Format pane holds what
//! the format bar of interface B had (step 2 of window M+).
//!
//! The toggles of the Format pane (bold, italic, underline, strikethrough
//! and alignment) are actions of the window, which the keyboard shortcuts
//! also run; the rest calls the editor through `editor`.

use crate::compact;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Alignment, Length, LineSpacing, ParaAttrs, Rgba};
use gtk::{gdk, glib};

use crate::editing::{Format, TextEditor};
use crate::effects::Effects;
use crate::i18n::{n_, tr};
use crate::inspector::{Context, Levels, Tab};
use crate::layout_pane::LayoutGroups;
use crate::lists_pane::ListsGroup;
use crate::preferences::{self, Units};
use crate::tab_stops::{self, TabStopsGroup};

/// Finds the editor of the window, if it shows a document.
pub type EditorOf = Rc<dyn Fn() -> Option<TextEditor>>;

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

/// The Inspector: Format, with the tabs of what is selected, and Document
/// (INT-13).
pub struct Inspector {
    root: gtk::Box,
    /// Format and Document, and their tabs.
    levels: Rc<Levels>,
    /// The counts of the Info tab of Document.
    statistics: InfoRows,
    /// What the format bar of interface B held for text: the paragraph
    /// style over the tabs, the character style and the link (step 2 of
    /// window M+).
    text: Rc<crate::format_pane::TextFormat>,
    /// The font and alignment of selected text, in its Style tab.
    text_font: Rc<crate::object_text_pane::ObjectTextGroups>,
    editor: EditorOf,
    line_spacing: compact::SpinRow,
    before: compact::SpinRow,
    after: compact::SpinRow,
    first_line: compact::SpinRow,
    left: compact::SpinRow,
    right: compact::SpinRow,
    /// Whether words break with hyphens (TXT-09).
    hyphenate: compact::SwitchRow,
    /// The drop cap of the paragraphs (PXV-06).
    drop_cap: Rc<crate::drop_cap_pane::DropCapGroup>,
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
    /// The font and alignment of the text of a selected shape or text
    /// box (OBJ-13).
    object_text: Rc<crate::object_text_pane::ObjectTextGroups>,
    /// The object styles of the selected object (MOD-04).
    object_styles: Rc<crate::object_styles_pane::StylesGroup>,
    /// The Table, Cell and Text tabs of a selected table (TAB-02).
    table: Rc<crate::table_pane::TableGroups>,
    /// The Chart, Axis and Series tabs of a selected chart (GRF-03).
    chart: Rc<crate::chart_pane::ChartGroups>,
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
        let object_text = crate::object_text_pane::ObjectTextGroups::new(editor.clone());
        let text = crate::format_pane::TextFormat::new(editor.clone());
        let text_font = crate::object_text_pane::ObjectTextGroups::new(editor.clone());
        // Lists has a group of its own in the Style tab of text.
        text_font.hide_list();
        let [text_family, text_alignment] = text_font.groups();
        text_family.add(text.character_row());
        // Character spacing, the baseline, capitalization, ligatures and
        // the background of characters, in the Advanced Options of the
        // Font group (B-019).
        for row in effects.advanced_rows() {
            text_font.add_advanced(row);
        }
        let arrange_group = crate::format_pane::arrange_group();
        let object_styles = crate::object_styles_pane::StylesGroup::new();
        let table = crate::table_pane::TableGroups::new(editor.clone());
        let chart = crate::chart_pane::ChartGroups::new(editor.clone());

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
        let drop_cap = crate::drop_cap_pane::DropCapGroup::new(editor.clone());
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
        // The tabs of Format for each kind of selection, and those of
        // Document (INT-13, the Inspector of interface B).
        let [metrics, shape, picture, media, graphic, wrap, text_box] = objects.groups();
        let [font, alignment] = object_text.groups();
        let [character, text_shadow, borders] = effects.groups();
        let [page, layout, section] = layouts.groups();
        let [link, bookmarks] = links.groups();
        let arrange = || Tab::new("arrange", n_("Arrange"), &[&arrange_group, metrics, wrap]);
        let styles = object_styles.group();
        let style = || Tab::new("style", n_("Style"), &[styles, graphic, shape]);
        let mut contexts = HashMap::new();
        contexts.insert(
            Context::Text,
            vec![
                Tab::new(
                    "style",
                    n_("Style"),
                    &[
                        text_family,
                        text_alignment,
                        &spacing,
                        lists.widget(),
                        character,
                        text_shadow,
                        drop_cap.group(),
                    ],
                ),
                Tab::new(
                    "layout",
                    n_("Layout"),
                    &[&indents, tabs.widget(), layout, borders],
                ),
                Tab::new("more", n_("More"), &[text.link_group(), &hyphenation]),
            ],
        );
        contexts.insert(
            Context::Image,
            vec![
                Tab::new("style", n_("Style"), &[styles, graphic]),
                Tab::new("image", n_("Image"), &[picture]),
                arrange(),
            ],
        );
        for context in [Context::Shape, Context::TextBox] {
            contexts.insert(
                context,
                vec![
                    style(),
                    Tab::new("text", n_("Text"), &[font, alignment, &spacing, text_box]),
                    arrange(),
                ],
            );
        }
        for context in [Context::Movie, Context::Sound] {
            contexts.insert(
                context,
                vec![
                    Tab::new("style", n_("Style"), &[graphic]),
                    Tab::new("media", n_("Media"), &[media]),
                    arrange(),
                ],
            );
        }
        contexts.insert(Context::Group, vec![style(), arrange()]);
        let [
            table_styles,
            table_group,
            rows_cols,
            cell,
            highlight,
            cell_text,
        ] = table.groups();
        contexts.insert(
            Context::Table,
            vec![
                Tab::new(
                    "table",
                    n_("Table"),
                    &[table_styles, table_group, rows_cols],
                ),
                Tab::new("cell", n_("Cell"), &[cell, highlight]),
                Tab::new("text", n_("Text"), &[cell_text]),
                arrange(),
            ],
        );
        let [
            chart_styles,
            chart_group,
            scene,
            value_axis,
            category_axis,
            series,
        ] = chart.groups();
        contexts.insert(
            Context::Chart,
            vec![
                Tab::new("chart", n_("Chart"), &[chart_styles, chart_group, scene]),
                Tab::new("axis", n_("Axis"), &[value_axis, category_axis]),
                Tab::new("series", n_("Series"), &[series]),
                arrange(),
            ],
        );
        let statistics = InfoRows::new();
        let document_tabs = vec![
            Tab::new(
                "document",
                n_("Document"),
                &[
                    page,
                    &document,
                    headers.widget(),
                    notes.widget(),
                    toc.widget(),
                ],
            ),
            Tab::new("section", n_("Section"), &[section]),
            Tab::new("info", n_("Info"), &[&statistics.group]),
        ];
        let levels = Levels::new(contexts, document_tabs);
        levels.set_header(Context::Text, text.header().widget());
        let link_popover = link_popover(&[link, bookmarks]);
        text.set_link_popover(&link_popover);
        let root = levels.widget().clone();
        {
            // The Colors popovers of the Inspector show the colors that
            // its document already uses (INT-16, B-019).
            let editor = editor.clone();
            crate::color_popover::set_document_colors(&root, move || {
                editor().map(|e| e.document().colors()).unwrap_or_default()
            });
        }

        let this = Rc::new(Self {
            root,
            levels,
            statistics,
            text,
            text_font,
            editor: editor.clone(),
            line_spacing,
            before,
            after,
            first_line,
            left,
            right,
            hyphenate,
            drop_cap,
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
            object_text,
            object_styles,
            table,
            chart,
            units: Cell::new(units),
            shown: RefCell::new(None),
            updating: Cell::new(false),
        });
        this.set_units(units);
        let weak = Rc::downgrade(&this);
        this.levels.connect_document_shown(move || {
            if let Some(this) = weak.upgrade() {
                this.show_statistics();
            }
        });
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

    /// The paragraph and character styles and the link of text.
    pub fn text(&self) -> &Rc<crate::format_pane::TextFormat> {
        &self.text
    }

    /// The button of the font family of selected text, in the Style tab,
    /// which the Fonts popover opens on (D-024).
    pub fn text_font_button(&self) -> &gtk::Button {
        self.text_font.family_button()
    }

    /// The button of the Colors popover of the text color: that of a
    /// selected shape's text (in its Text tab), else that of selected
    /// text (in the Style tab).
    pub fn text_color_button(&self, object: bool) -> &gtk::MenuButton {
        if object {
            self.object_text.colors_button()
        } else {
            self.text_font.colors_button()
        }
    }

    /// Adds the tabs of a kind of selection that only an app has, such as
    /// the slide of Desk Slides (KINT-13).
    pub fn add_context(&self, context: Context, tabs: Vec<Tab>) {
        self.levels.add_context(context, tabs);
    }

    /// Shows the tabs of a kind of selection in place of those the
    /// selection chose, such as the slide when nothing is selected in
    /// Desk Slides.
    pub fn show_context(&self, context: Context) {
        self.levels.show(context);
    }

    /// The button of the font family in the Text tab of a selected shape,
    /// which the Fonts popover opens on.
    pub fn object_font_button(&self) -> &gtk::Button {
        self.object_text.family_button()
    }

    /// Shows a tab, by its name: "style", "image", "arrange", "document"…
    pub fn show_tab(&self, name: &str) -> bool {
        self.levels.show_tab(name)
    }

    /// Shows the Table of Contents group of the Document tab, for Edit in
    /// the table of contents of the sidebar (W011): the tab shows, and
    /// the group takes the focus and its title goes to the top.
    pub fn show_toc_settings(&self) {
        self.levels.show_tab("document");
        let group = self.toc.widget().widget().clone();
        gtk::glib::idle_add_local_once(move || {
            group.child_focus(gtk::DirectionType::TabForward);
            // The focus only brings the group into view; its title goes
            // to the top of the tab.
            let Some(scrolled) = group
                .ancestor(gtk::ScrolledWindow::static_type())
                .and_downcast::<gtk::ScrolledWindow>()
            else {
                return;
            };
            if let Some(content) = scrolled.child()
                && let Some(point) =
                    group.compute_point(&content, &gtk::graphene::Point::new(0.0, 0.0))
            {
                scrolled.vadjustment().set_value(f64::from(point.y()));
            }
        });
    }

    /// Adds a level of an app before Document, such as Animation in
    /// Desk Slides (KINT-14).
    pub fn add_level(&self, name: &str, label: &str, child: &impl IsA<gtk::Widget>) {
        self.levels.add_level(name, label, child);
    }

    /// Shows a level: "format" or "document".
    pub fn show_level(&self, name: &str) {
        self.levels.show_level(name);
    }

    /// Counts the document again while its Info tab shows.
    fn show_statistics(&self) {
        if self.levels.document_tab().as_deref() != Some("info") {
            return;
        }
        if let Some(editor) = (self.editor)() {
            let (all, _) = editor.statistics();
            self.statistics.show(&all);
        }
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
        self.object_text.show(format, format.object_text);
        self.text_font.show_text(format);
        self.object_styles.show(format.object.as_ref());
        let context = Context::of(format.object.as_ref());
        if let Some(editor) = (self.editor)() {
            match context {
                Context::Table => self.table.show(&editor),
                Context::Chart => self.chart.show(&editor),
                _ => {}
            }
        }
        self.levels.show(context);
        self.show_statistics();
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
        self.drop_cap.show(paragraph);
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

/// The counts of the document in the Info tab of Document.
struct InfoRows {
    group: compact::Group,
    values: Vec<gtk::Label>,
}

impl InfoRows {
    fn new() -> Self {
        let group = compact::Group::builder().title(tr("Statistics")).build();
        let all = gtk::Button::builder()
            .label(tr("Show All"))
            .action_name("win.statistics")
            .css_classes(["flat"])
            .build();
        group.set_header_suffix(Some(&all));
        let values = [
            n_("Words"),
            n_("Characters"),
            n_("Paragraphs"),
            n_("Sections"),
            n_("Graphics"),
        ]
        .into_iter()
        .map(|title| {
            let row = compact::ActionRow::builder().title(tr(title)).build();
            let value = gtk::Label::builder()
                .css_classes(["numeric"])
                .selectable(true)
                .build();
            row.add_suffix(&value);
            group.add(&row);
            value
        })
        .collect();
        Self { group, values }
    }

    fn show(&self, statistics: &desk_model::Statistics) {
        let counts = [
            statistics.words,
            statistics.characters,
            statistics.paragraphs,
            statistics.sections,
            statistics.graphics,
        ];
        for (label, count) in self.values.iter().zip(counts) {
            let text = count.to_string();
            if label.label() != text {
                label.set_label(&text);
            }
        }
    }
}

/// A popover that holds the groups of the hyperlink and the bookmarks,
/// as the Link inspector of Pages became in interface B.
fn link_popover(groups: &[&compact::Group]) -> gtk::Popover {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .width_request(308)
        .build();
    for group in groups {
        content.append(group.widget());
    }
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(520)
        .build();
    gtk::Popover::builder().child(&scrolled).build()
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

/// Shows the strings of a list as labels, cut short to `chars`
/// characters when it is given.
pub(crate) fn label_factory(chars: Option<i32>) -> gtk::SignalListItemFactory {
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
