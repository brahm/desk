// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The document as an outline (TXT-14): a row for each paragraph, the
//! topics indented by their level and the text under them, each with a
//! handle that drags the topic with its subtopics and a triangle that
//! hides them.
//!
//! Return makes a topic of the same level, Tab a subtopic and Shift+Tab a
//! topic one level up. The bar above chooses the levels that show and
//! whether a topic shows only its first line and its objects in
//! miniature, and copies a topic as the
//! outline of Keynote takes it (IOP-26). The handles and triangles are
//! not part of the document, and never print.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::editing::OutlineRow;
use crate::formatting::{EditorOf, labelled};
use crate::i18n::tr;

/// How far each level is indented, in pixels.
const INDENT: i32 = 24;

/// The largest side of an object in miniature, in pixels.
const MINIATURE: u32 = 48;

/// The outline of a document window.
pub struct OutlineView {
    root: gtk::Box,
    list: gtk::Box,
    editor: EditorOf,
    /// The levels that show: 1 to 9, and 10 for the text too.
    levels: gtk::SpinButton,
    truncate: gtk::ToggleButton,
    /// Whether the objects of each row show in miniature (TXT-14).
    miniatures: gtk::ToggleButton,
    /// The topics whose subtopics are hidden, by the index of their
    /// paragraph.
    collapsed: RefCell<HashSet<usize>>,
    /// The text of each row, by the index of its paragraph.
    views: RefCell<Vec<Option<gtk::TextView>>>,
    /// Whether a change of the document comes from the outline itself,
    /// which then keeps its rows.
    editing: Cell<bool>,
    /// The row and offset to put the cursor in after the rows are made.
    focus: Cell<Option<(usize, usize)>>,
    /// The row with the cursor.
    current: Cell<usize>,
    shown: Cell<bool>,
    /// Whether the rows are to be made again soon.
    pending: Cell<bool>,
}

