// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The page thumbnails beside the pages (INT-19, EST-17, TXT-02).
//!
//! Each page shows small, with its number, and the pages of a section in
//! one frame. A click on a page shows it and selects its section, with a
//! yellow frame; Ctrl and Shift select more. The sections selected are cut,
//! copied, pasted, duplicated and deleted, after a question, from the keys
//! and the menu of the thumbnails, and dragged to another place, or copied
//! there with Ctrl.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::editing::{TextEditor, has_sections_clip};
use crate::formatting::EditorOf;
use crate::i18n::{tr, trn};
use crate::view::DocumentView;

/// The width of a page, in pixels.
const WIDTH: u32 = 120;

/// The thumbnails of a document window.
pub struct Thumbnails {
    root: gtk::ScrolledWindow,
    list: gtk::Box,
    editor: EditorOf,
    view: DocumentView,
    /// The sections selected, by index, in the order chosen.
    selected: RefCell<Vec<usize>>,
    /// The frames of the sections.
    frames: RefCell<Vec<(usize, gtk::Box)>>,
    shown: Cell<bool>,
    /// Whether a refresh is on its way.
    pending: Cell<bool>,
    /// The window to ask in.
    parent: RefCell<Option<gtk::Window>>,
    /// Cut, Copy, Paste, Duplicate and Delete.
    actions: gio::SimpleActionGroup,
}

impl Thumbnails {
    /// The thumbnails of the document of a view.
    pub fn new(view: &DocumentView, editor: EditorOf) -> Rc<Self> {
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        let root = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(i32::try_from(WIDTH).unwrap_or(120) + 48)
            .focusable(true)
            .build();
        root.update_property(&[gtk::accessible::Property::Label(&tr("Page Thumbnails"))]);
        let this = Rc::new(Self {
            root,
            list,
            editor,
            view: view.clone(),
            selected: RefCell::new(Vec::new()),
            frames: RefCell::new(Vec::new()),
            shown: Cell::new(false),
            pending: Cell::new(false),
            parent: RefCell::new(None),
            actions: gio::SimpleActionGroup::new(),
        });
        this.add_actions();
        this.connect_keys();
        this.connect_menu();
        this
    }

    /// The widget.
    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    /// Shows or hides them; they draw only while they show.
    pub fn set_shown(self: &Rc<Self>, shown: bool, parent: &gtk::Window) {
        self.shown.set(shown);
        *self.parent.borrow_mut() = Some(parent.clone());
        if shown {
            self.refresh();
        }
    }

