// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Styles drawer at the side of a window, as in Pages (INT-18): the
//! paragraph and character styles of the document, each shown in its
//! own font, with the style at the selection marked and an arrow where
//! the formatting there differs from it (STY-06). A click applies a
//! style, a double click applies it and drops the formatting set over
//! it, and the menu of a style redefines, reverts, selects its uses,
//! renames, deletes it and gives it a function key (STY-02 to STY-09).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{
    CharAttrs, CharFormat, ListStyle, ModelError, Style, StyleFamily, StyleId, Styles,
};
use gtk::{gdk, gio, glib, pango};

use crate::editing::TextEditor;
use crate::formatting::{EditorOf, labelled};
use crate::i18n::tr;

/// The largest size, in points, that the name of a style is shown at.
const MAX_PREVIEW_SIZE: f64 = 18.0;

/// The smallest size, in points, that the name of a style is shown at.
const MIN_PREVIEW_SIZE: f64 = 9.0;

/// What a row shows: the style, its name, its shortcut and its look.
type Shown = (StyleId, String, Option<u8>, CharFormat);

/// A row of a list of styles.
struct Row {
    id: StyleId,
    row: gtk::ListBoxRow,
    /// The arrow that marks formatting set over the style.
    arrow: gtk::Image,
}

/// The Styles drawer.
pub struct StylesDrawer {
    root: gtk::Box,
    paragraphs: gtk::ListBox,
    characters: gtk::ListBox,
    lists: gtk::ListBox,
    scrolled: gtk::ScrolledWindow,
    rows: RefCell<Vec<Row>>,
    /// What the lists show now, to build them again only on a change.
    shown: RefCell<Vec<Shown>>,
    /// The style whose menu is open.
    context: Cell<Option<StyleId>>,
    actions: gio::SimpleActionGroup,
    updating: Cell<bool>,
    editor: EditorOf,
}

/// The name of a function key for a style shortcut.
pub fn key_name(key: u8) -> String {
    format!("F{key}")
}

/// The look of the name of a style: its font, size within bounds, weight,
/// slant, lines, and its color unless it is a gray, which would not show
/// on a dark background.
fn preview(format: &CharFormat) -> pango::AttrList {
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrString::new_family(&format.font_family));
    let size = format
        .font_size
        .to_pt()
        .clamp(MIN_PREVIEW_SIZE, MAX_PREVIEW_SIZE);
    attrs.insert(pango::AttrSize::new(
        (size * f64::from(pango::SCALE)) as i32,
    ));
    if format.weight.is_bold() {
        attrs.insert(pango::AttrInt::new_weight(pango::Weight::Bold));
    }
    if format.italic {
        attrs.insert(pango::AttrInt::new_style(pango::Style::Italic));
    }
    if format.underline != desk_model::LineStyle::None {
        attrs.insert(pango::AttrInt::new_underline(pango::Underline::Single));
    }
    if format.strikethrough != desk_model::LineStyle::None {
        attrs.insert(pango::AttrInt::new_strikethrough(true));
    }
    let color = format.color;
    if !(color.r == color.g && color.g == color.b) {
        let wide = |c: u8| u16::from(c) * 257;
        attrs.insert(pango::AttrColor::new_foreground(
            wide(color.r),
            wide(color.g),
            wide(color.b),
        ));
    }
    attrs
}

/// How a style looks on its own.
fn look(styles: &Styles, style: &Style) -> CharFormat {
    match style {
        Style::Paragraph(p) => styles.resolve_character(p.id, None, &CharAttrs::default()),
        Style::Character(c) => styles.resolve_character(
            styles.default_paragraph(),
            Some(c.id),
            &CharAttrs::default(),
        ),
        Style::List(_) => {
            styles.resolve_character(styles.default_paragraph(), None, &CharAttrs::default())
        }
    }
}

/// The styles of a family by name.
pub fn sorted(styles: &Styles, family: StyleFamily) -> Vec<Style> {
    let mut list: Vec<Style> = styles
        .iter()
        .filter(|s| s.family() == family)
        .cloned()
        .collect();
    list.sort_by_key(|s| s.name().to_lowercase());
    list
}

