// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shapes, text boxes and connection lines of the Insert menu, and
//! the Arrange menu of Pages, for the selected objects (OBJ-04, OBJ-08,
//! OBJ-13, OBJ-24 to OBJ-26, OBJ-30 to OBJ-34, EST-21).

use std::rc::Rc;

use desk_model::{Distribution, Layer, ObjectAlignment};
use gtk::prelude::*;
use gtk::{gio, glib};

use super::DocumentWindow;
use crate::editing::{NewShape, TextEditor};
use crate::i18n::{tr, trc};

/// The actions of these menus, which need a document.
pub(super) const OBJECT_ACTIONS: [&str; 28] = [
    "mask-with-shape",
    "unmask",
    "duplicate-objects",
    "insert-picture",
    "move-to-master",
    "move-to-page",
    "background-selectable",
    "masters-selectable",
    "insert-connection",
    "draw-with-pen",
    "make-editable",
    "smooth-path",
    "sharpen-path",
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
pub(crate) const OBJECT_ACCELS: [(&str, &[&str]); 10] = [
    ("win.duplicate-objects", &["<Control>d"]),
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
        let simple: [(&str, EditorAction); 21] = [
            ("unmask", TextEditor::unmask_picture),
            ("duplicate-objects", TextEditor::duplicate_objects),
            ("move-to-master", TextEditor::move_objects_to_master),
            ("move-to-page", TextEditor::move_objects_to_page),
            ("insert-text-box", |e| e.insert_shape(NewShape::TextBox)),
            ("insert-connection", TextEditor::insert_connection),
            ("draw-with-pen", TextEditor::start_pen),
            ("make-editable", |e| {
                e.make_editable();
            }),
            ("smooth-path", |e| e.reshape_path(false)),
            ("sharpen-path", |e| e.reshape_path(true)),
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
                let Some(this) = weak.upgrade() else { return };
                // Pages selected in the thumbnails take Duplicate.
                if name == "duplicate-objects" && this.thumbnails.take_shortcut("duplicate") {
                    return;
                }
                if let Some(editor) = this.editor() {
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
        self.add_media_browser_action();
        self.add_selectable_actions();
        let with_string: [(&str, ChoiceAction); 4] = [
            ("insert-shape", |e, value| {
                e.insert_shape(named_shape(value))
            }),
            ("mask-with-shape", |e, value| {
                e.mask_picture(named_shape(value))
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

    /// Asks for an image, a PDF, a movie or a sound, and puts it on the
    /// page (OBJ-01, OBJ-18).
    fn choose_picture(self: &Rc<Self>) {
        let filters = crate::bullet_pictures::document_picture_filters(true);
        let dialog = gtk::FileDialog::builder()
            .title(tr("Choose an Image, Movie or Sound"))
            .modal(true)
            .filters(&filters)
            .build();
        let weak = Rc::downgrade(self);
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |result| {
            let (Some(this), Ok(file)) = (weak.upgrade(), result) else {
                return;
            };
            let Some(editor) = this.editor() else { return };
            if !editor.insert_file(&file, None) {
                this.alert(
                    &tr("Cannot open {name}").replace(
                        "{name}",
                        &file
                            .basename()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    ),
                    &tr("The file is not an image, movie or sound that Desk can read."),
                );
            }
        });
    }
}

impl DocumentWindow {
    /// Makes the objects behind the text, or those of the masters,
    /// selectable or not (OBJ-25, EST-21).
    fn add_selectable_actions(self: &Rc<Self>) {
        for (name, masters) in [
            ("background-selectable", false),
            ("masters-selectable", true),
        ] {
            let action = gio::SimpleAction::new_stateful(name, None, &false.to_variant());
            let weak = Rc::downgrade(self);
            action.connect_change_state(move |action, value| {
                let (Some(editor), Some(on)) = (
                    weak.upgrade().and_then(|this| this.editor()),
                    value.and_then(|v| v.get::<bool>()),
                ) else {
                    return;
                };
                let mut selectable = editor.selectable();
                if masters {
                    selectable.masters = on;
                } else {
                    selectable.background = on;
                }
                editor.set_selectable(selectable);
                action.set_state(&on.to_variant());
            });
            self.window.add_action(&action);
        }
    }

    /// Shows or hides the Media Browser, whose items go on the page shown
    /// (INT-14).
    fn add_media_browser_action(self: &Rc<Self>) {
        let action = gio::SimpleAction::new_stateful("media-browser", None, &false.to_variant());
        let weak = Rc::downgrade(self);
        action.connect_change_state(move |action, value| {
            let (Some(this), Some(on)) = (weak.upgrade(), value.and_then(|v| v.get::<bool>()))
            else {
                return;
            };
            action.set_state(&on.to_variant());
            this.show_side("media", on);
        });
        self.window.add_action(&action);
    }

    /// The Media Browser, made the first time it is asked for.
    pub(super) fn media_browser(self: &Rc<Self>) -> Rc<crate::media_browser::MediaBrowser> {
        self.media_browser
            .get_or_init(|| {
                let weak = Rc::downgrade(self);
                crate::media_browser::MediaBrowser::new(move |file| {
                    let Some(this) = weak.upgrade() else { return };
                    if let Some(editor) = this.editor() {
                        editor.insert_file(file, None);
                    }
                })
            })
            .clone()
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
    for (label, value) in shape_names().into_iter().chain([(tr("_Line"), "line")]) {
        shapes.append(Some(&label), Some(&format!("win.insert-shape('{value}')")));
    }
    shapes.append(Some(&tr("Pe_n")), Some("win.draw-with-pen"));
    menu.append_submenu(Some(&tr("_Shape")), &shapes);
    // Joins the two selected objects (OBJ-31).
    menu.append(Some(&tr("Connection L_ine")), Some("win.insert-connection"));
    menu
}

/// The shapes of Insert Shape and Mask with Shape, with their values.
fn shape_names() -> [(String, &'static str); 12] {
    [
        (tr("_Rectangle"), "rectangle"),
        (tr("Rounded Re_ctangle"), "rounded"),
        (tr("_Oval"), "ellipse"),
        (tr("Tr_iangle"), "triangle"),
        (tr("Ri_ght Triangle"), "right-triangle"),
        (tr("_Diamond"), "diamond"),
        (tr("_Arrow"), "arrow"),
        (tr("Dou_ble Arrow"), "double-arrow"),
        (tr("_Star"), "star"),
        (tr("_Polygon"), "polygon"),
        (tr("_Quote Bubble"), "quote-bubble"),
        (tr("Call_out"), "callout"),
    ]
}

/// The shape that a value of Insert Shape and Mask with Shape names.
fn named_shape(value: &str) -> NewShape {
    match value {
        "rounded" => NewShape::RoundedRectangle,
        "ellipse" => NewShape::Ellipse,
        "triangle" => NewShape::Triangle,
        "right-triangle" => NewShape::RightTriangle,
        "diamond" => NewShape::Diamond,
        "arrow" => NewShape::Arrow,
        "double-arrow" => NewShape::DoubleArrow,
        "star" => NewShape::Star,
        "polygon" => NewShape::Polygon,
        "quote-bubble" => NewShape::QuoteBubble,
        "callout" => NewShape::Callout,
        "line" => NewShape::Line,
        _ => NewShape::Rectangle,
    }
}

/// The Arrange menu of Pages (OBJ-04, OBJ-24, OBJ-26, OBJ-30, OBJ-32,
/// OBJ-34).
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
    let shape = gio::Menu::new();
    shape.append(Some(&tr("Make _Editable")), Some("win.make-editable"));
    shape.append(Some(&tr("Smoo_th Path")), Some("win.smooth-path"));
    shape.append(Some(&tr("Sha_rpen Path")), Some("win.sharpen-path"));
    // Mask with Shape and Unmask for the selected picture (OBJ-04).
    let masks = gio::Menu::new();
    for (label, value) in shape_names() {
        masks.append(
            Some(&label),
            Some(&format!("win.mask-with-shape('{value}')")),
        );
    }
    shape.append_submenu(Some(&tr("Mask with S_hape")), &masks);
    shape.append(Some(&tr("Un_mask")), Some("win.unmask"));
    let grouping = gio::Menu::new();
    // A copy a little lower and to the right (OBJ-22).
    grouping.append(
        Some(&trc("Arrange", "Dupl_icate")),
        Some("win.duplicate-objects"),
    );
    grouping.append(Some(&tr("_Group")), Some("win.group"));
    grouping.append(Some(&tr("_Ungroup")), Some("win.ungroup"));
    grouping.append(Some(&tr("_Lock")), Some("win.lock"));
    grouping.append(Some(&tr("U_nlock")), Some("win.unlock"));
    // The masters of the sections and the background (EST-21, OBJ-25).
    let masters = gio::Menu::new();
    masters.append(
        Some(&tr("Move to Section _Master")),
        Some("win.move-to-master"),
    );
    masters.append(Some(&tr("Move to _Page")), Some("win.move-to-page"));
    masters.append(
        Some(&tr("Make Background Objects _Selectable")),
        Some("win.background-selectable"),
    );
    masters.append(
        Some(&tr("Make Master _Objects Selectable")),
        Some("win.masters-selectable"),
    );
    let menu = gio::Menu::new();
    menu.append_section(None, &layers);
    menu.append_section(None, &placing);
    menu.append_section(None, &grouping);
    menu.append_section(None, &shape);
    menu.append_section(None, &masters);
    menu
}
