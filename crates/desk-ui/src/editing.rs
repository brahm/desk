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
use std::sync::Arc;
use std::time::Duration;

use desk_layout::DocumentLayout;
use desk_model::{
    CharAttrs, CharFormat, Document, Editor, FIELD_CHARACTER, Field, LINE_BREAK, Layout, Length,
    MarginPlace, ModelError, PageSetup, ParaAttrs, ParaFormat, Section, Start, StyleFamily,
    StyleId,
};
use desk_render::DisplayList;
use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::access::AccessText;
use crate::i18n::tr;
use crate::text_nav::{self, Selection, Spot};
use crate::view::{DocumentView, WeakView};

mod connections;
mod copies;
mod guides;
mod media;
mod navigate;
mod objects;
mod outline;
mod pen;
mod points;
mod sections;
mod stories;

pub use navigate::Navigate;
pub use objects::{NewShape, ObjectInfo};
pub use outline::OutlineRow;
pub use sections::has_sections_clip;

use desk_model::Story;

/// A break that Insert puts at the cursor (EST-04, EST-08, EST-09,
/// EST-16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakKind {
    /// The text after it starts a new page.
    Page,
    /// The text after it starts the next column.
    Column,
    /// The text after it starts a new layout, with the columns of the one
    /// it is in.
    Layout,
    /// The text after it starts a new section on a new page.
    Section,
}

/// A field that Insert puts at the cursor (EST-27).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// The number of the page.
    PageNumber,
    /// The number of pages.
    PageCount,
    /// Today's date.
    Date,
    /// The name of the file.
    FileName,
}

pub use crate::fields::today;

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
        story: None,
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
        story: None,
    };
    let end = edit.replace_found(&found, suggestion)?;
    Ok(desk_model::Position::new(at.paragraph, end.offset))
}

thread_local! {
    /// The text last copied in Desk, with its styles, while the clipboard
    /// still holds that text.
    static COPIED: RefCell<Option<(String, desk_model::Fragment)>> = const { RefCell::new(None) };
}

/// Forgets the text copied, as objects were copied after it.
fn forget_copied_text() {
    COPIED.with(|c| *c.borrow_mut() = None);
}

/// The marks of the notes in the body, by paragraph and offset, in order.
fn note_marks(document: &Document) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (index, paragraph) in document.body().iter().enumerate() {
        let mut offset = 0;
        for run in paragraph.runs() {
            if let Some(Some(Field::NoteMark { note })) = &run.attrs.field
                && document.notes().get(*note).is_some()
            {
                out.push((index, offset));
            }
            offset += run.len();
        }
    }
    out
}

/// The note whose mark is just before a spot in its paragraph, or else
/// the nearest one in the paragraph.
fn note_at(document: &Document, spot: Spot) -> Option<desk_model::Note> {
    let paragraph = document.body().get(spot.paragraph)?;
    let mut offset = 0;
    let mut best: Option<(usize, desk_model::NodeId)> = None;
    for run in paragraph.runs() {
        if let Some(Some(Field::NoteMark { note })) = &run.attrs.field
            && document.notes().get(*note).is_some()
        {
            let end = offset + run.len();
            let distance = if end <= spot.offset {
                spot.offset - end
            } else {
                offset.abs_diff(spot.offset) + 1
            };
            if best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, *note));
            }
        }
        offset += run.len();
    }
    best.and_then(|(_, note)| document.notes().get(note).cloned())
}

/// The linked text at a spot: the character after it, or else the one
/// before, with the link and the range of the text that shares it.
fn link_at(document: &Document, spot: Spot) -> Option<(Arc<str>, std::ops::Range<usize>)> {
    let paragraph = document.body().get(spot.paragraph)?;
    let mut runs: Vec<(std::ops::Range<usize>, Option<Arc<str>>)> = Vec::new();
    let mut offset = 0;
    for run in paragraph.runs() {
        runs.push((offset..offset + run.len(), run.attrs.link.clone().flatten()));
        offset += run.len();
    }
    let index = runs
        .iter()
        .position(|(r, l)| l.is_some() && r.start <= spot.offset && spot.offset < r.end)
        .or_else(|| {
            runs.iter()
                .position(|(r, l)| l.is_some() && r.end == spot.offset && r.start < r.end)
        })?;
    let link = runs[index].1.clone()?;
    let same = |i: usize| runs[i].1.as_deref() == Some(&*link);
    let mut first = index;
    while first > 0 && same(first - 1) {
        first -= 1;
    }
    let mut last = index;
    while last + 1 < runs.len() && same(last + 1) {
        last += 1;
    }
    Some((link, runs[first].0.start..runs[last].0.end))
}

/// The bookmarks of the body, in order: each name, from the start of its
/// first text to the end of its last.
fn bookmarks(document: &Document) -> Vec<(Arc<str>, Spot, Spot)> {
    let mut out: Vec<(Arc<str>, Spot, Spot)> = Vec::new();
    for (index, paragraph) in document.body().iter().enumerate() {
        let mut offset = 0;
        for run in paragraph.runs() {
            let end = offset + run.len();
            if let Some(Some(name)) = &run.attrs.bookmark {
                match out.iter_mut().find(|(n, _, _)| n == name) {
                    Some(found) => found.2 = Spot::new(index, end),
                    None => out.push((
                        name.clone(),
                        Spot::new(index, offset),
                        Spot::new(index, end),
                    )),
                }
            }
            offset = end;
        }
    }
    out
}

/// The email or web address that the text before the cursor ends with,
/// which Pages makes a link as it is typed (TXT-07): a word that starts
/// with "www." or "http", or an address with "@" and a dot after it.
/// Gives its range, without the punctuation after it, and where it
/// links to.
pub fn typed_address(before: &str) -> Option<(std::ops::Range<usize>, String)> {
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let word = &before[start..];
    let opening = |c: char| matches!(c, '(' | '[' | '{' | '"' | '\'' | '“' | '‘');
    let closing = |c: char| {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\'' | '”' | '’'
        )
    };
    let address = word.trim_start_matches(opening).trim_end_matches(closing);
    let lower = address.to_lowercase();
    let web = ["www.", "http://", "https://"]
        .iter()
        .any(|p| lower.starts_with(p) && lower.len() > p.len());
    let email = address.split_once('@').is_some_and(|(name, domain)| {
        !name.is_empty()
            && !domain.contains('@')
            && domain
                .split_once('.')
                .is_some_and(|(host, top)| !host.is_empty() && !top.is_empty())
    });
    if !(web || email) {
        return None;
    }
    let from = start + (word.len() - word.trim_start_matches(opening).len());
    Some((from..from + address.len(), link_target(address)))
}

/// Makes the address that ends before `at` a link, unless its text links
/// somewhere already (TXT-07).
fn detect_link(edit: &mut desk_model::Edit, at: desk_model::Position) -> Result<(), ModelError> {
    let paragraph = edit.document().paragraph(at.paragraph)?;
    let text = paragraph.text();
    let Some((range, target)) = typed_address(text.get(..at.offset).unwrap_or_default()) else {
        return Ok(());
    };
    let (_, attrs) = paragraph.format_at(range.start + 1);
    if attrs.link.is_some() {
        return Ok(());
    }
    edit.set_link(
        desk_model::Position::new(at.paragraph, range.start),
        desk_model::Position::new(at.paragraph, range.end),
        Some(Arc::from(target)),
    )
}

/// The place a link of the text names as it is typed: a web page when it
/// names no other kind of place.
pub fn link_target(text: &str) -> String {
    let text = text.trim();
    if text.is_empty()
        || text.starts_with('#')
        || text.contains("://")
        || text.starts_with("mailto:")
    {
        text.to_owned()
    } else if text.contains('@') && !text.contains(char::is_whitespace) {
        format!("mailto:{text}")
    } else {
        format!("https://{text}")
    }
}

