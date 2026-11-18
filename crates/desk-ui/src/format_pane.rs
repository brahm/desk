// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the format bar of interface B held for text, in the Format pane of
//! window M+ (step 2, INT-12, INT-13): the paragraph style over the tabs,
//! the character style in the Font group, and the link in the More tab.
//! The font, its size, style and color, and the alignment are the Font and
//! Alignment groups that a selected shape shows too
//! ([`crate::object_text_pane`]); the Fonts popover opens from the Family
//! row and the color chooser from the Text Color row, as in
//! `docs/interface/README.md`.
//!
//! The menus of the format bar stay as what Search Commands finds
//! ([`TextFormat::search_menus`]), and their commands as actions of the
//! window, which the keyboard runs too.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::StyleId;
use gtk::gio;

use crate::compact;
use crate::editing::Format;
use crate::formatting::{EditorOf, label_factory, labelled};
use crate::i18n::{n_, tr};

/// The line spacings of the menu, in percent of single, and their names.
pub const LINE_SPACINGS: [(u32, &str); 4] = [
    (100, n_("Single")),
    (115, n_("1.15")),
    (150, n_("1.5 Lines")),
    (200, n_("Double")),
];

/// The lists of the menu: the target of `win.list-style` and its name.
pub const LISTS: [(&str, &str); 4] = [
    ("none", n_("None")),
    ("bullets", n_("Bullets")),
    ("numbers", n_("Numbers")),
    ("tiered", n_("Tiered Numbers")),
];

/// The wraps of the menu: the target of `win.object-wrap` and its name.
pub const WRAPS: [(&str, &str); 7] = [
    ("both", n_("Around")),
    ("largest", n_("Largest Side")),
    ("left", n_("Left Side")),
    ("right", n_("Right Side")),
    ("above-and-below", n_("Above and Below")),
    ("through", n_("In Front of Text")),
    ("behind", n_("Behind Text")),
];

/// The alignments: an icon, the target of `win.align` and a name.
const ALIGNMENTS: [(&str, &str, &str); 4] = [
    ("format-justify-left-symbolic", "left", n_("Align Left")),
    ("format-justify-center-symbolic", "center", n_("Center")),
    ("format-justify-right-symbolic", "right", n_("Align Right")),
    ("format-justify-fill-symbolic", "justify", n_("Justify")),
];

/// The parts of the Format pane of text that the shape's Text tab does not
/// have.
pub struct TextFormat {
    /// The card over the tabs, with the paragraph style.
    header: compact::Group,
    styles: gtk::DropDown,
    style_ids: RefCell<Vec<StyleId>>,
    /// The character style, a row of the Font group (STY-02).
    character_row: compact::ActionRow,
    character_button: gtk::MenuButton,
    /// The character styles after none, as the items of its menu.
    character_ids: RefCell<Vec<Option<StyleId>>>,
    character_names: RefCell<Vec<String>>,
    character_menu: gio::Menu,
    /// The hyperlink at the selection, in the More tab: the Link inspector
    /// of Pages '09 is a popover (INT-13).
    link_group: compact::Group,
    link: gtk::MenuButton,
    /// Set while the pane shows the formatting at the selection, so that
    /// showing it does not change it.
    updating: Cell<bool>,
}

impl TextFormat {
    /// The parts for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let styles = gtk::DropDown::builder()
            .model(&gtk::StringList::new(&[]))
            .enable_search(true)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();
        // The button shows the name cut short, so that a long name does
        // not widen the pane; the list shows names whole.
        styles.set_factory(Some(&label_factory(Some(14))));
        styles.set_list_factory(Some(&label_factory(None)));
        labelled(&styles, &tr("Paragraph Style"));
        let style_row = compact::ActionRow::builder()
            .title(tr("Paragraph Style"))
            .build();
        style_row.add_suffix(&styles);
        let header = compact::Group::new();
        header.add(&style_row);

        let character_menu = gio::Menu::new();
        let character_button = gtk::MenuButton::builder()
            .menu_model(&character_menu)
            .label(tr("None"))
            .valign(gtk::Align::Center)
            .build();
        labelled(&character_button, &tr("Character Style"));
        let character_row = compact::ActionRow::builder()
            .title(tr("Character Style"))
            .build();
        character_row.add_suffix(&character_button);

        let link = gtk::MenuButton::builder()
            .icon_name("desk-link-symbolic")
            .tooltip_text(tr("Link"))
            .valign(gtk::Align::Center)
            .build();
        labelled(&link, &tr("Link"));
        let link_row = compact::ActionRow::builder()
            .title(tr("Link"))
            .subtitle(tr("A web page, an email or a bookmark"))
            .build();
        link_row.add_suffix(&link);
        let link_group = compact::Group::builder().title(tr("Link")).build();
        link_group.add(&link_row);

