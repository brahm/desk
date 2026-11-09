// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reduce Media (OBJ-20): a movie or a sound cut down to the part between
//! its start and its stop, without coding it again. A sound of samples
//! (WAVE, AIFF) is cut at the samples, an MP3 at its frames, and a
//! QuickTime or MPEG-4 file at the samples of its tracks, from the key
//! frame before the start, so that it still plays from there.

use crate::{Media, MediaFormat};

/// What a movie or a sound is cut to, in milliseconds of the file.
struct Cut {
    /// Where the part kept begins.
    from: u32,
    /// Where it ends, or none at the end of the file.
    to: Option<u32>,
}

/// The bytes of a part kept, and where it begins and how long it lasts,
/// in milliseconds of the file.
struct Kept {
    data: Vec<u8>,
    from: u32,
    length: u32,
}

impl Media {
    /// The units of a second that the times of a QuickTime or MPEG-4 file
    /// count, from its movie header; none for other files.
    pub fn time_scale(&self) -> Option<u64> {
        if !matches!(
            self.format(),
            MediaFormat::QuickTime | MediaFormat::Mp4 | MediaFormat::M4a
        ) {
            return None;
        }
        let top = atoms(self.data())?;
        let moov = atoms(find(&top, b"moov")?)?;
        time_scale(find(&moov, b"mvhd")?)
    }

    /// Whether Reduce Media would take anything out: its start or stop
    /// leaves out part of a file whose format Desk can cut.
    pub fn reducible(&self) -> bool {
        (self.start > 0 || self.stop.is_some())
            && matches!(
                self.format(),
                MediaFormat::Wav
                    | MediaFormat::Aiff
                    | MediaFormat::Mp3
                    | MediaFormat::QuickTime
                    | MediaFormat::Mp4
                    | MediaFormat::M4a
            )
    }

    /// The movie or sound with the parts before its start and after its
    /// stop taken out, playing as it did; none when there is nothing to
    /// take out or the file cannot be cut (OBJ-20).
    pub fn reduced(&self) -> Option<Media> {
        if !self.reducible() {
            return None;
        }
        let cut = Cut {
            from: self.start,
            to: self.stop,
        };
        let data = self.data();
        let kept = match self.format() {
            MediaFormat::Wav => wav(data, &cut),
            MediaFormat::Aiff => aiff(data, &cut),
            MediaFormat::Mp3 => mp3(data, &cut),
            MediaFormat::QuickTime | MediaFormat::Mp4 | MediaFormat::M4a => mp4(data, &cut),
            MediaFormat::Ogg | MediaFormat::WebM => None,
        }?;
        if kept.data.len() >= data.len() {
            return None;
        }
        let mut media = Media::from_bytes(kept.data)?;
        let length = kept.length.max(1);
        media.poster = self.poster.clone();
        media.duration = Some(length);
        media.start = self.start.saturating_sub(kept.from).min(length - 1);
        media.stop = self
            .stop
            .map(|stop| stop.saturating_sub(kept.from))
            .filter(|&stop| stop < length);
        media.poster_time = self.poster_time.saturating_sub(kept.from).min(length);
        media.repeat = self.repeat;
        media.volume = self.volume;
        Some(media)
    }
}

/// Milliseconds as units of a time scale, and back.
fn ticks(ms: u32, scale: u64) -> u64 {
    u64::from(ms) * scale / 1000
}

