// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Page templates of page layout documents (W095, PXV-20).
//!
//! As in Pages 15.4, a page of a page layout document can come from a
//! page template: the objects of the template are copied onto it, each
//! remembering the object of the template it copies. Some of them are
//! placeholders, for text or for media, which take the content of the
//! page; a tag carries that content to the placeholder of the same tag
//! when the page takes another template, and Reapply Page Template puts
//! the objects of the template back, keeping the content of its
//! placeholders.

use std::sync::Arc;

use crate::document::Document;
use crate::error::ModelError;
use crate::id::NodeId;
use crate::object::{Anchor, Object, ObjectKind};
use crate::op::Op;
use crate::transaction::Edit;

/// A page template: a name, and the objects of a page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageTemplate {
    /// The name it goes by.
    pub name: Arc<str>,
    /// Its objects, from back to front, on page 0.
    pub objects: Vec<Object>,
}

/// What a placeholder holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceholderKind {
    /// Text: a text box or a shape with text.
    Text,
    /// A picture, a movie or a sound.
    Media,
}

/// That an object is a placeholder (W095): Define as Placeholder Text
/// or as Media Placeholder of Pages 15.4.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Placeholder {
    /// What it holds.
    pub kind: PlaceholderKind,
    /// Its tag, which carries its content to the placeholder of the same
    /// tag of another template.
    pub tag: Option<Arc<str>>,
}

/// The page templates of a document, and the template of each page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageTemplates {
    /// The templates, in the order of their list.
    pub templates: imbl::Vector<PageTemplate>,
    /// The name of the template of each page, by index; none for a page
    /// that has none. Pages after the end have none.
    pub pages: imbl::Vector<Option<Arc<str>>>,
}

impl PageTemplates {
    /// A template by name.
    pub fn get(&self, name: &str) -> Option<&PageTemplate> {
        self.templates.iter().find(|t| &*t.name == name)
    }

    /// The name of the template of a page.
    pub fn of_page(&self, page: u32) -> Option<&Arc<str>> {
        self.pages.get(page as usize)?.as_ref()
    }

    /// The template of each page, as long as the pages are.
    pub(crate) fn fitted(&self, count: u32) -> imbl::Vector<Option<Arc<str>>> {
        let mut pages = self.pages.clone();
        while pages.len() < count as usize {
            pages.push_back(None);
        }
        pages.truncate(count as usize);
        pages
    }
}

impl Document {
    /// The page templates and the template of each page (W095).
    pub fn page_templates(&self) -> &PageTemplates {
        let crate::document::Container::Pages(flow) = &self.container;
        &flow.page_templates
    }

    /// The template of a page, when it has one that exists.
    pub fn page_template_of(&self, page: u32) -> Option<&PageTemplate> {
        let templates = self.page_templates();
        templates.get(templates.of_page(page)?)
    }
}

/// The content that a placeholder holds, to carry to another one.
#[derive(Clone)]
enum Content {
    Text(Arc<[crate::Paragraph]>),
    Media(ObjectKind),
}

fn content(object: &Object) -> Option<Content> {
    let placeholder = object.placeholder.as_ref()?;
    match placeholder.kind {
        PlaceholderKind::Text => object
            .text_box()
            .map(|t| Content::Text(t.paragraphs.clone())),
        PlaceholderKind::Media => match &object.kind {
            ObjectKind::Image(_) | ObjectKind::Media(_) => {
                Some(Content::Media(object.kind.clone()))
            }
            _ => None,
        },
    }
}

fn fill(object: &mut Object, content: Content) {
    match content {
        Content::Text(paragraphs) => {
            if let Some(text) = object.text_box_mut() {
                text.paragraphs = paragraphs;
            }
        }
        Content::Media(kind) => {
            if matches!(object.kind, ObjectKind::Image(_) | ObjectKind::Media(_)) {
                object.kind = kind;
            }
        }
    }
}

impl Edit {
    /// Sets the page templates and the template of each page.
    pub fn set_page_templates(&mut self, templates: PageTemplates) -> Result<(), ModelError> {
        if *self.document().page_templates() == templates {
            return Ok(());
        }
        self.apply(Op::SetPageTemplates { templates })
    }

    /// Changes the template of each page with `f`, as long as the pages.
    pub(crate) fn change_page_template_pages(
        &mut self,
        f: impl FnOnce(&mut imbl::Vector<Option<Arc<str>>>),
    ) -> Result<(), ModelError> {
        let mut templates = self.document().page_templates().clone();
        if templates.templates.is_empty() && templates.pages.iter().all(Option::is_none) {
            return Ok(());
        }
        let count = self.document().page_setup().layout_pages.unwrap_or(0);
        let mut pages = templates.fitted(count);
        f(&mut pages);
        templates.pages = pages;
        self.set_page_templates(templates)
    }

