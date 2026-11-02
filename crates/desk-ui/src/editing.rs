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
    CharAttrs, CharFormat, Document, Editor, LINE_BREAK, Length, ModelError, ParaAttrs, ParaFormat,
    StyleFamily, StyleId,
};
use desk_render::DisplayList;
use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::access::AccessText;
use crate::text_nav::{self, Selection, Spot};
use crate::view::{DocumentView, WeakView};

/// Whether a paragraph is an item of a list: its list style has a label at
/// its level.
fn in_list(document: &Document, paragraph: desk_model::NodeId) -> bool {
    let Ok(paragraph) = document.paragraph(paragraph) else {
        return false;
    };
    let styles = document.styles();
    let format = styles.resolve_paragraph(paragraph.style(), paragraph.attrs());
    format
        .list_style
        .and_then(|id| styles.list(id))
        .is_some_and(|list| list.level(format.list_level).label.is_some())
}

/// Makes the corrections of the word that ends at a position, as typing
/// a space or a punctuation mark after it does (TXT-07), and gives where
/// the position is after them.
fn autocorrect(
    edit: &mut desk_model::Edit,
    at: desk_model::Position,
    options: &desk_model::AutoCorrect,
) -> Result<desk_model::Position, ModelError> {
    let text = edit.document().paragraph(at.paragraph)?.text();
    let Some(correction) = options.correct(text.get(..at.offset).unwrap_or_default()) else {
        return Ok(at);
    };
    let found = desk_model::Found {
        paragraph: at.paragraph,
        index: 0,
        range: correction.range.clone(),
    };
    let end = edit.replace_found(&found, &correction.text)?;
    if let Some(raised) = &correction.superscript {
        let start = correction.range.start;
        edit.set_character_attrs(
            desk_model::Position::new(at.paragraph, start + raised.start),
            desk_model::Position::new(at.paragraph, start + raised.end),
            &CharAttrs {
                baseline: Some(desk_model::Baseline::Superscript),
                ..CharAttrs::default()
            },
        )?;
    }
    Ok(desk_model::Position::new(
        at.paragraph,
        at.offset + end.offset - correction.range.end,
    ))
}

/// Corrects the misspelled word that ends at `at` when the dictionary has
/// one suggestion for it (TXT-07), and returns where `at` moved.
fn accept_single_suggestion(
    edit: &mut desk_model::Edit,
    at: desk_model::Position,
) -> Result<desk_model::Position, ModelError> {
    let document = edit.document();
    let paragraph = document.paragraph(at.paragraph)?;
    let text = paragraph.text();
    let mistakes = crate::spelling::mistakes(document.styles(), paragraph, true, false);
    let Some(mistake) = mistakes.iter().find(|m| m.range.end == at.offset) else {
        return Ok(at);
    };
    let suggestions = mistake.suggestions(&text);
    let [suggestion] = suggestions.as_slice() else {
        return Ok(at);
    };
    let found = desk_model::Found {
        paragraph: at.paragraph,
        index: 0,
        range: mistake.range.clone(),
    };
    let end = edit.replace_found(&found, suggestion)?;
    Ok(desk_model::Position::new(at.paragraph, end.offset))
}

