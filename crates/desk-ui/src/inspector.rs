// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The two levels of the Inspector of interface B (INT-13): Format and
//! Document at the top, then the tabs of what is selected under a title
//! that says what it is, as the plan of the interface draws them.
//!
//! The groups of the panes move between the tabs: the Style tab of a
//! picture and that of a shape show the same Graphic group, so the tabs of
//! Format are made again when the selection becomes another kind of thing.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::ObjectKind;

use crate::compact;
use crate::editing::ObjectInfo;
use crate::i18n::{n_, tr, trc};

/// What is selected, which chooses the tabs of Format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Context {
    Text,
    Image,
    Shape,
    TextBox,
    Movie,
    Sound,
    Group,
    Table,
    Chart,
    /// An equation (W104).
    Equation,
    /// An image gallery (W111).
    Gallery,
    /// Nothing selected in Desk Slides: the slide itself (KINT-13).
    Slide,
}

impl Context {
    /// The kind of the first selected object, or text when none is.
    pub fn of(object: Option<&ObjectInfo>) -> Self {
        match object.map(|info| &info.object.kind) {
            None => Self::Text,
            Some(ObjectKind::Image(_)) => Self::Image,
            Some(ObjectKind::Shape(_)) => Self::Shape,
            Some(ObjectKind::TextBox(_)) => Self::TextBox,
            Some(ObjectKind::Media(media)) if media.is_sound() => Self::Sound,
            Some(ObjectKind::Media(_)) => Self::Movie,
            Some(ObjectKind::Table(_)) => Self::Table,
            Some(ObjectKind::Chart(_)) => Self::Chart,
            Some(ObjectKind::Equation(_)) => Self::Equation,
            Some(ObjectKind::Gallery(_)) => Self::Gallery,
            // A group, or a kind of object that came later.
            Some(_) => Self::Group,
        }
    }

    /// The title over the tabs.
    pub fn title(self) -> String {
        match self {
            Self::Text => tr("Text"),
            Self::Image => tr("Image"),
            Self::Shape => tr("Shape"),
            Self::TextBox => tr("Text Box"),
            Self::Movie => tr("Movie"),
            Self::Sound => tr("Sound"),
            // A noun, where the Arrange menu has the verb.
            Self::Group => trc("inspector title", "Group"),
            Self::Table => tr("Table"),
            Self::Chart => tr("Chart"),
            Self::Equation => tr("Equation"),
            Self::Gallery => tr("Image Gallery"),
            Self::Slide => tr("Slide"),
        }
    }
}

/// A tab: its name, its untranslated title and its groups.
pub struct Tab {
    pub name: &'static str,
    pub title: &'static str,
    pub groups: Vec<compact::Group>,
}

impl Tab {
    pub fn new(name: &'static str, title: &'static str, groups: &[&compact::Group]) -> Self {
        Self {
            name,
            title,
            groups: groups.iter().map(|g| (*g).clone()).collect(),
        }
    }
}

/// A tab as it shows: its page, the box of its groups, the line that says
/// nothing it sets is selected, and its groups.
struct Shown {
    page: gtk::ScrolledWindow,
    content: gtk::Box,
    empty: gtk::Label,
    groups: Vec<compact::Group>,
}

/// The tabs of a level, chosen by an inline switcher that hides when there
/// is only one.
struct Tabs {
    root: gtk::Box,
    switcher: adw::InlineViewSwitcher,
    stack: adw::ViewStack,
    shown: RefCell<Vec<Shown>>,
    /// Whether the tabs are being made again, when the tab shown changes
    /// without anyone choosing it.
    rebuilding: Cell<bool>,
}

impl Tabs {
    fn new() -> Self {
        let stack = adw::ViewStack::builder()
            .vhomogeneous(false)
            .vexpand(true)
            .build();
        let switcher = adw::InlineViewSwitcher::builder()
            .stack(&stack)
            .homogeneous(true)
            .css_classes(["flat", "desk-inspector-tabs"])
            // A title cut short says nothing: the Inspector widens first.
            .can_shrink(false)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .build();
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        root.append(&switcher);
        root.append(&stack);
        Self {
            root,
            switcher,
            stack,
            shown: RefCell::new(Vec::new()),
            rebuilding: Cell::new(false),
        }
    }