        let this = Rc::new(Self {
            header,
            styles,
            style_ids: RefCell::new(Vec::new()),
            character_row,
            character_button,
            character_ids: RefCell::new(Vec::new()),
            character_names: RefCell::new(Vec::new()),
            character_menu,
            link_group,
            link,
            updating: Cell::new(false),
        });
        let weak = Rc::downgrade(&this);
        this.styles.connect_selected_notify(move |dropdown| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let id = this
                .style_ids
                .borrow()
                .get(dropdown.selected() as usize)
                .copied();
            if let (Some(id), Some(editor)) = (id, editor()) {
                editor.set_paragraph_style(id);
                editor.focus();
            }
        });
        this
    }

    /// The card with the paragraph style, over the tabs of text.
    pub fn header(&self) -> &compact::Group {
        &self.header
    }

    /// The row of the character style, for the Font group.
    pub fn character_row(&self) -> &compact::ActionRow {
        &self.character_row
    }

    /// The Link group, for the More tab.
    pub fn link_group(&self) -> &compact::Group {
        &self.link_group
    }

    /// Gives the Link button the popover of the hyperlink.
    pub fn set_link_popover(&self, popover: &gtk::Popover) {
        self.link.set_popover(Some(popover));
    }

    /// The menus that the format bar of interface B had, with the names
    /// they go by, for Search Commands (GNM-10).
    pub fn search_menus(&self) -> Vec<(String, gio::MenuModel)> {
        vec![(tr("Format"), format_menu(&self.character_menu).upcast())]
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

    /// Shows the character styles of a document, after none, in the menu
    /// of the Character Style row. Returns whether they changed.
    pub fn set_character_styles(&self, styles: &[(StyleId, String)]) -> bool {
        let ids: Vec<Option<StyleId>> = std::iter::once(None)
            .chain(styles.iter().map(|(id, _)| Some(*id)))
            .collect();
        let mut names = vec![tr("None")];
        names.extend(styles.iter().map(|(_, n)| n.clone()));
        if *self.character_names.borrow() == names && *self.character_ids.borrow() == ids {
            return false;
        }
        self.character_menu.remove_all();
        for (index, name) in names.iter().enumerate() {
            let item = gio::MenuItem::new(Some(name), None);
            item.set_action_and_target_value(
                Some("win.character-style"),
                Some(&(index as i32).to_variant()),
            );
            self.character_menu.append_item(&item);
        }
        *self.character_ids.borrow_mut() = ids;
        *self.character_names.borrow_mut() = names;
        true
    }

    /// The place of a character style in its menu, as the state of
    /// `win.character-style`; -1 when it is not there.
    pub fn character_index(&self, style: Option<StyleId>) -> i32 {
        self.character_ids
            .borrow()
            .iter()
            .position(|id| *id == style)
            .map_or(-1, |i| i as i32)
    }

    /// The character style at a place of its menu: none for no style, and
    /// nothing when there is no such place.
    pub fn character_style(&self, index: i32) -> Option<Option<StyleId>> {
        let index = usize::try_from(index).ok()?;
        self.character_ids.borrow().get(index).copied()
    }

    /// Shows the paragraph and the character style at the selection.
    pub fn show(&self, format: &Format, character: Option<StyleId>) {
        self.updating.set(true);
        let index = format
            .style
            .and_then(|s| self.style_ids.borrow().iter().position(|id| *id == s));
        self.styles
            .set_selected(index.map_or(gtk::INVALID_LIST_POSITION, |i| i as u32));
        let name = usize::try_from(self.character_index(character))
            .ok()
            .and_then(|i| self.character_names.borrow().get(i).cloned());
        self.character_button
            .set_label(&name.unwrap_or_else(|| tr("None")));
        self.updating.set(false);
    }
}

/// The Line Spacing menu: single to double.
pub(crate) fn line_spacing_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    for (percent, name) in LINE_SPACINGS {
        let item = gio::MenuItem::new(Some(&tr(name)), None);
        item.set_action_and_target_value(
            Some("win.line-spacing"),
            Some(&percent.to_string().to_variant()),
        );
        menu.append_item(&item);
    }
    menu
}

/// The List menu: none, bullets or numbers.
fn list_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    for (target, name) in LISTS {
        let item = gio::MenuItem::new(Some(&tr(name)), None);
        item.set_action_and_target_value(Some("win.list-style"), Some(&target.to_variant()));
        menu.append_item(&item);
    }
    menu
}