thread_local! {
    /// The text last copied in Desk, with its styles, while the clipboard
    /// still holds that text.
    static COPIED: RefCell<Option<(String, desk_model::Fragment)>> = const { RefCell::new(None) };
}

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
    /// The text as screen readers read it.
    access: AccessText,
    on_change: Vec<Rc<dyn Fn()>>,
    /// The text of a word or paragraph that a double or triple click
    /// selected, which a drag then extends by whole words or paragraphs.
    unit: Unit,
    drag_from: Option<Selection>,
    /// Whether the last action was typing at the cursor, which the next
    /// letter typed continues as one step of the history.
    typing: bool,
    /// More selected ranges, which Select All Uses adds (STY-09). Changes
    /// of formatting apply to them too; moving the cursor drops them.
    others: Vec<Selection>,
    /// Whether a Ctrl-click or the drag after it is adding a range to the
    /// selection, which keeps the others (TXT-03).
    adding: bool,
    /// The cursor at the end of a word being typed, which the spelling
    /// checker leaves unmarked until the cursor leaves it.
    squiggle_skip: Option<Spot>,
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
    /// The list style of the paragraph, when it has one.
    pub list: Option<desk_model::ListStyle>,
    /// The settings of the text of the document.
    pub text_settings: desk_model::TextSettings,
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
                access: AccessText::default(),
                others: Vec::new(),
                adding: false,
                squiggle_skip: None,
            })),
        };
        this.show_pages();
        this.update_squiggles();
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

    /// Copies the selection to the clipboard, as text for other apps and
    /// with its styles for Desk (TXT-04).
    pub fn copy(&self) {
        let text = self.selected_text();
        if text.is_empty() {
            return;
        }
        let fragment = {
            let state = self.state.borrow();
            let document = state.editor.document();
            let (start, end) = state.selection.ordered();
            at_spot(document, start)
                .and_then(|a| at_spot(document, end).map(|b| (a, b)))
                .and_then(|(a, b)| document.copy_fragment(a, b))
                .ok()
        };
        COPIED.with(|c| *c.borrow_mut() = fragment.map(|f| (text.clone(), f)));
        self.view.area().clipboard().set_text(&text);
    }

    /// Copies the selection and deletes it.
    pub fn cut(&self) {
        if self.selection().is_empty() {
            return;
        }
        self.copy();
        self.replace_selection("Cut", "");
    }

    /// Inserts the clipboard over the selection: text copied in Desk
    /// keeps its styles, and text from other apps takes the format at the
    /// cursor (TXT-04).
    pub fn paste(&self) {
        self.paste_keeping_style(true);
    }

    /// Inserts the text of the clipboard over the selection with the
    /// format at the cursor: Paste and Match Style (TXT-04).
    pub fn paste_matching_style(&self) {
        self.paste_keeping_style(false);
    }

    fn paste_keeping_style(&self, keep: bool) {
        let weak = self.downgrade();
        self.view
            .area()
            .clipboard()
            .read_text_async(gtk::gio::Cancellable::NONE, move |result| {
                let (Ok(Some(text)), Some(this)) = (result, weak.upgrade()) else {
                    return;
                };
                let fragment = keep
                    .then(|| {
                        COPIED.with(|c| {
                            c.borrow()
                                .as_ref()
                                .filter(|(copied, _)| copied.as_str() == text.as_str())
                                .map(|(_, f)| f.clone())
                        })
                    })
                    .flatten();
                match fragment {
                    Some(fragment) => this.paste_fragment(&fragment),
                    None => this.replace_selection("Paste", &text),
                }
            });
    }

    /// Replaces a font family with another one in the whole document, as
    /// one action of the history (INT-22).
    pub fn replace_font(&self, from: &str, to: &str) {
        let before = self.selection();
        let result = self
            .state
            .borrow_mut()
            .editor
            .edit("Replace Font", |edit| edit.replace_font_family(from, to))
            .map(|()| Some(before.focus));
        self.finish_edit(before, result);
    }

    /// Replaces the selection with text copied with its styles.
    fn paste_fragment(&self, fragment: &desk_model::Fragment) {
        let before = self.selection();
        let (start, end) = before.ordered();
        let result = self.state.borrow_mut().editor.edit("Paste", |edit| {
            let from = at_spot(edit.document(), start)?;
            let to = at_spot(edit.document(), end)?;
            let at = if start == end {
                from
            } else {
                edit.delete(from, to)?
            };
            let after = edit.paste_fragment(at, fragment)?;
            Ok(Spot::of(edit.document(), after))
        });
        self.finish_edit(before, result);
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
            let preferences = crate::preferences::get();
            let edits = |edit: &mut desk_model::Edit| {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                let at = if start == end {
                    from
                } else {
                    edit.delete(from, to)?
                };
                let mut chars = text.chars();
                let typed = match (chars.next(), chars.next()) {
                    (Some(c), None) if name == "Typing" => Some(c),
                    _ => None,
                };
                let before = edit.document().paragraph(at.paragraph)?.text();
                let before = before.get(..at.offset).unwrap_or_default();
                // Smart quotes (TXT-06).
                let quoted;
                let text = match typed {
                    Some(c @ ('"' | '\'')) if preferences.smart_quotes => {
                        quoted = desk_model::smart_quote(before.chars().next_back(), c).to_string();
                        quoted.as_str()
                    }
                    _ => text,
                };
                let mut after = edit.insert_text(at, text)?;
                if let Some(pending) = &pending {
                    edit.set_character_attrs(at, after, pending)?;
                }
                // The corrections of the word that the character ends
                // (TXT-07).
                if let Some(c) = typed
                    && desk_model::ends_word(c)
                {
                    let mut corrected = autocorrect(edit, at, &preferences.autocorrect)?;
                    if preferences.accept_single_suggestion {
                        corrected = accept_single_suggestion(edit, corrected)?;
                    }
                    after.offset = after.offset + corrected.offset - at.offset;
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

    /// Splits the paragraph at the cursor, as Return does. A paragraph
    /// that starts with a marker such as "1. " becomes an item of a list
    /// first, when automatic lists are on (LST-01), and Return in an
    /// empty item ends the list instead (LST-02).
    fn split(&self) {
        let before = self.selection();
        let (start, end) = before.ordered();
        let preferences = crate::preferences::get();
        let automatic = preferences.automatic_lists;
        let result = {
            let mut state = self.state.borrow_mut();
            state.editor.edit("Typing", |edit| {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                if start == end && in_list(document, from.paragraph) {
                    let paragraph = document.paragraph(from.paragraph)?;
                    if paragraph.is_empty() {
                        edit.set_list_style(from, from, None)?;
                        return Ok(Spot::of(edit.document(), from));
                    }
                }
                let mut at = if start == end {
                    autocorrect(edit, from, &preferences.autocorrect)?
                } else {
                    edit.delete(from, to)?
                };
                if automatic && let Some(len) = edit.list_from_marker(at.paragraph)? {
                    at.offset = at.offset.saturating_sub(len);
                }
                let after = edit.split_paragraph(at)?;
                Ok(Spot::of(edit.document(), after))
            })
        };
        self.finish_edit(before, result);
    }

    /// Moves the selected items of lists `by` levels, with the items
    /// under them when `children` is true (LST-02). Returns false when
    /// the selection is not in a list, for Tab to insert a tab.
    pub fn change_list_level(&self, by: i32, children: bool) -> bool {
        let (start, _) = self.selection().ordered();
        let listed = {
            let state = self.state.borrow();
            let document = state.editor.document();
            at_spot(document, start).is_ok_and(|at| in_list(document, at.paragraph))
        };
        if listed {
            self.format_ranges("List Level", |edit, from, to| {
                edit.change_list_level(from, to, by, children)
            });
        }
        listed
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
        self.update_squiggles();
        self.changed();
    }

    /// Marks the misspelled words and the mistakes of the proofreader,
    /// as the preferences ask (TXT-10, TXT-11).
    pub fn update_squiggles(&self) {
        let preferences = crate::preferences::get();
        let squiggles = {
            let mut state = self.state.borrow_mut();
            let cursor =
                (state.typing && state.selection.is_empty()).then_some(state.selection.focus);
            let mut skipped = None;
            let mut squiggles = Vec::new();
            let document = state.editor.document();
            for (index, paragraph) in document.body().iter().enumerate() {
                for mistake in crate::spelling::mistakes(
                    document.styles(),
                    paragraph,
                    preferences.check_spelling,
                    preferences.proofread,
                ) {
                    if let Some(cursor) = cursor
                        && cursor.paragraph == index
                        && mistake.range.end == cursor.offset
                        && matches!(mistake.kind, crate::spelling::MistakeKind::Spelling { .. })
                    {
                        skipped = Some(cursor);
                        continue;
                    }
                    let selection = Selection {
                        anchor: Spot::new(index, mistake.range.start),
                        focus: Spot::new(index, mistake.range.end),
                    };
                    let proofreading =
                        matches!(mistake.kind, crate::spelling::MistakeKind::Proofreading(_));
                    for span in text_nav::selection_rects(&state.layout, &selection) {
                        squiggles.push(crate::view::Squiggle { span, proofreading });
                    }
                }
            }
            state.squiggle_skip = skipped;
            squiggles
        };
        self.view.set_squiggles(squiggles);
    }

    /// The mistakes of a paragraph, by its index, as the preferences ask.
    fn mistakes_of(
        &self,
        index: usize,
        spelling: bool,
        proofreading: bool,
    ) -> Vec<crate::spelling::Mistake> {
        let state = self.state.borrow();
        let document = state.editor.document();
        document
            .body()
            .get(index)
            .map(|p| crate::spelling::mistakes(document.styles(), p, spelling, proofreading))
            .unwrap_or_default()
    }

    /// The text of a paragraph, by its index.
    fn paragraph_text(&self, index: usize) -> String {
        let state = self.state.borrow();
        state
            .editor
            .document()
            .body()
            .get(index)
            .map(|p| p.text())
            .unwrap_or_default()
    }

    /// The mistake under a point of the document area, which it selects,
    /// with the text of its paragraph, for the context menu.
    pub fn mistake_at(&self, x: f64, y: f64) -> Option<(crate::spelling::Mistake, String)> {
        let preferences = crate::preferences::get();
        let spot = self.spot_at(x, y)?;
        let mistake = self
            .mistakes_of(
                spot.paragraph,
                preferences.check_spelling,
                preferences.proofread,
            )
            .into_iter()
            .find(|m| m.range.start <= spot.offset && spot.offset <= m.range.end)?;
        self.select_mistake(spot.paragraph, &mistake);
        Some((mistake, self.paragraph_text(spot.paragraph)))
    }

    fn select_mistake(&self, index: usize, mistake: &crate::spelling::Mistake) {
        self.state.borrow_mut().typing = false;
        self.select(Selection {
            anchor: Spot::new(index, mistake.range.start),
            focus: Spot::new(index, mistake.range.end),
        });
    }

    /// The mistake that the selection is, if it is one.
    pub fn selected_mistake(
        &self,
        spelling: bool,
        proofreading: bool,
    ) -> Option<(crate::spelling::Mistake, String)> {
        let (start, end) = self.selection().ordered();
        if start.paragraph != end.paragraph {
            return None;
        }
        let mistake = self
            .mistakes_of(start.paragraph, spelling, proofreading)
            .into_iter()
            .find(|m| m.range == (start.offset..end.offset))?;
        Some((mistake, self.paragraph_text(start.paragraph)))
    }

    /// Selects the next mistake after the selection, going round the
    /// document (Check Spelling), and gives it with the text of its
    /// paragraph.
    pub fn next_mistake(
        &self,
        spelling: bool,
        proofreading: bool,
    ) -> Option<(crate::spelling::Mistake, String)> {
        let (_, end) = self.selection().ordered();
        let count = self.state.borrow().editor.document().body().len();
        for step in 0..=count {
            let index = (end.paragraph + step) % count.max(1);
            let found = self
                .mistakes_of(index, spelling, proofreading)
                .into_iter()
                .find(|m| step > 0 || m.range.start >= end.offset)
                .or_else(|| {
                    // Round to the start of the first paragraph.
                    (step == count)
                        .then(|| {
                            self.mistakes_of(index, spelling, proofreading)
                                .into_iter()
                                .next()
                        })
                        .flatten()
                });
            if let Some(mistake) = found {
                self.select_mistake(index, &mistake);
                return Some((mistake, self.paragraph_text(index)));
            }
        }
        None
    }

    /// Replaces the selection with a correction, which takes the
    /// formatting of the text it replaces.
    pub fn correct(&self, with: &str) {
        let (start, end) = self.selection().ordered();
        if start.paragraph != end.paragraph {
            return;
        }
        let paragraph = {
            let state = self.state.borrow();
            state
                .editor
                .document()
                .body()
                .get(start.paragraph)
                .map(|p| p.id())
        };
        if let Some(paragraph) = paragraph {
            self.replace_found(
                &desk_model::Found {
                    paragraph,
                    index: start.paragraph,
                    range: start.offset..end.offset,
                },
                with,
            );
        }
    }

    fn changed(&self) {
        let callbacks = self.state.borrow().on_change.clone();
        for callback in callbacks {
            callback();
        }
    }

    fn show_pages(&self) {
        let (pages, invisibles, text) = {
            let state = self.state.borrow();
            let text = AccessText::new(state.editor.document(), &state.layout);
            let invisibles = if state.invisibles {
                text_nav::invisibles(state.editor.document(), &state.layout)
            } else {
                Vec::new()
            };
            (DisplayList::pages(&state.layout), invisibles, text)
        };
        self.view.set_pages(pages);
        self.view.set_invisibles(invisibles);
        self.view.access().set_text(text.clone());
        self.state.borrow_mut().access = text;
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
        let format = styles.resolve_paragraph(paragraph.style(), paragraph.attrs());
        Format {
            character: styles.resolve_character(paragraph.style(), style, &attrs),
            list: format.list_style.and_then(|id| styles.list(id)).cloned(),
            paragraph: format,
            style: Some(paragraph.style()),
            text_settings: document.text_settings().clone(),
        }
    }

    /// Changes the settings of the text of the whole document, as one
    /// action of the history.
    pub fn change_text_settings(
        &self,
        name: &str,
        change: impl FnOnce(&mut desk_model::TextSettings),
    ) {
        let mut settings = self
            .state
            .borrow()
            .editor
            .document()
            .text_settings()
            .clone();
        change(&mut settings);
        self.format_edit(name, move |edit| edit.set_text_settings(settings));
    }

    /// The word at the cursor or the start of the selection, when there is
    /// one: what Never Hyphenate takes.
    pub fn word_at_cursor(&self) -> Option<String> {
        let state = self.state.borrow();
        let document = state.editor.document();
        let (start, _) = state.selection.ordered();
        let text = document.body().get(start.paragraph)?.text();
        desk_lang::words(&text)
            .into_iter()
            .find(|r| r.start <= start.offset && start.offset <= r.end)
            .map(|r| text[r].to_owned())
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
        self.format_ranges(name, |edit, from, to| {
            edit.set_character_attrs(from, to, attrs)
        });
    }

    /// Sets paragraph formatting on the paragraphs of the selection.
    pub fn set_paragraph_attrs(&self, name: &str, attrs: &ParaAttrs) {
        self.format_ranges(name, |edit, from, to| {
            edit.set_paragraph_attrs(from, to, attrs)
        });
    }

    /// Changes the margins of the page, as one action of the history
    /// (FMT-18). With `keep_indents`, how far each edge of the text moved
    /// in, in points, the paragraphs of the selection change their indents
    /// so that they stay where they are on the page.
    pub fn set_margins(
        &self,
        name: &str,
        setup: desk_model::PageSetup,
        keep_indents: Option<(f64, f64)>,
    ) {
        let ranges = self.ranges();
        self.format_edit(name, move |edit| {
            edit.set_page_setup(setup)?;
            let Some((left, right)) = keep_indents else {
                return Ok(());
            };
            for (start, end) in ranges {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                for (id, _) in document.paragraph_ranges(from, to)? {
                    let paragraph = edit.document().paragraph(id)?;
                    let format = edit
                        .document()
                        .styles()
                        .resolve_paragraph(paragraph.style(), paragraph.attrs());
                    let indent = |value: Length, shift: f64| {
                        Length::from_pt((value.to_pt() - shift).max(0.0))
                    };
                    let at = desk_model::Position::new(id, 0);
                    edit.set_paragraph_attrs(
                        at,
                        at,
                        &ParaAttrs {
                            left_indent: Some(indent(format.left_indent, left)),
                            right_indent: Some(indent(format.right_indent, right)),
                            ..ParaAttrs::default()
                        },
                    )?;
                }
            }
            Ok(())
        });
    }

    /// Gives the paragraphs of the selection a paragraph style.
    pub fn set_paragraph_style(&self, style: StyleId) {
        self.format_ranges("Paragraph Style", |edit, from, to| {
            edit.set_paragraph_style(from, to, style)
        });
    }

    /// Gives the paragraphs of the selection a paragraph style and drops
    /// the formatting set over it, as a style shortcut with Alt does
    /// (STY-04).
    pub fn set_paragraph_style_clean(&self, style: StyleId) {
        self.format_ranges("Paragraph Style", |edit, from, to| {
            edit.set_paragraph_style_clean(from, to, style)
        });
    }

    /// Gives the selected text a character style, or none.
    pub fn set_character_style(&self, style: Option<StyleId>) {
        self.format_ranges("Character Style", |edit, from, to| {
            edit.set_character_style(from, to, style)
        });
    }

    /// Gives the selection a style of either family; with `clean`, the
    /// formatting set over it goes too.
    pub fn apply_style(&self, style: StyleId, clean: bool) {
        let family = self.styles().get(style).map(desk_model::Style::family);
        match family {
            Some(StyleFamily::Paragraph) if clean => self.set_paragraph_style_clean(style),
            Some(StyleFamily::Paragraph) => self.set_paragraph_style(style),
            Some(StyleFamily::Character) => {
                if clean {
                    self.format_ranges("Character Style", |edit, from, to| {
                        edit.set_character_style(from, to, Some(style))?;
                        edit.clear_character_attrs(from, to)
                    });
                } else {
                    self.set_character_style(Some(style));
                }
            }
            Some(StyleFamily::List) => self.set_list_style(Some(style)),
            None => {}
        }
    }

    /// The list style of the paragraph at the selection, when it is in a
    /// list.
    pub fn list_style(&self) -> Option<StyleId> {
        let format = self.format().paragraph;
        let styles = self.styles();
        format.list_style.filter(|id| {
            styles
                .list(*id)
                .is_some_and(desk_model::ListStyle::has_labels)
        })
    }

    /// Makes the selected paragraphs items of a list style, or takes them
    /// out of their lists (LST-02).
    pub fn set_list_style(&self, style: Option<StyleId>) {
        self.format_ranges("List Style", |edit, from, to| {
            edit.set_list_style(from, to, style)
        });
    }

    /// The text that the Writing Tools look up (INT-20): the selection,
    /// or the word at the cursor.
    pub fn lookup_text(&self) -> String {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.selected_text();
        }
        let word = self.unit_at(selection.focus, Unit::Word);
        let (start, end) = word.ordered();
        let state = self.state.borrow();
        state
            .editor
            .document()
            .body()
            .get(start.paragraph)
            .filter(|_| start.paragraph == end.paragraph)
            .and_then(|p| p.text().get(start.offset..end.offset).map(str::to_owned))
            .unwrap_or_default()
            .trim()
            .to_owned()
    }

    /// The counts of the document, and of the selection when there is
    /// one (INT-20).
    pub fn statistics(&self) -> (desk_model::Statistics, Option<desk_model::Statistics>) {
        let state = self.state.borrow();
        let document = state.editor.document();
        let all = document.statistics();
        let (start, end) = state.selection.ordered();
        let selected = (start != end)
            .then(|| {
                document
                    .statistics_between(start.position(document)?, end.position(document)?)
                    .ok()
            })
            .flatten();
        (all, selected)
    }

    /// The lines of the document, and of the selection when there is one
    /// (DOC-20).
    pub fn line_counts(&self) -> (usize, Option<usize>) {
        let state = self.state.borrow();
        let (start, end) = state.selection.ordered();
        let selected = (start != end).then(|| {
            state
                .layout
                .lines_between((start.paragraph, start.offset), (end.paragraph, end.offset))
        });
        (state.layout.line_count(), selected)
    }

    /// Everything that a search finds in the document (TXT-12, TXT-13).
    pub fn find(&self, query: &desk_model::Query) -> Vec<desk_model::Found> {
        self.state.borrow().editor.document().find(query)
    }

    /// Selects a piece of text that a search found, and shows it.
    pub fn select_found(&self, found: &desk_model::Found) {
        self.state.borrow_mut().typing = false;
        self.select(Selection {
            anchor: Spot {
                paragraph: found.index,
                offset: found.range.start,
            },
            focus: Spot {
                paragraph: found.index,
                offset: found.range.end,
            },
        });
    }

    /// Scrolls to the selection (Jump to Selection).
    pub fn show_selection(&self) {
        let selection = self.selection();
        self.select(selection);
    }

    /// The page, from 0, that a piece of text is on.
    pub fn page_of(&self, found: &desk_model::Found) -> Option<usize> {
        let state = self.state.borrow();
        let spot = Spot {
            paragraph: found.index,
            offset: found.range.start,
        };
        text_nav::caret(&state.layout, spot).map(|c| c.page)
    }

    /// Replaces a piece of text that a search found, as one action of the
    /// history, and puts the cursor after it.
    pub fn replace_found(&self, found: &desk_model::Found, with: &str) {
        let before = self.selection();
        let result = self.state.borrow_mut().editor.edit("Replace", |edit| {
            let after = edit.replace_found(found, with)?;
            Ok(Spot::of(edit.document(), after))
        });
        self.finish_edit(before, result);
    }

    /// Replaces everything a search finds, as one action of the history,
    /// and gives how many pieces were replaced.
    pub fn replace_all(&self, query: &desk_model::Query, with: &str) -> usize {
        let before = self.selection();
        let mut count = 0;
        let result = self.state.borrow_mut().editor.edit("Replace All", |edit| {
            count = edit.replace_all(query, with)?;
            Ok(Some(before.ordered().0))
        });
        if count > 0 {
            self.finish_edit(before, result);
        }
        count
    }

    /// The styles of the document.
    pub fn styles(&self) -> desk_model::Styles {
        self.state.borrow().editor.document().styles().clone()
    }

    /// The selected ranges: the selection, then those that Select All
    /// Uses added.
    fn ranges(&self) -> Vec<(Spot, Spot)> {
        let state = self.state.borrow();
        std::iter::once(&state.selection)
            .chain(&state.others)
            .map(Selection::ordered)
            .collect()
    }

    /// Changes each selected range as one action of the history.
    fn format_ranges(
        &self,
        name: &str,
        change: impl Fn(
            &mut desk_model::Edit,
            desk_model::Position,
            desk_model::Position,
        ) -> Result<(), ModelError>,
    ) {
        let ranges = self.ranges();
        self.format_edit(name, move |edit| {
            for (start, end) in ranges {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                change(edit, from, to)?;
            }
            Ok(())
        });
    }

    /// Runs a change of the styles on the selection, as one action of
    /// the history, and gives back its value. A change that the model
    /// refuses, such as a name that another style has, gives the error.
    pub fn style_edit<T>(
        &self,
        name: &str,
        change: impl FnOnce(
            &mut desk_model::Edit,
            desk_model::Position,
            desk_model::Position,
        ) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        let (start, end) = self.selection().ordered();
        let mut value = None;
        self.format_edit_quiet(name, |edit| {
            let document = edit.document();
            let from = at_spot(document, start)?;
            let to = at_spot(document, end)?;
            value = Some(change(edit, from, to)?);
            Ok(())
        })?;
        value.ok_or(ModelError::InvalidText)
    }

    /// The positions of the selection in the model.
    pub fn selected_positions(&self) -> Option<(desk_model::Position, desk_model::Position)> {
        let (start, end) = self.selection().ordered();
        let state = self.state.borrow();
        let document = state.editor.document();
        Some((start.position(document)?, end.position(document)?))
    }

    /// Where the formatting at the selection differs from its styles.
    pub fn overrides(&self) -> desk_model::Overrides {
        let Some((start, end)) = self.selected_positions() else {
            return desk_model::Overrides::default();
        };
        self.state
            .borrow()
            .editor
            .document()
            .overrides(start, end)
            .unwrap_or_default()
    }

    /// The character style at the selection.
    pub fn character_style(&self) -> Option<StyleId> {
        let (start, end) = self.selection().ordered();
        let state = self.state.borrow();
        let paragraph = state.editor.document().body().get(start.paragraph)?;
        let at = if start == end || start.offset >= paragraph.len() {
            start.offset
        } else {
            start.offset + 1
        };
        paragraph.format_at(at).0
    }

    /// Takes the style of the text at the selection (STY-08).
    pub fn copy_style(&self) -> Option<desk_model::CopiedStyle> {
        let (start, _) = self.selected_positions()?;
        self.state.borrow().editor.document().copy_style(start).ok()
    }

    /// Pastes a copied style over the selection.
    pub fn paste_style(&self, copied: &desk_model::CopiedStyle) {
        self.format_ranges("Paste Style", |edit, from, to| {
            edit.paste_style(from, to, copied)
        });
    }

    /// Selects every use of a style in the body (STY-09). Returns false
    /// when nothing uses it.
    pub fn select_uses(&self, style: StyleId) -> bool {
        let selections: Vec<Selection> = {
            let state = self.state.borrow();
            let document = state.editor.document();
            document
                .style_uses(style)
                .into_iter()
                .filter_map(|(paragraph, range)| {
                    let start =
                        Spot::of(document, desk_model::Position::new(paragraph, range.start))?;
                    let end = Spot::of(document, desk_model::Position::new(paragraph, range.end))?;
                    Some(Selection {
                        anchor: start,
                        focus: end,
                    })
                })
                .collect()
        };
        let Some((first, rest)) = selections.split_first() else {
            return false;
        };
        self.state.borrow_mut().typing = false;
        self.select(*first);
        self.state.borrow_mut().others = rest.to_vec();
        self.select(*first);
        true
    }

    /// Changes formatting as one action of the history, keeping the
    /// selection.
    fn format_edit(
        &self,
        name: &str,
        change: impl FnOnce(&mut desk_model::Edit) -> Result<(), ModelError>,
    ) {
        if let Err(error) = self.format_edit_quiet(name, change) {
            glib::g_warning!("desk", "cannot format: {error}");
        }
    }

    /// Changes formatting as one action of the history, keeping the
    /// selection, and gives back the error of a refused change.
    fn format_edit_quiet(
        &self,
        name: &str,
        change: impl FnOnce(&mut desk_model::Edit) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
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
            Err(error) => return Err(error),
        }
        Ok(())
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
                if !state.adding {
                    state.others.clear();
                }
            }
            state.selection = selection;
            let mut rects = text_nav::selection_rects(&state.layout, &selection);
            for other in &state.others {
                rects.extend(text_nav::selection_rects(&state.layout, other));
            }
            (rects, text_nav::caret(&state.layout, selection.focus))
        };
        self.view.set_marks(rects, caret);
        let (focus, range) = {
            let state = self.state.borrow();
            let document = state.editor.document();
            let selection = state.selection;
            let offset = |spot| state.access.offset_of(document, spot);
            let (start, end) = selection.ordered();
            let range = (!selection.is_empty()).then(|| (offset(start), offset(end)));
            (offset(selection.focus), range)
        };
        self.view.access().set_caret(focus, range);
        let skip = self.state.borrow().squiggle_skip;
        if skip.is_some_and(|s| s != self.state.borrow().selection.focus) {
            self.update_squiggles();
        }
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
                // In a list, Tab and Shift-Tab change the level of the
                // item and the items under it, and with Alt only of the
                // item (LST-02).
                let alt = modifiers.contains(gdk::ModifierType::ALT_MASK);
                let by = if shift { -1 } else { 1 };
                if !self.change_list_level(by, !alt) {
                    self.replace_selection("Typing", "\t");
                }
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
            let modifiers = gesture.current_event_state();
            let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
            let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
            {
                // Ctrl and a click start another range, and keep what was
                // selected (TXT-03).
                let mut state = this.state.borrow_mut();
                state.adding = control && !shift;
                if state.adding {
                    let current = state.selection;
                    if !current.is_empty() && !state.others.contains(&current) {
                        state.others.push(current);
                    }
                }
            }
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
        let weak = self.downgrade();
        click.connect_released(move |_, _, _, _| {
            if let Some(this) = weak.upgrade() {
                this.state.borrow_mut().adding = false;
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
            let mut state = this.state.borrow_mut();
            state.drag_from = None;
            state.adding = false;
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

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{Editor, Language};

    #[test]
    fn txt_07_a_misspelling_with_one_suggestion_is_corrected() {
        if desk_lang::speller("en-US").is_none() {
            return;
        }
        let english = CharAttrs {
            language: Some(Language::new("en-US")),
            ..CharAttrs::default()
        };
        let correct = |text: &str| {
            let mut editor = Editor::new(Document::new_word_processing());
            let start = editor.document().start();
            editor
                .edit("Typing", |edit| {
                    let end = edit.insert_text(start, text)?;
                    edit.set_character_attrs(start, end, &english)?;
                    accept_single_suggestion(edit, end)
                })
                .unwrap();
            editor.document().body().get(0).unwrap().text().to_string()
        };
        // The dictionary has one suggestion for "acheive", and three for
        // "recieve".
        assert_eq!(correct("We acheive"), "We achieve");
        assert_eq!(correct("We recieve"), "We recieve");
        assert_eq!(correct("We achieve"), "We achieve");
    }
}
