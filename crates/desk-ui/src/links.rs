// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a link points to (EST-25), as Pages' Link inspector offers it: a
//! web page, an e-mail message with its subject, another document, at
//! one of its bookmarks if wanted, or a bookmark of this document. Each
//! is kept as the text of a URI.

use gtk::gio::prelude::FileExt;
use gtk::glib;

/// What a link points to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A web page, or any other address.
    Web(String),
    /// An e-mail message to someone, with a subject.
    Email {
        /// Who the message is to.
        to: String,
        /// Its subject; empty for none.
        subject: String,
    },
    /// Another document, at one of its bookmarks or at its start.
    Document {
        /// The URI of the file, or its path from the folder of this
        /// document.
        file: String,
        /// The bookmark; empty for the start.
        bookmark: String,
    },
    /// A bookmark of this document.
    Bookmark(String),
}

/// The extensions of the documents that a link opens in the app.
const DOCUMENTS: [&str; 4] = ["odt", "pages", "fodt", "ott"];

/// Escapes text for a part of a URI.
fn escape(text: &str) -> String {
    glib::Uri::escape_string(text, None, false).to_string()
}

fn unescape(text: &str) -> String {
    glib::Uri::unescape_string(text, None::<&str>)
        .map_or_else(|| text.to_owned(), |s| s.to_string())
}

impl Target {
    /// What a link points to.
    pub fn parse(link: &str) -> Self {
        if let Some(name) = link.strip_prefix('#') {
            return Self::Bookmark(name.to_owned());
        }
        if let Some(rest) = link.strip_prefix("mailto:") {
            let (to, query) = rest.split_once('?').unwrap_or((rest, ""));
            let subject = query
                .split('&')
                .find_map(|pair| {
                    let (key, value) = pair.split_once('=')?;
                    key.eq_ignore_ascii_case("subject").then(|| unescape(value))
                })
                .unwrap_or_default();
            return Self::Email {
                to: unescape(to),
                subject,
            };
        }
        let (file, bookmark) = link.split_once('#').unwrap_or((link, ""));
        let has_scheme = file.split_once(':').is_some_and(|(scheme, _)| {
            scheme.len() > 1 && scheme.chars().all(|c| c.is_ascii_alphabetic())
        });
        let extension = file
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        let local = file.starts_with("file:") || !has_scheme;
        if local && DOCUMENTS.contains(&extension.as_str()) {
            return Self::Document {
                file: file.to_owned(),
                bookmark: unescape(bookmark),
            };
        }
        Self::Web(link.to_owned())
    }

    /// The link that points to it.
    pub fn link(&self) -> String {
        match self {
            Self::Web(address) => address.clone(),
            Self::Email { to, subject } if subject.is_empty() => format!("mailto:{to}"),
            Self::Email { to, subject } => format!("mailto:{to}?subject={}", escape(subject)),
            Self::Document { file, bookmark } if bookmark.is_empty() => file.clone(),
            Self::Document { file, bookmark } => format!("{file}#{}", escape(bookmark)),
            Self::Bookmark(name) => format!("#{name}"),
        }
    }

    /// Its place among the kinds, in the order the Inspector offers
    /// them.
    pub fn kind(&self) -> u32 {
        match self {
            Self::Web(_) => 0,
            Self::Email { .. } => 1,
            Self::Document { .. } => 2,
            Self::Bookmark(_) => 3,
        }
    }
}

/// The file that a link to a document opens, from the folder of this
/// document, and the bookmark to go to.
pub fn document_file(
    file: &str,
    bookmark: &str,
    folder: Option<&std::path::Path>,
) -> Option<(std::path::PathBuf, String)> {
    let path = if file.starts_with("file:") {
        gtk::gio::File::for_uri(file).path()?
    } else {
        let relative = std::path::PathBuf::from(unescape(file));
        if relative.is_absolute() {
            relative
        } else {
            folder?.join(relative)
        }
    };
    Some((path, bookmark.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_25_links_say_what_they_point_to() {
        let email = Target::Email {
            to: "ana@example.com".into(),
            subject: "Olá & até".into(),
        };
        assert_eq!(
            email.link(),
            "mailto:ana@example.com?subject=Ol%C3%A1%20%26%20at%C3%A9"
        );
        assert_eq!(Target::parse(&email.link()), email);
        assert_eq!(
            Target::parse("mailto:bo@example.com"),
            Target::Email {
                to: "bo@example.com".into(),
                subject: String::new()
            }
        );
        let document = Target::Document {
            file: "Capítulos/Dois.odt".into(),
            bookmark: "Fim do dia".into(),
        };
        assert_eq!(document.link(), "Capítulos/Dois.odt#Fim%20do%20dia");
        assert_eq!(Target::parse(&document.link()), document);
        assert_eq!(
            Target::parse("file:///home/a/Um.pages"),
            Target::Document {
                file: "file:///home/a/Um.pages".into(),
                bookmark: String::new()
            }
        );
        assert_eq!(
            Target::parse("https://example.com/a.odt#b"),
            Target::Web("https://example.com/a.odt#b".into())
        );
        assert_eq!(Target::parse("#Dois"), Target::Bookmark("Dois".into()));
    }

    #[test]
    fn est_25_links_to_documents_open_from_the_folder_of_this_one() {
        let folder = std::path::Path::new("/home/a/Livro");
        assert_eq!(
            document_file("Dois.odt", "Fim", Some(folder)),
            Some((folder.join("Dois.odt"), "Fim".into()))
        );
        assert_eq!(
            document_file("file:///tmp/T%C3%AAs.odt", "", Some(folder)),
            Some(("/tmp/Tês.odt".into(), String::new()))
        );
        assert_eq!(document_file("Dois.odt", "", None), None);
    }
}
