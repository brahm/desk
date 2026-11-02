// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The information of a document's file (DOC-21): its size, where it is,
//! and when it was made and last changed.

use std::path::Path;
use std::time::SystemTime;

use adw::prelude::*;
use gtk::glib;

/// The size of a file, or of all the files of a package folder, in bytes.
pub fn package_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    std::fs::read_dir(path).map_or(0, |entries| {
        entries
            .filter_map(Result::ok)
            .map(|entry| package_size(&entry.path()))
            .sum()
    })
}

/// A time as people read it, in their locale.
fn when(time: Option<SystemTime>) -> String {
    time.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .and_then(|d| glib::DateTime::from_unix_local(d.as_secs() as i64).ok())
        .and_then(|t| t.format("%c").ok())
        .map_or_else(|| "Unknown".to_owned(), String::from)
}

/// Shows the information of the file at `path` over `parent`.
pub fn show(parent: &impl IsA<gtk::Widget>, path: &Path) {
    let metadata = std::fs::metadata(path).ok();
    let size = package_size(path);
    let location = path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let rows = [
        ("Name", crate::window::display_name(path)),
        ("Location", location),
        (
            "Size",
            format!("{} ({size} bytes)", glib::format_size(size)),
        ),
        (
            "Created",
            when(metadata.as_ref().and_then(|m| m.created().ok())),
        ),
        (
            "Modified",
            when(metadata.as_ref().and_then(|m| m.modified().ok())),
        ),
    ];

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    for (title, value) in rows {
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(glib::markup_escape_text(&value))
            .subtitle_selectable(true)
            .css_classes(["property"])
            .build();
        list.append(&row);
    }
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_top(12)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&list);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&adw::Clamp::builder().child(&content).build()));

    let dialog = adw::Dialog::builder()
        .title("Document Info")
        .content_width(420)
        .child(&view)
        .build();
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_21_a_package_weighs_all_its_files() {
        let dir = std::env::temp_dir().join(format!("desk-ui-size-{}", std::process::id()));
        let package = dir.join("Report.pages");
        std::fs::create_dir_all(package.join("QuickLook")).unwrap();
        std::fs::write(package.join("index.xml.gz"), [0; 100]).unwrap();
        std::fs::write(package.join("QuickLook/Thumbnail.jpg"), [0; 20]).unwrap();
        std::fs::write(dir.join("Letter.odt"), [0; 7]).unwrap();
        assert_eq!(package_size(&package), 120);
        assert_eq!(package_size(&dir.join("Letter.odt")), 7);
        assert_eq!(package_size(&dir.join("missing")), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
