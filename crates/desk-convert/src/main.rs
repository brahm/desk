// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Command-line converter for Desk documents.
//!
//! It converts files with the same libraries as the apps, but without GTK
//! and without a display, for batch conversion of iWork documents and for
//! the automated tests.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage: desk-convert <INPUT> <OUTPUT>

Converts INPUT to the format that the file name extension of OUTPUT names.
Reads OpenDocument text (.odt) and Pages '09 and '08 documents (.pages,
a package folder or a flat file), and writes PDF (.pdf) or OpenDocument
text (.odt).

Options:
  -h, --help     Print this help
  -V, --version  Print the version";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Version,
    Convert { input: PathBuf, output: PathBuf },
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut paths = Vec::new();
    let mut options_done = false;
    for arg in args {
        if !options_done {
            match arg.to_str() {
                Some("--") => {
                    options_done = true;
                    continue;
                }
                Some("-h" | "--help") => return Ok(Command::Help),
                Some("-V" | "--version") => return Ok(Command::Version),
                Some(option) if option.starts_with('-') && option != "-" => {
                    return Err(format!("unknown option {option}"));
                }
                _ => {}
            }
        }
        paths.push(PathBuf::from(arg));
    }
    match <[PathBuf; 2]>::try_from(paths) {
        Ok([input, output]) => Ok(Command::Convert { input, output }),
        Err(_) => Err("expected an input file and an output file".to_owned()),
    }
}

/// The format a file name extension names.
#[derive(Debug, PartialEq, Eq)]
enum Format {
    Odt,
    Pages,
    Pdf,
}

fn format_of(path: &Path) -> Option<Format> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "odt" | "ott" => Some(Format::Odt),
        "pages" => Some(Format::Pages),
        "pdf" => Some(Format::Pdf),
        _ => None,
    }
}

fn convert(input: &Path, output: &Path) -> Result<(), String> {
    let read = |e: &dyn std::fmt::Display| format!("cannot read {}: {e}", input.display());
    let document = match format_of(input) {
        Some(Format::Odt) => {
            let bytes = std::fs::read(input).map_err(|e| read(&e))?;
            desk_filters::read_odt(&bytes).map_err(|e| read(&e))?
        }
        Some(Format::Pages) => desk_filters::read_pages_at(input).map_err(|e| read(&e))?,
        _ => {
            return Err(format!(
                "cannot read {}: only .odt and .pages files are read so far",
                input.display()
            ));
        }
    };
    let written = || format!("cannot write {}", output.display());
    match format_of(output) {
        Some(Format::Odt) => {
            let bytes =
                desk_filters::write_odt(&document).map_err(|e| format!("{}: {e}", written()))?;
            std::fs::write(output, bytes).map_err(|e| format!("{}: {e}", written()))
        }
        Some(Format::Pdf) => {
            let mut system = desk_text::TextSystem::new();
            let layout = desk_layout::paginate(&document, &mut system);
            for choice in system.substitutions() {
                eprintln!("desk-convert: warning: {}", substitution(&choice));
            }
            let pages = desk_render::DisplayList::pages(&layout);
            desk_render::Painter::new()
                .and_then(|mut painter| painter.write_pdf(&pages, document.metadata(), output))
                .map_err(|e| format!("{}: {e}", written()))
        }
        Some(Format::Pages) | None => Err(format!(
            "{}: only .pdf and .odt files are written",
            written()
        )),
    }
}

/// A font substitution, in one sentence (DOC-05).
fn substitution(choice: &desk_text::FontChoice) -> String {
    let how = match choice.kind {
        desk_text::FontMatch::Metric => ", which has the same metrics",
        desk_text::FontMatch::Style => ", which looks alike; lines may break elsewhere",
        desk_text::FontMatch::Fallback | desk_text::FontMatch::Installed => {
            "; lines may break elsewhere"
        }
    };
    format!(
        "the font {} is not installed, so {} is used{how}",
        choice.requested, choice.used
    )
}

fn main() -> ExitCode {
    match parse_args(std::env::args_os().skip(1)) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("desk-convert {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Convert { input, output }) => match convert(&input, &output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("desk-convert: {message}");
                ExitCode::FAILURE
            }
        },
        Err(message) => {
            eprintln!("desk-convert: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_input_and_output() {
        assert_eq!(
            parse(&["report.pages", "report.odt"]),
            Ok(Command::Convert {
                input: "report.pages".into(),
                output: "report.odt".into(),
            })
        );
    }

    #[test]
    fn parses_help_and_version() {
        assert_eq!(parse(&["--help"]), Ok(Command::Help));
        assert_eq!(parse(&["a.key", "-V"]), Ok(Command::Version));
    }

    #[test]
    fn takes_paths_that_start_with_a_dash_after_double_dash() {
        assert_eq!(
            parse(&["--", "-draft.numbers", "out.ods"]),
            Ok(Command::Convert {
                input: "-draft.numbers".into(),
                output: "out.ods".into(),
            })
        );
    }

    #[test]
    fn doc_05_substitutions_are_described() {
        let choice = desk_text::FontChoice {
            requested: "Helvetica".into(),
            used: "Liberation Sans".into(),
            kind: desk_text::FontMatch::Metric,
        };
        assert_eq!(
            substitution(&choice),
            "the font Helvetica is not installed, so Liberation Sans is used, which has the same metrics"
        );
    }

    #[test]
    fn formats_come_from_the_extension() {
        assert_eq!(format_of(Path::new("a.ODT")), Some(Format::Odt));
        assert_eq!(format_of(Path::new("dir.pdf/a.pdf")), Some(Format::Pdf));
        assert_eq!(format_of(Path::new("a.pages")), Some(Format::Pages));
        assert_eq!(format_of(Path::new("a.numbers")), None);
        assert_eq!(format_of(Path::new("pdf")), None);
    }

    #[test]
    fn rejects_unknown_options_and_wrong_path_counts() {
        assert!(parse(&["--pdf", "a.pages", "a.pdf"]).is_err());
        assert!(parse(&["a.pages"]).is_err());
        assert!(parse(&["a.pages", "b.odt", "c.odt"]).is_err());
    }
}
