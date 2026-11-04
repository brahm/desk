// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shapes and text boxes of the Insert menu, and the Arrange menu of
//! Pages, for the selected objects (OBJ-03 to OBJ-08, OBJ-13).

use std::rc::Rc;

use desk_model::{Distribution, Layer, ObjectAlignment};
use gtk::prelude::*;
use gtk::{gio, glib};

use super::DocumentWindow;
use crate::editing::{NewShape, TextEditor};
use crate::i18n::tr;

/// The actions of these menus, which need a document.
pub(super) const OBJECT_ACTIONS: [&str; 16] = [
    "insert-picture",
    "insert-shape",
    "insert-text-box",
    "add-linked-text-box",
    "bring-to-front",
    "bring-forward",
    "send-backward",
    "send-to-back",
    "align-objects",
    "distribute-objects",
    "group",
    "ungroup",
    "lock",
    "unlock",
    "flip-horizontal",
    "flip-vertical",
];

/// The keys of the Arrange menu.
pub(crate) const OBJECT_ACCELS: [(&str, &[&str]); 9] = [
    ("win.insert-picture", &["<Control><Shift>v"]),
    ("win.bring-to-front", &["<Control><Shift>bracketright"]),
    ("win.bring-forward", &["<Control>bracketright"]),
    ("win.send-backward", &["<Control>bracketleft"]),
    ("win.send-to-back", &["<Control><Shift>bracketleft"]),
    ("win.group", &["<Control><Alt>g"]),
    ("win.ungroup", &["<Control><Shift><Alt>g"]),
    ("win.lock", &["<Control><Alt>l"]),
    ("win.unlock", &["<Control><Shift><Alt>l"]),
];

/// What an action does to the editor.
type EditorAction = fn(&TextEditor);
/// What an action with a choice does to the editor.
type ChoiceAction = fn(&TextEditor, &str);

impl DocumentWindow {
    pub(super) fn add_object_actions(self: &Rc<Self>) {
        let simple: [(&str, EditorAction); 12] = [
            ("insert-text-box", |e| e.insert_shape(NewShape::TextBox)),
            ("add-linked-text-box", TextEditor::add_linked_text_box),
            ("bring-to-front", |e| e.layer_objects(Layer::Front)),
            ("bring-forward", |e| e.layer_objects(Layer::Forward)),
            ("send-backward", |e| e.layer_objects(Layer::Backward)),
            ("send-to-back", |e| e.layer_objects(Layer::Back)),
            ("group", TextEditor::group_objects),
            ("ungroup", TextEditor::ungroup_objects),
            ("lock", |e| e.lock_objects(true)),
            ("unlock", |e| e.lock_objects(false)),
            ("flip-horizontal", |e| e.flip_objects(true)),
            ("flip-vertical", |e| e.flip_objects(false)),
        ];
        for (name, run) in simple {
            let weak = Rc::downgrade(self);
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| {
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    run(&editor);
                }
            });
            self.window.add_action(&action);
        }
        let weak = Rc::downgrade(self);
        let action = gio::SimpleAction::new("insert-picture", None);
        action.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.choose_picture();
            }
        });
        self.window.add_action(&action);
        let with_string: [(&str, ChoiceAction); 3] = [
            ("insert-shape", |e, value| {
                let shape = match value {
                    "rounded" => NewShape::RoundedRectangle,
                    "ellipse" => NewShape::Ellipse,
                    "triangle" => NewShape::Triangle,
                    "line" => NewShape::Line,
                    _ => NewShape::Rectangle,
                };
                e.insert_shape(shape);
            }),
            ("align-objects", |e, value| {
                let alignment = match value {
                    "center" => ObjectAlignment::Center,
                    "right" => ObjectAlignment::Right,
                    "top" => ObjectAlignment::Top,
                    "middle" => ObjectAlignment::Middle,
                    "bottom" => ObjectAlignment::Bottom,
                    _ => ObjectAlignment::Left,
                };
                e.align_objects(alignment);
            }),
            ("distribute-objects", |e, value| {
                e.distribute_objects(if value == "vertically" {
                    Distribution::Vertically
                } else {
                    Distribution::Horizontally
                });
            }),
        ];
        for (name, run) in with_string {
            let weak = Rc::downgrade(self);
            let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
            action.connect_activate(move |_, parameter| {
                let Some(value) = parameter.and_then(|p| p.get::<String>()) else {
                    return;
                };
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    run(&editor, &value);
                }
            });
            self.window.add_action(&action);
        }
    }

    /// Asks for an image or a PDF, and puts it on the page (OBJ-01).
    fn choose_picture(self: &Rc<Self>) {
        let filters = crate::bullet_pictures::document_picture_filters();
        let dialog = gtk::FileDialog::builder()
            .title(tr("Choose an Image"))
            .modal(true)
            .filters(&filters)
            .build();
        let weak = Rc::downgrade(self);
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |result| {
            let (Some(this), Ok(file)) = (weak.upgrade(), result) else {
                return;
            };
            match crate::bullet_pictures::document_picture_from_file(&file) {
                Some(picture) => {
                    if let Some(editor) = this.editor() {
                        editor.insert_picture(picture);
                    }
                }
                None => this.alert(
                    &tr("Cannot open {name}").replace(
                        "{name}",
                        &file
                            .basename()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    ),
                    &tr("The file is not an image that Desk can read."),
                ),
            }
        });
    }
}

