// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Movies and sounds on the pages, as Pages places them (OBJ-18,
//! OBJ-19): the file as it came, with the frame that stands for a movie
//! on the page and in print, and how it plays.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::picture::Picture;

/// The format of a movie or a sound, from the start of its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaFormat {
    /// A QuickTime movie (MOV).
    QuickTime,
    /// An MPEG-4 movie.
    Mp4,
    /// An MPEG-4 sound, as AAC (M4A).
    M4a,
    /// An MP3 sound.
    Mp3,
    /// An AIFF sound.
    Aiff,
    /// A WAVE sound.
    Wav,
    /// An Ogg file, sound or movie.
    Ogg,
    /// A WebM or Matroska movie.
    WebM,
}

impl MediaFormat {
    /// The format of the bytes of a file, if it is a movie or a sound
    /// Desk knows.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        let at = |range: std::ops::Range<usize>| bytes.get(range).unwrap_or_default();
        if at(4..8) == b"ftyp" {
            return Some(match at(8..12) {
                b"qt  " => Self::QuickTime,
                b"M4A " | b"M4B " => Self::M4a,
                _ => Self::Mp4,
            });
        }
        // A QuickTime movie of old starts with another atom.
        if matches!(at(4..8), b"moov" | b"mdat" | b"wide" | b"free" | b"skip") {
            return Some(Self::QuickTime);
        }
        if at(0..4) == b"FORM" && matches!(at(8..12), b"AIFF" | b"AIFC") {
            return Some(Self::Aiff);
        }
        if at(0..4) == b"RIFF" && at(8..12) == b"WAVE" {
            return Some(Self::Wav);
        }
        if at(0..4) == b"OggS" {
            return Some(Self::Ogg);
        }
        if at(0..4) == [0x1A, 0x45, 0xDF, 0xA3] {
            return Some(Self::WebM);
        }
        // An ID3 tag, or a frame of MPEG audio layer III.
        if at(0..3) == b"ID3"
            || (at(0..2).len() == 2 && bytes[0] == 0xFF && bytes[1] & 0xE6 == 0xE2)
        {
            return Some(Self::Mp3);
        }
        None
    }

    /// The media type, as `video/quicktime`.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::QuickTime => "video/quicktime",
            Self::Mp4 => "video/mp4",
            Self::M4a => "audio/mp4",
            Self::Mp3 => "audio/mpeg",
            Self::Aiff => "audio/aiff",
            Self::Wav => "audio/wav",
            Self::Ogg => "application/ogg",
            Self::WebM => "video/webm",
        }
    }

    /// The extension of a file of the format, as `mov`.
    pub fn extension(self) -> &'static str {
        match self {
            Self::QuickTime => "mov",
            Self::Mp4 => "mp4",
            Self::M4a => "m4a",
            Self::Mp3 => "mp3",
            Self::Aiff => "aiff",
            Self::Wav => "wav",
            Self::Ogg => "ogg",
            Self::WebM => "webm",
        }
    }

    /// Whether files of the format are sounds without pictures, which
    /// show as a speaker.
    pub fn is_sound(self) -> bool {
        matches!(self, Self::M4a | Self::Mp3 | Self::Aiff | Self::Wav)
    }
}

/// How a movie or a sound plays again when it ends (OBJ-19).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Repeat {
    /// Once.
    #[default]
    None,
    /// Over and over.
    Loop,
    /// Forward and back again, over and over.
    BackAndForth,
}

/// A movie or a sound, with how it plays.
#[derive(Clone)]
pub struct Media {
    data: Arc<[u8]>,
    format: MediaFormat,
    /// The frame of a movie that stands for it on the page and in print,
    /// if it was taken.
    pub poster: Option<Picture>,
    /// How long it lasts, in milliseconds, if it is known.
    pub duration: Option<u32>,
    /// Where it starts playing, in milliseconds.
    pub start: u32,
    /// Where it stops, in milliseconds, or none at its end.
    pub stop: Option<u32>,
    /// Where the poster frame was taken, in milliseconds.
    pub poster_time: u32,
    /// How it plays again.
    pub repeat: Repeat,
    /// How loud it plays, in percent.
    pub volume: u8,
}

impl Media {
    /// A movie or a sound from the bytes of its file, if Desk knows its
    /// format, to play whole, once and at full volume.
    pub fn from_bytes(data: impl Into<Arc<[u8]>>) -> Option<Self> {
        let data = data.into();
        let format = MediaFormat::sniff(&data)?;
        Some(Self {
            data,
            format,
            poster: None,
            duration: None,
            start: 0,
            stop: None,
            poster_time: 0,
            repeat: Repeat::None,
            volume: 100,
        })
    }

    /// The bytes of the file.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The bytes of the file, shared rather than copied.
    pub fn shared_data(&self) -> Arc<[u8]> {
        Arc::clone(&self.data)
    }

    /// The format of the file.
    pub fn format(&self) -> MediaFormat {
        self.format
    }

    /// Whether it is a sound, which shows as a speaker.
    pub fn is_sound(&self) -> bool {
        self.format.is_sound()
    }

    /// Where it stops playing, in milliseconds, if that is known.
    pub fn end(&self) -> Option<u32> {
        self.stop.or(self.duration)
    }
}

impl std::fmt::Debug for Media {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Media")
            .field("format", &self.format)
            .field("bytes", &self.data.len())
            .field("poster", &self.poster)
            .field("duration", &self.duration)
            .field("start", &self.start)
            .field("stop", &self.stop)
            .field("poster_time", &self.poster_time)
            .field("repeat", &self.repeat)
            .field("volume", &self.volume)
            .finish()
    }
}

impl PartialEq for Media {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.data, &other.data) || self.data == other.data)
            && self.format == other.format
            && self.poster == other.poster
            && self.duration == other.duration
            && self.start == other.start
            && self.stop == other.stop
            && self.poster_time == other.poster_time
            && self.repeat == other.repeat
            && self.volume == other.volume
    }
}

impl Eq for Media {}

impl Hash for Media {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.len().hash(state);
        self.format.hash(state);
        self.start.hash(state);
        self.stop.hash(state);
        self.poster_time.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_18_movies_and_sounds_are_known_by_their_start() {
        let mut mov = vec![0, 0, 0, 20];
        mov.extend(b"ftypqt  ");
        let mut mp4 = vec![0, 0, 0, 20];
        mp4.extend(b"ftypisom");
        let mut m4a = vec![0, 0, 0, 20];
        m4a.extend(b"ftypM4A ");
        let cases: [(&[u8], Option<MediaFormat>); 8] = [
            (&mov, Some(MediaFormat::QuickTime)),
            (&mp4, Some(MediaFormat::Mp4)),
            (&m4a, Some(MediaFormat::M4a)),
            (b"ID3\x04\0\0\0\0\0\0", Some(MediaFormat::Mp3)),
            (&[0xFF, 0xFB, 0x90, 0x64], Some(MediaFormat::Mp3)),
            (b"FORM\0\0\0\0AIFF", Some(MediaFormat::Aiff)),
            (b"RIFF\0\0\0\0WAVE", Some(MediaFormat::Wav)),
            (b"%PDF-1.4", None),
        ];
        for (bytes, format) in cases {
            assert_eq!(MediaFormat::sniff(bytes), format, "{bytes:?}");
        }
        let sound = Media::from_bytes(b"ID3\x04\0\0\0\0\0\0".to_vec()).unwrap();
        assert!(sound.is_sound());
        assert_eq!((sound.start, sound.end(), sound.volume), (0, None, 100));
        assert!(!Media::from_bytes(mov).unwrap().is_sound());
    }
}
