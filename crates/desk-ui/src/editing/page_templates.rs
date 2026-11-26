// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The page templates of a page layout document in the window (W095,
//! PXV-20): a page added from one, the template of the page in sight
//! changed or applied again, a template made from a page, and the
//! placeholders of the selected objects with their tags.

use std::sync::Arc;

use desk_model::{Placeholder, PlaceholderKind};

use super::TextEditor;

impl TextEditor {
    /// The names of the page templates of the document, in order.
    pub fn page_template_names(&self) -> Vec<Arc<str>> {
        let state = self.state.borrow();
        Self::main_document(&state)
            .page_templates()
            .templates
            .iter()
            .map(|t| t.name.clone())
            .collect()
    }

    /// The template of the page of the selected object, or of the page
    /// in sight.
    pub fn current_page_template(&self) -> Option<Arc<str>> {
        let page = u32::try_from(self.cursor_page()?).ok()?;
        let state = self.state.borrow();
        Self::main_document(&state)
            .page_templates()
            .of_page(page)
            .cloned()
    }

    fn template_page(&self) -> u32 {
        self.cursor_page()
            .and_then(|p| u32::try_from(p).ok())
            .unwrap_or(0)
    }

    /// Adds a page from a template after the page in sight, and shows it:
    /// Add Page of Pages 15.4.
    pub fn add_page_from_template(&self, name: &str) {
        self.leave_story(None);
        let after = self.template_page();
        let name = name.to_owned();
        self.layout_page_edit("Add Page", |edit| {
            edit.add_page_from_template(after, &name).map(drop)
        });
        self.view.scroll_to_page(after as usize + 1);
    }

    /// Gives the page in sight a template, or the same one again: the
    /// Page Template of the Format pane and Reapply Page Template.
    pub fn apply_page_template(&self, name: &str) {
        self.leave_story(None);
        let page = self.template_page();
        let again = self.current_page_template().as_deref() == Some(name);
        let undo = if again {
            "Reapply Page Template"
        } else {
            "Change Page Template"
        };
        let name = name.to_owned();
        self.layout_page_edit(undo, |edit| edit.apply_page_template(page, &name));
    }

    /// Applies the template of the page in sight again, when it has one.
    pub fn reapply_page_template(&self) {
        if let Some(name) = self.current_page_template() {
            self.apply_page_template(&name);
        }
    }

    /// Makes a page template of the page in sight, or replaces the one
    /// of the name: Create Page Template From This Page.
    pub fn create_page_template(&self, name: &str) {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        self.leave_story(None);
        let page = self.template_page();
        self.layout_page_edit("Create Page Template", |edit| {
            edit.add_page_template_from_page(page, &name)
        });
    }

    /// Takes a page template away; its pages keep their objects.
    pub fn delete_page_template(&self, name: &str) {
        let name = name.to_owned();
        self.layout_page_edit("Delete Page Template", |edit| {
            edit.remove_page_template(&name)
        });
    }

    /// Makes the selected objects placeholders of a kind with a tag, or
    /// plain objects: Define as Placeholder Text and Define as Media
    /// Placeholder, and the Tag.
    pub fn set_placeholder(&self, kind: Option<PlaceholderKind>, tag: &str) {
        let ids = self.selected_objects();
        if ids.is_empty() {
            return;
        }
        let tag = tag.trim();
        let placeholder = kind.map(|kind| Placeholder {
            kind,
            tag: (!tag.is_empty()).then(|| Arc::from(tag)),
        });
        self.format_edit("Define as Placeholder", move |edit| {
            edit.set_placeholders(&ids, placeholder)
        });
    }
}

#[cfg(test)]
mod tests {
    use desk_model::{Anchor, Document, Frame, Object, ObjectKind, Op, PlaceholderKind, Shape};

    use super::TextEditor;

    #[test]
    fn w095_the_window_makes_templates_adds_pages_and_defines_placeholders() {
        crate::with_gtk(|| {
            let mut document = Document::new_page_layout();
            let id = document.new_id();
            let mut shape = Object::new(
                id,
                ObjectKind::Shape(Shape::Rectangle),
                Frame::from_pt(72.0, 72.0, 100.0, 50.0),
            );
            shape.anchor = Anchor::Page(0);
            document
                .apply(Op::InsertObject {
                    index: 0,
                    object: shape,
                })
                .unwrap();
            let view = crate::view::DocumentView::new();
            let editor = TextEditor::new(&view, document);
            editor.select_objects(vec![id]);
            editor.set_placeholder(Some(PlaceholderKind::Media), " picture ");
            let placeholder = editor
                .document()
                .objects()
                .get(id)
                .unwrap()
                .placeholder
                .clone();
            assert_eq!(
                placeholder.map(|p| (p.kind, p.tag.as_deref().map(str::to_owned))),
                Some((PlaceholderKind::Media, Some("picture".to_owned())))
            );

            editor.create_page_template("  Photo  ");
            assert_eq!(
                editor.page_template_names(),
                [std::sync::Arc::from("Photo")]
            );
            editor.add_page_from_template("Photo");
            let document = editor.document();
            assert_eq!(document.page_setup().layout_pages, Some(2));
            assert_eq!(
                document.page_templates().of_page(1).map(|n| &**n),
                Some("Photo")
            );
            assert!(editor.format().page_templates.is_some());

            editor.delete_page_template("Photo");
            assert!(editor.page_template_names().is_empty());
            // Undo brings it back.
            editor.undo();
            assert_eq!(editor.page_template_names().len(), 1);
        });
    }
}