    /// Adds a page template, or replaces the one of the same name, made
    /// of copies of the objects of a page: Create Page Template From This
    /// Page (W095). The objects of the page stay as they are.
    pub fn add_page_template_from_page(&mut self, page: u32, name: &str) -> Result<(), ModelError> {
        let mut objects: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| o.anchor == Anchor::Page(page))
            .cloned()
            .collect();
        for object in &mut objects {
            object.id = self.new_id();
            object.anchor = Anchor::Page(0);
            object.template_object = None;
            object.comments = crate::CommentIds::none();
            object.moved = None;
            if let ObjectKind::TextBox(text) = &mut object.kind {
                // A template's boxes are not linked to the page's.
                text.next = None;
            }
        }
        self.put_page_template(PageTemplate {
            name: name.into(),
            objects,
        })
    }

    /// Adds a page template, or replaces the one of the same name.
    pub fn put_page_template(&mut self, template: PageTemplate) -> Result<(), ModelError> {
        let mut templates = self.document().page_templates().clone();
        match templates
            .templates
            .iter_mut()
            .find(|t| t.name == template.name)
        {
            Some(old) => *old = template,
            None => templates.templates.push_back(template),
        }
        self.set_page_templates(templates)
    }

    /// Takes a page template away; its pages keep their objects and have
    /// no template.
    pub fn remove_page_template(&mut self, name: &str) -> Result<(), ModelError> {
        let mut templates = self.document().page_templates().clone();
        templates.templates.retain(|t| &*t.name != name);
        for page in templates.pages.iter_mut() {
            if page.as_deref() == Some(name) {
                *page = None;
            }
        }
        self.set_page_templates(templates)
    }

    /// Adds a page from a template after page `after` of a page layout
    /// document: Add Page of Pages 15.4 (W095). Gives back its index.
    pub fn add_page_from_template(&mut self, after: u32, name: &str) -> Result<u32, ModelError> {
        if self.document().page_setup().layout_pages.is_none() {
            return Ok(0);
        }
        self.add_layout_page(after)?;
        let page = after + 1;
        self.apply_page_template(page, name)?;
        Ok(page)
    }

    /// Gives a page a template: the objects that came from a template go,
    /// the objects of the new one are copied onto it, and its
    /// placeholders take the content of the old ones, by tag, or by the
    /// object they copy when the template is the same. Applying the
    /// template of the page again is Reapply Page Template.
    pub fn apply_page_template(&mut self, page: u32, name: &str) -> Result<(), ModelError> {
        let Some(template) = self.document().page_templates().get(name).cloned() else {
            return Ok(());
        };
        let same = self.document().page_templates().of_page(page).map(|n| &**n) == Some(name);
        let old: Vec<Object> = self
            .document()
            .objects()
            .iter()
            .filter(|o| o.anchor == Anchor::Page(page) && o.template_object.is_some())
            .cloned()
            .collect();
        // The content of the old placeholders, by the object they copy
        // and by tag.
        let by_object: Vec<(NodeId, Content)> = old
            .iter()
            .filter_map(|o| Some((o.template_object?, content(o)?)))
            .collect();
        let by_tag: Vec<(Arc<str>, Content)> = old
            .iter()
            .filter_map(|o| Some((o.placeholder.as_ref()?.tag.clone()?, content(o)?)))
            .collect();
        // Locked ones too: they are the template's.
        for object in &old {
            if let ObjectKind::TextBox(text) = &object.kind {
                self.unlink_text_box(object.id, text)?;
            }
            self.remove_object(object.id)?;
        }
        let copies: Vec<Object> = template
            .objects
            .iter()
            .map(|t| {
                let mut copy = t.clone();
                copy.anchor = Anchor::Page(page);
                copy.template_object = Some(t.id);
                let carried = same
                    .then(|| by_object.iter().find(|(id, _)| *id == t.id))
                    .flatten()
                    .map(|(_, c)| c.clone())
                    .or_else(|| {
                        let tag = t.placeholder.as_ref()?.tag.as_ref()?;
                        by_tag
                            .iter()
                            .find(|(n, _)| n == tag)
                            .map(|(_, c)| c.clone())
                    });
                if let Some(carried) = carried {
                    fill(&mut copy, carried);
                }
                copy
            })
            .collect();
        self.add_copies(&copies)?;
        let name: Arc<str> = template.name.clone();
        let count = self.document().page_setup().layout_pages.unwrap_or(0);
        let mut templates = self.document().page_templates().clone();
        let mut pages = templates.fitted(count.max(page + 1));
        if let Some(slot) = pages.get_mut(page as usize) {
            *slot = Some(name);
        }
        templates.pages = pages;
        self.set_page_templates(templates)
    }

    /// Makes objects placeholders, or plain objects again (W095).
    pub fn set_placeholders(
        &mut self,
        ids: &[NodeId],
        placeholder: Option<Placeholder>,
    ) -> Result<(), ModelError> {
        for &id in ids {
            let mut object = self.object(id)?.clone();
            if object.placeholder != placeholder {
                object.placeholder.clone_from(&placeholder);
                self.replace_object(object)?;
            }
        }
        Ok(())
    }
}
