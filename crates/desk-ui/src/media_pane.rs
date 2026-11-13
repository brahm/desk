// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QuickTime inspector of Pages, for a selected movie or sound
//! (OBJ-19): where it starts and stops, its poster frame, how it repeats,
//! how loud it plays, and a button to play it. Reduce Media takes out the
//! parts before the start and after the stop (OBJ-20).

use crate::compact;
use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Media, ObjectKind, Repeat};

use crate::editing::ObjectInfo;
use crate::formatting::EditorOf;
use crate::i18n::tr;

/// The longest a movie is taken to last when its length is not known, in
/// seconds.
const LONGEST: f64 = 24.0 * 3600.0;

/// The Media group of the Inspector.
pub struct MediaGroup {
    group: compact::Group,
    start: compact::SpinRow,
    stop: compact::SpinRow,
    poster: compact::SpinRow,
    repeat: compact::ComboRow,
    volume: compact::SpinRow,
    reduce: compact::ActionRow,
    updating: Cell<bool>,
}

/// What a row sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Setting {
    Start,
    Stop,
    Repeat,
    Volume,
}

/// Seconds as milliseconds.
fn millis(seconds: f64) -> u32 {
    (seconds.max(0.0) * 1000.0).round().min(f64::from(u32::MAX)) as u32
}

/// The repeats in the order of the menu.
const REPEATS: [Repeat; 3] = [Repeat::None, Repeat::Loop, Repeat::BackAndForth];

/// A movie or a sound with a setting changed: the start stays before the
/// stop, and a stop at the end is no stop.
fn changed(media: &Media, setting: Setting, value: f64) -> Media {
    let mut media = media.clone();
    match setting {
        Setting::Start => {
            let end = media.end().unwrap_or(u32::MAX);
            media.start = millis(value).min(end.saturating_sub(1));
        }
        Setting::Stop => {
            let stop = millis(value).max(media.start + 1);
            media.stop = (media.duration != Some(stop) && media.duration.is_none_or(|d| stop < d))
                .then_some(stop);
        }
        Setting::Repeat => {
            media.repeat = REPEATS.get(value as usize).copied().unwrap_or(Repeat::None);
        }
        Setting::Volume => media.volume = value.round().clamp(0.0, 100.0) as u8,
    }
    media
}

