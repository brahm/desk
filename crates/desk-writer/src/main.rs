// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Desk Writer, the word processing and page layout app of Desk.
//!
//! Desk Writer aims at feature parity and document compatibility with Pages
//! '09, and saves documents as ODF text documents.

use std::path::Path;

use desk_ui::gtk::glib;
use desk_ui::{AppInfo, Opened, Opener};

const APP: AppInfo = AppInfo {
    id: "br.com.brahm.Desk.Writer",
    name: "Desk Writer",
    version: env!("CARGO_PKG_VERSION"),
};

/// The documents that Desk Writer opens.
const OPENER: Opener = Opener {
    filters: &[
        ("OpenDocument Text", &["odt", "ott"]),
        ("Pages Documents", &["pages", "template"]),
    ],
    open,
};

/// Reads an OpenDocument text or a Pages '09 document.
fn open(path: &Path) -> Result<Opened, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("odt" | "ott") => {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let document = desk_filters::read_odt(&bytes).map_err(|e| e.to_string())?;
            Ok(Opened {
                document,
                warnings: Vec::new(),
            })
        }
        Some("pages" | "template") => {
            let (document, log) =
                desk_filters::read_pages_at_logged(path).map_err(|e| e.to_string())?;
            Ok(Opened {
                document,
                warnings: log.entries().iter().map(|e| format!("{e}.")).collect(),
            })
        }
        _ => Err(
            "Desk Writer opens OpenDocument texts (.odt) and Pages documents (.pages).".to_owned(),
        ),
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
        assert!(!opened.warnings.is_empty());
        assert!(opened.warnings[0].starts_with("Left out headers"));
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