fn millis(ticks: u64, scale: u64) -> u32 {
    (ticks * 1000 / scale.max(1)).min(u64::from(u32::MAX)) as u32
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The frames of samples kept of `frames`, at `rate` a second.
fn frame_range(cut: &Cut, rate: u64, frames: u64) -> Option<(u64, u64)> {
    let first = ticks(cut.from, rate).min(frames);
    let last = cut.to.map_or(frames, |to| ticks(to, rate).min(frames));
    (last > first).then_some((first, last))
}

/// A WAVE sound of samples, cut at its samples.
fn wav(data: &[u8], cut: &Cut) -> Option<Kept> {
    let mut at = 12;
    let mut chunks = Vec::new();
    while at + 8 <= data.len() {
        let id: [u8; 4] = data[at..at + 4].try_into().ok()?;
        let size = le32(data, at + 4)? as usize;
        let body = data.get(at + 8..(at + 8 + size).min(data.len()))?;
        chunks.push((id, body));
        at += 8 + size + size % 2;
    }
    let format = chunks.iter().find(|(id, _)| id == b"fmt ")?.1;
    // Samples, whole or floating, and nothing coded.
    if !matches!(le16(format, 0)?, 1 | 3 | 0xFFFE) {
        return None;
    }
    let rate = u64::from(le32(format, 4)?);
    let align = usize::from(le16(format, 12)?).max(1);
    let samples = chunks.iter().find(|(id, _)| id == b"data")?.1;
    let frames = (samples.len() / align) as u64;
    let (first, last) = frame_range(cut, rate, frames)?;
    let mut out = b"RIFF\0\0\0\0WAVE".to_vec();
    for (id, body) in chunks {
        // How many frames there are, which the new data says.
        if &id == b"fact" {
            continue;
        }
        let body = if &id == b"data" {
            &body[first as usize * align..last as usize * align]
        } else {
            body
        };
        out.extend(id);
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        if body.len() % 2 == 1 {
            out.push(0);
        }
    }
    let size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Some(Kept {
        data: out,
        from: millis(first, rate),
        length: millis(last - first, rate),
    })
}

/// An 80-bit floating number of AIFF, as the rate of a sound.
fn extended(b: &[u8]) -> Option<f64> {
    let exponent = i32::from(be16(b, 0)? & 0x7FFF) - 16383;
    let mantissa = be64(b, 2)?;
    Some(mantissa as f64 * 2f64.powi(exponent - 63))
}

/// An AIFF sound, cut at its samples.
fn aiff(data: &[u8], cut: &Cut) -> Option<Kept> {
    let compressed = data.get(8..12)? == b"AIFC";
    let mut at = 12;
    let mut chunks = Vec::new();
    while at + 8 <= data.len() {
        let id: [u8; 4] = data[at..at + 4].try_into().ok()?;
        let size = be32(data, at + 4)? as usize;
        let body = data.get(at + 8..(at + 8 + size).min(data.len()))?;
        chunks.push((id, body));
        at += 8 + size + size % 2;
    }
    let common = chunks.iter().find(|(id, _)| id == b"COMM")?.1;
    if compressed && !matches!(common.get(18..22)?, b"NONE" | b"sowt" | b"raw " | b"twos") {
        return None;
    }
    let channels = usize::from(be16(common, 0)?);
    let bits = usize::from(be16(common, 6)?);
    let rate = extended(common.get(8..18)?)?.round();
    if !(1.0..=1e7).contains(&rate) {
        return None;
    }
    let rate = rate as u64;
    let align = (channels * bits.div_ceil(8)).max(1);
    let sound = chunks.iter().find(|(id, _)| id == b"SSND")?.1;
    let offset = be32(sound, 0)? as usize;
    let samples = sound.get(8 + offset..)?;
    let frames = u64::from(be32(common, 2)?).min((samples.len() / align) as u64);
    let (first, last) = frame_range(cut, rate, frames)?;
    let mut out = data[..12].to_vec();
    for (id, body) in chunks {
        let owned;
        let body: &[u8] = match &id {
            b"COMM" => {
                let mut c = body.to_vec();
                c[2..6].copy_from_slice(&((last - first) as u32).to_be_bytes());
                owned = c;
                &owned
            }
            b"SSND" => {
                let mut c = vec![0; 8];
                c[4..8].copy_from_slice(&body[4..8]);
                c.extend(&samples[first as usize * align..last as usize * align]);
                owned = c;
                &owned
            }
            // Markers and loops name frames that are gone.
            b"MARK" | b"INST" => continue,
            _ => body,
        };
        out.extend(id);
        out.extend((body.len() as u32).to_be_bytes());
        out.extend(body);
        if body.len() % 2 == 1 {
            out.push(0);
        }
    }
    let size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&size.to_be_bytes());
    Some(Kept {
        data: out,
        from: millis(first, rate),
        length: millis(last - first, rate),
    })
}

/// The length in bytes of a frame of MPEG audio layer III, and its
/// samples and rate, from its header.
fn mp3_frame(header: u32) -> Option<(usize, u64, u64)> {
    if header >> 21 != 0x7FF || (header >> 17) & 3 != 1 {
        return None;
    }
    let version = (header >> 19) & 3;
    let bitrate = (header >> 12) & 0xF;
    let rate = (header >> 10) & 3;
    if version == 1 || bitrate == 0 || bitrate == 0xF || rate == 3 {
        return None;
    }
    let mpeg1 = version == 3;
    const V1: [u64; 15] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const V2: [u64; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let kbps = if mpeg1 { V1 } else { V2 }[bitrate as usize];
    let base = [44_100, 48_000, 32_000][rate as usize];
    let rate = match version {
        3 => base,
        2 => base / 2,
        _ => base / 4,
    };
    let samples = if mpeg1 { 1152 } else { 576 };
    let padding = u64::from((header >> 9) & 1);
    let length = samples / 8 * kbps * 1000 / rate + padding;
    Some((length as usize, samples, rate))
}

/// An MP3 sound, cut at its frames. Its tags stay; a frame that says how
/// long the whole sound was goes.
fn mp3(data: &[u8], cut: &Cut) -> Option<Kept> {
    let mut at = 0;
    if data.starts_with(b"ID3") {
        let size = data
            .get(6..10)?
            .iter()
            .fold(0usize, |n, b| (n << 7) | usize::from(b & 0x7F));
        let footer = if data.get(5)? & 0x10 != 0 { 10 } else { 0 };
        at = 10 + size + footer;
    }
    let tag = &data[..at.min(data.len())];
    let trailer = data
        .len()
        .checked_sub(128)
        .filter(|&t| t >= at && &data[t..t + 3] == b"TAG");
    let end = trailer.unwrap_or(data.len());
    let mut out = tag.to_vec();
    let (mut time, mut from, mut kept, mut rate) = (0u64, None, 0u64, 0u64);
    let mut first = true;
    while at + 4 <= end {
        let Some((length, samples, r)) = be32(data, at).and_then(mp3_frame) else {
            // Something else between the frames: look for the next one.
            at += 1;
            continue;
        };
        let frame = data.get(at..(at + length).min(end))?;
        let header_only = first
            && frame
                .windows(4)
                .take(40)
                .any(|w| w == b"Xing" || w == b"Info" || w == b"VBRI");
        first = false;
        at += length.max(4);
        if header_only {
            continue;
        }
        rate = r;
        let begins = time * 1000 / r;
        let ends = (time + samples) * 1000 / r;
        time += samples;
        let wanted = ends > u64::from(cut.from) && cut.to.is_none_or(|to| begins < u64::from(to));
        if wanted {
            from.get_or_insert(begins);
            kept += samples;
            out.extend(frame);
        }
    }
    let from = from?;
    if let Some(t) = trailer {
        out.extend(&data[t..]);
    }
    Some(Kept {
        data: out,
        from: from as u32,
        length: millis(kept, rate),
    })
}

/// A box of QuickTime or MPEG-4: its type and what it holds.
#[derive(Clone, Copy)]
struct Atom<'a> {
    kind: [u8; 4],
    body: &'a [u8],
}