impl MediaGroup {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let seconds = |title: String| {
            let row = compact::SpinRow::with_range(0.0, LONGEST, 0.1);
            row.set_title(&title);
            row.set_subtitle(&tr("Seconds"));
            row.set_digits(1);
            row
        };
        let start = seconds(tr("Start"));
        let stop = seconds(tr("Stop"));
        let poster = seconds(tr("Poster Frame"));
        let names = [tr("None"), tr("Loop"), tr("Loop Back and Forth")];
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let repeat = compact::ComboRow::builder()
            .title(tr("Repeat"))
            .model(&gtk::StringList::new(&names))
            .build();
        let volume = compact::SpinRow::with_range(0.0, 100.0, 5.0);
        volume.set_title(&tr("Volume"));
        volume.set_subtitle(&tr("Percent"));
        let play = gtk::Button::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text(tr("Play"))
            .valign(gtk::Align::Center)
            .build();
        play.add_css_class("flat");
        play.update_property(&[gtk::accessible::Property::Label(&tr("Play"))]);
        let group = compact::Group::builder()
            .title(tr("Movie and Sound"))
            .header_suffix(&play)
            .build();
        for row in [&start, &stop, &poster] {
            group.add(row);
        }
        group.add(&repeat);
        group.add(&volume);
        let reduce_button = gtk::Button::builder()
            .label(tr("Reduce"))
            .valign(gtk::Align::Center)
            .build();
        let reduce = compact::ActionRow::builder()
            .title(tr("Reduce Media"))
            .subtitle(tr("Removes the parts outside the start and stop"))
            .activatable_widget(&reduce_button)
            .build();
        reduce.add_suffix(&reduce_button);
        group.add(&reduce);
        let this = Rc::new(Self {
            group,
            start,
            stop,
            poster,
            repeat,
            volume,
            reduce,
            updating: Cell::new(false),
        });
        this.show(None);
        let set = {
            let editor = editor.clone();
            let weak = Rc::downgrade(&this);
            Rc::new(move |setting: Setting, value: f64| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    editor.set_object("Media", |object| {
                        if let ObjectKind::Media(media) = &object.kind {
                            object.kind = ObjectKind::Media(changed(media, setting, value));
                        }
                    });
                }
            })
        };
        for (row, setting) in [
            (&this.start, Setting::Start),
            (&this.stop, Setting::Stop),
            (&this.volume, Setting::Volume),
        ] {
            let set = set.clone();
            row.connect_value_notify(move |row| set(setting, row.value()));
        }
        {
            let set = set.clone();
            this.repeat.connect_selected_notify(move |row| {
                set(Setting::Repeat, f64::from(row.selected()))
            });
        }
        {
            let editor = editor.clone();
            let weak = Rc::downgrade(&this);
            this.poster.connect_value_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    editor.set_poster_time(millis(row.value()));
                }
            });
        }
        {
            let editor = editor.clone();
            let weak = Rc::downgrade(&this);
            reduce_button.connect_clicked(move |_| {
                let (Some(this), Some(editor)) = (weak.upgrade(), editor()) else {
                    return;
                };
                let reduced = editor
                    .object_info()
                    .and_then(|info| match &info.object.kind {
                        ObjectKind::Media(media) => media.reduced(),
                        _ => None,
                    });
                match reduced {
                    Some(reduced) => editor.set_object("Reduce Media", |object| {
                        object.kind = ObjectKind::Media(reduced);
                    }),
                    None => this.reduce.set_subtitle(&tr("This file cannot be reduced")),
                }
            });
        }
        play.connect_clicked(move |_| {
            if let Some(editor) = editor() {
                editor.play_selected_media();
            }
        });
        this
    }

    /// The group, to put in the Inspector.
    pub fn group(&self) -> &compact::Group {
        &self.group
    }

    /// Shows how the selected movie or sound plays, or hides the group
    /// when none is selected.
    pub fn show(&self, info: Option<&ObjectInfo>) {
        let media = match info.map(|i| &i.object.kind) {
            Some(ObjectKind::Media(media)) => Some(media),
            _ => None,
        };
        self.group.set_visible(media.is_some());
        let (Some(info), Some(media)) = (info, media) else {
            return;
        };
        self.group
            .set_sensitive(info.count == 1 && !info.object.locked);
        self.updating.set(true);
        let length = media.duration.map_or(LONGEST, |d| f64::from(d) / 1000.0);
        for row in [&self.start, &self.stop, &self.poster] {
            row.adjustment().set_upper(length);
        }
        let seconds = |ms: u32| f64::from(ms) / 1000.0;
        self.start.set_value(seconds(media.start));
        self.stop.set_value(media.end().map_or(length, seconds));
        self.poster.set_value(seconds(media.poster_time));
        // A sound has no frame to show.
        self.poster.set_visible(!media.is_sound());
        let repeat = REPEATS.iter().position(|r| *r == media.repeat).unwrap_or(0);
        self.repeat.set_selected(repeat as u32);
        self.volume.set_value(f64::from(media.volume));
        self.reduce.set_sensitive(media.reducible());
        self.reduce
            .set_subtitle(&tr("Removes the parts outside the start and stop"));
        self.updating.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_19_the_start_stays_before_the_stop() {
        let mut media = Media::from_bytes(b"ID3\x04\0\0\0\0\0\0".to_vec()).unwrap();
        media.duration = Some(10_000);
        let started = changed(&media, Setting::Start, 2.5);
        assert_eq!(started.start, 2_500);
        let stopped = changed(&started, Setting::Stop, 1.0);
        assert_eq!(stopped.stop, Some(2_501));
        // A stop at the end is the end.
        assert_eq!(changed(&started, Setting::Stop, 10.0).stop, None);
        let late = changed(&stopped, Setting::Start, 9.0);
        assert_eq!(late.start, 2_500);
        assert_eq!(
            changed(&media, Setting::Repeat, 2.0).repeat,
            Repeat::BackAndForth
        );
        assert_eq!(changed(&media, Setting::Volume, 140.0).volume, 100);
    }
}
