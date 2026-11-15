// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The navigation sidebar of the window of interface B (GNM-12): a pane
//! as high as the window, at the start, with a header bar of its own. The
//! content button in the middle of that header bar chooses what the pane
//! shows, and the Add button at its end adds a page, a sheet or a slide.
//!
//! The three apps share it. Desk Writer shows four pages in it, each an
//! entry of the content button: the page thumbnails, the styles that were
//! the Styles drawer of Pages (INT-18), the comments, and the search
//! results that were its search sidebar (TXT-13).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::formatting;
use crate::i18n::tr;

/// The width of the pane, in pixels, as in the mockups of interface B.
pub const WIDTH: f64 = 196.0;

/// What is called when another page shows.
type PageChanged = Rc<dyn Fn(&str)>;

/// The navigation sidebar of a window.
pub struct NavSidebar {
    root: adw::ToolbarView,
    stack: gtk::Stack,
    content_button: gtk::MenuButton,
    /// The entries of the content button.
    pages_menu: gio::Menu,
    /// The names, titles and widths of the pages, for the label of the
    /// button and the width of the pane.
    titles: RefCell<Vec<(String, String, f64)>>,
    /// Called with the name of the page shown when another shows.
    changed: RefCell<Vec<PageChanged>>,
    /// `sidebar.page`, whose state is the name of the page shown.
    page_action: gio::SimpleAction,
}

impl NavSidebar {
    /// An empty sidebar whose Add button has `add_label` as its tooltip
    /// and accessible name, and `add_menu` as its menu.
    pub fn new(add_label: &str, add_menu: &gio::MenuModel) -> Rc<Self> {
        let pages_menu = gio::Menu::new();
        let content_button = gtk::MenuButton::builder()
            .menu_model(&pages_menu)
            .tooltip_text(tr("Sidebar Content"))
            .css_classes(["flat"])
            .build();
        // Its name is the page it shows, and its tooltip says what it does.
        content_button.set_always_show_arrow(true);
        let add_button = gtk::MenuButton::builder()
            .icon_name("list-add-symbolic")
            .menu_model(add_menu)
            .tooltip_text(add_label)
            .build();
        formatting::labelled(&add_button, add_label);
        let header = adw::HeaderBar::builder()
            .title_widget(&content_button)
            .build();
        header.pack_end(&add_button);
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .vhomogeneous(false)
            .build();
        let root = adw::ToolbarView::new();
        root.add_top_bar(&header);
        root.set_content(Some(&stack));

        let page_action = gio::SimpleAction::new_stateful(
            "page",
            Some(glib::VariantTy::STRING),
            &"".to_variant(),
        );
        let group = gio::SimpleActionGroup::new();
        group.add_action(&page_action);
        root.insert_action_group("sidebar", Some(&group));

        let this = Rc::new(Self {
            root,
            stack,
            content_button,
            pages_menu,
            titles: RefCell::new(Vec::new()),
            changed: RefCell::new(Vec::new()),
            page_action,
        });
        let weak = Rc::downgrade(&this);
        this.page_action.connect_change_state(move |_, value| {
            if let (Some(this), Some(name)) =
                (weak.upgrade(), value.and_then(|v| v.get::<String>()))
            {
                this.set_page(&name);
            }
        });
        this
    }

    /// The widget, the sidebar of the split view of the window.
    pub fn widget(&self) -> &adw::ToolbarView {
        &self.root
    }

    /// Adds a page called `name`, which the content button lists as
    /// `title`, and which needs the pane `width` pixels wide, no less than
    /// [`WIDTH`]. The first page added is the one shown.
    pub fn add_page(&self, name: &str, title: &str, width: f64, child: &impl IsA<gtk::Widget>) {
        self.stack.add_named(child, Some(name));
        let item = gio::MenuItem::new(Some(title), None);
        item.set_action_and_target_value(Some("sidebar.page"), Some(&name.to_variant()));
        self.pages_menu.append_item(&item);
        let first = self.titles.borrow().is_empty();
        self.titles
            .borrow_mut()
            .push((name.to_owned(), title.to_owned(), width.max(WIDTH)));
        if first {
            self.set_page(name);
        }
    }

    /// Shows the page called `name`, if there is one.
    pub fn set_page(&self, name: &str) {
        let title = self
            .titles
            .borrow()
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, t, _)| t.clone());
        let Some(title) = title else { return };
        let changed = self.page() != name;
        self.stack.set_visible_child_name(name);
        self.content_button.set_label(&title);
        self.page_action.set_state(&name.to_variant());
        if changed {
            let callbacks = self.changed.borrow().clone();
            for f in callbacks {
                f(name);
            }
        }
    }

    /// The name of the page shown.
    pub fn page(&self) -> String {
        self.stack
            .visible_child_name()
            .map(|n| n.to_string())
            .unwrap_or_default()
    }

    /// The width that the page called `name` needs.
    pub fn width(&self, name: &str) -> f64 {
        self.titles
            .borrow()
            .iter()
            .find(|(n, _, _)| n == name)
            .map_or(WIDTH, |(_, _, w)| *w)
    }

    /// Calls `f` with the name of the page shown when another shows.
    pub fn connect_page_changed(&self, f: impl Fn(&str) + 'static) {
        self.changed.borrow_mut().push(Rc::new(f));
    }
}