/// The boxes one after another in some bytes.
fn atoms(mut data: &[u8]) -> Option<Vec<Atom<'_>>> {
    let mut all = Vec::new();
    while data.len() >= 8 {
        let size = be32(data, 0)? as u64;
        let kind: [u8; 4] = data[4..8].try_into().ok()?;
        let (header, size) = match size {
            0 => (8, data.len() as u64),
            1 => (16, be64(data, 8)?),
            _ => (8, size),
        };
        let size = usize::try_from(size).ok()?;
        if size < header || size > data.len() {
            return None;
        }
        all.push(Atom {
            kind,
            body: &data[header..size],
        });
        data = &data[size..];
    }
    Some(all)
}

fn find<'a>(atoms: &[Atom<'a>], kind: &[u8; 4]) -> Option<&'a [u8]> {
    atoms.iter().find(|a| &a.kind == kind).map(|a| a.body)
}

fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend(((body.len() + 8) as u32).to_be_bytes());
    out.extend(kind);
    out.extend(body);
    out
}

/// A full box: its version and flags, then `body`.
fn full_atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut all = vec![0; 4];
    all.extend(body);
    atom(kind, &all)
}

/// Writes a duration into the header of `mvhd`, `tkhd` or `mdhd`, at
/// the place of the version it has.
fn with_duration(body: &[u8], kind: &[u8; 4], duration: u64) -> Vec<u8> {
    let mut b = body.to_vec();
    let v1 = b.first() == Some(&1);
    let at = match (kind, v1) {
        (b"tkhd", false) => 20,
        (b"tkhd", true) => 28,
        (_, false) => 16,
        (_, true) => 24,
    };
    if v1 {
        if let Some(slot) = b.get_mut(at..at + 8) {
            slot.copy_from_slice(&duration.to_be_bytes());
        }
    } else if let Some(slot) = b.get_mut(at..at + 4) {
        slot.copy_from_slice(&(duration.min(u64::from(u32::MAX)) as u32).to_be_bytes());
    }
    b
}

/// The time scale in a header of `mvhd` or `mdhd`.
fn time_scale(body: &[u8]) -> Option<u64> {
    let at = if body.first() == Some(&1) { 20 } else { 12 };
    be32(body, at).map(u64::from).filter(|&s| s > 0)
}

/// The entries of a table box, after its version, flags and count.
fn table(body: &[u8], width: usize) -> Option<Vec<&[u8]>> {
    let count = be32(body, 4)? as usize;
    let rows = body.get(8..8 + count.checked_mul(width)?)?;
    Some(rows.chunks_exact(width).collect())
}

/// The samples of a track, and the part of them kept.
struct Track<'a> {
    stbl: Vec<Atom<'a>>,
    scale: u64,
    /// For each sample: where it begins in time, how long it lasts,
    /// where its bytes are and how many there are.
    samples: Vec<(u64, u64, usize, usize)>,
    /// The key frames, by sample from 0; none when every sample is one.
    sync: Option<Vec<usize>>,
    /// The samples kept.
    kept: std::ops::Range<usize>,
}

