// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Drop Cap of the Style tab of text, as in Pages 15.4 (PXV-06): the
//! switch, how many lines tall and how many characters the drop cap of
//! the selected paragraphs is, and its options: the lines it rises above
//! the text, whether the text follows the shape of the letters, whether
//! they stand in the margin, the space beside them, and the shape behind
//! them.

use std::cell::Cell;
use std::rc::Rc;

use desk_model::{DropCap, DropCapShape, Length, ParaAttrs, ParaFormat};

use crate::compact;
use crate::effects::{ColorRow, STYLES, style_name};
use crate::formatting::EditorOf;
use crate::i18n::tr;
use crate::tab_stops::string_list;

/// The Drop Cap group.
pub struct DropCapGroup {
    group: compact::Group,
    on: compact::SwitchRow,
    lines: compact::SpinRow,
    characters: compact::SpinRow,
    raised: compact::SpinRow,
    follow_shape: compact::SwitchRow,
    outdent: compact::SpinRow,
    spacing: compact::SpinRow,
    shape: ColorRow,
    border: ColorRow,
    border_width: compact::SpinRow,
    border_style: compact::ComboRow,
    scale: compact::SpinRow,
    radius: compact::SpinRow,
    /// Whether the rows are being set from the selection.
    updating: Cell<bool>,
}

