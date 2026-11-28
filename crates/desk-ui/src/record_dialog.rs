// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Record Audio of Insert in Pages 15.4 (W118): a button that records
//! from the microphone and stops, how long and how loud, Preview to hear
//! it, and Insert, which puts the sound on the page as a speaker.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use desk_model::Media;
use gtk::glib;

use crate::i18n::tr;
use crate::recorder::{Event, Recorder};

/// Minutes and seconds, as `1:05`.
fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The dialog, while it is open.
struct Dialog {
    dialog: adw::Dialog,
    record: gtk::Button,
    time: gtk::Label,
    level: gtk::LevelBar,
    status: gtk::Label,
    preview: gtk::Button,
    insert: gtk::Button,
    source: String,
    recorder: RefCell<Option<Rc<Recorder>>>,
    recorded: RefCell<Option<Media>>,
    player: RefCell<Option<gtk::MediaFile>>,
    closed: Cell<bool>,
}

impl Dialog {
    /// Shows that it records, or what it recorded.
    fn show(&self) {
        let recording = self.recorder.borrow().is_some();
        let recorded = self.recorded.borrow().clone();
        let (icon, label) = if recording {
            ("media-playback-stop-symbolic", tr("Stop Recording"))
        } else {
            ("media-record-symbolic", tr("Start Recording"))
        };
        self.record.set_icon_name(icon);
        self.record.set_tooltip_text(Some(&label));
        self.record
            .update_property(&[gtk::accessible::Property::Label(&label)]);
        if !recording {
            self.level.set_value(0.0);
        }
        self.preview.set_sensitive(!recording && recorded.is_some());
        self.insert.set_sensitive(!recording && recorded.is_some());
        if let (false, Some(media)) = (recording, &recorded) {
            let length = Duration::from_millis(u64::from(media.duration.unwrap_or(0)));
            self.time.set_label(&clock(length));
        }
    }

    /// Records, or stops recording.
    fn toggle(self: &Rc<Self>) {
        self.stop_preview();
        if let Some(recorder) = self.recorder.borrow().clone() {
            recorder.stop();
            self.record.set_sensitive(false);
            return;
        }
        let weak = Rc::downgrade(self);
        let started = Recorder::start(&self.source, move |event| {
            let Some(this) = weak.upgrade() else { return };
            match event {
                Event::Level(level) => {
                    this.level.set_value(level);
                    if let Some(recorder) = &*this.recorder.borrow() {
                        this.time.set_label(&clock(recorder.elapsed()));
                    }
                }
                Event::Done(result) => this.done(result),
            }
        });
        match started {
            Ok(recorder) => {
                self.recorder.replace(Some(recorder));
                self.status.set_visible(false);
                self.time.set_label(&clock(Duration::ZERO));
            }
            Err(error) => self.fail(&error),
        }
        self.show();
    }

    /// Takes what was recorded.
    fn done(&self, result: Result<Vec<u8>, String>) {
        self.recorder.replace(None);
        self.record.set_sensitive(true);
        match result {
            Ok(bytes) => match Media::from_bytes(bytes.clone()) {
                Some(mut media) => {
                    media.duration = crate::recorder::wave_length(&bytes);
                    self.recorded.replace(Some(media));
                }
                None => self.fail(&tr("Nothing was recorded")),
            },
            Err(error) => self.fail(&error),
        }
        self.show();
    }

    fn fail(&self, error: &str) {
        self.status
            .set_label(&tr("Desk cannot record: {error}").replace("{error}", error));
        self.status.set_visible(true);
    }

    /// Plays what was recorded, or stops playing it.
    fn preview(&self) {
        if self.stop_preview() {
            return;
        }
        let Some(file) = self
            .recorded
            .borrow()
            .as_ref()
            .and_then(crate::media::file_of)
        else {
            return;
        };
        let player = gtk::MediaFile::for_file(&file);
        player.play();
        self.preview.set_label(&tr("Stop"));
        self.player.replace(Some(player));
    }

    /// Stops the preview; whether one was playing.
    fn stop_preview(&self) -> bool {
        self.preview.set_label(&tr("Preview"));
        match self.player.take() {
            Some(player) => {
                player.pause();
                true
            }
            None => false,
        }
    }
}