    /// Shows `tabs`, with the one named `visible` in front when it is one
    /// of them.
    fn set(&self, tabs: &[Tab], visible: Option<&str>) {
        self.rebuilding.set(true);
        for old in self.shown.borrow_mut().drain(..) {
            while let Some(child) = old.content.first_child() {
                old.content.remove(&child);
            }
            self.stack.remove(&old.page);
        }
        let mut shown = Vec::new();
        for tab in tabs {
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(18)
                .margin_top(12)
                .margin_bottom(12)
                .margin_start(12)
                .margin_end(12)
                .build();
            let empty = gtk::Label::builder()
                .label(tr("Nothing that this tab sets is selected."))
                .wrap(true)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build();
            content.append(&empty);
            for group in &tab.groups {
                content.append(group.widget());
            }
            let page = gtk::ScrolledWindow::builder()
                .child(&content)
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vexpand(true)
                .build();
            self.stack.add_titled(&page, Some(tab.name), &tr(tab.title));
            shown.push(Shown {
                page,
                content,
                empty,
                groups: tab.groups.clone(),
            });
        }
        self.switcher.set_visible(tabs.len() > 1);
        // Tabs of one width only while every title fits in it: four tabs
        // of a table or a chart in 332 pixels take their own widths.
        let longest = tabs
            .iter()
            .map(|tab| tr(tab.title).chars().count())
            .max()
            .unwrap_or(0);
        self.switcher
            .set_homogeneous(longest <= 8 && longest * tabs.len() <= 24);
        let visible = visible
            .filter(|name| tabs.iter().any(|tab| tab.name == *name))
            .or_else(|| tabs.first().map(|tab| tab.name));
        if let Some(name) = visible {
            self.stack.set_visible_child_name(name);
        }
        *self.shown.borrow_mut() = shown;
        self.refresh_empty();
        self.rebuilding.set(false);
    }

    /// Says in each tab whether nothing it sets is selected.
    fn refresh_empty(&self) {
        for shown in self.shown.borrow().iter() {
            let any = shown.groups.iter().any(compact::Group::is_visible);
            shown.empty.set_visible(!any);
        }
    }

    fn visible(&self) -> Option<String> {
        self.stack.visible_child_name().map(|n| n.to_string())
    }

    fn has(&self, name: &str) -> bool {
        self.stack.child_by_name(name).is_some()
    }
}

/// The Inspector's two levels: Format, whose tabs follow the selection,
/// and Document.
pub struct Levels {
    root: gtk::Box,
    level: adw::ToggleGroup,
    title: gtk::Label,
    /// The place between the title and the tabs, where a kind of
    /// selection may put what applies to all its tabs, such as the
    /// paragraph style of text (step 2 of window M+).
    header: gtk::Box,
    headers: RefCell<HashMap<Context, gtk::Widget>>,
    format: Tabs,
    document: Tabs,
    /// The tabs of Format for each kind of selection.
    contexts: RefCell<HashMap<Context, Vec<Tab>>>,
    context: Cell<Option<Context>>,
    /// The tab last chosen for each kind of selection.
    chosen: RefCell<HashMap<Context, String>>,
}

/// The name of the first level of the Inspector that shows the format.
pub const FORMAT: &str = "format";
/// The name of the first level that shows the document.
pub const DOCUMENT: &str = "document";

impl Levels {
    /// The levels, with the tabs of Format for each kind of selection and
    /// those of Document.
    pub fn new(contexts: HashMap<Context, Vec<Tab>>, document: Vec<Tab>) -> Rc<Self> {
        let level = adw::ToggleGroup::builder()
            .homogeneous(true)
            .css_classes(["round"])
            .margin_top(8)
            .margin_bottom(4)
            .margin_start(12)
            .margin_end(12)
            .build();
        for (name, label) in [(FORMAT, n_("Format")), (DOCUMENT, n_("Document"))] {
            level.add(adw::Toggle::builder().name(name).label(tr(label)).build());
        }
        let title = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["heading"])
            .margin_start(14)
            .margin_top(12)
            .margin_bottom(2)
            .build();
        let format = Tabs::new();
        let document_tabs = Tabs::new();
        document_tabs.set(&document, None);
        let format_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .margin_start(12)
            .margin_end(12)
            .margin_top(4)
            .visible(false)
            .build();
        format_box.append(&title);
        format_box.append(&header);
        format_box.append(&format.root);
        let levels = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        levels.add_named(&format_box, Some(FORMAT));
        levels.add_named(&document_tabs.root, Some(DOCUMENT));
        level.set_active_name(Some(FORMAT));
        {
            let levels = levels.downgrade();
            level.connect_active_name_notify(move |level| {
                if let (Some(levels), Some(name)) = (levels.upgrade(), level.active_name()) {
                    levels.set_visible_child_name(&name);
                }
            });
        }
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(332)
            .build();
        root.append(&level);
        root.append(&levels);

