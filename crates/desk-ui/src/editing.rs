// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Editing in the document area: the cursor and the selection with the
//! pointer and the keyboard, typing through input methods, the clipboard,
//! and Undo and Redo (TXT-03 to TXT-05, DOC-12).
//!
//! Each change runs as a transaction of the model, which Undo takes back,
//! and the document is laid out again.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use desk_layout::DocumentLayout;
use desk_model::{
    CharAttrs, CharFormat, Document, Editor, LINE_BREAK, ModelError, ParaAttrs, ParaFormat,
    StyleFamily, StyleId,
};
use desk_render::DisplayList;
use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::text_nav::{self, Selection, Spot};
use crate::view::{DocumentView, WeakView};

/// The position of the model at a spot.
fn at_spot(document: &Document, spot: Spot) -> Result<desk_model::Position, ModelError> {
    spot.position(document).ok_or(ModelError::IndexOutOfBounds {
        index: spot.paragraph,
        len: document.body().len(),
    })
}

struct State {
    editor: Editor,
    layout: DocumentLayout,
    selection: Selection,
    /// The distance from the left of the page that Up and Down keep.
    goal: Option<f64>,
    /// The selection before and after each action of the history, so that
    /// Undo and Redo put it back, and a number that names the action.
    undo: Vec<(Selection, Selection, u64)>,
    redo: Vec<(Selection, Selection, u64)>,
    /// The number of the next action.
    next_action: u64,
    /// The last action done when the document was saved, or opened.
    saved: Option<u64>,
    /// Formatting chosen with nothing selected, which the next text typed
    /// at the cursor takes.
    pending: Option<CharAttrs>,
    on_select: Vec<Rc<dyn Fn()>>,
    /// Whether the invisible characters show (INT-10).
    invisibles: bool,
    on_change: Vec<Rc<dyn Fn()>>,
    /// The text of a word or paragraph that a double or triple click
    /// selected, which a drag then extends by whole words or paragraphs.
    unit: Unit,
    drag_from: Option<Selection>,
    /// Whether the last action was typing at the cursor, which the next
    /// letter typed continues as one step of the history.
    typing: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Unit {
    Character,
    Word,
    Paragraph,
}

/// The formatting at the selection, resolved through the styles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Format {
    /// The character formatting.
    pub character: CharFormat,
    /// The paragraph formatting.
    pub paragraph: ParaFormat,
    /// The paragraph style.
    pub style: Option<StyleId>,
}

/// Edits the document of a window.
#[derive(Clone)]
pub struct TextEditor {
    view: DocumentView,
    im: gtk::IMMulticontext,
    state: Rc<RefCell<State>>,
}

/// A text editor that its own signal handlers hold without keeping the
/// window alive.
#[derive(Clone)]
struct WeakEditor {
    view: WeakView,
    im: glib::WeakRef<gtk::IMMulticontext>,
    state: std::rc::Weak<RefCell<State>>,
}

impl WeakEditor {
    fn upgrade(&self) -> Option<TextEditor> {
        Some(TextEditor {
            view: self.view.upgrade()?,
            im: self.im.upgrade()?,
            state: self.state.upgrade()?,
        })
    }
}

impl TextEditor {
    fn downgrade(&self) -> WeakEditor {
        WeakEditor {
            view: self.view.downgrade(),
            im: self.im.downgrade(),
            state: Rc::downgrade(&self.state),
        }
    }

    /// Starts editing a document shown in a view.
    pub fn new(view: &DocumentView, document: Document) -> Self {
        let layout = crate::lay_out(&document);
        let im = gtk::IMMulticontext::new();
        let this = Self {
            view: view.clone(),
            im,
            state: Rc::new(RefCell::new(State {
                editor: Editor::new(document),
                layout,
                selection: Selection::default(),
                goal: None,
                undo: Vec::new(),
                redo: Vec::new(),
                next_action: 0,
                saved: None,
                on_change: Vec::new(),
                unit: Unit::Character,
                drag_from: None,
                typing: false,
                pending: None,
                on_select: Vec::new(),
                invisibles: false,
            })),
        };
        this.show_pages();
        this.connect_keyboard();
        this.connect_pointer();
        this.connect_input_method();
        this.start_blinking();
        this.view.area().grab_focus();
        this
    }

