// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Drop Cap of the Style tab of text, as in Pages 15.4 (PXV-06): the
//! switch, how many lines tall and how many characters the drop cap of
//! the selected paragraphs is, and its options: the lines it rises above
//! the text, whether the text follows the shape of the letters, whether
//! they stand in the margin, and the space beside them.

use std::cell::Cell;
use std::rc::Rc;

use desk_model::{DropCap, Length, ParaAttrs, ParaFormat};

use crate::compact;
use crate::formatting::EditorOf;
use crate::i18n::tr;

/// The Drop Cap group.
pub struct DropCapGroup {
    group: compact::Group,
    on: compact::SwitchRow,
    lines: compact::SpinRow,
    characters: compact::SpinRow,
    raised: compact::SpinRow,
    follow_shape: compact::SwitchRow,
    outdent: compact::SwitchRow,
    spacing: compact::SpinRow,
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
        let outdent = compact::SwitchRow::builder()
            .title(tr("Outdent"))
            .subtitle(tr("The letters stand in the margin"))
            .build();
        let spacing = spin(&tr("Spacing"), 0.0, 100.0);
        spacing.set_subtitle(&tr("Points"));
        let group = compact::Group::builder().title(tr("Drop Cap")).build();
        group.add(&on);
        group.add(&lines);
        group.add(&characters);
        group.add(&raised);
        group.add(&follow_shape);
        group.add(&outdent);
        group.add(&spacing);
        let this = Rc::new(Self {
            group,
            on,
            lines,
            characters,
            raised,
            follow_shape,
            outdent,
            spacing,
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
        for row in [&this.on, &this.follow_shape, &this.outdent] {
            let apply = apply.clone();
            row.connect_active_notify(move |_| apply());
        }
        for row in [&this.lines, &this.characters, &this.raised, &this.spacing] {
            let apply = apply.clone();
            row.connect_value_notify(move |_| apply());
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
            outdent: self.outdent.is_active(),
        }
    }

    /// Makes the rows of a drop cap usable when there is one.
    fn sensitive(&self, on: bool) {
        for row in [&self.lines, &self.characters, &self.raised, &self.spacing] {
            row.set_sensitive(on);
        }
        self.follow_shape.set_sensitive(on);
        self.outdent.set_sensitive(on);
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
        for (row, value) in [
            (&self.follow_shape, shown.follow_shape),
            (&self.outdent, shown.outdent),
        ] {
            if row.is_active() != value {
                row.set_active(value);
            }
        }
        self.sensitive(cap.is_some());
        self.updating.set(false);
    }
}
