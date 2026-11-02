// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Builds the app icons in data/icons into a GResource bundle, so that the
//! apps show their own icons even when they run from the build tree, and
//! builds the translations in po into the apps, so that they need no files
//! installed beside them.

use std::fmt::Write;
use std::path::Path;

fn main() {
    glib_build_tools::compile_resources(
        &["../../data/icons/hicolor"],
        "resources/icons.gresource.xml",
        "icons.gresource",
    );
    translations();
}

/// One message of a catalog: its context, its text, and its translations
/// (one, or one for each plural form).
struct Message {
    context: Option<String>,
    id: String,
    translations: Vec<String>,
}

/// Reads a quoted string of a PO file, with its escapes.
fn unquote(text: &str) -> String {
    let inner = text
        .trim()
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or_else(|| panic!("not a quoted string: {text}"));
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            other => panic!("unknown escape \\{other:?} in {text}"),
        }
    }
    out
}

/// Reads the messages of a PO file and its plural rule. Fuzzy and
/// untranslated messages are left out, so that the English shows.
fn read_po(text: &str) -> (Vec<Message>, String) {
    let mut messages = Vec::new();
    let mut plural = String::from("n != 1");
    for entry in text.split("\n\n") {
        let mut fuzzy = false;
        let mut fields: Vec<(String, String)> = Vec::new();
        for line in entry.lines() {
            let line = line.trim();
            if line.starts_with("#,") && line.contains("fuzzy") {
                fuzzy = true;
            } else if line.starts_with('#') || line.is_empty() {
            } else if line.starts_with('"') {
                let last = fields.last_mut().expect("a string continues a field");
                last.1.push_str(&unquote(line));
            } else {
                let (key, value) = line.split_once(' ').expect("a field and its value");
                fields.push((key.to_owned(), unquote(value)));
            }
        }
        let get = |key: &str| {
            fields
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };
        let Some(id) = get("msgid") else { continue };
        if id.is_empty() {
            if let Some(header) = get("msgstr")
                && let Some(rule) = header
                    .lines()
                    .find_map(|l| l.strip_prefix("Plural-Forms:"))
                    .and_then(|f| f.split("plural=").nth(1))
            {
                plural = rule.trim().trim_end_matches(';').trim().to_owned();
            }
            continue;
        }
        let translations: Vec<String> = if get("msgid_plural").is_some() {
            (0..).map_while(|i| get(&format!("msgstr[{i}]"))).collect()
        } else {
            get("msgstr").into_iter().collect()
        };
        if fuzzy || translations.is_empty() || translations.iter().any(String::is_empty) {
            continue;
        }
        messages.push(Message {
            context: get("msgctxt"),
            id,
            translations,
        });
    }
    (messages, plural)
}

fn translations() {
    let po = Path::new("../../po");
    println!("cargo::rerun-if-changed={}", po.display());
    let languages = std::fs::read_to_string(po.join("LINGUAS")).unwrap_or_default();
    let mut out = String::from("&[\n");
    for language in languages
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let path = po.join(format!("{language}.po"));
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let (mut messages, plural) = read_po(&text);
        let rule = match plural.replace(' ', "").as_str() {
            "(n!=1)" | "n!=1" => "Plural::NotOne",
            "(n>1)" | "n>1" => "Plural::MoreThanOne",
            "0" => "Plural::None",
            other => panic!("{language}: the plural rule {other} is not known yet"),
        };
        // Sorted by key, for a binary search.
        let key = |m: &Message| match &m.context {
            Some(context) => format!("{context}\u{4}{}", m.id),
            None => m.id.clone(),
        };
        messages.sort_by_key(key);
        let _ = writeln!(
            out,
            "    Catalog {{ language: {language:?}, plural: {rule}, messages: &["
        );
        for message in &messages {
            let _ = writeln!(
                out,
                "        ({:?}, &{:?}),",
                key(message),
                message.translations
            );
        }
        out.push_str("    ] },\n");
    }
    out.push(']');
    let dir = std::env::var("OUT_DIR").expect("Cargo sets OUT_DIR");
    std::fs::write(Path::new(&dir).join("catalogs.rs"), out).expect("OUT_DIR is writable");
}
