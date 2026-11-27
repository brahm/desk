// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Gallery tab of Pages 15.4, for a selected image gallery (W111,
//! PXV-23): its pictures in their order, each shown, moved or taken out,
//! with + to add more; whether each has its own caption; and the Image
//! Description of the one shown.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Gallery, ObjectKind, Picture};
use gtk::gio;

use crate::compact;
use crate::editing::ObjectInfo;
use crate::formatting::EditorOf;
use crate::i18n::tr;

/// The side of the pictures of the list, in pixels.
const THUMBNAIL: i32 = 40;

/// The Gallery group of the Inspector.
pub struct GalleryGroup {
    group: compact::Group,
    each_caption: compact::SwitchRow,
    list: gtk::Box,
    description: compact::EntryRow,
    editor: EditorOf,
    /// The pictures the list shows and the one shown, so that it is made
    /// again only when they change.
    listed: RefCell<Option<(Vec<Picture>, usize)>>,
    updating: Cell<bool>,
}

/// What a button of a picture does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Show,
    Up,
    Down,
    Remove,
}

impl GalleryGroup {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let add = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text(tr("Add Images…"))
            .valign(gtk::Align::Center)
            .build();
        add.add_css_class("flat");
        add.update_property(&[gtk::accessible::Property::Label(&tr("Add Images…"))]);
        let group = compact::Group::builder()
            .title(tr("Image Gallery"))
            .header_suffix(&add)
            .build();
        let each_caption = compact::SwitchRow::builder()
            .title(tr("Caption for Each Image"))
            .subtitle(tr(
                "Each image has a caption of its own, instead of one for all",
            ))
            .build();
        group.add(&each_caption);
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        group.add_widget(&list);
        let description = compact::EntryRow::builder()
            .title(tr("Image Description"))
            .build();
        description.set_subtitle(&tr("Describes the image shown for those who cannot see it"));
        group.add(&description);
        let this = Rc::new(Self {
            group,
            each_caption,
            list,
            description,
            editor,
            listed: RefCell::new(None),
            updating: Cell::new(false),
        });
        {
            let weak = Rc::downgrade(&this);
            this.each_caption.connect_active_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let each = row.is_active();
                if let Some(editor) = (this.editor)() {
                    editor.change_gallery("Caption for Each Image", |edit, id| {
                        edit.set_gallery_each_caption(id, each)
                    });
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.description.connect_apply(move |row| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let Some(editor) = (this.editor)() else {
                    return;
                };
                let Some((_, gallery)) = editor.selected_gallery() else {
                    return;
                };
                let text = row.text();
                let at = gallery.shown_index();
                if gallery
                    .shown_image()
                    .is_some_and(|image| image.description != text.as_str())
                {
                    editor.change_gallery("Image Description", |edit, id| {
                        edit.set_gallery_description(id, at, &text)
                    });
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            add.connect_clicked(move |button| {
                if let Some(this) = weak.upgrade() {
                    this.choose(button);
                }
            });
        }
        this.show(None);
        this
    }

    /// The group, to put in the Inspector.
    pub fn group(&self) -> &compact::Group {
        &self.group
    }

    /// Asks for pictures, and adds them to the selected gallery.
    fn choose(&self, button: &gtk::Button) {
        let dialog = gtk::FileDialog::builder()
            .title(tr("Add Images"))
            .modal(true)
            .filters(&crate::bullet_pictures::document_picture_filters(false))
            .build();
        let window = button.root().and_downcast::<gtk::Window>();
        let editor = self.editor.clone();
        dialog.open_multiple(window.as_ref(), gio::Cancellable::NONE, move |result| {
            let Ok(files) = result else { return };
            let pictures: Vec<Picture> = (0..files.n_items())
                .filter_map(|i| files.item(i).and_downcast::<gio::File>())
                .filter_map(|file| crate::bullet_pictures::document_picture_from_file(&file))
                .collect();
            if let Some(editor) = editor() {
                editor.add_gallery_pictures(pictures);
            }
        });
    }

    /// Does what a button of the picture at `index` asks.
    fn step(&self, index: usize, step: Step) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let Some((_, gallery)) = editor.selected_gallery() else {
            return;
        };
        let last = gallery.images.len().saturating_sub(1);
        match step {
            Step::Show => {
                editor.change_gallery("Show Image", |edit, id| edit.show_gallery_image(id, index))
            }
            Step::Up if index > 0 => editor.change_gallery("Move Image", |edit, id| {
                edit.move_gallery_image(id, index, index - 1)
            }),
            Step::Down if index < last => editor.change_gallery("Move Image", |edit, id| {
                edit.move_gallery_image(id, index, index + 1)
            }),
            Step::Remove => editor.change_gallery("Remove Image", |edit, id| {
                edit.remove_gallery_image(id, index)
            }),
            _ => {}
        }
    }

    /// Makes the list of pictures again.
    fn list(self: &Rc<Self>, gallery: &Gallery) {
        let pictures: Vec<Picture> = gallery.images.iter().map(|i| i.picture.clone()).collect();
        let key = (pictures, gallery.shown_index());
        if self.listed.borrow().as_ref() == Some(&key) {
            return;
        }
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let count = gallery.images.len();
        if count == 0 {
            let empty = compact::ActionRow::builder()
                .title(tr("No Images"))
                .subtitle(tr("Add images with +, or drop them on the gallery"))
                .build();
            self.list.append(empty.widget());
        }
        for (index, image) in gallery.images.iter().enumerate() {
            let title = tr("Image {number}").replace("{number}", &(index + 1).to_string());
            let row = compact::ActionRow::builder().title(title.as_str()).build();
            if index == gallery.shown_index() {
                row.set_subtitle(&tr("Shown"));
            }
            let thumbnail = gtk::Picture::builder()
                .content_fit(gtk::ContentFit::Contain)
                .width_request(THUMBNAIL)
                .height_request(THUMBNAIL)
                .can_shrink(true)
                .build();
            thumbnail.set_paintable(crate::bullet_pictures::texture(&image.picture).as_ref());
            if !image.description.is_empty() {
                thumbnail.set_alternative_text(Some(&image.description));
            }
            row.add_prefix(&thumbnail);
            let button = |icon: &str, tip: String, step: Step, sensitive: bool| {
                let button = gtk::Button::builder()
                    .icon_name(icon)
                    .tooltip_text(&tip)
                    .valign(gtk::Align::Center)
                    .sensitive(sensitive)
                    .build();
                button.add_css_class("flat");
                button.update_property(&[gtk::accessible::Property::Label(&tip)]);
                let weak = Rc::downgrade(self);
                button.connect_clicked(move |_| {
                    if let Some(this) = weak.upgrade() {
                        this.step(index, step);
                    }
                });
                button
            };
            let show = button(
                "view-reveal-symbolic",
                tr("Show"),
                Step::Show,
                index != gallery.shown_index(),
            );
            row.add_suffix(&show);
            row.add_suffix(&button(
                "go-up-symbolic",
                tr("Move Up"),
                Step::Up,
                index > 0,
            ));
            row.add_suffix(&button(
                "go-down-symbolic",
                tr("Move Down"),
                Step::Down,
                index + 1 < count,
            ));
            row.add_suffix(&button(
                "list-remove-symbolic",
                tr("Remove"),
                Step::Remove,
                true,
            ));
            self.list.append(row.widget());
        }
        self.listed.replace(Some(key));
    }

    /// Shows the selected gallery, or hides the group when none is.
    pub fn show(self: &Rc<Self>, info: Option<&ObjectInfo>) {
        let gallery = match info.map(|i| &i.object.kind) {
            Some(ObjectKind::Gallery(gallery)) => Some(gallery.clone()),
            _ => None,
        };
        self.group.set_visible(gallery.is_some());
        let (Some(info), Some(gallery)) = (info, gallery) else {
            self.listed.replace(None);
            return;
        };
        self.group
            .widget()
            .set_sensitive(info.count == 1 && !info.object.locked);
        self.updating.set(true);
        self.each_caption.set_active(gallery.each_caption);
        self.list(&gallery);
        let description = gallery
            .shown_image()
            .map(|image| image.description.as_str())
            .unwrap_or_default();
        if self.description.text() != description {
            self.description.set_text(description);
        }
        self.description
            .set_sensitive(gallery.shown_image().is_some());
        self.updating.set(false);
    }
}
