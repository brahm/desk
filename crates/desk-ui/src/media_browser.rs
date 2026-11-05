// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Media Browser of Pages (INT-14): the photos, music and movies in
//! the folders of the user, and in folders added to it, found by name.
//! An item is dragged to the page, or put there with a double click.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::i18n::tr;

/// How deep in a folder the browser looks.
const DEPTH: usize = 4;

/// The most items a kind of media shows.
const MOST: usize = 2_000;

/// How large the pictures of the photos and movies are, in pixels.
const TILE: i32 = 96;

/// A kind of media, a page of the browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Photos,
    Music,
    Movies,
}

impl Kind {
    const ALL: [Self; 3] = [Self::Photos, Self::Music, Self::Movies];

    /// Whether a file is one of the kind, by its extension.
    pub(crate) fn holds(self, path: &Path) -> bool {
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        let extension = extension.to_ascii_lowercase();
        let known: &[&str] = match self {
            Self::Photos => &[
                "png", "jpg", "jpeg", "gif", "tif", "tiff", "bmp", "webp", "pdf",
            ],
            Self::Music => &["mp3", "m4a", "aac", "aif", "aiff", "wav", "ogg", "oga"],
            Self::Movies => &["mov", "mp4", "m4v", "webm", "mkv", "ogv"],
        };
        known.contains(&extension.as_str())
    }

