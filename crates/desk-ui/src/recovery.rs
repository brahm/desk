// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Recovery drafts: every few minutes a window with unsaved changes keeps
//! a copy of its document in the data folder of the app, and drops it
//! when the document is saved or the window closes. Drafts that remain
//! when the app starts are from a crash, and the app offers them back.
//! Only documents that cannot save themselves keep drafts: new ones, and
//! files of another kind; a document with a file of the kind the app
//! saves saves itself and keeps versions instead (DOC-11, `versions`).

use std::cell::Cell;
use std::path::{Path, PathBuf};

use gtk::glib;

/// How often a window keeps a draft of unsaved changes.
pub const INTERVAL_SECONDS: u32 = 120;

/// The folder of the drafts.
fn folder() -> PathBuf {
    glib::user_data_dir().join("desk").join("recovery")
}

thread_local! {
    /// The drafts made since the app started, numbered.
    static COUNT: Cell<u32> = const { Cell::new(0) };
    /// What the names of the drafts of this run of the app start with:
    /// not the process ID, which in a Flatpak sandbox is the same at each
    /// run, so that the drafts of a crash looked like those of the run
    /// after it and were never offered back (review RV, batch 5).
    static RUN: String = glib::uuid_string_random().replace('-', "");
}

/// The draft of one window: the copy of the document and, beside it, the
/// name of the document, to show when it is offered back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub path: PathBuf,
}

impl Draft {
    /// A new draft for a window, with the extension of the files that
    /// the app saves. Nothing is written until [`Draft::name_file`] and a
    /// save of the document there.
    pub fn new(extension: &str) -> Self {
        let n = COUNT.with(|c| {
            c.set(c.get() + 1);
            c.get()
        });
        let stamp = glib::real_time();
        let run = RUN.with(Clone::clone);
        let file = format!("{run}-{stamp}-{n}.{extension}");
        Self {
            path: folder().join(file),
        }
    }

    /// The draft at a path, as [`leftovers`] lists them.
    fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// Makes the folder and writes the name of the document beside the
    /// draft.
    pub fn name_file(&self, name: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(folder())?;
        std::fs::write(self.name_path(), name)
    }

    fn name_path(&self) -> PathBuf {
        let mut name = self.path.clone().into_os_string();
        name.push(".name");
        PathBuf::from(name)
    }

    /// The name of the document that the draft keeps.
    pub fn name(&self) -> String {
        std::fs::read_to_string(self.name_path()).unwrap_or_default()
    }

    /// Removes the draft, if there is one.
    pub fn remove(&self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.name_path());
    }
}

/// The drafts that a crash left: those in the folder that this run of
/// the app did not make, oldest first.
pub fn leftovers() -> Vec<Draft> {
    RUN.with(|run| leftovers_in(&folder(), run))
}

fn leftovers_in(folder: &Path, run: &str) -> Vec<Draft> {
    let ours = format!("{run}-");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut drafts: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e != "name"))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with(&ours))
        })
        .map(|p| {
            let modified = p
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (modified, p)
        })
        .collect();
    drafts.sort();
    drafts.into_iter().map(|(_, p)| Draft::at(p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_11_leftovers_leave_out_the_drafts_of_this_run_and_the_names() {
        let folder = std::env::temp_dir().join(format!("desk-recovery-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for file in [
            "1-5-1.odt",
            "1-5-1.odt.name",
            "42-6-1.odt",
            "42-6-1.odt.name",
        ] {
            std::fs::write(folder.join(file), "x").unwrap();
        }
        let drafts = leftovers_in(&folder, "42");
        assert_eq!(drafts, vec![Draft::at(folder.join("1-5-1.odt"))]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_drafts_of_a_run_are_named_apart_from_its_process() {
        // Review RV, batch 5: in Flatpak each run has the same process
        // ID, and the drafts of a crash were taken for those of the run
        // after it.
        let draft = Draft::new("odt");
        let name = draft
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let run = RUN.with(Clone::clone);
        assert!(name.starts_with(&format!("{run}-")), "{name}");
        assert!(run.len() >= 32);
        assert!(!name.starts_with(&format!("{}-", std::process::id())));
    }
}