/// Opens Record Audio over `parent`, recording from `source`, as
/// [`crate::recorder::MICROPHONE`]; Insert calls `insert` with the sound.
pub fn open(parent: &impl IsA<gtk::Widget>, source: &str, on_insert: impl Fn(Media) + 'static) {
    let record = gtk::Button::builder()
        .icon_name("media-record-symbolic")
        .halign(gtk::Align::Center)
        .width_request(64)
        .height_request(64)
        .css_classes(["circular", "destructive-action"])
        .build();
    let time = gtk::Label::builder()
        .label(clock(Duration::ZERO))
        .css_classes(["title-1", "numeric"])
        .build();
    let level = gtk::LevelBar::builder()
        .min_value(0.0)
        .max_value(1.0)
        .build();
    level.update_property(&[gtk::accessible::Property::Label(&tr("Loudness"))]);
    let status = gtk::Label::builder()
        .wrap(true)
        .visible(false)
        .css_classes(["error"])
        .build();
    let hint = gtk::Label::builder()
        .label(tr("Records from the microphone of the computer"))
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    for widget in [
        record.upcast_ref::<gtk::Widget>(),
        time.upcast_ref(),
        level.upcast_ref(),
        hint.upcast_ref(),
        status.upcast_ref(),
    ] {
        content.append(widget);
    }
    let preview = gtk::Button::builder()
        .label(tr("Preview"))
        .sensitive(false)
        .build();
    let insert = gtk::Button::builder()
        .label(tr("_Insert"))
        .use_underline(true)
        .sensitive(false)
        .css_classes(["suggested-action"])
        .build();
    let footer = gtk::Box::builder()
        .spacing(6)
        .halign(gtk::Align::End)
        .margin_start(18)
        .margin_end(18)
        .margin_bottom(18)
        .build();
    footer.append(&preview);
    footer.append(&insert);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&content));
    view.add_bottom_bar(&footer);
    let dialog = adw::Dialog::builder()
        .title(tr("Record Audio"))
        .content_width(320)
        .child(&view)
        .build();
    let this = Rc::new(Dialog {
        dialog,
        record,
        time,
        level,
        status,
        preview,
        insert,
        source: source.to_owned(),
        recorder: RefCell::new(None),
        recorded: RefCell::new(None),
        player: RefCell::new(None),
        closed: Cell::new(false),
    });
    this.show();
    {
        let weak = Rc::downgrade(&this);
        this.record.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.toggle();
            }
        });
    }
    {
        let weak = Rc::downgrade(&this);
        this.preview.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.preview();
            }
        });
    }
    {
        let weak = Rc::downgrade(&this);
        this.insert.connect_clicked(move |_| {
            let Some(this) = weak.upgrade() else { return };
            this.stop_preview();
            if let Some(media) = this.recorded.take() {
                on_insert(media);
            }
            this.dialog.close();
        });
    }
    {
        // What is recorded and not inserted goes with the dialog, which
        // keeps the rest until it closes.
        let keep = RefCell::new(Some(this.clone()));
        this.dialog.clone().connect_closed(move |_| {
            let Some(this) = keep.take() else { return };
            this.closed.set(true);
            this.stop_preview();
            if let Some(recorder) = this.recorder.take() {
                recorder.cancel();
            }
            this.recorded.replace(None);
        });
    }
    // While it records, the time goes on between the levels.
    {
        let weak = Rc::downgrade(&this);
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let Some(this) = weak.upgrade().filter(|this| !this.closed.get()) else {
                return glib::ControlFlow::Break;
            };
            if let Some(recorder) = &*this.recorder.borrow() {
                this.time.set_label(&clock(recorder.elapsed()));
            }
            glib::ControlFlow::Continue
        });
    }
    this.dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w118_the_time_recorded_shows_in_minutes_and_seconds() {
        assert_eq!(clock(Duration::from_millis(900)), "0:00");
        assert_eq!(clock(Duration::from_secs(65)), "1:05");
        assert_eq!(clock(Duration::from_secs(600)), "10:00");
    }
}
