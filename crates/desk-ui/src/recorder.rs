// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Recording sound from the microphone, as Insert > Record Audio of Pages
//! 15.4 does (W118), by GStreamer, which plays movies and sounds through
//! GTK already. The sound is written as WAVE, which Reduce Media cuts, in
//! the cache of the user until it goes into the document.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gtk::glib;

/// Where the sound comes from: the microphone that the desktop chose.
pub const MICROPHONE: &str = "autoaudiosrc";

/// The rate and channels of the sound recorded, as one voice needs.
const RATE: u32 = 44_100;

/// How often the loudness is told, in nanoseconds.
const LEVEL_INTERVAL: u64 = 50_000_000;

/// The longest a recording may go on, as a document keeps it whole.
pub const LONGEST: Duration = Duration::from_secs(60 * 60);

/// How long stopping may take before the recording is left as it is.
const STOP_TIME: Duration = Duration::from_secs(5);

/// What a recording tells as it goes.
pub enum Event {
    /// How loud the sound is, from 0 to 1.
    Level(f64),
    /// That it stopped with what it recorded, a WAVE file, or with an
    /// error.
    Done(Recorded),
}

/// What is told of a recording.
type Told = Rc<dyn Fn(Event)>;

/// What a recording ended with: a WAVE file, or an error.
type Recorded = Result<Vec<u8>, String>;

/// A recording.
pub struct Recorder {
    pipeline: gst::Element,
    path: PathBuf,
    started: Instant,
    stopping: Cell<bool>,
    finished: Cell<bool>,
    watch: RefCell<Option<gst::bus::BusWatchGuard>>,
    told: RefCell<Option<Told>>,
}

/// The loudness of a `level` message, from the decibels of its channels.
fn loudness(structure: &gst::StructureRef) -> Option<f64> {
    #[allow(deprecated)]
    let rms = structure.get::<glib::ValueArray>("rms").ok()?;
    let db = rms
        .iter()
        .filter_map(|v| v.get::<f64>().ok())
        .fold(f64::NEG_INFINITY, f64::max);
    // From -60 dB, near silence, to 0 dB.
    Some(if db.is_finite() {
        ((db + 60.0) / 60.0).clamp(0.0, 1.0)
    } else {
        0.0
    })
}

