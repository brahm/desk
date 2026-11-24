// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The versions of a document (DOC-11, D-001, step 5 of window M+): a
//! document with a file saves itself while it is edited, and copies of
//! the file as it was are kept in the data folder of Desk, outside the
//! document, as macOS keeps those of iWork. The file stays plain ODF, the
//! same size, with nothing of its past in it to share by mistake.
//!
//! A version is kept when the document is saved for the first time after
//! it opens (the file as it was opened), every hour while it is edited,
//! when Ctrl+S asks for one, and before an older version takes its place.
//! Old versions are thinned as macOS thins them: all of the last day, one
//! a day for a month, one a week after that.

use std::path::{Path, PathBuf};

use gtk::glib;

/// How often a version is kept while a document is edited.
pub const HOURLY_SECONDS: i64 = 60 * 60;

const DAY: i64 = 24 * 60 * 60;

/// Why a version was kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The file as it was when the document opened.
    Opened,
    /// Kept with Ctrl+S.
    Saved,
    /// Kept by the hour while the document was edited, or before an
    /// older version took its place.
    Automatic,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Opened => "opened",
            Kind::Saved => "saved",
            Kind::Automatic => "auto",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        match word {
            "opened" => Some(Kind::Opened),
            "saved" => Some(Kind::Saved),
            "auto" => Some(Kind::Automatic),
            _ => None,
        }
    }
}

/// A version kept: a copy of the file, when, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub path: PathBuf,
    /// Seconds since 1970, in UTC.
    pub time: i64,
    pub kind: Kind,
}

/// The versions of one document, in a folder of their own named after
/// the path of the document.
#[derive(Debug, Clone)]
pub struct Store {
    folder: PathBuf,
}

/// The folder of the versions of every document.
fn root() -> PathBuf {
    glib::user_data_dir().join("desk").join("versions")
}

