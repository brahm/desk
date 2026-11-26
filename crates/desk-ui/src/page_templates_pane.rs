// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Page templates in the Format pane (W095, PXV-20).
//!
//! In a page layout document, the Section tab shows the Page Template of
//! the page of the selection, or of the page in sight, which another one
//! replaces and Reapply puts back; the Arrange tab of the selected
//! objects makes them placeholders of text or media, with the tag that
//! carries their content to the placeholder of the same tag of another
//! template.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use desk_model::PlaceholderKind;

use crate::compact;
use crate::editing::Format;
use crate::formatting::EditorOf;
use crate::i18n::{n_, tr};
use crate::tab_stops::string_list;

/// The kinds of placeholder of the Placeholder row, in its order.
const PLACEHOLDERS: [(Option<PlaceholderKind>, &str); 3] = [
    (None, n_("None")),
    (Some(PlaceholderKind::Text), n_("Text")),
    (Some(PlaceholderKind::Media), n_("Media")),
];

/// The Page Template group of the Section tab.
pub struct TemplateGroup {
    group: compact::Group,
    template: compact::ComboRow,
    reapply: gtk::Button,
    /// The names in the list, after None.
    names: RefCell<Vec<Arc<str>>>,
    updating: Cell<bool>,
    editor: EditorOf,
}

impl TemplateGroup {
    /// The group for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let template = compact::ComboRow::builder()
            .title(tr("Page Template"))
            .subtitle(tr("The template of the page of the selection or in sight"))
            .build();
        let reapply = gtk::Button::builder()
            .label(tr("Reapply"))
            .tooltip_text(tr(
                "Put back the objects of the template, keeping the content of its placeholders",
            ))
            .valign(gtk::Align::Center)
            .build();
        let templates = gtk::Button::builder()
            .label(tr("Templates…"))
            .action_name("win.page-templates")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let group = compact::Group::builder()
            .title(tr("Page Template"))
            .header_suffix(&templates)
            .build();
        let row = compact::ActionRow::builder()
            .title(tr("Reapply Page Template"))
            .build();
        row.add_suffix(&reapply);
        group.add(&template);
        group.add(&row);
        group.set_visible(false);
        let this = Rc::new(Self {
            group,
            template,
            reapply,
            names: RefCell::new(Vec::new()),
            updating: Cell::new(false),
            editor,
        });
        let weak = Rc::downgrade(&this);
        this.template.connect_selected_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let chosen = (row.selected() as usize)
                .checked_sub(1)
                .and_then(|i| this.names.borrow().get(i).cloned());
            if let (Some(name), Some(editor)) = (chosen, (this.editor)()) {
                editor.apply_page_template(&name);
            }
        });
        let weak = Rc::downgrade(&this);
        this.reapply.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade()
                && let Some(editor) = (this.editor)()
            {
                editor.reapply_page_template();
            }
        });
        this
    }

    /// The group.
    pub fn widget(&self) -> &compact::Group {
        &self.group
    }

    /// Shows the templates of a page layout document, and hides the group
    /// in another.
    pub fn show(&self, format: &Format) {
        let Some(crate::editing::PageTemplatesAt { names, current }) = &format.page_templates
        else {
            self.group.set_visible(false);
            return;
        };
        self.updating.set(true);
        self.group.set_visible(true);
        if *self.names.borrow() != *names {
            let list = std::iter::once(tr("None")).chain(names.iter().map(|n| n.to_string()));
            self.template.set_model(Some(&string_list(list)));
            *self.names.borrow_mut() = names.clone();
        }
        let at = current
            .as_ref()
            .and_then(|c| names.iter().position(|n| n == c))
            .map_or(0, |i| i as u32 + 1);
        if self.template.selected() != at {
            self.template.set_selected(at);
        }
        self.template.set_sensitive(!names.is_empty());
        self.reapply.set_sensitive(current.is_some());
        self.updating.set(false);
    }
}

/// The Placeholder group of the Arrange tab.
pub struct PlaceholderGroup {
    group: compact::Group,
    kind: compact::ComboRow,
    tag: compact::EntryRow,
    updating: Cell<bool>,
    editor: EditorOf,
}

impl PlaceholderGroup {
    /// The group for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let kind = compact::ComboRow::builder()
            .title(tr("Placeholder"))
            .subtitle(tr(
                "Content that a page keeps when its template is applied again",
            ))
            .model(&string_list(PLACEHOLDERS.iter().map(|(_, name)| tr(name))))
            .build();
        let tag = compact::EntryRow::builder().title(tr("Tag")).build();
        tag.set_subtitle(&tr(
            "Carries the content to the placeholder of the same tag of another template",
        ));
        let group = compact::Group::builder().title(tr("Placeholder")).build();
        group.add(&kind);
        group.add(&tag);
        group.set_visible(false);
        let this = Rc::new(Self {
            group,
            kind,
            tag,
            updating: Cell::new(false),
            editor,
        });
        let weak = Rc::downgrade(&this);
        this.kind.connect_selected_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.changed();
            }
        });
        let weak = Rc::downgrade(&this);
        this.tag.connect_apply(move |_| {
            if let Some(this) = weak.upgrade()
                && !this.updating.get()
            {
                this.changed();
            }
        });
        this
    }

    fn changed(&self) {
        let kind = PLACEHOLDERS
            .get(self.kind.selected() as usize)
            .and_then(|(kind, _)| *kind);
        self.tag.set_sensitive(kind.is_some());
        if let Some(editor) = (self.editor)() {
            editor.set_placeholder(kind, &self.tag.text());
        }
    }

    /// The group.
    pub fn widget(&self) -> &compact::Group {
        &self.group
    }

    /// Shows the placeholder of the first selected object of a page
    /// layout document, and hides the group otherwise.
    pub fn show(&self, format: &Format) {
        let object = format
            .object
            .as_ref()
            .filter(|_| format.page_templates.is_some());
        let Some(info) = object else {
            self.group.set_visible(false);
            return;
        };
        self.updating.set(true);
        self.group.set_visible(true);
        let placeholder = info.object.placeholder.as_ref();
        let at = PLACEHOLDERS
            .iter()
            .position(|(kind, _)| *kind == placeholder.map(|p| p.kind))
            .unwrap_or(0) as u32;
        if self.kind.selected() != at {
            self.kind.set_selected(at);
        }
        let tag = placeholder
            .and_then(|p| p.tag.as_deref())
            .unwrap_or_default();
        if self.tag.text() != tag {
            self.tag.set_text(tag);
        }
        self.tag.set_sensitive(placeholder.is_some());
        self.updating.set(false);
    }
}
