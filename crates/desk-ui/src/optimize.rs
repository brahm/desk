// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Optimizing movies and sounds, as Pages 15.4 does for iPhone and iPad
//! (W119): a movie coded again at most 1280 × 720 into MPEG-4, and a sound
//! of samples (WAVE, AIFF) into MP3, by the encoders of GStreamer that
//! the computer has. The file is kept only when it comes out smaller.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use desk_model::{Media, MediaFormat};
use gstreamer as gst;
use gstreamer::prelude::*;
use gtk::glib;
use gtk::prelude::FileExt;

/// The largest a movie is made, in pixels, as Pages makes it for iPhone
/// and iPad: the long side and the short side.
const LARGEST: (f64, f64) = (1280.0, 720.0);

/// The encoders of pictures that MPEG-4 holds, with their settings, best
/// first; the first the computer has is used.
const VIDEO_ENCODERS: [(&str, &[(&str, &str)]); 3] = [
    (
        "x264enc",
        &[("bitrate", "2500"), ("speed-preset", "veryfast")],
    ),
    ("avenc_mpeg4", &[("bitrate", "2500000")]),
    (
        "vp9enc",
        &[
            ("target-bitrate", "2500000"),
            ("deadline", "1"),
            ("cpu-used", "8"),
        ],
    ),
];

/// The encoders of sound that MPEG-4 holds, best first.
const AUDIO_ENCODERS: [(&str, &[(&str, &str)]); 4] = [
    ("avenc_aac", &[("bitrate", "128000")]),
    ("fdkaacenc", &[("bitrate", "128000")]),
    ("voaacenc", &[("bitrate", "128000")]),
    ("opusenc", &[("bitrate", "128000")]),
];

/// The encoder of an optimized sound.
const SOUND_ENCODER: (&str, &[(&str, &str)]) = (
    "lamemp3enc",
    &[("target", "bitrate"), ("bitrate", "128"), ("cbr", "true")],
);

/// An encoder with its settings.
type Encoder = (&'static str, &'static [(&'static str, &'static str)]);

/// The first of `encoders` that the computer has.
fn found(encoders: &[Encoder]) -> Option<Encoder> {
    gst::init().ok()?;
    encoders
        .iter()
        .copied()
        .find(|(name, _)| gst::ElementFactory::find(name).is_some())
}

/// What a file is coded into, or none when it cannot be optimized.
#[derive(Clone, Copy)]
enum Plan {
    Movie {
        video: Encoder,
        audio: Option<Encoder>,
    },
    Sound(Encoder),
}

fn plan(media: &Media) -> Option<Plan> {
    match media.format() {
        // Sounds of samples become MP3; MP3 and AAC are small already.
        MediaFormat::Wav | MediaFormat::Aiff => found(&[SOUND_ENCODER]).map(Plan::Sound),
        MediaFormat::QuickTime | MediaFormat::Mp4 | MediaFormat::WebM => Some(Plan::Movie {
            video: found(&VIDEO_ENCODERS)?,
            audio: found(&AUDIO_ENCODERS),
        }),
        // An Ogg file may be a sound or a movie, and an M4A or MP3 sound
        // is small already.
        MediaFormat::Ogg | MediaFormat::M4a | MediaFormat::Mp3 => None,
    }
}

/// Whether the movie or sound can be optimized on this computer.
pub fn optimizable(media: &Media) -> bool {
    plan(media).is_some()
}

/// The size a movie of `width` by `height` is made, in even pixels.
fn optimized_size(width: i32, height: i32) -> (i32, i32) {
    let (w, h) = (f64::from(width.max(2)), f64::from(height.max(2)));
    let (long, short) = if w >= h {
        LARGEST
    } else {
        (LARGEST.1, LARGEST.0)
    };
    let scale = (long / w).min(short / h).min(1.0);
    let even = |v: f64| ((v * scale / 2.0).round() as i32 * 2).max(2);
    (even(w), even(h))
}

fn element(name: &str, settings: &[(&str, &str)]) -> Result<gst::Element, glib::BoolError> {
    let element = gst::ElementFactory::make(name).build()?;
    for (key, value) in settings {
        element.set_property_from_str(key, value);
    }
    Ok(element)
}

/// Links the elements one after another, the first from `pad` and the
/// last into `into`, and starts them with the pipeline.
fn branch(
    pipeline: &gst::Pipeline,
    pad: &gst::Pad,
    elements: &[gst::Element],
    into: &gst::Element,
) -> Result<(), glib::BoolError> {
    pipeline.add_many(elements)?;
    gst::Element::link_many(elements)?;
    let (Some(first), Some(last)) = (elements.first(), elements.last()) else {
        return Ok(());
    };
    last.link(into)?;
    for element in elements {
        element.sync_state_with_parent()?;
    }
    let sink = first
        .static_pad("sink")
        .ok_or_else(|| glib::bool_error!("no sink"))?;
    pad.link(&sink)
        .map(|_| ())
        .map_err(|_| glib::bool_error!("cannot link"))
}