/// The name of the folder of a document: a digest of its full path, so
/// that two documents of the same name in different folders keep apart.
fn key(document: &Path) -> String {
    let full = std::fs::canonicalize(document).unwrap_or_else(|_| document.to_owned());
    glib::compute_checksum_for_string(glib::ChecksumType::Sha256, full.to_string_lossy().as_ref())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

impl Store {
    /// The versions of the document at `document`.
    pub fn for_document(document: &Path) -> Self {
        Self::in_root(&root(), document)
    }

    fn in_root(root: &Path, document: &Path) -> Self {
        Self {
            folder: root.join(key(document)),
        }
    }

    /// Keeps a copy of `file` as a version of `kind`, made at `time`.
    pub fn keep(&self, file: &Path, kind: Kind, time: i64) -> std::io::Result<Version> {
        std::fs::create_dir_all(&self.folder)?;
        let extension = file
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Versions of the same second keep apart, and in order, by a
        // number.
        let taken: Vec<u32> = self
            .list()
            .iter()
            .filter(|v| v.time == time)
            .filter_map(|v| {
                v.path
                    .file_stem()?
                    .to_str()?
                    .split('-')
                    .nth(1)?
                    .parse()
                    .ok()
            })
            .collect();
        let n = taken.iter().max().map_or(0, |n| n + 1);
        let name = format!("{time}-{n}-{}.{extension}", kind.word());
        let path = self.folder.join(name);
        std::fs::copy(file, &path)?;
        Ok(Version { path, time, kind })
    }

    /// The versions, newest first.
    pub fn list(&self) -> Vec<Version> {
        let Ok(entries) = std::fs::read_dir(&self.folder) else {
            return Vec::new();
        };
        let mut versions: Vec<(i64, u32, Version)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                let stem = path.file_stem()?.to_str()?.to_owned();
                let mut parts = stem.splitn(3, '-');
                let time = parts.next()?.parse().ok()?;
                let n = parts.next()?.parse().ok()?;
                let kind = Kind::from_word(parts.next()?)?;
                Some((time, n, Version { path, time, kind }))
            })
            .collect();
        versions.sort_by_key(|v| std::cmp::Reverse((v.0, v.1)));
        versions.into_iter().map(|(_, _, v)| v).collect()
    }

    /// The version kept last, if any.
    pub fn latest(&self) -> Option<Version> {
        self.list().into_iter().next()
    }

    /// Removes the versions that [`thinned`] leaves out, as at `now`, for
    /// a place `offset` seconds east of UTC.
    ///
    /// The newest version of the document as it was opened stays, as
    /// Revert to Last Opened goes back to it (review RV, batch 5).
    pub fn thin(&self, now: i64, offset: i64) {
        let versions = self.list();
        let times: Vec<i64> = versions.iter().map(|v| v.time).collect();
        let opened = versions.iter().position(|v| v.kind == Kind::Opened);
        for index in thinned(&times, now, offset) {
            if Some(index) != opened
                && let Some(version) = versions.get(index)
            {
                let _ = std::fs::remove_file(&version.path);
            }
        }
    }

    /// Renames the document at `from` to `to`, and takes its versions
    /// along.
    pub fn renaming(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::rename(from, to)?;
        self.move_to(to);
        Ok(())
    }

    /// Takes the versions along when the document moves to `document`,
    /// as when it is renamed.
    pub fn move_to(&self, document: &Path) -> Self {
        let parent = self.folder.parent().map(Path::to_owned).unwrap_or_default();
        let moved = Self::in_root(&parent, document);
        if self.folder.exists() && self.folder != moved.folder {
            // Versions left by a file of that name that is gone are not
            // shown as those of this document (review RV, batch 5), and
            // are removed rather than set aside, where they piled up with
            // nothing to show them (second review).
            // They are set aside under a name of their own, and removed
            // once the versions of this document are in their place
            // (third and fourth reviews). Others set aside before, by a
            // crash or a removal that failed, go first. This runs once the
            // document has taken the name, so no file of that name holds
            // them any more.
            let name = moved
                .folder
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let stale = format!("{name}.replaced");
            if let Ok(entries) = std::fs::read_dir(&parent) {
                for entry in entries.flatten() {
                    if entry.file_name().to_string_lossy().starts_with(&stale) {
                        let _ = std::fs::remove_dir_all(entry.path());
                    }
                }
            }
            let aside = parent.join(format!("{stale}-{:08x}", glib::random_int()));
            if moved.folder.exists() && std::fs::rename(&moved.folder, &aside).is_err() {
                // Not set aside: removed where they are, so that they are
                // never shown as the versions of this document.
                let _ = std::fs::remove_dir_all(&moved.folder);
            }
            if moved.folder.exists() {
                glib::g_warning!(
                    "desk",
                    "cannot remove the versions of a file gone: {}",
                    moved.folder.display()
                );
                return moved;
            }
            match std::fs::rename(&self.folder, &moved.folder) {
                Ok(()) => {
                    let _ = std::fs::remove_dir_all(&aside);
                }
                // The versions of the file gone stay aside, never shown,
                // until the next move to that name removes them.
                Err(error) => glib::g_warning!(
                    "desk",
                    "cannot take the versions along to {}: {error}",
                    moved.folder.display()
                ),
            }
        }
        moved
    }
}

/// Which versions to remove, by their index in `times`, newest first: all
/// those of the last day stay; from a day to a month old, the newest of
/// each day; older, the newest of each week. Days begin at midnight where
/// the clock is `offset` seconds east of UTC.
pub fn thinned(times: &[i64], now: i64, offset: i64) -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    let mut removed = Vec::new();
    for (index, &time) in times.iter().enumerate() {
        let age = now - time;
        if age < DAY {
            continue;
        }
        let day = (time + offset).div_euclid(DAY);
        let bucket = if age < 30 * DAY {
            (0, day)
        } else {
            (1, day.div_euclid(7))
        };
        // The list is newest first: the first of a bucket stays.
        if !seen.insert(bucket) {
            removed.push(index);
        }
    }
    removed
}

/// The time now, in seconds since 1970.
pub fn now() -> i64 {
    glib::real_time() / 1_000_000
}