impl DropCapGroup {
    /// The group, which sets the drop cap of the paragraphs of an editor.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let on = compact::SwitchRow::builder()
            .title(tr("Drop Cap"))
            .subtitle(tr("The first letters as tall as several lines"))
            .build();
        let spin = |title: &str, min: f64, max: f64| {
            let row = compact::SpinRow::with_range(min, max, 1.0);
            row.set_title(title);
            row.set_digits(0);
            row
        };
        let lines = spin(&tr("Lines"), 2.0, 20.0);
        let characters = spin(&tr("Characters"), 1.0, 20.0);
        let raised = spin(&tr("Raised Lines"), 0.0, 19.0);
        raised.set_subtitle(&tr("Above the first line of the text"));
        let follow_shape = compact::SwitchRow::builder()
            .title(tr("Follow the Shape"))
            .subtitle(tr("The text beside the letters follows their outline"))
            .build();
        let outdent = spin(&tr("Outdent"), 0.0, 200.0);
        outdent.set_subtitle(&tr("Percent of the width of the letters, in the margin"));
        let spacing = spin(&tr("Spacing"), 0.0, 1000.0);
        spacing.set_subtitle(&tr("Points"));
        let start = DropCapShape::default();
        let shape = ColorRow::new(&tr("Background Shape"), start.fill);
        let border = ColorRow::new(&tr("Border"), start.border_color);
        let border_width = spin(&tr("Border Width"), 0.0, 20.0);
        border_width.set_subtitle(&tr("Points"));
        let border_style = compact::ComboRow::builder()
            .title(tr("Border Style"))
            .model(&string_list(STYLES.map(style_name)))
            .build();
        let scale = spin(&tr("Letter Size"), 10.0, 100.0);
        scale.set_subtitle(&tr("Percent of the shape"));
        let radius = spin(&tr("Corner Radius"), 0.0, 100.0);
        radius.set_subtitle(&tr("Percent"));
        let group = compact::Group::builder().title(tr("Drop Cap")).build();
        group.add(&on);
        group.add(&lines);
        group.add(&characters);
        group.add(&raised);
        group.add(&follow_shape);
        group.add(&outdent);
        group.add(&spacing);
        group.add(&shape.row);
        group.add(&scale);
        group.add(&radius);
        group.add(&border.row);
        group.add(&border_width);
        group.add(&border_style);
        let this = Rc::new(Self {
            group,
            on,
            lines,
            characters,
            raised,
            follow_shape,
            outdent,
            spacing,
            shape,
            border,
            border_width,
            border_style,
            scale,
            radius,
            updating: Cell::new(false),
        });
        let apply = {
            let weak = Rc::downgrade(&this);
            move || {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let Some(editor) = editor() else { return };
                let cap = this.on.is_active().then(|| this.cap());
                this.sensitive(cap.is_some());
                editor.set_paragraph_attrs(
                    "Drop Cap",
                    &ParaAttrs {
                        drop_cap: Some(cap),
                        ..ParaAttrs::default()
                    },
                );
            }
        };
        {
            let apply = apply.clone();
            this.border_style.connect_selected_notify(move |_| apply());
        }
        for row in [&this.on, &this.follow_shape] {
            let apply = apply.clone();
            row.connect_active_notify(move |_| apply());
        }
        for row in [
            &this.lines,
            &this.characters,
            &this.raised,
            &this.spacing,
            &this.outdent,
            &this.border_width,
            &this.scale,
            &this.radius,
        ] {
            let apply = apply.clone();
            row.connect_value_notify(move |_| apply());
        }
        for row in [&this.shape, &this.border] {
            row.connect_changed(apply.clone());
        }
        this
    }

    /// The drop cap that the rows set.
    fn cap(&self) -> DropCap {
        let lines = self.lines.value().round().clamp(2.0, 255.0) as u8;
        DropCap {
            lines,
            characters: self.characters.value().round().clamp(1.0, 255.0) as u8,
            distance: Length::from_pt(self.spacing.value().max(0.0)),
            raised: (self.raised.value().round().max(0.0) as u8).min(lines - 1),
            follow_shape: self.follow_shape.is_active(),
            outdent: self.outdent.value().round().clamp(0.0, 200.0) as u8,
            background: self.shape.value().map(|fill| {
                let border = self.border.value();
                DropCapShape {
                    fill,
                    border_width: Length::from_pt(match border {
                        Some(_) => self.border_width.value().max(0.0),
                        None => 0.0,
                    }),
                    border_color: border.unwrap_or(DropCapShape::default().border_color),
                    border_style: STYLES
                        [(self.border_style.selected() as usize).min(STYLES.len() - 1)],
                    scale: self.scale.value().round().clamp(10.0, 100.0) as u8,
                    radius: self.radius.value().round().clamp(0.0, 100.0) as u8,
                }
            }),
        }
    }

    /// Makes the rows of a drop cap usable when there is one.
    fn sensitive(&self, on: bool) {
        for row in [
            &self.lines,
            &self.characters,
            &self.raised,
            &self.spacing,
            &self.outdent,
        ] {
            row.set_sensitive(on);
        }
        self.follow_shape.set_sensitive(on);
        self.shape.row.set_sensitive(on);
        let shape = on && self.shape.value().is_some();
        for row in [&self.scale, &self.radius] {
            row.set_sensitive(shape);
        }
        self.border.row.set_sensitive(shape);
        let line = shape && self.border.value().is_some();
        self.border_width.set_sensitive(line);
        self.border_style.set_sensitive(line);
    }

    /// The group, to put in the Style tab.
    pub fn group(&self) -> &compact::Group {
        &self.group
    }

    /// Shows the drop cap of the paragraph at the selection.
    pub fn show(&self, paragraph: &ParaFormat) {
        self.updating.set(true);
        let cap = paragraph.drop_cap;
        if self.on.is_active() != cap.is_some() {
            self.on.set_active(cap.is_some());
        }
        let shown = cap.unwrap_or_default();
        let set = |row: &compact::SpinRow, value: f64| {
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        };
        set(&self.lines, f64::from(shown.lines));
        set(&self.characters, f64::from(shown.characters));
        set(&self.raised, f64::from(shown.raised));
        set(&self.spacing, shown.distance.to_pt());
        set(&self.outdent, f64::from(shown.outdent));
        let background = shown.background;
        let start = background.unwrap_or_default();
        self.shape.show(background.map(|b| b.fill));
        let border = background.filter(|b| b.border_width > Length::ZERO);
        self.border.show(border.map(|b| b.border_color));
        set(
            &self.border_width,
            border.map_or(2.0, |b| b.border_width.to_pt()),
        );
        let style = STYLES
            .iter()
            .position(|s| *s == start.border_style)
            .unwrap_or_default() as u32;
        if self.border_style.selected() != style {
            self.border_style.set_selected(style);
        }
        set(&self.scale, f64::from(start.scale));
        set(&self.radius, f64::from(start.radius));
        if self.follow_shape.is_active() != shown.follow_shape {
            self.follow_shape.set_active(shown.follow_shape);
        }
        self.sensitive(cap.is_some());
        self.updating.set(false);
    }
}
