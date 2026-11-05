// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Movies and sounds as GTK plays them (OBJ-18, OBJ-19): the file the
//! player reads, the poster frame and length that it finds, and playing
//! from the start to the stop that the QuickTime inspector sets.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use desk_model::{Media, Picture, Repeat};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

/// How long finding the poster frame of a movie may take before it is
/// left without one.
const PROBE_TIME: Duration = Duration::from_secs(5);

/// The widest or highest a poster frame is kept, in pixels.
const POSTER_SIDE: f64 = 1280.0;

thread_local! {
    /// The files written for the movies and sounds played, by the
    /// address and length of their bytes.
    static FILES: RefCell<HashMap<(usize, usize), PathBuf>> = RefCell::new(HashMap::new());
}

/// A file with the bytes of a movie or a sound, which the player reads:
/// written once in the cache of the user for the bytes of a document.
pub fn file_of(media: &Media) -> Option<gio::File> {
    let data = media.shared_data();
    let key = (data.as_ptr() as usize, data.len());
    if let Some(path) = FILES.with(|f| f.borrow().get(&key).cloned())
        && path.exists()
    {
        return Some(gio::File::for_path(path));
    }
    let dir = glib::user_cache_dir().join("desk").join("media");
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!(
        "{}-{}-{:x}.{}",
        std::process::id(),
        FILES.with(|f| f.borrow().len()),
        data.len(),
        media.format().extension()
    );
    let path = dir.join(name);
    std::fs::write(&path, &data).ok()?;
    FILES.with(|f| f.borrow_mut().insert(key, path.clone()));
    Some(gio::File::for_path(path))
}

/// Takes out the files written for playing, as the app quits.
pub fn clean_up() {
    FILES.with(|f| {
        for path in f.borrow_mut().drain().map(|(_, p)| p) {
            let _ = std::fs::remove_file(path);
        }
    });
}

/// A frame of a movie as a picture in PNG.
fn picture_of(paintable: &gdk::Paintable) -> Option<Picture> {
    let (w, h) = (
        f64::from(paintable.intrinsic_width()),
        f64::from(paintable.intrinsic_height()),
    );
    if w < 1.0 || h < 1.0 {
        return None;
    }
    let scale = (POSTER_SIDE / w).min(POSTER_SIDE / h).min(1.0);
    let (w, h) = ((w * scale).round(), (h * scale).round());
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, w, h);
    let node = snapshot.to_node()?;
    let surface =
        gtk::cairo::ImageSurface::create(gtk::cairo::Format::Rgb24, w as i32, h as i32).ok()?;
    {
        let cr = gtk::cairo::Context::new(&surface).ok()?;
        node.draw(&cr);
    }
    surface.flush();
    let mut png = Vec::new();
    surface.write_to_png(&mut png).ok()?;
    Picture::from_png(png)
}

/// What is called with the poster frame and length that a probe found.
type Probed = dyn FnOnce(Option<Picture>, Option<u32>);

/// Finds how long a movie or a sound lasts and, for a movie, its frame
/// at `at` milliseconds, then calls `done` with them; with none of them
/// when GTK cannot play the file.
pub fn probe(media: &Media, at: u32, done: impl FnOnce(Option<Picture>, Option<u32>) + 'static) {
    let Some(file) = file_of(media) else {
        done(None, None);
        return;
    };
    let stream = gtk::MediaFile::for_file(&file);
    stream.set_muted(true);
    let done: Rc<RefCell<Option<Box<Probed>>>> = Rc::new(RefCell::new(Some(Box::new(done))));
    let duration = Rc::new(Cell::new(None::<u32>));
    let finish = {
        let done = done.clone();
        let duration = duration.clone();
        let stream = stream.clone();
        Rc::new(move |poster: Option<Picture>| {
            if let Some(done) = done.borrow_mut().take() {
                stream.pause();
                done(poster, duration.get());
            }
        })
    };
    let sound = media.is_sound();
    let seeked = Rc::new(Cell::new(false));
    {
        let finish = finish.clone();
        let duration = duration.clone();
        let seeked = seeked.clone();
        stream.connect_prepared_notify(move |stream| {
            if !stream.is_prepared() {
                return;
            }
            let length = stream.duration();
            if length > 0 {
                duration.set(Some((length / 1000).min(i64::from(u32::MAX)) as u32));
            }
            if sound || !stream.has_video() {
                finish(None);
                return;
            }
            seeked.set(true);
            stream.seek(i64::from(at) * 1000);
            stream.play();
        });
    }
    {
        let finish = finish.clone();
        stream.connect_invalidate_contents(move |stream| {
            if seeked.get() && !stream.is_seeking() {
                finish(picture_of(stream.upcast_ref()));
            }
        });
    }
    {
        let finish = finish.clone();
        stream.connect_error_notify(move |_| finish(None));
    }
    glib::timeout_add_local_once(PROBE_TIME, move || finish(None));
}

/// Plays a movie or a sound in a window over `parent`, `size` pixels
/// large, from its start to its stop, repeated as it says, until the
/// window closes.
pub fn play(parent: &impl IsA<gtk::Widget>, size: (i32, i32), media: &Media) {
    let Some(file) = file_of(media) else {
        return;
    };
    let stream = gtk::MediaFile::for_file(&file);
    stream.set_volume(f64::from(media.volume.min(100)) / 100.0);
    let (start, stop, repeat) = (media.start, media.end(), media.repeat);
    stream.connect_prepared_notify(move |stream| {
        if stream.is_prepared() {
            if start > 0 {
                stream.seek(i64::from(start) * 1000);
            }
            stream.play();
        }
    });
    let again = move |stream: &gtk::MediaFile| {
        if repeat == Repeat::None {
            stream.pause();
            stream.seek(i64::from(start) * 1000);
        } else {
            // Back and forth plays forward again, as GTK cannot play back.
            stream.seek(i64::from(start) * 1000);
            stream.play();
        }
    };
    stream.connect_timestamp_notify(move |stream| {
        if let Some(stop) = stop
            && stream.is_playing()
            && stream.timestamp() >= i64::from(stop) * 1000
        {
            again(stream);
        }
    });
    stream.connect_ended_notify(move |stream| {
        if stream.is_ended() && repeat != Repeat::None {
            again(stream);
        }
    });
    let video = gtk::Video::new();
    video.set_media_stream(Some(&stream));
    let (width, height) = if media.is_sound() {
        (320, 64)
    } else {
        (size.0.max(160), size.1.max(90))
    };
    video.set_size_request(width, height);
    let window = gtk::Window::builder()
        .title(if media.is_sound() {
            crate::i18n::tr("Sound")
        } else {
            crate::i18n::tr("Movie")
        })
        .child(&video)
        .modal(false)
        .build();
    if let Some(root) = parent.root().and_downcast::<gtk::Window>() {
        window.set_transient_for(Some(&root));
    }
    window.connect_close_request(move |_| {
        stream.pause();
        glib::Propagation::Proceed
    });
    window.present();
}
