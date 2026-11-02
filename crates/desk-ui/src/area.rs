// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The widget the pages are drawn on, which gives screen readers the text
//! of the document, the cursor and the selection through the accessible
//! text interface of GTK.

use std::cell::{Cell, RefCell};

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::access::{self, AccessText, Unit};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DocumentArea {
        pub text: RefCell<AccessText>,
        pub caret: Cell<u32>,
        pub selection: Cell<Option<(u32, u32)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DocumentArea {
        const NAME: &'static str = "DeskDocumentArea";
        type Type = super::DocumentArea;
        type ParentType = gtk::DrawingArea;
        type Interfaces = (gtk::AccessibleText,);

        fn class_init(class: &mut Self::Class) {
            class.set_accessible_role(gtk::AccessibleRole::TextBox);
        }
    }

    impl ObjectImpl for DocumentArea {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().update_property(&[
                gtk::accessible::Property::MultiLine(true),
                gtk::accessible::Property::Label(&crate::i18n::tr("Document")),
            ]);
        }
    }

    impl WidgetImpl for DocumentArea {}
    impl DrawingAreaImpl for DocumentArea {}

    impl AccessibleTextImpl for DocumentArea {
        fn contents(&self, start: u32, end: u32) -> Option<glib::Bytes> {
            let text = self.text.borrow();
            Some(glib::Bytes::from(text.slice(start, end).as_bytes()))
        }

        fn contents_at(
            &self,
            offset: u32,
            granularity: gtk::AccessibleTextGranularity,
        ) -> Option<(u32, u32, glib::Bytes)> {
            let unit = match granularity {
                gtk::AccessibleTextGranularity::Word => Unit::Word,
                gtk::AccessibleTextGranularity::Sentence => Unit::Sentence,
                gtk::AccessibleTextGranularity::Line => Unit::Line,
                gtk::AccessibleTextGranularity::Paragraph => Unit::Paragraph,
                _ => Unit::Character,
            };
            let text = self.text.borrow();
            let (start, end) = text.range_at(offset, unit);
            Some((
                start,
                end,
                glib::Bytes::from(text.slice(start, end).as_bytes()),
            ))
        }

        fn caret_position(&self) -> u32 {
            self.caret.get()
        }

        fn selection(&self) -> Vec<gtk::AccessibleTextRange> {
            self.selection
                .get()
                .map(|(start, end)| {
                    gtk::AccessibleTextRange::new(start as usize, (end - start) as usize)
                })
                .into_iter()
                .collect()
        }

        fn attributes(
            &self,
            _offset: u32,
        ) -> Vec<(gtk::AccessibleTextRange, glib::GString, glib::GString)> {
            Vec::new()
        }

        fn default_attributes(&self) -> Vec<(glib::GString, glib::GString)> {
            Vec::new()
        }
    }
}

glib::wrapper! {
    /// The widget the pages are drawn on.
    pub struct DocumentArea(ObjectSubclass<imp::DocumentArea>)
        @extends gtk::DrawingArea, gtk::Widget,
        @implements gtk::Accessible, gtk::AccessibleText, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for DocumentArea {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl DocumentArea {
    /// Gives screen readers the text as it is now, and tells them what
    /// changed.
    pub fn set_text(&self, text: AccessText) {
        let imp = self.imp();
        let (at, removed, inserted) = access::change(imp.text.borrow().text(), text.text());
        imp.text.replace(text);
        if removed > 0 {
            self.update_contents(gtk::AccessibleTextContentChange::Remove, at, at + removed);
        }
        if inserted > 0 {
            self.update_contents(gtk::AccessibleTextContentChange::Insert, at, at + inserted);
        }
    }

    /// Gives screen readers the cursor and the selection, as character
    /// offsets of the text.
    pub fn set_caret(&self, caret: u32, selection: Option<(u32, u32)>) {
        let imp = self.imp();
        if imp.caret.replace(caret) != caret {
            self.update_caret_position();
        }
        if imp.selection.replace(selection) != selection {
            self.update_selection_bound();
        }
    }
}