impl<'a> Track<'a> {
    fn read(trak: &'a [u8], file: &[u8]) -> Option<Self> {
        let mdia = atoms(find(&atoms(trak)?, b"mdia")?)?;
        let scale = time_scale(find(&mdia, b"mdhd")?)?;
        let minf = atoms(find(&mdia, b"minf")?)?;
        // The samples must be in the file itself.
        if let Some(dinf) = find(&minf, b"dinf") {
            let dref = find(&atoms(dinf)?, b"dref")?;
            for entry in atoms(dref.get(8..)?)? {
                if entry.body.get(3).is_none_or(|flags| flags & 1 == 0) {
                    return None;
                }
            }
        }
        let stbl = atoms(find(&minf, b"stbl")?)?;
        let stsd = find(&stbl, b"stsd")?;
        if be32(stsd, 4)? != 1 {
            return None;
        }
        let mut times = Vec::new();
        let mut time = 0;
        for row in table(find(&stbl, b"stts")?, 8)? {
            let (count, delta) = (be32(row, 0)?, u64::from(be32(row, 4)?));
            for _ in 0..count {
                times.push((time, delta));
                time += delta;
            }
        }
        let stsz = find(&stbl, b"stsz")?;
        let fixed = be32(stsz, 4)? as usize;
        let count = be32(stsz, 8)? as usize;
        if count != times.len() {
            return None;
        }
        let sizes: Vec<usize> = if fixed != 0 {
            vec![fixed; count]
        } else {
            let rows = stsz.get(12..12 + count.checked_mul(4)?)?;
            rows.as_chunks::<4>()
                .0
                .iter()
                .map(|r| u32::from_be_bytes(*r) as usize)
                .collect()
        };
        let chunks: Vec<u64> = if let Some(stco) = find(&stbl, b"stco") {
            table(stco, 4)?
                .iter()
                .map(|r| be32(r, 0).map(u64::from))
                .collect::<Option<_>>()?
        } else {
            table(find(&stbl, b"co64")?, 8)?
                .iter()
                .map(|r| be64(r, 0))
                .collect::<Option<_>>()?
        };
        let runs: Vec<(usize, usize)> = table(find(&stbl, b"stsc")?, 12)?
            .iter()
            .map(|r| Some((be32(r, 0)? as usize, be32(r, 4)? as usize)))
            .collect::<Option<_>>()?;
        let mut offsets = Vec::with_capacity(count);
        for (i, &chunk) in chunks.iter().enumerate() {
            let per = runs
                .iter()
                .rev()
                .find(|(first, _)| *first <= i + 1)
                .map_or(0, |r| r.1);
            let mut at = usize::try_from(chunk).ok()?;
            for _ in 0..per {
                let Some(&size) = sizes.get(offsets.len()) else {
                    break;
                };
                offsets.push(at);
                at += size;
            }
        }
        if offsets.len() != count {
            return None;
        }
        let samples: Vec<_> = (0..count)
            .map(|i| (times[i].0, times[i].1, offsets[i], sizes[i]))
            .collect();
        if samples.iter().any(|s| s.2 + s.3 > file.len()) {
            return None;
        }
        let sync = match find(&stbl, b"stss") {
            Some(stss) => Some(
                table(stss, 4)?
                    .iter()
                    .map(|r| be32(r, 0).map(|n| (n as usize).saturating_sub(1)))
                    .collect::<Option<Vec<_>>>()?,
            ),
            None => None,
        };
        Some(Self {
            stbl,
            scale,
            samples,
            sync,
            kept: 0..count,
        })
    }

    /// Where the key frame at or before a time begins, in milliseconds.
    fn key_before(&self, ms: u32) -> u32 {
        let at = ticks(ms, self.scale);
        let index = self
            .samples
            .partition_point(|s| s.0 <= at)
            .saturating_sub(1);
        let key = match &self.sync {
            Some(sync) => sync
                .iter()
                .copied()
                .filter(|&k| k <= index)
                .max()
                .unwrap_or(0),
            None => index,
        };
        self.samples.get(key).map_or(0, |s| millis(s.0, self.scale))
    }

    /// Keeps the samples from a time to another, in milliseconds.
    fn keep(&mut self, from: u32, to: Option<u32>) {
        let from = ticks(from, self.scale);
        // A sample that the time falls in.
        let first = self.samples.partition_point(|s| s.0 + s.1 <= from);
        let first = match &self.sync {
            Some(sync) => sync
                .iter()
                .copied()
                .filter(|&k| k <= first)
                .max()
                .unwrap_or(0),
            None => first,
        };
        let last = to.map_or(self.samples.len(), |to| {
            self.samples
                .partition_point(|s| s.0 < ticks(to, self.scale))
        });
        self.kept = first..last.max(first + 1).min(self.samples.len());
    }

    fn duration(&self) -> u64 {
        self.samples[self.kept.clone()].iter().map(|s| s.1).sum()
    }

    /// The bytes of the samples kept.
    fn data(&self, file: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for s in &self.samples[self.kept.clone()] {
            out.extend(&file[s.2..s.2 + s.3]);
        }
        out
    }