/// The shapes and the text box of the Insert menu (OBJ-08, OBJ-13).
pub(super) fn shapes_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("C_hoose…")), Some("win.insert-picture"));
    menu.append(Some(&tr("_Text Box")), Some("win.insert-text-box"));
    menu.append(
        Some(&tr("Lin_ked Text Box")),
        Some("win.add-linked-text-box"),
    );
    let shapes = gio::Menu::new();
    for (label, value) in [
        (tr("_Rectangle"), "rectangle"),
        (tr("Rounded Re_ctangle"), "rounded"),
        (tr("_Oval"), "ellipse"),
        (tr("Tr_iangle"), "triangle"),
        (tr("_Line"), "line"),
    ] {
        shapes.append(Some(&label), Some(&format!("win.insert-shape('{value}')")));
    }
    menu.append_submenu(Some(&tr("_Shape")), &shapes);
    menu
}

/// The Arrange menu of Pages (OBJ-03 to OBJ-07).
pub(super) fn arrange_menu() -> gio::Menu {
    let layers = gio::Menu::new();
    layers.append(Some(&tr("Bring to _Front")), Some("win.bring-to-front"));
    layers.append(Some(&tr("Bring _Forward")), Some("win.bring-forward"));
    layers.append(Some(&tr("Send _Backward")), Some("win.send-backward"));
    layers.append(Some(&tr("Send to Bac_k")), Some("win.send-to-back"));
    let align = gio::Menu::new();
    for (label, value) in [
        (tr("_Left"), "left"),
        (tr("_Center"), "center"),
        (tr("_Right"), "right"),
        (tr("_Top"), "top"),
        (tr("_Middle"), "middle"),
        (tr("_Bottom"), "bottom"),
    ] {
        align.append(Some(&label), Some(&format!("win.align-objects('{value}')")));
    }
    let distribute = gio::Menu::new();
    distribute.append(
        Some(&tr("_Horizontally")),
        Some("win.distribute-objects('horizontally')"),
    );
    distribute.append(
        Some(&tr("_Vertically")),
        Some("win.distribute-objects('vertically')"),
    );
    let placing = gio::Menu::new();
    placing.append_submenu(Some(&tr("_Align Objects")), &align);
    placing.append_submenu(Some(&tr("_Distribute Objects")), &distribute);
    placing.append(Some(&tr("Flip _Horizontally")), Some("win.flip-horizontal"));
    placing.append(Some(&tr("Flip _Vertically")), Some("win.flip-vertical"));
    let grouping = gio::Menu::new();
    grouping.append(Some(&tr("_Group")), Some("win.group"));
    grouping.append(Some(&tr("_Ungroup")), Some("win.ungroup"));
    grouping.append(Some(&tr("_Lock")), Some("win.lock"));
    grouping.append(Some(&tr("U_nlock")), Some("win.unlock"));
    let menu = gio::Menu::new();
    menu.append_section(None, &layers);
    menu.append_section(None, &placing);
    menu.append_section(None, &grouping);
    menu
}