/// What the format bar of interface B and its More menu ran, which Search
/// Commands keeps finding: the character style, strikethrough, columns,
/// the alignment, the line spacing, the list, and the Format menu of
/// Pages.
fn format_menu(character_menu: &gio::Menu) -> gio::Menu {
    let menu = gio::Menu::new();
    let character = gio::Menu::new();
    character.append_submenu(Some(&tr("C_haracter Style")), character_menu);
    character.append(Some(&tr("_Strikethrough")), Some("win.strikethrough"));
    let columns = gio::Menu::new();
    for (count, label) in [
        (1, n_("1 Column")),
        (2, n_("2 Columns")),
        (3, n_("3 Columns")),
        (4, n_("4 Columns")),
    ] {
        let item = gio::MenuItem::new(Some(&tr(label)), None);
        item.set_action_and_target_value(Some("win.columns"), Some(&count.to_variant()));
        columns.append_item(&item);
    }
    character.append_submenu(Some(&tr("Col_umns")), &columns);
    menu.append_section(None, &character);
    let paragraph = gio::Menu::new();
    let alignment = gio::Menu::new();
    for (_, target, name) in ALIGNMENTS {
        let item = gio::MenuItem::new(Some(&tr(name)), None);
        item.set_action_and_target_value(Some("win.align"), Some(&target.to_variant()));
        alignment.append_item(&item);
    }
    paragraph.append_submenu(Some(&tr("_Alignment")), &alignment);
    paragraph.append_submenu(Some(&tr("_Line Spacing")), &line_spacing_menu());
    paragraph.append_submenu(Some(&tr("L_ist")), &list_menu());
    menu.append_section(None, &paragraph);
    let format = crate::window::format_menu();
    for i in 0..format.n_items() {
        if let Some(section) = format.item_link(i, "section") {
            menu.append_section(None, &section);
        }
    }
    menu
}

/// The Arrange group of the Arrange tab: the order of the selected
/// objects, and the menu of aligning, distributing and flipping them, which
/// the format bar of interface B had for objects (INT-12).
pub(crate) fn arrange_group() -> compact::Group {
    let group = compact::Group::builder().title(tr("Arrange")).build();
    let order = gtk::Box::builder()
        .css_classes(["linked"])
        .valign(gtk::Align::Center)
        .build();
    for (icon, action, name) in [
        (
            "desk-front-symbolic",
            "win.bring-to-front",
            n_("Bring to Front"),
        ),
        ("desk-back-symbolic", "win.send-to-back", n_("Send to Back")),
    ] {
        let button = gtk::Button::builder()
            .icon_name(icon)
            .action_name(action)
            .tooltip_text(tr(name))
            .build();
        labelled(&button, &tr(name));
        order.append(&button);
    }
    let order_row = compact::ActionRow::builder().title(tr("Order")).build();
    order_row.add_suffix(&order);
    group.add(&order_row);
    let more = gtk::MenuButton::builder()
        .icon_name("desk-arrange-symbolic")
        .menu_model(&crate::window::arrange_menu())
        .tooltip_text(tr("Arrange"))
        .valign(gtk::Align::Center)
        .build();
    labelled(&more, &tr("Arrange"));
    let more_row = compact::ActionRow::builder()
        .title(tr("Align, Distribute and Flip"))
        .build();
    more_row.add_suffix(&more);
    group.add(&more_row);
    group
}

/// The menu of an object's More commands that the format bar had and the
/// Inspector has no row for, for Search Commands.
pub(crate) fn object_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let picture = gio::Menu::new();
    picture.append(Some(&tr("_Instant Alpha")), Some("win.instant-alpha"));
    picture.append(Some(&tr("Un_mask")), Some("win.unmask"));
    menu.append_section(None, &picture);
    let copy = gio::Menu::new();
    copy.append(
        Some(&crate::i18n::trc("Arrange", "Dupl_icate")),
        Some("win.duplicate-objects"),
    );
    copy.append(Some(&tr("_Lock")), Some("win.lock"));
    copy.append(Some(&tr("U_nlock")), Some("win.unlock"));
    menu.append_section(None, &copy);
    let defaults = gio::Menu::new();
    defaults.append(
        Some(&tr("Reapply _Defaults to Selection")),
        Some("win.reapply-defaults"),
    );
    defaults.append(
        Some(&tr("Set as Default _Text Box Appearance")),
        Some("win.default-text-box"),
    );
    defaults.append(
        Some(&tr("Show Co_nnection Lines")),
        Some("win.connection-lines"),
    );
    menu.append_section(None, &defaults);
    let wraps = gio::Menu::new();
    for (target, name) in WRAPS {
        let item = gio::MenuItem::new(Some(&tr(name)), None);
        item.set_action_and_target_value(Some("win.object-wrap"), Some(&target.to_variant()));
        wraps.append_item(&item);
    }
    let wrap = gio::Menu::new();
    wrap.append_submenu(Some(&tr("_Wrap")), &wraps);
    menu.append_section(None, &wrap);
    menu
}