    /// The new sample table, with the samples kept in one chunk at
    /// `offset` of the file.
    fn stbl(&self, offset: u64) -> Option<Vec<u8>> {
        let kept = &self.samples[self.kept.clone()];
        let n = kept.len() as u32;
        let mut out = Vec::new();
        for a in &self.stbl {
            match &a.kind {
                b"stsd" => out.extend(atom(b"stsd", a.body)),
                b"stts" => {
                    let mut rows: Vec<(u32, u32)> = Vec::new();
                    for s in kept {
                        let delta = s.1 as u32;
                        match rows.last_mut() {
                            Some((count, d)) if *d == delta => *count += 1,
                            _ => rows.push((1, delta)),
                        }
                    }
                    out.extend(full_atom(b"stts", &rows_of(&rows)));
                }
                b"ctts" => {
                    let mut all = Vec::new();
                    for row in table(a.body, 8)? {
                        for _ in 0..be32(row, 0)? {
                            all.push(be32(row, 4)?);
                        }
                    }
                    let mut rows: Vec<(u32, u32)> = Vec::new();
                    for &o in all.get(self.kept.clone())? {
                        match rows.last_mut() {
                            Some((count, d)) if *d == o => *count += 1,
                            _ => rows.push((1, o)),
                        }
                    }
                    let mut body = a.body[..4].to_vec();
                    body.extend(&rows_of(&rows)[..]);
                    out.extend(atom(b"ctts", &body));
                }
                b"stss" => {
                    let sync = self.sync.as_ref()?;
                    let mut body = vec![0; 4];
                    let keys: Vec<u32> = sync
                        .iter()
                        .filter(|k| self.kept.contains(k))
                        .map(|k| (k - self.kept.start + 1) as u32)
                        .collect();
                    body.extend((keys.len() as u32).to_be_bytes());
                    for k in keys {
                        body.extend(k.to_be_bytes());
                    }
                    out.extend(atom(b"stss", &body));
                }
                b"stsc" => {
                    let desc = be32(table(a.body, 12)?.first()?, 8)?;
                    let mut body = 1u32.to_be_bytes().to_vec();
                    body.extend(1u32.to_be_bytes());
                    body.extend(n.to_be_bytes());
                    body.extend(desc.to_be_bytes());
                    out.extend(full_atom(b"stsc", &body));
                }
                b"stsz" => {
                    let mut body = 0u32.to_be_bytes().to_vec();
                    body.extend(n.to_be_bytes());
                    for s in kept {
                        body.extend((s.3 as u32).to_be_bytes());
                    }
                    out.extend(full_atom(b"stsz", &body));
                }
                b"stco" | b"co64" => {
                    let mut body = 1u32.to_be_bytes().to_vec();
                    if let Ok(offset) = u32::try_from(offset) {
                        body.extend(offset.to_be_bytes());
                        out.extend(full_atom(b"stco", &body));
                    } else {
                        body.extend(offset.to_be_bytes());
                        out.extend(full_atom(b"co64", &body));
                    }
                }
                // Groups and dependencies of samples name samples gone.
                b"sdtp" | b"sbgp" | b"sgpd" | b"stps" | b"stsh" | b"cslg" => {}
                kind => out.extend(atom(kind, a.body)),
            }
        }
        Some(atom(b"stbl", &out))
    }
}

/// Rows of counts and values, after their number.
fn rows_of(rows: &[(u32, u32)]) -> Vec<u8> {
    let mut out = (rows.len() as u32).to_be_bytes().to_vec();
    for (a, b) in rows {
        out.extend(a.to_be_bytes());
        out.extend(b.to_be_bytes());
    }
    out
}

/// A QuickTime or MPEG-4 file, cut at the samples of its tracks: each
/// from the key frame before the start of the first, all in one `mdat`
/// after the `moov`.
fn mp4(data: &[u8], cut: &Cut) -> Option<Kept> {
    let top = atoms(data)?;
    let moov = atoms(find(&top, b"moov")?)?;
    let scale = time_scale(find(&moov, b"mvhd")?)?;
    let traks: Vec<&[u8]> = moov
        .iter()
        .filter(|a| &a.kind == b"trak")
        .map(|a| a.body)
        .collect();
    let mut tracks: Vec<Track> = traks
        .iter()
        .map(|t| Track::read(t, data))
        .collect::<Option<_>>()?;
    if tracks.is_empty() {
        return None;
    }
    // From the key frame before the start, of the track that has one the
    // furthest back.
    let from = tracks
        .iter()
        .map(|t| t.key_before(cut.from))
        .min()
        .unwrap_or(0);
    for track in &mut tracks {
        track.keep(from, cut.to);
    }
    let length = tracks
        .iter()
        .map(|t| millis(t.duration(), t.scale))
        .max()
        .unwrap_or(0);
    let build = |offsets: &[u64]| -> Option<Vec<u8>> {
        let mut out = Vec::new();
        let mut index = 0;
        for a in &moov {
            match &a.kind {
                b"mvhd" => out.extend(atom(
                    b"mvhd",
                    &with_duration(a.body, b"mvhd", ticks(length, scale)),
                )),
                b"trak" => {
                    let track = &tracks[index];
                    out.extend(trak(a.body, track, offsets[index], scale)?);
                    index += 1;
                }
                kind => out.extend(atom(kind, a.body)),
            }
        }
        Some(atom(b"moov", &out))
    };
    let ftyp = find(&top, b"ftyp").map(|f| atom(b"ftyp", f));
    let head = ftyp.as_ref().map_or(0, Vec::len);
    let bodies: Vec<Vec<u8>> = tracks.iter().map(|t| t.data(data)).collect();
    let moov_length = build(&vec![0; tracks.len()])?.len();
    let mut at = (head + moov_length + 8) as u64;
    let mut offsets = Vec::new();
    for body in &bodies {
        offsets.push(at);
        at += body.len() as u64;
    }
    let mut out = ftyp.unwrap_or_default();
    out.extend(build(&offsets)?);
    let size = 8 + bodies.iter().map(Vec::len).sum::<usize>();
    out.extend((u32::try_from(size).ok()?).to_be_bytes());
    out.extend(b"mdat");
    for body in bodies {
        out.extend(body);
    }
    Some(Kept {
        data: out,
        from,
        length,
    })
}

