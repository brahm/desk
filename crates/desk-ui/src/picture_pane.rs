// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The image tools of Pages, for a selected picture: its mask (OBJ-04),
//! Instant Alpha (OBJ-05), Adjust Image and Reduce File Size (OBJ-06).

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Adjustments, Frame, InstantAlpha, ObjectKind, Rgba};

use crate::editing::ObjectInfo;
use crate::formatting::{EditorOf, from_gdk, labelled, to_gdk};
use crate::i18n::tr;

/// The Image group of the Inspector.
pub struct PictureGroup {
    group: adw::PreferencesGroup,
    mask: adw::SwitchRow,
    scale: adw::SpinRow,
    left: adw::SpinRow,
    top: adw::SpinRow,
    alpha: adw::SwitchRow,
    alpha_color: gtk::ColorDialogButton,
    alpha_color_row: adw::ActionRow,
    tolerance: adw::SpinRow,
    adjust: [adw::SpinRow; 6],
    reset: gtk::Button,
    reduce: gtk::Button,
    /// How far the selected picture is placed on its page from its frame
    /// in the document, in points, as an object that moves with a
    /// paragraph is.
    shift: Cell<(f64, f64)>,
    updating: Cell<bool>,
}

fn spin(title: &str, min: f64, max: f64) -> adw::SpinRow {
    let row = adw::SpinRow::with_range(min, max, 1.0);
    row.set_title(title);
    row
}

/// The changes that the rows of Adjust Image ask for.
fn adjustments(rows: &[adw::SpinRow; 6]) -> Adjustments {
    let [
        brightness,
        contrast,
        saturation,
        exposure,
        temperature,
        tint,
    ] = rows
        .each_ref()
        .map(|r| r.value().round().clamp(-100.0, 100.0) as i8);
    Adjustments {
        brightness,
        contrast,
        saturation,
        exposure,
        temperature,
        tint,
    }
}

impl PictureGroup {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let mask = adw::SwitchRow::builder()
            .title(tr("Mask"))
            .subtitle(tr("Shows only the part of the picture inside its frame"))
            .build();
        let scale = spin(&tr("Picture Size"), 1.0, 1000.0);
        scale.set_subtitle(&tr("Percent of the frame"));
        let left = spin(&tr("Picture Left"), -10_000.0, 10_000.0);
        left.set_subtitle(&tr("Points from the frame"));
        let top = spin(&tr("Picture Top"), -10_000.0, 10_000.0);
        top.set_subtitle(&tr("Points from the frame"));
        let alpha = adw::SwitchRow::builder()
            .title(tr("Instant Alpha"))
            .subtitle(tr("Takes a color out of the picture"))
            .build();
        let alpha_color = gtk::ColorDialogButton::builder()
            .dialog(&gtk::ColorDialog::builder().title(tr("Color")).build())
            .rgba(&to_gdk(Rgba::WHITE))
            .valign(gtk::Align::Center)
            .build();
        labelled(&alpha_color, &tr("Color to Take Out"));
        let alpha_color_row = adw::ActionRow::builder()
            .title(tr("Color to Take Out"))
            .build();
        alpha_color_row.add_suffix(&crate::eyedropper::button(&alpha_color));
        alpha_color_row.add_suffix(&alpha_color);
        let tolerance = spin(&tr("Tolerance"), 0.0, 100.0);
        tolerance.set_subtitle(&tr("How near a color takes it out too"));
        let adjust = [
            tr("Brightness"),
            tr("Contrast"),
            tr("Saturation"),
            tr("Exposure"),
            tr("Temperature"),
            tr("Tint"),
        ]
        .map(|title| spin(&title, -100.0, 100.0));
        let reset = gtk::Button::builder()
            .label(tr("Reset Image"))
            .valign(gtk::Align::Center)
            .build();
        let reduce = gtk::Button::builder()
            .label(tr("Reduce File Size"))
            .valign(gtk::Align::Center)
            .tooltip_text(tr(
                "Keeps only the part of the picture that shows, at the size it is shown",
            ))
            .build();
        let buttons = adw::ActionRow::new();
        buttons.add_suffix(&reduce);
        buttons.add_suffix(&reset);
        let group = adw::PreferencesGroup::builder().title(tr("Image")).build();
        group.add(&mask);
        group.add(&scale);
        group.add(&left);
        group.add(&top);
        group.add(&alpha);
        group.add(&alpha_color_row);
        group.add(&tolerance);
        for row in &adjust {
            group.add(row);
        }
        group.add(&buttons);
        let this = Rc::new(Self {
            group,
            mask,
            scale,
            left,
            top,
            alpha,
            alpha_color,
            alpha_color_row,
            tolerance,
            adjust,
            reset,
            reduce,
            shift: Cell::new((0.0, 0.0)),
            updating: Cell::new(false),
        });
        this.show(None);