impl OutlineView {
    /// The outline of the document of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let levels = gtk::SpinButton::with_range(1.0, 10.0, 1.0);
        levels.set_value(10.0);
        levels.set_tooltip_text(Some(&tr("Visible Levels")));
        labelled(&levels, &tr("Visible Levels"));
        let truncate = gtk::ToggleButton::builder()
            .icon_name("format-justify-left-symbolic")
            .tooltip_text(tr("Show Only the First Line"))
            .build();
        labelled(&truncate, &tr("Show Only the First Line"));
        let miniatures = gtk::ToggleButton::builder()
            .icon_name("image-x-generic-symbolic")
            .tooltip_text(tr("Show Objects in Miniature"))
            .build();
        labelled(&miniatures, &tr("Show Objects in Miniature"));
        let button = |icon: &str, tip: String| {
            gtk::Button::builder()
                .icon_name(icon)
                .tooltip_text(tip)
                .build()
        };
        let promote = button("go-previous-symbolic", tr("Promote"));
        let demote = button("go-next-symbolic", tr("Demote"));
        let copy = button("edit-copy-symbolic", tr("Copy as Outline"));
        let paste = button("edit-paste-symbolic", tr("Paste as Outline"));
        let bar = gtk::Box::builder()
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();
        bar.append(&promote);
        bar.append(&demote);
        bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        bar.append(&gtk::Label::new(Some(&tr("Levels"))));
        bar.append(&levels);
        bar.append(&truncate);
        bar.append(&miniatures);
        bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        bar.append(&copy);
        bar.append(&paste);
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_top(12)
            .margin_bottom(48)
            .margin_start(24)
            .margin_end(24)
            .build();
        let clamp = adw::Clamp::builder().maximum_size(800).child(&list).build();
        let scroll = gtk::ScrolledWindow::builder()
            .child(&clamp)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["view"])
            .build();
        root.append(&bar);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&scroll);
        root.update_property(&[gtk::accessible::Property::Label(&tr("Outline"))]);
        let this = Rc::new(Self {
            root,
            list,
            editor,
            levels,
            truncate,
            miniatures,
            collapsed: RefCell::new(HashSet::new()),
            views: RefCell::new(Vec::new()),
            editing: Cell::new(false),
            focus: Cell::new(None),
            current: Cell::new(0),
            shown: Cell::new(false),
            pending: Cell::new(false),
        });
        let weak = Rc::downgrade(&this);
        this.levels.connect_value_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.rebuild();
            }
        });
        let weak = Rc::downgrade(&this);
        this.truncate.connect_toggled(move |_| {
            if let Some(this) = weak.upgrade() {
                this.rebuild();
            }
        });
        let weak = Rc::downgrade(&this);
        this.miniatures.connect_toggled(move |_| {
            if let Some(this) = weak.upgrade() {
                this.rebuild();
            }
        });
        let weak = Rc::downgrade(&this);
        promote.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.shift_level(this.current.get(), -1);
            }
        });
        let weak = Rc::downgrade(&this);
        demote.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.shift_level(this.current.get(), 1);
            }
        });
        let weak = Rc::downgrade(&this);
        copy.connect_clicked(move |button| {
            if let Some(this) = weak.upgrade()
                && let Some(editor) = (this.editor)()
            {
                let text = editor.outline_text(this.current.get());
                button.clipboard().set_text(&text);
            }
        });
        // Text from the outline of Keynote, or from anywhere, as topics
        // by its tabs, after the topic with the cursor (IOP-26).
        let weak = Rc::downgrade(&this);
        paste.connect_clicked(move |button| {
            let weak = weak.clone();
            button
                .clipboard()
                .read_text_async(gio::Cancellable::NONE, move |text| {
                    if let (Some(this), Ok(Some(text))) = (weak.upgrade(), text)
                        && let Some(editor) = (this.editor)()
                    {
                        let at = editor.topic_range(this.current.get()).end;
                        this.focus.set(Some((at, 0)));
                        editor.paste_outline(at, &text);
                    }
                });
        });
        this
    }

    /// The widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Shows or hides it; it makes its rows only while it shows.
    pub fn set_shown(self: &Rc<Self>, shown: bool) {
        self.shown.set(shown);
        if shown {
            self.rebuild();
            self.focus_row(self.current.get(), 0);
        }
    }

    /// Makes the rows again after a change of the document, unless the
    /// outline made it while typing.
    pub fn document_changed(self: &Rc<Self>) {
        if self.shown.get() && !self.editing.get() && !self.pending.replace(true) {
            // After the key that changed it, whose row goes away.
            let weak = Rc::downgrade(self);
            glib::idle_add_local_once(move || {
                if let Some(this) = weak.upgrade() {
                    this.pending.set(false);
                    this.rebuild();
                }
            });
        }
    }

    fn rebuild(self: &Rc<Self>) {
        if !self.shown.get() {
            return;
        }
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let Some(editor) = (self.editor)() else {
            return;
        };
        let rows = editor.outline_rows();
        let visible = self.levels.value() as u8;
        let mut views = vec![None; rows.len()];
        // The level of the topic each row is under, and whether a topic
        // above it is collapsed.
        let mut topic = 0u8;
        let mut hidden_below: Option<u8> = None;
        for (index, row) in rows.iter().enumerate() {
            let depth = match row.level {
                Some(level) => {
                    topic = level;
                    level
                }
                None => topic + 1,
            };
            if hidden_below.is_some_and(|l| depth > l) {
                continue;
            }
            hidden_below = None;
            if depth > visible {
                continue;
            }
            if row.level.is_some() && self.collapsed.borrow().contains(&index) {
                hidden_below = Some(depth);
            }
            let (widget, text) = self.row(index, row, depth);
            self.list.append(&widget);
            views[index] = Some(text);
        }
        *self.views.borrow_mut() = views;
        if let Some((row, offset)) = self.focus.take() {
            self.focus_row(row, offset);
        }
    }

    /// The row of a paragraph, and its text.
    fn row(
        self: &Rc<Self>,
        index: usize,
        row: &OutlineRow,
        depth: u8,
    ) -> (gtk::Box, gtk::TextView) {
        let line = gtk::Box::builder().spacing(4).build();
        line.set_margin_start(INDENT * i32::from(depth.saturating_sub(1)));
        let handle = gtk::Image::builder()
            .icon_name("list-drag-handle-symbolic")
            .css_classes(["dim-label"])
            .valign(gtk::Align::Start)
            .margin_top(4)
            .build();
        handle.set_cursor_from_name(Some("grab"));
        handle.set_tooltip_text(Some(&tr("Drag to move the topic")));
        let triangle = gtk::Button::builder()
            .css_classes(["flat", "circular"])
            .valign(gtk::Align::Start)
            .build();
        let collapsed = self.collapsed.borrow().contains(&index);
        triangle.set_icon_name(if collapsed {
            "pan-end-symbolic"
        } else {
            "pan-down-symbolic"
        });
        triangle.set_opacity(if row.has_children && row.level.is_some() {
            1.0
        } else {
            0.0
        });
        triangle.set_sensitive(row.has_children && row.level.is_some());
        triangle.set_tooltip_text(Some(&tr("Show or Hide the Subtopics")));
        let weak = Rc::downgrade(self);
        triangle.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                {
                    let mut collapsed = this.collapsed.borrow_mut();
                    if !collapsed.remove(&index) {
                        collapsed.insert(index);
                    }
                }
                this.rebuild();
            }
        });
        let text = gtk::TextView::builder()
            .hexpand(true)
            .wrap_mode(if self.truncate.is_active() {
                gtk::WrapMode::None
            } else {
                gtk::WrapMode::WordChar
            })
            .accepts_tab(false)
            .top_margin(2)
            .bottom_margin(2)
            .build();
        text.buffer().set_text(&row.text);
        if row.level.is_none() {
            text.add_css_class("dim-label");
        } else if depth == 1 {
            text.add_css_class("heading");
        }
        labelled(
            &text,
            &match row.level {
                Some(level) => tr("Topic, level {n}").replace("{n}", &level.to_string()),
                None => tr("Text"),
            },
        );
        self.connect_text(&text, index);
        line.append(&handle);
        line.append(&triangle);
        if self.miniatures.is_active() && !row.objects.is_empty() {
            let column = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .hexpand(true)
                .build();
            column.append(&text);
            column.append(&miniatures(&row.objects));
            line.append(&column);
        } else {
            line.append(&text);
        }
        self.connect_drag(&handle, &line, index);
        (line, text)
    }

    fn connect_text(self: &Rc<Self>, text: &gtk::TextView, index: usize) {
        let weak = Rc::downgrade(self);
        text.buffer().connect_changed(move |buffer| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            let typed = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            if let Some(editor) = (this.editor)() {
                this.editing.set(true);
                editor.set_paragraph_text(index, &typed);
                this.editing.set(false);
            }
        });
        let weak = Rc::downgrade(self);
        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter(move |_| {
            if let Some(this) = weak.upgrade() {
                this.current.set(index);
            }
        });
        text.add_controller(focus);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        let view = text.downgrade();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let (Some(this), Some(view)) = (weak.upgrade(), view.upgrade()) else {
                return glib::Propagation::Proceed;
            };
            let buffer = view.buffer();
            let cursor = buffer.iter_at_mark(&buffer.get_insert());
            let offset = cursor_bytes(&buffer, &cursor);
            let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
            match key {
                gdk::Key::Return | gdk::Key::KP_Enter => this.split(index, offset),
                gdk::Key::Tab | gdk::Key::ISO_Left_Tab => {
                    this.shift_level(
                        index,
                        if shift || key == gdk::Key::ISO_Left_Tab {
                            -1
                        } else {
                            1
                        },
                    );
                }
                gdk::Key::BackSpace if cursor.offset() == 0 && !buffer.has_selection() => {
                    this.join(index);
                }
                gdk::Key::Up if !view.backward_display_line(&mut cursor.clone()) => {
                    this.step(index, false);
                }
                gdk::Key::Down if !view.forward_display_line(&mut cursor.clone()) => {
                    this.step(index, true);
                }
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        text.add_controller(keys);
    }

    /// Return: a new topic of the same level after the cursor.
    fn split(self: &Rc<Self>, index: usize, offset: usize) {
        if let Some(editor) = (self.editor)() {
            self.focus.set(Some((index + 1, 0)));
            editor.outline_split(index, offset);
        }
    }

    /// Backspace at the start: the paragraph joins the one before.
    fn join(self: &Rc<Self>, index: usize) {
        if index == 0 {
            return;
        }
        if let Some(editor) = (self.editor)() {
            let offset = editor
                .outline_rows()
                .get(index - 1)
                .map_or(0, |r| r.text.chars().count());
            self.focus.set(Some((index - 1, offset)));
            editor.outline_join_previous(index);
        }
    }

    /// Tab and Shift+Tab: a level down or up. Text becomes a topic one
    /// level under the topic it is in, and a first level topic stays.
    fn shift_level(self: &Rc<Self>, index: usize, by: i8) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let rows = editor.outline_rows();
        let Some(row) = rows.get(index) else {
            return;
        };
        let level = match row.level {
            Some(level) => {
                let moved = (i16::from(level) + i16::from(by))
                    .clamp(1, i16::from(desk_model::MAX_OUTLINE_LEVEL));
                u8::try_from(moved).ok()
            }
            None => {
                let above = rows[..index]
                    .iter()
                    .rev()
                    .find_map(|r| r.level)
                    .unwrap_or(0);
                Some((above + 1).min(desk_model::MAX_OUTLINE_LEVEL))
            }
        };
        if level == row.level {
            return;
        }
        let offset = self.cursor_offset(index);
        self.focus.set(Some((index, offset)));
        editor.set_outline_level(index, level);
    }

    /// Up at the first line or Down at the last: the row before or after.
    fn step(&self, index: usize, forward: bool) {
        let views = self.views.borrow();
        let next = if forward {
            (index + 1..views.len()).find(|&i| views[i].is_some())
        } else {
            (0..index).rev().find(|&i| views[i].is_some())
        };
        drop(views);
        if let Some(next) = next {
            self.focus_row(next, if forward { 0 } else { usize::MAX });
        }
    }

    fn cursor_offset(&self, index: usize) -> usize {
        self.views
            .borrow()
            .get(index)
            .and_then(Option::as_ref)
            .map_or(0, |view| {
                let buffer = view.buffer();
                buffer.iter_at_mark(&buffer.get_insert()).offset().max(0) as usize
            })
    }

    /// Puts the cursor in a row, at a character.
    fn focus_row(&self, index: usize, offset: usize) {
        let views = self.views.borrow();
        let Some(view) = views
            .get(index)
            .and_then(Option::as_ref)
            .or_else(|| views.iter().flatten().next())
        else {
            return;
        };
        let buffer = view.buffer();
        let count = buffer.char_count().max(0) as usize;
        let at = buffer.iter_at_offset(i32::try_from(offset.min(count)).unwrap_or(0));
        buffer.place_cursor(&at);
        view.grab_focus();
        self.current.set(index);
    }

    fn connect_drag(self: &Rc<Self>, handle: &gtk::Image, line: &gtk::Box, index: usize) {
        let drag = gtk::DragSource::builder()
            .actions(gdk::DragAction::MOVE)
            .build();
        drag.connect_prepare(move |_, _, _| {
            Some(gdk::ContentProvider::for_value(
                &format!("desk-topic:{index}").to_value(),
            ))
        });
        handle.add_controller(drag);
        let drop = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
        let weak = Rc::downgrade(self);
        let target = line.downgrade();
        drop.connect_drop(move |_, value, _, y| {
            let (Some(this), Some(target)) = (weak.upgrade(), target.upgrade()) else {
                return false;
            };
            let Some(from) = value.get::<String>().ok().and_then(|v| {
                v.strip_prefix("desk-topic:")
                    .and_then(|n| n.parse::<usize>().ok())
            }) else {
                return false;
            };
            let Some(editor) = (this.editor)() else {
                return false;
            };
            // Before the topic dropped on, or after it and its subtopics.
            let before = if y < f64::from(target.height()) / 2.0 {
                index
            } else {
                editor.topic_range(index).end
            };
            let range = editor.topic_range(from);
            let at = if before > range.end {
                before - range.len()
            } else {
                before
            };
            this.focus.set(Some((at, 0)));
            editor.move_topic(from, before);
            true
        });
        line.add_controller(drop);
    }
}

/// The offset of a place of a buffer in bytes of its text.
fn cursor_bytes(buffer: &gtk::TextBuffer, at: &gtk::TextIter) -> usize {
    buffer.text(&buffer.start_iter(), at, false).len()
}

/// The objects of a row in miniature, side by side (TXT-14).
fn miniatures(objects: &[desk_render::DisplayList]) -> gtk::Box {
    let strip = gtk::Box::builder()
        .spacing(6)
        .margin_bottom(4)
        .accessible_role(gtk::AccessibleRole::Group)
        .build();
    strip.update_property(&[gtk::accessible::Property::Label(&tr("Objects"))]);
    let mut painter = desk_render::Painter::new().ok();
    for object in objects {
        let picture = gtk::Picture::builder()
            .can_shrink(false)
            .css_classes(["card"])
            .build();
        if let Some(png) = painter
            .as_mut()
            .and_then(|p| p.thumbnail_png(object, MINIATURE).ok())
            && let Ok(texture) = gdk::Texture::from_bytes(&glib::Bytes::from_owned(png))
        {
            picture.set_paintable(Some(&texture));
        }
        strip.append(&picture);
    }
    strip
}
