// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Object tab of Pages 15.4, for a selected 3D object (W116,
//! PXV-25): how far it is turned and tipped, which dragging the handle in
//! its middle also sets, Reset Rotation, Play and Pause for a model that
//! moves, and the Description that says what it shows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use desk_model::{MAX_PITCH, ObjectKind};

use crate::compact;
use crate::editing::ObjectInfo;
use crate::formatting::EditorOf;
use crate::i18n::tr;

/// The 3D Object group of the Inspector.
pub struct ModelGroup {
    group: compact::Group,
    yaw: compact::SpinRow,
    pitch: compact::SpinRow,
    animation: compact::ActionRow,
    play: gtk::Button,
    /// The file last shown, and whether its model moves, so that it is
    /// read once.
    moves: RefCell<Option<(Arc<[u8]>, bool)>>,
    description: compact::EntryRow,
    editor: EditorOf,
    updating: Cell<bool>,
}

impl ModelGroup {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let group = compact::Group::builder().title(tr("3D Object")).build();
        let yaw = compact::SpinRow::with_range(-180.0, 180.0, 5.0);
        yaw.set_title(&tr("Turn"));
        yaw.set_subtitle(&tr("Degrees about the vertical axis"));
        yaw.set_digits(1);
        group.add(&yaw);
        let most = f64::from(MAX_PITCH) / 100.0;
        let pitch = compact::SpinRow::with_range(-most, most, 5.0);
        pitch.set_title(&tr("Tip"));
        pitch.set_subtitle(&tr("Degrees about the horizontal axis"));
        pitch.set_digits(1);
        group.add(&pitch);
        let reset = gtk::Button::builder()
            .label(tr("_Reset"))
            .use_underline(true)
            .action_name("win.reset-model-rotation")
            .valign(gtk::Align::Center)
            .build();
        let rotation = compact::ActionRow::builder()
            .title(tr("Reset Rotation"))
            .subtitle(tr("Or drag the handle in the middle to turn it"))
            .activatable_widget(&reset)
            .build();
        rotation.add_suffix(&reset);
        group.add(&rotation);
        let play = gtk::Button::builder()
            .label(tr("_Play"))
            .use_underline(true)
            .action_name("win.play-model-animation")
            .valign(gtk::Align::Center)
            .build();
        let animation = compact::ActionRow::builder()
            .title(tr("Animation"))
            .subtitle(tr("Plays how the object moves, over and over"))
            .activatable_widget(&play)
            .build();
        animation.add_suffix(&play);
        group.add(&animation);
        let description = compact::EntryRow::builder()
            .title(tr("Description"))
            .build();
        description.set_subtitle(&tr("Says what it shows, for those who cannot see it"));
        group.add(&description);
        let this = Rc::new(Self {
            group,
            yaw,
            pitch,
            animation,
            play,
            moves: RefCell::new(None),
            description,
            editor,
            updating: Cell::new(false),
        });
        for row in [&this.yaw, &this.pitch] {
            let weak = Rc::downgrade(&this);
            row.connect_value_notify(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.turn();
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
                let Some((_, model)) = editor.selected_model() else {
                    return;
                };
                let text = row.text();
                let text = text.trim();
                let new = (!text.is_empty()).then(|| Arc::<str>::from(text));
                if model.description != new {
                    editor.set_object("Description", move |object| {
                        if let ObjectKind::Model(model) = &mut object.kind {
                            model.description = new;
                        }
                    });
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

    /// Turns the selected model as the rows say.
    fn turn(&self) {
        if self.updating.get() {
            return;
        }
        let centi = |row: &compact::SpinRow| (row.value() * 100.0).round() as i32;
        if let Some(editor) = (self.editor)() {
            editor.turn_selected_model("Rotate 3D Object", centi(&self.yaw), centi(&self.pitch));
        }
    }

    /// Shows the selected 3D object, or hides the group when none is.
    pub fn show(&self, info: Option<&ObjectInfo>) {
        let model = info.and_then(|i| i.object.model()).cloned();
        self.group.set_visible(model.is_some());
        let (Some(info), Some(model)) = (info, model) else {
            return;
        };
        self.group
            .widget()
            .set_sensitive(info.count == 1 && !info.object.locked);
        self.updating.set(true);
        self.yaw.set_value(f64::from(model.yaw) / 100.0);
        self.pitch.set_value(f64::from(model.pitch) / 100.0);
        let moves = {
            let mut known = self.moves.borrow_mut();
            match &*known {
                Some((data, moves)) if Arc::ptr_eq(data, &model.shared_data()) => *moves,
                _ => {
                    let moves = model.scene().is_ok_and(|s| s.animation.is_some());
                    *known = Some((model.shared_data(), moves));
                    moves
                }
            }
        };
        self.animation.set_visible(moves);
        let playing = (self.editor)().is_some_and(|e| e.model_playing());
        self.play
            .set_label(&if playing { tr("_Pause") } else { tr("_Play") });
        let description = model.description.as_deref().unwrap_or_default();
        if self.description.text() != description {
            self.description.set_text(description);
        }
        self.updating.set(false);
    }
}