impl StylesDrawer {
    /// A drawer that works on the editor that `editor` gives.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(220)
            .css_classes(["background"])
            .build();
        labelled(&root, &tr("Styles"));
        let header = gtk::Label::builder()
            .label(tr("Paragraph Styles"))
            .xalign(0.0)
            .margin_start(12)
            .margin_top(12)
            .margin_bottom(6)
            .css_classes(["heading"])
            .build();
        let paragraphs = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        labelled(&paragraphs, &tr("Paragraph Styles"));
        let character_header = gtk::Label::builder()
            .label(tr("Character Styles"))
            .xalign(0.0)
            .margin_start(12)
            .margin_top(12)
            .margin_bottom(6)
            .css_classes(["heading"])
            .build();
        let characters = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        labelled(&characters, &tr("Character Styles"));
        let character_section = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        character_section.append(&character_header);
        character_section.append(&holder(&characters));
        let list_header = gtk::Label::builder()
            .label(tr("List Styles"))
            .xalign(0.0)
            .margin_start(12)
            .margin_top(12)
            .margin_bottom(6)
            .css_classes(["heading"])
            .build();
        let lists = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        labelled(&lists, &tr("List Styles"));
        let list_section = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        list_section.append(&list_header);
        list_section.append(&holder(&lists));
        let sections = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        sections.append(&header);
        sections.append(&holder(&paragraphs));
        sections.append(&character_section);
        sections.append(&list_section);
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&sections)
            .build();
        root.append(&scrolled);

        // The bar at the bottom: new styles, and whether character styles
        // show.
        let new_menu = gio::Menu::new();
        new_menu.append(
            Some(&tr("New _Paragraph Style from Selection…")),
            Some("styles.new-paragraph"),
        );
        new_menu.append(
            Some(&tr("New _Character Style from Selection…")),
            Some("styles.new-character"),
        );
        new_menu.append(
            Some(&tr("New _List Style from Selection…")),
            Some("styles.new-list"),
        );
        let import = gio::Menu::new();
        import.append(Some(&tr("_Import Styles…")), Some("win.import-styles"));
        new_menu.append_section(None, &import);
        let add = gtk::MenuButton::builder()
            .icon_name("list-add-symbolic")
            .menu_model(&new_menu)
            .direction(gtk::ArrowType::Up)
            .tooltip_text(tr("New Style"))
            .css_classes(["flat"])
            .build();
        labelled(&add, &tr("New Style"));
        let show_characters = gtk::ToggleButton::builder()
            .icon_name("format-text-bold-symbolic")
            .active(true)
            .tooltip_text(tr("Show Character Styles"))
            .css_classes(["flat"])
            .build();
        labelled(&show_characters, &tr("Show Character Styles"));
        let show_lists = gtk::ToggleButton::builder()
            .icon_name("view-list-bullet-symbolic")
            .active(true)
            .tooltip_text(tr("Show List Styles"))
            .css_classes(["flat"])
            .build();
        labelled(&show_lists, &tr("Show List Styles"));
        let bar = gtk::ActionBar::new();
        bar.pack_start(&add);
        bar.pack_end(&show_lists);
        bar.pack_end(&show_characters);
        root.append(&bar);
        show_characters
            .bind_property("active", &character_section, "visible")
            .sync_create()
            .build();
        show_lists
            .bind_property("active", &list_section, "visible")
            .sync_create()
            .build();

        let actions = gio::SimpleActionGroup::new();
        root.insert_action_group("styles", Some(&actions));
        let this = Rc::new(Self {
            root,
            paragraphs,
            characters,
            lists,
            scrolled,
            rows: RefCell::new(Vec::new()),
            shown: RefCell::new(Vec::new()),
            context: Cell::new(None),
            actions,
            updating: Cell::new(false),
            editor,
        });
        this.connect_list(&this.paragraphs);
        this.connect_list(&this.characters);
        this.connect_list(&this.lists);
        this.add_actions();
        this
    }

    /// The widget to put at the side of the window.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn style_at(&self, row: &gtk::ListBoxRow) -> Option<StyleId> {
        self.rows
            .borrow()
            .iter()
            .find(|r| &r.row == row)
            .map(|r| r.id)
    }

    /// Applies a style on a click, applies it without the formatting set
    /// over it on a double click, and opens its menu on a secondary click
    /// or the Menu key.
    fn connect_list(self: &Rc<Self>, list: &gtk::ListBox) {
        let weak = Rc::downgrade(self);
        list.connect_row_activated(move |_, row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let character_none = row.widget_name() == "none";
            let list_none = row.widget_name() == "none-list";
            if let Some(editor) = (this.editor)() {
                if character_none {
                    editor.set_character_style(None);
                } else if list_none {
                    editor.set_list_style(None);
                } else if let Some(id) = this.style_at(row) {
                    editor.apply_style(id, false);
                }
                editor.focus();
            }
        });

        let double = gtk::GestureClick::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        let weak = Rc::downgrade(self);
        let list_weak = list.downgrade();
        double.connect_released(move |_, presses, _, y| {
            let (Some(this), Some(list)) = (weak.upgrade(), list_weak.upgrade()) else {
                return;
            };
            if presses != 2 {
                return;
            }
            let id = list.row_at_y(y as i32).and_then(|row| this.style_at(&row));
            if let (Some(id), Some(editor)) = (id, (this.editor)()) {
                editor.apply_style(id, true);
                editor.focus();
            }
        });
        list.add_controller(double);

        let menu = gtk::PopoverMenu::from_model(Some(&style_menu()));
        // On the box that holds the list, which takes only rows.
        menu.set_parent(&list.parent().expect("the list is in its holder"));
        menu.set_has_arrow(false);
        menu.set_halign(gtk::Align::Start);
        let secondary = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        let weak = Rc::downgrade(self);
        let list_weak = list.downgrade();
        let popover = menu.downgrade();
        secondary.connect_pressed(move |gesture, _, x, y| {
            let (Some(this), Some(list), Some(popover)) =
                (weak.upgrade(), list_weak.upgrade(), popover.upgrade())
            else {
                return;
            };
            let Some(id) = list.row_at_y(y as i32).and_then(|row| this.style_at(&row)) else {
                return;
            };
            gesture.set_state(gtk::EventSequenceState::Claimed);
            this.open_menu(id, &popover, x as i32, y as i32);
        });
        list.add_controller(secondary);
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        let list_weak = list.downgrade();
        let popover = menu.downgrade();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let menu_key = key == gdk::Key::Menu
                || (key == gdk::Key::F10 && modifiers == gdk::ModifierType::SHIFT_MASK);
            let (true, Some(this), Some(list), Some(popover)) = (
                menu_key,
                weak.upgrade(),
                list_weak.upgrade(),
                popover.upgrade(),
            ) else {
                return glib::Propagation::Proceed;
            };
            let row = list
                .focus_child()
                .and_then(|c| c.downcast::<gtk::ListBoxRow>().ok())
                .or_else(|| list.selected_row());
            let Some((id, row)) = row.and_then(|row| Some((this.style_at(&row)?, row))) else {
                return glib::Propagation::Proceed;
            };
            let y = row.compute_bounds(&list).map_or(0, |b| b.y() as i32);
            this.open_menu(id, &popover, 24, y + row.height() / 2);
            glib::Propagation::Stop
        });
        list.add_controller(keys);
    }

    fn open_menu(&self, id: StyleId, popover: &gtk::PopoverMenu, x: i32, y: i32) {
        self.context.set(Some(id));
        let key = (self.editor)()
            .and_then(|e| e.styles().get(id).and_then(Style::shortcut))
            .unwrap_or(0);
        if let Some(action) = self
            .actions
            .lookup_action("shortcut")
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        {
            action.set_state(&key.to_string().to_variant());
        }
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x, y, 1, 1)));
        popover.popup();
    }

    fn add_actions(self: &Rc<Self>) {
        type StyleAction = fn(&Rc<StylesDrawer>, &TextEditor, StyleId);
        let on_style: [(&str, StyleAction); 5] = [
            ("redefine", |this, editor, id| {
                let result = editor.style_edit(&tr("Redefine Style"), |edit, from, to| {
                    edit.redefine_style(id, from, to)
                });
                this.report(&tr("Cannot Redefine the Style"), result);
            }),
            ("revert", |this, editor, _| {
                let result = editor.style_edit(&tr("Revert to Defined Style"), |edit, from, to| {
                    edit.revert_to_style(from, to)
                });
                this.report(&tr("Cannot Revert to the Style"), result);
            }),
            ("select-uses", |_, editor, id| {
                if editor.select_uses(id) {
                    editor.focus();
                }
            }),
            ("rename", |this, editor, id| this.rename(editor, id)),
            ("delete", |this, editor, id| this.delete(editor, id)),
        ];
        for (name, run) in on_style {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                let Some(this) = weak.upgrade() else { return };
                let (Some(editor), Some(id)) = ((this.editor)(), this.context.get()) else {
                    return;
                };
                run(&this, &editor, id);
            });
            self.actions.add_action(&action);
        }

        let shortcut = gio::SimpleAction::new_stateful(
            "shortcut",
            Some(glib::VariantTy::STRING),
            &"0".to_variant(),
        );
        let weak = Rc::downgrade(self);
        shortcut.connect_change_state(move |action, value| {
            let (Some(this), Some(value)) = (weak.upgrade(), value) else {
                return;
            };
            let key = value
                .get::<String>()
                .and_then(|v| v.parse::<u8>().ok())
                .filter(|k| (1..=8).contains(k));
            action.set_state(value);
            let (Some(editor), Some(id)) = ((this.editor)(), this.context.get()) else {
                return;
            };
            let result = editor.style_edit(&tr("Style Shortcut"), |edit, _, _| {
                edit.set_style_shortcut(id, key)
            });
            this.report(&tr("Cannot Set the Shortcut"), result);
        });
        self.actions.add_action(&shortcut);

        for (name, family) in [
            ("new-paragraph", StyleFamily::Paragraph),
            ("new-character", StyleFamily::Character),
            ("new-list", StyleFamily::List),
        ] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                let Some(this) = weak.upgrade() else { return };
                if let Some(editor) = (this.editor)() {
                    this.new_style(&editor, family);
                }
            });
            self.actions.add_action(&action);
        }
    }

    /// Shows a dialog over the window; the document has the keyboard
    /// again when it closes.
    fn present(&self, dialog: &adw::AlertDialog) {
        let editor = self.editor.clone();
        dialog.connect_closed(move |_| {
            if let Some(editor) = editor() {
                editor.focus();
            }
        });
        dialog.present(Some(&self.root));
    }

    /// Says why a change of the styles was refused.
    fn report<T>(&self, heading: &str, result: Result<T, ModelError>) {
        if let Err(error) = result {
            let body = match error {
                ModelError::DuplicateStyleName(name) if name.is_empty() => {
                    tr("A style needs a name.")
                }
                ModelError::DuplicateStyleName(name) => {
                    tr("Another style is already named “{name}”.").replace("{name}", &name)
                }
                other => other.to_string(),
            };
            let dialog = adw::AlertDialog::new(Some(heading), Some(&body));
            dialog.add_response("close", &tr("_Close"));
            self.present(&dialog);
        }
    }

    /// Asks for the name of a new style made from the selection, and
    /// whether to apply it, and for a character style which formatting
    /// it takes (STY-03).
    pub fn new_style(self: &Rc<Self>, editor: &TextEditor, family: StyleFamily) {
        let styles = editor.styles();
        let base = match family {
            StyleFamily::Paragraph => editor
                .format()
                .style
                .and_then(|id| styles.get(id).map(|s| s.name().to_owned()))
                .unwrap_or_else(|| tr("Paragraph Style")),
            StyleFamily::Character => tr("Character Style"),
            StyleFamily::List => editor
                .list_style()
                .and_then(|id| styles.get(id).map(|s| s.name().to_owned()))
                .unwrap_or_else(|| tr("List Style")),
        };
        let heading = match family {
            StyleFamily::Paragraph => tr("New Paragraph Style"),
            StyleFamily::Character => tr("New Character Style"),
            StyleFamily::List => tr("New List Style"),
        };
        let dialog = adw::AlertDialog::new(Some(&heading), None);
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        let name = adw::EntryRow::builder()
            .title(tr("Name"))
            .activates_default(true)
            .build();
        name.set_text(&styles.unique_name(family, &base));
        list.append(&name);
        let focus = name.clone();
        let only_changes = adw::SwitchRow::builder()
            .title(tr("Only Formatting That Differs from the Paragraph"))
            .active(true)
            .build();
        if family == StyleFamily::Character {
            list.append(&only_changes);
        }
        let apply = adw::SwitchRow::builder()
            .title(tr("Apply This New Style on Creation"))
            .active(true)
            .build();
        list.append(&apply);
        dialog.set_extra_child(Some(&list));
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("create", &tr("C_reate"));
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("create"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        let editor = editor.clone();
        dialog.connect_response(None, move |_, response| {
            let Some(this) = weak.upgrade() else { return };
            if response != "create" {
                return;
            }
            let name = name.text().trim().to_owned();
            let apply = apply.is_active();
            let only_changes = only_changes.is_active();
            let result = editor.style_edit(&tr("New Style"), |edit, from, to| match family {
                StyleFamily::Paragraph => edit.new_paragraph_style(&name, from, to, apply),
                StyleFamily::Character => {
                    let (format, paragraph) = edit.document().character_formats_at(from)?;
                    let attrs = if only_changes {
                        CharAttrs::from(format).difference(&paragraph)
                    } else {
                        CharAttrs::from(format)
                    };
                    edit.new_character_style(&name, attrs, from, to, apply)
                }
                StyleFamily::List => {
                    let paragraph = edit.document().paragraph(from.paragraph)?;
                    let current = edit
                        .document()
                        .styles()
                        .resolve_paragraph(paragraph.style(), paragraph.attrs())
                        .list_style;
                    let source = match current {
                        Some(id) => id,
                        None => edit.list_style_or_add("Bullet", ListStyle::bullet)?,
                    };
                    let id = edit.new_list_style(&name, source)?;
                    if apply {
                        edit.set_list_style(from, to, Some(id))?;
                    }
                    Ok(id)
                }
            });
            this.report(&tr("Cannot Make the Style"), result);
        });
        self.present(&dialog);
        dialog.set_focus(Some(&focus));
    }

    fn rename(self: &Rc<Self>, editor: &TextEditor, id: StyleId) {
        let Some(style) = editor.styles().get(id).cloned() else {
            return;
        };
        let dialog = adw::AlertDialog::new(Some(&tr("Rename Style")), None);
        let entry = gtk::Entry::builder()
            .text(style.name())
            .activates_default(true)
            .build();
        labelled(&entry, &tr("Name"));
        dialog.set_extra_child(Some(&entry));
        let focus = entry.clone();
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("rename", &tr("_Rename"));
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        let editor = editor.clone();
        dialog.connect_response(None, move |_, response| {
            let Some(this) = weak.upgrade() else { return };
            if response != "rename" {
                return;
            }
            let name = entry.text().to_string();
            let result = editor.style_edit(&tr("Rename Style"), |edit, _, _| {
                edit.rename_style(id, &name)
            });
            this.report(&tr("Cannot Rename the Style"), result);
        });
        self.present(&dialog);
        dialog.set_focus(Some(&focus));
    }

    /// Deletes a style; when text or other styles use it, first asks
    /// which style takes its place (STY-05).
    fn delete(self: &Rc<Self>, editor: &TextEditor, id: StyleId) {
        let styles = editor.styles();
        let Some(style) = styles.get(id).cloned() else {
            return;
        };
        let family = style.family();
        if id == styles.default_paragraph() {
            self.report::<()>(
                &tr("Cannot Delete the Style"),
                Err(ModelError::DefaultStyle),
            );
            return;
        }
        let used = !editor.document().style_uses(id).is_empty();
        if !used {
            let result = editor.style_edit(&tr("Delete Style"), |edit, _, _| {
                edit.delete_style(id, None)
            });
            self.report(&tr("Cannot Delete the Style"), result);
            return;
        }
        let mut choices: Vec<Option<StyleId>> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        if family == StyleFamily::Character {
            choices.push(None);
            names.push(tr("None"));
        }
        for other in sorted(&styles, family) {
            if other.id() != id {
                choices.push(Some(other.id()));
                names.push(other.name().to_owned());
            }
        }
        let start = if family == StyleFamily::Paragraph {
            choices
                .iter()
                .position(|c| *c == Some(styles.default_paragraph()))
                .unwrap_or(0)
        } else {
            0
        };
        let dialog = adw::AlertDialog::new(
            Some(&tr("Delete “{name}”?").replace("{name}", style.name())),
            Some(&tr("The style is in use. Choose a style to replace it.")),
        );
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let replacement = gtk::DropDown::builder()
            .model(&gtk::StringList::new(&names))
            .selected(start as u32)
            .build();
        labelled(&replacement, &tr("Replace With"));
        dialog.set_extra_child(Some(&replacement));
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("delete", &tr("_Delete"));
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("delete"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        let editor = editor.clone();
        dialog.connect_response(None, move |_, response| {
            let Some(this) = weak.upgrade() else { return };
            if response != "delete" {
                return;
            }
            let choice = choices
                .get(replacement.selected() as usize)
                .copied()
                .flatten();
            let result = editor.style_edit(&tr("Delete Style"), |edit, _, _| {
                edit.delete_style(id, choice)
            });
            this.report(&tr("Cannot Delete the Style"), result);
        });
        self.present(&dialog);
    }

    /// Shows the styles of the document and the style at the selection.
    pub fn show(&self, editor: &TextEditor) {
        let styles = editor.styles();
        let mut wanted = Vec::new();
        for family in [
            StyleFamily::Paragraph,
            StyleFamily::Character,
            StyleFamily::List,
        ] {
            for style in sorted(&styles, family) {
                wanted.push((
                    style.id(),
                    style.name().to_owned(),
                    style.shortcut(),
                    look(&styles, &style),
                ));
            }
        }
        if *self.shown.borrow() != wanted {
            self.rebuild(&styles);
            *self.shown.borrow_mut() = wanted;
        }
        self.updating.set(true);
        let paragraph = editor.format().style;
        let character = editor.character_style();
        let list = editor.list_style();
        let overrides = editor.overrides();
        let rows = self.rows.borrow();
        let mut paragraph_row = None;
        let mut character_row = None;
        let mut list_row = None;
        for row in rows.iter() {
            let is_paragraph = Some(row.id) == paragraph;
            let is_character = Some(row.id) == character;
            if is_paragraph {
                paragraph_row = Some(row.row.clone());
            }
            if is_character {
                character_row = Some(row.row.clone());
            }
            if Some(row.id) == list {
                list_row = Some(row.row.clone());
            }
            row.arrow.set_visible(
                (is_paragraph && overrides.paragraph) || (is_character && overrides.character),
            );
        }
        self.paragraphs.select_row(paragraph_row.as_ref());
        if let Some(row) = &paragraph_row {
            self.scroll_to(row);
        }
        match character_row {
            Some(row) => self.characters.select_row(Some(&row)),
            None => self
                .characters
                .select_row(self.characters.row_at_index(0).as_ref()),
        }
        match list_row {
            Some(row) => self.lists.select_row(Some(&row)),
            None => self.lists.select_row(self.lists.row_at_index(0).as_ref()),
        }
        self.updating.set(false);
    }

    /// Scrolls the lists so that a row shows, once it has its place.
    fn scroll_to(&self, row: &gtk::ListBoxRow) {
        let scrolled = self.scrolled.clone();
        let row = row.clone();
        glib::idle_add_local_once(move || {
            let Some(child) = scrolled.child() else {
                return;
            };
            let Some(bounds) = row.compute_bounds(&child) else {
                return;
            };
            let adjustment = scrolled.vadjustment();
            let (top, bottom) = (
                f64::from(bounds.y()),
                f64::from(bounds.y() + bounds.height()),
            );
            let (value, page) = (adjustment.value(), adjustment.page_size());
            if top < value {
                adjustment.set_value(top);
            } else if bottom > value + page {
                adjustment.set_value(bottom - page);
            }
        });
    }

    fn rebuild(&self, styles: &Styles) {
        self.updating.set(true);
        self.paragraphs.remove_all();
        self.characters.remove_all();
        self.lists.remove_all();
        let mut rows = Vec::new();
        // No character style and no list come first.
        for (list, name) in [(&self.characters, "none"), (&self.lists, "none-list")] {
            let none = gtk::ListBoxRow::builder().name(name).build();
            let label = gtk::Label::builder()
                .label(tr("None"))
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build();
            none.set_child(Some(&label));
            labelled(&none, &tr("None"));
            list.append(&none);
        }
        for family in [
            StyleFamily::Paragraph,
            StyleFamily::Character,
            StyleFamily::List,
        ] {
            for style in sorted(styles, family) {
                let line = gtk::Box::builder().spacing(6).build();
                let name = gtk::Label::builder()
                    .label(style.name())
                    .xalign(0.0)
                    .hexpand(true)
                    .ellipsize(pango::EllipsizeMode::End)
                    .build();
                name.set_attributes(Some(&preview(&look(styles, &style))));
                line.append(&name);
                if let Some(key) = style.shortcut() {
                    let key = gtk::Label::builder()
                        .label(key_name(key))
                        .css_classes(["dim-label", "caption"])
                        .build();
                    line.append(&key);
                }
                let arrow = gtk::Image::builder()
                    .icon_name("pan-end-symbolic")
                    .css_classes(["error"])
                    .tooltip_text(tr("The formatting differs from the style"))
                    .visible(false)
                    .build();
                line.append(&arrow);
                let row = gtk::ListBoxRow::builder().child(&line).build();
                labelled(&row, style.name());
                match family {
                    StyleFamily::Paragraph => self.paragraphs.append(&row),
                    StyleFamily::Character => self.characters.append(&row),
                    StyleFamily::List => self.lists.append(&row),
                }
                rows.push(Row {
                    id: style.id(),
                    row,
                    arrow,
                });
            }
        }
        *self.rows.borrow_mut() = rows;
        self.updating.set(false);
    }
}

/// A box that holds a list alone, at its top left, for the menu of the
/// list.
fn holder(list: &gtk::ListBox) -> gtk::Box {
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    holder.append(list);
    holder
}

/// The menu of a style.
fn style_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let change = gio::Menu::new();
    change.append(
        Some(&tr("Re_define Style from Selection")),
        Some("styles.redefine"),
    );
    change.append(Some(&tr("Re_vert to Defined Style")), Some("styles.revert"));
    change.append(Some(&tr("_Select All Uses")), Some("styles.select-uses"));
    menu.append_section(None, &change);
    let shortcut = gio::Menu::new();
    let item = gio::MenuItem::new(Some(&tr("None")), None);
    item.set_action_and_target_value(Some("styles.shortcut"), Some(&"0".to_variant()));
    shortcut.append_item(&item);
    for key in 1..=8u8 {
        let item = gio::MenuItem::new(Some(&key_name(key)), None);
        item.set_action_and_target_value(
            Some("styles.shortcut"),
            Some(&key.to_string().to_variant()),
        );
        shortcut.append_item(&item);
    }
    let keys = gio::Menu::new();
    keys.append_submenu(Some(&tr("_Shortcut")), &shortcut);
    menu.append_section(None, &keys);
    let manage = gio::Menu::new();
    manage.append(Some(&tr("_Rename Style…")), Some("styles.rename"));
    manage.append(Some(&tr("_Delete Style…")), Some("styles.delete"));
    menu.append_section(None, &manage);
    menu
}