    /// The document as it is now.
    pub fn document(&self) -> Document {
        self.state.borrow().editor.snapshot()
    }

    /// The pages as they are now.
    pub fn pages(&self) -> Vec<DisplayList> {
        DisplayList::pages(&self.state.borrow().layout)
    }

    /// The number of words of the document.
    pub fn word_count(&self) -> usize {
        self.state.borrow().editor.document().statistics().words
    }

    /// Whether the document has changed since it was saved or opened.
    pub fn is_modified(&self) -> bool {
        let state = self.state.borrow();
        state.undo.last().map(|a| a.2) != state.saved
    }

    /// Notes that the document as it is now was saved. The next letter
    /// typed starts a new action, so that Undo goes back to what was saved.
    pub fn mark_saved(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.saved = state.undo.last().map(|a| a.2);
            state.typing = false;
        }
        self.changed();
    }

    /// Whether Undo has something to take back.
    pub fn can_undo(&self) -> bool {
        self.state.borrow().editor.undo_name().is_some()
    }

    /// Whether Redo has something to bring back.
    pub fn can_redo(&self) -> bool {
        self.state.borrow().editor.redo_name().is_some()
    }

    /// Calls `f` after every change of the document.
    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        self.state.borrow_mut().on_change.push(Rc::new(f));
    }

    /// The selection.
    pub fn selection(&self) -> Selection {
        self.state.borrow().selection
    }

    /// Selects the whole document.
    pub fn select_all(&self) {
        let document = self.state.borrow().editor.snapshot();
        self.select(Selection {
            anchor: Spot::default(),
            focus: text_nav::document_edge(&document, true),
        });
    }

    /// The selected text, with paragraphs on lines of their own.
    pub fn selected_text(&self) -> String {
        let state = self.state.borrow();
        let (start, end) = state.selection.ordered();
        let body = state.editor.document().body();
        let mut text = String::new();
        for index in start.paragraph..=end.paragraph {
            let Some(paragraph) = body.get(index) else {
                break;
            };
            let all = paragraph.text();
            let from = if index == start.paragraph {
                start.offset
            } else {
                0
            };
            let to = if index == end.paragraph {
                end.offset
            } else {
                all.len()
            };
            if index > start.paragraph {
                text.push('\n');
            }
            text.push_str(all.get(from..to).unwrap_or_default());
        }
        text
    }

    /// Copies the selection to the clipboard.
    pub fn copy(&self) {
        let text = self.selected_text();
        if !text.is_empty() {
            self.view.area().clipboard().set_text(&text);
        }
    }

    /// Copies the selection and deletes it.
    pub fn cut(&self) {
        if self.selection().is_empty() {
            return;
        }
        self.copy();
        self.replace_selection("Cut", "");
    }

    /// Inserts the text of the clipboard over the selection.
    pub fn paste(&self) {
        let weak = self.downgrade();
        self.view
            .area()
            .clipboard()
            .read_text_async(gtk::gio::Cancellable::NONE, move |result| {
                if let (Ok(Some(text)), Some(this)) = (result, weak.upgrade()) {
                    this.replace_selection("Paste", &text);
                }
            });
    }

    /// Undoes the last action.
    pub fn undo(&self) {
        self.state.borrow_mut().typing = false;
        let restored = {
            let mut state = self.state.borrow_mut();
            match state.editor.undo() {
                Ok(true) => {
                    let pair = state.undo.pop();
                    if let Some(pair) = pair {
                        state.redo.push(pair);
                    }
                    pair.map(|p| p.0)
                }
                _ => None,
            }
        };
        if let Some(selection) = restored {
            self.after_change(selection);
        }
    }

    /// Redoes the last undone action.
    pub fn redo(&self) {
        self.state.borrow_mut().typing = false;
        let restored = {
            let mut state = self.state.borrow_mut();
            match state.editor.redo() {
                Ok(true) => {
                    let pair = state.redo.pop();
                    if let Some(pair) = pair {
                        state.undo.push(pair);
                    }
                    pair.map(|p| p.1)
                }
                _ => None,
            }
        };
        if let Some(selection) = restored {
            self.after_change(selection);
        }
    }

    /// Replaces the selection with text, as one action of the history.
    pub fn replace_selection(&self, name: &str, text: &str) {
        let before = self.selection();
        let (start, end) = before.ordered();
        let typing = name == "Typing" && start == end;
        let result = {
            let mut state = self.state.borrow_mut();
            let continuing = typing && state.typing;
            let pending = if start == end {
                state.pending.take()
            } else {
                None
            };
            let edits = |edit: &mut desk_model::Edit| {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                let at = if start == end {
                    from
                } else {
                    edit.delete(from, to)?
                };
                let after = edit.insert_text(at, text)?;
                if let Some(pending) = &pending {
                    edit.set_character_attrs(at, after, pending)?;
                }
                Ok(Spot::of(edit.document(), after))
            };
            if continuing {
                state.editor.edit_continuing(name, edits)
            } else {
                state.editor.edit(name, edits).map(|spot| (spot, false))
            }
        };
        match result {
            // The letter joins the typing before it: Undo goes back to
            // where that started.
            Ok((Some(spot), true)) => {
                let after = Selection::cursor(spot);
                if let Some(last) = self.state.borrow_mut().undo.last_mut() {
                    last.1 = after;
                }
                self.after_change(after);
            }
            Ok((spot, false)) => self.finish_edit(before, Ok(spot)),
            Ok((None, true)) => {}
            Err(error) => self.finish_edit(before, Err(error)),
        }
        self.state.borrow_mut().typing = typing;
    }

    /// Deletes the selection, or what a key deletes next to the cursor.
    fn delete(&self, forward: bool, word: bool) {
        let before = self.selection();
        let (start, end) = if before.is_empty() {
            let document = self.state.borrow().editor.snapshot();
            let other = match (forward, word) {
                (true, false) => text_nav::move_char(&document, before.focus, true),
                (false, false) => text_nav::move_char(&document, before.focus, false),
                (true, true) => text_nav::move_word(&document, before.focus, true),
                (false, true) => text_nav::move_word(&document, before.focus, false),
            };
            if other == before.focus {
                return;
            }
            Selection {
                anchor: before.focus,
                focus: other,
            }
            .ordered()
        } else {
            before.ordered()
        };
        let result = {
            let mut state = self.state.borrow_mut();
            state.editor.edit("Delete", |edit| {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                let at = edit.delete(from, to)?;
                Ok(Spot::of(edit.document(), at))
            })
        };
        self.finish_edit(before, result);
    }

    /// Splits the paragraph at the cursor, as Return does.
    fn split(&self) {
        let before = self.selection();
        let (start, end) = before.ordered();
        let result = {
            let mut state = self.state.borrow_mut();
            state.editor.edit("Typing", |edit| {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                let at = if start == end {
                    from
                } else {
                    edit.delete(from, to)?
                };
                let after = edit.split_paragraph(at)?;
                Ok(Spot::of(edit.document(), after))
            })
        };
        self.finish_edit(before, result);
    }

    fn finish_edit(&self, before: Selection, result: Result<Option<Spot>, ModelError>) {
        self.state.borrow_mut().typing = false;
        match result {
            Ok(Some(spot)) => {
                let after = Selection::cursor(spot);
                {
                    let mut state = self.state.borrow_mut();
                    let action = state.next_action;
                    state.next_action += 1;
                    state.undo.push((before, after, action));
                    state.redo.clear();
                }
                self.after_change(after);
            }
            Ok(None) => {}
            Err(error) => glib::g_warning!("desk", "cannot edit: {error}"),
        }
    }

    /// Lays out the document again and shows a selection.
    fn after_change(&self, selection: Selection) {
        {
            let mut state = self.state.borrow_mut();
            state.layout = crate::lay_out(state.editor.document());
        }
        self.show_pages();
        self.state.borrow_mut().goal = None;
        self.select(selection);
        self.changed();
    }

    fn changed(&self) {
        let callbacks = self.state.borrow().on_change.clone();
        for callback in callbacks {
            callback();
        }
    }

    fn show_pages(&self) {
        let (pages, invisibles) = {
            let state = self.state.borrow();
            let invisibles = if state.invisibles {
                text_nav::invisibles(state.editor.document(), &state.layout)
            } else {
                Vec::new()
            };
            (DisplayList::pages(&state.layout), invisibles)
        };
        self.view.set_pages(pages);
        self.view.set_invisibles(invisibles);
    }

    /// Gives the keyboard back to the document.
    pub fn focus(&self) {
        self.view.area().grab_focus();
    }

    /// Shows or hides the invisible characters (INT-10).
    pub fn set_show_invisibles(&self, show: bool) {
        self.state.borrow_mut().invisibles = show;
        self.show_pages();
    }

    /// Calls `f` whenever the selection moves, or the formatting that text
    /// typed at the cursor takes changes.
    pub fn connect_selection_changed(&self, f: impl Fn() + 'static) {
        self.state.borrow_mut().on_select.push(Rc::new(f));
    }

    fn selection_changed(&self) {
        let callbacks = self.state.borrow().on_select.clone();
        for callback in callbacks {
            callback();
        }
    }

    /// The formatting at the selection: that of its first character, or
    /// that which text typed at the cursor takes.
    pub fn format(&self) -> Format {
        let state = self.state.borrow();
        let document = state.editor.document();
        let (start, end) = state.selection.ordered();
        let styles = document.styles();
        let Some(paragraph) = document.body().get(start.paragraph) else {
            return Format::default();
        };
        let at = if start == end || start.offset >= paragraph.len() {
            start.offset
        } else {
            // The character after the start of the selection.
            start.offset + 1
        };
        let (style, mut attrs) = paragraph.format_at(at);
        if start == end
            && let Some(pending) = &state.pending
        {
            attrs.merge(pending);
        }
        Format {
            character: styles.resolve_character(paragraph.style(), style, &attrs),
            paragraph: styles.resolve_paragraph(paragraph.style(), paragraph.attrs()),
            style: Some(paragraph.style()),
        }
    }

    /// The paragraph styles of the document, by name.
    pub fn paragraph_styles(&self) -> Vec<(StyleId, String)> {
        let state = self.state.borrow();
        let mut styles: Vec<_> = state
            .editor
            .document()
            .styles()
            .iter()
            .filter(|s| s.family() == StyleFamily::Paragraph)
            .map(|s| (s.id(), s.name().to_owned()))
            .collect();
        styles.sort_by_key(|s| s.1.to_lowercase());
        styles
    }

    /// Sets character formatting on the selection, as one action of the
    /// history. With nothing selected, the next text typed takes it.
    pub fn set_character_attrs(&self, name: &str, attrs: &CharAttrs) {
        let selection = self.selection();
        if selection.is_empty() {
            {
                let mut state = self.state.borrow_mut();
                let pending = state.pending.get_or_insert_with(CharAttrs::default);
                pending.merge(attrs);
                state.typing = false;
            }
            self.selection_changed();
            return;
        }
        let (start, end) = selection.ordered();
        self.format_edit(name, move |edit| {
            let document = edit.document();
            let from = at_spot(document, start)?;
            let to = at_spot(document, end)?;
            edit.set_character_attrs(from, to, attrs)
        });
    }

    /// Sets paragraph formatting on the paragraphs of the selection.
    pub fn set_paragraph_attrs(&self, name: &str, attrs: &ParaAttrs) {
        let (start, end) = self.selection().ordered();
        self.format_edit(name, move |edit| {
            let document = edit.document();
            let from = at_spot(document, start)?;
            let to = at_spot(document, end)?;
            edit.set_paragraph_attrs(from, to, attrs)
        });
    }

    /// Gives the paragraphs of the selection a paragraph style.
    pub fn set_paragraph_style(&self, style: StyleId) {
        let (start, end) = self.selection().ordered();
        self.format_edit("Paragraph Style", move |edit| {
            let document = edit.document();
            let from = at_spot(document, start)?;
            let to = at_spot(document, end)?;
            edit.set_paragraph_style(from, to, style)
        });
    }

    /// Changes formatting as one action of the history, keeping the
    /// selection.
    fn format_edit(
        &self,
        name: &str,
        change: impl FnOnce(&mut desk_model::Edit) -> Result<(), ModelError>,
    ) {
        let selection = self.selection();
        let depth = self.state.borrow().editor.undo_depth();
        let result = self.state.borrow_mut().editor.edit(name, change);
        self.state.borrow_mut().typing = false;
        match result {
            // Formatting that was there already changes nothing.
            Ok(()) if self.state.borrow().editor.undo_depth() == depth => {}
            Ok(()) => {
                {
                    let mut state = self.state.borrow_mut();
                    let action = state.next_action;
                    state.next_action += 1;
                    state.undo.push((selection, selection, action));
                    state.redo.clear();
                }
                self.after_change(selection);
            }
            Err(error) => glib::g_warning!("desk", "cannot format: {error}"),
        }
    }

    /// Sets the selection and shows it.
    fn select(&self, selection: Selection) {
        let (rects, caret) = {
            let mut state = self.state.borrow_mut();
            let document = state.editor.document();
            let selection = Selection {
                anchor: text_nav::clamp(document, selection.anchor),
                focus: text_nav::clamp(document, selection.focus),
            };
            if state.selection != selection {
                state.pending = None;
            }
            state.selection = selection;
            (
                text_nav::selection_rects(&state.layout, &selection),
                text_nav::caret(&state.layout, selection.focus),
            )
        };
        self.view.set_marks(rects, caret);
        self.selection_changed();
        if let Some(caret) = caret {
            self.view.scroll_to(&caret);
            if let Some((x, y)) = self.view.to_area(caret.page, caret.x, caret.top) {
                let (_, bottom) = self
                    .view
                    .to_area(caret.page, caret.x, caret.bottom)
                    .unwrap_or((x, y));
                self.im.set_cursor_location(&gdk::Rectangle::new(
                    x as i32,
                    y as i32,
                    1,
                    (bottom - y).max(1.0) as i32,
                ));
            }
        }
    }

    /// Moves the cursor, or the end of the selection with Shift.
    fn move_to(&self, spot: Spot, extend: bool, keep_goal: bool) {
        self.state.borrow_mut().typing = false;
        let goal = self.state.borrow().goal;
        let anchor = if extend {
            self.selection().anchor
        } else {
            spot
        };
        self.select(Selection {
            anchor,
            focus: spot,
        });
        if keep_goal {
            self.state.borrow_mut().goal = goal;
        } else {
            self.state.borrow_mut().goal = None;
        }
    }

    fn handle_key(&self, key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
        let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
        let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        let (document, layout, selection) = {
            let state = self.state.borrow();
            (
                state.editor.snapshot(),
                state.layout.clone(),
                state.selection,
            )
        };
        let focus = selection.focus;
        let (start, end) = selection.ordered();
        // Without Shift, an arrow first collapses a selection to its side.
        let collapse = |forward: bool| {
            (!shift && !selection.is_empty()).then_some(if forward { end } else { start })
        };
        match key {
            gdk::Key::Left | gdk::Key::KP_Left => {
                let spot = collapse(false).unwrap_or_else(|| {
                    if control {
                        text_nav::move_word(&document, focus, false)
                    } else {
                        text_nav::move_char(&document, focus, false)
                    }
                });
                self.move_to(spot, shift, false);
            }
            gdk::Key::Right | gdk::Key::KP_Right => {
                let spot = collapse(true).unwrap_or_else(|| {
                    if control {
                        text_nav::move_word(&document, focus, true)
                    } else {
                        text_nav::move_char(&document, focus, true)
                    }
                });
                self.move_to(spot, shift, false);
            }
            gdk::Key::Up | gdk::Key::KP_Up | gdk::Key::Down | gdk::Key::KP_Down => {
                let down = matches!(key, gdk::Key::Down | gdk::Key::KP_Down);
                if control {
                    self.move_to(
                        text_nav::paragraph_edge(&document, focus, down),
                        shift,
                        false,
                    );
                } else {
                    let goal = self.goal_for(&layout, focus);
                    let spot = text_nav::move_line(&layout, focus, goal, down, 1);
                    self.move_to(spot, shift, true);
                    self.state.borrow_mut().goal = Some(goal);
                }
            }
            gdk::Key::Page_Up
            | gdk::Key::Page_Down
            | gdk::Key::KP_Page_Up
            | gdk::Key::KP_Page_Down => {
                let down = matches!(key, gdk::Key::Page_Down | gdk::Key::KP_Page_Down);
                let goal = self.goal_for(&layout, focus);
                // About a screenful of lines of the size of this one.
                let lines = text_nav::caret(&layout, focus).map_or(20, |c| {
                    let height = (c.bottom - c.top).max(1.0)
                        * self.view.zoom()
                        * crate::geometry::PIXELS_PER_POINT;
                    ((self.view.widget().height() as f64 / height) as usize).max(1)
                });
                let spot = text_nav::move_line(&layout, focus, goal, down, lines);
                self.move_to(spot, shift, true);
                self.state.borrow_mut().goal = Some(goal);
            }
            gdk::Key::Home | gdk::Key::KP_Home => {
                let spot = if control {
                    text_nav::document_edge(&document, false)
                } else {
                    text_nav::line_edge(&layout, focus, false)
                };
                self.move_to(spot, shift, false);
            }
            gdk::Key::End | gdk::Key::KP_End => {
                let spot = if control {
                    text_nav::document_edge(&document, true)
                } else {
                    text_nav::line_edge(&layout, focus, true)
                };
                self.move_to(spot, shift, false);
            }
            gdk::Key::BackSpace => self.delete(false, control),
            gdk::Key::Delete | gdk::Key::KP_Delete => {
                if shift && !selection.is_empty() {
                    self.cut();
                } else {
                    self.delete(true, control);
                }
            }
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter => {
                if shift {
                    // A line break inside the paragraph.
                    self.replace_selection("Typing", &LINE_BREAK.to_string());
                } else {
                    self.split();
                }
            }
            gdk::Key::Tab | gdk::Key::ISO_Left_Tab if !control => {
                self.replace_selection("Typing", "\t");
            }
            gdk::Key::Insert if shift => self.paste(),
            gdk::Key::Insert if control => self.copy(),
            _ => return false,
        }
        true
    }

    fn goal_for(&self, layout: &DocumentLayout, focus: Spot) -> f64 {
        self.state
            .borrow()
            .goal
            .or_else(|| text_nav::caret(layout, focus).map(|c| c.x))
            .unwrap_or(0.0)
    }

    fn connect_keyboard(&self) {
        let keys = gtk::EventControllerKey::new();
        keys.set_im_context(Some(&self.im));
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if this.handle_key(key, modifiers) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.view.area().add_controller(keys);

        let focus = gtk::EventControllerFocus::new();
        let weak = self.downgrade();
        focus.connect_enter(move |_| {
            if let Some(this) = weak.upgrade() {
                this.im.focus_in();
                this.view.area().queue_draw();
            }
        });
        let weak = self.downgrade();
        focus.connect_leave(move |_| {
            if let Some(this) = weak.upgrade() {
                this.im.focus_out();
                this.view.area().queue_draw();
            }
        });
        self.view.area().add_controller(focus);
    }

    fn connect_input_method(&self) {
        self.im.set_client_widget(Some(self.view.area()));
        let weak = self.downgrade();
        self.im.connect_commit(move |_, text| {
            let Some(this) = weak.upgrade() else {
                return Default::default();
            };
            this.view.set_preedit("");
            this.replace_selection("Typing", text);
        });
        let weak = self.downgrade();
        self.im.connect_preedit_changed(move |im| {
            let Some(this) = weak.upgrade() else {
                return Default::default();
            };
            let (text, _, _) = im.preedit_string();
            this.view.set_preedit(&text);
        });
        let weak = self.downgrade();
        self.im.connect_preedit_end(move |_| {
            if let Some(this) = weak.upgrade() {
                this.view.set_preedit("");
            }
        });
    }

    fn spot_at(&self, x: f64, y: f64) -> Option<Spot> {
        let (page, px, py) = self.view.page_at(x, y)?;
        text_nav::hit(&self.state.borrow().layout, page, px, py)
    }

    /// The selection that a click selects: a cursor, a word or a paragraph.
    fn unit_at(&self, spot: Spot, unit: Unit) -> Selection {
        let document = self.state.borrow().editor.snapshot();
        match unit {
            Unit::Character => Selection::cursor(spot),
            Unit::Word => text_nav::word_at(&document, spot),
            Unit::Paragraph => text_nav::paragraph_at(&document, spot),
        }
    }

    /// Extends a selection that started as `from` to cover `spot`, by
    /// whole units.
    fn extend(&self, from: Selection, spot: Spot) {
        let unit = self.state.borrow().unit;
        let (from_start, from_end) = from.ordered();
        let here = self.unit_at(spot, unit);
        let (here_start, here_end) = here.ordered();
        let selection = if spot < from_start {
            Selection {
                anchor: from_end,
                focus: here_start,
            }
        } else {
            Selection {
                anchor: from_start,
                focus: here_end.max(from_end),
            }
        };
        self.select(selection);
    }

    fn connect_pointer(&self) {
        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        let weak = self.downgrade();
        click.connect_pressed(move |gesture, presses, x, y| {
            let Some(this) = weak.upgrade() else {
                return Default::default();
            };
            this.view.area().grab_focus();
            let Some(spot) = this.spot_at(x, y) else {
                return;
            };
            let shift = gesture
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK);
            let unit = match presses {
                1 => Unit::Character,
                2 => Unit::Word,
                _ => Unit::Paragraph,
            };
            this.state.borrow_mut().unit = unit;
            this.state.borrow_mut().typing = false;
            if shift && presses == 1 {
                // Shift and a click extend the selection (TXT-03).
                let anchor = this.selection().anchor;
                this.select(Selection {
                    anchor,
                    focus: spot,
                });
                this.state.borrow_mut().drag_from = Some(Selection::cursor(anchor));
            } else {
                let selection = this.unit_at(spot, unit);
                this.select(selection);
                this.state.borrow_mut().drag_from = Some(selection);
            }
        });
        self.view.area().add_controller(click);

        let drag = gtk::GestureDrag::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        let weak = self.downgrade();
        drag.connect_drag_update(move |gesture, dx, dy| {
            let Some(this) = weak.upgrade() else {
                return Default::default();
            };
            let Some((x, y)) = gesture.start_point() else {
                return;
            };
            let Some(from) = this.state.borrow().drag_from else {
                return;
            };
            if let Some(spot) = this.spot_at(x + dx, y + dy) {
                this.extend(from, spot);
            }
        });
        let weak = self.downgrade();
        drag.connect_drag_end(move |_, _, _| {
            let Some(this) = weak.upgrade() else {
                return Default::default();
            };
            this.state.borrow_mut().drag_from = None;
        });
        self.view.area().add_controller(drag);
    }

    /// Blinks the cursor, as GTK text views do, while the window lives.
    fn start_blinking(&self) {
        let blink = self.view.area().settings().is_gtk_cursor_blink();
        if !blink {
            return;
        }
        let view = self.view.downgrade();
        let mut on = true;
        glib::timeout_add_local(Duration::from_millis(600), move || {
            let Some(view) = view.upgrade() else {
                return glib::ControlFlow::Break;
            };
            on = !on;
            view.set_caret_on(on);
            glib::ControlFlow::Continue
        });
    }
}