/// How far the clock of this place is east of UTC, in seconds.
pub fn local_offset() -> i64 {
    glib::DateTime::now_local()
        .map(|d| d.utc_offset().as_seconds())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("desk-versions-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn doc_11_versions_are_kept_and_listed_newest_first() {
        let folder = scratch("keep");
        let document = folder.join("Letter.odt");
        std::fs::write(&document, "first").unwrap();
        let store = Store::in_root(&folder.join("versions"), &document);
        store.keep(&document, Kind::Opened, 100).unwrap();
        std::fs::write(&document, "second").unwrap();
        store.keep(&document, Kind::Saved, 100).unwrap();
        store.keep(&document, Kind::Automatic, 200).unwrap();
        let versions = store.list();
        let kinds: Vec<_> = versions.iter().map(|v| (v.time, v.kind)).collect();
        assert_eq!(
            kinds,
            [
                (200, Kind::Automatic),
                (100, Kind::Saved),
                (100, Kind::Opened)
            ]
        );
        assert_eq!(std::fs::read_to_string(&versions[2].path).unwrap(), "first");
        assert_eq!(versions[2].path.extension().unwrap(), "odt");
        assert_eq!(store.latest().unwrap().time, 200);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn doc_11_documents_in_different_folders_keep_their_versions_apart() {
        let root = Path::new("/nowhere");
        let a = Store::in_root(root, Path::new("/a/Letter.odt"));
        let b = Store::in_root(root, Path::new("/b/Letter.odt"));
        assert_ne!(a.folder, b.folder);
    }

    #[test]
    fn doc_11_versions_follow_a_renamed_document() {
        let folder = scratch("move");
        let document = folder.join("Letter.odt");
        std::fs::write(&document, "x").unwrap();
        let store = Store::in_root(&folder.join("versions"), &document);
        store.keep(&document, Kind::Opened, 1).unwrap();
        let moved = store.move_to(&folder.join("Report.odt"));
        assert_eq!(moved.list().len(), 1);
        assert!(store.list().is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_renamed_document_takes_its_versions_over_those_of_a_file_gone() {
        // Review RV, batch 5: renamed to the name of a deleted file, a
        // document showed the versions of that file.
        let folder = scratch("move-over");
        let root = folder.join("versions");
        let gone = folder.join("Report.odt");
        std::fs::write(&gone, "old report").unwrap();
        Store::in_root(&root, &gone)
            .keep(&gone, Kind::Opened, 1)
            .unwrap();
        std::fs::remove_file(&gone).unwrap();
        let letter = folder.join("Letter.odt");
        std::fs::write(&letter, "letter").unwrap();
        let store = Store::in_root(&root, &letter);
        store.keep(&letter, Kind::Opened, 2).unwrap();
        let moved = store.move_to(&gone);
        let versions = moved.list();
        assert_eq!(versions.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&versions[0].path).unwrap(),
            "letter"
        );
        // Nothing is left of the versions of the file gone.
        let folders = std::fs::read_dir(&root).unwrap().count();
        assert_eq!(folders, 1);
        // Fourth review: what a crash left aside does not stop the next
        // move, and goes.
        let key = moved
            .folder
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        std::fs::create_dir_all(root.join(format!("{key}.replaced"))).unwrap();
        std::fs::create_dir_all(root.join(format!("{key}.replaced-1234abcd"))).unwrap();
        let back = moved.move_to(&letter).move_to(&gone);
        assert_eq!(back.list().len(), 1);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_version_as_opened_is_not_thinned() {
        // Review RV, batch 5: a day after, the copy of the file as opened
        // went with the others of its day, and Revert had nothing to go
        // back to.
        let folder = scratch("thin-opened");
        let document = folder.join("Letter.odt");
        std::fs::write(&document, "x").unwrap();
        let store = Store::in_root(&folder.join("versions"), &document);
        let now = 1000 * DAY;
        store.keep(&document, Kind::Opened, now - 2 * DAY).unwrap();
        store
            .keep(&document, Kind::Automatic, now - 2 * DAY + 60)
            .unwrap();
        store.thin(now, 0);
        let kinds: Vec<_> = store.list().iter().map(|v| v.kind).collect();
        assert_eq!(kinds, [Kind::Automatic, Kind::Opened]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn doc_11_old_versions_are_thinned_by_day_and_by_week() {
        let now = 1000 * DAY;
        let hour = 3600;
        let times = [
            now - hour,               // 0: today, stays
            now - 2 * hour,           // 1: today, stays
            now - 2 * DAY + 2 * hour, // 2: newest of its day, stays
            now - 2 * DAY + hour,     // 3: same day, goes
            now - 3 * DAY,            // 4: newest of its day, stays
            now - 40 * DAY,           // 5: newest of its week, stays
            now - 41 * DAY,           // 6: same week as 5, goes
        ];
        assert_eq!(thinned(&times, now, 0), [3, 6]);
    }
}
