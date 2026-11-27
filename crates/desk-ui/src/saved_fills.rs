// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved fills (W121): the colors and gradients that the user keeps to
//! use again in any document, as Pages 15.4 keeps fills in the color
//! picker. They live in `saved-fills.txt` of the data folder of Desk, one
//! a line, as an id and the fill in words:
//!
//! - `color RRGGBBAA`
//! - `gradient RRGGBBAA RRGGBBAA ANGLE`
//! - `advanced ANGLE CENTER AT:RRGGBBAA:BLEND …`, where the center is `-`
//!   along a line, or `X,Y` in thousandths of the object.
//!
//! A picture is not saved: it belongs to its document. Lines this version
//! does not read are kept as they are when the others change.

use std::path::{Path, PathBuf};

use desk_model::{Fill, Gradient, GradientStop, Rgba};

/// The most fills kept, the most colors of a saved gradient, and the
/// largest file read.
pub(crate) const MAX_SAVED: usize = 100;
const MAX_STOPS: usize = 64;
const MAX_FILE: u64 = 1024 * 1024;

/// The first line of the file.
const HEADER: &str = "# Desk Saved Fills 1";

/// A saved fill.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Saved {
    /// What names it in the actions, as long as it is kept.
    pub id: u64,
    pub fill: Fill,
}

/// Where the saved fills are kept.
pub(crate) fn saved_path() -> PathBuf {
    gtk::glib::user_data_dir()
        .join("desk")
        .join("saved-fills.txt")
}

fn hex(color: Rgba) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        color.r, color.g, color.b, color.a
    )
}

fn color(text: &str) -> Option<Rgba> {
    if text.len() != 8 || !text.is_ascii() {
        return None;
    }
    let part = |i: usize| u8::from_str_radix(&text[i..i + 2], 16).ok();
    Some(Rgba {
        r: part(0)?,
        g: part(2)?,
        b: part(4)?,
        a: part(6)?,
    })
}

/// An angle in hundredths of a degree, within a turn.
fn angle(text: &str) -> Option<i32> {
    text.parse::<i32>()
        .ok()
        .filter(|a| (-36_000..=36_000).contains(a))
}

/// A fill in words, or none for one that is not saved, such as a
/// picture.
pub(crate) fn fill_to_text(fill: &Fill) -> Option<String> {
    match fill {
        Fill::Color(c) => Some(format!("color {}", hex(*c))),
        Fill::Gradient { from, to, angle } => {
            Some(format!("gradient {} {} {angle}", hex(*from), hex(*to)))
        }
        Fill::Advanced(gradient) => {
            if gradient.stops.len() < 2 || gradient.stops.len() > MAX_STOPS {
                return None;
            }
            let center = gradient
                .center
                .map_or_else(|| "-".to_owned(), |(x, y)| format!("{x},{y}"));
            let stops: Vec<String> = gradient
                .stops
                .iter()
                .map(|s| format!("{}:{}:{}", s.at, hex(s.color), s.blend))
                .collect();
            Some(format!(
                "advanced {} {center} {}",
                gradient.angle,
                stops.join(" ")
            ))
        }
        _ => None,
    }
}

/// The fill of words that [`fill_to_text`] writes.
pub(crate) fn fill_of_text(text: &str) -> Option<Fill> {
    let mut words = text.split(' ');
    let fill = match words.next()? {
        "color" => Fill::Color(color(words.next()?)?),
        "gradient" => Fill::Gradient {
            from: color(words.next()?)?,
            to: color(words.next()?)?,
            angle: angle(words.next()?)?,
        },
        "advanced" => {
            let angle = angle(words.next()?)?;
            let center = match words.next()? {
                "-" => None,
                point => {
                    let (x, y) = point.split_once(',')?;
                    let part = |v: &str| v.parse::<u16>().ok().filter(|v| *v <= 1000);
                    Some((part(x)?, part(y)?))
                }
            };
            let mut stops = Vec::new();
            for word in words.by_ref() {
                if stops.len() >= MAX_STOPS {
                    return None;
                }
                let mut fields = word.split(':');
                let at = fields.next()?.parse::<u16>().ok().filter(|v| *v <= 1000)?;
                let c = color(fields.next()?)?;
                let blend = fields.next()?.parse::<u16>().ok().filter(|v| *v <= 1000)?;
                if fields.next().is_some() {
                    return None;
                }
                stops.push(GradientStop {
                    at,
                    color: c,
                    blend,
                });
            }
            if stops.len() < 2 {
                return None;
            }
            return Some(Fill::Advanced(Gradient {
                stops,
                angle,
                center,
            }));
        }
        _ => return None,
    };
    words.next().is_none().then_some(fill)
}