    /// Draws them again soon, after the document changed.
    pub fn refresh(self: &Rc<Self>) {
        if !self.shown.get() || self.pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            if let Some(this) = weak.upgrade() {
                this.pending.set(false);
                this.rebuild();
            }
        });
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        self.frames.borrow_mut().clear();
        let Some(editor) = (self.editor)() else {
            return;
        };
        let pages = editor.pages();
        let sections = editor.page_sections();
        let count = sections.iter().max().map_or(0, |m| m + 1);
        self.selected.borrow_mut().retain(|&s| s < count);
        let mut painter = desk_render::Painter::new().ok();
        let mut frame: Option<(usize, gtk::Box)> = None;
        for (index, page) in pages.iter().enumerate() {
            let section = sections.get(index).copied().unwrap_or(0);
            if frame.as_ref().is_none_or(|(s, _)| *s != section) {
                let new = self.section_frame(section);
                self.list.append(&new);
                self.frames.borrow_mut().push((section, new.clone()));
                frame = Some((section, new));
            }
            let picture = gtk::Picture::builder()
                .can_shrink(false)
                .css_classes(["card"])
                .halign(gtk::Align::Center)
                .build();
            if let Some(png) = painter
                .as_mut()
                .and_then(|p| p.thumbnail_png(page, WIDTH).ok())
                && let Ok(texture) = gdk::Texture::from_bytes(&glib::Bytes::from_owned(png))
            {
                picture.set_paintable(Some(&texture));
            }
            let number = (index + 1).to_string();
            let label = gtk::Label::builder()
                .label(&number)
                .css_classes(["caption"])
                .build();
            let item = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .build();
            item.append(&picture);
            item.append(&label);
            item.update_property(&[gtk::accessible::Property::Label(
                &tr("Page {n}").replace("{n}", &number),
            )]);
            self.connect_page(&item, index, section);
            if let Some((_, frame)) = &frame {
                frame.append(&item);
            }
        }
        self.show_selection();
    }

    /// The frame of the pages of a section, which drags start from and
    /// drops land on.
    fn section_frame(self: &Rc<Self>, section: usize) -> gtk::Box {
        let frame = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["desk-thumb-section"])
            .build();
        let drag = gtk::DragSource::builder()
            .actions(gdk::DragAction::MOVE | gdk::DragAction::COPY)
            .build();
        let weak = Rc::downgrade(self);
        drag.connect_prepare(move |_, _, _| {
            let this = weak.upgrade()?;
            if !this.selected.borrow().contains(&section) {
                *this.selected.borrow_mut() = vec![section];
                this.show_selection();
            }
            Some(gdk::ContentProvider::for_value(&"desk-sections".to_value()))
        });
        frame.add_controller(drag);
        let drop = gtk::DropTarget::new(
            glib::Type::STRING,
            gdk::DragAction::MOVE | gdk::DragAction::COPY,
        );
        let weak = Rc::downgrade(self);
        let target = frame.downgrade();
        drop.connect_drop(move |drop, value, _, y| {
            let (Some(this), Some(target)) = (weak.upgrade(), target.upgrade()) else {
                return false;
            };
            if value.get::<String>().ok().as_deref() != Some("desk-sections") {
                return false;
            }
            // Before the section, or after it, by the half dropped on.
            let before = if y < f64::from(target.height()) / 2.0 {
                section
            } else {
                section + 1
            };
            let copy = drop
                .current_event_state()
                .contains(gdk::ModifierType::CONTROL_MASK);
            this.move_selected(before, copy);
            true
        });
        frame.add_controller(drop);
        frame
    }

    fn connect_page(self: &Rc<Self>, item: &gtk::Box, page: usize, section: usize) {
        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |gesture, _, _, _| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            this.root.grab_focus();
            let modifiers = gesture.current_event_state();
            {
                let mut selected = this.selected.borrow_mut();
                if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
                    if let Some(at) = selected.iter().position(|&s| s == section) {
                        selected.remove(at);
                    } else {
                        selected.push(section);
                    }
                } else if modifiers.contains(gdk::ModifierType::SHIFT_MASK)
                    && let Some(&from) = selected.last()
                {
                    let (a, b) = (from.min(section), from.max(section));
                    *selected = (a..=b).collect();
                } else {
                    *selected = vec![section];
                }
            }
            this.show_selection();
            this.view.scroll_to_page(page);
        });
        item.add_controller(click);
    }

    /// Frames the sections selected in yellow.
    fn show_selection(&self) {
        let selected = self.selected.borrow();
        for (section, frame) in self.frames.borrow().iter() {
            if selected.contains(section) {
                frame.add_css_class("selected");
            } else {
                frame.remove_css_class("selected");
            }
        }
        let any = !selected.is_empty();
        for name in ["cut", "copy", "duplicate", "delete"] {
            if let Some(action) = self
                .actions
                .lookup_action(name)
                .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
            {
                action.set_enabled(any);
            }
        }
    }

    fn selection(&self) -> Vec<usize> {
        let mut selected = self.selected.borrow().clone();
        selected.sort_unstable();
        selected.dedup();
        selected
    }

    fn with_editor(&self, f: impl FnOnce(&TextEditor, Vec<usize>)) {
        let selected = self.selection();
        if let Some(editor) = (self.editor)() {
            f(&editor, selected);
        }
    }

    fn move_selected(self: &Rc<Self>, before: usize, copy: bool) {
        let selected = self.selection();
        if selected.is_empty() || (!copy && selected.contains(&before) && selected.len() == 1) {
            return;
        }
        if let Some(editor) = (self.editor)() {
            editor.move_sections(&selected, before, copy);
        }
        self.selected.borrow_mut().clear();
    }

    /// Deletes the sections selected, after asking.
    fn delete_selected(self: &Rc<Self>) {
        let selected = self.selection();
        if selected.is_empty() {
            return;
        }
        let dialog = adw::AlertDialog::new(
            Some(&trn(
                "Delete Section?",
                "Delete Sections?",
                selected.len() as u64,
            )),
            Some(
                &trn(
                    "The section and all its pages will be deleted.",
                    "The {n} sections and all their pages will be deleted.",
                    selected.len() as u64,
                )
                .replace("{n}", &selected.len().to_string()),
            ),
        );
        dialog.add_response("cancel", &tr("_Cancel"));
        dialog.add_response("delete", &tr("_Delete"));
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.choose(
            self.parent.borrow().as_ref(),
            gio::Cancellable::NONE,
            move |response| {
                if let (Some(this), "delete") = (weak.upgrade(), response.as_str()) {
                    this.with_editor(|editor, selected| editor.delete_sections(&selected));
                    this.selected.borrow_mut().clear();
                }
            },
        );
    }

    fn add_actions(self: &Rc<Self>) {
        let group = &self.actions;
        let add = |name: &str, run: fn(&Rc<Self>)| {
            let action = gio::SimpleAction::new(name, None);
            let weak: Weak<Self> = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(this) = weak.upgrade() {
                    run(&this);
                }
            });
            group.add_action(&action);
        };
        add("cut", |this| {
            this.with_editor(|editor, selected| editor.cut_sections(&selected));
            this.selected.borrow_mut().clear();
        });
        add("copy", |this| {
            this.with_editor(|editor, selected| editor.copy_sections(&selected));
        });
        add("paste", |this| {
            // After the last section selected, or at the end.
            let count = (this.editor)()
                .map(|e| e.page_sections().last().map_or(0, |s| s + 1))
                .unwrap_or(0);
            let before = this.selection().last().map_or(count, |s| s + 1);
            if has_sections_clip()
                && let Some(editor) = (this.editor)()
            {
                editor.paste_sections(before);
            }
        });
        add("duplicate", |this| {
            let selected = this.selection();
            this.with_editor(|editor, selected| editor.duplicate_sections(&selected));
            // The copies are selected, after the last of them.
            if let Some(&last) = selected.last() {
                *this.selected.borrow_mut() = (last + 1..=last + selected.len()).collect();
            }
        });
        add("delete", Self::delete_selected);
        self.root.insert_action_group("thumbs", Some(group));
    }

    fn connect_keys(self: &Rc<Self>) {
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
            let action = match key {
                gdk::Key::Delete | gdk::Key::BackSpace => "delete",
                gdk::Key::x if control => "cut",
                gdk::Key::c if control => "copy",
                gdk::Key::v if control => "paste",
                gdk::Key::d if control => "duplicate",
                _ => return glib::Propagation::Proceed,
            };
            let _ = this.root.activate_action(&format!("thumbs.{action}"), None);
            glib::Propagation::Stop
        });
        self.root.add_controller(keys);
    }

    fn connect_menu(self: &Rc<Self>) {
        let menu = gio::Menu::new();
        menu.append(Some(&tr("Cu_t")), Some("thumbs.cut"));
        menu.append(Some(&tr("_Copy")), Some("thumbs.copy"));
        menu.append(Some(&tr("_Paste")), Some("thumbs.paste"));
        menu.append(Some(&tr("D_uplicate")), Some("thumbs.duplicate"));
        menu.append(Some(&tr("_Delete")), Some("thumbs.delete"));
        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&self.list);
        popover.set_has_arrow(false);
        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        click.connect_pressed(move |_, _, x, y| {
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
        });
        self.list.add_controller(click);
    }
}