impl Recorder {
    /// Starts recording from `source`, a GStreamer element, as
    /// [`MICROPHONE`], telling `told` how loud it is and when it is done.
    pub fn start(source: &str, told: impl Fn(Event) + 'static) -> Result<Rc<Self>, String> {
        gst::init().map_err(|e| e.to_string())?;
        let dir = glib::user_cache_dir().join("desk").join("media");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!(
            "recording-{}-{}.wav",
            std::process::id(),
            glib::monotonic_time()
        ));
        let description = format!(
            "{source} ! audioconvert ! audioresample \
             ! audio/x-raw,format=S16LE,rate={RATE},channels=1 \
             ! level interval={LEVEL_INTERVAL} post-messages=true \
             ! wavenc ! filesink name=sink"
        );
        let pipeline = gst::parse::launch(&description).map_err(|e| e.to_string())?;
        let sink = pipeline
            .downcast_ref::<gst::Bin>()
            .and_then(|bin| bin.by_name("sink"))
            .ok_or("no file to write")?;
        sink.set_property("location", path.to_string_lossy().as_ref());
        let this = Rc::new(Self {
            pipeline,
            path,
            started: Instant::now(),
            stopping: Cell::new(false),
            finished: Cell::new(false),
            watch: RefCell::new(None),
            told: RefCell::new(Some(Rc::new(told))),
        });
        let bus = this.pipeline.bus().ok_or("no bus")?;
        let weak = Rc::downgrade(&this);
        let watch = bus
            .add_watch_local(move |_, message| {
                let Some(this) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                match message.view() {
                    gst::MessageView::Element(element) => {
                        if let Some(level) = element
                            .structure()
                            .filter(|s| s.name() == "level")
                            .and_then(loudness)
                        {
                            this.tell(Event::Level(level));
                        }
                        if this.elapsed() >= LONGEST {
                            this.stop();
                        }
                    }
                    gst::MessageView::Eos(_) => this.finish(None),
                    gst::MessageView::Error(error) => {
                        this.finish(Some(error.error().to_string()));
                    }
                    _ => {}
                }
                glib::ControlFlow::Continue
            })
            .map_err(|e| e.to_string())?;
        this.watch.replace(Some(watch));
        if this.pipeline.set_state(gst::State::Playing).is_err() {
            let _ = this.pipeline.set_state(gst::State::Null);
            let _ = std::fs::remove_file(&this.path);
            return Err("the microphone cannot be opened".into());
        }
        Ok(this)
    }

    /// How long it has recorded.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Stops recording: the sound recorded comes with [`Event::Done`].
    pub fn stop(self: &Rc<Self>) {
        if self.stopping.replace(true) || self.finished.get() {
            return;
        }
        self.pipeline.send_event(gst::event::Eos::new());
        // A source that never ends the stream is stopped anyway.
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(STOP_TIME, move || {
            if let Some(this) = weak.upgrade() {
                this.finish(None);
            }
        });
    }

    /// Stops recording and forgets what was recorded.
    pub fn cancel(&self) {
        self.told.replace(None);
        self.finish(None);
    }

    fn tell(&self, event: Event) {
        // Taken out first, as what it does may stop the recording.
        let told = self.told.borrow().clone();
        if let Some(told) = told {
            told(event);
        }
    }

    fn finish(&self, error: Option<String>) {
        if self.finished.replace(true) {
            return;
        }
        let _ = self.pipeline.set_state(gst::State::Null);
        self.watch.replace(None);
        let read = std::fs::read(&self.path);
        let _ = std::fs::remove_file(&self.path);
        let result = match (error, read) {
            (Some(error), _) => Err(error),
            (None, Ok(bytes)) => Ok(bytes),
            (None, Err(error)) => Err(error.to_string()),
        };
        self.tell(Event::Done(result));
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if !self.finished.get() {
            let _ = self.pipeline.set_state(gst::State::Null);
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// How long a WAVE file of the recorder lasts, in milliseconds.
pub fn wave_length(bytes: &[u8]) -> Option<u32> {
    // The `data` chunk after the header of 44 bytes that wavenc writes,
    // of 16-bit samples of one channel.
    let at = bytes.windows(4).position(|w| w == b"data")?;
    let size = bytes.get(at + 4..at + 8)?;
    let size = u32::from_le_bytes(size.try_into().ok()?);
    let size = size.min(u32::try_from(bytes.len().saturating_sub(at + 8)).unwrap_or(u32::MAX));
    Some((u64::from(size) * 1000 / (u64::from(RATE) * 2)) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w118_a_recording_is_a_wave_file_of_its_length() {
        // On the thread of GTK, which holds the main loop of the tests.
        crate::with_gtk(|| {
            // A tone of a tenth of a second, as the microphone would give.
            let done: Rc<RefCell<Option<Recorded>>> = Rc::default();
            let levels = Rc::new(Cell::new(0));
            let recorder = {
                let (done, levels) = (done.clone(), levels.clone());
                Recorder::start(
                    "audiotestsrc num-buffers=10 samplesperbuffer=441",
                    move |event| match event {
                        Event::Level(level) => {
                            assert!((0.0..=1.0).contains(&level));
                            levels.set(levels.get() + 1);
                        }
                        Event::Done(result) => {
                            done.replace(Some(result));
                        }
                    },
                )
                .unwrap()
            };
            let context = glib::MainContext::default();
            let until = Instant::now() + Duration::from_secs(10);
            while done.borrow().is_none() && Instant::now() < until {
                context.iteration(false);
            }
            let bytes = done.take().unwrap().unwrap();
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(
                desk_model::MediaFormat::sniff(&bytes),
                Some(desk_model::MediaFormat::Wav)
            );
            assert_eq!(wave_length(&bytes), Some(100));
            assert!(levels.get() > 0);
            drop(recorder);
        });
    }
}
