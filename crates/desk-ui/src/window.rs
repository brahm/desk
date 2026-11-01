// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A document window: the header bar, the document area, and the zoom
//! menu and page count at the bottom, as in Pages.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::geometry;
use crate::view::DocumentView;
use crate::{AppInfo, Opener};

/// A window and the document it shows, if any.
pub struct DocumentWindow {
    window: adw::ApplicationWindow,
    title: adw::WindowTitle,
    stack: gtk::Stack,
    view: DocumentView,
    zoom_button: gtk::MenuButton,
    pages_label: gtk::Label,
    warnings_button: gtk::Button,
    path: RefCell<Option<PathBuf>>,
    warnings: RefCell<Vec<String>>,
}

thread_local! {
    /// The open windows, so that a file opened from outside goes to an
    /// empty window when there is one.
    static WINDOWS: RefCell<Vec<Rc<DocumentWindow>>> = const { RefCell::new(Vec::new()) };
}

impl DocumentWindow {
    /// A new, empty window.
    pub fn new(app: &adw::Application, info: AppInfo, opener: Option<&'static Opener>) -> Rc<Self> {
        let menu = gio::Menu::new();
        let file = gio::Menu::new();
        if opener.is_some() {
            file.append(Some("_Open…"), Some("app.open"));
        }
        file.append(Some("_New Window"), Some("app.new-window"));
        menu.append_section(None, &file);
        let help = gio::Menu::new();
        help.append(Some(&format!("_About {}", info.name)), Some("app.about"));
        menu.append_section(None, &help);

        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .primary(true)
            .tooltip_text("Main Menu")
            .build();

        let title = adw::WindowTitle::new(info.name, "");
        let header = adw::HeaderBar::builder().title_widget(&title).build();
        if opener.is_some() {
            let open = gtk::Button::builder()
                .label("_Open")
                .use_underline(true)
                .action_name("app.open")
                .tooltip_text("Open a Document")
                .build();
            header.pack_start(&open);
        }
        header.pack_end(&menu_button);
        let warnings_button = gtk::Button::builder()
            .icon_name("dialog-warning-symbolic")
            .tooltip_text("Document Warnings")
            .action_name("win.warnings")
            .visible(false)
            .build();
        header.pack_end(&warnings_button);

        let status = adw::StatusPage::builder()
            .icon_name(info.id)
            .title(info.name)
            .build();
        if opener.is_some() {
            status.set_description(Some(
                "Open an OpenDocument text or a Pages document to read it.",
            ));
            let open = gtk::Button::builder()
                .label("_Open a Document…")
                .use_underline(true)
                .action_name("app.open")
                .halign(gtk::Align::Center)
                .css_classes(["pill", "suggested-action"])
                .build();
            status.set_child(Some(&open));
        } else {
            status.set_description(Some(
                "This app is at an early stage of development and cannot open documents yet.",
            ));
        }

        let view = DocumentView::new();
        let stack = gtk::Stack::new();
        stack.add_named(&status, Some("empty"));
        stack.add_named(view.widget(), Some("document"));

        let zoom_button = gtk::MenuButton::builder()
            .label(geometry::zoom_label(geometry::DEFAULT_ZOOM))
            .menu_model(&zoom_menu())
            .direction(gtk::ArrowType::Up)
            .tooltip_text("Zoom")
            .css_classes(["flat"])
            .build();
        let pages_label = gtk::Label::builder()
            .css_classes(["dim-label", "caption"])
            .hexpand(true)
            .xalign(1.0)
            .build();
        let bottom = gtk::Box::builder()
            .spacing(6)
            .margin_start(6)
            .margin_end(12)
            .margin_top(3)
            .margin_bottom(3)
            .build();
        bottom.append(&zoom_button);
        bottom.append(&pages_label);
        let bottom_bar = gtk::ActionBar::new();
        bottom_bar.set_center_widget(None::<&gtk::Widget>);
        bottom_bar.pack_start(&bottom);
        bottom_bar.set_revealed(false);

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&stack));
        toolbar_view.add_bottom_bar(&bottom_bar);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(info.name)
            .default_width(900)
            .default_height(700)
            .content(&toolbar_view)
            .build();

        let this = Rc::new(Self {
            window,
            title,
            stack,
            view,
            zoom_button,
            pages_label,
            warnings_button,
            path: RefCell::new(None),
            warnings: RefCell::new(Vec::new()),
        });
        this.add_actions();
        {
            let label = this.zoom_button.downgrade();
            this.view.connect_zoom_changed(move |zoom| {
                if let Some(label) = label.upgrade() {
                    label.set_label(&geometry::zoom_label(zoom));
                }
            });
        }
        {
            let bar = bottom_bar.downgrade();
            this.stack.connect_visible_child_name_notify(move |stack| {
                if let Some(bar) = bar.upgrade() {
                    bar.set_revealed(stack.visible_child_name().as_deref() == Some("document"));
                }
            });
        }
        WINDOWS.with(|w| w.borrow_mut().push(this.clone()));
        let weak = Rc::downgrade(&this);
        this.window.connect_destroy(move |_| {
            WINDOWS.with(|w| {
                w.borrow_mut()
                    .retain(|o| weak.upgrade().is_none_or(|me| !Rc::ptr_eq(o, &me)));
            });
        });
        this
    }

    /// The GTK window.
    pub fn window(&self) -> &adw::ApplicationWindow {
        &self.window
    }

    /// Whether the window shows no document.
    pub fn is_empty(&self) -> bool {
        self.path.borrow().is_none()
    }

    /// An open window that shows no document, if any.
    pub fn find_empty() -> Option<Rc<Self>> {
        WINDOWS.with(|w| w.borrow().iter().find(|w| w.is_empty()).cloned())
    }

    /// Opens a document in this window, or says why it cannot.
    pub fn load(&self, opener: &Opener, path: &Path) {
        match crate::open_document(opener, path) {
            Ok(shown) => {
                self.view.set_pages(shown.pages);
                let name = display_name(path);
                self.title.set_title(&name);
                self.title.set_subtitle(&folder_of(path));
                self.window.set_title(Some(&name));
                let count = self.view.page_count();
                self.pages_label.set_label(&if count == 1 {
                    "1 page".to_owned()
                } else {
                    format!("{count} pages")
                });
                self.warnings_button.set_visible(!shown.warnings.is_empty());
                *self.warnings.borrow_mut() = shown.warnings.clone();
                *self.path.borrow_mut() = Some(path.to_owned());
                self.stack.set_visible_child_name("document");
                remember_recent(path);
                if shown.lossy {
                    self.show_warnings();
                }
            }
            Err(message) => {
                let dialog = adw::AlertDialog::new(
                    Some(&format!("Cannot open {}", display_name(path))),
                    Some(&message),
                );
                dialog.add_response("close", "_Close");
                dialog.present(Some(&self.window));
            }
        }
    }

    fn show_warnings(&self) {
        let name = self
            .path
            .borrow()
            .as_deref()
            .map(display_name)
            .unwrap_or_default();
        crate::warnings::show(&self.window, &name, &self.warnings.borrow());
    }

    fn add_actions(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let warnings = gio::SimpleAction::new("warnings", None);
        warnings.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.show_warnings();
            }
        });
        let weak = Rc::downgrade(self);
        let zoom = gio::SimpleAction::new("zoom", Some(glib::VariantTy::STRING));
        zoom.connect_activate(move |_, parameter| {
            let (Some(this), Some(value)) =
                (weak.upgrade(), parameter.and_then(|p| p.get::<String>()))
            else {
                return;
            };
            let view = &this.view;
            let zoom = match value.as_str() {
                "in" => geometry::zoom_in(view.zoom()),
                "out" => geometry::zoom_out(view.zoom()),
                "fit-width" => view.fit_width_zoom(),
                "fit-page" => view.fit_page_zoom(),
                level => level.parse().unwrap_or(geometry::DEFAULT_ZOOM),
            };
            view.set_zoom(zoom);
        });
        self.window.add_action(&warnings);
        self.window.add_action(&zoom);
    }
}

/// The zoom menu at the bottom of the window (INT-07).
fn zoom_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let levels = gio::Menu::new();
    for level in geometry::ZOOM_LEVELS {
        let item = gio::MenuItem::new(Some(&geometry::zoom_label(level)), None);
        item.set_action_and_target_value(Some("win.zoom"), Some(&level.to_string().to_variant()));
        levels.append_item(&item);
    }
    menu.append_section(None, &levels);
    let fit = gio::Menu::new();
    for (label, target) in [("Fit _Width", "fit-width"), ("Fit _Page", "fit-page")] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.zoom"), Some(&target.to_variant()));
        fit.append_item(&item);
    }
    menu.append_section(None, &fit);
    menu
}

/// The name of a document as the title of its window. A package of Pages
/// is a folder; its name is the document's.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The folder of a document, with the home folder as `~`.
fn folder_of(path: &Path) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    let home = glib::home_dir();
    match parent.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => parent.display().to_string(),
    }
}

/// Adds a document to the recent files of the desktop (DOC-03).
fn remember_recent(path: &Path) {
    let file = gio::File::for_path(path);
    gtk::RecentManager::default().add_item(&file.uri());
}
