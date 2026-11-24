// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a link points to (EST-25), as Pages' Link inspector offers it: a
//! web page, an e-mail message with its subject, another document, at
//! one of its bookmarks if wanted, or a bookmark of this document. Each
//! is kept as the text of a URI.
//!
//! Some links only work in one app, such as going to a slide in Keynote
//! (IOP-32). They are kept as `desk:slide/…` URIs, so that text copied
//! between the apps keeps them, and the other apps show what they do
//! without following them.

use crate::preferences::OpenLinks;
use gtk::gio::prelude::FileExt;
use gtk::glib;

use crate::i18n::tr;

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
    /// A slide of the presentation, which only Desk Slides follows.
    Slide(Slide),
}

/// The slide that a link of a presentation goes to, as Keynote's Link
/// inspector offers them (IOP-32).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slide {
    /// The slide of a number, from 1.
    Number(u32),
    /// The next slide.
    Next,
    /// The slide before.
    Previous,
    /// The first slide.
    First,
    /// The last slide.
    Last,
    /// The slide shown last.
    LastViewed,
    /// Ends the slideshow.
    Exit,
}

/// The words of `desk:slide/…` links, with their slides.
const SLIDES: [(&str, Slide); 6] = [
    ("next", Slide::Next),
    ("previous", Slide::Previous),
    ("first", Slide::First),
    ("last", Slide::Last),
    ("last-viewed", Slide::LastViewed),
    ("exit", Slide::Exit),
];

impl Slide {
    /// What the link does, for the apps that do not follow it.
    pub fn describe(self) -> String {
        match self {
            Self::Number(n) => tr("Goes to slide {n}").replace("{n}", &n.to_string()),
            Self::Next => tr("Goes to the next slide"),
            Self::Previous => tr("Goes to the previous slide"),
            Self::First => tr("Goes to the first slide"),
            Self::Last => tr("Goes to the last slide"),
            Self::LastViewed => tr("Goes to the slide shown last"),
            Self::Exit => tr("Ends the slideshow"),
        }
    }
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

/// Whether a link goes to a page on the web, or to an email without a
/// file attached. Any other link outside the document, which a file may
/// make point to a file of the user, a shared folder or another app,
/// shows its address and asks before it opens, whatever the preferences
/// say (review RV, batch 5).
pub fn is_web(link: &str) -> bool {
    let Some((scheme, rest)) = link.split_once(':') else {
        return false;
    };
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => true,
        "mailto" => rest.split_once('?').is_none_or(|(_, query)| {
            query.split('&').all(|pair| {
                let key = pair.split_once('=').map_or(pair, |(k, _)| k);
                ["subject", "body", "cc", "bcc"]
                    .iter()
                    .any(|allowed| key.eq_ignore_ascii_case(allowed))
            })
        }),
        _ => false,
    }
}

/// What a click on a link outside the document and its other documents
/// does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Follow {
    /// It opens.
    Open,
    /// Its address shows, and it opens if asked to.
    Confirm,
    /// The first click on a link to the web: it asks what such clicks
    /// do from now on, and keeps the answer in the preferences.
    Choose,
    /// It does not open.
    Refuse,
}

/// What a click on a link outside the document does, by the preferences.
pub fn follow(link: &str, choice: Option<OpenLinks>) -> Follow {
    match (choice, is_web(link)) {
        (Some(OpenLinks::Never), _) => Follow::Refuse,
        (_, false) | (Some(OpenLinks::Ask), true) => Follow::Confirm,
        (None, true) => Follow::Choose,
        (Some(OpenLinks::Open), true) => Follow::Open,
    }
}