/// A line of the file: a fill this version reads, or one it does not,
/// kept as it is.
enum Line {
    Fill(Saved),
    Other(String),
}

/// The lines of the file: none when there is none; an error when it
/// cannot be read whole, so that it is not written over.
fn read_lines(file: &Path) -> std::io::Result<Vec<Line>> {
    let size = match std::fs::metadata(file) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    if size > MAX_FILE {
        return Err(std::io::Error::other(
            "the file of saved fills is too large",
        ));
    }
    let text = std::fs::read_to_string(file)?;
    let mut lines = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for line in text.lines().filter(|l| !l.is_empty() && *l != HEADER) {
        let read = line.split_once('\t').and_then(|(id, fill)| {
            let id = u64::from_str_radix(id, 16).ok()?;
            if ids.len() >= MAX_SAVED || ids.contains(&id) {
                return None;
            }
            Some(Saved {
                id,
                fill: fill_of_text(fill)?,
            })
        });
        match read {
            Some(saved) => {
                ids.insert(saved.id);
                lines.push(Line::Fill(saved));
            }
            None => lines.push(Line::Other(line.to_owned())),
        }
    }
    Ok(lines)
}

/// Writes the lines to the file, all at once.
fn write_lines(file: &Path, lines: &[Line]) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let mut text = format!("{HEADER}\n");
    for line in lines {
        match line {
            Line::Fill(saved) => {
                let Some(fill) = fill_to_text(&saved.fill) else {
                    continue;
                };
                text.push_str(&format!("{:x}\t{fill}", saved.id));
            }
            Line::Other(other) => text.push_str(other),
        }
        text.push('\n');
    }
    // Written beside it first, so that a failure leaves the old file.
    let mut part = file.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    std::fs::write(&part, text)?;
    std::fs::rename(&part, file)
}

/// The saved fills in a file, in the order they were saved.
pub(crate) fn read_saved(file: &Path) -> Vec<Saved> {
    read_lines(file)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|line| match line {
            Line::Fill(saved) => Some(saved),
            Line::Other(_) => None,
        })
        .collect()
}

/// Why a fill was not saved.
#[derive(Debug)]
pub(crate) enum SaveError {
    /// It is a picture, or none.
    Unsupported,
    /// As many are saved as are kept.
    Full,
    /// The file cannot be read or written.
    File(std::io::Error),
}

/// Saves a fill in a file and returns it; one saved already is not saved
/// twice.
pub(crate) fn add_saved(file: &Path, fill: &Fill) -> Result<Saved, SaveError> {
    fill_to_text(fill).ok_or(SaveError::Unsupported)?;
    let mut lines = read_lines(file).map_err(SaveError::File)?;
    let saved: Vec<&Saved> = lines
        .iter()
        .filter_map(|l| match l {
            Line::Fill(s) => Some(s),
            Line::Other(_) => None,
        })
        .collect();
    if let Some(same) = saved.iter().find(|s| s.fill == *fill) {
        return Ok((*same).clone());
    }
    if saved.len() >= MAX_SAVED {
        return Err(SaveError::Full);
    }
    let taken = |id: u64| {
        lines.iter().any(|l| match l {
            Line::Fill(s) => s.id == id,
            Line::Other(text) => {
                text.split('\t')
                    .next()
                    .and_then(|i| u64::from_str_radix(i, 16).ok())
                    == Some(id)
            }
        })
    };
    let mut id = gtk::glib::real_time().unsigned_abs();
    while taken(id) {
        id = id.wrapping_add(1);
    }
    let added = Saved {
        id,
        fill: fill.clone(),
    };
    lines.push(Line::Fill(added.clone()));
    write_lines(file, &lines).map_err(SaveError::File)?;
    Ok(added)
}