/// A track with its samples cut, its edits gone and its durations new.
fn trak(body: &[u8], track: &Track, offset: u64, movie_scale: u64) -> Option<Vec<u8>> {
    let duration = track.duration();
    let mut out = Vec::new();
    for a in atoms(body)? {
        match &a.kind {
            b"tkhd" => out.extend(atom(
                b"tkhd",
                &with_duration(a.body, b"tkhd", duration * movie_scale / track.scale.max(1)),
            )),
            // The edits name times of the file as it was.
            b"edts" => {}
            b"mdia" => {
                let mut mdia = Vec::new();
                for m in atoms(a.body)? {
                    match &m.kind {
                        b"mdhd" => {
                            mdia.extend(atom(b"mdhd", &with_duration(m.body, b"mdhd", duration)))
                        }
                        b"minf" => {
                            let mut minf = Vec::new();
                            for i in atoms(m.body)? {
                                if &i.kind == b"stbl" {
                                    minf.extend(track.stbl(offset)?);
                                } else {
                                    minf.extend(atom(&i.kind, i.body));
                                }
                            }
                            mdia.extend(atom(b"minf", &minf));
                        }
                        kind => mdia.extend(atom(kind, m.body)),
                    }
                }
                out.extend(atom(b"mdia", &mdia));
            }
            kind => out.extend(atom(kind, a.body)),
        }
    }
    Some(atom(b"trak", &out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(rate: u32, frames: u32) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend(1u16.to_le_bytes());
        fmt.extend(1u16.to_le_bytes());
        fmt.extend(rate.to_le_bytes());
        fmt.extend((rate * 2).to_le_bytes());
        fmt.extend(2u16.to_le_bytes());
        fmt.extend(16u16.to_le_bytes());
        let samples: Vec<u8> = (0..frames).flat_map(|i| (i as u16).to_le_bytes()).collect();
        let mut out = b"RIFF\0\0\0\0WAVE".to_vec();
        out.extend(b"fmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(fmt);
        out.extend(b"data");
        out.extend((samples.len() as u32).to_le_bytes());
        out.extend(samples);
        let size = (out.len() - 8) as u32;
        out[4..8].copy_from_slice(&size.to_le_bytes());
        out
    }

    #[test]
    fn obj_20_a_wave_sound_keeps_the_samples_from_its_start_to_its_stop() {
        let mut media = Media::from_bytes(wave(1000, 10_000)).unwrap();
        assert!(!media.reducible());
        assert!(media.reduced().is_none());
        media.start = 2_000;
        media.stop = Some(5_000);
        media.poster_time = 3_000;
        media.volume = 40;
        let reduced = media.reduced().unwrap();
        assert_eq!(
            (
                reduced.start,
                reduced.stop,
                reduced.duration,
                reduced.poster_time
            ),
            (0, None, Some(3_000), 1_000)
        );
        assert_eq!(reduced.volume, 40);
        let data = reduced.data();
        assert_eq!(le32(data, 4).unwrap() as usize, data.len() - 8);
        assert_eq!(le32(data, 40).unwrap(), 6_000);
        // The first sample kept is the 2000th.
        assert_eq!(le16(data, 44).unwrap(), 2_000);
        assert_eq!(le16(data, data.len() - 2).unwrap(), 4_999);
        assert!(!reduced.reducible());
    }

    #[test]
    fn obj_20_an_aiff_sound_keeps_its_samples_and_says_how_many() {
        let mut common = Vec::new();
        common.extend(1u16.to_be_bytes());
        common.extend(8_000u32.to_be_bytes());
        common.extend(8u16.to_be_bytes());
        // 8000 as an 80-bit number.
        common.extend([0x40, 0x0B, 0xFA, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extended(&common[8..18]), Some(8000.0));
        let mut sound = vec![0; 8];
        sound.extend((0..8_000u32).map(|i| (i / 100) as u8));
        let mut data = b"FORM\0\0\0\0AIFF".to_vec();
        for (id, body) in [(b"COMM", &common), (b"SSND", &sound)] {
            data.extend(id);
            data.extend((body.len() as u32).to_be_bytes());
            data.extend(body);
        }
        let mut media = Media::from_bytes(data).unwrap();
        media.start = 500;
        let reduced = media.reduced().unwrap();
        assert_eq!(
            (reduced.start, reduced.stop, reduced.duration),
            (0, None, Some(500))
        );
        let data = reduced.data();
        assert_eq!(be32(data, 22).unwrap(), 4_000);
        assert_eq!(data[12 + 8 + 18 + 8 + 8], 40);
    }

    /// A frame of MPEG-1 layer III at 128 kb/s and 44.1 kHz, which lasts
    /// 1152 samples, about 26 milliseconds.
    fn frame(fill: u8) -> Vec<u8> {
        let mut frame = vec![0xFF, 0xFB, 0x90, 0x64];
        let (length, ..) = mp3_frame(u32::from_be_bytes([0xFF, 0xFB, 0x90, 0x64])).unwrap();
        frame.resize(length, fill);
        frame
    }

    #[test]
    fn obj_20_an_mp3_sound_keeps_its_frames_and_its_tags() {
        let mut data = b"ID3\x04\0\0\0\0\0\x02ab".to_vec();
        for i in 0..100 {
            data.extend(frame(i));
        }
        data.extend(b"TAG");
        data.resize(data.len() + 125, b' ');
        let mut media = Media::from_bytes(data).unwrap();
        media.start = 1_000;
        media.stop = Some(2_000);
        let reduced = media.reduced().unwrap();
        let data = reduced.data();
        assert!(data.starts_with(b"ID3\x04\0\0\0\0\0\x02ab"));
        assert_eq!(&data[data.len() - 128..data.len() - 125], b"TAG");
        let frames = (data.len() - 12 - 128) / frame(0).len();
        // From the frame the start falls in to the one the stop does.
        assert_eq!(frames, 39);
        assert_eq!(data[12 + 4], 38);
        let duration = reduced.duration.unwrap();
        assert!((1_000..1_050).contains(&duration), "{duration}");
        assert!(reduced.start < 30, "{}", reduced.start);
    }

    /// A track of `count` samples of four bytes, `delta` long at
    /// `scale` a second, in chunks of two at `chunks`.
    fn track(
        id: u32,
        scale: u32,
        count: u32,
        delta: u32,
        keys: Option<&[u32]>,
        chunks: &[u32],
    ) -> Vec<u8> {
        let mut tkhd = vec![0, 0, 0, 1];
        tkhd.extend([0; 8]);
        tkhd.extend(id.to_be_bytes());
        tkhd.extend([0; 4]);
        tkhd.extend((count * delta * 600 / scale).to_be_bytes());
        tkhd.extend([0; 60]);
        let mut mdhd = vec![0; 12];
        mdhd.extend(scale.to_be_bytes());
        mdhd.extend((count * delta).to_be_bytes());
        mdhd.extend([0; 4]);
        let mut stsd = 1u32.to_be_bytes().to_vec();
        stsd.extend(atom(b"mp4a", &[0; 28]));
        let stts = rows_of(&[(count, delta)]);
        let mut stsz = 0u32.to_be_bytes().to_vec();
        stsz.extend(count.to_be_bytes());
        for _ in 0..count {
            stsz.extend(4u32.to_be_bytes());
        }
        let mut stsc = 1u32.to_be_bytes().to_vec();
        for n in [1u32, 2, 1] {
            stsc.extend(n.to_be_bytes());
        }
        let mut stco = (chunks.len() as u32).to_be_bytes().to_vec();
        for c in chunks {
            stco.extend(c.to_be_bytes());
        }
        let mut stbl = full_atom(b"stsd", &stsd);
        stbl.extend(full_atom(b"stts", &stts));
        if let Some(keys) = keys {
            let mut stss = (keys.len() as u32).to_be_bytes().to_vec();
            for k in keys {
                stss.extend(k.to_be_bytes());
            }
            stbl.extend(full_atom(b"stss", &stss));
        }
        stbl.extend(full_atom(b"stsc", &stsc));
        stbl.extend(full_atom(b"stsz", &stsz));
        stbl.extend(full_atom(b"stco", &stco));
        let mut dref = 1u32.to_be_bytes().to_vec();
        dref.extend(atom(b"url ", &[0, 0, 0, 1]));
        let minf = [
            atom(b"dinf", &full_atom(b"dref", &dref)),
            atom(b"stbl", &stbl),
        ]
        .concat();
        let mdia = [atom(b"mdhd", &mdhd), atom(b"minf", &minf)].concat();
        let trak = [
            atom(b"tkhd", &tkhd),
            atom(b"edts", &[0; 8]),
            atom(b"mdia", &mdia),
        ]
        .concat();
        atom(b"trak", &trak)
    }

    /// A movie of a video track of ten frames a second for three
    /// seconds, with a key frame every fifth, and a sound track of twenty
    /// samples a second. Each sample begins with its track and number.
    fn movie() -> Vec<u8> {
        let mut mvhd = vec![0; 12];
        mvhd.extend(600u32.to_be_bytes());
        mvhd.extend(1800u32.to_be_bytes());
        mvhd.extend([0; 80]);
        let ftyp = atom(b"ftyp", b"qt  \0\0\0\0qt  ");
        let keys: &[u32] = &[1, 6, 11, 16, 21, 26];
        let moov = |video: &[u32], sound: &[u32]| {
            atom(
                b"moov",
                &[
                    atom(b"mvhd", &mvhd),
                    track(1, 600, 30, 60, Some(keys), video),
                    track(2, 1000, 60, 50, None, sound),
                ]
                .concat(),
            )
        };
        let at = (ftyp.len() + moov(&[0; 15], &[0; 30]).len() + 8) as u32;
        // The video first, then the sound, in chunks of eight bytes.
        let video: Vec<u32> = (0..15).map(|c| at + c * 8).collect();
        let sound: Vec<u32> = (0..30).map(|c| at + 120 + c * 8).collect();
        let mut mdat = Vec::new();
        for (base, count) in [(1u8, 30u32), (2, 60)] {
            for i in 0..count {
                mdat.extend([base, i as u8, 0, 0]);
            }
        }
        [ftyp, moov(&video, &sound), atom(b"mdat", &mdat)].concat()
    }

    #[test]
    fn obj_20_a_movie_keeps_its_samples_from_the_key_frame_before_the_start() {
        let data = movie();
        let mut media = Media::from_bytes(data.clone()).unwrap();
        assert_eq!(media.format(), MediaFormat::QuickTime);
        // Whole, the movie reads as it was written.
        let top = atoms(&data).unwrap();
        let traks: Vec<_> = atoms(find(&top, b"moov").unwrap())
            .unwrap()
            .into_iter()
            .filter(|a| &a.kind == b"trak")
            .collect();
        let video = Track::read(traks[0].body, &data).unwrap();
        assert_eq!(&data[video.samples[7].2..video.samples[7].2 + 2], [1, 7]);
        media.start = 1_200;
        media.stop = Some(2_000);
        media.poster_time = 1_500;
        let reduced = media.reduced().unwrap();
        // The key frame before 1.2 s is the sixth, at 1 s.
        assert_eq!(reduced.start, 200);
        assert_eq!(reduced.stop, None);
        assert_eq!(reduced.duration, Some(1_000));
        assert_eq!(reduced.poster_time, 500);
        let data = reduced.data();
        let top = atoms(data).unwrap();
        let kinds: Vec<_> = top.iter().map(|a| a.kind).collect();
        assert_eq!(kinds, [*b"ftyp", *b"moov", *b"mdat"]);
        let moov = atoms(find(&top, b"moov").unwrap()).unwrap();
        let traks: Vec<_> = moov.iter().filter(|a| &a.kind == b"trak").collect();
        let video = Track::read(traks[0].body, data).unwrap();
        let sound = Track::read(traks[1].body, data).unwrap();
        assert_eq!((video.samples.len(), sound.samples.len()), (10, 20));
        assert_eq!(video.sync, Some(vec![0, 5]));
        let first = |t: &Track| data[t.samples[0].2..t.samples[0].2 + 2].to_vec();
        assert_eq!((first(&video), first(&sound)), (vec![1, 10], vec![2, 20]));
        assert_eq!(video.duration(), 600);
        assert!(
            atoms(traks[0].body)
                .unwrap()
                .iter()
                .all(|a| &a.kind != b"edts")
        );
        assert_eq!(be32(find(&moov, b"mvhd").unwrap(), 16), Some(600));
        assert!(data.len() < media.data().len());
    }

    #[test]
    fn obj_20_a_movie_whose_samples_are_in_another_file_is_not_cut() {
        let mut data = movie();
        let at = data.windows(4).position(|w| w == b"url ").unwrap();
        data[at + 7] = 0;
        let mut media = Media::from_bytes(data).unwrap();
        media.start = 1_000;
        assert!(media.reducible());
        assert!(media.reduced().is_none());
    }

    #[test]
    fn obj_20_a_movie_of_mpeg_4_with_sound_is_cut_at_its_stop() {
        let data = include_bytes!("../../../tests/ui/fixtures/movie.mp4");
        let mut media = Media::from_bytes(data.to_vec()).unwrap();
        media.duration = Some(1_000);
        media.stop = Some(500);
        let reduced = media.reduced().expect("reduced");
        assert!(reduced.data().len() < data.len());
        // The sound lasts a little past the stop, which stays.
        assert_eq!(reduced.start, 0);
        assert!(reduced.stop.is_none_or(|s| s == 500), "{reduced:?}");
        assert!(
            reduced.duration.is_some_and(|d| (500..=600).contains(&d)),
            "{reduced:?}"
        );
    }
}