        let this = Rc::new(Self {
            root,
            level,
            title,
            header,
            headers: RefCell::new(HashMap::new()),
            format,
            document: document_tabs,
            contexts: RefCell::new(contexts),
            context: Cell::new(None),
            chosen: RefCell::new(HashMap::new()),
        });
        // The tab chosen is kept for the next selection of the same kind.
        let weak = Rc::downgrade(&this);
        this.format
            .stack
            .connect_visible_child_name_notify(move |stack| {
                let Some(this) = weak.upgrade() else { return };
                if this.format.rebuilding.get() {
                    return;
                }
                if let (Some(context), Some(name)) =
                    (this.context.get(), stack.visible_child_name())
                {
                    this.chosen.borrow_mut().insert(context, name.to_string());
                }
            });
        // A tab whose groups all hide says so.
        let groups: Vec<compact::Group> = this
            .contexts
            .borrow()
            .values()
            .flatten()
            .chain(&document)
            .flat_map(|tab| tab.groups.iter().cloned())
            .collect();
        for group in groups {
            let weak = Rc::downgrade(&this);
            group.connect_visible_notify(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.format.refresh_empty();
                    this.document.refresh_empty();
                }
            });
        }
        this.show(Context::Text);
        this
    }

    /// The widget to put at the side of the window.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Shows the tabs of Format for what is selected.
    pub fn show(&self, context: Context) {
        if self.context.get() == Some(context) {
            self.format.refresh_empty();
            return;
        }
        self.context.set(Some(context));
        self.title.set_label(&context.title());
        while let Some(child) = self.header.first_child() {
            self.header.remove(&child);
        }
        let header = self.headers.borrow().get(&context).cloned();
        self.header.set_visible(header.is_some());
        if let Some(header) = header {
            self.header.append(&header);
        }
        let contexts = self.contexts.borrow();
        let tabs = contexts.get(&context).map_or(&[][..], Vec::as_slice);
        let chosen = self.chosen.borrow().get(&context).cloned();
        self.format.set(tabs, chosen.as_deref());
    }

    /// Puts `header` between the title and the tabs while a kind of
    /// selection shows.
    pub fn set_header(&self, context: Context, header: &impl IsA<gtk::Widget>) {
        self.headers
            .borrow_mut()
            .insert(context, header.clone().upcast());
        if self.context.get() == Some(context) {
            self.context.set(None);
            self.show(context);
        }
    }

    /// Adds the tabs of a kind of selection that only an app has, such as
    /// the slide of Desk Slides (KINT-13).
    pub fn add_context(self: &Rc<Self>, context: Context, tabs: Vec<Tab>) {
        for group in tabs.iter().flat_map(|tab| tab.groups.iter()) {
            let weak = Rc::downgrade(self);
            group.connect_visible_notify(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.format.refresh_empty();
                }
            });
        }
        self.contexts.borrow_mut().insert(context, tabs);
    }

    /// Adds a level of an app before Document, such as Animation in
    /// Desk Slides (KINT-14).
    pub fn add_level(&self, name: &str, label: &str, child: &impl IsA<gtk::Widget>) {
        let Some(levels) = self.root.last_child().and_downcast::<gtk::Stack>() else {
            return;
        };
        levels.add_named(child, Some(name));
        let document = self.level.toggle_by_name(DOCUMENT);
        if let Some(document) = &document {
            self.level.remove(document);
        }
        self.level
            .add(adw::Toggle::builder().name(name).label(label).build());
        if let Some(document) = document {
            self.level.add(document);
        }
        // Three titles do not fit in toggles of one width.
        self.level.set_homogeneous(false);
    }

    /// Shows a level: [`FORMAT`] or [`DOCUMENT`].
    pub fn show_level(&self, name: &str) {
        self.level.set_active_name(Some(name));
    }

    /// The level shown.
    pub fn level(&self) -> Option<String> {
        self.level.active_name().map(|n| n.to_string())
    }

    /// Shows a tab of Format, or of Document when Format has none of the
    /// name. Whether there is one.
    pub fn show_tab(&self, name: &str) -> bool {
        for (level, tabs) in [(FORMAT, &self.format), (DOCUMENT, &self.document)] {
            if tabs.has(name) {
                self.show_level(level);
                tabs.stack.set_visible_child_name(name);
                return true;
            }
        }
        false
    }

    /// The tab of Document shown, while Document shows.
    pub fn document_tab(&self) -> Option<String> {
        (self.level().as_deref() == Some(DOCUMENT))
            .then(|| self.document.visible())
            .flatten()
    }

    /// Calls `f` when another level or tab of Document shows.
    pub fn connect_document_shown(&self, f: impl Fn() + 'static) {
        let f = Rc::new(f);
        let g = f.clone();
        self.level.connect_active_name_notify(move |_| g());
        self.document
            .stack
            .connect_visible_child_name_notify(move |_| f());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_13_every_selection_has_a_title() {
        for context in [
            Context::Text,
            Context::Image,
            Context::Shape,
            Context::TextBox,
            Context::Movie,
            Context::Sound,
            Context::Group,
            Context::Table,
            Context::Chart,
            Context::Equation,
            Context::Slide,
        ] {
            assert!(!context.title().is_empty());
        }
        assert_eq!(Context::of(None), Context::Text);
    }
}