        let weak = Rc::downgrade(&this);
        let mask_editor = editor.clone();
        this.mask.connect_active_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let on = row.is_active();
            let (dx, dy) = this.shift.get();
            if let Some(editor) = mask_editor() {
                let name = if on { "Mask" } else { "Unmask" };
                editor.set_object(name, |object| match (on, object.picture.mask) {
                    // The frame begins by showing all of the picture.
                    (true, None) => {
                        let f = object.frame;
                        object.picture.mask = Some(Frame {
                            x: desk_model::Length::ZERO,
                            y: desk_model::Length::ZERO,
                            width: f.width,
                            height: f.height,
                        });
                    }
                    // The frame grows to hold all of it again, given on the
                    // page as the Inspector gives frames.
                    (false, Some(m)) => {
                        let f = object.frame;
                        object.frame = Frame::from_pt(
                            (f.x + m.x).to_pt() + dx,
                            (f.y + m.y).to_pt() + dy,
                            m.width.to_pt(),
                            m.height.to_pt(),
                        );
                        object.picture.mask = None;
                    }
                    _ => {}
                });
            }
        });
        for row in [&this.scale, &this.left, &this.top] {
            let weak = Rc::downgrade(&this);
            let editor = editor.clone();
            row.connect_value_notify(move |_| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let (scale, left, top) = (
                    this.scale.value() / 100.0,
                    this.left.value(),
                    this.top.value(),
                );
                if let Some(editor) = editor() {
                    editor.set_object("Mask", |object| {
                        let Some(mask) = object.picture.mask else {
                            return;
                        };
                        // The size is of the width of the frame; the
                        // picture keeps its proportions.
                        let width = object.frame.width.to_pt() * scale;
                        let ratio = if mask.width.to_pt() > 0.0 {
                            mask.height.to_pt() / mask.width.to_pt()
                        } else {
                            1.0
                        };
                        object.picture.mask =
                            Some(Frame::from_pt(-left, -top, width, width * ratio));
                    });
                }
            });
        }
        let alpha_changed = {
            let weak = Rc::downgrade(&this);
            let editor = editor.clone();
            move || {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let alpha = this.alpha.is_active().then(|| InstantAlpha {
                    color: from_gdk(&this.alpha_color.rgba()),
                    tolerance: this.tolerance.value().round().clamp(0.0, 100.0) as u8,
                });
                this.show_rows();
                if let Some(editor) = editor() {
                    editor.set_object("Instant Alpha", |object| object.picture.alpha = alpha);
                }
            }
        };
        let on = alpha_changed.clone();
        this.alpha.connect_active_notify(move |_| on());
        let on = alpha_changed.clone();
        this.alpha_color.connect_rgba_notify(move |_| on());
        let on = alpha_changed;
        this.tolerance.connect_value_notify(move |_| on());
        for row in &this.adjust {
            let weak = Rc::downgrade(&this);
            let editor = editor.clone();
            row.connect_value_notify(move |_| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let adjust = adjustments(&this.adjust);
                if let Some(editor) = editor() {
                    editor.set_object("Adjust Image", |object| object.picture.adjust = adjust);
                }
            });
        }
        let reset_editor = editor.clone();
        this.reset.connect_clicked(move |_| {
            if let Some(editor) = reset_editor() {
                editor.set_object("Reset Image", |object| {
                    object.picture.adjust = Adjustments::default();
                    object.picture.alpha = None;
                });
            }
        });
        this.reduce.connect_clicked(move |_| {
            if let Some(editor) = editor() {
                editor.set_object("Reduce File Size", |object| {
                    let ObjectKind::Image(picture) = &object.kind else {
                        return;
                    };
                    let size = (object.frame.width.to_pt(), object.frame.height.to_pt());
                    if let Some(reduced) =
                        desk_render::reduced_picture(picture, object.picture.mask, size)
                    {
                        object.kind = ObjectKind::Image(reduced);
                        object.picture.mask = None;
                    }
                });
            }
        });
        this
    }

    /// The group, to put in the Inspector.
    pub fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    fn show_rows(&self) {
        let masked = self.mask.is_active();
        for row in [&self.scale, &self.left, &self.top] {
            row.set_visible(masked);
        }
        let alpha = self.alpha.is_active();
        self.alpha_color_row.set_visible(alpha);
        self.tolerance.set_visible(alpha);
    }

    /// Shows the selected picture, or hides the group when no picture is
    /// selected.
    pub fn show(&self, info: Option<&ObjectInfo>) {
        let Some(info) = info.filter(|i| matches!(i.object.kind, ObjectKind::Image(_))) else {
            self.group.set_visible(false);
            return;
        };
        self.group.set_visible(true);
        self.group
            .set_sensitive(info.count == 1 && !info.object.locked);
        self.updating.set(true);
        self.shift.set((
            info.frame.0 - info.object.frame.x.to_pt(),
            info.frame.1 - info.object.frame.y.to_pt(),
        ));
        let set = |row: &adw::SpinRow, value: f64| {
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        };
        let settings = &info.object.picture;
        if self.mask.is_active() != settings.mask.is_some() {
            self.mask.set_active(settings.mask.is_some());
        }
        if let Some(mask) = settings.mask {
            let width = info.object.frame.width.to_pt();
            if width > 0.0 {
                set(&self.scale, mask.width.to_pt() / width * 100.0);
            }
            set(&self.left, -mask.x.to_pt());
            set(&self.top, -mask.y.to_pt());
        }
        if self.alpha.is_active() != settings.alpha.is_some() {
            self.alpha.set_active(settings.alpha.is_some());
        }
        if let Some(alpha) = settings.alpha {
            let color = to_gdk(alpha.color);
            if self.alpha_color.rgba() != color {
                self.alpha_color.set_rgba(&color);
            }
            set(&self.tolerance, f64::from(alpha.tolerance));
        } else {
            set(&self.tolerance, 20.0);
        }
        let a = settings.adjust;
        for (row, value) in self.adjust.iter().zip([
            a.brightness,
            a.contrast,
            a.saturation,
            a.exposure,
            a.temperature,
            a.tint,
        ]) {
            set(row, f64::from(value));
        }
        self.show_rows();
        self.updating.set(false);
    }
}