    /// The folder of the user that holds the kind.
    fn folder(self) -> Option<PathBuf> {
        glib::user_special_dir(match self {
            Self::Photos => glib::UserDirectory::Pictures,
            Self::Music => glib::UserDirectory::Music,
            Self::Movies => glib::UserDirectory::Videos,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Photos => "photos",
            Self::Music => "music",
            Self::Movies => "movies",
        }
    }

    fn title(self) -> String {
        match self {
            Self::Photos => tr("Photos"),
            Self::Music => tr("Music"),
            Self::Movies => tr("Movies"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Photos => "image-x-generic-symbolic",
            Self::Music => "audio-x-generic-symbolic",
            Self::Movies => "video-x-generic-symbolic",
        }
    }
}

/// The files of a kind in folders, up to [`DEPTH`] deep and [`MOST`] of
/// them, sorted by name; hidden files and folders are left out.
pub(crate) fn found(kind: Kind, folders: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(kind: Kind, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            if out.len() >= MOST {
                return;
            }
            let hidden = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_none_or(|n| n.starts_with('.'));
            if hidden {
                continue;
            }
            if path.is_dir() {
                if depth > 0 {
                    walk(kind, &path, depth - 1, out);
                }
            } else if kind.holds(&path) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    for folder in folders {
        walk(kind, folder, DEPTH, &mut out);
    }
    out.sort_by_key(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    out.dedup();
    out
}

/// A page of the browser: the files of a kind, found by name.
struct Page {
    kind: Kind,
    /// The folders it looks in: that of the user and those added.
    folders: RefCell<Vec<PathBuf>>,
    files: gtk::StringList,
}

/// The Media Browser of a window.
pub struct MediaBrowser {
    window: adw::Window,
    stack: adw::ViewStack,
    pages: Vec<Rc<Page>>,
}

impl MediaBrowser {
    /// The browser, which puts an activated item on the page with
    /// `insert`.
    pub fn new(parent: &gtk::Window, insert: impl Fn(&gio::File) + 'static) -> Rc<Self> {
        let insert: Rc<dyn Fn(&gio::File)> = Rc::new(insert);
        let stack = adw::ViewStack::new();
        let mut pages = Vec::new();
        for kind in Kind::ALL {
            let (page, widget) = page(kind, insert.clone());
            stack
                .add_titled(&widget, Some(kind.name()), &kind.title())
                .set_icon_name(Some(kind.icon()));
            pages.push(page);
        }
        let switcher = adw::ViewSwitcher::builder()
            .stack(&stack)
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();
        let header = adw::HeaderBar::builder().title_widget(&switcher).build();
        let add = gtk::Button::builder()
            .icon_name("folder-new-symbolic")
            .tooltip_text(tr("Add Folder…"))
            .build();
        add.update_property(&[gtk::accessible::Property::Label(&tr("Add Folder…"))]);
        header.pack_start(&add);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.set_content(Some(&stack));
        let window = adw::Window::builder()
            .title(tr("Media Browser"))
            .default_width(420)
            .default_height(520)
            .transient_for(parent)
            .hide_on_close(true)
            .content(&view)
            .build();
        let this = Rc::new(Self {
            window,
            stack,
            pages,
        });
        let weak = Rc::downgrade(&this);
        add.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.add_folder();
            }
        });
        let weak = Rc::downgrade(&this);
        this.window.connect_show(move |_| {
            if let Some(this) = weak.upgrade() {
                for page in &this.pages {
                    if page.files.n_items() == 0 {
                        refresh(page);
                    }
                }
            }
        });
        this
    }

    /// Shows the browser, or hides it when it shows.
    pub fn toggle(&self) {
        if self.window.is_visible() {
            self.window.set_visible(false);
        } else {
            self.window.present();
        }
    }

    /// Whether the browser shows.
    pub fn showing(&self) -> bool {
        self.window.is_visible()
    }

    /// The window, whose visibility the menu follows.
    pub fn window(&self) -> &adw::Window {
        &self.window
    }

    /// Asks for a folder to look in for the kind of media shown.
    fn add_folder(self: &Rc<Self>) {
        let name = self.stack.visible_child_name();
        let Some(page) = self
            .pages
            .iter()
            .find(|p| Some(p.kind.name()) == name.as_deref())
            .cloned()
        else {
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title(tr("Add a Folder"))
            .modal(true)
            .build();
        dialog.select_folder(Some(&self.window), gio::Cancellable::NONE, move |result| {
            if let Some(path) = result.ok().and_then(|f| f.path()) {
                page.folders.borrow_mut().push(path);
                refresh(&page);
            }
        });
    }
}

/// Looks again for the files of a page, away from the interface.
fn refresh(page: &Rc<Page>) {
    let folders = page.folders.borrow().clone();
    let kind = page.kind;
    let page = page.clone();
    glib::spawn_future_local(async move {
        let Ok(files) = gio::spawn_blocking(move || found(kind, &folders)).await else {
            return;
        };
        let paths: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let names: Vec<&str> = paths.iter().map(String::as_str).collect();
        page.files.splice(0, page.files.n_items(), &names);
    });
}

/// The name of a file of a path, as the browser shows it.
fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// A page of the browser for a kind, with the widget that shows it.
fn page(kind: Kind, insert: Rc<dyn Fn(&gio::File)>) -> (Rc<Page>, gtk::Widget) {
    let files = gtk::StringList::new(&[]);
    let search = gtk::SearchEntry::builder()
        .placeholder_text(tr("Search"))
        .margin_start(6)
        .margin_end(6)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    let filter = {
        let search = search.clone();
        gtk::CustomFilter::new(move |item| {
            let text = search.text().to_lowercase();
            item.downcast_ref::<gtk::StringObject>()
                .is_some_and(|s| file_name(&s.string()).to_lowercase().contains(&text))
        })
    };
    {
        let filter = filter.clone();
        search.connect_search_changed(move |_| filter.changed(gtk::FilterChange::Different));
    }
    let model = gtk::FilterListModel::new(Some(files.clone()), Some(filter.clone()));
    let selection = gtk::SingleSelection::new(Some(model));
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let tile = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .width_request(TILE)
            .height_request(TILE)
            .build();
        let label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .max_width_chars(14)
            .build();
        tile.append(&picture);
        tile.append(&label);
        // Dragged to the page, as a file.
        let drag = gtk::DragSource::new();
        drag.set_actions(gdk::DragAction::COPY);
        {
            let tile = tile.clone();
            drag.connect_prepare(move |_, _, _| {
                let path = tile.tooltip_text()?;
                let files = gdk::FileList::from_array(&[gio::File::for_path(path.as_str())]);
                Some(gdk::ContentProvider::for_value(&files.to_value()))
            });
        }
        tile.add_controller(drag);
        item.set_child(Some(&tile));
    });
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let (Some(tile), Some(path)) = (
            item.child().and_downcast::<gtk::Box>(),
            item.item()
                .and_downcast::<gtk::StringObject>()
                .map(|s| s.string()),
        ) else {
            return;
        };
        tile.set_tooltip_text(Some(&path));
        let (Some(picture), Some(label)) = (
            tile.first_child().and_downcast::<gtk::Picture>(),
            tile.last_child().and_downcast::<gtk::Label>(),
        ) else {
            return;
        };
        label.set_label(&file_name(&path));
        if kind == Kind::Photos && !path.to_lowercase().ends_with(".pdf") {
            picture.set_filename(Some(path.as_str()));
        } else {
            let icon = gtk::IconTheme::for_display(&tile.display()).lookup_icon(
                kind.icon(),
                &[],
                TILE / 2,
                1,
                gtk::TextDirection::None,
                gtk::IconLookupFlags::empty(),
            );
            picture.set_paintable(Some(&icon));
        }
    });
    let grid = gtk::GridView::builder()
        .model(&selection)
        .factory(&factory)
        .max_columns(6)
        .single_click_activate(false)
        .build();
    let activated = selection.clone();
    grid.connect_activate(move |_, position| {
        if let Some(path) = activated
            .item(position)
            .and_downcast::<gtk::StringObject>()
            .map(|s| s.string())
        {
            insert(&gio::File::for_path(path.as_str()));
        }
    });
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&grid)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&search);
    content.append(&scrolled);
    let page = Rc::new(Page {
        kind,
        folders: RefCell::new(kind.folder().into_iter().collect()),
        files,
    });
    (page, content.upcast())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_14_the_browser_finds_the_media_of_each_kind_in_its_folders() {
        let root = std::env::temp_dir().join(format!("desk-media-browser-{}", std::process::id()));
        let deep = root.join("trip").join("day one");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::create_dir_all(root.join(".cache")).unwrap();
        for name in [
            "Beach.JPG",
            "trip/day one/Sunset.png",
            "trip/song.mp3",
            "clip.mov",
            "notes.txt",
            ".cache/hidden.png",
        ] {
            std::fs::write(root.join(name), b"").unwrap();
        }
        let names = |kind| {
            found(kind, std::slice::from_ref(&root))
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Kind::Photos), ["Beach.JPG", "Sunset.png"]);
        assert_eq!(names(Kind::Music), ["song.mp3"]);
        assert_eq!(names(Kind::Movies), ["clip.mov"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