impl Target {
    /// What a link points to.
    pub fn parse(link: &str) -> Self {
        if let Some(slide) = link.strip_prefix("desk:slide/") {
            let found = SLIDES
                .iter()
                .find(|(word, _)| *word == slide)
                .map(|(_, s)| *s)
                .or_else(|| slide.parse().ok().filter(|n| *n > 0).map(Slide::Number));
            if let Some(slide) = found {
                return Self::Slide(slide);
            }
        }
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
            Self::Slide(Slide::Number(n)) => format!("desk:slide/{n}"),
            Self::Slide(slide) => {
                let word = SLIDES
                    .iter()
                    .find(|(_, s)| s == slide)
                    .map_or("next", |(w, _)| w);
                format!("desk:slide/{word}")
            }
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
            Self::Slide(_) => 4,
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

/// The most characters of an address that a tooltip or a dialog shows.
const MAX_SHOWN: usize = 300;

/// An address as a tooltip or a dialog shows it: the characters that are
/// not seen, or that turn the text around, written as a URL writes them,
/// so that the address shown is the one opened; and a long one cut short
/// (second review of RV batch 5).
pub fn shown(link: &str) -> String {
    let hidden = |c: char| {
        c.is_control()
            || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2069}' | '\u{061C}' | '\u{FEFF}')
    };
    let mut out = String::new();
    for (count, c) in link.chars().enumerate() {
        if count == MAX_SHOWN {
            out.push('…');
            break;
        }
        if hidden(c) {
            let mut bytes = [0; 4];
            for byte in c.encode_utf8(&mut bytes).bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_shows_what_it_opens() {
        // Second review of RV batch 5: a link could hide or turn around
        // part of its address in the tooltip and the dialog.
        assert_eq!(shown("https://a.org/x"), "https://a.org/x");
        assert_eq!(
            shown("https://a.org/\u{202E}fdp.exe"),
            "https://a.org/%E2%80%AEfdp.exe"
        );
        assert_eq!(shown("a\nb\u{200B}"), "a%0Ab%E2%80%8B");
        let long = format!("https://a.org/{}", "x".repeat(1000));
        assert_eq!(shown(&long).chars().count(), MAX_SHOWN + 1);
        assert!(shown(&long).ends_with('…'));
    }

    #[test]
    fn links_outside_the_document_follow_the_preferences() {
        // Review RV, batch 5: a click on any link of a document opened it,
        // whatever it pointed to. The owner chose: a link to the web asks,
        // the first time, what such links do from now on.
        for link in [
            "https://example.com/a?b=c",
            "HTTP://example.com",
            "mailto:someone@example.com",
            "mailto:someone@example.com?subject=Hi&body=There",
        ] {
            assert!(is_web(link), "{link}");
            assert_eq!(follow(link, None), Follow::Choose);
            assert_eq!(follow(link, Some(OpenLinks::Open)), Follow::Open);
            assert_eq!(follow(link, Some(OpenLinks::Ask)), Follow::Confirm);
            assert_eq!(follow(link, Some(OpenLinks::Never)), Follow::Refuse);
        }
        for link in [
            "file:///home/user/Downloads/x.jar",
            "smb://server/share",
            "sftp://server/file",
            "steam://run/1",
            "mailto:someone@example.com?attach=/home/user/.ssh/id_rsa",
            "javascript:alert(1)",
            "/etc/passwd",
            "report.txt",
        ] {
            assert!(!is_web(link), "{link}");
            for choice in [None, Some(OpenLinks::Open), Some(OpenLinks::Ask)] {
                assert_eq!(follow(link, choice), Follow::Confirm, "{link}");
            }
            assert_eq!(follow(link, Some(OpenLinks::Never)), Follow::Refuse);
        }
    }

    #[test]
    fn iop_32_links_of_a_presentation_are_kept() {
        for (link, slide) in [
            ("desk:slide/3", Slide::Number(3)),
            ("desk:slide/next", Slide::Next),
            ("desk:slide/previous", Slide::Previous),
            ("desk:slide/first", Slide::First),
            ("desk:slide/last", Slide::Last),
            ("desk:slide/last-viewed", Slide::LastViewed),
            ("desk:slide/exit", Slide::Exit),
        ] {
            let target = Target::parse(link);
            assert_eq!(target, Target::Slide(slide), "{link}");
            assert_eq!(target.link(), link);
            assert!(!slide.describe().is_empty());
        }
        assert_eq!(Slide::Number(3).describe(), "Goes to slide 3");
        // Anything else of the scheme stays a plain address.
        assert_eq!(
            Target::parse("desk:slide/0"),
            Target::Web("desk:slide/0".into())
        );
    }

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