/// What tables of contents list in a document that chose nothing yet:
/// the paragraph styles whose names start with "Heading", in the order of
/// their names, each with the style "TOC" and its name for its entries
/// when there is one.
pub fn default_toc(document: &Document) -> desk_model::TocSettings {
    let styles = document.styles();
    let mut headings: Vec<(String, StyleId)> = styles
        .iter()
        .filter(|s| s.family() == StyleFamily::Paragraph && s.name().starts_with("Heading"))
        .map(|s| (s.name().to_owned(), s.id()))
        .collect();
    headings.sort();
    desk_model::TocSettings {
        levels: headings
            .into_iter()
            .map(|(name, style)| desk_model::TocLevel {
                style,
                entry: styles
                    .by_name(StyleFamily::Paragraph, &format!("TOC {name}"))
                    .map_or(styles.default_paragraph(), |s| s.id()),
                page_number: true,
            })
            .collect(),
        page_links: true,
    }
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
    /// A drag of the label of a list item (LST-02): where it started on
    /// its page, and how many levels it has moved the item.
    label_drag: Option<(f64, i32)>,
    /// The picture that clicks take colors out of, while Instant Alpha
    /// is on (OBJ-06), and whether a press on it is being dragged.
    alpha_pick: Option<(desk_model::NodeId, bool)>,
    /// The shape being drawn while the pen is on (OBJ-09).
    pen: Option<pen::Pen>,
    /// A drag of an end or the middle of a connection line (OBJ-31).
    connection_drag: Option<connections::ConnectionDrag>,
    /// The shape whose points show to be edited (OBJ-10).
    points_edit: Option<points::PointsEdit>,
    /// Whether the last action was typing at the cursor, which the next
    /// letter typed continues as one step of the history.
    typing: bool,
    /// The text that Check Spelling was limited to, by selecting it
    /// first (TXT-10), while it goes through it.
    spelling_scope: Option<(Spot, Spot)>,
    /// More selected ranges, which Select All Uses adds (STY-09). Changes
    /// of formatting apply to them too; moving the cursor drops them.
    others: Vec<Selection>,
    /// Whether a Ctrl-click or the drag after it is adding a range to the
    /// selection, which keeps the others (TXT-03).
    adding: bool,
    /// The name of the file of the document, which a field of the file
    /// name shows when it is inserted (EST-27).
    file_name: String,
    /// The cursor at the end of a word being typed, which the spelling
    /// checker leaves unmarked until the cursor leaves it.
    squiggle_skip: Option<Spot>,
    /// The header, footer or note being edited on the page, while the
    /// editor edits a copy of it (EST-12, EST-13).
    story: Option<stories::StoryMode>,
    /// The objects selected, from back to front, rather than text
    /// (OBJ-21).
    objects: Vec<desk_model::NodeId>,
    /// A drag of objects in progress.
    object_drag: Option<objects::ObjectDrag>,
    /// A drag that selects the objects in a rectangle (OBJ-21).
    marquee: Option<copies::MarqueeDrag>,
    /// Whether the layout view is on (INT-09).
    show_layout: bool,
    /// A drag of a fixed guide (OBJ-28).
    guide_drag: Option<guides::GuideDrag>,
    /// Which objects behind the text a press selects (OBJ-25, EST-21).
    selectable: objects::Selectable,
    /// The guides it snaps to.
    guides: Vec<crate::view::Guide>,
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
    /// The size and margins of the pages.
    pub setup: PageSetup,
    /// The layout that the selection starts in.
    pub layout: Layout,
    /// The section that the selection starts in.
    pub section: Section,
    /// Whether that section is the first one of the document.
    pub first_section: bool,
    /// The section whose headers and footers the selection shows, by the
    /// paragraph that starts it, or none for the first one's (EST-16).
    pub headers_section: Option<desk_model::NodeId>,
    /// The headers and footers of the document.
    pub headers: desk_model::HeadersFooters,
    /// The note whose mark is at the cursor, or nearest to it in its
    /// paragraph (EST-13).
    pub note: Option<desk_model::Note>,
    /// Where the text at the cursor links to (EST-25).
    pub link: Option<Arc<str>>,
    /// The names of the bookmarks of the document, in order (EST-26).
    pub bookmarks: Vec<Arc<str>>,
    /// The selected objects, when objects are selected (OBJ-21).
    pub object: Option<ObjectInfo>,
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
                label_drag: None,
                alpha_pick: None,
                pen: None,
                connection_drag: None,
                points_edit: None,
                typing: false,
                pending: None,
                on_select: Vec::new(),
                invisibles: false,
                access: AccessText::default(),
                spelling_scope: None,
                others: Vec::new(),
                adding: false,
                squiggle_skip: None,
                file_name: String::new(),
                story: None,
                objects: Vec::new(),
                object_drag: None,
                marquee: None,
                show_layout: false,
                guide_drag: None,
                selectable: objects::Selectable::default(),
                guides: Vec::new(),
            })),
        };
        this.show_pages();
        this.update_squiggles();
        this.connect_keyboard();
        this.connect_pointer();
        this.connect_drop();
        this.connect_input_method();
        this.start_blinking();
        this.view.area().grab_focus();
        this
    }

    /// The document as it is now.
    pub fn document(&self) -> Document {
        let state = self.state.borrow();
        match &state.story {
            Some(mode) => mode.main.snapshot(),
            None => state.editor.snapshot(),
        }
    }

    /// The pages as they are now.
    pub fn pages(&self) -> Vec<DisplayList> {
        let state = self.state.borrow();
        DisplayList::pages(Self::main_layout(&state))
    }

    /// The number of words of the document.
    pub fn word_count(&self) -> usize {
        Self::main_document(&self.state.borrow()).statistics().words
    }

    /// Whether the document has changed since it was saved or opened.
    pub fn is_modified(&self) -> bool {
        let state = self.state.borrow();
        let last = match &state.story {
            Some(mode) => state.undo.last().or(mode.undo.last()),
            None => state.undo.last(),
        };
        last.map(|a| a.2) != state.saved
    }

    /// Notes that the document as it is now was saved. The next letter
    /// typed starts a new action, so that Undo goes back to what was saved.
    pub fn mark_saved(&self) {
        {
            let mut state = self.state.borrow_mut();
            let last = match &state.story {
                Some(mode) => state.undo.last().or(mode.undo.last()),
                None => state.undo.last(),
            };
            state.saved = last.map(|a| a.2);
            state.typing = false;
        }
        self.changed();
    }

    /// Whether Undo has something to take back.
    pub fn can_undo(&self) -> bool {
        let state = self.state.borrow();
        state.editor.undo_name().is_some()
            || state
                .story
                .as_ref()
                .is_some_and(|m| m.main.undo_name().is_some())
    }

    /// The name of the change that Undo takes back, if any (DOC-12).
    pub fn undo_name(&self) -> Option<String> {
        let state = self.state.borrow();
        let main = state.story.as_ref().and_then(|m| m.main.undo_name());
        match (state.story.is_some() && state.undo.is_empty(), main) {
            // Nothing left of the visit: Undo goes on in the body.
            (true, Some(main)) => Some(main.to_owned()),
            _ => state.editor.undo_name().map(str::to_owned),
        }
    }

    /// The name of the change that Redo brings back, if any.
    pub fn redo_name(&self) -> Option<String> {
        let state = self.state.borrow();
        state
            .editor
            .redo_name()
            .or_else(|| state.story.as_ref().and_then(|m| m.main.redo_name()))
            .map(str::to_owned)
    }

    /// Whether Redo has something to bring back.
    pub fn can_redo(&self) -> bool {
        let state = self.state.borrow();
        state.editor.redo_name().is_some()
            || state
                .story
                .as_ref()
                .is_some_and(|m| m.main.redo_name().is_some())
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
        // Out of the text, it selects the objects (OBJ-21).
        if self.has_selected_objects() || (self.is_page_layout() && self.story().is_none()) {
            self.select_all_objects();
            return;
        }
        let document = self.state.borrow().editor.snapshot();
        self.select(Selection {
            anchor: Spot::default(),
            focus: text_nav::document_edge(&document, true),
        });
    }

    /// Shows the emoji and symbols of GTK at the cursor, and types the one
    /// chosen (TXT-05, as Edit ▸ Special Characters in Pages). Accents and
    /// the scripts of Chinese, Japanese and Korean come through the input
    /// methods.
    pub fn special_characters(&self) {
        let caret = {
            let state = self.state.borrow();
            text_nav::caret(&state.layout, state.selection.focus)
        };
        let area = self.view.area();
        let chooser = gtk::EmojiChooser::new();
        chooser.set_parent(area);
        if let Some(caret) = caret
            && let Some((x, y)) = self.view.to_area(caret.page, caret.x, caret.top)
        {
            let bottom = self
                .view
                .to_area(caret.page, caret.x, caret.bottom)
                .map_or(y + 1.0, |(_, b)| b);
            chooser.set_pointing_to(Some(&gdk::Rectangle::new(
                x as i32,
                y as i32,
                1,
                (bottom - y).max(1.0) as i32,
            )));
        }
        let weak = self.downgrade();
        chooser.connect_emoji_picked(move |_, text| {
            if let Some(this) = weak.upgrade() {
                this.replace_selection("Typing", text);
            }
        });
        chooser.connect_closed(|chooser| {
            let chooser = chooser.clone();
            glib::idle_add_local_once(move || chooser.unparent());
        });
        chooser.popup();
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

    /// Copies the selection to the clipboard: with its styles for Desk,
    /// with its formatting for the other programs that read it, and as
    /// text alone for the rest (TXT-04).
    pub fn copy(&self) {
        if self.has_selected_objects() {
            self.copy_objects();
            return;
        }
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
        let mut contents = vec![gdk::ContentProvider::for_value(&text.to_value())];
        if let Some((fragment, format)) = fragment.as_ref().zip(crate::objects_format()) {
            // The other apps of Desk get the text with its styles (IOP-17,
            // IOP-25).
            match (format.write_text)(fragment) {
                Ok(bytes) => contents.push(gdk::ContentProvider::for_bytes(
                    format.text_type,
                    &gtk::glib::Bytes::from_owned(bytes),
                )),
                Err(error) => {
                    gtk::glib::g_warning!("desk", "cannot copy the styles of the text: {error}");
                }
            }
            // Other programs get it with its formatting, as a word
            // processor or a web browser reads it.
            match (format.write_foreign_text)(fragment) {
                Ok(flavors) => contents.extend(flavors.into_iter().map(|(media_type, bytes)| {
                    gdk::ContentProvider::for_bytes(
                        media_type,
                        &gtk::glib::Bytes::from_owned(bytes),
                    )
                })),
                Err(error) => {
                    gtk::glib::g_warning!(
                        "desk",
                        "cannot copy the formatting of the text: {error}"
                    );
                }
            }
        }
        COPIED.with(|c| *c.borrow_mut() = fragment.map(|f| (text.clone(), f)));
        copies::forget_copied_objects();
        let clipboard = self.view.area().clipboard();
        if contents.len() == 1 {
            clipboard.set_text(&text);
        } else if let Err(error) =
            clipboard.set_content(Some(&gdk::ContentProvider::new_union(&contents)))
        {
            gtk::glib::g_warning!("desk", "cannot copy: {error}");
        }
    }

    /// Copies the selection and deletes it.
    pub fn cut(&self) {
        if self.has_selected_objects() {
            self.cut_objects();
            return;
        }
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
        if self.paste_objects() {
            return;
        }
        if keep && self.paste_text_of_other_apps() {
            return;
        }
        if self.paste_picture_of_other_programs() {
            return;
        }
        if keep && self.paste_formatted_text_of_other_programs() {
            return;
        }
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

    /// Pastes the text with its styles that another app of Desk put on the
    /// clipboard, when it holds some (IOP-17, IOP-25).
    fn paste_text_of_other_apps(&self) -> bool {
        let clipboard = self.view.area().clipboard();
        let Some(format) = crate::objects_format() else {
            return false;
        };
        if clipboard.is_local() || !clipboard.formats().contain_mime_type(format.text_type) {
            return false;
        }
        let weak = self.downgrade();
        gtk::glib::spawn_future_local(async move {
            let read = async {
                let (stream, _) = clipboard
                    .read_future(&[format.text_type], gtk::glib::Priority::DEFAULT)
                    .await
                    .map_err(|e| e.to_string())?;
                let out = gtk::gio::MemoryOutputStream::new_resizable();
                out.splice_future(
                    &stream,
                    gtk::gio::OutputStreamSpliceFlags::CLOSE_SOURCE
                        | gtk::gio::OutputStreamSpliceFlags::CLOSE_TARGET,
                    gtk::glib::Priority::DEFAULT,
                )
                .await
                .map_err(|e| e.to_string())?;
                (format.read_text)(&out.steal_as_bytes())
            };
            let fragment = match read.await {
                Ok(fragment) => fragment,
                Err(error) => {
                    gtk::glib::g_warning!("desk", "cannot paste the text: {error}");
                    return;
                }
            };
            if let Some(this) = weak.upgrade() {
                this.paste_fragment(&fragment);
            }
        });
        true
    }

    /// Pastes the formatted text that another program put on the
    /// clipboard, as LibreOffice and web browsers do, with its emphasis,
    /// sizes, colors and links (TXT-04).
    fn paste_formatted_text_of_other_programs(&self) -> bool {
        let clipboard = self.view.area().clipboard();
        let Some(format) = crate::objects_format() else {
            return false;
        };
        if clipboard.is_local() {
            return false;
        }
        let formats = clipboard.formats();
        let Some(&media_type) = format
            .foreign_text_types
            .iter()
            .find(|t| formats.contain_mime_type(t))
        else {
            return false;
        };
        let weak = self.downgrade();
        gtk::glib::spawn_future_local(async move {
            let fragment = match clipboard_bytes(&clipboard, media_type).await {
                Ok(bytes) => (format.read_foreign_text)(media_type, &bytes),
                Err(error) => Err(error),
            };
            let Some(this) = weak.upgrade() else { return };
            match fragment {
                Ok(fragment) if !fragment.text().is_empty() => this.paste_fragment(&fragment),
                // Nothing to read: the text alone.
                _ => {
                    clipboard.read_text_async(gtk::gio::Cancellable::NONE, move |result| {
                        if let (Ok(Some(text)), Some(this)) = (result, weak.upgrade()) {
                            this.replace_selection("Paste", &text);
                        }
                    });
                }
            }
        });
        true
    }

    /// Pastes the picture that another program put on the clipboard with
    /// no text, in the line of the text at the cursor (TXT-04).
    fn paste_picture_of_other_programs(&self) -> bool {
        let clipboard = self.view.area().clipboard();
        let formats = clipboard.formats();
        if clipboard.is_local()
            || !formats.contains_type(gdk::Texture::static_type())
            || formats.contain_mime_type("text/plain")
            || formats.contain_mime_type("text/plain;charset=utf-8")
        {
            return false;
        }
        let weak = self.downgrade();
        clipboard.read_texture_async(gtk::gio::Cancellable::NONE, move |result| {
            let (Ok(Some(texture)), Some(this)) = (result, weak.upgrade()) else {
                return;
            };
            let png = texture.save_to_png_bytes();
            match desk_model::Picture::from_png(png.to_vec()) {
                Some(picture) => this.insert_picture_in_line(picture),
                None => gtk::glib::g_warning!("desk", "cannot paste the picture"),
            }
        });
        true
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
        if self.toc_refuses(start, end) {
            return;
        }
        let result = self.state.borrow_mut().editor.edit("Paste", |edit| {
            let from = at_spot(edit.document(), start)?;
            let to = at_spot(edit.document(), end)?;
            let at = if start == end {
                from
            } else {
                edit.delete(from, to)?
            };
            let named = crate::objects_format().is_none_or(|f| f.named_styles);
            let after = if named {
                edit.paste_fragment(at, fragment)?
            } else {
                edit.paste_fragment_resolved(at, fragment)?
            };
            Ok(Spot::of(edit.document(), after))
        });
        self.finish_edit(before, result);
    }

    /// Undoes the last action.
    pub fn undo(&self) {
        self.state.borrow_mut().typing = false;
        if self.story().is_some() && self.state.borrow().undo.is_empty() {
            // Nothing left of the visit: Undo goes on in the body.
            self.leave_story(None);
        }
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
        if self.story().is_some() && self.state.borrow().redo.is_empty() {
            self.leave_story(None);
        }
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
        if self.toc_refuses(start, end) {
            return;
        }
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
                    && c.is_whitespace()
                    && preferences.detect_links
                {
                    detect_link(edit, at)?;
                }
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
        if self.toc_refuses(start, end) {
            return;
        }
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
        if self.toc_refuses(start, end) {
            return;
        }
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
                    if preferences.detect_links {
                        detect_link(edit, from)?;
                    }
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

    /// Puts a break at the cursor, or in place of the selection: the
    /// paragraph splits there, and the text after the break starts a
    /// page, a column, a layout or a section.
    pub fn insert_break(&self, kind: BreakKind) {
        if self.is_page_layout() {
            // A page layout document has pages, not text to break: a
            // page break adds a page (INT-03).
            if kind == BreakKind::Page {
                self.add_layout_page();
            }
            return;
        }
        if self.story().is_some() {
            // Not in headers, footers and notes, as in Pages.
            return;
        }
        let before = self.selection();
        let (start, end) = before.ordered();
        if self.toc_refuses(start, end) {
            return;
        }
        let result = {
            let mut state = self.state.borrow_mut();
            state.editor.edit("Insert Break", |edit| {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                let at = edit.delete(from, to)?;
                let index = edit.document().body().index_of(at.paragraph).unwrap_or(0);
                let (_, layout) = edit.document().layout_at(index);
                let after = edit.split_paragraph(at)?;
                let attrs = match kind {
                    BreakKind::Page => ParaAttrs {
                        page_break_before: Some(true),
                        ..ParaAttrs::default()
                    },
                    BreakKind::Column => ParaAttrs {
                        column_break_before: Some(true),
                        ..ParaAttrs::default()
                    },
                    BreakKind::Layout => ParaAttrs {
                        start: Some(Some(Arc::new(Start::Layout(layout)))),
                        ..ParaAttrs::default()
                    },
                    BreakKind::Section => ParaAttrs {
                        start: Some(Some(Arc::new(Start::Section(Section::default(), layout)))),
                        ..ParaAttrs::default()
                    },
                };
                edit.set_paragraph_attrs(after, after, &attrs)?;
                Ok(Spot::of(edit.document(), after))
            })
        };
        self.finish_edit(before, result);
    }

    /// Sets the file of the document, its path or only its name when it
    /// has none, which fields of the file name show.
    pub fn set_file_name(&self, name: &str) {
        self.state.borrow_mut().file_name = name.to_owned();
    }

    /// The file of the document: its path, or only its name.
    pub fn file_name(&self) -> String {
        self.state.borrow().file_name.clone()
    }

    /// A field of the file name, or of today's date, as Insert makes them.
    pub fn new_field(&self, kind: FieldKind) -> Field {
        match kind {
            FieldKind::PageNumber => Field::PAGE_NUMBER,
            FieldKind::PageCount => Field::PAGE_COUNT,
            FieldKind::Date => today(),
            FieldKind::FileName => Field::FileName {
                shown: desk_model::file_shown(&self.file_name(), false, true).into(),
                path: false,
                extension: true,
            },
        }
    }

    /// The field at the cursor that has a format (EST-27): the one
    /// selected, or just after or before the cursor. With the spot where
    /// it starts.
    pub fn field_at_cursor(&self) -> Option<(Spot, Field)> {
        let state = self.state.borrow();
        let (start, _) = state.selection.ordered();
        let paragraph = state.editor.document().body().get(start.paragraph)?;
        let mut offset = 0;
        let mut before = None;
        for run in paragraph.runs() {
            let end = offset + run.len();
            if let Some(Some(field)) = &run.attrs.field
                && !matches!(
                    field,
                    Field::NoteMark { .. } | Field::PageOf { .. } | Field::ObjectMark { .. }
                )
            {
                let found = (Spot::new(start.paragraph, offset), field.clone());
                if offset <= start.offset && start.offset < end {
                    return Some(found);
                }
                if end == start.offset {
                    before = Some(found);
                }
            }
            offset = end;
        }
        before
    }

    /// Selects the field under a point of the view, as a secondary click
    /// on it does before its menu opens. Whether there was one.
    pub fn select_field_at(&self, x: f64, y: f64) -> bool {
        let Some(spot) = self.spot_at(x, y) else {
            return false;
        };
        let (start, end) = self.selection().ordered();
        // A click inside the selection keeps it.
        if start != end && start <= spot && spot <= end {
            return false;
        }
        self.select(Selection::cursor(spot));
        match self.field_at_cursor() {
            Some((at, _)) => {
                self.select(Selection {
                    anchor: at,
                    focus: Spot::new(at.paragraph, at.offset + FIELD_CHARACTER.len_utf8()),
                });
                true
            }
            None => false,
        }
    }

    /// Changes the field at the cursor, as the menu of a field does.
    pub fn set_field_at_cursor(&self, field: Field) {
        let Some((spot, old)) = self.field_at_cursor() else {
            return;
        };
        if old == field {
            return;
        }
        let file = self.file_name();
        // A new format of a file name or date shows at once.
        let field = field
            .refreshed(&file, Some(&crate::fields::format_date))
            .unwrap_or(field);
        self.format_edit("Field Format", move |edit| {
            let at = at_spot(edit.document(), spot)?;
            let end =
                desk_model::Position::new(at.paragraph, at.offset + FIELD_CHARACTER.len_utf8());
            edit.set_character_attrs(
                at,
                end,
                &CharAttrs {
                    field: Some(Some(field)),
                    ..CharAttrs::default()
                },
            )
        });
    }

    /// Shows the file in the fields of the file name, as when the
    /// document is saved under another name (EST-27).
    pub fn refresh_fields(&self) {
        let file = self.file_name();
        self.format_edit("Update Fields", move |edit| {
            edit.refresh_fields(&file, None)
        });
    }

    /// Puts a field at the cursor, or in place of the selection (EST-27).
    pub fn insert_field(&self, field: Field) {
        let before = self.selection();
        let (start, end) = before.ordered();
        if self.toc_refuses(start, end) {
            return;
        }
        let pending = self.state.borrow_mut().pending.take();
        let result = {
            let mut state = self.state.borrow_mut();
            state.editor.edit("Insert Field", |edit| {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                let at = edit.delete(from, to)?;
                let after = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
                let mut attrs = pending.unwrap_or_default();
                attrs.field = Some(Some(field));
                edit.set_character_attrs(at, after, &attrs)?;
                Ok(Spot::of(edit.document(), after))
            })
        };
        self.finish_edit(before, result);
    }

    /// Puts the mark of a new note at the cursor, or in place of the
    /// selection, with an empty note (EST-13). The note takes the Footnote
    /// Text style when the document has one.
    pub fn insert_note(&self) {
        if self.story().is_some() {
            // Not in headers, footers and notes, as in Pages.
            return;
        }
        let before = self.selection();
        let (start, end) = before.ordered();
        if self.toc_refuses(start, end) {
            return;
        }
        let pending = self.state.borrow_mut().pending.take();
        let result = {
            let mut state = self.state.borrow_mut();
            state.editor.edit("Insert Note", |edit| {
                let document = edit.document();
                let styles = document.styles();
                let style = ["Footnote Text", "Footnote"]
                    .iter()
                    .find_map(|name| styles.by_name(StyleFamily::Paragraph, name))
                    .map_or(styles.default_paragraph(), |s| s.id());
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                let at = edit.delete(from, to)?;
                let note = edit.new_id();
                let after = edit.insert_text(at, &FIELD_CHARACTER.to_string())?;
                let mark = CharAttrs {
                    field: Some(Some(Field::NoteMark { note })),
                    baseline: Some(desk_model::Baseline::Superscript),
                    ..pending.unwrap_or_default().without_field()
                };
                edit.set_character_attrs(at, after, &mark)?;
                let id = edit.new_id();
                let paragraph = desk_model::Paragraph::from_runs(
                    id,
                    style,
                    ParaAttrs::default(),
                    [
                        desk_model::Run::with_attrs(&FIELD_CHARACTER.to_string(), None, mark),
                        desk_model::Run::new(" "),
                    ],
                )?;
                edit.set_note(
                    note,
                    Some(desk_model::Note {
                        id: note,
                        mark: None,
                        paragraphs: vec![paragraph].into(),
                    }),
                )?;
                Ok(Spot::of(edit.document(), after))
            })
        };
        self.finish_edit(before, result);
    }

    /// Sets the text of a note from the text of its row in the Inspector,
    /// after its mark, keeping the formatting of its first paragraph.
    pub fn set_note_text(&self, note: desk_model::NodeId, text: &str) {
        let text = text.to_owned();
        let file_name = self.file_name();
        self.format_edit("Note Text", move |edit| {
            let Some(old) = edit.document().notes().get(note).cloned() else {
                return Ok(());
            };
            let paragraphs = crate::notes_pane::paragraphs(edit, &old, &text, &file_name)?;
            edit.set_note(
                note,
                Some(desk_model::Note {
                    paragraphs: paragraphs.into(),
                    ..old
                }),
            )
        });
    }

    /// Gives a note a mark of its own, or a number again when the mark is
    /// empty (EST-14).
    pub fn set_note_mark(&self, note: desk_model::NodeId, mark: &str) {
        let mark = mark.trim();
        let mark = (!mark.is_empty()).then(|| Arc::<str>::from(mark));
        self.format_edit("Note Mark", move |edit| {
            let Some(old) = edit.document().notes().get(note).cloned() else {
                return Ok(());
            };
            edit.set_note(note, Some(desk_model::Note { mark, ..old }))
        });
    }

    /// Links the selection to the place its text names, or puts a new
    /// link at the cursor, selected, to name its place in the Inspector
    /// (EST-25).
    pub fn insert_link(&self) {
        let selection = self.selection();
        let (start, end) = selection.ordered();
        if start != end {
            let text = self.selected_text();
            let names: Vec<Arc<str>> = bookmarks(&self.state.borrow().editor.snapshot())
                .into_iter()
                .map(|(n, _, _)| n)
                .collect();
            let trimmed = text.trim();
            let link = if names.iter().any(|n| &**n == trimmed) {
                format!("#{trimmed}")
            } else if trimmed.contains(char::is_whitespace) || !trimmed.contains('.') {
                "https://".to_owned()
            } else {
                link_target(trimmed)
            };
            self.format_edit("Insert Link", move |edit| {
                let document = edit.document();
                let from = at_spot(document, start)?;
                let to = at_spot(document, end)?;
                edit.set_link(from, to, Some(link.into()))
            });
            return;
        }
        let before = selection;
        let pending = self.state.borrow_mut().pending.take();
        let text = "https://";
        let result = {
            let mut state = self.state.borrow_mut();
            state.editor.edit("Insert Link", |edit| {
                let at = at_spot(edit.document(), start)?;
                let after = edit.insert_text(at, text)?;
                if let Some(attrs) = pending {
                    edit.set_character_attrs(at, after, &attrs.without_field())?;
                }
                edit.set_link(at, after, Some(text.into()))?;
                Ok(Spot::of(edit.document(), after))
            })
        };
        self.finish_edit(before, result);
        self.select(Selection {
            anchor: start,
            focus: Spot::new(start.paragraph, start.offset + text.len()),
        });
    }

    /// Changes where the linked text at the cursor, or the selection,
    /// links to. An empty place takes the link away.
    pub fn set_link(&self, place: &str) {
        let (start, end) = self.selection().ordered();
        let place = link_target(place);
        let link = (!place.is_empty()).then(|| Arc::<str>::from(place));
        let range = {
            let state = self.state.borrow();
            link_at(state.editor.document(), start)
        };
        let (from, to) = match range {
            Some((_, range))
                if start == end
                    || start.paragraph == end.paragraph
                        && range.start <= start.offset
                        && end.offset <= range.end =>
            {
                (
                    Spot::new(start.paragraph, range.start),
                    Spot::new(start.paragraph, range.end),
                )
            }
            _ if start != end => (start, end),
            _ => return,
        };
        let name = if link.is_some() {
            "Link"
        } else {
            "Remove Link"
        };
        self.format_edit(name, move |edit| {
            let document = edit.document();
            let from = at_spot(document, from)?;
            let to = at_spot(document, to)?;
            edit.set_link(from, to, link)
        });
    }

    /// Follows the link of the text at a spot when the links of the
    /// document are active: a bookmark of the document, or a page in the
    /// browser. Whether there was a link to follow.
    fn follow_link(&self, spot: Spot) -> bool {
        let link = {
            let state = self.state.borrow();
            let document = state.editor.document();
            if document.text_settings().links_inactive {
                return false;
            }
            link_at(document, spot).map(|(l, _)| l)
        };
        match link {
            Some(link) => {
                self.open_link(&link);
                true
            }
            None => false,
        }
    }

    /// A click on an entry of a table of contents (EST-22, EST-23): on its
    /// page number, when those are links, it goes to the heading;
    /// elsewhere it updates the tables and selects the entries of its
    /// level, which are formatted together. Whether the spot was an
    /// entry.
    fn click_toc_entry(&self, spot: Spot) -> bool {
        let (target, style, follow) = {
            let state = self.state.borrow();
            let document = state.editor.document();
            let Some(paragraph) = document.body().get(spot.paragraph) else {
                return false;
            };
            let Some(entry) = paragraph.attrs().toc_entry.flatten() else {
                return false;
            };
            let settings = document.text_settings();
            let on_number = paragraph
                .text()
                .rfind('\t')
                .is_some_and(|tab| spot.offset > tab);
            let follow = settings.toc.page_links && !settings.links_inactive && on_number;
            (entry.target, paragraph.style(), follow)
        };
        let heading = |this: &Self| {
            let state = this.state.borrow();
            let document = state.editor.document();
            target.and_then(|t| document.body().index_of(t))
        };
        if follow && let Some(index) = heading(self) {
            self.move_to(Spot::new(index, 0), false, false);
            return true;
        }
        self.update_tables_of_contents();
        let selections: Vec<Selection> = {
            let state = self.state.borrow();
            let document = state.editor.document();
            let body = document.body();
            // The entry again, which updating may have made anew.
            let index = (0..body.len())
                .find(|&i| {
                    body.get(i).is_some_and(|p| {
                        p.style() == style
                            && p.attrs().toc_entry.flatten().map(|e| e.target) == Some(target)
                    })
                })
                .unwrap_or(spot.paragraph.min(body.len().saturating_sub(1)));
            document
                .entries_of_level(index)
                .into_iter()
                .map(|i| Selection {
                    anchor: Spot::new(i, 0),
                    focus: Spot::new(i, body.get(i).map_or(0, |p| p.len())),
                })
                .collect()
        };
        let Some((first, rest)) = selections.split_first() else {
            return true;
        };
        self.state.borrow_mut().typing = false;
        self.select(*first);
        self.state.borrow_mut().others = rest.to_vec();
        self.select(*first);
        true
    }

    /// Whether a change of the text from one spot to another would change
    /// a table of contents, which only updating it changes; the bell
    /// rings then (EST-22). Nothing changes the body of a page layout
    /// document (INT-03).
    fn toc_refuses(&self, start: Spot, end: Spot) -> bool {
        if self.story().is_none() && self.is_page_layout() {
            // A page layout document has no body text to edit (INT-03).
            return true;
        }
        let refused = {
            let state = self.state.borrow();
            let document = state.editor.document();
            std::iter::once((start, end))
                .chain(state.others.iter().map(|o| o.ordered()))
                .any(|(a, b)| {
                    document.toc_protects((a.paragraph, a.offset), (b.paragraph, b.offset))
                })
        };
        if refused {
            self.view.area().error_bell();
        }
        refused
    }

    /// Opens a link: a bookmark of the document, or a page in the browser.
    pub fn open_link(&self, link: &str) {
        if let Some(name) = link.strip_prefix('#') {
            self.go_to_bookmark(name);
            return;
        }
        // A link of a presentation does nothing in a document (IOP-32).
        if let crate::links::Target::Slide(_) = crate::links::Target::parse(link) {
            self.view.area().error_bell();
            return;
        }
        // Another document opens in the app, at its bookmark.
        if let crate::links::Target::Document { .. } = crate::links::Target::parse(link) {
            if let Some(root) = self.view.area().root() {
                let _ = root.activate_action("win.open-linked", Some(&link.to_variant()));
            }
            return;
        }
        let window = self.view.area().root().and_downcast::<gtk::Window>();
        gtk::UriLauncher::new(link).launch(
            window.as_ref(),
            gtk::gio::Cancellable::NONE,
            |result| {
                if let Err(error) = result {
                    glib::g_warning!("desk", "cannot open the link: {error}");
                }
            },
        );
    }

    /// Makes the selection, or the word at the cursor, a bookmark named by
    /// its text (EST-26).
    pub fn insert_bookmark(&self) {
        let (mut start, mut end) = self.selection().ordered();
        if start == end {
            let state = self.state.borrow();
            let Some(paragraph) = state.editor.document().body().get(start.paragraph) else {
                return;
            };
            let text = paragraph.text();
            let Some(word) = desk_lang::words(&text)
                .into_iter()
                .find(|r| r.start <= start.offset && start.offset <= r.end)
            else {
                return;
            };
            start.offset = word.start;
            end.offset = word.end;
        }
        let text: String = {
            let state = self.state.borrow();
            let body = state.editor.document().body();
            let paragraph = body
                .get(start.paragraph)
                .map(|p| p.text())
                .unwrap_or_default();
            let to = if end.paragraph == start.paragraph {
                end.offset
            } else {
                paragraph.len()
            };
            paragraph
                .get(start.offset..to)
                .unwrap_or_default()
                .to_owned()
        };
        let base: String = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|c| !c.is_control() && *c != FIELD_CHARACTER)
            .take(40)
            .collect();
        let base = if base.is_empty() {
            tr("Bookmark")
        } else {
            base
        };
        let names: Vec<Arc<str>> = bookmarks(&self.state.borrow().editor.snapshot())
            .into_iter()
            .map(|(n, _, _)| n)
            .collect();
        let mut name = base.clone();
        let mut n = 2;
        while names.iter().any(|b| **b == *name) {
            name = format!("{base} {n}");
            n += 1;
        }
        self.format_edit("Insert Bookmark", move |edit| {
            let document = edit.document();
            let from = at_spot(document, start)?;
            let to = at_spot(document, end)?;
            edit.set_bookmark(from, to, Some(name.into()))
        });
    }

    /// Selects the text of a bookmark.
    pub fn go_to_bookmark(&self, name: &str) -> bool {
        let found = bookmarks(&self.state.borrow().editor.snapshot())
            .into_iter()
            .find(|(n, _, _)| &**n == name);
        match found {
            Some((_, start, end)) => {
                self.move_to(start, false, false);
                self.select(Selection {
                    anchor: start,
                    focus: end,
                });
                true
            }
            None => false,
        }
    }

    /// Renames a bookmark, and the links to it; an empty name takes the
    /// bookmark away, and leaves its links.
    pub fn rename_bookmark(&self, old: &str, new: &str) {
        let old: Arc<str> = old.into();
        let new = new.trim().trim_start_matches('#').to_owned();
        let name = if new.is_empty() {
            "Remove Bookmark"
        } else {
            "Rename Bookmark"
        };
        self.format_edit(name, move |edit| {
            if !new.is_empty()
                && bookmarks(edit.document())
                    .iter()
                    .any(|(n, _, _)| **n == *new)
            {
                return Ok(());
            }
            let new: Option<Arc<str>> = (!new.is_empty()).then(|| new.into());
            let to_old = format!("#{old}");
            let mut marked = Vec::new();
            let mut linked = Vec::new();
            for paragraph in edit.document().body().iter() {
                let mut offset = 0;
                for run in paragraph.runs() {
                    let at = desk_model::Position::new(paragraph.id(), offset);
                    let end = desk_model::Position::new(paragraph.id(), offset + run.len());
                    if run.attrs.bookmark.clone().flatten().as_deref() == Some(&*old) {
                        marked.push((at, end));
                    }
                    if run.attrs.link.clone().flatten().as_deref() == Some(to_old.as_str()) {
                        linked.push((at, end));
                    }
                    offset += run.len();
                }
            }
            for (at, end) in marked {
                edit.set_bookmark(at, end, new.clone())?;
            }
            if let Some(new) = &new {
                let link: Arc<str> = format!("#{new}").into();
                for (at, end) in linked {
                    edit.set_link(at, end, Some(link.clone()))?;
                }
            }
            Ok(())
        });
    }

    /// Puts a table of contents before the paragraph of the cursor
    /// (EST-22). A document whose tables list nothing yet lists its
    /// headings, by the styles whose names start with "Heading".
    pub fn insert_table_of_contents(&self) {
        if self.story().is_some() {
            // Not in headers, footers and notes, as in Pages.
            return;
        }
        let before = self.selection();
        let (start, _) = before.ordered();
        let result = {
            let mut state = self.state.borrow_mut();
            state.editor.edit("Insert Table of Contents", |edit| {
                if edit.document().text_settings().toc.levels.is_empty() {
                    let mut settings = edit.document().text_settings().clone();
                    settings.toc = default_toc(edit.document());
                    edit.set_text_settings(settings)?;
                }
                edit.add_toc_entry_styles()?;
                edit.insert_table_of_contents(start.paragraph)?;
                Ok(Some(Spot::new(start.paragraph, 0)))
            })
        };
        self.finish_edit(before, result);
    }

    /// Makes the entries of every table of contents again (EST-22).
    pub fn update_tables_of_contents(&self) {
        self.format_edit("Update Table of Contents", |edit| {
            edit.update_tables_of_contents()
        });
    }

    /// Changes what tables of contents list, and updates them (EST-23).
    pub fn set_toc(&self, toc: desk_model::TocSettings) {
        self.format_edit("Table of Contents", move |edit| {
            let mut settings = edit.document().text_settings().clone();
            settings.toc = toc;
            edit.set_text_settings(settings)?;
            edit.update_tables_of_contents()
        });
    }

    /// Moves the cursor after the mark of the next note in the body, or of
    /// the one before (EST-15). The Inspector then shows that note.
    pub fn go_to_note(&self, forward: bool) -> bool {
        let (start, end) = self.selection().ordered();
        let marks = {
            let state = self.state.borrow();
            note_marks(state.editor.document())
        };
        let mark = FIELD_CHARACTER.len_utf8();
        let spot = if forward {
            marks
                .into_iter()
                .find(|&(p, o)| (p, o) >= (end.paragraph, end.offset))
        } else {
            marks
                .into_iter()
                .rev()
                .find(|&(p, o)| (p, o + mark) < (start.paragraph, start.offset))
        };
        match spot {
            Some((paragraph, offset)) => {
                self.move_to(Spot::new(paragraph, offset + mark), false, false);
                true
            }
            None => false,
        }
    }

    /// Sets the text of a header or a footer from the text of its row in
    /// the Inspector, as [`crate::headers_pane`] writes it.
    pub fn set_header_footer_text(&self, place: MarginPlace, text: &str) {
        let file_name = self.file_name();
        let text = text.to_owned();
        let (start, _) = self.selection().ordered();
        self.format_edit("Header and Footer", move |edit| {
            // Those of the section at the cursor (EST-16).
            let section = edit.document().headers_section(start.paragraph);
            let old = edit
                .document()
                .section_headers_footers(section)
                .get(place)
                .to_vec();
            let paragraphs = crate::headers_pane::paragraphs(edit, &text, &old, &file_name)?;
            edit.set_section_header_footer(section, place, paragraphs)
        });
    }

    /// Puts the numbers of pages in the headers or footers, as Insert >
    /// Auto Page Numbers (EST-27). `text` is the line, where `{page}` and
    /// `{pages}` are the number of the page and of pages.
    pub fn set_auto_page_numbers(&self, numbers: desk_model::AutoPageNumbers, text: &str) {
        let file_name = self.file_name();
        let text = text.to_owned();
        let (start, _) = self.selection().ordered();
        self.format_edit("Auto Page Numbers", move |edit| {
            let at = start.paragraph;
            let section = edit.document().headers_section(at);
            let place = MarginPlace::new(numbers.kind, desk_model::MarginPages::Default);
            let old = edit
                .document()
                .section_headers_footers(section)
                .get(place)
                .to_vec();
            let mut line = crate::headers_pane::paragraphs(edit, &text, &old, &file_name)?;
            let Some(line) = line.pop() else {
                return Ok(());
            };
            edit.set_auto_page_numbers(at, numbers, &line)
        });
    }

    /// Changes the layout that the cursor is in, as one action of the
    /// history.
    pub fn set_layout(&self, name: &str, layout: Layout) {
        let (start, _) = self.selection().ordered();
        self.format_edit(name, move |edit| {
            let document = edit.document();
            match document.layout_at(start.paragraph) {
                (None, _) => {
                    let setup = PageSetup {
                        layout,
                        ..*document.page_setup()
                    };
                    edit.set_page_setup(setup)
                }
                (Some(index), _) => {
                    let paragraph = document.body().get(index).expect("the paragraph is there");
                    let held = paragraph.attrs().start.clone().flatten();
                    let start = match held.as_deref() {
                        Some(Start::Section(section, _)) => Start::Section(*section, layout),
                        _ => Start::Layout(layout),
                    };
                    let at = desk_model::Position::new(paragraph.id(), 0);
                    edit.set_paragraph_attrs(
                        at,
                        at,
                        &ParaAttrs {
                            start: Some(Some(Arc::new(start))),
                            ..ParaAttrs::default()
                        },
                    )
                }
            }
        });
    }

    /// Changes the section that the cursor is in, as one action of the
    /// history.
    pub fn set_section(&self, name: &str, section: Section) {
        let (start, _) = self.selection().ordered();
        self.format_edit(name, move |edit| {
            let document = edit.document();
            match document.section_at(start.paragraph) {
                (None, _) => {
                    let setup = PageSetup {
                        section,
                        ..*document.page_setup()
                    };
                    edit.set_page_setup(setup)
                }
                (Some(index), _) => {
                    let paragraph = document.body().get(index).expect("the paragraph is there");
                    let layout = *paragraph
                        .attrs()
                        .start
                        .clone()
                        .flatten()
                        .expect("a section starts there")
                        .layout();
                    let at = desk_model::Position::new(paragraph.id(), 0);
                    edit.set_paragraph_attrs(
                        at,
                        at,
                        &ParaAttrs {
                            start: Some(Some(Arc::new(Start::Section(section, layout)))),
                            ..ParaAttrs::default()
                        },
                    )
                }
            }
        });
    }

    /// Changes the page setup, as one action of the history.
    pub fn set_page_setup(&self, name: &str, setup: PageSetup) {
        self.format_edit(name, move |edit| edit.set_page_setup(setup));
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
        if !self.sync_story() {
            let mut state = self.state.borrow_mut();
            state.layout = crate::lay_out(state.editor.document());
        }
        if self.story().is_some_and(|story| !self.story_exists(story)) {
            self.leave_story(None);
            return;
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
            // The headers, footers and notes not being edited too, and
            // the body while one of them is.
            let story = state.story.as_ref().map(|m| m.story);
            if let Some(mode) = &state.story {
                let main = mode.main.document();
                for (index, paragraph) in main.body().iter().enumerate() {
                    for mistake in crate::spelling::mistakes(
                        main.styles(),
                        paragraph,
                        preferences.check_spelling,
                        preferences.proofread,
                    ) {
                        let selection = Selection {
                            anchor: Spot::new(index, mistake.range.start),
                            focus: Spot::new(index, mistake.range.end),
                        };
                        let proofreading =
                            matches!(mistake.kind, crate::spelling::MistakeKind::Proofreading(_));
                        for span in text_nav::selection_rects(&mode.main_layout, &selection) {
                            squiggles.push(crate::view::Squiggle { span, proofreading });
                        }
                    }
                }
            }
            squiggles.extend(stories::story_squiggles(
                Self::main_document(&state),
                Self::main_layout(&state),
                story,
                preferences.check_spelling,
                preferences.proofread,
            ));
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
    /// paragraph. With text selected that is not a mistake, it looks only
    /// in that text, from its start, and goes on through it each time it
    /// is asked again (TXT-10).
    pub fn next_mistake(
        &self,
        spelling: bool,
        proofreading: bool,
    ) -> Option<(crate::spelling::Mistake, String)> {
        let (start, end) = self.selection().ordered();
        let scope = if start != end && self.selected_mistake(spelling, proofreading).is_none() {
            Some((start, end, start))
        } else {
            let scope = self.state.borrow().spelling_scope;
            scope
                .filter(|&(from, to)| from <= start && end <= to)
                .map(|(from, to)| (from, to, end))
        };
        self.state.borrow_mut().spelling_scope = scope.map(|(from, to, _)| (from, to));
        if let Some((_, to, after)) = scope {
            for index in after.paragraph..=to.paragraph {
                let found = self
                    .mistakes_of(index, spelling, proofreading)
                    .into_iter()
                    .find(|m| {
                        (index > after.paragraph || m.range.start >= after.offset)
                            && (index < to.paragraph || m.range.end <= to.offset)
                    });
                if let Some(mistake) = found {
                    self.select_mistake(index, &mistake);
                    return Some((mistake, self.paragraph_text(index)));
                }
            }
            self.state.borrow_mut().spelling_scope = None;
            return None;
        }
        // From the selection to the end of its text, then through the
        // headers, footers and notes, and round to the selection.
        let contexts: Vec<Option<Story>> = std::iter::once(None)
            .chain(
                stories::story_pages(Self::main_layout(&self.state.borrow()))
                    .into_iter()
                    .map(|(story, _)| Some(story)),
            )
            .collect();
        let here = self.story();
        let rank = contexts.iter().position(|c| *c == here).unwrap_or(0);
        let count = contexts.len();
        for step in 0..=count {
            let context = contexts[(rank + step) % count];
            let (styles, paragraphs) = {
                let state = self.state.borrow();
                let document = Self::main_document(&state);
                let paragraphs = match context {
                    None => document.body().iter().cloned().collect(),
                    Some(story) => document.story_paragraphs(story),
                };
                (document.styles().clone(), paragraphs)
            };
            for (index, paragraph) in paragraphs.iter().enumerate() {
                let found = crate::spelling::mistakes(&styles, paragraph, spelling, proofreading)
                    .into_iter()
                    .find(|m| {
                        let at = (index, m.range.start);
                        if step == 0 {
                            at >= (end.paragraph, end.offset)
                        } else if step == count {
                            at < (end.paragraph, end.offset)
                        } else {
                            true
                        }
                    });
                if let Some(mistake) = found {
                    self.go_to_story(context);
                    self.select_mistake(index, &mistake);
                    return Some((mistake, paragraph.text()));
                }
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
                    story: None,
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
            (
                DisplayList::pages(Self::main_layout(&state)),
                invisibles,
                text,
            )
        };
        self.view.set_book(self.facing_pages());
        self.view.set_pages(pages);
        self.view.set_invisibles(invisibles);
        self.show_outlines();
        self.show_guides();
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
            setup: *document.page_setup(),
            layout: document.layout_at(start.paragraph).1,
            section: document.section_at(start.paragraph).1,
            first_section: document.section_at(start.paragraph).0.is_none(),
            headers_section: document.headers_section(start.paragraph),
            headers: document
                .section_headers_footers(document.headers_section(start.paragraph))
                .clone(),
            note: note_at(document, start),
            link: link_at(document, start).map(|(link, _)| link),
            bookmarks: bookmarks(document)
                .into_iter()
                .map(|(name, _, _)| name)
                .collect(),
            object: self.object_info(),
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

    /// The character styles of the document, by name.
    pub fn character_styles(&self) -> Vec<(StyleId, String)> {
        let state = self.state.borrow();
        let mut styles: Vec<_> = state
            .editor
            .document()
            .styles()
            .iter()
            .filter(|s| s.family() == StyleFamily::Character)
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
        if self.style_selected_objects(
            "Paragraph Style",
            desk_model::TextStyling::Paragraph {
                style,
                clean: false,
            },
        ) {
            return;
        }
        self.format_ranges("Paragraph Style", |edit, from, to| {
            edit.apply_paragraph_style(from, to, style)
        });
    }

    /// Gives the paragraphs of the selection a paragraph style and drops
    /// the formatting and character styles set over it, as a double click
    /// on the style or a style shortcut with Shift does (STY-04, STY-06).
    pub fn set_paragraph_style_clean(&self, style: StyleId) {
        if self.style_selected_objects(
            "Paragraph Style",
            desk_model::TextStyling::Paragraph { style, clean: true },
        ) {
            return;
        }
        self.format_ranges("Paragraph Style", |edit, from, to| {
            edit.set_paragraph_style_clean(from, to, style)
        });
    }

    /// Gives the selected text a character style, or none.
    pub fn set_character_style(&self, style: Option<StyleId>) {
        if self.style_selected_objects(
            "Character Style",
            desk_model::TextStyling::Character {
                style,
                clean: false,
            },
        ) {
            return;
        }
        self.format_ranges("Character Style", |edit, from, to| {
            edit.set_character_style(from, to, style)
        });
    }

    /// Gives all the text of the selected objects a style, when objects
    /// rather than text are selected, as one action (STY-02). Whether
    /// they were.
    fn style_selected_objects(&self, name: &str, styling: desk_model::TextStyling) -> bool {
        if self.story().is_some() || !self.has_selected_objects() {
            return false;
        }
        self.arrange_objects(name, |edit, ids| {
            for &id in ids {
                edit.style_object_text(id, styling)?;
            }
            Ok(None)
        });
        true
    }

    /// Gives the selection a style of either family; with `clean`, the
    /// formatting set over it goes too.
    pub fn apply_style(&self, style: StyleId, clean: bool) {
        let family = self.styles().get(style).map(desk_model::Style::family);
        match family {
            Some(StyleFamily::Paragraph) if clean => self.set_paragraph_style_clean(style),
            Some(StyleFamily::Paragraph) => self.set_paragraph_style(style),
            Some(StyleFamily::Character) => {
                if clean
                    && !self.style_selected_objects(
                        "Character Style",
                        desk_model::TextStyling::Character {
                            style: Some(style),
                            clean: true,
                        },
                    )
                {
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

    /// Everything that a search finds in the document, and in its
    /// headers, footers and notes (TXT-12, TXT-13).
    pub fn find(&self, query: &desk_model::Query) -> Vec<desk_model::Found> {
        let stories = self.all_stories();
        let state = self.state.borrow();
        Self::main_document(&state).find_everywhere(query, &stories)
    }

    /// The headers, footers, notes, text boxes and shapes with text that
    /// the pages show.
    pub fn all_stories(&self) -> Vec<Story> {
        stories::story_pages(Self::main_layout(&self.state.borrow()))
            .into_iter()
            .map(|(story, _)| story)
            .collect()
    }

    /// Whether any text uses a style: in the body, or in a header, a
    /// footer, a note, a text box or a shape (STY-09).
    pub fn style_used(&self, style: StyleId) -> bool {
        let stories = self.all_stories();
        let state = self.state.borrow();
        let document = Self::main_document(&state);
        !document.style_uses(style).is_empty()
            || stories.iter().any(|&story| {
                !document
                    .with_body(document.story_paragraphs(story))
                    .style_uses(style)
                    .is_empty()
            })
    }

    /// Selects a piece of text that a search found, and shows it, in the
    /// header, footer or note it is in.
    pub fn select_found(&self, found: &desk_model::Found) {
        self.go_to_story(found.story);
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
        let layout = Self::main_layout(&state);
        match found.story {
            Some(story) => stories::story_pages(layout)
                .into_iter()
                .find(|(s, _)| *s == story)
                .map(|(_, page)| page),
            None => {
                let spot = Spot {
                    paragraph: found.index,
                    offset: found.range.start,
                };
                text_nav::caret(layout, spot).map(|c| c.page)
            }
        }
    }

    /// Replaces a piece of text that a search found, as one action of the
    /// history, and puts the cursor after it.
    pub fn replace_found(&self, found: &desk_model::Found, with: &str) {
        self.go_to_story(found.story);
        let before = self.selection();
        let result = self.state.borrow_mut().editor.edit("Replace", |edit| {
            let after = edit.replace_found(found, with)?;
            Ok(Spot::of(edit.document(), after))
        });
        self.finish_edit(before, result);
    }

    /// Replaces everything a search finds, in the body and in the
    /// headers, footers and notes, as one action of the history, and
    /// gives how many pieces were replaced.
    pub fn replace_all(&self, query: &desk_model::Query, with: &str) -> usize {
        self.leave_story(None);
        let before = self.selection();
        let stories: Vec<Story> = stories::story_pages(&self.state.borrow().layout)
            .into_iter()
            .map(|(story, _)| story)
            .collect();
        let mut count = 0;
        let result = self.state.borrow_mut().editor.edit("Replace All", |edit| {
            count = edit.replace_everywhere(query, with, &stories)?;
            let last = edit.document().body().len().saturating_sub(1);
            let (start, _) = before.ordered();
            let paragraph = start.paragraph.min(last);
            let length = edit.document().body().get(paragraph).map_or(0, |p| p.len());
            Ok(Some(Spot::new(paragraph, start.offset.min(length))))
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
        self.show_objects();
        if let Some(caret) = caret.filter(|_| !self.has_selected_objects()) {
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
        let alt = modifiers.contains(gdk::ModifierType::ALT_MASK);
        match key {
            // In a list, Alt+Shift+Right and Alt+Shift+Left change the
            // level of the item alone, as Option-Tab does in Pages:
            // Alt+Tab belongs to GNOME (LST-02).
            gdk::Key::Right | gdk::Key::KP_Right | gdk::Key::Left | gdk::Key::KP_Left
                if alt
                    && shift
                    && !control
                    && self.change_list_level(
                        if matches!(key, gdk::Key::Left | gdk::Key::KP_Left) {
                            -1
                        } else {
                            1
                        },
                        false,
                    ) => {}
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
                // Alt, as Option in Pages, goes to the edge of the
                // paragraph too (TXT-03).
                if control || modifiers.contains(gdk::ModifierType::ALT_MASK) {
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
                // item and the items under it (LST-02).
                let by = if shift { -1 } else { 1 };
                if !self.change_list_level(by, true) {
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
            // Esc takes the cursor from a header, footer or note back to
            // the body.
            if this.pen_key(key) || this.points_key(key) {
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Escape && this.instant_alpha_on() {
                this.stop_instant_alpha();
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Escape && this.story().is_some() {
                this.leave_story(None);
                return glib::Propagation::Stop;
            }
            if this.object_key(key, modifiers) || this.handle_key(key, modifiers) {
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
            // Typing with objects selected does not reach the text.
            if this.has_selected_objects() {
                return;
            }
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
            // While Instant Alpha is on, a click on the picture takes out
            // the color under it (OBJ-06).
            if this.press_alpha(x, y)
                || this.press_pen(presses, x, y)
                || (this.editing_points()
                    && this.press_points(presses, x, y, gesture.current_event_state()))
            {
                this.state.borrow_mut().drag_from = None;
                return;
            }
            // A click on an object selects it, or on a handle of one
            // starts resizing it (OBJ-33).
            if this.press_object(presses, x, y, gesture.current_event_state()) {
                this.state.borrow_mut().drag_from = None;
                return;
            }
            // A click in a header, footer or note puts the cursor there,
            // and a click in the body takes it back (EST-12, EST-13).
            if !gesture
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK)
            {
                this.press_story(x, y);
            }
            let Some(spot) = this.spot_at(x, y) else {
                return;
            };
            // A double click on the mark of a note goes to the note, and
            // on the mark at the start of the note back to the body.
            if presses == 2 && this.double_click_mark(spot) {
                let mut state = this.state.borrow_mut();
                state.drag_from = None;
                return;
            }
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
            // A press on the label of a list item selects the item, and a
            // drag of it moves the item to another level (LST-02).
            if presses == 1
                && !shift
                && !control
                && let Some((page, px, py)) = this.view.page_at(x, y)
                && let Some(paragraph) =
                    text_nav::label_at(&this.state.borrow().layout, page, px, py)
            {
                let selection = text_nav::paragraph_at(
                    &this.state.borrow().editor.snapshot(),
                    Spot::new(paragraph, 0),
                );
                this.select(selection);
                let mut state = this.state.borrow_mut();
                state.typing = false;
                state.drag_from = None;
                state.label_drag = Some((px, 0));
                return;
            }
            // A click on a link follows it, unless the document's links
            // are inactive (EST-25).
            if presses == 1
                && !shift
                && !control
                && (this.click_toc_entry(spot) || this.follow_link(spot))
            {
                let mut state = this.state.borrow_mut();
                state.adding = false;
                state.drag_from = None;
                return;
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
            if this.drag_alpha(dx, dy)
                || this.drag_pen(dx, dy)
                || this.drag_points(x + dx, y + dy)
                || this.drag_connection(x + dx, y + dy)
            {
                return;
            }
            if this.drag_guide(x + dx, y + dy) {
                this.state.borrow_mut().drag_from = None;
                return;
            }
            if this.drag_marquee(x + dx, y + dy) {
                this.state.borrow_mut().drag_from = None;
                return;
            }
            if this.dragging_objects() {
                this.drag_objects(x + dx, y + dy, gesture.current_event_state());
                return;
            }
            let label_drag = this.state.borrow().label_drag;
            if let Some((from, moved)) = label_drag {
                // A level for each step of the indent of a level.
                const STEP: f64 = 18.0;
                if let Some((_, px, _)) = this.view.page_at(x + dx, y) {
                    let levels = ((px - from) / STEP).trunc() as i32;
                    if levels != moved {
                        this.change_list_level(levels - moved, true);
                        this.state.borrow_mut().label_drag = Some((from, levels));
                    }
                }
                return;
            }
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
            this.end_object_drag();
            this.end_marquee();
            this.end_guide_drag();
            this.end_alpha_drag();
            this.end_pen_drag();
            this.end_points_drag();
            this.end_connection_drag();
            let mut state = this.state.borrow_mut();
            state.drag_from = None;
            state.label_drag = None;
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

/// The bytes of the clipboard as `media_type`.
async fn clipboard_bytes(
    clipboard: &gdk::Clipboard,
    media_type: &str,
) -> Result<glib::Bytes, String> {
    let (stream, _) = clipboard
        .read_future(&[media_type], glib::Priority::DEFAULT)
        .await
        .map_err(|e| e.to_string())?;
    let out = gtk::gio::MemoryOutputStream::new_resizable();
    out.splice_future(
        &stream,
        gtk::gio::OutputStreamSpliceFlags::CLOSE_SOURCE
            | gtk::gio::OutputStreamSpliceFlags::CLOSE_TARGET,
        glib::Priority::DEFAULT,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(out.steal_as_bytes())
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

    #[test]
    fn txt_07_typed_addresses_become_links() {
        fn found(text: &str) -> Option<(&str, String)> {
            typed_address(text).map(|(r, t)| (&text[r], t))
        }
        assert_eq!(
            found("See www.gnome.org"),
            Some(("www.gnome.org", "https://www.gnome.org".into()))
        );
        assert_eq!(
            found("(https://a.b/c),"),
            Some(("https://a.b/c", "https://a.b/c".into()))
        );
        assert_eq!(
            found("Mail me@example.com."),
            Some(("me@example.com", "mailto:me@example.com".into()))
        );
        assert_eq!(
            found("Olá “www.x.pt”"),
            Some(("www.x.pt", "https://www.x.pt".into()))
        );
        for text in ["www.", "http://", "a@b", "@b.c", "gnome.org", "e-mail", ""] {
            assert_eq!(found(text), None, "{text}");
        }
        // In the text, the address before the cursor takes the link,
        // unless it links somewhere already.
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Typing", |edit| {
                let end = edit.insert_text(start, "Go to www.gnome.org")?;
                detect_link(edit, end)
            })
            .unwrap();
        let document = editor.document();
        let (link, range) = link_at(document, Spot::new(0, 8)).unwrap();
        assert_eq!(&*link, "https://www.gnome.org");
        assert_eq!(range, 6..19);
        assert!(link_at(document, Spot::new(0, 2)).is_none());
        editor
            .edit("Typing", |edit| {
                let id = edit.document().body().get(0).unwrap().id();
                let at = desk_model::Position::new(id, 19);
                edit.set_link(start, at, Some(Arc::from("#Elsewhere")))?;
                detect_link(edit, at)
            })
            .unwrap();
        let (link, _) = link_at(editor.document(), Spot::new(0, 8)).unwrap();
        assert_eq!(&*link, "#Elsewhere");
    }

    #[test]
    fn est_25_links_name_their_kind_of_place() {
        assert_eq!(link_target("example.com"), "https://example.com");
        assert_eq!(link_target(" http://a.b/c "), "http://a.b/c");
        assert_eq!(link_target("me@example.com"), "mailto:me@example.com");
        assert_eq!(link_target("#Intro"), "#Intro");
        assert_eq!(link_target(""), "");
    }

    #[test]
    fn est_25_est_26_links_and_bookmarks_are_found_in_the_text() {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Text", |edit| {
                let id = start.paragraph;
                edit.insert_text(start, "See the site now")?;
                let at = |o| desk_model::Position::new(id, o);
                edit.set_link(at(4), at(8), Some("https://a.b".into()))?;
                edit.set_link(at(8), at(13), Some("https://a.b".into()))?;
                edit.set_bookmark(at(0), at(3), Some("See".into()))
            })
            .unwrap();
        let document = editor.document();
        let (link, range) = link_at(document, Spot::new(0, 10)).unwrap();
        assert_eq!(&*link, "https://a.b");
        assert_eq!(range, 4..13);
        // At the end of a link, the link is still the one before.
        assert_eq!(link_at(document, Spot::new(0, 13)).unwrap().1, 4..13);
        assert!(link_at(document, Spot::new(0, 2)).is_none());
        let marks = bookmarks(document);
        assert_eq!(marks.len(), 1);
        assert_eq!(&*marks[0].0, "See");
        assert_eq!((marks[0].1, marks[0].2), (Spot::new(0, 0), Spot::new(0, 3)));
    }

    #[test]
    fn est_23_a_new_table_lists_the_headings() {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Styles", |edit| {
                for name in ["Heading 2", "Heading 1", "TOC Heading 1", "Quote"] {
                    edit.new_paragraph_style(name, start, start, false)?;
                }
                Ok(())
            })
            .unwrap();
        let document = editor.document();
        let styles = document.styles();
        let id = |name| styles.by_name(StyleFamily::Paragraph, name).unwrap().id();
        let toc = default_toc(document);
        let levels: Vec<_> = toc.levels.iter().map(|l| (l.style, l.entry)).collect();
        assert_eq!(
            levels,
            [
                (id("Heading 1"), id("TOC Heading 1")),
                (id("Heading 2"), styles.default_paragraph()),
            ]
        );
    }
}
