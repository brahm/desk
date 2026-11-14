// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Recovery drafts: every few minutes a window with unsaved changes keeps
//! a copy of its document in the data folder of the app, and drops it
//! when the document is saved or the window closes. Drafts that remain
//! when the app starts are from a crash, and the app offers them back.
//! There is no browser of versions, as GNOME apps keep none.

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
        let file = format!("{}-{stamp}-{n}.{extension}", std::process::id());
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
    leftovers_in(&folder(), std::process::id())
}

fn leftovers_in(folder: &Path, pid: u32) -> Vec<Draft> {
    let ours = format!("{pid}-");
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
        let drafts = leftovers_in(&folder, 42);
        assert_eq!(drafts, vec![Draft::at(folder.join("1-5-1.odt"))]);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
