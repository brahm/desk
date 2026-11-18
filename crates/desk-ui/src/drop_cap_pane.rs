// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Drop Cap of the Style tab of text, as in Pages 15.4 (PXV-06): the
//! switch, and how many lines tall and how many characters the drop cap
//! of the selected paragraphs is.

use std::cell::Cell;
use std::rc::Rc;

use desk_model::{DropCap, ParaAttrs, ParaFormat};

use crate::compact;
use crate::formatting::EditorOf;
use crate::i18n::tr;

/// The Drop Cap group.
pub struct DropCapGroup {
    group: compact::Group,
    on: compact::SwitchRow,
    lines: compact::SpinRow,
    characters: compact::SpinRow,
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
        let spin = |title: &str, min: f64| {
            let row = compact::SpinRow::with_range(min, 20.0, 1.0);
            row.set_title(title);
            row.set_digits(0);
            row
        };
        let lines = spin(&tr("Lines"), 2.0);
        let characters = spin(&tr("Characters"), 1.0);
        let group = compact::Group::builder().title(tr("Drop Cap")).build();
        group.add(&on);
        group.add(&lines);
        group.add(&characters);
        let this = Rc::new(Self {
            group,
            on,
            lines,
            characters,
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
                let cap = this.on.is_active().then(|| DropCap {
                    lines: this.lines.value().round().clamp(2.0, 255.0) as u8,
                    characters: this.characters.value().round().clamp(1.0, 255.0) as u8,
                    ..editor.format().paragraph.drop_cap.unwrap_or_default()
                });
                this.lines.set_sensitive(cap.is_some());
                this.characters.set_sensitive(cap.is_some());
                editor.set_paragraph_attrs(
                    "Drop Cap",
                    &ParaAttrs {
                        drop_cap: Some(cap),
                        ..ParaAttrs::default()
                    },
                );
            }
        };
        let on_switch = apply.clone();
        this.on.connect_active_notify(move |_| on_switch());
        let on_lines = apply.clone();
        this.lines.connect_value_notify(move |_| on_lines());
        this.characters.connect_value_notify(move |_| apply());
        this
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
        let set = |row: &compact::SpinRow, value: u8| {
            if (row.value() - f64::from(value)).abs() > 0.001 {
                row.set_value(f64::from(value));
            }
        };
        set(&self.lines, shown.lines);
        set(&self.characters, shown.characters);
        self.lines.set_sensitive(cap.is_some());
        self.characters.set_sensitive(cap.is_some());
        self.updating.set(false);
    }
}
