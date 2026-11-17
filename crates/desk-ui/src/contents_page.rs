// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The table of contents in the navigation sidebar of Desk Writer (W011),
//! as the sidebar of Pages 15.4 has it: the headings of the document, one
//! row each, indented by their level. A click on a heading goes to it.
//! Edit chooses, in the Inspector, the paragraph styles that it lists,
//! which are the ones the tables of contents in the text list; Insert puts
//! a table of contents in the text at the cursor.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use desk_model::Document;
use gtk::glib;

use crate::formatting::EditorOf;
use crate::i18n::tr;

/// One heading of the document, as the sidebar lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    /// Its level, from 0: the rank of its style among the styles listed
    /// that the document uses.
    pub level: usize,
    /// Its text, without the characters of fields and objects.
    pub text: String,
    /// The index of its paragraph in the body.
    pub paragraph: usize,
}

/// The headings of a document: the paragraphs of the body whose style the
/// tables of contents list, or, when the document chose none yet, the
/// styles whose names start with "Heading" (EST-23). The entries of the
/// tables of contents themselves and empty paragraphs are left out.
pub fn headings(document: &Document) -> Vec<Heading> {
    let chosen = &document.text_settings().toc;
    let toc = if chosen.levels.is_empty() {
        crate::editing::default_toc(document)
    } else {
        chosen.clone()
    };
    let mut found = document
        .body()
        .iter()
        .enumerate()
        .filter(|(_, p)| p.attrs().toc_entry.flatten().is_none())
        .filter_map(|(index, paragraph)| {
            let level = toc
                .levels
                .iter()
                .position(|l| l.style == paragraph.style())?;
            let text: String = paragraph
                .text()
                .chars()
                .map(|c| if c == '\t' || c == '\u{2028}' { ' ' } else { c })
                .filter(|c| {
                    !c.is_control() && !('\u{e000}'..='\u{f8ff}').contains(c) && *c != '\u{fffc}'
                })
                .collect();
            let text = text.trim();
            (!text.is_empty()).then(|| Heading {
                level,
                text: text.to_owned(),
                paragraph: index,
            })
        })
        .collect::<Vec<_>>();
    // The levels count only the styles that the document uses, so that a
    // list of ten styles with two in use indents by one step.
    let mut used: Vec<usize> = found.iter().map(|h| h.level).collect();
    used.sort_unstable();
    used.dedup();
    for heading in &mut found {
        heading.level = used.binary_search(&heading.level).unwrap_or(0);
    }
    found
}

/// The page of the sidebar.
pub struct ContentsPage {
    root: gtk::Box,
    list: gtk::ListBox,
    empty: adw::StatusPage,
    editor: EditorOf,
    /// The paragraphs of the rows, in order.
    paragraphs: std::cell::RefCell<Vec<usize>>,
    shown: Cell<bool>,
    pending: Cell<bool>,
}

impl ContentsPage {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["navigation-sidebar"])
            .build();
        list.update_property(&[gtk::accessible::Property::Label(&tr("Table of Contents"))]);
        let empty = adw::StatusPage::builder()
            .icon_name("view-list-symbolic")
            .title(tr("No Headings"))
            .description(tr("Paragraphs with a heading style show here."))
            .css_classes(["compact"])
            .vexpand(true)
            .build();
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        let edit = gtk::Button::builder()
            .label(tr("_Edit"))
            .use_underline(true)
            .action_name("win.edit-toc")
            .tooltip_text(tr("Choose the styles that the table of contents lists"))
            .build();
        let insert = gtk::Button::builder()
            .label(tr("_Insert"))
            .use_underline(true)
            .action_name("win.insert-toc")
            .tooltip_text(tr("Insert a table of contents at the cursor"))
            .build();
        let buttons = gtk::Box::builder()
            .spacing(6)
            .homogeneous(true)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        buttons.append(&edit);
        buttons.append(&insert);
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        root.append(&scrolled);
        root.append(&empty);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&buttons);
        let this = Rc::new(Self {
            root,
            list,
            empty,
            editor,
            paragraphs: std::cell::RefCell::new(Vec::new()),
            shown: Cell::new(false),
            pending: Cell::new(false),
        });
        let weak = Rc::downgrade(&this);
        this.list.connect_row_activated(move |_, row| {
            let Some(this) = weak.upgrade() else { return };
            let paragraph = usize::try_from(row.index())
                .ok()
                .and_then(|i| this.paragraphs.borrow().get(i).copied());
            if let (Some(editor), Some(paragraph)) = ((this.editor)(), paragraph) {
                editor.go_to_paragraph(paragraph);
            }
        });
        this.rebuild();
        this
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Shows or hides it; it is made only while it shows.
    pub fn set_shown(self: &Rc<Self>, shown: bool) {
        self.shown.set(shown);
        if shown {
            self.rebuild();
        }
    }

    /// Makes it again soon, after the document changed.
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

    fn rebuild(&self) {
        self.list.remove_all();
        let found = (self.editor)().map_or_else(Vec::new, |e| headings(&e.document()));
        for heading in &found {
            let label = gtk::Label::builder()
                .label(&heading.text)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .tooltip_text(&heading.text)
                .margin_start(12 * i32::try_from(heading.level.min(5)).unwrap_or(0))
                .build();
            if heading.level == 0 {
                label.add_css_class("heading");
            }
            self.list.append(&label);
        }
        *self.paragraphs.borrow_mut() = found.iter().map(|h| h.paragraph).collect();
        self.list.set_visible(!found.is_empty());
        self.empty.set_visible(found.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{Editor, TocLevel};

    #[test]
    fn w011_the_sidebar_lists_the_headings_by_level() {
        let mut editor = Editor::new(Document::new_word_processing());
        let start = editor.document().start();
        editor
            .edit("Setup", |edit| {
                // Chapter, text, Part, an empty heading.
                let end = edit.insert_text(start, "Chapter")?;
                let second = edit.split_paragraph(end)?;
                let end = edit.insert_text(second, "Some text")?;
                let third = edit.split_paragraph(end)?;
                let end = edit.insert_text(third, "Part\tone")?;
                let fourth = edit.split_paragraph(end)?;
                edit.new_paragraph_style("Heading 1", start, start, true)?;
                let two = edit.new_paragraph_style("Heading 2", third, third, true)?;
                edit.set_paragraph_style(fourth, fourth, two)?;
                Ok(())
            })
            .unwrap();
        let found = headings(editor.document());
        assert_eq!(
            found,
            [
                Heading {
                    level: 0,
                    text: "Chapter".into(),
                    paragraph: 0
                },
                Heading {
                    level: 1,
                    text: "Part one".into(),
                    paragraph: 2
                },
            ]
        );

        // The styles the document chose for its tables of contents win.
        editor
            .edit("Choose", |edit| {
                let styles = edit.document().styles();
                let two = styles
                    .by_name(desk_model::StyleFamily::Paragraph, "Heading 2")
                    .unwrap()
                    .id();
                let mut settings = edit.document().text_settings().clone();
                settings.toc.levels = vec![TocLevel {
                    style: two,
                    entry: styles.default_paragraph(),
                    page_number: true,
                }];
                edit.set_text_settings(settings)
            })
            .unwrap();
        let found = headings(editor.document());
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].level, found[0].paragraph), (0, 2));
    }
}
