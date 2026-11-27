// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Drawing tab of Pages 15.4, for a selected drawing (W117, PXV-26):
//! Edit Drawing, which draws on it again, and Animate Drawing, with how
//! long the animation takes, whether it loops, and Play, which draws its
//! lines again on the page.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{ANIMATION_DURATION, DrawingAnimation, ObjectKind};

use crate::compact;
use crate::editing::ObjectInfo;
use crate::formatting::EditorOf;
use crate::i18n::tr;

/// The Drawing group of the Inspector.
pub struct DrawingGroup {
    group: compact::Group,
    animate: compact::SwitchRow,
    duration: compact::SpinRow,
    repeat: compact::SwitchRow,
    play: gtk::Button,
    editor: EditorOf,
    updating: Cell<bool>,
}

impl DrawingGroup {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let edit = gtk::Button::builder()
            .label(tr("_Edit Drawing"))
            .use_underline(true)
            .action_name("win.edit-drawing")
            .valign(gtk::Align::Center)
            .build();
        let group = compact::Group::builder().title(tr("Drawing")).build();
        let draw = compact::ActionRow::builder()
            .title(tr("Draw on It"))
            .subtitle(tr("Or double click the drawing"))
            .activatable_widget(&edit)
            .build();
        draw.add_suffix(&edit);
        group.add(&draw);
        let animate = compact::SwitchRow::builder()
            .title(tr("Animate Drawing"))
            .subtitle(tr(
                "Its lines are drawn again, in the order they were drawn",
            ))
            .build();
        group.add(&animate);
        let seconds = |ms: u32| f64::from(ms) / 1000.0;
        let duration = compact::SpinRow::with_range(
            seconds(*ANIMATION_DURATION.start()),
            seconds(*ANIMATION_DURATION.end()),
            0.1,
        );
        duration.set_title(&tr("Duration"));
        duration.set_subtitle(&tr("Seconds"));
        duration.set_digits(1);
        group.add(&duration);
        let repeat = compact::SwitchRow::builder()
            .title(tr("Loop"))
            .subtitle(tr("Starts again when it ends"))
            .build();
        group.add(&repeat);
        let play = gtk::Button::builder()
            .label(tr("_Play"))
            .use_underline(true)
            .action_name("win.play-drawing")
            .valign(gtk::Align::Center)
            .build();
        let preview = compact::ActionRow::builder()
            .title(tr("Preview"))
            .activatable_widget(&play)
            .build();
        preview.add_suffix(&play);
        group.add(&preview);
        let this = Rc::new(Self {
            group,
            animate,
            duration,
            repeat,
            play,
            editor,
            updating: Cell::new(false),
        });
        {
            let weak = Rc::downgrade(&this);
            this.animate.connect_active_notify(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.apply();
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.duration.connect_value_notify(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.apply();
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            this.repeat.connect_active_notify(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.apply();
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

    /// Gives the selected drawing the animation the rows show.
    fn apply(&self) {
        if self.updating.get() {
            return;
        }
        let animation = self.animate.is_active().then(|| DrawingAnimation {
            duration: (self.duration.value() * 1000.0).round() as u32,
            repeat: self.repeat.is_active(),
        });
        if let Some(editor) = (self.editor)() {
            editor.set_selected_drawing_animation(animation);
        }
    }

    /// Shows the selected drawing, or hides the group when none is.
    pub fn show(&self, info: Option<&ObjectInfo>) {
        let drawing = match info.map(|i| &i.object.kind) {
            Some(ObjectKind::Drawing(drawing)) => Some(drawing.clone()),
            _ => None,
        };
        self.group.set_visible(drawing.is_some());
        let (Some(info), Some(drawing)) = (info, drawing) else {
            return;
        };
        self.group
            .widget()
            .set_sensitive(info.count == 1 && !info.object.locked);
        self.updating.set(true);
        let animation = drawing.animation;
        self.animate.set_active(animation.is_some());
        let shown = animation.unwrap_or_default();
        self.duration.set_value(f64::from(shown.duration) / 1000.0);
        self.repeat.set_active(shown.repeat);
        self.duration.set_sensitive(animation.is_some());
        self.repeat.set_sensitive(animation.is_some());
        let playing = (self.editor)().is_some_and(|e| e.drawing_playing());
        self.play
            .set_label(&if playing { tr("_Stop") } else { tr("_Play") });
        self.updating.set(false);
    }
}
