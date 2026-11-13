// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Desk Writer, the word processing and page layout app of Desk.
//!
//! Desk Writer aims at feature parity and document compatibility with Pages
//! '09, and saves documents as ODF text documents.

mod warnings;

use std::path::Path;

use desk_ui::gtk::glib;
use desk_ui::i18n::{n_, tr};
use desk_ui::{AppInfo, Document, Fragment, Object, ObjectsFormat, Opened, Opener};

const APP: AppInfo = AppInfo {
    id: "br.com.brahm.Desk.Writer",
    name: "Desk Writer",
    version: env!("CARGO_PKG_VERSION"),
};

/// The documents that Desk Writer opens.
const OPENER: Opener = Opener {
    filters: &[
        (n_("OpenDocument Text"), &["odt", "ott"]),
        (n_("Pages Documents"), &["pages", "template"]),
    ],
    open,
    saves: (n_("OpenDocument Text"), "odt"),
    save,
    objects: Some(ObjectsFormat {
        media_type: desk_filters::clip::OBJECTS_TYPE,
        write: copy_objects,
        read: paste_objects,
        text_type: desk_filters::clip::TEXT_TYPE,
        write_text: copy_text,
        read_text: paste_text,
        foreign_text_types: &desk_filters::clip::FOREIGN_TEXT_TYPES,
        read_foreign_text: paste_foreign_text,
        write_foreign_text: copy_foreign_text,
        named_styles: true,
    }),
};

/// Puts objects on the clipboard for the other apps of Desk.
fn copy_objects(document: &Document, objects: &[Object]) -> Result<Vec<u8>, String> {
    desk_filters::clip::write_objects(document, objects).map_err(|e| e.to_string())
}

/// Reads the objects that an app of Desk put on the clipboard.
fn paste_objects(bytes: &[u8]) -> Result<Document, String> {
    desk_filters::clip::read_objects(bytes)
        .map(|clip| clip.document().clone())
        .map_err(|e| e.to_string())
}

/// Puts text with its styles on the clipboard for the other apps of Desk.
fn copy_text(fragment: &Fragment) -> Result<Vec<u8>, String> {
    desk_filters::clip::write_text(fragment).map_err(|e| e.to_string())
}

/// Reads the text with its styles that an app of Desk put on the
/// clipboard.
fn paste_text(bytes: &[u8]) -> Result<Fragment, String> {
    desk_filters::clip::read_text(bytes).map_err(|e| e.to_string())
}

/// Reads formatted text that another program put on the clipboard.
fn paste_foreign_text(media_type: &str, bytes: &[u8]) -> Result<Fragment, String> {
    desk_filters::clip::read_foreign_text(media_type, bytes).map_err(|e| e.to_string())
}

/// Puts text with its formatting on the clipboard for other programs.
fn copy_foreign_text(fragment: &Fragment) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    desk_filters::clip::write_foreign_text(fragment).map_err(|e| e.to_string())
}

/// Writes a document as an OpenDocument text, with its preview.
fn save(document: &Document, thumbnail: Option<&[u8]>) -> Result<Vec<u8>, String> {
    desk_filters::write_odt_with_thumbnail(document, thumbnail).map_err(|e| e.to_string())
}

/// Reads an OpenDocument text or a Pages '09 document.
fn open(path: &Path) -> Result<Opened, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("odt" | "ott") => {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let (document, log) =
                desk_filters::read_odt_logged(&bytes).map_err(|e| e.to_string())?;
            Ok(Opened {
                document,
                warnings: log.entries().iter().map(warnings::describe).collect(),
            })
        }
        Some("pages" | "template") => {
            let (document, log) =
                desk_filters::read_pages_at_logged(path).map_err(|e| e.to_string())?;
            Ok(Opened {
                document,
                warnings: log.entries().iter().map(warnings::describe).collect(),
            })
        }
        _ => Err(tr(
            "Desk Writer opens OpenDocument texts (.odt) and Pages documents (.pages).",
        )),
    }
}

fn main() -> glib::ExitCode {
    desk_ui::run(APP, Some(&OPENER))
}

#[cfg(test)]
mod tests {
    use desk_ui::gtk::gio;

    #[test]
    fn doc_03_opens_the_documents_of_the_fixtures() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages");
        let opened = super::open(&dir.join("lossy-import.pages")).unwrap();
        // What it opens, it saves as ODT, which opens again.
        let bytes = super::save(&opened.document, None).unwrap();
        let saved = desk_filters::read_odt(&bytes).unwrap();
        assert_eq!(saved.statistics().words, opened.document.statistics().words);
        assert!(!opened.warnings.is_empty());
        assert!(opened.warnings[0].starts_with("Left out"));
        assert!(super::open(&dir.join("manifest.tsv")).is_err());
    }

    #[test]
    fn app_id_is_valid() {
        assert!(gio::Application::id_is_valid(super::APP.id));
    }

    #[test]
    fn app_has_its_icons() {
        assert!(desk_ui::has_icon(super::APP.id));
        assert!(desk_ui::has_icon(&format!("{}-symbolic", super::APP.id)));
    }
}