/// Takes a saved fill out of a file; returns whether it was there and the
/// file was written.
pub(crate) fn delete_saved(file: &Path, id: u64) -> bool {
    let Ok(mut lines) = read_lines(file) else {
        return false;
    };
    let count = lines.len();
    lines.retain(|l| !matches!(l, Line::Fill(s) if s.id == id));
    if lines.len() == count {
        return false;
    }
    match write_lines(file, &lines) {
        Ok(()) => true,
        Err(error) => {
            gtk::glib::g_warning!("desk", "cannot write the saved fills: {error}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advanced() -> Fill {
        Fill::Advanced(Gradient {
            stops: vec![
                GradientStop::new(0, Rgba::rgb(10, 20, 30)),
                GradientStop {
                    at: 600,
                    color: Rgba::rgb(40, 50, 60),
                    blend: 300,
                },
                GradientStop::new(1000, Rgba::WHITE),
            ],
            angle: -4500,
            center: Some((300, 700)),
        })
    }

    #[test]
    fn w121_fills_are_written_and_read_back() {
        let mut half = Rgba::rgb(1, 2, 3);
        half.a = 128;
        for fill in [
            Fill::Color(half),
            Fill::Gradient {
                from: Rgba::BLACK,
                to: Rgba::WHITE,
                angle: 9000,
            },
            advanced(),
            Fill::Advanced(Gradient::linear(Rgba::BLACK, Rgba::WHITE, 0)),
        ] {
            let text = fill_to_text(&fill).unwrap();
            assert_eq!(fill_of_text(&text), Some(fill), "{text}");
        }
        for bad in [
            "",
            "color",
            "color 12345",
            "color gggggggg",
            "color 00000000 extra",
            "gradient 00000000 ffffffff 99999999",
            "advanced 0 - 0:000000ff:500",
            "advanced 0 2000,0 0:000000ff:500 1000:ffffffff:500",
            "advanced 0 - 0:000000ff:500 1001:ffffffff:500",
            "advanced 0 - 0:000000ff:500:1 1000:ffffffff:500",
            "pattern 1",
        ] {
            assert!(fill_of_text(bad).is_none(), "{bad}");
        }
        let many = format!("advanced 0 - {}", "0:000000ff:500 ".repeat(MAX_STOPS + 1));
        assert!(fill_of_text(many.trim()).is_none());
    }

    #[test]
    fn w121_fills_are_saved_and_deleted_keeping_lines_not_read() {
        let folder = std::env::temp_dir().join(format!("desk-saved-fills-{}", std::process::id()));
        let file = folder.join("saved-fills.txt");
        let _ = std::fs::remove_dir_all(&folder);
        assert!(read_saved(&file).is_empty());
        let red = Fill::Color(Rgba::rgb(255, 0, 0));
        let first = add_saved(&file, &red).unwrap();
        // Saved once only.
        assert_eq!(add_saved(&file, &red).unwrap().id, first.id);
        let second = add_saved(&file, &advanced()).unwrap();
        assert_ne!(first.id, second.id);
        let picture = desk_model::Picture::from_png({
            let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
            png.extend(1u32.to_be_bytes());
            png.extend(1u32.to_be_bytes());
            png.extend([8, 6, 0, 0, 0]);
            png
        })
        .unwrap();
        let image = Fill::Image {
            picture,
            mode: desk_model::ImageFill::default(),
            tint: None,
        };
        assert!(matches!(
            add_saved(&file, &image),
            Err(SaveError::Unsupported)
        ));
        assert_eq!(read_saved(&file), [first.clone(), second.clone()]);
        let mut text = std::fs::read_to_string(&file).unwrap();
        text.push_str("7\tpattern stripes\n");
        std::fs::write(&file, text).unwrap();
        assert!(delete_saved(&file, first.id));
        assert!(!delete_saved(&file, first.id));
        assert_eq!(read_saved(&file), std::slice::from_ref(&second));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.lines().any(|l| l == "7\tpattern stripes"));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