/// The pipeline that codes the file at `from` into `to`.
fn pipeline(plan: Plan, from: &Path, to: &Path) -> Result<gst::Pipeline, glib::BoolError> {
    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("filesrc")
        .property("location", from.to_string_lossy().as_ref())
        .build()?;
    let decode = gst::ElementFactory::make("decodebin").build()?;
    let sink = gst::ElementFactory::make("filesink")
        .property("location", to.to_string_lossy().as_ref())
        .build()?;
    pipeline.add_many([&source, &decode, &sink])?;
    source.link(&decode)?;
    let mux = match plan {
        Plan::Movie { .. } => {
            let mux = gst::ElementFactory::make("mp4mux").build()?;
            mux.set_property_from_str("faststart", "true");
            pipeline.add(&mux)?;
            mux.link(&sink)?;
            mux
        }
        Plan::Sound(_) => sink.clone(),
    };
    // One stream of pictures and one of sound, the first of each.
    let taken = Arc::new(Mutex::new((false, false)));
    let weak = pipeline.downgrade();
    decode.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else {
            return;
        };
        let Some(caps) = pad.current_caps().or_else(|| Some(pad.query_caps(None))) else {
            return;
        };
        let Some(structure) = caps.structure(0) else {
            return;
        };
        let name = structure.name().to_string();
        let Ok(mut taken) = taken.lock() else { return };
        let made = match plan {
            Plan::Movie { video, audio } if name.starts_with("video/") && !taken.0 => {
                taken.0 = true;
                let width = structure.get::<i32>("width").unwrap_or(LARGEST.0 as i32);
                let height = structure.get::<i32>("height").unwrap_or(LARGEST.1 as i32);
                let (w, h) = optimized_size(width, height);
                let _ = audio;
                (|| {
                    let caps = gst::Caps::builder("video/x-raw")
                        .field("width", w)
                        .field("height", h)
                        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
                        .build();
                    let filter = gst::ElementFactory::make("capsfilter")
                        .property("caps", &caps)
                        .build()?;
                    branch(
                        &pipeline,
                        pad,
                        &[
                            element("queue", &[])?,
                            element("videoconvert", &[])?,
                            element("videoscale", &[])?,
                            filter,
                            element(video.0, video.1)?,
                            element("queue", &[])?,
                        ],
                        &mux,
                    )
                })()
            }
            Plan::Movie {
                audio: Some(audio), ..
            } if name.starts_with("audio/") && !taken.1 => {
                taken.1 = true;
                (|| {
                    branch(
                        &pipeline,
                        pad,
                        &[
                            element("queue", &[])?,
                            element("audioconvert", &[])?,
                            element("audioresample", &[])?,
                            element(audio.0, audio.1)?,
                            element("queue", &[])?,
                        ],
                        &mux,
                    )
                })()
            }
            Plan::Sound(encoder) if name.starts_with("audio/") && !taken.1 => {
                taken.1 = true;
                (|| {
                    branch(
                        &pipeline,
                        pad,
                        &[
                            element("audioconvert", &[])?,
                            element("audioresample", &[])?,
                            element(encoder.0, encoder.1)?,
                        ],
                        &mux,
                    )
                })()
            }
            _ => Ok(()),
        };
        if let Err(error) = made {
            gst::element_error!(pipeline, gst::CoreError::Negotiation, ("{}", error));
        }
    });
    Ok(pipeline)
}

/// A file of the cache that goes when the optimizing ends.
struct Temporary(PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Codes a movie or a sound again, calling `done` with the optimized one,
/// which plays as `media` does, or none when it cannot be optimized or
/// would not come out smaller.
pub fn optimize(media: &Media, done: impl FnOnce(Option<Media>) + 'static) {
    let Some((pipeline, output)) = start(media) else {
        done(None);
        return;
    };
    let Some(bus) = pipeline.bus() else {
        done(None);
        return;
    };
    // The watch lives until the end, which drops it.
    let watch: Rc<RefCell<Option<gst::bus::BusWatchGuard>>> = Rc::default();
    let finish = {
        let media = media.clone();
        let pipeline = pipeline.clone();
        let watch = watch.clone();
        let mut done = Some(done);
        move |ok: bool| {
            let _ = pipeline.set_state(gst::State::Null);
            watch.replace(None);
            let Some(done) = done.take() else { return };
            let optimized = ok
                .then(|| std::fs::read(&output.0).ok())
                .flatten()
                .filter(|bytes| !bytes.is_empty() && bytes.len() < media.data().len())
                .and_then(|bytes| media.with_data(bytes));
            done(optimized);
        }
    };
    let finish = Rc::new(RefCell::new(finish));
    let guard = {
        let finish = finish.clone();
        bus.add_watch_local(move |_, message| {
            let ok = match message.view() {
                gst::MessageView::Eos(_) => true,
                gst::MessageView::Error(_) => false,
                _ => return glib::ControlFlow::Continue,
            };
            (finish.borrow_mut())(ok);
            glib::ControlFlow::Break
        })
    };
    match guard {
        Ok(guard) => {
            watch.replace(Some(guard));
            if pipeline.set_state(gst::State::Playing).is_err() {
                (finish.borrow_mut())(false);
            }
        }
        Err(_) => (finish.borrow_mut())(false),
    }
}

/// The pipeline started for a movie or a sound, with the file it writes.
fn start(media: &Media) -> Option<(gst::Pipeline, Temporary)> {
    let plan = plan(media)?;
    let from = crate::media::file_of(media)?.path()?;
    let dir = glib::user_cache_dir().join("desk").join("media");
    std::fs::create_dir_all(&dir).ok()?;
    let extension = match plan {
        Plan::Movie { .. } => "mp4",
        Plan::Sound(_) => "mp3",
    };
    let output = Temporary(dir.join(format!(
        "optimized-{}-{}.{extension}",
        std::process::id(),
        glib::monotonic_time()
    )));
    let pipeline = pipeline(plan, &from, &output.0).ok()?;
    Some((pipeline, output))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the main loop until `until` holds, for ten seconds at most.
    fn wait(until: impl Fn() -> bool) {
        let context = glib::MainContext::default();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !until() && std::time::Instant::now() < end {
            context.iteration(true);
        }
    }

    /// The bytes that a pipeline writes into a file.
    fn made(description: &str, extension: &str) -> Vec<u8> {
        gst::init().unwrap();
        let path = std::env::temp_dir().join(format!(
            "desk-optimize-{}-{}.{extension}",
            std::process::id(),
            glib::monotonic_time()
        ));
        let pipeline = gst::parse::launch(&format!("{description} ! filesink name=sink"))
            .unwrap()
            .downcast::<gst::Bin>()
            .unwrap();
        pipeline
            .by_name("sink")
            .unwrap()
            .set_property("location", path.to_string_lossy().as_ref());
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        let message = bus
            .timed_pop_filtered(
                gst::ClockTime::from_seconds(30),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            )
            .unwrap();
        assert!(
            matches!(message.view(), gst::MessageView::Eos(_)),
            "{message:?}"
        );
        pipeline.set_state(gst::State::Null).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        bytes
    }

    fn optimized(media: &Media) -> Option<Media> {
        let result: Rc<RefCell<Option<Option<Media>>>> = Rc::default();
        {
            let result = result.clone();
            optimize(media, move |m| {
                result.replace(Some(m));
            });
        }
        wait(|| result.borrow().is_some());
        result.take().unwrap()
    }

    #[test]
    fn w119_a_movie_is_made_no_larger_than_720p() {
        assert_eq!(optimized_size(1920, 1080), (1280, 720));
        assert_eq!(optimized_size(1080, 1920), (720, 1280));
        assert_eq!(optimized_size(640, 480), (640, 480));
        assert_eq!(optimized_size(4000, 1000), (1280, 320));
        assert_eq!(optimized_size(333, 201), (334, 202));
    }

    #[test]
    fn w119_a_sound_of_samples_becomes_a_smaller_mp3() {
        // On the thread of GTK, which holds the main loop of the tests.
        crate::with_gtk(|| {
            let wave = made(
                "audiotestsrc num-buffers=50 samplesperbuffer=4410 \
                 ! audio/x-raw,format=S16LE,rate=44100,channels=2 ! wavenc",
                "wav",
            );
            let mut sound = Media::from_bytes(wave).unwrap();
            sound.volume = 30;
            assert!(optimizable(&sound));
            let mp3 = optimized(&sound).expect("optimized");
            assert_eq!(mp3.format(), MediaFormat::Mp3);
            assert!(mp3.data().len() < sound.data().len() / 4);
            assert_eq!(mp3.volume, 30);
            // An MP3 is small already.
            assert!(!optimizable(&mp3));
        });
    }

    #[test]
    fn w119_a_large_movie_becomes_a_smaller_mpeg_4() {
        // On the thread of GTK, which holds the main loop of the tests.
        crate::with_gtk(|| {
            // A movie of 1920 × 1080 coded with little loss, with sound.
            let movie = made(
                "videotestsrc num-buffers=15 pattern=ball \
                 ! video/x-raw,width=1920,height=1080,framerate=15/1 \
                 ! jpegenc quality=95 ! queue ! mux. \
                 audiotestsrc num-buffers=10 ! audioconvert ! queue ! mux. \
                 qtmux name=mux",
                "mov",
            );
            let movie = Media::from_bytes(movie).unwrap();
            assert_eq!(movie.format(), MediaFormat::QuickTime);
            assert!(optimizable(&movie));
            let optimized = optimized(&movie).expect("optimized");
            assert_eq!(optimized.format(), MediaFormat::Mp4);
            assert!(optimized.data().len() < movie.data().len());
            // With its pictures and its sound.
            let has = |tag: &[u8]| optimized.data().windows(4).any(|w| w == tag);
            assert!(has(b"vide") && has(b"soun"));
        });
    }
}
